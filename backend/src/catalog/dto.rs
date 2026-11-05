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
