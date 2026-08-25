//! Config loading + validation.
//!
//! Secrets NEVER live in the config file — only the NAME of the env var that
//! holds them (`confirm.bot_token_env`, `confirm.chat_id_env`). Charter DoD #8.
//!
//! `#[serde(default)]` on every struct reproduces the prototype's deep-merge:
//! a key absent from huba.config.json falls back to the default, and sibling
//! keys inside a partially-specified table keep theirs.

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

// `TIERS` and `ALWAYS_HUMAN_ACTIONS` lived here until 2026-08-08. They were the
// vocabulary of `policy.rs` — how much the robot could do unattended, and the
// five things it could never do at any tier. With no robot deciding anything,
// there is no tier to set: huba does exactly what the owner typed, and the wall
// that matters now is `sessions::DENIED_TOOLS`, which is enforced on the CLI
// call itself rather than described in a config file.

/// Tự đóng sổ khi ngữ cảnh đầy.
///
/// Hà chốt bật 2026-08-10, kèm điều kiện: *"phải đảm bảo đã chạy hết chỗ dở"*.
/// Đó là ràng buộc quan trọng hơn cái ngưỡng — cắt ngang một phiên đang làm là
/// mất đúng thứ nó đang làm.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AutoHandoverCfg {
    pub enabled: bool,
    /// Ngưỡng ngữ cảnh (%) để bắt đầu nghĩ tới việc đóng sổ.
    pub at_percent: u8,
    /// Phải đứng yên bao lâu mới coi là "đã chạy hết chỗ dở".
    ///
    /// Chỉ nhìn màn hình là chưa đủ: giữa hai lệnh, màn cũng không có đồng hồ
    /// trong tích tắc. Đòi thêm nhật ký im lặng đủ lâu thì mới chắc.
    pub idle_sec: u64,
}

impl Default for AutoHandoverCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            at_percent: 80,
            idle_sec: 120,
        }
    }
}

/// Tự chạy hộ lệnh mà một phiên ĐANG ĐỨNG CHỜ nhờ chủ máy chạy.
///
/// 🔴 Hà 2026-08-23: *"luồng kiểm tra phiên dừng lại chờ sẽ quét nội dung trả về
/// có lệnh cần tôi chạy thì sẽ chạy luôn lệnh đó, kết quả chạy được sẽ gửi vào
/// hàng chờ của phiên đó luôn"*.
///
/// Đây là cái nút `▶️` tự bấm. Và chính vì thế nó phải có một cổng KHÁC với
/// cổng của cái nút: `keys::commands_in_report` trả lời *"đây có phải một lệnh
/// không"* — đủ để BÀY một cái nút, vì còn một ngón tay người quyết định. Bỏ
/// ngón tay ấy đi thì câu phải trả lời là *"lệnh này chạy được khi KHÔNG AI
/// NHÌN không"*, và không bộ nhận-dạng-hình-dạng nào trả lời được câu đó.
///
/// Bằng chứng có sẵn trong chính tệp này: 13/08 một bộ gác từ chối
/// `git filter-branch` rồi in ra câu giải thích CHỨA lệnh ấy; huba thấy hình
/// dạng một lệnh và bày nút `▶ git filter-branch --force`. Cái nút ấy còn cần
/// người bấm. Bản tự chạy thì không.
///
/// 🔴 ĐỔI NGUỒN 2026-08-24, sau khi Hà hỏi *"Tại sao lại cần allow làm gì
/// vậy?"* rồi chốt *"Chỉ dấu, bỏ allow"*.
///
/// Bản đầu có một danh sách cho phép, và nó cần thiết vì `auto_run` **đoán**
/// theo hình dạng — nên phải tự chặn lại thứ chính nó đoán bừa. Nay nguồn khác
/// hẳn: chỉ chạy dòng phiên **CỐ Ý đánh dấu** bằng `keys::RUN_MARK`
/// (`#huba-run` chiếm trọn một dòng, ngay trên dòng lệnh). Hết đoán thì hết thứ
/// để chặn.
///
/// ⚠ Cái dấu nói *"mô hình cố ý bảo chạy"*, KHÔNG nói *"chủ máy cho phép"*. Hà
/// biết rủi ro ấy và chọn thế; đừng lặng lẽ dựng lại một danh sách ở đây.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AutoRunCfg {
    pub enabled: bool,
}

impl Default for AutoRunCfg {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Xác nhận lần hai qua Telegram cho những lệnh KHÔNG lùi lại được.
///
/// Hà 2026-08-10: *"riêng một số lệnh dừng hoặc tắt phiên cần có xác thực qua
/// tele"*. Đây không phải kênh hộp thư quay lại — nó không đọc gì, không tạo
/// việc, không tiêu hạn mức. Nó chỉ hỏi đúng một câu và chờ một cái bấm.
///
/// Vì sao là kênh KHÁC: nút "Dừng" nằm ngay trên danh sách phiên, một ngón tay
/// chạm nhầm là mất tiến trình đang chạy. Hỏi lại trên cùng cái điện thoại ấy
/// chỉ chặn được tay nhầm; hỏi qua Telegram còn chặn được cả trường hợp mất
/// máy — người cầm điện thoại vẫn phải mở được hòm Telegram của chủ.
///
/// Theo luật §4: ở đây chỉ có TÊN biến môi trường, không bao giờ có giá trị.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfirmCfg {
    /// Tắt thì lệnh huỷ chạy thẳng như trước. Bật mà thiếu khoá thì huba TỪ
    /// CHỐI lệnh chứ không lặng lẽ bỏ qua chốt chặn — xem `confirm.rs`.
    pub enabled: bool,
    pub bot_token_env: String,
    pub chat_id_env: String,
    /// Chờ bấm nút bao lâu rồi coi như không đồng ý.
    pub timeout_sec: u64,
    /// Tin huba gửi sang Telegram sống bao lâu rồi tự xoá. `0` = không xoá.
    ///
    /// Hà 2026-08-12: *"đã có cơ chế tự xóa tin nhắn cũ hơn 1.5 ngày chưa"* —
    /// chưa, không chỗ nào. Mặc định **36 giờ** đúng bằng con số ấy.
    ///
    /// 🔴 Trần CỨNG của Telegram là **48 giờ**: quá đó bot không xoá được tin
    /// của chính nó nữa, vĩnh viễn. Nên con số này phải nằm dưới 48 một khoảng
    /// đủ cho một lần huba nằm im (mất mạng, máy ngủ) mà vẫn kịp quay lại xoá.
    /// Đặt 47 là tự dựng một cái bẫy: huba tỉnh dậy sau một giấc là cả loạt tin
    /// rơi ra ngoài cửa.
    #[serde(default = "default_delete_after_hours")]
    pub delete_after_hours: u64,
}

fn default_delete_after_hours() -> u64 {
    36
}

impl Default for ConfirmCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            bot_token_env: "HUB_TELEGRAM_BOT_TOKEN".to_string(),
            chat_id_env: "HUB_TELEGRAM_CHAT_ID".to_string(),
            timeout_sec: 90,
            delete_after_hours: default_delete_after_hours(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CallCfg {
    /// Ceiling for ONE `claude` call huba makes on the owner's behalf, and a
    /// floor under the measured per-call estimate (`sessions::fork_call`).
    ///
    /// Was `triage.max_budget_usd`, the guard on a robot classifying an inbox.
    /// The inbox is gone; what is left are calls a person pressed a button for,
    /// so this is not a spending policy — it is a stop on one call running away.
    #[serde(default = "default_call_budget")]
    pub max_budget_usd: f64,
    /// Wall-clock stop for the same call.
    #[serde(default = "default_call_timeout")]
    pub timeout_sec: u64,
}

impl Default for CallCfg {
    fn default() -> Self {
        Self {
            max_budget_usd: default_call_budget(),
            timeout_sec: default_call_timeout(),
        }
    }
}

pub fn default_call_budget() -> f64 {
    0.5
}

pub fn default_call_timeout() -> u64 {
    240
}

// 🔴 ĐÃ BỎ `Tfl5Cfg`, `Adapters` và `Trust`, 2026-08-14, cùng lượt gỡ tfl5 theo
// lời Hà: *"tạm thời không dùng tfl5 để xem cứ xóa hết đi"*.
//
// `adapters.tfl5` khai một KÊNH không còn tồn tại: máy chủ, phòng, hai tên biến
// môi trường, trần thời gian chờ tiếng vọng. Kênh đi rồi thì mọi ô ấy chỉ còn là
// chỗ để gõ vào cho vui.
//
// `trust` thì đáng nói kỹ hơn, vì nó nghe như một cái cổng bảo mật:
//
// * `trusted_sources` **chưa bao giờ được đọc** — khai trong cấu hình, đặt trong
//   test, không một chỗ nào hỏi tới nó. Một hàng rào không ai đi qua.
// * `tfl5_user_tids` là cổng THẬT của phòng chat, nhưng sau khi phòng đóng thì
//   `pipeline` phải tự bịa ra người gõ để đi qua chính nó: lấy `first()` của
//   danh sách rồi đem so với danh sách ấy. Một cổng cấu tạo sao cho không bao
//   giờ từ chối được — trừ đúng một trường hợp: danh sách RỖNG, và khi ấy nó từ
//   chối **mọi** mệnh lệnh, im lặng, chỉ để lại một dòng `command_from_non_owner`
//   trong nhật ký. Tức là gỡ tfl5 khỏi `huba.config.json` mà giữ cổng này lại thì
//   Telegram câm hẳn, không ai hiểu vì sao.
//
// Cổng người thật nay ở đúng chỗ nó thuộc về — KÊNH: `telegram.rs` chỉ nhận tin
// từ `chat_id` của chủ máy (`telegram.rs:1326` cho chữ, `:1731` cho nút), khoá
// lấy từ `huba.env`. Xem `verbs::parse_command` để biết vì sao bộ phân tích lệnh
// không còn nhận tham số "ai đang gõ".

/// One project, registered under its FOLDER NAME — which is also the name
/// `/project` and `/new` accept.
///
/// It used to carry `repos` (the GitHub repositories that routed mail to this
/// project) and `tier` (how much the robot could answer on its own). Both were
/// answers to questions the inbox asked. A project is now just a folder huba can
/// open a session in, so the only thing left worth storing is a note for a
/// person — and even that, huba never reads.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ProjectCfg {
    /// Free note for humans; huba never reads it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct NotifyCfg {
    pub file: PathBuf,
    pub macos_notification: bool,
}

impl Default for NotifyCfg {
    fn default() -> Self {
        Self {
            file: PathBuf::from("logs/notify.log"),
            macos_notification: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub db: PathBuf,
    pub log_file: PathBuf,
    /// Empty = derive from the huba directory (`<workspace>/AI/huba` → workspace).
    #[serde(skip_serializing_if = "is_empty_path")]
    pub workspace_root: PathBuf,
    /// Folders under `workspace_root` that hold projects, searched in order.
    /// `""` means the workspace root itself, and it comes first: the root is
    /// the base, `AI/` is just one folder in it.
    #[serde(default = "default_project_roots")]
    pub project_roots: Vec<String>,
    pub poll_interval_sec: u64,
    /// Ceiling + timeout for ONE `claude` call the owner asked for.
    ///
    /// Everything else that used to live here — `max_triage_per_cycle`,
    /// `coalesce_hours`, `daily_budget_usd`, the per-source ceilings, `act`,
    /// `autonomy`, `routing`, `leak_patterns`, `web` — belonged to the inbox and
    /// went with it on 2026-08-08. An unknown key in an existing file is ignored
    /// by serde, so an old `huba.config.json` still loads; the next save drops it.
    pub call: CallCfg,
    /// Tự đóng sổ khi ngữ cảnh đầy — xem [`AutoHandoverCfg`].
    pub auto_handover: AutoHandoverCfg,
    pub auto_run: AutoRunCfg,
    /// Xác nhận lần hai cho lệnh không lùi lại được — xem [`ConfirmCfg`].
    #[serde(default)]
    pub confirm: ConfirmCfg,
    /// THE project registry, keyed by folder name under `project_roots`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, ProjectCfg>,
    pub notify: NotifyCfg,
    /// The `claude` executable used to LIST sessions (`sessions.rs`).
    #[serde(default = "default_claude_cli")]
    pub claude_cli: String,
    /// Accounts to enumerate. Empty = just the ambient account, which is the
    /// only thing true on a fresh machine; this Mac has three, declared in
    /// `huba.config.json`. Mechanism, not policy — the code must not know a
    /// particular person's setup.
    #[serde(default)]
    pub claude_accounts: Vec<ClaudeAccountCfg>,
    /// Where `projects/<slug>/<session>.jsonl` lives. Empty = `~/.claude`.
    /// One root covers every account while their `projects` dirs are symlinked
    /// together (true here since 2026-08-06).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub claude_transcript_root: String,
    /// `/new` mở một CỬA SỔ Terminal thật (mặc định), thay vì phiên `--bg`.
    ///
    /// Mặc định BẬT vì phiên có cửa sổ mới là thứ chủ máy tự tạo khi ngồi trước
    /// máy — và chỉ trên đó thì màn sống, `/btw`, dòng "đang làm gì" mới chạy.
    /// Tắt cờ này ⟹ quay về `--bg`: không cửa sổ, nhưng sống sót khi đóng
    /// Terminal, và không cần quyền Automation.
    #[serde(default = "default_true")]
    pub new_in_terminal: bool,

    #[serde(skip)]
    pub config_file: PathBuf,
    #[serde(skip)]
    pub hub_home: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db: PathBuf::from("data/huba.sqlite"),
            log_file: PathBuf::from("logs/huba.log"),
            workspace_root: PathBuf::new(),
            project_roots: default_project_roots(), // filled in by load(): <hub_home>/../..
            poll_interval_sec: 120,
            call: CallCfg::default(),
            auto_handover: AutoHandoverCfg::default(),
            auto_run: AutoRunCfg::default(),
            confirm: ConfirmCfg::default(),
            projects: BTreeMap::new(),
            notify: NotifyCfg::default(),
            claude_cli: default_claude_cli(),
            claude_accounts: vec![],
            claude_transcript_root: String::new(),
            new_in_terminal: true,
            config_file: PathBuf::new(),
            hub_home: PathBuf::new(),
        }
    }
}

pub fn expand_home(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    p.to_path_buf()
}

fn absolutize(p: &Path, base: &Path) -> PathBuf {
    let e = expand_home(p);
    if e.is_absolute() {
        e
    } else {
        base.join(e)
    }
}

/// Walk up from `start` looking for huba.config.json; the huba directory is
/// wherever that file lives (falls back to `start`).
pub fn find_hub_home(start: &Path) -> PathBuf {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        if dir.join("huba.config.json").is_file() {
            return dir.to_path_buf();
        }
        cur = dir.parent();
    }
    start.to_path_buf()
}

/// Throws on invalid config — a huba that starts with a broken policy is worse
/// than one that refuses to start.
pub fn validate(cfg: &Config) -> Result<()> {
    let mut problems: Vec<String> = vec![];
    if cfg.poll_interval_sec < 10 {
        problems.push("poll_interval_sec must be >= 10".into());
    }
    // A per-call ceiling of 0 does not mean "unlimited" to the CLI — it means
    // the call dies immediately and is still billed for what it loaded.
    if cfg.call.max_budget_usd <= 0.0 {
        problems.push("call.max_budget_usd must be > 0".into());
    }
    if cfg.call.timeout_sec < 10 {
        problems.push("call.timeout_sec must be >= 10".into());
    }
    // 🔴 Hai luật của phòng chat tfl5 (`app_tid` phải có, `trust.tfl5_user_tids`
    // không được rỗng) đã đi theo cái kênh, 2026-08-14. Cổng của Telegram không
    // kiểm được ở đây: khoá của nó là BÍ MẬT (`huba.env`), không phải một trường
    // trong tệp cấu hình — `telegram::Inbox::start` tự bỏ qua CÓ LOG khi thiếu
    // khoá, đúng luật #4 (thiếu bí mật là SKIP-WITH-LOG, không phải chết máy).
    if !problems.is_empty() {
        bail!("invalid huba config:\n  - {}", problems.join("\n  - "));
    }
    Ok(())
}

/// Load config: explicit path → `HUB_CONFIG` → nearest huba.config.json walking
/// up from the current directory. Missing file = defaults.
pub fn load(explicit: Option<&Path>) -> Result<Config> {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let file = match explicit {
        Some(p) => expand_home(p),
        None => match env::var_os("HUB_CONFIG") {
            Some(v) => expand_home(Path::new(&v)),
            None => find_hub_home(&cwd).join("huba.config.json"),
        },
    };

    let mut cfg: Config = if file.is_file() {
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("cannot read config {}", file.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("cannot parse config {}", file.display()))?
    } else {
        Config::default()
    };

    let home = file.parent().map(|p| p.to_path_buf()).unwrap_or(cwd);
    cfg.config_file = file.clone();
    cfg.hub_home = home.clone();

    // BEFORE validate(): some rules read the environment (a non-loopback
    // web.bind requires a password), and under launchd those values live in
    // huba.env. Validating first made a documented setup crash-loop with
    // exit 70 forever.
    let loaded = load_env_file(&home);
    if !loaded.is_empty() {
        crate::logging::info("hub_env_loaded", serde_json::json!({ "keys": loaded }));
    }
    cfg.db = absolutize(&cfg.db.clone(), &home);
    cfg.log_file = absolutize(&cfg.log_file.clone(), &home);
    cfg.notify.file = absolutize(&cfg.notify.file.clone(), &home);
    cfg.workspace_root = if cfg.workspace_root.as_os_str().is_empty() {
        find_workspace_root(&home, &cfg.project_roots)
    } else {
        absolutize(&cfg.workspace_root.clone(), &home)
    };

    validate(&cfg)?;
    Ok(cfg)
}

/// Gốc workspace: **đi ngược lên tìm**, đừng đếm số bậc.
///
/// 🔴 Dòng cũ là `home.parent().parent()` — hai bậc, gõ cứng đúng hình dạng
/// `<workspace>/AI/huba`. Nó đúng cho tới đúng ngày huba không nằm ở đó nữa, và
/// nó sai **không kêu một tiếng**: `workspace_root` trỏ nhầm ⟹ danh sách dự án
/// rỗng, `/new` mở phiên ở nhầm thư mục, bảng sức khoẻ thôi so được cây mã.
/// Cùng một họ với `runtime.rs` từng so bản cài với `~/Documents/projects` sau
/// khi gốc dời đi (2026-08-12): một con số viết sẵn thay cho một phép đo.
///
/// Hà 2026-08-13: *"chuyển ra ngoài thư mục gốc đi"* — huba rời `AI/huba` sang
/// `<workspace>/huba`. Thay vì sửa `2` thành `1` (đổi một hằng số sai lấy một
/// hằng số sai khác), hỏi thẳng câu cần hỏi: **thư mục nào là gốc?** Gốc là
/// thư mục có chứa các NGĂN KÉO dự án mà cấu hình đã khai (`project_roots`,
/// ở máy này là `["", "AI"]`). Đo được, và đúng cho cả hai chỗ huba từng nằm —
/// tức lần chuyển sau nữa cũng không phải đụng vào dòng nào.
///
/// Không có ngăn kéo nào tên tuổi (`project_roots` chỉ có `""`, ca của người
/// mới kéo repo về) thì lấy thư mục cha — huba nằm thẳng trong chỗ làm việc.
/// Trèo tối đa 4 bậc: không tìm thấy thì dừng, đừng leo tới `/`.
fn find_workspace_root(home: &Path, project_roots: &[String]) -> PathBuf {
    let drawers: Vec<&str> = project_roots
        .iter()
        .map(|r| r.trim().trim_matches('/'))
        .filter(|r| !r.is_empty())
        .collect();
    let fallback = home.parent().unwrap_or(home).to_path_buf();
    if drawers.is_empty() {
        return fallback;
    }
    let mut dir = home;
    for _ in 0..4 {
        let Some(parent) = dir.parent() else { break };
        if drawers.iter().any(|d| parent.join(d).is_dir()) {
            return parent.to_path_buf();
        }
        dir = parent;
    }
    fallback
}

// `&Path` would be the better Rust, but serde's `skip_serializing_if` calls
// this with `&PathBuf` and will not compile against the slice.
#[allow(clippy::ptr_arg)]
fn is_empty_path(p: &PathBuf) -> bool {
    p.as_os_str().is_empty()
}

fn relativize(p: &Path, base: &Path) -> PathBuf {
    p.strip_prefix(base)
        .map(|r| r.to_path_buf())
        .unwrap_or_else(|_| p.to_path_buf())
}

/// Persist a config back to its file. Validates first (a UI must not be able to
/// write a config the binary would then refuse to load), keeps one `.bak`, and
/// writes through a temp file + rename so a crash cannot leave a half file.
pub fn save(cfg: &Config) -> Result<()> {
    validate(cfg)?;

    // Paths inside the huba directory go back as relative, so the file stays
    // portable instead of being rewritten full of absolute paths.
    let mut out = cfg.clone();
    out.db = relativize(&cfg.db, &cfg.hub_home);
    out.log_file = relativize(&cfg.log_file, &cfg.hub_home);
    out.notify.file = relativize(&cfg.notify.file, &cfg.hub_home);
    if cfg.workspace_root
        == cfg
            .hub_home
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(&cfg.hub_home)
    {
        out.workspace_root = PathBuf::new(); // default: derived from hub_home
    }

    let text = serde_json::to_string_pretty(&out).context("serialize config")? + "\n";
    let target = &cfg.config_file;
    if target.as_os_str().is_empty() {
        bail!("config has no file path to save to");
    }
    if target.is_file() {
        std::fs::copy(target, target.with_extension("json.bak"))
            .with_context(|| format!("backup {}", target.display()))?;
    }
    let tmp = target.with_extension("json.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, target).with_context(|| format!("rename into {}", target.display()))?;
    Ok(())
}

/// Where a project lives. Most sit under AI/, some (dwork, social, sso-user,
/// uiux, video) sit directly in the workspace root — never hardcode `AI/<name>`.
/// Is this a plausible project NAME, as opposed to a path?
///
/// Load-bearing, not defensive typing. `project` reaches here from the model's
/// structured output (`decision.project`), and `act.rs` feeds the resolved
/// directory to `git worktree add`. Without this, a value like `../../elsewhere`
/// would walk straight out of the workspace and point the act stage at a repo
/// nobody chose. Names are single path segments; anything else is refused.
/// Root first, then `AI/` — see `project_dir`.
fn default_true() -> bool {
    true
}

pub fn default_project_roots() -> Vec<String> {
    vec![String::new(), "AI".into()]
}

pub fn default_claude_cli() -> String {
    "claude".into()
}

/// One Claude CLI account. `config_dir` empty/absent means the AMBIENT account:
/// the CLI selects it by `CLAUDE_CONFIG_DIR` being unset, and setting the
/// variable to the default directory reports "not logged in" — so this is a
/// real distinction, not a formatting choice.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct ClaudeAccountCfg {
    /// Label shown in the UI. Not a credential and not an email address.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_dir: Option<String>,
    /// TỪ chủ máy gõ ở terminal để vào tài khoản này — `claude`, `claude2`,
    /// `claude3`.
    ///
    /// 🔴 Hà 2026-08-15: *"tôi có 3 tài khoản và trên terminal tôi gõ 'claude'
    /// 'claude2' 'claude3' sẽ tương ứng dùng các tài khoản khác nhau"*. Đo trên
    /// máy (`~/.zshrc:51-52`): `alias claude3='CLAUDE_CONFIG_DIR=$HOME/.claude-acc3
    /// claude'` — tức alias giãn ra ĐÚNG thứ huba vẫn tự dựng lấy.
    ///
    /// Vậy vì sao vẫn khai? Vì phép thử CẦU NỐI: ngồi ở máy thì chủ máy gõ
    /// `claude3`, nên cửa sổ huba mở phải mang đúng dòng ấy — anh nhìn vào là
    /// đọc được, và **một nguồn duy nhất** quyết tài khoản. Hôm nay có hai bản
    /// chép: alias trong `.zshrc` và `config_dir` trong `huba.config.json`; hai
    /// bản y hệt nhau cho tới ngày một bên đổi.
    ///
    /// Không khai thì rơi về cách cũ (`CLAUDE_CONFIG_DIR=<dir> claude`) — cùng
    /// kết quả, chỉ khác chỗ đọc. KHÔNG đoán tên alias theo `accN`: đoán tên
    /// một lệnh sắp chạy trên máy người khác là đúng thứ tệp này cấm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch: Option<String>,
}

impl Config {
    /// Accounts to enumerate, with the ambient account as the fallback so a
    /// machine that never configured this still sees its own sessions.
    pub fn claude_accounts_or_ambient(&self) -> Vec<ClaudeAccountCfg> {
        if self.claude_accounts.is_empty() {
            vec![ClaudeAccountCfg {
                name: "mặc định".into(),
                config_dir: None,
                launch: None,
            }]
        } else {
            self.claude_accounts.clone()
        }
    }

    pub fn claude_transcript_root(&self) -> PathBuf {
        if !self.claude_transcript_root.is_empty() {
            return expand_home(Path::new(&self.claude_transcript_root));
        }
        match env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(".claude"),
            None => PathBuf::from(".claude"),
        }
    }
}

pub fn is_project_name(project: &str) -> bool {
    !project.is_empty()
        && project != "unknown"
        && project != "."
        && project != ".."
        && !project.contains('/')
        && !project.contains('\\')
        && !project.starts_with('.')
        && !project.contains('\0')
}

/// Where a project name is looked up, relative to `workspace_root`, in order.
///
/// The workspace root IS the base. `AI/` is one folder inside it, not a second
/// home — which is why root is searched first: a project living at the top
/// level must never be shadowed by a same-named folder under `AI/`.
/// Configurable via `project_roots`, because which folders hold projects is a
/// property of the workspace, not something huba gets to hardcode.
pub fn project_dir(cfg: &Config, project: &str) -> Option<PathBuf> {
    if !is_project_name(project) {
        return None;
    }
    for base in project_bases(cfg) {
        let candidate = base.join(project);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

/// Thư mục này là một DỰ ÁN, hay chỉ là một ngăn kéo đựng dự án?
///
/// Một chỗ duy nhất trả lời, vì câu hỏi này đã được hỏi ở hai nơi với hai câu
/// trả lời khác nhau, và cái khác nhau ấy hiện ra trên màn: `known_projects` đo
/// bằng marker (đúng), còn `sessions::folder_from_tail` gõ cứng đúng một tên
/// ngăn kéo — `"AI"`. Nên phiên làm việc trong `AI/tcc/amm` bị khai là `AI/tcc`
/// (Hà 2026-08-12: *"sao phiên fb rõ ràng là ai/tcc/amm nhưng danh sách phiên
/// chỉ hiện ai/tcc"*), trong khi `AI/tcc` **không có marker nào** — nó là ngăn
/// kéo y hệt `AI`, chỉ khác là không ai nghĩ ra đặt tên nó vào mã.
pub fn looks_like_project(dir: &Path) -> bool {
    ["CLAUDE.md", ".git", "Cargo.toml", "package.json"]
        .iter()
        .any(|marker| dir.join(marker).exists())
        || dir.join("logs").join("devlog.sqlite").exists()
}

/// The directories `project_dir` searches, already joined onto the workspace.
pub fn project_bases(cfg: &Config) -> Vec<PathBuf> {
    cfg.project_roots
        .iter()
        .map(|r| {
            let r = r.trim().trim_matches('/');
            if r.is_empty() {
                cfg.workspace_root.clone()
            } else {
                cfg.workspace_root.join(r)
            }
        })
        .collect()
}

/// Load `<hub_home>/huba.env` (KEY=VALUE lines) into the process environment.
///
/// launchd does NOT read your shell profile, so an auto-started daemon has no
/// `HUB_TELEGRAM_BOT_TOKEN`. Putting secrets in the plist works but spreads them
/// into a file that gets synced/backed up; a single chmod-600 env file next to
/// the config is easier to keep private. Values already present in the
/// environment always win, so an interactive shell can still override.
///
/// Returns the NAMES that were loaded — never the values.
/// Bí mật cho bản chạy dưới launchd, đọc từ `<hub_home>/huba.env` **và**
/// `<hub_home>/.env`.
///
/// Hai file chứ không một, vì 2026-08-10 Hà để khoá Telegram vào `.env` —
/// cái tên mà mọi công cụ khác trên máy đều dùng — rồi báo "cho vào file .env
/// rồi", trong khi huba chỉ nhìn `huba.env` và im lặng không thấy gì. Bắt người
/// dùng nhớ đúng một cái tên riêng của huba là bắt sai người: huba biết đọc cả hai
/// thì rẻ hơn nhiều so với một buổi ngồi hỏi "sao không nhận khoá".
///
/// `huba.env` đọc trước nên nó thắng khi trùng khoá; và **môi trường thật luôn
/// thắng cả hai** — luật cũ, giữ nguyên.
pub fn load_env_file(hub_home: &Path) -> Vec<String> {
    let mut loaded = load_one_env_file(&hub_home.join("huba.env"));
    loaded.extend(load_one_env_file(&hub_home.join(".env")));
    loaded
}

fn load_one_env_file(path: &Path) -> Vec<String> {
    let path = path.to_path_buf();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        // KHÔNG có tệp là chuyện thường (phần lớn máy không dùng `huba.env`) —
        // im lặng đúng. Nhưng "có tệp mà đọc không được" là chuyện khác hẳn:
        // sai quyền sau một lần `chmod`, sai chủ sau một lần `sudo`. Bản trước
        // nuốt cả hai vào `Err(_)`, nên một tệp bí mật không đọc được sẽ hiện
        // ra dưới dạng "chưa đặt biến môi trường" ở tận cuối đường — một chẩn
        // đoán nghe hợp lý mà sai, và không dòng log nào cãi lại được.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return vec![],
        Err(e) => {
            crate::logging::warn(
                "env_file_unreadable",
                serde_json::json!({ "path": path.display().to_string(), "err": e.to_string() }),
            );
            return vec![];
        }
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&path) {
            let mode = meta.permissions().mode() & 0o077;
            if mode != 0 {
                crate::logging::warn(
                    "hub_env_too_open",
                    serde_json::json!({
                        "file": path.display().to_string(),
                        "advice": format!("chmod 600 {}", path.display()),
                    }),
                );
            }
        }
    }

    let mut loaded = vec![];
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            crate::logging::warn("hub_env_bad_line", serde_json::json!({ "line_no": n + 1 }));
            continue;
        };
        let key = k.trim();
        let val = v.trim().trim_matches('"').trim_matches('\'');
        if key.is_empty() {
            crate::logging::warn("hub_env_bad_line", serde_json::json!({ "line_no": n + 1 }));
            continue;
        }
        if env::var_os(key).is_some() {
            continue; // the real environment wins
        }
        env::set_var(key, val);
        loaded.push(key.to_string());
    }
    loaded
}

/// Read a secret by env-var name. `None` when unset or blank so the caller can
/// SKIP-WITH-LOG instead of crashing (charter DoD #6: log-on-skip).
pub fn secret_from_env(name: &str) -> Option<String> {
    let v = env::var(name).ok()?;
    let t = v.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}
