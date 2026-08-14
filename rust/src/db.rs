//! hub.sqlite — the little that has to survive a restart.
//!
//!   runs     per-poll health — a failed poll leaves a row
//!   cursors  poll watermarks, the followed session, the last handover/aside
//!   spend    what the owner's own calls cost, recorded and never shown
//!
//! It used to hold four more tables — `messages`, `decisions`, `outbox`,
//! `dead_letter` — the whole inbox. They went on 2026-08-08 with the product
//! that filled them, but the ROWS stayed. This header used to say "nothing here
//! drops them, because deleting a person's data to tidy up a schema is not a
//! decision code gets to make" — right, until the owner made the decision. He
//! made it 2026-08-10, once the rows had been counted: 379 dead rows no query
//! can reach, stopped dead on 08-08, three of them matching the leak-scan
//! patterns. Data nobody reads that might carry a secret is not tidiness, it is
//! a liability — so schema step 4 drops the four tables, once, logging each
//! table with its row count (`drop_legacy_inbox`).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

pub const SCHEMA_VERSION: i64 = 4;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS schema_meta (
  k TEXT PRIMARY KEY,
  v TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS runs (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  adapter     TEXT NOT NULL,
  phase       TEXT NOT NULL,
  started_at  TEXT NOT NULL,
  finished_at TEXT,
  ok          INTEGER,
  n_new       INTEGER DEFAULT 0,
  skipped     TEXT,
  err         TEXT
);
CREATE INDEX IF NOT EXISTS idx_runs_adapter ON runs(adapter, started_at);

-- Money spent OUTSIDE triage. Every `claude` call costs, and the daily ceiling
-- is only honest if it sees all of them: a handover that quietly spent $0.30
-- would make `daily_budget_usd` a number about one code path rather than about
-- the day (non-negotiable #9).
CREATE TABLE IF NOT EXISTS spend (
  id       INTEGER PRIMARY KEY AUTOINCREMENT,
  ts       TEXT NOT NULL,
  kind     TEXT NOT NULL,
  ref      TEXT,
  cost_usd REAL NOT NULL DEFAULT 0,
  detail   TEXT
);
CREATE INDEX IF NOT EXISTS idx_spend_ts ON spend(ts);

CREATE TABLE IF NOT EXISTS cursors (
  k          TEXT PRIMARY KEY,
  v          TEXT,
  updated_at TEXT NOT NULL
);

"#;

fn now() -> String {
    crate::logging::now_iso()
}

// ─── row types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RunRow {
    pub id: i64,
    pub adapter: String,
    pub phase: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub ok: Option<i64>,
    pub n_new: Option<i64>,
    pub skipped: Option<String>,
    pub err: Option<String>,
}

#[derive(Debug)]
pub struct RunFinish {
    pub ok: bool,
    pub n_new: i64,
    pub err: Option<String>,
    pub skipped: Option<String>,
}

// ─── connection ──────────────────────────────────────────────────────────

pub struct Db {
    pub conn: Connection,
}

/// Bốn bảng của sản phẩm hộp thư đã bị xoá — dọn ở bước nâng cấp lược đồ 4.
///
/// Vì sao chúng còn nằm đó tới hôm nay: `CLAUDE.md` từng ghi *"nothing drops
/// them, and no query can see them"*, tức cố ý để lại chứ không phải quên. Đo
/// lại 2026-08-10 thì lý do "để lại cũng chẳng sao" không còn đứng được:
/// **379 dòng chết** (messages 200 · outbox 90 · decisions 87 · dead_letter 2),
/// dừng đúng ngày 08-08 khi nhánh hộp thư bị xoá, và **3 dòng khớp mẫu bí mật**.
/// Dữ liệu không ai đọc mà có thể mang bí mật thì giữ chỉ là gánh nợ.
///
/// Làm bằng BƯỚC NÂNG CẤP chứ không phải một lệnh gõ tay: nó nằm trong mã, có
/// test, có log, chạy đúng một lần trên mọi máy, và ai đọc lịch sử cũng thấy
/// được vì sao. Hà chốt 2026-08-10; điểm lùi `data/hub-before-legacy-drop.sqlite`
/// đã dựng và đã qua `PRAGMA integrity_check` trước khi bước này ra đời.
fn drop_legacy_inbox(conn: &Connection) -> Result<()> {
    // TẮT kiểm khoá ngoại trong lúc dọn, rồi bật lại — kể cả khi hỏng.
    //
    // Bốn bảng ấy tham chiếu lẫn nhau (outbox → decisions → messages), mà
    // `open()` bật `foreign_keys = ON` ngay phía trên. Bản đầu bỏ qua điều đó
    // và daemon CHẾT NGAY LÚC DỰNG LÊN: `FOREIGN KEY constraint failed`
    // (Error 787), `last exit code = 70`, hub nằm im — đo thật 2026-08-10.
    // Sửa bằng cách tắt kiểm chứ không phải xếp thứ tự xoá: thứ tự đúng hôm nay
    // là thứ tự sai vào ngày ai đó thêm một tham chiếu, còn cái này thì không.
    //
    // 📌 Con lỗi này lộ ra trong 20 giây là nhờ bản vá sáng nay cho
    // `bin/hubd.rs`: trước đó nó chết bằng `eprintln!` và lý do sẽ chỉ nằm ở
    // stderr của launchd, nơi không ai đọc.
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let out = drop_legacy_inbox_inner(conn);
    conn.pragma_update(None, "foreign_keys", "ON")?;
    out
}

fn drop_legacy_inbox_inner(conn: &Connection) -> Result<()> {
    for t in ["messages", "outbox", "decisions", "dead_letter"] {
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![t],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if !exists {
            continue;
        }
        // Đếm TRƯỚC khi xoá: một dòng log nói "đã bỏ 200 dòng" là thứ đọc lại
        // được sau này; "đã dọn xong" thì không nói gì cả.
        let rows: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
            .unwrap_or(-1);
        conn.execute_batch(&format!("DROP TABLE {t}"))?;
        crate::logging::info(
            "schema_legacy_table_dropped",
            serde_json::json!({ "table": t, "rows": rows }),
        );
    }
    Ok(())
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("cannot open db {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;

        // Phiên bản CŨ phải đọc trước khi dán phiên bản mới lên.
        let was: i64 = conn
            .query_row("SELECT v FROM schema_meta WHERE k = 'version'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if was < 4 {
            drop_legacy_inbox(&conn)?;
        }

        conn.execute(
            "INSERT INTO schema_meta (k, v) VALUES ('version', ?1) ON CONFLICT(k) DO UPDATE SET v = excluded.v",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(Db { conn })
    }

    // 🔴 ĐÃ XOÁ cả bộ điều khiển giao dịch — `begin`/`commit`/`rollback`
    // (2026-08-14). Chú thích của chúng nói thẳng chúng phục vụ ai: *"decision
    // + outbox rows + message status ONE commit point"* — tức cái hộp thư đã bị
    // xoá ngày 08-08. Không còn giao dịch nhiều bảng nào trong hub.
    //
    // Ghi lại một bước hụt của chính lượt dọn này, vì nó đúng loại sai đã ghi
    // ở luật 7: tôi viết "giữ `commit` lại, nó dùng trong lượt nâng cấp lược
    // đồ" — nghe hợp lý, và sai. Đếm lại thì `commit()` có ĐÚNG 0 chỗ gọi. Một
    // mệnh đề nghe hợp lý mà chưa đếm thì vẫn chỉ là một mệnh đề.

    // ── decisions ──

    // ── cursors / runs / dead-letter ──

    pub fn get_cursor(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT v FROM cursors WHERE k = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()?
            .flatten())
    }

    /// Mốc đã đặt, và **nói ra khi đọc hỏng**.
    ///
    /// `get_cursor` trả `Result<Option<_>>` với hai nghĩa rất khác nhau:
    /// `Ok(None)` = chưa từng đặt (chuyện thường), còn `Err` = SQLite hỏng thật
    /// — khoá, tệp hỏng, hết đĩa. Gộp cả hai thành "không có" là bug đã đếm được
    /// **12 chỗ** trong mã này (9 chỗ `.ok().flatten()`, 3 chỗ `match … _ =>`),
    /// và hậu quả nhìn từ điện thoại là: bấm ⏹ Dừng trên một phiên đang mở thì
    /// hub trả lời *"chưa theo phiên nào"* — đúng câu nó nói khi chưa ai chọn gì
    /// — mà **không dòng log nào** cho biết cơ sở dữ liệu vừa không đọc được.
    ///
    /// Đặt chốt Ở ĐÂY chứ không ở từng chỗ gọi: có mười hai chỗ gọi, và chỗ thứ
    /// mười ba sẽ quên. Ai thật sự cần phân biệt `Err` với `Ok(None)` thì vẫn
    /// gọi `get_cursor` như cũ.
    pub fn cursor_or_log(&self, key: &str) -> Option<String> {
        match self.get_cursor(key) {
            Ok(v) => v,
            Err(e) => {
                crate::logging::error(
                    "cursor_read_failed",
                    serde_json::json!({ "key": key, "err": e.to_string() }),
                );
                None
            }
        }
    }

    pub fn all_cursors(&self) -> Result<BTreeMap<String, String>> {
        let mut stmt = self.conn.prepare("SELECT k, v FROM cursors")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (k, v) = row?;
            out.insert(k, v.unwrap_or_default());
        }
        Ok(out)
    }

    pub fn set_cursor(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cursors (k, v, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(k) DO UPDATE SET v = excluded.v, updated_at = excluded.updated_at",
            params![key, value, now()],
        )?;
        Ok(())
    }

    pub fn start_run(&self, adapter: &str, phase: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO runs (adapter, phase, started_at) VALUES (?1, ?2, ?3)",
            params![adapter, phase, now()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn finish_run(&self, id: i64, f: RunFinish) -> Result<()> {
        self.conn.execute(
            "UPDATE runs SET finished_at = ?1, ok = ?2, n_new = ?3, err = ?4, skipped = ?5 WHERE id = ?6",
            params![now(), if f.ok { 1 } else { 0 }, f.n_new, f.err.map(|e| crate::exec::truncate(&e, 2000)), f.skipped, id],
        )?;
        Ok(())
    }

    pub fn last_runs(&self, limit: i64) -> Result<Vec<RunRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM runs ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(RunRow {
                id: r.get("id")?,
                adapter: r.get("adapter")?,
                phase: r.get("phase")?,
                started_at: r.get("started_at")?,
                finished_at: r.get("finished_at")?,
                ok: r.get("ok")?,
                n_new: r.get("n_new")?,
                skipped: r.get("skipped")?,
                err: r.get("err")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Today's owner-initiated spend — the `spend` table only.
    pub fn owner_cost_on_day(&self, day: &str) -> Result<f64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM spend WHERE substr(ts, 1, 10) = ?1",
            params![day],
            |r| r.get(0),
        )?)
    }

    /// Book a non-triage `claude` call against the day.
    pub fn record_spend(
        &self,
        kind: &str,
        reference: &str,
        cost_usd: f64,
        detail: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO spend (ts, kind, ref, cost_usd, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![now(), kind, reference, cost_usd, detail],
        )?;
        Ok(())
    }
}
