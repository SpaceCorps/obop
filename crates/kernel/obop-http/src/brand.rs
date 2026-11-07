//! The logo route, `GET /api/brand/logo.svg` (docs/spec/branding.md, sections 4 and 9), and the
//! one way a handler of this crate asks the reader of the instance brand.
//!
//! **The route.** Public like `/api/openapi.json`; it takes no path or id and a query string is
//! ignored, so it is not a file server. It answers the artwork as the reader re-serialised it, or
//! the generated tile, and nothing else, with the fixed headers of `obop_brand::logo` and an `ETag`
//! that is the version `v` of what is served, quoted (a matching `If-None-Match` is a 304 that
//! carries the `Content-Length` of the 200). It does not read the store itself: it asks the reader
//! of the dispatcher, whose answer is the one read check every surface shares, so a stored artwork
//! that fails the check is the tile here too.
//!
//! **No brand set.** The record does not exist, and for `GET` and `HEAD` the path answers as an
//! unknown one-segment path of the API does (`/api/nothing`): the 404 that the layout the route is
//! mounted in gives, status, headers and body (`api_not_found` for the API alone and for the back
//! office build next to it, the JSON 404 of the public site's router). The layout passes its own
//! answer in. So the answer to a `GET` does not tell a scanner whether a brand is set. It does not
//! hide that the route exists, and no test or comment may say it does: a path of the same shape,
//! `/api/zzz/logo.svg`, is matched by `/api/{model}/{command}`, which takes `POST` only, and answers
//! a 405 with `Allow: POST`, while this path answers every method but `GET` and `HEAD` (a `POST`,
//! an `OPTIONS` that no CORS layer takes) with a 405 and `Allow: GET,HEAD`; and while the store
//! cannot be read the path answers the 503 of D74 where an unknown path answers 404.
//!
//! **The reader is asked through [`current`]**, never on an async worker: the PostgreSQL client
//! panics there (`blocking`). A call that finds the cell fresh takes a standard mutex for a moment
//! and never touches the store. A store failure is the fault the request met: it is answered with
//! the 503 of D74 by `ops::storage_faults`, whatever this handler would have sent, exactly as for
//! every other request that meets one.

use super::{HttpState, blocking};
use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::Response,
    routing::get,
};
use obop_brand::logo::{LOGO_HEADERS, LOGO_PATH, logo_etag};
use obop_commands::brand::{Reader, Shown};
use std::sync::Arc;

/// The answer of a layout to a path of the API that is no route: what the logo route sends when no
/// brand is set.
pub(crate) type NotFound = fn() -> Response;

/// The brand for display, asked through the blocking pool: the reader's cache-hit path takes a
/// standard mutex only, and the one refill that reads the store runs on a blocking thread, where
/// the store's client may run. Every handler of this crate that shows the brand asks it this way.
pub(crate) async fn current(reader: Arc<Reader>) -> Arc<Shown> {
    blocking(move || reader.current()).await
}

/// The route, for every layout: `api_routes` merges it, and each layout says what a path of the API
/// that is no route answers.
pub(crate) fn routes(not_found: NotFound) -> axum::Router<Arc<HttpState>> {
    axum::Router::new().route(
        LOGO_PATH,
        get(move |State(s): State<Arc<HttpState>>, h: HeaderMap| logo(s, h, not_found)),
    )
}

async fn logo(s: Arc<HttpState>, h: HeaderMap, not_found: NotFound) -> Response {
    let shown = current(s.dispatcher.brand()).await;
    let Some(logo) = shown.logo() else {
        return not_found();
    };
    let etag = logo_etag(logo.v());
    let same = if_none_match(&h, &etag);
    let mut r = Response::builder().status(if same {
        StatusCode::NOT_MODIFIED
    } else {
        StatusCode::OK
    });
    let mut set = |name: HeaderName, value: &str| {
        if let Ok(value) = HeaderValue::from_str(value) {
            r = std::mem::take(&mut r).header(name, value);
        }
    };
    set(header::ETAG, &etag);
    for (name, value) in LOGO_HEADERS {
        let Ok(name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        // A 304 carries the validators and the headers a cache may update, not the representation
        // metadata: the `Content-Type` is the one of the answer it validates.
        if same && name == header::CONTENT_TYPE {
            continue;
        }
        set(name, value);
    }
    // The length of the 200, also on a 304: RFC 9110 section 8.6 allows it there and forbids any
    // other value. Left unset, axum sets `Content-Length: 0` and the server layer sends it with a
    // `HEAD` 304 (measured with curl on the real binary); it sends none with a `GET` 304, which is
    // as valid.
    set(header::CONTENT_LENGTH, &logo.served().len().to_string());
    if same {
        return r.body(Body::empty()).expect("static response");
    }
    r.body(Body::from(logo.served().to_string()))
        .expect("valid response")
}

/// Whether `If-None-Match` matches `etag`: `*`, or any entity-tag of the list under the weak
/// comparison of RFC 9110 (a `W/` prefix is ignored on both sides), over all the header lines.
fn if_none_match(h: &HeaderMap, etag: &str) -> bool {
    h.get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .any(|t| t == "*" || t.strip_prefix("W/").unwrap_or(t) == etag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(lines: &[&str]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for l in lines {
            h.append(header::IF_NONE_MATCH, HeaderValue::from_str(l).unwrap());
        }
        h
    }

    #[test]
    fn the_validator_is_matched_the_weak_way_in_a_list_on_every_line() {
        let tag = "\"0123456789abcdef\"";
        for (lines, want) in [
            (vec![], false),
            (vec!["\"0123456789abcdef\""], true),
            (vec!["W/\"0123456789abcdef\""], true),
            (vec!["\"other\", \"0123456789abcdef\""], true),
            (vec!["\"other\",W/\"0123456789abcdef\" , \"x\""], true),
            (vec!["\"other\"", "\"0123456789abcdef\""], true),
            (vec!["*"], true),
            (vec!["\"other\", *"], true),
            (vec!["\"other\""], false),
            (vec!["0123456789abcdef"], false),
            (vec!["w/\"0123456789abcdef\""], false),
            (vec!["\"0123456789abcdef"], false),
            (vec!["\"0123456789abcdef\"x"], false),
            (vec![""], false),
            (vec!["W/"], false),
            // `*` is the whole entry, not a character in it; a prefix is taken off once.
            (vec!["\"*\""], false),
            (vec!["a*"], false),
            (vec!["**"], false),
            (vec!["W/*"], false),
            (vec!["W/W/\"0123456789abcdef\""], false),
            (vec!["x\"0123456789abcdef\""], false),
        ] {
            assert_eq!(if_none_match(&headers(&lines), tag), want, "{lines:?}");
        }
    }

    /// A header value that is not text (a byte above 0x7f is legal in a header) is no validator
    /// and is skipped, whatever else is on the line or on the other lines; it never panics.
    #[test]
    fn a_header_value_that_is_not_text_is_skipped_and_never_a_panic() {
        let tag = "\"0123456789abcdef\"";
        let opaque = HeaderValue::from_bytes(b"\xff\xfe").unwrap();
        assert!(
            opaque.to_str().is_err(),
            "the case is not text to begin with"
        );
        let mut h = HeaderMap::new();
        h.append(header::IF_NONE_MATCH, opaque.clone());
        assert!(!if_none_match(&h, tag));
        // The line before and the line after it still count.
        h.append(header::IF_NONE_MATCH, HeaderValue::from_str(tag).unwrap());
        assert!(if_none_match(&h, tag));
        let mut h = HeaderMap::new();
        h.append(header::IF_NONE_MATCH, HeaderValue::from_str(tag).unwrap());
        h.append(header::IF_NONE_MATCH, opaque);
        assert!(if_none_match(&h, tag));
    }
}
