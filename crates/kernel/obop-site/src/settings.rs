//! The `site` object of the instance settings: what the owner sets to brand the public site.
//! Every value is validated here, so nothing the owner (or someone with `setting.update`) types
//! can break out of an attribute, a URL or a stylesheet later.

use crate::i18n::Lang;
use crate::theme;
use serde_json::{Map, Value as J};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `site.enabled`: false answers every public page with 503 (the admin keeps working).
    pub enabled: bool,
    pub title: String,
    pub tagline: String,
    /// Shown next to or instead of a logo image.
    pub logo_text: String,
    /// `https://...` or a path on this site, shown as an image in the header.
    pub logo_url: Option<String>,
    pub accent: String,
    pub footer_text: String,
    /// (label, https URL), at most eight.
    pub social: Vec<(String, String)>,
    pub default_lang: Lang,
    /// Public origin without a trailing slash; empty means "from the request's Host".
    pub base_url: Option<String>,
    /// Ask search engines to stay away (a staging site).
    pub noindex: bool,
    /// `mailto:` or `https:` contact for `/.well-known/security.txt`.
    pub security_contact: Option<String>,
    pub shop_currency: String,
    pub shop_page_size: usize,
    /// Extra sentence under the checkout button, for example how you will ask for payment.
    pub checkout_note: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            title: "My Shop".into(),
            tagline: String::new(),
            logo_text: String::new(),
            logo_url: None,
            accent: theme::DEFAULT_ACCENT.into(),
            footer_text: String::new(),
            social: Vec::new(),
            default_lang: Lang::En,
            base_url: None,
            noindex: false,
            security_contact: None,
            shop_currency: "EUR".into(),
            shop_page_size: 12,
            checkout_note: String::new(),
        }
    }
}

fn text(m: &Map<String, J>, k: &str, max: usize) -> Option<String> {
    let s = m.get(k)?.as_str()?;
    let s: String = s
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect();
    let s = s.trim();
    (!s.is_empty()).then(|| s.chars().take(max).collect())
}

fn flag(m: &Map<String, J>, k: &str) -> Option<bool> {
    match m.get(k)? {
        J::Bool(b) => Some(*b),
        J::String(s) => match s.as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// A URL that is safe to put into an attribute: no whitespace, quotes or angle brackets.
fn clean_url(u: &str) -> bool {
    !u.is_empty()
        && u.len() <= 500
        && !u
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "\"'<>`\\".contains(c))
}

/// An https origin such as `https://shop.example.com` (optionally with a port), no path.
pub fn origin_ok(u: &str) -> bool {
    let Some(rest) = u
        .strip_prefix("https://")
        .or_else(|| u.strip_prefix("http://"))
    else {
        return false;
    };
    clean_url(u)
        && !rest.is_empty()
        && !rest.contains(['/', '?', '#', '@'])
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c))
}

pub fn currency_ok(c: &str) -> bool {
    c.len() == 3 && c.chars().all(|c| c.is_ascii_uppercase())
}

impl Settings {
    pub fn from_map(m: &Map<String, J>) -> Settings {
        let d = Settings::default();
        let title = text(m, "title", 80).unwrap_or(d.title);
        let logo_url = text(m, "logo_url", 500).filter(|u| {
            clean_url(u)
                && ((u.starts_with("https://") && !u.contains("@"))
                    || (u.starts_with('/') && !u.starts_with("//")))
        });
        let social = match m.get("social") {
            Some(J::Object(o)) => {
                o.iter()
                    .filter_map(|(k, v)| {
                        let url = v.as_str()?.trim();
                        (clean_url(url) && url.starts_with("https://") && !k.trim().is_empty())
                            .then(|| {
                                (
                                    k.trim().chars().take(30).collect::<String>(),
                                    url.to_string(),
                                )
                            })
                    })
                    .take(8)
                    .collect()
            }
            _ => Vec::new(),
        };
        let security_contact = text(m, "security_contact", 200).and_then(|c| {
            if c.contains('@') && !c.contains(':') && clean_url(&c) {
                Some(format!("mailto:{c}"))
            } else if (c.starts_with("mailto:") || c.starts_with("https://")) && clean_url(&c) {
                Some(c)
            } else {
                None
            }
        });
        Settings {
            enabled: flag(m, "enabled").unwrap_or(d.enabled),
            logo_text: text(m, "logo_text", 40).unwrap_or_else(|| title.clone()),
            tagline: text(m, "tagline", 160).unwrap_or_default(),
            logo_url,
            accent: text(m, "accent", 9)
                .and_then(|a| theme::parse_hex(&a).map(theme::to_hex))
                .unwrap_or(d.accent),
            footer_text: text(m, "footer_text", 300).unwrap_or_default(),
            social,
            default_lang: text(m, "default_lang", 10)
                .and_then(|l| Lang::parse(&l))
                .unwrap_or(d.default_lang),
            base_url: text(m, "base_url", 200)
                .map(|u| u.trim_end_matches('/').to_string())
                .filter(|u| origin_ok(u)),
            noindex: flag(m, "noindex").unwrap_or(false),
            security_contact,
            shop_currency: text(m, "shop_currency", 3)
                .map(|c| c.to_ascii_uppercase())
                .filter(|c| currency_ok(c))
                .unwrap_or(d.shop_currency),
            shop_page_size: m
                .get("shop_page_size")
                .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
                .map_or(d.shop_page_size, |n| n.clamp(4, 48) as usize),
            checkout_note: text(m, "checkout_note", 300).unwrap_or_default(),
            title,
        }
    }
}

/// The keys the site understands, with a one-line description (for `obop site set` and the docs).
pub const KEYS: &[(&str, &str)] = &[
    (
        "enabled",
        "true or false; false takes the public site offline (503)",
    ),
    ("title", "site name, up to 80 characters"),
    ("tagline", "one line under the name, up to 160 characters"),
    ("logo_text", "text shown as the logo; defaults to the title"),
    (
        "logo_url",
        "https URL or /path of a logo image shown before the text",
    ),
    (
        "accent",
        "brand colour as #rrggbb; adjusted for contrast automatically",
    ),
    ("footer_text", "text in the footer, up to 300 characters"),
    (
        "social",
        "JSON object of label to https URL, for example {\"Mastodon\":\"https://...\"}",
    ),
    (
        "default_lang",
        "en, de, fr, pl, fi, sv or es: the language of the content and the default for visitors",
    ),
    (
        "base_url",
        "public origin such as https://shop.example.com; set it in production",
    ),
    ("noindex", "true keeps search engines away (a staging site)"),
    (
        "security_contact",
        "email or https URL published in /.well-known/security.txt",
    ),
    (
        "shop_currency",
        "ISO 4217 code used when a listing names none (default EUR)",
    ),
    ("shop_page_size", "products per page, 4 to 48 (default 12)"),
    (
        "checkout_note",
        "sentence shown at checkout, for example how you will ask for payment",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn from(v: J) -> Settings {
        Settings::from_map(v.as_object().unwrap())
    }

    #[test]
    fn defaults_are_sensible() {
        let s = from(json!({}));
        assert!(s.enabled && !s.noindex);
        assert_eq!(
            (s.title.as_str(), s.shop_currency.as_str()),
            ("My Shop", "EUR")
        );
        assert_eq!(s.default_lang, Lang::En);
        assert_eq!(s.shop_page_size, 12);
        assert_eq!(s.logo_text, "My Shop");
    }

    #[test]
    fn every_value_is_validated() {
        let s = from(json!({
            "title": "  Café <b>Nord</b>  ",
            "accent": "javascript:alert(1)",
            "logo_url": "javascript:alert(1)",
            "base_url": "https://evil.test/path?x=1",
            "default_lang": "xx",
            "shop_currency": "eur",
            "shop_page_size": 9999,
            "security_contact": "javascript:alert(1)",
            "social": {"Mastodon": "https://m.example/@me", "Bad": "javascript:x", "Http": "http://x.test"},
            "enabled": "false",
        }));
        assert_eq!(
            s.title, "Café <b>Nord</b>",
            "text is kept, escaped on output"
        );
        assert_eq!(s.accent, theme::DEFAULT_ACCENT);
        assert_eq!(s.logo_url, None);
        assert_eq!(s.base_url, None);
        assert_eq!(s.default_lang, Lang::En);
        assert_eq!(s.shop_currency, "EUR");
        assert_eq!(s.shop_page_size, 48);
        assert_eq!(s.security_contact, None);
        assert_eq!(
            s.social,
            [("Mastodon".to_string(), "https://m.example/@me".to_string())]
        );
        assert!(!s.enabled);
    }

    #[test]
    fn good_values_are_accepted() {
        let s = from(json!({
            "accent": "#E11D48", "logo_url": "/brand/logo.svg", "base_url": "https://shop.example.com/",
            "default_lang": "de", "security_contact": "sec@example.com", "noindex": true,
            "shop_currency": "SEK", "shop_page_size": "24",
        }));
        assert_eq!(s.accent, "#e11d48");
        assert_eq!(s.logo_url.as_deref(), Some("/brand/logo.svg"));
        assert_eq!(s.base_url.as_deref(), Some("https://shop.example.com"));
        assert_eq!(s.default_lang, Lang::De);
        assert_eq!(
            s.security_contact.as_deref(),
            Some("mailto:sec@example.com")
        );
        assert!(s.noindex);
        assert_eq!((s.shop_currency.as_str(), s.shop_page_size), ("SEK", 24));
    }

    #[test]
    fn origins_are_origins_only() {
        assert!(origin_ok("https://a.example"));
        assert!(origin_ok("http://localhost:8080"));
        for bad in [
            "https://a.example/",
            "https://a.example/x",
            "ftp://a",
            "https://",
            "https://u@a.example",
            "//a.example",
            "https://a b",
        ] {
            assert!(!origin_ok(bad), "{bad}");
        }
    }
}
