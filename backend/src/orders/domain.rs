//! Pure selling rules: bill numbers, pricing, discount spreading, payment allocation.

use chrono::{DateTime, Datelike, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::inventory::ist;

/// Indian fiscal year (April to March) of an instant, in shop time: `"2026-27"`.
pub fn fiscal_year(at: DateTime<Utc>) -> String {
    let local = at.with_timezone(&ist());
    let y = local.year();
    let start = if local.month() >= 4 { y } else { y - 1 };
    format!("{start}-{:02}", (start + 1) % 100)
}

/// Bill number such as `JJ/2026-27/000042`.
pub fn format_number(fy: &str, seq: i64) -> String {
    format!("JJ/{fy}/{seq:06}")
}

#[derive(Debug, Clone)]
pub struct LineIn {
    pub quantity: i64,
    /// Carried for snapshotting only; does not affect the maths.
    pub list_price: Decimal,
    pub unit_price: Decimal,
    pub line_discount: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PricedLine {
    /// quantity x unit_price
    pub gross: Decimal,
    pub line_discount: Decimal,
    pub order_discount_share: Decimal,
    pub net_amount: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PriceError {
    /// Index of the line whose discount exceeds its gross.
    LineDiscountTooBig(usize),
    /// Order discount exceeds the subtotal (sum of lines after line discounts).
    OrderDiscountTooBig { max: Decimal },
}

fn to_paise(d: Decimal) -> i128 {
    let mut x = d.round_dp(2);
    x.rescale(2);
    x.mantissa()
}

fn from_paise(p: i128) -> Decimal {
    Decimal::from_i128_with_scale(p, 2)
}

/// Price lines and spread the order discount over line bases (gross - line discount).
///
/// Returns `(lines, subtotal, total)` where subtotal = sum of bases (lines at the price
/// charged, before order discount) and total = subtotal - order discount.
/// Shares are floored to the paisa; leftover paise go one each to the lines with the
/// largest truncated remainder, ties to the lowest index, so shares sum exactly.
pub fn price_lines(
    lines: &[LineIn],
    order_discount: Decimal,
) -> Result<(Vec<PricedLine>, Decimal, Decimal), PriceError> {
    let mut gross_p = Vec::with_capacity(lines.len());
    let mut base_p = Vec::with_capacity(lines.len());
    for (i, l) in lines.iter().enumerate() {
        let g = to_paise(l.unit_price) * i128::from(l.quantity);
        let d = to_paise(l.line_discount);
        if d > g {
            return Err(PriceError::LineDiscountTooBig(i));
        }
        gross_p.push(g);
        base_p.push(g - d);
    }
    let subtotal: i128 = base_p.iter().sum();
    let disc = to_paise(order_discount);
    if disc > subtotal {
        return Err(PriceError::OrderDiscountTooBig { max: from_paise(subtotal) });
    }

    let mut shares = vec![0i128; lines.len()];
    if subtotal > 0 && disc > 0 {
        let mut rems = Vec::with_capacity(lines.len());
        for (i, b) in base_p.iter().enumerate() {
            let n = disc * b;
            shares[i] = n / subtotal;
            rems.push((n % subtotal, i));
        }
        let leftover = disc - shares.iter().sum::<i128>();
        // Largest remainder first; ties to the lowest index.
        rems.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for &(_, i) in rems.iter().take(leftover as usize) {
            shares[i] += 1;
        }
    }

    let priced = lines
        .iter()
        .enumerate()
        .map(|(i, l)| PricedLine {
            gross: from_paise(gross_p[i]),
            line_discount: from_paise(to_paise(l.line_discount)),
            order_discount_share: from_paise(shares[i]),
            net_amount: from_paise(base_p[i] - shares[i]),
        })
        .collect();
    Ok((priced, from_paise(subtotal), from_paise(subtotal - disc)))
}

/// Spread a payment over invoices, oldest first. Zero shares are skipped.
pub fn allocate(amount: Decimal, owed: &[(Uuid, Decimal)]) -> Vec<(Uuid, Decimal)> {
    let mut left = amount;
    let mut out = Vec::new();
    for &(id, due) in owed {
        if left <= Decimal::ZERO {
            break;
        }
        let take = left.min(due);
        if take > Decimal::ZERO {
            out.push((id, take));
            left -= take;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    macro_rules! dec {
        ($e:expr) => {
            stringify!($e).parse::<Decimal>().unwrap()
        };
    }

    fn line(q: i64, price: Decimal, disc: Decimal) -> LineIn {
        LineIn { quantity: q, list_price: price, unit_price: price, line_discount: disc }
    }

    #[test]
    fn fiscal_year_boundary() {
        let t = |y, m, d, h, mi, s| Utc.with_ymd_and_hms(y, m, d, h, mi, s).unwrap();
        assert_eq!(fiscal_year(t(2026, 3, 31, 18, 29, 59)), "2025-26");
        assert_eq!(fiscal_year(t(2026, 3, 31, 18, 30, 0)), "2026-27");
        assert_eq!(fiscal_year(t(2027, 1, 15, 0, 0, 0)), "2026-27");
    }

    #[test]
    fn format_number_pads_to_six() {
        assert_eq!(format_number("2026-27", 42), "JJ/2026-27/000042");
    }

    #[test]
    fn price_lines_spreads_order_discount_exactly() {
        let lines = [line(1, dec!(100), dec!(0)), line(1, dec!(200), dec!(0)), line(1, dec!(300), dec!(0))];
        let (p, sub, total) = price_lines(&lines, dec!(100)).unwrap();
        let shares: Vec<_> = p.iter().map(|l| l.order_discount_share).collect();
        assert_eq!(shares, [dec!(16.67), dec!(33.33), dec!(50.00)]);
        assert_eq!(shares.iter().sum::<Decimal>(), dec!(100.00));
        assert_eq!(sub, dec!(600));
        assert_eq!(total, dec!(500.00));
        for l in &p {
            assert_eq!(l.net_amount, l.gross - l.line_discount - l.order_discount_share);
        }
    }

    #[test]
    fn price_lines_remainder_goes_to_largest_fraction() {
        let lines = [line(1, dec!(100), dec!(0)), line(1, dec!(100), dec!(0)), line(1, dec!(100), dec!(0))];
        let (p, _, _) = price_lines(&lines, dec!(10)).unwrap();
        let shares: Vec<_> = p.iter().map(|l| l.order_discount_share).collect();
        assert_eq!(shares, [dec!(3.34), dec!(3.33), dec!(3.33)]);
        assert_eq!(shares.iter().sum::<Decimal>(), dec!(10.00));
    }

    #[test]
    fn price_lines_line_discount_applies_before_spreading() {
        let lines = [line(2, dec!(500), dec!(100)), line(1, dec!(100), dec!(0))];
        let (p, sub, total) = price_lines(&lines, dec!(100)).unwrap();
        assert_eq!(p[0].gross, dec!(1000));
        assert_eq!(p[0].order_discount_share, dec!(90.00));
        assert_eq!(p[1].order_discount_share, dec!(10.00));
        assert_eq!(p[0].net_amount, dec!(810.00));
        assert_eq!(sub, dec!(1000));
        assert_eq!(total, dec!(900.00));
    }

    #[test]
    fn price_lines_rejects_discounts_larger_than_amount() {
        assert_eq!(
            price_lines(&[line(1, dec!(100), dec!(101))], dec!(0)).unwrap_err(),
            PriceError::LineDiscountTooBig(0)
        );
        let lines = [line(1, dec!(100), dec!(0)), line(1, dec!(200), dec!(0)), line(1, dec!(300), dec!(0))];
        assert_eq!(
            price_lines(&lines, dec!(601)).unwrap_err(),
            PriceError::OrderDiscountTooBig { max: dec!(600) }
        );
    }

    #[test]
    fn price_lines_full_discount_gives_zero_total() {
        let lines = [line(1, dec!(100), dec!(0)), line(3, dec!(33.33), dec!(0))];
        let (p, sub, total) = price_lines(&lines, dec!(199.99)).unwrap();
        assert_eq!(sub, dec!(199.99));
        assert_eq!(total, dec!(0.00));
        assert!(p.iter().all(|l| l.net_amount == dec!(0.00)));
    }

    #[test]
    fn allocate_fills_oldest_first() {
        let (a, b) = (Uuid::now_v7(), Uuid::now_v7());
        let owed = [(a, dec!(300)), (b, dec!(300))];
        assert_eq!(allocate(dec!(450), &owed), vec![(a, dec!(300)), (b, dec!(150))]);
        assert_eq!(allocate(dec!(300), &owed), vec![(a, dec!(300))]);
    }
}
