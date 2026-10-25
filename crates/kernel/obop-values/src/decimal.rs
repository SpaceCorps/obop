//! Exact decimal numbers for the `decimal` field kind (D72).
//!
//! A [`Decimal`] is a whole number of units (an `i128`) and a scale: `units / 10^scale`. Nothing
//! here ever touches a float. Arithmetic is checked: an overflow is an error, never a wrap or a
//! silent loss of digits. The JSON form of a decimal is a canonical string with exactly `scale`
//! fractional digits (`"12.3400"`, `"7"` at scale 0), and [`Decimal::parse`] is strict: digits, an
//! optional minus sign and an optional point between digits, nothing else.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

/// Most digits (before and after the point together) a `decimal` field may declare.
pub const MAX_PRECISION: u8 = 38;
/// Most digits after the point a `decimal` field may declare.
pub const MAX_SCALE: u8 = 12;
/// Longest text [`Decimal::parse`] looks at.
pub const MAX_INPUT_CHARS: usize = 80;

/// How a value with more fractional digits than the scale is brought to the scale. Only the
/// import path rounds (`bulk_create`, `obop import`), and only when the field declares a mode;
/// everywhere else too many digits are an error. Computed fields and aggregates round with the
/// mode they declare or `half_even`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Round {
    /// Ties go to the even neighbour (banker's rounding): 0.125 -> 0.12, 0.135 -> 0.14.
    #[default]
    HalfEven,
    /// Ties go away from zero: 0.125 -> 0.13, -0.125 -> -0.13.
    HalfUp,
    /// Toward zero: the extra digits are dropped.
    Down,
}

impl Round {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "half_even" => Some(Self::HalfEven),
            "half_up" => Some(Self::HalfUp),
            "down" => Some(Self::Down),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::HalfEven => "half_even",
            Self::HalfUp => "half_up",
            Self::Down => "down",
        }
    }
}

/// Why a text is not a decimal of a given shape. `Display` is the reason, in English, that is
/// appended to "`model.field` must be ...".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecimalError {
    Empty,
    TooLong,
    /// Not the grammar `-?digits(.digits)?`; the text says what was wrong.
    Syntax(String),
    /// More significant digits than the largest precision, 38.
    TooManyDigits,
    /// More fractional digits than the scale and no rounding mode.
    FractionDigits {
        have: usize,
        scale: u8,
    },
    /// The integer part does not fit the declared precision.
    Precision {
        precision: u8,
        scale: u8,
    },
    /// An arithmetic result does not fit (128 bits, or 38 digits).
    Overflow,
    DivisionByZero,
}

impl fmt::Display for DecimalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "it is empty"),
            Self::TooLong => write!(f, "it is longer than {MAX_INPUT_CHARS} characters"),
            Self::Syntax(why) => write!(f, "{why}"),
            Self::TooManyDigits => write!(f, "it has more than {MAX_PRECISION} digits"),
            Self::FractionDigits { have, scale } => write!(
                f,
                "it has {have} fractional digits but the field keeps {scale}; send at most {scale}"
            ),
            Self::Precision { precision, scale } => write!(
                f,
                "it has too many digits: at most {precision} in all, {scale} after the point, so {} before it",
                u32::from(*precision).saturating_sub(u32::from(*scale))
            ),
            Self::Overflow => write!(f, "the result is too large"),
            Self::DivisionByZero => write!(f, "division by zero"),
        }
    }
}

impl std::error::Error for DecimalError {}

/// `10^n` for `n` up to 38.
fn pow10(n: u32) -> Option<i128> {
    10i128.checked_pow(n)
}

/// `n / d` rounded as `mode` says, `d > 0`. The remainder is compared with `d - |r|` instead of
/// being doubled, so nothing overflows near the top of the 128 bit range.
pub fn div_round(n: i128, d: i128, mode: Round) -> i128 {
    debug_assert!(d > 0);
    let q = n / d;
    let r = n % d;
    if r == 0 {
        return q;
    }
    let away = if n < 0 { q - 1 } else { q + 1 };
    match mode {
        Round::Down => q,
        Round::HalfUp => {
            if r.abs() >= d - r.abs() {
                away
            } else {
                q
            }
        }
        Round::HalfEven => match r.abs().cmp(&(d - r.abs())) {
            Ordering::Less => q,
            Ordering::Greater => away,
            Ordering::Equal => {
                if q % 2 == 0 {
                    q
                } else {
                    away
                }
            }
        },
    }
}

/// A decimal number: `units / 10^scale`. Equality, ordering and hashing are by numeric value
/// (`1.0 == 1.00`); the scale only decides how it is written.
#[derive(Debug, Clone, Copy)]
pub struct Decimal {
    units: i128,
    scale: u8,
}

impl Decimal {
    /// The value `units / 10^scale`. `None` when the scale is above 38 or the number has more
    /// than 38 digits.
    pub fn new(units: i128, scale: u8) -> Option<Self> {
        if scale > MAX_PRECISION || digits(units) > u32::from(MAX_PRECISION) {
            return None;
        }
        Some(Self { units, scale })
    }

    pub fn from_i64(n: i64) -> Self {
        Self {
            units: i128::from(n),
            scale: 0,
        }
    }

    pub fn zero(scale: u8) -> Self {
        Self { units: 0, scale }
    }

    pub fn units(self) -> i128 {
        self.units
    }

    pub fn scale(self) -> u8 {
        self.scale
    }

    pub fn is_zero(self) -> bool {
        self.units == 0
    }

    /// Significant digits of the whole number of units (`0` has none).
    pub fn digits(self) -> u32 {
        digits(self.units)
    }

    /// Parse the strict grammar `-?[0-9]+(\.[0-9]+)?`. The scale of the result is the number of
    /// fractional digits written. No sign but `-`, no spaces, exponent, separators, `NaN`,
    /// hexadecimal, `1.` or `.5`; at most [`MAX_PRECISION`] significant digits; `-0` is `0`.
    pub fn parse(s: &str) -> Result<Self, DecimalError> {
        if s.is_empty() {
            return Err(DecimalError::Empty);
        }
        if s.len() > MAX_INPUT_CHARS {
            return Err(DecimalError::TooLong);
        }
        let b = s.as_bytes();
        let (neg, body) = match b[0] {
            b'-' => (true, &b[1..]),
            _ => (false, b),
        };
        let mut units: i128 = 0;
        let mut seen_point = false;
        let mut int_digits = 0usize;
        let mut frac_digits = 0usize;
        let mut sig = 0u32;
        for (i, &c) in body.iter().enumerate() {
            match c {
                b'0'..=b'9' => {
                    if seen_point {
                        frac_digits += 1;
                    } else {
                        int_digits += 1;
                    }
                    if sig > 0 || c != b'0' {
                        sig += 1;
                    }
                    if sig > u32::from(MAX_PRECISION) {
                        return Err(DecimalError::TooManyDigits);
                    }
                    units = units * 10 + i128::from(c - b'0');
                }
                b'.' if !seen_point => {
                    if int_digits == 0 {
                        return Err(DecimalError::Syntax(
                            "a digit is needed before the point (write 0.5, not .5)".into(),
                        ));
                    }
                    if i + 1 == body.len() {
                        return Err(DecimalError::Syntax(
                            "a digit is needed after the point (write 5, not 5.)".into(),
                        ));
                    }
                    seen_point = true;
                }
                _ => return Err(DecimalError::Syntax(unexpected(s, i + usize::from(neg)))),
            }
        }
        if int_digits == 0 {
            return Err(DecimalError::Syntax(if neg {
                "a minus sign needs digits after it".into()
            } else {
                unexpected(s, 0)
            }));
        }
        if frac_digits > usize::from(MAX_PRECISION) {
            return Err(DecimalError::TooManyDigits);
        }
        Ok(Self {
            units: if neg { -units } else { units },
            scale: frac_digits as u8,
        })
    }

    /// Parse for a field of `precision` and `scale`. A value with more fractional digits than
    /// the scale is an error unless `round` names a mode (the import path); fewer digits are
    /// padded with zeros. The result has exactly the field's scale and fits its precision.
    pub fn parse_for(
        s: &str,
        precision: u8,
        scale: u8,
        round: Option<Round>,
    ) -> Result<Self, DecimalError> {
        let d = Self::parse(s)?;
        if d.scale > scale && round.is_none() {
            return Err(DecimalError::FractionDigits {
                have: usize::from(d.scale),
                scale,
            });
        }
        d.fit(precision, scale, round.unwrap_or_default())
    }

    /// This value at exactly `scale` (rounded by `mode` when digits are dropped) and within
    /// `precision` digits.
    pub fn fit(self, precision: u8, scale: u8, mode: Round) -> Result<Self, DecimalError> {
        let d = self.rescale(scale, mode)?;
        if d.digits() > u32::from(precision) {
            return Err(DecimalError::Precision { precision, scale });
        }
        Ok(d)
    }

    /// The same number at another scale; digits that are dropped are rounded by `mode`.
    pub fn rescale(self, scale: u8, mode: Round) -> Result<Self, DecimalError> {
        match scale.cmp(&self.scale) {
            Ordering::Equal => Ok(self),
            Ordering::Greater => {
                let k = u32::from(scale - self.scale);
                let f = pow10(k).ok_or(DecimalError::Overflow)?;
                let units = self.units.checked_mul(f).ok_or(DecimalError::Overflow)?;
                Ok(Self { units, scale })
            }
            Ordering::Less => {
                let k = u32::from(self.scale - scale);
                // `scale` is below `self.scale <= 38`, so 10^k fits.
                let f = pow10(k).ok_or(DecimalError::Overflow)?;
                Ok(Self {
                    units: div_round(self.units, f, mode),
                    scale,
                })
            }
        }
    }

    pub fn neg(self) -> Self {
        Self {
            units: -self.units,
            scale: self.scale,
        }
    }

    /// The two values as units at the larger scale.
    fn aligned(self, o: Self) -> Result<(i128, i128, u8), DecimalError> {
        let scale = self.scale.max(o.scale);
        let up = |d: Self| -> Result<i128, DecimalError> {
            let f = pow10(u32::from(scale - d.scale)).ok_or(DecimalError::Overflow)?;
            d.units.checked_mul(f).ok_or(DecimalError::Overflow)
        };
        Ok((up(self)?, up(o)?, scale))
    }

    /// Exact sum; the scale is the larger of the two.
    pub fn checked_add(self, o: Self) -> Result<Self, DecimalError> {
        let (a, b, scale) = self.aligned(o)?;
        let units = a.checked_add(b).ok_or(DecimalError::Overflow)?;
        Self::checked(units, scale)
    }

    /// Exact difference; the scale is the larger of the two.
    pub fn checked_sub(self, o: Self) -> Result<Self, DecimalError> {
        self.checked_add(o.neg())
    }

    /// Exact product; the scale is the sum of the two scales.
    pub fn checked_mul(self, o: Self) -> Result<Self, DecimalError> {
        let units = self
            .units
            .checked_mul(o.units)
            .ok_or(DecimalError::Overflow)?;
        let scale = self.scale + o.scale;
        if scale > MAX_PRECISION {
            return Err(DecimalError::Overflow);
        }
        Self::checked(units, scale)
    }

    /// Quotient at `scale`, rounded once by `mode`.
    pub fn checked_div(self, o: Self, scale: u8, mode: Round) -> Result<Self, DecimalError> {
        if o.units == 0 {
            return Err(DecimalError::DivisionByZero);
        }
        // units = a * 10^(scale + o.scale - a.scale) / b
        let e = i32::from(scale) + i32::from(o.scale) - i32::from(self.scale);
        let (mut num, mut den) = (self.units, o.units);
        if e >= 0 {
            let f = pow10(e as u32).ok_or(DecimalError::Overflow)?;
            num = num.checked_mul(f).ok_or(DecimalError::Overflow)?;
        } else {
            let f = pow10((-e) as u32).ok_or(DecimalError::Overflow)?;
            den = den.checked_mul(f).ok_or(DecimalError::Overflow)?;
        }
        if den < 0 {
            num = -num;
            den = -den;
        }
        Self::checked(div_round(num, den, mode), scale)
    }

    fn checked(units: i128, scale: u8) -> Result<Self, DecimalError> {
        if scale > MAX_PRECISION || digits(units) > u32::from(MAX_PRECISION) {
            return Err(DecimalError::Overflow);
        }
        Ok(Self { units, scale })
    }

    /// Numeric comparison, exact for every pair of values.
    pub fn compare(self, o: Self) -> Ordering {
        match self.aligned(o) {
            Ok((a, b, _)) => a.cmp(&b),
            // Scaling one side overflowed, so that side is far larger in magnitude than the
            // other (whose units are below 10^38).
            Err(_) => {
                let (big, flip) = if self.scale < o.scale {
                    (self, false)
                } else {
                    (o, true)
                };
                let ord = big.units.cmp(&0);
                if flip { ord.reverse() } else { ord }
            }
        }
    }

    /// The number as text with exactly `scale` fractional digits: `12.3400`, `-0.05`, `7`.
    fn write(&self, out: &mut String) {
        let neg = self.units < 0;
        let abs = self.units.unsigned_abs();
        let scale = usize::from(self.scale);
        let digits = abs.to_string();
        if neg {
            out.push('-');
        }
        if scale == 0 {
            out.push_str(&digits);
        } else if digits.len() <= scale {
            out.push_str("0.");
            for _ in digits.len()..scale {
                out.push('0');
            }
            out.push_str(&digits);
        } else {
            let (int, frac) = digits.split_at(digits.len() - scale);
            out.push_str(int);
            out.push('.');
            out.push_str(frac);
        }
    }

    /// Sum of `values` (all at one scale) divided by their number, at `scale`, rounded by `mode`.
    pub fn average(
        sum: i128,
        sum_scale: u8,
        count: u64,
        scale: u8,
        mode: Round,
    ) -> Result<Self, DecimalError> {
        if count == 0 {
            return Err(DecimalError::DivisionByZero);
        }
        Self::checked(sum, sum_scale)?.checked_div(
            Self::checked(i128::from(count), 0)?,
            scale,
            mode,
        )
    }
}

fn unexpected(s: &str, at: usize) -> String {
    let rest = s.get(at..).unwrap_or("");
    let lower = s.to_ascii_lowercase();
    if lower.contains("nan") || lower.contains("inf") {
        return "NaN and infinity are not numbers".into();
    }
    if lower.starts_with("0x") || lower.starts_with("-0x") {
        return "hexadecimal is not allowed".into();
    }
    match rest.chars().next() {
        Some(c) if c.is_whitespace() => "spaces around the number are not allowed".into(),
        Some('+') => "a leading + is not allowed (write 5, not +5)".into(),
        Some('e' | 'E') => "exponent notation is not allowed (write 1000, not 1e3)".into(),
        Some(',') => {
            "a comma is not allowed: use a point for the decimal point and no thousands separators"
                .into()
        }
        Some('_' | '\'') => "thousands separators are not allowed".into(),
        Some('.') => "a number has only one decimal point".into(),
        Some('-') => "a minus sign is only allowed at the start".into(),
        Some(c) => format!(
            "unexpected character {c:?}; a decimal is digits with an optional minus sign and point, such as -12.50"
        ),
        None => "it is not a decimal number".into(),
    }
}

/// Significant digits of `n` (0 has none).
fn digits(n: i128) -> u32 {
    let mut a = n.unsigned_abs();
    let mut d = 0;
    while a > 0 {
        a /= 10;
        d += 1;
    }
    d
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = String::new();
        self.write(&mut s);
        f.write_str(&s)
    }
}

impl PartialEq for Decimal {
    fn eq(&self, o: &Self) -> bool {
        self.compare(*o) == Ordering::Equal
    }
}
impl Eq for Decimal {}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.compare(*o))
    }
}
impl Ord for Decimal {
    fn cmp(&self, o: &Self) -> Ordering {
        self.compare(*o)
    }
}

impl Hash for Decimal {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Equal values hash alike whatever their scale: drop trailing zeros first.
        let (mut u, mut s) = (self.units, self.scale);
        while s > 0 && u % 10 == 0 {
            u /= 10;
            s -= 1;
        }
        if u == 0 {
            s = 0;
        }
        u.hash(state);
        s.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Decimal {
        Decimal::parse(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn parses_the_strict_grammar_and_writes_the_canonical_form() {
        for (src, out, scale) in [
            ("0", "0", 0),
            ("12", "12", 0),
            ("12.3400", "12.3400", 4),
            ("-12.34", "-12.34", 2),
            ("-0", "0", 0),
            ("-0.00", "0.00", 2),
            ("007.50", "7.50", 2),
            ("0.05", "0.05", 2),
            ("-0.05", "-0.05", 2),
            (
                "99999999999999999999999999999999999999",
                "99999999999999999999999999999999999999",
                0,
            ),
            ("0.000000000001", "0.000000000001", 12),
        ] {
            let d = p(src);
            assert_eq!((d.to_string().as_str(), d.scale()), (out, scale), "{src}");
        }
    }

    #[test]
    fn hostile_text_is_refused_with_a_reason() {
        for (src, want) in [
            ("", "empty"),
            (" 1.5", "spaces"),
            ("1.5 ", "spaces"),
            ("1e3", "exponent"),
            ("1E3", "exponent"),
            ("1,5", "comma"),
            ("1,000.50", "comma"),
            ("NaN", "NaN"),
            ("-Infinity", "NaN"),
            ("0x10", "hexadecimal"),
            ("+1", "leading +"),
            (".5", "before the point"),
            ("5.", "after the point"),
            ("-", "minus"),
            ("--1", "minus"),
            ("1-1", "minus"),
            ("1.2.3", "one decimal point"),
            ("1_000", "separators"),
            ("1\u{0660}", "unexpected"),
            ("1.5\0", "unexpected"),
            ("١٢", "unexpected"),
            ("1.5f", "unexpected"),
            ("123456789012345678901234567890123456789", "more than 38"),
            ("0.1234567890123456789012345678901234567891", "more than 38"),
        ] {
            let e = Decimal::parse(src).unwrap_err().to_string();
            assert!(e.contains(want), "{src:?} -> {e}");
        }
        assert_eq!(Decimal::parse(&"1".repeat(81)), Err(DecimalError::TooLong));
    }

    #[test]
    fn a_field_takes_at_most_its_scale_and_precision() {
        let f = |s: &str, round| Decimal::parse_for(s, 6, 2, round).map(|d| d.to_string());
        assert_eq!(f("12", None).unwrap(), "12.00");
        assert_eq!(f("12.3", None).unwrap(), "12.30");
        assert_eq!(f("9999.99", None).unwrap(), "9999.99");
        assert!(
            f("10000.00", None)
                .unwrap_err()
                .to_string()
                .contains("too many digits")
        );
        assert!(
            f("1.234", None)
                .unwrap_err()
                .to_string()
                .contains("3 fractional digits")
        );
        assert!(
            f("1.230", None).is_err(),
            "even when the extra digit is a zero"
        );
        assert_eq!(f("1.234", Some(Round::Down)).unwrap(), "1.23");
        assert_eq!(f("1.235", Some(Round::HalfUp)).unwrap(), "1.24");
        assert_eq!(f("1.235", Some(Round::HalfEven)).unwrap(), "1.24");
        assert_eq!(f("1.225", Some(Round::HalfEven)).unwrap(), "1.22");
        assert_eq!(f("1.225", Some(Round::HalfUp)).unwrap(), "1.23");
        assert_eq!(f("-1.225", Some(Round::HalfUp)).unwrap(), "-1.23");
        assert_eq!(f("-1.239", Some(Round::Down)).unwrap(), "-1.23");
        assert_eq!(f("-1.5", Some(Round::HalfEven)).unwrap(), "-1.50");
        // Rounding up may carry into a digit the precision does not have.
        assert!(f("9999.999", Some(Round::HalfUp)).is_err());
        assert_eq!(f("-0.001", Some(Round::HalfEven)).unwrap(), "0.00");
    }

    #[test]
    fn rounding_modes_agree_with_a_naive_reference() {
        // Reference on small integers, written with floor division and the exact half test.
        for d in [2i128, 4, 5, 10, 100, 1000] {
            for n in -2500i128..2500 {
                let (floor, r) = (n.div_euclid(d), n.rem_euclid(d));
                let down = if n < 0 && r != 0 { floor + 1 } else { floor };
                let tie_up = if n >= 0 { floor + 1 } else { floor };
                let tie_even = if floor.rem_euclid(2) == 0 {
                    floor
                } else {
                    floor + 1
                };
                let nearest = |tie: i128| match (2 * r).cmp(&d) {
                    Ordering::Less => floor,
                    Ordering::Greater => floor + 1,
                    Ordering::Equal => tie,
                };
                assert_eq!(div_round(n, d, Round::Down), down, "down {n}/{d}");
                assert_eq!(
                    div_round(n, d, Round::HalfUp),
                    nearest(tie_up),
                    "half_up {n}/{d}"
                );
                assert_eq!(
                    div_round(n, d, Round::HalfEven),
                    nearest(tie_even),
                    "half_even {n}/{d}"
                );
            }
        }
    }

    #[test]
    fn arithmetic_is_exact_and_checked() {
        assert_eq!(p("0.1").checked_add(p("0.2")).unwrap().to_string(), "0.3");
        assert_eq!(p("0.1").checked_add(p("0.20")).unwrap().to_string(), "0.30");
        assert_eq!(p("1").checked_sub(p("0.001")).unwrap().to_string(), "0.999");
        assert_eq!(
            p("1.5").checked_mul(p("-2.25")).unwrap().to_string(),
            "-3.375"
        );
        assert_eq!(
            p("10")
                .checked_div(p("3"), 4, Round::HalfEven)
                .unwrap()
                .to_string(),
            "3.3333"
        );
        assert_eq!(
            p("2")
                .checked_div(p("3"), 2, Round::HalfUp)
                .unwrap()
                .to_string(),
            "0.67"
        );
        assert_eq!(
            p("-2")
                .checked_div(p("3"), 2, Round::Down)
                .unwrap()
                .to_string(),
            "-0.66"
        );
        assert_eq!(
            p("1").checked_div(p("0"), 2, Round::Down),
            Err(DecimalError::DivisionByZero)
        );
        let big = p("99999999999999999999999999999999999999");
        assert_eq!(big.checked_add(big), Err(DecimalError::Overflow));
        assert_eq!(big.checked_mul(p("10")), Err(DecimalError::Overflow));
        assert_eq!(
            Decimal::average(1000, 2, 3, 2, Round::HalfEven)
                .unwrap()
                .to_string(),
            "3.33"
        );
    }

    #[test]
    fn comparison_is_numeric_not_lexical() {
        assert!(p("9.5") < p("10.25"));
        assert!(p("-10") < p("-9.99"));
        assert_eq!(p("1.0"), p("1.00"));
        assert!(p("0.001") > p("0"));
        assert!(
            p("99999999999999999999999999999999999999")
                > p("0.00000000000000000000000000000000000001")
        );
        assert!(
            p("-99999999999999999999999999999999999999")
                < p("0.00000000000000000000000000000000000001")
        );
        let mut v = [p("10.25"), p("9.5"), p("-3"), p("0")];
        v.sort();
        assert_eq!(
            v.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["-3", "0", "9.5", "10.25"]
        );
        use std::collections::HashSet;
        let s: HashSet<Decimal> = [p("1.0"), p("1.00"), p("2")].into_iter().collect();
        assert_eq!(s.len(), 2);
    }
}
