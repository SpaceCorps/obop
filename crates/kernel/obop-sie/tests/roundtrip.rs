use obop_sie::*;

fn d(y: u16, m: u8, dd: u8) -> Date {
    Date::new(y, m, dd).unwrap()
}

fn sample() -> SieFile {
    let mut f = SieFile::new("Café Åkerö AB", d(2026, 1, 1), d(2026, 12, 31));
    f.generated = d(2026, 11, 3);
    f.company.org_number = Some("556000-0167".into());
    f.chart = Some("BAS2014".into());
    for (n, name, k) in [
        (1930, "Företagskonto", AccountKind::Asset),
        (2440, "Leverantörsskulder", AccountKind::Liability),
        (3010, "Försäljning \"kaffe\"", AccountKind::Income),
        (6071, "Representation", AccountKind::Cost),
    ] {
        f.accounts.push(Account {
            number: n,
            name: name.into(),
            kind: Some(k),
        });
    }
    f.opening.push(Balance {
        year_index: 0,
        account: 1930,
        amount_minor: 100_000,
    });
    f.opening.push(Balance {
        year_index: 0,
        account: 2440,
        amount_minor: -100_000,
    });
    f.vouchers.push(Voucher {
        series: "A".into(),
        number: "1".into(),
        date: d(2026, 3, 1),
        text: "Försäljning é".into(),
        registered: Some(d(2026, 3, 2)),
        signed: Some("Åsa".into()),
        lines: vec![
            Line::new(1930, 125_050, Some("insättning")),
            Line {
                account: 3010,
                amount_minor: -125_050,
                text: None,
                objects: vec![("1".into(), "P1".into())],
            },
        ],
    });
    f.vouchers.push(Voucher {
        series: "A".into(),
        number: "2".into(),
        date: d(2026, 3, 5),
        text: "Fika".into(),
        registered: None,
        signed: None,
        lines: vec![Line::new(6071, 5, None), Line::new(1930, -5, None)],
    });
    f
}

#[test]
fn round_trip_is_stable() {
    let a = export(&sample());
    let parsed = parse(&a).unwrap();
    let b = export(&parsed);
    assert_eq!(a, b);
    assert_eq!(parse(&b).unwrap(), parsed);
}

#[test]
fn cp437_letters_round_trip() {
    let bytes = export(&sample());
    assert!(
        std::str::from_utf8(&bytes).is_err(),
        "output is CP437, not UTF-8"
    );
    assert!(bytes.contains(&0x8F) && bytes.contains(&0x84) && bytes.contains(&0x94));
    let p = parse(&bytes).unwrap();
    assert_eq!(p.company.name, "Café Åkerö AB");
    assert_eq!(p.vouchers[0].text, "Försäljning é");
    assert_eq!(p.vouchers[0].signed.as_deref(), Some("Åsa"));
    assert_eq!(p.accounts[2].name, "Försäljning \"kaffe\"");
    assert_eq!(decode_cp437(&encode_cp437("åäöÅÄÖé")), "åäöÅÄÖé");
    assert_eq!(encode_cp437("åäö"), vec![0x86, 0x84, 0x94]);
    assert_eq!(encode_cp437("é"), vec![0x82]);
}

#[test]
fn crlf_and_header() {
    let s = decode_cp437(&export(&sample()));
    assert!(s.starts_with("#FLAGGA 0\r\n#PROGRAM \"obop\""));
    assert!(s.contains("#FORMAT PC8\r\n") && s.contains("#SIETYP 4\r\n"));
    assert!(s.contains("#TRANS 1930 {} 1250.50"));
    assert!(s.contains("#TRANS 3010 {\"1\" \"P1\"} -1250.50"));
    assert!(!s.replace("\r\n", "").contains('\n'));
}

#[test]
fn derives_balances() {
    let s = decode_cp437(&export(&sample()));
    assert!(s.contains("#IB 0 1930 1000.00"));
    assert!(s.contains("#UB 0 1930 2250.45"), "{s}");
    assert!(s.contains("#RES 0 3010 -1250.50"));
    assert!(s.contains("#RES 0 6071 0.05"));
}

#[test]
fn unbalanced_voucher_is_refused() {
    let mut f = sample();
    f.vouchers[1].lines[0].amount_minor = 7;
    match parse(&export(&f)).unwrap_err() {
        SieError::Unbalanced {
            series,
            number,
            sum_minor,
            line,
        } => {
            assert_eq!((series.as_str(), number.as_str(), sum_minor), ("A", "2", 2));
            assert!(line > 1);
        }
        e => panic!("{e:?}"),
    }
}

#[test]
fn hand_written_sample_parses() {
    let src = "#FLAGGA 0\n#PROGRAM \"Kassan\" 2.1\n#FORMAT PC8\n#GEN 20260102\n#SIETYP 4\n\
        #FNAMN \"Test AB\"\n#RAR 0 20260101 20261231\n#RAR -1 20250101 20251231\n#KONTO 1910 \"Kassa\"\n\
        #KTYP 1910 T\n#UNKNOWN 1 2 3\n#IB 0 1910 50,5\n\
        #VER \"\" 7 20260110 \"Kontantf\u{f6}rs\"\n{\n#TRANS 1910 {} 100.00\n\
        #TRANS 3010 {1 \"A\" 2 \"B\"} -100.00 20260110 \"x\"\n#RTRANS 3010 {} -1\n}\n";
    let p = parse(src.as_bytes()).unwrap();
    assert_eq!(p.years.len(), 2);
    assert_eq!(p.opening[0].amount_minor, 5050);
    let v = &p.vouchers[0];
    assert_eq!((v.series.as_str(), v.number.as_str()), ("", "7"));
    assert_eq!(v.text, "Kontantförs");
    assert_eq!(v.lines.len(), 2);
    assert_eq!(v.lines[1].objects.len(), 2);
    assert_eq!(v.lines[1].text.as_deref(), Some("x"));
}

#[test]
fn errors_carry_line_numbers() {
    let bad = "#SIETYP 4\n#FNAMN \"A\"\n#VER \"A\" 1 2026xx01 \"t\"\n{\n}\n";
    assert_eq!(
        parse(bad.as_bytes()).unwrap_err(),
        SieError::Syntax {
            line: 3,
            message: "invalid voucher date `2026xx01`, expected YYYYMMDD".into()
        }
    );
    let bad = "#SIETYP 4\n#VER \"A\" 1 20260101 \"t\"\n{\n#TRANS 1930 {} 1.234\n}\n";
    assert!(matches!(
        parse(bad.as_bytes()),
        Err(SieError::Syntax { line: 4, .. })
    ));
    let bad = "#SIETYP 4\n#VER \"A\" 1 20260101 \"t\"\n{\n#TRANS 1930 {} 1.00\n";
    assert!(matches!(
        parse(bad.as_bytes()),
        Err(SieError::Syntax { line: 2, .. })
    ));
    assert!(parse(b"#FNAMN \"A\"\n").is_err());
}

#[test]
fn amounts() {
    assert_eq!(format_amount(-5), "-0.05");
    assert_eq!(format_amount(i64::MIN), "-92233720368547758.08");
    assert_eq!(parse_amount("-92233720368547758.08"), Some(i64::MIN));
    assert_eq!(parse_amount("1.50"), Some(150));
    assert_eq!(parse_amount("1,5"), Some(150));
    assert_eq!(parse_amount("2.500"), Some(250));
    assert_eq!(parse_amount("2.505"), None);
    assert_eq!(parse_amount("x"), None);
    assert_eq!(parse_amount("92233720368547758.08"), None);
}

#[test]
fn utf8_input_is_accepted() {
    let src = "\u{feff}#SIETYP 4\r\n#FNAMN \"Åre\"\r\n";
    assert_eq!(parse(src.as_bytes()).unwrap().company.name, "Åre");
}

/// Property: random balanced vouchers sum to 0, survive the round trip, and stay balanced.
#[test]
fn property_voucher_sums_are_zero() {
    let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let mut f = SieFile::new("Prop", d(2026, 1, 1), d(2026, 12, 31));
    for n in 1..=200u32 {
        let k = 1 + (next() % 5) as usize;
        let mut lines: Vec<Line> = (0..k)
            .map(|_| {
                Line::new(
                    1000 + (next() % 8000) as u32,
                    (next() % 2_000_000) as i64 - 1_000_000,
                    None,
                )
            })
            .collect();
        let total: i64 = lines.iter().map(|l| l.amount_minor).sum();
        lines.push(Line::new(1930, -total, None));
        f.vouchers.push(Voucher {
            series: "P".into(),
            number: n.to_string(),
            date: d(2026, 1 + (n % 12) as u8, 1 + (n % 28) as u8),
            text: format!("v{n}"),
            registered: None,
            signed: None,
            lines,
        });
    }
    assert!(f.vouchers.iter().all(|v| v.sum_minor() == 0));
    let bytes = export(&f);
    let p = parse(&bytes).unwrap();
    assert_eq!(p.vouchers.len(), 200);
    assert!(p.vouchers.iter().all(|v| v.sum_minor() == 0));
    assert_eq!(export(&p), bytes);
}
