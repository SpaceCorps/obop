//! Model rules (D88, first slice) and `[model.no_overlap]` in packages: what the parser and the
//! install gate accept and refuse, and the rules of the bundled packages with their translations.

use crate::manifest::Manifest;
use crate::{Bundled, Marketplace};
use obop_registry::{Applies, check_module};

const HEAD: &str = "[module]\nid = \"mr\"\nversion = \"0.1.0\"\nname = \"Mr\"\ndescription = \"d\"\nlicense = \"MIT\"\nrequires_core = \"^0.2.2\"\n\n[[model]]\nname = \"mr_slot\"\ndescription = \"d\"\nread_group = \"mr.user\"\nwrite_group = \"mr.user\"\n";

const FIELDS: &str = r#"
[[model.field]]
name = "room"
description = "d"
class = "none"

[[model.field]]
name = "day"
description = "d"
class = "none"
kind = "date"

[[model.field]]
name = "start"
description = "d"
class = "none"
kind = "number"

[[model.field]]
name = "end"
description = "d"
class = "none"
kind = "number"

[[model.field]]
name = "state"
description = "d"
class = "none"
kind = "select"
options = ["open", "cancelled"]
"#;

/// The error of parsing, building and gating a package, or `None` when it is fine.
fn problem(extra: &str) -> Option<String> {
    let text = format!("{HEAD}{extra}{FIELDS}");
    let m = match Manifest::parse(&text) {
        Ok(m) => m,
        Err(e) => return Some(e),
    };
    match m.to_module() {
        Ok(module) => {
            let v = check_module(&module);
            (!v.is_empty()).then(|| format!("{v:?}"))
        }
        Err(e) => Some(e),
    }
}

const RULE: &str = "\n[[model.rule]]\nkey = \"ends_after_start\"\nexpr = \"end >= start\"\nmessage = \"The slot ends before it starts\"\n";
const OVERLAP: &str = "\n[model.no_overlap]\nrule = \"no_double_booking\"\nkey = [\"room\", \"day\"]\nstart = \"start\"\nend = \"end\"\nignore_states = [\"cancelled\"]\nmessage = \"The room is taken at that time\"\n";

#[test]
fn a_package_declares_rules_and_a_no_overlap_rule() {
    assert_eq!(problem(&format!("{RULE}{OVERLAP}")), None);
    let text = format!("{HEAD}{RULE}{OVERLAP}{FIELDS}");
    let module = Manifest::parse(&text).unwrap().to_module().unwrap();
    let def = &module.models[0];
    assert_eq!(def.rule_keys(), ["ends_after_start", "no_double_booking"]);
    assert_eq!(def.model_rules()[0].applies, Applies::Touched);
    let n = def.no_overlap().unwrap();
    assert_eq!((n.start, n.end, n.state_field), ("start", "end", "state"));
}

#[test]
fn the_parser_and_the_gate_refuse_a_bad_rule() {
    let cases = [
        (
            RULE.replace("message = ", "applies = \"frozen\"\nmessage = "),
            "applies must be",
        ),
        (RULE.replace("end >= start", "end >= "), "ends_after_start"),
        (RULE.replace("end >= start", "end + start"), "true/false"),
        (
            RULE.replace("end >= start", "ghost > 1"),
            "no field `ghost`",
        ),
        (
            RULE.replace("key = ", "colour = \"red\"\nkey = "),
            "unknown field",
        ),
        (
            OVERLAP.replace("start = \"start\"", "start = \"room\""),
            "number, date or datetime",
        ),
        (
            OVERLAP.replace("[\"cancelled\"]", "[\"gone\"]"),
            "no option \"gone\"",
        ),
        // `room` is text, not a reference to a record that holds a capacity.
        (
            OVERLAP.replace("message = ", "capacity = \"room.seats\"\nmessage = "),
            "`capacity` must be",
        ),
    ];
    for (extra, want) in cases {
        let p = problem(&extra).unwrap_or_default();
        assert!(p.contains(want), "{want:?} not in {p:?} for {extra}");
    }
    let public = format!("{RULE}\n[model.public]\nsubmit = true\nsubmit_fields = [\"room\"]\n");
    let p = problem(&public).unwrap_or_default();
    assert!(p.contains("public submissions"), "{p}");
}

#[test]
fn the_bundled_packages_hold_their_times_with_rules() {
    let want: &[(&str, &str, &[&str])] = &[
        (
            "appointments",
            "appointments_appointment",
            &["ends_after_start", "no_double_booking"],
        ),
        (
            "planning",
            "planning_shift",
            &["ends_after_start", "no_double_shift"],
        ),
        (
            "accounting",
            "accounting_fiscal_year",
            &["ends_after_start"],
        ),
        ("accounting", "accounting_period", &["ends_after_start"]),
    ];
    for (id, model, keys) in want {
        let text = Bundled.fetch(id).unwrap();
        let module = Manifest::parse(&text).unwrap().to_module().unwrap();
        assert_eq!(check_module(&module), vec![], "{id}");
        let def = module.models.iter().find(|m| m.name == *model).unwrap();
        assert_eq!(def.rule_keys(), *keys, "{model}");
        for key in *keys {
            for lang in ["de", "fr", "pl", "fi", "sv", "es"] {
                assert!(
                    module.translations.rule_message(model, key, lang).is_some(),
                    "{model}.rule.{key} has no {lang} message"
                );
            }
        }
        if let Some(r) = def.model_rules().first() {
            assert!(
                r.failure_in(Some("de"))
                    .ends_with("(rule ends_after_start failed)"),
                "{}",
                r.failure_in(Some("de"))
            );
            assert_ne!(
                r.failure_in(Some("de")),
                r.failure_in(None),
                "{model}: German"
            );
        }
    }
}

/// The texts people and agents read say what the rules hold: the overlap is prevented, and the
/// field that takes only an IBAN is described as one, in English and in every label file.
#[test]
fn the_descriptions_say_what_the_rules_hold() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../packages");
    let read = |p: &str| std::fs::read_to_string(format!("{root}/{p}")).unwrap();
    let appt = read("appointments.toml");
    assert!(
        !appt.contains("not prevented"),
        "appointments says overlaps are allowed"
    );
    assert!(appt.contains("cannot be booked twice"));
    let stale = [
        ("de", "nicht automatisch verhindert", "Bankkonto von"),
        ("es", "no se evitan", "Cuenta bancaria"),
        ("fi", "ei estetä", "pankkitili"),
        ("fr", "ne sont pas empêchés", "Compte bancaire"),
        ("pl", "nie jest blokowane", "Rachunek bankowy"),
        ("sv", "förhindras inte", "bankkonto"),
    ];
    for (lang, overlap, account) in stale {
        let a = read(&format!("labels/{lang}/appointments.toml"));
        assert!(!a.contains(overlap), "{lang}: appointments description");
        let acc = read(&format!("labels/{lang}/accounting.toml"));
        let at = acc
            .find("field.counterparty_iban]")
            .expect("counterparty_iban labels");
        let block: String = acc[at..].lines().take(3).collect();
        assert!(!block.contains(account), "{lang}: {block}");
        assert!(block.contains("IBAN"), "{lang}: {block}");
    }
}

#[test]
fn accounting_holds_its_iban_and_its_fiscal_dates() {
    let text = Bundled.fetch("accounting").unwrap();
    let module = Manifest::parse(&text).unwrap().to_module().unwrap();
    let field = |model: &str, name: &str| {
        module
            .models
            .iter()
            .find(|m| m.name == model)
            .and_then(|m| m.fields.iter().find(|f| f.name == name))
            .cloned()
            .unwrap()
    };
    let iban = field("accounting_bank_statement_line", "counterparty_iban");
    assert!(iban.description.contains("IBAN"), "{}", iban.description);
    assert!(iban.check_text("123123").is_err());
    assert!(iban.check_text("DE89370400440532013000").is_ok());
    let journal_currency = field("accounting_journal", "currency");
    assert!(journal_currency.check_text("12y12").is_err());
    for model in ["accounting_fiscal_year", "accounting_period"] {
        for name in ["start_on", "end_on"] {
            let f = field(model, name);
            assert!(f.check_date("0313-12-13").is_err(), "{model}.{name}");
            assert!(f.check_date("2026-01-01").is_ok(), "{model}.{name}");
        }
    }
}

#[test]
fn a_date_bound_is_a_day_as_text_or_as_a_toml_date() {
    let date = |v: &str| {
        format!(
            "\n[[model.field]]\nname = \"due_on\"\ndescription = \"d\"\nclass = \"none\"\nkind = \"date\"\nmin = {v}\n"
        )
    };
    assert_eq!(problem(&date("\"1900-01-01\"")), None);
    assert_eq!(problem(&date("1900-01-01")), None);
    let p = problem(&date("\"1900-02-30\"")).unwrap_or_default();
    assert!(p.contains("a real day"), "{p}");
}

/// The upgrade plan shows the changelog: a version that starts refusing writes says so first.
#[test]
fn the_changelogs_name_the_refusals_the_versions_bring() {
    for (id, since) in [
        ("projects", "Refused since 0.5.1"),
        ("planning", "Refused since 0.3.1"),
        ("appointments", "Refused since 0.3.2"),
    ] {
        let text = Bundled.fetch(id).unwrap();
        let m = Manifest::parse(&text).unwrap();
        assert!(m.module.changelog.starts_with(since), "{id}");
    }
}
