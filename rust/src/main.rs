//! hub — CLI for the comms hub.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use hub::config::{self, Config};
use hub::db::Db;
use hub::exec::{run, truncate, RunOpts};
use hub::logging;
use hub::pipeline::{known_projects, run_once};

#[derive(Parser)]
#[command(
    name = "hub",
    about = "hub — quản lý các phiên Claude CLI trên máy này, điều khiển từ Telegram",
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
    /// One cycle: run the orders that arrived, then the bookkeeping
    Once,
    // 🔴 `hub ingest` đã bỏ 2026-08-14 cùng chặng hỏi vòng: nó đọc phòng chat
    // tfl5, và Telegram thì tự đẩy tới chứ không chờ ai hỏi. `hub once` vẫn còn
    // — nó chạy nốt những gì đã tới, thứ vẫn có nghĩa.
    /// Spend hôm nay + những vòng chạy gần đây
    Status,
    /// Every Claude CLI session alive on this machine, across all accounts
    Sessions {
        /// Machine-readable, for the portal snapshot and for scripting
        #[arg(long)]
        json: bool,
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
    let chatty = matches!(cli.command, Command::Once);
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
        Command::Status => cmd_status(&db),
        Command::Sessions { json } => cmd_sessions(&db, &cfg, json),
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
    // Ẩn đi mà im lặng thì dòng đếm này nói dối. Trang điện thoại đã nói ra từ
    // 2026-08-09 (`fe/index.html`), còn dòng CLI thì chưa — mà đây mới là chỗ
    // người ngồi trước máy đối chiếu với `claude agents`.
    let mut hidden = match snap.hidden_editor {
        0 => String::new(),
        n => format!(
            "  · {n} phiên trong VS Code không liệt kê (không có cửa sổ Terminal để gõ vào)"
        ),
    };
    if snap.hidden_dead > 0 {
        hidden.push_str(&format!(
            "  · {} hàng không liệt kê (phiên tắt từ lâu, hoặc phép dò của chính hub)",
            snap.hidden_dead
        ));
    }
    println!("{} phiên đang sống  ({per}){hidden}\n", snap.sessions.len());

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
    // Telegram đứng TRƯỚC: nó là kênh chính từ 2026-08-11, và cho tới hôm nay
    // `doctor` không kiểm nó một dòng nào — người mới làm đúng theo README vẫn
    // không biết bot nối được chưa (xem `telegram::health`).
    let tg = hub::telegram::health(cfg);
    println!(
        "  {:<9} {}       {}",
        "telegram",
        if tg.ok { "OK  " } else { "FAIL" },
        truncate(&tg.detail, 90)
    );
    // 🔴 Vòng dò sức khoẻ các "adapter" đã bỏ 2026-08-14: chỉ còn đúng một
    // kênh, và nó vừa được in ngay phía trên. Một vòng lặp trên danh sách rỗng
    // in ra không dòng nào, nhưng nó để lại ấn tượng rằng hub còn nhiều kênh —
    // và cái ấn tượng ấy là thứ khiến người ta đi tìm một trang không còn tồn
    // tại.

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
    // "last polls" tới 2026-08-14 — mỗi hàng là một lượt hỏi phòng chat. Không
    // còn ai hỏi vòng, nên hàng nay là một VÒNG (`run_once`), và cái tên phải
    // đi theo: đọc "polls" trên một máy không poll gì cả là đọc sai.
    println!("last cycles:");
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
