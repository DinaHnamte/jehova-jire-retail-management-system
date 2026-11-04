# Jehovah Jireh (JJ Retail)

Retail management system for one shop: catalogue, label scanning, inventory,
POS, orders, delivery. Design docs: [Architecture.md](Architecture.md),
[Delivery.md](Delivery.md), [Operations.md](Operations.md), [docs/adr/](docs/adr/README.md).

## Development

Requirements: Rust (stable), Node 22+ with pnpm, PostgreSQL 16+ (Docker or local),
`sqlx-cli` (`cargo install sqlx-cli --no-default-features --features rustls,postgres`).

```sh
# 1. Database (Docker, port 5433) — or use a local Postgres and adjust DATABASE_URL
docker compose up -d db

# 2. API
cd backend
cp .env.example .env            # edit DATABASE_URL if needed
sqlx migrate run
cargo run                       # http://127.0.0.1:8000/health/ready
cargo test                      # needs DATABASE_URL; each test gets its own database

# 3. Web app
cd frontend
pnpm install
pnpm dev                        # http://localhost:5173, proxies /api to the API
```

## Deploy

See [deploy/README.md](deploy/README.md): one-time server setup, then `deploy/deploy.sh user@host`.
CI (`.github/workflows/ci.yml`) runs fmt, clippy, the SQLx offline-data check and tests for the API,
plus typecheck, tests and build for the web app.
