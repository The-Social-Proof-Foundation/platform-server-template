# Platform Server Template (Rust)

Production-ready Rust backend template for MySo social platforms. Mirrors the DripDrop architecture: **on-chain social writes**, **Postgres + pgvector reads**, **Redis** caching/auth, **gRPC checkpoint indexer**, **APNs + Resend + WebSocket** notifications, and an optional **Redpanda → ClickHouse** analytics pipeline.

## Renaming the template

After forking, run the interactive renamer to replace project-scoped `platform-*` / `platform_*` names with your project name (crates, binary, migrations, topics, docker defaults):

```bash
./scripts/rename_project.sh
```

Chain-domain names (`PLATFORM_ID`, `platform_id` columns, `platforms` table) are left unchanged.

## Quick start

```bash
# 1. Start local dependencies
docker compose up -d

# Optional analytics stack (Redpanda + ClickHouse)
docker compose --profile analytics up -d

# 2. Configure environment
cp .env.example .env

# 3. Run the server (applies SQL migrations on startup)
cargo run -p platform-server
```

Smoke test (server must be running):

```bash
./scripts/smoke_test.sh
```

## Architecture

```
Clients ──► Axum REST + WebSocket
                │
                ├──► Postgres (primary writes / indexer)
                ├──► Postgres read pool (feeds, posts)
                ├──► Redis (nonces, refresh tokens, rate limits, presence, counters)
                ├──► MySo gRPC indexer (optional, INDEXER_ENABLED=true)
                ├──► APNs + Resend + WS notifications
                └──► Redpanda outbox poller ──► ClickHouse (optional)
```

| Mode | Env | Behavior |
|------|-----|----------|
| API replica | `INDEXER_ENABLED=false` | REST + WS; reads from `POSTGRES_READ_URL` when set |
| Indexer worker | `INDEXER_ENABLED=true` | Runs gRPC checkpoint stream + writes to primary |

Run **exactly one** indexer instance. Scale API replicas with `INDEXER_ENABLED=false`.

## Crate layout

| Crate | Purpose |
|-------|---------|
| `platform-core` | Config, errors, app state, indexer metrics |
| `platform-db` | SQL migrations, Redis helpers, counters, analytics outbox |
| `platform-indexer` | MySo gRPC client, event parsers, handlers, side effects |
| `platform-api` | Axum routes, auth middleware, recommendations |
| `platform-notify` | APNs, Resend, WebSocket hub, delivery pipeline |
| `platform-analytics` | Redpanda producer, outbox poller |
| `platform-server` | Binary wiring + graceful shutdown |

## Environment variables

See [`.env.example`](.env.example) for the full list. Key variables:

| Variable | Purpose |
|----------|---------|
| `POSTGRES_URL` | Primary Postgres connection |
| `POSTGRES_READ_URL` | Read replica (falls back to primary) |
| `REDIS_URL` | Redis connection |
| `JWT_SECRET` / `JWT_REFRESH_SECRET` | Wallet auth tokens |
| `INTERNAL_API_KEY` | Protects `/performance/*` and `/recommendations/indexer/metrics` |
| `INDEXER_ENABLED` | Enable gRPC indexer on one instance |
| `MYSO_GRPC_URL` | MySo fullnode gRPC URL |
| `PLATFORM_ID` | On-chain platform object filter |
| `STREAM_WEBHOOK_SECRET` | HMAC secret for `POST /streams/webhook` |
| `REDPANDA_BROKERS` | Enables analytics outbox publishing |
| `CLICKHOUSE_INGEST_ENABLED` | Optional ClickHouse ingest flag |

## Client integration

### Wallet auth

1. `POST /user/request-signature` with `{ "publicKey": "0x..." }`
2. Sign the returned message with the wallet (EIP-191)
3. `POST /user` or `POST /user/login` with `publicKey` + `signature`
4. Use `Authorization: Bearer <accessToken>` on protected routes
5. Refresh via `POST /user/refreshSession`

### On-chain writes

Social mutations (create post, like, comment) are **not** exposed as REST writes. Clients submit MySo transactions; the indexer persists results to Postgres.

### Read API surface

- `GET /post/feed/following` — chronological follows feed
- `GET /post/:user_id` — posts by wallet address
- `GET /post/:post_id/data` — single post
- `GET /recommendations/feed` — pgvector timeline feed
- `GET /recommendations/friends` — profile embedding suggestions
- `GET /recommendations/indexer/metrics` — indexer lag/throughput (internal key)

### WebSocket protocol

Connect to `GET /ws?token=<accessToken>`.

Outbound message shape:

```json
{
  "type": "notification | stream_event | refresh_token",
  "...": "payload fields"
}
```

Presence is tracked in Redis (`user:{id}:isOnline`). Online users receive in-app notifications over WS; offline users fall back to APNs, then Resend email when configured.

## Stream webhooks

`POST /streams/webhook` accepts generic JSON payloads from your platform's event source.

- Header: `x-stream-signature` — HMAC-SHA256 hex digest of raw body using `STREAM_WEBHOOK_SECRET`
- Persists to `analytics_outbox` and publishes to Redpanda topic `platform.stream.events`
- Fans out to connected WebSocket clients as `{ "type": "stream_event", ... }`

Example payload:

```json
{
  "userId": "uuid-or-wallet",
  "event": "price_alert",
  "data": { "symbol": "MYSO", "price": 1.23 }
}
```

## Analytics pipeline

When `REDPANDA_BROKERS` is set, a background task polls `analytics_outbox` and publishes to topics:

| Topic | Source |
|-------|--------|
| `platform.chain.events` | Indexer handlers |
| `platform.api.logs` | HTTP middleware (stub) |
| `platform.notifications` | Delivery outcomes |
| `platform.stream.events` | Stream webhooks |

ClickHouse DDL lives in [`docker/clickhouse-init.sql`](docker/clickhouse-init.sql). Set `CLICKHOUSE_INGEST_ENABLED=true` to opt into embedded ingest (stub logs readiness today; wire a consumer as a follow-up).

## Platform delivery config

Per-platform APNs and Resend credentials are stored in Postgres table `platform_delivery_config` (see migration `004_delivery_and_analytics.sql`). Insert rows keyed by your `PLATFORM_ID` for production push/email delivery.

## Forking guide

Customize these areas per platform:

| Area | Files |
|------|-------|
| Event parsers | `crates/platform-indexer/src/parsers/` |
| Indexer handlers | `crates/platform-indexer/src/handlers/` |
| Platform filter | `crates/platform-indexer/src/filters/` |
| REST routes | `crates/platform-api/src/routes/` |
| Recommendation SQL | `crates/platform-api/src/recommend/` |
| Env defaults | `.env.example`, `crates/platform-core/src/config.rs` |
| Delivery config | `platform_delivery_config` rows in Postgres |

Embedding generation is stubbed (schema + placeholder rows). Wire OpenAI or your embedding provider as a follow-up — same as DripDrop.

## Deploy

Build from the **ProjectYZ monorepo root** (the Dockerfile copies `myso-rust-sdk` for the path dependency):

```bash
cd ..
docker build -f platform-server-template/docker/Dockerfile -t platform-server .
```

Railway: [`railway.toml`](railway.toml) uses `/health` for health checks.

Mount APNs `.p8` key via `APNS_KEY_PATH` or inject at deploy time (see DripDrop Dockerfile pattern).

## Development

```bash
cargo build
cargo test
cargo run -p platform-server
```

Parser unit tests live in `platform-indexer` (`post_events`).

## Out of scope (v1)

- Neo4j social graph
- Moralis / CoinGecko wallet routes
- REST endpoints for create post / like / comment
- FCM/Android push (APNs + Resend only)
- OpenAI embedding pipeline (schema stub only)
