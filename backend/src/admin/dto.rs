use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct SessionUser {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub permissions: Vec<String>,
}

#[derive(Serialize)]
pub struct SessionResponse {
    pub user: SessionUser,
    pub csrf_token: String,
}

#[derive(Serialize)]
pub struct UserListItem {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub is_active: bool,
    pub roles: Vec<String>,
}
