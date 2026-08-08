# hub — operating rules

`hub` is the workspace's comms channel: inbound items from **email, GitHub,
project devlogs and chat** land in one normalized store, a bounded `claude -p`
call triages each one, and the result is either an answer sent back on the same
channel, a brief for the owner, or a code change on a branch.

Read `README.md` for the architecture and the CLI. Read `PLAN.md` for what is
built vs. pending. This file is the rules for working ON hub.

## Stack

- **Rust 2021**, crate in `rust/`, two binaries: `hub` (CLI) and `hubd` (loop).
  Deliberately **synchronous** — this process spends its life waiting on `gh`,
  `claude`, and a 20s Telegram long-poll, so an async runtime would add moving
  parts without removing a single wait. **No `unsafe` anywhere.**
- Deps match mailler so the workspace keeps one toolchain: `rusqlite` (bundled),
  `reqwest` (blocking + rustls), `serde`/`serde_json`, `clap`, `regex`, `chrono`,
  `anyhow`. All present in the local cargo cache → `cargo build --offline` works.
- Store: `data/hub.sqlite` (WAL). Schema in `rust/src/db.rs`, version in
  `SCHEMA_VERSION`. The schema is byte-compatible with the archived Node
  prototype (`legacy-node/`) — do not "tidy" column names without a migration.
- Tests: `cd rust && cargo test --offline` → 50 integration tests, 0 warnings.
- `./hub …` is a wrapper that builds on first use then execs `rust/target/release/hub`.

## Non-negotiables

1. **Untrusted content never becomes instructions.** Inbound bodies are fenced
   in `<<<INBOUND … INBOUND>>>` and the triage call runs with `--tools ""`. If
   you add a stage that gives the model tools, it must be reachable only after a
   human approval step, and its tool denylist stays in `rust/src/act.rs`.
2. **No silent failure.** Every error path logs (`rust/src/logging.rs`) *and*
   leaves a row: `runs.err` for adapters, `outbox.last_error` + `dead_letter` for
   sends, `messages.last_error` for triage. An `Err` mapped to a default value
   without a log line is a bug — same rule as a swallowed `catch {}`.
3. **Credentials come from env vars only.** The config file holds the *name* of
   the env var (`token_env`, `api_key_env`), never the value. A missing secret
   means SKIP-WITH-LOG (`adapters::Skip`), not a crash and not a silent no-op.
4. **Nothing leaves the machine without passing policy.** `rust/src/policy.rs` is
   the only place that decides send-vs-ask, `rust/src/redaction.rs` is the last
   gate before an outbound auto-reply. Untrusted sender ⇒ tier L0. deploy / merge
   / force-push / data-deletion / secret-rotation ⇒ always human.
5. **Cursors advance only after the messages they cover are committed.** A crash
   must re-poll, never skip.
6. **Cost is a feature.** One triage call is $0.05–$0.11 (measured 2026-07-26).
   Coalescing, `max_triage_per_cycle`, and `--max-budget-usd` exist for that
   reason; do not remove them to "simplify".
7. **One approve path.** CLI, Telegram, the console and the tfl5 board all go
   through `pipeline::{approve_decision, reject_decision, close_message,
   reply_to_message}` — the board's buttons send the same `/approve` `/close`
   text a person types. Never re-implement send-and-bookkeeping in a surface.
   A slash command is an ORDER, never a message: the poller AND `live.rs` must
   both `parse_command` before ingesting (missing that cost $0.18 on 08-07).
   Verbs: approve · reject · close · reply · ingest · run · doctor · set · help.
8. **The web UI is a privileged surface.** Loopback by default, `x-hub-token` on
   every `/api/*` call, config writes validated + backed up + temp-renamed. If
   you add an endpoint, add the token check; config fields must round-trip
   through `Config` (never free-form JSON onto disk). A non-loopback `web.bind`
   **must** carry a password — enforced in `config::validate` AND `web::serve`.
9. **Unattended means bounded.** `hubd` runs on its own with money attached, so
   `daily_budget_usd` stops triage at the ceiling and says so once per day. Any
   new spending path must be counted the same way, and stopping must stay loud.
10. **Secrets for the daemon come from `hub.env`** (chmod 600), never the plist,
    never the config. Log key NAMES only. The real environment always wins.

## When you change something

- Adapter contract: `poll(cfg, cursors[, workspace_root]) -> Result<PollResult>`
  plus optional `health(cfg)` / `send(cfg, target, subject, body)`. Normalizers
  must be pure and unit-tested against a **captured real payload** (see
  `rust/tests/fixtures/notifications.real.json`).
- `external_id` must embed whatever makes an item "new again" (GitHub embeds
  `updated_at`), because dedupe is `UNIQUE(source, external_id)`.
- Adding a policy rule ⇒ add the matching case to `rust/tests/policy.rs`. The
  invariant tests (untrusted caps at L0, tripwire outranks confidence, deploy is
  human-only) are load-bearing; do not relax them to make a feature fit.
- Touched the send path? Re-run a real cycle (`./hub once`) + `./hub status` —
  green unit tests are necessary, not sufficient.
- Changed the portal snapshot shape or a chat verb? Rebuild **release** and
  restart `hubd` in the same pass: the running binary is the consumer, and a
  stale one silently overwrites the new shape (twice on 2026-08-07).

## Project layout

```
hub                     wrapper script → rust/target/release/hub
hub.config.json         config (no secrets — only env var NAMES)
rust/src/main.rs        CLI: doctor init once ingest triage flush inbox show say
                        approve reject close reply act status
rust/src/bin/hubd.rs    daemon loop (pid lock, exponential backoff)
rust/src/{config,db}.rs config + validation + secret_from_env() · schema and every query
rust/src/pipeline.rs    ingest → triage → policy → outbox, one cycle · the channel verbs
rust/src/triage.rs      the bounded claude -p call + prompt + injection tripwire
rust/src/policy.rs      routing, trust, tiers, send-vs-ask
rust/src/{redaction,outbound}.rs  outbound leak scan · dispatch + retry + dead-letter
rust/src/act.rs         approved code change, in a git worktree, on a branch
rust/src/web.rs         local web console (axum, 127.0.0.1 + per-boot token)
rust/src/portal.rs      read-only snapshot pushed to tfl5 (docs, not files — see its header)
rust/assets/            ui.html + vendored echarts.min.js (embedded in the binary)
rust/src/adapters/      github · devlog · email (mailler) · telegram · tfl5 (chat)
rust/tests/             integration tests + captured real fixture
fe/                     chat FE + board tab, shipped to tfl5 as an app bundle
fe-deploy.mjs           zip → Releases → Activate via the console UI, then verifies the served bytes
fe-*.mjs                Playwright over the DEPLOYED bundle: -smoke (chat), -board (panels),
                        -command + -config (button → chat verb → state/file changed), -denied, -watch
console-acl.mjs         grant/revoke app access through the tfl5 console UI
ui-smoke.mjs            Playwright headless check of the web UI (0 console errors)
hub.env(.example)       secrets for launchd runs — chmod 600, gitignored
deploy/*.plist          launchd unit · legacy-node/ archived prototype (port oracle)
```
