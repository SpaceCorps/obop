//! Order arithmetic (D61): discounts, taxes and totals, in whole minor currency units.
//!
//! Everything here is a pure function of numbers that came from the server's own records (the
//! published listing prices, the active discount code and tax rates, the chosen delivery method).
//! Nothing a browser sends is a number in this module except the quantity, and that is checked
//! before it gets here.
//!
//! **Rounding.** Every rounding is "half away from zero" on one exact integer division, done on
//! `i128` so no product overflows: `round_div(n, d) = (2n + d) / (2d)` for non-negative `n`. A
//! percentage discount is rounded once for the whole order, then split over the lines it applies
//! to by the largest remainder method (so the lines add up to the discount to the cent). Tax is
//! computed **per line**, on the line's amount after its share of the discount, and rounded per
//! line; the tax lines of the order are the sums of the per-line amounts. Delivery is its own line
//! and is taxed with the rate that applies to delivery in the buyer's country.
//!
//! **Included and added tax.** A rate with `included` means the listing prices already contain it
//! (the usual way in the EU): the tax part of a gross amount `g` at rate `r` basis points is
//! `round_div(g * r, 10000 + r)` and the buyer pays `g`. A rate that is not included is added on
//! top: tax `round_div(g * r, 10000)` and the buyer pays `g + tax`.
//!
//! **Out of scope** (stated, not hidden): VAT identification numbers, reverse charge, the EU
//! One-Stop-Shop and destination-country VAT thresholds, tax on a different rate per product
//! type beyond a category, and any country's legal invoice rules. The shop computes the rates the
//! owner entered; whether they are the right ones is the owner's responsibility.

/// `n / d` rounded half away from zero, for `n >= 0` and `d > 0`.
pub fn round_div(n: i128, d: i128) -> i64 {
    debug_assert!(n >= 0 && d > 0);
    ((2 * n + d) / (2 * d)) as i64
}

/// Split `total` over `weights` in proportion, in whole units, so the parts add up to `total`
/// exactly (largest remainder; ties go to the earlier part). All weights zero gives all zeros.
pub fn allocate(total: i64, weights: &[i64]) -> Vec<i64> {
    let sum: i128 = weights.iter().map(|w| i128::from(*w)).sum();
    if sum <= 0 || total <= 0 {
        return vec![0; weights.len()];
    }
    let mut parts: Vec<i64> = weights
        .iter()
        .map(|w| (i128::from(total) * i128::from(*w) / sum) as i64)
        .collect();
    let mut rest = total - parts.iter().sum::<i64>();
    let mut order: Vec<usize> = (0..weights.len()).collect();
    // Largest fractional remainder first; stable, so ties keep their order.
    order.sort_by_key(|i| std::cmp::Reverse((i128::from(total) * i128::from(weights[*i])) % sum));
    for i in order {
        if rest == 0 {
            break;
        }
        if weights[i] > 0 {
            parts[i] += 1;
            rest -= 1;
        }
    }
    parts
}

/// A tax rate as the owner entered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rate {
    pub name: String,
    /// Basis points: 1900 is 19.00 percent.
    pub bp: i64,
    /// Two-letter country, upper case; empty for "every country without a rate of its own".
    pub country: String,
    /// Only listings of this category; `None` for every listing and for delivery.
    pub category: Option<String>,
    pub included: bool,
}

/// The one rate that applies to a line of `category` delivered to `country`: country-and-category
/// specific beats category-only beats country-only beats the general one. Among equals the first
/// (the earlier entered) wins, so the choice never depends on anything but the rates themselves.
pub fn pick_rate<'a>(rates: &'a [Rate], country: &str, category: Option<&str>) -> Option<&'a Rate> {
    let score = |r: &Rate| i32::from(r.category.is_some()) * 2 + i32::from(!r.country.is_empty());
    let mut best: Option<&Rate> = None;
    for r in rates
        .iter()
        .filter(|r| r.country.is_empty() || r.country.eq_ignore_ascii_case(country))
        .filter(|r| r.category.is_none() || r.category.as_deref() == category)
    {
        // Strictly greater, so among equals the earlier entry stays.
        if best.is_none_or(|b| score(r) > score(b)) {
            best = Some(r);
        }
    }
    best
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscountKind {
    /// Basis points of the amount it applies to.
    Percent,
    /// Minor currency units off, never more than the amount it applies to.
    Fixed,
}

/// A discount that passed every check (active, in date, used fewer times than allowed, minimum
/// order reached); only the arithmetic is left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discount {
    pub kind: DiscountKind,
    pub value: i64,
    /// Only lines of this category; `None` for the whole order.
    pub category: Option<String>,
}

/// One product line as priced from the published listing.
#[derive(Debug, Clone)]
pub struct LineIn {
    /// Unit price times quantity, at the listing price.
    pub gross: i64,
    pub category: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineOut {
    pub gross: i64,
    pub discount: i64,
    pub tax_name: String,
    pub tax_bp: i64,
    pub tax: i64,
    /// What the buyer pays for the line: gross minus discount, plus tax that is added on top.
    pub payable: i64,
}

/// One row of the tax summary: the same rate summed over lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxLine {
    pub name: String,
    pub bp: i64,
    pub included: bool,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Priced {
    pub lines: Vec<LineOut>,
    /// Sum of the lines at listing prices.
    pub subtotal: i64,
    pub discount_total: i64,
    pub shipping: i64,
    pub shipping_tax: i64,
    /// All tax: the part inside the prices and the part added on top.
    pub tax_total: i64,
    pub tax_lines: Vec<TaxLine>,
    /// Lines minus the discount, plus delivery, plus tax added on top.
    pub total: i64,
}

fn tax_of(gross: i64, rate: Option<&Rate>) -> (i64, i64) {
    match rate {
        None => (0, 0),
        Some(r) if r.included => (
            round_div(
                i128::from(gross) * i128::from(r.bp),
                10_000 + i128::from(r.bp),
            ),
            0,
        ),
        Some(r) => {
            let t = round_div(i128::from(gross) * i128::from(r.bp), 10_000);
            (t, t)
        }
    }
}

/// The price of an order. `Err` names the only way it can fail: a sum that does not fit.
pub fn price(
    lines: &[LineIn],
    discount: Option<&Discount>,
    shipping: i64,
    rates: &[Rate],
    country: &str,
) -> Result<Priced, &'static str> {
    let overflow = "the order is too large";
    let subtotal = lines
        .iter()
        .try_fold(0i64, |a, l| a.checked_add(l.gross))
        .ok_or(overflow)?;
    // The discount applies to the lines it may apply to.
    let eligible: Vec<i64> = lines
        .iter()
        .map(|l| match discount {
            Some(d) if d.category.is_none() || d.category == l.category => l.gross,
            _ => 0,
        })
        .collect();
    let base: i64 = eligible.iter().sum();
    let total_discount = match discount {
        Some(d) if base > 0 => match d.kind {
            DiscountKind::Percent => round_div(
                i128::from(base) * i128::from(d.value.clamp(0, 10_000)),
                10_000,
            ),
            DiscountKind::Fixed => d.value.max(0),
        }
        .min(base),
        _ => 0,
    };
    let shares = allocate(total_discount, &eligible);

    let mut out = Vec::with_capacity(lines.len());
    let mut tax_total = 0i64;
    let mut added = 0i64;
    let mut summary: Vec<TaxLine> = Vec::new();
    let mut note = |r: Option<&Rate>, amount: i64| {
        if let Some(r) = r {
            match summary
                .iter_mut()
                .find(|t| t.name == r.name && t.bp == r.bp && t.included == r.included)
            {
                Some(t) => t.amount += amount,
                None => summary.push(TaxLine {
                    name: r.name.clone(),
                    bp: r.bp,
                    included: r.included,
                    amount,
                }),
            }
        }
    };
    for (l, share) in lines.iter().zip(&shares) {
        let net = l.gross - share;
        let rate = pick_rate(rates, country, l.category.as_deref());
        let (tax, on_top) = tax_of(net, rate);
        tax_total = tax_total.checked_add(tax).ok_or(overflow)?;
        added = added.checked_add(on_top).ok_or(overflow)?;
        let payable = net.checked_add(on_top).ok_or(overflow)?;
        note(rate, tax);
        out.push(LineOut {
            gross: l.gross,
            discount: *share,
            tax_name: rate.map(|r| r.name.clone()).unwrap_or_default(),
            tax_bp: rate.map_or(0, |r| r.bp),
            tax,
            payable,
        });
    }
    let ship_rate = pick_rate(rates, country, None);
    let (ship_tax, ship_on_top) = tax_of(shipping, ship_rate);
    tax_total = tax_total.checked_add(ship_tax).ok_or(overflow)?;
    added = added.checked_add(ship_on_top).ok_or(overflow)?;
    note(ship_rate, ship_tax);
    let total = subtotal
        .checked_sub(total_discount)
        .and_then(|t| t.checked_add(shipping))
        .and_then(|t| t.checked_add(added))
        .ok_or(overflow)?;
    Ok(Priced {
        lines: out,
        subtotal,
        discount_total: total_discount,
        shipping,
        shipping_tax: ship_tax,
        tax_total,
        tax_lines: summary,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(name: &str, bp: i64, country: &str, category: Option<&str>, included: bool) -> Rate {
        Rate {
            name: name.into(),
            bp,
            country: country.into(),
            category: category.map(String::from),
            included,
        }
    }

    fn line(gross: i64, cat: Option<&str>) -> LineIn {
        LineIn {
            gross,
            category: cat.map(String::from),
        }
    }

    #[test]
    fn rounding_is_half_away_from_zero_on_exact_division() {
        assert_eq!(round_div(5, 2), 3); // 2.5 -> 3
        assert_eq!(round_div(4, 2), 2);
        assert_eq!(round_div(1, 3), 0); // 0.33
        assert_eq!(round_div(2, 3), 1); // 0.67
        assert_eq!(round_div(1, 2), 1); // 0.5 -> 1
        assert_eq!(round_div(0, 7), 0);
        // No overflow with the largest amounts the shop accepts.
        assert_eq!(
            round_div(i128::from(100_000_000_000i64) * 10_000, 20_000),
            50_000_000_000
        );
    }

    #[test]
    fn allocation_adds_up_exactly_and_follows_the_weights() {
        for (total, w) in [
            (100, vec![1, 1, 1]),
            (1, vec![5, 5]),
            (7, vec![0, 3, 0, 4]),
            (999, vec![123, 456, 789]),
            (0, vec![1, 2]),
            (5, vec![0, 0]),
        ] {
            let parts = allocate(total, &w);
            let want = if w.iter().sum::<i64>() == 0 { 0 } else { total };
            assert_eq!(parts.iter().sum::<i64>(), want, "{total} {w:?}");
            assert!(parts.iter().zip(&w).all(|(p, w)| *w > 0 || *p == 0));
        }
        assert_eq!(allocate(100, &[1, 1, 1]), vec![34, 33, 33]);
        assert_eq!(allocate(7, &[0, 3, 0, 4]), vec![0, 3, 0, 4]);
    }

    #[test]
    fn included_tax_is_the_tax_part_of_the_gross_amount() {
        // 11.90 incl. 19% -> 1.90 tax
        let r = [rate("VAT", 1900, "", None, true)];
        let p = price(&[line(1190, None)], None, 0, &r, "DE").unwrap();
        assert_eq!((p.total, p.tax_total, p.lines[0].tax), (1190, 190, 190));
        assert_eq!(p.tax_lines.len(), 1);
        // 12.50 incl. 19%: 1250*1900/11900 = 199.58 -> 200 (half away)
        let p = price(&[line(1250, None)], None, 0, &r, "DE").unwrap();
        assert_eq!(p.lines[0].tax, 200);
        assert_eq!(
            p.total, 1250,
            "an included tax does not change what is paid"
        );
    }

    #[test]
    fn added_tax_goes_on_top_and_per_line_rounding_is_documented_behaviour() {
        let r = [rate("Sales tax", 825, "", None, false)];
        // Three lines of 0.99: each 0.99*8.25% = 8.17 -> 8 cents; per line 3 x 8 = 24, not the
        // 24.5 -> 25 that rounding the sum would give.
        let ls = [line(99, None), line(99, None), line(99, None)];
        let p = price(&ls, None, 0, &r, "US").unwrap();
        assert_eq!(p.tax_total, 24);
        assert_eq!(p.total, 3 * 99 + 24);
        assert!(p.lines.iter().all(|l| l.tax == 8 && l.payable == 107));
    }

    #[test]
    fn the_most_specific_rate_wins_and_ties_go_to_the_first() {
        let rs = [
            rate("General", 2000, "", None, true),
            rate("DE", 1900, "DE", None, true),
            rate("Books", 700, "", Some("books"), true),
            rate("DE books", 700, "DE", Some("books"), true),
            rate("DE again", 1500, "DE", None, true),
        ];
        let name = |c: &str, cat: Option<&str>| pick_rate(&rs, c, cat).map(|r| r.name.clone());
        assert_eq!(name("FR", None).as_deref(), Some("General"));
        assert_eq!(name("DE", None).as_deref(), Some("DE"), "first of equals");
        assert_eq!(
            name("de", None).as_deref(),
            Some("DE"),
            "case does not matter"
        );
        assert_eq!(name("FR", Some("books")).as_deref(), Some("Books"));
        assert_eq!(name("DE", Some("books")).as_deref(), Some("DE books"));
        assert_eq!(name("DE", Some("tea")).as_deref(), Some("DE"));
        assert!(pick_rate(&[], "DE", None).is_none());
        let only_de = [rate("DE", 1900, "DE", None, true)];
        assert!(pick_rate(&only_de, "FR", None).is_none());
    }

    #[test]
    fn a_percent_discount_is_rounded_once_and_split_over_the_lines() {
        let d = Discount {
            kind: DiscountKind::Percent,
            value: 1000,
            category: None,
        };
        // 10% of 3 x 33 = 99 -> 9.9 -> 10; split 4,3,3 (largest remainder, earlier first).
        let ls = [line(33, None), line(33, None), line(33, None)];
        let p = price(&ls, Some(&d), 0, &[], "DE").unwrap();
        assert_eq!(p.discount_total, 10);
        assert_eq!(p.lines.iter().map(|l| l.discount).sum::<i64>(), 10);
        assert_eq!(p.total, 99 - 10);
    }

    #[test]
    fn a_fixed_discount_never_exceeds_what_it_applies_to() {
        let d = Discount {
            kind: DiscountKind::Fixed,
            value: 5000,
            category: None,
        };
        let p = price(&[line(1250, None)], Some(&d), 490, &[], "DE").unwrap();
        assert_eq!(p.discount_total, 1250, "capped at the lines");
        assert_eq!(p.total, 490, "delivery is not discounted");
    }

    #[test]
    fn a_category_discount_touches_only_those_lines_and_the_tax_follows_the_reduced_amount() {
        let d = Discount {
            kind: DiscountKind::Percent,
            value: 5000,
            category: Some("tea".into()),
        };
        let r = [rate("VAT", 1900, "", None, true)];
        let ls = [line(1000, Some("tea")), line(2000, Some("pots"))];
        let p = price(&ls, Some(&d), 0, &r, "DE").unwrap();
        assert_eq!((p.lines[0].discount, p.lines[1].discount), (500, 0));
        assert_eq!(p.discount_total, 500);
        // tax on 500 and on 2000, included in the price
        assert_eq!(p.lines[0].tax, round_div(500 * 1900, 11900));
        assert_eq!(p.lines[1].tax, round_div(2000 * 1900, 11900));
        assert_eq!(p.total, 2500);
        // No eligible line: no discount.
        let none = price(&[line(2000, Some("pots"))], Some(&d), 0, &r, "DE").unwrap();
        assert_eq!(none.discount_total, 0);
    }

    #[test]
    fn delivery_is_taxed_with_the_general_rate_and_a_category_rate_never_applies_to_it() {
        let rs = [
            rate("Books", 700, "", Some("books"), false),
            rate("VAT", 1900, "", None, false),
        ];
        let p = price(&[line(1000, Some("books"))], None, 500, &rs, "DE").unwrap();
        // book 7% on top: 70; delivery 19% on top: 95
        assert_eq!(p.lines[0].tax, 70);
        assert_eq!(p.shipping_tax, 95);
        assert_eq!(p.tax_total, 165);
        assert_eq!(p.total, 1000 + 500 + 165);
        assert_eq!(p.tax_lines.len(), 2);
    }

    #[test]
    fn overflowing_sums_are_refused_not_wrapped() {
        let big = [line(i64::MAX, None), line(1, None)];
        assert!(price(&big, None, 0, &[], "DE").is_err());
        assert!(price(&[line(i64::MAX, None)], None, 1, &[], "DE").is_err());
        let r = [rate("VAT", 1900, "", None, false)];
        assert!(price(&[line(i64::MAX - 10, None)], None, 0, &r, "DE").is_err());
    }
}
