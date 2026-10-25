//! Tool definitions generated from the registry for one caller (D31): one tool per visible
//! command, named `<model>__<command>`, with a JSON Schema input, an output schema and
//! annotations. Both transports list tools through this module, so they cannot diverge.

use obop_commands::{CommandInfo, Dispatcher, FieldInfo, ModelInfo, Principal, Surface, names};
use serde_json::{Value as J, json};
use std::collections::HashMap;

/// The extra read-only tool that returns what `GET /api/meta` returns.
pub const META_TOOL: &str = "obop__meta";

const OPS: [&str; 9] = [
    "eq", "ne", "contains", "in", "gt", "gte", "lt", "lte", "is_null",
];

/// JSON Schema of one field kind (D47).
pub fn field_schema(f: &FieldInfo) -> J {
    let mut s = match f.kind {
        "number" => json!({"type": "integer"}),
        "money" => json!({"type": "integer"}),
        "bool" => json!({"type": "boolean"}),
        "date" => json!({"type": "string", "format": "date"}),
        "datetime" => json!({"type": "string", "format": "date-time"}),
        "decimal" => json!({"type": "string", "pattern": f.decimal_pattern()}),
        "select" => json!({"type": "string", "enum": f.options}),
        _ => json!({"type": "string"}),
    };
    let mut desc = f.description.to_string();
    match f.kind {
        "money" => desc.push_str(" (integer, minor currency units such as cents; never a float)"),
        "date" => desc.push_str(" (calendar date, YYYY-MM-DD)"),
        "datetime" => desc.push_str(
            " (an instant: RFC 3339 with an offset or Z, such as 2026-03-29T01:30:00Z; microsecond precision; returned in UTC as YYYY-MM-DDTHH:MM:SS.ffffffZ)",
        ),
        "decimal" => desc.push_str(&format!(
            " (an exact decimal sent and returned as a STRING such as \"{}\", never a JSON number; at most {} digits, exactly {} after the point in answers)",
            f.decimal_example().unwrap_or_default(),
            f.precision.unwrap_or(0),
            f.scale.unwrap_or(0)
        )),
        "ref" => desc.push_str(&format!(
            " (the id of a {} record)",
            f.to.unwrap_or("referenced")
        )),
        "image" | "file" => desc.push_str(
            " (a blob id: 64 lowercase hex characters, returned by an upload; you receive ids and metadata, never the bytes)",
        ),
        _ => {}
    }
    if f.personal_data {
        desc.push_str(
            " (personal data: agents read it as a mask token and may send the token back)",
        );
    }
    if f.computed {
        desc.push_str(" (computed from other data when read; read-only, never send it)");
    }
    if f.sequence {
        desc.push_str(" (a number the system assigns; read-only, never send it)");
    }
    if f.unique && !f.sequence {
        desc.push_str(" (must be unique: no other record may have the same value)");
    }
    if f.governed {
        desc.push_str(" (moves only through the model's actions)");
    }
    s["description"] = J::from(desc);
    s
}

fn id_property(model: &str) -> J {
    json!({"type": "string", "description": format!("The id of the {model} record.")})
}

fn query_properties() -> J {
    json!({
        "filter": {
            "type": "array",
            "description": "Conditions, all of which must hold. Fields you cannot read in clear are refused.",
            "items": {
                "type": "object",
                "required": ["field", "op"],
                "properties": {
                    "field": {"type": "string"},
                    "op": {"type": "string", "enum": OPS},
                    "value": {"description": "Omit for is_null; an array for in. Decimals and instants are strings; a decimal compares as a number, an instant as a point in time whatever offset it is written with."},
                    "bucket": {"type": "string", "enum": ["day", "week", "iso_week", "month", "year"], "description": "On a date or datetime field: compare the period it falls in; value is then the period label (2026-04, 2026-W15, 2026-04-01). This is the explicit date-in-zone conversion."},
                    "zone": {"type": "string", "description": "With bucket on a datetime: an IANA time zone name (default: the platform zone)."}
                }
            }
        },
        "order": {
            "type": "array",
            "items": {
                "type": "object",
                "required": ["field"],
                "properties": {
                    "field": {"type": "string"},
                    "dir": {"type": "string", "enum": ["asc", "desc"]}
                }
            }
        },
        "limit": {"type": "integer", "minimum": 1, "maximum": 5000, "description": "Page size; ask for a small page."},
        "offset": {"type": "integer", "minimum": 0}
    })
}

/// The `aggregate` input of `search`: groups and measures computed on the server (D4).
fn aggregate_schema() -> J {
    json!({
        "type": "object",
        "description": "Instead of rows, return groups with counts and sums over every matching record. Takes filter (not order, limit or offset).",
        "properties": {
            "group_by": {
                "type": "array",
                "maxItems": 2,
                "items": {"oneOf": [
                    {"type": "string", "description": "A field name."},
                    {"type": "object", "required": ["field"], "properties": {
                        "field": {"type": "string"},
                        "bucket": {"type": "string", "enum": ["day", "week", "iso_week", "month", "year"], "description": "Required for date and datetime fields. Weeks start on Monday; iso_week is YYYY-Www."},
                        "zone": {"type": "string", "description": "For a datetime: an IANA time zone name (default: the platform zone)."}
                    }}
                ]}
            },
            "measures": {
                "type": "array",
                "maxItems": 8,
                "items": {"type": "object", "required": ["op"], "properties": {
                    "op": {"type": "string", "enum": ["count", "sum", "min", "max", "avg"]},
                    "field": {"type": "string", "description": "A number, money or decimal field (avg: decimal only); omit for count. A decimal measure is a string."},
                    "scale": {"type": "integer", "minimum": 0, "maximum": 12, "description": "avg only: digits after the point (default: the field's)."},
                    "round": {"type": "string", "enum": ["half_even", "half_up", "down"], "description": "avg only (default half_even)."}
                }}
            }
        }
    })
}

fn object(properties: J, required: &[String]) -> J {
    let mut s = json!({"type": "object", "properties": properties});
    if !required.is_empty() {
        s["required"] = json!(required);
    }
    s
}

fn field_properties(model: &ModelInfo, skip: &[&str]) -> serde_json::Map<String, J> {
    model
        .fields
        .iter()
        // Computed and numbered fields are outputs only.
        .filter(|f| !skip.contains(&f.name) && !f.computed && !f.sequence)
        .map(|f| (f.name.to_string(), field_schema(f)))
        .collect()
}

/// Input schema of command `cmd` of `model`.
pub fn input_schema(model: &ModelInfo, cmd: &str) -> J {
    // Select fields an action moves are governed (D48): they change only through the action.
    let governed: Vec<&str> = model
        .actions
        .iter()
        .filter_map(|a| a.transition.map(|t| t.field))
        .collect();
    match cmd {
        "get" | "delete" => object(json!({"id": id_property(&model.name)}), &["id".to_string()]),
        "list" => object(query_properties(), &[]),
        "search" => {
            let mut props = query_properties();
            props["lang"] = json!({"type": "string", "description": "BCP-47 tag (sv, de-AT) choosing how text is ordered; default is the language-neutral order."});
            props["aggregate"] = aggregate_schema();
            object(props, &[])
        }
        "create" => {
            let props = field_properties(model, &governed);
            let required: Vec<String> = model
                .fields
                .iter()
                .filter(|f| f.required && !governed.contains(&f.name) && !f.computed)
                .map(|f| f.name.to_string())
                .collect();
            object(J::Object(props), &required)
        }
        "bulk_create" => {
            let required: Vec<String> = model
                .fields
                .iter()
                .filter(|f| f.required && !governed.contains(&f.name) && !f.computed)
                .map(|f| f.name.to_string())
                .collect();
            // A decimal that rounds on import takes any number of fractional digits in a row.
            let mut row_props = field_properties(model, &governed);
            for f in model.fields.iter().filter(|f| f.kind == "decimal" && f.round.is_some()) {
                if let Some(p) = row_props.get_mut(f.name) {
                    p["pattern"] = json!(f.decimal_import_pattern());
                }
            }
            let row = object(J::Object(row_props), &required);
            object(
                json!({
                    "rows": {
                        "type": "array",
                        "maxItems": 1000,
                        "items": row,
                        "description": "The records to create, one object per row with the fields of create. At most 1000. Give rows or csv."
                    },
                    "csv": {
                        "type": "string",
                        "description": "Instead of rows: the text of a CSV file (UTF-8, comma or semicolon, a header line of field names, at most 1 MiB). Number fields take whole numbers and money minor units; a decimal is written with a point (12.50), a datetime with an offset or Z (2026-03-29T01:30:00Z), and a decimal field that rounds on import takes extra digits; true/false for yes/no fields, the id of the record for references. An empty cell leaves the field out."
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["all_or_nothing", "partial"],
                        "description": "all_or_nothing (default): one bad row creates nothing. partial: the good rows are created and the bad ones reported by row."
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "Check every row, number nothing for good and write nothing; the answer lists the problems."
                    },
                    "idempotency_key": {
                        "type": "string",
                        "maxLength": 128,
                        "description": "Any text you make up and reuse when you retry the same call: the retry returns the first answer instead of creating the rows again (kept 24 hours). A key reused for different rows is refused."
                    }
                }),
                &[],
            )
        }
        "update" => {
            let mut props = field_properties(model, &[]);
            props.insert("id".into(), id_property(&model.name));
            object(J::Object(props), &["id".to_string()])
        }
        _ => {
            // An action: the record to act on, and any fields the action needs filled.
            let mut props = field_properties(model, &governed);
            props.insert("id".into(), id_property(&model.name));
            object(J::Object(props), &["id".to_string()])
        }
    }
}

/// Output schema: every result is an object so it can be structured content. `value` is typed
/// by shape only, because masking replaces some fields with strings.
pub fn output_schema(cmd: &str) -> J {
    let value = match cmd {
        "list" => json!({"type": "array", "items": {"type": "object"}}),
        "search" => json!({
            "type": "object",
            "properties": {
                "rows": {"type": "array", "items": {"type": "object"}},
                "total": {"type": "integer", "description": "Matches before limit and offset."},
                "groups": {"type": "array", "description": "With aggregate: {key, count, measures} per group.", "items": {"type": "object"}}
            }
        }),
        _ => json!({"type": "object"}),
    };
    json!({
        "type": "object",
        "properties": {
            "status": {"type": "string", "enum": ["ok", "approval_required"]},
            "value": value,
            "denied": {"type": "array", "items": {"type": "string"}, "description": "Fields withheld from this caller (model.field)."},
            "plan_id": {"type": "string", "description": "With approval_required: the plan a human must approve."},
            "instruction": {"type": "string"}
        },
        "required": ["status"]
    })
}

fn annotations(cmd: &str, model: &str) -> J {
    let read_only = matches!(cmd, "get" | "list" | "search");
    json!({
        "title": format!("{model} {cmd}"),
        "readOnlyHint": read_only,
        "destructiveHint": cmd == "delete",
        "idempotentHint": read_only || cmd == "update",
        "openWorldHint": false,
    })
}

fn describe(c: &CommandInfo, cmd: &str) -> String {
    let mut d = c.description.to_string();
    if c.needs_approval {
        d.push_str(
            " Needs human approval: called with an agent token it does not run, it returns status \"approval_required\" and a plan_id; ask a human to approve the plan.",
        );
    }
    if cmd == "update" {
        d.push_str(
            " A status field only moves through its action; moving it may need approval too.",
        );
    }
    d
}

/// Everything `tools/list` shows this caller, sorted by name.
pub fn tools(d: &Dispatcher, p: &Principal) -> Vec<J> {
    let meta = d.meta(Surface::Mcp, p);
    let models: HashMap<&str, &ModelInfo> = meta
        .iter()
        .flat_map(|m| m.models.iter())
        .map(|m| (m.name.as_str(), m))
        .collect();
    let mut out = vec![json!({
        "name": META_TOOL,
        "title": "obop meta",
        "description": "Describes this instance for the caller: modules, models, the fields the caller may read (with kind, required, select options and ref targets), and actions. Start here.",
        "inputSchema": {"type": "object", "properties": {}},
        "annotations": {"title": "obop meta", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false},
    })];
    for c in d.catalog(Surface::Mcp, p) {
        let Some((model, cmd)) = c.id.split_once('.') else {
            continue;
        };
        let input = match models.get(model) {
            Some(m) => input_schema(m, cmd),
            None => {
                json!({"type": "object", "additionalProperties": true, "description": "The same JSON object as the REST request body of this command."})
            }
        };
        let ann = annotations(cmd, model);
        out.push(json!({
            "name": names::to_mcp(&c.id),
            "title": ann["title"],
            "description": describe(&c, cmd),
            "inputSchema": input,
            "outputSchema": output_schema(cmd),
            "annotations": ann,
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(kind: &'static str) -> FieldInfo {
        FieldInfo {
            name: "x",
            description: "a field",
            class: "none",
            personal_data: false,
            kind,
            required: false,
            options: vec!["a", "b"],
            to: Some("party"),
            max_bytes: None,
            precision: None,
            scale: None,
            round: None,
            labels: Default::default(),
            descriptions: Default::default(),
            option_labels: Default::default(),
            governed: false,
            computed: false,
            unique: false,
            sequence: false,
        }
    }

    #[test]
    fn each_kind_maps_to_json_schema() {
        assert_eq!(field_schema(&field("money"))["type"], "integer");
        assert_eq!(field_schema(&field("number"))["type"], "integer");
        assert_eq!(field_schema(&field("bool"))["type"], "boolean");
        assert_eq!(field_schema(&field("date"))["format"], "date");
        assert_eq!(field_schema(&field("select"))["enum"], json!(["a", "b"]));
        let r = field_schema(&field("ref"));
        assert_eq!(r["type"], "string");
        assert!(r["description"].as_str().unwrap().contains("party"));
        assert_eq!(field_schema(&field("text"))["type"], "string");
        assert_eq!(field_schema(&field("longtext"))["type"], "string");
        let mut pd = field("text");
        pd.personal_data = true;
        assert!(
            field_schema(&pd)["description"]
                .as_str()
                .unwrap()
                .contains("mask")
        );
    }

    fn model() -> ModelInfo {
        let f = |name: &'static str, kind: &'static str, required: bool| FieldInfo {
            name,
            description: "d",
            class: "none",
            personal_data: false,
            kind,
            required,
            options: vec!["draft", "posted"],
            to: Some("party"),
            max_bytes: None,
            precision: (kind == "decimal").then_some(10),
            scale: (kind == "decimal").then_some(2),
            round: None,
            labels: Default::default(),
            descriptions: Default::default(),
            option_labels: Default::default(),
            governed: false,
            computed: false,
            unique: false,
            sequence: false,
        };
        ModelInfo {
            name: "doc".into(),
            description: "a doc",
            fields: vec![
                f("title", "text", true),
                f("total", "money", true),
                f("due", "date", false),
                f("paid", "bool", false),
                f("state", "select", true),
                f("customer", "ref", false),
            ],
            actions: vec![CommandInfo {
                id: "doc.post".into(),
                description: "post it",
                permission_group: "x",
                needs_approval: true,
                transition: Some(obop_registry::Transition {
                    field: "state",
                    from: Some("draft"),
                    to: "posted",
                }),
                labels: Default::default(),
                descriptions: Default::default(),
            }],
            can_create: true,
            can_bulk_create: true,
            can_update: true,
            can_delete: true,
            labels: Default::default(),
            descriptions: Default::default(),
            access: None,
        }
    }

    #[test]
    fn input_schemas_per_command() {
        let m = model();
        let create = input_schema(&m, "create");
        assert_eq!(create["properties"]["due"]["format"], "date");
        assert_eq!(create["properties"]["paid"]["type"], "boolean");
        assert_eq!(create["properties"]["total"]["type"], "integer");
        assert_eq!(create["properties"]["customer"]["type"], "string");
        // The governed field is not set on create, and so is not required there.
        assert!(create["properties"].get("state").is_none());
        assert_eq!(create["required"], json!(["title", "total"]));
        let update = input_schema(&m, "update");
        assert_eq!(update["required"], json!(["id"]));
        assert_eq!(
            update["properties"]["state"]["enum"],
            json!(["draft", "posted"])
        );
        for c in ["get", "delete", "post"] {
            let s = input_schema(&m, c);
            assert_eq!(s["required"], json!(["id"]), "{c}");
            assert_eq!(s["properties"]["id"]["type"], "string");
        }
        assert!(
            input_schema(&m, "post")["properties"]
                .get("state")
                .is_none()
        );
        for c in ["list", "search"] {
            let s = input_schema(&m, c);
            assert!(s.get("required").is_none());
            assert_eq!(s["properties"]["limit"]["maximum"], 5000);
            assert_eq!(
                s["properties"]["order"]["items"]["properties"]["dir"]["enum"],
                json!(["asc", "desc"])
            );
        }
    }

    #[test]
    fn output_schemas_are_objects() {
        for c in ["list", "search", "get", "post"] {
            assert_eq!(output_schema(c)["type"], "object");
        }
        assert_eq!(
            output_schema("list")["properties"]["value"]["type"],
            "array"
        );
        assert!(output_schema("search")["properties"]["value"]["properties"]["total"].is_object());
    }
}
