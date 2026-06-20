# Platform Server Template — Scratchpad

## Background and Motivation

Greenfield Rust backend template mirroring DripDrop production architecture with Resend email, generic stream webhooks, and Redpanda→ClickHouse analytics. Postgres-only social graph.

## Key Challenges and Analysis

- **Axum 0.8 upgrade** required for tonic/myso-rpc compatibility; stateful routers use `Extension<SharedApiState>` + `Router<()>` for `axum::serve`.
- **Middleware Send bounds**: rate-limit helpers must not hold `&Request` across `.await`; extract client IP synchronously first.
- **IndexerMetrics** moved to `platform-core` to avoid axum version conflict from `platform-indexer` in `platform-api`.

## High-Level Task Breakdown

1. Workspace scaffold — done
2. SQL migrations + runner — done
3. Auth + user REST API — done
4. gRPC indexer + parser tests — done
5. Read APIs + pgvector recommendations — done
6. Notifications (APNs, Resend, WS) — done
7. Streams + analytics outbox/Redpanda — done
8. Deploy docs (Docker, Railway, README, smoke test) — done

## Project Status Board

- [x] Cargo workspace + crates
- [x] docker-compose (Postgres+pgvector, Redis, Redpanda, ClickHouse profile)
- [x] `.env.example`
- [x] Full SQL migrations (001–004)
- [x] Wallet auth + user routes + middleware
- [x] gRPC indexer with post.move parsers
- [x] Recommendation engines + Redis cache
- [x] Notification pipeline
- [x] Stream webhooks + analytics outbox
- [x] Dockerfile, railway.toml, README, smoke test
- [x] `cargo build` + `cargo test` passing

## Executor's Feedback or Assistance Requests

None — template compiles and tests pass locally.

## Lessons

- Axum middleware futures must not borrow `Request` across await points (breaks `Send`).
- `from_fn_with_state` + final `with_state` produces `Router<S>` incompatible with `axum::serve`; use `Extension` layer for app state when serving directly.
- Pin axum 0.8 across workspace when depending on myso-rpc/tonic.
