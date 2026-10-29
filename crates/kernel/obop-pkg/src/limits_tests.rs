//! Field limits (D90) in packages: the keys `max_length`, `min_length`, `min` and `max` of
//! `[[model.field]]`, what the manifest parser and the install gate refuse (naming the field and
//! the key), the version a package that uses them declares, and the upgrade diff.

use crate::manifest::Manifest;
use crate::migrate::diff;
use crate::resolve::{World, plan_install};
use crate::semver::{Req, Version};
use crate::{Bundled, Marketplace};
use obop_registry::{Bound, DEFAULT_TEXT_MAX, check_module};
use std::collections::BTreeMap;

/// A package with one model and the given fields; `core` is the `requires_core` it declares.
fn pkg_with_core(core: &str, version: &str, fields: &str) -> String {
    format!(
        "[module]\nid = \"lim\"\nversion = \"{version}\"\nname = \"Lim\"\ndescription = \"d\"\nlicense = \"MIT\"\nrequires_core = \"{core}\"\n\n[[model]]\nname = \"lim_doc\"\ndescription = \"d\"\nread_group = \"lim.user\"\nwrite_group = \"lim.user\"\n{fields}"
    )
}

fn pkg(fields: &str) -> String {
    pkg_with_core("^0.2.1", "0.1.0", fields)
}

fn field(name: &str, extra: &str) -> String {
    format!(
        "\n[[model.field]]\nname = \"{name}\"\ndescription = \"d\"\nclass = \"none\"\n{extra}\n"
    )
}

/// The error of parsing, building and gating a package, or `None` when it is fine.
fn problem(text: &str) -> Option<String> {
    let m = match Manifest::parse(text) {
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

#[test]
fn every_limit_key_is_read_into_the_field() {
    let text = pkg(&[
        field("title", "max_length = 100\nmin_length = 2"),
        field("notes", "kind = \"longtext\"\nmax_length = 50000"),
        field("qty", "kind = \"number\"\nmin = 0\nmax = 100"),
        field("price", "kind = \"money\"\nmin = -500\nmax = 100000"),
        field(
            "rate",
            "kind = \"decimal\"\nprecision = 12\nscale = 7\nmin = \"0.1000000\"\nmax = \"0.9000000\"",
        ),
        // A whole number is a fine way to write a decimal bound; it takes the scale of the field.
        field(
            "share",
            "kind = \"decimal\"\nprecision = 5\nscale = 2\nmin = 0\nmax = 1",
        ),
        field("plain", ""),
    ]
    .concat());
    assert_eq!(problem(&text), None);
    let module = Manifest::parse(&text).unwrap().to_module().unwrap();
    let by = |n: &str| {
        module.models[0]
            .fields
            .iter()
            .find(|f| f.name == n)
            .unwrap()
            .clone()
    };
    let l = by("title").declared_limits();
    assert_eq!((l.min_length, l.max_length), (Some(2), Some(100)));
    assert_eq!(by("title").effective_max_length(), Some(100));
    assert_eq!(by("notes").effective_max_length(), Some(50_000));
    let l = by("qty").declared_limits();
    assert_eq!((l.min, l.max), (Some(Bound::Int(0)), Some(Bound::Int(100))));
    let l = by("price").declared_limits();
    assert_eq!(
        (l.min, l.max),
        (Some(Bound::Int(-500)), Some(Bound::Int(100_000)))
    );
    let l = by("rate").declared_limits();
    assert_eq!(
        (l.min.unwrap().to_string(), l.max.unwrap().to_string()),
        ("0.1000000".to_string(), "0.9000000".to_string())
    );
    let l = by("share").declared_limits();
    assert_eq!(
        (l.min.unwrap().to_string(), l.max.unwrap().to_string()),
        ("0.00".to_string(), "1.00".to_string())
    );
    // A field with no key has no declared limit and the default of its kind.
    assert!(by("plain").declared_limits().is_empty());
    assert_eq!(by("plain").effective_max_length(), Some(DEFAULT_TEXT_MAX));
}

#[test]
fn a_bad_limit_is_refused_naming_the_field_and_the_key() {
    // (the field's keys, a piece of the message). Each message names the model and the field.
    let cases: &[(&str, &str)] = &[
        ("max_length = 0", "`max_length` must be at least 1"),
        ("min_length = 0", "`min_length` must be at least 1"),
        (
            "max_length = 10001",
            "above the largest a text field may declare, 10000",
        ),
        (
            "kind = \"longtext\"\nmax_length = 1000001",
            "above the largest a longtext field may declare, 1000000",
        ),
        (
            "min_length = 5\nmax_length = 4",
            "`min_length` 5 is above `max_length` 4",
        ),
        ("min_length = 2049", "above the default maximum"),
        ("max_length = -1", "invalid"),
        ("max_length = \"100\"", "invalid"),
        ("max_length = 1.5", "invalid"),
        ("maxlength = 5", "unknown field"),
        (
            "kind = \"number\"\nmax_length = 5",
            "only valid for kind \"text\" or \"longtext\"",
        ),
        (
            "kind = \"date\"\nmin = 1",
            "only valid for kind \"number\", \"money\" or \"decimal\"",
        ),
        (
            "min = 1",
            "only valid for kind \"number\", \"money\" or \"decimal\"",
        ),
        ("kind = \"number\"\nmin = 0.5", "must be a whole number"),
        ("kind = \"number\"\nmin = \"1\"", "must be a whole number"),
        (
            "kind = \"decimal\"\nprecision = 10\nscale = 2\nmax = 1.5",
            "a TOML float is not exact",
        ),
        (
            "kind = \"decimal\"\nprecision = 10\nscale = 2\nmax = \"1.234\"",
            "is not a decimal that fits decimal(10, 2)",
        ),
        (
            "kind = \"decimal\"\nprecision = 4\nscale = 2\nmax = \"1000.00\"",
            "is not a decimal that fits decimal(4, 2)",
        ),
        (
            "kind = \"decimal\"\nprecision = 10\nscale = 2\nmax = \"abc\"",
            "is not a decimal",
        ),
        (
            "kind = \"number\"\nmin = 5\nmax = 4",
            "`min` 5 is above `max` 4",
        ),
        (
            "kind = \"decimal\"\nprecision = 10\nscale = 2\nmin = \"0.11\"\nmax = \"0.10\"",
            "`min` 0.11 is above `max` 0.10",
        ),
        (
            "computed = \"1 + 1\"\nkind = \"number\"\nmax = 5",
            "computed or numbered",
        ),
    ];
    for (keys, want) in cases {
        let text = pkg(&field("target", keys));
        let e = problem(&text).unwrap_or_else(|| panic!("{keys:?} should be refused"));
        assert!(e.contains(want), "{keys:?}: wanted {want:?}, got {e}");
        // Where it happens: the error does not leave the person looking for the field.
        if !e.contains("unknown field") && !e.contains("invalid") {
            assert!(
                e.contains("target"),
                "{keys:?} does not name the field: {e}"
            );
        }
    }
}

#[test]
fn the_edges_of_what_is_allowed_are_allowed() {
    for keys in [
        "max_length = 1",
        "max_length = 10000",
        "min_length = 1\nmax_length = 1",
        "kind = \"longtext\"\nmax_length = 1000000",
        "kind = \"number\"\nmin = 5\nmax = 5",
        "kind = \"number\"\nmin = -9223372036854775808\nmax = 9223372036854775807",
        "kind = \"decimal\"\nprecision = 4\nscale = 2\nmin = \"-99.99\"\nmax = \"99.99\"",
    ] {
        assert_eq!(problem(&pkg(&field("target", keys))), None, "{keys}");
    }
}

#[test]
fn a_default_of_a_migration_step_is_held_to_the_limits_of_its_field() {
    let step = |value: &str| {
        format!(
            "\n[[migration]]\nfrom = \"<0.2\"\nop = \"set_default\"\nmodel = \"lim_doc\"\nfield = \"title\"\nvalue = {value}\n"
        )
    };
    let with = |value: &str| {
        pkg_with_core(
            "^0.2.1",
            "0.2.0",
            &format!("{}{}", field("title", "max_length = 5"), step(value)),
        )
    };
    assert_eq!(problem(&with("\"fits\"")), None);
    let e = problem(&with("\"too long\"")).expect("a default of 8 characters into 5");
    assert!(
        e.contains("outside the limits of title") && e.contains("at most 5 characters; this has 8"),
        "{e}"
    );
}

#[test]
fn a_float_default_into_a_number_or_money_field_is_refused() {
    // A float is not a whole number: a write refuses it, and it would also pass the bounds of the
    // field unseen (review of PR 33, F19). `set_default` takes the integer form only.
    for kind in ["number", "money"] {
        let with = |value: &str| {
            pkg_with_core(
                "^0.2.1",
                "0.2.0",
                &format!(
                    "{}\n[[migration]]\nfrom = \"<0.2\"\nop = \"set_default\"\nmodel = \"lim_doc\"\nfield = \"qty\"\nvalue = {value}\n",
                    field("qty", &format!("kind = \"{kind}\"\nmax = 100"))
                ),
            )
        };
        assert_eq!(problem(&with("50")), None, "{kind}");
        for float in ["1000.0", "50.5", "50.0"] {
            let e =
                problem(&with(float)).unwrap_or_else(|| panic!("{kind} {float} must be refused"));
            assert!(
                e.contains("does not fit the") && e.contains("a whole number"),
                "{kind} {float}: {e}"
            );
        }
        // An integer above the bound is still refused by the limit.
        let e = problem(&with("1000")).expect("1000 into max 100");
        assert!(e.contains("at most 100; this is 1000"), "{e}");
    }
}

// ---- the version a package that uses limits declares --------------------------------------

/// D88 Decision 14: a feature that widens what the manifest parser accepts takes the next patch
/// number of the 0.2 line, and a package that uses it declares `^0.2.N`. This is the number of
/// the limits, written out here and not computed from `CORE_VERSION`, so that the core cannot
/// move it by accident.
const LIMITS_CORE_PATCH: u64 = 1;

#[test]
fn core_version_requirement_kinds_limits() {
    let n = LIMITS_CORE_PATCH;
    let text = pkg_with_core(
        &format!("^0.2.{n}"),
        "0.1.0",
        &field("title", "max_length = 100"),
    );
    let m = Manifest::parse(&text).unwrap();
    let req = m.core_req();

    // `^0.2.1` is satisfied by the core that has the keys and by the core that is running.
    assert!(req.matches(&Version::new(0, 2, n)));
    assert!(req.matches(&Version::parse(obop_types::CORE_VERSION).unwrap()));
    assert!(Version::parse(obop_types::CORE_VERSION).unwrap() >= Version::new(0, 2, n));
    // It is not satisfied by the core before it, nor by 0.3.0.
    assert!(!req.matches(&Version::new(0, 2, n - 1)));
    assert!(!req.matches(&Version::new(0, 3, 0)));

    // The resolver turns that into a blocker on the older core, and into a plan on this one.
    let market = |_: &str| Ok(None);
    let world = |core: Version| World {
        core,
        builtin: BTreeMap::new(),
        installed: BTreeMap::new(),
        market: &market,
        registered: &|_| false,
    };
    let old = plan_install(&world(Version::new(0, 2, n - 1)), &m, "upload");
    assert!(!old.ok());
    assert_eq!(
        old.blockers[0].message,
        format!("lim 0.1.0 needs core ^0.2.{n}, you run 0.2.{}", n - 1)
    );
    let current = plan_install(
        &world(Version::parse(obop_types::CORE_VERSION).unwrap()),
        &m,
        "upload",
    );
    assert!(current.ok(), "{:?}", current.blockers);

    // What the parser does not do, said plainly (D88 Decision 14): it does not check that the
    // number a package declares is high enough for the keys it uses. A package with `^0.2` and
    // a limit parses and gates like any other; the older core fails on the key at parse time.
    let lax = pkg_with_core("^0.2", "0.1.0", &field("title", "max_length = 100"));
    assert_eq!(problem(&lax), None);
}

#[test]
fn a_bundled_package_that_uses_a_limit_must_declare_the_core_that_has_them() {
    // The 23 bundled packages keep `^0.2` (OR-35 k). Any of them that gains a limit key must
    // declare at least the core that has the keys, so that this test fails if one forgets.
    let first = Version::new(0, 2, LIMITS_CORE_PATCH);
    let before = Version::new(0, 2, LIMITS_CORE_PATCH - 1);
    let mut checked = 0;
    for listed in Bundled.list().unwrap() {
        let m = Manifest::parse(&Bundled.fetch(&listed.id).unwrap()).unwrap();
        let module = m.to_module().unwrap();
        let uses = module
            .models
            .iter()
            .flat_map(|d| &d.fields)
            .any(|f| !f.declared_limits().is_empty());
        let req: Req = m.core_req();
        assert!(
            req.matches(&Version::parse(obop_types::CORE_VERSION).unwrap()),
            "{} does not run on the core",
            listed.id
        );
        if uses {
            assert!(
                req.matches(&first) && !req.matches(&before),
                "{} uses field limits and declares {}",
                listed.id,
                m.module.requires_core
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 23);
}

// ---- upgrades -----------------------------------------------------------------------------

#[test]
fn a_changed_limit_is_a_listed_change_and_never_breaking() {
    let old = Manifest::parse(&pkg(&field("title", "max_length = 100"))).unwrap();
    let kinds = |a: &Manifest, b: &Manifest| -> Vec<(String, bool)> {
        diff(a, b)
            .into_iter()
            .map(|c| (format!("{}:{}", c.kind, c.detail), c.breaking))
            .collect()
    };
    // Lowering a limit may leave a stored record above it; that is not breaking: such a record
    // stays editable in its other fields (D90 Decision 6).
    let lower = Manifest::parse(&pkg(&field("title", "max_length = 10"))).unwrap();
    let c = kinds(&old, &lower);
    assert_eq!(c.len(), 1, "{c:?}");
    assert!(c[0].0.starts_with("field_limits:"), "{c:?}");
    assert!(c[0].0.contains("100") && c[0].0.contains("10"), "{c:?}");
    assert!(!c[0].1, "{c:?}");
    // Adding one, and dropping one, are listed too.
    let none = Manifest::parse(&pkg(&field("title", ""))).unwrap();
    assert_eq!(kinds(&none, &old).len(), 1);
    assert_eq!(kinds(&old, &none).len(), 1);
    // The same limits are no change.
    assert!(kinds(&old, &old).is_empty());
    // A number bound is listed as well.
    let a = Manifest::parse(&pkg(&field("qty", "kind = \"number\"\nmax = 10"))).unwrap();
    let b = Manifest::parse(&pkg(&field("qty", "kind = \"number\"\nmax = 5"))).unwrap();
    assert_eq!(kinds(&a, &b).len(), 1);
}

/// The detail of the one `field_limits` change from `old` to `new`.
fn limits_change(old: &str, new: &str) -> String {
    let (a, b) = (
        Manifest::parse(&pkg(old)).unwrap(),
        Manifest::parse(&pkg(new)).unwrap(),
    );
    let c = diff(&a, &b);
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(c[0].kind, "field_limits");
    assert!(!c[0].breaking);
    c[0].detail.clone()
}

#[test]
fn the_plan_shows_the_default_that_applies_when_a_limit_is_removed_and_which_way_it_goes() {
    let title = |extra: &str| field("title", extra);
    // Dropping a max_length does not make the field unlimited: the default of the kind applies.
    // 5000 to 2048 tightens (review of PR 33, F2).
    let d = limits_change(&title("max_length = 5000"), &title(""));
    assert!(
        d.contains("limits tighten from max_length 5000 to max_length 2048 (default)"),
        "{d}"
    );
    assert!(
        d.contains("values already stored are not re-checked"),
        "{d}"
    );
    assert!(!d.contains("none"), "{d}");
    // A longtext has its own default.
    let d = limits_change(
        &field("notes", "kind = \"longtext\"\nmax_length = 300000"),
        &field("notes", "kind = \"longtext\""),
    );
    assert!(
        d.contains("tighten from max_length 300000 to max_length 100000 (default)"),
        "{d}"
    );
    // Declaring a larger one than the default loosens; the old side shows the default it had.
    let d = limits_change(&title(""), &title("max_length = 5000"));
    assert!(
        d.contains("limits loosen from max_length 2048 (default) to max_length 5000"),
        "{d}"
    );
    // A lower maximum or a new minimum tightens; a higher maximum or no minimum loosens.
    assert!(
        limits_change(&title("max_length = 100"), &title("max_length = 10"))
            .contains("tighten from")
    );
    assert!(
        limits_change(&title(""), &title("min_length = 2")).contains(
            "tighten from max_length 2048 (default) to min_length 2, max_length 2048 (default)"
        )
    );
    assert!(
        limits_change(&title("min_length = 2"), &title("")).contains(
            "loosen from min_length 2, max_length 2048 (default) to max_length 2048 (default)"
        )
    );
    // One limit tighter and another looser is said so.
    let d = limits_change(
        &title("max_length = 100"),
        &title("min_length = 3\nmax_length = 200"),
    );
    assert!(d.contains("limits tighten and loosen from"), "{d}");
    // Numbers: nothing is implied, so a removed bound is `none`, and the direction is read from
    // the bounds, decimals included.
    let qty = |extra: &str| field("qty", &format!("kind = \"number\"\n{extra}"));
    assert!(
        limits_change(&qty("max = 10"), &qty("max = 5"))
            .contains("limits tighten from max 10 to max 5")
    );
    assert!(
        limits_change(&qty("max = 10"), &qty("")).contains("limits loosen from max 10 to none")
    );
    assert!(limits_change(&qty(""), &qty("min = 0")).contains("limits tighten from none to min 0"));
    let rate = |extra: &str| {
        field(
            "rate",
            &format!("kind = \"decimal\"\nprecision = 8\nscale = 2\n{extra}"),
        )
    };
    assert!(
        limits_change(&rate("max = \"5.00\""), &rate("max = \"9.50\""))
            .contains("loosen from max 5.00 to max 9.50")
    );
    assert!(
        limits_change(&rate("min = \"1.00\""), &rate("min = \"0.50\""))
            .contains("loosen from min 1.00 to min 0.50")
    );
    // What is held counts, not what is spelled: a declared max_length equal to the default is the
    // default, and the same bound spelled as 5 and as "5.00" is the same bound.
    let (a, b) = (
        Manifest::parse(&pkg(&title(&format!("max_length = {DEFAULT_TEXT_MAX}")))).unwrap(),
        Manifest::parse(&pkg(&title(""))).unwrap(),
    );
    assert!(diff(&a, &b).is_empty() && diff(&b, &a).is_empty());
    let (a, b) = (
        Manifest::parse(&pkg(&rate("max = 5"))).unwrap(),
        Manifest::parse(&pkg(&rate("max = \"5.00\""))).unwrap(),
    );
    assert!(diff(&a, &b).is_empty());
}
