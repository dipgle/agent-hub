//! What is running on this machine right now — daemon, accounts, recent errors.
//!
//! Hà asked for it in one line (2026-08-09): *"nên tạo 1 tool chụp được tình
//! trạng đang chạy, đã dừng, lỗi, options… liên tục để phản hồi lên ui"*. Until
//! now the phone could see SESSIONS but nothing about the thing watching them:
//! whether `hubad` was even alive, whether it would come back after a reboot,
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

use std::path::{Path, PathBuf};
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

/// Hạn mức đổi nhanh hơn "plist đã cài chưa", và Hà chỉ cần 5 phút một lần.
const USAGE_TTL_MS: i64 = 5 * 60 * 1000;

static STARTED_AT: OnceLock<i64> = OnceLock::new();
static SLOW_CACHE: OnceLock<Mutex<Option<(i64, Value)>>> = OnceLock::new();
static USAGE_CACHE: OnceLock<Mutex<Option<(i64, Value)>>> = OnceLock::new();
/// Đang có một luồng đi hỏi hạn mức hay chưa — để vòng chạy kế tiếp không đẻ
/// thêm ba tiến trình `claude` nữa trong lúc lượt trước còn dở.
static USAGE_REFRESHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Called once by `hubad` at boot so "how long has it been up" is a fact rather
/// than a guess from the first cycle.
/// Cho phép một luồng nói với vòng chạy: *"có thứ vừa tới, đừng ngồi hết giấc
/// ngủ nữa"*.
///
/// 🔴 Chuyển từ `live.rs` sang đây ngày 2026-08-14, khi Hà chốt bỏ trang tfl5:
/// *"tạm thời không dùng tfl5 để xem cứ xóa hết đi"*. `live.rs` là cái socket
/// giữ mở với phòng chat tfl5 — nó đi theo kênh ấy. Nhưng `Waker` thì không:
/// hubad dùng nó để NGỦ (`waker.sleep(slice)`) ở ba chỗ, và Telegram dùng nó để
/// cắt giấc ngủ khi có lệnh.
///
/// Vì sao cái cắt giấc ngủ ấy đáng giữ, dù `run_telegram_now` đã chạy lệnh ngay
/// ở luồng riêng: lệnh chạy xong thì ẢNH CHỤP vẫn cũ cho tới vòng sau, nên cái
/// loa "vừa xong / vừa tắt" và trang trạng thái đi sau thực tế tới 120 giây.
/// Đánh thức là để phần CÒN LẠI của huba bắt kịp thứ vừa xảy ra.
#[derive(Default)]
pub struct Waker {
    inner: std::sync::Mutex<bool>,
    cv: std::sync::Condvar,
}

impl Waker {
    pub fn new() -> std::sync::Arc<Waker> {
        std::sync::Arc::new(Waker::default())
    }

    pub fn wake(&self) {
        *self.inner.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.cv.notify_all();
    }

    /// Ngủ tối đa `d`, trả về **true** khi bị đánh thức sớm.
    ///
    /// Chỗ gọi cần biết là cái nào: vòng bám sát ngủ thành từng lát, và một lát
    /// kết thúc vì có tin tới thì phải trả quyền cho một vòng đầy đủ chứ không
    /// tích tiếp. Cờ được xoá ngay sau khi đọc — một cú đánh thức giữa chừng
    /// không bị mất, nhưng cũng không bị tính hai lần.
    pub fn sleep(&self, d: Duration) -> bool {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let (mut flagged, _) = self
            .cv
            .wait_timeout_while(guard, d, |woken| !*woken)
            .unwrap_or_else(|e| e.into_inner());
        let woken = *flagged;
        *flagged = false;
        woken
    }
}

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
        // Nhịp RIÊNG, ngắn hơn khối chậm: Hà xin 5 phút cho hạn mức, còn
        // "plist đã cài chưa" thì đổi theo ngày. Gộp chung là hoặc hỏi
        // `launchctl` nhiều gấp đôi cần thiết, hoặc để hạn mức cũ gấp đôi.
        "usage": usage_cached(cfg, now),
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

/// `/accounts` — ba tài khoản trên máy này, nói thành câu cho điện thoại.
///
/// Hà 2026-08-12: *"chưa có lệnh xem danh sách acc"*, rồi ngay sau đó *"vậy
/// lệnh new chọn acc kiểu gì? hay đang để random?"*. Câu thứ hai là lý do câu
/// thứ nhất đáng làm: **không random** — `/new` không mang `@acc` thì chạy bằng
/// tài khoản KHÔNG đặt `CLAUDE_CONFIG_DIR` (`sessions::terminal_command`), tức
/// luôn luôn một tài khoản duy nhất. Cái đó phải NÓI RA trên màn, vì hậu quả
/// của nó (tuần cạn hạn mức thì phiên mới chết giữa chừng) chỉ lộ ra về sau.
///
/// Hạn mức lấy từ SỔ của chính CLI (`quota::read_all` — đọc tệp, không spawn) và
/// từ bản dò đã đo sẵn (`usage_cached`, 5 phút một lượt). Không đẻ thêm một tiến
/// trình `claude` nào cho một lệnh xem.
pub fn accounts_say(cfg: &Config, live: &SessionsSnapshot, now: i64) -> String {
    accounts_text(
        cfg,
        live,
        &usage_cached(cfg, now),
        &crate::quota::read_all(cfg),
    )
}

/// Phần dựng câu, tách khỏi phần đi đo — để test được mà không spawn `claude`.
///
/// Một hàm vừa gọi tiến trình vừa dựng chữ thì test của nó hoặc phải chạy thật
/// (chậm, phụ thuộc máy) hoặc không có test. Ranh giới đặt ở đây vì `usage` và
/// `quotas` là hai thứ DUY NHẤT phải đi hỏi ra ngoài.
///
/// 🔴 `quotas` là THAM SỐ, không phải một lượt `quota::read` gọi tại chỗ — và
/// tôi vừa phá đúng ranh giới ấy một lần trong ngày 30/08. Cho hàm này tự đọc
/// `$HOME/.claude*.json` làm nó phụ thuộc máy đang chạy, và bài kiểm
/// `usage_still_being_measured_says_so_instead_of_showing_zero` đỏ ngay: nó chấm
/// *"không được bịa 0%"* trên một câu nay mang `5 tiếng 0%` thật của acc2. Cổng
/// bắt đúng chỗ, và cái nó bắt không phải con chữ — là ranh giới.
pub fn accounts_text(
    cfg: &Config,
    live: &SessionsSnapshot,
    usage: &Value,
    quotas: &[crate::quota::Quota],
) -> String {
    let pending = usage.get("pending").and_then(Value::as_bool) == Some(true);
    let per_acc = usage.get("accounts");
    let accounts = cfg.claude_accounts_or_ambient();

    let mut out = format!("👤 {} tài khoản claude\n", accounts.len());
    for acc in &accounts {
        let mine: Vec<_> = live
            .sessions
            .iter()
            .filter(|s| s.account == acc.name && s.host != "dead")
            .collect();
        // Tài khoản mặc định = tài khoản KHÔNG có `config_dir`. Đó chính là
        // định nghĩa `sessions::account_dir` dùng, nên đọc cùng một chỗ.
        let is_default = acc.config_dir.as_deref().unwrap_or("").is_empty();
        out.push_str(&format!(
            "\n{}{} · {}\n",
            acc.name,
            if is_default {
                " ⭐ mặc định của /new"
            } else {
                ""
            },
            match mine.len() {
                0 => "không có phiên nào".to_string(),
                n => format!(
                    "{n} phiên: {}",
                    mine.iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        ));
        // Tài khoản KHÔNG liệt kê được phiên ở lượt này: nói thẳng, vì con số
        // "0 phiên" ở trên là con số của một phép đo hỏng — xem `watch::Mark::a`.
        if live.blind.contains(&acc.name) {
            let why = live
                .notes
                .iter()
                .find(|n| n.starts_with(&format!("{}:", acc.name)))
                .and_then(|n| n.split_once(':').map(|x| x.1.trim().to_string()))
                .unwrap_or_default();
            out.push_str(&format!(
                "    ⚠ lượt này KHÔNG liệt kê được phiên — con số trên không đáng tin: {why}\n"
            ));
        }
        // 🔴 HẠN MỨC ĐỌC TỪ SỔ CỦA CHÍNH CLI — thêm 2026-08-30, và nó là dòng
        // hạn mức DUY NHẤT thật sự tới được màn.
        //
        // Đường cũ (`per_acc`, từ `claude -p "/usage"`) treo tới trần 60 giây và
        // trả 0 byte từ 12/08; đo trên nhật ký thì `/accounts` chạy đúng ba lần
        // trong đời và cả ba lần in *"hạn mức: đang đo, hỏi lại sau một phút"*.
        // Một dòng không bao giờ có số thì không phải một phép đo.
        //
        // Dòng mới đọc tệp, nên nó có số ngay — và nó in kèm TUỔI của số ấy, vì
        // đây là chỗ chủ máy soi lại luật chọn tài khoản (`watch::suggest_account`).
        // Không có bản đọc cho tài khoản này thì IM ở dòng ấy — im khác hẳn với
        // in ra một câu "chưa đo được" mà chính chỗ gọi chưa hề đi đo.
        if let Some(q) = quotas.iter().find(|q| q.account == acc.name) {
            out.push_str(&format!("    hạn mức: {}\n", q.say(crate::quota::now_ms())));
        }
        let row = per_acc.and_then(|m| m.get(&acc.name));
        match row {
            // Nhãn khác hẳn dòng trên, cố ý: hai NGUỒN khác nhau cho cùng một
            // câu hỏi thì phải đọc ra là hai dòng, không thì lúc chúng lệch nhau
            // người đọc không biết tin cái nào.
            Some(v) if v.get("err").is_some() => out.push_str(&format!(
                "    (dò /usage: chưa đo được — {})\n",
                v.get("err").and_then(Value::as_str).unwrap_or("")
            )),
            Some(v) => {
                let mut parts: Vec<String> = Vec::new();
                if let Some(p) = v.get("week_pct").and_then(Value::as_u64) {
                    parts.push(format!("tuần {p}%"));
                }
                if let Some(p) = v.get("session_pct").and_then(Value::as_u64) {
                    parts.push(format!("phiên {p}%"));
                }
                if let (Some(n), Some(p)) = (
                    v.get("week_model_name").and_then(Value::as_str),
                    v.get("week_model_pct").and_then(Value::as_u64),
                ) {
                    parts.push(format!("{n} {p}%"));
                }
                if parts.is_empty() {
                    // `parse_usage` giữ nguyên câu thô khi lời của CLI đổi —
                    // thà một dòng thô còn hơn một con số bịa.
                    if let Some(raw) = v.get("raw").and_then(Value::as_str) {
                        parts.push(raw.to_string());
                    }
                }
                if !parts.is_empty() {
                    out.push_str(&format!("    (dò /usage: {})\n", parts.join(" · ")));
                }
            }
            // "Chưa đo xong" KHÁC "đã đo và bằng 0". Nói đúng cái đang có.
            // Giữ nguyên chữ "đang đo": nó là thứ phân biệt *"chưa đo xong"* với
            // *"đã đo và bằng 0"*, và có một bài kiểm đứng trên đúng con chữ ấy.
            None if pending => out.push_str("    (dò /usage: đang đo, chưa có số)\n"),
            None => {}
        }
    }
    out.push_str("\nMở phiên bằng tài khoản khác: /new -a acc2 -s dwork [việc]");
    out
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
        "autostart": autostart_state(cfg),
        "claude_cli": cfg.claude_cli.clone(),
        "auth": auth_block(cfg),
    });
    if let Ok(mut guard) = cell.lock() {
        *guard = Some((now, v.clone()));
    }
    v
}

/// Hạn mức, cache 5 phút — và **không bao giờ bắt vòng chạy đứng đợi**.
///
/// Hết hạn thì trả BẢN CŨ ngay rồi cho một luồng riêng đi hỏi lại. Vì sao không
/// hỏi tại chỗ: ba lần spawn `claude` kéo một vòng lên **80 giây** (đo
/// 2026-08-10 ngay sau khi thêm), mà mỗi vòng là một nhịp huba đọc lệnh từ điện
/// thoại — nên cái giá không phải "số liệu chậm 30 giây" mà là "lệnh của chủ máy
/// nằm chờ hơn một phút". Đúng bài học đã ghi trong `CLAUDE.md` đêm trước về
/// luật tự đóng sổ (90s → 3,2s), và tôi vừa tái phạm bằng một phép dò mới.
///
/// Một số liệu trễ 5 phút mà màn vẫn mượt thì tốt hơn một số liệu tươi mà cả
/// huba khựng lại.
fn usage_cached(cfg: &Config, now: i64) -> Value {
    let cell = USAGE_CACHE.get_or_init(|| Mutex::new(None));
    let cached = cell.lock().ok().and_then(|g| g.clone());
    if let Some((at, v)) = &cached {
        if now - at < USAGE_TTL_MS {
            return v.clone();
        }
    }

    // Chỉ một luồng làm mới tại một thời điểm: vòng chạy tới trước khi lượt
    // trước xong thì cứ dùng bản cũ, đừng đẻ thêm ba tiến trình `claude` nữa.
    if !USAGE_REFRESHING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let cfg = cfg.clone();
        std::thread::spawn(move || {
            let v = json!({
                "checked_at": chrono::Utc::now().timestamp_millis(),
                "accounts": usage_block(&cfg),
            });
            if let Ok(mut guard) = USAGE_CACHE.get_or_init(|| Mutex::new(None)).lock() {
                *guard = Some((chrono::Utc::now().timestamp_millis(), v));
            }
            USAGE_REFRESHING.store(false, std::sync::atomic::Ordering::SeqCst);
        });
    }

    // Lần đầu tiên thì chưa có gì để trả. Nói "đang đo" chứ đừng trả một đối
    // tượng rỗng trông y hệt "đã đo xong và mọi thứ bằng 0".
    cached
        .map(|(_, v)| v)
        .unwrap_or_else(|| json!({ "pending": true }))
}

/// **Còn bao nhiêu hạn mức** — hỏi `claude -p "/usage"` cho từng tài khoản.
///
/// Hà 2026-08-10: *"thông tin tài khoản không có thông tin usage?"*. Không có
/// thật, và ba đường tĩnh đều đã thử rồi loại bằng đo đạc:
/// - `claude auth status` chỉ có hòm thư + gói, không có hạn mức;
/// - `claude auth --help` chỉ có `login/logout/status`;
/// - nhật ký phiên chỉ ghi token TỪNG LƯỢT (`usage.input_tokens`…), không ghi
///   cửa sổ giới hạn; `stats-cache.json` thì cũ và chỉ có ở một tài khoản.
///   Và cả ba tài khoản **dùng chung một kho nhật ký** (`~/.claude-accN/projects`
///   là symlink tới `~/.claude/projects`), nên không thể tách token theo tài
///   khoản bằng thư mục.
///
/// Đường lấy được số THẬT: `claude -p "/usage" --output-format json`. Đo
/// 2026-08-10 — `num_turns: 0`, `duration_api_ms: 0`, `total_cost_usd: 0`: đây
/// là lệnh phía client, **không gọi model, không tiêu hạn mức**. Trả về đúng cái
/// Claude tự tính:
///
/// ```text
/// Current session: 6% used · resets Aug 10 at 1:29pm (Asia/Saigon)
/// Current week (all models): 98% used · resets Aug 11 at 12:59pm
/// Current week (Fable): 50% used · resets Aug 11 at 1pm
/// ```
///
/// Cache 5 phút (Hà: *"không cần thường xuyên, chỉ cần 5p 1 lần là được"*) —
/// mỗi lượt là ba lần spawn tiến trình, nên đừng gắn nó vào mỗi vòng poll.
fn usage_block(cfg: &Config) -> Value {
    let mut map = serde_json::Map::new();
    for acc in cfg.claude_accounts_or_ambient() {
        let env = account_env(&acc);
        let out = run(
            &cfg.claude_cli,
            &["-p", "/usage", "--output-format", "json"],
            RunOpts {
                timeout: Some(Duration::from_secs(60)),
                env,
                ..Default::default()
            },
        );
        let row = match out {
            Ok(r) => {
                let text = serde_json::from_str::<Value>(r.stdout.trim())
                    .ok()
                    .and_then(|v| v.get("result").and_then(Value::as_str).map(str::to_string));
                match text {
                    Some(t) => parse_usage(&t),
                    None => {
                        // Một dòng "không đọc được" gộp hai chuyện khác hẳn
                        // nhau, và tôi đã đoán nhầm vì đúng chỗ này (đo
                        // 2026-08-12: hubad hỏng cả ba tài khoản trong khi chạy
                        // tay thì 6 giây ra đủ số — `code: null` hoá ra là HẾT
                        // GIỜ, không phải câu trả lời khó hiểu). `RunOut` đã
                        // mang sẵn `timed_out` và `ms`; dòng log cũ vứt đi cả
                        // hai. Không log nội dung stdout: nó mang email tài
                        // khoản và số hạn mức.
                        crate::logging::warn(
                            "usage_probe_unparsed",
                            json!({
                                "account": acc.name,
                                "code": r.code,
                                "timed_out": r.timed_out,
                                "ms": r.ms as u64,
                                "stdout_bytes": r.stdout.len(),
                                "stderr": crate::exec::truncate(r.stderr.trim(), 200),
                            }),
                        );
                        let err = if r.timed_out {
                            format!("/usage hết giờ sau {}ms", r.ms)
                        } else {
                            "không đọc được câu trả lời của /usage".to_string()
                        };
                        json!({ "err": err })
                    }
                }
            }
            Err(e) => {
                crate::logging::warn(
                    "usage_probe_failed",
                    json!({ "account": acc.name, "err": e.to_string() }),
                );
                json!({ "err": e.to_string() })
            }
        };
        map.insert(acc.name.clone(), row);
    }
    Value::Object(map)
}

/// Bóc ba dòng phần trăm ra khỏi câu trả lời của `/usage`.
///
/// Giữ nguyên `raw` khi không khớp: câu chữ của CLI có thể đổi, và lúc ấy thà
/// hiện một dòng thô còn hơn hiện `0%` — một con số bịa trông y hệt một con số
/// thật, và đây là con số dùng để quyết định mở phiên bằng tài khoản nào.
fn parse_usage(text: &str) -> Value {
    let mut out = serde_json::Map::new();
    let mut hit = false;
    for line in text.lines() {
        let line = line.trim();
        let Some((head, rest)) = line.split_once(':') else {
            continue;
        };
        let Some(pct_str) = rest.split('%').next() else {
            continue;
        };
        let Ok(pct) = pct_str.trim().parse::<u32>() else {
            continue;
        };
        let resets = rest
            .split_once("resets")
            .map(|(_, r)| r.trim().trim_end_matches('.').to_string());
        let key = match head.trim() {
            "Current session" => "session",
            h if h.starts_with("Current week") && h.contains("all models") => "week",
            h if h.starts_with("Current week") => "week_model",
            _ => continue,
        };
        hit = true;
        out.insert(format!("{key}_pct"), json!(pct));
        if let Some(r) = resets {
            out.insert(format!("{key}_resets"), json!(r));
        }
        if key == "week_model" {
            // "Current week (Fable)" → "Fable"
            if let Some(name) = head.split_once('(').and_then(|(_, n)| n.split_once(')')) {
                out.insert("week_model_name".into(), json!(name.0));
            }
        }
    }
    if !hit {
        return json!({ "raw": text.lines().take(3).collect::<Vec<_>>().join(" · ") });
    }
    Value::Object(out)
}

/// Môi trường chọn tài khoản cho một lời gọi `claude`.
///
/// Tài khoản mặc định chọn bằng cách KHÔNG có `CLAUDE_CONFIG_DIR` — trỏ nó vào
/// `~/.claude` là một chuyện khác. Đo được ngay ngày đầu: gọi tay trong shell
/// đang set `CLAUDE_CONFIG_DIR=acc3` thì "acc1" trả về hòm thư của acc3.
fn account_env(acc: &crate::config::ClaudeAccountCfg) -> Vec<(String, Option<String>)> {
    match acc.config_dir.as_ref() {
        Some(d) => vec![(
            "CLAUDE_CONFIG_DIR".to_string(),
            Some(
                crate::config::expand_home(Path::new(d))
                    .display()
                    .to_string(),
            ),
        )],
        None => vec![("CLAUDE_CONFIG_DIR".to_string(), None)],
    }
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
        let env = account_env(&acc);
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

/// Will huba come back by itself after a reboot?
///
/// Measured 2026-08-09: the answer was NO — `com.dipgle.hubd.plist` sat
/// in the repo, never installed, and the daemon was alive only because someone
/// had started it by hand. That is exactly the kind of fact that is invisible
/// until the day it matters, so it belongs on the screen.
fn autostart_state(cfg: &Config) -> Value {
    let plist =
        crate::config::expand_home(Path::new("~/Library/LaunchAgents/com.dipgle.hubd.plist"));
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
            // telling the owner autostart is off when huba simply could not ask.
            Ok(_) => None,
            Err(e) => {
                crate::logging::warn("launchctl_probe_failed", json!({ "err": e.to_string() }));
                None
            }
        }
    } else {
        Some(false)
    };
    let (signature, stale) = installed_binary_state(cfg);
    json!({
        "plist_installed": installed,
        "loaded": loaded,
        "plist_path": plist.display().to_string(),
        "signature": signature,
        "stale": stale,
        // Câu hướng dẫn phải trỏ vào cây mã ĐANG chạy, không phải một đường dẫn
        // gõ cứng: gốc workspace đã đổi một lần (2026-08-12), và một dòng hướng
        // dẫn cũ thì bảo chủ máy cài đè plist của thư mục cũ.
        // 🔴 Tên đổi 2026-08-16 (Hà: *"xóa deploy đi sửa thành
        // /huba/install_update.sh"*): workspace CHẶN mọi lệnh Bash nêu một tệp
        // có chữ ấy trong tên, nên dòng hướng dẫn cũ là một dòng không ai gõ
        // được — kể cả chủ máy dán lại nó vào phiên.
        "how_to_install": format!(
            "{home}/install_update.sh && \
             cp {home}/com.dipgle.hubd.plist ~/Library/LaunchAgents/ && \
             launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.dipgle.hubd.plist",
            home = cfg.hub_home.display()
        ),
    })
}

/// Tự dựng lại chính mình: build → ký → cài → (khởi động lại ở bước riêng).
///
/// 🔴 Hà 2026-08-13: *"sao lại cần install.sh để chạy, tại sao không phải là
/// luồng chạy độc lập trên rust, tức là mọi lệnh và luồng xử lý phải nằm trong
/// binary"*. Phần "logic nằm ngoài" thì không đúng — `install.sh` không xử lý
/// nghiệp vụ gì cả. Nhưng phần LÕI của câu hỏi thì đúng, và nó là một lỗ hổng
/// của cây cầu: **huba không tự cài được chính nó**, nên mỗi bản vá đều phải có
/// người ngồi ở máy gõ một dòng — đúng thứ mà cả dự án này sinh ra để bỏ đi.
///
/// Ba bước dưới đây chép nguyên `install_update.sh`, và giữ nguyên hai lý do
/// tồn tại của nó (đo 2026-08-10, xem `sign.sh`):
/// - **ký bằng chứng chỉ**, vì `cargo` ad-hoc-ký lại mỗi lần link và TCC gắn
///   quyền theo *danh tính chữ ký* — mất chữ ký là mất quyền, im lặng, chỉ lộ
///   ở lần khởi động máy sau;
/// - **cài ra đường riêng**, ngoài tầm với của cargo, để `cargo test` không
///   xoá mất chữ ký ấy.
///
/// KHÔNG đụng vào bản đang cài nếu bất kỳ bước nào hỏng: build hỏng, không tìm
/// thấy danh tính ký, hay chữ ký ra không phải `certificate root` thì dừng tại
/// chỗ, xoá bản tạm, và bản đang chạy còn nguyên.
/// Đường tới `cargo`, KHÔNG tin vào `PATH`.
///
/// 🔴 Hà 2026-08-14 gõ `/upgrade` lúc 08:26 và bản cài không đổi. huba báo đúng
/// chứ không im — *"⚠ không dựng lại được (bản đang chạy GIỮ NGUYÊN): spawn
/// cargo failed: No such file or directory"* — nhưng tin ấy trôi mất giữa mấy
/// tin khác, nên nhìn từ điện thoại y hệt một cái lệnh không làm gì.
///
/// Gốc: `hubad` chạy dưới launchd, và `PATH` của nó là dòng khai trong plist —
/// dòng ấy liệt kê `gh`, `claude`, `git`, `sqlite3` nhưng **không có
/// `~/.cargo/bin`**, vì route `/upgrade` (bản Rust của `install.sh`) mới ra đời
/// 2026-08-13, sau khi dòng PATH được viết. Chạy tay ở Terminal thì thấy cargo,
/// chạy từ điện thoại thì không — đúng một cái cầu gãy nhịp cuối.
///
/// Vá hai tầng vì mỗi tầng hỏng theo một kiểu: plist thêm `~/.cargo/bin` (đường
/// đúng cho MỌI lệnh sau này), và hàm này tự tìm (đường đúng cho một bản cài đã
/// nằm sẵn trên máy người khác, nơi không ai sửa plist hộ).
fn cargo_bin() -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let p = std::path::PathBuf::from(home).join(".cargo/bin/cargo");
        if p.exists() {
            return p.display().to_string();
        }
    }
    "cargo".to_string()
}

/// Mốc nhận diện BẢN ĐANG CHẠY: đường + mtime của chính binary này.
pub const BOOT_BINARY_KEY: &str = "boot:binary";

/// Lượt lên này có đáng NÓI không — tức bản có khác lần trước không.
///
/// Tách ra vì đây là một QUYẾT ĐỊNH về việc phát ngôn, cùng họ với
/// `pipeline::watch_book_usable`: chưa từng ghi ⟹ nói (lần đầu sau khi dựng cơ
/// chế này chính là một lần cài); khác bản ⟹ nói; **giống hệt ⟹ im**, vì hubad
/// còn lên lại vì crash và vì `KeepAlive`, và một cái chuông kêu ở đó là chuông
/// kêu lúc không có tin.
pub fn boot_is_news(before: Option<&str>, now: &str) -> bool {
    before.is_none_or(|b| b != now)
}

/// Nói MỘT câu ra Telegram khi huba vừa lên bằng một bản KHÁC bản lần trước.
///
/// 🔴 Hà 2026-08-15: *"Cài lại báo đang restart rồi đứng im, không có cơ chế
/// xác thực cài lại xong chưa"*. Anh đúng, và đây là một lỗ hổng đúng hình dạng
/// đã ghi trong `CLAUDE.md`: `/upgrade` cố ý báo **TRƯỚC** khi restart, vì tiến
/// trình đang trả lời sẽ bị thay thế giữa câu (bài học 13/08 — ba lần bấm nút
/// tự cài lại, lệnh chạy xong mà lời báo bị giết giữa chừng). Nhưng nửa còn lại
/// thì chưa ai làm: **không có ai nói sau khi bản mới đã lên**. Nhìn từ điện
/// thoại, "đang restart" rồi im lặng đọc y hệt một lần cài chết giữa đường.
///
/// Hai điều nó cố ý KHÔNG làm:
/// - **Không nói khi bản không đổi.** hubad còn khởi động lại vì crash, vì
///   `KeepAlive`, vì máy ngủ dậy. Nói mỗi lượt lên là dựng một cái chuông kêu
///   đúng lúc không có tin gì — thứ luật 11 sinh ra để tránh. Đổi bản mới là
///   tin; lên lại cùng bản chỉ là một dòng log.
/// - **Không tự khẳng định "đã cài đúng thứ anh vừa build"**: câu nó nói là
///   mtime + chữ ký của **binary đang chạy**, tức thứ đo được từ bên trong
///   chính tiến trình ấy. So với cây mã là việc của bảng sức khoẻ.
pub fn announce_boot(db: &crate::db::Db, cfg: &Config, signature: &str) {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            crate::logging::warn(
                "boot_binary_unknown",
                json!({ "err": e.to_string(),
                        "why": "không đọc được đường của chính binary — bỏ qua lời báo cài xong" }),
            );
            return;
        }
    };
    let stamp = std::fs::metadata(&exe)
        .and_then(|m| m.modified())
        .map(|t| {
            let ts: chrono::DateTime<chrono::Utc> = t.into();
            ts.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
        .unwrap_or_default();
    let now = format!("{}@{stamp}", exe.display());
    let before = db.cursor_or_log(BOOT_BINARY_KEY);

    if let Err(e) = db.set_cursor(BOOT_BINARY_KEY, &now) {
        // Ghi hỏng thì vẫn NÓI (lời báo là việc chính), nhưng phải nói ra rằng
        // lượt sau có thể nói lại — thà lặp một lần còn hơn im.
        crate::logging::error("boot_binary_not_saved", json!({ "err": e.to_string() }));
    }
    if !boot_is_news(before.as_deref(), &now) {
        crate::logging::info(
            "hubd_boot_same_binary",
            json!({ "binary": now, "signature": signature,
                    "why": "lên lại đúng bản cũ (crash/KeepAlive) — không phải tin, không nói" }),
        );
        return;
    }
    // 🔴 QUYỀN TRỢ NĂNG NÓI NGAY Ở CÂU CHÀO — Hà 2026-08-19: *"Bật trợ năng là
    // sao, sao tin nhắn tôi không thấy chi tiết về thông tin này"*.
    //
    // `hubad` ký chứng chỉ cố định nên quyền ấy SỐNG QUA mọi lần cài lại — nhưng
    // "sống qua" chỉ là lời hứa cho tới khi có ai đo. Đây là chỗ rẻ nhất để đo:
    // mỗi lần cài lại, câu chào tự khai luôn nó còn tay hay không. Không có
    // dòng này thì cách duy nhất biết là đi bấm một cái nút CẦN quyền rồi đọc
    // lỗi — bắt người ta thử cửa để biết cửa khoá.
    let keys = if crate::cgkeys::trusted() {
        "🔑 phím rời: có quyền"
    } else {
        "🔑 phím rời: CHƯA có quyền (Cài đặt Hệ thống ▸ Quyền riêng tư & Bảo mật ▸ Trợ năng ▸ bật hubad)"
    };
    let text = format!(
        "✅ huba đã cài lại xong và đang chạy.\nbản: {stamp} · chữ ký: {signature} · pid {}\n{keys}\n(bản trước: {})",
        std::process::id(),
        before
            .as_deref()
            .and_then(|b| b.split_once('@').map(|(_, t)| t.to_string()))
            .unwrap_or_else(|| "chưa ghi".to_string())
    );
    match crate::confirm::tell(cfg, &text) {
        Ok(()) => crate::logging::info(
            "hubd_boot_announced",
            json!({ "binary": now, "signature": signature,
                    "accessibility": crate::cgkeys::trusted() }),
        ),
        // Không nói được thì phải để lại dấu: đây đúng là lúc chủ máy đang ngồi
        // chờ một câu trả lời.
        Err(e) => crate::logging::error(
            "hubd_boot_announce_failed",
            json!({ "err": e, "binary": now }),
        ),
    }
}

/// Bỏ dòng ĐẦU của `otool -s`: dòng ấy là ĐƯỜNG DẪN của tệp. Giữ nó lại thì hai
/// bản chép của cùng một binary luôn "khác nhau", và cái cổng so nội dung sẽ kêu
/// oan ở MỌI lượt cài — tệ ngang với việc nó không bao giờ kêu.
fn otool_body(stdout: &str) -> &str {
    match stdout.find('\n') {
        Some(i) => &stdout[i + 1..],
        None => "",
    }
}

/// Vân tay NỘI DUNG của một Mach-O, độc lập với chữ ký.
///
/// 🔴 Vì sao có, và nó đã chạy sai một lần THẬT (đo 2026-08-20). Lượt cài lúc
/// 09:37 báo thành công ở cả ba phép nghiệm thu đang có — chữ ký `cert`, `lsof`
/// mở đúng DB, mtime bản cài mới hơn `.rs` mới nhất — trong khi thứ nằm ở đích
/// là build của HÔM TRƯỚC. Thủ phạm là chính hàm [`self_install`] ở bản
/// tiền-đổi-tên: nó chép `target/release/hubd`, cái tên `cargo` KHÔNG còn sinh
/// ra sau khi bin đổi thành `hubad`, mà tệp cũ thì vẫn nằm trên đĩa nên không
/// có gì để `bail`. Ba phép đo cũ đều trả lời đúng câu hỏi CỦA CHÚNG; không câu
/// nào hỏi *"bản cài có phải thứ cây mã này vừa sinh ra không"*.
///
/// Đọc `__TEXT` chứ không băm cả tệp: `codesign` viết lại khối chữ ký trong
/// `__LINKEDIT`, nên so cả tệp thì bản ĐÃ KÝ luôn khác bản vừa build — một phép
/// đo luôn báo lệch cũng vô dụng y như một phép đo luôn báo khớp. Đo hai chiều
/// trên máy này trước khi tin: bản cài (đã ký) và `target/release/hubd` (chưa
/// ký, cùng build) ra CÙNG một vân tay; một build khác thì ra khác.
fn text_id(bin: &Path) -> anyhow::Result<String> {
    let mut out = String::new();
    for sect in ["__text", "__cstring"] {
        let r = run(
            "otool",
            &["-s", "__TEXT", sect, &bin.display().to_string()],
            RunOpts {
                timeout: Some(Duration::from_secs(60)),
                // 🔴 `otool -s` in ra bản kết xuất HEX, to gấp ~2,3 lần chính
                // cái binary: đo 22/08 trên `hubad` 8.238.928 byte ⟹
                // **19.015.353 byte**. Trần chung 8 MB không phải là "giữ 8 MB
                // đầu" mà là "treo lệnh" (xem `exec::drain_capped`), nên chỗ
                // này phải khai thẳng nó xin một biển chữ.
                //
                // 64 MB: gấp ~3,4 lần chỗ đang cần, tức còn chỗ cho binary lớn
                // gấp ba. Không bỏ trần hẳn — một `otool` chạy vào tệp sai vẫn
                // phải dừng ở đâu đó thay vì nuốt hết RAM.
                max_bytes: Some(64 * 1024 * 1024),
                ..Default::default()
            },
        )?;
        // 🔴 BA KIỂU HỎNG, BA CÂU KHÁC NHAU. Bản trước gộp cả ba vào một câu
        // *"otool … hỏng"* — và khi thủ phạm là hết giờ thì `stderr` RỖNG, nên
        // câu ấy đọc lên thành "otool hỏng" về một lệnh chưa bao giờ hỏng. Hà
        // nhận đúng câu ấy nhiều lần, suốt nhiều ngày, và nó chặn mọi bản vá
        // của huba: *"Lâu lắm rồi không chạy được lệnh"*.
        if r.timed_out {
            anyhow::bail!(
                "otool -s __TEXT {sect} KHÔNG XONG trong 60 giây trên {} — \
                 không phải otool hỏng. Nhìn log `exec_output_cut` xem output có bị chặn không.",
                bin.display()
            );
        }
        if !r.ok() {
            anyhow::bail!(
                "otool -s __TEXT {sect} trả mã {} trên {}: {}",
                r.code.map(|c| c.to_string()).unwrap_or_else(|| "?".into()),
                bin.display(),
                crate::exec::truncate(r.stderr.trim(), 120)
            );
        }
        // Bản CỤT không được đem đi so: hai tệp khác nhau mà cùng bị cắt ở
        // 8 MB đầu thì vân tay giống hệt nhau, và cổng "bản cài có đúng mã
        // này không" biến thành một dấu ✅ vô điều kiện.
        if r.cut_bytes > 0 {
            anyhow::bail!(
                "output của otool -s __TEXT {sect} trên {} bị cắt mất {} byte — \
                 vân tay dựng từ bản cụt thì hai tệp khác nhau vẫn khớp nhau. Nới `max_bytes`.",
                bin.display(),
                r.cut_bytes
            );
        }
        out.push_str(otool_body(&r.stdout));
    }
    // Không đo được thì phải NÓI. Trả chuỗi rỗng là để hai tệp cùng "rỗng" khớp
    // nhau, tức biến cổng này thành một dấu ✅ vô điều kiện — đúng hình dạng
    // phép đo mù mà nó sinh ra để chặn.
    if out.trim().is_empty() {
        anyhow::bail!(
            "không đọc được __TEXT của {} — không dám kết luận",
            bin.display()
        );
    }
    Ok(out)
}

pub fn self_install(cfg: &Config) -> anyhow::Result<String> {
    let rust_dir = source_tree(cfg);
    if !rust_dir.is_dir() {
        anyhow::bail!("không thấy cây mã ở {}", rust_dir.display());
    }
    // 1. Build.
    let out = run(
        &cargo_bin(),
        &["build", "--release", "--offline"],
        RunOpts {
            cwd: Some(&rust_dir),
            timeout: Some(Duration::from_secs(900)),
            ..Default::default()
        },
    )?;
    if !out.ok() {
        anyhow::bail!(
            "cargo build hỏng (mã {:?}): {}",
            out.code,
            crate::exec::truncate(out.stderr.trim(), 300)
        );
    }
    let src = rust_dir.join("target/release/hubad");
    if !src.exists() {
        anyhow::bail!("build xong mà không thấy {}", src.display());
    }
    // 🔴 NGUỒN PHẢI TƯƠI HƠN CÂY MÃ, và câu hỏi này phải đứng TRƯỚC cú chép:
    // cửa "so nội dung" ở bước 5 chỉ chứng minh ta chép đúng một tệp, nó không
    // chứng minh tệp ấy là sản phẩm của cây mã này. Ca 20/08 nằm gọn ở đây —
    // bin đổi tên `hubd`→`hubad`, bản cũ ở lại trên đĩa, và chính chỗ này chép
    // nó đi cài trong khi `cargo` vừa dựng ra một cái tên khác.
    match (mtime(&src), newest_source_mtime(&rust_dir)) {
        (Some(built_at), Some(changed_at)) if changed_at > built_at => {
            anyhow::bail!(
                "{} CŨ HƠN cây mã — nó không phải thứ lượt build vừa sinh ra, \
                 nên không cài. Xem cargo vừa dựng ra cái tên gì: ls -lat {}/target/release/",
                src.display(),
                rust_dir.display()
            );
        }
        (a, b) => {
            // Mù thì nói ra rồi đi tiếp — cửa so nội dung vẫn đứng. Im lặng ở
            // đây là để một lượt KHÔNG được gác đọc lên y hệt một lượt đã gác.
            if a.is_none() || b.is_none() {
                crate::logging::warn(
                    "self_install_freshness_unknown",
                    json!({ "bin": src.display().to_string(),
                            "src_tree": rust_dir.display().to_string(),
                            "why": "không đọc được mtime của bản build hoặc của cây mã — \
                                    cửa TƯƠI không gác được lượt này" }),
                );
            }
        }
    }
    // 2. Danh tính ký — TÌM, tuyệt đối không tự tạo cái mới: một chứng chỉ mới
    //    là một `designated requirement` mới, tức mọi quyền TCC mất sạch.
    let ids = run(
        "security",
        &["find-identity", "-p", "codesigning"],
        RunOpts {
            timeout: Some(Duration::from_secs(20)),
            ..Default::default()
        },
    )?;
    let sha = ids
        .stdout
        .lines()
        .find(|l| l.contains(&format!("\"{SIGNING_CN}\"")))
        .and_then(|l| l.split_whitespace().nth(1))
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("không thấy danh tính ký '{SIGNING_CN}' trong keychain"))?;
    // 3. Chép ra bản TẠM rồi ký ở đó — bản đang chạy không được thấy một tệp
    //    ghi dở, và macOS từ chối ghi đè một image đang chạy.
    let dest = crate::config::expand_home(Path::new(INSTALLED_HUBD));
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = dest.with_extension("new");
    std::fs::copy(&src, &tmp)?;
    let fail = |e: anyhow::Error| -> anyhow::Error {
        let _ = std::fs::remove_file(&tmp);
        e
    };
    let signed = run(
        "codesign",
        &[
            "--force",
            "--sign",
            &sha,
            "--identifier",
            "com.dipgle.hubd",
            "--timestamp=none",
            &tmp.display().to_string(),
        ],
        RunOpts {
            timeout: Some(Duration::from_secs(60)),
            ..Default::default()
        },
    )?;
    if !signed.ok() {
        return Err(fail(anyhow::anyhow!(
            "codesign hỏng: {}",
            crate::exec::truncate(signed.stderr.trim(), 200)
        )));
    }
    // 4. Chứng minh chữ ký là loại BỀN, không phải `cdhash`. Đây là điều duy
    //    nhất cả việc này tồn tại để bảo đảm, nên nó là lỗi chứ không phải
    //    cảnh báo.
    let dr = run(
        "codesign",
        &["-d", "-r-", &tmp.display().to_string()],
        RunOpts {
            timeout: Some(Duration::from_secs(20)),
            ..Default::default()
        },
    )?;
    let dr_text = format!("{}{}", dr.stdout, dr.stderr);
    if !dr_text.contains("certificate root") {
        return Err(fail(anyhow::anyhow!(
            "ký xong mà designated requirement không phải certificate root — bản đang cài GIỮ NGUYÊN"
        )));
    }
    // 5. Và thứ sắp đặt xuống có ĐÚNG là thứ vừa build không. Hỏi TRƯỚC khi
    //    `rename`, cùng luật với bốn bước trên: hỏng ở bất kỳ đâu thì bản đang
    //    cài GIỮ NGUYÊN. Cửa này bắt phần cửa "tươi" không thấy — một cú chép
    //    cụt, hoặc `cargo` link lại `src` xen vào giữa chép và ký (bài học
    //    10/08: `cargo test --release` ký đè ad-hoc lên chính tệp ấy).
    let id_src = match text_id(&src) {
        Ok(v) => v,
        Err(e) => return Err(fail(e)),
    };
    let id_tmp = match text_id(&tmp) {
        Ok(v) => v,
        Err(e) => return Err(fail(e)),
    };
    if id_src != id_tmp {
        return Err(fail(anyhow::anyhow!(
            "bản vừa ký KHÔNG khớp nội dung bản vừa build ({} vs {} byte __TEXT) — \
             bản đang cài GIỮ NGUYÊN",
            id_src.len(),
            id_tmp.len()
        )));
    }
    std::fs::rename(&tmp, &dest)?;
    Ok(format!("đã cài {}", dest.display()))
}

/// Bảo launchd nạp lại hubad. Gọi SAU khi đã trả lời, vì nó giết chính mình.
pub fn restart_daemon() -> anyhow::Result<String> {
    let uid = run(
        "id",
        &["-u"],
        RunOpts {
            timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        },
    )?;
    let target = format!("gui/{}/com.dipgle.hubd", uid.stdout.trim());
    let out = run(
        "launchctl",
        &["kickstart", "-k", &target],
        RunOpts {
            timeout: Some(Duration::from_secs(30)),
            ..Default::default()
        },
    )?;
    if !out.ok() {
        // 🔴 "Chưa nạp" KHÔNG phải "cài hỏng" — đo 2026-08-13, ngay lượt chuyển
        // huba từ `AI/huba` sang `~/projects/huba`. Kịch bản chuyển `bootout` agent
        // TRƯỚC (bắt buộc: `KeepAlive` làm `kill` vô nghĩa), rồi gọi
        // `self-install`. Build xong, ký xong, **binary đã nằm đúng chỗ** — rồi
        // hàm này `bail!` vì `kickstart` không tìm thấy service, và `set -e`
        // giết luôn kịch bản ở bước áp chót. Kết cục: đã cài mà mã trả về nói
        // là hỏng, còn agent thì không ai nạp.
        //
        // Phân biệt bằng chính câu launchctl trả: *"Could not find service … in
        // domain"* nghĩa là chưa nạp, và với một lượt cài thì đó là chuyện bình
        // thường (máy mới, hoặc vừa bootout). Mọi lỗi khác vẫn là lỗi.
        let why = out.stderr.trim();
        if why.contains("Could not find service") || why.contains("No such process") {
            crate::logging::info(
                "self_install_not_loaded",
                json!({ "target": target, "why": why,
                        "next": "launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.dipgle.hubd.plist" }),
            );
            return Ok(format!("{target} (chưa nạp — cần bootstrap một lần)"));
        }
        anyhow::bail!(
            "launchctl kickstart hỏng: {}",
            crate::exec::truncate(why, 200)
        );
    }
    Ok(target)
}

/// Đường dẫn bản hubad mà launchd chạy. KHÔNG phải bản `cargo` vừa build.
const INSTALLED_HUBD: &str = "~/Library/Application Support/hub/bin/hubd";

/// Tên chứng chỉ ký, đúng như nó nằm trong keychain — **`Hub`, không phải
/// `Huba`**.
///
/// 🔴 Lượt đổi tên `hub` → `huba` (2026-08-20) sửa cả những chuỗi đặt tên một
/// vật thể ĐÃ TỒN TẠI, và chứng chỉ là một trong số đó. Nó không đổi tên theo
/// được: đổi là sinh một `designated requirement` mới, tức mọi quyền TCC của
/// `hubad` rụng sạch — đúng thứ chứng chỉ cố định này sinh ra để tránh.
///
/// Ngày 21/08 chỗ này được vá **một nửa**: `sign.sh` và `make-signing-cert.sh`
/// trả về `Hub Local Signing`, còn bản Rust ở đây thì không — mà `/upgrade` đi
/// đúng đường Rust. Nên `install_update.sh` chạy được, `/upgrade` thì trả
/// *"không thấy danh tính ký 'Huba Local Signing' trong keychain"*, trong khi
/// `security find-identity -p codesigning` in ra rõ ràng
/// `9DE8EC03… "Hub Local Signing" (CSSMERR_TP_NOT_TRUSTED)`. Hà bấm ba lần
/// (09:31 · 09:33 · 09:33 ngày 22/08) và ba lần nhận cùng một câu.
///
/// Bài học không phải "gõ đúng tên" mà là **hai bản chép thì hai bản sẽ lệch**,
/// và lệch âm thầm vì hai đường đi khác nhau. Hằng số này bị khoá vào chính hai
/// tệp shell kia bằng `the_signing_name_matches_the_shell_scripts`.
///
/// `CSSMERR_TP_NOT_TRUSTED` là ĐÚNG, không phải hỏng: chứng chỉ tự ký và cố ý
/// không được tin, nên `find-identity -v` (chỉ liệt kê "valid") trả 0 — vì thế
/// câu lệnh ở trên phải là `-p codesigning` KHÔNG kèm `-v`.
const SIGNING_CN: &str = "Hub Local Signing";

/// Hai câu hỏi mà thiết kế "cài bản đã ký ra đường riêng" vừa đẻ ra, và cả hai
/// đều im lặng nếu không ai hỏi:
///
/// 1. **Bản cài có còn mang chữ ký cố định không** (`cert`)? Nếu là `adhoc` thì
///    quyền TCC sẽ mất ở lần build kế tiếp — huba vẫn chạy ngon cho tới lúc khởi
///    động lại máy, rồi im.
/// 2. **Bản cài có cũ hơn bản vừa build không**? Đây là cái giá của việc tách
///    hai file: sửa mã, `cargo build`, test xanh, deploy trang — và daemon vẫn
///    đang chạy mã của hôm qua vì không ai chạy `install_update.sh`. Không có
///    dòng này thì không gì phát hiện ra.
///
/// Trả `None` cho câu nào không hỏi được, không đoán bừa (`unknown` ≠ `sai`).
fn installed_binary_state(cfg: &Config) -> (Value, Value) {
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

    (signature, stale_against_build(cfg))
}

/// Bản cài có còn là mã hiện tại không?
///
/// Hỏi **mã nguồn**, không hỏi sản phẩm build. Hai đường kia đều đã thử và đều
/// sai (đo 2026-08-10):
/// - *mtime của `target/release/hubad`*: `cargo test --release` link lại mỗi lượt
///   test, mtime nhảy dù mã không đổi.
/// - *cdhash của `target/release/hubad`*: tưởng ổn định vì build của Rust lặp
///   lại đúng byte, nhưng `cargo test --release` cho ra một binary KHÁC hẳn
///   (`2f624e8b…` so với `bbd8ba58…` của `cargo build --release`), và lệnh build
///   sau đó lại trả về hash cũ. Cùng một mã, hai câu trả lời.
///
/// Một cảnh báo kêu oan sau mỗi lượt test là một cảnh báo bị phớt lờ, tức tệ
/// hơn không có. Còn `.rs`/`Cargo.toml`/`Cargo.lock` thì chỉ đổi mtime khi có
/// người thật sự sửa — đúng câu hỏi cần trả lời: *sửa mã xong đã cài lại chưa?*
///
/// 📌 Cây mã hỏi ở đâu: **`<hub_home>/rust`**, tức nơi huba ĐANG chạy, không phải
/// một đường dẫn gõ cứng. Đường cứng `~/Documents/projects/AI/huba/rust` đứng ở
/// đây tới 2026-08-12 — ngày gốc workspace dời sang `~/projects`. Nó không kêu
/// một tiếng nào, vì mất cây mã thì hàm này trả `None` ⟹ `null` ⟹ tấm bảng sức
/// khoẻ **thôi cảnh báo daemon cũ**, đúng cái nó sinh ra để nói. Một phép đo tắt
/// tiếng đọc lên y hệt một phép đo nói "không sao".
fn stale_against_build(cfg: &Config) -> Value {
    let bin = crate::config::expand_home(Path::new(INSTALLED_HUBD));
    let src = source_tree(cfg);
    let Some(installed_at) = mtime(&bin) else {
        return Value::Null;
    };
    // Không có mã nguồn ở máy này (bản cài đem từ nơi khác) thì không có gì để
    // so — nhưng nói ra là mình MÙ, đừng im: đây chính là hình dạng của lỗi ở
    // trên, và cái phân biệt "không có gì để so" với "tôi đang nhìn nhầm chỗ"
    // là đường dẫn đã nhìn.
    match newest_source_mtime(&src) {
        Some(changed_at) => Value::from(changed_at > installed_at),
        None => {
            crate::logging::warn(
                "hubd_stale_check_no_source",
                json!({ "src": src.display().to_string() }),
            );
            Value::Null
        }
    }
}

/// Cây mã của bản huba ĐANG chạy — bám `hub_home`, không bám `$HOME`.
///
/// Một dòng riêng vì đây đúng là chỗ đã sai: `hub_home` do `HUB_CONFIG` trong
/// plist quyết, nên nó theo huba đi bất cứ đâu, còn một đường dẫn gõ cứng thì
/// chỉ đúng cho tới lần dời thư mục kế tiếp.
fn source_tree(cfg: &Config) -> PathBuf {
    cfg.hub_home.join("rust")
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

    /// Dòng đầu của `otool -s` là ĐƯỜNG DẪN tệp, và hai thứ đem so nhau ở
    /// [`self_install`] nằm ở hai đường khác nhau (bản build vs bản sắp cài).
    /// Giữ dòng ấy lại thì mọi lượt cài đều đọc ra "lệch" — một cổng kêu oan
    /// mỗi lần là một cổng bị tắt, y như một cổng không bao giờ kêu.
    #[test]
    fn otool_body_bo_dong_duong_dan_nen_hai_ban_chep_van_khop() {
        let build = "/Users/ai/rust/target/release/hubad:\n\
                     Contents of (__TEXT,__text) section\n0001 aa bb\n";
        let cai = "/Users/ai/Library/Application Support/hub/bin/hubd:\n\
                   Contents of (__TEXT,__text) section\n0001 aa bb\n";
        assert_eq!(
            otool_body(build),
            otool_body(cai),
            "cùng nội dung, khác đường dẫn ⟹ phải KHỚP"
        );
        assert!(otool_body(build).starts_with("Contents of"));

        // …và cửa không được nuốt luôn cả sự khác biệt thật.
        let khac = "/x:\nContents of (__TEXT,__text) section\n0001 aa cc\n";
        assert_ne!(otool_body(build), otool_body(khac));

        // Không có thân thì trả RỖNG — `text_id` biến chuỗi rỗng thành một câu
        // từ chối, chứ không phải thành một dấu ✅ vô điều kiện.
        assert_eq!(otool_body("chỉ một dòng, không có \\n"), "");
        assert_eq!(otool_body(""), "");
    }

    /// Ba nơi gọi tên MỘT chứng chỉ phải gọi cùng một tên.
    ///
    /// 🔴 Bài kiểm này sinh ra từ ca 22/08: `sign.sh` và `make-signing-cert.sh`
    /// nói `Hub Local Signing`, `runtime.rs` nói `Huba Local Signing`, và không
    /// có gì đỏ — vì hai bên chạy ở hai đường khác nhau (`install_update.sh` vs
    /// `/upgrade`). Người dùng là chỗ duy nhất phát hiện ra, sau ba lần bấm.
    ///
    /// Nó đọc HAI TỆP KHÁC, không tự soi mình: một bài kiểm quét chính nó tìm
    /// một cái tên thì luôn tự khớp, và đó là phép đo mù
    /// (`OPERATING-CHARTER.md` §2d).
    #[test]
    fn the_signing_name_matches_the_shell_scripts() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("rust/ phải có thư mục cha là gốc repo")
            .to_path_buf();
        // Đọc `CERT_CN="…"` từ một tệp shell. Trả `None` khi không thấy, để
        // nhánh assert dưới phân biệt được "lệch tên" với "tệp đã đổi hình
        // dạng" — hai chuyện phải sửa theo hai cách khác nhau.
        let cn_of = |ten: &str| -> Option<String> {
            let doc = std::fs::read_to_string(repo.join(ten)).ok()?;
            doc.lines()
                .find_map(|l| l.trim().strip_prefix("CERT_CN="))
                .map(|v| v.trim().trim_matches('"').to_string())
        };
        for ten in ["sign.sh", "make-signing-cert.sh"] {
            let got = cn_of(ten).unwrap_or_else(|| {
                panic!(
                    "không đọc được `CERT_CN=` trong {ten} — bài kiểm này mất chỗ đối chiếu, \
                     nên nó KHÔNG được xanh: sửa phép đọc, đừng bỏ bài kiểm"
                )
            });
            assert!(
                !got.is_empty(),
                "{ten} khai `CERT_CN` rỗng — không có gì để đối chiếu"
            );
            assert_eq!(
                got, SIGNING_CN,
                "{ten} gọi chứng chỉ là {got:?} còn runtime.rs gọi {SIGNING_CN:?}. \
                 Hai bản chép thì hai bản sẽ lệch, và lệch âm thầm vì `install_update.sh` \
                 với `/upgrade` đi hai đường khác nhau."
            );
        }
    }

    /// Cái bẫy mà phép đo này sinh ra để tránh: đếm nhầm một file KHÔNG PHẢI mã
    /// nguồn (sản phẩm build, ghi chú) thành "mã vừa đổi", rồi báo daemon đã cũ
    /// sau mỗi lượt `cargo test`. Cảnh báo kêu oan là cảnh báo bị tắt.
    #[test]
    fn newest_source_mtime_counts_rust_sources_and_ignores_build_output() {
        let root = std::env::temp_dir().join(format!("huba-rt-{}", std::process::id()));
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
        std::fs::write(root.join("target/release/hubad.rs"), "// build output").unwrap();
        std::fs::write(root.join("src/notes.txt"), "// not code").unwrap();
        assert_eq!(
            newest_source_mtime(&root),
            Some(expected),
            "mốc bị kéo theo một file không phải mã nguồn"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Cây mã phải đi THEO huba, không neo vào một đường dẫn gõ cứng.
    ///
    /// Đây là con bug đã xảy ra thật: `~/Documents/projects/AI/huba/rust` nằm
    /// trong mã tới 2026-08-12, ngày gốc workspace dời sang `~/projects`. Nó
    /// không làm gãy gì to tát — nó chỉ làm tấm bảng sức khoẻ **thôi trả lời**
    /// câu "sửa mã xong đã cài lại chưa", tức mất đúng thứ duy nhất phát hiện
    /// ra daemon đang chạy mã của hôm qua.
    #[test]
    fn the_source_tree_follows_hub_home_not_a_hardcoded_path() {
        let cfg = Config {
            hub_home: PathBuf::from("/tmp/somewhere-else/AI/huba"),
            ..Default::default()
        };
        assert_eq!(
            source_tree(&cfg),
            PathBuf::from("/tmp/somewhere-else/AI/huba/rust")
        );
        assert!(
            !source_tree(&cfg).starts_with(crate::config::expand_home(Path::new("~/Documents"))),
            "cây mã lại bị neo vào đường cũ"
        );
    }

    /// Không có cây nguồn ở chỗ đang nhìn thì trả `None` để màn nói "không rõ",
    /// KHÔNG phải `false` — đoán bừa là cách một phép đo biến thành một lời nói
    /// dối yên tâm. (Kèm một dòng log, vì "không rõ" mà im lặng thì đọc y hệt
    /// "không sao".)
    #[test]
    fn a_missing_source_tree_answers_unknown_never_up_to_date() {
        let cfg = Config {
            hub_home: std::env::temp_dir().join(format!("huba-nosrc-{}", std::process::id())),
            ..Default::default()
        };
        assert_eq!(stale_against_build(&cfg), Value::Null);
    }

    /// Câu trả lời thật của `claude -p "/usage"`, chép nguyên văn 2026-08-10.
    const USAGE_SAMPLE: &str = "You are currently using your subscription to power your Claude Code usage\n\nCurrent session: 6% used · resets Aug 10 at 1:29pm (Asia/Saigon)\nCurrent week (all models): 98% used · resets Aug 11 at 12:59pm (Asia/Saigon)\nCurrent week (Fable): 50% used · resets Aug 11 at 1pm (Asia/Saigon)\n\nWhat's contributing to your limits usage?";

    #[test]
    fn usage_is_read_out_of_the_real_answer() {
        let v = parse_usage(USAGE_SAMPLE);
        assert_eq!(v["session_pct"], 6);
        assert_eq!(v["week_pct"], 98);
        assert_eq!(v["week_model_pct"], 50);
        assert_eq!(v["week_model_name"], "Fable");
        assert!(v["week_resets"].as_str().unwrap().contains("Aug 11"));
        assert!(v.get("raw").is_none());
    }

    /// Câu chữ của CLI đổi thì phải trả về NGUYÊN VĂN, tuyệt đối không trả 0%.
    /// Một con số bịa trông y hệt một con số thật, và đây là con số dùng để
    /// quyết định mở phiên bằng tài khoản nào.
    #[test]
    fn unknown_wording_keeps_the_raw_text_instead_of_inventing_zero() {
        let v = parse_usage("Usage limits are not available for this account.\nSecond line.");
        assert!(v.get("session_pct").is_none(), "bịa ra một con số: {v}");
        assert!(v.get("week_pct").is_none());
        assert!(v["raw"].as_str().unwrap().contains("not available"));
    }

    /// Một dòng đọc được, một dòng không, thì giữ dòng đọc được — đừng vứt cả
    /// hai chỉ vì CLI thêm một mục mới.
    #[test]
    fn a_half_understood_answer_keeps_what_it_understood() {
        let v = parse_usage("Current session: 12% used · resets tomorrow\nSomething new: yes");
        assert_eq!(v["session_pct"], 12);
        assert!(v.get("week_pct").is_none());
        assert!(v.get("raw").is_none());
    }

    /// Không có cây nguồn ở đường được đưa (bản cài đem từ nơi khác) thì trả
    /// `None` — tầng trên dịch thành "không rõ", không phải "đã mới".
    #[test]
    fn newest_source_mtime_is_unknown_when_there_is_no_tree() {
        let missing = std::env::temp_dir().join(format!("huba-rt-missing-{}", std::process::id()));
        assert_eq!(newest_source_mtime(&missing), None);
    }
}
