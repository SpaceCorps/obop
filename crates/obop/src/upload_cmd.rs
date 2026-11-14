//! `obop upload FILE --field MODEL.FIELD [--record ID] [--content-type TYPE] [--url URL]` (D60):
//! a local file becomes an uploaded file or image of an instance, and with `--record` the value
//! of that field of one record. Until this command the bytes of a file could reach an instance
//! only over REST or through the back office (OR-4: every capability is reachable from the CLI).
//!
//! It has two ways to the instance and the same rules on both. Without `--url` it works like
//! every other command here: in process, on the database of `--db`, as the caller of the token,
//! through `BlobService::upload` (the type, the size and the groups of the field) and the
//! generated `update` command. With `--url` (or `OBOP_URL`) it is a client of a running instance:
//! `POST /api/blobs?field=MODEL.FIELD` and `POST /api/MODEL/update`, with the token as the bearer.
//! That is the first HTTP client mode of the CLI (docs/spec/cli.md 1.3 proposes it for every
//! command); nothing else uses it yet.
//!
//! The token never travels over plain HTTP, except to a loopback address.

use super::{Exit, Opts, dispatch_call, open_app, principal};
use obop_commands::{BlobError, Value, errors};
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::process::ExitCode;

pub struct Args<'a> {
    pub file: &'a str,
    /// `model.field`: the image or file field the upload is for.
    pub field: Option<&'a str>,
    /// The record whose field is set to the upload.
    pub record: Option<&'a str>,
    /// The type to declare; guessed from the file name when absent.
    pub content_type: Option<&'a str>,
    /// The address of a running instance (`--url` or `OBOP_URL`).
    pub url: Option<&'a str>,
}

/// Why the command stopped.
pub enum Refused {
    /// A line for stderr and the exit status.
    Line(String, u8),
    /// Already said on stderr by the function that failed (opening the database, the token).
    Said(ExitCode),
}

impl Refused {
    /// The line, for a test to read.
    #[cfg(test)]
    pub fn line(&self) -> &str {
        match self {
            Refused::Line(l, _) => l,
            Refused::Said(_) => "",
        }
    }
}

pub const USAGE: &str = "usage: obop upload FILE --field MODEL.FIELD [--record ID] [--content-type TYPE] [--url URL] [--token TOKEN]";

fn refused(line: impl Into<String>) -> Refused {
    Refused::Line(line.into(), 1)
}

pub fn run(o: &Opts, a: &Args) -> Exit {
    match upload(o, a) {
        Ok(answer) => {
            println!("{}", serde_json::to_string_pretty(&answer).unwrap());
            Ok(())
        }
        Err(Refused::Line(line, status)) => {
            eprintln!("{line}");
            Err(ExitCode::from(status))
        }
        Err(Refused::Said(code)) => Err(code),
    }
}

/// A name of the registry (a model or a field): lower case letters, digits and `_`. Checked
/// here because the two names become a path of the instance in the HTTP mode.
fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}

/// The content type to declare for a file name, by its extension: the types an upload accepts
/// (`obop_media::FILE_TYPES`). Empty for anything else, which leaves the decision to the check of
/// the bytes; a ZIP-based document needs its own type, so those are all here.
pub fn guess_content_type(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "zip" => "application/zip",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "odp" => "application/vnd.oasis.opendocument.presentation",
        _ => "",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(64);
    for b in Sha256::digest(bytes) {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// What the command sends, read and checked before anything is opened or contacted.
struct Upload<'a> {
    model: &'a str,
    field: &'a str,
    target: &'a str,
    record: Option<&'a str>,
    filename: String,
    content_type: String,
    bytes: Vec<u8>,
}

/// The upload as JSON (`id`, `content_type`, `size`, `sha256` of the local file, `filename`, and
/// `record` when it was attached), or the line and the exit status of the refusal.
pub fn upload(o: &Opts, a: &Args) -> Result<J, Refused> {
    let target = a.field.ok_or_else(|| Refused::Line(USAGE.to_string(), 2))?;
    let (model, field) = target
        .split_once('.')
        .filter(|(m, f)| is_name(m) && is_name(f))
        .ok_or_else(|| {
            refused(format!(
                "error: --field must name an image or file field as model.field, not {target:?}"
            ))
        })?;
    if a.record.is_some_and(|r| r.trim().is_empty()) {
        return Err(refused("error: --record needs the id of a record"));
    }
    let bytes =
        std::fs::read(a.file).map_err(|e| refused(format!("cannot read {}: {e}", a.file)))?;
    let filename = std::path::Path::new(a.file)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let content_type = a
        .content_type
        .map_or_else(|| guess_content_type(&filename), str::trim)
        .to_string();
    let up = Upload {
        model,
        field,
        target,
        record: a.record,
        filename,
        content_type,
        bytes,
    };
    let sha256 = sha256_hex(&up.bytes);
    let stored = match a.url {
        Some(url) => remote(o, url, &up)?,
        None => local(o, &up)?,
    };
    let mut answer = json!({
        "id": stored.id,
        "content_type": stored.content_type,
        "size": stored.size,
        "sha256": sha256,
        "filename": up.filename,
    });
    if let Some(record) = up.record {
        answer["record"] = json!({"model": up.model, "id": record, "field": up.field});
    }
    Ok(answer)
}

/// What the instance stored.
struct Stored {
    id: String,
    content_type: String,
    size: u64,
}

/// The words of a refused upload; the same as the REST answers of `obop-http`, plus the reason of
/// a storage failure, which the person at this command line may see.
fn blob_refusal(e: &BlobError) -> String {
    match e {
        BlobError::Bad(m) | BlobError::Unsupported(m) => m.clone(),
        BlobError::TooLarge(max) => format!("the file is larger than {} KiB", max / 1024),
        BlobError::NotFound => "not found".into(),
        BlobError::Forbidden => "you may not upload to this field".into(),
        BlobError::Rate(secs) => format!("too many uploads, try again in {secs} seconds"),
        BlobError::Storage(m) => format!("could not store the file: {m}"),
    }
}

/// An upload that was stored and then not attached: said in one line, so that nobody takes the
/// id on the line above for a finished job.
fn not_attached(id: &str, why: &str) -> String {
    format!(
        "error: the file is uploaded as {id} but the record was not changed: {why}. An upload that no record takes is removed after a day"
    )
}

/// In process, on the database of `--db`, as the caller of the token.
fn local(o: &Opts, up: &Upload) -> Result<Stored, Refused> {
    let app = open_app(o).map_err(Refused::Said)?;
    let p = principal(&app, o).map_err(Refused::Said)?;
    let svc = app
        .dispatcher
        .blob_service()
        .ok_or_else(|| refused("error: the file store is not available"))?;
    // One request, as the HTTP server opens for the route, so that a refusal of the store (D74)
    // is told from a refusal of the upload.
    let (uploaded, seen) =
        errors::requested(|| svc.upload(&p, up.target, &up.content_type, &up.filename, &up.bytes));
    let storage = |own_answer_failed: bool| {
        errors::storage_outcome(&seen, own_answer_failed)
            .map(|c| Refused::Line(format!("error: {}", c.cli), c.exit))
    };
    let info = match uploaded {
        Ok(info) => match storage(false) {
            Some(r) => return Err(r),
            None => info,
        },
        Err(e) => {
            return Err(
                storage(true).unwrap_or_else(|| refused(format!("error: {}", blob_refusal(&e))))
            );
        }
    };
    if let Some(record) = up.record {
        let input = Value::map([
            ("id", Value::text(record)),
            (up.field, Value::text(&info.id)),
        ]);
        dispatch_call(&app, &p, &format!("{}.update", up.model), &input).map_err(
            |(line, status)| {
                Refused::Line(
                    not_attached(&info.id, line.trim_start_matches("error: ")),
                    status,
                )
            },
        )?;
    }
    Ok(Stored {
        id: info.id,
        content_type: info.content_type,
        size: info.size,
    })
}

/// The address of an instance the token may be sent to: `https`, or `http` to a loopback host (a
/// local instance or a test). Returned without a trailing `/`.
pub fn instance_url(url: &str) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/');
    let lower = url.to_ascii_lowercase();
    let rest = if let Some(rest) = lower.strip_prefix("https://") {
        rest
    } else if let Some(rest) = lower.strip_prefix("http://") {
        // An IPv6 address is in brackets and has colons of its own.
        let host = match rest.find(']') {
            Some(end)
                if rest.starts_with('[')
                    && matches!(rest.as_bytes().get(end + 1), None | Some(b':' | b'/')) =>
            {
                &rest[..=end]
            }
            _ => rest.split(['/', ':', '?', '#']).next().unwrap_or(""),
        };
        if !matches!(host, "localhost" | "127.0.0.1" | "[::1]") {
            return Err(format!(
                "{url} is not https: the token is not sent over plain http, except to localhost"
            ));
        }
        rest
    } else {
        return Err(format!(
            "{url} is not the address of an instance: give --url (or OBOP_URL) as https://HOST"
        ));
    };
    if rest.is_empty() || rest.contains(['?', '#', '@', ' ']) {
        return Err(format!(
            "{url} is not the address of an instance: give the address alone, without a query, a fragment or a user name"
        ));
    }
    Ok(url.to_string())
}

/// The longest answer read from an instance: the JSON of an upload or of an updated record.
const MAX_ANSWER_BYTES: u64 = 4 * 1024 * 1024;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .http_status_as_error(false)
        // A redirect would carry the token and the file to another address.
        .max_redirects(0)
        .build()
        .into()
}

/// The status and the JSON of an answer; `Null` when the body is not JSON.
fn answer_of(mut resp: ureq::http::Response<ureq::Body>) -> (u16, J) {
    let status = resp.status().as_u16();
    let text = resp
        .body_mut()
        .with_config()
        .limit(MAX_ANSWER_BYTES)
        .read_to_string()
        .unwrap_or_default();
    (status, serde_json::from_str(&text).unwrap_or(J::Null))
}

/// What an instance said when it refused: the `error` text of its JSON, else the status alone.
fn said(status: u16, body: &J) -> String {
    let text = match &body["error"] {
        J::String(s) => s.clone(),
        J::Null => String::new(),
        other => other.to_string(),
    };
    match (status, text.is_empty()) {
        (401, _) => "the instance did not accept the token (401)".into(),
        (_, true) => format!("the instance answered {status}"),
        (_, false) => format!("{text} ({status})"),
    }
}

/// As a client of a running instance, with the token as the bearer.
fn remote(o: &Opts, url: &str, up: &Upload) -> Result<Stored, Refused> {
    let base = instance_url(url).map_err(|e| refused(format!("error: {e}")))?;
    let token = o.token.as_deref().filter(|t| !t.is_empty()).ok_or_else(|| {
        refused("no token: pass --token (or OBOP_TOKEN); the instance issues one with `obop agent-token` or POST /api/auth/agent-token")
    })?;
    let bearer = format!("Bearer {token}");
    let unreachable = |e: ureq::Error| refused(format!("error: cannot reach {base}: {e}"));
    let declared = if up.content_type.is_empty() {
        "application/octet-stream"
    } else {
        &up.content_type
    };
    let resp = agent()
        .post(format!("{base}/api/blobs"))
        .query("field", up.target)
        .query("filename", &up.filename)
        .header("User-Agent", "obop-cli")
        .header("Authorization", &bearer)
        .header("Content-Type", declared)
        .send(&up.bytes[..])
        .map_err(unreachable)?;
    let (status, body) = answer_of(resp);
    if status != 201 {
        return Err(refused(format!("error: {}", said(status, &body))));
    }
    let stored = Stored {
        id: body["id"].as_str().unwrap_or_default().to_string(),
        content_type: body["content_type"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        size: body["size"].as_u64().unwrap_or(0),
    };
    if stored.id.is_empty() {
        return Err(refused(
            "error: the instance answered 201 without the id of the upload",
        ));
    }
    if let Some(record) = up.record {
        let resp = agent()
            .post(format!("{base}/api/{}/update", up.model))
            .header("User-Agent", "obop-cli")
            .header("Authorization", &bearer)
            .header("Content-Type", "application/json")
            .send(json!({"id": record, up.field: stored.id}).to_string())
            .map_err(|e| {
                refused(not_attached(
                    &stored.id,
                    &format!("cannot reach {base}: {e}"),
                ))
            })?;
        let (status, body) = answer_of(resp);
        if status != 200 {
            return Err(refused(not_attached(&stored.id, &said(status, &body))));
        }
    }
    Ok(stored)
}
