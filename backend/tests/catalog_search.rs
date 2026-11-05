//! Product list paging and ranked code search (Architecture.md §6.4).
mod common;
use common::{Session, TestApp, app, seed_user};
use serde_json::{Value, json};
use sqlx::PgPool;

const PW: &str = "correct horse battery";

struct Ctx {
    a: TestApp,
    s: Session,
    boss: Session,
}

async fn ctx(pool: PgPool) -> Ctx {
    seed_user(&pool, "boss", PW, &["manager"]).await;
    let a = app(pool);
    let s = a.login("boss", PW).await;
    let boss = a.login("boss", PW).await;
    Ctx { a, s, boss }
}

impl Ctx {
    async fn add(&self, code: &str, name: &str, category: Option<&str>) -> String {
        let mut body = json!({"name": name, "code": code, "selling_price": "100"});
        if let Some(c) = category {
            body["category_id"] = json!(c);
        }
        let r = self
            .a
            .post_json(
                "/api/v1/products",
                body,
                Some(&self.s.cookie),
                Some(&self.s.csrf),
            )
            .await;
        assert_eq!(r.status(), 201, "{}", String::from_utf8_lossy(&r.body));
        r.json()["id"].as_str().unwrap().to_string()
    }

    async fn seed(&self) {
        for (code, name) in [
            ("IP 1060-11\"", "4 Section Plate"),
            ("IP 1060-9", "4 Section Plate 9in"),
            ("B 6143-8", "Storage Box"),
            ("B 6143-10", "Storage Box"),
            ("B 6275-4.75", "Round Bowl"),
            ("IB 1290-7.8\"", "Easy Lunch"),
        ] {
            self.add(code, name, None).await;
        }
    }

    async fn get(&self, path: &str) -> Value {
        let r = self.a.get_with(path, Some(&self.s.cookie)).await;
        assert_eq!(
            r.status(),
            200,
            "{path}: {}",
            String::from_utf8_lossy(&r.body)
        );
        r.json()
    }

    async fn search(&self, q: &str) -> Vec<(String, String)> {
        let q = urlencode(q);
        self.get(&format!("/api/v1/products/search?q={q}")).await["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["code"].as_str().unwrap().to_string(),
                    r["match"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn codes(v: &[(String, String)]) -> Vec<&str> {
    v.iter().map(|(c, _)| c.as_str()).collect()
}

#[sqlx::test]
async fn exact_code_comes_first(pool: PgPool) {
    let c = ctx(pool).await;
    c.seed().await;
    let r = c.search("ip 1060-11").await;
    assert_eq!(r[0], ("IP1060-11".into(), "exact".into()));
    assert!(
        !codes(&r)
            .iter()
            .any(|c| c.starts_with("B6143") || c.starts_with("IB1290")),
        "{r:?}"
    );
}

#[sqlx::test]
async fn base_code_returns_all_sizes_in_size_order(pool: PgPool) {
    let c = ctx(pool).await;
    c.seed().await;
    let r = c.search("B6143").await;
    assert_eq!(codes(&r), ["B6143-8", "B6143-10"]);
    assert!(r.iter().all(|(_, m)| m == "base"));
}

#[sqlx::test]
async fn digit_prefix_finds_model(pool: PgPool) {
    let c = ctx(pool).await;
    c.seed().await;
    let r = c.search("614").await;
    assert_eq!(codes(&r), ["B6143-8", "B6143-10"]);
    assert!(r.iter().all(|(_, m)| m == "prefix"));
    assert_eq!(codes(&c.search("1290").await), ["IB1290-7.8"]);
    // both sizes of the plate, smallest first
    assert_eq!(codes(&c.search("1060").await), ["IP1060-9", "IP1060-11"]);
}

#[sqlx::test]
async fn name_search_finds_by_words(pool: PgPool) {
    let c = ctx(pool).await;
    c.seed().await;
    assert_eq!(
        c.search("lunch").await,
        [("IB1290-7.8".to_string(), "name".to_string())]
    );
    assert_eq!(codes(&c.search("storage").await), ["B6143-8", "B6143-10"]);
}

#[sqlx::test]
async fn alias_is_searchable(pool: PgPool) {
    let c = ctx(pool).await;
    let id = c.add("B 6275-4.75", "Round Bowl", None).await;
    let r =
        c.a.post_json(
            &format!("/api/v1/products/{id}/identifiers"),
            json!({"type": "alias", "value": "katori"}),
            Some(&c.s.cookie),
            Some(&c.s.csrf),
        )
        .await;
    assert_eq!(r.status(), 201);
    assert_eq!(codes(&c.search("katori").await), ["B6275-4.75"]);
}

#[sqlx::test]
async fn search_treats_wildcards_literally(pool: PgPool) {
    let c = ctx(pool).await;
    c.seed().await;
    for q in ["%", "_", "\\", "'", "100%", "   "] {
        let r = c.search(q).await;
        if q != "100%" {
            assert!(r.is_empty(), "{q:?} -> {r:?}");
        }
    }
    assert!(
        c.get("/api/v1/products/search").await["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test]
async fn archived_products_hidden_but_code_reserved(pool: PgPool) {
    let c = ctx(pool).await;
    c.seed().await;
    let id = c.add("B 9999-1", "Old Jug", None).await;
    let r =
        c.a.post_json(
            &format!("/api/v1/products/{id}/archive"),
            json!({}),
            Some(&c.boss.cookie),
            Some(&c.boss.csrf),
        )
        .await;
    assert_eq!(r.status(), 200);
    assert!(c.search("9999").await.is_empty());
    let all = c.get("/api/v1/products").await;
    assert!(
        !all["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["code"] == "B9999-1")
    );
    let archived = c.get("/api/v1/products?status=archived").await;
    assert_eq!(archived["total"], 1);
    assert_eq!(archived["items"][0]["code"], "B9999-1");
    assert_eq!(c.get("/api/v1/products?status=all").await["total"], 7);
    let r =
        c.a.post_json(
            "/api/v1/products",
            json!({"name": "New Jug", "code": "B 9999-1", "selling_price": "1"}),
            Some(&c.s.cookie),
            Some(&c.s.csrf),
        )
        .await;
    assert_eq!(r.status(), 409);
    assert_eq!(r.json()["error"]["details"]["archived"], true);
}

#[sqlx::test]
async fn list_pages_and_counts(pool: PgPool) {
    let c = ctx(pool).await;
    for i in 0..30 {
        c.add(
            &format!("T {:04}-1", 1000 + i),
            &format!("Item {i:02}"),
            None,
        )
        .await;
    }
    let p = c.get("/api/v1/products?per_page=25&page=2").await;
    assert_eq!(p["items"].as_array().unwrap().len(), 5);
    assert_eq!(p["total"], 30);
    assert_eq!(p["page"], 2);
    assert_eq!(p["items"][0]["name"], "Item 25");
    let p = c.get("/api/v1/products?page=9").await;
    assert_eq!(p["items"].as_array().unwrap().len(), 0);
    assert_eq!(p["total"], 30);
    assert_eq!(
        c.get("/api/v1/products?per_page=1000").await["per_page"],
        100
    );
    assert_eq!(
        c.get("/api/v1/products?page=-3&per_page=0").await["page"],
        1
    );
}

#[sqlx::test]
async fn list_filters_by_category_and_q(pool: PgPool) {
    let c = ctx(pool).await;
    let cat =
        c.a.post_json(
            "/api/v1/categories",
            json!({"name": "Boxes"}),
            Some(&c.s.cookie),
            Some(&c.s.csrf),
        )
        .await
        .json()["id"]
            .as_str()
            .unwrap()
            .to_string();
    c.add("B 6143-8", "Storage Box", Some(&cat)).await;
    c.add("B 6143-10", "Storage Box", Some(&cat)).await;
    c.add("IP 1060-11", "4 Section Plate", None).await;
    let p = c.get(&format!("/api/v1/products?category_id={cat}")).await;
    assert_eq!(p["total"], 2);
    assert!(
        p["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["category"]["name"] == "Boxes")
    );
    let p = c.get("/api/v1/products?q=storage").await;
    assert_eq!(p["total"], 2);
    let p = c.get("/api/v1/products?q=1060").await;
    assert_eq!(p["total"], 1);
    assert_eq!(p["items"][0]["code"], "IP1060-11");
}

#[sqlx::test]
async fn cashier_can_search(pool: PgPool) {
    seed_user(&pool, "cash", PW, &["cashier"]).await;
    let a = app(pool);
    let s = a.login("cash", PW).await;
    assert_eq!(
        a.get_with("/api/v1/products/search?q=1", Some(&s.cookie))
            .await
            .status(),
        200
    );
    assert_eq!(
        a.get_with("/api/v1/products", Some(&s.cookie))
            .await
            .status(),
        200
    );
    assert_eq!(a.get("/api/v1/products/search?q=1").await.status(), 401);
}

/// Phase 2 exit criterion: search under 100 ms server time at 10,000 products.
/// Ignored by default; CI runs it with `--ignored`.
#[sqlx::test]
#[ignore]
async fn search_is_fast_at_10k_products(pool: PgPool) {
    let c = ctx(pool.clone()).await;
    let user: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'boss'")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "WITH p AS (
           INSERT INTO products (id, brand_id, name, selling_price, created_by)
           SELECT gen_random_uuid(), '00000000-0000-7000-8001-000000000001',
                  'Product number ' || n, 100, $1
           FROM generate_series(1, 10000) n
           RETURNING id, name
         )
         INSERT INTO product_identifiers (id, product_id, brand_id, identifier_type, raw_value,
                                          normalized_value, base_code, variant, variant_num, digits)
         SELECT gen_random_uuid(), p.id, '00000000-0000-7000-8001-000000000001', 'product_code',
                'T' || n || '-' || (n % 7), 'T' || n || '-' || (n % 7), 'T' || n, (n % 7)::text, n % 7,
                n || '-' || (n % 7)
         FROM p, LATERAL (SELECT substring(p.name from '[0-9]+$')::int AS n) x",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("ANALYZE").execute(&pool).await.unwrap();

    let queries = ["T5000-2", "T4321", "123", "number 77", "T99", "bowl"];
    let mut times = Vec::new();
    for i in 0..20 {
        let q = queries[i % queries.len()];
        let t = std::time::Instant::now();
        let r =
            c.a.get_with(
                &format!("/api/v1/products/search?q={}", urlencode(q)),
                Some(&c.s.cookie),
            )
            .await;
        times.push(t.elapsed());
        assert_eq!(r.status(), 200);
    }
    times.sort();
    let median = times[times.len() / 2];
    println!("search median {median:?}, max {:?}", times.last().unwrap());
    assert!(median.as_millis() < 100, "median {median:?}");
}

#[sqlx::test]
async fn same_name_lists_sizes_in_order(pool: PgPool) {
    let c = ctx(pool).await;
    for code in ["IP 1060-11", "IP 1060-9", "IP 1060-12"] {
        c.add(code, "4 Section Plate", None).await;
    }
    let p = c.get("/api/v1/products").await;
    let got: Vec<&str> = p["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["code"].as_str().unwrap())
        .collect();
    assert_eq!(got, ["IP1060-9", "IP1060-11", "IP1060-12"]);
}
