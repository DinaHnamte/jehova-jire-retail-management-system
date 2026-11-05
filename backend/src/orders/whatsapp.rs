//! WhatsApp receipt text and the click-to-chat link (spec §5). One function builds the text so
//! the screens only open the link.
use rust_decimal::Decimal;

use super::read::PublicReceipt;
use crate::{inventory::ist, shared::money::format_rupees};

const MAX_ROWS: usize = 15;

/// `₹1,200` for whole amounts, `₹1,200.50` otherwise.
pub fn money_short(d: Decimal) -> String {
    let s = format_rupees(d);
    s.strip_suffix(".00").map(str::to_string).unwrap_or(s)
}

/// The receipt as a WhatsApp message. `payment_methods` are the labels actually paid with
/// (`cash`, `UPI`), already in display order.
pub fn receipt_text(r: &PublicReceipt, url: &str, payment_methods: &[&str]) -> String {
    let date = r.created_at.with_timezone(&ist()).format("%-d %b %Y");
    let mut out = format!("*{}* — Receipt {}\n{date}\n\n", r.shop_name, r.number);
    for l in r.lines.iter().take(MAX_ROWS) {
        let what = l.code.as_deref().unwrap_or(&l.name);
        out.push_str(&format!(
            "{} × {what} — {}\n",
            l.quantity,
            money_short(l.net_amount)
        ));
    }
    if r.lines.len() > MAX_ROWS {
        out.push_str(&format!(
            "+{} more items — see link\n",
            r.lines.len() - MAX_ROWS
        ));
    }
    if r.order_discount > Decimal::ZERO {
        out.push_str(&format!("Discount — −{}\n", money_short(r.order_discount)));
    }
    out.push_str(&format!("*Total {}*\n", money_short(r.total)));
    let methods = if payment_methods.is_empty() {
        String::new()
    } else {
        format!(" ({})", payment_methods.join(" + "))
    };
    if r.owed.is_zero() {
        out.push_str(&format!("Paid in full{methods}\n"));
    } else if r.paid.is_zero() {
        out.push_str(&format!("*Due {}*\n", money_short(r.owed)));
    } else {
        out.push_str(&format!(
            "Paid {}{methods} · *Due {}*\n",
            money_short(r.paid),
            money_short(r.owed)
        ));
    }
    out.push_str(&format!("\nView: {url}"));
    out
}

/// `https://wa.me/91<phone>?text=<percent-encoded text>`.
pub fn wa_link(phone10: &str, text: &str) -> String {
    let mut enc = String::with_capacity(text.len() * 2);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            enc.push(b as char);
        } else {
            enc.push_str(&format!("%{b:02X}"));
        }
    }
    format!("https://wa.me/91{phone10}?text={enc}")
}

#[cfg(test)]
mod tests {
    use super::super::read::ReceiptLine;
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    fn line(name: &str, code: Option<&str>, qty: i64, amount: &str) -> ReceiptLine {
        ReceiptLine {
            name: name.into(),
            code: code.map(str::to_string),
            quantity: qty,
            unit_price: d(amount),
            list_price: d(amount),
            net_amount: d(amount),
        }
    }

    fn receipt(lines: Vec<ReceiptLine>, discount: &str, total: &str, paid: &str) -> PublicReceipt {
        let (total, paid) = (d(total), d(paid));
        PublicReceipt {
            shop_name: "Jehovah Jireh".into(),
            number: "JJ/2026-27/000042".into(),
            // 6 Nov 2026, 10:00 IST
            created_at: Utc.with_ymd_and_hms(2026, 11, 6, 4, 30, 0).unwrap(),
            customer_name: Some("Mary".into()),
            lines,
            subtotal: total + d(discount),
            order_discount: d(discount),
            total,
            paid,
            owed: total - paid,
            cancelled: false,
        }
    }

    #[test]
    fn receipt_text_matches_spec() {
        let r = receipt(
            vec![
                line("Plate", Some("IP 1060-11\""), 2, "900"),
                line("Bowl", Some("B 6143-8 MP4952"), 1, "350"),
            ],
            "50",
            "1200",
            "800",
        );
        let text = receipt_text(&r, "https://jehovah-jire.hnamtek.in/r/TOKEN", &["cash"]);
        let want = "*Jehovah Jireh* — Receipt JJ/2026-27/000042\n\
                    6 Nov 2026\n\
                    \n\
                    2 × IP 1060-11\" — ₹900\n\
                    1 × B 6143-8 MP4952 — ₹350\n\
                    Discount — −₹50\n\
                    *Total ₹1,200*\n\
                    Paid ₹800 (cash) · *Due ₹400*\n\
                    \n\
                    View: https://jehovah-jire.hnamtek.in/r/TOKEN";
        assert_eq!(text, want);
    }

    #[test]
    fn receipt_text_paid_in_full_unpaid_and_name_fallback() {
        let r = receipt(
            vec![line("Plate", None, 1, "100.50")],
            "0",
            "100.50",
            "100.50",
        );
        let t = receipt_text(&r, "u", &["cash", "UPI"]);
        assert!(
            t.contains("1 × Plate — ₹100.50\n*Total ₹100.50*\nPaid in full (cash + UPI)\n"),
            "{t}"
        );
        assert!(!t.contains("Discount"));
        let r = receipt(vec![line("Plate", None, 1, "100")], "0", "100", "0");
        let t = receipt_text(&r, "u", &[]);
        assert!(t.contains("*Total ₹100*\n*Due ₹100*\n"), "{t}");
    }

    #[test]
    fn receipt_text_truncates_after_15_lines() {
        let lines = (0..17)
            .map(|i| line(&format!("Item {i}"), None, 1, "10"))
            .collect();
        let t = receipt_text(&receipt(lines, "0", "170", "170"), "u", &["cash"]);
        assert!(t.contains("1 × Item 14 — ₹10\n"));
        assert!(!t.contains("Item 15"));
        assert!(t.contains("+2 more items — see link\n"));
    }

    #[test]
    fn wa_link_encodes() {
        assert_eq!(
            wa_link("9862012345", "a b*\n₹"),
            "https://wa.me/919862012345?text=a%20b%2A%0A%E2%82%B9"
        );
    }
}
