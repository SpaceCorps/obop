//! OAS knows four account kinds (T assets, S liabilities and equity, K costs, I income);
//! `accounting_account` has twelve types. The mapping is lossy, so a run only uses the built-in
//! table when the person accepts it (`--default-account-types`), and `--account-types FILE`
//! overrides single accounts. The ranges follow the class structure of Swedish charts of
//! accounts (1 assets, 2 equity and liabilities, 3 to 8 results) and are UNVERIFIED; the
//! account numbers and names themselves are always the user's own, copied as they are.

use std::collections::BTreeMap;

pub const TYPES: &[&str] = &[
    "asset_receivable",
    "asset_cash",
    "asset_current",
    "asset_fixed",
    "liability_payable",
    "liability_current",
    "liability_non_current",
    "equity",
    "income",
    "income_other",
    "expense",
    "expense_direct_cost",
];

pub fn default_type(kind: &str, n: i64) -> Option<&'static str> {
    Some(match kind {
        "T" => match n {
            1510..=1519 => "asset_receivable",
            1900..=1999 => "asset_cash",
            1000..=1399 => "asset_fixed",
            _ => "asset_current",
        },
        "S" => match n {
            2000..=2099 => "equity",
            2300..=2399 => "liability_non_current",
            2400..=2499 => "liability_payable",
            _ => "liability_current",
        },
        "K" => match n {
            4000..=4999 => "expense_direct_cost",
            _ => "expense",
        },
        "I" => match n {
            3900..=3999 | 8000..=8999 => "income_other",
            _ => "income",
        },
        _ => return None,
    })
}

/// `number,type` per line; blank lines, `#` comments and a `number,type` header are ignored.
pub fn parse_overrides(text: &str) -> Result<BTreeMap<i64, &'static str>, String> {
    let mut out = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (n, t) = line
            .split_once(',')
            .ok_or_else(|| format!("account types, line {}: expected number,type", i + 1))?;
        let Ok(n) = n.trim().parse::<i64>() else {
            if i == 0 && n.trim().eq_ignore_ascii_case("number") {
                continue;
            }
            return Err(format!(
                "account types, line {}: {:?} is not a number",
                i + 1,
                n.trim()
            ));
        };
        let t = t.trim();
        let ty = TYPES.iter().find(|x| **x == t).ok_or_else(|| {
            format!(
                "account types, line {}: {t:?} is not one of {}",
                i + 1,
                TYPES.join(", ")
            )
        })?;
        out.insert(n, *ty);
    }
    Ok(out)
}
