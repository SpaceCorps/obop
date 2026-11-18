//! Mail addresses as obop accepts them: plain ASCII `local@domain`, nothing that could end a
//! header or start another one. International addresses (SMTPUTF8, IDNA) are refused for now
//! (docs/spec/mail.md 4).

/// The most recipients one message may have (to, cc and bcc together).
pub const MAX_RECIPIENTS: usize = 50;

/// Characters the local part may hold (RFC 5322 `atext` and the dot).
fn local_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-/=?^_`{|}~.".contains(c)
}

/// Check one address. The error names what is wrong, never more of the input than the address.
pub fn check(addr: &str) -> Result<(), String> {
    let bad = |why: &str| Err(format!("{addr:?} is not a mail address: {why}"));
    if addr.is_empty() {
        return Err("a mail address is empty".into());
    }
    if addr.len() > 254 {
        return bad("it is longer than 254 characters");
    }
    if !addr.is_ascii() {
        return bad("only ASCII addresses are supported");
    }
    let Some((local, domain)) = addr.rsplit_once('@') else {
        return bad("it has no @");
    };
    if local.is_empty() || local.len() > 64 {
        return bad("the part before @ must be 1 to 64 characters");
    }
    if !local.chars().all(local_char)
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
    {
        return bad("the part before @ holds a character that is not allowed");
    }
    if domain.len() > 253 || !domain.contains('.') {
        return bad("the domain must have a dot, such as example.com");
    }
    for label in domain.split('.') {
        let ok = !label.is_empty()
            && label.len() <= 63
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-');
        if !ok {
            return bad("the domain is not a valid host name");
        }
    }
    Ok(())
}

/// A list of addresses separated by commas or semicolons; empty entries are skipped. Every
/// address is checked, and the same address twice is kept once.
pub fn parse_list(text: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for part in text.split([',', ';']) {
        let a = part.trim();
        if a.is_empty() {
            continue;
        }
        check(a)?;
        if !out.iter().any(|x| x.eq_ignore_ascii_case(a)) {
            out.push(a.to_string());
        }
    }
    Ok(out)
}

/// A display name: one line, at most 200 characters.
pub fn check_name(name: &str) -> Result<(), String> {
    if name.chars().count() > 200 {
        return Err("a sender name is longer than 200 characters".into());
    }
    if name.chars().any(char::is_control) {
        return Err("a sender name must be one line".into());
    }
    Ok(())
}

/// A subject: one line, at most 998 characters (the line limit of RFC 5322).
pub fn check_subject(s: &str) -> Result<(), String> {
    if s.trim().is_empty() {
        return Err("the subject is empty".into());
    }
    if s.chars().count() > 998 {
        return Err("the subject is longer than 998 characters".into());
    }
    if s.chars().any(char::is_control) {
        return Err("the subject must be one line".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_addresses_pass() {
        for a in [
            "a@example.com",
            "first.last+tag@mail.example.co.uk",
            "x_y-z@a-b.de",
        ] {
            check(a).unwrap();
        }
    }

    #[test]
    fn anything_that_could_reach_a_header_or_a_command_is_refused() {
        for a in [
            "a@example.com\r\nBcc: x@evil.test",
            "a@example.com>",
            "<a@example.com",
            "a b@example.com",
            "a@example",
            "a@-x.com",
            "a..b@example.com",
            "\"q\"@example.com",
            "ä@example.com",
            "a@exämple.com",
            "@example.com",
            "a@",
            "a@example.com\n",
        ] {
            assert!(check(a).is_err(), "{a:?} passed");
        }
    }

    #[test]
    fn a_list_is_split_checked_and_kept_once() {
        assert_eq!(
            parse_list(" a@example.com; b@example.com,,A@example.com ").unwrap(),
            vec!["a@example.com", "b@example.com"]
        );
        assert!(parse_list("a@example.com, nope").is_err());
    }

    #[test]
    fn names_and_subjects_are_one_line() {
        assert!(check_name("Nordlicht\r\nBcc: x").is_err());
        assert!(check_subject("Hi\nthere").is_err());
        assert!(check_subject("  ").is_err());
        check_subject("Rechnung RE-2026-0001 für Sie").unwrap();
    }
}
