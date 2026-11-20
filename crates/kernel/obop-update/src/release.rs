//! The release feed (D63): which core versions have been published, with their notes.
//!
//! The feed is a JSON document served by the control plane (`GET /api/releases`) or any host the
//! operator names with `OBOP_UPDATE_FEED`. Each release carries a digest over its fields, so a
//! release that was altered or truncated on the way is dropped rather than shown. The digest is
//! an integrity check: whoever serves the feed can also compute it. A release may also carry an
//! Ed25519 signature over that digest (H7), made with the release key that the owner holds
//! (`obop platform release keygen`, `publish --signing-key`); once a public key is pinned in
//! [`RELEASE_PUBLIC_KEYS`], a release without a valid signature of one of them is dropped. No key
//! is pinned yet (the key and its custody are the owner's), so today the digest is the only
//! check. Either way the feed never decides what runs. A managed instance only tells the control plane *which
//! published version* it wants, and the control plane deploys the image it published itself
//! (docs/DEPLOY.md, "Updating an instance from inside").

use obop_pkg::semver::Version;
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};

/// The public keys of the release key, hex (64 characters each), that a release must be signed
/// with. Empty until the owner generates the key and publishes its public half: then the key goes
/// here and every release without its signature is dropped from the feed. Two entries while a key
/// is rotated.
pub const RELEASE_PUBLIC_KEYS: &[&str] = &[];

/// The feed an instance reads when nothing else is configured.
pub const DEFAULT_FEED: &str = "https://obop.dev/api/releases";
const MAX_FEED_BYTES: u64 = 256 * 1024;

/// One published core version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// A plain `major.minor.patch`: the core has no pre-releases.
    pub version: String,
    /// Unix seconds.
    pub published_at: u64,
    /// What changed, plain text.
    pub notes: String,
    /// The image of this version (`ghcr.io/spacecorps/obop:0.2.1`), for people who update by
    /// hand; a managed instance never sends it anywhere.
    pub image: String,
    /// The oldest version this one can be installed over, or empty for any.
    pub min_from: String,
    /// SHA-256 over the fields above.
    pub digest: String,
    /// Ed25519 over [`Release::signed_text`], hex; empty for a release that was not signed.
    pub signature: String,
}

impl Release {
    pub fn new(version: &str, image: &str, notes: &str, published_at: u64, min_from: &str) -> Self {
        Self {
            version: version.into(),
            published_at,
            notes: notes.into(),
            image: image.into(),
            min_from: min_from.into(),
            digest: String::new(),
            signature: String::new(),
        }
        .sealed()
    }

    pub fn compute_digest(&self) -> String {
        let text = format!(
            "obop-release-v1\n{}\n{}\n{}\n{}\n{}",
            self.version, self.image, self.published_at, self.min_from, self.notes
        );
        obop_keys::hex(&Sha256::digest(text.as_bytes()))
    }

    /// The release with its digest filled in.
    pub fn sealed(mut self) -> Self {
        self.digest = self.compute_digest();
        self
    }

    pub fn intact(&self) -> bool {
        self.digest == self.compute_digest()
    }

    /// What the release key signs: the digest of the fields, under its own label.
    pub fn signed_text(&self) -> String {
        format!("obop-release-signature-v1\n{}", self.compute_digest())
    }

    /// The release signed with the release key (a PKCS#8 Ed25519 document, as `keygen` writes).
    pub fn signed_with(mut self, pkcs8: &[u8]) -> Result<Self, String> {
        let key = Ed25519KeyPair::from_pkcs8(pkcs8)
            .map_err(|_| "the signing key is not an Ed25519 key in PKCS#8 form".to_string())?;
        self.signature = obop_keys::hex(key.sign(self.signed_text().as_bytes()).as_ref());
        Ok(self)
    }

    /// Whether the release carries a valid signature of one of `keys` (hex public keys).
    pub fn signed_by(&self, keys: &[&str]) -> bool {
        let Some(sig) = unhex(&self.signature) else {
            return false;
        };
        keys.iter().filter_map(|k| unhex(k)).any(|k| {
            UnparsedPublicKey::new(&ED25519, k)
                .verify(self.signed_text().as_bytes(), &sig)
                .is_ok()
        })
    }

    pub fn semver(&self) -> Option<Version> {
        Version::parse(&self.version).ok()
    }

    pub fn to_json(&self) -> J {
        let mut j = json!({"version": self.version, "published_at": self.published_at,
               "notes": self.notes, "image": self.image, "min_from": self.min_from,
               "digest": self.digest});
        if !self.signature.is_empty() {
            j["signature"] = J::from(self.signature.clone());
        }
        j
    }

    pub fn from_json(v: &J) -> Option<Release> {
        let t = |k: &str| v.get(k).and_then(J::as_str).map(String::from);
        Some(Release {
            version: t("version")?,
            published_at: v.get("published_at").and_then(J::as_u64).unwrap_or(0),
            notes: t("notes").unwrap_or_default(),
            image: t("image").unwrap_or_default(),
            min_from: t("min_from").unwrap_or_default(),
            digest: t("digest")?,
            signature: t("signature").unwrap_or_default(),
        })
    }
}

/// What a feed yielded: the releases that are intact and well formed, newest first, and a note
/// for each entry that was dropped.
#[derive(Debug, Clone, Default)]
pub struct Feed {
    pub releases: Vec<Release>,
    pub dropped: Vec<String>,
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// A new release key: the PKCS#8 document to keep secret and the public key (hex) to pin in
/// [`RELEASE_PUBLIC_KEYS`]. Only `obop platform release keygen` calls it, on the owner's machine.
pub fn generate_release_key() -> Result<(Vec<u8>, String), String> {
    let rng = ring::rand::SystemRandom::new();
    let doc = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| "the system's random source failed".to_string())?;
    let public = public_key_of(doc.as_ref())?;
    Ok((doc.as_ref().to_vec(), public))
}

/// The public key (hex) of a release key in PKCS#8 form.
pub fn public_key_of(pkcs8: &[u8]) -> Result<String, String> {
    let key = Ed25519KeyPair::from_pkcs8(pkcs8)
        .map_err(|_| "the signing key is not an Ed25519 key in PKCS#8 form".to_string())?;
    Ok(obop_keys::hex(key.public_key().as_ref()))
}

/// Read a feed document: `{"releases": [...]}`, checked against the pinned release keys.
pub fn parse_feed(text: &str) -> Result<Feed, String> {
    parse_feed_with(text, RELEASE_PUBLIC_KEYS)
}

/// [`parse_feed`] with the public keys a release must be signed with; none means no signature is
/// asked for (the digest is still checked).
pub fn parse_feed_with(text: &str, keys: &[&str]) -> Result<Feed, String> {
    let j: J =
        serde_json::from_str(text).map_err(|e| format!("the release feed is not JSON: {e}"))?;
    let list = j
        .get("releases")
        .and_then(J::as_array)
        .ok_or("the release feed has no `releases` list")?;
    let mut feed = Feed::default();
    for entry in list.iter().take(500) {
        let Some(r) = Release::from_json(entry) else {
            feed.dropped
                .push("an entry without a version or digest".into());
            continue;
        };
        match r.semver() {
            Some(v) if !v.is_prerelease() => {}
            _ => {
                feed.dropped
                    .push(format!("{}: not a release version", r.version));
                continue;
            }
        }
        if !r.intact() {
            feed.dropped.push(format!(
                "{}: its digest does not match its contents, so it was left out",
                r.version
            ));
            continue;
        }
        if !keys.is_empty() && !r.signed_by(keys) {
            feed.dropped.push(format!(
                "{}: it is not signed with the release key, so it was left out",
                r.version
            ));
            continue;
        }
        if feed.releases.iter().any(|x| x.version == r.version) {
            continue;
        }
        feed.releases.push(r);
    }
    feed.releases
        .sort_by_key(|r| std::cmp::Reverse(r.semver().unwrap_or_else(|| Version::new(0, 0, 0))));
    Ok(feed)
}

/// Only `https`, or `http` to a loopback host (a local control plane or a test), may be read.
pub fn feed_url_ok(url: &str) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return Ok(());
    }
    if let Some(rest) = lower.strip_prefix("http://") {
        let host = rest.split(['/', ':', '?']).next().unwrap_or("");
        if host == "localhost" || host == "127.0.0.1" || host == "[::1]" {
            return Ok(());
        }
    }
    Err(format!(
        "{url} is not https (plain http is only accepted for localhost); fix OBOP_UPDATE_FEED"
    ))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// GET `url` and return the body, bounded in size and time. Anything but a 200 is an error that
/// says what the other side answered.
pub(crate) fn http_get(url: &str, bearer: Option<&str>) -> Result<String, String> {
    feed_url_ok(url)?;
    let mut req = agent().get(url).header("User-Agent", "obop-update");
    if let Some(t) = bearer {
        req = req.header("Authorization", &format!("Bearer {t}"));
    }
    let mut resp = req.call().map_err(|e| format!("cannot reach {url}: {e}"))?;
    let status = resp.status().as_u16();
    let body = resp
        .body_mut()
        .with_config()
        .limit(MAX_FEED_BYTES)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    if status != 200 {
        return Err(format!(
            "{url} answered {status}: {}",
            error_text(&body).unwrap_or_else(|| "no explanation".into())
        ));
    }
    Ok(body)
}

/// POST JSON to `url` with a bearer token and return the JSON answer.
pub(crate) fn http_post(url: &str, bearer: &str, body: &J) -> Result<J, String> {
    feed_url_ok(url)?;
    let mut resp = agent()
        .post(url)
        .header("User-Agent", "obop-update")
        .header("Authorization", &format!("Bearer {bearer}"))
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .map_err(|e| format!("cannot reach {url}: {e}"))?;
    let status = resp.status().as_u16();
    let text = resp
        .body_mut()
        .with_config()
        .limit(MAX_FEED_BYTES)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    if !(200..300).contains(&status) {
        return Err(
            error_text(&text).unwrap_or_else(|| format!("the control plane answered {status}"))
        );
    }
    serde_json::from_str(&text).map_err(|e| format!("unexpected answer: {e}"))
}

fn error_text(body: &str) -> Option<String> {
    serde_json::from_str::<J>(body)
        .ok()?
        .get("error")?
        .as_str()
        .map(String::from)
}

pub fn fetch_feed(url: &str) -> Result<Feed, String> {
    parse_feed(&http_get(url, None)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_of(releases: &[Release]) -> String {
        json!({"releases": releases.iter().map(Release::to_json).collect::<Vec<_>>()}).to_string()
    }

    #[test]
    fn a_feed_is_sorted_newest_first_and_every_release_is_checked() {
        let a = Release::new("0.2.1", "img:0.2.1", "fixes", 10, "");
        let b = Release::new("0.10.0", "img:0.10.0", "new", 20, "0.2.0");
        let c = Release::new("0.3.0", "img:0.3.0", "mid", 15, "");
        let f = parse_feed(&feed_of(&[a, b, c])).unwrap();
        let v: Vec<_> = f.releases.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(v, ["0.10.0", "0.3.0", "0.2.1"], "numeric, not alphabetical");
        assert!(f.dropped.is_empty());
    }

    #[test]
    fn altered_releases_prereleases_and_nonsense_are_dropped_with_a_reason() {
        let mut altered = Release::new("0.2.1", "img:0.2.1", "fixes", 10, "");
        altered.notes = "run this script".into();
        let pre = Release::new("0.3.0-rc.1", "img", "", 1, "");
        let junk = Release::new("soon", "img", "", 1, "");
        let ok = Release::new("0.2.2", "img:0.2.2", "ok", 11, "");
        let f = parse_feed(&feed_of(&[altered, pre, junk, ok])).unwrap();
        assert_eq!(f.releases.len(), 1);
        assert_eq!(f.releases[0].version, "0.2.2");
        assert_eq!(f.dropped.len(), 3);
        assert!(
            f.dropped
                .iter()
                .any(|d| d.contains("digest does not match"))
        );
        assert!(parse_feed("{}").is_err());
        assert!(parse_feed("not json").is_err());
    }

    #[test]
    fn with_a_pinned_key_only_releases_signed_with_it_are_kept() {
        let (key, public) = generate_release_key().unwrap();
        let (other, _) = generate_release_key().unwrap();
        let good = Release::new("0.3.0", "img:0.3.0", "signed", 10, "")
            .signed_with(&key)
            .unwrap();
        let unsigned = Release::new("0.3.1", "img:0.3.1", "unsigned", 11, "");
        let wrong = Release::new("0.3.2", "img:0.3.2", "other key", 12, "")
            .signed_with(&other)
            .unwrap();
        // Signed, then changed: whoever changes it can compute the digest again, not the
        // signature.
        let mut forged = Release::new("0.3.3", "img:0.3.3", "notes", 13, "")
            .signed_with(&key)
            .unwrap();
        let sig = forged.signature.clone();
        forged.image = "evil:latest".into();
        forged = forged.sealed();
        forged.signature = sig;
        assert!(forged.intact());
        let text = feed_of(&[good.clone(), unsigned, wrong, forged]);
        let f = parse_feed_with(&text, &[public.as_str()]).unwrap();
        let v: Vec<_> = f.releases.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(v, ["0.3.0"]);
        assert_eq!(f.dropped.len(), 3, "{:?}", f.dropped);
        assert!(
            f.dropped
                .iter()
                .all(|d| d.contains("not signed with the release key"))
        );
        // The signature travels in the feed and survives a round trip.
        assert_eq!(f.releases[0], good);
        // Without a pinned key every intact release is kept, as before.
        assert_eq!(parse_feed_with(&text, &[]).unwrap().releases.len(), 4);
        assert!(
            RELEASE_PUBLIC_KEYS.is_empty(),
            "no key is pinned until the owner gives one"
        );
        assert_eq!(public_key_of(&key).unwrap(), public);
        assert!(
            Release::new("1.0.0", "i", "", 1, "")
                .signed_with(b"not a key")
                .is_err()
        );
    }

    #[test]
    fn only_https_or_loopback_http_may_be_read() {
        assert!(feed_url_ok("https://obop.dev/api/releases").is_ok());
        assert!(feed_url_ok("http://127.0.0.1:8090/api/releases").is_ok());
        assert!(feed_url_ok("http://localhost/x").is_ok());
        assert!(feed_url_ok("http://example.com/feed").is_err());
        assert!(feed_url_ok("ftp://example.com").is_err());
        assert!(feed_url_ok("file:///etc/passwd").is_err());
    }
}
