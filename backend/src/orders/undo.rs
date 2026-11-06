//! Pure return rules for cancelling and returning sold items (Phase 5b rulings 3 and 4).

use rust_decimal::Decimal;

fn to_paise(d: Decimal) -> i128 {
    let mut x = d.round_dp(2);
    x.rescale(2);
    x.mantissa()
}

fn from_paise(p: i128) -> Decimal {
    Decimal::from_i128_with_scale(p, 2)
}

/// `round(n * k / q)` in paise, half up (inputs are non-negative).
fn share(n: i128, k: i128, q: i128) -> i128 {
    (2 * n * k + q) / (2 * q)
}

/// Refund value of `take` units of a line, computed cumulatively so that returns made in
/// several goes add up exactly to the line's net amount:
/// `round(N * (already + take) / quantity) - round(N * already / quantity)`, half up.
/// The caller guarantees `0 < take <= quantity - already`.
pub fn return_line_amount(net: Decimal, quantity: i64, already: i64, take: i64) -> Decimal {
    let n = to_paise(net);
    let q = i128::from(quantity);
    let upto = share(n, i128::from(already + take), q);
    let before = share(n, i128::from(already), q);
    from_paise(upto - before)
}

/// Split a return amount into `(owed_reduced, refund)`: what the customer owes is reduced
/// first, the rest is paid back. An `owed` below zero counts as zero.
pub fn split_return(amount: Decimal, owed: Decimal) -> (Decimal, Decimal) {
    let owed = owed.max(Decimal::ZERO);
    let reduced = amount.min(owed);
    (reduced, amount - reduced)
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! dec {
        ($e:expr) => {
            stringify!($e).parse::<Decimal>().unwrap()
        };
    }

    #[test]
    fn cumulative_return_amounts_sum_to_net() {
        let net = dec!(100.00);
        let mut parts = Vec::new();
        for already in 0..3 {
            parts.push(return_line_amount(net, 3, already, 1));
        }
        assert_eq!(parts, [dec!(33.33), dec!(33.34), dec!(33.33)]);
        assert_eq!(parts.iter().sum::<Decimal>(), dec!(100.00));
    }

    #[test]
    fn whole_line_return_is_net() {
        assert_eq!(return_line_amount(dec!(449.99), 2, 0, 2), dec!(449.99));
    }

    #[test]
    fn zero_net_line_returns_zero() {
        assert_eq!(return_line_amount(dec!(0.00), 3, 1, 2), dec!(0.00));
    }

    #[test]
    fn split_return_reduces_owed_first() {
        assert_eq!(split_return(dec!(300), dec!(200)), (dec!(200), dec!(100)));
        assert_eq!(split_return(dec!(150), dec!(200)), (dec!(150), dec!(0)));
        assert_eq!(split_return(dec!(100), dec!(0)), (dec!(0), dec!(100)));
        assert_eq!(split_return(dec!(100), dec!(-5)), (dec!(0), dec!(100)));
    }
}
