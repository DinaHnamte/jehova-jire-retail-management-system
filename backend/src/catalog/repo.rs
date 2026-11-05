//! SQL for the catalogue. Every function takes the caller's connection/transaction.
use sqlx::PgConnection;
use uuid::Uuid;

use super::dto::NamedRef;
use crate::error::AppResult;

pub async fn list_brands(conn: &mut PgConnection) -> AppResult<Vec<NamedRef>> {
    Ok(
        sqlx::query_as!(NamedRef, "SELECT id, name FROM brands ORDER BY lower(name)")
            .fetch_all(conn)
            .await?,
    )
}

pub async fn insert_brand(
    conn: &mut PgConnection,
    id: Uuid,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!("INSERT INTO brands (id, name) VALUES ($1, $2)", id, name)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn list_categories(conn: &mut PgConnection) -> AppResult<Vec<NamedRef>> {
    Ok(sqlx::query_as!(
        NamedRef,
        "SELECT id, name FROM categories ORDER BY lower(name)"
    )
    .fetch_all(conn)
    .await?)
}

pub async fn insert_category(
    conn: &mut PgConnection,
    id: Uuid,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO categories (id, name) VALUES ($1, $2)",
        id,
        name
    )
    .execute(conn)
    .await?;
    Ok(())
}
