# hub — operating rules

`hub` puts the **Claude CLI sessions running on this Mac** on a phone. One chat
room on tfl5 carries ORDERS (`/session`, `/ask`, `/new`, `/tell`, `/stop`,
`/handover`); a read-only snapshot travels the other way so the page can show
what every session is doing. Nothing here reads mail, and nothing here spends
money unless the owner presses a button.

Read `README.md` for the architecture and the CLI. Read `PLAN.md` for what is
built vs. pending. This file is the rules for working ON hub.

## The one test every design decision must pass

Hà, 2026-08-11, restating the founding intent: *"cli claude cài trên máy tôi,
hub là **cầu kết nối** ra ui để tôi làm việc, điều khiển, giao tiếp phiên"*.

**hub is a BRIDGE, not an owner.** The sessions belong to the CLI on this Mac;
hub carries them to a screen he can reach and carries his hands back. That gives
one test, and it cuts both ways:

- **Anything hub does that has no equivalent at the terminal is a smell.** It
  means hub invented a way of working he cannot see, take over, or reason about.
  The example that forced the rule, and the one that proves it pays: `/new` used
  to create a `--bg` session — headless, no window, no live screen, impossible
  to type into without stopping it first. He would never produce that by sitting
  down at the machine; he would open a window. Three features built on
  2026-08-10 (the activity line, `/btw`, screen reading) simply did not work
  there, and that was the tell. **Fixed 2026-08-11**: `/new` now runs `do script`
  and opens a real Terminal window, matched back to `claude agents` by **tty** —
  the only handle that exists at that moment (the name is auto-assigned, the id
  does not exist yet). `--bg` survives as the fallback when no window can be
  opened, or when `new_in_terminal` is off.
  Closing had to follow, or the bridge is one-way: hub could open a window and
  then refuse to close it. `/stop` on a hub-opened window session types `/exit`,
  waits for Terminal to report the tab idle, then closes the window — Hà's own
  definition of *tắt hẳn*.
- **Anything he can do at the terminal but not from the phone is a gap.** Today:
  watch more than one session's screen at once (`portal.rs:96` — live screen
  only for the focused session), scroll back further than the captured window
  (`portal.rs:108` — 16 lines), answer an OS dialog.

When a choice is unclear, ask what he would do sitting at the machine, and make
the phone do that — not something cleverer.

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
- Store: `data/hub.sqlite` (WAL). Three tables — `runs`, `cursors`, `spend`
  (plus `schema_meta`). The four inbox tables are GONE as of schema step 4
  (2026-08-10): they had outlived the product by two days and held 379 rows no
  query could reach. Two facts worth keeping from that migration — the four
  reference each other, so it has to run with `foreign_keys` OFF (the first
  version died at boot on `FOREIGN KEY constraint failed`, exit 70), and it
  logs each table with its row count rather than a silent "cleaned up".
- Tests: `cd rust && cargo test --offline` → 104 tests, 0 warnings.
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
   The shape that broke this rule twelve times is `db.get_cursor(k).ok()` —
   `Ok(None)` means "never set" and `Err` means SQLite itself failed, and
   folding them together made every phone command answer *"no session
   selected"* on a broken database, with nothing in the log. Read cursors
   through **`Db::cursor_or_log`**; the gate lives in one place because twelve
   call sites means the thirteenth forgets. Same reasoning as the visibility
   guard inside `stickToBottom` (`fe/index.html`) and `pending_for_display`
   (`sessions.rs`): put the rule at the source, not at every caller.
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

   **Call them ROUTES** (Hà 2026-08-11: *"tại sao không gọi nó là route?"* — no
   good reason, and it is the truer word). That is exactly the shape: a button
   on the phone hits a named path, a handler runs, an ack comes back; the chat
   room is only the transport. Say "route" in docs and in conversation — the
   code's own `CommandKind` is internal vocabulary, and speaking it at the owner
   is how `/new` ended up being explained to someone who only ever sees buttons.
   One caveat the word must not hide: these routes are **not open**. Only the
   owner's tid can invoke them, so they are authenticated at the human layer,
   not public endpoints.

   Routes: session · new · ask · tell · stop · handover · type · key · shot ·
   project · ingest · run · doctor · set · help. A route that already answered
   must end its arm with `Some(ack)` — the fall-through that used to exist
   logged "Không tìm thấy decision #0" as the reply for every `/session` and
   `/ask` ever issued.
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
    **A BACKGROUND subagent's `tool_result` arrives immediately** — it says
    "agent launched", not "agent finished" — so pairing `tool_use` with
    `tool_result` reports a fan-out as over the instant it starts. Measured
    2026-08-10: two agents running, `hub sessions` said `pending 0`, and the
    background case is precisely the one the count exists for (a blocking
    subagent leaves the parent visibly busy; a background one leaves it looking
    idle). Three structural facts make the real answer reachable, none of them a
    string match on CLI prose: a background agent writes
    `<slug>/<session_id>/subagents/agent-<agentId>.jsonl` (`isSidechain: true`)
    beside `agent-<agentId>.meta.json`, whose `toolUseId` names the call that
    spawned it; the parent transcript later receives a `<task-notification>`
    block whose `<tool-use-id>` is that same id — carried by **three different
    record shapes**, not one: a normal `user` turn
    (`message.content[].text`) when the parent was idle, and
    `queue-operation.content` then `attachment.prompt` when the agent finished
    while the parent was mid-command. Reading only the first shape leaves a
    ghost on the screen forever, and no acceptance script catches it, because
    they all measure while an agent is genuinely running; and a subagent is NOT
    a process, so `ps` will never find one. A dead session's un-notified agents
    are NOT running — the process took them with it — so liveness gates the
    count (`sessions::pending_for_display`), or the screen grows ghosts.
11. **hub speaks only on a CHANGE, never on a state.** `watch.rs` compares this
    cycle's snapshot with the previous one and announces only transitions — a
    session that finished its turn, a session that ended. Announcing a *state*
    from a loop that ticks every ~10s is a phone that buzzes forever, and a phone
    that buzzes forever gets muted, taking the messages that mattered with it.
    Three rules hold the line: say it once; say **nothing** on the first round
    after a restart (an empty book means hub just woke up, not that everything
    just changed); and treat *leaving the list* as the main "ended" signal,
    because `claude agents` drops a stopped session within seconds and usually
    never shows it as `dead`. `IDLE_AFTER_SEC` (180s) is a promise about TRUTH,
    not speed: this repo's own `cargo test` runs over two minutes, so a shorter
    window announces "finished" while the session is still working — a wrong
    sentence, not a late one, and it goes to Telegram. Both mouths (the room and
    Telegram) say the SAME sentence (`Change::say`), or nobody can reconcile them
    later. Neither call spends quota.
12. **The phone page is the only UI.** There is no local console any more. If you
    add a surface, it must go through the same room commands — one path, one set
    of books.
13. **macOS facts that cost a night to learn** (2026-08-10). Typing into a live
    session goes through Terminal's own `do script`, NOT `System Events keystroke`
    — the latter is refused outright (*"osascript is not allowed to send
    keystrokes (1002)"*) and no amount of granting Accessibility fixes it,
    because the process asking is `/usr/bin/osascript`, a system binary. `do
    script` needs only **Automation**, which hub already has. It **always
    appends a newline** and that cannot be turned off: fine for `claude`'s
    prompt box, but it means an arrow key both MOVES and CONFIRMS, so hub sends
    an arrow only when it can **prove there is no choice dialog** — not merely
    when it fails to see one. That distinction is the whole gate: `screen_of`
    used to fold three outcomes into `None` (no window · osascript failed ·
    **the screen looks like it holds a secret**), and the gate read `None` as
    "no dialog" and sent, so it failed OPEN exactly when hub was blindest —
    including the case where a password was on screen. `keys::look` now returns
    `Saw` / `Withheld` / `Blind`, and `keys::arrow_verdict` sends only on a
    proven-empty choice list. `Withheld` still decides correctly: the choice
    COUNT is a number, and a number carries no text off the machine.
    And when the session is mid-run, `claude` **queues** the text
    rather than showing it — so a reply must read the screen back and say WHERE
    the text landed. `osascript` returning 0 proves only that bytes reached the
    tab.
    **Opening and closing a window** (2026-08-11). `do script "<cmd>"` makes a
    new window and returns its *tab*; `tty of` that tab is the handle that ties
    it to the `claude agents` row that appears seconds later. Closing is
    ordered, and the order is not politeness: `/exit` first, then close, because
    closing a window with a live process raises Terminal's own *"Do you want to
    terminate running processes?"* modal — and a modal **blocks every automation
    command after it**, so getting this backwards gags hub. Two measurements
    pin the steps: typing `/exit` really does end the CLI (pid gone, `busy` →
    `false`), so no `kill` and no lost end-of-session bookkeeping; and the
    window does **not** close itself when the shell exits (the profile keeps it,
    showing `[Process completed]`), so the close step is required, not cosmetic.
    If the tab is still busy after 10s, hub refuses to close — better a live
    window than a modal that silences everything.
    One trap unique to this path: the command goes through a **shell**, so every
    `DENIED_TOOLS` pattern must be quoted. `Bash(git push:*)` bare is a shell
    syntax error, and the window would open to a red line with no session and no
    guard rail. The `--bg` path never had this because it passes argv directly.
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
- **Look at the picture. Every deploy ends with one.** `fe-deploy.mjs` now runs
  `fe-shots.mjs` after a successful activate and prints
  `ui-shots/after-<version>-*.png`; open them before saying anything is done.
  This is mechanical on purpose. Twice on 2026-08-10 the assertions were green
  while the screen was wrong: the "snapshot is stale" warning was **cut off**
  mid-sentence (7/7 checks passed — they read `textContent`, which holds the
  whole string even when the screen truncates it), and the "what is it doing"
  line rendered **above** the session name, so the eye read `Brewing…` before
  knowing which session. An assertion tests what you thought to check; a picture
  shows what you didn't. `fe-shots` only reads — it never calls `claude`, so
  there is no reason to skip it.
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
rust/src/watch.rs       "vừa xong" / "vừa tắt" — so hai lượt ảnh chụp, nói MỘT lần
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
                        -newsession (UC-S06), -subagent (UC-S02b — needs a REAL
                        subagent running, else it exits 2 rather than passing),
                        -config (form → /set → disk), -denied, -phone, -url, -type
fe-shots.mjs            screenshots of all 5 screens; reads only, never calls claude
console-acl.mjs         grant/revoke app access through the tfl5 console UI
hub.env(.example)       secrets for launchd runs — chmod 600, gitignored
deploy/*.plist          launchd unit (runs the INSTALLED hubd, not target/)
legacy-node/            archived prototype
```
