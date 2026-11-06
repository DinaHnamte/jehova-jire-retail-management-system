//! The delivery block of a checkout request, checked.
use rust_decimal::Decimal;
use serde::Deserialize;

use super::domain::{Collector, PayMethod};
use crate::{
    error::{AppError, AppResult},
    shared::{money::parse_money, phone::normalize_phone},
};

#[derive(Debug, Deserialize)]
pub struct DeliveryRequest {
    pub recipient_name: Option<String>,
    pub recipient_phone: Option<String>,
    pub address_line: Option<String>,
    pub locality: Option<String>,
    pub landmark: Option<String>,
    pub instructions: Option<String>,
    pub fee: Option<String>,
    pub driver_pay: Option<String>,
    pub collector: Option<String>,
    pub pay_method: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeliveryIn {
    pub recipient_name: String,
    pub recipient_phone: String,
    pub address_line: String,
    pub locality: String,
    pub landmark: Option<String>,
    pub instructions: Option<String>,
    pub fee: Decimal,
    pub driver_pay: Option<Decimal>,
    pub collector: Collector,
    pub pay_method: PayMethod,
}

fn bad(name: &str, message: impl Into<String>) -> AppError {
    AppError::FieldInvalid {
        field: format!("delivery.{name}"),
        message: message.into(),
    }
}

fn trimmed(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn required(v: &Option<String>, name: &str, label: &str, max: usize) -> AppResult<String> {
    let s = trimmed(v).ok_or_else(|| bad(name, format!("Enter {label}.")))?;
    if s.chars().count() > max {
        return Err(bad(
            name,
            format!("Keep {label} to {max} characters or fewer."),
        ));
    }
    Ok(s)
}

fn optional(v: &Option<String>, name: &str, label: &str, max: usize) -> AppResult<Option<String>> {
    let s = trimmed(v);
    if s.as_ref().is_some_and(|s| s.chars().count() > max) {
        return Err(bad(
            name,
            format!("Keep {label} to {max} characters or fewer."),
        ));
    }
    Ok(s)
}

/// Checks the delivery block. Collector defaults to the driver; the driver's pay method
/// defaults to keeping it from the collection, or to the shop paying when the shop collects.
pub fn validate_delivery(raw: &DeliveryRequest) -> AppResult<DeliveryIn> {
    let recipient_name = required(
        &raw.recipient_name,
        "recipient_name",
        "the recipient's name",
        120,
    )?;
    let phone_raw = trimmed(&raw.recipient_phone)
        .ok_or_else(|| bad("recipient_phone", "Enter the recipient's phone number."))?;
    let recipient_phone = normalize_phone(&phone_raw).map_err(|m| bad("recipient_phone", m))?;
    let address_line = required(&raw.address_line, "address_line", "the address", 300)?;
    let locality = required(&raw.locality, "locality", "the locality", 80)?;
    let landmark = optional(&raw.landmark, "landmark", "the landmark", 120)?;
    let instructions = optional(&raw.instructions, "instructions", "the instructions", 300)?;
    let fee = parse_money(
        trimmed(&raw.fee)
            .ok_or_else(|| bad("fee", "Enter the delivery fee, or 0 for free delivery."))?
            .as_str(),
    )
    .map_err(|m| bad("fee", m))?;
    let driver_pay = trimmed(&raw.driver_pay)
        .map(|s| parse_money(&s))
        .transpose()
        .map_err(|m| bad("driver_pay", m))?;
    let collector = match trimmed(&raw.collector) {
        None => Collector::Driver,
        Some(s) => Collector::parse(&s).ok_or_else(|| {
            bad(
                "collector",
                "Pick who collects the money: the shop or the driver.",
            )
        })?,
    };
    let pay_method = match trimmed(&raw.pay_method) {
        None if collector == Collector::Shop => PayMethod::PaidByShop,
        None => PayMethod::RetainFromCollection,
        Some(s) => PayMethod::parse(&s).ok_or_else(|| {
            bad(
                "pay_method",
                "Pick how the driver is paid: from the collection or by the shop.",
            )
        })?,
    };
    if pay_method == PayMethod::RetainFromCollection && collector == Collector::Shop {
        return Err(bad(
            "pay_method",
            "The driver can only keep their pay from money they collect. Pick \"paid by the shop\" or let the driver collect.",
        ));
    }
    Ok(DeliveryIn {
        recipient_name,
        recipient_phone,
        address_line,
        locality,
        landmark,
        instructions,
        fee,
        driver_pay,
        collector,
        pay_method,
    })
}
