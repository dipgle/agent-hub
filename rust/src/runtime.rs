//! What is running on this machine right now — daemon, accounts, recent errors.
//!
//! Hà asked for it in one line (2026-08-09): *"nên tạo 1 tool chụp được tình
//! trạng đang chạy, đã dừng, lỗi, options… liên tục để phản hồi lên ui"*. Until
//! now the phone could see SESSIONS but nothing about the thing watching them:
//! whether `hubd` was even alive, whether it would come back after a reboot,
//! which of the three accounts answered, what the last error was. Every one of
//! those questions had an answer on the machine and no way to reach a phone.
//!
//! Two rules shaped this module:
//!
//! * **Every cycle, not on a button.** A status page you have to ask for is a
//!   status page nobody reads. This runs inside `portal::push`, so it travels
//!   with the snapshot the page already polls.
//! * **Cheap enough to run every cycle.** Anything that spawns a process is
//!   cached (`SLOW_TTL_MS`); everything else is read from memory or the local
//!   database. A status collector that makes the daemon slower would be a
//!   status collector that changes what it measures.

use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};

use crate::config::Config;
use crate::db::Db;
use crate::exec::{run, RunOpts};
use crate::sessions::SessionsSnapshot;

/// Process spawns are cached this long. Autostart registration and account
/// login state change on the scale of days, not seconds.
const SLOW_TTL_MS: i64 = 10 * 60 * 1000;

static STARTED_AT: OnceLock<i64> = OnceLock::new();
static SLOW_CACHE: OnceLock<Mutex<Option<(i64, Value)>>> = OnceLock::new();

/// Called once by `hubd` at boot so "how long has it been up" is a fact rather
/// than a guess from the first cycle.
pub fn mark_start() {
    let _ = STARTED_AT.set(chrono::Utc::now().timestamp_millis());
}

/// Everything the phone needs to answer "is the thing watching my sessions
/// actually alive, and is anything broken?".
pub fn snapshot(cfg: &Config, db: &Db, live: &SessionsSnapshot) -> Value {
    let now = chrono::Utc::now().timestamp_millis();
    json!({
        "daemon": daemon_block(now),
        "accounts": accounts_block(cfg, live),
        "errors": errors_block(db),
        "slow": slow_block(cfg, now),
    })
}

fn daemon_block(now: i64) -> Value {
    let started = STARTED_AT.get().copied();
    json!({
        "pid": std::process::id(),
        // `None` means this snapshot was built by the CLI (`portal-push
        // --dry-run`), not by the daemon — say so instead of printing an uptime
        // of zero, which would read as "just crashed and restarted".
        "started_at_ms": started,
        "uptime_sec": started.map(|s| (now - s) / 1000),
    })
}

/// Per-account state, joined onto the sessions already listed this cycle.
///
/// The join matters: `sessions.notes` is where a failed `claude agents` lands,
/// and without pairing it with the account name a phone shows "3 accounts, 5
/// sessions" while one account has silently been logged out for a day.
fn accounts_block(cfg: &Config, live: &SessionsSnapshot) -> Value {
    let accounts = cfg.claude_accounts_or_ambient();
    let rows: Vec<Value> = accounts
        .iter()
        .map(|acc| {
            let mine: Vec<_> = live
                .sessions
                .iter()
                .filter(|s| s.account == acc.name)
                .collect();
            let alive = mine.iter().filter(|s| s.host != "dead").count();
            // A note is keyed by account name at the front (`"acc2: …"`), which
            // is how `sessions::snapshot` writes it.
            let note = live
                .notes
                .iter()
                .find(|n| n.starts_with(&format!("{}:", acc.name)))
                .cloned();
            let dir = acc
                .config_dir
                .as_ref()
                .map(|d| crate::config::expand_home(Path::new(d)));
            json!({
                "name": acc.name,
                // The PATH, never the contents: this travels to a server.
                "config_dir": dir.as_ref().map(|d| d.display().to_string()),
                "config_dir_exists": dir.as_ref().map(|d| d.exists()),
                "sessions": mine.len(),
                "alive": alive,
                "ok": note.is_none(),
                "note": note,
            })
        })
        .collect();
    Value::Array(rows)
}

/// The last handful of failed cycles, newest first.
///
/// Read from `runs`, not from the log file: the log is append-only text that
/// grows to megabytes, and tailing it every cycle would make the collector the
/// most expensive thing in the loop.
fn errors_block(db: &Db) -> Value {
    match db.last_runs(40) {
        Ok(rows) => {
            let bad: Vec<Value> = rows
                .into_iter()
                .filter(|r| r.ok == Some(0) || r.err.as_deref().is_some_and(|e| !e.is_empty()))
                .take(5)
                .map(|r| {
                    json!({
                        "at": r.started_at,
                        "adapter": r.adapter,
                        "phase": r.phase,
                        "err": r.err,
                    })
                })
                .collect();
            Value::Array(bad)
        }
        Err(e) => {
            crate::logging::warn("runtime_errors_unreadable", json!({ "err": e.to_string() }));
            Value::Array(vec![])
        }
    }
}

/// The part that costs a process spawn — cached.
fn slow_block(cfg: &Config, now: i64) -> Value {
    let cell = SLOW_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cell.lock() {
        if let Some((at, v)) = &*guard {
            if now - at < SLOW_TTL_MS {
                return v.clone();
            }
        }
    }
    let v = json!({
        "checked_at": now,
        "autostart": autostart_state(),
        "claude_cli": cfg.claude_cli.clone(),
        "auth": auth_block(cfg),
    });
    if let Ok(mut guard) = cell.lock() {
        *guard = Some((now, v.clone()));
    }
    v
}

/// **Tài khoản này là AI** — hỏi `claude auth status` cho từng cấu hình.
///
/// Hà, 2026-08-10, nhìn màn: *"danh sách acc đang thiếu nhiều thông tin"*. Đúng:
/// màn in `acc2 — 2/2 phiên đang sống`, còn ô chọn lúc mở phiên mới in trần
/// `acc1 (mặc định) · acc2 · acc3`. Ba cái tên ấy **không nói được acc nào là
/// hòm thư nào**, mà đó chính là lý do có ba tài khoản: chia hạn mức. Chọn
/// nhầm acc từ điện thoại thì phải mở terminal ra mới biết mình vừa chọn ai.
///
/// Lấy được: `logged_in`, `email`, `subscription`, `org`. **Không** lấy được
/// *hạn mức còn lại* — `claude` CLI không có lệnh nào trả nó ngoài phiên tương
/// tác (`/usage`), nên đừng bịa một con số ra màn.
///
/// Nằm trong khối chậm vì mỗi tài khoản là một lần spawn tiến trình; câu trả
/// lời đổi theo ngày chứ không theo giây. Tài khoản mặc định chọn bằng cách
/// KHÔNG có `CLAUDE_CONFIG_DIR` — trỏ nó vào `~/.claude` là một chuyện khác.
fn auth_block(cfg: &Config) -> Value {
    let mut map = serde_json::Map::new();
    for acc in cfg.claude_accounts_or_ambient() {
        let env = match acc.config_dir.as_ref() {
            Some(d) => vec![(
                "CLAUDE_CONFIG_DIR".to_string(),
                Some(
                    crate::config::expand_home(Path::new(d))
                        .display()
                        .to_string(),
                ),
            )],
            None => vec![("CLAUDE_CONFIG_DIR".to_string(), None)],
        };
        let out = run(
            &cfg.claude_cli,
            &["auth", "status"],
            RunOpts {
                timeout: Some(Duration::from_secs(20)),
                env,
                ..Default::default()
            },
        );
        let row = match out {
            Ok(r) => match serde_json::from_str::<Value>(r.stdout.trim()) {
                Ok(j) => json!({
                    "logged_in": j.get("loggedIn").and_then(Value::as_bool),
                    "email": j.get("email").and_then(Value::as_str),
                    "subscription": j.get("subscriptionType").and_then(Value::as_str),
                    "org": j.get("orgName").and_then(Value::as_str),
                }),
                // Trả lời không đọc được KHÁC với chưa đăng nhập. Ghi lại chữ
                // thật để màn nói "chưa hỏi được" thay vì dựng một tài khoản
                // hỏng ra từ chỗ không có gì.
                Err(e) => {
                    crate::logging::warn(
                        "claude_auth_status_unparsed",
                        json!({ "account": acc.name, "err": e.to_string(), "code": r.code }),
                    );
                    json!({ "err": r.stderr.lines().next().unwrap_or("không đọc được") })
                }
            },
            Err(e) => {
                crate::logging::warn(
                    "claude_auth_status_failed",
                    json!({ "account": acc.name, "err": e.to_string() }),
                );
                json!({ "err": e.to_string() })
            }
        };
        map.insert(acc.name.clone(), row);
    }
    Value::Object(map)
}

/// Will hub come back by itself after a reboot?
///
/// Measured 2026-08-09: the answer was NO — `deploy/com.dipgle.hubd.plist` sat
/// in the repo, never installed, and the daemon was alive only because someone
/// had started it by hand. That is exactly the kind of fact that is invisible
/// until the day it matters, so it belongs on the screen.
fn autostart_state() -> Value {
    let plist = crate::config::expand_home(Path::new(
        "~/Library/LaunchAgents/com.dipgle.hubd.plist",
    ));
    let installed = plist.exists();
    let loaded = if installed {
        match run(
            "launchctl",
            &["list"],
            RunOpts {
                timeout: Some(Duration::from_secs(10)),
                ..Default::default()
            },
        ) {
            Ok(r) if r.code == Some(0) => Some(r.stdout.contains("com.dipgle.hubd")),
            // A probe that failed is NOT a "no": say "unknown" rather than
            // telling the owner autostart is off when hub simply could not ask.
            Ok(_) => None,
            Err(e) => {
                crate::logging::warn("launchctl_probe_failed", json!({ "err": e.to_string() }));
                None
            }
        }
    } else {
        Some(false)
    };
    let (signature, stale) = installed_binary_state();
    json!({
        "plist_installed": installed,
        "loaded": loaded,
        "plist_path": plist.display().to_string(),
        "signature": signature,
        "stale": stale,
        "how_to_install":
            "deploy/install.sh && \
             cp ~/Documents/projects/AI/hub/deploy/com.dipgle.hubd.plist ~/Library/LaunchAgents/ && \
             launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.dipgle.hubd.plist",
    })
}

/// Đường dẫn bản hubd mà launchd chạy. KHÔNG phải bản `cargo` vừa build.
const INSTALLED_HUBD: &str = "~/Library/Application Support/hub/bin/hubd";

/// Hai câu hỏi mà thiết kế "cài bản đã ký ra đường riêng" vừa đẻ ra, và cả hai
/// đều im lặng nếu không ai hỏi:
///
/// 1. **Bản cài có còn mang chữ ký cố định không** (`cert`)? Nếu là `adhoc` thì
///    quyền TCC sẽ mất ở lần build kế tiếp — hub vẫn chạy ngon cho tới lúc khởi
///    động lại máy, rồi im.
/// 2. **Bản cài có cũ hơn bản vừa build không**? Đây là cái giá của việc tách
///    hai file: sửa mã, `cargo build`, test xanh, deploy trang — và daemon vẫn
///    đang chạy mã của hôm qua vì không ai chạy `deploy/install.sh`. Không có
///    dòng này thì không gì phát hiện ra.
///
/// Trả `None` cho câu nào không hỏi được, không đoán bừa (`unknown` ≠ `sai`).
fn installed_binary_state() -> (Value, Value) {
    let bin = crate::config::expand_home(Path::new(INSTALLED_HUBD));
    if !bin.exists() {
        return (Value::Null, Value::Null);
    }

    let signature = match run(
        "codesign",
        &["-d", "-r-", &bin.display().to_string()],
        RunOpts {
            timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        },
    ) {
        // codesign prints the requirement on stderr; `run` keeps them apart.
        Ok(r) => {
            let text = format!("{}{}", r.stdout, r.stderr);
            if text.contains("certificate root") {
                Value::from("cert")
            } else if text.contains("cdhash") {
                Value::from("adhoc")
            } else {
                crate::logging::warn(
                    "codesign_probe_unreadable",
                    json!({ "bin": bin.display().to_string(), "code": r.code }),
                );
                Value::Null
            }
        }
        Err(e) => {
            crate::logging::warn("codesign_probe_failed", json!({ "err": e.to_string() }));
            Value::Null
        }
    };

    (signature, stale_against_build())
}

/// Bản cài có còn là mã hiện tại không?
///
/// Hỏi **mã nguồn**, không hỏi sản phẩm build. Hai đường kia đều đã thử và đều
/// sai (đo 2026-08-10):
/// - *mtime của `target/release/hubd`*: `cargo test --release` link lại mỗi lượt
///   test, mtime nhảy dù mã không đổi.
/// - *cdhash của `target/release/hubd`*: tưởng ổn định vì build của Rust lặp
///   lại đúng byte, nhưng `cargo test --release` cho ra một binary KHÁC hẳn
///   (`2f624e8b…` so với `bbd8ba58…` của `cargo build --release`), và lệnh build
///   sau đó lại trả về hash cũ. Cùng một mã, hai câu trả lời.
///
/// Một cảnh báo kêu oan sau mỗi lượt test là một cảnh báo bị phớt lờ, tức tệ
/// hơn không có. Còn `.rs`/`Cargo.toml`/`Cargo.lock` thì chỉ đổi mtime khi có
/// người thật sự sửa — đúng câu hỏi cần trả lời: *sửa mã xong đã cài lại chưa?*
fn stale_against_build() -> Value {
    let bin = crate::config::expand_home(Path::new(INSTALLED_HUBD));
    let src = crate::config::expand_home(Path::new("~/Documents/projects/AI/hub/rust"));
    let Some(installed_at) = mtime(&bin) else {
        return Value::Null;
    };
    // Không có mã nguồn ở máy này (bản cài đem từ nơi khác) thì không có gì để
    // so — im lặng, đừng doạ.
    match newest_source_mtime(&src) {
        Some(changed_at) => Value::from(changed_at > installed_at),
        None => Value::Null,
    }
}

fn mtime(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).ok()?.modified().ok()
}

/// mtime mới nhất trong cây nguồn Rust: mọi `.rs` dưới `src/`, cộng
/// `Cargo.toml`/`Cargo.lock`. Bỏ qua `target/` — đó là sản phẩm, không phải mã.
fn newest_source_mtime(rust_dir: &Path) -> Option<std::time::SystemTime> {
    let mut newest: Option<std::time::SystemTime> = None;
    let mut take = |p: &Path| {
        if let Some(t) = mtime(p) {
            if newest.is_none_or(|n| t > n) {
                newest = Some(t);
            }
        }
    };
    take(&rust_dir.join("Cargo.toml"));
    take(&rust_dir.join("Cargo.lock"));

    // Duyệt tay thay vì kéo thêm một crate: cây này chỉ vài chục file.
    let mut stack = vec![rust_dir.join("src")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(p),
                Ok(_) if p.extension().is_some_and(|x| x == "rs") => take(&p),
                _ => {}
            }
        }
    }
    newest
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cái bẫy mà phép đo này sinh ra để tránh: đếm nhầm một file KHÔNG PHẢI mã
    /// nguồn (sản phẩm build, ghi chú) thành "mã vừa đổi", rồi báo daemon đã cũ
    /// sau mỗi lượt `cargo test`. Cảnh báo kêu oan là cảnh báo bị tắt.
    #[test]
    fn newest_source_mtime_counts_rust_sources_and_ignores_build_output() {
        let root = std::env::temp_dir().join(format!("hub-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/nested")).unwrap();
        std::fs::create_dir_all(root.join("target/release")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(root.join("src/lib.rs"), "// a").unwrap();

        // File mã nguồn viết SAU CÙNG phải là mốc mới nhất.
        std::fs::write(root.join("src/nested/late.rs"), "// b").unwrap();
        let expected = mtime(&root.join("src/nested/late.rs")).unwrap();
        assert_eq!(newest_source_mtime(&root), Some(expected));

        // Rồi ghi hai thứ KHÔNG phải mã nguồn, muộn hơn: một file trong
        // `target/` (sản phẩm build — `cargo test` đụng vào nó mỗi lượt) và một
        // file không phải `.rs`. Mốc không được nhúc nhích.
        std::fs::write(root.join("target/release/hubd.rs"), "// build output").unwrap();
        std::fs::write(root.join("src/notes.txt"), "// not code").unwrap();
        assert_eq!(
            newest_source_mtime(&root),
            Some(expected),
            "mốc bị kéo theo một file không phải mã nguồn"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Không có cây nguồn trên máy (bản cài đem từ nơi khác) thì trả `None` để
    /// màn nói "không rõ", KHÔNG phải `false` — đoán bừa là cách một phép đo
    /// biến thành một lời nói dối yên tâm.
    #[test]
    fn newest_source_mtime_is_unknown_when_there_is_no_tree() {
        let missing = std::env::temp_dir().join(format!("hub-rt-missing-{}", std::process::id()));
        assert_eq!(newest_source_mtime(&missing), None);
    }
}
