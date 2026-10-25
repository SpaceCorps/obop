//! OpenAPI 3.1 descriptions, so agents and tools can discover the API without reading docs
//! (D31): a fixed one for the control plane, and one generated from the registry for each
//! instance, listing exactly the commands the caller may use.

use obop_commands::ModuleInfo;
use serde_json::{Value as J, json};

fn error_schema() -> J {
    json!({
        "type": "object",
        "required": ["error"],
        "properties": {
            "error": {"type": "string", "description": "What went wrong, in a sentence."},
            "code": {"type": "string", "description": "Stable machine-readable code such as not_found, unauthorized, invalid_input, forbidden, rate_limited."},
            "hint": {"type": "string", "description": "What to do next."}
        }
    })
}

fn error_responses() -> J {
    json!({
        "400": {"description": "The input was rejected; the error says which field and why.", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Error"}}}},
        "401": {"description": "Missing, expired or revoked bearer token.", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Error"}}}},
        "404": {"description": "No such command or record, or it is hidden from this token.", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Error"}}}},
        "503": {"description": "The store was busy, unreachable or too slow (D74): error is storage_busy, storage_unavailable or storage_timeout, and Retry-After says how many seconds to wait. Nothing was started for storage_busy; retry it. After storage_unavailable or storage_timeout a write may or may not have been applied.", "headers": {"Retry-After": {"description": "Seconds to wait before trying again.", "schema": {"type": "integer"}}}, "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Error"}}}}
    })
}

fn bearer() -> J {
    json!({"bearerAuth": {"type": "http", "scheme": "bearer", "description": "A session token from login, or a scoped agent token."}})
}

fn json_body(schema: J) -> J {
    json!({"required": true, "content": {"application/json": {"schema": schema}}})
}

fn ok(schema: J, description: &str) -> J {
    json!({"description": description, "content": {"application/json": {"schema": schema}}})
}

/// The control plane: sign up, sign in, manage instances, plus the public status feeds.
pub fn platform(base: &str) -> J {
    let s = json!([{"bearerAuth": []}]);
    let token = ok(
        json!({"type": "object", "properties": {"token": {"type": "string"}}}),
        "A bearer token.",
    );
    let instance = json!({
        "type": "object",
        "properties": {
            "id": {"type": "string"}, "name": {"type": "string"}, "slug": {"type": "string"},
            "status": {"type": "string", "enum": ["provisioning", "running", "failed", "deleting"]},
            "url": {"type": "string"}, "error": {"type": "string"}, "phase": {"type": "string", "description": "What the background job is doing right now."}
        }
    });
    let creds = json_body(
        json!({"type": "object", "required": ["email", "password"], "properties": {"email": {"type": "string", "format": "email"}, "password": {"type": "string", "minLength": 12}}}),
    );
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "obop control plane API",
            "version": obop_types::CORE_VERSION,
            "description": "Sign up, create your own obop instance, watch its setup, and delete it. Each instance then has its own API, described at /api/openapi.json on the instance. Errors are JSON with error, code and hint. A 503 with a Retry-After header (storage_busy, storage_unavailable, storage_timeout) means the database could not serve the request: try again after that many seconds."
        },
        "servers": [{"url": base}],
        "paths": {
            "/api/platform/signup": {"post": {"operationId": "signup", "summary": "Create an account", "requestBody": creds.clone(), "responses": {"200": token.clone(), "400": error_responses()["400"].clone()}}},
            "/api/platform/login": {"post": {"operationId": "login", "summary": "Sign in", "requestBody": creds, "responses": {"200": token, "401": error_responses()["401"].clone()}}},
            "/api/platform/login/mfa": {"post": {"operationId": "loginMfa", "summary": "Second sign-in step for an account with two-factor on: mfa_token plus code or recovery_code", "requestBody": json_body(json!({"type": "object", "required": ["mfa_token"], "properties": {"mfa_token": {"type": "string"}, "code": {"type": "string"}, "recovery_code": {"type": "string"}}})), "responses": {"200": ok(json!({"type": "object", "properties": {"token": {"type": "string"}}}), "A session token."), "401": error_responses()["401"].clone(), "429": {"description": "Too many wrong codes."}}}},
            "/api/platform/mfa": {"get": {"operationId": "mfaStatus", "summary": "Optional TOTP two-factor of your own account: status. Also POST /api/platform/mfa/begin, /confirm, /disable and /recovery, as on an instance (/api/auth/mfa/*)", "security": s.clone(), "responses": {"200": {"description": "enabled, pending, recovery_remaining."}, "401": error_responses()["401"].clone()}}},
            "/api/platform/me": {"get": {"operationId": "me", "summary": "The signed-in account", "security": s.clone(), "responses": {"200": ok(json!({"type": "object", "properties": {"email": {"type": "string"}}}), "The account."), "401": error_responses()["401"].clone()}}},
            "/api/platform/instances": {
                "get": {"operationId": "listInstances", "summary": "Your instances", "security": s.clone(), "responses": {"200": ok(json!({"type": "object", "properties": {"instances": {"type": "array", "items": {"$ref": "#/components/schemas/Instance"}}}}), "Your instances."), "401": error_responses()["401"].clone()}},
                "post": {"operationId": "createInstance", "summary": "Create an instance (returns at once; poll the list for status and phase)", "security": s.clone(), "requestBody": json_body(json!({"type": "object", "required": ["name"], "properties": {"name": {"type": "string", "minLength": 3, "maxLength": 30}}})), "responses": {"202": ok(json!({"$ref": "#/components/schemas/Instance"}), "Provisioning started."), "400": error_responses()["400"].clone(), "401": error_responses()["401"].clone()}}
            },
            "/api/platform/instances/{id}/delete": {"post": {"operationId": "deleteInstance", "summary": "Delete an instance (returns at once with status deleting)", "security": s, "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}], "responses": {"202": ok(json!({"$ref": "#/components/schemas/Instance"}), "Teardown started."), "404": error_responses()["404"].clone(), "401": error_responses()["401"].clone()}}},
            "/api/releases": {"get": {"operationId": "releases", "summary": "The published core releases (D63): version, notes, image, min_from and a SHA-256 digest each. Public; instances read it when an administrator checks for updates", "responses": {"200": {"description": "{releases: [...]}"}}}},
            "/api/platform/update/status": {"get": {"operationId": "updateStatus", "summary": "State of an instance's update. Authenticated with the instance's update token (obu_...), which opens nothing else", "security": s.clone(), "responses": {"200": {"description": "state idle|running|succeeded|failed|rolled_back, phase, target, from, image, error, can_rollback."}, "401": error_responses()["401"].clone()}}},
            "/api/platform/update/start": {"post": {"operationId": "updateStart", "summary": "Ask to be moved to a published release. Only a published version can be named (no image field); one request at a time, rate limited", "security": s.clone(), "requestBody": json_body(json!({"type": "object", "required": ["version"], "properties": {"version": {"type": "string"}, "from_version": {"type": "string"}}})), "responses": {"200": {"description": "Started; poll the status."}, "401": error_responses()["401"].clone(), "409": {"description": "An update is already running or the instance already runs this release."}, "429": {"description": "Too many requests."}}}},
            "/api/platform/update/rollback": {"post": {"operationId": "updateRollback", "summary": "Ask to go back to the image before the last update", "security": s.clone(), "responses": {"200": {"description": "Started; poll the status."}, "401": error_responses()["401"].clone(), "409": {"description": "Busy."}}}},
            "/status/api/v2/summary.json": {"get": {"operationId": "statusSummary", "summary": "Service status (Atlassian Statuspage v2 shape)", "responses": {"200": {"description": "Status summary."}}}},
            "/status/api/v2/components.json": {"get": {"operationId": "statusComponents", "summary": "Component statuses", "responses": {"200": {"description": "Components."}}}},
            "/status/api/v2/incidents.json": {"get": {"operationId": "statusIncidents", "summary": "Incidents", "responses": {"200": {"description": "Incidents."}}}}
        },
        "components": {"securitySchemes": bearer(), "schemas": {"Error": error_schema(), "Instance": instance}}
    })
}

fn kind_schema(kind: &str, options: Option<&[String]>, to: Option<&str>) -> J {
    match kind {
        "number" | "money" => {
            json!({"type": "integer", "description": if kind == "money" { "Minor currency units (cents); never a float." } else { "An integer." }})
        }
        "bool" => json!({"type": "boolean"}),
        "date" => json!({"type": "string", "format": "date"}),
        "select" => json!({"type": "string", "enum": options.unwrap_or(&[])}),
        "ref" => {
            json!({"type": "string", "description": format!("The id of a {} record.", to.unwrap_or("referenced"))})
        }
        "longtext" => json!({"type": "string"}),
        "image" | "file" => json!({
            "type": "string",
            "pattern": "^[0-9a-f]{64}$",
            "description": "A blob id: the SHA-256 of an uploaded object. Upload with POST /api/blobs?field=model.field and use the returned id. Reads return the id and metadata, never the bytes."
        }),
        _ => json!({"type": "string"}),
    }
}

/// The instance API for one caller. `modules` is what that caller may see (empty when anonymous).
pub fn instance(base: &str, modules: &[ModuleInfo]) -> J {
    let sec = json!([{"bearerAuth": []}]);
    let mut paths = serde_json::Map::new();
    paths.insert(
        "/api/auth/login".into(),
        json!({"post": {"operationId": "login", "summary": "Sign in", "requestBody": json_body(json!({"type": "object", "required": ["username", "password"], "properties": {"username": {"type": "string"}, "password": {"type": "string"}}})), "responses": {"200": ok(json!({"type": "object", "properties": {"token": {"type": "string"}, "mfa_required": {"type": "boolean", "description": "True when a second factor is needed: no token, post mfa_token and a code to /api/auth/login/mfa."}, "mfa_token": {"type": "string", "description": "Single purpose, valid 5 minutes, at most 5 attempts."}, "mfa_enrolment_required": {"type": "boolean", "description": "The session may only set up two-factor authentication."}}}), "A session token, or the demand for a second factor."), "401": error_responses()["401"].clone()}}}),
    );
    let code_body = json_body(
        json!({"type": "object", "properties": {"code": {"type": "string", "description": "The 6-digit code of the authenticator app."}, "recovery_code": {"type": "string", "description": "A single-use recovery code instead of the code."}}}),
    );
    paths.insert(
        "/api/auth/login/mfa".into(),
        json!({"post": {"operationId": "loginMfa", "summary": "Second sign-in step: exchange an mfa_token and a code for a session", "requestBody": json_body(json!({"type": "object", "required": ["mfa_token"], "properties": {"mfa_token": {"type": "string"}, "code": {"type": "string"}, "recovery_code": {"type": "string"}}})), "responses": {"200": ok(json!({"type": "object", "properties": {"token": {"type": "string"}}}), "A session token."), "401": error_responses()["401"].clone(), "429": {"description": "Too many wrong codes."}}}}),
    );
    paths.insert(
        "/api/auth/mfa".into(),
        json!({"get": {"operationId": "mfaStatus", "summary": "Two-factor status of the signed-in person (a human session only; an agent token gets 404)", "security": sec.clone(), "responses": {"200": {"description": "enabled, pending, recovery_remaining, limited, require_admins, require_all."}}}}),
    );
    paths.insert(
        "/api/auth/mfa/begin".into(),
        json!({"post": {"operationId": "mfaBegin", "summary": "Start enrolment: returns a base32 secret and an otpauth:// URI; nothing is active yet", "security": sec.clone(), "responses": {"200": {"description": "secret, uri."}, "503": {"description": "The server has no OBOP_SECRET_KEY."}}}}),
    );
    paths.insert(
        "/api/auth/mfa/confirm".into(),
        json!({"post": {"operationId": "mfaConfirm", "summary": "Activate two-factor with a valid code; returns 10 recovery codes, shown once", "security": sec.clone(), "requestBody": json_body(json!({"type": "object", "required": ["code"], "properties": {"code": {"type": "string"}}})), "responses": {"200": {"description": "recovery_codes."}, "401": error_responses()["401"].clone()}}}),
    );
    paths.insert(
        "/api/auth/mfa/disable".into(),
        json!({"post": {"operationId": "mfaDisable", "summary": "Turn two-factor off: needs the password and a current code or a recovery code", "security": sec.clone(), "requestBody": code_body.clone(), "responses": {"200": {"description": "Turned off."}, "401": error_responses()["401"].clone()}}}),
    );
    paths.insert(
        "/api/auth/mfa/recovery".into(),
        json!({"post": {"operationId": "mfaRecovery", "summary": "Replace the recovery codes: needs the password and a current code or a recovery code", "security": sec.clone(), "requestBody": code_body, "responses": {"200": {"description": "recovery_codes."}, "401": error_responses()["401"].clone()}}}),
    );
    paths.insert(
        "/api/auth/mfa/reset".into(),
        json!({"post": {"operationId": "mfaReset", "summary": "platform.admin removes another person's two-factor (audited; never your own)", "security": sec.clone(), "requestBody": json_body(json!({"type": "object", "required": ["user"], "properties": {"user": {"type": "string"}}})), "responses": {"200": {"description": "Reset."}, "404": error_responses()["404"].clone()}}}),
    );
    paths.insert(
        "/api/auth/agent-token".into(),
        json!({"post": {"operationId": "issueAgentToken", "summary": "A signed-in human issues a scoped agent token", "security": sec.clone(), "requestBody": json_body(json!({"type": "object", "required": ["scopes"], "properties": {"scopes": {"type": "array", "items": {"type": "string"}, "description": "Permission groups, each one the issuer holds."}, "label": {"type": "string"}}})), "responses": {"200": ok(json!({"type": "object", "properties": {"token": {"type": "string"}}}), "The token, shown once."), "400": error_responses()["400"].clone()}}}),
    );
    paths.insert(
        "/api/meta".into(),
        json!({"get": {"operationId": "meta", "summary": "Modules, models, readable fields and actions this token may use", "security": sec.clone(), "responses": {"200": {"description": "The metadata."}, "401": error_responses()["401"].clone()}}}),
    );
    paths.insert(
        "/api/approve/{plan_id}".into(),
        json!({"post": {"operationId": "approvePlan", "summary": "A human approves a plan an agent proposed", "security": sec.clone(), "parameters": [{"name": "plan_id", "in": "path", "required": true, "schema": {"type": "string"}}], "responses": {"200": {"description": "The plan ran."}, "404": error_responses()["404"].clone()}}}),
    );
    paths.insert(
        "/api/blobs".into(),
        json!({"post": {"operationId": "uploadBlob", "summary": "Upload an image or file for an image/file field (D60)", "description": "Send the bytes as the request body with their Content-Type (the type is checked against the content, never trusted). `field` names the target as model.field; the caller needs write access to it. Images are decoded and re-encoded (metadata removed); SVG is refused. Returns the blob id to store in the field. Rate limited per principal.", "security": sec.clone(), "parameters": [{"name": "field", "in": "query", "required": true, "schema": {"type": "string", "example": "product.image"}}, {"name": "filename", "in": "query", "required": false, "schema": {"type": "string"}}], "requestBody": {"required": true, "content": {"application/octet-stream": {"schema": {"type": "string", "format": "binary"}}}}, "responses": {"201": ok(json!({"type": "object", "properties": {"id": {"type": "string"}, "content_type": {"type": "string"}, "size": {"type": "integer"}, "width": {"type": "integer"}, "height": {"type": "integer"}}}), "The stored blob."), "400": error_responses()["400"].clone(), "401": error_responses()["401"].clone(), "413": {"description": "Larger than the field allows."}, "415": {"description": "Not an accepted type, or the content is not what the Content-Type says."}, "429": {"description": "Too many uploads."}}}}),
    );
    paths.insert(
        "/api/blobs/{id}".into(),
        json!({"get": {"operationId": "downloadBlob", "summary": "Download a stored image or file (D60)", "description": "Allowed to a signed-in user who may read a record that holds the blob (or who just uploaded it). Agents get 404: they work with ids and metadata. `w` asks for a cached thumbnail of 320, 640, 960 or 1280 pixels. Strong ETag; files are sent as attachments.", "security": sec.clone(), "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string", "pattern": "^[0-9a-f]{64}$"}}, {"name": "w", "in": "query", "required": false, "schema": {"type": "integer", "enum": [320, 640, 960, 1280]}}], "responses": {"200": {"description": "The bytes."}, "304": {"description": "Not modified."}, "401": error_responses()["401"].clone(), "404": error_responses()["404"].clone()}}}),
    );
    let mut schemas = serde_json::Map::new();
    schemas.insert("Error".into(), error_schema());
    schemas.insert(
        "ApprovalRequired".into(),
        json!({"type": "object", "properties": {"approval_required": {"type": "boolean"}, "plan_id": {"type": "string"}}, "description": "An agent may only propose this step: ask a human to approve the plan."}),
    );
    for m in modules {
        for model in &m.models {
            let name = &model.name;
            let mut props = serde_json::Map::new();
            let mut required = Vec::new();
            for f in &model.fields {
                let options: Vec<String> = f.options.iter().map(|o| o.to_string()).collect();
                let mut sch = kind_schema(
                    f.kind,
                    (!options.is_empty()).then_some(options.as_slice()),
                    f.to,
                );
                let desc = if f.personal_data {
                    format!("{} (personal data: agents see it masked)", f.description)
                } else {
                    f.description.to_string()
                };
                let mut desc = desc;
                if f.computed {
                    desc.push_str(" (computed when read; read-only)");
                    sch["readOnly"] = J::from(true);
                }
                if f.sequence {
                    desc.push_str(" (a number the system assigns; read-only)");
                    sch["readOnly"] = J::from(true);
                }
                if f.unique && !f.sequence {
                    desc.push_str(" (unique)");
                }
                if f.governed {
                    desc.push_str(" (moves only through the model's actions)");
                }
                sch["description"] = J::from(desc);
                props.insert(f.name.to_string(), sch);
                if f.required && !f.computed {
                    required.push(f.name.to_string());
                }
            }
            let rec =
                json!({"type": "object", "properties": props, "description": model.description});
            // Computed and numbered fields appear in records but are not accepted as input.
            let input_props: serde_json::Map<String, J> = props
                .iter()
                .filter(|(_, v)| v.get("readOnly") != Some(&J::from(true)))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            schemas.insert(name.clone(), rec.clone());
            let op = |id: &str, summary: &str, body: Option<J>, resp: J| {
                let mut o = json!({"operationId": format!("{name}.{id}"), "summary": summary, "tags": [m.name.clone()], "security": sec.clone(), "responses": {"200": resp, "202": ok(json!({"$ref": "#/components/schemas/ApprovalRequired"}), "Proposed; needs a human to approve."), "400": error_responses()["400"].clone(), "401": error_responses()["401"].clone(), "404": error_responses()["404"].clone(), "503": error_responses()["503"].clone()}});
                if let Some(b) = body {
                    o["requestBody"] = b;
                }
                json!({"post": o})
            };
            let rows = ok(
                json!({"type": "object", "properties": {"value": {"type": "array", "items": {"$ref": format!("#/components/schemas/{name}")}}, "denied": {"type": "array", "items": {"type": "string"}}}}),
                "Records.",
            );
            let one = ok(
                json!({"type": "object", "properties": {"value": {"$ref": format!("#/components/schemas/{name}")}, "denied": {"type": "array", "items": {"type": "string"}}}}),
                "A record.",
            );
            let id_body = json_body(
                json!({"type": "object", "required": ["id"], "properties": {"id": {"type": "string"}}}),
            );
            let mut create_schema = json!({"type": "object", "properties": input_props.clone()});
            if !required.is_empty() {
                create_schema["required"] = json!(required);
            }
            paths.insert(
                format!("/api/{name}/list"),
                op("list", &format!("List {name} records"), None, rows.clone()),
            );
            paths.insert(
                format!("/api/{name}/search"),
                op("search", &format!("Search {name} records with filter, order, limit and offset"), Some(json_body(json!({"type": "object", "properties": {"filter": {"type": "array", "items": {"type": "object", "required": ["field", "op"], "properties": {"field": {"type": "string"}, "op": {"type": "string", "enum": ["eq", "ne", "contains", "in", "gt", "gte", "lt", "lte", "is_null"]}, "value": {}}}}, "order": {"type": "array", "items": {"type": "object", "properties": {"field": {"type": "string"}, "dir": {"type": "string", "enum": ["asc", "desc"]}}}}, "limit": {"type": "integer", "minimum": 1, "maximum": 5000}, "offset": {"type": "integer", "minimum": 0}, "lang": {"type": "string", "description": "BCP-47 tag that picks the text collation"}, "aggregate": {"type": "object", "description": "Groups with counts and sums over every matching record instead of rows", "properties": {"group_by": {"type": "array", "maxItems": 2, "items": {"oneOf": [{"type": "string"}, {"type": "object", "required": ["field"], "properties": {"field": {"type": "string"}, "bucket": {"type": "string", "enum": ["day", "week", "month"]}}}]}}, "measures": {"type": "array", "maxItems": 8, "items": {"type": "object", "required": ["op"], "properties": {"op": {"type": "string", "enum": ["count", "sum", "min", "max"]}, "field": {"type": "string"}}}}}}}}))), ok(json!({"type": "object", "properties": {"value": {"type": "object", "properties": {"rows": {"type": "array", "items": {"$ref": format!("#/components/schemas/{name}")}}, "total": {"type": "integer"}, "groups": {"type": "array", "items": {"type": "object", "properties": {"key": {"type": "object"}, "count": {"type": "integer"}, "measures": {"type": "object"}}}}}}}}), "A page of records and the total.")),
            );
            paths.insert(
                format!("/api/{name}/get"),
                op(
                    "get",
                    &format!("Read one {name}"),
                    Some(id_body.clone()),
                    one.clone(),
                ),
            );
            if model.can_create {
                paths.insert(
                    format!("/api/{name}/create"),
                    op(
                        "create",
                        &format!("Create a {name}"),
                        Some(json_body(create_schema)),
                        one.clone(),
                    ),
                );
            }
            if model.can_bulk_create {
                let mut row = json!({"type": "object", "properties": input_props.clone()});
                if !required.is_empty() {
                    row["required"] = json!(required);
                }
                paths.insert(
                    format!("/api/{name}/bulk_create"),
                    op(
                        "bulk_create",
                        &format!("Create up to 1000 {name} records in one call; all or nothing unless mode is partial"),
                        Some(json_body(json!({"type": "object", "properties": {
                            "rows": {"type": "array", "maxItems": 1000, "items": row},
                            "csv": {"type": "string", "description": "Instead of rows: the text of a CSV file with field names as headers (at most 1 MiB)"},
                            "mode": {"type": "string", "enum": ["all_or_nothing", "partial"]},
                            "dry_run": {"type": "boolean"},
                            "idempotency_key": {"type": "string", "maxLength": 128}
                        }}))),
                        ok(json!({"type": "object", "properties": {"value": {"type": "object", "properties": {
                            "ok": {"type": "boolean"},
                            "dry_run": {"type": "boolean"},
                            "mode": {"type": "string"},
                            "total": {"type": "integer"},
                            "created": {"type": "integer"},
                            "valid": {"type": "integer", "description": "With dry_run: rows that would be created"},
                            "failed": {"type": "integer"},
                            "errors": {"type": "array", "items": {"type": "object", "properties": {"row": {"type": "integer"}, "line": {"type": "integer"}, "error": {"type": "string"}}}},
                            "ids": {"type": "array", "items": {"type": ["string", "null"]}},
                            "replayed": {"type": "boolean"}
                        }}}}), "The report of the call."),
                    ),
                );
            }
            if model.can_update {
                let mut upd = json!({"type": "object", "required": ["id"], "properties": input_props.clone()});
                upd["properties"]["id"] = json!({"type": "string"});
                paths.insert(
                    format!("/api/{name}/update"),
                    op(
                        "update",
                        &format!(
                            "Update a {name}; a status field only moves along a declared action"
                        ),
                        Some(json_body(upd)),
                        one.clone(),
                    ),
                );
            }
            if model.can_delete {
                paths.insert(
                    format!("/api/{name}/delete"),
                    op(
                        "delete",
                        &format!("Delete a {name} (agents only propose)"),
                        Some(id_body.clone()),
                        ok(json!({"type": "object"}), "Deleted."),
                    ),
                );
            }
            for a in &model.actions {
                let cmd = a.id.rsplit('.').next().unwrap_or(&a.id);
                let mut o = op(cmd, a.description, Some(id_body.clone()), one.clone());
                if a.needs_approval {
                    o["post"]["description"] = J::from(
                        "Irreversible or money-moving: an agent token only proposes this and a human approves.",
                    );
                }
                paths.insert(format!("/api/{name}/{cmd}"), o);
            }
        }
    }
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "obop instance API",
            "version": obop_types::CORE_VERSION,
            "description": "Every model has the same generated commands: list, search, get, create, update, delete and its own actions, all POST with a JSON body. Personal and financial fields are masked for agent tokens; irreversible steps answer 202 with a plan for a human to approve. Errors are JSON with error, code and hint. Sign in (or present a token) to see the commands your token may use; this document lists only those."
        },
        "servers": [{"url": base}],
        "x-mcp": crate::mcp::x_mcp(base),
        "paths": paths,
        "components": {"securitySchemes": bearer(), "schemas": schemas}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_control_plane_document_is_valid_looking_openapi() {
        let d = platform("https://x.test");
        assert_eq!(d["openapi"], "3.1.0");
        assert!(d["paths"]["/api/platform/instances"]["post"]["operationId"].is_string());
        // Every $ref resolves to a schema we define.
        let text = d.to_string();
        for r in ["Error", "Instance"] {
            assert!(
                text.contains(&format!("#/components/schemas/{r}"))
                    && d["components"]["schemas"][r].is_object()
            );
        }
    }

    #[test]
    fn kinds_map_to_json_schema() {
        assert_eq!(kind_schema("money", None, None)["type"], "integer");
        assert_eq!(kind_schema("date", None, None)["format"], "date");
        let opts = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            kind_schema("select", Some(&opts), None)["enum"],
            json!(["a", "b"])
        );
        assert!(
            kind_schema("ref", None, Some("party"))["description"]
                .as_str()
                .unwrap()
                .contains("party")
        );
    }
}
