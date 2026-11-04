# Jehovah Jireh — Operations

| | |
|---|---|
| **Status** | Draft; completed during roadmap phase 7 |
| **Last updated** | 5 November 2026 |
| **Related** | [Architecture.md](Architecture.md) · [Delivery.md](Delivery.md) |

How the system is hosted, backed up, monitored, and run safely. It also
covers the business and legal checks that software alone can't settle.

---

## 1. Hosting

([ADR-0002](docs/adr/0002-cloud-hosting-online-only.md))

| Item | Choice |
|---|---|
| Server | One Linux VPS in an Indian region (Mumbai/Hyderabad/Chennai/Delhi). Start at 2 vCPU / 4 GB RAM / 50 GB SSD |
| OS | Current Ubuntu LTS or Debian stable, unattended security updates on |
| Database | PostgreSQL 16 on the same server; data directory on the persistent disk |
| Web | Caddy: automatic HTTPS, serves the Vue build, proxies `/api` to `127.0.0.1:8000` |
| API | `jj-retail-api` binary as a systemd service, running as its own unprivileged user |
| Images | Local directory outside the web root, included in backups |
| Firewall | Only 22 (SSH, key-only, no root login), 80 and 443 open |
| Domain | One domain or subdomain; same origin for app and API |

Deploy: CI builds the binary and frontend, copies them to the server,
runs `sqlx migrate run`, then `systemctl restart jj-retail-api`.
Migrations must be backward compatible with the running binary (add
first, remove later). The previous release is kept on the server for a
one-command rollback.

Secrets (`DATABASE_URL`, `SESSION_SECRET`) live in
`/etc/jj-retail/env` (root-owned, mode 0600, read by systemd), never in the repository. A
`.env.example` with placeholder values is committed.

```dotenv
APP_ENV=production
APP_BIND=127.0.0.1:8000
APP_TIMEZONE=Asia/Kolkata
DATABASE_URL=postgres://jj_retail:CHANGE_ME@localhost:5432/jj_retail
DATABASE_MAX_CONNECTIONS=10
SESSION_SECRET=CHANGE_ME_64_RANDOM_BYTES
STORAGE_PATH=/var/lib/jj-retail/files
```

## 2. Backups and restore

| What | How | Retention |
|---|---|---|
| Database, continuous | WAL archiving to off-site object storage (`wal-g` or `pgBackRest`) | 14 days point-in-time recovery |
| Database, nightly | `pg_dump -Fc`, encrypted, to the same off-site storage | 30 daily, 12 monthly |
| Images | Nightly sync to off-site storage | Same as database |
| Server config | Kept in the repository (`deploy/`) | — |

Off-site storage must be with a **different provider or account** from
the VPS.

**Restore drill:** before go-live and then every quarter, restore the
latest backup to a scratch server, start the API against it, and check
that the last order and the stock and ledger reconciliation match.
Record the date and how long it took. A backup that hasn't been
restored hasn't been proven.

## 3. Monitoring

- `GET /health/live` (process up) and `GET /health/ready` (database
  reachable, migrations current).
- An external uptime check on `/health/ready` every minute, alerting the
  owner by email or phone.
- JSON logs via journald, rotated. Each request has an ID that is shown
  to the user in error messages, so a reported problem can be found.
- Nightly reconciliation job (stock and ledger). A mismatch sends an
  alert. It is never auto-corrected.
- Alerts for disk > 80% and a failed backup.

## 4. Delivery drivers: legal and compliance

Moved here from v1 §27.5–27.8 and §28.11. **This is a checklist of
questions for professionals, not legal advice.**

### 4.1 Before the first driver starts

Ask the Mizoram transport authority, the insurer and a local
legal/accounting professional:

- Is delivering retail goods by two-wheeler a regulated activity here,
  and does it need any permit or authorisation?
- Does the driver's vehicle registration and **insurance** cover
  commercial goods delivery? Ordinary private-use cover may not. A
  bike-taxi (passenger) permit doesn't automatically cover goods, and
  goods cover doesn't cover passengers.
- What's the correct legal and tax treatment of part-time delivery
  providers and what they're paid?
- Who is responsible for accidents, damaged or lost goods, and
  third-party claims? Is goods-in-transit or public liability cover
  sensible?

The software records facts accurately. It can't make an arrangement
lawful, and a template agreement or app sign-up doesn't decide whether
someone is legally an employee or a contractor. That depends on how the
work is actually done.

### 4.2 Driver record (phase 1 fields)

Kept in `driver_profiles`: name, phone, vehicle registration number,
licence checked (yes/no, date, by whom), licence expiry, insurance
checked (yes/no, date, by whom), insurance expiry, agreement date.

- Assignment is blocked when the licence or insurance is expired. An
  admin override needs a reason, is audited, and must not be used to
  get around a legal requirement.
- Store **checked/expiry facts**, not document scans, in phase 1.
  Uploaded documents (phase 2) will be access-restricted, encrypted at
  rest, and have a retention period.

### 4.3 Delivery agreement checklist

To be prepared or reviewed by a professional: scope and service area;
own vehicle and insurance responsibilities; required licences;
delivery fee vs driver pay; cash collection, handover deadlines and
the shortage/dispute process; responsibility for goods while the
driver has them; failed deliveries and returns; accident and incident
reporting; customer data privacy; termination.

## 5. Privacy and retention

- Collect only what's needed: name and phone for delivery customers;
  no customer account required for counter sales.
- Drivers see customer details only for their active assigned delivery
  ([Delivery.md §4](Delivery.md#4-driver-access)).
- No continuous location tracking. No delivery photos in v1.
- Logs don't contain full phone numbers, addresses, passwords or
  tokens.
- Retention: orders, ledger and stock records are kept indefinitely
  (needed for business records). Inactive customer contact details
  are anonymised on request; their past orders keep the snapshot.
  Sessions older than 90 days are deleted.

## 6. Go-live checklist

- [ ] Restore drill passed and recorded
- [ ] Uptime and backup alerts reach the owner
- [ ] Admin account with a strong password; staff accounts with
      least-privilege roles
- [ ] Opening stock count entered as `opening_balance` movements and
      checked
- [ ] Receipt prints correctly on the shop's printer
- [ ] POS and scanner tested on the shop's actual devices and network
- [ ] Tax mode confirmed (`none` unless GST registration is confirmed
      with an accountant)
- [ ] Delivery zones, fees and default driver pay set
- [ ] §4.1 questions answered before enabling any driver
- [ ] Staff guide written (§7)

## 7. Staff procedures (to write)

1. Opening and closing the shop: count the till against the cash report.
2. Counter sale, discount, return and refund.
3. Registering a new product by scanning.
4. Receiving goods from a supplier.
5. Stock count and adjustments.
6. Creating a delivery order and dispatching it.
7. Driver handover at the end of the day.
8. What to do if the app is down: write sales on paper, enter them
   afterwards as orders with the original time noted.
