//! Upload and download of images and files (D60).
//!
//! `POST /api/blobs?field=model.field` takes the bytes as the request body (a raw body with the
//! `Content-Type` of the file; there is no multipart parser to attack) and returns the blob id.
//! `GET /api/blobs/{id}` (optionally `?w=640`) sends it to a signed-in user who may read a
//! record that holds it. Both need a bearer token. The rules live in
//! `obop_commands::BlobService`; this module only speaks HTTP.
//!
//! Virus scanning: bytes pass `obop_storage::blob::Scanner` before they are stored. obop ships no
//! scanner and sends nothing anywhere; an operator who needs one implements the trait and installs
//! it with `Store::with_blobs`.

use super::{AuthRejection, HttpState, blocking};
use axum::{
    body::{Body, to_bytes},
    extract::{Path, Query, Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use obop_commands::{BlobError, Served, disposition};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

fn error(status: StatusCode, message: &str) -> Response {
    (status, axum::Json(json!({"error": message}))).into_response()
}

fn blob_error(e: BlobError) -> Response {
    match e {
        BlobError::Bad(m) => error(StatusCode::BAD_REQUEST, &m),
        BlobError::TooLarge(max) => error(
            StatusCode::PAYLOAD_TOO_LARGE,
            &format!("the file is larger than {} KiB", max / 1024),
        ),
        BlobError::Unsupported(m) => error(StatusCode::UNSUPPORTED_MEDIA_TYPE, &m),
        BlobError::NotFound => error(StatusCode::NOT_FOUND, "not found"),
        BlobError::Forbidden => error(StatusCode::FORBIDDEN, "you may not upload to this field"),
        BlobError::Rate(secs) => {
            let mut r = error(
                StatusCode::TOO_MANY_REQUESTS,
                "too many uploads, try again later",
            );
            if let Ok(v) = HeaderValue::from_str(&secs.to_string()) {
                r.headers_mut().insert(header::RETRY_AFTER, v);
            }
            r
        }
        BlobError::Storage(_) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not store the file",
        ),
    }
}

/// `POST /api/blobs`.
pub async fn upload(
    State(s): State<Arc<HttpState>>,
    Query(q): Query<HashMap<String, String>>,
    req: Request,
) -> Response {
    let (parts, body) = req.into_parts();
    let headers = parts.headers;
    let target = q.get("field").cloned().unwrap_or_default();
    let (s2, h2, t2) = (s.clone(), headers.clone(), target.clone());
    // Who is asking, and how much may they send? Decided before the body is read, so a refused
    // caller never makes the server buffer anything.
    let prep = blocking(move || {
        let (p, _) = s2.auth(&h2)?;
        let max = s2
            .dispatcher
            .blob_service()
            .map_or(0, |svc| svc.max_upload(&t2));
        Ok::<_, AuthRejection>((p, max))
    })
    .await;
    let (principal, max) = match prep {
        Ok(x) => x,
        Err(r) => return r.into_response(),
    };
    if max == 0 {
        return blob_error(BlobError::Bad(
            "field must name an image or file field as model.field".into(),
        ));
    }
    if headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|n| n > max)
    {
        return blob_error(BlobError::TooLarge(max));
    }
    let bytes = match to_bytes(body, max + 1).await {
        Ok(b) if b.len() <= max => b,
        _ => return blob_error(BlobError::TooLarge(max)),
    };
    let declared = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let filename = q.get("filename").cloned().unwrap_or_default();
    blocking(move || {
        let Some(svc) = s.dispatcher.blob_service() else {
            return error(StatusCode::NOT_FOUND, "not found");
        };
        match svc.upload(&principal, &target, &declared, &filename, &bytes) {
            Ok(info) => (
                StatusCode::CREATED,
                axum::Json(json!({
                    "id": info.id, "content_type": info.content_type, "size": info.size,
                    "width": info.width, "height": info.height,
                })),
            )
                .into_response(),
            Err(e) => blob_error(e),
        }
    })
    .await
}

/// Response headers every blob carries.
pub fn blob_response(served: Served, req_headers: &HeaderMap, cache: &str) -> Response {
    let not_modified = req_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',')
                .any(|t| t.trim().trim_start_matches("W/") == served.etag || t.trim() == "*")
        });
    let mut r = Response::builder().status(if not_modified {
        StatusCode::NOT_MODIFIED
    } else {
        StatusCode::OK
    });
    let mut set = |k: HeaderName, v: &str| {
        if let Ok(v) = HeaderValue::from_str(v) {
            r = std::mem::take(&mut r).header(k, v);
        }
    };
    set(header::ETAG, &served.etag);
    set(header::CACHE_CONTROL, cache);
    set(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    // A blob is a picture or a download, never a page: nothing in it may run or load anything.
    set(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; img-src data:; style-src 'unsafe-inline'; sandbox",
    );
    set(
        HeaderName::from_static("cross-origin-resource-policy"),
        "same-site",
    );
    if not_modified {
        return r.body(Body::empty()).expect("static response");
    }
    set(header::CONTENT_TYPE, &served.content_type);
    set(
        header::CONTENT_DISPOSITION,
        &disposition(served.attachment, &served.filename),
    );
    set(header::CONTENT_LENGTH, &served.bytes.len().to_string());
    r.body(Body::from(served.bytes)).expect("valid response")
}

/// `GET /api/blobs/{id}`.
pub async fn download(
    State(s): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    blocking(move || {
        let (p, _) = match s.auth(&headers) {
            Ok(x) => x,
            Err(r) => return r.into_response(),
        };
        let width = match q.get("w") {
            None => None,
            Some(w) => match w.parse::<u32>() {
                Ok(w) => Some(w),
                Err(_) => return blob_error(BlobError::NotFound),
            },
        };
        let Some(svc) = s.dispatcher.blob_service() else {
            return error(StatusCode::NOT_FOUND, "not found");
        };
        match svc.get(&p, &id, width) {
            // Content addressed, so the URL never changes meaning; `private` keeps shared caches
            // from handing it to someone else.
            Ok(served) => {
                let mut r = blob_response(served, &headers, "private, max-age=31536000, immutable");
                r.headers_mut()
                    .insert(header::VARY, HeaderValue::from_static("Authorization"));
                r
            }
            Err(e) => blob_error(e),
        }
    })
    .await
}
