//! The strict SVG reader and writer (docs/spec/branding.md, section 8; section 13, "SVG reader").

use crate::Refusal;
use crate::svg::{MAX_BYTES, MAX_ELEMENTS, canonical, parse, parse_str};
use crate::testutil::{Rng, text};

const NS: &str = "http://www.w3.org/2000/svg";

/// The wrapper of every small test: a root with a viewBox.
fn wrap(body: &str) -> String {
    format!("<svg xmlns=\"{NS}\" viewBox=\"0 0 64 64\">{body}</svg>")
}

/// Every output of the writer is one line made only of the bytes of section 8, and every `"`
/// delimits an attribute value that the writer produced: it follows `=`, and what follows the
/// closing one is a space, `/` or `>`.
fn check_output(out: &str) {
    assert!(out.len() <= MAX_BYTES, "{} bytes", out.len());
    for b in out.bytes() {
        assert!(
            b.is_ascii_alphanumeric() || b" ,.:/#()=<>\"-".contains(&b),
            "byte {b:#04x} in {out}"
        );
    }
    let parts: Vec<&str> = out.split('"').collect();
    assert!(parts.len() % 2 == 1, "an odd number of quotes in {out}");
    for i in (1..parts.len()).step_by(2) {
        assert!(
            parts[i - 1].ends_with('='),
            "a quote that does not follow = in {out}"
        );
        let after = parts[i + 1];
        assert!(
            after.starts_with([' ', '/', '>']),
            "a closing quote followed by {after:?} in {out}"
        );
        assert!(
            !parts[i].contains(['<', '>', '=', '"']),
            "markup inside a value: {:?}",
            parts[i]
        );
    }
}

/// Reads a file that must be accepted; also checks the output and that printing is idempotent.
fn accept(input: &str) -> String {
    let out = canonical(input.as_bytes())
        .unwrap_or_else(|r| panic!("refused: {:?}\n{input}", messages(&r)));
    check_output(&out);
    let again = canonical(out.as_bytes())
        .unwrap_or_else(|r| panic!("the output was refused: {r:?}\n{out}"));
    assert_eq!(again, out, "print(parse(print(x))) must equal print(x)");
    out
}

fn messages(r: &[Refusal]) -> Vec<String> {
    r.iter()
        .map(|r| format!("{}: {}", r.code, r.message))
        .collect()
}

/// Reads a file that must be refused; returns the refusals.
fn refuse(input: &str) -> Vec<Refusal> {
    match parse(input.as_bytes()) {
        Ok(_) => panic!("accepted: {input}"),
        Err(r) => {
            assert!(!r.is_empty());
            r
        }
    }
}

/// Refused, and a reason names `named`.
fn refuse_naming(input: &str, named: &str) -> Vec<Refusal> {
    let r = refuse(input);
    assert!(
        r.iter().any(|r| r.message.contains(named)),
        "no reason names {named:?}: {:?}\n{input}",
        messages(&r)
    );
    r
}

// ---- What is accepted ---------------------------------------------------------------------------------

/// The "M" artwork of the placeholder: a rounded square and an M.
const M_ARTWORK: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"#495670\"/><path d=\"M 16 48 L 16 16 L 24 16 L 32 30 L 40 16 L 48 16 L 48 48 L 41 48 L 41 29 L 32 44 L 23 29 L 23 48 Z\" fill=\"#ffffff\"/></svg>";

#[test]
fn the_m_artwork_is_accepted_and_reserialises_to_the_same_text() {
    assert_eq!(accept(M_ARTWORK), M_ARTWORK);
}

#[test]
fn a_file_with_groups_transforms_strokes_an_ellipse_a_line_and_a_polygon_is_accepted() {
    let input = r##"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<svg xmlns="http://www.w3.org/2000/svg" width="24px" height="24px" viewBox="0,0,24,24">
  <g fill="#FFF" stroke="currentColor" stroke-width="1.50" transform="translate(2,3) matrix(1,0,0,1,.5,0.5) scale(2)">
    <g fill-rule="evenodd">
      <ellipse cx="12" cy="12" rx="10" ry="5.25" fill="none"/>
      <line x1="0" y1="0" x2="24" y2="24" stroke="#00f" stroke-width="2"/>
    </g>
    <polygon points="0,0 24,0 12,20" fill="#a1b2c3" fill-rule="nonzero"/>
    <circle cx="12" cy="12" r="3"/>
  </g>
  <rect x="1" y="2" width="3" height="4" rx="1" stroke="none"></rect>
</svg>
"##;
    assert_eq!(
        accept(input),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">\
         <g fill=\"#ffffff\" stroke=\"currentColor\" stroke-width=\"1.5\" \
         transform=\"translate(2 3) matrix(1 0 0 1 0.5 0.5) scale(2)\">\
         <g fill-rule=\"evenodd\">\
         <ellipse cx=\"12\" cy=\"12\" rx=\"10\" ry=\"5.25\" fill=\"none\"/>\
         <line x1=\"0\" y1=\"0\" x2=\"24\" y2=\"24\" stroke=\"#0000ff\" stroke-width=\"2\"/>\
         </g>\
         <polygon points=\"0 0 24 0 12 20\" fill=\"#a1b2c3\" fill-rule=\"nonzero\"/>\
         <circle cx=\"12\" cy=\"12\" r=\"3\"/>\
         </g>\
         <rect x=\"1\" y=\"2\" width=\"3\" height=\"4\" rx=\"1\" stroke=\"none\"/>\
         </svg>"
    );
}

#[test]
fn width_and_height_of_the_root_are_ignored_and_not_stored() {
    let out = accept(&format!(
        "<svg xmlns=\"{NS}\" width=\"100%\" height=\"24.5px\" viewBox=\"0 0 1 1\"/>"
    ));
    assert_eq!(
        out,
        format!("<svg xmlns=\"{NS}\" viewBox=\"0 0 1 1\"></svg>")
    );
    assert!(!out.contains("width") && !out.contains("height"));
    // A value that is no plain number is refused even though it would be ignored.
    refuse_naming(
        &format!("<svg xmlns=\"{NS}\" width=\"javascript:1\" viewBox=\"0 0 1 1\"/>"),
        "width",
    );
}

#[test]
fn a_root_without_the_namespace_is_accepted_and_the_output_carries_it() {
    assert_eq!(
        accept("<svg viewBox=\"0 0 1 1\"></svg>"),
        format!("<svg xmlns=\"{NS}\" viewBox=\"0 0 1 1\"></svg>")
    );
}

#[test]
fn fill_rule_evenodd_and_nonzero_are_accepted_and_printed_as_such() {
    let out = accept(&wrap(
        "<path d=\"M0 0\" fill-rule=\"evenodd\"/><path d=\"M0 0\" fill-rule=\"nonzero\"/>",
    ));
    assert!(out.contains("fill-rule=\"evenodd\"") && out.contains("fill-rule=\"nonzero\""));
}

#[test]
fn colours_are_printed_from_the_parsed_value() {
    let out = accept(&wrap(
        "<path d=\"M0 0\" fill=\"#ABC\" stroke=\"#FfEeDd\"/><g fill=\"none\" stroke=\"currentColor\"/>",
    ));
    assert_eq!(
        out,
        wrap_canonical(
            "<path d=\"M 0 0\" fill=\"#aabbcc\" stroke=\"#ffeedd\"/><g fill=\"none\" stroke=\"currentColor\"/>"
        )
    );
}

fn wrap_canonical(body: &str) -> String {
    format!("<svg xmlns=\"{NS}\" viewBox=\"0 0 64 64\">{body}</svg>")
}

#[test]
fn attributes_are_printed_in_one_order_whatever_the_input_order() {
    let a = accept(&wrap(
        "<rect fill=\"#000\" rx=\"1\" height=\"4\" stroke=\"#fff\" width=\"3\" y=\"2\" x=\"1\"/>",
    ));
    let b = accept(&wrap(
        "<rect x=\"1\" y=\"2\" width=\"3\" height=\"4\" rx=\"1\" fill=\"#000\" stroke=\"#fff\"/>",
    ));
    assert_eq!(a, b);
}

// ---- Numbers ------------------------------------------------------------------------------------------

fn circle_r(value: &str) -> Result<String, Vec<Refusal>> {
    canonical(wrap(&format!("<circle r=\"{value}\"/>")).as_bytes()).map(|out| {
        check_output(&out);
        out.split(" r=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_string()
    })
}

fn x_of_rect(value: &str) -> Result<String, Vec<Refusal>> {
    canonical(wrap(&format!("<rect x=\"{value}\"/>")).as_bytes()).map(|out| {
        check_output(&out);
        out.split(" x=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_string()
    })
}

#[test]
fn numbers_are_rounded_to_four_decimals_with_no_trailing_zeros() {
    // The rounding of a long coordinate.
    assert_eq!(x_of_rect("0.30000000000000004").unwrap(), "0.3");
    assert_eq!(x_of_rect("1.23456").unwrap(), "1.2346");
    assert_eq!(x_of_rect("1.23454").unwrap(), "1.2345");
    assert_eq!(x_of_rect("2.50000").unwrap(), "2.5");
    assert_eq!(x_of_rect("12.0").unwrap(), "12");
    assert_eq!(x_of_rect("0.00004").unwrap(), "0");
    assert_eq!(
        x_of_rect("99999.99996").unwrap(),
        "100000",
        "rounds up to the limit"
    );
    // The extremes of the rule.
    assert_eq!(x_of_rect("-100000").unwrap(), "-100000");
    assert_eq!(x_of_rect("100000").unwrap(), "100000");
    assert_eq!(x_of_rect("+5").unwrap(), "5", "an optional sign");
}

#[test]
fn a_number_that_rounds_to_zero_prints_zero_never_minus_zero() {
    for input in [
        "-0", "-0.0", "-0.00001", "+0", "-.0", "-0.", "0.00001", "-0.00004",
    ] {
        assert_eq!(x_of_rect(input).unwrap(), "0", "{input}");
    }
}

#[test]
fn either_side_of_the_point_may_be_empty_but_not_both() {
    assert_eq!(x_of_rect(".5").unwrap(), "0.5");
    assert_eq!(x_of_rect("5.").unwrap(), "5");
    assert_eq!(x_of_rect("-.5").unwrap(), "-0.5");
    assert_eq!(x_of_rect("5.5").unwrap(), "5.5");
    refuse_naming(&wrap("<rect x=\".\"/>"), "expected a number");
    refuse_naming(&wrap("<rect x=\"-\"/>"), "expected a number");
    refuse_naming(&wrap("<rect x=\"\"/>"), "expected a number");
}

#[test]
fn a_number_that_breaks_the_rule_is_refused_and_named() {
    // 25 characters.
    let long = "1.2345678901234567890123";
    assert_eq!(long.len(), 24);
    assert!(x_of_rect(long).is_ok(), "24 characters are allowed");
    let too_long = "1.23456789012345678901234";
    assert_eq!(too_long.len(), 25);
    let r = x_of_rect(too_long).unwrap_err();
    assert!(r[0].message.contains(too_long), "{}", r[0].message);
    // Larger than 100000.
    let r = x_of_rect("100000.1").unwrap_err();
    assert!(r[0].message.contains("100000.1"), "{}", r[0].message);
    assert!(x_of_rect("-100000.1").is_err());
    // An exponent.
    let r = x_of_rect("1e5").unwrap_err();
    assert!(
        r[0].message.contains("1e5") && r[0].message.contains("exponent"),
        "{}",
        r[0].message
    );
    assert!(
        x_of_rect("1E5").is_err() && x_of_rect("1.5e-3").is_err() && x_of_rect("1e+2").is_err()
    );
    // Units and lists where a number stands.
    assert!(x_of_rect("10px").is_err() && x_of_rect("1 2").is_err() && x_of_rect("1,2").is_err());
    assert!(x_of_rect("NaN").is_err() && x_of_rect("inf").is_err() && x_of_rect("0x10").is_err());
    assert!(x_of_rect("1_000").is_err() && x_of_rect("١٢").is_err());
}

#[test]
fn a_size_and_a_radius_cannot_be_negative_but_a_coordinate_can() {
    assert_eq!(x_of_rect("-5").unwrap(), "-5");
    assert!(circle_r("-1").is_err());
    assert_eq!(circle_r("0").unwrap(), "0");
    for attr in ["width", "height", "rx"] {
        assert!(
            canonical(wrap(&format!("<rect {attr}=\"-1\"/>")).as_bytes()).is_err(),
            "{attr}"
        );
    }
    assert!(canonical(wrap("<path d=\"M0 0\" stroke-width=\"-1\"/>").as_bytes()).is_err());
    assert!(canonical(wrap("<path d=\"M0 0\" stroke-width=\"0\"/>").as_bytes()).is_ok());
}

#[test]
fn a_printed_number_is_at_most_twelve_characters() {
    for input in [
        "-99999.99994",
        "-100000",
        "99999.9999",
        "-0.0001",
        "12345.6789",
        "-12345.6789",
    ] {
        let printed = x_of_rect(input).unwrap();
        assert!(printed.len() <= 12, "{input} printed as {printed}");
    }
    assert_eq!(x_of_rect("-99999.9999").unwrap(), "-99999.9999");
    assert_eq!(x_of_rect("-99999.9999").unwrap().len(), 11);
}

// ---- Transforms ---------------------------------------------------------------------------------------

fn transform_of(value: &str) -> Result<String, Vec<Refusal>> {
    canonical(wrap(&format!("<g transform=\"{value}\"/>")).as_bytes()).map(|out| {
        check_output(&out);
        match out.split("transform=\"").nth(1) {
            Some(rest) => rest.split('"').next().unwrap().to_string(),
            None => String::new(),
        }
    })
}

#[test]
fn a_transform_is_reprinted_from_the_parsed_numbers_and_not_folded() {
    assert_eq!(
        transform_of("translate(10,20)scale(2)matrix(1 0 0 1 5 5)").unwrap(),
        "translate(10 20) scale(2) matrix(1 0 0 1 5 5)"
    );
    // Two translates stay two: nothing is combined.
    assert_eq!(
        transform_of("translate(1 1) translate(2 2)").unwrap(),
        "translate(1 1) translate(2 2)"
    );
    assert_eq!(transform_of("scale(1.50 -2.0)").unwrap(), "scale(1.5 -2)");
    assert_eq!(
        transform_of(" translate( 3 , 4 ) , scale(2) ").unwrap(),
        "translate(3 4) scale(2)"
    );
    // The compact grammar inside the parentheses: 1, -0, 0, 1.5, .5, 0.
    assert_eq!(
        transform_of("matrix(1-0 0 1.5.5 0)").unwrap(),
        "matrix(1 0 0 1.5 0.5 0)"
    );
    assert_eq!(transform_of("").unwrap(), "", "an empty transform is none");
}

#[test]
fn at_most_four_functions_and_no_rotation_or_skew() {
    assert!(transform_of("scale(1) scale(1) scale(1) scale(1)").is_ok());
    refuse_naming(
        &wrap("<g transform=\"scale(1) scale(1) scale(1) scale(1) scale(1)\"/>"),
        "more than 4",
    );
    for bad in [
        "rotate(45)",
        "skewX(10)",
        "skewY(10)",
        "rotate(45 1 1)",
        "translateX(1)",
        "Matrix(1 0 0 1 0 0)",
    ] {
        let r = transform_of(bad).unwrap_err();
        assert!(
            r.iter()
                .any(|r| r.message.contains("not allowed") || r.message.contains("matrix")),
            "{bad}"
        );
    }
    // Wrong counts.
    assert!(transform_of("matrix(1 0 0 1 0)").is_err());
    assert!(transform_of("matrix(1 0 0 1 0 0 0)").is_err());
    assert!(transform_of("translate()").is_err());
    assert!(transform_of("translate(1 2 3)").is_err());
    assert!(transform_of("scale(1 2 3)").is_err());
    assert!(
        transform_of("scale(1,)").is_ok(),
        "a trailing comma is tolerated, and printed away"
    );
    assert!(transform_of("translate(1").is_err());
    assert!(transform_of("translate 1").is_err());
    assert!(transform_of("translate(1e2)").is_err());
    assert!(transform_of("translate(a)").is_err());
    assert!(transform_of("url(#x)").is_err());
}

// ---- Path data and points ---------------------------------------------------------------------------

fn d_of(value: &str) -> Result<String, Vec<Refusal>> {
    canonical(wrap(&format!("<path d=\"{value}\"/>")).as_bytes()).map(|out| {
        check_output(&out);
        out.split(" d=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_string()
    })
}

#[test]
fn path_data_is_tokenised_and_re_emitted_separated_by_one_space() {
    assert_eq!(
        d_of("M10,10L20 20 30,30z").unwrap(),
        "M 10 10 L 20 20 30 30 z"
    );
    assert_eq!(d_of("m0 0h5v5H0V0Z").unwrap(), "m 0 0 h 5 v 5 H 0 V 0 Z");
    assert_eq!(
        d_of("M0 0C1 1 2 2 3 3S4 4 5 5Q6 6 7 7T8 8").unwrap(),
        "M 0 0 C 1 1 2 2 3 3 S 4 4 5 5 Q 6 6 7 7 T 8 8"
    );
    // Implicit repeats of a command stay as written.
    assert_eq!(d_of("M0 0 1 1 2 2").unwrap(), "M 0 0 1 1 2 2");
    assert_eq!(d_of("  M 0 \n 0\tL 1 1  ").unwrap(), "M 0 0 L 1 1");
}

#[test]
fn the_compact_grammar_of_svg_is_read_as_the_numbers_it_is() {
    // A sign starts a new number, a second point starts a new number.
    assert_eq!(d_of("M10-5").unwrap(), "M 10 -5");
    assert_eq!(d_of("M.5.5").unwrap(), "M 0.5 0.5");
    assert!(
        d_of("M1.5.5-.5").is_err(),
        "three numbers do not fill M's pairs"
    );
    assert_eq!(d_of("M1.5.5-.5-1").unwrap(), "M 1.5 0.5 -0.5 -1");
    assert_eq!(d_of("M5.-5.").unwrap(), "M 5 -5");
    assert_eq!(d_of("M1+2").unwrap(), "M 1 2");
}

#[test]
fn arc_flags_are_single_characters_that_may_be_packed_with_the_next_number() {
    assert_eq!(d_of("M0 0a1 1 0 011 1").unwrap(), "M 0 0 a 1 1 0 0 1 1 1");
    assert_eq!(d_of("M0 0A1 1 0 1 0 2 2").unwrap(), "M 0 0 A 1 1 0 1 0 2 2");
    assert_eq!(d_of("M0 0A1,1,0,1,0,2,2").unwrap(), "M 0 0 A 1 1 0 1 0 2 2");
    assert_eq!(d_of("M0 0a1 1 0 01-1 2").unwrap(), "M 0 0 a 1 1 0 0 1 -1 2");
    // Packed flags with a fraction after them: 0 1 then .5 and 2.
    assert_eq!(
        d_of("M0 0a1 1 0 01.5 2").unwrap(),
        "M 0 0 a 1 1 0 0 1 0.5 2"
    );
    // Implicit repeats of an arc, each with its own flags.
    assert_eq!(
        d_of("M0 0a1 1 0 011 1 1 1 0 100 2").unwrap(),
        "M 0 0 a 1 1 0 0 1 1 1 1 1 0 1 0 0 2"
    );
    // A flag is 0 or 1 and nothing else.
    for bad in [
        "M0 0a1 1 0 2 0 1 1",
        "M0 0a1 1 0 0 -1 1 1",
        "M0 0a1 1 0 .5 0 1 1",
        "M0 0a1 1 0",
    ] {
        assert!(d_of(bad).is_err(), "{bad}");
    }
}

#[test]
fn path_data_with_anything_else_is_refused() {
    for (bad, why) in [
        ("", "empty"),
        ("   ", "empty"),
        ("L 0 0", "moveto"),
        ("0 0", "moveto"),
        ("M 0", "expected a number"),
        ("M 0 0 L", "expected a number"),
        ("M 0 0 L 1", "expected a number"),
        ("M 0 0 Z 5", "after Z"),
        ("M 0 0 E 1 1", "unexpected"),
        ("M 0 0 X", "unexpected"),
        ("M 0 0 l 1 1 e", "unexpected"),
        ("M 0 0 1e5 1", "exponent"),
        ("M 0 0 100001 1", "larger"),
        ("M 0 0 L 1,,1", "comma"),
        ("M 0 0 L 1 1,", "comma"),
        ("M 0 0 L url(#a)", "expected a number"),
        ("M 0 0 javascript:alert(1)", "unexpected"),
        ("M 0 0 <", "expected a number"),
    ] {
        let r = d_of(bad).unwrap_err();
        assert!(
            r.iter().any(|r| r.message.contains(why)),
            "{bad:?}: {:?}",
            messages(&r)
        );
    }
}

#[test]
fn points_are_pairs_re_emitted_from_the_numbers() {
    let out = accept(&wrap("<polygon points=\"0,0 10-5,5.5 .5.5 1\"/>"));
    assert!(out.contains("points=\"0 0 10 -5 5.5 0.5 0.5 1\""), "{out}");
    assert!(
        canonical(wrap("<polygon points=\"0 0 1\"/>").as_bytes()).is_err(),
        "an odd count"
    );
    assert!(
        canonical(wrap("<polygon points=\"\"/>").as_bytes()).is_err(),
        "empty"
    );
    assert!(canonical(wrap("<polygon points=\"0 0 a b\"/>").as_bytes()).is_err());
    assert!(canonical(wrap("<polygon points=\"0 0, , 1 1\"/>").as_bytes()).is_err());
}

#[test]
fn a_path_or_a_list_over_8_kib_is_refused() {
    let numbers = |n: usize| "1 ".repeat(n / 2);
    let ok = format!("M {}", numbers(8000));
    assert!(ok.len() <= 8192);
    assert!(d_of(ok.trim_end()).is_ok());
    let big = format!("M {}", numbers(8300));
    let r = canonical(wrap(&format!("<path d=\"{big}\"/>")).as_bytes()).unwrap_err();
    assert!(
        r.iter().any(|r| r.message.contains("8 KiB")),
        "{:?}",
        messages(&r)
    );
    let r = canonical(wrap(&format!("<polygon points=\"{big}\"/>")).as_bytes()).unwrap_err();
    assert!(r.iter().any(|r| r.message.contains("8 KiB")));
}

// ---- The root, the declaration, the limits ---------------------------------------------------------

#[test]
fn the_view_box_is_four_finite_numbers_with_a_positive_size() {
    for ok in [
        "0 0 64 64",
        "0,0,24,24",
        "-10 -10 20 20",
        ".5 .5 1 1",
        "0 0 100000 100000",
    ] {
        accept(&format!("<svg viewBox=\"{ok}\"/>"));
    }
    for bad in [
        "",
        "0 0 64",
        "0 0 64 64 64",
        "0 0 0 64",
        "0 0 64 -1",
        "0 0 a b",
        "0 0 1e2 1",
        "0 0 64px 64",
    ] {
        refuse_naming(&format!("<svg viewBox=\"{bad}\"/>"), "viewBox");
    }
    refuse_naming("<svg xmlns=\"http://www.w3.org/2000/svg\"/>", "viewBox");
}

#[test]
fn the_first_line_is_optional_and_only_a_utf8_declaration_is_accepted() {
    let body = "<svg viewBox=\"0 0 1 1\"/>";
    for ok in [
        "<?xml version=\"1.0\"?>",
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<?xml version='1.0' encoding='utf-8' standalone='yes'?>",
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>\n",
    ] {
        accept(&format!("{ok}{body}"));
    }
    refuse_naming(
        &format!("<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>{body}"),
        "UTF-8",
    );
    refuse_naming(&format!("<?xml version=\"1.0\" foo=\"x\"?>{body}"), "foo");
    refuse("<?xml version=\"1.0\"");
    // A declaration after leading white space is a processing instruction like any other.
    refuse_naming(
        &format!(" <?xml version=\"1.0\"?>{body}"),
        "processing instruction",
    );
    refuse_naming(&format!("{body}<?php echo 1 ?>"), "processing instruction");
}

#[test]
fn input_over_16_kib_is_refused_and_16_kib_is_not() {
    let mut exact = "<svg viewBox=\"0 0 1 1\"/>".to_string();
    exact.push_str(&" ".repeat(MAX_BYTES - exact.len()));
    assert_eq!(exact.len(), MAX_BYTES);
    assert!(parse(exact.as_bytes()).is_ok());
    exact.push(' ');
    let r = parse(exact.as_bytes()).unwrap_err();
    assert!(
        r[0].message.contains("16 KiB") || r[0].message.contains("16384"),
        "{}",
        r[0].message
    );
    assert!(parse_str(&exact).is_err());
}

#[test]
fn canonical_text_over_16_kib_is_refused() {
    // ".5" is two bytes on input and "0.5 " four on output. One list stays under 8 KiB when it is
    // written out (7,601 bytes); the text of three of them does not stay under 16 KiB.
    let pairs = format!("M{}", ".5".repeat(1_900));
    let path = format!("<path d=\"{pairs}\"/>");
    let input = wrap(&path.repeat(3));
    assert!(input.len() < MAX_BYTES, "{}", input.len());
    let r = canonical(input.as_bytes()).unwrap_err();
    assert!(
        r.iter().any(|r| r.message.contains("canonical")),
        "{:?}",
        messages(&r)
    );
    // Two of them are fine.
    assert!(canonical(wrap(&path.repeat(2)).as_bytes()).is_ok());
}

#[test]
fn at_most_128_elements_below_the_root() {
    let body = |n: usize| "<path d=\"M0 0\"/>".repeat(n);
    assert!(parse(wrap(&body(MAX_ELEMENTS)).as_bytes()).is_ok());
    let r = refuse(&wrap(&body(MAX_ELEMENTS + 1)));
    assert!(
        r.iter().any(|r| r.message.contains("128")),
        "{:?}",
        messages(&r)
    );
    // Groups count as elements.
    let groups =
        "<g>".repeat(4) + &"<path d=\"M0 0\"/>".repeat(MAX_ELEMENTS - 4) + &"</g>".repeat(4);
    assert!(parse(wrap(&groups).as_bytes()).is_ok());
}

#[test]
fn groups_nest_at_most_four_deep() {
    let nested = |n: usize| format!("{}<path d=\"M0 0\"/>{}", "<g>".repeat(n), "</g>".repeat(n));
    assert!(parse(wrap(&nested(4)).as_bytes()).is_ok());
    let r = refuse(&wrap(&nested(5)));
    assert!(
        r.iter()
            .any(|r| r.message.contains("<g> nested more than 4 deep")),
        "{:?}",
        messages(&r)
    );
    // A shape cannot hold anything.
    refuse_naming(&wrap("<path d=\"M0 0\"><g/></path>"), "cannot hold");
    refuse_naming(&wrap("<rect><rect/></rect>"), "cannot hold");
}

#[test]
fn a_duplicate_attribute_is_refused() {
    refuse_naming(&wrap("<path d=\"M0 0\" d=\"M1 1\"/>"), "twice");
    refuse_naming(&wrap("<g fill=\"#000\" fill=\"#fff\"/>"), "twice");
}

#[test]
fn a_byte_order_mark_and_other_encodings_are_refused() {
    let svg = wrap("");
    let mut with_bom = vec![0xEF, 0xBB, 0xBF];
    with_bom.extend_from_slice(svg.as_bytes());
    let r = parse(&with_bom).unwrap_err();
    assert!(r[0].message.contains("byte order mark"), "{}", r[0].message);
    let r = parse_str(&format!("\u{FEFF}{svg}")).unwrap_err();
    assert!(r[0].message.contains("byte order mark"));
    let mut utf16 = vec![0xFF, 0xFE];
    utf16.extend(svg.encode_utf16().flat_map(u16::to_le_bytes));
    assert!(parse(&utf16).unwrap_err()[0].message.contains("UTF-16"));
    let mut be = vec![0xFE, 0xFF];
    be.extend(svg.encode_utf16().flat_map(u16::to_be_bytes));
    assert!(parse(&be).unwrap_err()[0].message.contains("UTF-16"));
    let mut bad = svg.into_bytes();
    bad.splice(10..10, [0xC3, 0x28]);
    assert!(parse(&bad).unwrap_err()[0].message.contains("UTF-8"));
}

#[test]
fn every_reason_is_listed_up_to_a_limit() {
    let r = refuse(&wrap(
        "<script>x</script><path d=\"M0 0\" onload=\"x\" style=\"y\" fill=\"url(#a)\"/><image href=\"a\"/><!-- c -->",
    ));
    let m = messages(&r).join("\n");
    for named in ["<script>", "onload", "style", "url(", "<image>", "comment"] {
        assert!(m.contains(named), "{named} in\n{m}");
    }
    // A hostile file cannot make a long list.
    let many: String = (0..100).map(|i| format!("<f{i}/>")).collect();
    let r = refuse(&wrap(&many));
    assert_eq!(r.len(), crate::svg::MAX_REFUSALS);
}

// ---- The hostile corpus ----------------------------------------------------------------------------

/// The hostile corpus: `(label, input, a word the list of reasons must hold)`. Shared by the test
/// that refuses each entry and by the totality test, which cuts every entry at every length.
fn hostile_corpus() -> Vec<(&'static str, String, &'static str)> {
    vec![
        ("script", wrap("<script>alert(1)</script>"), "<script>"),
        (
            "script with a CDATA",
            wrap("<script><![CDATA[alert(1)]]></script>"),
            "CDATA",
        ),
        (
            "onload on the root",
            format!("<svg xmlns=\"{NS}\" viewBox=\"0 0 1 1\" onload=\"alert(1)\"/>"),
            "onload",
        ),
        (
            "onload on a path",
            wrap("<path d=\"M0 0\" onload=\"alert(1)\"/>"),
            "onload",
        ),
        ("onclick", wrap("<g onclick=\"alert(1)\"/>"), "onclick"),
        (
            "foreignObject",
            wrap("<foreignObject><div/></foreignObject>"),
            "<foreignObject>",
        ),
        (
            "external href",
            wrap("<path d=\"M0 0\" href=\"http://x.test/y\"/>"),
            "href",
        ),
        (
            "a javascript: href",
            wrap("<path d=\"M0 0\" href=\"javascript:alert(1)\"/>"),
            "href",
        ),
        (
            "javascript: in a fill",
            wrap("<path d=\"M0 0\" fill=\"javascript:alert(1)\"/>"),
            "fill",
        ),
        (
            "javascript: in a transform",
            wrap("<g transform=\"javascript:alert(1)\"/>"),
            "transform",
        ),
        (
            "javascript: in a path",
            wrap("<path d=\"javascript:alert(1)\"/>"),
            "d on <path>",
        ),
        (
            "style attribute",
            wrap("<path d=\"M0 0\" style=\"fill:red\"/>"),
            "style",
        ),
        (
            "style element",
            wrap("<style>*{fill:red}</style>"),
            "<style>",
        ),
        ("use", wrap("<use href=\"#a\"/>"), "<use>"),
        ("text", wrap("<text>hi</text>"), "<text>"),
        ("an entity in text", wrap("a &amp; b"), "entity"),
        (
            "an entity in an attribute",
            wrap("<path d=\"M0 0\" fill=\"&#35;fff\"/>"),
            "fill",
        ),
        (
            "a DOCTYPE",
            format!(
                "<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\">{}",
                wrap("")
            ),
            "DOCTYPE",
        ),
        (
            "a DOCTYPE with an entity",
            format!("<!DOCTYPE svg [<!ENTITY a \"b\">]>{}", wrap("")),
            "DOCTYPE",
        ),
        ("CDATA", wrap("<![CDATA[x]]>"), "CDATA"),
        ("a comment", wrap("<!-- hello -->"), "comment"),
        ("a nested svg", wrap("<svg viewBox=\"0 0 1 1\"/>"), "<svg>"),
        (
            "a url( fill",
            wrap("<path d=\"M0 0\" fill=\"url(#a)\"/>"),
            "url(",
        ),
        (
            "a URL( stroke in capitals",
            wrap("<path d=\"M0 0\" stroke=\"URL(#a)\"/>"),
            "url(",
        ),
        ("a rotate", wrap("<g transform=\"rotate(45)\"/>"), "rotate"),
        (
            "fill-rule inherit",
            wrap("<path d=\"M0 0\" fill-rule=\"inherit\"/>"),
            "inherit",
        ),
        (
            "fill-rule with a second attribute",
            wrap("<path d=\"M0 0\" fill-rule='evenodd\" onload=\"x'/>"),
            "fill-rule",
        ),
        (
            "an oversized path",
            wrap(&format!("<path d=\"M {}\"/>", "1 ".repeat(5000))),
            "8 KiB",
        ),
        (
            "129 elements",
            wrap(&"<path d=\"M0 0\"/>".repeat(129)),
            "128",
        ),
        (
            "a g nested five deep",
            wrap(&format!("{}{}", "<g>".repeat(5), "</g>".repeat(5))),
            "nested",
        ),
        (
            "a second root",
            format!("{}{}", wrap(""), wrap("")),
            "second root",
        ),
        ("text after the root", format!("{}hello", wrap("")), "text"),
        ("defs", wrap("<defs/>"), "<defs>"),
        (
            "a gradient",
            wrap("<linearGradient id=\"a\"/>"),
            "<linearGradient>",
        ),
        (
            "a radial gradient",
            wrap("<radialGradient/>"),
            "<radialGradient>",
        ),
        ("a pattern", wrap("<pattern/>"), "<pattern>"),
        ("a clip path", wrap("<clipPath/>"), "<clipPath>"),
        ("a filter", wrap("<filter/>"), "<filter>"),
        ("a mask", wrap("<mask/>"), "<mask>"),
        (
            "animation",
            wrap("<animate attributeName=\"x\"/>"),
            "<animate>",
        ),
        ("a set element", wrap("<set attributeName=\"x\"/>"), "<set>"),
        ("a link", wrap("<a href=\"x\"/>"), "<a>"),
        (
            "an image",
            wrap("<image href=\"data:image/png;base64,AAAA\"/>"),
            "<image>",
        ),
        (
            "a polyline",
            wrap("<polyline points=\"0 0 1 1\"/>"),
            "<polyline>",
        ),
        ("a title", wrap("<title>x</title>"), "<title>"),
        ("metadata", wrap("<metadata/>"), "<metadata>"),
        (
            "xlink:href",
            wrap("<path d=\"M0 0\" xlink:href=\"#a\"/>"),
            "xlink:href",
        ),
        (
            "xmlns:xlink",
            format!(
                "<svg xmlns=\"{NS}\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 1 1\"/>"
            ),
            "xmlns:xlink",
        ),
        (
            "another xmlns",
            "<svg xmlns=\"http://evil.test/ns\" viewBox=\"0 0 1 1\"/>".to_string(),
            "xmlns",
        ),
        (
            "an opacity",
            wrap("<path d=\"M0 0\" opacity=\".5\"/>"),
            "opacity",
        ),
        ("an id", wrap("<path d=\"M0 0\" id=\"a\"/>"), "id"),
        ("a class", wrap("<path d=\"M0 0\" class=\"a\"/>"), "class"),
        (
            "a stroke-dasharray",
            wrap("<path d=\"M0 0\" stroke-dasharray=\"1 2\"/>"),
            "stroke-dasharray",
        ),
        ("a ry on a rect", wrap("<rect ry=\"1\"/>"), "ry"),
        (
            "an xml:space",
            format!("<svg xmlns=\"{NS}\" viewBox=\"0 0 1 1\" xml:space=\"preserve\"/>"),
            "xml:space",
        ),
        (
            "a version",
            format!("<svg xmlns=\"{NS}\" version=\"1.1\" viewBox=\"0 0 1 1\"/>"),
            "version",
        ),
        (
            "a processing instruction",
            wrap("<?php system('x') ?>"),
            "processing instruction",
        ),
        (
            "an unclosed element",
            "<svg viewBox=\"0 0 1 1\"><g>".to_string(),
            "ends inside",
        ),
        ("a mismatched end tag", wrap("<g></path>"), "does not match"),
        (
            "an unquoted attribute",
            "<svg viewBox=0/>".to_string(),
            "not quoted",
        ),
        ("a root that is not svg", "<html/>".to_string(), "<html>"),
        ("nothing", String::new(), "no <svg>"),
        ("only a comment", "<!-- x -->".to_string(), "comment"),
        (
            "a NUL",
            wrap("<path d=\"M0 0\u{0}\"/>"),
            "expected a number",
        ),
    ]
}

#[test]
fn the_hostile_corpus_is_refused_with_a_named_reason() {
    for (label, input, named) in hostile_corpus() {
        let r = refuse(&input);
        assert!(
            r.iter().any(|r| r.message.contains(named)),
            "{label}: no reason names {named:?}: {:?}",
            messages(&r)
        );
        // A refusal never repeats a character that could do harm on a page or in a log.
        for refusal in &r {
            assert!(
                !refusal
                    .message
                    .contains(['\n', '\0', '\u{1b}', '\u{202E}', '\r']),
                "{label}: {:?}",
                refusal.message
            );
        }
    }
}

#[test]
fn a_refusal_names_the_element_even_when_the_name_is_hostile() {
    let r = refuse(&wrap("<a\u{0}b/>"));
    assert!(!r[0].message.contains('\u{0}'));
    let r = refuse(&wrap(&format!("<{} />", "x".repeat(200))));
    assert!(r.iter().all(|r| r.message.len() < 400), "{r:?}");
    let r = refuse(&wrap("<path d=\"M0 0\" on\u{202E}load=\"1\"/>"));
    assert!(!r.is_empty());
}

// ---- Total: never panics, depth bounded ----------------------------------------------------------------

/// A number as a design tool, an optimiser or a person might write it.
fn num(rng: &mut Rng) -> String {
    match rng.below(10) {
        0 => format!("{}", rng.below(2000)),
        1 => format!("{}.{}", rng.below(300), rng.below(100_000)),
        2 => format!(".{}", rng.below(10_000)),
        3 => format!("{}.", rng.below(500)),
        4 => format!("-{}.{}", rng.below(100), rng.below(1000)),
        5 => format!("+{}", rng.below(50)),
        6 => [
            "-0",
            "-0.0",
            "-0.00001",
            "0.30000000000000004",
            "100000",
            "-100000",
            "0.5",
        ][rng.below(7)]
        .to_string(),
        7 => format!("{}.{:06}", rng.below(99), rng.below(1_000_000)),
        8 => format!("{}", rng.below(100_000)),
        _ => format!("-.{}", rng.below(1000)),
    }
}

fn nums(rng: &mut Rng, n: usize) -> Vec<String> {
    (0..n).map(|_| num(rng)).collect()
}

/// Joins number texts the way the grammar allows, compact forms included.
fn join(rng: &mut Rng, items: &[String]) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            let compact_ok = item.starts_with(['-', '+'])
                || (item.starts_with('.') && items[i - 1].contains('.'));
            let seps = [" ", ",", ", ", "\n", " \t", " , "];
            if !(compact_ok && rng.below(3) == 0) {
                out.push_str(rng.pick(&seps));
            }
        }
        out.push_str(item);
    }
    out
}

fn join_nums(rng: &mut Rng, n: usize) -> String {
    let items = nums(rng, n);
    join(rng, &items)
}

fn path_text(rng: &mut Rng) -> String {
    // Now and then a long compact run: `.5` is two bytes on input and four on output, so it passes
    // the 8 KiB of the input and may not pass it written out.
    if rng.below(40) == 0 {
        return format!("M{}", ".5".repeat(2 * (750 + rng.below(1_500))));
    }
    let mut parts: Vec<String> = Vec::new();
    let mut tokens = format!("M{}", join_nums(rng, 2));
    for _ in 0..rng.below(8) {
        let cmds = b"MmLlHhVvCcSsQqTtAaZz";
        let c = cmds[rng.below(cmds.len())];
        let arity = match c {
            b'Z' | b'z' => 0,
            b'H' | b'h' | b'V' | b'v' => 1,
            b'M' | b'm' | b'L' | b'l' | b'T' | b't' => 2,
            b'S' | b's' | b'Q' | b'q' => 4,
            b'C' | b'c' => 6,
            _ => 7,
        };
        parts.push(tokens);
        tokens = String::new();
        tokens.push(c as char);
        if arity == 0 {
            continue;
        }
        for _ in 0..=rng.below(2) {
            if arity == 7 {
                let head = join_nums(rng, 3);
                let (f1, f2) = (rng.below(2), rng.below(2));
                let tail = join_nums(rng, 2);
                let sep = if rng.below(2) == 0 { " " } else { "," };
                // The flags may be separate, packed with each other, or packed with the next
                // number as well.
                let flags = match rng.below(3) {
                    0 => format!("{f1} {f2} {tail}"),
                    1 => format!("{f1}{f2} {tail}"),
                    _ => format!("{f1}{f2}{tail}"),
                };
                tokens.push_str(&format!("{head}{sep}{flags} "));
            } else {
                tokens.push_str(&join_nums(rng, arity));
                tokens.push(' ');
            }
        }
    }
    parts.push(tokens);
    parts.concat()
}

fn paint_text(rng: &mut Rng) -> String {
    [
        "#abc",
        "#A1B2C3",
        "none",
        "currentColor",
        "#000",
        "#FFFFFF",
        "#0f766e",
    ][rng.below(7)]
    .to_string()
}

fn element_text(rng: &mut Rng, depth: usize, budget: &mut usize) -> String {
    if *budget == 0 {
        return String::new();
    }
    *budget -= 1;
    let kinds = ["g", "path", "rect", "circle", "ellipse", "line", "polygon"];
    let kind = if depth >= 4 {
        kinds[1 + rng.below(6)]
    } else {
        kinds[rng.below(7)]
    };
    let mut attrs: Vec<String> = Vec::new();
    let give = |rng: &mut Rng, attrs: &mut Vec<String>, name: &str, value: String| {
        if rng.below(3) != 0 {
            attrs.push(format!("{name}=\"{value}\""));
        }
    };
    match kind {
        "path" => {
            let d = path_text(rng);
            give(rng, &mut attrs, "d", d);
        }
        "rect" => {
            for n in ["x", "y", "width", "height", "rx"] {
                let v = if matches!(n, "width" | "height" | "rx") {
                    format!("{}", rng.below(300))
                } else {
                    num(rng)
                };
                give(rng, &mut attrs, n, v);
            }
        }
        "circle" => {
            for n in ["cx", "cy"] {
                let v = num(rng);
                give(rng, &mut attrs, n, v);
            }
            let r = format!("{}.{}", rng.below(50), rng.below(1000));
            give(rng, &mut attrs, "r", r);
        }
        "ellipse" => {
            for n in ["cx", "cy"] {
                let v = num(rng);
                give(rng, &mut attrs, n, v);
            }
            let rx = format!("{}", rng.below(50));
            give(rng, &mut attrs, "rx", rx);
            let ry = format!("{}.5", rng.below(50));
            give(rng, &mut attrs, "ry", ry);
        }
        "line" => {
            for n in ["x1", "y1", "x2", "y2"] {
                let v = num(rng);
                give(rng, &mut attrs, n, v);
            }
        }
        "polygon" => {
            let n = 2 * (1 + rng.below(5));
            let points = if rng.below(40) == 0 {
                ".5".repeat(2 * (750 + rng.below(1_500)))
            } else {
                join_nums(rng, n)
            };
            give(rng, &mut attrs, "points", points);
        }
        _ => {}
    }
    for n in ["fill", "stroke"] {
        let v = paint_text(rng);
        give(rng, &mut attrs, n, v);
    }
    let rule = ["evenodd", "nonzero"][rng.below(2)].to_string();
    give(rng, &mut attrs, "fill-rule", rule);
    let width = format!("{}.{}", rng.below(10), rng.below(100));
    give(rng, &mut attrs, "stroke-width", width);
    if rng.below(3) == 0 {
        let mut fns: Vec<String> = Vec::new();
        for _ in 0..=rng.below(3) {
            fns.push(match rng.below(3) {
                0 => format!("matrix({})", join_nums(rng, 6)),
                1 => {
                    let n = 1 + rng.below(2);
                    format!("translate({})", join_nums(rng, n))
                }
                _ => {
                    let n = 1 + rng.below(2);
                    format!("scale({})", join_nums(rng, n))
                }
            });
        }
        let sep = if rng.below(2) == 0 { " " } else { ", " };
        attrs.push(format!("transform=\"{}\"", fns.join(sep)));
    }
    // Shuffle the attribute order.
    for i in (1..attrs.len()).rev() {
        let j = rng.below(i + 1);
        attrs.swap(i, j);
    }
    let open = format!(
        "<{kind}{}{}",
        if attrs.is_empty() { "" } else { " " },
        attrs.join(" ")
    );
    if kind == "g" && rng.below(4) != 0 {
        let mut inner = String::new();
        for _ in 0..rng.below(4) {
            inner.push_str(&element_text(rng, depth + 1, budget));
            if rng.below(3) == 0 {
                inner.push('\n');
            }
        }
        format!("{open}>{inner}</g>")
    } else if rng.below(2) == 0 {
        format!("{open}/>")
    } else {
        format!("{open}></{kind}>")
    }
}

fn document_text(rng: &mut Rng) -> String {
    let mut budget = 1 + rng.below(40);
    let mut body = String::new();
    while budget > 0 {
        body.push_str(&element_text(rng, 0, &mut budget));
        if rng.below(2) == 0 {
            body.push_str("\n  ");
        }
    }
    let decl = if rng.below(3) == 0 {
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
    } else {
        ""
    };
    let size = if rng.below(2) == 0 {
        " width=\"24\" height=\"24\""
    } else {
        ""
    };
    // The size of the view box is positive; its origin is any number. Now and then the size is tiny:
    // the writer rounds to 4 decimals, so 0.00004 prints as 0 and the file is refused.
    let view_box = {
        let mut items = nums(rng, 2);
        for _ in 0..2 {
            items.push(if rng.below(8) == 0 {
                rng.pick(&["0.00004", "0.0001", ".00005", "0.00001", "0.5"])
                    .to_string()
            } else {
                format!("{}", 1 + rng.below(500))
            });
        }
        join(rng, &items)
    };
    format!("{decl}<svg xmlns=\"{NS}\"{size} viewBox=\"{view_box}\">{body}</svg>\n")
}

/// A property test over a fixed-seed generator written in the test: whatever the reader accepts, the
/// output has only the bytes of section 8 and the quote rule holds, the reader accepts its own
/// output (what `configure` stores is read back), and the printer is idempotent:
/// `print(parse(print(x)))` equals `print(x)` byte for byte.
#[test]
fn the_printer_is_idempotent_over_generated_files() {
    let mut rng = Rng::new(0x5EED_5151);
    let mut accepted = 0;
    for n in 0..3_000 {
        let input = document_text(&mut rng);
        match parse(input.as_bytes()) {
            Ok(svg) => {
                accepted += 1;
                let p1 = svg.print();
                check_output(&p1);
                let p2 = canonical(p1.as_bytes())
                    .unwrap_or_else(|r| panic!("#{n} the output was refused: {r:?}\n{p1}"));
                assert_eq!(p2, p1, "#{n}\n{input}");
                // The inline form differs by the size only.
                let inline = svg.print_inline(28);
                assert_eq!(inline.replacen(" width=\"28\" height=\"28\"", "", 1), p1);
            }
            Err(r) => {
                // A generated file may break a rule by chance (a flag packed oddly, a negative
                // width); the reason is always one of the reader's.
                assert!(
                    r.iter().all(|r| r.code.starts_with("svg_")),
                    "#{n} {r:?}\n{input}"
                );
            }
        }
    }
    assert!(
        accepted > 1_500,
        "the generator should be accepted most of the time: {accepted}"
    );
}

/// Every accepted file of this module goes through `accept`, which checks the same two things; this
/// runs the fixed ones again as one list so a printer change that moves one is seen at once.
#[test]
fn every_accepted_file_of_the_tests_is_idempotent() {
    for input in [
        M_ARTWORK,
        "<svg viewBox=\"0 0 1 1\"/>",
        "<svg viewBox=\"0 0 24 24\"><g transform=\"translate(.5)\"><path d=\"M10-5L.5.5a1 1 0 011 1z\"/></g></svg>",
        "<svg viewBox=\"-0 -0.0 1 1\"/>",
    ] {
        accept(input);
    }
}

#[test]
fn the_reader_never_panics_on_truncated_mutated_and_hostile_text() {
    let mut rng = Rng::new(0xB4A9_D003);
    let mut seeds: Vec<String> = (0..25).map(|_| document_text(&mut rng)).collect();
    seeds.push(M_ARTWORK.to_string());
    seeds.push(wrap(
        "<script>alert(1)</script><!-- c --><![CDATA[x]]><?php ?>&amp;<a href='x'/>",
    ));
    seeds.push(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE svg [<!ENTITY a \"b\">]><svg/>"
            .to_string(),
    );
    seeds.push(wrap(&"<g>".repeat(300)));
    seeds.push(wrap(&"</g>".repeat(300)));
    // The hostile corpus itself: every entry is cut at every length below, and mutated.
    let corpus = hostile_corpus();
    assert!(corpus.len() >= 60, "{} entries", corpus.len());
    let corpus_seeds = corpus.len();
    seeds.extend(corpus.into_iter().map(|(_, input, _)| input));
    assert!(seeds.len() >= corpus_seeds + 25);
    // One cut for every character boundary and the end of every seed, at least.
    let cuts: usize = seeds.iter().map(|s| s.chars().count() + 1).sum();
    let outcome = std::panic::catch_unwind(move || {
        let mut tried = 0usize;
        for seed in &seeds {
            // Cut at every length (at every character boundary).
            for (end, _) in seed.char_indices().chain([(seed.len(), ' ')]) {
                let prefix = &seed[..end];
                let _ = parse_str(prefix);
                let _ = parse(prefix.as_bytes());
                tried += 1;
            }
            // Mutations: delete, insert or replace a character.
            for _ in 0..20 {
                let mut chars: Vec<char> = seed.chars().collect();
                if chars.is_empty() {
                    break;
                }
                let at = rng.below(chars.len());
                match rng.below(3) {
                    0 => {
                        chars.remove(at);
                    }
                    1 => chars.insert(at, *rng.pick(crate::testutil::NASTY)),
                    _ => {
                        chars[at] = *rng.pick(&[
                            '<', '>', '"', '\'', '=', '/', ' ', '&', '-', '.', 'e', '0', '9', '#',
                            '(', ')', ',', '?', '!',
                        ])
                    }
                }
                let mutated: String = chars.into_iter().collect();
                if let Ok(svg) = parse_str(&mutated) {
                    let p1 = svg.print();
                    check_output(&p1);
                    assert_eq!(canonical(p1.as_bytes()).unwrap(), p1);
                }
                tried += 1;
            }
            // Random garbage with the reader's own alphabet.
            for _ in 0..5 {
                let _ = parse_str(&text(&mut rng, 300));
            }
        }
        // Random bytes: not valid UTF-8 most of the time.
        for _ in 0..200 {
            let n = rng.below(400);
            let bytes: Vec<u8> = (0..n).map(|_| (rng.next() & 0xFF) as u8).collect();
            let _ = parse(&bytes);
        }
        tried
    });
    let tried = outcome.expect("the reader panicked");
    assert!(tried > 10_000 && tried >= cuts, "{tried} of {cuts} cuts");
}

#[test]
fn nesting_is_bounded_and_a_deep_file_is_refused_without_overflow() {
    // 16 KiB of opening tags, the worst a file can do.
    let deep = format!(
        "<svg viewBox=\"0 0 1 1\">{}",
        "<g>".repeat(MAX_BYTES / 3 - 40)
    );
    assert!(deep.len() <= MAX_BYTES);
    let r = parse(deep.as_bytes()).unwrap_err();
    assert!(
        r.iter()
            .any(|r| r.message.contains("nested") || r.message.contains("128")),
        "{:?}",
        messages(&r)
    );
    // And closed properly, at 17 levels: refused as nesting, not as a crash.
    let closed = format!(
        "{}{}{}",
        "<svg viewBox=\"0 0 1 1\">",
        "<g>".repeat(17),
        "</g>".repeat(17)
    );
    assert!(parse(format!("{closed}</svg>").as_bytes()).is_err());
    // Attributes: a hostile element with hundreds of attributes.
    let many: String = (0..500).map(|i| format!(" a{i}=\"1\"")).collect();
    let r = parse(wrap(&format!("<g{many}/>")).as_bytes()).unwrap_err();
    assert!(r.iter().any(|r| r.message.contains("attributes")));
}

// ---- What the reader accepts is read back; the numbers of section 8 are pinned by value ------------------

/// Whether a reason of the list has this code.
fn has_code(r: &[Refusal], code: &str) -> bool {
    r.iter().any(|r| r.code == code)
}

#[test]
fn a_size_that_prints_as_zero_is_refused_because_the_print_would_not_be_read_back() {
    // The writer rounds to 4 decimals: 0.00004 prints as 0, and the read side refuses a size of 0.
    for bad in [
        "0 0 0.00004 64",
        "0 0 64 0.00004",
        "0 0 0.00001 0.00001",
        "0 0 .00004 .00004",
        "5 5 0.00004 1",
    ] {
        let r = refuse_naming(&format!("<svg viewBox=\"{bad}\"/>"), "viewBox");
        assert!(has_code(&r, "svg_canonical"), "{bad}: {:?}", messages(&r));
    }
    // The smallest size that survives the print is 0.0001; an origin that rounds to 0 is fine.
    assert_eq!(
        accept("<svg viewBox=\"0 0 0.0001 0.0001\"/>"),
        format!("<svg xmlns=\"{NS}\" viewBox=\"0 0 0.0001 0.0001\"></svg>")
    );
    assert_eq!(
        accept("<svg viewBox=\"0.00004 -0.00004 1 1\"/>"),
        format!("<svg xmlns=\"{NS}\" viewBox=\"0 0 1 1\"></svg>")
    );
}

#[test]
fn a_compact_list_that_prints_longer_than_8_kib_is_refused_when_it_is_read() {
    // `.5` is two bytes on input and `0.5 ` four on output.
    let points = |k: usize, negatives: usize| {
        let mut list = String::new();
        for i in 0..k {
            list.push_str(if i < negatives { "-.5" } else { ".5" });
        }
        wrap(&format!("<polygon points=\"{list}\"/>"))
    };
    // 2048 numbers print as 8191 bytes, and one `-` more as 8192: accepted.
    let printed = canonical(points(2048, 0).as_bytes()).unwrap();
    assert!(printed.contains(&format!("points=\"{}\"", ["0.5"; 2048].join(" "))));
    let exact = canonical(points(2048, 1).as_bytes()).unwrap();
    assert_eq!(exact.split('"').nth(5).unwrap().len(), 8_192);
    // 8193 and more: refused, though the input list is under 8 KiB (4,097 bytes and more).
    for (k, negatives) in [(2048, 2), (2050, 0), (3_000, 0)] {
        let input = points(k, negatives);
        let r = canonical(input.as_bytes()).unwrap_err();
        assert!(has_code(&r, "svg_canonical"), "{k}: {:?}", messages(&r));
        assert!(
            r.iter()
                .all(|r| r.message.contains("points on <polygon>") && r.message.contains("8 KiB")),
            "{k}: {:?}",
            messages(&r)
        );
    }
    // The same for a path: `Z` is one byte on input and two on output.
    let path = |start: &str, zs: usize| wrap(&format!("<path d=\"{start}{}\"/>", "Z".repeat(zs)));
    let printed = canonical(path("M10 0", 4_093).as_bytes()).unwrap();
    assert_eq!(printed.split('"').nth(5).unwrap().len(), 8_192);
    assert!(canonical(path("M0 0", 4_093).as_bytes()).is_ok(), "8191");
    for (start, zs) in [("M0 0", 4_094), ("M10 0", 4_094), ("M0 0", 4_100)] {
        let input = path(start, zs);
        let r = canonical(input.as_bytes()).unwrap_err();
        assert!(has_code(&r, "svg_canonical"), "{start} {zs}");
        assert!(
            r.iter()
                .all(|r| r.message.contains("d on <path>") && r.message.contains("8 KiB")),
            "{start} {zs}: {:?}",
            messages(&r)
        );
    }
}

#[test]
fn the_numbers_of_section_8_are_pinned_by_value() {
    assert_eq!(MAX_BYTES, 16_384);
    assert_eq!(MAX_ELEMENTS, 128);
    assert_eq!(crate::svg::MAX_GROUP_DEPTH, 4);
    assert_eq!(crate::svg::MAX_LIST_BYTES, 8_192);
    assert_eq!(crate::svg::MAX_TRANSFORMS, 4);
    assert_eq!(crate::svg::MAX_NUMBER_CHARS, 24);
    assert!((crate::svg::MAX_ABS - 100_000.0).abs() < f64::EPSILON);
    assert_eq!(crate::svg::MAX_REFUSALS, 20);
}

#[test]
fn the_input_limit_is_16384_bytes_to_the_byte() {
    let mut exact = String::from("<svg viewBox=\"0 0 1 1\"/>");
    exact.push_str(&" ".repeat(16_384 - exact.len()));
    assert_eq!(exact.len(), 16_384);
    assert!(parse(exact.as_bytes()).is_ok());
    assert!(parse_str(&exact).is_ok());
    exact.push(' ');
    assert_eq!(exact.len(), 16_385);
    assert!(parse(exact.as_bytes()).is_err());
    assert!(parse_str(&exact).is_err());
}

/// A file whose canonical text is exactly `target` bytes. It is written in the compact form so that
/// the file itself stays small: `.5` is two bytes on input and ` 0.5` four on output, and `-.5` is
/// three and ` -0.5` five, which gives a step of one byte.
fn file_with_canonical_len(target: usize) -> String {
    let empty = canonical(wrap("").as_bytes()).unwrap().len();
    let remaining = target - empty;
    for paths in 1..=8usize {
        for negatives in 0..8usize {
            // Each path is `<path d="M` + ` 0.5` for each number + `"/>`: 13 bytes and 4 per number.
            let Some(rest) = remaining.checked_sub(13 * paths + negatives) else {
                continue;
            };
            if rest % 8 != 0 {
                continue;
            }
            let mut numbers = rest / 4;
            if numbers < 2 * paths || numbers > 2_046 * paths {
                continue;
            }
            let mut negatives_left = negatives;
            let mut body = String::new();
            for i in 0..paths {
                let reserve = 2 * (paths - i - 1);
                let k = (numbers - reserve).min(2_046);
                numbers -= k;
                let mut d = String::from("M");
                for _ in 0..k {
                    if negatives_left > 0 {
                        negatives_left -= 1;
                        d.push_str("-.5");
                    } else {
                        d.push_str(".5");
                    }
                }
                body.push_str(&format!("<path d=\"{d}\"/>"));
            }
            let input = wrap(&body);
            assert!(input.len() <= 16_384, "{} bytes in", input.len());
            return input;
        }
    }
    panic!("no file of {target} bytes");
}

#[test]
fn the_canonical_limit_is_16384_bytes_to_the_byte() {
    let at = file_with_canonical_len(16_384);
    assert!(at.len() < 16_384, "the input is smaller than its print");
    assert_eq!(canonical(at.as_bytes()).unwrap().len(), 16_384);
    for over in [16_385, 16_386, 17_000] {
        let file = file_with_canonical_len(over);
        let r = canonical(file.as_bytes()).unwrap_err();
        assert!(
            r.iter().any(|r| r.code == "svg_too_large"
                && r.message.contains(&format!("{over} bytes"))
                && r.message.contains("16384")),
            "{over}: {:?}",
            messages(&r)
        );
    }
}

#[test]
fn a_list_of_8192_bytes_on_input_is_read_and_8193_is_refused() {
    let padded = |head: &str, total: usize| format!("{head}{}", " ".repeat(total - head.len()));
    let d = |total: usize| wrap(&format!("<path d=\"{}\"/>", padded("M 0 0", total)));
    let points = |total: usize| wrap(&format!("<polygon points=\"{}\"/>", padded("0 0", total)));
    assert!(canonical(d(8_192).as_bytes()).is_ok());
    assert!(canonical(points(8_192).as_bytes()).is_ok());
    for refused in [d(8_193), points(8_193)] {
        let r = canonical(refused.as_bytes()).unwrap_err();
        assert!(
            r.iter().any(|r| r.message.contains("8192 bytes (8 KiB)")),
            "{:?}",
            messages(&r)
        );
    }
}

#[test]
fn identical_reasons_are_listed_once() {
    let r = refuse(&wrap(&"<script/>".repeat(100)));
    assert_eq!(r.len(), 1, "{:?}", messages(&r));
    assert!(r[0].message.contains("<script>"));
}

#[test]
fn a_radius_cannot_be_negative_and_a_transform_cannot_start_with_a_comma() {
    for attr in ["rx", "ry"] {
        let r = refuse_naming(&wrap(&format!("<ellipse {attr}=\"-1\"/>")), attr);
        assert!(r.iter().any(|r| r.message.contains("negative")));
    }
    assert!(canonical(wrap("<ellipse rx=\"0\" ry=\"0\"/>").as_bytes()).is_ok());
    assert!(transform_of(",translate(1)").is_err());
    assert!(transform_of("translate(1),scale(2)").is_ok());
}
