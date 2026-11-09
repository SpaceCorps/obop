//! The HTML of the signer's pages (D92). Server-rendered, no script, no `style` attribute, no
//! cookie: one stylesheet, allowed by its hash. The words come from the catalogue of
//! `sign_text.rs`; this file knows the markup and nothing about the store.
//!
//! The list of the signatures so far on the file (`History`) is shown only when the request asks
//! for it; a page without it is the page it always was, byte for byte, and the stylesheet is not
//! touched for it (it would change the hash in the policy and so every page).
//!
//! **The instance brand (docs/spec/branding.md, slice S1d).** Every page has the line above the card
//! of the OAuth pages, `obop_oauth::page::brand_line`, and, with a brand set, the tab icon link and
//! `img-src 'self'` at the end of the policy (`icon_link` and `img_src`, the same functions, so the
//! two pages follow one predicate). The caller reads the brand once, first, and hands that one value
//! to the page. The texts of the page name no product (the catalogue of `sign_text.rs` has none) and
//! the brand never enters them: the words, and so the consent id that is hashed from them, are the
//! same with a brand and without one. With no brand every page is what it was, byte for byte.
//!
//! The PDF is offered to the browser's own viewer in an `<object>` that falls back to the text
//! "open it in full size", and a visible link opens it in a new tab (a phone shows one page of an
//! embedded PDF at most, so on a coarse pointer the object is hidden and the link is the way to
//! read it). The page's policy names `frame-src 'self'`: Chromium's viewer is a nested frame and
//! `default-src 'none'` would otherwise leave the box empty (spike of 2026-11-03, docs/SIGN.md).

use crate::sign_text::{self as txt, text};
use obop_commands::brand::Shown;
use obop_commands::sign::History;
use obop_oauth::page::{brand_line, esc, icon_link, img_src};
use obop_oauth::util::b64std;
use sha2::{Digest, Sha256};

pub(crate) struct Page {
    pub html: String,
    pub csp: String,
}

const CSS: &str = r#"
:root{--bg:#f6f7f9;--card:#fff;--fg:#14181f;--muted:#4b5565;--line:#d5dae2;--accent:#1d4ed8;--accent-fg:#fff;--danger:#b42318;--danger-bg:#fef3f2;--warn-bg:#fffaeb;--warn-line:#f2c86b;color-scheme:light dark}
@media (prefers-color-scheme:dark){:root{--bg:#0e1116;--card:#171b22;--fg:#eceff4;--muted:#a3adbd;--line:#2c3341;--accent:#6ea0ff;--accent-fg:#0b1020;--danger:#ff8f85;--danger-bg:#3a1713;--warn-bg:#2b2210;--warn-line:#7a5d17}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.5 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
main{max-width:46rem;margin:0 auto;padding:24px 16px 48px}
.brand{font-weight:700;letter-spacing:.02em;margin:0 0 16px}
.card{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:20px}
h1{font-size:1.35rem;line-height:1.3;margin:0 0 12px}
h2{font-size:1rem;margin:20px 0 6px}
p{margin:8px 0}
.muted{color:var(--muted)}
.msg{white-space:pre-wrap;overflow-wrap:anywhere}
.notice{border:1px solid var(--warn-line);background:var(--warn-bg);border-radius:8px;padding:10px 12px;margin:12px 0}
.error{border:1px solid var(--danger);background:var(--danger-bg);color:var(--danger);border-radius:8px;padding:10px 12px;margin:12px 0}
object.doc{display:block;width:100%;height:70vh;min-height:24rem;border:1px solid var(--line);border-radius:8px;background:var(--bg)}
label{display:block;font-weight:600;margin:12px 0 4px}
input[type=text]{width:100%;font:inherit;color:var(--fg);background:var(--bg);border:1px solid var(--line);border-radius:8px;padding:10px 12px;min-height:44px}
.opt{display:flex;gap:10px;align-items:flex-start;font-weight:400;margin:16px 0 8px;min-height:32px}
.opt input{margin-top:5px;width:20px;height:20px;flex:none}
.opt label{font-weight:400;margin:0}
button{font:inherit;font-weight:600;border-radius:8px;border:1px solid var(--line);background:var(--card);color:var(--fg);padding:10px 18px;min-height:44px;cursor:pointer}
button.primary{background:var(--accent);border-color:var(--accent);color:var(--accent-fg)}
a.btn{display:block;box-sizing:border-box;text-align:center;text-decoration:none;font-weight:600;border-radius:8px;border:1px solid var(--line);background:var(--card);color:var(--fg);padding:10px 18px;min-height:44px}
button:focus-visible,input:focus-visible,a.btn:focus-visible{outline:3px solid var(--accent);outline-offset:2px}
@media (pointer:coarse){object.doc{display:none}}
"#;

/// The policy of every page. With a brand set it ends with `img-src 'self'` for the tab icon, and
/// is otherwise the policy it always was (the hash of the stylesheet does not change).
fn csp(brand: &Shown) -> String {
    let hash = b64std(&Sha256::digest(CSS.as_bytes()));
    let img = img_src(brand);
    format!(
        "default-src 'none'; style-src 'sha256-{hash}'; object-src 'self'; frame-src 'self'; child-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'{img}"
    )
}

fn shell(lang: &str, title: &str, body: &str, brand: &Shown) -> Page {
    let html = format!(
        "<!doctype html>\n<html lang=\"{lang}\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<meta name=\"color-scheme\" content=\"light dark\"><meta name=\"referrer\" content=\"same-origin\">\
<meta name=\"robots\" content=\"noindex,nofollow\"><title>{title}</title>{icon}<style>{CSS}</style></head>\
<body><main>{line}<div class=\"card\">{body}</div></main></body></html>\n",
        title = esc(title),
        icon = icon_link(brand),
        line = brand_line(brand, lang),
    );
    Page {
        html,
        csp: csp(brand),
    }
}

/// A page with one message: the link is over, unknown, busy, or a post that did not come
/// from the page.
pub(crate) fn message(lang: &str, key: &str, brand: &Shown) -> Page {
    shell(
        lang,
        text(lang, "title"),
        &format!(
            "<h1>{}</h1><p>{}</p>",
            esc(text(lang, "title")),
            txt::html(lang, key, &[])
        ),
        brand,
    )
}

/// The page after signing, and for a link that was used. `history` is the list of the signatures
/// so far when the request asks for it, with the signer's own entry marked.
pub(crate) fn done(lang: &str, at: &str, history: Option<&History>, brand: &Shown) -> Page {
    shell(
        lang,
        text(lang, "done_title"),
        &format!(
            "<h1>{}</h1><p>{}</p>{}",
            txt::html(lang, "done_title", &[]),
            txt::html(lang, "done_body", &[("time", at)]),
            history.map_or_else(String::new, |h| history_block(lang, h))
        ),
        brand,
    )
}

/// The block "Signed so far": a heading and the entries, oldest first, each the typed name and the
/// time (UTC, to the second) and nothing else; the line for an empty list; the line that says the
/// list is cut. Every value is escaped. No class is new, so the stylesheet is the one of every page.
fn history_block(lang: &str, h: &History) -> String {
    let mut out = format!("<h2>{}</h2>", txt::html(lang, "history_title", &[]));
    if h.entries.is_empty() {
        out.push_str(&format!(
            "<p class=\"muted\">{}</p>",
            txt::html(lang, "history_none", &[])
        ));
    } else {
        out.push_str("<ul>");
        for e in &h.entries {
            let you = if e.own {
                format!(" <strong>{}</strong>", txt::html(lang, "history_you", &[]))
            } else {
                String::new()
            };
            out.push_str(&format!(
                "<li>{} <span class=\"muted\">{}</span>{you}</li>",
                esc(&e.name),
                esc(&e.at)
            ));
        }
        out.push_str("</ul>");
    }
    if h.cut {
        out.push_str(&format!(
            "<p class=\"muted\">{}</p>",
            txt::html(lang, "history_more", &[])
        ));
    }
    out
}

/// What the form shows.
pub(crate) struct Form<'a> {
    pub rid: &'a str,
    pub token: &'a str,
    pub title: &'a str,
    pub message: &'a str,
    pub signer_name: &'a str,
    pub document_title: &'a str,
    pub document_hash: &'a str,
    /// The name as typed on an earlier try, to be given back.
    pub typed: &'a str,
    /// `err_name`, `err_agree` or `err_history` when the last try was refused.
    pub error: Option<&'a str>,
    /// The signatures so far on this file, when the request asks for them (`show_history`): the
    /// list, the sentence after the notice that tells the signer their name and time are shown, and
    /// the hidden field that lets the door see the signer read it. `None` leaves the page as it was.
    pub history: Option<&'a History>,
}

pub(crate) fn form(lang: &str, f: &Form<'_>, brand: &Shown) -> Page {
    let base = format!("/sign/{}/{}", esc(f.rid), esc(f.token));
    let pdf = format!("{base}/document.pdf");
    let error = f.error.map_or_else(String::new, |k| {
        format!(
            "<div class=\"error\" role=\"alert\">{}</div>",
            txt::html(lang, k, &[])
        )
    });
    let message = if f.message.is_empty() {
        String::new()
    } else {
        format!("<p class=\"msg\">{}</p>", esc(f.message))
    };
    let short: String = f.document_hash.chars().take(16).collect();
    let body = format!(
        "<h1>{request}</h1><p class=\"muted\">{for_}</p>{message}\
<h2>{document}: {doc_title}</h2>\
<object class=\"doc\" type=\"application/pdf\" data=\"{pdf}\"><p>{no_viewer}</p></object>\
<p><a class=\"btn\" href=\"{pdf}\" target=\"_blank\" rel=\"noopener noreferrer\">{open_full}</a></p>\
<p class=\"muted\">{fingerprint}</p>{history}{error}\
<form method=\"post\" action=\"{base}\">\
<input type=\"hidden\" name=\"doc\" value=\"{hash}\"><input type=\"hidden\" name=\"lang\" value=\"{lang}\">{seen}\
<label for=\"name\">{name_label}</label>\
<input id=\"name\" name=\"name\" type=\"text\" maxlength=\"120\" autocomplete=\"name\" required value=\"{typed}\">\
<p class=\"muted\">{name_hint}</p>\
<div class=\"opt\"><input id=\"agree\" type=\"checkbox\" name=\"agree\" value=\"1\" required>\
<label for=\"agree\">{consent}</label></div>\
<p class=\"notice\">{records}{history_notice}</p>\
<button class=\"primary\" type=\"submit\">{submit}</button></form>",
        request = txt::html(lang, "request", &[("title", f.title)]),
        for_ = txt::html(lang, "for", &[("name", f.signer_name)]),
        document = txt::html(lang, "document", &[]),
        doc_title = esc(f.document_title),
        no_viewer = txt::html(lang, "no_viewer", &[]),
        open_full = txt::html(lang, "open_full", &[]),
        fingerprint = txt::html(lang, "fingerprint", &[("hash", &short)]),
        hash = esc(f.document_hash),
        lang = esc(lang),
        name_label = txt::html(lang, "name_label", &[]),
        typed = esc(f.typed),
        name_hint = txt::html(lang, "name_hint", &[]),
        consent = txt::html(lang, "consent", &[("name", f.signer_name)]),
        records = txt::html(lang, "records", &[]),
        submit = txt::html(lang, "submit", &[]),
        history = f
            .history
            .map_or_else(String::new, |h| history_block(lang, h)),
        seen = if f.history.is_some() {
            "<input type=\"hidden\" name=\"hist\" value=\"1\">"
        } else {
            ""
        },
        history_notice = if f.history.is_some() {
            format!(" {}", txt::html(lang, "history_notice", &[]))
        } else {
            String::new()
        },
    );
    shell(
        lang,
        &txt::fill(lang, "request", &[("title", f.title)]),
        &body,
        brand,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_oauth::page::LANGS;

    /// The brand of an instance with no row: the pages are what they were before the brand.
    fn unbranded() -> Shown {
        Shown::not_set(None)
    }

    fn form_html(lang: &str) -> String {
        form(
            lang,
            &Form {
                rid: "rid1",
                token: "tok1",
                title: "Test NDA",
                message: "",
                signer_name: "Jane Test",
                document_title: "Test NDA",
                document_hash: &"ab".repeat(32),
                typed: "",
                error: None,
                history: None,
            },
            &unbranded(),
        )
        .html
    }

    /// The first element of `html` that starts with `<{tag}` and has `needle` in its opening tag.
    fn tag_with<'a>(html: &'a str, tag: &str, needle: &str) -> &'a str {
        let open = format!("<{tag}");
        let mut from = 0;
        while let Some(i) = html[from..].find(&open) {
            let start = from + i;
            let end = start + html[start..].find('>').expect("an unclosed tag");
            if html[start..=end].contains(needle) {
                return &html[start..=end];
            }
            from = end;
        }
        panic!("no <{tag}> with {needle} in the page");
    }

    fn sha(s: &str) -> String {
        Sha256::digest(s.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// The pages of a request that does not ask for the history, for the same input, as the code
    /// before the history rendered them (taken from `origin/main` at 0986150, before any change of
    /// this feature): the SHA-256 of the whole page, for the form, the page after signing and a
    /// refusal. A change to the markup, the stylesheet or the texts of these pages fails it.
    const BEFORE_THE_HISTORY: [(&str, &str, &str, &str); 7] = [
        (
            "en",
            "e2f5a74fadf062bd79179968af846ddc813393613acf41ba2e9941cb51863003",
            "00aeefff203f27f268115d0486b7b96f72625ed1ad5f6c67d2b74415e9de5d0f",
            "fc0e820fa0379310efbbc309ec8a279f708695ded8eb166a0a69d9387aeabada",
        ),
        (
            "de",
            "6cc03defc51ddf23c8b7c3873a67d4d3fa7a1bb162742ae256031e892f68fcff",
            "9f923b401b135fc382a046fd277a2381e7186c260a06beaa089d75ce0fbe3dad",
            "d76174b3d47b31039d055b962c3bcf251bf10116f43f02fd675983c4f4f686e5",
        ),
        (
            "fr",
            "b4c3aeaf9fbb9f847707c1bbab7466bbe10ee4413c976239d718f6b869c4aeeb",
            "5d2289079d28fa8e573163bda368759d20fbd233e4d93522571a5e1aa05320cb",
            "7dc5fa9345cc22488faec6b1ffedd3e89f311cdf1d3f3d48f6ab163ca4a702a5",
        ),
        (
            "pl",
            "a0f2e158d6827802ddf37019ed9c2778f2d80db160fc182115be2a64014fc7dc",
            "02f1e59b6267abe7d667e7e57dd0a17f672c1e2b86b45884ce086b263fbd3dd5",
            "1e5cab3926d75356d646ad4f4adbe29e9cd603ac5245cfa487ba6e74f9f0b387",
        ),
        (
            "fi",
            "3148cb8f06c2070f537619b392af000d0365eddac3f92473092c3c0ac8bc3a5d",
            "cddb81a95ce9401428fded6e9d8db9104d73811cca0faeef0932bbb9b80879f9",
            "aa5471f0c91b1887cca657e115958611610af0e70cef2777c6360e8d54e60590",
        ),
        (
            "sv",
            "1eb834f4e95498af9e15239d73c84131fdeaefeb9c84eebf8405b02a85f0e1cf",
            "9549d7acf2ef94dd44dfaa9105ef7922760faca82124e07a2fc4cb2e46d1b0ef",
            "4340a6f63b713a38b91c1a0d84d5caef884533ce35dd646d471fbabcf7809b21",
        ),
        (
            "es",
            "4e84a06fb316ecc79a5b35d8fb0c7e26ec35781b0b23d48999abd10871dbd19d",
            "da15b9b7dc498c819ddd78425064f0edc01e0849bf6e27e7d3f21629ac1b653f",
            "49fc77ed26da535e42adfde6fba660a18240178580a8f6e6b543ef5fc9f8435e",
        ),
    ];

    #[test]
    fn the_pages_without_a_history_are_byte_for_byte_what_they_were_before_it() {
        for (lang, form_hash, done_hash, closed_hash) in BEFORE_THE_HISTORY {
            assert_eq!(sha(&form_html(lang)), form_hash, "{lang}: the form");
            assert_eq!(
                sha(&done(lang, "2026-11-03T10:00:00.000000Z", None, &unbranded()).html),
                done_hash,
                "{lang}: the page after signing"
            );
            assert_eq!(
                sha(&message(lang, "closed", &unbranded()).html),
                closed_hash,
                "{lang}: a refusal"
            );
        }
    }

    /// The form again after a refused try (`err_name`, `err_agree`), as the door gives it back.
    fn form_again(lang: &str, error: &str, history: Option<&History>) -> String {
        form(
            lang,
            &Form {
                rid: "rid1",
                token: "tok1",
                title: "Test NDA",
                message: "",
                signer_name: "Jane Test",
                document_title: "Test NDA",
                document_hash: &"ab".repeat(32),
                typed: "",
                error: Some(error),
                history,
            },
            &unbranded(),
        )
        .html
    }

    #[test]
    fn the_form_that_comes_back_after_a_refused_try_has_no_history_when_the_request_does_not_ask() {
        for lang in LANGS {
            let plain = form_html(lang);
            let sentence = txt::html(lang, "history_notice", &[]);
            for key in ["err_name", "err_agree"] {
                let again = form_again(lang, key, None);
                // Not the hidden field and not the sentence: the signer of a request that does not
                // ask is not told that their name is shown, and the post does not claim it was.
                assert!(
                    !again.contains("name=\"hist\""),
                    "{lang} {key}: the hidden field"
                );
                assert!(!again.contains(&sentence), "{lang} {key}: the sentence");
                // The error line is the only difference from the page they first had.
                let line = format!(
                    "<div class=\"error\" role=\"alert\">{}</div>",
                    txt::html(lang, key, &[])
                );
                assert_eq!(
                    again.matches(&line).count(),
                    1,
                    "{lang} {key}: the error line"
                );
                // It sits where the door puts it, right before the form, and nothing else moved.
                let expected = plain.replacen("<form method", &format!("{line}<form method"), 1);
                assert_eq!(again, expected, "{lang} {key}: nothing else");
            }
        }
    }

    fn entry(name: &str, at: &str, own: bool) -> obop_commands::sign::HistoryEntry {
        obop_commands::sign::HistoryEntry {
            name: name.to_string(),
            at: at.to_string(),
            own,
        }
    }

    fn form_with(lang: &str, history: Option<&History>) -> Page {
        form_in(lang, history, &unbranded())
    }

    /// The form again, for the brand a request read.
    fn form_in(lang: &str, history: Option<&History>, brand: &Shown) -> Page {
        form(
            lang,
            &Form {
                rid: "rid1",
                token: "tok1",
                title: "Test NDA",
                message: "",
                signer_name: "Jane Test",
                document_title: "Test NDA",
                document_hash: &"ab".repeat(32),
                typed: "",
                error: None,
                history,
            },
            brand,
        )
    }

    #[test]
    fn a_page_with_a_history_adds_to_the_page_and_changes_neither_the_style_nor_the_policy() {
        let h = History {
            entries: vec![
                entry("Alice Example", "2026-11-03T10:00:00Z", false),
                entry("Bob Example", "2026-11-03T11:30:05Z", false),
            ],
            cut: false,
        };
        for lang in LANGS {
            let plain = form_with(lang, None);
            let with = form_with(lang, Some(&h));
            assert_eq!(plain.csp, with.csp, "{lang}: the policy");
            assert!(with.html.contains(CSS), "{lang}");
            // The block sits between the fingerprint and the form, in the order given.
            let title = txt::html(lang, "history_title", &[]);
            let fingerprint = txt::html(lang, "fingerprint", &[("hash", &"ab".repeat(8))]);
            let (f, t, a, b, form_at) = (
                with.html.find(&fingerprint).unwrap(),
                with.html.find(&format!("<h2>{title}</h2>")).unwrap(),
                with.html.find("Alice Example").unwrap(),
                with.html.find("Bob Example").unwrap(),
                with.html.find("<form").unwrap(),
            );
            assert!(f < t && t < a && a < b && b < form_at, "{lang}: order");
            assert!(with.html.contains("2026-11-03T10:00:00Z"), "{lang}");
            assert!(with.html.contains("2026-11-03T11:30:05Z"), "{lang}");
            // The sentence is in the notice, after the words it already holds, and the form tells
            // the door that the signer was shown it.
            let records = txt::html(lang, "records", &[]);
            let notice = txt::html(lang, "history_notice", &[]);
            assert!(
                with.html
                    .contains(&format!("<p class=\"notice\">{records} {notice}</p>")),
                "{lang}: the notice"
            );
            assert_eq!(with.html.matches("name=\"hist\"").count(), 1, "{lang}");
            assert_eq!(plain.html.matches("name=\"hist\"").count(), 0, "{lang}");
            // Nothing else was added: take the additions out and the page is the plain one.
            let stripped = with
                .html
                .replace(&format!(" {notice}"), "")
                .replace("<input type=\"hidden\" name=\"hist\" value=\"1\">", "");
            let block_start = stripped.find(&format!("<h2>{title}</h2>")).unwrap();
            let block_end = stripped[block_start..].find("<form").unwrap() + block_start;
            let stripped = format!("{}{}", &stripped[..block_start], &stripped[block_end..]);
            assert_eq!(stripped, plain.html, "{lang}");
            // Still no script and no style attribute.
            assert!(!with.html.contains("<script") && !with.html.contains(" style="));
        }
    }

    #[test]
    fn the_history_block_is_escaped_marks_the_viewer_and_says_when_nothing_or_not_everything() {
        let evil = "<script>alert(1)</script> & \"q\" 'x'";
        let h = History {
            entries: vec![
                entry(evil, "2026-11-03T10:00:00Z", false),
                entry("Alice Example", "2026-11-03T10:00:01Z", true),
            ],
            cut: true,
        };
        let page = done("en", "2026-11-03T10:00:01.000000Z", Some(&h), &unbranded()).html;
        assert!(!page.contains("<script"), "{page}");
        assert!(
            page.contains(&esc(evil)) && esc(evil).contains("&lt;script&gt;"),
            "{page}"
        );
        // Only the viewer's own row is marked.
        assert_eq!(page.matches("<strong>(you)</strong>").count(), 1, "{page}");
        let own = page.find("Alice Example").unwrap();
        assert!(page[own..].contains("<strong>(you)</strong>"));
        assert!(page[..own].matches("<strong>").count() == 0);
        // Cut: the line says so.
        assert!(page.contains("Only the earliest signatures are listed here."));
        // Empty, and not cut.
        let none = done(
            "en",
            "2026-11-03T10:00:01.000000Z",
            Some(&History::default()),
            &unbranded(),
        )
        .html;
        assert!(none.contains("Nobody has signed this file yet."));
        assert!(!none.contains("<ul>") && !none.contains("Only the earliest"));
        // The page after signing is the old one plus the block, in every language.
        for lang in LANGS {
            let plain = done(lang, "2026-11-03T10:00:01.000000Z", None, &unbranded()).html;
            let with = done(lang, "2026-11-03T10:00:01.000000Z", Some(&h), &unbranded()).html;
            let head = &plain[..plain.find("</div></main>").unwrap()];
            assert!(
                with.starts_with(head),
                "{lang}: the old page is the start of the new"
            );
            assert!(
                with.contains(&txt::html(lang, "history_you", &[])),
                "{lang}"
            );
        }
    }

    #[test]
    fn the_consent_checkbox_is_named_by_the_consent_sentence() {
        for lang in LANGS {
            let html = form_html(lang);
            let input = tag_with(&html, "input", "type=\"checkbox\"");
            // The box has an id and a label next to it that says `for` that id and holds the
            // sentence, the plain pattern every browser, screen reader and test tool reads. (A
            // label that wraps the box is named correctly by a browser, but a tool that only
            // follows `for` shows the box's value, "1", as its name.)
            assert!(input.contains("id=\"agree\""), "{lang}: {input}");
            let label = tag_with(&html, "label", "for=\"agree\"");
            let at = html.find(label).unwrap() + label.len();
            let sentence = txt::html(lang, "consent", &[("name", "Jane Test")]);
            assert!(
                html[at..].starts_with(&format!("{sentence}</label>")),
                "{lang}: the label of the box does not hold the consent sentence"
            );
            // It is the box's own label: next to it in the row, not around something else.
            let row = tag_with(&html, "div", "class=\"opt\"");
            let row_at = html.find(row).unwrap() + row.len();
            assert!(
                html[row_at..].starts_with(input),
                "{lang}: the box is not the first thing in its row"
            );
            // The value stays what the door reads, and the box is still required.
            assert!(
                input.contains("name=\"agree\"") && input.contains("value=\"1\""),
                "{lang}: {input}"
            );
            assert!(input.contains(" required"), "{lang}: {input}");
            // Every id on the page is used once, so a name cannot point at two things.
            for id in ["agree", "name"] {
                assert_eq!(
                    html.matches(&format!("id=\"{id}\"")).count(),
                    1,
                    "{lang}: id {id}"
                );
            }
            assert_eq!(html.matches("for=\"agree\"").count(), 1, "{lang}");
        }
    }

    #[test]
    fn the_page_stays_free_of_script_and_style_attributes() {
        let html = form_html("en");
        assert!(!html.contains("<script") && !html.contains(" style="));
        for handler in [
            "onclick", "onload", "onerror", "onsubmit", "onchange", "oninput",
        ] {
            assert!(
                !html.contains(&format!(" {handler}=")),
                "{handler} on the page"
            );
        }
    }

    /// The title of a request is staff text, and `fill` leaves it raw, so the `<title>` element of
    /// the form is made safe by `shell` alone. A title that closes the element and opens a tag is
    /// text in the head, with a brand and without, in every language.
    #[test]
    fn the_title_of_the_request_is_escaped_in_the_title_element_of_the_form() {
        let hostile = "T</title><i>x</i>";
        let escaped = "T&lt;/title&gt;&lt;i&gt;x&lt;/i&gt;";
        let plain = unbranded();
        let with = mohv();
        for brand in [&plain, &*with] {
            for lang in LANGS {
                let html = form(
                    lang,
                    &Form {
                        rid: "rid1",
                        token: "tok1",
                        title: hostile,
                        message: "",
                        signer_name: "Jane Test",
                        document_title: "Test NDA",
                        document_hash: &"ab".repeat(32),
                        typed: "",
                        error: None,
                        history: None,
                    },
                    brand,
                )
                .html;
                let open = html.find("<title>").expect("a title element") + "<title>".len();
                let close = html.find("</title>").expect("a title element");
                assert!(
                    html[open..close].contains(escaped),
                    "{lang}: {}",
                    &html[open..close]
                );
                assert_eq!(html.matches("</title>").count(), 1, "{lang}");
                assert!(
                    !html.contains("<i>") && !html.contains("</title><i"),
                    "{lang}: the title is a tag on the page"
                );
            }
        }
    }

    #[test]
    fn a_foreign_post_and_a_closed_request_show_their_own_texts_in_every_language() {
        for lang in LANGS {
            for key in ["forbidden", "closed"] {
                let page = message(lang, key, &unbranded());
                assert!(
                    page.html.contains(&txt::html(lang, key, &[])),
                    "{lang}/{key}: the page does not hold the catalogue text"
                );
                assert!(
                    page.html.contains(&format!("<html lang=\"{lang}\"")),
                    "{lang}/{key}: wrong page language"
                );
            }
        }
    }

    /// The `"key"` literals that follow `prefix` in `src`.
    fn after<'a>(src: &'a str, prefix: &str) -> Vec<&'a str> {
        src.match_indices(prefix)
            .filter_map(|(i, _)| {
                let rest = src[i + prefix.len()..].strip_prefix('"')?;
                Some(&rest[..rest.find('"')?])
            })
            .collect()
    }

    #[test]
    fn a_key_asked_for_below_a_test_module_is_seen_and_one_in_it_is_not() {
        // The file is cut at the end of the test items, not at the first `#[cfg(test)]`: here a
        // test-only `use` comes first and a key is asked for below it. (In `sign.rs` the first
        // `#[cfg(test)]` is the test module, below every handler, so the real file would not
        // show the difference; the files where it matters are those like this crate's `lib.rs`,
        // with a test-only module declared near the top.)
        let src = "#[cfg(test)]
use crate::x;
fn early() { txt::html(lang, \"early_key\"); }
#[cfg(test)]
mod tests { fn t() { txt::html(lang, \"only_in_tests\"); } }
fn late() { txt::html(lang, \"late_key\"); }
";
        let code = crate::src_scan::without_test_items(src);
        let keys = after(&code, "txt::html(lang, ");
        assert_eq!(keys, ["early_key", "late_key"], "{code}");
    }

    #[test]
    fn every_key_the_door_and_its_pages_ask_for_is_in_the_catalogue() {
        // A key that is only in the code reads as an empty paragraph, in every language. This
        // reads the two files that ask for words and checks each literal key against the catalogue.
        let sources = [
            crate::src_scan::without_test_items(include_str!("sign.rs")),
            crate::src_scan::without_test_items(include_str!("sign_page.rs")),
        ];
        let prefixes = [
            "txt::html(lang, ",
            "txt::fill(lang, ",
            "text(lang, ",
            "message(lang, ",
            "StatusCode::NOT_FOUND, ",
            "StatusCode::GONE, ",
            "StatusCode::CONFLICT, ",
            "StatusCode::TOO_MANY_REQUESTS, ",
            "StatusCode::SERVICE_UNAVAILABLE, ",
            "StatusCode::BAD_REQUEST, ",
            "=> ",
        ];
        let mut seen = std::collections::BTreeSet::new();
        for src in &sources {
            for p in prefixes {
                for key in after(src, p) {
                    // `=> "x"` is any match arm; only the error keys of this catalogue are looked up.
                    if p == "=> " && !key.starts_with("err_") {
                        continue;
                    }
                    seen.insert(key);
                }
            }
        }
        assert!(
            ["closed", "forbidden", "gone_revoked", "err_agree"]
                .iter()
                .all(|k| seen.contains(k)),
            "the scan missed a key it should find: {seen:?}"
        );
        for key in seen {
            assert!(
                crate::sign_text::CATALOGUE.iter().any(|(k, _)| *k == key),
                "the code asks for the key {key}, which the catalogue does not have"
            );
        }
    }

    // ---- Golden pins of the whole page and its policy (S1a-0 of the branding work) -----------

    /// The pin of a page: the SHA-256 of its HTML joined to its policy header, in this form
    /// (`BEFORE_THE_HISTORY` above hashes the HTML alone):
    ///
    /// ```text
    /// {html}\nContent-Security-Policy: {policy}\n
    /// ```
    ///
    /// The signer pages are therefore pinned as HTML plus the policy header only: the status code
    /// and the other headers of the answer are not pinned here (the same holds for the OAuth pages;
    /// `obop-oauth/src/golden_tests.rs` says so too and lists every filter that runs the pins).
    fn pin_of(p: &Page) -> String {
        sha(&format!("{}\nContent-Security-Policy: {}\n", p.html, p.csp))
    }

    /// Print the table as source (`OBOP_PRINT_PINS=1 cargo test -p obop-http signer_pins --
    /// --nocapture --test-threads=1`), or hold the pages to it.
    fn check(table: &str, pinned: &[(&str, &str)], now: &[(String, String)]) {
        if std::env::var("OBOP_PRINT_PINS").is_ok_and(|v| v == "1") {
            let mut out = format!("\n//@pins {table}\nconst {table}: &[(&str, &str)] = &[\n");
            for (name, digest) in now {
                out.push_str(&format!("    (\"{name}\", \"{digest}\"),\n"));
            }
            out.push_str(&format!("];\n//@end {table}\n"));
            println!("{out}");
            panic!(
                "OBOP_PRINT_PINS=1 is print mode, not a check: this run printed the table and \
                 proved nothing. Unset it to run the pins."
            );
        }
        assert!(
            !pinned.is_empty(),
            "{table} is empty: print it with OBOP_PRINT_PINS=1"
        );
        let mut moved = Vec::new();
        for (i, (name, digest)) in now.iter().enumerate() {
            match pinned.get(i) {
                Some((n, d)) if n == name && d == digest => {}
                Some((n, _)) if n != name => moved.push(format!("{name}: the table has {n} here")),
                Some(_) => moved.push(format!("{name}: {digest}")),
                None => moved.push(format!("{name}: not in the table")),
            }
        }
        if pinned.len() > now.len() {
            moved.push(format!(
                "the table has {} entries, the test makes {}",
                pinned.len(),
                now.len()
            ));
        }
        assert!(
            moved.is_empty(),
            "{table}: {} of {} pins moved (a page differs from what an instance with no brand \
             row has always sent):\n{}",
            moved.len(),
            now.len(),
            moved.join("\n")
        );
    }

    /// The signatures so far, as a page that asks for them shows them: one other signer, the
    /// viewer's own entry, and the line that says the list is cut.
    fn pinned_history() -> History {
        History {
            entries: vec![
                entry("Alice Example", "2026-11-03T10:00:00Z", false),
                entry("Bob Example", "2026-11-03T11:30:05Z", true),
            ],
            cut: true,
        }
    }

    /// Every page the signer's door can send, in one language, for the brand a request read: the
    /// form, the page after signing, the closed request, the same two with the history, and the
    /// eight refusals (`refusal_parts` in `sign.rs`). 13 pages, named as the pins name them.
    fn every_page(lang: &str, brand: &Shown) -> Vec<(String, Page)> {
        let history = pinned_history();
        let mut out = Vec::new();
        let mut add = |kind: &str, p: Page| out.push((kind.to_string(), p));
        add("form", form_in(lang, None, brand));
        add(
            "done",
            done(lang, "2026-11-03T10:00:00.000000Z", None, brand),
        );
        add("closed", message(lang, "closed", brand));
        add("form_history", form_in(lang, Some(&history), brand));
        add(
            "done_history",
            done(lang, "2026-11-03T10:00:00.000000Z", Some(&history), brand),
        );
        for key in [
            "unknown",
            "gone_expired",
            "gone_revoked",
            "changed",
            "busy",
            "unavailable",
            "err_name",
            "forbidden",
        ] {
            add(&format!("msg_{key}"), message(lang, key, brand));
        }
        out
    }

    /// Every page the signer's door can send, in every language, with its policy header. 13 pages in
    /// each of the 7 languages.
    #[test]
    fn signer_pins_every_page_and_its_policy_is_what_it_was_before_the_brand() {
        let mut now = Vec::new();
        for lang in LANGS {
            for (kind, p) in every_page(lang, &unbranded()) {
                now.push((format!("{kind}/{lang}"), pin_of(&p)));
            }
        }
        assert_eq!(now.len(), 13 * 7, "13 pages in 7 languages");
        check("SIGNER_PINS", SIGNER_PINS, &now);
    }

    // Printed by `OBOP_PRINT_PINS=1` on origin/main 12a59a79 and pasted here.
    #[rustfmt::skip]
    const SIGNER_PINS: &[(&str, &str)] = &[
        ("form/en", "1e1d61cad3d9af5e945acefb1c286c694b30929cf95938f16606ac9ef128a32f"),
        ("done/en", "ce47739c881ba48ad44e4455cbd57c76c30ae86ea238cff596bba02ac72bec85"),
        ("closed/en", "6e684a757dfb11494f102e1d90073a9e702d1f101fe722ad7c40a042dc7a3e5f"),
        ("form_history/en", "10e23fa0199c3b28aff773a8b789b741f8692cb87fc616d13b68cda0f0a5bc1d"),
        ("done_history/en", "1092efbae7248530dbea2b67c3a08adb0ce3d822323ccd766735d92a8ddd3e7d"),
        ("msg_unknown/en", "d7cb391cdb401549948bdd034bbe9ce3c60571608d4b623f8e453fa8cfee60dc"),
        ("msg_gone_expired/en", "04e23020bf82642ed3cf06d708ea501204ed9b7e7bec0d1d83563918a7482fc7"),
        ("msg_gone_revoked/en", "bb4b357166fad56c9fcbcce4798066a2cb5479d0891b46ffbcfb5691816ec8ea"),
        ("msg_changed/en", "3347857fa267f186efaf4b9a7105b3420b61db17fad910409a5f047d1289b245"),
        ("msg_busy/en", "7948c335b5550837708bff8dbf71731e51f6cb91339c96cf41fa7783c8fc53d4"),
        ("msg_unavailable/en", "ea56b5716c103b12be2631c95035778d480547643b1b731e6a0cb538a6ac32b4"),
        ("msg_err_name/en", "b25ae8e71423892640c8363924790e69eec36dfd289e0be565fec23eb106995a"),
        ("msg_forbidden/en", "4acf9e30d488b55ae0679dee0acc36dad17351badc291af1f366790f6dcf7800"),
        ("form/de", "9d6ff40df2320f0d18c4cf87f36d62986e76bf629f6ad318fea2d9481bd55820"),
        ("done/de", "0d0e8e6912f6b2382f94e90968e801cf76b83f77ec5f2101be622fbd67c22b7b"),
        ("closed/de", "4e7747ae7b1553942944d25b041f8574ac877164fbc8626961e1b19a1f44aece"),
        ("form_history/de", "1ec5ff78e2ebb510240c746b05b695848a6804ba65e4f06d8d682bb0c0ac18f8"),
        ("done_history/de", "81a9e006d182b432480d07c307a2201627d6f07a66b4ec91446db6c60b4bc11a"),
        ("msg_unknown/de", "4e6cec7497089d57caa24f49452144a8ae0d07704595c0ac7ef60f509e44087f"),
        ("msg_gone_expired/de", "9673c46a6725a080134491e304c2aacbabd8751a4cec10a2168d896eae178b66"),
        ("msg_gone_revoked/de", "0802e9a90bbba1ccd846af058ef8955de344021524e8d4fdb8a5b1d12b437adb"),
        ("msg_changed/de", "0d408127537a9bf927811b0201e3c6d1f21a9e3b33d561c35c8984c3490b598e"),
        ("msg_busy/de", "dd04d4dbc331bea4702cc52feed76fb0292e353ae2ba63a49657ce70cdaaec6e"),
        ("msg_unavailable/de", "bf93933df3694c8ffb7d78b6c30781c16d857dd6920549febe84ff31e31bebaf"),
        ("msg_err_name/de", "82fd7834c8424122278a62fa75a474f228bb3b98aaf6abbfbc364f021f62e50b"),
        ("msg_forbidden/de", "8dfb498d51829dec1ed614c1a4591cda2982343e0a16a1e0741f32a6240e3cac"),
        ("form/fr", "97c9f9889c71097785bdb4f1ebd019eef6d14eebd7a37c497109c95f19f0e2bf"),
        ("done/fr", "6a2b5976597425eb122f01b9bd5b50873ac91899d7f7d6c34a96e61381832744"),
        ("closed/fr", "3dc5230584f02df78441571d92f1a24fdffdef7404cbdc843a21c0a62525f856"),
        ("form_history/fr", "44db871bebcbe2d3798c3c49e5bd3e1c514ed7ea97c0ae147722a5da62b2b70b"),
        ("done_history/fr", "98c0eceb392bc7a53ec8121e6d31f8c94f30995daf794d57c08ba2a6251e6761"),
        ("msg_unknown/fr", "344a9e58918d714e3b178f676ed8a2122e6c7f81227dfa051b765b8fecb21d6c"),
        ("msg_gone_expired/fr", "14aca3df43cabb32a1020dba5e5a347c37efe0196b48bc1ded8942ce4f286514"),
        ("msg_gone_revoked/fr", "fd1ce4b06078135255d513a9fee4e1b4986fe388c68dff68f5d1e9657f3b5564"),
        ("msg_changed/fr", "ef8f28d800ff58eb217345c16c64ac61460c25ee5f61d5ff18b45d9ecd719b41"),
        ("msg_busy/fr", "7a0ec2d5c763c7c4646b33b221b6db9b7cf95578bb1e71cd8648f3e0b9ec5206"),
        ("msg_unavailable/fr", "14e1b4576516b5479cdd369ad27b238118c53fb05e17acea51ba54647c085ca3"),
        ("msg_err_name/fr", "44b24198b9f85214f1a90d6c8b834e7d096cdc350b089341028b4f3d7360fae2"),
        ("msg_forbidden/fr", "ae95927ee81afc5de863d3c08ca78f694eb4efd6de59b4d92b1771242d39d72a"),
        ("form/pl", "55964dde3173ca70c5146f415fdf6e8878bd2455f391f6071620f74c833e5f93"),
        ("done/pl", "1094928de630d5257f8d3aaa3ad188aa074c9d30aff11e12f3561da1a446d0f1"),
        ("closed/pl", "71c85b1a39f147daad6cef28d5501e246cfcd3041c1456a10a1c0ce425e11c65"),
        ("form_history/pl", "68eac7a254a29651317d91fb1d16377666ee9e5b475224e6d01a33ab026b9234"),
        ("done_history/pl", "c23e2da1c9da3ecf4bf9ef9480dbe04d934cba58561549b5867f8a6636cd74c1"),
        ("msg_unknown/pl", "83358ddaf731426628f2efa390824d8ff333e163d3359731eb140cd4718b2b6f"),
        ("msg_gone_expired/pl", "ec5a042e5fc9034037990974539dc1608ac0f85a157acf808810808196aa1554"),
        ("msg_gone_revoked/pl", "df8d2267865fc82a9ecffc5adbdf9c9a88d0f42cc9489fa044db2005eff5734e"),
        ("msg_changed/pl", "8cf8c9cebbc1fffa5a5989a2922568fc3b45a148d7cf651e6572a1ee107968b0"),
        ("msg_busy/pl", "7a73c87fb5c6b3bbaebd2660a1f9cc4e6814f6181a70d1aebf53e26e496e2d11"),
        ("msg_unavailable/pl", "679c00b82c2a7442554cfe5ba9834fa1871900570c836870c06f1f76d7a8157f"),
        ("msg_err_name/pl", "a89772c18ea8e0db5ec1bfa970524b7551e5a3c5de437496afd126b64d05ce1a"),
        ("msg_forbidden/pl", "7bad13652deee7638ac920d946834f4e74c51f8d89af2a977601906fd7be5aee"),
        ("form/fi", "32278ca1006a4cbb6c2628b1f2fc3dd482ec72fc843cc5ebb335be8baa052b71"),
        ("done/fi", "f30440399c1f304bae115136b046278cef2e35a5f112f9cd49f8d90391bb6c60"),
        ("closed/fi", "6db6dad0266304b454522927ed04fbb94b0c9367146ebfb65b4ee2ae69ec80e3"),
        ("form_history/fi", "d9149f5936dbf540967dc5cc98346c075faf0030c163e90ac4ad5f54788f8fa1"),
        ("done_history/fi", "15d798f42a7eeb0cc110a20483cc0c3c4913928d10a8dc56fcca6f7e779d2838"),
        ("msg_unknown/fi", "329c82ce5cb53b205c9b5ff6f750b2decb559c62b91d1020f69afb8c9026dd3e"),
        ("msg_gone_expired/fi", "f44932b4116dbf608c47492e0b52346c36eaca9de2604be97e39f91b8d6e5dd4"),
        ("msg_gone_revoked/fi", "32b5458fb64866b84b425e531474f6e84f77e8c0750a1b59e28c37fa4f167fa8"),
        ("msg_changed/fi", "cbc42c8b279cbfdeefef013a95ce0fef9da72730d44616b7367864a0193a0d91"),
        ("msg_busy/fi", "d8d6e672fe713982da3c9ba06d5ed34b2011cb6559adf31e4e3533c0e961059c"),
        ("msg_unavailable/fi", "5947e830f0d0710810a943ae54fe40b725c6067fa45e306eb599341f0d1685e6"),
        ("msg_err_name/fi", "e291f3f4be3ec6d231a93a862fd205e381103454e886b7adc249f76f011312f3"),
        ("msg_forbidden/fi", "b9206e44ad7ef6974c8cc895052904fac270e05fa9c3041adf314323f79bb837"),
        ("form/sv", "b2a91adcbfb6d6d6c2e1abc0e86e5f020a0839b21db6294198f2d97ecfbb9d33"),
        ("done/sv", "eafb8f385ce59443e9f5117097d29f5a87d4b9f6f6c5a05a8ce61ba71ad98907"),
        ("closed/sv", "66d6bea8f5acb0562716fa587332c61c3e7c3b32b5a176f2294fc0183d344bfb"),
        ("form_history/sv", "949aec6eff2a0ce4ca821432857bb91a63f5cd2e2cbfb87b90529436a541f931"),
        ("done_history/sv", "4189d23ac7654333a11db951d7a8f1f3ccfcef22bb93d675574f96bc34594087"),
        ("msg_unknown/sv", "91fc264c52034cc9ea17d14332a507fa891795f56f128878a99a0659ac73a447"),
        ("msg_gone_expired/sv", "beb2c64e230c8e69f15a3a298a7351ccfbc987232247b61a1416cb045cadd87d"),
        ("msg_gone_revoked/sv", "bc92d71f6035a1c2a13d4f23d6be2d0e95d90c7745537b5b492ab02af404541e"),
        ("msg_changed/sv", "0797098731124eb1d6113266fafc9a0c8c6ae78e77e46892b840779b73a8c487"),
        ("msg_busy/sv", "1a93aafc11a6577dc446bc13a59836f9457cdf1e0b95f8e44c0232afb6ffd777"),
        ("msg_unavailable/sv", "47a596b9a16b8dedddc8df22847361f6c54c27f2ee521f016b6fd1696819e0e2"),
        ("msg_err_name/sv", "3795016c2ac9ea049d3cf1aefcd3877bb9a3fb877d409fdc72ca884c5186f9df"),
        ("msg_forbidden/sv", "a1371a049e878f0ba55f311cfa01475ca759680a88546c11fc52d13313ae3eba"),
        ("form/es", "e5a6b373fa09a384a08c55ada5593262277e665caa34fcdaa9ea67f4f5f9f989"),
        ("done/es", "b73b576eb0cd45a69f838e0346fc0289f5e9caf6fc10be296eb6cf324e9f1660"),
        ("closed/es", "2edd31cb803477e617e990f47f7dad927491b263adaf91fab8aff12c5b0d3ea2"),
        ("form_history/es", "450b135424ddcb5f323089e2ed2d1b616452ae82716b3563c1d18bdceba374e7"),
        ("done_history/es", "9cb530b5d12794cea94d5170c3200036aaf27f18a15006b3bbd65faaa5a28d0e"),
        ("msg_unknown/es", "8f19d80bc2733308847012bb5eec8fe95a7f9d7b8a91b29233b2637668d15a14"),
        ("msg_gone_expired/es", "44c3533f68f40058bdf876abf8c69e262109f32aa361aa18021c356201d7ffd1"),
        ("msg_gone_revoked/es", "52950d2463ac7775bf91f72e378a186d0b5a2dd2700ccee4559b6dc50c66090a"),
        ("msg_changed/es", "22ec2e66b4b0f626e4b65c070c7c6f7b5f363e485e938cf2a8f7d7d1c0509417"),
        ("msg_busy/es", "21044c6536717fbf5fa3ac2f1e746ec4b4f0d704b2fd52f9b6478546e9b80e84"),
        ("msg_unavailable/es", "defd82bb91836f34222f0333017787b1363f584f4e40225b5804bb6f3c9ac076"),
        ("msg_err_name/es", "288bb64e475a47886250e5a0cc0bc98f33f3132c4eb6951dc50216b2f68afcf1"),
        ("msg_forbidden/es", "0a6ac8acbc00597d3b05ab68c80902352bb36d1a189e12723024da5bb01a3838"),
    ];

    // ---- With a brand (slice S1d) -----------------------------------------------------------------

    /// The artwork of the placeholder logo (a plain "M") in the canonical form the writer prints.
    const ARTWORK: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"#495670\"/><path d=\"M 16 48 L 16 16 L 24 16 L 32 30 L 40 16 L 48 16 L 48 48 L 41 48 L 41 29 L 32 44 L 23 29 L 23 48 Z\" fill=\"#ffffff\"/></svg>";

    /// A brand written through `brand::apply`, as `instance_brand.configure` writes it, and read back
    /// through a reader (the only way a `Shown` is made).
    fn branded(pairs: &[(&str, &str)]) -> std::sync::Arc<Shown> {
        use obop_commands::brand::{self, Action, Reader};
        let backend = obop_storage::open("memory:").unwrap();
        let reader = Reader::with_bound(backend.clone(), std::time::Duration::ZERO);
        let supplied = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        brand::apply(
            &*backend,
            Some(&reader),
            "operator",
            &Action::Configure(supplied),
            false,
        )
        .unwrap_or_else(|e| panic!("the brand was refused: {e}"));
        reader.current()
    }

    /// A row put into the store as it is, whatever it holds (a restore, an import, a direct edit),
    /// read back through a reader: a row written around the command.
    fn row_around(pairs: &[(&str, serde_json::Value)]) -> std::sync::Arc<Shown> {
        use obop_commands::brand::{self, Reader};
        use obop_storage::Op;
        let backend = obop_storage::open("memory:").unwrap();
        backend
            .apply_checked(&[Op::Put {
                model: brand::MODEL.into(),
                id: brand::ROW_ID.into(),
                fields: pairs
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
            }])
            .unwrap();
        Reader::with_bound(backend, std::time::Duration::ZERO).current()
    }

    fn mohv() -> std::sync::Arc<Shown> {
        branded(&[("name", "MOHV"), ("logo_svg", ARTWORK)])
    }

    /// The tab icon link of a brand with version `v`.
    fn icon_of(brand: &Shown) -> String {
        format!(
            "<link rel=\"icon\" href=\"/api/brand/logo.svg?v={}\" type=\"image/svg+xml\">",
            brand.v()
        )
    }

    /// What a person reads on a page: no style, no tag.
    fn visible(html: &str) -> String {
        let mut s = html.to_string();
        while let Some(a) = s.find("<style") {
            let b = s[a..].find("</style>").map_or(s.len(), |i| a + i + 8);
            s.replace_range(a..b, " ");
        }
        let (mut out, mut in_tag) = (String::new(), false);
        for c in s.chars() {
            match c {
                '<' => {
                    in_tag = true;
                    out.push(' ');
                }
                '>' if in_tag => in_tag = false,
                c if !in_tag => out.push(c),
                _ => {}
            }
        }
        out
    }

    /// With a brand set, every state of the page in every language shows the line and the tab icon
    /// and names no product anywhere, the policy is the policy of the page with no brand and
    /// `; img-src 'self'`, and the page is the page with no brand in everything else: the brand line,
    /// the icon link and the policy are the only three differences, so the texts, and with them the
    /// consent sentence that the consent id is hashed from, are the ones of every instance.
    #[test]
    fn with_a_brand_every_state_in_every_language_differs_by_the_line_the_icon_and_the_policy() {
        let brand = mohv();
        assert!(brand.is_set() && !brand.v().is_empty());
        let mut pages = 0;
        for lang in LANGS {
            let line = brand_line(&brand, lang);
            assert!(
                line.starts_with("<p class=\"brand\"><span aria-hidden=\"true\"><svg ")
                    && line.ends_with("</svg></span> MOHV</p>"),
                "{lang}: {line}"
            );
            let plain = every_page(lang, &unbranded());
            let with = every_page(lang, &brand);
            assert_eq!(plain.len(), 13);
            for ((kind, p), (kind2, b)) in plain.iter().zip(&with) {
                assert_eq!(kind, kind2);
                pages += 1;
                let expected = p
                    .html
                    .replacen("<p class=\"brand\">obop</p>", &line, 1)
                    .replacen(
                        "</title><style>",
                        &format!("</title>{}<style>", icon_of(&brand)),
                        1,
                    );
                assert_eq!(
                    b.html, expected,
                    "{lang}/{kind}: more than the line and the icon"
                );
                assert_eq!(
                    b.csp,
                    format!("{}; img-src 'self'", p.csp),
                    "{lang}/{kind}: the policy"
                );
                assert!(!p.csp.contains("img-src"), "{lang}/{kind}");
                // Nothing on the page names the product, in the text or in the markup.
                assert!(
                    !b.html.to_lowercase().contains("obop"),
                    "{lang}/{kind}: {}",
                    visible(&b.html)
                );
                assert!(visible(&b.html).contains("MOHV"), "{lang}/{kind}");
                // One icon link, one brand line.
                assert_eq!(b.html.matches("rel=\"icon\"").count(), 1, "{lang}/{kind}");
                assert_eq!(
                    b.html.matches("class=\"brand\"").count(),
                    1,
                    "{lang}/{kind}"
                );
                // Still no script and no style attribute, and the hash of the style sheet is the
                // same: the stylesheet is not edited.
                assert!(!b.html.contains("<script") && !b.html.contains(" style="));
                assert_eq!(p.html.contains(CSS), b.html.contains(CSS));
                assert!(b.html.contains(CSS), "{lang}/{kind}");
            }
        }
        assert_eq!(pages, 13 * 7);
    }

    /// The title of the language, then the English title, then the name: the fallback chain of the
    /// OAuth pages' brand line, on the signer's pages.
    #[test]
    fn the_line_is_the_title_of_the_language_then_the_english_title_then_the_name() {
        let brand = branded(&[
            ("name", "MOHV"),
            ("title_en", "MOHV Mileage"),
            ("title_sv", "MOHV Körjournal"),
        ]);
        let line_of = |lang: &str| {
            let html = message(lang, "closed", &brand).html;
            let a = html.find("<p class=\"brand\">").unwrap();
            let end = html[a..].find("</p>").unwrap() + a;
            html[a..end].to_string()
        };
        assert!(
            line_of("sv").ends_with("MOHV Körjournal"),
            "{}",
            line_of("sv")
        );
        assert!(line_of("de").ends_with("MOHV Mileage"), "{}", line_of("de"));
        assert!(line_of("en").ends_with("MOHV Mileage"), "{}", line_of("en"));
        let name_only = mohv();
        let html = message("fi", "closed", &name_only).html;
        assert!(html.contains("</span> MOHV</p>"), "{html}");
    }

    /// The text of the line beside the logo on a page.
    fn line_text(html: &str) -> String {
        let a = html.find("<p class=\"brand\">").unwrap();
        let end = html[a..].find("</p>").unwrap() + a;
        let line = &html[a..end];
        line[line.rfind("</span> ").unwrap() + "</span> ".len()..].to_string()
    }

    /// A brand with a title of its own in each of the seven languages shows that title, and no other,
    /// on every kind of page in that language: the message pages (the refusals), the page after
    /// signing and the form, with and without the list of the signatures so far. (A brand with the
    /// titles of three languages cannot tell the language of a page from the English fallback in the
    /// other four.)
    #[test]
    fn every_language_shows_its_own_title_on_the_message_the_done_and_the_form_page() {
        let titles: Vec<(String, String)> = LANGS
            .iter()
            .map(|l| {
                (
                    format!("title_{l}"),
                    format!("MOHV Title {}", l.to_uppercase()),
                )
            })
            .collect();
        let mut pairs: Vec<(&str, &str)> = vec![("name", "MOHV")];
        pairs.extend(titles.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        let brand = branded(&pairs);
        assert!(brand.fallbacks().is_empty(), "{:?}", brand.fallbacks());
        let history = pinned_history();
        let mut seen = 0;
        for lang in LANGS {
            let want = format!("MOHV Title {}", lang.to_uppercase());
            for (kind, page) in [
                ("message", message(lang, "closed", &brand)),
                (
                    "done",
                    done(lang, "2026-11-03T10:00:00.000000Z", None, &brand),
                ),
                (
                    "done_history",
                    done(lang, "2026-11-03T10:00:00.000000Z", Some(&history), &brand),
                ),
                ("form", form_in(lang, None, &brand)),
                ("form_history", form_in(lang, Some(&history), &brand)),
            ] {
                assert_eq!(line_text(&page.html), want, "{lang}/{kind}");
                seen += 1;
            }
        }
        assert_eq!(seen, 5 * 7);
        // An unknown language is shown in English, on the page and in the line alike.
        assert_eq!(
            line_text(&message("zz", "closed", &brand).html),
            "MOHV Title EN"
        );
    }

    /// A row written around the command: a name with every character that HTML needs escaped (the
    /// read check lets `&`, `"` and `'` through, and refuses `<` and `>`), a title with a tag in it
    /// (which falls back to the name). Neither reaches a page raw, in any state.
    #[test]
    fn a_hostile_name_and_title_written_around_the_command_are_escaped_on_every_page() {
        let evil = "Acme & \"Sons\" 'x'";
        let brand = row_around(&[
            ("name", serde_json::json!(evil)),
            ("title_sv", serde_json::json!("<script>alert(1)</script>")),
            ("title_de", serde_json::json!("Tit & \"el\"")),
            ("logo_svg", serde_json::json!(ARTWORK)),
        ]);
        assert!(brand.is_set());
        let escaped = esc(evil);
        assert_eq!(escaped, "Acme &amp; &quot;Sons&quot; &#39;x&#39;");
        for lang in LANGS {
            for (kind, p) in every_page(lang, &brand) {
                assert!(!p.html.contains("<script"), "{lang}/{kind}");
                assert!(!p.html.contains(evil), "{lang}/{kind}: the name raw");
                let shown = match lang {
                    "de" => esc("Tit & \"el\""),
                    _ => escaped.clone(),
                };
                assert!(
                    p.html.contains(&format!("</span> {shown}</p>")),
                    "{lang}/{kind}: {}",
                    brand_line(&brand, lang)
                );
            }
        }
        // A name that fails the read check altogether (a tag) is the default name, and the line
        // says so rather than showing the tag.
        let tag = row_around(&[("name", serde_json::json!("<b>Bold</b>"))]);
        for lang in LANGS {
            let html = message(lang, "closed", &tag).html;
            assert!(
                !html.contains("<b>") && !html.contains("&lt;b&gt;"),
                "{html}"
            );
            assert!(html.contains("</span> obop</p>") || html.contains("class=\"brand\">obop</p>"));
        }
    }

    /// The policy of a page differs from the policy of the page with no brand by `; img-src 'self'`
    /// at the end and by nothing else, and it is that on the very predicate the OAuth pages use: a
    /// brand that is not set (no row) leaves the policy and the head as they are.
    #[test]
    fn the_policy_gains_only_the_image_source_and_the_head_only_the_icon() {
        let plain = message("en", "closed", &unbranded());
        let with = message("en", "closed", &mohv());
        assert!(!plain.csp.contains("img-src"));
        assert!(with.csp.ends_with("; img-src 'self'"));
        assert_eq!(
            with.csp.strip_suffix("; img-src 'self'"),
            Some(plain.csp.as_str())
        );
        assert_eq!(plain.html.matches("<link").count(), 0);
        assert_eq!(with.html.matches("<link").count(), 1);
        // The hash of the style sheet is in both.
        let hash = b64std(&Sha256::digest(CSS.as_bytes()));
        assert!(plain.csp.contains(&format!("style-src 'sha256-{hash}'")));
        assert!(with.csp.contains(&format!("style-src 'sha256-{hash}'")));
        // The frame, object and form directives of the page are the ones it had.
        for d in [
            "object-src 'self'",
            "frame-src 'self'",
            "form-action 'self'",
            "frame-ancestors 'none'",
            "base-uri 'none'",
            "default-src 'none'",
        ] {
            assert!(with.csp.contains(d), "{d}");
        }
    }

    /// The texts of the page are the ones of the catalogue, and the catalogue names no product in any
    /// language: a brand has no word to replace, so it cannot change the consent id that eight of the
    /// texts (nine with the history) are hashed into. (The id of a signature that was made with a
    /// brand set is held at the door, in `crates/obop`.)
    #[test]
    fn the_texts_of_the_signer_name_no_product_in_any_language() {
        for lang in LANGS {
            for (key, _) in crate::sign_text::CATALOGUE {
                let text = txt::fill(lang, key, &[("title", "x"), ("name", "x"), ("hash", "x")]);
                assert!(
                    !text.to_lowercase().contains("obop"),
                    "{lang}/{key} names the product: {text}"
                );
            }
        }
    }
}
