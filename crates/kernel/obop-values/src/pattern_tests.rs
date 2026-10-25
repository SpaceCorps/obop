//! The regular expressions and examples that OpenAPI, MCP and the error messages print for a
//! decimal field (D72) against the parser that decides what a decimal is. A small matcher for the
//! subset the patterns use stands in for a regex crate, which would be a new dependency for
//! tests alone.

use crate::decimal::{Decimal, Round, example, import_pattern, pattern};

/// One occurrence of a thing, repeated between `min` and `max` times.
struct Item {
    node: Node,
    min: u32,
    max: u32,
}

enum Node {
    Class(Vec<(char, char)>),
    Group(Vec<Vec<Item>>),
}

fn parse_alts(c: &[char], i: &mut usize) -> Vec<Vec<Item>> {
    let mut alts = vec![Vec::new()];
    while *i < c.len() && c[*i] != ')' {
        let node = match c[*i] {
            '|' => {
                *i += 1;
                alts.push(Vec::new());
                continue;
            }
            // The whole text must match, so the anchors need no node.
            '^' | '$' => {
                *i += 1;
                continue;
            }
            '(' => {
                *i += 1;
                let g = parse_alts(c, i);
                assert_eq!(c[*i], ')', "unclosed group");
                *i += 1;
                Node::Group(g)
            }
            '[' => {
                *i += 1;
                let mut ranges = Vec::new();
                while c[*i] != ']' {
                    if c[*i + 1] == '-' && c[*i + 2] != ']' {
                        ranges.push((c[*i], c[*i + 2]));
                        *i += 3;
                    } else {
                        ranges.push((c[*i], c[*i]));
                        *i += 1;
                    }
                }
                *i += 1;
                Node::Class(ranges)
            }
            '\\' => {
                *i += 2;
                Node::Class(vec![(c[*i - 1], c[*i - 1])])
            }
            ch => {
                assert!(!"?+*{}".contains(ch), "a quantifier with nothing to repeat");
                *i += 1;
                Node::Class(vec![(ch, ch)])
            }
        };
        let (min, max) = match c.get(*i) {
            Some('?') => {
                *i += 1;
                (0, 1)
            }
            Some('+') => {
                *i += 1;
                (1, u32::MAX)
            }
            Some('{') => {
                let end = c[*i..].iter().position(|&x| x == '}').expect("unclosed {") + *i;
                let body: String = c[*i + 1..end].iter().collect();
                *i = end + 1;
                match body.split_once(',') {
                    Some((a, b)) => (a.parse().unwrap(), b.parse().unwrap()),
                    None => (body.parse().unwrap(), body.parse().unwrap()),
                }
            }
            _ => (1, 1),
        };
        alts.last_mut().unwrap().push(Item { node, min, max });
    }
    alts
}

fn node(n: &Node, t: &[char], pos: usize, k: &dyn Fn(usize) -> bool) -> bool {
    match n {
        Node::Class(r) => {
            pos < t.len() && r.iter().any(|&(a, b)| (a..=b).contains(&t[pos])) && k(pos + 1)
        }
        Node::Group(alts) => alts.iter().any(|a| seq(a, t, pos, k)),
    }
}

fn item(it: &Item, t: &[char], pos: usize, count: u32, k: &dyn Fn(usize) -> bool) -> bool {
    (count < it.max
        && node(&it.node, t, pos, &|p| {
            p != pos && item(it, t, p, count + 1, k)
        }))
        || (count >= it.min && k(pos))
}

fn seq(items: &[Item], t: &[char], pos: usize, k: &dyn Fn(usize) -> bool) -> bool {
    match items.split_first() {
        None => k(pos),
        Some((first, rest)) => item(first, t, pos, 0, &|p| seq(rest, t, p, k)),
    }
}

/// Whether the whole of `text` matches `re`.
fn matches(re: &str, text: &str) -> bool {
    let c: Vec<char> = re.chars().collect();
    let alts = parse_alts(&c, &mut 0);
    let t: Vec<char> = text.chars().collect();
    alts.iter().any(|a| seq(a, &t, 0, &|p| p == t.len()))
}

#[test]
fn the_matcher_reads_the_patterns_it_is_used_for() {
    let re = "^-?(0|[1-9][0-9]{0,2})(\\.[0-9]{1,2})?$";
    for ok in ["0", "-0", "7", "999", "-12.5", "0.05", "10.00"] {
        assert!(matches(re, ok), "{ok}");
    }
    for bad in ["", "-", "00", "01", "1000", "1.", ".5", "1.234", "1,5", " 1", "1 ", "--1"] {
        assert!(!matches(re, bad), "{bad:?}");
    }
    assert!(matches("^a+b?$", "aaab") && !matches("^a+b?$", "b"));
}

/// Texts of every shape that matters near the edges of a decimal of `precision` and `scale`.
fn candidates(precision: u8, scale: u8) -> Vec<String> {
    let int_digits = usize::from(precision - scale);
    let mut ints = vec!["", "0", "00", "01", "5", "10", "1234567"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    for n in [int_digits.saturating_sub(1), int_digits, int_digits + 1] {
        ints.push("9".repeat(n));
        ints.push(format!("1{}", "0".repeat(n.saturating_sub(1))));
    }
    let mut out = Vec::new();
    for sign in ["", "-", "+"] {
        for int in &ints {
            out.push(format!("{sign}{int}"));
            out.push(format!("{sign}{int}."));
            for j in 1..=usize::from(scale) + 2 {
                out.push(format!("{sign}{int}.{}", "5".repeat(j)));
            }
        }
    }
    out
}

/// A spelling the pattern describes: an optional minus, an integer part without leading zeros
/// and, perhaps, a point and digits. (The parser also takes `007.50` and normalises it.)
fn is_plain(text: &str) -> bool {
    let t = text.strip_prefix('-').unwrap_or(text);
    let (int, frac) = t.split_once('.').map_or((t, Some("")), |(i, f)| (i, Some(f)));
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    !int.is_empty()
        && digits(int)
        && (int == "0" || !int.starts_with('0'))
        && frac.is_some_and(|f| digits(f) && (!t.contains('.') || !f.is_empty()))
}

#[test]
fn the_pattern_is_the_language_of_the_parser_for_every_shape() {
    let mut checked = 0;
    for precision in 1..=38u8 {
        for scale in 0..=precision.min(12) {
            let (strict, lenient) = (
                pattern(precision, scale),
                import_pattern(precision, scale),
            );
            for text in candidates(precision, scale) {
                let parsed = Decimal::parse_for(&text, precision, scale, None).is_ok();
                let fits = matches(&strict, &text);
                // Whatever the schema lets through, the server takes; and every plain spelling
                // the server takes, the schema lets through.
                assert!(
                    !fits || parsed,
                    "decimal({precision}, {scale}) {text:?} matches {strict} and is refused"
                );
                assert!(
                    !(parsed && is_plain(&text)) || fits,
                    "decimal({precision}, {scale}) {text:?} is taken and does not match {strict}"
                );
                // On import extra fractional digits are rounded. (The parser reads at most 38
                // digits in all, a limit the field's `round` does not lift.)
                let digits = text.chars().filter(char::is_ascii_digit).count();
                if digits <= 38 {
                    let rounds =
                        Decimal::parse_for(&text, precision, scale, Some(Round::HalfEven)).is_ok();
                    let fits = matches(&lenient, &text);
                    // Rounding 99.5 up carries into a digit the precision does not have: a schema
                    // cannot say that, and the server refuses it with the precision message.
                    let int = text.trim_start_matches('-').split('.').next().unwrap_or("");
                    let carries = text.contains('.') && !int.is_empty() && int.chars().all(|c| c == '9');
                    assert!(
                        !fits || rounds || carries,
                        "decimal({precision}, {scale}) {text:?} matches {lenient} and is refused on import"
                    );
                    assert!(
                        !(rounds && is_plain(&text)) || fits,
                        "decimal({precision}, {scale}) {text:?} is taken on import and does not match {lenient}"
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 100_000, "{checked}");
}

#[test]
fn the_example_fits_its_own_shape_and_its_pattern() {
    for precision in 1..=38u8 {
        for scale in 0..=precision.min(12) {
            let e = example(precision, scale);
            let d = Decimal::parse_for(&e, precision, scale, None)
                .unwrap_or_else(|err| panic!("decimal({precision}, {scale}) example {e}: {err}"));
            assert_eq!(d.to_string(), e, "an example is written the way answers are");
            assert!(matches(&pattern(precision, scale), &e), "{e}");
            assert!(matches(&import_pattern(precision, scale), &e), "{e}");
        }
    }
    assert_eq!(example(5, 0), "12");
    assert_eq!(example(2, 2), "0.33");
    assert_eq!(example(3, 2), "1.33");
}
