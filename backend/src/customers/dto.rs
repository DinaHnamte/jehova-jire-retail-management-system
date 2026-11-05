//! Customer request and response shapes.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    catalog::dto::double,
    orders::read::{OrderSummary, PaymentRow},
};

#[derive(Debug, Serialize, Clone)]
pub struct Customer {
    pub id: Uuid,
    pub phone: String,
    pub name: Option<String>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct CustomerRow {
    #[serde(flatten)]
    pub customer: Customer,
    /// Amount owed, 2 decimal places.
    pub owes: String,
    pub last_sale_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct CustomerDetail {
    pub customer: Customer,
    pub owes: String,
    /// Latest 50 orders, newest first.
    pub orders: Vec<OrderSummary>,
    /// Latest 50 payments, newest first; each amount is the whole payment.
    pub payments: Vec<PaymentRow>,
    /// WhatsApp reminder link, present when the customer owes money.
    pub reminder_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCustomer {
    pub phone: String,
    pub name: Option<String>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateCustomer {
    pub phone: Option<String>,
    #[serde(default, deserialize_with = "double")]
    pub name: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub note: Option<Option<String>>,
}

#[derive(Deserialize)]
pub struct ListParams {
    pub q: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}
