//! The approvals inbox, server side (D97, PROPOSED): what a person needs to see an agent's
//! proposal, decide it and learn afterwards what became of it, and what the agent is told.
//!
//! A proposal is still the sealed plan `plan:{id}` of D18 and D42, byte for byte, living seven
//! days. Beside it the dispatcher now writes a **receipt** `planrec:{id}` (sealed, 37 days): who
//! proposed which command on which record, when it runs out, and what became of it (approved,
//! rejected with the person's reason, failed with the code of the refusal, expired). A receipt
//! holds no input value.
//!
//! Five commands on the built-in model `plan`, bound by [`Dispatcher::register_plans`] and run by
//! the dispatcher itself (a handler gets only its input, and these need the plans, the receipts,
//! the registry and the reads of the person who asks):
//!
//! - `plan.count`, `plan.list`, `plan.approve` and `plan.reject` exist for a person at the
//!   keyboard only: any other principal (an agent, a phone's token) is answered as for a command
//!   that does not exist, on every surface, and the catalogue does not show them to it;
//! - `plan.get` answers a person with the plan and the change it makes, read as that person, and an
//!   agent with the state of its own plan and what to do next.
//!
//! **Who sees a plan.** A waiting plan is shown to a person who could decide it (they hold the
//! command's group and can read the target record through the generated `get`, which applies
//! the read group, the row rules and the masking; a command without a record is shown on the group
//! alone) and to the owner of the proposing agent (`{owner}/agent`). A decided plan is shown to the
//! owner, to the person who decided it and to the people who could have. To everybody else the
//! plan does not exist: the answer is the not-found of an unknown plan, the same for hidden,
//! unknown, decided long ago and not yours. The count is the length of the same list.
//!
//! **Deciding.** `plan.approve` is the approval of D42 with three additions: the person must pass
//! the test above, `seen_version` (when given) must equal the version of the record the person is
//! shown now, or nothing runs and the plan stays (`precondition_failed`), and the loser of a race
//! hears `already_decided` (who, when) when they may see the plan. `plan.reject` takes the plan out
//! under the same lock and keeps the person's reason (at most 300 characters, hidden characters
//! removed) in its own field of the receipt. `POST /api/approve/{plan_id}` keeps its answers
//! ([`Dispatcher::approve`]).
//!
//! **The version.** The records carry no `row_version` yet (D75: no model is tracked). The
//! version a person is shown is therefore the record's `row_version` when it has one, and else a
//! fingerprint of the record as that person reads it (SHA-256 of its canonical JSON, the first 53
//! bits, so that it is a safe JavaScript integer). It changes whenever a field the person can see
//! changes; it is computed from what the person may read only, so it tells nothing of a field
//! hidden from them.

use crate::shared::{PLAN_TTL_MS, Pending};
use crate::{ANY_GROUP, CallError, Dispatcher, Principal, Registered, Response, Surface, Value};
use crate::{audit, errors};
use obop_masking::mode_for;
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module};
use obop_types::{DataClass, MaskMode};
use serde_json::{Value as J, json};
use std::collections::{BTreeMap, HashMap};

/// The built-in model that carries the five commands.
pub const MODEL: &str = "plan";
/// The most plans one agent may have waiting (D97); the next proposal is refused.
pub const MAX_WAITING_PER_AGENT: usize = 100;
/// The longest reason a person may give when rejecting, in characters (after cleaning).
pub const REASON_MAX_CHARS: usize = 300;
/// The most rows one page of `plan.list` has.
pub const LIST_MAX: usize = 50;
/// How long a receipt outlives the decision (or the end of the plan), in seconds.
const KEEP_AFTER_S: i64 = 30 * 86_400;
/// How long a plan waits, in seconds (the TTL of `plan:{id}`).
const PLAN_TTL_S: i64 = (PLAN_TTL_MS / 1000) as i64;

/// The commands of the model, by short name.
pub const COMMANDS: [&str; 5] = ["count", "list", "get", "approve", "reject"];

/// The states of a receipt.
pub mod state {
    pub const PENDING: &str = "pending";
    pub const APPROVED: &str = "approved";
    pub const REJECTED: &str = "rejected";
    pub const FAILED: &str = "failed";
    pub const EXPIRED: &str = "expired";
}

/// The `security_event` kinds of D97. `detail` is `plan {id}: {command} on {model}:{record}`,
/// never a value.
pub mod event {
    pub const PROPOSED: &str = "plan_proposed";
    pub const APPROVED: &str = "plan_approved";
    pub const REJECTED: &str = "plan_rejected";
    pub const FAILED: &str = "plan_failed";
    pub const EXPIRED: &str = "plan_expired";
}

/// What an agent is told when it proposes (MCP `instruction`, beside the `plan_id`).
pub fn proposal_instruction(plan_id: &str) -> String {
    format!(
        "Nothing has run. A person must approve plan {plan_id} in the back office, on the Approvals page. Check it later with plan__get (plan.get with id). Do not call this tool again for the same step."
    )
}

/// What an agent is told by `plan.get`, by the status of its plan. The person's reason is never
/// part of it: it is in its own field.
pub fn agent_instruction(status: &str) -> &'static str {
    match status {
        "waiting" => {
            "Nothing has run yet. A person must approve this in the back office, on the Approvals page. Check again later; do not propose it again."
        }
        "approved" => "A person approved this and it ran.",
        "rejected" => {
            "A person rejected this. Do not propose it again unchanged. Tell your user what you tried and ask what to change."
        }
        "failed" => {
            "The person approved, but the system refused it (see failure). The request is closed; propose again after fixing that."
        }
        _ => "Nobody decided in 7 days. Nothing ran. Propose again if it is still needed.",
    }
}

/// The registry metadata of the model `plan` (part of the Customize module). `get` and `list` are
/// the generated names, bound by hand; `count`, `approve` and `reject` are its own commands. None
/// needs approval and none ever makes a plan.
pub fn model_def() -> ModelDef {
    let f = |name, description, kind| {
        FieldDef::new(name, description, DataClass::None, false, ANY_GROUP).kind(kind)
    };
    ModelDef {
        name: MODEL,
        description: "A request an agent made that waits for a person, or was decided: who proposed which command on which record, when, and what became of it. A person sees the requests they could decide and those of their own agents; an agent sees only its own, with get",
        read_group: ANY_GROUP,
        write_group: ANY_GROUP,
        access: None,
        fields: vec![
            f(
                "seen_version",
                "Input of approve: the version of the record that the person was shown (seen_version in the answer of get); when the record has changed since, nothing runs and the request keeps waiting",
                FieldKind::Number,
            ),
            f(
                "reason",
                "Input of reject: why, in at most 300 characters, for the agent to read; kept apart from any instruction",
                FieldKind::Text,
            ),
        ],
        commands: vec![
            CommandDef::new(
                "count",
                "How many requests wait for the person who asks (waiting), how many of them their own agents made (mine_waiting), and whether they may decide any (can_decide). A person only",
                ANY_GROUP,
            ),
            CommandDef::new(
                "approve",
                "Approve a waiting request: the command runs as the agent proposed it. Give id and, from get, seen_version: when the record changed since, nothing runs. A person only",
                ANY_GROUP,
            ),
            CommandDef::new(
                "reject",
                "Reject a waiting request: nothing runs and the agent is told. Give id and, if you like, a reason (at most 300 characters). A person only",
                ANY_GROUP,
            ),
        ],
        ext: Default::default(),
    }
}

/// The descriptions of the two generated names, which mean something else here.
pub(crate) fn describe(model: &str, command: &str) -> Option<&'static str> {
    (model == MODEL).then_some(())?;
    match command {
        "list" => Some(
            "The requests the person who asks may see: state waiting (the default, oldest first) or decided (the last 30 days, newest first); filters model, record_id and mine (only their own agents' requests); limit up to 50 and offset. Answers {rows, total}. A person only",
        ),
        "get" => Some(
            "One request by id. A person who may see it gets what it changes, read as them (changes, seen_version); an agent gets the state of its own request (status waiting, approved, rejected, failed or expired), the person's reason in its own field, and what to do next (instruction)",
        ),
        _ => None,
    }
}

/// The receipt of a plan (D97). Times are Unix seconds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Receipt {
    pub proposer: String,
    pub command: String,
    pub model: String,
    pub target: Option<String>,
    pub created_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub state: String,
    pub decided_by: Option<String>,
    pub decided_at: Option<i64>,
    pub reason: Option<String>,
    pub failure: Option<String>,
}

impl Receipt {
    fn to_json(&self) -> J {
        json!({
            "v": 1,
            "proposer": self.proposer,
            "command": self.command,
            "model": self.model,
            "target": self.target,
            "created_at": self.created_at,
            "expires_at": self.expires_at,
            "state": self.state,
            "decided_by": self.decided_by,
            "decided_at": self.decided_at,
            "reason": self.reason,
            "failure": self.failure,
        })
    }

    fn from_json(j: &J) -> Option<Receipt> {
        let s = |k: &str| j[k].as_str().map(String::from);
        Some(Receipt {
            proposer: s("proposer")?,
            command: s("command")?,
            model: s("model").unwrap_or_default(),
            target: s("target"),
            created_at: j["created_at"].as_i64(),
            expires_at: j["expires_at"].as_i64(),
            state: s("state")?,
            decided_by: s("decided_by"),
            decided_at: j["decided_at"].as_i64(),
            reason: s("reason"),
            failure: s("failure"),
        })
    }

    /// Waiting by its own word, but past its end.
    fn ran_out(&self, now: i64) -> bool {
        self.state == state::PENDING && self.expires_at.is_some_and(|e| e <= now)
    }
}

/// The owner of an agent principal: `ada` for `ada/agent`.
fn owner_of(proposer: &str) -> Option<&str> {
    proposer.strip_suffix("/agent").filter(|o| !o.is_empty())
}

/// Who a person is now, by account name: their current principal, `None` when the account is
/// disabled or does not exist. Installed by the composition that knows the identity provider
/// ([`Dispatcher::set_proposer_source`]).
pub type ProposerSource = Box<dyn Fn(&str) -> Option<Principal> + Send + Sync>;

/// The longest record id kept from an agent's input without asking the store (see
/// [`Dispatcher::target_for`]).
const TARGET_MAX: usize = 64;

/// Whether `id` looks like the id of a record: at most [`TARGET_MAX`] characters of letters,
/// digits and `.`, `_`, `:`, `-`, `@`. A sentence, a line break or a long text does not.
fn record_shaped(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= TARGET_MAX
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:-@".contains(c))
}

/// The record a call names: its `id`, as text, exactly as the agent sent it. Only for reading the
/// record as a person; what is kept or shown goes through [`Dispatcher::target_for`].
fn target_of(input: &Value) -> Option<String> {
    match input {
        Value::Map(m) => match m.get("id") {
            Some(Value::Text(t)) if !t.is_empty() => Some(t.clone()),
            Some(Value::Int(i)) => Some(i.to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn rfc(t: Option<i64>) -> J {
    t.map_or(J::Null, |s| J::from(audit::rfc3339(s.max(0) as u64)))
}

/// A person's text for an agent: hidden characters removed (`devices::strip_hidden`), at most
/// [`REASON_MAX_CHARS`] characters, empty is none.
pub fn clean_reason(raw: &str) -> Result<Option<String>, String> {
    if raw.chars().count() > REASON_MAX_CHARS * 4 {
        return Err(format!(
            "the reason has more than {REASON_MAX_CHARS} characters"
        ));
    }
    let s = crate::devices::strip_hidden(raw);
    let s = s.trim();
    if s.chars().count() > REASON_MAX_CHARS {
        return Err(format!(
            "the reason has more than {REASON_MAX_CHARS} characters"
        ));
    }
    Ok((!s.is_empty()).then(|| s.to_string()))
}

/// The version of a record as a person reads it (see the module comment).
pub fn version_of(record: &Value) -> i64 {
    if let Value::Map(m) = record
        && let Some(Value::Int(v)) = m.get("row_version")
    {
        return *v;
    }
    use sha2::{Digest, Sha256};
    let mut text = String::new();
    canon(&record.to_json(), &mut text);
    let h = Sha256::digest(text.as_bytes());
    let mut b = [0u8; 8];
    b.copy_from_slice(&h[..8]);
    (u64::from_be_bytes(b) & ((1u64 << 53) - 1)) as i64
}

/// JSON with the keys of every object sorted, whatever `serde_json` was built with.
fn canon(v: &J, out: &mut String) {
    match v {
        J::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&J::from(k.as_str()).to_string());
                out.push(':');
                canon(&m[*k], out);
            }
            out.push('}');
        }
        J::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canon(x, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// One plan as the inbox knows it: the plan if it still waits, its receipt if it has one (a plan
/// made by a process older than D97 has none).
struct Item {
    id: String,
    pending: Option<Pending>,
    /// The record the waiting plan names, as [`Dispatcher::target_for`] keeps it.
    pending_target: Option<String>,
    receipt: Option<Receipt>,
}

impl Item {
    fn proposer(&self) -> &str {
        match (&self.receipt, &self.pending) {
            (Some(r), _) => &r.proposer,
            (None, Some(p)) => &p.principal.id,
            (None, None) => "",
        }
    }
    fn command(&self) -> &str {
        match (&self.pending, &self.receipt) {
            (Some(p), _) => &p.id,
            (None, Some(r)) => &r.command,
            (None, None) => "",
        }
    }
    fn target(&self) -> Option<String> {
        match (&self.pending, &self.receipt) {
            (Some(_), _) => self.pending_target.clone(),
            (None, Some(r)) => r.target.clone(),
            (None, None) => None,
        }
    }
    /// waiting, approved, rejected, failed or expired; `None` for a plan that is neither waiting
    /// nor decided by its receipt (taken out by an approval still running, or whose receipt was
    /// lost): it does not exist for anyone.
    fn status(&self, now: i64) -> Option<&'static str> {
        if let Some(r) = &self.receipt {
            if r.ran_out(now) {
                return Some(state::EXPIRED);
            }
            return match r.state.as_str() {
                state::PENDING => self.pending.as_ref().map(|_| "waiting"),
                state::APPROVED => Some(state::APPROVED),
                state::REJECTED => Some(state::REJECTED),
                state::FAILED => Some(state::FAILED),
                state::EXPIRED => Some(state::EXPIRED),
                _ => None,
            };
        }
        self.pending.as_ref().map(|_| "waiting")
    }
}

/// What a person can read of a record, asked once per record and request.
enum Read {
    /// Through the generated `get`, as this value.
    Yes(Value),
    /// The model has no `get`, or the store holds no record of the model under that id (the id
    /// names something else: an account, a module, a record still to be made): there is no row
    /// rule to apply, and the group decides alone.
    NoRecord,
    /// The record is stored and this person may not read it.
    No,
}

/// The reads of one person in one request.
struct Look<'a> {
    d: &'a Dispatcher,
    p: &'a Principal,
    cache: HashMap<(String, String), Option<Value>>,
    /// The owners of agents as their accounts are now, by name, asked once per request.
    owners: HashMap<String, Option<Principal>>,
}

impl<'a> Look<'a> {
    fn new(d: &'a Dispatcher, p: &'a Principal) -> Self {
        Self {
            d,
            p,
            cache: HashMap::new(),
            owners: HashMap::new(),
        }
    }

    /// Whether the proposer of a plan can still run `group`: [`Dispatcher::proposer_now`], with
    /// the owner's account read once per request.
    fn proposer_runs(&mut self, stored: &Principal, group: &str) -> bool {
        let d = self.d;
        let (Some(source), Some(owner)) = (d.proposer_source.get(), owner_of(&stored.id)) else {
            return stored.allows(group);
        };
        self.owners
            .entry(owner.to_string())
            .or_insert_with(|| source(owner))
            .as_ref()
            .is_some_and(|now| narrowed(stored, now).allows(group))
    }

    /// The record `model:id` as this person reads it, through the generated `get` and so through
    /// the read group, the row rules and the masking of every other read.
    fn read(&mut self, model: &str, id: &str) -> Read {
        let get = format!("{model}.get");
        if !self.d.commands.read().unwrap().contains_key(&get) {
            return Read::NoRecord;
        }
        let key = (model.to_string(), id.to_string());
        if !self.cache.contains_key(&key) {
            let r = self
                .d
                .call(
                    Surface::Rest,
                    self.p,
                    &mut crate::Vault::new(),
                    &get,
                    &Value::map([("id", Value::text(id))]),
                )
                .ok()
                .map(|r| r.value);
            self.cache.insert(key.clone(), r);
        }
        match &self.cache[&key] {
            Some(v) => Read::Yes(v.clone()),
            None if self.d.stored(model, id) => Read::No,
            None => Read::NoRecord,
        }
    }

    fn is_owner(&self, proposer: &str) -> bool {
        self.p.is_person() && owner_of(proposer) == Some(self.p.id.as_str())
    }

    /// The person holds the group of `command` and can read its record (when it names one).
    fn could_decide(&mut self, command: &str, target: Option<&str>) -> bool {
        if !self.p.is_person() {
            return false;
        }
        let Some(c) = self.d.registered(command) else {
            return false;
        };
        if !self.p.allows(c.permission_group) {
            return false;
        }
        match target {
            None => true,
            Some(id) => !matches!(self.read(c.model, id), Read::No),
        }
    }

    /// A decided plan: its owner, the person who decided it, and the people who could have.
    fn may_see_decided(&mut self, r: &Receipt) -> bool {
        self.p.is_person()
            && (self.is_owner(&r.proposer)
                || r.decided_by.as_deref() == Some(self.p.id.as_str())
                || self.could_decide(&r.command, r.target.as_deref()))
    }
}

/// The stored proposer with the groups its owner holds now; its own scopes (the token's) stay, so
/// it keeps only what is both still held and was handed to it.
fn narrowed(stored: &Principal, now: &Principal) -> Principal {
    let mut p = stored.clone();
    p.groups = now.groups.clone();
    p.mfa_limited = stored.mfa_limited || now.mfa_limited;
    p
}

/// What the person who asks may do with a waiting plan.
struct Rights {
    /// They could decide it: they may reject it, and see what it changes.
    decider: bool,
    /// The agent that proposed it still holds the command's group: it can run.
    runs: bool,
    owner: bool,
}

impl Rights {
    fn visible(&self) -> bool {
        self.decider || self.owner
    }
    fn can_approve(&self) -> bool {
        self.decider && self.runs
    }
}

impl Dispatcher {
    fn now(&self) -> i64 {
        (self.clock.0.read().unwrap())()
    }

    /// Whether the store holds a record of `model` under `id`. Without a shared store (tests,
    /// embedding) the answer is yes, the strict one: the person must then read it.
    fn stored(&self, model: &str, id: &str) -> bool {
        match self.shared.get() {
            Some(s) => !matches!(s.backend.get(model, id), Ok(None)),
            None => true,
        }
    }

    pub(crate) fn registered(&self, id: &str) -> Option<std::sync::Arc<Registered>> {
        self.commands.read().unwrap().get(id).cloned()
    }

    /// Install who the owner of an agent is now (once; later calls are ignored). Without it an
    /// approved plan runs as the copy of the agent taken when it proposed.
    pub fn set_proposer_source(&self, source: ProposerSource) {
        let _ = self.proposer_source.set(source);
    }

    /// The principal an approved plan of `stored` runs as now. For an agent of a person
    /// (`{owner}/agent`): the stored agent with the groups its owner's account holds now, so a
    /// group taken from the owner is gone from the agent too; `None` when the account is disabled
    /// or gone. Any other proposer (a connector), and every proposer without a source: the copy.
    pub(crate) fn proposer_now(&self, stored: &Principal) -> Option<Principal> {
        match (self.proposer_source.get(), owner_of(&stored.id)) {
            (Some(source), Some(owner)) => source(owner).map(|now| narrowed(stored, &now)),
            _ => Some(stored.clone()),
        }
    }

    /// The record id of a call that is kept in a receipt, written in the security log and shown
    /// on a row (D97 rule 4: no input value there). The agent's `id` when it names a record the
    /// store holds or has the shape of a record id ([`record_shaped`]); otherwise none. A
    /// sentence or a long text an agent sends as `id` is no record, and is neither kept nor shown.
    pub(crate) fn target_for(&self, command: &str, input: &Value) -> Option<String> {
        let id = target_of(input)?;
        if record_shaped(&id) {
            return Some(id);
        }
        let model = self.registered(command)?.model;
        (id.len() <= 4 * TARGET_MAX && self.shared.get().is_some() && self.stored(model, &id))
            .then_some(id)
    }

    /// Bind the five commands of the model `plan` of `module` (which must declare
    /// [`model_def`]). They run in the dispatcher; `count`, `list`, `approve` and `reject` exist
    /// for a person only.
    pub fn register_plans(&self, module: &Module) {
        for c in COMMANDS {
            self.register(
                module,
                MODEL,
                c,
                Box::new(|_| Err("the plan commands are run by the dispatcher".into())),
            );
            let id = format!("{MODEL}.{c}");
            let mut cmds = self.commands.write().unwrap();
            if let Some(r) = cmds.get(&id).cloned() {
                cmds.insert(
                    id,
                    std::sync::Arc::new(Registered {
                        id: r.id.clone(),
                        module: r.module.clone(),
                        description: r.description,
                        needs_approval: false,
                        model: r.model,
                        permission_group: r.permission_group,
                        surfaces: r.surfaces,
                        transition: None,
                        envelope: false,
                        actor: false,
                        writes: matches!(c, "approve" | "reject"),
                        generated: false,
                        native: true,
                        person_only: c != "get",
                        handler: Box::new(|_| {
                            Err("the plan commands are run by the dispatcher".into())
                        }),
                    }),
                );
            }
        }
    }

    /// A call of one of the five commands (`call` has checked the surface, the group and that a
    /// person-only command has a person).
    pub(crate) fn plan_call(
        &self,
        p: &Principal,
        vault: &mut crate::Vault,
        id: &str,
        input: &Value,
    ) -> Result<Response, CallError> {
        let empty = BTreeMap::new();
        let m = match input {
            Value::Map(m) => m,
            Value::Null => &empty,
            _ => return Err(CallError::Handler("expected an object".into())),
        };
        let text = |k: &str| match m.get(k) {
            Some(Value::Text(t)) if !t.trim().is_empty() => Some(t.trim().to_string()),
            _ => None,
        };
        let plan_id = || text("id").ok_or_else(|| CallError::Handler("missing id".into()));
        let value = match id.strip_prefix("plan.").unwrap_or("") {
            "count" => self.plan_count(p),
            "list" => self.plan_list(p, m)?,
            "get" => self.plan_get(p, &plan_id()?)?,
            "approve" => {
                let seen = match m.get("seen_version") {
                    None | Some(Value::Null) => None,
                    Some(Value::Int(n)) => Some(*n),
                    Some(Value::Text(t)) => Some(t.trim().parse::<i64>().map_err(|_| {
                        CallError::Handler("seen_version must be a whole number".into())
                    })?),
                    Some(_) => {
                        return Err(CallError::Handler(
                            "seen_version must be a whole number".into(),
                        ));
                    }
                };
                return self.decide(p, vault, &plan_id()?, seen, false);
            }
            "reject" => {
                let reason = match m.get("reason") {
                    None | Some(Value::Null) => None,
                    Some(Value::Text(t)) => clean_reason(t).map_err(CallError::Handler)?,
                    Some(_) => return Err(CallError::Handler("reason must be text".into())),
                };
                self.reject(p, &plan_id()?, reason)?
            }
            _ => return Err(CallError::UnknownCommand(id.into())),
        };
        Ok(Response {
            value: Value::from_json(&value),
            denied: Vec::new(),
        })
    }

    // ---- storage ---------------------------------------------------------------------------

    pub(crate) fn receipt(&self, plan_id: &str) -> Option<Receipt> {
        let text = self.receipts.get(self.shared.get(), plan_id)?;
        Receipt::from_json(&serde_json::from_str(&text).ok()?)
    }

    /// Write a receipt. Its life: 37 days while it waits, 30 days after it was decided.
    fn put_receipt(&self, plan_id: &str, r: &Receipt) -> Result<(), String> {
        let now = self.now();
        let until = match r.state.as_str() {
            state::PENDING => r.expires_at.unwrap_or(now + PLAN_TTL_S) + KEEP_AFTER_S,
            state::EXPIRED => r.expires_at.unwrap_or(now).max(now - KEEP_AFTER_S) + KEEP_AFTER_S,
            _ => r.decided_at.unwrap_or(now) + KEEP_AFTER_S,
        };
        let ttl_ms = (until - now).max(60) as u64 * 1000;
        self.receipts
            .put(self.shared.get(), plan_id, &r.to_json().to_string(), ttl_ms)
    }

    /// Write a receipt after a decision ran: its loss is told on stderr and costs only the
    /// answer the agent gets (the plan then reads as not found, never as approved).
    fn put_receipt_after(&self, plan_id: &str, r: &Receipt) {
        let put = obop_storage::fault::bookkeeping(|| self.put_receipt(plan_id, r));
        if let Err(e) = put {
            eprintln!(
                "the receipt of plan {plan_id} ({}) could not be written: {e}",
                r.command
            );
        }
    }

    fn log(&self, kind: &str, proposer: &str, actor: &str, plan_id: &str, r: &Receipt) {
        if let Some(s) = self.shared.get() {
            let what = match &r.target {
                Some(t) => format!("plan {plan_id}: {} on {}:{t}", r.command, r.model),
                None => format!("plan {plan_id}: {}", r.command),
            };
            audit::record_event(
                &*s.backend,
                self.now().max(0) as u64,
                proposer,
                kind,
                actor,
                &what,
            );
        }
    }

    /// A receipt for a plan that has none (made before D97): what the plan itself says.
    fn receipt_from(&self, p: &Pending) -> Receipt {
        Receipt {
            proposer: p.principal.id.clone(),
            command: p.id.clone(),
            model: self
                .registered(&p.id)
                .map(|c| c.model.to_string())
                .unwrap_or_default(),
            target: self.target_for(&p.id, &p.input),
            created_at: None,
            expires_at: None,
            state: state::PENDING.into(),
            ..Receipt::default()
        }
    }

    /// The proposal path of `call`: refuse the 101st waiting plan of an agent, then store the plan
    /// and its receipt and write `plan_proposed`. Called under the transaction lock.
    pub(crate) fn propose(&self, plan_id: &str, plan: &Pending) -> Result<(), CallError> {
        let now = self.now();
        let waiting = self
            .receipts
            .all(self.shared.get())
            .map_err(CallError::Handler)?
            .into_iter()
            .filter_map(|(_, t)| Receipt::from_json(&serde_json::from_str(&t).ok()?))
            .filter(|r| {
                r.proposer == plan.principal.id && r.state == state::PENDING && !r.ran_out(now)
            })
            .count();
        if waiting >= MAX_WAITING_PER_AGENT {
            return Err(CallError::Handler(errors::coded(
                errors::code::TOO_MANY_WAITING,
                &format!(
                    "this agent already has {MAX_WAITING_PER_AGENT} requests waiting; a person must decide some first"
                ),
            )));
        }
        let mut r = self.receipt_from(plan);
        r.created_at = Some(now);
        r.expires_at = Some(now + PLAN_TTL_S);
        // The receipt first: a plan without one would still work (an old process makes those),
        // but a receipt without a plan only ever reads as not found.
        self.put_receipt(plan_id, &r).map_err(CallError::Handler)?;
        self.plans
            .put(self.shared.get(), plan_id, plan)
            .map_err(CallError::Handler)?;
        self.log(
            event::PROPOSED,
            &plan.principal.id,
            &plan.principal.id,
            plan_id,
            &r,
        );
        Ok(())
    }

    /// A plan past its end: taken out, its receipt marked, `plan_expired` written once in all
    /// processes. `seen` is the receipt as the caller read it, maybe a while ago: the step takes
    /// the transaction lock that approve and reject hold, reads the receipt again and does nothing
    /// unless it still waits, so a decision that ran in the meantime is never relabelled expired.
    pub(crate) fn settle_expired(&self, plan_id: &str, seen: &Receipt) {
        if !seen.ran_out(self.now()) {
            return;
        }
        let Ok(_txn) = self.txn_guard() else {
            return;
        };
        self.settle_expired_locked(plan_id);
    }

    /// [`Dispatcher::settle_expired`] for a caller that holds the transaction lock.
    fn settle_expired_locked(&self, plan_id: &str) {
        let Some(r) = self.receipt(plan_id) else {
            return;
        };
        if !r.ran_out(self.now()) || !self.receipts.first_to_expire(self.shared.get(), plan_id) {
            return;
        }
        let _ = obop_storage::fault::bookkeeping(|| self.plans.remove(self.shared.get(), plan_id));
        let mut done = r.clone();
        done.state = state::EXPIRED.into();
        self.put_receipt_after(plan_id, &done);
        self.log(event::EXPIRED, &r.proposer, "system", plan_id, &r);
    }

    fn item(&self, plan_id: &str) -> Item {
        let pending = self.plans.get(self.shared.get(), plan_id);
        Item {
            id: plan_id.to_string(),
            pending_target: pending
                .as_ref()
                .and_then(|p| self.target_for(&p.id, &p.input)),
            pending,
            receipt: self.receipt(plan_id),
        }
    }

    /// Every plan the store knows, waiting or decided, oldest first; plans past their end are
    /// settled on the way.
    fn items(&self) -> Result<Vec<Item>, CallError> {
        let now = self.now();
        let mut by: BTreeMap<String, Item> = BTreeMap::new();
        for (id, text) in self
            .receipts
            .all(self.shared.get())
            .map_err(CallError::Handler)?
        {
            if let Some(r) = serde_json::from_str(&text)
                .ok()
                .and_then(|j| Receipt::from_json(&j))
            {
                by.insert(
                    id.clone(),
                    Item {
                        id,
                        pending: None,
                        pending_target: None,
                        receipt: Some(r),
                    },
                );
            }
        }
        for (id, p) in self
            .plans
            .all(self.shared.get())
            .map_err(CallError::Handler)?
        {
            let it = by.entry(id.clone()).or_insert_with(|| Item {
                id,
                pending: None,
                pending_target: None,
                receipt: None,
            });
            it.pending_target = self.target_for(&p.id, &p.input);
            it.pending = Some(p);
        }
        for it in by.values() {
            if let Some(r) = &it.receipt
                && r.ran_out(now)
            {
                self.settle_expired(&it.id, r);
            }
        }
        Ok(by.into_values().collect())
    }

    fn rights(&self, look: &mut Look, it: &Item) -> Rights {
        let command = it.command().to_string();
        let target = it.target();
        let decider = look.could_decide(&command, target.as_deref());
        let runs = match (&it.pending, self.registered(&command)) {
            (Some(p), Some(c)) => look.proposer_runs(&p.principal, c.permission_group),
            _ => false,
        };
        Rights {
            decider,
            runs,
            owner: look.is_owner(it.proposer()),
        }
    }

    // ---- what a person sees ----------------------------------------------------------------

    /// The row of a plan for the person of `look`.
    fn row(&self, look: &mut Look, it: &Item, status: &str, rights: Option<&Rights>) -> J {
        let command = it.command();
        let reg = self.registered(command);
        let model = reg
            .as_ref()
            .map(|c| c.model.to_string())
            .or_else(|| it.receipt.as_ref().map(|r| r.model.clone()))
            .unwrap_or_default();
        let action = command.rsplit('.').next().unwrap_or(command).to_string();
        let target = it.target();
        let title = match (&target, rights.is_some_and(|r| r.decider)) {
            (Some(id), true) => match look.read(&model, id) {
                Read::Yes(rec) => self.title_of(&model, &rec),
                _ => None,
            },
            _ => None,
        };
        let r = it.receipt.clone().unwrap_or_default();
        let proposer = it.proposer().to_string();
        json!({
            "id": it.id,
            "command": command,
            "model": model,
            "action": action,
            "kind": self.kind_of(command, reg.as_deref()),
            "record_id": target,
            "record_title": title,
            "proposer": proposer,
            "owner": owner_of(&proposer),
            "own_agent": look.is_owner(&proposer),
            "created_at": rfc(r.created_at),
            "expires_at": rfc(r.expires_at),
            "status": status,
            "decided_by": r.decided_by,
            "decided_at": rfc(r.decided_at),
            "reason": r.reason,
            "failure": r.failure,
            "destructive": self.destructive(command),
            "can_approve": status == "waiting" && rights.is_some_and(Rights::can_approve),
            "can_reject": status == "waiting" && rights.is_some_and(|r| r.decider),
            "proposer_can_run": status == "waiting" && rights.is_some_and(|r| r.runs),
        })
    }

    fn kind_of(&self, command: &str, c: Option<&Registered>) -> &'static str {
        let short = command.rsplit('.').next().unwrap_or(command);
        match short {
            "update" => "update",
            "create" => "create",
            "bulk_create" => "bulk_create",
            "delete" => "delete",
            _ if c.is_some_and(|c| c.transition.is_some()) => "action",
            _ => "other",
        }
    }

    /// A delete, and an action that declares `ui.danger` (D95).
    fn destructive(&self, command: &str) -> bool {
        let Some((model, short)) = command.split_once('.') else {
            return false;
        };
        short == "delete" || self.action_ui(model, short).is_some_and(|u| u.danger)
    }

    fn action_ui(
        &self,
        model: &str,
        short: &str,
    ) -> Option<std::sync::Arc<obop_registry::ActionUi>> {
        let models = self.models.read().unwrap();
        models
            .get(model)?
            .def
            .commands
            .iter()
            .find(|c| c.name == short)
            .and_then(|c| c.ui.clone())
    }

    /// The record's title by the model's `ui.title` template (only placeholders of its own
    /// fields), else its name, number, reference, code or title.
    fn title_of(&self, model: &str, rec: &Value) -> Option<String> {
        let Value::Map(m) = rec else { return None };
        let field = |k: &str| match m.get(k) {
            Some(Value::Text(t)) if !t.trim().is_empty() => Some(t.clone()),
            Some(Value::Int(i)) => Some(i.to_string()),
            Some(Value::Num(n)) => Some(n.clone()),
            _ => None,
        };
        let template = self
            .models
            .read()
            .unwrap()
            .get(model)
            .and_then(|m| m.def.ext.ui.as_ref().and_then(|u| u.title.clone()));
        if let Some(t) = template {
            'alt: for alt in t.split('|') {
                let mut out = String::new();
                let mut rest = alt;
                while let Some(open) = rest.find('{') {
                    out.push_str(&rest[..open]);
                    let Some(close) = rest[open..].find('}') else {
                        continue 'alt;
                    };
                    let name = &rest[open + 1..open + close];
                    match (name.contains('.'), field(name)) {
                        (false, Some(v)) => out.push_str(&v),
                        _ => continue 'alt,
                    }
                    rest = &rest[open + close + 1..];
                }
                out.push_str(rest);
                let out = out.trim().to_string();
                if !out.is_empty() {
                    return Some(out);
                }
            }
        }
        ["name", "number", "reference", "code", "title"]
            .iter()
            .find_map(|k| field(k))
    }

    /// Whether the person may see the value of `field` of `model`: they hold its group and its
    /// class is not one the masking denies them. A name the model does not declare is never shown.
    fn shown(&self, p: &Principal, model: &str, field: &str) -> bool {
        let models = self.models.read().unwrap();
        let Some(def) = models
            .get(model)
            .and_then(|m| m.fields.iter().find(|f| f.name == field))
        else {
            return false;
        };
        let class = def.class.unwrap_or(DataClass::Secret);
        p.allows(def.permission_group)
            && mode_for(p.kind, class, def.personal_data) == MaskMode::Allow
    }

    /// What a waiting plan changes, for the person of `look` (D97 "the change view"): the fields
    /// it sets that differ from the record as they read it now, `{field, from, to, hidden}`, a
    /// field they may not read or a secret one as hidden with no value. Also the record itself
    /// (as they read it), its version, the transition and the action's own texts.
    fn change_view(&self, look: &mut Look, plan: &Pending) -> J {
        let p = look.p;
        let reg = self.registered(&plan.id);
        let model = reg.as_ref().map(|c| c.model).unwrap_or("");
        let short = plan.id.rsplit('.').next().unwrap_or("");
        let record = match target_of(&plan.input) {
            Some(id) => match look.read(model, &id) {
                Read::Yes(v) => Some(v),
                _ => None,
            },
            None => None,
        };
        let now_of = |k: &str| match &record {
            Some(Value::Map(m)) => m.get(k).cloned().unwrap_or(Value::Null),
            _ => Value::Null,
        };
        let mut changes = Vec::new();
        let mut rows = J::Null;
        let mut rows_total = J::Null;
        let input = match &plan.input {
            Value::Map(m) => m.clone(),
            _ => BTreeMap::new(),
        };
        if short == "bulk_create" {
            if let Some(Value::List(list)) = input.get("rows") {
                rows_total = J::from(list.len());
                rows = J::Array(
                    list.iter()
                        .take(5)
                        .map(|row| {
                            let Value::Map(r) = row else { return J::Null };
                            J::Array(
                                r.iter()
                                    .map(|(k, v)| self.change(p, model, k, Value::Null, v.clone()))
                                    .collect(),
                            )
                        })
                        .collect(),
                );
            } else if let Some(Value::Text(csv)) = input.get("csv") {
                rows_total = J::from(
                    csv.lines()
                        .filter(|l| !l.trim().is_empty())
                        .count()
                        .saturating_sub(1),
                );
            }
        } else if short != "delete" {
            let transition = reg.as_ref().and_then(|c| c.transition);
            for (k, v) in &input {
                if k == "id" || k.starts_with('_') {
                    continue;
                }
                if transition.is_some_and(|t| t.field == k) {
                    continue;
                }
                let from = now_of(k);
                if record.is_some() && self.shown(p, model, k) && from == *v {
                    continue;
                }
                changes.push(self.change(p, model, k, from, v.clone()));
            }
            if let Some(t) = transition {
                changes.push(self.change(p, model, t.field, now_of(t.field), Value::text(t.to)));
            }
        }
        let ui = self.action_ui(model, short);
        json!({
            "changes": changes,
            "rows": rows,
            "rows_total": rows_total,
            "record": record.as_ref().map(Value::to_json),
            "seen_version": record.as_ref().map(version_of),
            "transition": reg.as_ref().and_then(|c| c.transition).map(|t| json!({"field": t.field, "from": t.from, "to": t.to})),
            "confirm": ui.as_ref().and_then(|u| u.confirm.clone()),
            "done": ui.as_ref().and_then(|u| u.done.clone()),
            "edges": [],
        })
    }

    fn change(&self, p: &Principal, model: &str, field: &str, from: Value, to: Value) -> J {
        if self.shown(p, model, field) {
            json!({"field": field, "from": from.to_json(), "to": to.to_json(), "hidden": false})
        } else {
            json!({"field": field, "from": null, "to": null, "hidden": true})
        }
    }

    // ---- the commands ----------------------------------------------------------------------

    fn plan_count(&self, p: &Principal) -> J {
        let mut look = Look::new(self, p);
        let now = self.now();
        let (mut waiting, mut mine) = (0, 0);
        for it in self.items().unwrap_or_default() {
            if it.status(now) != Some("waiting") {
                continue;
            }
            let r = self.rights(&mut look, &it);
            if r.decider {
                waiting += 1;
            }
            if r.owner {
                mine += 1;
            }
        }
        let can_decide = p.is_person()
            && self
                .commands
                .read()
                .unwrap()
                .values()
                .any(|c| c.needs_approval && p.allows(c.permission_group));
        json!({"waiting": waiting, "mine_waiting": mine, "can_decide": can_decide || waiting > 0})
    }

    fn plan_list(&self, p: &Principal, m: &BTreeMap<String, Value>) -> Result<J, CallError> {
        let bad = |s: &str| CallError::Handler(s.into());
        let text = |k: &str| match m.get(k) {
            Some(Value::Text(t)) if !t.is_empty() => Some(t.as_str()),
            _ => None,
        };
        let int = |k: &str| -> Result<Option<i64>, CallError> {
            match m.get(k) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::Int(n)) => Ok(Some(*n)),
                Some(Value::Text(t)) => t
                    .parse()
                    .map(Some)
                    .map_err(|_| bad(&format!("{k} must be a whole number"))),
                _ => Err(bad(&format!("{k} must be a whole number"))),
            }
        };
        let decided = match text("state") {
            None | Some("waiting") => false,
            Some("decided") => true,
            Some(_) => return Err(bad("state must be waiting or decided")),
        };
        let mine = match m.get("mine") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            Some(Value::Text(t)) if t == "true" || t == "false" => t == "true",
            _ => return Err(bad("mine must be true or false")),
        };
        let limit = int("limit")?.unwrap_or(LIST_MAX as i64);
        if !(1..=LIST_MAX as i64).contains(&limit) {
            return Err(bad(&format!("limit must be from 1 to {LIST_MAX}")));
        }
        let offset = int("offset")?.unwrap_or(0);
        if offset < 0 {
            return Err(bad("offset must not be negative"));
        }
        let (model, record) = (text("model"), text("record_id"));
        let now = self.now();
        let mut look = Look::new(self, p);
        let mut rows: Vec<(i64, String, J)> = Vec::new();
        for it in self.items()? {
            let Some(status) = it.status(now) else {
                continue;
            };
            if (status != "waiting") != decided {
                continue;
            }
            if mine && !look.is_owner(it.proposer()) {
                continue;
            }
            let reg = self.registered(it.command());
            let it_model = reg
                .as_ref()
                .map(|c| c.model.to_string())
                .or_else(|| it.receipt.as_ref().map(|r| r.model.clone()))
                .unwrap_or_default();
            if model.is_some_and(|x| x != it_model)
                || record.is_some_and(|x| it.target().as_deref() != Some(x))
            {
                continue;
            }
            let row = if decided {
                let Some(r) = &it.receipt else { continue };
                if !look.may_see_decided(r) {
                    continue;
                }
                self.row(&mut look, &it, status, None)
            } else {
                let rights = self.rights(&mut look, &it);
                if !rights.visible() {
                    continue;
                }
                self.row(&mut look, &it, status, Some(&rights))
            };
            let when = it
                .receipt
                .as_ref()
                .and_then(|r| r.decided_at.or(r.expires_at))
                .unwrap_or(0);
            rows.push((when, it.id.clone(), row));
        }
        if decided {
            // Newest decision first.
            rows.sort_by(|a, b| (b.0, &b.1).cmp(&(a.0, &a.1)));
        } else {
            // Oldest first (the ids are UUIDv7, ordered by time): the one nearest to its end.
            rows.sort_by(|a, b| a.1.cmp(&b.1));
        }
        let total = rows.len();
        let page: Vec<J> = rows
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|(_, _, r)| r)
            .collect();
        Ok(json!({"rows": page, "total": total}))
    }

    fn plan_get(&self, p: &Principal, plan_id: &str) -> Result<J, CallError> {
        let not_found = || CallError::UnknownPlan(plan_id.into());
        let now = self.now();
        let it = self.item(plan_id);
        if let Some(r) = &it.receipt
            && r.ran_out(now)
        {
            self.settle_expired(plan_id, r);
        }
        let status = it.status(now).ok_or_else(not_found)?;
        if !p.is_person() {
            // An agent: its own plans only, and what to do next.
            if it.proposer() != p.id {
                return Err(not_found());
            }
            let r = it.receipt.clone().unwrap_or_default();
            return Ok(json!({
                "id": plan_id,
                "command": it.command(),
                "record_id": it.target(),
                "status": status,
                "created_at": rfc(r.created_at),
                "expires_at": rfc(r.expires_at),
                "decided_at": rfc(r.decided_at),
                "reason": r.reason,
                "failure": r.failure,
                "instruction": agent_instruction(status),
            }));
        }
        let mut look = Look::new(self, p);
        if status != "waiting" {
            let r = it.receipt.as_ref().ok_or_else(not_found)?;
            if !look.may_see_decided(r) {
                return Err(not_found());
            }
            return Ok(self.row(&mut look, &it, status, None));
        }
        let rights = self.rights(&mut look, &it);
        if !rights.visible() {
            return Err(not_found());
        }
        let mut out = self.row(&mut look, &it, status, Some(&rights));
        // The owner who could not decide sees no field of a record they cannot read.
        let readable = match (it.target(), self.registered(it.command())) {
            (Some(id), Some(c)) => !matches!(look.read(c.model, &id), Read::No),
            _ => true,
        };
        if let (Some(plan), true) = (&it.pending, rights.decider || readable)
            && let (J::Object(o), J::Object(view)) = (&mut out, self.change_view(&mut look, plan))
        {
            o.extend(view);
        }
        Ok(out)
    }

    /// The answer for a plan that does not wait any more, to the person who asked to decide it:
    /// who decided it and when, if they may see it, else not found. `legacy` (the REST alias)
    /// answers not found as it always has.
    fn gone(&self, look: &mut Look, plan_id: &str, legacy: bool, locked: bool) -> CallError {
        if legacy {
            return CallError::UnknownPlan(plan_id.into());
        }
        let now = self.now();
        let mut receipt = self.receipt(plan_id);
        // A waiting receipt without its plan is a decision in flight in another call: it holds the
        // lock until the receipt says what became of the plan, so the answer waits for it.
        if !locked
            && receipt
                .as_ref()
                .is_some_and(|r| r.state == state::PENDING && !r.ran_out(now))
            && let Ok(_txn) = self.txn_guard()
        {
            receipt = self.receipt(plan_id);
        }
        match receipt {
            Some(r) if r.state != state::PENDING || r.ran_out(now) => {
                if !look.may_see_decided(&r) {
                    return CallError::UnknownPlan(plan_id.into());
                }
                let expired = r.ran_out(now) || r.state == state::EXPIRED;
                CallError::AlreadyDecided {
                    state: if expired {
                        state::EXPIRED.into()
                    } else {
                        r.state.clone()
                    },
                    by: if expired { None } else { r.decided_by.clone() },
                    at: if expired {
                        r.expires_at.map(|t| audit::rfc3339(t.max(0) as u64))
                    } else {
                        r.decided_at.map(|t| audit::rfc3339(t.max(0) as u64))
                    },
                }
            }
            _ => CallError::UnknownPlan(plan_id.into()),
        }
    }

    /// Approve a plan: `plan.approve` (`legacy` false) and `POST /api/approve/{plan_id}` (`legacy`
    /// true, through [`Dispatcher::approve`]).
    pub(crate) fn decide(
        &self,
        approver: &Principal,
        vault: &mut crate::Vault,
        plan_id: &str,
        seen_version: Option<i64>,
        legacy: bool,
    ) -> Result<Response, CallError> {
        self.sync();
        if !approver.is_person() {
            return Err(if legacy {
                CallError::Forbidden("approve".into())
            } else {
                CallError::UnknownCommand(format!("{MODEL}.approve"))
            });
        }
        let now = self.now();
        let mut look = Look::new(self, approver);
        // Who may decide is asked before the lock: it reads the record through `get`.
        let it = self.item(plan_id);
        let Some(first) = &it.pending else {
            return Err(self.gone(&mut look, plan_id, legacy, false));
        };
        if let Some(r) = &it.receipt
            && r.ran_out(now)
        {
            self.settle_expired(plan_id, r);
            return Err(self.gone(&mut look, plan_id, legacy, false));
        }
        if !self.rights(&mut look, &it).decider {
            return Err(if legacy {
                CallError::Forbidden(first.id.clone())
            } else {
                CallError::UnknownPlan(plan_id.into())
            });
        }
        let _txn = self.txn_guard()?;
        // The plan may have been decided by another process meanwhile.
        let Some(plan) = self.plans.get(self.shared.get(), plan_id) else {
            return Err(self.gone(&mut look, plan_id, legacy, true));
        };
        // It may have run out while this call waited for the lock.
        if self.receipt(plan_id).is_some_and(|r| r.ran_out(self.now())) {
            self.settle_expired_locked(plan_id);
            return Err(self.gone(&mut look, plan_id, legacy, true));
        }
        let c = self
            .registered(&plan.id)
            .ok_or_else(|| CallError::UnknownCommand(plan.id.clone()))?;
        // The agent runs as its owner's account is now: not at all when it is disabled or gone,
        // and without a group the owner has lost since the proposal.
        let proposer = self
            .proposer_now(&plan.principal)
            .filter(|q| q.allows(c.permission_group));
        let Some(proposer) = proposer.filter(|_| approver.allows(c.permission_group)) else {
            return Err(CallError::Forbidden(plan.id.clone()));
        };
        // They approve what they saw: the record is read again, as the person, under the lock.
        // (The generated `get` takes no lock of its own.)
        if let (Some(want), Some(id)) = (seen_version, target_of(&plan.input)) {
            let mut fresh = Look::new(self, approver);
            let current = match fresh.read(c.model, &id) {
                Read::Yes(rec) => Some(version_of(&rec)),
                _ => None,
            };
            if current != Some(want) {
                return Err(CallError::PreconditionFailed {
                    current: current.unwrap_or(0),
                });
            }
        }
        // The record may have changed since the proposal, so the guard runs again, and the
        // approver must hold the groups of any transition the call implicitly runs (D48).
        let mut input = plan.input.clone();
        // The guard sees who is approving, so that it can refuse before the plan is spent.
        crate::stamp_approver(&mut input, approver);
        let mut pre = Vec::new();
        let guard = self.guards.read().unwrap().get(&plan.id).cloned();
        if let Some(g) = guard {
            let v = (g.guard)(&proposer, &input).map_err(|m| crate::refusal(m, &input))?;
            if v.requires.iter().any(|r| !approver.allows(r)) {
                return Err(CallError::Forbidden(plan.id.clone()));
            }
            pre = v.pre;
            if let Some(i) = v.input {
                input = i;
            }
        }
        // A module's screen of the command (`Dispatcher::screen`) checks again before the plan is
        // spent, so a refusal leaves the plan for another approver; it sees the principal the
        // plan runs as, as the guard does.
        self.run_screen(&plan.id, &proposer, &mut input)?;
        // Taken out first, so it runs once; if it is still there the plan would run again.
        self.plans
            .remove(self.shared.get(), plan_id)
            .map_err(CallError::Handler)?;
        // The person who approved is who a handler may name in its log; the plan keeps the agent
        // that proposed it as the caller. Stamped again after the guard, which may have rewritten
        // the input.
        crate::stamp_approver(&mut input, approver);
        let result = self.run(&c, &proposer, vault, input, pre);
        // The store refused before the call changed anything: the plan goes back (D74; see
        // `put_plan_back`) and the receipt still says it waits.
        let st = obop_storage::fault::state();
        if st.fault.is_some()
            && !st.wrote
            && !st.in_flight
            && self.put_plan_back(approver, plan_id, &plan)
        {
            return result;
        }
        let mut r = self
            .receipt(plan_id)
            .filter(|r| r.state == state::PENDING)
            .unwrap_or_else(|| self.receipt_from(&plan));
        r.decided_by = Some(approver.id.clone());
        r.decided_at = Some(self.now());
        match &result {
            Ok(_) => {
                r.state = state::APPROVED.into();
                self.put_receipt_after(plan_id, &r);
                self.log(event::APPROVED, &r.proposer, &approver.id, plan_id, &r);
            }
            Err(e) => {
                r.state = state::FAILED.into();
                // The code of the refusal, never its text: a refusal may echo what the agent sent.
                r.failure = Some(errors::classify(e).code.to_string());
                self.put_receipt_after(plan_id, &r);
                self.log(event::FAILED, &r.proposer, &approver.id, plan_id, &r);
            }
        }
        result
    }

    /// Reject a waiting plan: nothing runs, the receipt keeps who, when and the reason.
    fn reject(&self, p: &Principal, plan_id: &str, reason: Option<String>) -> Result<J, CallError> {
        let now = self.now();
        let mut look = Look::new(self, p);
        let it = self.item(plan_id);
        if it.pending.is_none() {
            return Err(self.gone(&mut look, plan_id, false, false));
        }
        if let Some(r) = &it.receipt
            && r.ran_out(now)
        {
            self.settle_expired(plan_id, r);
            return Err(self.gone(&mut look, plan_id, false, false));
        }
        if !self.rights(&mut look, &it).decider {
            return Err(CallError::UnknownPlan(plan_id.into()));
        }
        let decided = {
            let _txn = self.txn_guard()?;
            let Some(plan) = self.plans.get(self.shared.get(), plan_id) else {
                return Err(self.gone(&mut look, plan_id, false, true));
            };
            // It may have run out while this call waited for the lock.
            if self.receipt(plan_id).is_some_and(|r| r.ran_out(self.now())) {
                self.settle_expired_locked(plan_id);
                return Err(self.gone(&mut look, plan_id, false, true));
            }
            self.plans
                .remove(self.shared.get(), plan_id)
                .map_err(CallError::Handler)?;
            let mut r = self
                .receipt(plan_id)
                .filter(|r| r.state == state::PENDING)
                .unwrap_or_else(|| self.receipt_from(&plan));
            r.state = state::REJECTED.into();
            r.decided_by = Some(p.id.clone());
            r.decided_at = Some(self.now());
            r.reason = reason;
            self.put_receipt_after(plan_id, &r);
            self.log(event::REJECTED, &r.proposer, &p.id, plan_id, &r);
            r
        };
        let it = Item {
            id: plan_id.to_string(),
            pending: None,
            pending_target: None,
            receipt: Some(decided),
        };
        Ok(self.row(&mut look, &it, state::REJECTED, None))
    }
}
