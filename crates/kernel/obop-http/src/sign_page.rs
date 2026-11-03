//! The HTML of the signer's pages (D92). Server-rendered, no script, no `style` attribute, no
//! cookie: one stylesheet, allowed by its hash. The words come from the catalogue of
//! `sign_text.rs`; this file knows the markup and nothing about the store.
//!
//! The PDF is offered to the browser's own viewer in an `<object>` that falls back to the text
//! "open it in full size", and a visible link opens it in a new tab (a phone shows one page of an
//! embedded PDF at most, so on a coarse pointer the object is hidden and the link is the way to
//! read it). The page's policy names `frame-src 'self'`: Chromium's viewer is a nested frame and
//! `default-src 'none'` would otherwise leave the box empty (spike of 2026-11-03, docs/SIGN.md).

use crate::sign_text::{self as txt, text};
use obop_oauth::page::esc;
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

fn csp() -> String {
    let hash = b64std(&Sha256::digest(CSS.as_bytes()));
    format!(
        "default-src 'none'; style-src 'sha256-{hash}'; object-src 'self'; frame-src 'self'; child-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
    )
}

fn shell(lang: &str, title: &str, body: &str) -> Page {
    let html = format!(
        "<!doctype html>\n<html lang=\"{lang}\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<meta name=\"color-scheme\" content=\"light dark\"><meta name=\"referrer\" content=\"same-origin\">\
<meta name=\"robots\" content=\"noindex,nofollow\"><title>{title}</title><style>{CSS}</style></head>\
<body><main><p class=\"brand\">obop</p><div class=\"card\">{body}</div></main></body></html>\n",
        title = esc(title)
    );
    Page { html, csp: csp() }
}

/// A page with one message: the link is over, unknown, busy, or a post that did not come
/// from the page.
pub(crate) fn message(lang: &str, key: &str) -> Page {
    shell(
        lang,
        text(lang, "title"),
        &format!(
            "<h1>{}</h1><p>{}</p>",
            esc(text(lang, "title")),
            txt::html(lang, key, &[])
        ),
    )
}

/// The page after signing, and for a link that was used.
pub(crate) fn done(lang: &str, at: &str) -> Page {
    shell(
        lang,
        text(lang, "done_title"),
        &format!(
            "<h1>{}</h1><p>{}</p>",
            txt::html(lang, "done_title", &[]),
            txt::html(lang, "done_body", &[("time", at)])
        ),
    )
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
    /// `err_name` or `err_agree` when the last try was refused.
    pub error: Option<&'a str>,
}

pub(crate) fn form(lang: &str, f: &Form<'_>) -> Page {
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
<p class=\"muted\">{fingerprint}</p>{error}\
<form method=\"post\" action=\"{base}\">\
<input type=\"hidden\" name=\"doc\" value=\"{hash}\"><input type=\"hidden\" name=\"lang\" value=\"{lang}\">\
<label for=\"name\">{name_label}</label>\
<input id=\"name\" name=\"name\" type=\"text\" maxlength=\"120\" autocomplete=\"name\" required value=\"{typed}\">\
<p class=\"muted\">{name_hint}</p>\
<div class=\"opt\"><input id=\"agree\" type=\"checkbox\" name=\"agree\" value=\"1\" required>\
<label for=\"agree\">{consent}</label></div>\
<p class=\"notice\">{records}</p>\
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
    );
    shell(
        lang,
        &txt::fill(lang, "request", &[("title", f.title)]),
        &body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_oauth::page::LANGS;

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
            },
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

    #[test]
    fn a_foreign_post_and_a_closed_request_show_their_own_texts_in_every_language() {
        for lang in LANGS {
            for key in ["forbidden", "closed"] {
                let page = message(lang, key);
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

    /// The part of a source file before its tests.
    fn code(src: &str) -> &str {
        src.split("#[cfg(test)]").next().unwrap()
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
    fn every_key_the_door_and_its_pages_ask_for_is_in_the_catalogue() {
        // A key that is only in the code reads as an empty paragraph, in every language. This
        // reads the two files that ask for words and checks each literal key against the catalogue.
        let sources = [
            code(include_str!("sign.rs")),
            code(include_str!("sign_page.rs")),
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
        for src in sources {
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
}
