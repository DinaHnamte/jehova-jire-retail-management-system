use jj_retail_api::{admin::repo, error::AppError};
use sqlx::PgPool;

#[sqlx::test]
async fn create_user_assigns_roles_and_permissions(pool: PgPool) {
    let mut c = pool.acquire().await.unwrap();
    let id = repo::create_user(&mut c, "Cashier1", "Cashier One", "h", &["cashier"])
        .await
        .unwrap();
    assert_eq!(
        repo::user_role_codes(&mut c, id).await.unwrap(),
        ["cashier"]
    );
    let perms = repo::user_permissions(&mut c, id).await.unwrap();
    assert!(perms.contains(&"orders.create".to_string()));
    assert!(!perms.contains(&"users.manage".to_string()));
}

#[sqlx::test]
async fn create_user_rejects_duplicate_username_case_insensitive(pool: PgPool) {
    let mut c = pool.acquire().await.unwrap();
    repo::create_user(&mut c, "dinah", "D", "h", &["admin"])
        .await
        .unwrap();
    let err = repo::create_user(&mut c, "DINAH", "D", "h", &["admin"])
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "{err:?}");
}

#[sqlx::test]
async fn create_user_rejects_unknown_role(pool: PgPool) {
    let mut c = pool.acquire().await.unwrap();
    let err = repo::create_user(&mut c, "x", "X", "h", &["superuser"])
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)), "{err:?}");
}

#[sqlx::test]
async fn find_user_for_login_is_case_insensitive(pool: PgPool) {
    let mut c = pool.acquire().await.unwrap();
    let id = repo::create_user(&mut c, "Dinah", "D", "hash", &["admin"])
        .await
        .unwrap();
    let u = repo::find_user_for_login(&mut c, "dINAH")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(u.id, id);
    assert!(u.is_active);
    assert!(
        repo::find_user_for_login(&mut c, "nobody")
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test]
async fn create_admin_rejects_short_password(pool: PgPool) {
    let err = jj_retail_api::admin::create_admin(&pool, "boss", "Boss", "short-pass")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)), "{err:?}");
}

#[sqlx::test]
async fn create_admin_creates_verifiable_admin(pool: PgPool) {
    let id = jj_retail_api::admin::create_admin(&pool, "boss", "Boss", "a-long-enough-pass")
        .await
        .unwrap();
    let mut c = pool.acquire().await.unwrap();
    assert_eq!(repo::user_role_codes(&mut c, id).await.unwrap(), ["admin"]);
    let u = repo::find_user_for_login(&mut c, "boss")
        .await
        .unwrap()
        .unwrap();
    assert!(jj_retail_api::admin::password::verify_password(
        "a-long-enough-pass",
        &u.password_hash
    ));
}
