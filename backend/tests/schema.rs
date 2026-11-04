//! Role/permission seed and admin schema invariants.
use sqlx::PgPool;

async fn role_perms(pool: &PgPool, role: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT rp.permission_code FROM role_permissions rp JOIN roles r ON r.id = rp.role_id
         WHERE r.code = $1 ORDER BY 1",
    )
    .bind(role)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test]
async fn admin_role_has_every_permission(pool: PgPool) {
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM permissions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 36);
    assert_eq!(role_perms(&pool, "admin").await.len(), 36);
}

#[sqlx::test]
async fn six_roles_are_seeded(pool: PgPool) {
    let roles: Vec<String> = sqlx::query_scalar("SELECT code FROM roles ORDER BY code")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        roles,
        [
            "admin",
            "auditor",
            "cashier",
            "delivery_driver",
            "manager",
            "stock_operator"
        ]
    );
}

#[sqlx::test]
async fn driver_role_has_only_own_scoped_permissions(pool: PgPool) {
    assert_eq!(
        role_perms(&pool, "delivery_driver").await,
        [
            "deliveries.collect_own",
            "deliveries.read_own",
            "deliveries.update_own",
            "settlements.read_own"
        ]
    );
}

#[sqlx::test]
async fn cashier_cannot_manage_users_or_adjust_stock(pool: PgPool) {
    let p = role_perms(&pool, "cashier").await;
    for forbidden in [
        "users.manage",
        "inventory.adjust",
        "orders.void",
        "settings.manage",
    ] {
        assert!(
            !p.contains(&forbidden.to_string()),
            "cashier has {forbidden}"
        );
    }
    assert!(p.contains(&"orders.create".to_string()));
}

#[sqlx::test]
async fn manager_lacks_admin_and_driver_permissions(pool: PgPool) {
    let p = role_perms(&pool, "manager").await;
    assert!(
        !p.iter()
            .any(|x| x == "users.manage" || x == "settings.manage" || x.ends_with("_own"))
    );
    assert!(p.contains(&"inventory.adjust".to_string()));
}

#[sqlx::test]
async fn auditor_is_read_only(pool: PgPool) {
    let p = role_perms(&pool, "auditor").await;
    assert!(
        p.iter().all(|x| x.ends_with(".read") || x == "audit.read"),
        "{p:?}"
    );
    assert!(p.contains(&"audit.read".to_string()));
}

#[sqlx::test]
async fn usernames_are_case_insensitive_unique(pool: PgPool) {
    let ins = "INSERT INTO users (id, username, display_name, password_hash) VALUES (gen_random_uuid(), $1, 'x', 'h')";
    sqlx::query(ins).bind("Dinah").execute(&pool).await.unwrap();
    let err = sqlx::query(ins)
        .bind("dinah")
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        err.as_database_error().unwrap().code().as_deref(),
        Some("23505")
    );
}

#[sqlx::test]
async fn users_with_sessions_cannot_be_deleted(pool: PgPool) {
    let uid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (id, username, display_name, password_hash) VALUES (gen_random_uuid(), 'u', 'u', 'h') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sessions (id, token_hash, csrf_token, user_id, expires_at) VALUES (gen_random_uuid(), '\\x00', 'c', $1, now())",
    )
    .bind(uid)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(uid)
            .execute(&pool)
            .await
            .is_err()
    );
}
