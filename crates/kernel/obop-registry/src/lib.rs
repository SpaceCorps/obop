//! Layer 3 registry: model, field and command metadata, and the D41 install gate.

use obop_types::DataClass;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

/// Registry metadata uses `&'static str`. Modules installed at runtime build their names from
/// manifests, so those strings are interned: each distinct string is leaked once, which bounds
/// the cost at the number of distinct names ever installed.
pub fn intern(s: &str) -> &'static str {
    static POOL: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut pool = POOL.get_or_init(Default::default).lock().unwrap();
    if let Some(found) = pool.get(s) {
        return found;
    }
    let leaked: &'static str = Box::leak(s.to_owned().into_boxed_str());
    pool.insert(leaked);
    leaked
}

/// The type of a field (D47). Serialised lowercase (`FieldKind::name`) in manifests and in
/// `/api/meta`. Cheap to clone: options are shared, the target model name is interned.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FieldKind {
    /// JSON string.
    #[default]
    Text,
    /// JSON string, multi-line.
    LongText,
    /// JSON integer.
    Number,
    /// JSON integer in minor currency units, never a float.
    Money,
    /// JSON string `YYYY-MM-DD`, a real calendar date.
    Date,
    /// JSON boolean.
    Bool,
    /// JSON string, one of the options (in status order for a `state` field).
    Select(Arc<[&'static str]>),
    /// JSON string: the id of an existing record of the named model.
    Ref(&'static str),
}

impl FieldKind {
    /// The lowercase name used in manifests and `/api/meta`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::LongText => "longtext",
            Self::Number => "number",
            Self::Money => "money",
            Self::Date => "date",
            Self::Bool => "bool",
            Self::Select(_) => "select",
            Self::Ref(_) => "ref",
        }
    }

    /// Whether `name` is one of the kind names.
    pub fn is_known(name: &str) -> bool {
        matches!(
            name,
            "text" | "longtext" | "number" | "money" | "date" | "bool" | "select" | "ref"
        )
    }

    /// The options of a `select`, empty for every other kind.
    pub fn options(&self) -> &[&'static str] {
        match self {
            Self::Select(o) => o,
            _ => &[],
        }
    }

    /// The target model of a `ref`.
    pub fn target(&self) -> Option<&'static str> {
        match self {
            Self::Ref(t) => Some(t),
            _ => None,
        }
    }

    /// What a valid value is, for "`model.field` must be ..." messages.
    pub fn describe(&self) -> String {
        match self {
            Self::Text | Self::LongText => "a string".into(),
            Self::Number => "an integer".into(),
            Self::Money => "an integer in minor currency units".into(),
            Self::Date => "a real calendar date as YYYY-MM-DD".into(),
            Self::Bool => "true or false".into(),
            Self::Select(o) => format!("one of: {}", o.join(", ")),
            Self::Ref(t) => format!("the id of an existing {t} record"),
        }
    }
}

/// Whether `s` is a real calendar date written `YYYY-MM-DD` (leap years included).
pub fn is_valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = s.get(r)?;
        if part.bytes().all(|c| c.is_ascii_digit()) {
            part.parse().ok()
        } else {
            None
        }
    };
    let (Some(y), Some(m), Some(d)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    y >= 1 && (1..=days).contains(&d)
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: &'static str,
    pub description: &'static str,
    pub class: Option<DataClass>,
    pub permission_group: &'static str,
    pub personal_data: bool,
    pub kind: FieldKind,
    pub required: bool,
}

#[derive(Debug, Clone)]
pub struct CommandDef {
    pub name: &'static str,
    pub description: &'static str,
    pub permission_group: &'static str,
    /// Agents may only propose this command; a human approves it (D18, D42). Posting, paying
    /// and destructive commands set this.
    pub needs_approval: bool,
    /// Exposed through generated REST, MCP and CLI (D41: all or none, so this is not optional).
    pub surfaces: Surfaces,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surfaces {
    pub rest: bool,
    pub mcp: bool,
    pub cli: bool,
}

impl CommandDef {
    pub fn new(
        name: &'static str,
        description: &'static str,
        permission_group: &'static str,
    ) -> Self {
        Self {
            name,
            description,
            permission_group,
            needs_approval: false,
            surfaces: Surfaces::ALL,
        }
    }

    pub fn approval(mut self) -> Self {
        self.needs_approval = true;
        self
    }
}

impl FieldDef {
    pub fn new(
        name: &'static str,
        description: &'static str,
        class: DataClass,
        personal_data: bool,
        permission_group: &'static str,
    ) -> Self {
        Self {
            name,
            description,
            class: Some(class),
            permission_group,
            personal_data,
            kind: FieldKind::Text,
            required: false,
        }
    }

    pub fn kind(mut self, kind: FieldKind) -> Self {
        self.kind = kind;
        self
    }

    /// A `select` with these options (interned, so literals and manifest strings both work).
    pub fn select(self, options: &[&str]) -> Self {
        let o: Vec<&'static str> = options.iter().map(|x| intern(x)).collect();
        self.kind(FieldKind::Select(o.into()))
    }

    /// A `ref` to records of `model`.
    pub fn reference(self, model: &str) -> Self {
        self.kind(FieldKind::Ref(intern(model)))
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }
}

impl Surfaces {
    pub const ALL: Self = Self {
        rest: true,
        mcp: true,
        cli: true,
    };

    fn is_all_or_none(self) -> bool {
        (self.rest && self.mcp && self.cli) || !(self.rest || self.mcp || self.cli)
    }
}

#[derive(Debug, Clone)]
pub struct ModelDef {
    pub name: &'static str,
    pub description: &'static str,
    /// Group that may call the generated `get` and `list` commands.
    pub read_group: &'static str,
    /// Group that may call the generated `create` and `update` commands.
    pub write_group: &'static str,
    pub fields: Vec<FieldDef>,
    /// Custom commands beyond the generated CRUD set.
    pub commands: Vec<CommandDef>,
}

impl ModelDef {
    /// Generated `get`, `list`, `create` and `update`, followed by the custom commands. The
    /// REST, MCP and CLI surfaces are derived from this list (D29, D41).
    pub fn all_commands(&self) -> Vec<CommandDef> {
        let crud = CommandDef::new;
        let mut v = vec![
            crud("get", "Read one record by id", self.read_group),
            crud("list", "List records", self.read_group),
            crud("create", "Create a record", self.write_group),
            crud("update", "Update fields of a record", self.write_group),
        ];
        v.extend(self.commands.iter().cloned());
        v
    }
}

#[derive(Debug, Clone, Default)]
pub struct Module {
    pub id: String,
    /// Name shown in the app launcher.
    pub name: String,
    pub models: Vec<ModelDef>,
    /// Network destinations the module declares in `obop.toml` (D41).
    pub network_destinations: Vec<String>,
}

impl Module {
    /// Every permission group the module refers to, sorted and de-duplicated.
    pub fn permission_groups(&self) -> Vec<&'static str> {
        let mut g: Vec<&'static str> = self
            .models
            .iter()
            .flat_map(|m| {
                [m.read_group, m.write_group]
                    .into_iter()
                    .chain(m.fields.iter().map(|f| f.permission_group))
                    .chain(m.commands.iter().map(|c| c.permission_group))
            })
            .collect();
        g.sort_unstable();
        g.dedup();
        g
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub path: String,
    pub problem: String,
}

/// The D41 install gate: every model, field and command carries a description, a permission
/// group and a data class; personal data cannot be classed `none`; surfaces are all or none.
pub fn check_module(m: &Module) -> Vec<Violation> {
    let mut v = Vec::new();
    let mut bad = |path: String, problem: &str| {
        v.push(Violation {
            path,
            problem: problem.into(),
        })
    };
    for model in &m.models {
        let mp = format!("{}.{}", m.id, model.name);
        if model.description.trim().is_empty() {
            bad(mp.clone(), "model has no description");
        }
        if model.read_group.trim().is_empty() || model.write_group.trim().is_empty() {
            bad(mp.clone(), "model has no read or write permission group");
        }
        for f in &model.fields {
            let fp = format!("{mp}.{}", f.name);
            if f.description.trim().is_empty() {
                bad(fp.clone(), "field has no description");
            }
            if f.permission_group.trim().is_empty() {
                bad(fp.clone(), "field has no permission group");
            }
            match f.class {
                None => bad(
                    fp.clone(),
                    "field has no data class (use `none` explicitly if not personal)",
                ),
                Some(DataClass::None) if f.personal_data => {
                    bad(fp.clone(), "personal data cannot be classed `none`")
                }
                _ => {}
            }
            if f.name == "id" {
                bad(
                    fp.clone(),
                    "a field cannot be named `id`; it is the record id",
                );
            }
            match &f.kind {
                FieldKind::Select(o) if o.is_empty() => {
                    bad(fp.clone(), "select field has no options")
                }
                FieldKind::Select(o) => {
                    let mut seen = HashSet::new();
                    if o.iter().any(|x| x.is_empty() || !seen.insert(*x)) {
                        bad(fp.clone(), "select options must be non-empty and unique");
                    }
                }
                FieldKind::Ref(t) if t.trim().is_empty() => {
                    bad(fp.clone(), "ref field has no target model")
                }
                _ => {}
            }
        }
        for c in &model.all_commands() {
            let cp = format!("{mp}::{}", c.name);
            if c.description.trim().is_empty() {
                bad(cp.clone(), "command has no description");
            }
            if c.permission_group.trim().is_empty() {
                bad(cp.clone(), "command has no permission group");
            }
            if !c.surfaces.is_all_or_none() {
                bad(cp, "command must be on REST, MCP and CLI or on none (D41)");
            }
        }
    }
    v
}

#[cfg(test)]
mod intern_tests {
    #[test]
    fn same_string_same_pointer() {
        assert!(std::ptr::eq(
            super::intern("abc"),
            super::intern(&String::from("abc"))
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(class: Option<DataClass>, personal: bool) -> FieldDef {
        FieldDef {
            name: "email",
            description: "Email",
            class,
            permission_group: "base.user",
            personal_data: personal,
            kind: FieldKind::Text,
            required: false,
        }
    }

    fn module(f: FieldDef) -> Module {
        Module {
            id: "t".into(),
            name: "T".into(),
            models: vec![ModelDef {
                name: "party",
                description: "Party",
                read_group: "g",
                write_group: "g",
                fields: vec![f],
                commands: vec![],
            }],
            network_destinations: vec![],
        }
    }

    #[test]
    fn complete_module_passes() {
        assert!(check_module(&module(field(Some(DataClass::Internal), true))).is_empty());
    }

    #[test]
    fn missing_class_fails() {
        let v = check_module(&module(field(None, false)));
        assert_eq!(v.len(), 1);
        assert!(v[0].problem.contains("no data class"));
    }

    #[test]
    fn personal_data_cannot_be_none() {
        assert_eq!(
            check_module(&module(field(Some(DataClass::None), true))).len(),
            1
        );
    }

    #[test]
    fn partial_surfaces_fail() {
        let mut m = module(field(Some(DataClass::None), false));
        m.models[0].commands.push(CommandDef {
            name: "merge",
            description: "Merge",
            permission_group: "base.admin",
            needs_approval: false,
            surfaces: Surfaces {
                rest: true,
                mcp: false,
                cli: true,
            },
        });
        assert_eq!(check_module(&m).len(), 1);
    }

    fn gate(f: FieldDef) -> Vec<Violation> {
        check_module(&module(f))
    }

    fn ok_field() -> FieldDef {
        field(Some(DataClass::Internal), true)
    }

    #[test]
    fn typed_fields_pass_when_well_formed() {
        assert!(gate(ok_field().select(&["a", "b"]).required()).is_empty());
        assert!(gate(ok_field().reference("party")).is_empty());
    }

    #[test]
    fn select_needs_unique_options() {
        assert!(
            gate(ok_field().select(&[]))[0]
                .problem
                .contains("no options")
        );
        assert!(
            gate(ok_field().select(&["a", "a"]))[0]
                .problem
                .contains("unique")
        );
    }

    #[test]
    fn ref_needs_a_target() {
        assert!(gate(ok_field().reference(""))[0].problem.contains("target"));
    }

    #[test]
    fn a_field_cannot_be_named_id() {
        let mut f = ok_field();
        f.name = "id";
        assert!(gate(f)[0].problem.contains("`id`"));
    }

    #[test]
    fn kind_names_and_descriptions() {
        assert_eq!(FieldKind::Money.name(), "money");
        assert_eq!(FieldKind::default(), FieldKind::Text);
        let f = ok_field().select(&["x", "y"]);
        assert_eq!(f.kind.describe(), "one of: x, y");
        assert_eq!(f.kind.options(), ["x", "y"]);
        assert_eq!(ok_field().reference("party").kind.target(), Some("party"));
        assert!(FieldKind::is_known("longtext") && !FieldKind::is_known("float"));
    }

    #[test]
    fn dates_are_checked_against_the_calendar() {
        for ok in ["2024-02-29", "2000-02-29", "2023-12-31", "0001-01-01"] {
            assert!(is_valid_date(ok), "{ok}");
        }
        for bad in [
            "2023-02-29",
            "1900-02-29",
            "2024-13-01",
            "2024-00-10",
            "2024-04-31",
            "2024-1-01",
            "2024-01-001",
            "20240101",
            "2024/01/01",
            "+024-01-01",
            "0000-01-01",
            "",
        ] {
            assert!(!is_valid_date(bad), "{bad}");
        }
    }
}
