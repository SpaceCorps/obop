//! The storage layer and the registry both know the names of the system fields (D75) and the
//! shape of their timestamps, and neither depends on the other; this crate depends on both, so
//! it is where they are held against each other.

use obop_registry::{RESERVED_FIELDS, SYSTEM_FIELDS, system_defs};
use obop_storage::sysfields::{self, SYSTEM_KEYS, fmt_micros, parse_micros};
use obop_values::Instant;

#[test]
fn the_registry_and_the_storage_layer_name_the_same_system_fields_in_the_same_order() {
    assert_eq!(SYSTEM_FIELDS, SYSTEM_KEYS);
    assert_eq!(RESERVED_FIELDS[..SYSTEM_FIELDS.len()], SYSTEM_FIELDS);
    for name in SYSTEM_FIELDS {
        assert!(sysfields::is_system_key(name), "{name}");
        assert!(obop_registry::is_system_name(name), "{name}");
    }
    // The other two reserved names are not keys of a record.
    for name in ["id", "if_version"] {
        assert!(!sysfields::is_system_key(name), "{name}");
        assert!(!obop_registry::is_system_name(name), "{name}");
    }
    let defs: Vec<_> = system_defs("g").iter().map(|d| d.name).collect();
    assert_eq!(defs, SYSTEM_KEYS);
}

#[test]
fn the_timestamp_of_the_storage_layer_is_the_canonical_datetime() {
    // Both formats are written by hand (the storage layer cannot depend on the values crate), so
    // a sample over the whole range, the ends and the leap days tells whether they are one.
    let mut samples: Vec<i64> = vec![
        0,
        -1,
        1,
        999_999,
        1_000_000,
        951_782_400_000_000,   // 2000-02-29
        4_107_542_400_000_000, // 2100-03-01, not a leap year
        -62_135_596_800_000_000,
        253_402_300_799_999_999,
    ];
    let mut n: i64 = 0x1234_5678_9abc_def0;
    for _ in 0..20_000 {
        n = n
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let span = 253_402_300_799_999_999i64 + 62_135_596_800_000_000i64 + 1;
        samples.push(-62_135_596_800_000_000 + (n as u64 % span as u64) as i64);
    }
    for us in samples {
        let theirs = Instant::from_micros(us).expect("in range").canonical();
        assert_eq!(fmt_micros(us), theirs, "{us}");
        assert_eq!(parse_micros(&theirs), Some(us), "{theirs}");
        assert_eq!(Instant::parse(&theirs).unwrap().micros(), us, "{theirs}");
    }
    // What the storage layer refuses is not a canonical instant either.
    for text in [
        "2026-10-25T15:40:00Z",
        "2026-10-25T15:40:00.123Z",
        "2026-02-30T00:00:00.000000Z",
        "2026-10-25T15:40:00.123456+00:00",
    ] {
        assert_eq!(parse_micros(text), None, "{text}");
    }
}
