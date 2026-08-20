//! Shared test helpers. Each integration-test binary compiles the whole module,
//! so helpers used by only some of them are legitimately unused elsewhere.
#![allow(dead_code)]

use huba::config::{Config, NotifyCfg};
use huba::db::Db;
use tempfile::TempDir;

pub fn cfg_for_tests() -> Config {
    Config {
        // 🔴 `adapters` + `trust` gỡ 2026-08-14 cùng tfl5. Thứ còn phải đặt tay
        // ở đây là cái duy nhất có tác dụng phụ ra ngoài tiến trình: một bài
        // kiểm không được bắn thông báo lên màn hình máy thật.
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
    let db = Db::open(&dir.path().join("huba.sqlite")).expect("open db");
    (db, dir)
}
