//! Pure inventory rules: movement types and weighted-average cost (Architecture.md §8, §9).
use rust_decimal::{Decimal, RoundingStrategy};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementType {
    OpeningBalance,
    PurchaseReceipt,
    AdjustmentIn,
    AdjustmentOut,
    Damage,
    Sale,
}

impl MovementType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpeningBalance => "opening_balance",
            Self::PurchaseReceipt => "purchase_receipt",
            Self::AdjustmentIn => "adjustment_in",
            Self::AdjustmentOut => "adjustment_out",
            Self::Damage => "damage",
            Self::Sale => "sale",
        }
    }

    /// True when the movement adds stock (stored with a positive quantity).
    pub fn is_inbound(self) -> bool {
        matches!(
            self,
            Self::OpeningBalance | Self::PurchaseReceipt | Self::AdjustmentIn
        )
    }
}

impl FromStr for MovementType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        Ok(match s {
            "opening_balance" => Self::OpeningBalance,
            "purchase_receipt" => Self::PurchaseReceipt,
            "adjustment_in" => Self::AdjustmentIn,
            "adjustment_out" => Self::AdjustmentOut,
            "damage" => Self::Damage,
            "sale" => Self::Sale,
            _ => return Err(()),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    GoodsReceipt,
    StockCount,
    Adjustment,
    Order,
}

impl SourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GoodsReceipt => "goods_receipt",
            Self::StockCount => "stock_count",
            Self::Adjustment => "adjustment",
            Self::Order => "order",
        }
    }
}

/// Weighted-average cost after `qty_in` units arrive at `cost`.
/// Unknown cost keeps the old average; no stock or unknown average takes the new cost.
pub fn next_avg_cost(
    on_hand: i64,
    avg: Option<Decimal>,
    qty_in: i64,
    cost: Option<Decimal>,
) -> Option<Decimal> {
    let Some(cost) = cost else { return avg };
    let Some(avg) = avg.filter(|_| on_hand > 0) else {
        return Some(cost);
    };
    let total = Decimal::from(on_hand) * avg + Decimal::from(qty_in) * cost;
    Some(
        (total / Decimal::from(on_hand + qty_in))
            .round_dp_with_strategy(4, RoundingStrategy::MidpointAwayFromZero),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    /// (on_hand, avg, qty_in, cost, expected)
    type Case = (
        i64,
        Option<&'static str>,
        i64,
        Option<&'static str>,
        Option<&'static str>,
    );

    #[test]
    fn avg_cost_cases() {
        let cases: [Case; 8] = [
            (0, None, 10, Some("100"), Some("100")),
            (10, Some("100"), 10, Some("120"), Some("110.0000")),
            (3, Some("100"), 1, Some("101"), Some("100.2500")),
            (2, Some("10"), 1, Some("11"), Some("10.3333")),
            // Restocking a sold-out product: the old average is not blended in.
            (0, Some("90"), 5, Some("120"), Some("120")),
            (5, None, 5, Some("80"), Some("80")),
            (5, Some("80"), 5, None, Some("80")),
            (0, None, 5, None, None),
        ];
        for (on_hand, avg, qty, cost, want) in cases {
            let got = next_avg_cost(on_hand, avg.map(d), qty, cost.map(d));
            assert_eq!(got, want.map(d), "{on_hand} {avg:?} {qty} {cost:?}");
        }
    }

    #[test]
    fn movement_type_round_trips() {
        for t in [
            MovementType::OpeningBalance,
            MovementType::PurchaseReceipt,
            MovementType::AdjustmentIn,
            MovementType::AdjustmentOut,
            MovementType::Damage,
            MovementType::Sale,
        ] {
            assert_eq!(MovementType::from_str(t.as_str()), Ok(t));
        }
        assert_eq!(MovementType::OpeningBalance.as_str(), "opening_balance");
        assert!(MovementType::PurchaseReceipt.is_inbound());
        assert!(!MovementType::Damage.is_inbound());
        assert!(!MovementType::Sale.is_inbound());
        assert!(MovementType::from_str("refund").is_err());
        assert_eq!(SourceType::GoodsReceipt.as_str(), "goods_receipt");
        assert_eq!(SourceType::Order.as_str(), "order");
    }
}
