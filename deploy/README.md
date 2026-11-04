# Deployment

One Linux VPS (Operations.md §1). Run the one-time setup once, then `deploy/deploy.sh` for every release.

**Production:** `debian@15.235.247.241` (Debian 13, x86-64), site `https://jehovah-jire.hnamtek.in`
behind Cloudflare's proxy (SSL/TLS mode must be **Full (strict)**). PostgreSQL 17 from Debian,
Caddy from the official Caddy apt repository (the Caddyfile needs Caddy 2.7+). The API binary is
compiled on the server inside Docker (`BUILD_ON=remote`, the default), so Docker must stay installed.

## One-time server setup (Ubuntu/Debian)

```sh
# Packages
sudo apt update && sudo apt install -y postgresql rsync curl unattended-upgrades docker.io
# Caddy 2.7+ from the official repo: https://caddyserver.com/docs/install#debian-ubuntu-raspbian

# Service user and directories
sudo useradd --system --home /var/lib/jj-retail --shell /usr/sbin/nologin jj-retail
sudo install -d -o jj-retail -g jj-retail -m 0750 /var/lib/jj-retail /var/lib/jj-retail/files
sudo install -d /opt/jj-retail/releases /srv/jj-retail/web-releases /etc/jj-retail

# Database
sudo -u postgres psql -c "CREATE ROLE jj_retail LOGIN PASSWORD '<generate a strong password>';"
sudo -u postgres psql -c "CREATE DATABASE jj_retail OWNER jj_retail;"
sudo -u postgres psql -d jj_retail -c "CREATE EXTENSION IF NOT EXISTS pg_trgm;"

# Configuration (copy deploy/env.production.example and fill in DATABASE_URL).
# Root-owned: systemd reads it for the service, so the service user can't rewrite it.
# Percent-encode special characters in the DATABASE_URL password.
sudo install -o root -g root -m 0600 env.production.example /etc/jj-retail/env
sudoedit /etc/jj-retail/env

# Service and web server
sudo cp jj-retail-api.service /etc/systemd/system/ && sudo systemctl daemon-reload
sudo systemctl enable jj-retail-api
sudo cp Caddyfile /etc/caddy/Caddyfile
sudo systemctl edit caddy        # add: [Service]\nEnvironment=JJ_DOMAIN=shop.example.com
sudo systemctl restart caddy

# Firewall: SSH, HTTP, HTTPS only
sudo ufw allow OpenSSH && sudo ufw allow 80,443/tcp && sudo ufw enable
```

Then from your machine: `deploy/deploy.sh deploy@shop.example.com`, and create the first
administrator on the server:

```sh
sudo systemd-run --pty --wait --collect --uid=jj-retail --gid=jj-retail \
  -p EnvironmentFile=/etc/jj-retail/env \
  /opt/jj-retail/current/jj-retail-api create-admin --username admin --display-name "Owner"
```

Backups and monitoring: Operations.md §2–3 (set up before go-live).

## Rollback

`deploy.sh` rolls back automatically when `/health/ready` fails. To roll back by hand, point
`/opt/jj-retail/current` and `/srv/jj-retail/web` at the previous release and
`sudo systemctl restart jj-retail-api`. Migrations are not rolled back, which is why they must
stay compatible with the previous release (add first, remove later).
