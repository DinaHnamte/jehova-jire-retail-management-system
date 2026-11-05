//! CSV product import with dry run (plan Task 5).
mod common;
use common::{Session, TestApp, app, seed_user};
use serde_json::{Value, json};
use sqlx::PgPool;

const PW: &str = "correct horse battery";

async fn setup(pool: PgPool, role: &str) -> (TestApp, Session) {
    seed_user(&pool, "u", PW, &[role]).await;
    let a = app(pool);
    let s = a.login("u", PW).await;
    (a, s)
}

async fn import(a: &TestApp, s: &Session, csv: &str, dry_run: bool) -> common::TestResponse {
    a.post_raw(
        &format!("/api/v1/products/import?dry_run={dry_run}"),
        "text/csv",
        csv.to_string(),
        Some(&s.cookie),
        Some(&s.csrf),
    )
    .await
}

async fn total(a: &TestApp, s: &Session) -> i64 {
    a.get_with("/api/v1/products?status=all", Some(&s.cookie))
        .await
        .json()["total"]
        .as_i64()
        .unwrap()
}

fn statuses(report: &Value) -> Vec<String> {
    report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["status"].as_str().unwrap().to_string())
        .collect()
}

const THREE: &str = "code,name,selling_price,mrp\n\
IP 1060-11\",4 Section Plate,700,750\n\
B 6143-8,Storage Box,1000,1090\n\
IB 1290-7.8\",Easy Lunch,735,735\n";

#[sqlx::test]
async fn dry_run_reports_without_writing(pool: PgPool) {
    let (a, s) = setup(pool, "stock_operator").await;
    let r = import(&a, &s, THREE, true).await;
    assert_eq!(r.status(), 200, "{}", String::from_utf8_lossy(&r.body));
    let rep = r.json();
    assert_eq!(rep["created"], 3);
    assert_eq!(statuses(&rep), ["create", "create", "create"]);
    assert_eq!(rep["rows"][0]["row"], 2);
    assert_eq!(rep["rows"][0]["code"], "IP1060-11");
    assert_eq!(total(&a, &s).await, 0);
}

#[sqlx::test]
async fn import_creates_products_and_categories(pool: PgPool) {
    let (a, s) = setup(pool, "stock_operator").await;
    let csv = "code,name,selling_price,category,aliases\n\
IP 1060-11,4 Section Plate,700,Plates,4 sec plate|thali\n\
B 6143-8,Storage Box,1000,Boxes,\n";
    let r = import(&a, &s, csv, false).await;
    assert_eq!(r.status(), 200, "{}", String::from_utf8_lossy(&r.body));
    let rep = r.json();
    assert_eq!(rep["created"], 2);
    assert_eq!(rep["new_categories"], json!(["Boxes", "Plates"]));
    let hits = a
        .get_with("/api/v1/products/search?q=6143", Some(&s.cookie))
        .await
        .json();
    assert_eq!(hits["results"][0]["category"]["name"], "Boxes");
    let hits = a
        .get_with("/api/v1/products/search?q=thali", Some(&s.cookie))
        .await
        .json();
    assert_eq!(hits["results"][0]["code"], "IP1060-11");
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = 'product.import'")
            .fetch_one(&a.pool)
            .await
            .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test]
async fn existing_and_repeated_codes_are_skipped(pool: PgPool) {
    let (a, s) = setup(pool, "stock_operator").await;
    assert_eq!(
        import(
            &a,
            &s,
            "code,name,selling_price\nB 6143-8,Storage Box,1000\n",
            false
        )
        .await
        .status(),
        200
    );
    let csv = "code,name,selling_price\n\
b6143-8,Storage Box again,1000\n\
B 6275-4.75,Bowl,100\n\
B6275-4.75,Bowl twice,100\n";
    let r = import(&a, &s, csv, false).await;
    assert_eq!(r.status(), 200, "{}", String::from_utf8_lossy(&r.body));
    let rep = r.json();
    assert_eq!(statuses(&rep), ["skip", "create", "skip"]);
    assert_eq!(rep["skipped"], 2);
    assert_eq!(rep["created"], 1);
    assert_eq!(total(&a, &s).await, 2);
}

#[sqlx::test]
async fn any_error_blocks_the_whole_import(pool: PgPool) {
    let (a, s) = setup(pool, "stock_operator").await;
    let csv = "code,name,selling_price,mrp\n\
B 6143-8,Storage Box,1000,1090\n\
B 6275-4.75,Bowl,800,750\n";
    let r = import(&a, &s, csv, false).await;
    assert_eq!(r.status(), 422);
    let rep = r.json();
    let rep = if rep.get("rows").is_some() {
        rep
    } else {
        rep["error"]["details"].clone()
    };
    assert_eq!(statuses(&rep), ["create", "error"]);
    assert_eq!(rep["rows"][1]["row"], 3);
    assert_eq!(rep["rows"][1]["errors"][0]["field"], "selling_price");
    assert_eq!(total(&a, &s).await, 0);
}

#[sqlx::test]
async fn unknown_brand_is_a_row_error(pool: PgPool) {
    let (a, s) = setup(pool, "stock_operator").await;
    let csv = "code,name,selling_price,brand\nB 1-1,Thing,1,Nobody Brand\nB 2-1,Other,1,ektra by superware\n";
    let rep = import(&a, &s, csv, true).await.json();
    assert_eq!(statuses(&rep), ["error", "create"]);
    assert_eq!(rep["rows"][0]["errors"][0]["field"], "brand");
}

#[sqlx::test]
async fn header_is_flexible_and_bom_is_ignored(pool: PgPool) {
    let (a, s) = setup(pool, "stock_operator").await;
    let csv = "\u{feff}Name,CODE,Selling_Price\nStorage Box,B 6143-8,1000\n";
    let r = import(&a, &s, csv, false).await;
    assert_eq!(r.status(), 200, "{}", String::from_utf8_lossy(&r.body));
    assert_eq!(r.json()["created"], 1);
}

#[sqlx::test]
async fn missing_required_column_is_422(pool: PgPool) {
    let (a, s) = setup(pool, "stock_operator").await;
    let r = import(&a, &s, "name,selling_price\nBox,10\n", true).await;
    assert_eq!(r.status(), 422);
    assert!(
        r.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("code")
    );
    let r = import(
        &a,
        &s,
        "code,name,selling_price,colour\nB 1-1,Box,10,red\n",
        true,
    )
    .await;
    assert_eq!(r.status(), 422);
    assert!(
        r.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("colour")
    );
}

#[sqlx::test]
async fn too_many_rows_is_422(pool: PgPool) {
    let (a, s) = setup(pool, "stock_operator").await;
    let mut csv = String::from("code,name,selling_price\n");
    for i in 0..2001 {
        csv.push_str(&format!("T {i}-1,Item,1\n"));
    }
    assert_eq!(import(&a, &s, &csv, true).await.status(), 422);
}

#[sqlx::test]
async fn cashier_cannot_import(pool: PgPool) {
    let (a, s) = setup(pool, "cashier").await;
    assert_eq!(import(&a, &s, THREE, true).await.status(), 403);
}

#[sqlx::test]
async fn row_numbers_match_spreadsheet_lines(pool: PgPool) {
    let (a, s) = setup(pool, "stock_operator").await;
    // line 3 is blank; line 4's description spans two lines; the bad row is on line 6.
    let csv = "code,name,selling_price,description\n\
B 1-1,One,1,\n\
\n\
B 2-1,Two,1,\"first line\nsecond line\"\n\
B 3-1,Three,1,\n\
B 4-1,Four,abc,\n";
    let rep = import(&a, &s, csv, true).await.json();
    let rows: Vec<i64> = rep["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["row"].as_i64().unwrap())
        .collect();
    assert_eq!(rows, [2, 4, 6, 7]);
    assert_eq!(rep["rows"][3]["status"], "error");
}

#[sqlx::test]
async fn skipped_rows_say_why(pool: PgPool) {
    let (a, s) = setup(pool, "stock_operator").await;
    assert_eq!(
        import(
            &a,
            &s,
            "code,name,selling_price\nB 6143-8,Storage Box,1000\n",
            false
        )
        .await
        .status(),
        200
    );
    let csv =
        "code,name,selling_price\nB 6143-8,Storage Box,1000\nB 7-1,Plate,5\nB7-1,Lunch Box,5\n";
    let rep = import(&a, &s, csv, true).await.json();
    assert_eq!(statuses(&rep), ["skip", "create", "skip"]);
    assert_eq!(rep["rows"][0]["note"], "Already in the catalogue.");
    assert_eq!(rep["rows"][2]["note"], "Same code as row 3.");
    assert_eq!(rep["rows"][1]["note"], Value::Null);
}

#[sqlx::test]
async fn import_reads_design_codes(pool: PgPool) {
    let (a, s) = setup(pool, "stock_operator").await;
    let csv = "code,design_code,name,selling_price\n\
IP 1060-11,MP4952,4 Section Plate,700\n\
IP 1060-11,MG4705,4 Section Plate,700\n\
IP 1060-11,mp 4952,4 Section Plate again,700\n\
IP 1060-11,MP#49,Bad design,700\n";
    let rep = import(&a, &s, csv, true).await.json();
    assert_eq!(statuses(&rep), ["create", "create", "skip", "error"]);
    assert_eq!(rep["rows"][2]["note"], "Same code as row 2.");
    assert_eq!(rep["rows"][3]["errors"][0]["field"], "design_code");
    assert_eq!(rep["rows"][0]["design_code"], "MP4952");
}
