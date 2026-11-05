//! Money parsing for request bodies. Amounts travel as strings ("750.00").
use std::str::FromStr;

use rust_decimal::Decimal;

/// Largest amount that fits `NUMERIC(12,2)`.
pub const MAX_MONEY: &str = "9999999.99";

/// Parse a user-entered amount: digits, optional thousands commas, at most two decimals.
/// Returns the amount scaled to two decimals, or a message to show the person.
pub fn parse_money(s: &str) -> Result<Decimal, String> {
    let t = s.trim().replace(',', "");
    let valid = !t.is_empty()
        && t.chars().all(|c| c.is_ascii_digit() || c == '.')
        && t.matches('.').count() <= 1
        && !t.starts_with('.')
        && !t.ends_with('.');
    if !valid {
        return Err("Enter an amount like 750 or 750.50.".into());
    }
    if t.split_once('.').is_some_and(|(_, f)| f.len() > 2) {
        return Err("Use at most two decimal places.".into());
    }
    let mut d =
        Decimal::from_str(&t).map_err(|_| "Enter an amount like 750 or 750.50.".to_string())?;
    if d > Decimal::from_str(MAX_MONEY).unwrap() {
        return Err("That amount is too large.".into());
    }
    d.rescale(2);
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn parse_money_accepts_common_forms() {
        assert_eq!(parse_money("750").unwrap(), d("750.00"));
        assert_eq!(parse_money("1,090.00").unwrap(), d("1090.00"));
        assert_eq!(parse_money("750.5").unwrap(), d("750.50"));
        assert_eq!(parse_money("0").unwrap(), d("0.00"));
        assert_eq!(parse_money("9999999.99").unwrap(), d("9999999.99"));
        assert_eq!(parse_money("750").unwrap().to_string(), "750.00");
    }

    #[test]
    fn parse_money_rejects_bad_values() {
        for bad in ["-1", "1.234", "10000000", "abc", "", "  ", "1e3"] {
            assert!(parse_money(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
