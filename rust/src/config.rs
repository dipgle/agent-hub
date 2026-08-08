//! Config loading + validation.
//!
//! Secrets NEVER live in the config file — only the NAME of the env var that
//! holds them (`token_env`, `api_key_env`). Charter DoD #8.
//!
//! `#[serde(default)]` on every struct reproduces the prototype's deep-merge:
//! a key absent from hub.config.json falls back to the default, and sibling
//! keys inside a partially-specified table keep theirs.

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const TIERS: [&str; 3] = ["L0", "L1", "L2"];

/// Actions the hub may never perform without a human pressing approve —
/// regardless of tier or config. Deliberately not configurable.
pub const ALWAYS_HUMAN_ACTIONS: [&str; 5] = [
    "deploy",
    "merge",
    "force_push",
    "delete_data",
    "rotate_secret",
];

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct TriageCfg {
    pub model: String,
    pub max_budget_usd: f64,
    pub timeout_sec: u64,
    pub min_confidence_auto: f64,
    pub context_bytes: usize,
}

impl Default for TriageCfg {
    fn default() -> Self {
        Self {
            model: "sonnet".into(),
            max_budget_usd: 0.5,
            timeout_sec: 240,
            min_confidence_auto: 0.8,
            context_bytes: 6000,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ActCfg {
    pub enabled: bool,
    pub model: String,
    pub max_budget_usd: f64,
    pub timeout_sec: u64,
}

impl Default for ActCfg {
    fn default() -> Self {
        Self {
            enabled: false,
            model: "sonnet".into(),
            max_budget_usd: 3.0,
            timeout_sec: 1800,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Autonomy {
    pub default: TierName,
    pub projects: BTreeMap<String, String>,
}

/// Newtype so the default is "L0" (draft only) rather than an empty string.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct TierName(pub String);

impl Default for TierName {
    fn default() -> Self {
        TierName("L0".into())
    }
}

impl std::ops::Deref for TierName {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
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
    /// baseline rather than triaging (and paying for) every old message.
    pub backfill: bool,
    /// How long to wait for tfl5 to echo a sent message back before calling
    /// delivery unconfirmed.
    pub reply_timeout_sec: u64,
    /// Hold messages younger than this so a burst of short lines becomes ONE
    /// triage call instead of one per line. 0 disables the wait.
    pub silence_window_sec: u64,
    /// Drop anything shorter than this before it reaches the model. "ok" and
    /// "👍" are not questions, and each one would cost real money.
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
    /// room is untrusted here until listed, so `policy.rs` keeps them at L0.
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

/// Every field is optional and only the ones present must match. They are
/// skipped when empty so a saved config stays readable instead of filling up
/// with explicit nulls.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RoutingWhen {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_contains: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_contains: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoutingRule {
    pub when: RoutingWhen,
    pub project: String,
}

/// One project, registered under its FOLDER NAME.
///
/// The single registry. Before this there were three lists that had to be kept
/// in step by hand — a `routing` table whose right-hand side was free text, a
/// per-project tier map, and an implicit set of folders — and they drifted:
/// six of the eight routed projects had no devlog, and a typo in any of them
/// produced no error at all, just a hub that answered with no context.
///
/// Now the folder is the identity and everything else hangs off it. A GitHub
/// repo is an OPTION on a project, not a separate table with its own idea of
/// what a project is.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ProjectCfg {
    /// Autonomy tier for this project. Absent = `autonomy.default`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// GitHub repos that belong to this project — the common case, spelled
    /// simply instead of as a routing rule.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<String>,
    /// Anything the `repos` shorthand cannot express (sender, chat id, subject
    /// or body text). Same matcher the old routing table used, so nothing that
    /// worked before becomes unexpressible.
    #[serde(rename = "match", skip_serializing_if = "Vec::is_empty")]
    pub matchers: Vec<RoutingWhen>,
    /// Free note for humans; hub never reads it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct WebCfg {
    /// Serve the console from inside `hubd` too, so one launchd agent gives you
    /// both the loop and the UI.
    pub enabled: bool,
    pub port: u16,
    /// Interface to bind. Loopback by default **on purpose**: the per-boot token
    /// is embedded in the page, so anyone who can load `/` is already trusted.
    pub bind: String,
    /// Name of the env var holding a password. REQUIRED to bind anything other
    /// than loopback — see `is_loopback_bind`.
    pub password_env: String,
    /// Extra hostnames accepted in the `Host` header when bound off-loopback
    /// (the domain a reverse proxy fronts this with). Loopback binds always
    /// accept 127.0.0.1 / localhost / ::1 and nothing else.
    pub allowed_hosts: Vec<String>,
}

impl Default for WebCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 9200,
            bind: "127.0.0.1".into(),
            password_env: "HUB_WEB_PASSWORD".into(),
            allowed_hosts: vec![],
        }
    }
}

pub fn is_loopback_bind(bind: &str) -> bool {
    matches!(bind, "127.0.0.1" | "::1" | "localhost")
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
    pub max_triage_per_cycle: i64,
    /// A repeat item on a thread that already has an unanswered decision is
    /// attached to it instead of triaged again (0 disables). One triage call
    /// costs $0.05–$0.11 (measured 2026-07-26) and CI repeats itself.
    pub coalesce_hours: i64,
    pub triage: TriageCfg,
    pub act: ActCfg,
    pub autonomy: Autonomy,
    pub adapters: Adapters,
    pub trust: Trust,
    /// THE project registry, keyed by folder name under `project_roots`.
    /// Replaces `routing` + `autonomy.projects`; both are still read for
    /// existing configs (see `resolve_project` / `effective_tier`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, ProjectCfg>,
    /// Legacy repo→project table. Kept working so an existing config does not
    /// break; new entries belong in `projects`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routing: Vec<RoutingRule>,
    /// Extra regexes that must never appear in an outbound auto-reply.
    pub leak_patterns: Vec<String>,
    pub notify: NotifyCfg,
    pub web: WebCfg,
    /// Per-source ceilings, on top of `daily_budget_usd`. A chat room open to
    /// other people must not be able to spend the whole day's budget before
    /// anything else is looked at. Mechanism, not policy: any source name
    /// works, absent = no per-source ceiling.
    #[serde(default)]
    pub source_daily_budget_usd: std::collections::BTreeMap<String, f64>,
    /// Per-source override of `coalesce_hours`.
    ///
    /// The global default assumes a thread IS a topic — true for a GitHub
    /// issue, false for a chat room, where `thread_key` is the ROOM and 12
    /// hours of unrelated questions would collapse into whichever decision
    /// happened to be open. Observed for real on 2026-08-06: a genuine
    /// question was swallowed into a pending decision about an earlier smoke
    /// message, and the draft answered the wrong thing. Chat wants minutes
    /// (fractional hours), not hours.
    #[serde(default)]
    pub source_coalesce_hours: std::collections::BTreeMap<String, f64>,
    /// How long a conversation on one thread keeps its `claude` session, per
    /// source. Set for chat, where "và cái kia thì sao?" only means anything if
    /// the previous turn is still in context; absent/0 = every message is
    /// triaged from a clean session, which is the safer default and what every
    /// non-chat source wants.
    ///
    /// A thread that ever tripped the injection wire is never resumed — see
    /// `Db::last_session_for_thread`.
    #[serde(default)]
    pub source_thread_memory_hours: std::collections::BTreeMap<String, f64>,
    /// Hard ceiling on triage spend per calendar day (UTC). 0 = no ceiling.
    /// Exists because an always-on daemon spends money while nobody watches:
    /// once the day's decisions add up to this, triage stops and says so.
    pub daily_budget_usd: f64,

    /// Ceiling for actions the OWNER initiates (a button on his phone), kept
    /// apart from `daily_budget_usd`.
    ///
    /// `daily_budget_usd` exists to rein in an unattended robot triaging an
    /// inbox nobody asked for. A tap on the phone is not that: it is the owner
    /// doing his own work through a different keyboard, and refusing it because
    /// the robot's noise budget is gone makes no sense. Counted from the
    /// `spend` table only. 0 = no ceiling of its own.
    #[serde(default = "default_owner_budget")]
    pub owner_daily_budget_usd: f64,
    /// The `claude` executable used to LIST sessions (`sessions.rs`). Separate
    /// from whatever triage spawns: this one only ever reads.
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
            max_triage_per_cycle: 8,
            coalesce_hours: 12,
            triage: TriageCfg::default(),
            act: ActCfg::default(),
            autonomy: Autonomy::default(),
            adapters: Adapters::default(),
            trust: Trust::default(),
            projects: BTreeMap::new(),
            routing: vec![],
            leak_patterns: vec![],
            notify: NotifyCfg::default(),
            web: WebCfg::default(),
            daily_budget_usd: 5.0,
            source_daily_budget_usd: Default::default(),
            source_coalesce_hours: Default::default(),
            source_thread_memory_hours: Default::default(),
            owner_daily_budget_usd: default_owner_budget(),
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
    if !TIERS.contains(&&*cfg.autonomy.default) {
        problems.push(format!(
            "autonomy.default must be one of {}",
            TIERS.join("|")
        ));
    }
    for (proj, tier) in &cfg.autonomy.projects {
        if !TIERS.contains(&tier.as_str()) {
            problems.push(format!(
                "autonomy.projects.{proj} = {tier} is not a valid tier"
            ));
        }
    }
    if cfg.poll_interval_sec < 10 {
        problems.push("poll_interval_sec must be >= 10".into());
    }
    if !(0.0..=1.0).contains(&cfg.triage.min_confidence_auto) {
        problems.push("triage.min_confidence_auto must be within 0..1".into());
    }
    if cfg.triage.max_budget_usd <= 0.0 {
        problems.push("triage.max_budget_usd must be > 0".into());
    }
    if cfg.max_triage_per_cycle < 1 {
        problems.push("max_triage_per_cycle must be >= 1".into());
    }
    for rule in &cfg.routing {
        if rule.project.trim().is_empty() {
            problems.push("routing rule needs a non-empty project".into());
        }
    }
    if cfg.daily_budget_usd < 0.0 {
        problems.push("daily_budget_usd must be >= 0 (0 disables the ceiling)".into());
    }
    if cfg.web.enabled && cfg.web.port < 1024 {
        problems.push("web.port must be >= 1024 (no privileged ports)".into());
    }
    // Exposing the console off-loopback without a password would hand full
    // approve/config rights to anyone who can reach the port, because the page
    // itself carries the API token.
    if cfg.web.enabled
        && !is_loopback_bind(&cfg.web.bind)
        && secret_from_env(&cfg.web.password_env).is_none()
    {
        problems.push(format!(
            "web.bind = {} is not loopback, so {} must be set (or put the UI behind an SSH tunnel and keep bind=127.0.0.1)",
            cfg.web.bind, cfg.web.password_env
        ));
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

pub fn default_owner_budget() -> f64 {
    2.0
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
/// `HUB_TELEGRAM_TOKEN`. Putting secrets in the plist works but spreads them
/// into a file that gets synced/backed up; a single chmod-600 env file next to
/// the config is easier to keep private. Values already present in the
/// environment always win, so an interactive shell can still override.
///
/// Returns the NAMES that were loaded — never the values.
pub fn load_env_file(hub_home: &Path) -> Vec<String> {
    let path = hub_home.join("hub.env");
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
                    serde_json::json!({ "file": path.display().to_string(), "advice": "chmod 600 hub.env" }),
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
