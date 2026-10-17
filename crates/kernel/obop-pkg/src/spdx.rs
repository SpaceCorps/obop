//! A small SPDX licence-expression checker (SPDX 2.3 Annex D grammar) with a built-in list of
//! the identifiers a business module realistically uses. No dependency: the full SPDX list has
//! 600+ entries, nearly all irrelevant to a module, and a package that needs another can name it
//! with `LicenseRef-<name>` (custom terms) instead (D55).
//!
//! Accepted: `Apache-2.0`, `AGPL-3.0-or-later`, `GPL-2.0+`, `MIT OR Apache-2.0`,
//! `GPL-3.0-only WITH Classpath-exception-2.0`, `(MIT AND BSD-3-Clause)`,
//! `LicenseRef-Acme-EULA`, and the two non-SPDX markers `proprietary` and `UNLICENSED`, which
//! are allowed for private modules and are flagged as such.

/// Identifiers recognised without `LicenseRef-`. Matched case-insensitively, as SPDX specifies;
/// the canonical spelling is the one listed.
const IDS: &[&str] = &[
    "0BSD",
    "AGPL-1.0-only",
    "AGPL-1.0-or-later",
    "AGPL-3.0-only",
    "AGPL-3.0-or-later",
    "Apache-1.1",
    "Apache-2.0",
    "Artistic-2.0",
    "BlueOak-1.0.0",
    "BSD-1-Clause",
    "BSD-2-Clause",
    "BSD-2-Clause-Patent",
    "BSD-3-Clause",
    "BSD-3-Clause-Clear",
    "BSD-4-Clause",
    "BSL-1.0",
    "CC-BY-3.0",
    "CC-BY-4.0",
    "CC-BY-NC-4.0",
    "CC-BY-ND-4.0",
    "CC-BY-SA-3.0",
    "CC-BY-SA-4.0",
    "CC0-1.0",
    "CDDL-1.0",
    "CDDL-1.1",
    "CECILL-2.1",
    "EPL-1.0",
    "EPL-2.0",
    "EUPL-1.1",
    "EUPL-1.2",
    "FSL-1.1-Apache-2.0",
    "FSL-1.1-MIT",
    "GPL-2.0-only",
    "GPL-2.0-or-later",
    "GPL-3.0-only",
    "GPL-3.0-or-later",
    "ISC",
    "LGPL-2.1-only",
    "LGPL-2.1-or-later",
    "LGPL-3.0-only",
    "LGPL-3.0-or-later",
    "MIT",
    "MIT-0",
    "MPL-1.1",
    "MPL-2.0",
    "MS-PL",
    "MS-RL",
    "NCSA",
    "ODbL-1.0",
    "OFL-1.1",
    "OSL-3.0",
    "PostgreSQL",
    "Python-2.0",
    "SSPL-1.0",
    "BUSL-1.1",
    "Unicode-3.0",
    "Unlicense",
    "UPL-1.0",
    "WTFPL",
    "Zlib",
];

/// Deprecated identifiers whose `+` form SPDX replaced; accepted, since old files use them.
const DEPRECATED: &[&str] = &["AGPL-3.0", "GPL-2.0", "GPL-3.0", "LGPL-2.1", "LGPL-3.0"];

const EXCEPTIONS: &[&str] = &[
    "Classpath-exception-2.0",
    "GCC-exception-3.1",
    "LLVM-exception",
    "Linux-syscall-note",
    "Autoconf-exception-3.0",
    "Bison-exception-2.2",
    "Font-exception-2.0",
    "OpenSSL-exception",
    "LGPL-3.0-linking-exception",
    "Universal-FOSS-exception-1.0",
];

/// What kind of licence the expression is, for the flag in the module card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseKind {
    /// Built only from SPDX identifiers.
    Spdx,
    /// Mentions a `LicenseRef-...`: custom terms the package ships with.
    Custom,
    /// `proprietary` or `UNLICENSED`.
    Proprietary,
}

fn canonical<'a>(list: &'a [&'a str], id: &str) -> Option<&'a str> {
    list.iter().find(|x| x.eq_ignore_ascii_case(id)).copied()
}

fn token_ok(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '.'
}

/// Check a licence expression and say what kind it is.
pub fn check(expr: &str) -> Result<LicenseKind, String> {
    let e = expr.trim();
    if e.is_empty() {
        return Err("the licence is empty".into());
    }
    if e.len() > 200 {
        return Err("the licence expression is too long".into());
    }
    if e == "proprietary" || e == "UNLICENSED" {
        return Ok(LicenseKind::Proprietary);
    }
    let mut p = Parser {
        toks: tokenize(e)?,
        at: 0,
        custom: false,
    };
    p.expr(0)?;
    if p.at != p.toks.len() {
        return Err(format!("unexpected {:?} in the licence", p.toks[p.at]));
    }
    Ok(if p.custom {
        LicenseKind::Custom
    } else {
        LicenseKind::Spdx
    })
}

fn tokenize(e: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if !cur.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    for c in e.chars() {
        if c == '(' || c == ')' {
            flush(&mut cur, &mut out);
            out.push(c.to_string());
        } else if c.is_whitespace() {
            flush(&mut cur, &mut out);
        } else if token_ok(c) || c == '+' || c == ':' {
            cur.push(c);
        } else {
            return Err(format!("{c:?} is not allowed in a licence expression"));
        }
    }
    flush(&mut cur, &mut out);
    Ok(out)
}

struct Parser {
    toks: Vec<String>,
    at: usize,
    custom: bool,
}

impl Parser {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.at).map(String::as_str)
    }

    /// `expr := term (("AND" | "OR") term)*`, nesting at most 8 deep.
    fn expr(&mut self, depth: usize) -> Result<(), String> {
        if depth > 8 {
            return Err("the licence expression is nested too deeply".into());
        }
        self.term(depth)?;
        while matches!(self.peek(), Some("AND" | "OR")) {
            self.at += 1;
            self.term(depth)?;
        }
        Ok(())
    }

    fn term(&mut self, depth: usize) -> Result<(), String> {
        let Some(t) = self.peek().map(String::from) else {
            return Err("the licence expression ends where a licence was expected".into());
        };
        self.at += 1;
        if t == "(" {
            self.expr(depth + 1)?;
            if self.peek() != Some(")") {
                return Err("a ( in the licence has no closing )".into());
            }
            self.at += 1;
            return Ok(());
        }
        if matches!(t.as_str(), ")" | "AND" | "OR" | "WITH") {
            return Err(format!("{t:?} cannot start a licence"));
        }
        let (name, plus) = match t.strip_suffix('+') {
            Some(n) => (n, true),
            None => (t.as_str(), false),
        };
        if name.starts_with("LicenseRef-") || name.starts_with("DocumentRef-") {
            let ok = match name.strip_prefix("DocumentRef-") {
                Some(rest) => rest.split_once(":LicenseRef-").is_some_and(|(d, l)| {
                    !d.is_empty()
                        && !l.is_empty()
                        && d.chars().all(token_ok)
                        && l.chars().all(token_ok)
                }),
                None => {
                    let r = &name["LicenseRef-".len()..];
                    !r.is_empty() && r.chars().all(token_ok)
                }
            };
            if !ok || plus {
                return Err(format!(
                    "{t:?} is not a valid custom licence; write LicenseRef-<letters, digits, - and .>"
                ));
            }
            self.custom = true;
        } else if canonical(IDS, name).is_some() || canonical(DEPRECATED, name).is_some() {
            if plus
                && canonical(IDS, name)
                    .is_some_and(|c| c.ends_with("-only") || c.ends_with("-or-later"))
            {
                return Err(format!(
                    "{t:?}: + cannot follow an -only or -or-later identifier"
                ));
            }
        } else {
            return Err(format!(
                "{name:?} is not a known SPDX licence identifier (use e.g. Apache-2.0, AGPL-3.0-or-later, MIT, or LicenseRef-<name> for your own terms)"
            ));
        }
        if self.peek() == Some("WITH") {
            self.at += 1;
            let Some(x) = self.peek().map(String::from) else {
                return Err("WITH needs an exception name".into());
            };
            self.at += 1;
            if canonical(EXCEPTIONS, &x).is_none() && !x.starts_with("LicenseRef-") {
                return Err(format!("{x:?} is not a known SPDX licence exception"));
            }
        }
        Ok(())
    }
}

/// The URL of the SPDX page for a single identifier, for the link in the module's detail panel.
/// `None` for expressions, custom and proprietary licences.
pub fn url_for(expr: &str) -> Option<String> {
    let e = expr.trim();
    let id = canonical(IDS, e).or_else(|| canonical(DEPRECATED, e))?;
    Some(format!("https://spdx.org/licenses/{id}.html"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_expressions_pass() {
        for ok in [
            "Apache-2.0",
            "AGPL-3.0-or-later",
            "apache-2.0",
            "GPL-2.0+",
            "MIT OR Apache-2.0",
            "(MIT AND BSD-3-Clause) OR Apache-2.0",
            "GPL-3.0-only WITH Classpath-exception-2.0",
            "LicenseRef-Acme-EULA",
            "MIT OR LicenseRef-Acme.1",
            "DocumentRef-acme:LicenseRef-eula",
        ] {
            assert!(check(ok).is_ok(), "{ok}: {:?}", check(ok));
        }
    }

    #[test]
    fn kinds_are_told_apart() {
        assert_eq!(check("MIT").unwrap(), LicenseKind::Spdx);
        assert_eq!(check("MIT OR LicenseRef-x").unwrap(), LicenseKind::Custom);
        assert_eq!(check("proprietary").unwrap(), LicenseKind::Proprietary);
        assert_eq!(check("UNLICENSED").unwrap(), LicenseKind::Proprietary);
    }

    #[test]
    fn mistakes_are_refused() {
        for bad in [
            "",
            "   ",
            "Apache",
            "Apache-2.0 OR",
            "AND MIT",
            "(MIT",
            "MIT)",
            "MIT AND",
            "MIT WITH",
            "MIT WITH Nonsense-exception",
            "GPL-3.0-or-later+",
            "LicenseRef-",
            "LicenseRef-a b",
            "Proprietary",
            "MIT; DROP",
            "MIT Apache-2.0",
            "proprietary OR MIT",
        ] {
            assert!(check(bad).is_err(), "{bad:?} should fail");
        }
        let deep = format!("{}MIT{}", "(".repeat(20), ")".repeat(20));
        assert!(check(&deep).is_err());
    }

    #[test]
    fn spdx_link_only_for_a_single_identifier() {
        assert_eq!(
            url_for("apache-2.0").as_deref(),
            Some("https://spdx.org/licenses/Apache-2.0.html")
        );
        assert!(url_for("MIT OR Apache-2.0").is_none());
        assert!(url_for("LicenseRef-x").is_none());
    }
}
