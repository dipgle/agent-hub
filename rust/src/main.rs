//! hub — CLI for the comms hub.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use serde_json::Value;

use hub::adapters::{email, github, telegram};
use hub::config::{self, Config};
use hub::db::{Db, DecisionRow, Message, NewMessage};
use hub::exec::{run, truncate, RunOpts};
use hub::logging;
use hub::outbound::flush;
use hub::pipeline::{
    approve_decision, ingest, known_projects, reject_decision, run_once, triage_message_by_id,
    triage_new, ADAPTER_NAMES,
};

#[derive(Parser)]
#[command(
    name = "hub",
    about = "Comms hub: email / GitHub / project devlog / chat -> claude triage -> reply, brief, or branch",
    version
)]
struct Cli {
    /// Path to hub.config.json (default: nearest one walking up from cwd)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Log every step (default: warn for interactive commands)
    #[arg(long, global = true)]
    debug: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check every channel + credential, honestly
    Doctor,
    /// Write hub.config.json + create the db
    Init {
        #[arg(long)]
        force: bool,
    },
    /// One full cycle: ingest -> triage -> send
    Once,
    /// Poll every enabled adapter
    Ingest,
    /// Triage the queue (up to max_triage_per_cycle)
    Triage,
    /// Send whatever is queued in the outbox
    Flush,
    /// What came in and what was decided
    Inbox {
        #[arg(long)]
        status: Option<String>,
        #[arg(long, short = 'p')]
        project: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    /// One message + its decision in full (`show 12` or `show d9`)
    Show { id: String },
    /// Put your own message into the hub and answer it now
    Say {
        text: Vec<String>,
        #[arg(long, short = 'p')]
        project: Option<String>,
        #[arg(long)]
        no_triage: bool,
    },
    /// Send the drafted reply / green-light the action
    Approve {
        id: i64,
        #[arg(long)]
        body: Option<String>,
    },
    /// Drop it (cancels anything queued)
    Reject { id: i64, reason: Vec<String> },
    /// Skip a message without paying for triage
    Close { id: i64, reason: Vec<String> },
    /// Reply in your own words through that channel
    Reply { id: i64, text: Vec<String> },
    /// Implement an approved change on a branch
    Act {
        id: i64,
        #[arg(long)]
        force: bool,
    },
    /// Counts, poll health, spend
    Status,
    /// Connect Telegram: read the chat id that messaged your bot and write it
    /// into the config (enables the adapter + trusts that chat).
    TelegramLink {
        /// Chat id to link; omit to auto-detect the one that messaged the bot
        chat_id: Option<String>,
    },
    /// Serve the local web UI (inbox, approve/reject, config)
    Web {
        /// Override web.port from the config
        #[arg(long)]
        port: Option<u16>,
    },
    /// Post a message into the tfl5 chat room (hub dials out; nothing listens)
    Tfl5Say {
        /// The message text
        text: Vec<String>,
    },
    /// Triage ONE message now, jumping the queue (for testing and for a message
    /// the owner wants answered before a backlog of CI noise)
    TriageOne {
        id: i64,
        /// Triage it on its own even if its thread has an open decision
        #[arg(long)]
        no_coalesce: bool,
    },
    /// Every Claude CLI session alive on this machine, across all accounts
    Sessions {
        /// Machine-readable, for the portal snapshot and for scripting
        #[arg(long)]
        json: bool,
    },
    /// Read recent messages from the tfl5 chat room
    Tfl5Tail {
        /// How many messages to show
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    /// Push the read-only status snapshot to the tfl5 app so the chat page can
    /// show the inbox, health and spend next to the conversation
    PortalPush {
        /// Print the snapshot instead of uploading it
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("hub: {}", logging::err_chain(&e));
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let cli = Cli::parse();

    let cfg = config::load(cli.config.as_deref())?;
    logging::set_log_file(&cfg.log_file);
    // Same secret source as the daemon, so CLI and launchd behave identically.
    config::load_env_file(&cfg.hub_home);
    let chatty = matches!(
        cli.command,
        Command::Once | Command::Ingest | Command::Triage | Command::Flush
    );
    if cli.debug {
        logging::set_level_from_name("debug");
    } else if !chatty {
        logging::set_level_from_name("warn");
    }
    if let Ok(level) = std::env::var("HUB_LOG_LEVEL") {
        logging::set_level_from_name(&level);
    }

    let db = Db::open(&cfg.db)?;

    match cli.command {
        Command::Doctor => cmd_doctor(&db, &cfg),
        Command::Init { force } => cmd_init(&cfg, force),
        Command::Once => {
            println!("{}", serde_json::to_string_pretty(&run_once(&db, &cfg)?)?);
            Ok(())
        }
        Command::Ingest => {
            println!("{}", serde_json::to_string_pretty(&ingest(&db, &cfg)?)?);
            Ok(())
        }
        Command::Triage => {
            println!("{}", serde_json::to_string_pretty(&triage_new(&db, &cfg)?)?);
            Ok(())
        }
        Command::Flush => {
            println!("{}", serde_json::to_string_pretty(&flush(&db, &cfg, 20)?)?);
            Ok(())
        }
        Command::Inbox {
            status,
            project,
            limit,
        } => cmd_inbox(&db, status.as_deref(), project.as_deref(), limit),
        Command::Show { id } => cmd_show(&db, &id),
        Command::Say {
            text,
            project,
            no_triage,
        } => cmd_say(&db, &cfg, &text.join(" "), project.as_deref(), no_triage),
        Command::Approve { id, body } => cmd_approve(&db, &cfg, id, body.as_deref()),
        Command::Reject { id, reason } => cmd_reject(&db, id, &reason.join(" ")),
        Command::Close { id, reason } => cmd_close(&db, id, &reason.join(" ")),
        Command::Reply { id, text } => cmd_reply(&db, &cfg, id, &text.join(" ")),
        Command::Act { id, force } => cmd_act(&db, &cfg, id, force),
        Command::Status => cmd_status(&db),
        Command::TelegramLink { chat_id } => cmd_telegram_link(cfg, chat_id.as_deref()),
        Command::Web { port } => {
            let p = port.unwrap_or(cfg.web.port);
            hub::web::serve(cfg, p)
        }
        Command::TriageOne { id, no_coalesce } => {
            let out = hub::pipeline::triage_message_by_id(&db, &cfg, id, !no_coalesce)?;
            println!("{}", serde_json::to_string_pretty(&out)?);
            Ok(())
        }
        Command::Sessions { json } => cmd_sessions(&cfg, json),
        Command::Tfl5Say { text } => cmd_tfl5_say(&cfg, &text.join(" ")),
        Command::Tfl5Tail { limit } => cmd_tfl5_tail(&cfg, limit),
        Command::PortalPush { dry_run } => cmd_portal_push(&db, &cfg, dry_run),
    }
}

/// What `claude` is doing on this machine right now. Read-only: this lists and
/// reads transcripts, it never starts, stops, or types into a session.
fn cmd_sessions(cfg: &Config, as_json: bool) -> Result<()> {
    let snap = hub::sessions::snapshot(cfg);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
        return Ok(());
    }

    let counts = hub::sessions::count_by_account(&snap);
    let per = counts
        .iter()
        .map(|(a, n)| format!("{a} {n}"))
        .collect::<Vec<_>>()
        .join(" · ");
    println!("{} phiên đang sống  ({per})\n", snap.sessions.len());

    let now = chrono::Utc::now();
    for s in &snap.sessions {
        let ago = s
            .last_activity
            .as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| {
                let mins = (now - t.with_timezone(&chrono::Utc)).num_minutes().max(0);
                if mins < 60 {
                    format!("{mins}p")
                } else if mins < 60 * 48 {
                    format!("{}g", mins / 60)
                } else {
                    format!("{}ng", mins / (60 * 24))
                }
            })
            .unwrap_or_else(|| "—".into());

        println!(
            "  [{}] {:<24} {:>5} trước  pid {:<7} {}",
            s.account, s.name, ago, s.pid, s.kind
        );
        // Only when there is text. A withheld preview prints its reason on the
        // note line below; printing "(không có chữ)" there would read as "the
        // session said nothing", which is the opposite of what happened.
        if let (Some(role), Some(text)) = (&s.last_role, &s.last_text) {
            println!("        {role}: {}", text.replace('\n', " "));
        }
        if let Some(note) = &s.note {
            println!("        ⚠ {note}");
        }
    }

    if !snap.notes.is_empty() {
        println!("\nTài khoản không trả lời được:");
        for n in &snap.notes {
            println!("  ⚠ {n}");
        }
    }
    Ok(())
}

/// Publish the snapshot the tfl5 chat page reads. `--dry-run` prints it so the
/// shape can be inspected without a round trip (and without credentials).
fn cmd_portal_push(db: &Db, cfg: &Config, dry_run: bool) -> Result<()> {
    if dry_run {
        let snap = hub::portal::build(db, cfg, hub::portal::INBOX_LIMIT)?;
        println!("{}", serde_json::to_string_pretty(&snap)?);
        return Ok(());
    }
    let out = hub::portal::push(cfg, db)?;
    println!(
        "đã đẩy ảnh chụp ({} mục, {} byte) vào resource {}{}",
        out.items,
        out.bytes,
        hub::portal::RESOURCE_MA,
        if out.resource_created {
            " (vừa tạo resource)"
        } else {
            ""
        }
    );
    Ok(())
}

/// Post one line into the tfl5 room and report the receipt tfl5 handed back.
fn cmd_tfl5_say(cfg: &Config, text: &str) -> Result<()> {
    if text.trim().is_empty() {
        bail!("cần nội dung: hub tfl5-say \"…\"");
    }
    let c = &cfg.adapters.tfl5;
    if c.app_tid.is_empty() {
        bail!(
            "adapters.tfl5.app_tid chưa đặt trong {}",
            cfg.config_file.display()
        );
    }
    let s = hub::adapters::tfl5::login(c)?;
    match hub::adapters::tfl5::send_text(c, &s, &c.app_tid, &c.room, text)? {
        Some(tid) => println!("đã gửi vào {} · room {} · tid {tid}", c.app_tid, c.room),
        // `send_text` only returns Ok(None) if tfl5 ever changes its echo
        // contract; say so rather than printing a success we did not observe.
        None => println!("gửi xong nhưng tfl5 không trả tid — chưa xác nhận được"),
    }
    Ok(())
}

/// Show the tail of the room, oldest line first (history comes newest-first).
fn cmd_tfl5_tail(cfg: &Config, limit: i64) -> Result<()> {
    let c = &cfg.adapters.tfl5;
    if c.app_tid.is_empty() {
        bail!(
            "adapters.tfl5.app_tid chưa đặt trong {}",
            cfg.config_file.display()
        );
    }
    let s = hub::adapters::tfl5::login(c)?;
    let mut rows = hub::adapters::tfl5::history(c, &s, limit, None)?;
    rows.reverse();
    if rows.is_empty() {
        println!("(phòng {} chưa có tin nào)", c.room);
    }
    for m in rows {
        println!(
            "{}  {:<16}  {}",
            m["ts"].as_i64().map(iso_ms).unwrap_or_default(),
            m["from"].as_str().unwrap_or("?"),
            m["text"].as_str().unwrap_or("")
        );
    }
    Ok(())
}

fn iso_ms(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| ms.to_string())
}

/// Turn "message my bot once" into a working channel without hand-editing JSON.
fn cmd_telegram_link(mut cfg: Config, explicit: Option<&str>) -> Result<()> {
    if config::secret_from_env(&cfg.adapters.telegram.token_env).is_none() {
        bail!(
            "{} chưa được set. Lấy token từ @BotFather rồi:\n  export {}='<token>'",
            cfg.adapters.telegram.token_env,
            cfg.adapters.telegram.token_env
        );
    }

    let health = telegram::health(&cfg.adapters.telegram);
    if !health.ok {
        bail!("token không dùng được: {}", health.detail);
    }
    println!("bot: {}", health.detail);

    let chat_id = match explicit {
        Some(id) => id.to_string(),
        None => {
            let seen = telegram::observed_chat_ids(&cfg.adapters.telegram)?;
            match seen.len() {
                0 => bail!(
                    "chưa thấy ai nhắn cho bot. Mở chat với bot, bấm Start, rồi chạy lại lệnh này."
                ),
                1 => {
                    // "12345 (username)" → "12345"
                    let id = seen[0]
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string();
                    println!("thấy 1 chat: {}", seen[0]);
                    id
                }
                _ => bail!(
                    "thấy nhiều chat ({}). Chạy lại với id cụ thể: hub telegram-link <chat_id>",
                    seen.join(", ")
                ),
            }
        }
    };

    cfg.adapters.telegram.enabled = true;
    if !cfg.adapters.telegram.allowed_chat_ids.contains(&chat_id) {
        cfg.adapters.telegram.allowed_chat_ids.push(chat_id.clone());
    }
    if !cfg.trust.telegram_chat_ids.contains(&chat_id) {
        cfg.trust.telegram_chat_ids.push(chat_id.clone());
    }
    config::save(&cfg)?;

    println!("đã ghi vào {}:", cfg.config_file.display());
    println!("  adapters.telegram.enabled = true");
    println!("  adapters.telegram.allowed_chat_ids += {chat_id}");
    println!("  trust.telegram_chat_ids += {chat_id}   (chat này được coi là bạn)");
    println!();
    println!("thử ngay: ./hub once   → brief sẽ đẩy về Telegram kèm nút Duyệt/Bỏ");
    Ok(())
}

// ─── commands ────────────────────────────────────────────────────────────

fn cmd_doctor(db: &Db, cfg: &Config) -> Result<()> {
    println!(
        "config      {}{}",
        cfg.config_file.display(),
        if cfg.config_file.is_file() {
            ""
        } else {
            "  (defaults — file not created yet)"
        }
    );
    println!("db          {}", cfg.db.display());
    println!("workspace   {}", cfg.workspace_root.display());
    // Make the third list visible: which folders a project NAME resolves
    // against. It used to be hardcoded and invisible, so a typo looked
    // identical to a project with nothing to report.
    println!(
        "project dirs {}",
        config::project_bases(cfg)
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("  ·  ")
    );
    println!(
        "autonomy    default={}  per-project={}",
        &*cfg.autonomy.default,
        serde_json::to_string(&cfg.autonomy.projects)?
    );

    // The registry, checked against the folders it claims. A name with no
    // folder used to be invisible: context came back empty and hub answered
    // anyway, so a typo read exactly like a quiet project.
    if !cfg.projects.is_empty() {
        println!();
        println!("projects:");
        for (name, p) in &cfg.projects {
            let dir = config::project_dir(cfg, name);
            let bits = [
                p.tier.clone().map(|t| format!("tier {t}")),
                (!p.repos.is_empty()).then(|| p.repos.join(", ")),
                (!p.matchers.is_empty()).then(|| format!("{} luật khác", p.matchers.len())),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            match dir {
                Some(d) => println!("  {name:<14} OK   {}{}", d.display(), if bits.is_empty() { String::new() } else { format!("  ({bits})") }),
                None => println!("  {name:<14} SAI  không có thư mục nào tên '{name}' trong project dirs — hub sẽ triage MÙ (không git log, không devlog)"),
            }
        }
    }
    println!(
        "act stage   {}",
        if cfg.act.enabled {
            format!(
                "enabled (model={}, budget ${})",
                cfg.act.model, cfg.act.max_budget_usd
            )
        } else {
            "disabled".into()
        }
    );
    println!();

    match run(
        "claude",
        &["--version"],
        RunOpts {
            timeout: Some(std::time::Duration::from_secs(20)),
            ..Default::default()
        },
    ) {
        Ok(r) if r.code == Some(0) => println!("claude      OK  {}", r.stdout.trim()),
        Ok(r) => println!(
            "claude      FAIL {}",
            truncate(
                if r.stderr.trim().is_empty() {
                    &r.stdout
                } else {
                    &r.stderr
                },
                120
            )
        ),
        Err(e) => println!("claude      FAIL {e}"),
    }
    println!(
        "triage      model={} budget=${} timeout={}s min_conf_auto={}",
        cfg.triage.model,
        cfg.triage.max_budget_usd,
        cfg.triage.timeout_sec,
        cfg.triage.min_confidence_auto
    );
    println!();

    println!("channels:");
    for name in ADAPTER_NAMES {
        // Same table the ingest loop uses — see `pipeline::adapter_enabled`.
        if !hub::pipeline::adapter_enabled(cfg, name) {
            println!("  {name:<9} off        (adapters.{name}.enabled = false)");
            continue;
        }
        match name {
            "github" => {
                let h = github::health();
                println!(
                    "  {name:<9} {}       {}",
                    if h.ok { "OK  " } else { "FAIL" },
                    truncate(&h.detail, 90)
                );
            }
            "devlog" => println!("  {name:<9} on         (read-only tail, no credential)"),
            "email" => {
                let h = email::health(&cfg.adapters.email);
                println!(
                    "  {name:<9} {}       {}",
                    if h.ok { "OK  " } else { "FAIL" },
                    truncate(&h.detail, 90)
                );
            }
            "telegram" => {
                let h = telegram::health(&cfg.adapters.telegram);
                println!(
                    "  {name:<9} {}       {}",
                    if h.ok { "OK  " } else { "FAIL" },
                    truncate(&h.detail, 90)
                );
                if h.ok && cfg.adapters.telegram.allowed_chat_ids.is_empty() {
                    match telegram::observed_chat_ids(&cfg.adapters.telegram) {
                        Ok(ids) if !ids.is_empty() => {
                            println!("             ↳ allowed_chat_ids is empty. Chat ids seen recently: {}", ids.join(", "))
                        }
                        Ok(_) => println!("             ↳ allowed_chat_ids is empty and no chat has messaged the bot yet"),
                        Err(e) => println!("             ↳ could not read updates: {e}"),
                    }
                }
            }
            "tfl5" => {
                let h = hub::adapters::tfl5::health(&cfg.adapters.tfl5);
                println!(
                    "  {name:<9} {}       {}",
                    if h.ok { "OK  " } else { "FAIL" },
                    truncate(&h.detail, 90)
                );
            }
            _ => {}
        }
    }

    println!();
    let projects = known_projects(cfg);
    println!(
        "projects with a devlog: {}",
        if projects.is_empty() {
            "(none found)".to_string()
        } else {
            projects.join(", ")
        }
    );
    let c = db.counts()?;
    println!(
        "db state: messages={} outbox={} dead_letter={} spend=${:.4}",
        serde_json::to_string(&c.messages)?,
        serde_json::to_string(&c.outbox)?,
        c.dead_letter,
        c.cost_usd_total
    );
    Ok(())
}

fn cmd_init(cfg: &Config, force: bool) -> Result<()> {
    if cfg.config_file.is_file() && !force {
        println!(
            "config already exists: {} (use --force to overwrite)",
            cfg.config_file.display()
        );
    } else {
        std::fs::write(
            &cfg.config_file,
            serde_json::to_string_pretty(&Config::default())? + "\n",
        )?;
        println!("wrote {}", cfg.config_file.display());
    }
    println!(
        "db ready at {} (schema v{})",
        cfg.db.display(),
        hub::db::SCHEMA_VERSION
    );
    Ok(())
}

fn short(s: Option<&str>, n: usize) -> String {
    let s = s.unwrap_or("");
    if s.chars().count() > n {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    } else {
        s.to_string()
    }
}

fn cmd_inbox(db: &Db, status: Option<&str>, project: Option<&str>, limit: i64) -> Result<()> {
    let rows = db.list_messages(status, project, limit)?;
    if rows.is_empty() {
        println!("(empty)");
        return Ok(());
    }
    println!("msg  status          source    project     kind/sev      conf  subject");
    for m in &rows {
        let d = db.latest_decision_for(m.id)?;
        let kind = match &d {
            Some(d) => format!(
                "{}/{}",
                d.kind.clone().unwrap_or_default(),
                d.severity.clone().unwrap_or_default()
            ),
            None => "-".into(),
        };
        let conf = d
            .as_ref()
            .and_then(|d| d.confidence)
            .map(|c| format!("{c:<5}"))
            .unwrap_or_else(|| "-    ".into());
        let tail = match &d {
            Some(d) => format!(
                "   [d#{}{}]",
                d.id,
                if d.status == "pending" {
                    " pending"
                } else {
                    ""
                }
            ),
            None => String::new(),
        };
        println!(
            "{:<4} {:<15} {:<9} {:<11} {:<13} {} {}{}",
            m.id,
            m.status,
            m.source,
            m.project.clone().unwrap_or_else(|| "-".into()),
            kind,
            conf,
            short(m.subject.as_deref(), 50),
            tail
        );
    }
    let c = db.counts()?;
    println!();
    println!(
        "totals: {}  queued_out={}  spend=${:.4}",
        serde_json::to_string(&c.messages)?,
        c.outbox.get("queued").copied().unwrap_or(0),
        c.cost_usd_total
    );
    Ok(())
}

fn decision_and_message(db: &Db, id: i64) -> Result<(DecisionRow, Message)> {
    let d = db
        .get_decision(id)?
        .ok_or_else(|| anyhow!("no decision #{id}"))?;
    let m = db
        .get_message(d.message_id)?
        .ok_or_else(|| anyhow!("decision #{id} has no message (db inconsistent)"))?;
    Ok((d, m))
}

fn cmd_show(db: &Db, id: &str) -> Result<()> {
    let (m, d) = match id.strip_prefix('d').or_else(|| id.strip_prefix('D')) {
        Some(rest) => {
            let (d, m) = decision_and_message(db, rest.parse()?)?;
            (m, Some(d))
        }
        None => {
            let m = db
                .get_message(id.parse()?)?
                .ok_or_else(|| anyhow!("no message #{id}"))?;
            let d = db.latest_decision_for(m.id)?;
            (m, d)
        }
    };

    println!("message #{}  [{}]", m.id, m.status);
    println!(
        "  source {} · sender {} ({}) · project {}",
        m.source,
        m.sender.clone().unwrap_or_default(),
        m.sender_trust,
        m.project.clone().unwrap_or_else(|| "-".into())
    );
    println!("  external_id {}", m.external_id);
    println!(
        "  received {} · ingested {}",
        m.received_at.clone().unwrap_or_else(|| "?".into()),
        m.ingested_at
    );
    if let Some(u) = &m.url {
        println!("  url {u}");
    }
    if let Some(e) = &m.last_error {
        println!("  last_error {e}");
    }
    println!(
        "  subject: {}",
        m.subject.clone().unwrap_or_else(|| "(none)".into())
    );
    println!();
    println!("--- body ---");
    println!(
        "{}",
        m.body
            .clone()
            .unwrap_or_default()
            .chars()
            .take(4000)
            .collect::<String>()
    );
    println!("--- end body ---");

    let d = match d {
        Some(d) => d,
        None => {
            println!("\n(no decision yet)");
            return Ok(());
        }
    };

    println!();
    println!(
        "decision #{}  [{}] tier={} model={} cost=${:.4}",
        d.id,
        d.status,
        d.tier,
        d.model.clone().unwrap_or_default(),
        d.cost_usd.unwrap_or(0.0)
    );
    println!(
        "  {}/{} · project {} · confidence {} · needs_human={}",
        d.kind.clone().unwrap_or_default(),
        d.severity.clone().unwrap_or_default(),
        d.project.clone().unwrap_or_else(|| "-".into()),
        d.confidence.unwrap_or(0.0),
        d.needs_human
    );
    if let Some(t) = &d.tripwire {
        println!("  ⚠ tripwire: {t}");
    }
    println!("  summary: {}", d.summary.clone().unwrap_or_default());
    if let Value::Array(actions) = d.actions_json() {
        if !actions.is_empty() {
            println!("  proposed actions:");
            for a in actions {
                println!(
                    "    - {}: {}",
                    a.get("type").and_then(|v| v.as_str()).unwrap_or("?"),
                    a.get("detail").and_then(|v| v.as_str()).unwrap_or("")
                );
            }
        }
    }
    if let Some(ev) = d
        .evidence
        .as_deref()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
    {
        if !ev.is_empty() {
            println!("  evidence:");
            for e in ev {
                println!("    · {e}");
            }
        }
    }
    if let Some(outcome) = d.raw_json().get("outcome") {
        println!(
            "  policy: {} — {}{}",
            outcome
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
            outcome.get("reason").and_then(|v| v.as_str()).unwrap_or(""),
            match (
                outcome.get("channel").and_then(|v| v.as_str()),
                outcome.get("target").and_then(|v| v.as_str())
            ) {
                (Some(c), Some(t)) => format!(" → {c}:{t}"),
                _ => String::new(),
            }
        );
    }
    let draft = d.reply_draft.clone().unwrap_or_default();
    if !draft.trim().is_empty() {
        println!();
        println!("--- reply draft ---");
        println!("{}", draft.trim());
        println!("--- end draft ---");
    }
    if let Some(o) = &d.outcome {
        println!("\n  outcome: {o}");
    }
    if d.status == "pending" {
        println!(
            "\n  → hub approve {0}   |   hub reject {0}   |   hub act {0}",
            d.id
        );
    }
    Ok(())
}

fn cmd_say(
    db: &Db,
    cfg: &Config,
    text: &str,
    project: Option<&str>,
    no_triage: bool,
) -> Result<()> {
    if text.trim().is_empty() {
        bail!("usage: hub say \"your message\" [-p project]");
    }
    // Refuse a bad project at the door. Resolution already refuses to follow a
    // path, but the string was still stored and shown, and a name that simply
    // does not exist produced no error at all — hub answered with no context
    // and nothing said why. Say it here, once, where it is fixable.
    if let Some(p) = project.filter(|p| !p.is_empty()) {
        if !config::is_project_name(p) {
            bail!("\"{p}\" không phải tên project (phải là tên thư mục, không có / hay ..)");
        }
        if config::project_dir(cfg, p).is_none() {
            bail!(
                "không thấy thư mục project \"{p}\" trong: {}\nBỏ -p để hub tự đoán, hoặc kiểm lại tên.",
                config::project_bases(cfg).iter().map(|b| b.display().to_string()).collect::<Vec<_>>().join(", ")
            );
        }
    }
    let stamp = chrono::Utc::now().timestamp_millis();
    let (id, inserted) = db.insert_message(&NewMessage {
        source: "cli".into(),
        external_id: format!("cli:{stamp}"),
        // Each question is its own thread — otherwise every CLI question would
        // coalesce into the previous unanswered one.
        thread_key: Some(format!("cli:{stamp}")),
        project: project.map(|p| p.to_string()),
        sender: Some("cli:owner".into()),
        sender_trust: Some("trusted".into()),
        subject: Some(short(Some(text), 100)),
        body: Some(text.to_string()),
        url: None,
        received_at: Some(logging::now_iso()),
        raw: Some(serde_json::json!({ "stream": "cli" })),
    })?;
    if !inserted {
        println!("duplicate — nothing inserted");
        return Ok(());
    }
    let id = id.ok_or_else(|| anyhow!("insert reported new but returned no id"))?;
    println!("queued message #{id}");

    if !no_triage {
        let counters = triage_message_by_id(db, cfg, id, false)?;
        flush(db, cfg, 20)?;
        println!("{}", serde_json::to_string(&counters)?);
        if db.latest_decision_for(id)?.is_some() {
            println!();
            cmd_show(db, &id.to_string())?;
        } else {
            println!("(no decision produced — see logs/hub.log)");
        }
    }
    Ok(())
}

fn cmd_approve(db: &Db, cfg: &Config, id: i64, body_override: Option<&str>) -> Result<()> {
    let (d, _m) = decision_and_message(db, id)?;
    if d.status != "pending" {
        println!(
            "note: decision #{id} is already '{}' — re-approving",
            d.status
        );
    }
    // One approve path for CLI, Telegram buttons and the web UI.
    let r = approve_decision(db, cfg, id, body_override)?;
    if !r.queued {
        println!(
            "không có gì để gửi (thiếu nháp trả lời hoặc không có target) — chỉ đánh dấu approved"
        );
    }
    println!(
        "approved #{}; outbox sent={} failed={}",
        r.decision_id, r.sent.sent, r.sent.failed
    );
    if !r.leaks.is_empty() {
        // Sent, because you approved it — but you should know what went out.
        println!(
            "⚠ nội dung đã gửi ra {} khớp mẫu nội bộ: {}",
            r.channel.clone().unwrap_or_default(),
            r.leaks.join(", ")
        );
    }
    if r.code_change_proposed {
        if cfg.act.enabled {
            println!(
                "this decision proposes a code change → run: hub act {}",
                r.decision_id
            );
        } else {
            println!("this decision proposes a code change, but act.enabled=false in config");
        }
    }
    Ok(())
}

fn cmd_reject(db: &Db, id: i64, reason: &str) -> Result<()> {
    reject_decision(db, id, reason)?;
    println!("rejected #{id}; queued replies cancelled");
    Ok(())
}

fn cmd_close(db: &Db, id: i64, reason: &str) -> Result<()> {
    println!("{}", hub::pipeline::close_message(db, id, reason)?);
    Ok(())
}

fn cmd_reply(db: &Db, cfg: &Config, id: i64, text: &str) -> Result<()> {
    println!("{}", hub::pipeline::reply_to_message(db, cfg, id, text)?);
    Ok(())
}

fn cmd_act(db: &Db, cfg: &Config, id: i64, force: bool) -> Result<()> {
    if !cfg.act.enabled {
        bail!("act.enabled = false in config — turn it on deliberately before running the code-change stage");
    }
    let (d, m) = decision_and_message(db, id)?;
    if d.status != "approved" && !force {
        bail!(
            "decision #{id} is '{}' — approve it first (hub approve {id}) or pass --force",
            d.status
        );
    }
    println!(
        "act stage: project={} model={} budget=${} timeout={}s",
        d.project
            .clone()
            .or_else(|| m.project.clone())
            .unwrap_or_default(),
        cfg.act.model,
        cfg.act.max_budget_usd,
        cfg.act.timeout_sec
    );
    let r = hub::act::act(&m, &d, cfg);
    println!();
    if !r.ok {
        let err = r.error.clone().unwrap_or_default();
        db.set_decision_status(d.id, "approved", Some(&format!("act failed: {err}")))?;
        bail!(
            "act failed: {err}\nworktree: {}",
            r.worktree
                .map(|w| w.display().to_string())
                .unwrap_or_else(|| "(none)".into())
        );
    }
    let wt = r.worktree.clone().unwrap_or_default();
    db.set_decision_status(
        d.id,
        "executed",
        Some(&format!(
            "branch {} in {}",
            r.branch.clone().unwrap_or_default(),
            wt.display()
        )),
    )?;
    println!("branch:   {}", r.branch.clone().unwrap_or_default());
    println!("worktree: {}", wt.display());
    println!("cost:     ${:.4}", r.cost_usd);
    println!();
    println!("--- diffstat ---");
    println!(
        "{}",
        if r.diffstat.is_empty() {
            "(no changes committed)"
        } else {
            &r.diffstat
        }
    );
    println!("--- agent report ---");
    println!("{}", r.report);
    println!();
    println!("review:  git -C {} diff HEAD~1..HEAD", wt.display());
    if let Some(dir) = config::project_dir(
        cfg,
        &d.project
            .clone()
            .or_else(|| m.project.clone())
            .unwrap_or_default(),
    ) {
        println!(
            "discard: git -C {} worktree remove {} --force",
            dir.display(),
            wt.display()
        );
    }
    Ok(())
}

fn cmd_status(db: &Db) -> Result<()> {
    let c = db.counts()?;
    println!("messages     {}", serde_json::to_string(&c.messages)?);
    println!("decisions    {}", serde_json::to_string(&c.decisions)?);
    println!("outbox       {}", serde_json::to_string(&c.outbox)?);
    println!("dead_letter  {}", c.dead_letter);
    println!("spend        ${:.4}", c.cost_usd_total);
    println!();
    println!("last polls:");
    for r in db.last_runs(12)? {
        println!(
            "  {}  {:<9} {}  new={}{}{}",
            r.started_at,
            r.adapter,
            match r.ok {
                None => "running",
                Some(1) => "ok ",
                _ => "ERR",
            },
            r.n_new.unwrap_or(0),
            r.skipped
                .map(|s| format!("  skipped: {}", short(Some(&s), 60)))
                .unwrap_or_default(),
            r.err
                .map(|e| format!("  err: {}", short(Some(&e), 80)))
                .unwrap_or_default(),
        );
    }
    Ok(())
}
