//! Tests of the field limits (D90) at the level of the registry: counting, the default, exact
//! bounds, the sentences, and the install gate.

use crate::limits::*;
use crate::{FieldDef, FieldKind, ModelDef, Module, Reset, check_module};
use obop_types::DataClass;
use obop_values::Decimal;

fn f(name: &'static str) -> FieldDef {
    FieldDef::new(name, "d", DataClass::None, false, "t.user")
}
fn dec(s: &str, scale: u8) -> Bound {
    Bound::Dec(Decimal::parse_for(s, 12, scale, None).unwrap())
}
fn with(field: FieldDef, l: Limits) -> FieldDef {
    field.with_limits(l)
}

#[test]
fn the_defaults_are_below_the_caps_and_named_in_one_place() {
    assert_eq!((DEFAULT_TEXT_MAX, DEFAULT_LONGTEXT_MAX), (2_048, 100_000));
    assert_eq!((TEXT_MAX_CAP, LONGTEXT_MAX_CAP), (10_000, 1_000_000));
    // The order of the four is checked by the compiler, so a change of one number that breaks it
    // does not build: each default is below its cap, and a text may not be allowed more than a
    // longtext has by default.
    const {
        assert!(DEFAULT_TEXT_MAX < TEXT_MAX_CAP && DEFAULT_LONGTEXT_MAX < LONGTEXT_MAX_CAP);
        assert!(TEXT_MAX_CAP <= DEFAULT_LONGTEXT_MAX);
    }
}

#[test]
fn the_effective_maximum_is_the_declared_one_or_the_default_of_the_kind() {
    assert_eq!(f("a").effective_max_length(), Some(DEFAULT_TEXT_MAX));
    assert_eq!(
        f("a").kind(FieldKind::LongText).effective_max_length(),
        Some(DEFAULT_LONGTEXT_MAX)
    );
    let own = with(
        f("a"),
        Limits {
            max_length: Some(30),
            ..Default::default()
        },
    );
    assert_eq!(own.effective_max_length(), Some(30));
    // A declared value may also be above the default, up to the cap.
    let big = with(
        f("a"),
        Limits {
            max_length: Some(TEXT_MAX_CAP),
            ..Default::default()
        },
    );
    assert_eq!(big.effective_max_length(), Some(TEXT_MAX_CAP));
    // No length for the other kinds, and none for a field the system writes.
    for k in [
        FieldKind::Number,
        FieldKind::Money,
        FieldKind::Date,
        FieldKind::Bool,
    ] {
        assert_eq!(f("a").kind(k.clone()).effective_max_length(), None, "{k:?}");
    }
    assert_eq!(f("a").computed("1").effective_max_length(), None);
    assert_eq!(
        f("a")
            .sequence("{seq}", false, Reset::Never)
            .effective_max_length(),
        None
    );
    assert_eq!(f("a").effective_min_length(), None);
}

/// The count the plain way, to compare the shortcut by bytes against.
fn naive(s: &str, min: Option<u32>, max: Option<u32>) -> Result<(), Breach> {
    let have = s.chars().count();
    if let Some(m) = max
        && have > m as usize
    {
        return Err(Breach::TooLong { max: m, have });
    }
    if let Some(m) = min
        && have < m as usize
    {
        return Err(Breach::TooShort { min: m, have });
    }
    Ok(())
}

#[test]
fn counting_by_bytes_first_never_changes_the_answer() {
    // One letter of each width in UTF-8 (1, 2, 3 and 4 bytes) and a letter with a combining mark.
    let alphabet = ["a", "å", "€", "\u{1F600}", "e\u{301}"];
    let mut strings = vec![String::new()];
    let mut layer = vec![String::new()];
    for _ in 0..4 {
        layer = layer
            .iter()
            .flat_map(|s| alphabet.iter().map(move |c| format!("{s}{c}")))
            .collect();
        strings.extend(layer.iter().cloned());
    }
    let mut checked = 0usize;
    for s in &strings {
        for max in [None, Some(1), Some(2), Some(3), Some(5), Some(8)] {
            for min in [None, Some(1), Some(2), Some(4)] {
                if let (Some(lo), Some(hi)) = (min, max)
                    && lo > hi
                {
                    continue;
                }
                let field = with(
                    f("a"),
                    Limits {
                        min_length: min,
                        max_length: max,
                        ..Default::default()
                    },
                );
                // With no maximum declared the default (2,048) applies; none of these strings is
                // near it, so the plain count with no maximum gives the same answer.
                assert_eq!(
                    field.check_text(s),
                    naive(s, min, max),
                    "{s:?} min {min:?} max {max:?}"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 5_000, "{checked}");
}

#[test]
fn characters_are_code_points() {
    assert_eq!(char_count("åäö"), 3);
    assert_eq!(char_count("\u{1F600}"), 1);
    // A flag is two regional indicators and a family five code points: counted as they are.
    assert_eq!(char_count("\u{1F1EB}\u{1F1EE}"), 2);
    assert_eq!(char_count("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"), 5);
    let three = with(
        f("a"),
        Limits {
            max_length: Some(3),
            ..Default::default()
        },
    );
    assert_eq!(three.check_text("åäö"), Ok(()));
    assert_eq!(three.check_text("\u{1F600}\u{1F600}\u{1F600}"), Ok(()));
    assert_eq!(
        three.check_text("\u{1F600}\u{1F600}\u{1F600}\u{1F600}"),
        Err(Breach::TooLong { max: 3, have: 4 })
    );
}

#[test]
fn the_default_applies_to_a_text_field_and_a_longtext_has_its_own() {
    let s = "x".repeat(DEFAULT_TEXT_MAX as usize);
    assert_eq!(f("a").check_text(&s), Ok(()));
    assert_eq!(
        f("a").check_text(&format!("{s}x")),
        Err(Breach::TooLong {
            max: DEFAULT_TEXT_MAX,
            have: DEFAULT_TEXT_MAX as usize + 1
        })
    );
    assert_eq!(
        f("a")
            .kind(FieldKind::LongText)
            .check_text(&format!("{s}x")),
        Ok(())
    );
}

#[test]
fn whole_number_and_decimal_bounds_are_inclusive_and_exact() {
    let n = with(
        f("n").kind(FieldKind::Number),
        Limits {
            min: Some(Bound::Int(-3)),
            max: Some(Bound::Int(3)),
            ..Default::default()
        },
    );
    assert_eq!(n.check_int(-3), Ok(()));
    assert_eq!(n.check_int(3), Ok(()));
    assert_eq!(n.check_int(-4), Err(Breach::Below(Bound::Int(-3))));
    assert_eq!(n.check_int(4), Err(Breach::Above(Bound::Int(3))));
    assert_eq!(f("n").kind(FieldKind::Number).check_int(i64::MIN), Ok(()));

    // Seven digits after the point: 0.1000001 is above 0.1000000 and 0.0999999 is below it,
    // which a double cannot tell apart from 0.1.
    let d = with(
        f("d").decimal(12, 7),
        Limits {
            min: Some(dec("0.1000000", 7)),
            max: Some(dec("0.9000000", 7)),
            ..Default::default()
        },
    );
    let v = |s: &str| Decimal::parse_for(s, 12, 7, None).unwrap();
    assert_eq!(d.check_decimal(v("0.1000000")), Ok(()));
    assert_eq!(d.check_decimal(v("0.1000001")), Ok(()));
    assert_eq!(d.check_decimal(v("0.9000000")), Ok(()));
    assert!(d.check_decimal(v("0.0999999")).is_err());
    assert!(d.check_decimal(v("0.9000001")).is_err());
    assert_eq!(
        d.check_decimal(v("-0.0000001")),
        Err(Breach::Below(dec("0.1000000", 7)))
    );
    // Larger than a double holds exactly: 2^53 + 1 digits apart.
    let big = with(
        f("d").decimal(20, 0),
        Limits {
            max: Some(Bound::Dec(
                Decimal::parse_for("9007199254740993", 20, 0, None).unwrap(),
            )),
            ..Default::default()
        },
    );
    let at = |s: &str| Decimal::parse_for(s, 20, 0, None).unwrap();
    assert_eq!(big.check_decimal(at("9007199254740993")), Ok(()));
    assert!(big.check_decimal(at("9007199254740994")).is_err());
}

#[test]
fn a_breach_says_what_was_wanted_and_what_was_written() {
    let msg = |b: Breach, v: &str| b.message("m", "f", v);
    assert_eq!(
        msg(
            Breach::TooLong {
                max: 2000,
                have: 2001
            },
            ""
        ),
        "m.f must be at most 2000 characters; this has 2001"
    );
    assert_eq!(
        msg(Breach::TooLong { max: 1, have: 2 }, ""),
        "m.f must be at most 1 character; this has 2"
    );
    assert_eq!(
        msg(Breach::TooShort { min: 3, have: 0 }, ""),
        "m.f must be at least 3 characters; this has 0"
    );
    assert_eq!(
        msg(Breach::Below(Bound::Int(0)), "-5"),
        "m.f must be at least 0; this is -5"
    );
    assert_eq!(
        msg(Breach::Above(dec("0.90", 2)), "1.00"),
        "m.f must be at most 0.90; this is 1.00"
    );
}

/// The refusal for a caller who may not be told the size of what was written (an agent holding a
/// masked token, a value an action copied from a field the caller cannot read; the review of
/// PR 33, F7) says what the field takes and nothing about the value.
#[test]
fn a_refusal_without_the_value_says_only_what_the_field_takes() {
    let all = [
        (
            Breach::TooLong {
                max: 10,
                have: 4_000,
            },
            "m.f must be at most 10 characters",
        ),
        (
            Breach::TooLong { max: 1, have: 40 },
            "m.f must be at most 1 character",
        ),
        (
            Breach::TooShort { min: 3, have: 0 },
            "m.f must be at least 3 characters",
        ),
        (Breach::Below(Bound::Int(0)), "m.f must be at least 0"),
        (Breach::Above(Bound::Int(100)), "m.f must be at most 100"),
        (Breach::Above(dec("0.90", 2)), "m.f must be at most 0.90"),
    ];
    for (b, want) in all {
        assert_eq!(b.message_without_value("m", "f"), want);
        assert_eq!(b.requirement(), want.strip_prefix("m.f must be ").unwrap());
        // The sentence with the value is the same one plus the value.
        assert!(
            b.message("m", "f", "73250").starts_with(want),
            "{}",
            b.message("m", "f", "73250")
        );
        assert!(!b.message_without_value("m", "f").contains("this "));
    }
}

#[test]
fn the_note_for_descriptions_only_speaks_of_declared_limits() {
    assert_eq!(f("a").limits_note(), "");
    let own = |l: Limits| with(f("a"), l).limits_note();
    assert_eq!(
        own(Limits {
            max_length: Some(100),
            ..Default::default()
        }),
        " (at most 100 characters)"
    );
    assert_eq!(
        own(Limits {
            min_length: Some(1),
            ..Default::default()
        }),
        " (at least 1 character)"
    );
    assert_eq!(
        own(Limits {
            min_length: Some(1),
            max_length: Some(80),
            ..Default::default()
        }),
        " (from 1 character to 80 characters)"
    );
    assert_eq!(
        limits_note(None, None, Some(Bound::Int(0)), Some(Bound::Int(100))),
        " (from 0 to 100)"
    );
    assert_eq!(
        limits_note(None, None, None, Some(Bound::Int(100))),
        " (at most 100)"
    );
}

// ---- the install gate ---------------------------------------------------------------------

fn gate(field: FieldDef) -> Vec<String> {
    problems(&field)
}

#[test]
fn the_gate_accepts_what_makes_sense_and_a_field_with_no_limits_is_free() {
    assert!(gate(f("a")).is_empty());
    let ok = [
        with(
            f("a"),
            Limits {
                min_length: Some(1),
                max_length: Some(1),
                ..Default::default()
            },
        ),
        with(
            f("a"),
            Limits {
                max_length: Some(TEXT_MAX_CAP),
                ..Default::default()
            },
        ),
        with(
            f("a").kind(FieldKind::LongText),
            Limits {
                max_length: Some(LONGTEXT_MAX_CAP),
                ..Default::default()
            },
        ),
        with(
            f("a").kind(FieldKind::Number),
            Limits {
                min: Some(Bound::Int(5)),
                max: Some(Bound::Int(5)),
                ..Default::default()
            },
        ),
        with(
            f("a").decimal(10, 2),
            Limits {
                min: Some(dec("0.00", 2)),
                max: Some(dec("99999999.99", 2)),
                ..Default::default()
            },
        ),
    ];
    for field in ok {
        assert_eq!(
            gate(field.clone()),
            Vec::<String>::new(),
            "{:?}",
            field.limits
        );
    }
}

fn has(found: Vec<String>, needle: &str) {
    assert!(
        found.iter().any(|p| p.contains(needle)),
        "wanted {needle:?} in {found:?}"
    );
}

#[test]
fn the_gate_refuses_a_length_that_cannot_hold() {
    let text = |l: Limits| gate(with(f("a"), l));
    let long = |l: Limits| gate(with(f("a").kind(FieldKind::LongText), l));
    has(
        text(Limits {
            max_length: Some(0),
            ..Default::default()
        }),
        "`max_length` must be at least 1",
    );
    has(
        text(Limits {
            min_length: Some(0),
            ..Default::default()
        }),
        "`min_length` must be at least 1",
    );
    has(
        text(Limits {
            max_length: Some(TEXT_MAX_CAP + 1),
            ..Default::default()
        }),
        "above the largest a text field may declare, 10000",
    );
    has(
        long(Limits {
            max_length: Some(LONGTEXT_MAX_CAP + 1),
            ..Default::default()
        }),
        "above the largest a longtext field may declare, 1000000",
    );
    has(
        text(Limits {
            min_length: Some(10),
            max_length: Some(9),
            ..Default::default()
        }),
        "`min_length` 10 is above `max_length` 9",
    );
    has(
        text(Limits {
            min_length: Some(DEFAULT_TEXT_MAX + 1),
            ..Default::default()
        }),
        "above the default maximum",
    );
}

/// The advice for a `min_length` with no `max_length` must not send the author to a number the
/// gate refuses next (the review of PR 33, F20): it names the cap, and above the cap it says so
/// directly and points to a longtext field.
#[test]
fn the_advice_for_a_min_length_without_a_max_length_names_the_cap() {
    let only = |field: FieldDef, min: u32| {
        gate(with(
            field,
            Limits {
                min_length: Some(min),
                ..Default::default()
            },
        ))
    };
    // Between the default and the cap: declare a max_length, at most the cap; text suggests
    // longtext for more.
    let found = only(f("a"), DEFAULT_TEXT_MAX + 1);
    has(
        found.clone(),
        "declare a `max_length` of at least 2049, at most 10000",
    );
    has(found, "use a longtext field for more than 10000");
    let found = only(f("a").kind(FieldKind::LongText), DEFAULT_LONGTEXT_MAX + 1);
    has(
        found.clone(),
        "declare a `max_length` of at least 100001, at most 1000000",
    );
    assert!(
        !found.iter().any(|p| p.contains("use a longtext field")),
        "a longtext field has nowhere larger to go: {found:?}"
    );
    // Above the cap no max_length can help: the cap is reported directly, with no advice that
    // the gate would refuse.
    let found = only(f("a"), TEXT_MAX_CAP + 1);
    has(
        found.clone(),
        "`min_length` 10001 is above 10000, the most a text field can hold; use a longtext field for more (up to 1000000)",
    );
    assert!(
        !found.iter().any(|p| p.contains("declare a `max_length`")),
        "no advice to declare a max_length that is itself above the cap: {found:?}"
    );
    let found = only(f("a").kind(FieldKind::LongText), LONGTEXT_MAX_CAP + 1);
    has(
        found.clone(),
        "`min_length` 1000001 is above 1000000, the most a longtext field can hold",
    );
    assert!(
        !found.iter().any(|p| p.contains("use a longtext field")),
        "{found:?}"
    );
    // At the cap itself it fits, with the max_length advice.
    has(
        only(f("a"), TEXT_MAX_CAP),
        "declare a `max_length` of at least 10000, at most 10000",
    );
}

#[test]
fn the_gate_refuses_keys_on_the_wrong_kind() {
    has(
        gate(with(
            f("a").kind(FieldKind::Number),
            Limits {
                max_length: Some(5),
                ..Default::default()
            },
        )),
        "only valid for kind \"text\" or \"longtext\"",
    );
    has(
        gate(with(
            f("a").kind(FieldKind::Date),
            Limits {
                min: Some(Bound::Int(1)),
                ..Default::default()
            },
        )),
        "only valid for kind \"number\", \"money\" or \"decimal\"",
    );
    has(
        gate(with(
            f("a"),
            Limits {
                max: Some(Bound::Int(1)),
                ..Default::default()
            },
        )),
        "only valid for kind \"number\"",
    );
    // A field the system writes takes none.
    has(
        gate(with(
            f("a").computed("1"),
            Limits {
                max_length: Some(3),
                ..Default::default()
            },
        )),
        "computed or numbered",
    );
}

#[test]
fn the_gate_refuses_bounds_that_do_not_fit_the_kind_or_each_other() {
    has(
        gate(with(
            f("a").kind(FieldKind::Number),
            Limits {
                min: Some(dec("1.0", 1)),
                ..Default::default()
            },
        )),
        "must be a whole number",
    );
    has(
        gate(with(
            f("a").decimal(10, 2),
            Limits {
                max: Some(Bound::Int(5)),
                ..Default::default()
            },
        )),
        "must be a decimal at the scale of the field",
    );
    has(
        gate(with(
            f("a").decimal(10, 2),
            Limits {
                max: Some(dec("1.000", 3)),
                ..Default::default()
            },
        )),
        "does not fit the field",
    );
    has(
        gate(with(
            f("a").kind(FieldKind::Number),
            Limits {
                min: Some(Bound::Int(2)),
                max: Some(Bound::Int(1)),
                ..Default::default()
            },
        )),
        "`min` 2 is above `max` 1",
    );
    has(
        gate(with(
            f("a").decimal(10, 2),
            Limits {
                min: Some(dec("0.11", 2)),
                max: Some(dec("0.10", 2)),
                ..Default::default()
            },
        )),
        "`min` 0.11 is above `max` 0.10",
    );
}

#[test]
fn check_module_runs_the_gate_so_install_and_start_both_refuse() {
    let bad = with(
        f("title"),
        Limits {
            max_length: Some(0),
            ..Default::default()
        },
    );
    let module = Module {
        id: "t".into(),
        name: "T".into(),
        models: vec![ModelDef {
            name: "t_doc",
            description: "d",
            read_group: "t.user",
            write_group: "t.user",
            access: None,
            fields: vec![bad],
            commands: vec![],
            ext: Default::default(),
        }],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    };
    let v = check_module(&module);
    assert!(
        v.iter()
            .any(|v| v.path.contains("t_doc.title") && v.problem.contains("max_length")),
        "{v:?}"
    );
}
