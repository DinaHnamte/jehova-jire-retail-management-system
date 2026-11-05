//! Catalogue use cases. Each opens and owns its transaction.
use sqlx::PgPool;

use super::{dto::NamedRef, is_unique_violation, repo};
use crate::{
    error::{AppError, AppResult},
    shared::ids::new_id,
};

const MAX_NAME: usize = 100;

fn clean_name(name: &str) -> AppResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > MAX_NAME {
        return Err(AppError::Validation(format!(
            "Enter a name of 1 to {MAX_NAME} characters."
        )));
    }
    Ok(n.to_string())
}

pub async fn list_brands(db: &PgPool) -> AppResult<Vec<NamedRef>> {
    repo::list_brands(&mut *db.acquire().await?).await
}

pub async fn create_brand(db: &PgPool, name: &str) -> AppResult<NamedRef> {
    let name = clean_name(name)?;
    let id = new_id();
    repo::insert_brand(&mut *db.acquire().await?, id, &name)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                AppError::Conflict(format!("A brand called {name} already exists."))
            } else {
                e.into()
            }
        })?;
    Ok(NamedRef { id, name })
}

pub async fn list_categories(db: &PgPool) -> AppResult<Vec<NamedRef>> {
    repo::list_categories(&mut *db.acquire().await?).await
}

pub async fn create_category(db: &PgPool, name: &str) -> AppResult<NamedRef> {
    let name = clean_name(name)?;
    let id = new_id();
    repo::insert_category(&mut *db.acquire().await?, id, &name)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                AppError::Conflict(format!("A category called {name} already exists."))
            } else {
                e.into()
            }
        })?;
    Ok(NamedRef { id, name })
}
