//! Delivery inputs, checked: the checkout block, an edit before pickup and a status step.
use rust_decimal::Decimal;
use serde::Deserialize;

use super::domain::{Collector, FailReason, PayMethod};
use crate::{
    catalog::dto::double,
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

/// Field names are `delivery.`-prefixed in a checkout body and bare in a PATCH body.
const CHECKOUT: &str = "delivery.";
const EDIT: &str = "";

fn bad(prefix: &str, name: &str, message: impl Into<String>) -> AppError {
    AppError::FieldInvalid {
        field: format!("{prefix}{name}"),
        message: message.into(),
    }
}

fn trimmed(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn required(
    prefix: &str,
    v: &Option<String>,
    name: &str,
    label: &str,
    max: usize,
) -> AppResult<String> {
    let s = trimmed(v).ok_or_else(|| bad(prefix, name, format!("Enter {label}.")))?;
    if s.chars().count() > max {
        return Err(bad(
            prefix,
            name,
            format!("Keep {label} to {max} characters or fewer."),
        ));
    }
    Ok(s)
}

fn optional(
    prefix: &str,
    v: &Option<String>,
    name: &str,
    label: &str,
    max: usize,
) -> AppResult<Option<String>> {
    let s = trimmed(v);
    if s.as_ref().is_some_and(|s| s.chars().count() > max) {
        return Err(bad(
            prefix,
            name,
            format!("Keep {label} to {max} characters or fewer."),
        ));
    }
    Ok(s)
}

fn recipient_name(prefix: &str, v: &Option<String>) -> AppResult<String> {
    required(prefix, v, "recipient_name", "the recipient's name", 120)
}

fn recipient_phone(prefix: &str, v: &Option<String>) -> AppResult<String> {
    let raw = trimmed(v).ok_or_else(|| {
        bad(
            prefix,
            "recipient_phone",
            "Enter the recipient's phone number.",
        )
    })?;
    normalize_phone(&raw).map_err(|m| bad(prefix, "recipient_phone", m))
}

fn address_line(prefix: &str, v: &Option<String>) -> AppResult<String> {
    required(prefix, v, "address_line", "the address", 300)
}

fn locality(prefix: &str, v: &Option<String>) -> AppResult<String> {
    required(prefix, v, "locality", "the locality", 80)
}

fn landmark(prefix: &str, v: &Option<String>) -> AppResult<Option<String>> {
    optional(prefix, v, "landmark", "the landmark", 120)
}

fn instructions(prefix: &str, v: &Option<String>) -> AppResult<Option<String>> {
    optional(prefix, v, "instructions", "the instructions", 300)
}

fn fee(prefix: &str, v: &Option<String>) -> AppResult<Decimal> {
    parse_money(
        trimmed(v)
            .ok_or_else(|| {
                bad(
                    prefix,
                    "fee",
                    "Enter the delivery fee, or 0 for free delivery.",
                )
            })?
            .as_str(),
    )
    .map_err(|m| bad(prefix, "fee", m))
}

fn driver_pay(prefix: &str, v: &Option<String>) -> AppResult<Option<Decimal>> {
    trimmed(v)
        .map(|s| parse_money(&s))
        .transpose()
        .map_err(|m| bad(prefix, "driver_pay", m))
}

fn collector(prefix: &str, s: &str) -> AppResult<Collector> {
    Collector::parse(s).ok_or_else(|| {
        bad(
            prefix,
            "collector",
            "Pick who collects the money: the shop or the driver.",
        )
    })
}

fn pay_method(prefix: &str, s: &str) -> AppResult<PayMethod> {
    PayMethod::parse(s).ok_or_else(|| {
        bad(
            prefix,
            "pay_method",
            "Pick how the driver is paid: from the collection or by the shop.",
        )
    })
}

/// The driver can only keep their pay from money they collect.
pub fn check_collection(prefix: &str, c: Collector, p: PayMethod) -> AppResult<()> {
    if p == PayMethod::RetainFromCollection && c == Collector::Shop {
        return Err(bad(
            prefix,
            "pay_method",
            "The driver can only keep their pay from money they collect. Pick \"paid by the shop\" or let the driver collect.",
        ));
    }
    Ok(())
}

/// The pay method when the request doesn't name one: the shop pays when the shop collects.
pub fn default_pay_method(c: Collector) -> PayMethod {
    match c {
        Collector::Shop => PayMethod::PaidByShop,
        Collector::Driver => PayMethod::RetainFromCollection,
    }
}

/// Checks the delivery block. Collector defaults to the driver; the driver's pay method
/// defaults to keeping it from the collection, or to the shop paying when the shop collects.
pub fn validate_delivery(raw: &DeliveryRequest) -> AppResult<DeliveryIn> {
    let p = CHECKOUT;
    let recipient_name = recipient_name(p, &raw.recipient_name)?;
    let recipient_phone = recipient_phone(p, &raw.recipient_phone)?;
    let address_line = address_line(p, &raw.address_line)?;
    let locality = locality(p, &raw.locality)?;
    let landmark = landmark(p, &raw.landmark)?;
    let instructions = instructions(p, &raw.instructions)?;
    let fee = fee(p, &raw.fee)?;
    let driver_pay = driver_pay(p, &raw.driver_pay)?;
    let collector = match trimmed(&raw.collector) {
        None => Collector::Driver,
        Some(s) => collector(p, &s)?,
    };
    let pay_method = match trimmed(&raw.pay_method) {
        None => default_pay_method(collector),
        Some(s) => pay_method(p, &s)?,
    };
    check_collection(p, collector, pay_method)?;
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

/// `PATCH /deliveries/{id}`: any arrangement field; absent means unchanged. For the optional
/// texts and the driver pay, `null` or empty clears the value.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditRequest {
    pub recipient_name: Option<String>,
    pub recipient_phone: Option<String>,
    pub address_line: Option<String>,
    pub locality: Option<String>,
    #[serde(default, deserialize_with = "double")]
    pub landmark: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub instructions: Option<Option<String>>,
    pub fee: Option<String>,
    #[serde(default, deserialize_with = "double")]
    pub driver_pay: Option<Option<String>>,
    pub collector: Option<String>,
    pub pay_method: Option<String>,
}

/// An edit, checked field by field. `None` leaves the field as it is. Whether collector and pay
/// method fit together is checked against the merged values by the caller.
#[derive(Debug, Default)]
pub struct EditIn {
    pub recipient_name: Option<String>,
    pub recipient_phone: Option<String>,
    pub address_line: Option<String>,
    pub locality: Option<String>,
    pub landmark: Option<Option<String>>,
    pub instructions: Option<Option<String>>,
    pub fee: Option<Decimal>,
    pub driver_pay: Option<Option<Decimal>>,
    pub collector: Option<Collector>,
    pub pay_method: Option<PayMethod>,
}

/// Checks the fields an edit names, with the same rules as checkout and bare field names.
pub fn validate_edit(raw: &EditRequest) -> AppResult<EditIn> {
    let p = EDIT;
    Ok(EditIn {
        recipient_name: raw
            .recipient_name
            .as_ref()
            .map(|_| recipient_name(p, &raw.recipient_name))
            .transpose()?,
        recipient_phone: raw
            .recipient_phone
            .as_ref()
            .map(|_| recipient_phone(p, &raw.recipient_phone))
            .transpose()?,
        address_line: raw
            .address_line
            .as_ref()
            .map(|_| address_line(p, &raw.address_line))
            .transpose()?,
        locality: raw
            .locality
            .as_ref()
            .map(|_| locality(p, &raw.locality))
            .transpose()?,
        landmark: raw.landmark.as_ref().map(|v| landmark(p, v)).transpose()?,
        instructions: raw
            .instructions
            .as_ref()
            .map(|v| instructions(p, v))
            .transpose()?,
        fee: raw.fee.as_ref().map(|_| fee(p, &raw.fee)).transpose()?,
        driver_pay: raw
            .driver_pay
            .as_ref()
            .map(|v| driver_pay(p, v))
            .transpose()?,
        collector: raw
            .collector
            .as_deref()
            .map(|s| collector(p, s.trim()))
            .transpose()?,
        pay_method: raw
            .pay_method
            .as_deref()
            .map(|s| pay_method(p, s.trim()))
            .transpose()?,
    })
}

/// Money typed on an assignment or a retry (`driver_pay`, `trip_pay`); empty is none.
pub fn optional_money(v: &Option<String>, name: &str) -> AppResult<Option<Decimal>> {
    trimmed(v)
        .map(|s| parse_money(&s))
        .transpose()
        .map_err(|m| bad(EDIT, name, m))
}

/// `POST /deliveries/{id}/status` and the driver's own status call.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusRequest {
    pub to: String,
    pub received_by: Option<String>,
    pub reason: Option<String>,
    pub note: Option<String>,
}

/// A status step asked for by the owner or the driver, checked.
#[derive(Debug, Clone)]
pub enum StatusChange {
    PickedUp,
    Delivered {
        received_by: String,
    },
    Failed {
        reason: FailReason,
        note: Option<String>,
    },
}

pub fn validate_status(raw: &StatusRequest) -> AppResult<StatusChange> {
    let p = EDIT;
    match raw.to.trim() {
        "picked_up" => Ok(StatusChange::PickedUp),
        "delivered" => {
            let received_by = trimmed(&raw.received_by)
                .filter(|s| s.chars().count() <= 80)
                .ok_or_else(|| bad(p, "received_by", "Write who received it."))?;
            Ok(StatusChange::Delivered { received_by })
        }
        "failed" => {
            let reason = trimmed(&raw.reason)
                .and_then(|s| FailReason::parse(&s))
                .ok_or_else(|| bad(p, "reason", "Pick why it couldn't be delivered."))?;
            let note = optional(p, &raw.note, "note", "the note", 500)?;
            if reason == FailReason::Other && note.is_none() {
                return Err(bad(p, "note", "Write what happened."));
            }
            Ok(StatusChange::Failed { reason, note })
        }
        _ => Err(bad(
            p,
            "to",
            "Pick picked up, delivered or couldn't deliver.",
        )),
    }
}
