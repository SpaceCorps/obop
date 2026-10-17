//! A small semantic-versioning implementation: versions with pre-release ordering and version
//! requirements (`*`, `1.2.3`, `^`, `~`, `>=`, `>`, `<=`, `<`, `=`, comma-separated ANDs).
//!
//! The grammar and the matching rules follow Cargo's, so a Rust developer already knows them:
//! a bare `1.2.3` means `^1.2.3`; `^0.2` stays below `0.3.0` (while a version is below 1.0 a
//! minor bump is breaking); a pre-release version only satisfies a requirement that names a
//! pre-release of the same `major.minor.patch`. Build metadata (`+...`) is not accepted.
//! Hand-written because it is about 300 lines and obop wants no dependency for it (D55).

use std::cmp::Ordering;
use std::fmt;

/// A semantic version: `major.minor.patch` and an optional pre-release (`-beta.1`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pre: Vec<Ident>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Ident {
    Num(u64),
    Alpha(String),
}

impl Ord for Ident {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Ident::Num(a), Ident::Num(b)) => a.cmp(b),
            (Ident::Num(_), Ident::Alpha(_)) => Ordering::Less,
            (Ident::Alpha(_), Ident::Num(_)) => Ordering::Greater,
            (Ident::Alpha(a), Ident::Alpha(b)) => a.cmp(b),
        }
    }
}
impl PartialOrd for Ident {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn number(s: &str) -> Option<u64> {
    let ok = !s.is_empty()
        && s.len() <= 9
        && s.bytes().all(|b| b.is_ascii_digit())
        && (s == "0" || !s.starts_with('0'));
    if ok { s.parse().ok() } else { None }
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            pre: Vec::new(),
        }
    }

    /// `1.2.3` or `1.2.3-beta.1`: exactly three numbers without leading zeros.
    pub fn parse(s: &str) -> Result<Self, String> {
        let bad = || format!("{s:?} is not a version like 1.2.3 or 1.2.3-beta.1");
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (s, None),
        };
        let nums: Vec<&str> = core.split('.').collect();
        if nums.len() != 3 {
            return Err(bad());
        }
        let n: Vec<u64> = nums
            .iter()
            .map(|x| number(x).ok_or_else(bad))
            .collect::<Result<_, _>>()?;
        let mut v = Version::new(n[0], n[1], n[2]);
        if let Some(p) = pre {
            v.pre = parse_pre(p).ok_or_else(bad)?;
        }
        Ok(v)
    }

    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }

    fn triple(&self) -> (u64, u64, u64) {
        (self.major, self.minor, self.patch)
    }
}

fn parse_pre(p: &str) -> Option<Vec<Ident>> {
    if p.is_empty() || p.len() > 40 {
        return None;
    }
    p.split('.')
        .map(|id| {
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                None
            } else if id.bytes().all(|b| b.is_ascii_digit()) {
                number(id).map(Ident::Num)
            } else {
                Some(Ident::Alpha(id.to_string()))
            }
        })
        .collect()
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.triple().cmp(&other.triple()).then_with(|| {
            match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            }
        })
    }
}
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            let p: Vec<String> = self
                .pre
                .iter()
                .map(|i| match i {
                    Ident::Num(n) => n.to_string(),
                    Ident::Alpha(a) => a.clone(),
                })
                .collect();
            write!(f, "-{}", p.join("."))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cmp {
    Gt,
    Ge,
    Lt,
    Le,
}

/// One comparator as written: `^1.2`, `>=0.3.1`, `1.*`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Comparator {
    /// Primitive bounds all of which must hold.
    bounds: Vec<(Cmp, Version)>,
    /// The `major.minor.patch` of an explicit pre-release in the comparator, if any.
    pre_of: Option<(u64, u64, u64)>,
}

/// A version requirement such as `>=0.2, <0.4`, `^0.2` or `*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Req {
    text: String,
    comparators: Vec<Comparator>,
}

impl Req {
    /// Matches every release version.
    pub fn any() -> Self {
        Self {
            text: "*".into(),
            comparators: Vec::new(),
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        let bad = |why: &str| {
            format!(
                "{s:?} is not a version requirement ({why}); examples: \"*\", \"^0.2\", \">=0.2, <0.4\""
            )
        };
        let text = s.trim();
        if text.is_empty() {
            return Err(bad("it is empty"));
        }
        if text.len() > 100 {
            return Err(bad("it is too long"));
        }
        let mut comparators = Vec::new();
        for part in text.split(',') {
            comparators.push(parse_comparator(part.trim()).map_err(|e| bad(&e))?);
        }
        Ok(Self {
            text: text.to_string(),
            comparators,
        })
    }

    pub fn matches(&self, v: &Version) -> bool {
        if !self
            .comparators
            .iter()
            .all(|c| c.bounds.iter().all(|(op, b)| holds(*op, v, b)))
        {
            return false;
        }
        if v.is_prerelease() {
            return self
                .comparators
                .iter()
                .any(|c| c.pre_of == Some(v.triple()));
        }
        true
    }

    /// True for `*` (any release).
    pub fn is_any(&self) -> bool {
        self.comparators.iter().all(|c| c.bounds.is_empty())
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Whether the requirement rules out some future release: `^0.2` and `>=0.2, <1` do, `*` and
    /// `>=0.2` do not. A module that declares no upper bound promises to run on every version
    /// that will ever exist, which no author can know (D63).
    pub fn is_bounded(&self) -> bool {
        !self.matches(&Version::new(u64::MAX / 2, 0, 0))
    }

    /// Whether some release satisfies the requirement at all. `>=2, <1` and `>1, <1.0.1` do
    /// not. The check tries the versions at and just around each bound, which is where a
    /// requirement over release versions changes its answer.
    pub fn is_satisfiable(&self) -> bool {
        let mut candidates = vec![Version::new(0, 0, 0)];
        for c in &self.comparators {
            for (_, b) in &c.bounds {
                let (a, m, p) = b.triple();
                candidates.push(Version::new(a, m, p));
                candidates.push(Version::new(a, m, p.saturating_add(1)));
                candidates.push(Version::new(a, m.saturating_add(1), 0));
                candidates.push(Version::new(a.saturating_add(1), 0, 0));
                if p > 0 {
                    candidates.push(Version::new(a, m, p - 1));
                }
            }
        }
        candidates.iter().any(|v| self.matches(v))
    }
}

impl fmt::Display for Req {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

fn holds(op: Cmp, v: &Version, b: &Version) -> bool {
    match op {
        Cmp::Gt => v > b,
        Cmp::Ge => v >= b,
        Cmp::Lt => v < b,
        Cmp::Le => v <= b,
    }
}

/// A possibly partial version: `1`, `1.2`, `1.2.3`, `1.2.3-beta`, `1.*`.
struct Partial {
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
    pre: Vec<Ident>,
}

fn parse_partial(s: &str) -> Result<Partial, String> {
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (s, None),
    };
    let mut it = core.split('.');
    let wild = |x: &str| x == "*" || x == "x" || x == "X";
    let first = it.next().unwrap_or("");
    let major = number(first).ok_or_else(|| format!("{first:?} is not a number"))?;
    let mut parts: Vec<Option<u64>> = Vec::new();
    let mut seen_wild = false;
    for x in it {
        if wild(x) {
            seen_wild = true;
            parts.push(None);
        } else if seen_wild {
            return Err("a number cannot follow a wildcard".into());
        } else {
            parts.push(Some(
                number(x).ok_or_else(|| format!("{x:?} is not a number"))?,
            ));
        }
    }
    if parts.len() > 2 {
        return Err("a version has at most three numbers".into());
    }
    let pre = match pre {
        Some(p) => {
            if parts.len() != 2 || parts.iter().any(Option::is_none) {
                return Err("a pre-release needs a full major.minor.patch".into());
            }
            parse_pre(p).ok_or_else(|| format!("{p:?} is not a valid pre-release"))?
        }
        None => Vec::new(),
    };
    Ok(Partial {
        major,
        minor: parts.first().copied().flatten(),
        patch: parts.get(1).copied().flatten(),
        pre,
    })
}

fn parse_comparator(s: &str) -> Result<Comparator, String> {
    if s.is_empty() {
        return Err("an empty comparator".into());
    }
    if s == "*" || s == "x" || s == "X" {
        return Ok(Comparator {
            bounds: Vec::new(),
            pre_of: None,
        });
    }
    let (op, rest) = ["<=", ">=", "<", ">", "=", "^", "~"]
        .iter()
        .find_map(|o| s.strip_prefix(o).map(|r| (*o, r.trim_start())))
        .unwrap_or(("", s));
    if rest.is_empty() {
        return Err(format!("{op:?} needs a version"));
    }
    if rest.contains(char::is_whitespace) {
        return Err("separate comparators with commas".into());
    }
    let p = parse_partial(rest)?;
    let full = Version::new;
    let lo = Version {
        major: p.major,
        minor: p.minor.unwrap_or(0),
        patch: p.patch.unwrap_or(0),
        pre: p.pre.clone(),
    };
    // The exclusive upper bound of "everything that starts with this partial version".
    let next = match (p.minor, p.patch) {
        (None, _) => full(p.major + 1, 0, 0),
        (Some(m), None) => full(p.major, m + 1, 0),
        (Some(m), Some(pa)) => full(p.major, m, pa + 1),
    };
    let bounds = match op {
        "=" if p.patch.is_some() => vec![(Cmp::Ge, lo.clone()), (Cmp::Le, lo)],
        "" | "^" => {
            // Caret: the leftmost non-zero number may not change.
            let upper = match (p.major, p.minor, p.patch) {
                (0, Some(0), Some(pa)) => full(0, 0, pa + 1),
                (0, Some(0), None) => full(0, 1, 0),
                (0, Some(m), _) => full(0, m + 1, 0),
                (0, None, _) => full(1, 0, 0),
                (maj, _, _) => full(maj + 1, 0, 0),
            };
            vec![(Cmp::Ge, lo), (Cmp::Lt, upper)]
        }
        "~" => {
            let upper = match p.minor {
                Some(m) => full(p.major, m + 1, 0),
                None => full(p.major + 1, 0, 0),
            };
            vec![(Cmp::Ge, lo), (Cmp::Lt, upper)]
        }
        ">=" => vec![(Cmp::Ge, lo)],
        ">" if p.patch.is_some() => vec![(Cmp::Gt, lo)],
        ">" => vec![(Cmp::Ge, next)],
        "<" => vec![(Cmp::Lt, lo)],
        "<=" if p.patch.is_some() => vec![(Cmp::Le, lo)],
        "<=" => vec![(Cmp::Lt, next)],
        // "=" with a partial version: the whole series.
        _ => vec![(Cmp::Ge, lo), (Cmp::Lt, next)],
    };
    let pre_of = (!p.pre.is_empty()).then(|| (p.major, p.minor.unwrap_or(0), p.patch.unwrap_or(0)));
    Ok(Comparator { bounds, pre_of })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }
    fn ok(req: &str, ver: &str) -> bool {
        Req::parse(req).unwrap().matches(&v(ver))
    }

    #[test]
    fn versions_parse_and_reject_the_malformed() {
        assert_eq!(v("1.2.3").to_string(), "1.2.3");
        assert_eq!(v("1.2.3-beta.1").to_string(), "1.2.3-beta.1");
        for bad in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.2.x",
            "v1.2.3",
            "1.2.3-",
            "1.2.3+b",
            "1.2.3-a..b",
            "1.2.3-é",
            "1.-2.3",
            "1234567890.0.0",
        ] {
            assert!(Version::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn ordering_follows_semver_precedence() {
        let order = [
            "0.9.9",
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.1.0",
            "2.0.0",
            "10.0.0",
        ];
        for w in order.windows(2) {
            assert!(v(w[0]) < v(w[1]), "{} < {}", w[0], w[1]);
        }
        assert_eq!(v("1.0.0").cmp(&v("1.0.0")), Ordering::Equal);
    }

    #[test]
    fn star_and_bare_versions() {
        assert!(Req::parse("*").unwrap().is_any());
        assert!(ok("*", "0.0.1") && ok("*", "99.0.0"));
        assert!(
            !ok("*", "1.0.0-rc.1"),
            "a pre-release never satisfies a star"
        );
        // A bare version is a caret requirement, as in Cargo.
        assert!(ok("1.2.3", "1.9.0") && !ok("1.2.3", "2.0.0") && !ok("1.2.3", "1.2.2"));
    }

    #[test]
    fn caret_keeps_the_leftmost_nonzero_number() {
        assert!(ok("^0.2", "0.2.0") && ok("^0.2", "0.2.9") && !ok("^0.2", "0.3.0"));
        assert!(ok("^0.2.3", "0.2.3") && !ok("^0.2.3", "0.2.2") && !ok("^0.2.3", "0.3.0"));
        assert!(ok("^0.0.3", "0.0.3") && !ok("^0.0.3", "0.0.4"));
        assert!(ok("^0", "0.9.9") && !ok("^0", "1.0.0"));
        assert!(ok("^1", "1.99.0") && !ok("^1", "2.0.0") && !ok("^1", "0.9.0"));
        assert!(ok("^ 1.2", "1.2.0"), "space after the operator is fine");
    }

    #[test]
    fn tilde_allows_patch_changes() {
        assert!(ok("~1.2.3", "1.2.9") && !ok("~1.2.3", "1.3.0") && !ok("~1.2.3", "1.2.2"));
        assert!(ok("~1.2", "1.2.0") && !ok("~1.2", "1.3.0"));
        assert!(ok("~1", "1.9.0") && !ok("~1", "2.0.0"));
    }

    #[test]
    fn comparison_operators_and_partial_versions() {
        assert!(ok(">=0.2", "0.2.0") && !ok(">=0.2", "0.1.9"));
        assert!(ok(">0.2.0", "0.2.1") && !ok(">0.2.0", "0.2.0"));
        assert!(
            ok(">1.2", "1.3.0") && !ok(">1.2", "1.2.9"),
            "> on a partial skips the series"
        );
        assert!(ok("<0.4", "0.3.9") && !ok("<0.4", "0.4.0"));
        assert!(ok("<=1.2.3", "1.2.3") && !ok("<=1.2.3", "1.2.4"));
        assert!(ok("<=1.2", "1.2.9") && !ok("<=1.2", "1.3.0"));
        assert!(ok("=1.2.3", "1.2.3") && !ok("=1.2.3", "1.2.4"));
        assert!(ok("=1.2", "1.2.7") && !ok("=1.2", "1.3.0"));
        assert!(ok("1.*", "1.5.0") && !ok("1.*", "2.0.0"));
        assert!(ok("=1.2.x", "1.2.4") && !ok("=1.2.x", "1.3.0"));
    }

    #[test]
    fn commas_are_and() {
        let r = Req::parse(">=0.2, <0.4").unwrap();
        assert!(r.matches(&v("0.2.0")) && r.matches(&v("0.3.9")));
        assert!(!r.matches(&v("0.1.9")) && !r.matches(&v("0.4.0")));
        assert_eq!(r.to_string(), ">=0.2, <0.4");
        assert!(!ok(">=1, <1", "1.0.0"), "an empty range matches nothing");
    }

    #[test]
    fn prereleases_only_match_when_asked_for() {
        assert!(!ok(">=0.2", "0.3.0-beta.1"));
        assert!(!ok("^0.3", "0.3.0-beta.1"));
        assert!(ok(">=0.3.0-beta.1", "0.3.0-beta.2"));
        assert!(ok(">=0.3.0-beta.1", "0.3.0"));
        assert!(
            !ok(">=0.3.0-beta.1", "0.4.0-beta.1"),
            "another series stays out"
        );
        assert!(ok("^0.3.0-beta.1", "0.3.0-rc.1") && !ok("^0.3.0-beta.1", "0.3.0-alpha"));
        assert!(ok("<0.3.0-beta.2", "0.3.0-beta.1"));
        assert!(ok("<1.0.0", "0.9.0") && !ok("<1.0.0", "1.0.0-rc.1"));
    }

    #[test]
    fn bounded_and_satisfiable_requirements() {
        for (r, bounded) in [
            ("*", false),
            (">=0.2", false),
            (">0", false),
            ("^0.2", true),
            ("~1.2", true),
            (">=0.3, <0.5", true),
            ("<2", true),
            ("1.2.3", true),
            ("=0.2.0", true),
            ("0.*", true),
        ] {
            assert_eq!(Req::parse(r).unwrap().is_bounded(), bounded, "{r}");
        }
        for r in ["*", "^0.2", ">=0.3, <0.5", "=1.0.0", ">1.0.0, <1.0.2", "<1"] {
            assert!(Req::parse(r).unwrap().is_satisfiable(), "{r}");
        }
        for r in [
            ">=2, <1",
            ">1, <1.0.1",
            "<0.0.0",
            "^0.2, ^0.3",
            ">=1.0.0, <1.0.0",
        ] {
            assert!(!Req::parse(r).unwrap().is_satisfiable(), "{r}");
        }
    }

    #[test]
    fn malformed_requirements_are_refused_with_examples() {
        for bad in [
            "", ",", ">=", "1.2.3.4", ">=1,", "foo", "1.x.3", ">= 1 < 2", "^", "1.2-beta", "~a",
        ] {
            let e = Req::parse(bad).unwrap_err();
            assert!(e.contains("not a version requirement"), "{bad:?}: {e}");
        }
        assert!(Req::parse(&"1,".repeat(60)).is_err());
    }
}
