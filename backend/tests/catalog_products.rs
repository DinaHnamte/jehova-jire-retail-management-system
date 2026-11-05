//! Catalogue API: brands, categories, products and identifiers (Architecture.md §6, §15).
mod common;
use common::{app, seed_user};
use serde_json::json;
use sqlx::PgPool;

const PW: &str = "correct horse battery";

#[sqlx::test]
async fn brands_list_includes_seeded_ektra(pool: PgPool) {
    seed_user(&pool, "cash", PW, &["cashier"]).await;
    let a = app(pool);
    let s = a.login("cash", PW).await;
    let r = a.get_with("/api/v1/brands", Some(&s.cookie)).await;
    assert_eq!(r.status(), 200);
    let names: Vec<String> = r
        .json()
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.contains(&"Ektra by Superware".to_string()),
        "{names:?}"
    );
}

#[sqlx::test]
async fn cashier_cannot_create_category(pool: PgPool) {
    seed_user(&pool, "cash", PW, &["cashier"]).await;
    let a = app(pool);
    let s = a.login("cash", PW).await;
    let r = a
        .post_json(
            "/api/v1/categories",
            json!({"name": "Plates"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 403);
    assert_eq!(r.json()["error"]["code"], "FORBIDDEN");
}

#[sqlx::test]
async fn stock_operator_creates_category_and_duplicate_conflicts(pool: PgPool) {
    seed_user(&pool, "stock", PW, &["stock_operator"]).await;
    let a = app(pool);
    let s = a.login("stock", PW).await;
    let r = a
        .post_json(
            "/api/v1/categories",
            json!({"name": "Plates"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 201);
    assert_eq!(r.json()["name"], "Plates");
    let r = a
        .post_json(
            "/api/v1/categories",
            json!({"name": "plates"}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 409);
    let r = a.get_with("/api/v1/categories", Some(&s.cookie)).await;
    assert_eq!(r.json().as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn category_name_must_not_be_blank(pool: PgPool) {
    seed_user(&pool, "stock", PW, &["stock_operator"]).await;
    let a = app(pool);
    let s = a.login("stock", PW).await;
    let r = a
        .post_json(
            "/api/v1/categories",
            json!({"name": "  "}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 422);
    let long = "x".repeat(101);
    let r = a
        .post_json(
            "/api/v1/categories",
            json!({"name": long}),
            Some(&s.cookie),
            Some(&s.csrf),
        )
        .await;
    assert_eq!(r.status(), 422);
}

// ---------- products ----------

struct Ctx {
    a: common::TestApp,
    stock: common::Session,
    cash: common::Session,
}

async fn ctx(pool: PgPool) -> Ctx {
    seed_user(&pool, "stock", PW, &["stock_operator"]).await;
    seed_user(&pool, "cash", PW, &["cashier"]).await;
    let a = app(pool);
    let stock = a.login("stock", PW).await;
    let cash = a.login("cash", PW).await;
    Ctx { a, stock, cash }
}

fn plate() -> serde_json::Value {
    json!({"name": "4 Section Plate", "code": "IP 1060-11\"", "selling_price": "700", "mrp": "750"})
}

impl Ctx {
    async fn create(&self, body: serde_json::Value) -> common::TestResponse {
        self.a
            .post_json(
                "/api/v1/products",
                body,
                Some(&self.stock.cookie),
                Some(&self.stock.csrf),
            )
            .await
    }
}

#[sqlx::test]
async fn create_product_returns_normalised_code(pool: PgPool) {
    let c = ctx(pool).await;
    let r = c.create(plate()).await;
    assert_eq!(r.status(), 201, "{}", String::from_utf8_lossy(&r.body));
    let p = r.json();
    assert_eq!(p["code"], "IP1060-11");
    assert_eq!(p["base_code"], "IP1060");
    assert_eq!(p["variant"], "11");
    assert_eq!(p["selling_price"], "700.00");
    assert_eq!(p["mrp"], "750.00");
    assert_eq!(p["brand"]["name"], "Ektra by Superware");
    assert_eq!(p["archived"], false);
    assert_eq!(p["unit"], "piece");
}

#[sqlx::test]
async fn duplicate_code_in_other_spelling_conflicts(pool: PgPool) {
    let c = ctx(pool).await;
    assert_eq!(c.create(plate()).await.status(), 201);
    let r = c
        .create(
            json!({"name": "Another", "code": "ip 1060 \u{2013} 11\u{2033}", "selling_price": "1"}),
        )
        .await;
    assert_eq!(r.status(), 409);
    let e = &r.json()["error"];
    assert_eq!(e["code"], "DUPLICATE_CODE");
    assert_eq!(e["details"]["name"], "4 Section Plate");
    assert_eq!(e["details"]["archived"], false);
}

#[sqlx::test]
async fn concurrent_create_same_code_one_wins(pool: PgPool) {
    let c = ctx(pool).await;
    let (r1, r2) = tokio::join!(c.create(plate()), c.create(plate()));
    let mut s = [r1.status(), r2.status()];
    s.sort();
    assert_eq!(s, [201, 409]);
}

#[sqlx::test]
async fn selling_price_above_mrp_is_rejected(pool: PgPool) {
    let c = ctx(pool).await;
    let r = c
        .create(
            json!({"name": "Plate", "code": "IP 1060-11", "selling_price": "751", "mrp": "750"}),
        )
        .await;
    assert_eq!(r.status(), 422);
    let e = &r.json()["error"];
    assert!(e["message"].as_str().unwrap().contains("₹750.00"), "{e}");
    assert_eq!(e["details"]["field"], "selling_price");
    let r = c
        .create(
            json!({"name": "Plate", "code": "IP 1060-11", "selling_price": "750", "mrp": "750"}),
        )
        .await;
    assert_eq!(r.status(), 201);
}

#[sqlx::test]
async fn money_must_be_a_string_amount(pool: PgPool) {
    let c = ctx(pool).await;
    let r = c
        .create(json!({"name": "Plate", "code": "B 1-1", "selling_price": "1.234"}))
        .await;
    assert_eq!(r.status(), 422);
    let r = c
        .create(json!({"name": "Plate", "code": "B 1-1", "selling_price": -5}))
        .await;
    assert!([400, 422].contains(&r.status()), "got {}", r.status());
}

#[sqlx::test]
async fn cashier_cannot_create_or_see_cost(pool: PgPool) {
    let c = ctx(pool).await;
    let r =
        c.a.post_json(
            "/api/v1/products",
            plate(),
            Some(&c.cash.cookie),
            Some(&c.cash.csrf),
        )
        .await;
    assert_eq!(r.status(), 403);
    let mut body = plate();
    body["default_cost"] = json!("400");
    let id = c.create(body).await.json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let path = format!("/api/v1/products/{id}");
    let as_cash = c.a.get_with(&path, Some(&c.cash.cookie)).await.json();
    assert_eq!(as_cash["default_cost"], serde_json::Value::Null);
    assert_eq!(as_cash["name"], "4 Section Plate");
    let as_stock = c.a.get_with(&path, Some(&c.stock.cookie)).await.json();
    assert_eq!(as_stock["default_cost"], "400.00");
}

#[sqlx::test]
async fn patch_updates_price_and_audits(pool: PgPool) {
    let c = ctx(pool).await;
    let id = c.create(plate()).await.json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let path = format!("/api/v1/products/{id}");
    let r =
        c.a.patch_json(
            &path,
            json!({"selling_price": "720"}),
            Some(&c.stock.cookie),
            Some(&c.stock.csrf),
        )
        .await;
    assert_eq!(r.status(), 200, "{}", String::from_utf8_lossy(&r.body));
    assert_eq!(r.json()["selling_price"], "720.00");
    assert_eq!(r.json()["code"], "IP1060-11");
    let (before, after): (serde_json::Value, serde_json::Value) = sqlx::query_as(
        "SELECT before, after FROM audit_logs WHERE action = 'product.update' AND entity_id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&c.a.pool)
    .await
    .unwrap();
    assert_eq!(before["selling_price"], "700.00");
    assert_eq!(after["selling_price"], "720.00");
    assert!(
        after.get("name").is_none(),
        "only changed fields are audited: {after}"
    );
}

#[sqlx::test]
async fn patch_can_change_code_and_clear_mrp(pool: PgPool) {
    let c = ctx(pool).await;
    let id = c.create(plate()).await.json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let path = format!("/api/v1/products/{id}");
    let r =
        c.a.patch_json(
            &path,
            json!({"code": "IP 1060-12", "mrp": null}),
            Some(&c.stock.cookie),
            Some(&c.stock.csrf),
        )
        .await;
    assert_eq!(r.status(), 200, "{}", String::from_utf8_lossy(&r.body));
    assert_eq!(r.json()["code"], "IP1060-12");
    assert_eq!(r.json()["mrp"], serde_json::Value::Null);
}

#[sqlx::test]
async fn archive_and_restore(pool: PgPool) {
    let c = ctx(pool).await;
    let id = c.create(plate()).await.json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let post = |p: String, s: &common::Session| {
        let (ck, cs) = (s.cookie.clone(), s.csrf.clone());
        let a = &c.a;
        async move { a.post_json(&p, json!({}), Some(&ck), Some(&cs)).await }
    };
    // stock operators can't archive; seed a manager.
    assert_eq!(
        post(format!("/api/v1/products/{id}/archive"), &c.stock)
            .await
            .status(),
        403
    );
    seed_user(&c.a.pool, "boss", PW, &["manager"]).await;
    let m = c.a.login("boss", PW).await;
    let r = post(format!("/api/v1/products/{id}/archive"), &m).await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.json()["archived"], true);
    assert_eq!(
        post(format!("/api/v1/products/{id}/archive"), &m)
            .await
            .status(),
        200
    );
    let r = post(format!("/api/v1/products/{id}/restore"), &m).await;
    assert_eq!(r.json()["archived"], false);
    assert_eq!(
        post(format!("/api/v1/products/{id}/archive"), &c.cash)
            .await
            .status(),
        403
    );
}

#[sqlx::test]
async fn aliases_can_be_added_and_removed(pool: PgPool) {
    let c = ctx(pool).await;
    let id = c.create(plate()).await.json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let base = format!("/api/v1/products/{id}/identifiers");
    let r =
        c.a.post_json(
            &base,
            json!({"type": "alias", "value": "4 sec plate"}),
            Some(&c.stock.cookie),
            Some(&c.stock.csrf),
        )
        .await;
    assert_eq!(r.status(), 201, "{}", String::from_utf8_lossy(&r.body));
    let iid = r.json()["id"].as_str().unwrap().to_string();
    let p =
        c.a.get_with(&format!("/api/v1/products/{id}"), Some(&c.stock.cookie))
            .await
            .json();
    assert!(
        p["identifiers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["raw_value"] == "4 sec plate")
    );
    let r =
        c.a.delete_with(
            &format!("{base}/{iid}"),
            Some(&c.stock.cookie),
            Some(&c.stock.csrf),
        )
        .await;
    assert_eq!(r.status(), 204);
    let r =
        c.a.post_json(
            &base,
            json!({"type": "product_code", "value": "X 1-1"}),
            Some(&c.stock.cookie),
            Some(&c.stock.csrf),
        )
        .await;
    assert_eq!(r.status(), 422);
    // the product's own code can't be deleted through this endpoint
    let p =
        c.a.get_with(&format!("/api/v1/products/{id}"), Some(&c.stock.cookie))
            .await
            .json();
    let code_id = p["identifiers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "product_code")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r =
        c.a.delete_with(
            &format!("{base}/{code_id}"),
            Some(&c.stock.cookie),
            Some(&c.stock.csrf),
        )
        .await;
    assert_eq!(r.status(), 422);
}

#[sqlx::test]
async fn unknown_product_is_404(pool: PgPool) {
    let c = ctx(pool).await;
    let r =
        c.a.get_with(
            "/api/v1/products/0190f3c2-0000-7000-8000-000000000000",
            Some(&c.cash.cookie),
        )
        .await;
    assert_eq!(r.status(), 404);
    assert_eq!(r.json()["error"]["code"], "NOT_FOUND");
}
