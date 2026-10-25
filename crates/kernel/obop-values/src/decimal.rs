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
        // Padding a long integer part up to the scale can leave 128 bits; that is the same
        // mistake as any other integer part that is too long, and says so.
        let too_many = DecimalError::Precision { precision, scale };
        let d = self.rescale(scale, mode).map_err(|e| match e {
            DecimalError::Overflow if scale > self.scale => too_many.clone(),
            other => other,
        })?;
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

    pub fn negated(self) -> Self {
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
        self.checked_add(o.negated())
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
        // units = a * 10^(scale + o.scale - a.scale) / b, rounded once. The product may need
        // more than 128 bits even when the quotient is small (a divisor written with many
        // trailing zeros), so it is held in 256 bits.
        let e = i32::from(scale) + i32::from(o.scale) - i32::from(self.scale);
        let (up, down) = if e >= 0 { (e as u32, 0) } else { (0, e.unsigned_abs()) };
        Self::checked(
            scaled_div(self.units, up, o.units, down, mode)?,
            scale,
        )
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

/// A value that fits a decimal of `precision` and `scale`, written the way answers are written:
/// `12.33` for (10,2), `12` for (5,0), `1.3` for (2,1), `0.33` for (2,2). For documentation and
/// examples; it always parses for the same shape.
pub fn example(precision: u8, scale: u8) -> String {
    let int = match precision.saturating_sub(scale) {
        0 => "0",
        1 => "1",
        _ => "12",
    };
    if scale == 0 {
        int.to_string()
    } else {
        format!("{int}.{}", "3".repeat(usize::from(scale)))
    }
}

/// Like [`pattern`], for a field that rounds on import: any number of fractional digits.
pub fn import_pattern(precision: u8, scale: u8) -> String {
    let p = pattern(precision, scale);
    if scale == 0 {
        // A whole-number field has no fraction to round: the shape is the same.
        return p;
    }
    p.replace(&format!("{{1,{scale}}}"), "+")
}

/// The regular expression for the text of a decimal of `precision` and `scale`: digits that fit
/// the precision, then up to `scale` fractional digits (OpenAPI and MCP `pattern`). Responses
/// always carry exactly `scale` of them.
pub fn pattern(precision: u8, scale: u8) -> String {
    let int = match precision.saturating_sub(scale) {
        0 => "0".to_string(),
        1 => "[0-9]".to_string(),
        n => format!("(0|[1-9][0-9]{{0,{}}})", n - 1),
    };
    let frac = if scale == 0 {
        String::new()
    } else {
        format!("(\\.[0-9]{{1,{scale}}})?")
    };
    format!("^-?{int}{frac}$")
}

/// The product of two `u128` as `(high, low)` halves of a 256 bit number.
fn mul_wide(a: u128, b: u128) -> (u128, u128) {
    const LOW: u128 = u64::MAX as u128;
    let (a0, a1, b0, b1) = (a & LOW, a >> 64, b & LOW, b >> 64);
    let (p00, p01, p10, p11) = (a0 * b0, a0 * b1, a1 * b0, a1 * b1);
    let mid = (p00 >> 64) + (p01 & LOW) + (p10 & LOW);
    let lo = (p00 & LOW) | (mid << 64);
    let hi = p11 + (p01 >> 64) + (p10 >> 64) + (mid >> 64);
    (hi, lo)
}

/// A 256 bit number divided by `d` (at most 2^127): quotient as `(high, low)` and the remainder.
fn div_wide(hi: u128, lo: u128, d: u128) -> (u128, u128, u128) {
    debug_assert!(d != 0 && d <= 1 << 127);
    let (mut rem, mut q_hi, mut q_lo) = (0u128, 0u128, 0u128);
    for i in (0..256u32).rev() {
        let bit = if i >= 128 {
            (hi >> (i - 128)) & 1
        } else {
            (lo >> i) & 1
        };
        // `rem < d <= 2^127`, so doubling it cannot overflow.
        rem = (rem << 1) | bit;
        let take = rem >= d;
        if take {
            rem -= d;
        }
        if i >= 128 {
            q_hi |= u128::from(take) << (i - 128);
        } else {
            q_lo |= u128::from(take) << i;
        }
    }
    (q_hi, q_lo, rem)
}

/// `n * 10^up / (d * 10^down)` rounded by `mode`, exact whatever the size of the intermediate
/// product; an error only when the quotient does not fit 128 bits or `up` is beyond 38.
fn scaled_div(n: i128, up: u32, d: i128, down: u32, mode: Round) -> Result<i128, DecimalError> {
    if d == 0 {
        return Err(DecimalError::DivisionByZero);
    }
    if n == 0 {
        return Ok(0);
    }
    let neg = (n < 0) != (d < 0);
    let (nm, dm) = (n.unsigned_abs(), d.unsigned_abs());
    let overflow = || DecimalError::Overflow;
    let (q, rem, den) = if down > 0 {
        // The divisor grows. `nm` is below 2^127; when the divisor passes 128 bits the quotient
        // is below one half and rounds to zero in every mode.
        let Some(den) = 10u128
            .checked_pow(down)
            .and_then(|f| dm.checked_mul(f))
        else {
            return Ok(0);
        };
        (nm / den, nm % den, den)
    } else {
        let f = 10u128.checked_pow(up).ok_or_else(overflow)?;
        let (hi, lo) = mul_wide(nm, f);
        let (q_hi, q_lo, rem) = div_wide(hi, lo, dm);
        if q_hi != 0 {
            return Err(overflow());
        }
        (q_lo, rem, dm)
    };
    let away = match mode {
        Round::Down => false,
        _ if rem == 0 => false,
        Round::HalfUp => rem >= den - rem,
        Round::HalfEven => {
            let other = den - rem;
            rem > other || (rem == other && q % 2 == 1)
        }
    };
    let q = if away { q.checked_add(1).ok_or_else(overflow)? } else { q };
    let q = i128::try_from(q).map_err(|_| overflow())?;
    Ok(if neg { -q } else { q })
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
        Some(self.cmp(o))
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

    /// Grade school addition on digit columns: a big-decimal adder that shares no code with
    /// `Decimal` (columns of the written digits, carries at the end).
    fn column_sum(values: &[String], scale: usize) -> String {
        let mut cols = vec![0i64; 40 + scale];
        let mut neg_cols = vec![0i64; 40 + scale];
        for v in values {
            let (neg, body) = v
                .strip_prefix('-')
                .map_or((false, v.as_str()), |b| (true, b));
            let (int, frac) = body.split_once('.').unwrap_or((body, ""));
            let digits: Vec<u8> = format!("{int}{frac:0<w$}", w = scale).bytes().collect();
            let target = if neg { &mut neg_cols } else { &mut cols };
            for (i, d) in digits.iter().rev().enumerate() {
                target[i] += i64::from(d - b'0');
            }
        }
        let carry = |c: &mut Vec<i64>| {
            for i in 0..c.len() - 1 {
                c[i + 1] += c[i] / 10;
                c[i] %= 10;
            }
        };
        carry(&mut cols);
        carry(&mut neg_cols);
        let text = |c: &[i64]| -> String {
            let s: String = c
                .iter()
                .rev()
                .map(|d| char::from(b'0' + *d as u8))
                .collect();
            s.trim_start_matches('0').to_string()
        };
        let (pos, neg) = (text(&cols), text(&neg_cols));
        // Subtract the smaller from the larger by comparing the digit strings.
        let (big, small, sign) = if (pos.len(), &pos) >= (neg.len(), &neg) {
            (pos, neg, "")
        } else {
            (neg, pos, "-")
        };
        let (b, s): (Vec<i64>, Vec<i64>) = (
            big.bytes().rev().map(|d| i64::from(d - b'0')).collect(),
            small.bytes().rev().map(|d| i64::from(d - b'0')).collect(),
        );
        let mut out = Vec::new();
        let mut borrow = 0;
        for (i, top) in b.iter().enumerate() {
            let mut d = top - borrow - s.get(i).copied().unwrap_or(0);
            borrow = 0;
            if d < 0 {
                d += 10;
                borrow = 1;
            }
            out.push(d);
        }
        let digits: String = out
            .iter()
            .rev()
            .map(|d| char::from(b'0' + *d as u8))
            .collect();
        let digits = format!("{:0>w$}", digits.trim_start_matches('0'), w = scale + 1);
        let (int, frac) = digits.split_at(digits.len() - scale);
        let body = if scale == 0 {
            int.to_string()
        } else {
            format!("{int}.{frac}")
        };
        if body.bytes().all(|c| c == b'0' || c == b'.') {
            body
        } else {
            format!("{sign}{body}")
        }
    }

    #[test]
    fn a_million_scale_two_values_sum_and_average_like_an_independent_big_decimal() {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            seed ^= seed >> 12;
            seed ^= seed << 25;
            seed ^= seed >> 27;
            seed.wrapping_mul(0x2545_f491_4f6c_dd1d)
        };
        let values: Vec<String> = (0..1_000_000)
            .map(|_| {
                let units = (next() % 2_000_000_000) as i64 - 1_000_000_000; // up to 10 million with cents
                format!(
                    "{}{}.{:02}",
                    if units < 0 { "-" } else { "" },
                    units.abs() / 100,
                    units.abs() % 100
                )
            })
            .collect();
        let mut sum = Decimal::zero(2);
        for v in &values {
            sum = sum
                .checked_add(Decimal::parse_for(v, 12, 2, None).unwrap())
                .unwrap();
        }
        assert_eq!(sum.to_string(), column_sum(&values, 2));
        // The average at four digits, half even, from the oracle's exact sum by long division.
        let total: i128 = column_sum(&values, 2).replace('.', "").parse().unwrap();
        let n = values.len() as i128;
        let num = total * 100; // hundredths to ten-thousandths
        let (q, r) = (num.div_euclid(n), num.rem_euclid(n));
        let even = match (2 * r).cmp(&n) {
            Ordering::Less => q,
            Ordering::Greater => q + 1,
            Ordering::Equal => {
                if q % 2 == 0 {
                    q
                } else {
                    q + 1
                }
            }
        };
        let want = format!(
            "{}{}.{:04}",
            if even < 0 { "-" } else { "" },
            even.abs() / 10_000,
            even.abs() % 10_000
        );
        let avg = Decimal::average(sum.units(), 2, 1_000_000, 4, Round::HalfEven).unwrap();
        assert_eq!(avg.to_string(), want);
        // 0.1 + 0.2 is 0.3 where a double says 0.30000000000000004.
        assert_eq!(column_sum(&["0.1".into(), "0.2".into()], 1), "0.3");
    }

    #[test]
    fn division_does_not_overflow_because_of_the_size_of_its_intermediate() {
        // 2e14 at scale 2 over 3.000000000000: the scaled dividend is 2e38, above 128 bits,
        // though the quotient has twenty digits.
        let big = Decimal::parse("200000000000000.00").unwrap();
        let three = Decimal::parse("3.000000000000").unwrap();
        assert_eq!(
            big.checked_div(three, 12, Round::HalfEven).unwrap().to_string(),
            "66666666666666.666666666667"
        );
        assert_eq!(
            big.checked_div(Decimal::parse("3").unwrap(), 12, Round::Down).unwrap().to_string(),
            "66666666666666.666666666666"
        );
        // A divisor so large that the quotient rounds to zero in every mode.
        let huge = Decimal::parse("99999999999999999999999999999999999999").unwrap();
        assert_eq!(
            Decimal::parse("1").unwrap().checked_div(huge, 0, Round::HalfUp).unwrap().to_string(),
            "0"
        );
        // A real overflow is still an error.
        assert_eq!(
            huge.checked_div(Decimal::parse("0.001").unwrap(), 0, Round::Down),
            Err(DecimalError::Overflow)
        );
        // The average of values whose sum is large: 5e28 hundredths over 1000 values.
        assert_eq!(
            Decimal::average(5 * 10i128.pow(28), 2, 1000, 12, Round::HalfEven)
                .unwrap()
                .to_string(),
            "500000000000000000000000.000000000000"
        );
    }

    #[test]
    fn wide_division_agrees_with_the_plain_one_wherever_that_fits() {
        let mut seed = 0x5deece66du64;
        let mut next = || {
            seed ^= seed >> 12;
            seed ^= seed << 25;
            seed ^= seed >> 27;
            seed.wrapping_mul(0x2545_f491_4f6c_dd1d)
        };
        for _ in 0..30_000 {
            let n = (i128::from(next() >> 8) - (1 << 55)) * i128::from(next() % 1_000_000_007);
            let d = i128::from(next() % 1_000_000_007) + 1;
            let d = if next() % 2 == 0 { -d } else { d };
            let (up, down) = if next() % 2 == 0 { (next() as u32 % 12, 0) } else { (0, next() as u32 % 12) };
            for mode in [Round::Down, Round::HalfUp, Round::HalfEven] {
                let (mut num, mut den) = (n * 10i128.pow(up), d * 10i128.pow(down));
                if den < 0 {
                    num = -num;
                    den = -den;
                }
                assert_eq!(
                    scaled_div(n, up, d, down, mode),
                    Ok(div_round(num, den, mode)),
                    "{n} * 10^{up} / ({d} * 10^{down}) {mode:?}"
                );
            }
        }
    }

    #[test]
    fn too_many_integer_digits_get_the_precision_message_whatever_their_number() {
        for digits in [2usize, 20, 27, 28, 30, 38] {
            let text = "9".repeat(digits);
            let e = Decimal::parse_for(&text, 38, 12, None).err();
            if digits <= 26 {
                assert!(e.is_none(), "{digits}");
            } else {
                assert_eq!(
                    e,
                    Some(DecimalError::Precision { precision: 38, scale: 12 }),
                    "{digits} digits"
                );
            }
        }
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
