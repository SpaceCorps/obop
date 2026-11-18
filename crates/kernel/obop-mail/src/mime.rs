//! The message as it goes out: RFC 5322 headers and one `text/plain; charset=utf-8` part in
//! base64 (safe for any text and any line length). Names and subjects that are not plain ASCII
//! are RFC 2047 encoded words. Every value was checked before (`address`), so no header can be
//! ended early by what a caller typed; the builder still refuses a line break as a last guard.

use base64::Engine as _;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// What one message holds. `bcc` goes to the envelope only, never into a header.
#[derive(Debug, Clone)]
pub struct Message {
    pub from_name: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: Option<String>,
    pub subject: String,
    pub text: String,
    /// Without the angle brackets.
    pub message_id: String,
    /// Seconds since the Unix epoch.
    pub date: u64,
}

impl Message {
    /// Everyone the envelope names.
    pub fn recipients(&self) -> Vec<String> {
        self.to
            .iter()
            .chain(&self.cc)
            .chain(&self.bcc)
            .cloned()
            .collect()
    }

    /// The text with line breaks normalised to CRLF.
    pub fn crlf_text(&self) -> String {
        self.text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\n', "\r\n")
    }

    /// The whole message, headers and body, with CRLF line ends.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let mut h: Vec<String> = vec![
            format!("Date: {}", rfc5322_date(self.date)),
            format!("From: {}", mailbox(&self.from_name, &self.from)),
            format!("To: {}", self.to.join(", ")),
        ];
        if !self.cc.is_empty() {
            h.push(format!("Cc: {}", self.cc.join(", ")));
        }
        if let Some(r) = &self.reply_to {
            h.push(format!("Reply-To: {r}"));
        }
        h.push(format!("Subject: {}", encode_text(&self.subject)));
        h.push(format!("Message-ID: <{}>", self.message_id));
        h.push("MIME-Version: 1.0".into());
        h.push("Content-Type: text/plain; charset=utf-8".into());
        h.push("Content-Transfer-Encoding: base64".into());
        if h.iter().any(|l| l.contains(['\r', '\n'])) {
            return Err("a header would hold a line break".into());
        }
        let mut out = h.join("\r\n");
        out.push_str("\r\n\r\n");
        let body = B64.encode(self.crlf_text().as_bytes());
        for chunk in body.as_bytes().chunks(76) {
            out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
            out.push_str("\r\n");
        }
        Ok(out.into_bytes())
    }
}

/// `Name <addr>`, `"Name, Inc." <addr>` or an encoded word, or the bare address without a name.
pub fn mailbox(name: &str, addr: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        return addr.to_string();
    }
    let plain = name.chars().all(|c| c.is_ascii_graphic() || c == ' ');
    let shown = if !plain {
        encode_words(name)
    } else if name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || " !#$%&'*+-/=?^_`{|}~".contains(c))
    {
        name.to_string()
    } else {
        format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
    };
    format!("{shown} <{addr}>")
}

/// A header text: as it is when plain ASCII, else encoded words.
fn encode_text(s: &str) -> String {
    let plain = s.chars().all(|c| c.is_ascii_graphic() || c == ' ') && !s.contains("=?");
    if plain && s.len() <= 900 {
        s.to_string()
    } else {
        encode_words(s)
    }
}

/// RFC 2047 `=?UTF-8?B?...?=` words of at most 45 source bytes each (75 characters encoded),
/// split on character boundaries and folded onto continuation lines.
pub fn encode_words(s: &str) -> String {
    let mut words = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if cur.len() + c.len_utf8() > 45 {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
        .iter()
        .map(|w| format!("=?UTF-8?B?{}?=", B64.encode(w.as_bytes())))
        .collect::<Vec<_>>()
        .join("\r\n ")
}

/// `Tue, 18 Nov 2026 09:05:00 +0000`.
pub fn rfc5322_date(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil(days);
    // 1970-01-01 was a Thursday.
    let wd = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][(days.rem_euclid(7)) as usize];
    let mon = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(m - 1) as usize];
    format!(
        "{wd}, {d:02} {mon} {y} {:02}:{:02}:{:02} +0000",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Year, month and day of a day count since 1970-01-01 (Howard Hinnant's algorithm).
fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg() -> Message {
        Message {
            from_name: "Nordlicht Werkzeuge GmbH".into(),
            from: "rechnung@example.com".into(),
            to: vec!["kunde@example.org".into()],
            cc: vec![],
            bcc: vec!["archiv@example.com".into()],
            reply_to: Some("buchhaltung@example.com".into()),
            subject: "Rechnung für Sie".into(),
            text: "Guten Tag,\nanbei.\n.\nGruß".into(),
            message_id: "0190-abc@example.com".into(),
            date: 1_795_000_000,
        }
    }

    #[test]
    fn the_date_is_rfc_5322() {
        assert_eq!(rfc5322_date(0), "Thu, 01 Jan 1970 00:00:00 +0000");
        assert_eq!(
            rfc5322_date(1_795_000_000),
            "Wed, 18 Nov 2026 11:06:40 +0000"
        );
        assert_eq!(rfc5322_date(951_782_400), "Tue, 29 Feb 2000 00:00:00 +0000");
    }

    #[test]
    fn headers_are_encoded_and_bcc_is_not_one() {
        let text = String::from_utf8(msg().to_bytes().unwrap()).unwrap();
        assert!(text.contains("From: Nordlicht Werkzeuge GmbH <rechnung@example.com>\r\n"));
        assert!(text.contains("Subject: =?UTF-8?B?"));
        assert!(text.contains("Reply-To: buchhaltung@example.com\r\n"));
        assert!(
            !text.contains("archiv@example.com"),
            "bcc leaked into a header"
        );
        let (_, body) = text.split_once("\r\n\r\n").unwrap();
        let decoded = B64.decode(body.replace("\r\n", "")).unwrap();
        assert_eq!(
            String::from_utf8(decoded).unwrap(),
            "Guten Tag,\r\nanbei.\r\n.\r\nGruß"
        );
        assert!(body.lines().all(|l| l.len() <= 76));
    }

    #[test]
    fn a_name_with_specials_is_quoted_and_a_long_word_is_split() {
        assert_eq!(mailbox("A, B", "x@example.com"), "\"A, B\" <x@example.com>");
        let long = "ä".repeat(60);
        let enc = encode_words(&long);
        assert!(enc.split("\r\n ").all(|w| w.len() <= 75), "{enc}");
        assert!(encode_text("=?UTF-8?B?x?=").starts_with("=?UTF-8?B?"));
    }
}
