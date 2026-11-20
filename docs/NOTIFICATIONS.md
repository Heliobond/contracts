# Notification System

**Issue:** [#131](https://github.com/Heliobond/contracts/issues/131)

The Heliobond notification system keeps investors informed when their invested projects' impact scores change. It consists of two parts:

1. **Enhanced on-chain events** — The `ProjectRegistry` contract emits a `ScoreChanged` event with old and new score values.
2. **Off-chain notification service** — A Node.js service that monitors these events and dispatches email/webhook notifications to investors.

---

## On-Chain Event: `ScoreChanged`

The `ProjectRegistry` contract emits `ScoreChanged` whenever an impact score is updated via `update_impact_score` or `update_credit_quality_score`. The event carries both old and new values so off-chain consumers can calculate the precise delta.

### Event Structure

| Field | Type | Description |
|-------|------|-------------|
| `project_id` (topic) | `u32` | The project whose scores changed |
| `old_credit_quality` | `u32` | Previous credit quality (0–100) |
| `new_credit_quality` | `u32` | New credit quality (0–100) |
| `old_green_impact` | `u32` | Previous green impact (0–100) |
| `new_green_impact` | `u32` | New green impact (0–100) |
| `old_rate_bps` | `u32` | Previous interest rate in basis points (500–1000) |
| `new_rate_bps` | `u32` | New interest rate in basis points (500–1000) |

### When It Fires

- `update_impact_score(project_id, credit_quality, green_impact)` — when either score changes
- `update_credit_quality_score(project_id, credit_quality)` — when credit quality changes

If the new values are identical to the old values, no event is emitted (no-op).

### Example (Rust)

```rust
// Initial scores: credit_quality = 0, green_impact = 0 → rate = 1000 bps
// After update:  credit_quality = 80, green_impact = 60 → rate = 650 bps
contract.update_impact_score(&project_id, &80u32, &60u32);
// Emitted ScoreChanged {
//   project_id: 1,
//   old_credit_quality: 0, new_credit_quality: 80,
//   old_green_impact: 0,  new_green_impact: 60,
//   old_rate_bps: 1000,   new_rate_bps: 650,
// }
```

### Decoded Event Shape (JSON)

Off-chain consumers that decode the raw Soroban event (as `notification-service/src/listener.ts` does via `decodeScoreChanged`) receive a flat object combining the topic field, the data-map fields, and metadata about where the event was emitted:

```json
{
  "project_id": 1,
  "old_credit_quality": 0,
  "new_credit_quality": 80,
  "old_green_impact": 0,
  "new_green_impact": 60,
  "old_rate_bps": 1000,
  "new_rate_bps": 650,
  "timestamp": 1751025600,
  "ledger": 123456
}
```

| Field | Type | Description |
|-------|------|--------------|
| `project_id` | `number` | The project whose scores changed (decoded from the event's topic) |
| `old_credit_quality` / `new_credit_quality` | `number` | Credit quality before/after (0–100) |
| `old_green_impact` / `new_green_impact` | `number` | Green impact before/after (0–100) |
| `old_rate_bps` / `new_rate_bps` | `number` | Interest rate in basis points before/after (500–1000) |
| `timestamp` | `number` | Unix timestamp (seconds) of the ledger close time |
| `ledger` | `number` | Stellar ledger sequence number the event was emitted in |

All fields are required — the decoder rejects any raw event that doesn't decode to every field above as a finite number, rather than passing through `null`/`NaN`.

---

## Off-Chain Notification Service

The notification service lives in `notification-service/`. It listens for `ScoreChanged` events from the `ProjectRegistry` contract and dispatches notifications to registered investors.

### Architecture

```
Soroban RPC  ──►  Listener  ──►  Investor Index  ──►  Notifier
                          │                          ├── Email (SMTP)
                          │                          └── Webhook (HTTP POST)
                          │
                     REST API ◄── Investors manage preferences
```

### Components

| Component | File | Description |
|-----------|------|-------------|
| `Listener` | `src/listener.ts` | Polls Soroban RPC for `ScoreChanged` events |
| `Store` | `src/db.ts` | SQLite database for investor preferences and project-investor index |
| `Notifier` | `src/notifier.ts` | Dispatches email (nodemailer) and webhook (HTTP POST) notifications |
| `API` | `src/api.ts` | Express REST API for managing notification preferences |
| `Config` | `src/config.ts` | Environment-based configuration |

### Quick Start

```bash
cd notification-service
cp .env.example .env
# Edit .env with your Stellar RPC URL, contract IDs, and SMTP settings
npm install
npm run dev
```

### Configuration

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `STELLAR_RPC_URL` | `https://soroban-testnet.stellar.org` | Soroban RPC endpoint |
| `STELLAR_NETWORK_PASSPHRASE` | Testnet passphrase | Network passphrase |
| `REGISTRY_CONTRACT_ID` | (required) | Deployed `ProjectRegistry` contract ID |
| `VAULT_CONTRACT_ID` | (optional) | Deployed `InvestmentVault` contract ID |
| `DB_PATH` | `./data/notifications.db` | SQLite database path |
| `POLL_INTERVAL_MS` | `30000` | Event polling interval |
| `FROM_EMAIL` | — | Sender email address |
| `SMTP_HOST` | — | SMTP server hostname |
| `SMTP_PORT` | `587` | SMTP port |
| `SMTP_SECURE` | `false` | Use TLS for SMTP |
| `SMTP_USER` | — | SMTP username |
| `SMTP_PASS` | — | SMTP password |
| `API_PORT` | `3000` | REST API port |

### REST API

#### Manage Notification Preferences

**GET `/preferences`** — List all registered preferences.

**GET `/preferences/:address`** — Get preference for a specific investor address.

**PUT `/preferences/:address`** — Create or update a preference.

Request body:
```json
{
  "email": "investor@example.com",
  "webhook_url": "https://my-app.com/heliobond-webhook",
  "enabled": true,
  "min_delta": 5
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `email` | `string` | No | Email address for email notifications |
| `webhook_url` | `string` | No | HTTPS URL for webhook POST notifications |
| `enabled` | `boolean` | No (default: true) | Master toggle for notifications |
| `min_delta` | `number` | No (default: 1) | Minimum absolute score change (0–100) to trigger a notification |

At least one of `email` or `webhook_url` must be provided. Both can be set simultaneously.

**DELETE `/preferences/:address`** — Remove an investor's preferences.

#### Notification History

**GET `/notifications/history`** — List sent notifications, most recent first, paginated.

Query parameters:

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `limit` | `number` | `50` (max `200`) | Maximum number of entries to return |
| `offset` | `number` | `0` | Number of entries to skip |
| `investor_address` | `string` | — | Restrict results to a single investor |

Response body:
```json
{
  "items": [
    {
      "id": 42,
      "investor_address": "GABCD...",
      "project_id": 1,
      "channel": "webhook",
      "ledger": 123456,
      "sent_at": "2026-06-27T12:00:00.000Z"
    }
  ],
  "total": 87,
  "limit": 50,
  "offset": 0
}
```

**GET `/health`** — Health check.

#### Metrics

**GET `/metrics`** — In-memory counters for events processed and notifications sent.

Response body:
```json
{
  "eventsProcessed": 128,
  "notificationsSent": 94
}
```

Only registered when a `Metrics` instance is passed to `createApi`; if none is configured, this route doesn't exist and requests return the default Express 404. As of #336, `Metrics` isn't currently instantiated in the running service at all, so this endpoint is not reachable in the default deployment.

### Webhook Payload

When a score change triggers a webhook notification, the service sends an HTTP POST with the following JSON body:

```json
{
  "event": "score_changed",
  "project_id": 1,
  "old_scores": {
    "credit_quality": 60,
    "green_impact": 40
  },
  "new_scores": {
    "credit_quality": 85,
    "green_impact": 40
  },
  "old_rate_bps": 750,
  "new_rate_bps": 690,
  "investor_address": "G...",
  "timestamp": "2026-06-27T12:00:00.000Z"
}
```

The webhook endpoint should respond with HTTP 200/201 to acknowledge receipt. Retry logic is not yet implemented; the endpoint should be idempotent.

### Building for Production

```bash
cd notification-service
npm run build
npm start
```

### Docker

#### Quick start (ephemeral — not for production)

```bash
cd notification-service
docker build -t heliobond-notification-service .
docker run -p 3000:3000 --env-file .env heliobond-notification-service
```

> **Warning:** the command above stores the database inside the container. Replacing or removing the container permanently loses all ledger checkpoints and notification-dedup history. The service will restart polling from the configured `START_LEDGER` and re-notify investors for every event it re-processes.

#### Persistent storage with Docker Compose (recommended)

Use `notification-service/docker-compose.yml`, which wires a named volume to `/data` and sets `DB_PATH=/data/notifications.db`:

```bash
cd notification-service
cp .env.example .env   # fill in REGISTRY_CONTRACT_ID and other required vars
docker compose up -d
```

The named volume `notification-data` is created automatically on first run and survives `docker compose down`, container rebuilds, and image updates.

#### File ownership

The service runs as the `node` user (uid `1000`, gid `1000`). The `/data` directory inside the container must be writable by that user.

- **Named volume** (default): Docker creates the volume as root, but the container's entrypoint runs as `node`. On first start the directory is writable; on restart it already exists. No action required.
- **Bind mount** (e.g. `./data:/data`): Create the host directory and set ownership before the first run:

  ```bash
  mkdir -p ./data
  sudo chown 1000:1000 ./data
  ```

If the data directory is unwritable, `better-sqlite3` throws `SQLITE_CANTOPEN` and the process exits immediately. The Docker health check (`/health`) will then fail and the container will restart — it will not silently continue with a missing database.

#### Replacing the container while retaining data

```bash
# 1. Pull or build the new image.
docker compose pull          # or: docker compose build

# 2. Recreate the container.  The volume is NOT removed.
docker compose up -d

# 3. Verify that the checkpoint cursor survived the upgrade.
docker compose exec notification-service \
  wget -qO- http://localhost:3000/health
# Expected: {"status":"ok","db":true}

# 4. Confirm the last processed ledger is non-zero (i.e. history was retained).
docker compose exec notification-service \
  node -e "
    const Database = require('better-sqlite3');
    const db = new Database(process.env.DB_PATH || '/data/notifications.db', { readonly: true });
    const row = db.prepare('SELECT MAX(ledger) AS ledger FROM processed_ledgers').get();
    console.log('Last processed ledger:', row.ledger ?? '(none — fresh database)');
  "
```

A non-zero ledger value confirms that the processed-ledger cursor and notification-history rows from the previous container are intact.

#### Backup

SQLite supports online backups with the `.backup` command. Run it against the live database without stopping the service:

```bash
# Copy the database to a timestamped file on the host.
docker compose exec notification-service \
  node -e "
    const Database = require('better-sqlite3');
    const src  = new Database(process.env.DB_PATH || '/data/notifications.db', { readonly: true });
    src.backup('/data/notifications-backup-\$(date +%Y%m%d-%H%M%S).db')
      .then(() => { console.log('Backup complete'); src.close(); })
      .catch((err) => { console.error('Backup failed:', err); process.exit(1); });
  "

# Or copy the named-volume data to the host using a temporary container.
docker run --rm \
  -v notification-service_notification-data:/data \
  -v "$(pwd)/backups:/backups" \
  busybox \
  cp /data/notifications.db /backups/notifications-$(date +%Y%m%d-%H%M%S).db
```

Store backups outside the named volume so they survive an accidental `docker volume rm`.

#### Failure modes

| Situation | Behaviour | Resolution |
|-----------|-----------|------------|
| `DB_PATH` directory does not exist | `SQLITE_CANTOPEN` on startup, process exits | Create the directory and ensure write permission |
| `DB_PATH` directory exists but is unwritable | Same as above | `chown 1000:1000 <dir>` on the host |
| Container replaced without a volume | Fresh database; cursor resets to `START_LEDGER`; investors re-notified | Attach a named volume or bind mount before first run |
| Named volume accidentally removed | Same as above | Restore from backup |
| Disk full | SQLite returns `SQLITE_FULL`; write operations fail; process may exit | Free disk space; the volume can be resized online for most volume drivers |

---

## Investor-Project Index

The notification service maintains an SQLite table `investor_projects` that tracks which investors have holdings in which projects. This index is built by monitoring:

- `Deposit` events from the `InvestmentVault` — identifies active investors
- `ProjectFunded` events — identifies which projects are funded

This index allows the service to route score change notifications to only the relevant investors, rather than broadcasting to all registered users.
