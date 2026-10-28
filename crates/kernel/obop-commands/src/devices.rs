//! The `device` records of native apps (the D64 amendment, D87): one record for each sign-in of a
//! mobile client, created in the same write as the refresh-token family it is bound to, ended
//! together with it. The records are written by `obop-oauth` (registration, last seen, revocation)
//! and `obop-auth` (offboarding, a changed credential), and read, renamed and revoked by people
//! through the `device` commands of the Customize module (`obop-pkg`). This module holds what the
//! three share: the names, the shape of a record, the rules for the texts an app sends, and the
//! one rule that says who may see a device.
//!
//! Whether this model and the `pos_device` record of the POS design (D28) are one model is open
//! (docs/spec/mobile.md 15, item 2); nothing here decides it.

use crate::Principal;
use obop_storage::Fields;
use serde_json::Value as J;

/// The model (and the storage table) of the records.
pub const MODEL: &str = "device";

/// Reads and revokes the devices of everybody; the instance administrator holds it implicitly.
pub const ADMIN_GROUP: &str = "api.device_admin";

/// The two states of the governed `state` field: `revoke` moves one to the other, once.
pub const ACTIVE: &str = "active";
pub const REVOKED: &str = "revoked";

/// The security-log kinds of a device.
pub const EVENT_REGISTERED: &str = "device_registered";
pub const EVENT_RENAMED: &str = "device_renamed";
pub const EVENT_REVOKED: &str = "device_revoked";

pub const MAX_NAME_CHARS: usize = 80;
pub const MAX_PLATFORM_CHARS: usize = 24;
pub const MAX_VERSION_CHARS: usize = 32;

/// What a device is called when the app did not say.
pub const DEFAULT_NAME: &str = "Mobile device";
pub const DEFAULT_PLATFORM: &str = "unknown";

/// A point in time in the canonical text of a `datetime` field (D72).
pub fn instant(secs: u64) -> String {
    i64::try_from(secs)
        .ok()
        .and_then(obop_values::Instant::from_secs)
        .map(obop_values::Instant::canonical)
        .unwrap_or_default()
}

/// A character that cannot be seen or that changes how the text around it is shown or read: the
/// control characters; the format characters (soft hyphen, the Arabic letter mark, the zero-width
/// and direction marks and overrides, the invisible operators and the isolates, the byte order
/// mark, the interlinear annotations); the line and paragraph separators; the invisible fillers;
/// the Mongolian free variation selectors; and the Tag block, which can carry a whole hidden
/// sentence. A text from an application that holds any of these is refused (a device name) or
/// stripped (the name of a client).
pub fn is_invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00ad}'
                | '\u{061c}'
                | '\u{115f}'
                | '\u{1160}'
                | '\u{17b4}'
                | '\u{17b5}'
                | '\u{180b}'..='\u{180f}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{3164}'
                | '\u{fe00}'..='\u{fe0e}'
                | '\u{feff}'
                | '\u{ffa0}'
                | '\u{fff9}'..='\u{fffb}'
                | '\u{1bca0}'..='\u{1bca3}'
                | '\u{1d173}'..='\u{1d17a}'
                | '\u{e0000}'..='\u{e007f}'
                | '\u{e0100}'..='\u{e01ef}'
        )
}

/// A device name as stored: trimmed, 1 to [`MAX_NAME_CHARS`] characters, no character that
/// [`is_invisible`]. `None` for anything else.
pub fn name_of(raw: &str) -> Option<String> {
    let s = raw.trim();
    let n = s.chars().count();
    ((1..=MAX_NAME_CHARS).contains(&n) && !s.chars().any(is_invisible)).then(|| s.to_string())
}

/// A platform name as stored: lower case letters, digits, `.`, `_` and `-`, starting with a
/// letter, at most [`MAX_PLATFORM_CHARS`] characters (`android`, `ios`, `ipados`).
pub fn platform_of(raw: &str) -> Option<String> {
    let s = raw.trim().to_ascii_lowercase();
    let ok = !s.is_empty()
        && s.len() <= MAX_PLATFORM_CHARS
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    ok.then_some(s)
}

/// An app version as stored: letters, digits, `.`, `_`, `+` and `-`, at most
/// [`MAX_VERSION_CHARS`] characters (`1.4.2`, `2026.10.1+45`).
pub fn version_of(raw: &str) -> Option<String> {
    let s = raw.trim();
    let ok = !s.is_empty()
        && s.len() <= MAX_VERSION_CHARS
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    ok.then(|| s.to_string())
}

/// What an app says about the device it runs on, already checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub name: Option<String>,
    pub platform: Option<String>,
    pub app_version: Option<String>,
}

/// A new record. `grant` is the id of the refresh-token family, `expires_secs` the end of that
/// family (the end of the sign-in).
pub fn new_record(
    user: &str,
    client_name: &str,
    meta: &Metadata,
    grant: &str,
    now_secs: u64,
    expires_secs: u64,
) -> Fields {
    let at = instant(now_secs);
    let mut f = Fields::new();
    for (k, v) in [
        ("user", J::from(user)),
        (
            "name",
            J::from(meta.name.clone().unwrap_or_else(|| DEFAULT_NAME.into())),
        ),
        ("client_name", J::from(client_name)),
        (
            "platform",
            J::from(
                meta.platform
                    .clone()
                    .unwrap_or_else(|| DEFAULT_PLATFORM.into()),
            ),
        ),
        (
            "app_version",
            J::from(meta.app_version.clone().unwrap_or_default()),
        ),
        ("registered_at", J::from(at.clone())),
        ("last_seen", J::from(at)),
        ("expires_at", J::from(instant(expires_secs))),
        ("state", J::from(ACTIVE)),
        ("grant", J::from(grant)),
    ] {
        f.insert(k.into(), v);
    }
    f
}

/// What ending a device changes in its record.
pub fn revoked_patch(now_secs: u64) -> Fields {
    let mut f = Fields::new();
    f.insert("state".into(), J::from(REVOKED));
    f.insert("revoked_at".into(), J::from(instant(now_secs)));
    f
}

/// Whether the record is one that is in force: not ended by a person, by reuse or by an offboarding.
pub fn is_active(rec: &Fields) -> bool {
    rec.get("state").and_then(J::as_str) == Some(ACTIVE)
}

/// Whether the record was written by the OAuth server (it names its refresh-token family and when
/// it was registered). A row of the table that does not is somebody else's (a package that once
/// had a model of this name) and is never touched by what ends devices.
pub fn is_ours(rec: &Fields) -> bool {
    rec.get("grant")
        .and_then(J::as_str)
        .is_some_and(|g| !g.is_empty())
        && rec.get("registered_at").is_some_and(J::is_string)
}

/// The account a principal acts for: the person behind an agent token is the person.
pub fn owner_of(p: &Principal) -> &str {
    p.id.strip_suffix("/agent").unwrap_or(&p.id)
}

/// What a call does to a device, for [`may_see`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Use {
    Read,
    Rename,
    Revoke,
}

/// Whether the principal holds a group that sees every device: [`ADMIN_GROUP`] or the instance
/// administrator (and, for a token, was handed it).
pub fn administers(p: &Principal) -> bool {
    p.allows(crate::ADMIN_GROUP) || p.allows(ADMIN_GROUP)
}

/// Whether the principal may list and search devices: a person at the keyboard (a session sees
/// their own, by the row access of the model) and the administrators (everybody's). A mobile app's
/// token and an agent's token may not: a narrowed token learns nothing of the person's other
/// devices.
pub fn may_list(p: &Principal) -> bool {
    p.is_person() || administers(p)
}

/// Who may see, rename and revoke the device `id` (the record `rec`):
///
/// - the holders of [`ADMIN_GROUP`] and the instance administrator, any device to read and to
///   revoke (docs/spec/mobile.md 5.7: "read and revoke any"), and their own to rename: what a
///   person calls their phone is theirs;
/// - a mobile app's token (a principal with a device), its own device and no other;
/// - an agent's token, only to *propose* the revoking of its owner's device (it cannot read,
///   list or rename);
/// - a person at the keyboard, their own devices.
///
/// The row access of the model (`own`, `see_all`) gives the generated reads the same for a person
/// and the administrators; the guards of the commands add the rest. A device that fails the rule is
/// reported exactly like one that does not exist.
pub fn may_see(p: &Principal, id: &str, rec: &Fields, what: Use) -> bool {
    if administers(p) && what != Use::Rename {
        return true;
    }
    let mine = !p.id.is_empty() && rec.get("user").and_then(J::as_str) == Some(owner_of(p));
    if let Some(own) = &p.device {
        return mine && own == id;
    }
    if p.kind == crate::PrincipalKind::Agent {
        return mine && what == Use::Revoke;
    }
    p.is_person() && mine
}

/// Groups a mobile app is never offered or handed, by name. They are the groups of
/// docs/CONVENTIONS.md section 4 whose holders approve, post, pay, erase, publish, export, see
/// special data or run standing outbound work: a phone is lost, and the token of an app is the
/// person themselves, so none of the approval gates that hold an AI agent (D18, D42) and none of
/// the masking of D40 apply to it. The patterns of [`never_for_mobile`] catch the same by role;
/// neither list is a boundary on its own: the boundary is what the client declared when it
/// registered and what the person ticks.
pub const NEVER_FOR_MOBILE: &[&str] = &[
    // Security, privacy and audit.
    "security.officer",
    "privacy.officer",
    "audit.reader",
    // Money: posting, paying, approving.
    "account.payment_approver",
    "account.manager",
    "account.bookkeeper",
    "account.billing",
    "account.adviser",
    "payroll.preparer",
    "payroll.approver",
    "expenses.finance",
    "purchase.manager",
    "commissions.manager",
    // People: special data, line management, recruitment decisions.
    "hr.special_reader",
    "hr.hrbp",
    "hr.line_manager",
    "hr.officer",
    "hr.admin",
    "recruitment.recruiter",
    // Integrations and standing outbound state (a connection runs as the person after the
    // device is gone), import and export of the whole business.
    "connectors.user",
    "api.import_operator",
    "api.export_owner",
    "api.webhook_admin",
    "api.connection_admin",
    "api.client_admin",
    "api.extension_admin",
    "api.device_admin",
    // Designers of automations, approvals and analytics.
    "customize.designer",
    "automation.designer",
    "approvals.designer",
    "analytics.designer",
    // Publishing to the public.
    "website.publisher",
    "website.editor",
];

/// Whether a group may never be handed to a mobile app: [`NEVER_FOR_MOBILE`], the groups of the
/// platform (`platform.*`), and every group named like a role with approval or administrative
/// power: `*.admin`, `*_admin`, `*.manager`, `*.approver`, `*_approver`, `*.officer`,
/// `*.designer`, `*.reviewer`, `*.publisher`, `*.preparer`, `*.owner`, `*_owner`.
pub fn never_for_mobile(group: &str) -> bool {
    const ENDS: [&str; 12] = [
        ".admin",
        "_admin",
        ".manager",
        ".approver",
        "_approver",
        ".officer",
        ".designer",
        ".reviewer",
        ".publisher",
        ".preparer",
        ".owner",
        "_owner",
    ];
    group == crate::ADMIN_GROUP
        || group.starts_with("platform.")
        || NEVER_FOR_MOBILE.contains(&group)
        || ENDS.iter().any(|e| group.ends_with(e))
}

/// Most groups a mobile client may declare.
pub const MAX_DECLARED_GROUPS: usize = 32;

/// The scope a mobile client declares at registration (RFC 7591 `scope`): space separated group
/// names, at most [`MAX_DECLARED_GROUPS`] of them, each a group name (letters, digits, `.`, `_`
/// and `-`, at most 64 characters). An error names the first that is not, or that
/// [`never_for_mobile`].
pub fn parse_declared_scope(raw: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for g in raw.split_whitespace() {
        let ok = g.len() <= 64
            && g.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if !ok {
            return Err(format!("{g:?} is not a group name"));
        }
        if never_for_mobile(g) {
            return Err(format!(
                "{g} can never be handed to a mobile app (administrative, approving or standing groups are refused)"
            ));
        }
        if !out.iter().any(|x| x == g) {
            out.push(g.to_string());
        }
    }
    if out.len() > MAX_DECLARED_GROUPS {
        return Err(format!("declare at most {MAX_DECLARED_GROUPS} groups"));
    }
    Ok(out)
}

/// How a device is named in the security log: its name, platform, app version and app, and its id.
/// No token and no secret is ever part of a record that could end up here.
pub fn describe(rec: &Fields, id: &str) -> String {
    let text = |k: &str| rec.get(k).and_then(J::as_str).unwrap_or("");
    let version = if text("app_version").is_empty() {
        String::new()
    } else {
        format!(" {}", text("app_version"))
    };
    format!(
        "{} ({}{version}, {}) [{id}]",
        text("name"),
        text("platform"),
        text("client_name")
    )
}

/// The port the `device` commands of the Customize module use to end a device. `obop-oauth`
/// implements it, so ending a device is the very code that ends a connected application (the
/// refresh-token family, the access token in circulation, the retry rules of D74).
pub trait DevicesApi: Send + Sync {
    /// End the device `id`: no refresh token of its family works again, its access token stops
    /// working, the record says `revoked`. `actor` is who the security log names: the person or
    /// administrator who revoked it, or who approved an agent's proposal; `proposed_by` is that
    /// agent. A step the store refuses is an error whose text starts with the storage code word (a
    /// 503 for the caller), never a success; asking again finishes what was left undone.
    fn revoke_device(&self, id: &str, actor: &str, proposed_by: Option<&str>)
    -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_platforms_and_versions_are_checked() {
        assert_eq!(name_of("  Pixel 8  ").as_deref(), Some("Pixel 8"));
        assert!(name_of("Ada's phone \u{1F4F1}").is_some());
        assert_eq!(name_of(""), None);
        assert_eq!(name_of("   "), None);
        assert_eq!(name_of(&"x".repeat(81)), None);
        assert!(name_of(&"x".repeat(80)).is_some());
        assert_eq!(name_of("a\u{0}b"), None);
        assert_eq!(name_of("a\nb"), None);
        assert_eq!(name_of("evil\u{202e}name"), None);
        assert_eq!(platform_of("Android").as_deref(), Some("android"));
        assert_eq!(platform_of("ipados-18.1").as_deref(), Some("ipados-18.1"));
        for bad in [
            "",
            "1ios",
            "and roid",
            "android;drop",
            "a".repeat(25).as_str(),
            "ünï",
        ] {
            assert_eq!(platform_of(bad), None, "{bad:?}");
        }
        assert_eq!(version_of("1.4.2+45").as_deref(), Some("1.4.2+45"));
        for bad in ["", "1 4", "1.4\n2", "<b>", "x".repeat(33).as_str()] {
            assert_eq!(version_of(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn nothing_that_hides_or_reorders_text_is_a_name() {
        // Every one of these is invisible or changes how text is shown or read; a name that holds
        // one is refused, and the same predicate strips a client's name.
        for c in [
            '\u{00ad}',
            '\u{061c}',
            '\u{115f}',
            '\u{1160}',
            '\u{17b4}',
            '\u{180e}',
            '\u{200b}',
            '\u{200d}',
            '\u{200e}',
            '\u{2028}',
            '\u{2029}',
            '\u{202a}',
            '\u{202e}',
            '\u{2060}',
            '\u{2062}',
            '\u{2066}',
            '\u{2069}',
            '\u{206a}',
            '\u{206f}',
            '\u{3164}',
            '\u{fe00}',
            '\u{feff}',
            '\u{ffa0}',
            '\u{fff9}',
            '\u{e0001}',
            '\u{e0069}',
            '\u{e007f}',
            '\u{e0100}',
        ] {
            assert!(is_invisible(c), "{:x}", u32::from(c));
            assert_eq!(name_of(&format!("Ada{c}phone")), None, "{:x}", u32::from(c));
        }
        // What is meant to be seen stays: letters of every script, emoji with their presentation
        // selector, digits.
        for ok in [
            "Äli's Telefon",
            "Телефон",
            "電話",
            "Pixel 8 Pro",
            "📱 phone",
            "☎\u{fe0f}",
        ] {
            assert!(name_of(ok).is_some(), "{ok}");
        }
    }

    #[test]
    fn a_record_has_the_declared_shape_and_dates_in_the_canonical_form() {
        let m = Metadata {
            name: Some("Pixel 8".into()),
            platform: Some("android".into()),
            app_version: Some("1.0.0".into()),
        };
        let f = new_record("ada", "Mileage", &m, "og_1", 1_800_000_000, 1_802_592_000);
        assert_eq!(f["state"], ACTIVE);
        assert_eq!(f["registered_at"], "2027-01-15T08:00:00.000000Z");
        assert_eq!(f["expires_at"], "2027-02-14T08:00:00.000000Z");
        assert_eq!(f["name"], "Pixel 8");
        assert!(is_active(&f) && is_ours(&f));
        let p = revoked_patch(1_800_000_100);
        assert_eq!(p["state"], REVOKED);
        assert_eq!(p["revoked_at"], "2027-01-15T08:01:40.000000Z");
        let d = new_record("ada", "Mileage", &Metadata::default(), "og_1", 0, 0);
        assert_eq!(
            (d["name"].as_str(), d["platform"].as_str()),
            (Some(DEFAULT_NAME), Some(DEFAULT_PLATFORM))
        );
        assert!(!describe(&d, "x").contains('\n'));
        // A row of somebody else's table of the same name is not a device of ours.
        let mut foreign = Fields::new();
        foreign.insert("user".into(), J::from("ada"));
        foreign.insert("state".into(), J::from("active"));
        assert!(is_active(&foreign) && !is_ours(&foreign));
    }

    fn mobile(user: &str, device: &str, groups: &[&str]) -> Principal {
        Principal {
            scopes: Some(groups.iter().map(|g| g.to_string()).collect()),
            device: Some(device.to_string()),
            ..Principal::user(user, &["base.user", "sales.user", ADMIN_GROUP])
        }
    }

    #[test]
    fn who_may_see_a_device_for_every_kind_of_principal() {
        let f = new_record("ada", "Mileage", &Metadata::default(), "og_1", 0, 0);
        let all = [Use::Read, Use::Rename, Use::Revoke];
        let sees = |p: &Principal, id: &str, what: Use| may_see(p, id, &f, what);
        // A person at the keyboard: their own, nobody else's, and nobody owns nothing.
        let ada = Principal::user("ada", &["base.user"]);
        let bob = Principal::user("bob", &["base.user"]);
        for u in all {
            assert!(sees(&ada, "d1", u) && !sees(&bob, "d1", u));
            assert!(!sees(&Principal::user("", &[]), "d1", u));
        }
        assert!(may_list(&ada) && may_list(&bob));
        // The administrators of devices and of the instance: every device to read and to revoke
        // (docs/spec/mobile.md 5.7), but not to rename: a name is the owner's.
        for admin in [
            Principal::user("bob", &[ADMIN_GROUP]),
            Principal::user("bob", &["platform.admin"]),
        ] {
            assert!(may_list(&admin));
            assert!(sees(&admin, "d1", Use::Read) && sees(&admin, "d1", Use::Revoke));
            assert!(!sees(&admin, "d1", Use::Rename), "not somebody else's name");
        }
        for admin in [
            Principal::user("ada", &[ADMIN_GROUP]),
            Principal::user("ada", &["platform.admin"]),
        ] {
            assert!(sees(&admin, "d1", Use::Rename), "their own they rename");
        }
        // A mobile app's token: its own device (any use), another device of the same person never,
        // a device of somebody else never; it lists nothing.
        let app = mobile("ada", "d1", &["base.user"]);
        assert!(!app.is_person() && !may_list(&app));
        for u in all {
            assert!(sees(&app, "d1", u), "{u:?}");
            assert!(!sees(&app, "d2", u), "{u:?} another device of ada");
            assert!(!may_see(&mobile("bob", "d1", &["base.user"]), "d1", &f, u));
        }
        // The groups it holds do not make it an administrator: the token is narrowed.
        assert!(!administers(&app));
        // An agent's token: it may only propose the revoking of its owner's device.
        let agent = Principal::agent("ada/agent", &["base.user"], &["base.user"]);
        assert!(!may_list(&agent));
        assert!(sees(&agent, "d1", Use::Revoke));
        assert!(!sees(&agent, "d1", Use::Read) && !sees(&agent, "d1", Use::Rename));
        let bobs_agent = Principal::agent("bob/agent", &["base.user"], &["base.user"]);
        assert!(!sees(&bobs_agent, "d1", Use::Revoke));
        // An agent that was handed the device administration is an administrator.
        let admin_agent = Principal::agent("bob/agent", &[ADMIN_GROUP], &[ADMIN_GROUP]);
        assert!(may_list(&admin_agent) && sees(&admin_agent, "d1", Use::Read));
        // A token narrowed to other groups does not carry what its owner holds.
        let narrowed = Principal {
            scopes: Some(["base.user".to_string()].into()),
            ..Principal::user("bob", &[ADMIN_GROUP, "base.user"])
        };
        assert!(!administers(&narrowed) && !may_list(&narrowed));
        assert!(!sees(&narrowed, "d1", Use::Read));
    }

    #[test]
    fn groups_a_mobile_app_can_never_have() {
        // The groups of CONVENTIONS 4 that approve, post, pay, erase, export, publish or administer.
        for g in [
            "platform.admin",
            "platform.anything",
            "base.admin",
            "hr.admin",
            "pos.admin",
            "ai.admin",
            "api.device_admin",
            "api.connection_admin",
            "mail.account_admin",
            "marketing.deliverability_admin",
            "security.officer",
            "privacy.officer",
            "audit.reader",
            "account.payment_approver",
            "account.manager",
            "account.bookkeeper",
            "payroll.approver",
            "payroll.preparer",
            "timesheet.approver",
            "expenses.finance",
            "purchase.manager",
            "sales.manager",
            "crm.manager",
            "product.manager",
            "hr.special_reader",
            "hr.hrbp",
            "connectors.user",
            "api.import_operator",
            "api.export_owner",
            "approvals.designer",
            "website.publisher",
            "mileage.reviewer",
            "mileage.admin",
        ] {
            assert!(never_for_mobile(g), "{g}");
        }
        for g in [
            "base.user",
            "sales.user",
            "crm.user",
            "mileage.driver",
            "fleet.driver",
        ] {
            assert!(!never_for_mobile(g), "{g}");
        }
    }

    #[test]
    fn a_declared_scope_is_parsed_checked_and_refused_when_it_asks_for_what_a_phone_cannot_have() {
        assert_eq!(
            parse_declared_scope("mileage.driver  base.user mileage.driver").unwrap(),
            ["mileage.driver", "base.user"]
        );
        assert!(parse_declared_scope("").unwrap().is_empty());
        for bad in [
            "platform.admin",
            "mileage.driver sales.manager",
            "bad/name",
            "has;semicolon",
            &"x".repeat(65),
        ] {
            assert!(parse_declared_scope(bad).is_err(), "{bad}");
        }
        let many = (0..33)
            .map(|i| format!("g.n{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(parse_declared_scope(&many).is_err());
    }
}
