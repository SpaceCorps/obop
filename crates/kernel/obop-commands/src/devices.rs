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

fn hidden_or_control(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// A device name as stored: trimmed, 1 to [`MAX_NAME_CHARS`] characters, no control character and
/// no character that reorders or hides text. `None` for anything else.
pub fn name_of(raw: &str) -> Option<String> {
    let s = raw.trim();
    let n = s.chars().count();
    ((1..=MAX_NAME_CHARS).contains(&n) && !s.chars().any(hidden_or_control)).then(|| s.to_string())
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

/// The account a principal acts for: the person behind an agent token is the person.
pub fn owner_of(p: &Principal) -> &str {
    p.id.strip_suffix("/agent").unwrap_or(&p.id)
}

/// Who may see, rename and revoke a device: its owner (an agent acts for its owner), the holders of
/// [`ADMIN_GROUP`] and the instance administrator. The same rule the row access of the model
/// (`own`, `see_all`) gives the generated reads; a device that fails it is reported exactly like
/// one that does not exist.
pub fn may_see(p: &Principal, rec: &Fields) -> bool {
    if p.allows(crate::ADMIN_GROUP) || p.allows(ADMIN_GROUP) {
        return true;
    }
    !p.id.is_empty() && rec.get("user").and_then(J::as_str) == Some(owner_of(p))
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
    /// working, the record says `revoked`. `actor` is who asked, for the security log. A step the
    /// store refuses is an error whose text starts with the storage code word (a 503 for the
    /// caller), never a success; asking again finishes what was left undone.
    fn revoke_device(&self, id: &str, actor: &str) -> Result<(), String>;
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
        assert!(is_active(&f));
        let p = revoked_patch(1_800_000_100);
        assert_eq!(p["state"], REVOKED);
        assert_eq!(p["revoked_at"], "2027-01-15T08:01:40.000000Z");
        let d = new_record("ada", "Mileage", &Metadata::default(), "og_1", 0, 0);
        assert_eq!(
            (d["name"].as_str(), d["platform"].as_str()),
            (Some(DEFAULT_NAME), Some(DEFAULT_PLATFORM))
        );
        assert!(!describe(&d, "x").contains('\n'));
    }

    #[test]
    fn who_may_see_a_device() {
        let f = new_record("ada", "Mileage", &Metadata::default(), "og_1", 0, 0);
        assert!(may_see(&Principal::user("ada", &["base.user"]), &f));
        assert!(!may_see(&Principal::user("bob", &["base.user"]), &f));
        assert!(
            !may_see(&Principal::user("", &[]), &f),
            "nobody owns nothing"
        );
        // An agent acts for its owner.
        assert!(may_see(
            &Principal::agent("ada/agent", &["base.user"], &["base.user"]),
            &f
        ));
        assert!(!may_see(
            &Principal::agent("bob/agent", &["base.user"], &["base.user"]),
            &f
        ));
        // The administrators of devices and of the instance see every device ...
        assert!(may_see(&Principal::user("bob", &[ADMIN_GROUP]), &f));
        assert!(may_see(&Principal::user("bob", &["platform.admin"]), &f));
        // ... but a token narrowed to other groups does not carry that.
        let narrowed = Principal {
            scopes: Some(["base.user".to_string()].into()),
            ..Principal::user("bob", &[ADMIN_GROUP, "base.user"])
        };
        assert!(!may_see(&narrowed, &f));
    }
}
