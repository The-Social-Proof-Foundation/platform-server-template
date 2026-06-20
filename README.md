# Platform Server Template (Rust)

Production-ready Rust backend template for MySo social platforms. **MySocial GraphQL** is the read path for on-chain social data (posts, profiles, graph). This server owns **wallet auth**, **waitlist/referrals**, **pgvector recommendations**, **push/email/WS notifications**, and **platform-specific side effects** driven by a gRPC checkpoint stream or inbound social webhooks — not a duplicate social indexer.

## Renaming the template

After forking, run the interactive renamer to replace project-scoped `platform-*` / `platform_*` names with your project name (crates, binary, migrations, topics, docker defaults):

```bash
./scripts/rename_project.sh
```

Chain-domain names (`PLATFORM_ID`, `platform_id` on indexed events) are left unchanged.

## Quick start

```bash
# 1. Start local dependencies
docker compose up -d

# Optional analytics stack (Redpanda + ClickHouse)
docker compose --profile analytics up -d

# Optional monitoring stack (Prometheus + Grafana + DB/Redis exporters)
docker compose --profile monitoring up -d

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
Clients ──► MySocial GraphQL (posts, profiles, graph, engagement reads)
Clients ──► Axum REST + WebSocket (auth, waitlist, recommendations, settings)
                │
                ├──► Postgres (users, settings, content_vectors, notifications)
                ├──► Redis (auth, graph cache, rate limits, presence)
                ├──► MySo gRPC side-effect listener (INDEXER_ENABLED=true)
                │         OR POST /social/events webhook from hosted MySocial
                ├──► APNs + FCM + Resend + WS notifications
                └──► Redpanda outbox poller ──► ClickHouse (optional)
```

| Mode | Env | Behavior |
|------|-----|----------|
| API replica | `INDEXER_ENABLED=false` | REST + WS; reads from `POSTGRES_READ_URL` when set |
| Side-effect worker | `INDEXER_ENABLED=true` | gRPC `subscribe_checkpoints` → notifications + embeddings (no social mirror tables) |

Run **exactly one** side-effect worker instance. Scale API replicas with `INDEXER_ENABLED=false`.

## Crate layout

| Crate | Purpose |
|-------|---------|
| `platform-core` | Config, errors, app state, Prometheus metrics, settings catalog |
| `platform-db` | SQL migrations, Redis helpers, counters, analytics outbox, delivery config |
| `platform-embeddings` | OpenAI `text-embedding-3-large` client (3072 dims) |
| `platform-indexer` | MySo gRPC client, event parsers, side-effect handlers (notifications, embeddings, Redis graph cache) |
| `platform-api` | Axum routes, auth middleware, recommendations, MySocial GraphQL blended feed |
| `platform-notify` | APNs, FCM, Resend, WebSocket hub, delivery pipeline with notification prefs |
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
| `MYSO_GRPC_URL` | MySo fullnode gRPC URL (side-effect worker) |
| `PLATFORM_ID` | On-chain platform object filter for side effects |
| `SOCIAL_WEBHOOK_SECRET` | HMAC secret for `POST /social/events` (hosted MySocial push) |
| `STREAM_WEBHOOK_SECRET` | HMAC secret for `POST /streams/webhook` |
| `REDPANDA_BROKERS` | Enables analytics outbox publishing |
| `CLICKHOUSE_INGEST_ENABLED` | Optional ClickHouse ingest flag |
| `REFERRALS_ENABLED` | Mount `/referrals/*` routes (default `false`) |
| `INVITES_ENABLED` | Mount `/invites/*` routes (default `false`) |
| `WAITLIST_ENABLED` | Waitlist queue, access gating, batch job (default `false`) |
| `WAITLIST_BATCH_ADMISSION_ENABLED` | Scheduled FCFS batch admissions (default `true`) |
| `WAITLIST_INVITE_BYPASS_ENABLED` | Invite codes grant immediate access (default `true`) |
| `INVITE_CIRCULATION_PUBLIC` | Public `GET /waitlist/invites/circulation` tease endpoint |
| `METRICS_ENABLED` | Expose Prometheus metrics on `METRICS_PORT` (default `true`) |
| `METRICS_PORT` | Prometheus scrape port (default `9091`) |
| `METRICS_BIND` | Metrics bind address (default `127.0.0.1`; use `0.0.0.0` with Docker monitoring profile) |
| `OPENAI_API_KEY` | OpenAI API key for content/profile embeddings |
| `OPENAI_EMBEDDING_MODEL` | Embedding model (default `text-embedding-3-large`) |
| `EMBEDDINGS_ENABLED` | Set `false` to skip OpenAI calls in dev (default `true`) |
| `MYSO_GRAPHQL_URL` | MySocial GraphQL endpoint for blended feed following slice |
| `APP_PUBLIC_URL` | Base URL for email verification links |
| `EMAIL_VERIFICATION_ENABLED` | Enable `POST /user/email` verification flow (default `true`) |
| `FCM_SERVER_KEY` | Global FCM fallback (per-platform key in `platform_delivery_config`) |

## SQL migrations

Migrations run automatically on startup. Avoid semicolons inside SQL comments — the runner strips comments before splitting statements, but keeping comments semicolon-free prevents confusion when editing files by hand.

Migration `008_app_features.sql` adds email verification columns on `users`.

## User settings and references

Dynamic key/value preferences live in Postgres table `settings`. Saved items (bookmarks, pinned posts, etc.) live in `user_references`.

### Settings API

| Method | Path | Notes |
|--------|------|-------|
| GET | `/user/settings` | `{ settings: [{ setting_name, setting_value }], blockedCount }` |
| GET | `/user/settings/catalog` | `{ definitions: [...] }` — includes notification preference keys |
| POST | `/user/setting` | Body: `{ settingName, settingValue }` (snake_case also accepted) |
| DELETE | `/user/setting` | Body: `{ settingName }` |

The template ships notification preference keys in `crates/platform-core/src/settings/mod.rs` (`notify.push.enabled`, `notify.mentions`, etc.). Push and email delivery respect these prefs; in-app WebSocket notifications remain always-on.

Add product-specific entries to `SETTING_DEFINITIONS` when your fork defines additional keys:

```rust
pub const SETTING_DEFINITIONS: &[SettingDefinition] = &[
    SettingDefinition {
        key: "theme",
        default_value: Some("system"),
        description: Some("UI theme: light, dark, or system"),
    },
];
```

Use `platform_db::get_setting` / `get_bool_setting(pool, user_id, key, fallback)` or `notification_allowed(pool, user_id, type, channel)` in your routes or notification pipeline.

### References API

| Method | Path | Body |
|--------|------|------|
| GET | `/user/references?type=saved_post&limit=50` | — |
| POST | `/user/reference` | `{ referenceType, referenceKey, metadata? }` |
| DELETE | `/user/reference` | `{ referenceType, referenceKey }` |

Reference types are not enforced — use whatever strings fit your product (`saved_post`, `bookmark`, etc.).

## Optional referrals and invites

Disabled by default. Enable with `REFERRALS_ENABLED=true` and/or `INVITES_ENABLED=true`.

**Referrals** (`crates/platform-db/src/referral.rs`):

- Constants: `REFERRALS_REQUIRED` (default 5), `REFERRAL_MIN_ACCOUNT_AGE_DAYS`
- Hook: `on_referral_threshold_reached` — upserts `referral.reward.claimed` and fires `referral_reward` notification
- Routes: `GET /referrals/stats`, `GET /referrals`, `POST /referrals/record`
- Signup: pass `referrerId` on `POST /user` to record a referral automatically

**Invites** (`crates/platform-db/src/invite.rs`):

- Constants: `MAX_INVITES_PER_USER` (10), `INVITE_EXPIRY_DAYS` (7), `MAX_ACCEPTED_INVITES_PER_USER` (1)
- Hook: `on_invite_accepted` — fill in engagement rewards
- Routes: `POST /invites`, `GET /invites`, `POST /invites/accept`, `GET /invites/:code` (public preview)

**Waitlist / early access** (`crates/platform-db/src/waitlist.rs`):

- Env: `WAITLIST_ENABLED`, `WAITLIST_BATCH_ADMISSION_ENABLED`, `WAITLIST_INVITE_BYPASS_ENABLED`, `INVITE_CIRCULATION_PUBLIC`
- **Waitlist open** (signup, referrals, queue bumps) stays on when `WAITLIST_ENABLED=true`
- **Batch admission** (scheduled FCFS drip every 12h/24h) is controlled by `WAITLIST_BATCH_ADMISSION_ENABLED` + admin pause
- **Invite bypass** (immediate access via invite code) is controlled by `WAITLIST_INVITE_BYPASS_ENABLED`
- While waiting: JWT works but only waitlist, referral, auth refresh, and invite preview routes are allowed
- Routes: `GET /waitlist/status`, `GET /waitlist/invites/circulation` (public aggregate), `GET /referrals/code`
- Admin (header `x-internal-api-key`): `GET|POST /waitlist/admin/config`, `POST /waitlist/admin/pause|resume|run-batch`, `POST /waitlist/admin/users/grant-access`, `POST /waitlist/admin/users/:id/approve|invites`

**Admin grant access** (`POST /waitlist/admin/users/grant-access`) — recommended one-shot operator path for investors or VIP-style access:

- Body: `{ "userId" | "walletAddress", "mintInvites": N }` (exactly one identifier required)
- Approves the **target user** off the waitlist, enables invite creation on their profile, and mints `N` invite codes **on their account** (not the caller's)
- Sends `waitlist_approved` notification to the target user via WS → APNs → email
- Response includes the target user's profile and minted codes for operator audit only
- Signup: optional `referralCode`, `inviteCode` (invite wins when bypass enabled)
- Notifications: `waitlist_joined`, `waitlist_bump`, `referral_claimed`, `invite_accepted`, `waitlist_approved`

| Scenario | Batch admission | Invite bypass |
|----------|-----------------|---------------|
| Controlled launch | on | on |
| Viral growth (good) | off | on |
| Incident / capacity | off + pause | off |

## Client integration

### Wallet auth (universal MySocial wallet + JWT)

Platform-server uses **complementary** wallet login and JWT session tokens:

1. `POST /user/request-signature` with `{ "publicKey": "0x..." }` — returns a nonce and EIP-191 sign message
2. Sign the returned message with the wallet (EIP-191). If your universal MySocial wallet uses a different signing scheme, extend `verify_wallet_signature` in `crates/platform-api/src/auth/wallet.rs`
3. `POST /user` (signup) or `POST /user/login` with `publicKey` + `signature`
4. Server verifies the signature, maps `public_key` / `wallet_address` / `chain_address` to the same normalized address, and returns JWT access + refresh tokens
5. Use `Authorization: Bearer <accessToken>` on protected routes
6. Refresh via `POST /user/refreshSession` — re-sign with the wallet only when refresh expires or you need a fresh nonce

**MySocial GraphQL** handles social reads. **Wallet-signed on-chain transactions** handle social writes. This server does not duplicate social mirror tables.

Sign message format (see `generate_login_message`):

```text
Sign in to Platform Server

Address: 0x...
Nonce: <uuid>
```

### On-chain writes and reads

Social mutations (create post, follow, like, comment, block) are **not** exposed as REST writes on this server. Clients submit MySo transactions on-chain.

**Reads** (feeds, profiles, posts, reactions, tips, social graph) come from **hosted MySocial GraphQL** — query your `MYSO_GRAPHQL_URL` endpoint directly from client apps. This server does not mirror social tables locally and does not poll GraphQL.

### Platform-server API surface

- `GET /recommendations/feed` — pgvector timeline feed (chain post IDs)
- `GET /recommendations/blended-feed?chronoLimit=50&discoverLimit=50` — MySocial following posts + vector discovery merge
- `GET /recommendations/friends` — profile embedding suggestions
- `POST /interactions` — single engagement event (watch/open/skip/share)
- `POST /interactions/batch` — up to 50 events per request
- `POST /user/email` — set email and send verification link (requires `APP_PUBLIC_URL` + Resend)
- `GET /user/email/verify?token=...` — confirm email verification
- `POST /recommendations/admin/content/{contentId}/moderate` — moderation override (internal key)
- `GET /recommendations/indexer/metrics` — side-effect worker lag/throughput (internal key)
- Wallet auth, waitlist, referrals, invites, settings, references — see routes above

### Side-effect delivery (notifications + embeddings)

Platform-specific side effects are **push-driven**, never GraphQL polling:

1. **Path A (default):** enable `INDEXER_ENABLED=true` on one worker. The gRPC `subscribe_checkpoints` stream triggers notifications, `content_vectors` inserts, OpenAI embeddings (when configured), and Redis follow/block cache updates.
2. **Path B (hosted):** MySocial infrastructure POSTs parsed chain events to `POST /social/events` with header `x-signature` (HMAC-SHA256 of raw body using `SOCIAL_WEBHOOK_SECRET`). Same handlers as Path A.

Follow/block graph state for recommendation filters lives in Redis (`follows:{wallet}`, `blocked:{wallet}`), populated from on-chain Follow/Block events.

### WebSocket protocol

Connect to `GET /ws?token=<accessToken>`.

Outbound message shape:

```json
{
  "type": "notification | stream_event | refresh_token",
  "...": "payload fields"
}
```

Presence is tracked in Redis (`user:{id}:isOnline`). Online users receive in-app notifications over WS; offline users fall back to APNs (iOS) or FCM (Android), then verified-email Resend when configured. Push and email respect user notification preferences from `/user/settings`.

## Social event webhooks

`POST /social/events` accepts typed MySocial chain events for platforms that receive push delivery from hosted MySocial instead of running their own gRPC listener.

- Header: `x-signature` — HMAC-SHA256 hex digest of raw body using `SOCIAL_WEBHOOK_SECRET`
- Body: `{ "event": <ParsedChainEvent>, "tx_digest": "...", "checkpoint_seq": 123 }`
- Runs the same side-effect handlers as the gRPC worker (notifications, `content_vectors`, Redis graph cache)

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
| `platform.chain.events` | Side-effect worker handlers |
| `platform.api.logs` | HTTP middleware (stub) |
| `platform.notifications` | Delivery outcomes |
| `platform.stream.events` | Stream webhooks |

ClickHouse DDL lives in [`docker/clickhouse-init.sql`](docker/clickhouse-init.sql). Set `CLICKHOUSE_INGEST_ENABLED=true` to opt into embedded ingest (stub logs readiness today; wire a consumer as a follow-up).

## Monitoring

Prometheus metrics are exposed on a **separate port** from the public API (default `http://127.0.0.1:9091/metrics`). JSON debug endpoints remain at `/performance/metrics` and `/recommendations/indexer/metrics`.

```bash
# Start Postgres + Redis, then the monitoring stack
docker compose up -d
docker compose --profile monitoring up -d

# Allow Prometheus (in Docker) to scrape the host-run server
export METRICS_BIND=0.0.0.0

cargo run -p platform-server
```

| URL | Service |
|-----|---------|
| http://localhost:3000 | Grafana (anonymous admin — dev only) |
| http://localhost:9090 | Prometheus |
| http://localhost:9091/metrics | Platform server metrics |

Pre-provisioned dashboard: **Platform Overview** (HTTP, indexer, WebSocket, notifications, analytics outbox, Postgres/Redis exporters).

Production: scrape metrics from a private network or Grafana Cloud. Do not expose unauthenticated `/metrics` on the public internet.

## Platform delivery config

Per-platform APNs, FCM, and Resend credentials are stored in Postgres table `platform_delivery_config` (see migration `004_delivery_and_analytics.sql`). The notify pipeline loads rows keyed by `PLATFORM_ID` with a 300s Redis cache (`delivery:{platform_id}`). Global env vars (`APNS_*`, `FCM_SERVER_KEY`, `RESEND_*`) serve as fallbacks when per-platform columns are null.

## Forking guide

Customize these areas per platform:

| Area | Files |
|------|-------|
| Event parsers | `crates/platform-indexer/src/parsers/` |
| Indexer handlers | `crates/platform-indexer/src/handlers/` |
| Platform filter | `crates/platform-indexer/src/filters/` |
| REST routes | `crates/platform-api/src/routes/` |
| User settings catalog | `crates/platform-core/src/settings/mod.rs` |
| Setting lookups | `crates/platform-db/src/settings.rs` |
| Referral / invite hooks | `crates/platform-db/src/referral.rs`, `invite.rs` |
| Waitlist queue + batch job | `crates/platform-db/src/waitlist.rs`, `crates/platform-api/src/waitlist_processor.rs` |
| Recommendation SQL | `crates/platform-api/src/recommend/` |
| Env defaults | `.env.example`, `crates/platform-core/src/config.rs` |
| Embeddings | `crates/platform-embeddings/`, `platform-db/src/embeddings.rs` |
| Delivery config cache | `platform-db/src/delivery.rs`, `platform-notify/src/service.rs` |
| MySocial GraphQL client | `crates/platform-api/src/mysocial/` |

OpenAI embeddings run via `platform-embeddings` when `OPENAI_API_KEY` is set. Set `EMBEDDINGS_ENABLED=false` to skip API calls during local development.

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
- REST endpoints for create post / like / comment / follow / block (use on-chain txs + MySocial GraphQL)
- ClickHouse consumer implementation
- Full-text search index
