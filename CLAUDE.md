# hub — operating rules

`hub` puts the **Claude CLI sessions running on this Mac** on a phone. One chat
room on tfl5 carries ORDERS (`/session`, `/ask`, `/new`, `/tell`, `/stop`,
`/handover`); a read-only snapshot travels the other way so the page can show
what every session is doing. Nothing here reads mail, and nothing here spends
money unless the owner presses a button.

Read `README.md` for the architecture and the CLI. Read `PLAN.md` for what is
built vs. pending. This file is the rules for working ON hub.

## What hub is NOT (2026-08-08)

hub was an inbox: GitHub notifications, project devlogs, email and Telegram fed
one queue, a bounded `claude -p` call triaged every item, and a policy engine
decided send-vs-ask. **That product is deleted** — adapters, triage, policy,
redaction of outbound replies, the outbox, the act stage, the local web console,
the inbox tab on the phone, and every number with a `$` in front of it.

Three measurements decided it, and they are worth keeping because they answer
"can we bring a bit of it back?" with numbers instead of taste:

- **65% of what it carried was CI noise** — 117 of 180 items were GitHub CI
  notifications, and they cost $5.89 of $9.12 total spend.
- **$2.24 of one day's $2.98 triage bill** belonged to the github and devlog
  branches *after they had already been deleted* — the ceiling built to contain
  that spending was mostly the ghost of a dead product, and it ended up standing
  between the owner and his own machine.
- The thing he actually wanted — drive a session from a phone — had run **once**.

Hà, three times in one evening: *"bỏ hết github rồi sao vẫn trần chuồng gì thế"*
· *"sao vẫn nhắc tới tiền vậy, đã bảo xóa hết github rồi mà"* · *"đã bảo xóa
hoàn toàn rồi cơ mà"*. **Before adding anything, ask: does this help him watch
or drive a session from a phone?** If not, it does not belong here.

## Stack

- **Rust 2021**, crate in `rust/`, two binaries: `hub` (CLI) and `hubd` (loop).
  Deliberately **synchronous** — this process spends its life waiting on
  `claude` and a 20s tfl5 long-poll, so an async runtime would add moving parts
  without removing a single wait. **No `unsafe` anywhere.**
- Deps: `rusqlite` (bundled), `reqwest` (blocking + rustls), `serde`/`serde_json`,
  `clap`, `regex`, `chrono`, `anyhow`, `tungstenite`. All in the local cargo
  cache → `cargo build --offline` works.
- Store: `data/hub.sqlite` (WAL). Three tables — `runs`, `cursors`, `spend`.
  An existing file still HAS the four inbox tables and their rows; nothing drops
  them, and no query can see them.
- Tests: `cd rust && cargo test --offline` → 67 tests, 0 warnings.
- `./hub …` is a wrapper that builds on first use then execs `rust/target/release/hub`.

## Non-negotiables

1. **Nothing on this Mac runs unattended without a wall.** `/new` and `/tell`
   start a `claude` process with the owner's own shell, from one sentence typed
   on a phone. They run behind `sessions::DENIED_TOOLS`: no push/merge/reset, no
   ssh/scp/rsync/sudo/rm/docker/launchctl/`*deploy*`, no WebFetch/WebSearch.
   Writes inside the working tree stay allowed — that is the work. Never widen
   this list to make a feature fit.
2. **Anything hub runs on a live session runs on a FORK, read-only.** `/ask` and
   `/handover` go through `sessions::fork_call`: `--fork-session` so the original
   transcript is untouched, and `FORK_TOOLS` (`Read,Grep,Glob`) so a question
   typed on a phone has no hand to write with. Measured 2026-08-08: `--tools ""`
   breaks on tool-heavy history, and `--disallowedTools` without an allowlist
   loads the full tool schema ($0.2185 for one sentence). The UC's real
   acceptance is the ORIGINAL file: same bytes, same mtime, same `last_activity`.
   A correct answer that added a turn to the live session is a FAILED UC.
3. **No silent failure.** Every error path logs (`rust/src/logging.rs`) *and*
   leaves a row where one exists (`runs.err`). An `Err` mapped to a default
   without a log line is a bug — same rule as a swallowed `catch {}`.
4. **Credentials come from env vars only.** The config holds the *name* of the
   env var (`user_env`, `password_env`), never the value. A missing secret means
   SKIP-WITH-LOG (`adapters::Skip`), not a crash and not a silent no-op. Secrets
   for the daemon come from `hub.env` (chmod 600), never the plist, never the
   config. Log key NAMES only. The real environment always wins.
5. **Nothing about a session leaves this Mac unscanned.** `sessions::preview_risk`
   runs every transcript preview through `redaction::leak_scan` before it can
   reach the snapshot — the first real run of `hub sessions` printed a session
   whose latest turn stated a login password in plain text. The snapshot lands in
   a doc on a server; that is "leaving the machine".
6. **Cursors advance only after the orders they cover have run.** A crash must
   re-poll, never skip.
7. **The room takes ORDERS from the owner only.** `tfl5::parse_command` checks
   `trust.tfl5_user_tids` first; anyone else typing `/new` is just typing text.
   Being in the room is tfl5's decision; driving this Mac is the owner's.
   Verbs: session · new · ask · tell · stop · handover · project · ingest · run ·
   doctor · set · help. A verb that already answered must end its arm with
   `Some(ack)` — the fall-through that used to exist logged "Không tìm thấy
   decision #0" as the reply for every `/session` and `/ask` ever issued.
8. **hub consumes nothing on its own.** There is no triage, so a cycle costs
   nothing; the only calls are `/ask`, `/handover`, `/new`, `/tell` — a person
   pressing a button, at the same cost as typing it in the terminal. They are
   **counted in `spend`, never refused**.
   **What that cost IS, exactly** (settled 2026-08-09, after this file said
   "money" once too often): this Mac is on **Max** — `claude auth status` →
   `subscriptionType: max`. Nothing is invoiced per call; a call spends **plan
   quota**. The `total_cost_usd` the CLI hands back is computed at API LIST
   price, so read every `$` in this repo as a **size gauge** — "how big was that
   call" — never as a bill. The per-call cap still bites, because the CLI
   computes that same number internally regardless of subscription. A daily ceiling on them was built and
   thrown out the same day (2026-08-08). The per-call cap stays and is
   **measured, not guessed**: `sessions::fork_cost_estimate` sizes it from the
   transcript (`USD_PER_MB`, from a real 0.986 MB → $1.72), because a flat cap
   smaller than the load cost means paying for a call that dies anyway.
9. **No money on the screen.** `spend` records silently so the question can be
   ANSWERED if it is ever asked; it stops being asked on every screen. The
   snapshot carries no `owner_spend`, no `owner_budget`, no `cost_days`, no
   `budget`, and `cost_usd` is `#[serde(skip_serializing)]` on `Handover`,
   `Aside` and `Told`. `portal.rs` and `fe-board-uc.mjs` both assert **absence** —
   this grew back once already (ceiling → price tag).
10. **`claude` CLI facts that cost a real run to learn.** Do not re-derive them:
    `--bg` conflicts with `-p`, so the prompt is POSITIONAL — and
    `--disallowedTools` is VARIADIC, so a prompt placed after it is eaten as one
    more pattern and the agent comes up with no task at all. `claude stop` takes
    the SHORT id (8 chars); a full uuid answers "No job matching". Resuming a
    LIVE background agent is refused outright — stop it first, then `--resume`
    appends to the same session (same id, transcript grows). There is no
    primitive for typing into a running session, so UC-S05b level 1 has no path.
    And a background session opened anywhere under this workspace comes up
    `state: "blocked"` on an interactive MCP-approval dialog it can never
    answer; `sessions::start_background` watches for that, stops it, and reports
    the one-time fix rather than claiming success.
11. **The phone page is the only UI.** There is no local console any more. If you
    add a surface, it must go through the same room commands — one path, one set
    of books.
12. **macOS facts that cost a night to learn** (2026-08-10). Typing into a live
    session goes through Terminal's own `do script`, NOT `System Events keystroke`
    — the latter is refused outright (*"osascript is not allowed to send
    keystrokes (1002)"*) and no amount of granting Accessibility fixes it,
    because the process asking is `/usr/bin/osascript`, a system binary. `do
    script` needs only **Automation**, which hub already has. It **always
    appends a newline** and that cannot be turned off: fine for `claude`'s
    prompt box, but it means an arrow key both MOVES and CONFIRMS, so hub
    refuses to send arrows while a choice dialog is up and asks for the number
    instead. And when the session is mid-run, `claude` **queues** the text
    rather than showing it — so a reply must read the screen back and say WHERE
    the text landed. `osascript` returning 0 proves only that bytes reached the
    tab.
    **Autostart survives rebuilds now — and here is the whole mechanism, because
    two of the three obvious ways to do it are wrong** (measured 2026-08-10).
    TCC pins a grant to a binary's *designated requirement*. `cargo` ad-hoc-signs
    everything it links, and an ad-hoc DR is `cdhash H"…"` — a hash of the bytes,
    so every build is a different program to macOS. Signing with a certificate
    changes the DR to `identifier "com.dipgle.hubd" and certificate root = H"…"`,
    which is an *identity*: proven by two builds with different bytes
    (`6e9f7db7…` → `bb381cfe…`) carrying the same DR. The cert is self-signed,
    lives in the login keychain, and is deliberately **not** trusted — `codesign`
    signs happily with an untrusted identity and TCC matches on the requirement,
    so nothing here needs an admin password.
    - **Signing `target/release/hubd` in place does not hold.** The next
      `cargo test --release` or `cargo clippy --all-targets` relinks and stamps
      its own ad-hoc signature over it, silently. Caught only because `hubd`
      prints `hubd_signature` at boot and it read `adhoc` twenty minutes after
      being signed `cert`. So launchd runs an **installed copy** at
      `~/Library/Application Support/hub/bin/hubd`, out of cargo's reach; put it
      there with `deploy/install.sh`, never by hand.
    - **That split introduces its own silent failure** — build, test green,
      deploy the page, and the daemon is still running yesterday's code because
      nobody ran `install.sh`. The health panel answers it, by comparing the
      newest `.rs` under `rust/src` against the installed binary's mtime.
      **Not** by comparing the built artifact: `cargo test --release` produces a
      *different binary* from `cargo build --release` (`2f624e8b…` vs
      `bbd8ba58…`, and the next build flips it back), so any check reading
      `target/` cries wolf after every test run, and a warning that cries wolf
      is a warning nobody reads.
    `hubd` prints `hubd_boot` before touching anything, then `hubd_signature`
    (`cert` · `adhoc` · `unreadable`), so "never entered main", "will lose its
    grants at the next build" and "cannot even read itself" are three
    distinguishable lines instead of one pid sitting there.
    **What was NOT true, though the last two sessions believed it:** that TCC
    blocks the launchd copy from `~/Documents`. It does not — the launchd copy
    loads `hub.env` and the pid lock from there, with `hub_env_loaded` as
    evidence. The `EX_CONFIG` (78) hang was `StandardOutPath`, which **launchd
    itself** opens before the program runs; moving the logs to `~/Library/Logs`
    fixed that and nothing else was ever blocked. A binary running by hand from
    a terminal borrows that terminal's grants and honestly reports `adhoc`; only
    the launchd copy needs an identity of its own.

## When you change something

- Changed the snapshot shape or a chat verb? `deploy/install.sh` in the same
  pass — it builds, signs and restarts the launchd job. The running binary is
  the consumer, and a stale one silently overwrites the new shape (twice on
  2026-08-07). `cargo build` alone updates `target/`, which nothing runs.
- A verb that parses must have a handler. A verb with no handler is worse than
  an unknown one: the room accepts it, nothing happens, nothing says so.
- Deploying the page: `node fe-deploy.mjs <version> "<notes>"`. Bundle versions
  are IMMUTABLE — re-using a name after editing the page ships nothing. The
  script compares the served bytes against what it packed and fails loudly;
  that check used to be skipped when the version was already live, and on
  2026-08-08 it reported "ĐẠT" for a deploy that shipped nothing.
- E2E runs against the DEPLOYED bundle, as `alice_local` (the owner). Logging in
  as `hubbot` tests a permission nobody uses — every command comes back
  `tfl5_command_from_non_owner`.
- `fe-stream-uc` and `fe-aside-uc` make REAL `claude` calls on the owner's
  account. **They are gated on that size gauge and skip by default.** Each script sizes the
  transcript first (`USD_PER_MB = 1.75`, from a measured 0.99 MB → $1.72); if the
  estimate is over `HUB_UC_MAX_USD` (default **$0.25**) the paid step does not
  run, the checks behind it are NOT counted as passed, and the summary prints
  `N BỎ QUA vì tốn hạn mức` plus what was not verified. To actually buy the
  evidence: `HUB_UC_PAY=1 node fe-stream-uc.mjs …`.
  Why the gate exists: on 2026-08-08 these two scripts were re-run after every
  bundle bump and spent **$6.75 in one evening** re-proving the same thing —
  including $1.70 lost to a mid-run server restart, and $1.10 spent by a
  half-finished version of this very gate that printed the estimate and then
  called anyway. A price printed is not a price stopped.

## Project layout

```
hub                     wrapper script → rust/target/release/hub
hub.config.json         config (no secrets — only env var NAMES)
deploy/install.sh       build → install a SIGNED hubd where launchd runs it
deploy/sign.sh          re-sign one binary with the stable identity
deploy/make-signing-cert.sh  create that identity — ONCE, ever
rust/src/main.rs        CLI: doctor init once ingest status sessions
                        tfl5-say tfl5-tail portal-push
rust/src/bin/hubd.rs    daemon loop (pid lock, exponential backoff, local alarm)
rust/src/{config,db}.rs config + validation + secret_from_env() · runs/cursors/spend
rust/src/pipeline.rs    one cycle: read the room, run the orders, answer
rust/src/sessions.rs    list · stream · fork (/ask, /handover) · background (/new, /tell, /stop)
rust/src/portal.rs      read-only snapshot pushed to tfl5 (docs, not files — see its header)
rust/src/redaction.rs   leak scan, used by session previews
rust/src/live.rs        held-open /ws/chat socket: wakes the cycle when an order lands
rust/src/adapters/      tfl5 (the one channel) + the poll/command contract
rust/tests/             integration tests + captured real fixture
fe/index.html           the phone page, shipped to tfl5 as an app bundle
fe-deploy.mjs           zip → Releases → Activate through the console UI, then verifies bytes
fe-*.mjs                Playwright over the DEPLOYED bundle at 390×844:
                        -smoke (chat), -board (tabs/health/config, absence of the inbox),
                        -sessions + -stream (UC-S01..S04, S07), -aside (UC-S05b),
                        -newsession (UC-S06), -config (form → /set → disk), -denied, -phone
console-acl.mjs         grant/revoke app access through the tfl5 console UI
hub.env(.example)       secrets for launchd runs — chmod 600, gitignored
deploy/*.plist          launchd unit (runs the INSTALLED hubd, not target/)
legacy-node/            archived prototype
```
