# Deployment

One Linux VPS (Operations.md §1). Run the one-time setup once, then `deploy/deploy.sh` for every release.

## One-time server setup (Ubuntu/Debian)

```sh
# Packages
sudo apt update && sudo apt install -y postgresql caddy rsync curl unattended-upgrades

# Service user and directories
sudo useradd --system --home /var/lib/jj-retail --shell /usr/sbin/nologin jj-retail
sudo install -d -o jj-retail -g jj-retail -m 0750 /var/lib/jj-retail /var/lib/jj-retail/files
sudo install -d /opt/jj-retail/releases /srv/jj-retail/web-releases /etc/jj-retail

# Database
sudo -u postgres psql -c "CREATE ROLE jj_retail LOGIN PASSWORD '<generate a strong password>';"
sudo -u postgres psql -c "CREATE DATABASE jj_retail OWNER jj_retail;"
sudo -u postgres psql -d jj_retail -c "CREATE EXTENSION IF NOT EXISTS pg_trgm;"

# Configuration (copy deploy/env.production.example and fill in DATABASE_URL)
sudo install -o jj-retail -g jj-retail -m 0600 env.production.example /etc/jj-retail/env
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
sudo -u jj-retail bash -c 'set -a; . /etc/jj-retail/env; /opt/jj-retail/current/jj-retail-api create-admin --username admin --display-name "Owner"'
```

Backups and monitoring: Operations.md §2–3 (set up before go-live).

## Rollback

`deploy.sh` rolls back automatically when `/health/ready` fails. To roll back by hand, point
`/opt/jj-retail/current` and `/srv/jj-retail/web` at the previous release and
`sudo systemctl restart jj-retail-api`. Migrations are not rolled back, which is why they must
stay compatible with the previous release (add first, remove later).
