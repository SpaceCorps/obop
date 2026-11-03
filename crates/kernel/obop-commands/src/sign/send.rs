//! Sending a request: `send` keeps its old contract for a request made from a template, and
//! `send_document` is the send of a request about a Document in Documents.
//!
//! `send_document` is its own command so that connections subscribed to `sign_request.send` (a
//! DocuSeal connection) do not fire for it: a document request has nothing to hand to them.

use super::*;
use crate::engine::Actor;
use crate::engine::access::Gate;
use crate::engine::unit::Plain;
use crate::Value;
use obop_storage::Op;
use obop_storage::blob::Owner;

pub(super) fn install(d: &Dispatcher, svc: &Arc<SignService>) {
    let s = svc.clone();
    d.around(
        "sign_request.send",
        Box::new(move |_, input, inner| {
            s.check_template_send(input)?;
            inner(input)
        }),
    );
    let s = svc.clone();
    d.around(
        "sign_request.send_document",
        Box::new(move |p, input, inner| s.send_document(p, input, inner)),
    );
}

/// The person who approved an agent's proposal, when the call carries the stamp of one
/// (`stamp_approver`): the dispatcher strips these keys from what a client sends.
pub(super) fn approver_actor(input: &Value) -> Option<Actor> {
    let Value::Map(m) = input else {
        return None;
    };
    let prefix = format!("{}_approver", crate::query::ACTOR_PREFIX);
    let Some(Value::Text(id)) = m.get(&prefix) else {
        return None;
    };
    let groups = match m.get(&format!("{prefix}_groups")) {
        Some(Value::List(l)) => l
            .iter()
            .filter_map(|x| match x {
                Value::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect(),
        _ => Default::default(),
    };
    Some(Actor {
        id: id.clone(),
        admin: m.get(&format!("{prefix}_admin")) == Some(&Value::Bool(true)),
        groups,
        caller: id.clone(),
        agent: false,
    })
}

impl SignService {
    /// The Document `doc_id` as `p` may see it: the caller reads Documents and the row rules of
    /// the Document let them see this one, and so does the approver when a person approved an
    /// agent's proposal. A Document that is not visible answers like one that does not exist.
    pub(super) fn visible_document(
        &self,
        p: &Principal,
        input: &Value,
        doc_id: &str,
    ) -> Result<Fields, String> {
        let miss = || format!("there is no documents_document {doc_id}");
        let models = self.models.read().unwrap();
        let def = models
            .get("documents_document")
            .map(|m| m.def.clone())
            .ok_or_else(miss)?;
        let lookup = |n: &str| models.get(n).map(|m| m.def.clone());
        let reads = Plain(&*self.store.backend);
        let load = |a: &Actor| -> Option<Fields> {
            if !a.holds(def.read_group) {
                return None;
            }
            Gate::new(&reads, &lookup, a).load(&def, doc_id).ok()
        };
        let rec = load(&Actor::of(p)).ok_or_else(miss)?;
        if let Some(approver) = approver_actor(input)
            && load(&approver).is_none()
        {
            return Err(miss());
        }
        Ok(rec)
    }

    /// The held copy or the Document's file as bytes, checked to be what its id says and a PDF
    /// of an acceptable size. `blob` is the id (the SHA-256) it must have.
    pub(super) fn checked_pdf(&self, blob: &str) -> Result<Vec<u8>, String> {
        let blobs = &self.store.blobs;
        let info = blobs
            .info(blob)?
            .ok_or_else(|| "the file is not in the store".to_string())?;
        if info.content_type != "application/pdf" {
            return Err("only a PDF can be sent for signature".into());
        }
        if info.size > MAX_PDF {
            return Err("the PDF is larger than 25 MiB".into());
        }
        let bytes = blobs
            .read_all(blob, MAX_PDF)?
            .ok_or_else(|| "the file is not in the store".to_string())?;
        if token::sha256_hex(&bytes) != blob {
            return Err("the stored file does not match its hash".into());
        }
        Ok(bytes)
    }

    /// `send` keeps the contract it had: a request made from a template, with a signer address.
    /// A request about a document is sent with `send_document`.
    fn check_template_send(&self, input: &Value) -> Result<(), String> {
        let Ok(id) = id_of(input) else {
            return Ok(());
        };
        let Some(req) = self.get("sign_request", &id)? else {
            return Ok(());
        };
        let (doc, tpl, email) = (
            text(&req, "document_id"),
            text(&req, "template_id"),
            text(&req, "signer_email"),
        );
        match (doc.is_empty(), tpl.is_empty()) {
            (false, true) => Err(
                "sign_request: this request is about a document; send it with send_document".into(),
            ),
            (true, true) => Err("sign_request: a request needs a template or a document".into()),
            (_, false) if email.is_empty() => Err(
                "sign_request.signer_email is required to send a request made from a template"
                    .into(),
            ),
            _ => Ok(()),
        }
    }

    fn send_document(
        &self,
        p: &Principal,
        input: &Value,
        inner: &dyn Fn(&Value) -> Result<Value, String>,
    ) -> Result<Value, String> {
        let id = id_of(input)?;
        let Some(req) = self.get("sign_request", &id)? else {
            // The generated handler words the refusal.
            return inner(input);
        };
        if text(&req, "state") != "draft" {
            return inner(input);
        }
        let doc_id = text(&req, "document_id").to_string();
        if doc_id.is_empty() {
            return Err("sign_request: set document_id (the PDF in Documents) before sending".into());
        }
        let doc = self.visible_document(p, input, &doc_id)?;
        let file = text(&doc, "attachment").to_string();
        if file.is_empty() {
            return Err(format!("documents_document {doc_id} has no file to sign"));
        }
        // The hash is the blob id; recompute it from the stored bytes so that a store that was
        // written behind our back cannot give us a hash of something else.
        self.checked_pdf(&file)?;
        // Hold an exact copy for this request, so that replacing the Document's file later does
        // not delete what was sent. A copy held before for another file is let go first.
        let owner = Owner {
            filename: "document.pdf".to_string(),
            ..Owner::new("sign_request", &id, "document_file")
        };
        let previous = text(&req, "document_file").to_string();
        let blobs = self.store.blobs.clone();
        if !previous.is_empty() && previous != file {
            obop_storage::fault::needed(|| blobs.detach(&previous, &owner))?;
        }
        obop_storage::fault::needed(|| blobs.attach(&file, &owner))?;
        let mut patch = Fields::new();
        patch.insert("document_file".into(), J::from(file.clone()));
        patch.insert("document_hash".into(), J::from(file.clone()));
        if text(&req, "sent_on").is_empty() {
            patch.insert("sent_on".into(), J::from(self.today()));
        }
        let op = Op::Patch {
            model: "sign_request".into(),
            id: id.clone(),
            patch,
            expect: vec![("state".into(), J::from("draft"))],
        };
        self.store
            .backend
            .apply_checked(&[op])
            .map_err(|e| format!("could not hold the document for sign_request {id}: {e}"))?;
        // A crash or a refusal from here on leaves a draft that carries a hash and a held copy:
        // harmless, and a retry replaces both. A sent request never lacks them.
        inner(input)
    }
}
