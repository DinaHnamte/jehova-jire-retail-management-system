//! Product rules: pure validation, no I/O (Architecture.md §5.1, §6; ADR-0003).
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    error::AppError,
    shared::{
        code::{ProductCode, normalize_code},
        money::{format_rupees, parse_money},
    },
};

pub const UNITS: &[&str] = &["piece", "set", "pack", "box", "pair", "dozen"];
const MAX_NAME: usize = 200;
const MAX_DESCRIPTION: usize = 2000;
const MAX_HSN: usize = 20;
const MAX_PACK_QTY: i32 = 10_000;

/// A rule broken by one input field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: &'static str,
    pub message: String,
}

impl FieldError {
    fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

impl From<FieldError> for AppError {
    fn from(e: FieldError) -> Self {
        AppError::FieldInvalid {
            field: e.field.to_string(),
            message: e.message,
        }
    }
}

/// Product fields as entered (strings for money), before validation.
#[derive(Debug, Clone, Default)]
pub struct ProductDraft {
    pub name: String,
    pub code: String,
    pub category_id: Option<Uuid>,
    pub description: Option<String>,
    pub unit: Option<String>,
    pub pack_qty: Option<i32>,
    pub attributes: Option<Value>,
    pub selling_price: Option<String>,
    pub mrp: Option<String>,
    pub default_cost: Option<String>,
    pub hsn_code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ValidProduct {
    pub name: String,
    pub raw_code: String,
    pub code: ProductCode,
    pub category_id: Option<Uuid>,
    pub description: Option<String>,
    pub unit: String,
    pub pack_qty: i32,
    pub attributes: Value,
    pub selling_price: Decimal,
    pub mrp: Option<Decimal>,
    pub default_cost: Option<Decimal>,
    pub hsn_code: Option<String>,
}

/// Trimmed text, or `None` when blank.
fn opt_text(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

fn opt_money(field: &'static str, s: &Option<String>) -> Result<Option<Decimal>, FieldError> {
    opt_text(s)
        .map(|t| parse_money(&t).map_err(|m| FieldError::new(field, m)))
        .transpose()
}

pub fn validate_product(d: &ProductDraft) -> Result<ValidProduct, FieldError> {
    let name = d.name.trim().to_string();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Err(FieldError::new(
            "name",
            format!("Enter a name of 1 to {MAX_NAME} characters."),
        ));
    }
    let raw_code = d.code.trim().to_string();
    let code = normalize_code(&raw_code).ok_or_else(|| {
        FieldError::new(
            "code",
            "Enter the model code printed on the label, for example IP 1060-11.",
        )
    })?;
    let selling_price = opt_money("selling_price", &d.selling_price)?
        .ok_or_else(|| FieldError::new("selling_price", "Enter the selling price."))?;
    let mrp = opt_money("mrp", &d.mrp)?;
    if let Some(m) = mrp
        && selling_price > m
    {
        return Err(FieldError::new(
            "selling_price",
            format!(
                "Selling price can't be more than the MRP ({}).",
                format_rupees(m)
            ),
        ));
    }
    let default_cost = opt_money("default_cost", &d.default_cost)?;
    let unit = opt_text(&d.unit)
        .unwrap_or_else(|| "piece".into())
        .to_lowercase();
    if !UNITS.contains(&unit.as_str()) {
        return Err(FieldError::new(
            "unit",
            format!("Unit must be one of: {}.", UNITS.join(", ")),
        ));
    }
    let pack_qty = d.pack_qty.unwrap_or(1);
    if !(1..=MAX_PACK_QTY).contains(&pack_qty) {
        return Err(FieldError::new(
            "pack_qty",
            format!("Pack quantity must be 1 to {MAX_PACK_QTY}."),
        ));
    }
    let attributes = d.attributes.clone().unwrap_or_else(|| json!({}));
    if !attributes.is_object() {
        return Err(FieldError::new(
            "attributes",
            "Attributes must be an object.",
        ));
    }
    let description = opt_text(&d.description);
    if description
        .as_ref()
        .is_some_and(|t| t.chars().count() > MAX_DESCRIPTION)
    {
        return Err(FieldError::new(
            "description",
            format!("Keep the description under {MAX_DESCRIPTION} characters."),
        ));
    }
    let hsn_code = opt_text(&d.hsn_code);
    if hsn_code
        .as_ref()
        .is_some_and(|t| t.chars().count() > MAX_HSN)
    {
        return Err(FieldError::new("hsn_code", "HSN code is too long."));
    }
    Ok(ValidProduct {
        name,
        raw_code,
        code,
        category_id: d.category_id,
        description,
        unit,
        pack_qty,
        attributes,
        selling_price,
        mrp,
        default_cost,
        hsn_code,
    })
}

/// Size as a number for sorting (`8` < `10`); `None` for non-numeric sizes.
pub fn variant_num(variant: Option<&str>) -> Option<Decimal> {
    let v = variant?;
    let ok = !v.is_empty()
        && v.chars().all(|c| c.is_ascii_digit() || c == '.')
        && v.matches('.').count() <= 1
        && !v.starts_with('.')
        && !v.ends_with('.');
    if ok { v.parse().ok() } else { None }
}

impl ValidProduct {
    /// Audited fields, for before/after diffs.
    pub fn snapshot(&self) -> Map<String, Value> {
        let money = |d: &Option<Decimal>| {
            d.map(|v| Value::String(v.to_string()))
                .unwrap_or(Value::Null)
        };
        let Value::Object(m) = json!({
            "name": self.name,
            "code": self.code.normalized,
            "category_id": self.category_id,
            "description": self.description,
            "unit": self.unit,
            "pack_qty": self.pack_qty,
            "attributes": self.attributes,
            "selling_price": self.selling_price.to_string(),
            "mrp": money(&self.mrp),
            "default_cost": money(&self.default_cost),
            "hsn_code": self.hsn_code,
        }) else {
            unreachable!()
        };
        m
    }
}

/// Fields that differ between two snapshots: `(before, after)` with only those keys.
pub fn diff(
    before: &Map<String, Value>,
    after: &Map<String, Value>,
) -> (Map<String, Value>, Map<String, Value>) {
    let mut b = Map::new();
    let mut a = Map::new();
    for (k, v) in after {
        let old = before.get(k).cloned().unwrap_or(Value::Null);
        if &old != v {
            b.insert(k.clone(), old);
            a.insert(k.clone(), v.clone());
        }
    }
    (b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> ProductDraft {
        ProductDraft {
            name: "Plate".into(),
            code: "IP 1060-11".into(),
            selling_price: Some("700".into()),
            mrp: Some("750".into()),
            ..Default::default()
        }
    }

    #[test]
    fn valid_draft_passes_with_defaults() {
        let p = validate_product(&draft()).unwrap();
        assert_eq!(p.unit, "piece");
        assert_eq!(p.pack_qty, 1);
        assert_eq!(p.code.normalized, "IP1060-11");
        assert_eq!(p.selling_price.to_string(), "700.00");
    }

    #[test]
    fn price_above_mrp_names_the_mrp() {
        let mut d = draft();
        d.selling_price = Some("1100".into());
        d.mrp = Some("1090".into());
        let e = validate_product(&d).unwrap_err();
        assert_eq!(e.field, "selling_price");
        assert_eq!(
            e.message,
            "Selling price can't be more than the MRP (₹1,090.00)."
        );
    }

    #[test]
    fn blank_optionals_are_none_and_bad_fields_are_named() {
        let mut d = draft();
        d.mrp = Some("  ".into());
        assert!(validate_product(&d).unwrap().mrp.is_none());
        for (f, mutate) in [
            (
                "name",
                Box::new(|d: &mut ProductDraft| d.name = " ".into())
                    as Box<dyn Fn(&mut ProductDraft)>,
            ),
            (
                "code",
                Box::new(|d: &mut ProductDraft| d.code = "Plate".into()),
            ),
            (
                "selling_price",
                Box::new(|d: &mut ProductDraft| d.selling_price = None),
            ),
            (
                "unit",
                Box::new(|d: &mut ProductDraft| d.unit = Some("crate".into())),
            ),
            (
                "pack_qty",
                Box::new(|d: &mut ProductDraft| d.pack_qty = Some(0)),
            ),
        ] {
            let mut d = draft();
            mutate(&mut d);
            assert_eq!(validate_product(&d).unwrap_err().field, f);
        }
    }

    #[test]
    fn variant_num_sorts_sizes() {
        assert_eq!(variant_num(Some("8")).unwrap().to_string(), "8");
        assert_eq!(variant_num(Some("7.8")).unwrap().to_string(), "7.8");
        assert!(variant_num(Some("H5")).is_none());
        assert!(variant_num(None).is_none());
    }
}
