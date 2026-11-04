# Phase 1 — Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A deployable skeleton: Rust API + Vue PWA where a user can log in, reach a permission-protected page, and every request has the standard error format, logging and health checks; CI runs everything.

**Architecture:** One Axum binary (`backend/`) with modules `http`, `shared`, `admin` (users/roles/sessions), plus a Vue 3 PWA (`frontend/`). Server-side sessions in Postgres with an HttpOnly cookie and a CSRF header. Same origin: Vite proxies `/api` in dev; Caddy does it in prod.

**Tech Stack:** Rust (Axum, Tokio, SQLx/Postgres, argon2, tracing, rust_decimal not needed yet), Vue 3 + TypeScript + Vite + Pinia + vue-router + Tailwind + vite-plugin-pwa, Vitest, PostgreSQL 16 (Docker for dev), GitHub Actions.

**Spec:** [Architecture.md](../../../Architecture.md) §3.1, §4, §5, §12, §13, §15, §16, §20 phase 1; [Operations.md](../../../Operations.md) §1, §3.

## Global Constraints

- Crate name `jj-retail-api`; frontend package `jj-retail-web`; database `jj_retail`; API prefix `/api/v1`.
- Primary keys UUIDv7; timestamps `timestamptz`; parameterised SQL only (SQLx macros).
- Error body: `{ "error": { "code", "message", "details", "request_id" } }`; never leak SQL errors or stack traces.
- Status codes: 400 malformed · 401 unauthenticated · 403 forbidden · 404 · 409 · 422 · 429 rate-limited · 500.
- Passwords Argon2id. Session cookie `jj_session`: random 32-byte token, `HttpOnly; SameSite=Lax; Path=/`, `Secure` unless `COOKIE_SECURE=false`; only `sha256(token)` stored.
- Session idle timeout: 12 h; 30 days if the user's only role is `delivery_driver`.
- State-changing requests (non-GET/HEAD/OPTIONS) under `/api/v1` except `POST /auth/login` require `X-CSRF-Token` equal to the session's CSRF token.
- Authorization checks permission codes, never role names.
- Logs: JSON via `tracing`; every response carries `x-request-id`; never log passwords, tokens, cookies.
- Health: `GET /health/live` → 200 always; `GET /health/ready` → 200 when DB reachable and migrations applied, else 503. (Not under `/api/v1`.)
- Dev Postgres via Docker on port **5433** (a local Postgres 15 already uses 5432).

## Review Focus

1. **Expired or revoked session** → API returns 401 `UNAUTHENTICATED`; frontend clears session state and redirects to `/login?next=…`. (Task 4 test `expired_session_is_rejected`; Task 5 test `redirects to login on 401`.)
2. **POST with a valid cookie but missing/wrong CSRF header** → 403 `CSRF_INVALID`, nothing changed. (Task 4 `logout_without_csrf_is_rejected`.)
3. **Unknown username vs wrong password** → identical 401 `INVALID_CREDENTIALS` body; unknown user still runs an Argon2 verify against a dummy hash. (Task 4 `unknown_user_and_wrong_password_look_identical`.)
4. **Deactivated user** → cannot log in, and their existing sessions stop working on the next request. (Task 4 `deactivated_user_session_is_rejected`.)
5. **Login brute force** → 6th failed attempt for the same username within 15 min returns 429 `RATE_LIMITED`. (Task 4 `login_is_rate_limited`.)

---

## File structure

```text
.
├── .gitignore  README.md  docker-compose.yml
├── .github/workflows/ci.yml
├── deploy/  Caddyfile  jj-retail-api.service  deploy.sh  env.production.example
├── backend/
│   ├── Cargo.toml  .env.example  .sqlx/ (committed, from `cargo sqlx prepare`)
│   ├── migrations/
│   │   ├── 20261105000001_admin.sql          users, roles, permissions, role_permissions, user_roles, sessions, audit_logs, settings
│   │   └── 20261105000002_seed_roles.sql     permissions + roles + grants
│   ├── src/
│   │   ├── main.rs            CLI: `serve` (default) | `create-admin`
│   │   ├── lib.rs             pub fn build_app(state) -> Router  (used by tests)
│   │   ├── config.rs          Config::from_env()
│   │   ├── state.rs           AppState { db: PgPool, config: Arc<Config>, login_limiter: Arc<LoginLimiter> }
│   │   ├── error.rs           AppError + IntoResponse
│   │   ├── http/mod.rs        router assembly, /health/*
│   │   ├── http/request_id.rs request-id middleware
│   │   ├── http/csrf.rs       CSRF middleware
│   │   ├── shared/ids.rs      new_id() -> Uuid (v7)
│   │   └── admin/
│   │       ├── mod.rs
│   │       ├── password.rs    hash_password, verify_password, DUMMY_HASH
│   │       ├── session.rs     create/lookup/touch/delete session; CurrentUser extractor
│   │       ├── limiter.rs     LoginLimiter
│   │       ├── routes.rs      /auth/login, /auth/logout, /auth/me, GET /users
│   │       ├── repo.rs        SQL
│   │       └── dto.rs
│   └── tests/
│       ├── common/mod.rs      spawn_app(pool) -> TestApp (axum-test or tower::oneshot helper), seed_user()
│       ├── health.rs  errors.rs  auth.rs  permissions.rs
└── frontend/
    ├── package.json  vite.config.ts  tsconfig*.json  tailwind config (v4: via @tailwindcss/vite)  index.html
    └── src/
        ├── main.ts  App.vue
        ├── app/router.ts  app/guards.ts  app/layouts/AppLayout.vue  app/nav.ts
        ├── shared/api/client.ts  shared/api/errors.ts
        ├── modules/auth/LoginPage.vue  modules/auth/session.store.ts
        ├── modules/admin/UsersPage.vue
        └── modules/overview/OverviewPage.vue
        tests alongside: *.test.ts
```

---

### Task 1: Repository, dev database, backend skeleton with health checks and error format

**Files:**
- Create: `.gitignore`, `README.md`, `docker-compose.yml`, `backend/Cargo.toml`, `backend/.env.example`, `backend/src/{main,lib,config,state,error}.rs`, `backend/src/http/{mod,request_id}.rs`, `backend/src/shared/{mod,ids}.rs`, `backend/migrations/20261105000001_admin.sql` (empty-safe placeholder created in Task 2 — for this task create `backend/migrations/20261105000000_init.sql` with `CREATE EXTENSION IF NOT EXISTS pg_trgm;`)
- Test: `backend/tests/common/mod.rs`, `backend/tests/health.rs`, `backend/tests/errors.rs`

**Interfaces:**
- Produces: `jj_retail_api::build_app(state: AppState) -> axum::Router`; `AppState { db, config, login_limiter }` (limiter added in Task 4 — start with `db`, `config`); `AppError` enum with variants `Unauthenticated`, `Forbidden`, `CsrfInvalid`, `InvalidCredentials`, `RateLimited`, `Validation(String)`, `NotFound`, `Conflict(String)`, `Internal(anyhow::Error)` and `impl From<sqlx::Error> for AppError` (→ Internal, logged, not exposed); `shared::ids::new_id() -> Uuid`; test helper `common::TestApp { router, pool }` with `async fn get(&self, path) -> TestResponse` and `post_json(path, body, cookie: Option<&str>, csrf: Option<&str>)`.

- [ ] **Step 1:** `git init`; `.gitignore` (target/, node_modules/, dist/, .env, .DS_Store, storage/). `docker-compose.yml`: service `db`, image `postgres:16`, port `5433:5432`, user/password/db `jj_retail`, named volume. `README.md`: dev commands (`docker compose up -d db`, `cd backend && sqlx migrate run && cargo run`, `cd frontend && pnpm dev`).
- [ ] **Step 2:** `cargo new backend --lib --name jj-retail-api` + bin `main.rs`; add deps: axum, tokio (full), sqlx (runtime-tokio, tls-rustls, postgres, uuid, chrono, migrate, macros), serde, serde_json, uuid (v7, serde), chrono (serde), tracing, tracing-subscriber (json, env-filter), tower-http (trace, request-id), anyhow, thiserror, dotenvy. Dev-deps: axum-test (or http-body-util + tower for oneshot).
- [ ] **Step 3: Write failing tests** (`#[sqlx::test]` gives each test a fresh database with migrations applied):

```rust
// tests/health.rs
#[sqlx::test] async fn live_is_ok(pool: PgPool)  { assert_eq!(app(pool).get("/health/live").await.status(), 200); }
#[sqlx::test] async fn ready_is_ok_with_db(pool: PgPool) {
    let r = app(pool).get("/health/ready").await;
    assert_eq!(r.status(), 200); assert_eq!(r.json()["database"], "ok");
}
#[sqlx::test] async fn ready_is_503_when_db_closed(pool: PgPool) {
    let a = app(pool.clone()); pool.close().await;
    assert_eq!(a.get("/health/ready").await.status(), 503);
}
#[sqlx::test] async fn every_response_has_request_id(pool: PgPool) {
    assert!(app(pool).get("/health/live").await.header("x-request-id").is_some());
}
// tests/errors.rs
#[sqlx::test] async fn unknown_api_route_uses_error_shape(pool: PgPool) {
    let r = app(pool).get("/api/v1/nope").await;
    assert_eq!(r.status(), 404);
    let e = &r.json()["error"];
    assert_eq!(e["code"], "NOT_FOUND"); assert!(e["request_id"].is_string());
}
```
- [ ] **Step 4:** Run `cd backend && DATABASE_URL=postgres://jj_retail:jj_retail@localhost:5433/jj_retail cargo test` → FAIL (no app).
- [ ] **Step 5:** Implement. `Config::from_env()` reads `APP_BIND` (default `127.0.0.1:8000`), `DATABASE_URL`, `DATABASE_MAX_CONNECTIONS` (10), `COOKIE_SECURE` (true), `LOG_FORMAT` (`json`|`pretty`, default json). Request ID: honour incoming `x-request-id` if it's a valid UUID, else generate v7; store in request extensions; `AppError::into_response` reads it from a task-local set by the middleware. `/health/ready`: `SELECT 1` with a 2 s timeout, plus check `_sqlx_migrations` count equals `sqlx::migrate!().iter().count()`; JSON `{ "database": "ok"|"error", "migrations": "ok"|"pending" }`. API fallback returns `AppError::NotFound`. `main.rs serve`: load `.env`, init tracing, connect pool, `sqlx::migrate!().run()` only if `RUN_MIGRATIONS=true` (dev), serve with graceful shutdown on SIGTERM/Ctrl-C.
- [ ] **Step 6:** Run tests → PASS. `cargo clippy -- -D warnings` clean.
- [ ] **Step 7:** Commit `chore: repo skeleton, backend health checks and error format`.

### Task 2: Admin schema and role/permission seed

**Files:**
- Create: `backend/migrations/20261105000001_admin.sql`, `backend/migrations/20261105000002_seed_roles.sql`
- Test: `backend/tests/permissions.rs` (schema part)

**Interfaces:**
- Produces tables: `users(id, username UNIQUE citext-like via lower() unique index, display_name, password_hash, is_active, created_at, updated_at)`, `roles(id, code UNIQUE, name)`, `permissions(code PK, description)`, `role_permissions(role_id, permission_code)`, `user_roles(user_id, role_id)`, `sessions(id, token_hash BYTEA UNIQUE, csrf_token TEXT, user_id, created_at, last_seen_at, expires_at, user_agent, ip)`, `audit_logs(id, actor_id NULL, action, entity_type, entity_id NULL, before JSONB, after JSONB, request_id, created_at)`, `settings(key PK, value JSONB, updated_at, updated_by)`.
- All FKs `ON DELETE RESTRICT`. UUID PKs generated in Rust (`new_id()`), so no DB default needed.

- [ ] **Step 1: Failing tests**

```rust
#[sqlx::test] async fn admin_role_has_every_permission(pool: PgPool) {
    let missing: i64 = sqlx::query_scalar("SELECT count(*) FROM permissions p WHERE NOT EXISTS (
        SELECT 1 FROM role_permissions rp JOIN roles r ON r.id = rp.role_id
        WHERE r.code='admin' AND rp.permission_code=p.code)").fetch_one(&pool).await.unwrap();
    assert_eq!(missing, 0);
}
#[sqlx::test] async fn driver_role_has_only_own_scoped_permissions(pool: PgPool) {
    let perms: Vec<String> = /* permissions of role 'delivery_driver' sorted */;
    assert_eq!(perms, ["deliveries.collect_own","deliveries.read_own","deliveries.update_own","settlements.read_own"]);
}
#[sqlx::test] async fn cashier_cannot_manage_users_or_adjust_stock(pool: PgPool) { /* assert 'users.manage','inventory.adjust','orders.void' not granted to cashier */ }
#[sqlx::test] async fn usernames_are_case_insensitive_unique(pool: PgPool) { /* insert 'Dinah' then 'dinah' → unique violation */ }
```
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Write migrations. Seed permissions (exact codes): `products.read products.create products.update products.archive inventory.read inventory.receive inventory.adjust inventory.count purchases.read purchases.create purchases.receive orders.read orders.create orders.confirm orders.void orders.discount_override payments.record payments.verify payments.reverse returns.create reports.read audit.read users.manage settings.manage deliveries.read deliveries.assign deliveries.cancel deliveries.check_in deliveries.read_own deliveries.update_own deliveries.collect_own settlements.read settlements.receive settlements.pay settlements.read_own drivers.manage`. Roles and grants:
  - `admin`: all.
  - `manager`: all except `users.manage`, `settings.manage`, and the `*_own` driver permissions.
  - `cashier`: `products.read inventory.read orders.read orders.create orders.confirm payments.record returns.create deliveries.read settlements.receive`.
  - `stock_operator`: `products.read products.create products.update inventory.read inventory.receive inventory.count purchases.read purchases.receive`.
  - `delivery_driver`: the four `*_own` permissions.
  - `auditor`: every `*.read` permission except `*_own`, plus `audit.read`.
  Role UUIDs: fixed literal UUIDs in the seed so they're stable across environments.
- [ ] **Step 4:** Run → PASS. Run `cargo sqlx prepare` later in Task 4 once queries exist.
- [ ] **Step 5:** Commit `feat(admin): users, roles, permissions, sessions schema with seeded roles`.

### Task 3: Passwords and `create-admin` CLI

**Files:**
- Create: `backend/src/admin/{mod,password,repo}.rs`
- Modify: `backend/src/main.rs`
- Test: unit tests in `password.rs`; `backend/tests/auth.rs::create_admin_*`

**Interfaces:**
- Produces: `password::hash_password(&str) -> anyhow::Result<String>` (Argon2id, default params, PHC string); `password::verify_password(plain: &str, phc: &str) -> bool`; `password::DUMMY_HASH: &str` (a valid PHC hash of a random string, generated once and pasted as a const); `repo::create_user(conn, username, display_name, password_hash, role_codes: &[&str]) -> Result<Uuid, AppError>` (Conflict on duplicate username); `repo::find_user_for_login(conn, username) -> Option<LoginUser { id, password_hash, is_active }>`; `repo::user_permissions(conn, user_id) -> Vec<String>`; `repo::user_role_codes(conn, user_id) -> Vec<String>`.
- CLI: `jj-retail-api create-admin --username <u> --display-name <n>`; reads password from `JJ_ADMIN_PASSWORD` env or prompts without echo (`rpassword`); minimum length 12.

- [ ] **Step 1: Failing tests:** `hash_then_verify_roundtrip`, `verify_rejects_wrong_password`, `verify_with_dummy_hash_is_false`, `hash_is_argon2id` (starts with `$argon2id$`); integration `create_user_rejects_duplicate_username_case_insensitive` → `AppError::Conflict`.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement (password hashing runs in `tokio::task::spawn_blocking` at call sites in Task 4). **Step 4:** Run → PASS.
- [ ] **Step 5:** Commit `feat(admin): argon2id passwords and create-admin command`.

### Task 4: Sessions, login/logout/me, CSRF, rate limit, permission checks

**Files:**
- Create: `backend/src/admin/{session,limiter,routes,dto}.rs`, `backend/src/http/csrf.rs`
- Modify: `backend/src/{state,lib}.rs`, `backend/src/http/mod.rs`
- Test: `backend/tests/auth.rs`, `backend/tests/permissions.rs`

**Interfaces:**
- Consumes: Task 3 repo + password functions.
- Produces:
  - `CurrentUser { id: Uuid, username: String, display_name: String, permissions: HashSet<String>, session_id: Uuid, csrf_token: String }` as an Axum extractor (rejects with `Unauthenticated`).
  - `impl CurrentUser { fn require(&self, perm: &str) -> Result<(), AppError> }` → `Forbidden` if missing.
  - `session::create(conn, user_id, ttl: Duration, ua, ip) -> (token: String, csrf: String)`; `session::lookup(conn, token) -> Option<CurrentUser>` (joins active user; rejects expired; touches `last_seen_at`/`expires_at` when last touch > 5 min ago); `session::delete(conn, session_id)`; `session::delete_all_for_user(conn, user_id)`.
  - `LoginLimiter::check(&self, key: &str) -> bool` and `record_failure(key)`, `reset(key)`; in-memory, 5 failures per 15 min per lower-cased username, plus 20 per 15 min per IP.
  - Routes: `POST /api/v1/auth/login {username,password}` → 200 `{ user: {id, username, display_name, permissions[]}, csrf_token }` + `Set-Cookie`; `POST /api/v1/auth/logout` → 204, cookie cleared; `GET /api/v1/auth/me` → same body as login; `GET /api/v1/users` (requires `users.manage`) → `[{id, username, display_name, is_active, roles[]}]`.
  - Audit rows: `auth.login`, `auth.logout`, `auth.login_failed` (entity = username in `after`, never password).

- [ ] **Step 1: Failing tests** (`tests/auth.rs`, helper `seed_user(pool, "cashier1", "correct horse battery", &["cashier"])`):
  - `login_sets_httponly_cookie_and_returns_csrf` — 200; `set-cookie` contains `jj_session=`, `HttpOnly`, `SameSite=Lax`; body has `csrf_token` and `user.permissions` includes `"orders.create"`.
  - `me_requires_session` — no cookie → 401 `UNAUTHENTICATED`.
  - `me_with_session_returns_user` — 200, username `cashier1`.
  - `unknown_user_and_wrong_password_look_identical` — both 401 with byte-identical JSON except `request_id`.
  - `deactivated_user_cannot_login` and `deactivated_user_session_is_rejected` (log in, set `is_active=false`, `/auth/me` → 401).
  - `expired_session_is_rejected` (log in, `UPDATE sessions SET expires_at = now() - interval '1 second'`, → 401).
  - `logout_without_csrf_is_rejected` — 403 `CSRF_INVALID`, session still valid afterwards.
  - `logout_with_csrf_ends_session` — 204, then `/auth/me` → 401.
  - `login_is_rate_limited` — 5 wrong passwords then a 6th attempt with the **correct** password → 429 `RATE_LIMITED`.
  - `session_token_is_stored_hashed` — `sessions.token_hash` ≠ cookie value bytes and has length 32.
  - `driver_session_lasts_30_days_staff_12_hours` — check `expires_at - created_at`.
  - `tests/permissions.rs`: `users_list_requires_users_manage` (cashier → 403 `FORBIDDEN`), `admin_can_list_users` (200, contains both users).
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement. Token: 32 bytes from `rand::rngs::OsRng`, base64url (no padding) in cookie; `token_hash = sha256(token bytes)`. CSRF middleware runs on `/api/v1` for unsafe methods, skips `/auth/login`, compares header with `CurrentUser.csrf_token` in constant time (`subtle`). Client IP from `X-Forwarded-For` only when `TRUST_PROXY=true` (prod behind Caddy), else socket address. Login: check limiter → look up user → `spawn_blocking(verify)` against stored hash or `DUMMY_HASH` → on failure record + audit + `InvalidCredentials`; on success reset limiter, create session, audit.
- [ ] **Step 4:** Run → PASS. `cargo sqlx prepare` and commit `.sqlx/`.
- [ ] **Step 5:** Commit `feat(auth): cookie sessions, CSRF, login rate limit, permission checks`.

### Task 5: Frontend skeleton — API client, session store, login, guarded layout, PWA

**Files:**
- Create: everything under `frontend/` listed in File structure.
- Test: `frontend/src/shared/api/client.test.ts`, `frontend/src/app/guards.test.ts`, `frontend/src/app/nav.test.ts`

**Interfaces:**
- Consumes: Task 4 endpoints and JSON shapes.
- Produces:
  - `class ApiError extends Error { status: number; code: string; details: unknown; requestId?: string }`.
  - `api.get<T>(path)`, `api.post<T>(path, body?)`, `api.patch<T>(path, body?)` — base `/api/v1`, `credentials: 'same-origin'`, adds `X-CSRF-Token` from the session store on unsafe methods, parses the error shape into `ApiError`, calls a registered `onUnauthenticated()` handler on 401.
  - `useSessionStore()` (Pinia): `user`, `csrfToken`, `can(perm: string): boolean`, `load()` (calls `/auth/me`, tolerates 401), `login(u, p)`, `logout()`.
  - `navItems: { label, to, perm?: string }[]` and `visibleNav(can): NavItem[]` — Overview (no perm), Users (`users.manage`); later phases append.
  - Router guard: routes have `meta.perm?`, `meta.public?`. Not logged in → `/login?next=<path>`; logged in without perm → `/forbidden`.

- [ ] **Step 1:** `pnpm create vite frontend --template vue-ts`; package name `jj-retail-web`; add pinia, vue-router, tailwindcss + @tailwindcss/vite, vite-plugin-pwa, vitest, @vue/test-utils, jsdom. `vite.config.ts`: dev proxy `/api` and `/health` → `http://127.0.0.1:8000`; PWA manifest name "Jehovah Jireh", short_name "JJ Retail", `registerType: 'autoUpdate'`, workbox navigateFallback, **no runtime caching for `/api`**.
- [ ] **Step 2: Failing tests:**
  - `client.test.ts`: `adds csrf header on POST but not GET`; `parses error shape into ApiError` (status 422, code `VALIDATION_FAILED`, requestId); `calls onUnauthenticated on 401`; `non-JSON 502 becomes ApiError with code NETWORK_ERROR`.
  - `guards.test.ts`: `redirects to login on 401` (store user null → `/login?next=/users`); `forbidden without permission`; `allows public route`.
  - `nav.test.ts`: `cashier does not see Users`; `admin sees Users`.
- [ ] **Step 3:** Run `pnpm vitest run` → FAIL. **Step 4:** Implement client, store, guard, nav, `LoginPage.vue` (username, password, submit; shows `INVALID_CREDENTIALS` as "Wrong username or password", `RATE_LIMITED` as "Too many attempts — wait 15 minutes"), `AppLayout.vue` (sidebar from `visibleNav`, user name, logout), `OverviewPage.vue` placeholder, `UsersPage.vue` (table from `GET /users`), `/forbidden` page. Mobile layout works at 360 px width.
- [ ] **Step 5:** Run `pnpm vitest run && pnpm vue-tsc --noEmit && pnpm build` → all pass.
- [ ] **Step 6: Manual check:** `docker compose up -d db`, run API with `RUN_MIGRATIONS=true COOKIE_SECURE=false`, `create-admin`, `pnpm dev`; in a browser: log in, see Users page, log out, `/users` redirects to login. Record result in the commit message.
- [ ] **Step 7:** Commit `feat(web): Vue PWA shell with login, session store, guarded routes`.

### Task 6: CI and deployment files

**Files:**
- Create: `.github/workflows/ci.yml`, `deploy/Caddyfile`, `deploy/jj-retail-api.service`, `deploy/deploy.sh`, `deploy/env.production.example`
- Modify: `README.md` (deploy section)

**Interfaces:**
- Consumes: binary `jj-retail-api` with `serve` / `create-admin`; frontend `dist/`.

- [ ] **Step 1:** `ci.yml` with two jobs. **backend**: service `postgres:16` (env `POSTGRES_USER/PASSWORD/DB=jj_retail`), `DATABASE_URL` set, `SQLX_OFFLINE=true` for the build, steps `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo sqlx prepare --check`, `cargo test`. **frontend**: pnpm install (frozen lockfile), `vue-tsc --noEmit`, `vitest run`, `vite build`.
- [ ] **Step 2:** `Caddyfile`: site `{$JJ_DOMAIN}`; `handle /api/* { reverse_proxy 127.0.0.1:8000 }`, `handle /health/* { reverse_proxy 127.0.0.1:8000 }`, else `root * /srv/jj-retail/web` + `try_files {path} /index.html` + `file_server`; headers: HSTS, `X-Content-Type-Options nosniff`, `Referrer-Policy same-origin`, CSP `default-src 'self'; img-src 'self' blob: data:; worker-src 'self' blob:; connect-src 'self'`; `sw.js` and `index.html` with `Cache-Control: no-cache`.
- [ ] **Step 3:** `jj-retail-api.service`: `User=jj-retail`, `EnvironmentFile=/etc/jj-retail/env`, `ExecStart=/opt/jj-retail/current/jj-retail-api serve`, `Restart=on-failure`, hardening (`NoNewPrivileges`, `ProtectSystem=strict`, `ReadWritePaths=/var/lib/jj-retail`, `PrivateTmp`).
- [ ] **Step 4:** `deploy.sh <host>`: build release binary (Linux target, via `cross` or CI artifact) and frontend; rsync to `/opt/jj-retail/releases/<timestamp>` and `/srv/jj-retail/web`; run `sqlx migrate run` against prod; flip `current` symlink; `systemctl restart`; curl `/health/ready` and roll the symlink back if it fails. `env.production.example` per Operations.md §1 with `COOKIE_SECURE=true`, `TRUST_PROXY=true`, `LOG_FORMAT=json`.
- [ ] **Step 5: Verify:** `bash -n deploy/deploy.sh`; `caddy validate --config deploy/Caddyfile` via `docker run --rm -v $PWD/deploy:/d caddy:2 caddy validate --config /d/Caddyfile --adapter caddyfile` (with `JJ_DOMAIN=example.com`); run CI steps locally (`cargo fmt --check && cargo clippy … && cargo test`, frontend commands) → all pass.
- [ ] **Step 6:** Commit `ci: GitHub Actions; deploy: Caddy, systemd and deploy script`.

---

## Phase 1 exit criteria (from Architecture.md §20)

- Local: log in, reach a permission-protected page, log out — verified in Task 5 step 6.
- "On a phone over HTTPS" and "deploys run from CI" need a server and a GitHub remote, neither of which exists yet. Deploy files are written and validated; the first real deploy happens when a VPS and domain are provided.
