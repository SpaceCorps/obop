//! The first administrator of a new instance, made in the browser (D96, PROPOSED).
//!
//! While no account exists, each server process that starts makes a one-time **set-up code** (20
//! symbols of a 32-symbol alphabet, 100 bits), prints it in its own log and keeps only its SHA-256
//! in the shared expiring store for 60 minutes ([`Auth::open_setup`]). `POST /api/auth/setup`
//! creates the first administrator with it ([`SetupApi::complete`]). The rules, each held by a test
//! in `setup_tests.rs`:
//!
//! - **Closed for good.** Both routes answer "closed" as soon as any account exists, however it was
//!   made. The count of accounts is read strictly: a store that cannot count is an error, never
//!   "no accounts" (`Auth::user_count` answers 0 then, and is not used here).
//! - **Order of checks.** Closed, server key, limits, code, input; then the lock, closed again, code
//!   again, the writes. For a caller without the code no password is hashed and no account, brand
//!   or session is written. What such a caller does cause: the typed code is hashed to be compared,
//!   the guess counters are counted, and a throttled `setup_refused` line is written.
//! - **Fail closed.** The guess limits (10 in 10 minutes per address, 60 an hour in all) refuse
//!   when the counter cannot be kept (`obop_storage::limit::hit` lets a hit through then, which is
//!   right for a public page and wrong for this door). One answer for a wrong, expired, burnt,
//!   missing or unknown code.
//! - **A server key is required.** Without `OBOP_SECRET_KEY` the two-factor policy cannot be
//!   applied (`enrolment_required_for`), so the first administrator would hold a full session that
//!   never proved a second factor.
//! - **One winner.** The creation runs under the cross-process lock `auth.setup` and counts again
//!   inside it; `obop admin create` takes the same lock ([`Auth::create_user_from_cli`]). Lock order:
//!   `auth.setup`, then the brand's lock, then `auth.mfa`; never the reverse.
//! - **The code is never written down**: not in the security log, an error, an answer or a metric.
//!   The log line of the server's start is the only place it is shown.

use crate::admin::valid_name;
use crate::{Auth, MIN_PASSWORD_CHARS, hex, random_bytes, totp};
use obop_commands::brand::{self, Action, Failure};
use obop_commands::{
    ADMIN_GROUP, LoginOutcome, MfaError, NewUser, SetupApi, SetupError, SetupRequest, SetupStatus,
};
use sha2::{Digest, Sha256};

/// The prefix of the expiring values that hold the hashes of the codes in force.
pub(crate) const CODE_PREFIX: &str = "auth.setup.code:";
/// The cross-process lock of the creation; `obop admin create` takes it too.
pub const SETUP_LOCK: &str = "auth.setup";
/// How long a code works.
pub const CODE_TTL_MS: u64 = 60 * 60 * 1000;
/// The symbols of a code: no 0, O, 1 or I. 32 of them, so each random byte gives one symbol with
/// no bias (`b & 31`).
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// Symbols in a code: 20 x 5 bits = 100 bits.
const CODE_SYMBOLS: usize = 20;
/// Attempts per address in [`ADDR_WINDOW_MS`], and in all in [`ALL_WINDOW_MS`].
pub const ADDR_MAX: u64 = 10;
pub const ADDR_WINDOW_MS: u64 = 10 * 60 * 1000;
pub const ALL_MAX: u64 = 60;
pub const ALL_WINDOW_MS: u64 = 60 * 60 * 1000;
/// One `setup_refused` line per address and reason in this window, and at most
/// [`REFUSED_LINES_PER_HOUR`] in an hour in all.
const REFUSED_THROTTLE_MS: u64 = 10 * 60 * 1000;
const REFUSED_LINES_PER_HOUR: u64 = 30;
/// One `setup_open` line in this window, whichever process starts.
const OPEN_THROTTLE_MS: u64 = 10 * 60 * 1000;

/// What a server start did about set-up ([`Auth::open_setup`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupOpened {
    /// An account exists: no code.
    Closed,
    /// No account, but no server key: no code, the page says what is missing.
    NeedsSecretKey,
    /// A code was made: the text to print, in groups of five. Shown once, here.
    Code(String),
}

/// The text the hash is taken of: upper case, without spaces and hyphens.
fn normalise(code: &str) -> String {
    code.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// The key of the expiring value of a code.
fn code_key(code: &str) -> String {
    let digest = Sha256::digest(format!("obop-setup-code\n{}", normalise(code)).as_bytes());
    format!("{CODE_PREFIX}{}", hex(&digest))
}

fn new_code() -> String {
    let bytes = random_bytes::<CODE_SYMBOLS>();
    let symbols: Vec<char> = bytes
        .iter()
        .map(|b| char::from(ALPHABET[usize::from(b & 31)]))
        .collect();
    symbols
        .chunks(5)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// A hit of a guess limit: `Ok(None)` within the limit, `Ok(Some(seconds))` over it. A counter the
/// store would not keep is an error, so the attempt is refused (unlike `obop_storage::limit::hit`).
fn strict_hit(
    b: &dyn obop_storage::Backend,
    key: &str,
    max: u64,
    window_ms: u64,
) -> Result<Option<u64>, String> {
    let n = b.counter_add(key, window_ms, false)?;
    if n <= max {
        return Ok(None);
    }
    let left = b.counter_ttl_ms(key)?.unwrap_or(window_ms);
    Ok(Some(left.div_ceil(1000).max(1)))
}

fn unavailable(e: impl Into<String>) -> SetupError {
    SetupError::Unavailable(e.into())
}

impl Auth {
    /// Turn the set-up page on: the first administrator gets `groups`, which must be what
    /// `obop admin create NAME --all-groups` writes (the composition root passes the same list), less
    /// `*`, and must hold the administrator group. Until this is called, [`Authenticator::setup`] is `None`.
    ///
    /// [`Authenticator::setup`]: obop_commands::Authenticator::setup
    pub fn enable_setup(&self, groups: Vec<String>) -> Result<(), String> {
        if !groups.iter().any(|g| g == ADMIN_GROUP) {
            return Err(format!(
                "the groups of a first administrator must include {ADMIN_GROUP}"
            ));
        }
        // `*` means "any signed-in principal" and is no group to hold: `App::all_groups` lists it
        // because a model names it, and the account writer refuses it as a name.
        let groups: Vec<String> = groups
            .into_iter()
            .filter(|g| g != obop_commands::ANY_GROUP)
            .collect();
        self.setup_groups
            .set(groups)
            .map_err(|_| "the set-up page is already on".to_string())
    }

    pub(crate) fn setup_on(&self) -> bool {
        self.setup_groups.get().is_some()
    }

    /// The number of accounts, or the store's error: never 0 on a guess.
    fn strict_user_count(&self) -> Result<usize, String> {
        self.backend.count("auth_user")
    }

    /// At the start of `obop serve`: while no account exists and a server key is set, make a code,
    /// keep its hash for 60 minutes and return it for the log of this process. An earlier code of
    /// another process stays in force until its own time ends.
    pub fn open_setup(&self) -> Result<SetupOpened, String> {
        self.open_setup_for(CODE_TTL_MS)
    }

    pub(crate) fn open_setup_for(&self, ttl_ms: u64) -> Result<SetupOpened, String> {
        if self.strict_user_count()? > 0 {
            return Ok(SetupOpened::Closed);
        }
        if self.config.keys.is_empty() {
            return Ok(SetupOpened::NeedsSecretKey);
        }
        let mut stored = None;
        for _ in 0..3 {
            let code = new_code();
            if self
                .backend
                .kv_put_new(&code_key(&code), "1", Some(ttl_ms))?
            {
                stored = Some(code);
                break;
            }
        }
        let code = stored.ok_or("no new set-up code could be stored")?;
        // One line in ten minutes, whichever process starts: a crash loop does not fill the log.
        // A counter that cannot be read writes the line (a line too many is better than none).
        let first = self
            .backend
            .counter_add("auth.evt:setup_open", OPEN_THROTTLE_MS, false)
            .map_or(true, |n| n == 1);
        if first {
            self.event(
                "-",
                "setup_open",
                "-",
                "no account exists; a set-up code was printed in this server's log; it works once and for 60 minutes",
            );
        }
        Ok(SetupOpened::Code(code))
    }

    /// Whether `code` is one in force. Every code in force is compared, in constant time, with no
    /// early exit, so the time of the answer says nothing about which one was near.
    fn code_in_force(&self, code: &str) -> Result<bool, String> {
        let want = code_key(code);
        let held = self.backend.kv_scan(CODE_PREFIX)?;
        Ok(held.iter().fold(false, |hit, (k, _)| {
            totp::ct_eq(k.as_bytes(), want.as_bytes()) | hit
        }))
    }

    /// Remove every code in force: the first account exists. Returns how many could not be
    /// removed (they stop working anyway: the count is checked first).
    fn burn_codes(&self) -> usize {
        let held = obop_storage::fault::patiently(3, std::time::Duration::from_millis(100), || {
            self.backend.kv_scan(CODE_PREFIX)
        });
        let Ok(held) = held else {
            return 1;
        };
        held.iter()
            .filter(|(k, _)| {
                obop_storage::fault::patiently(3, std::time::Duration::from_millis(100), || {
                    self.backend.kv_delete(k)
                })
                .is_err()
            })
            .count()
    }

    /// A refused attempt, in the log: one line per address and reason in ten minutes, at most
    /// thirty an hour in all. The address is a key of a counter only, never in the text.
    fn refused(&self, source: &str, word: &str) {
        let key = format!(
            "auth.evt:setup:{}",
            hex(&Sha256::digest(format!("{source}\n{word}").as_bytes()))
        );
        let first = self
            .backend
            .counter_add(&key, REFUSED_THROTTLE_MS, false)
            .map_or(true, |n| n == 1);
        if !first {
            return;
        }
        let n = self
            .backend
            .counter_add("auth.evt:setup:hour", 60 * 60 * 1000, false)
            .unwrap_or(0);
        if n > REFUSED_LINES_PER_HOUR {
            if n == REFUSED_LINES_PER_HOUR + 1 {
                self.event(
                    "-",
                    "setup_refused",
                    "-",
                    "the lines about refused set-up attempts reached the ceiling of 30 an hour: the others of this hour are not written",
                );
            }
            return;
        }
        let why = match word {
            "code" => "the set-up code was wrong, expired or missing",
            "rate" => "too many attempts",
            "closed" => "an account exists",
            _ => "the server has no OBOP_SECRET_KEY",
        };
        self.event(
            "-",
            "setup_refused",
            "-",
            &format!(
                "a set-up attempt was refused: {why} (the next lines like this one from this address are not written for 10 minutes)"
            ),
        );
    }

    /// `obop admin create`: the same lock as the set-up page, so the two cannot both make a first
    /// account, and a line in the security log (the command wrote none before). When it makes the
    /// first account, the set-up codes are burnt and the log says the page is closed. Same rules
    /// and messages as [`Auth::create_user`].
    pub fn create_user_from_cli(
        &self,
        username: &str,
        password: &str,
        groups: &[String],
    ) -> Result<(), String> {
        let _lock = self.backend.lock(SETUP_LOCK)?;
        let before = self.strict_user_count()?;
        self.create_user(username, password, groups)?;
        let name = username.trim().to_lowercase();
        let list = if groups.is_empty() {
            "(none)".to_string()
        } else {
            groups.join(", ")
        };
        self.event(
            &name,
            "user_create",
            "operator",
            &format!("account created from the command line; groups: {list}"),
        );
        if before == 0 {
            let left = self.burn_codes();
            self.event(
                &name,
                "setup_complete",
                "operator",
                &format!(
                    "the first account was created from the command line; the set-up page is closed{}",
                    if left > 0 {
                        "; some set-up codes could not be removed, and they no longer work"
                    } else {
                        ""
                    }
                ),
            );
        }
        Ok(())
    }

    /// The checks before the lock, in the order of the rules: closed, key, limits, code, input.
    /// Returns the cleaned name and the instance name to write.
    fn setup_precheck(
        &self,
        req: &SetupRequest<'_>,
    ) -> Result<(String, Option<String>), SetupError> {
        if self.strict_user_count().map_err(unavailable)? > 0 {
            self.refused(req.source, "closed");
            return Err(SetupError::Closed);
        }
        if self.config.keys.is_empty() {
            self.refused(req.source, "needs_key");
            return Err(SetupError::NeedsSecretKey);
        }
        let b = &*self.backend;
        let over = match strict_hit(
            b,
            &format!("rl:setup:addr:{}", req.source),
            ADDR_MAX,
            ADDR_WINDOW_MS,
        )
        .map_err(unavailable)?
        {
            Some(wait) => Some(wait),
            None => strict_hit(b, "rl:setup:all", ALL_MAX, ALL_WINDOW_MS).map_err(unavailable)?,
        };
        if let Some(wait) = over {
            self.refused(req.source, "rate");
            return Err(SetupError::RateLimited(wait));
        }
        if !self.code_in_force(req.code).map_err(unavailable)? {
            self.refused(req.source, "code");
            return Err(SetupError::CodeInvalid);
        }
        // The code is proved: only now is the input looked at.
        let name = req.username.trim().to_lowercase();
        valid_name(&name).map_err(|e| SetupError::Invalid(describe(e)))?;
        if req.password.chars().count() < MIN_PASSWORD_CHARS {
            return Err(SetupError::Invalid(format!(
                "the password must be at least {MIN_PASSWORD_CHARS} characters"
            )));
        }
        let instance = req
            .instance_name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(String::from);
        if let Some(n) = &instance {
            let checked = brand::apply(&*self.backend, None, "setup", &name_action(n), true)
                .map_err(brand_failure)?;
            if !checked.refusals.is_empty() {
                return Err(SetupError::InstanceNameRefused(refusals_text(
                    &checked.refusals,
                )));
            }
        }
        Ok((name, instance))
    }
}

fn name_action(name: &str) -> Action {
    Action::Configure(vec![("name".to_string(), name.to_string())])
}

fn refusals_text(list: &[brand::Refused]) -> String {
    list.iter()
        .map(|r| format!("{}: {}", r.field, r.rule))
        .collect::<Vec<_>>()
        .join("; ")
}

fn brand_failure(f: Failure) -> SetupError {
    match f {
        Failure::Refused(list) => SetupError::InstanceNameRefused(refusals_text(&list)),
        Failure::Storage(e) => SetupError::Unavailable(e),
    }
}

fn describe(e: MfaError) -> String {
    match e {
        MfaError::Invalid(m)
        | MfaError::Denied(m)
        | MfaError::Locked(m)
        | MfaError::Unavailable(m) => m,
        MfaError::BadPassword => "wrong password".into(),
        MfaError::Expired => "expired".into(),
        MfaError::NotFound => "not allowed".into(),
    }
}

impl SetupApi for Auth {
    fn status(&self) -> Result<SetupStatus, SetupError> {
        if self.strict_user_count().map_err(unavailable)? > 0 {
            return Ok(SetupStatus {
                open: false,
                code_active: false,
                needs_secret_key: false,
            });
        }
        let needs_secret_key = self.config.keys.is_empty();
        let code_active = !needs_secret_key
            && !self
                .backend
                .kv_scan(CODE_PREFIX)
                .map_err(unavailable)?
                .is_empty();
        Ok(SetupStatus {
            open: true,
            code_active,
            needs_secret_key,
        })
    }

    fn complete(&self, req: &SetupRequest<'_>) -> Result<LoginOutcome, SetupError> {
        let Some(groups) = self.setup_groups.get() else {
            return Err(SetupError::Closed);
        };
        let (name, instance) = self.setup_precheck(req)?;
        let lock = self.backend.lock(SETUP_LOCK).map_err(unavailable)?;
        // Again, inside the lock: an account made meanwhile (by another request, another process
        // or the command line) closes the door, and so does a code burnt meanwhile.
        if self.strict_user_count().map_err(unavailable)? > 0 {
            self.refused(req.source, "closed");
            return Err(SetupError::Closed);
        }
        if !self.code_in_force(req.code).map_err(unavailable)? {
            self.refused(req.source, "code");
            return Err(SetupError::CodeInvalid);
        }
        // The name first: an authenticator app keeps the name it was given at enrolment.
        if let Some(n) = &instance {
            brand::apply(&*self.backend, None, "setup", &name_action(n), false)
                .map_err(brand_failure)?;
        }
        let new = NewUser {
            name: &name,
            password: Some(req.password),
            groups,
            must_change_password: false,
        };
        self.create_user_as("setup", &new, groups)
            .map_err(|e| match e {
                MfaError::Invalid(m) => SetupError::Invalid(m),
                other => SetupError::Unavailable(describe(other)),
            })?;
        let left = self.burn_codes();
        self.event(
            &name,
            "setup_complete",
            "setup",
            &format!(
                "the first administrator was created through the set-up page{}{}",
                if instance.is_some() {
                    "; the instance name was set"
                } else {
                    ""
                },
                if left > 0 {
                    "; some set-up codes could not be removed, and they no longer work"
                } else {
                    ""
                }
            ),
        );
        drop(lock);
        // The answer of a password sign-in: limited to enrolment where the policy says so. The
        // policy is applied to the groups just written, not to a read of the account: a read the
        // store refused would give no groups, and with them a full session that never proved a
        // second factor (`after_first_factor` reads them leniently). A new account has no second
        // factor yet, so there is no second step to ask for.
        let limited = self.enrolment_required_for(groups);
        if limited {
            self.event(
                &name,
                "limited_session",
                &name,
                "signed in without two-factor; only enrolment is allowed",
            );
        }
        self.session_outcome(&name, limited, &None)
            .map_err(|e| match e {
                crate::mfa::SessionError::Store(m) => SetupError::Unavailable(m),
                crate::mfa::SessionError::Refused => SetupError::Unavailable(
                    "the new account changed while the set-up was finishing; sign in instead"
                        .into(),
                ),
            })
    }
}

#[cfg(test)]
pub(crate) fn code_key_for_test(code: &str) -> String {
    code_key(code)
}

#[cfg(test)]
pub(crate) fn new_code_for_test() -> String {
    new_code()
}
