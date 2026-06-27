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

**GET `/health`** — Health check.

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

```bash
cd notification-service
docker build -t heliobond-notification-service .
docker run -p 3000:3000 --env-file .env heliobond-notification-service
```

---

## Investor-Project Index

The notification service maintains an SQLite table `investor_projects` that tracks which investors have holdings in which projects. This index is built by monitoring:

- `Deposit` events from the `InvestmentVault` — identifies active investors
- `ProjectFunded` events — identifies which projects are funded

This index allows the service to route score change notifications to only the relevant investors, rather than broadcasting to all registered users.
