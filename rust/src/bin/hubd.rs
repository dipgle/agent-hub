//! hubd — the always-on loop. One process, one machine: a pid lock keeps two
//! daemons from double-replying to the same message.
//!
//!   hubd                                     run in the foreground
//!   launchctl load …com.dipgle.hubd.plist    run supervised (see README)
//!
//! Failures never kill the loop silently: each cycle error is logged, counted,
//! and backed off exponentially (up to 10 min), and after 5 consecutive
//! failures the local notify channel gets a heads-up.
//!
//! On SIGTERM/SIGINT the process dies with the default action and leaves its
//! lock file behind; the next start detects that the recorded pid is gone and
//! reclaims it. That is deliberately simpler than installing signal handlers —
//! no unsafe code anywhere in this crate.

use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use serde_json::json;

use hub::config;
use hub::db::Db;
use hub::exec::{run, RunOpts};
use hub::logging;
use hub::pipeline::run_once;

/// The one alarm that must reach a person who is not looking at the logs: a
/// file line plus a macOS banner.
///
/// Was `outbound::notify`, the "local channel" of a five-channel send path.
/// That module went with the inbox on 2026-08-08 and this is all that outlived
/// it, because the loop can still fail five times in a row while nobody reads
/// `hubd.out`. Errors here are returned, never swallowed — an alarm that fails
/// quietly is worse than no alarm.
fn notify(cfg: &hub::config::Config, subject: Option<&str>, body: &str) -> Result<()> {
    use std::fs::{create_dir_all, OpenOptions};
    use std::io::Write;

    if let Some(dir) = cfg.notify.file.parent() {
        create_dir_all(dir)?;
    }
    let entry = format!(
        "\n===== {} {} =====\n{}\n",
        logging::now_iso(),
        subject.unwrap_or(""),
        body
    );
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg.notify.file)
        .and_then(|mut f| f.write_all(entry.as_bytes()))
        .map_err(|e| anyhow::anyhow!("cannot append to {}: {e}", cfg.notify.file.display()))?;

    if cfg.notify.macos_notification && cfg!(target_os = "macos") {
        let raw = subject.unwrap_or(body);
        let text: String = raw
            .chars()
            .take(200)
            .map(|c| {
                if c == '"' || c == '\\' || c == '\n' {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        let script = format!("display notification \"{text}\" with title \"hub\"");
        match run(
            "osascript",
            &["-e", &script],
            RunOpts {
                timeout: Some(Duration::from_secs(10)),
                ..Default::default()
            },
        ) {
            Ok(r) if r.code != Some(0) => logging::warn(
                "osascript_notify_failed",
                json!({ "err": hub::exec::truncate(&r.stderr, 200) }),
            ),
            Err(e) => logging::warn("osascript_notify_failed", json!({ "err": e.to_string() })),
            _ => {}
        }
    }
    Ok(())
}

/// `kill -0 <pid>` — exit 0 means the process exists.
///
/// Fail CLOSED: if the probe itself fails (spawn error, timeout under load) we
/// report "alive" so a second daemon refuses to start. Guessing "dead" would
/// let two loops run, and two loops means paying twice and replying twice.
fn pid_alive(pid: &str) -> bool {
    match run(
        "kill",
        &["-0", pid],
        RunOpts {
            timeout: Some(Duration::from_secs(5)),
            ..Default::default()
        },
    ) {
        Ok(r) if r.timed_out => {
            logging::warn(
                "pid_check_timed_out",
                json!({ "pid": pid, "assumed": "alive" }),
            );
            true
        }
        Ok(r) => r.code == Some(0),
        Err(e) => {
            logging::warn(
                "pid_check_failed",
                json!({ "pid": pid, "err": e.to_string(), "assumed": "alive" }),
            );
            true
        }
    }
}

enum LockState {
    /// The file holds our pid.
    Ours,
    /// The file holds someone else's pid — a real takeover.
    Taken(String),
    /// The file could not be read at all, after retries.
    Unreadable(String),
}

/// Read the pid out of the lock, retrying a transient failure.
///
/// Three tries over ~1.5s: long enough to ride out a package swap under the
/// process tree, short enough that a real takeover is noticed within a cycle.
fn read_lock_pid(lock: &std::path::Path) -> LockState {
    let mut last = String::from("unknown");
    for attempt in 0..3 {
        match fs::read_to_string(lock) {
            Ok(s) => {
                let holder = s.trim().to_string();
                return if holder == std::process::id().to_string() {
                    LockState::Ours
                } else {
                    LockState::Taken(holder)
                };
            }
            Err(e) => {
                last = e.to_string();
                if attempt < 2 {
                    thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }
    LockState::Unreadable(last)
}

/// mtime as (secs, nanos); `None` when the file is missing or unreadable.
fn config_mtime(path: &std::path::Path) -> Option<(i64, u32)> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let dur = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some((dur.as_secs() as i64, dur.subsec_nanos()))
}

/// Chữ ký của chính binary này, trả lời đúng một câu: **quyền TCC có sống qua
/// lần build sau không?**
///
/// - `cert` — designated requirement neo theo *danh tính* (`identifier … and
///   certificate root = H"…"`). Build lại bao nhiêu lần cũng vẫn là một chương
///   trình, quyền còn nguyên. Đây là thứ `deploy/sign.sh` tạo ra.
/// - `adhoc` — `cargo` ký ad-hoc, requirement là `cdhash H"…"` tức vân tay của
///   ĐÚNG dãy byte ấy. Lần build tới macOS coi là chương trình khác và bản chạy
///   dưới launchd mất sạch Full Disk Access lẫn Automation. Thấy dòng này ở bản
///   launchd nghĩa là ai đó trỏ nó vào `target/release/` thay vì bản đã cài —
///   **chạy `deploy/install.sh`**.
///   (Bản chạy TAY trong terminal in `adhoc` là chuyện bình thường: nó mượn
///   quyền của terminal, không cần danh tính riêng.)
/// - `unreadable` — không đọc nổi chữ ký của chính mình trong 3 giây. Với binary
///   nằm dưới `~/Documents` thì đó gần như chắc chắn là TCC đang chặn, và cũng
///   là thứ sắp làm `config::load()` treo. Biết trước một dòng còn hơn một pid
///   nằm im.
///
/// Chạy trong luồng riêng có hạn giờ: phép đo này KHÔNG được phép trở thành một
/// chỗ treo mới ngay ở chỗ vừa dựng ra để chẩn đoán treo.
fn signature_kind() -> &'static str {
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let out = std::env::current_exe().ok().and_then(|exe| {
            std::process::Command::new("/usr/bin/codesign")
                .args(["-d", "-r-", "--"])
                .arg(exe)
                .output()
                .ok()
        });
        // codesign in requirement ra stderr; stdout để trống.
        let _ = tx.send(out.map(|o| {
            let mut s = String::from_utf8_lossy(&o.stderr).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stdout));
            s
        }));
    });
    match rx.recv_timeout(Duration::from_secs(3)) {
        Ok(Some(text)) if text.contains("certificate root") => "cert",
        Ok(Some(text)) if text.contains("cdhash") => "adhoc",
        _ => "unreadable",
    }
}

fn main() {
    // Dòng đầu tiên của cả tiến trình, in THẲNG ra stderr — chưa đọc cấu hình,
    // chưa mở sổ, chưa chạm vào `~/Documents`.
    //
    // Vì sao phải sớm đến thế (đo 2026-08-10): dưới launchd, một binary vừa
    // build lại KHÔNG còn quyền TCC vào `~/Documents` — macOS neo quyền theo
    // dấu vân tay binary, mà `cargo` ký ad-hoc với định danh sinh từ UUID nên
    // mỗi lần build là một chương trình khác trong mắt nó. Hệ quả không phải
    // một lỗi: lời gọi `getcwd()`/`open()` TREO CÂM chờ một hộp thoại không bao
    // giờ hiện cho tiến trình nền. Không log, không mã thoát, chỉ một pid nằm
    // im — thứ khó chẩn đoán nhất. Có dòng này thì phân biệt được ngay "chưa
    // vào nổi main" với "vào rồi nhưng kẹt ở quyền".
    eprintln!(
        "{{\"level\":\"info\",\"msg\":\"hubd_boot\",\"pid\":\"{}\"}}",
        std::process::id()
    );
    eprintln!(
        "{{\"level\":\"info\",\"msg\":\"hubd_signature\",\"kind\":\"{}\"}}",
        signature_kind()
    );
    if let Err(e) = real_main() {
        // `logging::error`, KHÔNG phải `eprintln!`: nó ghi ra CẢ stderr lẫn tệp
        // log khi tệp đã được đặt (`logging.rs`). Bản trước chỉ in stderr, mà
        // stderr của một daemon do launchd chạy thì chỉ nằm trong
        // `~/Library/Logs/hubd.err` — nơi không panel nào đọc và không ai theo
        // dõi. Hậu quả: hubd chết lúc dựng lên (mở được DB không, ghi được khoá
        // pid không) và LÝ DO không vào sổ nào cả.
        //
        // Hỏng trước khi `set_log_file` chạy thì vẫn ra stderr y như cũ — tức
        // dùng hàm này không mất gì, chỉ được thêm.
        logging::error(
            "hubd_fatal",
            serde_json::json!({ "err": logging::err_chain(&e) }),
        );
        std::process::exit(70);
    }
}

fn real_main() -> Result<()> {
    // Mốc khởi động: để màn nói được "chạy liên tục N phút" thay vì đoán từ
    // vòng đầu tiên.
    hub::runtime::mark_start();
    let cfg = config::load(None)?;
    logging::set_log_file(&cfg.log_file);
    if let Ok(level) = std::env::var("HUB_LOG_LEVEL") {
        logging::set_level_from_name(&level);
    }

    // launchd does not read your shell profile, so secrets come from
    // <hub_home>/hub.env (chmod 600). Names only in the log, never values.
    let loaded = config::load_env_file(&cfg.hub_home);
    if !loaded.is_empty() {
        logging::info("hub_env_loaded", json!({ "keys": loaded }));
    }

    let lock: PathBuf = cfg.hub_home.join("data").join("hubd.lock");
    if let Some(dir) = lock.parent() {
        fs::create_dir_all(dir)?;
    }

    let my_pid = std::process::id().to_string();
    if lock.exists() {
        let existing = fs::read_to_string(&lock)
            .unwrap_or_default()
            .trim()
            .to_string();
        if !existing.is_empty() && existing != my_pid && pid_alive(&existing) {
            logging::error(
                "hubd_already_running",
                json!({ "pid": existing, "lock": lock.display().to_string() }),
            );
            eprintln!(
                "hubd already running (pid {existing}); lock: {}",
                lock.display()
            );
            std::process::exit(3);
        }
        logging::warn(
            "stale_lock_removed",
            json!({ "lock": lock.display().to_string(), "pid": existing }),
        );
        // A removal failure surfaces immediately: the create_new below returns
        // the real error and the daemon refuses to start.
        let _ = fs::remove_file(&lock);
    }
    // create_new = O_EXCL: two daemons racing to claim the lock cannot both
    // win. The loser fails here, before it has ingested or spent anything.
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
            .map_err(|e| {
                anyhow::anyhow!(
                    "cannot take the lock at {} ({e}) — another hubd is starting",
                    lock.display()
                )
            })?;
        f.write_all(my_pid.as_bytes())?;
    }

    let db = Db::open(&cfg.db)?;

    // No console thread any more. `web.rs` served the inbox — a list, a detail
    // pane, approve/reject buttons and two spend charts — on 127.0.0.1 behind a
    // per-boot token. The inbox is gone (2026-08-08), and everything that
    // outlived it (sessions, health, config) is on the phone page, which is the
    // surface the owner actually uses. A second UI for the same three things is
    // a second thing to keep true.

    // The room pushes instead of hub asking: one socket held open, and a wake
    // so a message that lands mid-sleep does not wait out the poll interval.
    // The poller stays enabled as the durable backstop — see `live.rs`.
    // 🔴 Socket giữ mở với phòng chat tfl5 đã gỡ ngày 2026-08-14 (Hà: *"tạm
    // thời không dùng tfl5 để xem cứ xóa hết đi"*). `Waker` thì ở lại — nó là
    // đồng hồ ngủ của chính vòng này, không phải của kênh ấy; nhà mới:
    // `runtime::Waker`.
    let waker = hub::runtime::Waker::new();

    // Telegram làm KÊNH RA LỆNH, không chỉ cái loa (Hà 2026-08-11: *"làm việc
    // hoàn toàn qua kênh tele"*). Vòng đọc chạy nền; thiếu khoá thì nó tự bỏ
    // qua CÓ LOG, và mọi thứ khác vẫn chạy như cũ.
    //
    // Cùng cái `waker` của phòng chat: lệnh gõ trên Telegram phải chạy ngay như
    // nút bấm trên trang, không nằm chờ hết giấc ngủ 120 giây của vòng (đo
    // 2026-08-11: `/help` chờ 2 phút 16 giây trước khi có cái này).
    hub::telegram::Inbox::start(&cfg, Some(waker.clone()));

    logging::info(
        "hubd_started",
        json!({
            "pid": my_pid,
            "db": cfg.db.display().to_string(),
            "interval_sec": cfg.poll_interval_sec,
            "adapters": {
                "tfl5": cfg.adapters.tfl5.enabled,
            }
        }),
    );

    let mut cfg = cfg;
    let mut config_seen = config_mtime(&cfg.config_file);
    let mut consecutive_failures: u32 = 0;
    loop {
        // Re-read the config when the file changes. Without this, every switch
        // in the console (disable an adapter, lower daily_budget_usd, drop a
        // tier) reported success while the running loop kept its boot-time
        // copy — a kill-switch that does not switch anything off.
        let now_mtime = config_mtime(&cfg.config_file);
        if now_mtime != config_seen {
            match config::load(Some(&cfg.config_file.clone())) {
                Ok(fresh) => {
                    logging::info(
                        "config_reloaded",
                        json!({
                            "file": fresh.config_file.display().to_string(),
                            "poll_interval_sec": fresh.poll_interval_sec,
                        }),
                    );
                    cfg = fresh;
                }
                // Keep running on the last good config rather than dying: a
                // half-written file must not take the daemon down.
                Err(e) => logging::error(
                    "config_reload_failed",
                    json!({ "err": logging::err_chain(&e) }),
                ),
            }
            config_seen = now_mtime;
        }

        let mut delay = Duration::from_secs(cfg.poll_interval_sec);
        match run_once(&db, &cfg) {
            Ok(_) => consecutive_failures = 0,
            Err(e) => {
                consecutive_failures += 1;
                logging::error(
                    "cycle_failed",
                    json!({ "consecutive": consecutive_failures, "err": logging::err_chain(&e) }),
                );
                let backoff = cfg
                    .poll_interval_sec
                    .saturating_mul(1u64 << consecutive_failures.min(6));
                delay = Duration::from_secs(backoff.min(600));
                if consecutive_failures == 5 {
                    if let Err(e2) = notify(
                        &cfg,
                        Some("hubd: 5 cycles failed in a row"),
                        &format!("last error: {e}\nbacking off to {}s", delay.as_secs()),
                    ) {
                        logging::error("failure_notify_failed", json!({ "err": e2.to_string() }));
                    }
                }
            }
        }

        // If someone else took the lock (manual override), step aside — but
        // only when the file actually SAYS so.
        //
        // `unwrap_or(true)` used to treat "cannot read the lock" as "someone
        // took it", which killed the daemon twice on 2026-08-08: the `claude`
        // CLI auto-updated (11:16 and 15:16) and, for the seconds that took,
        // every file read in this process tree returned EPERM. A transient
        // filesystem error is not a takeover. Retry a few times, and only give
        // up when the file is readable and holds a different pid.
        match read_lock_pid(&lock) {
            LockState::Ours => {}
            LockState::Taken(other) => {
                logging::warn(
                    "hubd_lock_lost",
                    json!({ "lock": lock.display().to_string(), "holder": other }),
                );
                return Ok(());
            }
            LockState::Unreadable(err) => {
                // Loud, and keep going: dying here means the daemon is down
                // until somebody notices, for a blip that fixed itself.
                logging::error(
                    "hubd_lock_unreadable",
                    json!({ "lock": lock.display().to_string(), "err": err, "action": "tiếp tục chạy" }),
                );
            }
        }

        // Interruptible: a chat message arriving now starts the next cycle
        // immediately instead of sitting out up to `poll_interval_sec`.
        //
        // While somebody is reading a session on their phone, the sleep is cut
        // into slices so the stream can follow the file instead of waiting out
        // a two-minute cycle (UC-S03: "thấy đủ mà chậm hai phút thì vẫn không
        // giống ngồi máy").
        follow_sleep(&cfg, &db, &waker, delay);
    }
}



// 🔴 ĐÃ BỎ cả nhánh BÁM SÁT (`follow_sleep` + `idle_activity_sleep`),
// 2026-08-14, cùng lượt gỡ tfl5 theo lời Hà: *"tạm thời không dùng tfl5 để xem
// cứ xóa hết đi"*.
//
// Hai hàm ấy tồn tại vì đúng một lý do: **đẩy ảnh chụp lên trang tfl5 sớm hơn
// nhịp vòng** — theo dõi nhật ký phiên đang mở, đọc lại màn, và bơm một bản
// cập nhật mỗi khi có gì đổi. Trang đi thì việc ấy không còn ai đọc; giữ lại là
// đốt một lượt AppleScript mỗi 12 giây cho hư không.
//
// Thứ chúng phục vụ trên Telegram thì đã có đường riêng: cái loa "vừa xong /
// vừa tắt" (`watch.rs`) chạy trong chính vòng, và `/shot` đọc màn khi được hỏi.
fn follow_sleep(_cfg: &hub::config::Config, _db: &Db, waker: &hub::runtime::Waker, delay: Duration) {
    waker.sleep(delay);
}
