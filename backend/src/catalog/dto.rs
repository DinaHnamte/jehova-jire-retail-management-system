use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Clone)]
pub struct NamedRef {
    pub id: Uuid,
    pub name: String,
}

#[derive(Deserialize)]
pub struct NameRequest {
    pub name: String,
}

use chrono::{DateTime, Utc};
use serde::Deserializer;
use serde_json::Value;

/// Distinguishes "field absent" (`None`) from "field set to null" (`Some(None)`).
fn double<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProduct {
    pub name: String,
    pub code: String,
    pub brand_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub description: Option<String>,
    pub unit: Option<String>,
    pub pack_qty: Option<i32>,
    pub attributes: Option<Value>,
    pub selling_price: String,
    pub mrp: Option<String>,
    pub default_cost: Option<String>,
    pub hsn_code: Option<String>,
    pub design_code: Option<String>,
}

/// PATCH body: absent fields are unchanged; `null` clears an optional field.
/// The brand can't be changed after creation.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct UpdateProduct {
    pub name: Option<String>,
    pub code: Option<String>,
    #[serde(default, deserialize_with = "double")]
    pub category_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double")]
    pub description: Option<Option<String>>,
    pub unit: Option<String>,
    pub pack_qty: Option<i32>,
    pub attributes: Option<Value>,
    pub selling_price: Option<String>,
    #[serde(default, deserialize_with = "double")]
    pub mrp: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub default_cost: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub hsn_code: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub design_code: Option<Option<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddIdentifier {
    #[serde(rename = "type")]
    pub identifier_type: String,
    pub value: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct IdentifierOut {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub identifier_type: String,
    pub raw_value: String,
    pub normalized_value: String,
}

#[derive(Debug, Serialize)]
pub struct ProductOut {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub brand: NamedRef,
    pub category: Option<NamedRef>,
    pub code: Option<String>,
    pub base_code: Option<String>,
    pub variant: Option<String>,
    pub design_code: Option<String>,
    pub unit: String,
    pub pack_qty: i32,
    pub attributes: Value,
    pub selling_price: rust_decimal::Decimal,
    pub mrp: Option<rust_decimal::Decimal>,
    /// Only for callers who can edit products; `null` otherwise.
    pub default_cost: Option<rust_decimal::Decimal>,
    pub hsn_code: Option<String>,
    pub archived: bool,
    pub identifiers: Vec<IdentifierOut>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
