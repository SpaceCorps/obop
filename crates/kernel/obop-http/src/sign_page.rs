//! The HTML of the signer's pages (D92). Server-rendered, no script, no `style` attribute, no
//! cookie: one stylesheet, allowed by its hash. The words come from the catalogue of
//! `sign_text.rs`; this file knows the markup and nothing about the store.
//!
//! The list of the signatures so far on the file (`History`) is shown only when the request asks
//! for it; a page without it is the page it always was, byte for byte, and the stylesheet is not
//! touched for it (it would change the hash in the policy and so every page).
//!
//! The PDF is offered to the browser's own viewer in an `<object>` that falls back to the text
//! "open it in full size", and a visible link opens it in a new tab (a phone shows one page of an
//! embedded PDF at most, so on a coarse pointer the object is hidden and the link is the way to
//! read it). The page's policy names `frame-src 'self'`: Chromium's viewer is a nested frame and
//! `default-src 'none'` would otherwise leave the box empty (spike of 2026-11-03, docs/SIGN.md).

use crate::sign_text::{self as txt, text};
use obop_commands::sign::History;
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

/// The page after signing, and for a link that was used. `history` is the list of the signatures
/// so far when the request asks for it, with the signer's own entry marked.
pub(crate) fn done(lang: &str, at: &str, history: Option<&History>) -> Page {
    shell(
        lang,
        text(lang, "done_title"),
        &format!(
            "<h1>{}</h1><p>{}</p>{}",
            txt::html(lang, "done_title", &[]),
            txt::html(lang, "done_body", &[("time", at)]),
            history.map_or_else(String::new, |h| history_block(lang, h))
        ),
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
                history: None,
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
                sha(&done(lang, "2026-11-03T10:00:00.000000Z", None).html),
                done_hash,
                "{lang}: the page after signing"
            );
            assert_eq!(
                sha(&message(lang, "closed").html),
                closed_hash,
                "{lang}: a refusal"
            );
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
        let page = done("en", "2026-11-03T10:00:01.000000Z", Some(&h)).html;
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
        )
        .html;
        assert!(none.contains("Nobody has signed this file yet."));
        assert!(!none.contains("<ul>") && !none.contains("Only the earliest"));
        // The page after signing is the old one plus the block, in every language.
        for lang in LANGS {
            let plain = done(lang, "2026-11-03T10:00:01.000000Z", None).html;
            let with = done(lang, "2026-11-03T10:00:01.000000Z", Some(&h)).html;
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
}
