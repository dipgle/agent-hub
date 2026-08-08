//! Outbound leak scan — the last gate before text leaves this machine.
//!
//! WHY THIS EXISTS (observed, not theoretical): on 2026-07-26 a real triage run
//! produced a reply draft whose evidence lines quoted the workspace auto-memory
//! ("memory: tfl5 security hardening 2026-07 — CÒN OPEN: PG SPOF, …"). The
//! triage subprocess has no tools, but `claude -p` still loads the workspace's
//! memory and instruction files into its context. Fine for a brief the owner
//! reads; not fine in a reply to an outside sender.
//!
//! A hit does not rewrite the text — it downgrades the item to human review,
//! because silently truncating a reply is its own kind of failure.

use std::sync::OnceLock;

use regex::Regex;

const PATTERNS: [(&str, &str); 10] = [
    (r"(?i)\bvps-[ab]\b", "internal_host"),
    (r"\b(?:\d{1,3}\.){3}\d{1,3}\b", "ip_address"),
    (r"(?i)/Users/[a-z0-9._-]+/", "local_filesystem_path"),
    (
        r"(?i)\b(?:memory|MEMORY\.md|active-context\.md|CLAUDE\.md)\s*:",
        "internal_notes_citation",
    ),
    (r"(?i)\[\[[a-z0-9-]+\]\]", "memory_wikilink"),
    (
        r"(?i)\b(api[_-]?key|bearer|password|secret|access[_-]?token|private[_-]?key)\b",
        "credential_word",
    ),
    // The same words in the language this workspace actually speaks. Without
    // this the gate is decorative: on 2026-08-08 a session transcript said
    // "Mật khẩu là <value>" in plain text and every English pattern passed it
    // through. A leak scan that only reads English on a Vietnamese machine
    // reports zero and means nothing.
    (
        r"(?i)m[aậ]t\s*kh[aẩ]u|m[aã]\s*b[ií]\s*m[aậ]t|kho[áa]\s*(b[ií]\s*m[aậ]t|ri[êe]ng)|th[oô]ng\s*tin\s*[đd][aă]ng\s*nh[aậ]p",
        "credential_word_vi",
    ),
    (
        r"\b(sk-[A-Za-z0-9]{10,}|gh[pousr]_[A-Za-z0-9]{20,}|\d{8,10}:[A-Za-z0-9_-]{30,})\b",
        "credential_literal",
    ),
    (r"-----BEGIN [A-Z ]*PRIVATE KEY-----", "private_key_block"),
    (
        r"(?i)\bSPOF\b|\bchưa fix\b|\bblocker\b",
        "internal_risk_language",
    ),
];

fn compiled() -> &'static Vec<(Regex, &'static str)> {
    static C: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    C.get_or_init(|| {
        PATTERNS
            .iter()
            .map(|(src, label)| {
                (
                    Regex::new(src).expect("built-in leak pattern must compile"),
                    *label,
                )
            })
            .collect()
    })
}

/// Channels whose payload actually leaves this machine.
///
/// `tfl5` counts: the room lives on a server other people can read, so a reply
/// posted there has left this machine exactly as much as an email has.
pub const EXTERNAL_CHANNELS: [&str; 4] = ["github", "email", "telegram", "tfl5"];

pub fn is_external_channel(channel: &str) -> bool {
    EXTERNAL_CHANNELS.contains(&channel)
}

/// Compile `leak_patterns` from config; a bad pattern is reported, never swallowed.
pub fn compile_extra(list: &[String]) -> Vec<(Regex, String)> {
    let mut out = vec![];
    for src in list {
        match Regex::new(&format!("(?i){src}")) {
            Ok(re) => out.push((re, format!("config:{}", crate::exec::truncate(src, 30)))),
            Err(e) => crate::logging::error(
                "bad_leak_pattern",
                serde_json::json!({ "pattern": src, "err": e.to_string() }),
            ),
        }
    }
    out
}

/// Labels of everything in `text` that must not go out unreviewed.
pub fn leak_scan(text: &str, extra: &[(Regex, String)]) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }
    let mut hits: Vec<String> = vec![];
    for (re, label) in compiled() {
        if re.is_match(text) && !hits.iter().any(|h| h == label) {
            hits.push((*label).to_string());
        }
    }
    for (re, label) in extra {
        if re.is_match(text) && !hits.contains(label) {
            hits.push(label.clone());
        }
    }
    hits
}
