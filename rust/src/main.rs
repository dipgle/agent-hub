//! hub — CLI for the comms hub.

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

use hub::config::{self, Config};
use hub::db::Db;
use hub::exec::{run, truncate, RunOpts};
use hub::logging;
use hub::pipeline::{ingest, known_projects, run_once};

#[derive(Parser)]
#[command(
    name = "hub",
    about = "hub — quản lý các phiên Claude CLI trên máy này, điều khiển từ phòng chat tfl5",
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
    /// Check the channel + secrets, honestly
    Doctor,
    /// Dựng lại chính hub: build → ký → cài → khởi động lại launchd
    ///
    /// Hà 2026-08-13: *"tại sao không phải là luồng chạy độc lập trên rust, tức
    /// là mọi lệnh và luồng xử lý phải nằm trong binary"*. Đây là bản Rust của
    /// `deploy/install.sh`, giữ nguyên hai bước không được bỏ (ký bằng chứng
    /// chỉ, cài ra đường riêng ngoài tầm với của cargo) — xem
    /// `runtime::self_install`.
    SelfInstall {
        /// Cài xong thì KHÔNG khởi động lại (để tự bấm sau)
        #[arg(long)]
        no_restart: bool,
    },
    /// Mở trang cấu hình ngay trên máy này rồi ghi hub.env (chmod 600)
    ///
    /// Hà 2026-08-13: *"đóng gói hub thành app và cài trên máy có ui để cấu
    /// hình biến môi trường"*. Trang chạy ở 127.0.0.1, đóng ngay sau khi lưu.
    Setup,
    /// Write hub.config.json + create the db
    Init {
        #[arg(long)]
        force: bool,
    },
    /// One cycle: read the room, run the orders in it, push the snapshot
    Once,
    /// Poll the room now
    Ingest,
    /// Counts, poll health, spend
    Status,
    /// Post a message into the tfl5 chat room (hub dials out; nothing listens)
    Tfl5Say {
        /// The message text
        text: Vec<String>,
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
    /// show the sessions and health next to the conversation
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
    let chatty = matches!(cli.command, Command::Once | Command::Ingest);
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
        Command::SelfInstall { no_restart } => {
            println!("{}", hub::runtime::self_install(&cfg)?);
            if no_restart {
                println!("chưa khởi động lại (--no-restart)");
            } else {
                println!("đã khởi động lại {}", hub::runtime::restart_daemon()?);
            }
            Ok(())
        }
        Command::Setup => hub::setup::serve(&cfg.hub_home),
        Command::Init { force } => cmd_init(&cfg, force),
        Command::Once => {
            println!("{}", serde_json::to_string_pretty(&run_once(&db, &cfg)?)?);
            Ok(())
        }
        Command::Ingest => {
            println!("{}", serde_json::to_string_pretty(&ingest(&db, &cfg)?)?);
            Ok(())
        }
        Command::Status => cmd_status(&db),
        Command::Sessions { json } => cmd_sessions(&db, &cfg, json),
        Command::Tfl5Say { text } => cmd_tfl5_say(&cfg, &text.join(" ")),
        Command::Tfl5Tail { limit } => cmd_tfl5_tail(&cfg, limit),
        Command::PortalPush { dry_run } => cmd_portal_push(&db, &cfg, dry_run),
    }
}

/// What `claude` is doing on this machine right now. Read-only: this lists and
/// reads transcripts, it never starts, stops, or types into a session.
fn cmd_sessions(db: &hub::db::Db, cfg: &Config, as_json: bool) -> Result<()> {
    let mut snap = hub::sessions::snapshot(cfg);
    // Cùng một nguồn sự thật với ảnh chụp: màn và CLI không được nói khác nhau
    // về việc ai mở phiên nào.
    hub::pipeline::mark_started_by_hub(db, &mut snap);
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
        let snap = hub::portal::build(db, cfg)?;
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

    // The registry, checked against the folders it claims. A name with no
    // folder used to be invisible: context came back empty and hub answered
    // anyway, so a typo read exactly like a quiet project.
    if !cfg.projects.is_empty() {
        println!();
        println!("projects:");
        for (name, p) in &cfg.projects {
            let dir = config::project_dir(cfg, name);
            let bits = [p.note.clone()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            match dir {
                Some(d) => println!("  {name:<14} OK   {}{}", d.display(), if bits.is_empty() { String::new() } else { format!("  ({bits})") }),
                None => println!("  {name:<14} SAI  không có thư mục nào tên '{name}' trong project dirs — /new sẽ không mở được phiên ở đó"),
            }
        }
    }
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
        "một lần gọi  trần ${}  ·  tối đa {}s",
        cfg.call.max_budget_usd, cfg.call.timeout_sec
    );
    println!();

    println!("channels:");
    for name in hub::pipeline::ADAPTER_NAMES {
        // Same table the ingest loop uses — see `pipeline::adapter_enabled`.
        if !hub::pipeline::adapter_enabled(cfg, name) {
            println!("  {name:<9} off        (adapters.{name}.enabled = false)");
            continue;
        }
        let h = hub::adapters::tfl5::health(&cfg.adapters.tfl5);
        println!(
            "  {name:<9} {}       {}",
            if h.ok { "OK  " } else { "FAIL" },
            truncate(&h.detail, 90)
        );
    }

    println!();
    let projects = known_projects(cfg);
    println!(
        "thư mục dự án nhận ra được: {}",
        if projects.is_empty() {
            "(none found)".to_string()
        } else {
            projects.join(", ")
        }
    );
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    println!("spend hôm nay: ${:.4}", db.owner_cost_on_day(&today)?);
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

fn cmd_status(db: &Db) -> Result<()> {
    // No message/decision/outbox counts: those tables belonged to the inbox and
    // nothing writes them any more. What is left is what hub actually does —
    // what the owner's own calls cost, and whether the room is being read.
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    println!("spend hôm nay  ${:.4}", db.owner_cost_on_day(&today)?);
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
