//! Shared test helpers. Each integration-test binary compiles the whole module,
//! so helpers used by only some of them are legitimately unused elsewhere.
#![allow(dead_code)]

use std::collections::BTreeMap;

use hub::config::{
    ActCfg, Adapters, Autonomy, Config, NotifyCfg, RoutingRule, RoutingWhen, TierName, TriageCfg,
    Trust,
};
use hub::db::{Db, Message, NewMessage};
use serde_json::Value;
use tempfile::TempDir;

/// A message as it would look after ingest, without touching a database.
pub fn msg(source: &str, sender: &str, raw: Value) -> Message {
    Message {
        id: 1,
        source: source.into(),
        external_id: "x".into(),
        thread_key: None,
        project: None,
        sender: Some(sender.into()),
        sender_trust: "untrusted".into(),
        subject: None,
        body: None,
        url: None,
        raw: Some(raw.to_string()),
        received_at: None,
        ingested_at: "2026-07-26T00:00:00.000Z".into(),
        status: "new".into(),
        attempts: 0,
        last_error: None,
        claimed_at: None,
        coalesced_into: None,
    }
}

pub fn with_subject(mut m: Message, subject: &str) -> Message {
    m.subject = Some(subject.into());
    m
}

pub fn cfg_for_tests() -> Config {
    let mut projects = BTreeMap::new();
    projects.insert("tfl5".to_string(), "L2".to_string());
    projects.insert("sdvi".to_string(), "L0".to_string());

    Config {
        autonomy: Autonomy {
            default: TierName("L1".into()),
            projects,
        },
        triage: TriageCfg {
            min_confidence_auto: 0.8,
            ..TriageCfg::default()
        },
        act: ActCfg::default(),
        adapters: Adapters::default(),
        trust: Trust {
            github_logins: vec!["dipgle".into()],
            emails: vec!["owner@dipgle.com".into()],
            telegram_chat_ids: vec!["12345".into()],
            tfl5_user_tids: vec!["u-owner".into()],
            trusted_sources: vec!["devlog".into(), "cli".into()],
        },
        routing: vec![
            RoutingRule {
                when: RoutingWhen {
                    repo: Some("dipgle/tcc-node".into()),
                    ..Default::default()
                },
                project: "tcc".into(),
            },
            RoutingRule {
                when: RoutingWhen {
                    source: Some("email".into()),
                    subject_contains: Some("sdvi".into()),
                    ..Default::default()
                },
                project: "sdvi".into(),
            },
        ],
        notify: NotifyCfg {
            macos_notification: false,
            ..NotifyCfg::default()
        },
        ..Config::default()
    }
}

/// A throwaway database; the TempDir must stay alive for the test's duration.
pub fn fresh_db() -> (Db, TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&dir.path().join("hub.sqlite")).expect("open db");
    (db, dir)
}

pub fn sample_new_message(external_id: &str) -> NewMessage {
    NewMessage {
        source: "github".into(),
        external_id: external_id.into(),
        thread_key: Some("dipgle/tfl5:Issue:x".into()),
        project: Some("tfl5".into()),
        sender: Some("dipgle".into()),
        sender_trust: Some("trusted".into()),
        subject: Some("[dipgle/tfl5] CI failed".into()),
        body: Some("log...".into()),
        url: None,
        received_at: Some("2026-07-26T00:00:00Z".into()),
        raw: Some(serde_json::json!({ "repo": "dipgle/tfl5" })),
    }
}
