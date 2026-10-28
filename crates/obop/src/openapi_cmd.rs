//! `obop openapi [--out FILE]`: the OpenAPI 3.1 document of an instance, written without a server
//! and without a database (D31, D15). The shared TypeScript client (`web/packages/client`) is
//! generated from this document, so the output is made to be byte for byte the same on every run
//! and every platform: one in-memory app with every linked module, the caller is an account that
//! holds every permission group (so the document lists every command of every model that has a
//! `list` command: a model without one, such as `setting` or `instance_update`, has no operation
//! here, exactly as in the document an instance serves), the base URL is a fixed placeholder, keys
//! are sorted, nothing carries a time or a host name, and the text ends with one newline.

use super::{Exit, fail};
use crate::app::App;
use obop_auth::Auth;
use obop_commands::{Authenticator, Principal, Surface};
use serde_json::Value as J;
use std::sync::Arc;

/// The address that `servers` and `x-mcp` name in the exported document. A placeholder (the
/// default address of `obop serve`): a client takes the real base URL of its instance at run time.
pub const BASE_URL: &str = "http://localhost:8080";

/// The same value with the keys of every object in sorted order, whatever features `serde_json`
/// was built with. Arrays keep their order: the document builds them in a fixed order.
fn sorted(v: J) -> J {
    match v {
        J::Object(map) => {
            let mut entries: Vec<(String, J)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            J::Object(entries.into_iter().map(|(k, v)| (k, sorted(v))).collect())
        }
        J::Array(items) => J::Array(items.into_iter().map(sorted).collect()),
        other => other,
    }
}

/// The document as the value an instance serves at `/api/openapi.json` to an account that holds
/// every group.
pub fn value() -> J {
    let app = App::new();
    // The account commands (`user_account`, D69) exist on an instance with real sign-in, as served.
    let auth: Arc<dyn Authenticator> = Arc::new(Auth::new(app.store.backend()));
    app.register_accounts(auth);
    let groups = app.all_groups();
    let groups: Vec<&str> = groups.iter().map(String::as_str).collect();
    let caller = Principal::user("openapi-export", &groups);
    let modules = app.dispatcher.meta(Surface::Rest, &caller);
    let mut doc = obop_http::openapi::instance(BASE_URL, &modules);
    obop_http::openapi::describe_oauth(&mut doc, BASE_URL);
    doc
}

/// The exported text: two-space indent, sorted keys, LF line ends, one trailing newline.
pub fn document() -> String {
    let mut text = serde_json::to_string_pretty(&sorted(value())).expect("a JSON value serialises");
    text.push('\n');
    text
}

/// `obop openapi [--out FILE]`: print the document, or write it to FILE.
pub fn run(out: Option<&str>) -> Exit {
    let text = document();
    match out {
        Some(path) => std::fs::write(path, text.as_bytes())
            .map_err(|e| fail(&format!("cannot write {path}: {e}"))),
        None => {
            use std::io::Write;
            std::io::stdout()
                .lock()
                .write_all(text.as_bytes())
                .map_err(|e| fail(&format!("cannot write the document: {e}")))
        }
    }
}
