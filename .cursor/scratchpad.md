# Platform Server Template — Scratchpad

## Background and Motivation

Greenfield Rust backend template with dynamic user settings, user references, and optional referral/invite engagement modules.

## Project Status Board

- [x] Migration 005 — `user_references` table
- [x] Migration 006 — `user_referrals`, `user_invites` tables
- [x] Blank-slate `SETTING_DEFINITIONS` in `platform-core`
- [x] `platform-db` settings, user_references, referral, invite modules
- [x] Settings/references API (DripDrop/iOS parity)
- [x] Optional `/referrals` and `/invites` routes (env-gated)
- [x] README fork guide for settings and engagement hooks
- [x] `cargo build` + `cargo test` passing

## Executor's Feedback

Template ships no predefined setting keys. Forks populate `SETTING_DEFINITIONS` and wire `get_bool_setting` in notify/recs as needed.

## Lessons

- Setting catalog is intentionally empty — product defaults belong in fork code.
- Referral recording on signup uses optional `referrerId` when `REFERRALS_ENABLED=true`.
