//! Anonymous public access to declared records (D50).
//!
//! A package may declare `[model.public]` (see `obop-pkg`): which fields of which *published*
//! records anybody may read without signing in, and which fields an anonymous visitor may submit
//! to create a record. This module is the only door for that. It is not a principal with groups:
//! it never goes through `Dispatcher::call`, so no bearer token, MCP client or CLI user can ever
//! act as "the public", and the public can reach nothing the declarations do not name.
//!
//! - **Reads** return only the whitelisted fields of records whose publish flag is set. A record
//!   that is unpublished and one that does not exist are indistinguishable (`Lookup::NotFound`).
//!   The one exception is `Lookup::Gone`: a record in the declared `gone_value` state (for
//!   example `archived`) was public once, so saying so reveals nothing new.
//! - **Submissions** create records through the same validation as the generated `create`
//!   (typed fields D47, required fields, the governed first state D48). Visitor-supplied keys
//!   are restricted to `submit_fields`; refs a visitor supplies must point at a published record
//!   of a public model. Fields the route computes itself (`server`) are validated too but never
//!   come from the visitor. A batch (an order and its lines) is written in one transaction.
//! - **Rate limits** per model and source, and across all sources, in memory.

use crate::store::{declared_fields, validate_with};
use crate::{Dispatcher, Principal, Store, Value};
use obop_registry::{FieldKind, ModelDef};
use obop_storage::{Backend, Fields};
use obop_types::DataClass;
use serde_json::Value as J;
use std::collections::BTreeMap;
use std::sync::Arc;

/// What of a model anybody may read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicRead {
    /// The `bool` or `select` field that decides whether a record is public.
    pub published_field: String,
    /// For a `select`: the option that means published. `None`: the field is a `bool` that must
    /// be true.
    pub published_value: Option<String>,
    /// For a `select`: an option meaning "was public, is retired for good" (answered with 410).
    pub gone_value: Option<String>,
    /// The only fields a public read returns.
    pub fields: Vec<String>,
    pub order_by: Option<String>,
    pub order_desc: bool,
}

/// What an anonymous visitor may create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicSubmit {
    /// The only fields a visitor may supply.
    pub fields: Vec<String>,
    /// Submissions per source (and, times 50, across all sources) per hour.
    pub rate_limit_per_hour: u32,
}

/// Read-back by an unguessable token: what the person who submitted a record may see of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicReceipt {
    /// A `text` field of class `secret` holding the token (a root record), or `None` for a child.
    pub token_field: Option<String>,
    /// A `ref` to the parent model that has a token: the record is shown with its parent.
    pub parent_field: Option<String>,
    /// The fields shown. May include personal data the submitter gave, because only the holder
    /// of the token can see them.
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PublicSpec {
    pub model: String,
    pub read: Option<PublicRead>,
    pub submit: Option<PublicSubmit>,
    pub receipt: Option<PublicReceipt>,
    /// The server (never a visitor) may read every field of this model's records through
    /// [`PublicApi::server_records`]: for checks it must make itself, such as a discount code.
    /// Nothing about the model becomes public.
    pub server_read: bool,
    /// Integer fields the server may change atomically with an order
    /// ([`PublicApi::submit_order`]): stock, a code's use counter. Never a visitor.
    pub server_adjust: Vec<String>,
}

/// Classes a public form must never collect.
fn submit_class_ok(c: DataClass) -> bool {
    matches!(
        c,
        DataClass::None | DataClass::PublicDirectory | DataClass::Internal
    )
}

impl PublicSpec {
    /// The install gate for a public declaration (D41, D50): everything that becomes public is
    /// explicitly non-personal, and what a visitor can submit is not sensitive.
    pub fn check(&self, def: &ModelDef) -> Result<(), String> {
        let at = &self.model;
        let field = |n: &str| {
            def.fields
                .iter()
                .find(|f| f.name == n)
                .ok_or_else(|| format!("{at}.public: no field {n:?}"))
        };
        if self.read.is_none()
            && self.submit.is_none()
            && self.receipt.is_none()
            && !self.server_read
            && self.server_adjust.is_empty()
        {
            return Err(format!("{at}.public: declares nothing public"));
        }
        for n in &self.server_adjust {
            if !matches!(field(n)?.kind, FieldKind::Number) {
                return Err(format!(
                    "{at}.{n}: server_adjust fields must be numbers (counters)"
                ));
            }
        }
        if let Some(r) = &self.receipt {
            if r.fields.is_empty() {
                return Err(format!("{at}.public: a receipt needs receipt_fields"));
            }
            match (&r.token_field, &r.parent_field) {
                (Some(t), None) => {
                    let f = field(t)?;
                    if !matches!(f.kind, FieldKind::Text) || f.class != Some(DataClass::Secret) {
                        return Err(format!(
                            "{at}.{t}: a receipt token must be a text field of class secret"
                        ));
                    }
                }
                (None, Some(p)) => {
                    if !matches!(field(p)?.kind, FieldKind::Ref(_)) {
                        return Err(format!("{at}.{p}: receipt_parent must be a ref"));
                    }
                }
                _ => {
                    return Err(format!(
                        "{at}.public: a receipt has either receipt_token_field or receipt_parent"
                    ));
                }
            }
            for n in &r.fields {
                let f = field(n)?;
                if !f.class.is_some_and(submit_class_ok) {
                    return Err(format!(
                        "{at}.{n}: a receipt cannot show class {} data",
                        f.class.map_or("none", |c| c.as_str())
                    ));
                }
            }
        }
        if let Some(r) = &self.read {
            let pf = field(&r.published_field)?;
            match (&pf.kind, &r.published_value) {
                (FieldKind::Bool, None) => {}
                (FieldKind::Select(o), Some(v)) if o.contains(&v.as_str()) => {}
                (FieldKind::Bool, Some(_)) => {
                    return Err(format!(
                        "{at}.public: published_value is only for a select; {} is a bool",
                        pf.name
                    ));
                }
                (FieldKind::Select(_), _) => {
                    return Err(format!(
                        "{at}.public: published_value must be one of the options of {}",
                        pf.name
                    ));
                }
                _ => {
                    return Err(format!(
                        "{at}.public: published_field {} must be a bool or a select",
                        pf.name
                    ));
                }
            }
            if let Some(g) = &r.gone_value {
                let ok = matches!(&pf.kind, FieldKind::Select(o) if o.contains(&g.as_str()))
                    && Some(g) != r.published_value.as_ref();
                if !ok {
                    return Err(format!(
                        "{at}.public: gone_value must be another option of the select {}",
                        pf.name
                    ));
                }
            }
            if r.fields.is_empty() {
                return Err(format!("{at}.public: read = true needs `fields`"));
            }
            for n in &r.fields {
                let f = field(n)?;
                if !matches!(f.class, Some(DataClass::None | DataClass::PublicDirectory)) {
                    return Err(format!(
                        "{at}.{n}: only fields of class none or public_directory can be public"
                    ));
                }
                if f.personal_data {
                    return Err(format!(
                        "{at}.{n}: personal data can never be public{}",
                        if f.kind.is_blob() {
                            " (a photo of a person is personal data: mark the field personal_data and keep it private)"
                        } else {
                            ""
                        }
                    ));
                }
                if f.computed.is_some() {
                    return Err(format!(
                        "{at}.{n}: a computed field is not stored, so it cannot be public"
                    ));
                }
                if f.permission_group != def.read_group {
                    return Err(format!(
                        "{at}.{n}: a field restricted to {} cannot be public",
                        f.permission_group
                    ));
                }
            }
            if let Some(o) = &r.order_by {
                field(o)?;
            }
        }
        if let Some(s) = &self.submit {
            if s.fields.is_empty() {
                return Err(format!("{at}.public: submit = true needs `submit_fields`"));
            }
            if s.rate_limit_per_hour == 0 {
                return Err(format!(
                    "{at}.public: rate_limit_per_hour must be at least 1"
                ));
            }
            let governed = def.governed_fields();
            for n in &s.fields {
                let f = field(n)?;
                if governed.contains(&f.name) {
                    return Err(format!(
                        "{at}.{n}: a governed state field cannot be submitted; it starts at its first option"
                    ));
                }
                if f.computed.is_some() || f.sequence.is_some() {
                    return Err(format!(
                        "{at}.{n}: a computed or numbered field is the system's; a visitor cannot submit it"
                    ));
                }
                if !f.class.is_some_and(submit_class_ok) {
                    return Err(format!(
                        "{at}.{n}: a public form cannot collect class {} data",
                        f.class.map_or("none", |c| c.as_str())
                    ));
                }
                if f.kind.is_blob() {
                    return Err(format!(
                        "{at}.{n}: visitors cannot upload images or files (D60)"
                    ));
                }
            }
        }
        Ok(())
    }

    fn published(&self, rec: &Fields) -> bool {
        let Some(r) = &self.read else { return false };
        match (&r.published_value, rec.get(&r.published_field)) {
            (None, Some(J::Bool(b))) => *b,
            (Some(want), Some(J::String(s))) => s == want,
            _ => false,
        }
    }

    fn gone(&self, rec: &Fields) -> bool {
        let Some(r) = &self.read else { return false };
        match (&r.gone_value, rec.get(&r.published_field)) {
            (Some(want), Some(J::String(s))) => s == want,
            _ => false,
        }
    }
}

/// A public view of one record: its id and the whitelisted fields only.
#[derive(Debug, Clone, PartialEq)]
pub struct PublicRecord {
    pub id: String,
    pub fields: serde_json::Map<String, J>,
}

impl PublicRecord {
    pub fn text(&self, k: &str) -> &str {
        self.fields.get(k).and_then(J::as_str).unwrap_or("")
    }
    pub fn int(&self, k: &str) -> Option<i64> {
        self.fields.get(k).and_then(J::as_i64)
    }
    pub fn flag(&self, k: &str) -> bool {
        self.fields.get(k).and_then(J::as_bool).unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    Found(PublicRecord),
    /// Was public, is retired for good.
    Gone,
    /// Unpublished and nonexistent look the same.
    NotFound,
}

/// A condition on an exposed field of a public list.
#[derive(Debug, Clone)]
pub enum PublicFilter {
    Eq(Value),
    Contains(String),
}

/// A counter change that must happen together with an order, or not at all: take stock, count a
/// discount use. The field has to be declared `server_adjust` on the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adjust {
    pub model: String,
    pub id: String,
    pub field: String,
    pub delta: i64,
    /// The result must not fall below this (stock never goes below 0).
    pub min: Option<i64>,
    /// The result must not rise above this (a code's maximum number of uses).
    pub max: Option<i64>,
    /// If the record does not exist yet, create it with these fields first (an upsert counter,
    /// for example one use counter per code and customer).
    pub create: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SubmitError {
    /// An [`Adjust`] could not be applied: the stock ran out or a code was used up. The number
    /// is the index in the list that was passed. Nothing was written.
    Unavailable {
        adjust: usize,
    },
    /// The model does not accept public submissions (or does not exist).
    NotPublic,
    /// The data was refused; the message names the field and is safe to show.
    Rejected(String),
    RateLimited {
        retry_after_secs: u64,
    },
    Storage(String),
}

/// One record of a public submission batch.
#[derive(Debug, Clone, Default)]
pub struct Submission {
    pub model: String,
    /// Fields the visitor supplied; restricted to the model's `submit_fields`.
    pub visitor: BTreeMap<String, Value>,
    /// Fields the route computed itself (prices from the published listing, totals, a number).
    pub server: BTreeMap<String, Value>,
    /// Fields that refer to a record earlier in the same batch: (field, index in the batch).
    pub links: Vec<(String, usize)>,
}

const HOUR_MS: u64 = 3_600_000;
const GLOBAL_FACTOR: usize = 50;

/// The anonymous door, shared by the public routes.
#[derive(Clone)]
pub struct PublicApi {
    d: Arc<Dispatcher>,
    store: Store,
}

impl Dispatcher {
    /// Register the public declarations of `module`. The models must be registered already; the
    /// gate runs again here, so a declaration that does not pass cannot take effect.
    pub fn register_public(&self, module: &str, specs: Vec<PublicSpec>) -> Result<(), String> {
        let models = self.models.read().unwrap();
        for s in &specs {
            let meta = models
                .get(&s.model)
                .ok_or_else(|| format!("{}.public: no such model", s.model))?;
            s.check(&meta.def)?;
            if let Some(pf) = s.receipt.as_ref().and_then(|r| r.parent_field.as_ref()) {
                let to = meta
                    .def
                    .fields
                    .iter()
                    .find(|f| f.name == pf.as_str())
                    .and_then(|f| f.kind.target())
                    .unwrap_or_default();
                let has_token = specs
                    .iter()
                    .find(|x| x.model == to)
                    .is_some_and(|x| x.receipt.as_ref().is_some_and(|r| r.token_field.is_some()));
                if !has_token {
                    return Err(format!(
                        "{}.{pf}: receipt_parent must refer to a model with a receipt_token_field",
                        s.model
                    ));
                }
            }
        }
        drop(models);
        let mut w = self.public.write().unwrap();
        for s in specs {
            w.insert(s.model.clone(), (module.to_string(), s));
        }
        Ok(())
    }

    /// The public declaration of a model, if it has one.
    pub fn public_spec(&self, model: &str) -> Option<PublicSpec> {
        self.public
            .read()
            .unwrap()
            .get(model)
            .map(|(_, s)| s.clone())
    }

    /// Whether any model is publicly readable or submittable.
    pub fn has_public(&self) -> bool {
        !self.public.read().unwrap().is_empty()
    }
}

impl PublicApi {
    pub fn new(d: Arc<Dispatcher>, store: Store) -> Self {
        Self { d, store }
    }

    pub fn spec(&self, model: &str) -> Option<PublicSpec> {
        self.d.public_spec(model)
    }

    pub fn is_readable(&self, model: &str) -> bool {
        self.spec(model).is_some_and(|s| s.read.is_some())
    }

    fn def(&self, model: &str) -> Option<ModelDef> {
        self.d
            .models
            .read()
            .unwrap()
            .get(model)
            .map(|m| (*m.def).clone())
    }

    fn project(&self, spec: &PublicSpec, id: &str, rec: &Fields, embed: bool) -> PublicRecord {
        let read = spec.read.as_ref().expect("readable spec");
        let def = self.def(&spec.model);
        let mut fields = serde_json::Map::new();
        for name in &read.fields {
            let Some(v) = rec.get(name) else { continue };
            let target = def.as_ref().and_then(|d| {
                d.fields
                    .iter()
                    .find(|f| f.name == name.as_str())
                    .and_then(|f| match f.kind {
                        FieldKind::Ref(to) => Some(to),
                        _ => None,
                    })
            });
            match (target, v) {
                (Some(to), J::String(tid)) => {
                    // A ref is shown as the public view of the target, only if the target is
                    // public-read and that record is published; otherwise it is left out.
                    if embed
                        && let Some(ts) = self.spec(to).filter(|s| s.read.is_some())
                        && let Ok(Some(t)) = self.store.backend.get(to, tid)
                        && ts.published(&t)
                    {
                        let r = self.project(&ts, tid, &t, false);
                        let mut o = r.fields;
                        o.insert("id".into(), J::String(tid.clone()));
                        fields.insert(name.clone(), J::Object(o));
                    }
                }
                _ => {
                    fields.insert(name.clone(), v.clone());
                }
            }
        }
        PublicRecord {
            id: id.to_string(),
            fields,
        }
    }

    /// Published records of a model, in the declared order, optionally narrowed by conditions on
    /// exposed fields. Returns the page and the number of matches before paging.
    pub fn list(
        &self,
        model: &str,
        filters: &[(String, PublicFilter)],
        limit: Option<usize>,
        offset: usize,
    ) -> Result<(Vec<PublicRecord>, usize), String> {
        let spec = self
            .spec(model)
            .filter(|s| s.read.is_some())
            .ok_or("not public")?;
        let read = spec.read.as_ref().expect("checked");
        let def = self.def(model).ok_or("not public")?;
        let mut conds = vec![Value::map([
            ("field", Value::text(&read.published_field)),
            ("op", Value::text("eq")),
            (
                "value",
                match &read.published_value {
                    Some(v) => Value::text(v),
                    None => Value::Bool(true),
                },
            ),
        ])];
        for (f, c) in filters {
            if !read.fields.contains(f) {
                return Err(format!("unknown field {model}.{f}"));
            }
            conds.push(match c {
                PublicFilter::Eq(v) => Value::map([
                    ("field", Value::text(f)),
                    ("op", Value::text("eq")),
                    ("value", v.clone()),
                ]),
                PublicFilter::Contains(t) => Value::map([
                    ("field", Value::text(f)),
                    ("op", Value::text("contains")),
                    ("value", Value::text(t)),
                ]),
            });
        }
        let mut input = BTreeMap::from([("filter".to_string(), Value::List(conds))]);
        if let Some(o) = &read.order_by {
            input.insert(
                "order".into(),
                Value::List(vec![Value::map([
                    ("field", Value::text(o)),
                    (
                        "dir",
                        Value::text(if read.order_desc { "desc" } else { "asc" }),
                    ),
                ])]),
            );
        }
        if let Some(l) = limit {
            input.insert("limit".into(), Value::Int(l.clamp(1, 5000) as i64));
        }
        if offset > 0 {
            input.insert("offset".into(), Value::Int(offset as i64));
        }
        let q = crate::query::Query::parse(&def, &Value::Map(input), &|_| true)?;
        q.no_aggregate()?;
        let (page, total) = q.apply(self.store.backend.list(model)?);
        Ok((
            page.iter()
                .map(|(id, rec)| self.project(&spec, id, rec, true))
                .collect(),
            total,
        ))
    }

    /// One record by id.
    pub fn get(&self, model: &str, id: &str) -> Result<Lookup, String> {
        let Some(spec) = self.spec(model).filter(|s| s.read.is_some()) else {
            return Ok(Lookup::NotFound);
        };
        Ok(match self.store.backend.get(model, id)? {
            Some(rec) if spec.published(&rec) => Lookup::Found(self.project(&spec, id, &rec, true)),
            Some(rec) if spec.gone(&rec) => Lookup::Gone,
            _ => Lookup::NotFound,
        })
    }

    /// The first published record whose exposed text field `field` equals `value` (a slug).
    pub fn find_by(&self, model: &str, field: &str, value: &str) -> Result<Lookup, String> {
        let Some(spec) = self.spec(model).filter(|s| s.read.is_some()) else {
            return Ok(Lookup::NotFound);
        };
        let read = spec.read.as_ref().expect("checked");
        if !read.fields.iter().any(|f| f == field) {
            return Err(format!("unknown field {model}.{field}"));
        }
        let mut gone = false;
        for (id, rec) in self.store.backend.list(model)? {
            if rec.get(field).and_then(J::as_str) != Some(value) {
                continue;
            }
            if spec.published(&rec) {
                return Ok(Lookup::Found(self.project(&spec, &id, &rec, true)));
            }
            gone |= spec.gone(&rec);
        }
        Ok(if gone { Lookup::Gone } else { Lookup::NotFound })
    }

    /// A field of a *published* record that is not exposed to visitors, for the server's own use
    /// (the product behind a listing). Only fields of class `none` that are not personal data.
    pub fn published_server_field(
        &self,
        model: &str,
        id: &str,
        field: &str,
    ) -> Result<Option<J>, String> {
        let Some(spec) = self.spec(model).filter(|s| s.read.is_some()) else {
            return Ok(None);
        };
        let def = self.def(model).ok_or("not public")?;
        let ok = def.fields.iter().any(|f| {
            f.name == field
                && f.class == Some(DataClass::None)
                && !f.personal_data
                && f.permission_group == def.read_group
        });
        if !ok {
            return Err(format!("{model}.{field} cannot be read by the server here"));
        }
        Ok(match self.store.backend.get(model, id)? {
            Some(rec) if spec.published(&rec) => rec.get(field).cloned(),
            _ => None,
        })
    }

    /// The record that `token` belongs to, with the child records that point at it, limited to
    /// the declared receipt fields. `None` for a wrong or malformed token.
    pub fn receipt(&self, model: &str, token: &str) -> Result<Option<Receipt>, String> {
        let spec = self.spec(model);
        let Some(r) = spec.as_ref().and_then(|s| s.receipt.as_ref()) else {
            return Ok(None);
        };
        let Some(tf) = &r.token_field else {
            return Ok(None);
        };
        // Tokens are 256 random bits; anything much shorter cannot be one.
        if token.len() < 32 || token.len() > 128 {
            return Ok(None);
        }
        let mut found = None;
        for (id, rec) in self.store.backend.list(model)? {
            // Compare every candidate the same way, without stopping at the first difference.
            if rec
                .get(tf)
                .and_then(J::as_str)
                .is_some_and(|t| ct_eq(t.as_bytes(), token.as_bytes()))
            {
                found = Some((id, rec));
            }
        }
        let Some((id, rec)) = found else {
            return Ok(None);
        };
        let pick = |id: &str, rec: &Fields, fields: &[String]| PublicRecord {
            id: id.to_string(),
            fields: fields
                .iter()
                .filter_map(|n| rec.get(n).map(|v| (n.clone(), v.clone())))
                .collect(),
        };
        let mut children = Vec::new();
        let specs: Vec<PublicSpec> = self
            .d
            .public
            .read()
            .unwrap()
            .values()
            .map(|(_, s)| s.clone())
            .collect();
        for cs in specs {
            let Some(cr) = &cs.receipt else { continue };
            let Some(pf) = &cr.parent_field else { continue };
            let Some(def) = self.def(&cs.model) else {
                continue;
            };
            let points_here = def
                .fields
                .iter()
                .any(|f| f.name == pf.as_str() && f.kind.target() == Some(model));
            if !points_here {
                continue;
            }
            let rows = self
                .store
                .backend
                .list(&cs.model)?
                .into_iter()
                .filter(|(_, c)| c.get(pf).and_then(J::as_str) == Some(id.as_str()))
                .map(|(cid, c)| pick(&cid, &c, &cr.fields))
                .collect();
            children.push((cs.model.clone(), rows));
        }
        Ok(Some(Receipt {
            record: pick(&id, &rec, &r.fields),
            children,
        }))
    }

    /// Whether anybody may fetch this blob: it is held, through a field its package declared
    /// public, by a record that is published and whose required parents (a gallery image's
    /// listing) are published too. A blob that is held only by private records, or only waits as
    /// an upload, is not public, and neither is one that merely shares its bytes with a public
    /// one under another owner's name.
    fn blob_is_public(&self, info: &obop_storage::blob::BlobInfo) -> bool {
        info.owners.iter().any(|o| {
            let Some(spec) = self.spec(&o.model) else {
                return false;
            };
            let Some(read) = spec.read.as_ref() else {
                return false;
            };
            if !read.fields.contains(&o.field) {
                return false;
            }
            let Ok(Some(rec)) = self.store.backend.get(&o.model, &o.record) else {
                return false;
            };
            if !spec.published(&rec) {
                return false;
            }
            let Some(def) = self.def(&o.model) else {
                return false;
            };
            def.fields.iter().filter(|f| f.required).all(|f| match f.kind {
                FieldKind::Ref(to) => match (self.spec(to), rec.get(f.name).and_then(J::as_str)) {
                    (Some(ts), Some(tid)) if ts.read.is_some() => {
                        matches!(self.store.backend.get(to, tid), Ok(Some(t)) if ts.published(&t))
                    }
                    _ => true,
                },
                _ => true,
            })
        })
    }

    /// Width, height and type of a publicly visible blob (for `width`/`height` attributes).
    pub fn public_blob_info(&self, id: &str) -> Option<(Option<u32>, Option<u32>, String)> {
        let info = self.store.blobs.info(id).ok().flatten()?;
        self.blob_is_public(&info)
            .then_some((info.width, info.height, info.content_type))
    }

    /// A blob for an anonymous visitor, or `None` (answered as not found) unless
    /// [`Self::blob_is_public`]. `width` asks for a cached thumbnail.
    pub fn public_blob(&self, id: &str, width: Option<u32>) -> Option<crate::blobs::Served> {
        let info = self.store.blobs.info(id).ok().flatten()?;
        if !self.blob_is_public(&info) {
            return None;
        }
        crate::blobs::serve(&self.store, &info, width).ok()
    }

    /// The `site` object of the instance settings: the only settings the public side reads.
    pub fn site_settings(&self) -> serde_json::Map<String, J> {
        self.store
            .backend
            .get("obop_setting", "instance")
            .ok()
            .flatten()
            .and_then(|f| f.get("value").and_then(J::as_str).map(String::from))
            .and_then(|t| serde_json::from_str::<J>(&t).ok())
            .and_then(|v| match v.get("site") {
                Some(J::Object(o)) => Some(o.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Count a hit for the rate limit; `Err` carries seconds until the window ends. The count is
    /// kept in the storage backend, so every server process on the database shares it (B1).
    fn hit(&self, key: &str, limit: usize) -> Result<(), u64> {
        obop_storage::limit::hit(
            &*self.store.backend,
            &format!("rl:public:{}", key.replace('\0', ":")),
            limit as u64,
            HOUR_MS,
        )
    }

    /// The storage backend behind the public door.
    pub fn backend(&self) -> &Arc<dyn Backend> {
        &self.store.backend
    }

    /// Create the records of a batch in one transaction and return their ids. `source` is an
    /// opaque key for the visitor (a hash of the address); the first record's model carries the
    /// rate limit.
    pub fn submit_batch(
        &self,
        source: &str,
        batch: Vec<Submission>,
    ) -> Result<Vec<String>, SubmitError> {
        let rows = self.prepare(source, batch)?;
        self.store
            .backend
            .insert_many(&rows)
            .map_err(SubmitError::Storage)?;
        Ok(rows.into_iter().map(|(_, id, _)| id).collect())
    }

    /// Like [`Self::submit_batch`], and the `adjust` changes happen in the same transaction,
    /// checked against their bounds under a lock: two buyers cannot both take the last unit, and
    /// a code cannot be used past its maximum. Either everything is written or nothing is.
    pub fn submit_order(
        &self,
        source: &str,
        batch: Vec<Submission>,
        adjust: &[Adjust],
    ) -> Result<Vec<String>, SubmitError> {
        let mut ops: Vec<obop_storage::Op> = Vec::new();
        for a in adjust {
            let declared = self
                .spec(&a.model)
                .is_some_and(|s| s.server_adjust.contains(&a.field));
            if !declared || a.delta.unsigned_abs() > 1_000_000 {
                return Err(SubmitError::Rejected(format!(
                    "{}.{} cannot be adjusted",
                    a.model, a.field
                )));
            }
            let def = self.def(&a.model).ok_or(SubmitError::NotPublic)?;
            let create = match &a.create {
                None => None,
                Some(c) => {
                    let f = declared_fields(&def, &Value::Map(c.clone()))
                        .map_err(SubmitError::Rejected)?;
                    Some(f.iter().map(|(k, v)| (k.clone(), v.to_json())).collect())
                }
            };
            ops.push(obop_storage::Op::Add {
                model: a.model.clone(),
                id: a.id.clone(),
                field: a.field.clone(),
                delta: a.delta,
                min: a.min,
                max: a.max,
                create,
            });
        }
        let rows = self.prepare(source, batch)?;
        let n = ops.len();
        for (model, id, fields) in &rows {
            ops.push(obop_storage::Op::Insert {
                model: model.clone(),
                id: id.clone(),
                fields: fields.clone(),
            });
        }
        match self.store.backend.apply_checked(&ops) {
            Ok(()) => Ok(rows.into_iter().map(|(_, id, _)| id).collect()),
            Err(obop_storage::ApplyError::Conflict { index, .. }) if index < n => {
                Err(SubmitError::Unavailable { adjust: index })
            }
            Err(e) => Err(SubmitError::Storage(e.to_string())),
        }
    }

    /// Every record of a model that declared `server_read`, with all its fields, for checks the
    /// server makes itself (a discount code). Nothing of it is ever rendered by the generic
    /// public routes.
    pub fn server_records(&self, model: &str) -> Result<Vec<(String, Fields)>, String> {
        if !self.spec(model).is_some_and(|s| s.server_read) {
            return Err(format!("{model} is not readable by the server here"));
        }
        self.store.backend.list(model)
    }

    /// One record of a `server_read` model.
    pub fn server_record(&self, model: &str, id: &str) -> Result<Option<Fields>, String> {
        if !self.spec(model).is_some_and(|s| s.server_read) {
            return Err(format!("{model} is not readable by the server here"));
        }
        self.store.backend.get(model, id)
    }

    /// Validate a batch and turn it into rows, charging the rate limits.
    fn prepare(
        &self,
        source: &str,
        batch: Vec<Submission>,
    ) -> Result<Vec<(String, String, Fields)>, SubmitError> {
        let Some(first) = batch.first() else {
            return Err(SubmitError::Rejected("nothing to submit".into()));
        };
        let root = self
            .spec(&first.model)
            .and_then(|s| s.submit)
            .ok_or(SubmitError::NotPublic)?;
        let limit = root.rate_limit_per_hour as usize;
        self.hit(
            &format!("{}\0*", first.model),
            limit.saturating_mul(GLOBAL_FACTOR),
        )
        .and_then(|()| self.hit(&format!("{}\0{source}", first.model), limit))
        .map_err(|retry_after_secs| SubmitError::RateLimited { retry_after_secs })?;

        let anon = Principal::public();
        let mut rows: Vec<(String, String, Fields)> = Vec::new();
        let mut pending: Vec<(String, String)> = Vec::new();
        for s in batch.iter() {
            let submit = self
                .spec(&s.model)
                .and_then(|x| x.submit)
                .ok_or(SubmitError::NotPublic)?;
            let def = self.def(&s.model).ok_or(SubmitError::NotPublic)?;
            let mut fields: BTreeMap<String, Value> = BTreeMap::new();
            for (k, v) in &s.visitor {
                let Some(f) = def.fields.iter().find(|f| f.name == k.as_str()) else {
                    return Err(SubmitError::Rejected(format!(
                        "unknown field {}.{k}",
                        s.model
                    )));
                };
                if !submit.fields.contains(k) {
                    return Err(SubmitError::Rejected(format!(
                        "{}.{k} cannot be set publicly",
                        s.model
                    )));
                }
                if let FieldKind::Ref(to) = f.kind {
                    self.check_public_ref(&s.model, k, to, v)?;
                }
                fields.insert(k.clone(), v.clone());
            }
            for (k, v) in &s.server {
                if fields.insert(k.clone(), v.clone()).is_some() {
                    return Err(SubmitError::Rejected(format!(
                        "{}.{k} is set by the server",
                        s.model
                    )));
                }
            }
            for (field, idx) in &s.links {
                let Some((_, id)) = pending.get(*idx) else {
                    return Err(SubmitError::Rejected(format!(
                        "{}.{field} refers to a later record",
                        s.model
                    )));
                };
                fields.insert(field.clone(), Value::text(id));
            }
            let mut input = Value::Map(fields);
            let guard = self
                .d
                .guards
                .read()
                .unwrap()
                .get(&format!("{}.create", s.model))
                .cloned();
            if let Some(g) = guard {
                let v = (g.guard)(&anon, &input).map_err(SubmitError::Rejected)?;
                if let Some(i) = v.input {
                    input = i;
                }
            }
            let fields = declared_fields(&def, &input).map_err(SubmitError::Rejected)?;
            validate_with(&def, &self.store, &fields, true, &pending)
                .map_err(SubmitError::Rejected)?;
            let id = uuid::Uuid::now_v7().to_string();
            pending.push((s.model.clone(), id.clone()));
            rows.push((
                s.model.clone(),
                id,
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect(),
            ));
        }
        Ok(rows)
    }

    /// A ref a visitor supplies must point at a published record of a public-read model.
    fn check_public_ref(
        &self,
        model: &str,
        field: &str,
        to: &str,
        v: &Value,
    ) -> Result<(), SubmitError> {
        let unavailable = || SubmitError::Rejected(format!("{model}.{field} is not available"));
        let Value::Text(id) = v else {
            return Err(unavailable());
        };
        match self.get(to, id).map_err(SubmitError::Storage)? {
            Lookup::Found(_) => Ok(()),
            _ => Err(unavailable()),
        }
    }
}

impl Principal {
    /// The anonymous visitor: no groups and an empty scope, so it satisfies no permission check
    /// even if it were ever handed to the dispatcher. Crate-private; only `PublicApi` uses it.
    pub(crate) fn public() -> Self {
        Self {
            id: "public".into(),
            kind: crate::PrincipalKind::User,
            groups: Default::default(),
            scopes: Some(Default::default()),
            mfa_limited: false,
        }
    }
}

/// What the holder of a receipt token may see: the record and its child records.
#[derive(Debug, Clone, PartialEq)]
pub struct Receipt {
    pub record: PublicRecord,
    /// (child model, records) such as the lines of an order.
    pub children: Vec<(String, Vec<PublicRecord>)>,
}

/// Constant-time comparison of two byte strings (length differences are not secret).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests;
