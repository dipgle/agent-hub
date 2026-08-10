//! Config loading + validation.
//!
//! Secrets NEVER live in the config file — only the NAME of the env var that
//! holds them (`user_env`, `password_env`). Charter DoD #8.
//!
//! `#[serde(default)]` on every struct reproduces the prototype's deep-merge:
//! a key absent from hub.config.json falls back to the default, and sibling
//! keys inside a partially-specified table keep theirs.

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

// `TIERS` and `ALWAYS_HUMAN_ACTIONS` lived here until 2026-08-08. They were the
// vocabulary of `policy.rs` — how much the robot could do unattended, and the
// five things it could never do at any tier. With no robot deciding anything,
// there is no tier to set: hub does exactly what the owner typed, and the wall
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
        Self { enabled: true, at_percent: 80, idle_sec: 120 }
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
    /// Tắt thì lệnh huỷ chạy thẳng như trước. Bật mà thiếu khoá thì hub TỪ
    /// CHỐI lệnh chứ không lặng lẽ bỏ qua chốt chặn — xem `confirm.rs`.
    pub enabled: bool,
    pub bot_token_env: String,
    pub chat_id_env: String,
    /// Chờ bấm nút bao lâu rồi coi như không đồng ý.
    pub timeout_sec: u64,
}

impl Default for ConfirmCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            bot_token_env: "HUB_TELEGRAM_BOT_TOKEN".to_string(),
            chat_id_env: "HUB_TELEGRAM_CHAT_ID".to_string(),
            timeout_sec: 90,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CallCfg {
    /// Ceiling for ONE `claude` call hub makes on the owner's behalf, and a
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

/// The tfl5 chat room hub talks through. `base_url` is the tfl5 server, NOT a
/// port hub opens — hub only ever dials out (see `adapters/tfl5.rs`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Tfl5Cfg {
    pub enabled: bool,
    pub base_url: String,
    pub app_tid: String,
    pub room: String,
    /// Env var NAMES only — the values never live in this file (rule #3).
    pub user_env: String,
    pub password_env: String,
    pub limit: i64,
    /// First poll of a room that already has history: take the tip as the
    /// baseline rather than replaying every old line looking for orders.
    pub backfill: bool,
    /// How long to wait for tfl5 to echo a sent message back before calling
    /// delivery unconfirmed.
    pub reply_timeout_sec: u64,
    /// Hold lines younger than this before reading them, so a burst that is
    /// still being typed arrives whole. 0 disables the wait.
    pub silence_window_sec: u64,
    /// Ignore anything shorter than this. "ok" and "👍" are not orders.
    pub min_chars: usize,
    /// Hold the `/ws/chat` socket open in `hubd` so tfl5 pushes messages
    /// instead of hub asking every cycle. The poller stays on regardless as
    /// the durable backstop — turning this off costs latency, not messages.
    pub live: bool,
}

impl Default for Tfl5Cfg {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://localhost:8090".into(),
            app_tid: String::new(),
            room: "hub".into(),
            user_env: "HUB_TFL5_USER".into(),
            password_env: "HUB_TFL5_PASSWORD".into(),
            limit: 50,
            backfill: false,
            reply_timeout_sec: 15,
            silence_window_sec: 10,
            min_chars: 3,
            live: true,
        }
    }
}

/// One channel. Was five until 2026-08-08 — see `adapters/mod.rs`.
///
/// Unknown keys in an existing `hub.config.json` are ignored by serde, so a
/// file still carrying `github`/`devlog`/`email`/`telegram` loads fine; the
/// next `config::save` drops them.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Adapters {
    pub tfl5: Tfl5Cfg,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Trust {
    /// tfl5 `user_tid`s hub treats as the owner. Deliberately SEPARATE from
    /// tfl5's own ACL: tfl5 answers "may this account enter the room", which is
    /// not the same question as "may this person make hub act". Everyone in the
    /// room is untrusted here until listed, and `tfl5::parse_command` refuses
    /// their orders outright (logged, never silently dropped).
    pub tfl5_user_tids: Vec<String>,
    pub trusted_sources: Vec<String>,
}

impl Default for Trust {
    fn default() -> Self {
        Self {
            tfl5_user_tids: vec![],
            trusted_sources: vec!["cli".into()],
        }
    }
}

/// One project, registered under its FOLDER NAME — which is also the name
/// `/project` and `/new` accept.
///
/// It used to carry `repos` (the GitHub repositories that routed mail to this
/// project) and `tier` (how much the robot could answer on its own). Both were
/// answers to questions the inbox asked. A project is now just a folder hub can
/// open a session in, so the only thing left worth storing is a note for a
/// person — and even that, hub never reads.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ProjectCfg {
    /// Free note for humans; hub never reads it.
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
    /// Empty = derive from the hub directory (`<workspace>/AI/hub` → workspace).
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
    /// by serde, so an old `hub.config.json` still loads; the next save drops it.
    pub call: CallCfg,
    /// Tự đóng sổ khi ngữ cảnh đầy — xem [`AutoHandoverCfg`].
    pub auto_handover: AutoHandoverCfg,
    /// Xác nhận lần hai cho lệnh không lùi lại được — xem [`ConfirmCfg`].
    #[serde(default)]
    pub confirm: ConfirmCfg,
    pub adapters: Adapters,
    pub trust: Trust,
    /// THE project registry, keyed by folder name under `project_roots`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, ProjectCfg>,
    pub notify: NotifyCfg,
    /// The `claude` executable used to LIST sessions (`sessions.rs`).
    #[serde(default = "default_claude_cli")]
    pub claude_cli: String,
    /// Accounts to enumerate. Empty = just the ambient account, which is the
    /// only thing true on a fresh machine; this Mac has three, declared in
    /// `hub.config.json`. Mechanism, not policy — the code must not know a
    /// particular person's setup.
    #[serde(default)]
    pub claude_accounts: Vec<ClaudeAccountCfg>,
    /// Where `projects/<slug>/<session>.jsonl` lives. Empty = `~/.claude`.
    /// One root covers every account while their `projects` dirs are symlinked
    /// together (true here since 2026-08-06).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub claude_transcript_root: String,

    #[serde(skip)]
    pub config_file: PathBuf,
    #[serde(skip)]
    pub hub_home: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db: PathBuf::from("data/hub.sqlite"),
            log_file: PathBuf::from("logs/hub.log"),
            workspace_root: PathBuf::new(),
            project_roots: default_project_roots(), // filled in by load(): <hub_home>/../..
            poll_interval_sec: 120,
            call: CallCfg::default(),
            auto_handover: AutoHandoverCfg::default(),
            confirm: ConfirmCfg::default(),
            adapters: Adapters::default(),
            trust: Trust::default(),
            projects: BTreeMap::new(),
            notify: NotifyCfg::default(),
            claude_cli: default_claude_cli(),
            claude_accounts: vec![],
            claude_transcript_root: String::new(),
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

/// Walk up from `start` looking for hub.config.json; the hub directory is
/// wherever that file lives (falls back to `start`).
pub fn find_hub_home(start: &Path) -> PathBuf {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        if dir.join("hub.config.json").is_file() {
            return dir.to_path_buf();
        }
        cur = dir.parent();
    }
    start.to_path_buf()
}

/// Throws on invalid config — a hub that starts with a broken policy is worse
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
    // The room is where orders come from, so an app_tid that is not set means
    // hub is listening to nothing — say it at startup, not in a silent no-op.
    if cfg.adapters.tfl5.enabled && cfg.adapters.tfl5.app_tid.trim().is_empty() {
        problems.push("adapters.tfl5.enabled needs adapters.tfl5.app_tid".into());
    }
    // Without an owner tid every slash command in the room is refused, and the
    // refusal is only visible in the log — a hub nobody can drive.
    if cfg.adapters.tfl5.enabled && cfg.trust.tfl5_user_tids.is_empty() {
        problems.push("trust.tfl5_user_tids is empty, so no one can give hub an order".into());
    }
    if !problems.is_empty() {
        bail!("invalid hub config:\n  - {}", problems.join("\n  - "));
    }
    Ok(())
}

/// Load config: explicit path → `HUB_CONFIG` → nearest hub.config.json walking
/// up from the current directory. Missing file = defaults.
pub fn load(explicit: Option<&Path>) -> Result<Config> {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let file = match explicit {
        Some(p) => expand_home(p),
        None => match env::var_os("HUB_CONFIG") {
            Some(v) => expand_home(Path::new(&v)),
            None => find_hub_home(&cwd).join("hub.config.json"),
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
    // hub.env. Validating first made a documented setup crash-loop with
    // exit 70 forever.
    let loaded = load_env_file(&home);
    if !loaded.is_empty() {
        crate::logging::info("hub_env_loaded", serde_json::json!({ "keys": loaded }));
    }
    cfg.db = absolutize(&cfg.db.clone(), &home);
    cfg.log_file = absolutize(&cfg.log_file.clone(), &home);
    cfg.notify.file = absolutize(&cfg.notify.file.clone(), &home);
    cfg.workspace_root = if cfg.workspace_root.as_os_str().is_empty() {
        // <workspace>/AI/hub → <workspace>
        home.parent()
            .and_then(|p| p.parent())
            .unwrap_or(&home)
            .to_path_buf()
    } else {
        absolutize(&cfg.workspace_root.clone(), &home)
    };

    validate(&cfg)?;
    Ok(cfg)
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

    // Paths inside the hub directory go back as relative, so the file stays
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
}

impl Config {
    /// Accounts to enumerate, with the ambient account as the fallback so a
    /// machine that never configured this still sees its own sessions.
    pub fn claude_accounts_or_ambient(&self) -> Vec<ClaudeAccountCfg> {
        if self.claude_accounts.is_empty() {
            vec![ClaudeAccountCfg {
                name: "mặc định".into(),
                config_dir: None,
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
/// property of the workspace, not something hub gets to hardcode.
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

/// Load `<hub_home>/hub.env` (KEY=VALUE lines) into the process environment.
///
/// launchd does NOT read your shell profile, so an auto-started daemon has no
/// `HUB_TFL5_PASSWORD`. Putting secrets in the plist works but spreads them
/// into a file that gets synced/backed up; a single chmod-600 env file next to
/// the config is easier to keep private. Values already present in the
/// environment always win, so an interactive shell can still override.
///
/// Returns the NAMES that were loaded — never the values.
/// Bí mật cho bản chạy dưới launchd, đọc từ `<hub_home>/hub.env` **và**
/// `<hub_home>/.env`.
///
/// Hai file chứ không một, vì 2026-08-10 Hà để khoá Telegram vào `.env` —
/// cái tên mà mọi công cụ khác trên máy đều dùng — rồi báo "cho vào file .env
/// rồi", trong khi hub chỉ nhìn `hub.env` và im lặng không thấy gì. Bắt người
/// dùng nhớ đúng một cái tên riêng của hub là bắt sai người: hub biết đọc cả hai
/// thì rẻ hơn nhiều so với một buổi ngồi hỏi "sao không nhận khoá".
///
/// `hub.env` đọc trước nên nó thắng khi trùng khoá; và **môi trường thật luôn
/// thắng cả hai** — luật cũ, giữ nguyên.
pub fn load_env_file(hub_home: &Path) -> Vec<String> {
    let mut loaded = load_one_env_file(&hub_home.join("hub.env"));
    loaded.extend(load_one_env_file(&hub_home.join(".env")));
    loaded
}

fn load_one_env_file(path: &Path) -> Vec<String> {
    let path = path.to_path_buf();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return vec![], // absent is the normal case
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
