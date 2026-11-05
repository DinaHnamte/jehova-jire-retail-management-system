#![allow(dead_code)]
use axum::{
    Router,
    body::{Body, Bytes},
    http::{HeaderMap, Request, StatusCode},
};
use http_body_util::BodyExt;
use jj_retail_api::{AppState, build_app, config::Config};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

pub struct TestApp {
    pub router: Router,
    pub pool: PgPool,
}

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl TestResponse {
    pub fn status(&self) -> u16 {
        self.status.as_u16()
    }
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).expect("response body is JSON")
    }
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers
            .get(name)
            .map(|v| v.to_str().unwrap().to_string())
    }
}

pub fn test_config() -> Config {
    Config {
        bind: "127.0.0.1:0".into(),
        database_url: String::new(),
        database_max_connections: 5,
        cookie_secure: true,
        trust_proxy: false,
        log_format: "pretty".into(),
        run_migrations: false,
    }
}

pub fn app(pool: PgPool) -> TestApp {
    let state = AppState::new(pool.clone(), test_config());
    TestApp {
        router: build_app(state),
        pool,
    }
}

impl TestApp {
    pub async fn send(&self, req: Request<Body>) -> TestResponse {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        TestResponse {
            status,
            headers,
            body,
        }
    }

    pub async fn get(&self, path: &str) -> TestResponse {
        self.get_with(path, None).await
    }

    pub async fn get_with(&self, path: &str, cookie: Option<&str>) -> TestResponse {
        let mut b = Request::get(path);
        if let Some(c) = cookie {
            b = b.header("cookie", c);
        }
        self.send(b.body(Body::empty()).unwrap()).await
    }

    pub async fn post_json(
        &self,
        path: &str,
        body: Value,
        cookie: Option<&str>,
        csrf: Option<&str>,
    ) -> TestResponse {
        let mut b = Request::post(path).header("content-type", "application/json");
        if let Some(c) = cookie {
            b = b.header("cookie", c);
        }
        if let Some(t) = csrf {
            b = b.header("x-csrf-token", t);
        }
        self.send(b.body(Body::from(body.to_string())).unwrap())
            .await
    }
}

impl TestApp {
    /// Any method with an optional body, cookie and CSRF header.
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        content_type: Option<&str>,
        body: Option<String>,
        cookie: Option<&str>,
        csrf: Option<&str>,
    ) -> TestResponse {
        let mut b = Request::builder().method(method).uri(path);
        if let Some(ct) = content_type {
            b = b.header("content-type", ct);
        }
        if let Some(c) = cookie {
            b = b.header("cookie", c);
        }
        if let Some(t) = csrf {
            b = b.header("x-csrf-token", t);
        }
        self.send(
            b.body(body.map(Body::from).unwrap_or_else(Body::empty))
                .unwrap(),
        )
        .await
    }

    pub async fn patch_json(
        &self,
        path: &str,
        body: Value,
        cookie: Option<&str>,
        csrf: Option<&str>,
    ) -> TestResponse {
        self.request(
            "PATCH",
            path,
            Some("application/json"),
            Some(body.to_string()),
            cookie,
            csrf,
        )
        .await
    }

    pub async fn delete_with(
        &self,
        path: &str,
        cookie: Option<&str>,
        csrf: Option<&str>,
    ) -> TestResponse {
        self.request("DELETE", path, None, None, cookie, csrf).await
    }

    pub async fn post_raw(
        &self,
        path: &str,
        content_type: &str,
        body: String,
        cookie: Option<&str>,
        csrf: Option<&str>,
    ) -> TestResponse {
        self.request("POST", path, Some(content_type), Some(body), cookie, csrf)
            .await
    }
}

pub async fn seed_user(
    pool: &PgPool,
    username: &str,
    password: &str,
    roles: &[&str],
) -> uuid::Uuid {
    let hash = jj_retail_api::admin::password::hash_password(password).unwrap();
    let mut c = pool.acquire().await.unwrap();
    jj_retail_api::admin::repo::create_user(&mut c, username, username, &hash, roles)
        .await
        .unwrap()
}

pub struct Session {
    pub cookie: String,
    pub csrf: String,
}

impl TestApp {
    pub async fn login_raw(&self, username: &str, password: &str) -> TestResponse {
        self.post_json(
            "/api/v1/auth/login",
            serde_json::json!({ "username": username, "password": password }),
            None,
            None,
        )
        .await
    }

    /// Log in and return the cookie pair (`jj_session=...`) and CSRF token. Panics unless 200.
    pub async fn login(&self, username: &str, password: &str) -> Session {
        let r = self.login_raw(username, password).await;
        assert_eq!(
            r.status(),
            200,
            "login failed: {:?}",
            String::from_utf8_lossy(&r.body)
        );
        let set = r.header("set-cookie").expect("set-cookie");
        Session {
            cookie: set.split(';').next().unwrap().to_string(),
            csrf: r.json()["csrf_token"].as_str().unwrap().to_string(),
        }
    }
}
