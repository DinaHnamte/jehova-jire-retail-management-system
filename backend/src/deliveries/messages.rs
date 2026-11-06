//! WhatsApp texts for a delivery (spec §6) and the map link. Built on the server, like receipt
//! links (Ruling 10), so the screens only open them.
use rust_decimal::Decimal;

use crate::orders::whatsapp::{money_short, url_encode};

/// What the driver needs to find the place and know what to collect.
pub struct Job<'a> {
    pub id: uuid::Uuid,
    pub order_number: &'a str,
    pub recipient_name: &'a str,
    pub recipient_phone: &'a str,
    pub address_line: &'a str,
    pub locality: &'a str,
    pub landmark: Option<&'a str>,
    pub instructions: Option<&'a str>,
    pub amount_to_collect: Decimal,
}

/// A Google Maps search for `address_line, locality`.
pub fn maps_url(address_line: &str, locality: &str) -> String {
    format!(
        "https://www.google.com/maps/search/?api=1&query={}",
        url_encode(&format!("{address_line}, {locality}"))
    )
}

pub fn driver_text(j: &Job, public_url: &str) -> String {
    let mut out = format!(
        "*Delivery {}*\n{} · {}\n{}, {}\n",
        j.order_number, j.recipient_name, j.recipient_phone, j.address_line, j.locality
    );
    if let Some(l) = j.landmark {
        out.push_str(&format!("Landmark: {l}\n"));
    }
    if let Some(n) = j.instructions {
        out.push_str(&format!("Note: {n}\n"));
    }
    out.push_str(&format!("Map: {}\n", maps_url(j.address_line, j.locality)));
    if j.amount_to_collect.is_zero() {
        out.push_str("Collect: nothing\n");
    } else {
        out.push_str(&format!("Collect: {}\n", money_short(j.amount_to_collect)));
    }
    out.push_str(&format!("Open: {public_url}/driver/deliveries/{}", j.id));
    out
}

/// "On its way" for the customer; the amount is left out when nothing is owed.
pub fn customer_text(
    shop: &str,
    order_number: &str,
    driver_name: &str,
    driver_phone: &str,
    owed: Decimal,
) -> String {
    let mut out = format!(
        "Your order {order_number} from {shop} is on its way with {driver_name} ({driver_phone})."
    );
    if owed > Decimal::ZERO {
        out.push_str(&format!(" Amount to pay: {}.", money_short(owed)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn job(collect: &str, landmark: Option<&'static str>) -> Job<'static> {
        Job {
            id: uuid::Uuid::nil(),
            order_number: "JJ/2026-27/000042",
            recipient_name: "Lalthangi",
            recipient_phone: "9862000000",
            address_line: "House 12, Chanmari West",
            locality: "Chanmari",
            landmark,
            instructions: None,
            amount_to_collect: Decimal::from_str(collect).unwrap(),
        }
    }

    #[test]
    fn driver_text_matches_spec() {
        let t = driver_text(&job("1050", Some("near the church")), "https://x.test");
        assert_eq!(
            t,
            "*Delivery JJ/2026-27/000042*\n\
             Lalthangi · 9862000000\n\
             House 12, Chanmari West, Chanmari\n\
             Landmark: near the church\n\
             Map: https://www.google.com/maps/search/?api=1&query=House%2012%2C%20Chanmari%20West%2C%20Chanmari\n\
             Collect: ₹1,050\n\
             Open: https://x.test/driver/deliveries/00000000-0000-0000-0000-000000000000"
        );
        let t = driver_text(&job("0", None), "u");
        assert!(t.contains("Collect: nothing\n"), "{t}");
        assert!(!t.contains("Landmark"), "{t}");
    }

    #[test]
    fn customer_text_leaves_out_nothing_owed() {
        let d = |s| Decimal::from_str(s).unwrap();
        assert_eq!(
            customer_text("Jehovah Jireh", "JJ/1", "Lalbiaka", "9862000001", d("1050")),
            "Your order JJ/1 from Jehovah Jireh is on its way with Lalbiaka (9862000001). Amount to pay: ₹1,050."
        );
        assert_eq!(
            customer_text("Jehovah Jireh", "JJ/1", "Lalbiaka", "9862000001", d("0")),
            "Your order JJ/1 from Jehovah Jireh is on its way with Lalbiaka (9862000001)."
        );
    }
}
