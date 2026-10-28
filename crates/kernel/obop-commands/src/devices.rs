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
/// control characters; the format characters (soft hyphen, the Arabic letter mark and the Arabic
/// number signs, the zero-width and direction marks and overrides, the invisible operators and
/// the isolates, the byte order mark, the interlinear annotations); the line and paragraph
/// separators; the characters that draw a blank cell (the Hangul and Braille fillers, the
/// combining grapheme joiner); the Mongolian free variation selectors; and the Tag block, which
/// can carry a whole hidden sentence. A text from an application has them stripped
/// ([`strip_hidden`]: a device name, platform and version, the name of a client).
pub fn is_invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00ad}'
                | '\u{034f}'
                | '\u{0600}'..='\u{0605}'
                | '\u{061c}'
                | '\u{06dd}'
                | '\u{070f}'
                | '\u{08e2}'
                | '\u{115f}'
                | '\u{1160}'
                | '\u{17b4}'
                | '\u{17b5}'
                | '\u{180b}'..='\u{180f}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{2800}'
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

/// The two joiners that scripts and emoji sequences need: the zero-width joiner (the family and
/// "woman technologist" emoji, Indic conjuncts) and the zero-width non-joiner (Persian, Kurdish,
/// Urdu, several Indic scripts). [`is_invisible`] holds them, [`strip_hidden`] keeps one where it
/// sits between two characters that are seen.
fn is_joiner(c: char) -> bool {
    matches!(c, '\u{200c}' | '\u{200d}')
}

/// A character that is drawn: not white space, not [`is_invisible`] (so not a joiner either).
fn is_seen(c: char) -> bool {
    !c.is_whitespace() && !is_invisible(c)
}

/// An app's text about its device with what hides or reorders text taken out, instead of the
/// whole text being refused (a phone named with an emoji family or in Persian must still sign
/// in): every [`is_invisible`] character is dropped, white space of any kind becomes one space,
/// the ends are trimmed, and a joiner stays only where it sits between two seen characters
/// (so a family emoji keeps its joiners and a joiner at the end, or next to a hidden character,
/// does not). The result can be empty.
pub fn strip_hidden(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::with_capacity(raw.len());
    for (i, &c) in chars.iter().enumerate() {
        if c.is_whitespace() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
        } else if is_joiner(c) {
            let between =
                i > 0 && is_seen(chars[i - 1]) && chars.get(i + 1).is_some_and(|n| is_seen(*n));
            if between {
                out.push(c);
            }
        } else if !is_invisible(c) {
            out.push(c);
        }
    }
    out.trim_end().to_string()
}

/// Longest raw text of an app that is looked at; a device name is short, and nothing is gained by
/// cleaning a megabyte.
const MAX_RAW_CHARS: usize = 512;

/// A device name as stored: what is left of `raw` after [`strip_hidden`], 1 to [`MAX_NAME_CHARS`]
/// characters, with at least one that is drawn (a variation selector alone is not). `None` when
/// nothing visible is left, or it is too long.
pub fn name_of(raw: &str) -> Option<String> {
    if raw.chars().count() > MAX_RAW_CHARS {
        return None;
    }
    let s = strip_hidden(raw);
    let n = s.chars().count();
    ((1..=MAX_NAME_CHARS).contains(&n) && s.chars().any(|c| is_seen(c) && c != '\u{fe0f}'))
        .then_some(s)
}

/// A platform name as stored: lower case letters, digits, `.`, `_` and `-`, starting with a
/// letter, at most [`MAX_PLATFORM_CHARS`] characters (`android`, `ios`, `ipados`).
pub fn platform_of(raw: &str) -> Option<String> {
    if raw.chars().count() > MAX_RAW_CHARS {
        return None;
    }
    let s = strip_hidden(raw).to_ascii_lowercase();
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
    if raw.chars().count() > MAX_RAW_CHARS {
        return None;
    }
    let s = strip_hidden(raw);
    let s = s.as_str();
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
/// the masking of D40 apply to it.
///
/// The list is a second line behind the patterns of [`never_for_mobile`], not the groups the
/// patterns miss: most of its entries (`security.officer`, `account.manager`, `hr.admin`,
/// `api.device_admin`, `payroll.approver`, `website.publisher` and so on) end the way the
/// patterns look for and are listed for clarity, and a test keeps the two apart. The ones that
/// only the list catches are [`ONLY_BY_NAME`]. A group of section 4 that is in neither (for
/// example `pos.supervisor` and `sales.invoicing`) is not excluded: neither list is a boundary
/// on its own, the boundary is what the client declared when it registered and what the person
/// ticks.
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
    NEVER_FOR_MOBILE.contains(&group) || by_pattern(group)
}

/// The entries of [`NEVER_FOR_MOBILE`] that no pattern of [`never_for_mobile`] matches: the list
/// is what excludes them.
pub const ONLY_BY_NAME: [&str; 12] = [
    "audit.reader",
    "account.bookkeeper",
    "account.billing",
    "account.adviser",
    "expenses.finance",
    "hr.special_reader",
    "hr.hrbp",
    "hr.line_manager",
    "recruitment.recruiter",
    "connectors.user",
    "api.import_operator",
    "website.editor",
];

/// The patterns: the groups of the platform and every group named like a role with approval or
/// administrative power.
fn by_pattern(group: &str) -> bool {
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
        // What hides or reorders text is taken out, not a reason to refuse the name.
        assert_eq!(name_of("a\u{0}b").as_deref(), Some("ab"));
        assert_eq!(name_of("a\nb").as_deref(), Some("a b"));
        assert_eq!(name_of("evil\u{202e}name").as_deref(), Some("evilname"));
        assert_eq!(name_of("  a \t\u{a0} b  ").as_deref(), Some("a b"));
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
    fn nothing_that_hides_or_reorders_text_is_kept_in_a_name() {
        // Every one of these is invisible or changes how text is shown or read; it is taken out
        // of a name, and a name of nothing else is refused. The same predicate strips a client's
        // name.
        for c in [
            '\u{00ad}',
            '\u{034f}',
            '\u{0600}',
            '\u{0605}',
            '\u{061c}',
            '\u{06dd}',
            '\u{070f}',
            '\u{08e2}',
            '\u{115f}',
            '\u{1160}',
            '\u{17b4}',
            '\u{180e}',
            '\u{200b}',
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
            '\u{2800}',
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
            // White space between two words becomes one space; the rest vanishes.
            let want = if c.is_whitespace() {
                "Ada phone"
            } else {
                "Adaphone"
            };
            assert_eq!(
                name_of(&format!("Ada{c}phone")).as_deref(),
                Some(want),
                "{:x}",
                u32::from(c)
            );
            // A name of nothing else has nothing visible left.
            assert_eq!(name_of(&format!("{c}{c}")), None, "{:x}", u32::from(c));
            assert_eq!(name_of(&format!(" {c} ")), None, "{:x}", u32::from(c));
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
            assert_eq!(name_of(ok).as_deref(), Some(ok), "{ok}");
        }
        // A variation selector alone draws nothing.
        assert_eq!(name_of("\u{fe0f}"), None);
        // A name that is far too long is refused before it is cleaned.
        assert_eq!(name_of(&"a\u{200b}".repeat(600)), None);
    }

    #[test]
    fn the_joiners_of_emoji_and_of_scripts_stay_between_characters_that_are_seen() {
        // The family emoji, the woman technologist, a flag made with a joiner: signing in on a
        // phone named like that must not fail.
        for ok in [
            "Family \u{1F468}\u{200d}\u{1F469}\u{200d}\u{1F467}",
            "\u{1F469}\u{1F3FD}\u{200d}\u{1F4BB}'s phone",
            "\u{1F3F3}\u{fe0f}\u{200d}\u{1F308}",
            // Persian with a zero-width non-joiner (the plural and the verb prefix of the language).
            "\u{0645}\u{06cc}\u{200c}\u{062e}\u{0648}\u{0627}\u{0647}\u{0645}",
        ] {
            assert_eq!(name_of(ok).as_deref(), Some(ok), "{ok:?}");
        }
        // At an end, next to a hidden character, next to white space, or twice in a row, a joiner
        // is hidden text like any other and goes.
        for (raw, want) in [
            ("\u{200d}ab", "ab"),
            ("ab\u{200c}", "ab"),
            ("a \u{200d} b", "a b"),
            ("a\u{200d}\u{200d}b", "ab"),
            ("a\u{200b}\u{200d}b", "ab"),
            ("a\u{200d}\u{202e}b", "ab"),
            ("a\u{200c}\u{2800}b", "ab"),
        ] {
            assert_eq!(name_of(raw).as_deref(), Some(want), "{raw:?}");
        }
        // Platform and version are cleaned the same way and then held to their own rules.
        assert_eq!(platform_of("android\u{200b}").as_deref(), Some("android"));
        assert_eq!(platform_of("\u{202e}iOS\u{200d}").as_deref(), Some("ios"));
        assert_eq!(
            platform_of("and\u{200d}roid"),
            None,
            "a joiner is no letter"
        );
        assert_eq!(version_of("1.4\u{2800}.2").as_deref(), Some("1.4.2"));
        assert_eq!(version_of("\u{200b}"), None);
        assert_eq!(platform_of("\u{2800}"), None);
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
    fn the_named_list_is_a_second_line_and_only_some_of_it_is_not_caught_by_the_patterns() {
        // What the list catches that the patterns do not: exactly ONLY_BY_NAME, which is what
        // docs/CONVENTIONS.md section 4 says. The rest of the list is there for clarity.
        let mut only: Vec<&str> = NEVER_FOR_MOBILE
            .iter()
            .copied()
            .filter(|g| !by_pattern(g))
            .collect();
        let mut named = ONLY_BY_NAME.to_vec();
        only.sort_unstable();
        named.sort_unstable();
        assert_eq!(only, named);
        for g in ONLY_BY_NAME {
            assert!(never_for_mobile(g), "{g}");
        }
        for g in [
            "security.officer",
            "account.manager",
            "hr.admin",
            "api.device_admin",
        ] {
            assert!(NEVER_FOR_MOBILE.contains(&g) && by_pattern(g), "{g}");
        }
        // Groups of section 4 that are in neither are not excluded (the declaration and the tick
        // are the boundary for them).
        for g in [
            "pos.supervisor",
            "sales.invoicing",
            "pos.cashier",
            "stock.operator",
        ] {
            assert!(!never_for_mobile(g), "{g}");
        }
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
