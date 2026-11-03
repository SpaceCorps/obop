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

/// A page with one message: the link is over, unknown, busy.
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

/// The message of an answer that has no catalogue key (the refusal of a cross-site post).
pub(crate) fn plain_message(lang: &str, english: &str) -> Page {
    shell(
        lang,
        text(lang, "title"),
        &format!("<h1>{}</h1><p>{}</p>", esc(text(lang, "title")), esc(english)),
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
<label class=\"opt\"><input type=\"checkbox\" name=\"agree\" value=\"1\" required><span>{consent}</span></label>\
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
    shell(lang, &txt::fill(lang, "request", &[("title", f.title)]), &body)
}
