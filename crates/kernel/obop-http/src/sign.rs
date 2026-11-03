//! The HTTP side of the signer's door (D92): `GET` and `POST` `/sign/{request}/{token}` and
//! `GET` `/sign/{request}/{token}/document.pdf`. The rules and the state are in
//! `obop_commands::sign`; this module carries requests, sets the headers, and checks where a
//! browser post came from.
//!
//! No cookie is set or read, there is no script, and a `GET` never changes anything (a mail
//! scanner or a link preview that opens the link is harmless). Every answer that is not the PDF
//! has the headers of the other pages of the server (`harden`) plus `noindex`; the PDF has its
//! own set (`inline_pdf_response`), because `X-Frame-Options: DENY` would stop the page from
//! showing it. The address comes from `client_ip`, which trusts `X-Forwarded-For` only with
//! `--trust-proxy`; the log entry of a signature also holds the peer and the raw header.

use super::oauth::{client_ip, harden, pair, parse_pairs, same_origin, see_other};
use super::sign_page::{self, Page};
use super::sign_text as txt;
use super::{HttpState, blocking};
use axum::{
    body::Bytes,
    extract::{ConnectInfo, Path, State},
    http::{HeaderName, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use obop_commands::sign::{Client, Done, DoorError, Gone, Invalid, SignForm, View, ViewData};
use std::net::SocketAddr;
use std::sync::Arc;

type Shared = State<Arc<HttpState>>;

/// Size of the body of the signer's post.
pub(crate) const MAX_BODY: usize = 16 * 1024;

fn page_response(status: StatusCode, p: Page) -> Response {
    let mut r = (status, p.html).into_response();
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    if let Ok(v) = HeaderValue::from_str(&p.csp) {
        h.insert(header::CONTENT_SECURITY_POLICY, v);
    }
    h.insert(
        HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow"),
    );
    harden(&mut r);
    r
}

fn lang_for(s: &HttpState, parts: &Parts, form_lang: Option<&str>) -> &'static str {
    let param = form_lang.map(str::to_string).or_else(|| {
        parts
            .uri
            .query()
            .and_then(parse_pairs)
            .and_then(|q| pair(&q, "lang").map(str::to_string))
    });
    txt::choose_language(
        param.as_deref(),
        parts
            .headers
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok()),
        s.default_language(),
    )
}

fn client(s: &HttpState, parts: &Parts) -> Client {
    let cut = |v: Option<&HeaderValue>, n: usize| {
        v.and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .chars()
            .take(n)
            .collect::<String>()
    };
    Client {
        addr: client_ip(s, parts),
        peer: parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip().to_string())
            .unwrap_or_default(),
        forwarded: cut(parts.headers.get("x-forwarded-for"), 200),
        user_agent: cut(parts.headers.get(header::USER_AGENT), 400),
    }
}

/// The answer for everything the door refuses.
fn refusal(e: &DoorError, lang: &str) -> Response {
    let (status, key) = match e {
        DoorError::NotFound => (StatusCode::NOT_FOUND, "unknown"),
        DoorError::Gone(Gone::Expired) => (StatusCode::GONE, "gone_expired"),
        DoorError::Gone(Gone::Revoked | Gone::Closed) => (StatusCode::GONE, "gone_revoked"),
        DoorError::Changed => (StatusCode::CONFLICT, "changed"),
        DoorError::Busy(_) => (StatusCode::TOO_MANY_REQUESTS, "busy"),
        DoorError::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        // The form is shown again by the caller; this is only the fallback.
        DoorError::Invalid(_) => (StatusCode::BAD_REQUEST, "err_name"),
        DoorError::Refused => (StatusCode::CONFLICT, "gone_revoked"),
    };
    let mut r = page_response(status, sign_page::message(lang, key));
    let wait = match e {
        DoorError::Busy(n) => Some(*n),
        DoorError::Unavailable => Some(30),
        _ => None,
    };
    if let Some(w) = wait
        && let Ok(v) = HeaderValue::from_str(&w.to_string())
    {
        r.headers_mut().insert(header::RETRY_AFTER, v);
    }
    r
}

struct FormView<'a> {
    lang: &'a str,
    rid: &'a str,
    tok: &'a str,
    data: &'a ViewData,
    typed: &'a str,
    error: Option<&'a str>,
    status: StatusCode,
}

fn form_page(v: &FormView<'_>) -> Response {
    page_response(
        v.status,
        sign_page::form(
            v.lang,
            &sign_page::Form {
                rid: v.rid,
                token: v.tok,
                title: &v.data.title,
                message: &v.data.message,
                signer_name: &v.data.signer_name,
                document_title: &v.data.document_title,
                document_hash: &v.data.document_hash,
                typed: v.typed,
                error: v.error,
            },
        ),
    )
}

/// `GET /sign/{request}/{token}`.
pub(crate) async fn page(
    State(s): Shared,
    Path((rid, tok)): Path<(String, String)>,
    parts: Parts,
) -> Response {
    blocking(move || {
        let lang = lang_for(&s, &parts, None);
        let Some(svc) = s.dispatcher.sign_service() else {
            return refusal(&DoorError::NotFound, lang);
        };
        match svc.view(&rid, &tok, &client(&s, &parts)) {
            Ok(View::Form(d)) => form_page(&FormView {
                lang,
                rid: &rid,
                tok: &tok,
                data: &d,
                typed: "",
                error: None,
                status: StatusCode::OK,
            }),
            Ok(View::Signed { at }) => page_response(StatusCode::OK, sign_page::done(lang, &at)),
            Err(e) => refusal(&e, lang),
        }
    })
    .await
}

/// `POST /sign/{request}/{token}`.
pub(crate) async fn submit(
    State(s): Shared,
    Path((rid, tok)): Path<(String, String)>,
    parts: Parts,
    body: Bytes,
) -> Response {
    blocking(move || {
        let mut lang = lang_for(&s, &parts, None);
        let Some(svc) = s.dispatcher.sign_service() else {
            return refusal(&DoorError::NotFound, lang);
        };
        let base = s.mcp.base_url(&parts.headers);
        if !same_origin(&parts.headers, &base) {
            // Where a post came from is the browser's word; a proxy that hides the scheme makes
            // every post look foreign (docs/SIGN.md: set OBOP_PUBLIC_URL). The log says so.
            eprintln!("sign: a post was refused because its origin is not {base}");
            return page_response(
                StatusCode::FORBIDDEN,
                sign_page::plain_message(
                    lang,
                    "This request could not be checked. Open the link again and sign from that page.",
                ),
            );
        }
        let ct = parts
            .headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let pairs = if ct.starts_with("application/x-www-form-urlencoded") {
            std::str::from_utf8(&body).ok().and_then(parse_pairs)
        } else {
            None
        };
        let Some(pairs) = pairs else {
            return page_response(
                StatusCode::BAD_REQUEST,
                sign_page::message(lang, "err_name"),
            );
        };
        lang = lang_for(&s, &parts, pair(&pairs, "lang"));
        let name = pair(&pairs, "name").unwrap_or("").to_string();
        let form = SignForm {
            name: name.clone(),
            agree: pair(&pairs, "agree").is_some_and(|v| !v.is_empty()),
            doc: pair(&pairs, "doc").unwrap_or("").to_string(),
            consent_version: txt::consent_version_for(lang),
        };
        let c = client(&s, &parts);
        match svc.submit(&rid, &tok, &form, &c) {
            // A relative address, so that the redirect does not depend on the derived base.
            Ok(Done { .. }) => see_other(&format!("/sign/{rid}/{tok}")),
            Err(DoorError::Invalid(why)) => {
                // The form again, with what was typed and what is wrong.
                match svc.view(&rid, &tok, &c) {
                    Ok(View::Form(d)) => form_page(&FormView {
                        lang,
                        rid: &rid,
                        tok: &tok,
                        data: &d,
                        typed: &name,
                        error: Some(match why {
                            Invalid::Name => "err_name",
                            Invalid::Agree => "err_agree",
                        }),
                        status: StatusCode::BAD_REQUEST,
                    }),
                    Ok(View::Signed { at }) => {
                        page_response(StatusCode::OK, sign_page::done(lang, &at))
                    }
                    Err(e) => refusal(&e, lang),
                }
            }
            Err(e) => refusal(&e, lang),
        }
    })
    .await
}

/// `GET /sign/{request}/{token}/document.pdf`: the held copy of this request, only while its
/// link works.
pub(crate) async fn document(
    State(s): Shared,
    Path((rid, tok)): Path<(String, String)>,
    parts: Parts,
) -> Response {
    blocking(move || {
        let lang = lang_for(&s, &parts, None);
        let Some(svc) = s.dispatcher.sign_service() else {
            return refusal(&DoorError::NotFound, lang);
        };
        match svc.pdf(&rid, &tok, &client(&s, &parts)) {
            Ok(served) => {
                super::blobs::inline_pdf_response(served, &parts.headers, "private, no-store")
            }
            Err(e) => refusal(&e, lang),
        }
    })
    .await
}
