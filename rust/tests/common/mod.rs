//! Shared test helpers. Each integration-test binary compiles the whole module,
//! so helpers used by only some of them are legitimately unused elsewhere.
#![allow(dead_code)]

use hub::config::{Adapters, Config, NotifyCfg, Trust};
use hub::db::Db;
use tempfile::TempDir;

pub fn cfg_for_tests() -> Config {
    Config {
        adapters: Adapters::default(),
        trust: Trust {
            tfl5_user_tids: vec!["u-owner".into()],
            trusted_sources: vec!["cli".into()],
        },
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
