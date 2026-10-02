//! SỔ TIN ĐẾN — mỗi update Telegram nằm trên ĐĨA trước khi con dấu `offset` tiến.
//!
//! 🔴 Vì sao có (02/10). Tối ấy máy hết pin; Hà nói đã trả lời 5 câu cho một
//! phiên mà không tìm lại được ở đâu trên máy, rồi hỏi *"sao không có log dự
//! phòng"* và bảo phiên dhub *"chuyển đề xuất sang phiên huba"*. Lần ấy câu trả
//! lời chưa từng tới huba (đo: tin cuối trước lúc tắt là một `/shot`), nhưng chỗ
//! hở thì có thật: vòng đọc `getUpdates` tiến `offset` rồi mới giao update cho
//! luồng thợ, và từ đó tới lúc lệnh chạy xong mệnh lệnh chỉ sống trong BỘ NHỚ.
//! Lượt `getUpdates` kế tiếp (gọi ngay) mang `offset` mới ⟹ Telegram coi như đã
//! giao, không giao lại. Máy tắt, `hubad` chết, hay một lượt `install_update.sh`
//! khởi động lại daemon trong khoảng ấy ⟹ lệnh mất, không một dòng nào nói ra.
//! Luật 6 (*"a crash must re-read, never skip"*) phủ `focus:session`, sổ watch,
//! ghim dự án — chưa phủ tin đến. Nay phủ.
//!
//! Bước của một tin, và việc lượt khởi động lại làm với từng bước:
//!
//! | bước | nghĩa | khởi động lại thì |
//! |---|---|---|
//! | `nhan` | đã ghi đĩa, lệnh CHƯA bắt đầu chạy | chạy lại — trừ khi đã quá [`crate::telegram::TOO_OLD_SEC`] |
//! | `chay` | lệnh đã bắt đầu chạy | KHÔNG chạy lại — báo Hà, chuyển `do` |
//! | `xong` | chạy xong, hoặc tin không sinh lệnh nào | không làm gì |
//! | `do`   | đang chạy dở lúc huba tắt (đã báo) | không làm gì |
//! | `bo`   | quá cũ nên không chạy (đã báo) | không làm gì |
//!
//! Vì sao `chay` thì không chạy lại: chữ gõ vào phiên, `/new`, `/upgrade` đều
//! không lùi lại được — chạy lại một `/type` là gõ HAI lần vào phiên, chạy lại
//! `/upgrade` là tự khởi động lại mãi. Cùng luật với [`crate::so_viec`]: việc đã
//! bắt đầu chạy thì không tự chạy lại.
//!
//! Vì sao SQLite chứ không Redis như `so_viec`: ca phải đỡ là MÁY TẮT, và huba
//! đã cất mọi thứ phải sống qua khởi động lại trong `data/huba.sqlite` — không
//! phụ thuộc một tiến trình thứ hai còn sống hay không. Tin đến đi theo nhịp
//! người gõ, nên một lượt ghi đồng bộ không nghẽn gì: đo 02/10 trên máy này,
//! `synchronous=FULL` + `fullfsync=ON` ⟹ trung vị 0,55 ms · p90 1,7 ms · tối đa
//! 21 ms một lượt ghi.
//!
//! Chữ của tin nằm [`GIU_NGAY`] ngày rồi dọn ([`don_cu`]) — đây chính là "log dự
//! phòng" Hà hỏi: tin đã tới huba thì tra lại được, kể cả tin không chạy.
//! Tra: `sqlite3 data/huba.sqlite "SELECT nhan_luc, buoc, chu FROM tin_den ORDER BY update_id DESC LIMIT 20"`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::logging;

/// Số ngày giữ một tin đã khép (`xong` · `do` · `bo`) trước khi dọn.
pub const GIU_NGAY: i64 = 7;

/// Bước của một tin. Chuỗi lưu trong sổ là [`Buoc::as_str`] — đổi chuỗi là đổi
/// nghĩa của mọi dòng đang nằm trên đĩa, nên chuỗi là HẰNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Buoc {
    /// Đã ghi đĩa; lệnh chưa bắt đầu chạy.
    Nhan,
    /// Lệnh đã bắt đầu chạy.
    Chay,
    /// Chạy xong, hoặc tin không sinh lệnh nào.
    Xong,
    /// Đang chạy dở lúc huba tắt — đã báo, không chạy lại.
    Do,
    /// Quá cũ nên không chạy — đã báo.
    Bo,
}

impl Buoc {
    pub fn as_str(self) -> &'static str {
        match self {
            Buoc::Nhan => "nhan",
            Buoc::Chay => "chay",
            Buoc::Xong => "xong",
            Buoc::Do => "do",
            Buoc::Bo => "bo",
        }
    }

    pub fn tu_chuoi(s: &str) -> Option<Buoc> {
        Some(match s {
            "nhan" => Buoc::Nhan,
            "chay" => Buoc::Chay,
            "xong" => Buoc::Xong,
            "do" => Buoc::Do,
            "bo" => Buoc::Bo,
            _ => return None,
        })
    }

    /// Những bước ĐƯỢC PHÉP chuyển sang bước này. Bước khép (`do` · `bo`) không
    /// bao giờ bị ghi đè: một tin đã báo "không chạy" mà sau đó lặng lẽ thành
    /// `xong` là sổ nói dối với chính người đã đọc lời báo.
    ///
    /// `xong → chay` được phép, cố ý: một update có thể sinh HAI lệnh (nút ⏎ xếp
    /// `Esc` rồi `/type`), và lệnh thứ hai có thể rơi vào một lô sau lô đã khép
    /// lệnh thứ nhất.
    fn tu(self) -> &'static [&'static str] {
        match self {
            Buoc::Nhan => &[],
            Buoc::Chay => &["nhan", "xong"],
            Buoc::Xong => &["nhan", "chay"],
            Buoc::Do => &["chay"],
            Buoc::Bo => &["nhan"],
        }
    }
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS tin_den (
  update_id INTEGER PRIMARY KEY,
  nhan_luc  TEXT NOT NULL,
  gui_luc   INTEGER,
  loai      TEXT NOT NULL,
  chu       TEXT NOT NULL,
  goc       TEXT NOT NULL,
  buoc      TEXT NOT NULL,
  doi_luc   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tin_den_buoc ON tin_den(buoc);
"#;

/// Mở sổ tin đến ở đúng tệp DB của huba, với lượt ghi BỀN qua mất điện.
///
/// Kết nối riêng chứ không mượn [`crate::db::Db`]: `fullfsync` là cờ của từng
/// kết nối, và chỉ sổ này cần nó — phần còn lại của huba ghi theo nhịp vòng.
pub fn mo(path: &Path) -> Result<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    let conn =
        Connection::open(path).with_context(|| format!("cannot open db {}", path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    // macOS: `fsync` trần chỉ đẩy tới bộ đệm của ổ; mất điện là mất. Ca phải đỡ
    // ở đây đúng là mất điện (máy hết pin 02/10).
    conn.pragma_update(None, "fullfsync", "ON")?;
    conn.execute_batch(SCHEMA)?;
    chi_chu_may_doc(path);
    Ok(conn)
}

/// Tệp DB (và `-wal`/`-shm`) chỉ CHỦ MÁY đọc được — `0600`, như `huba.env`.
///
/// 🔴 Đo 02/10 ngay sau lượt cài đầu: `data/huba.sqlite` nằm `0644`. Trước sổ
/// này nó chỉ giữ con trỏ và sổ sức khoẻ; nay nó giữ NGUYÊN VĂN chữ chủ máy gõ
/// suốt [`GIU_NGAY`] ngày, mà chữ gõ vào phiên có lúc là mã OTP. Luật 5 (không
/// giấu chữ với CHỦ MÁY) không đổi — đây là chặn người dùng KHÁC trên máy.
/// Hỏng thì nói ra, không chặn việc.
fn chi_chu_may_doc(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for duoi in ["", "-wal", "-shm"] {
            let p = PathBuf::from(format!("{}{duoi}", path.display()));
            let Ok(m) = std::fs::metadata(&p) else {
                continue;
            };
            if m.permissions().mode() & 0o777 == 0o600 {
                continue;
            }
            if let Err(e) = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)) {
                logging::warn(
                    "tin_den_chmod_failed",
                    json!({ "path": p.display().to_string(), "err": e.to_string() }),
                );
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Một tin đọc lại từ sổ.
#[derive(Debug, Clone)]
pub struct Tin {
    pub update_id: i64,
    pub nhan_luc: String,
    pub gui_luc: Option<i64>,
    pub loai: String,
    pub chu: String,
    pub goc: Value,
    pub buoc: Buoc,
}

/// `(loại, chữ cho người đọc, lúc gửi)` của một update.
///
/// `chu` là thứ người đọc sổ cần nhận ra tin của mình — nguyên văn câu gõ, dữ
/// liệu của nút, hay tên tệp. Bản gốc đầy đủ vẫn nằm ở cột `goc` để chạy lại.
pub fn tom_tat(u: &Value) -> (&'static str, String, Option<i64>) {
    let gui = crate::telegram::text_sent_at(u);
    if let Some(d) = u.pointer("/callback_query/data").and_then(Value::as_str) {
        return ("nut", d.to_string(), None);
    }
    let msg = u.get("message").or_else(|| u.get("edited_message"));
    if let Some(m) = msg {
        if let Some(t) = m.get("text").and_then(Value::as_str) {
            return ("chu", t.to_string(), gui);
        }
        let ten = [
            "document",
            "audio",
            "video",
            "voice",
            "animation",
            "video_note",
        ]
        .iter()
        .find_map(|k| m.get(*k))
        .map(|f| {
            f.get("file_name")
                .and_then(Value::as_str)
                .unwrap_or("(tệp)")
                .to_string()
        })
        .or_else(|| m.get("photo").map(|_| "(ảnh)".to_string()));
        if let Some(ten) = ten {
            let chu = match m.get("caption").and_then(Value::as_str) {
                Some(c) => format!("{ten} — {c}"),
                None => ten,
            };
            return ("tep", chu, gui);
        }
    }
    ("khac", String::new(), gui)
}

/// Ghi một update vừa nhận, bước `nhan`. `Ok(false)` = update này ĐÃ có trong
/// sổ: Telegram giao lại một tin mà lượt chạy-lại lúc khởi động đã lo (con dấu
/// `offset` chỉ được Telegram ghi nhận ở lượt `getUpdates` SAU, nên chết giữa
/// hai lượt thì tin vừa nằm trong sổ vừa được giao lại).
pub fn ghi_nhan(conn: &Connection, u: &Value) -> Result<bool> {
    let id = u
        .get("update_id")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("update không có update_id"))?;
    let (loai, chu, gui) = tom_tat(u);
    let now = logging::now_iso();
    let n = conn.execute(
        "INSERT OR IGNORE INTO tin_den (update_id, nhan_luc, gui_luc, loai, chu, goc, buoc, doi_luc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'nhan', ?2)",
        params![id, now, gui, loai, chu, u.to_string()],
    )?;
    Ok(n == 1)
}

/// Chuyển những tin này sang `buoc` — chỉ những dòng đang ở bước được phép
/// chuyển từ ([`Buoc::tu`]). Trả số dòng đã đổi.
pub fn danh_dau(conn: &Connection, ids: &[i64], buoc: Buoc) -> Result<usize> {
    let tu = buoc.tu();
    if ids.is_empty() || tu.is_empty() {
        return Ok(0);
    }
    let now = logging::now_iso();
    let mut n = 0;
    for id in ids {
        n += conn.execute(
            &format!(
                "UPDATE tin_den SET buoc = ?1, doi_luc = ?2 WHERE update_id = ?3 AND buoc IN ({})",
                tu.iter()
                    .map(|s| format!("'{s}'"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            params![buoc.as_str(), now, id],
        )?;
    }
    Ok(n)
}

/// Như [`danh_dau`] nhưng tự mở sổ, và hỏng thì NÓI RA rồi đi tiếp: sổ hỏng
/// không được chặn lệnh của chủ máy — nó chỉ làm mất lưới dự phòng, và việc mất
/// ấy phải có một dòng `error` (hiện trên `/doctor`).
pub fn danh_dau_tai(db: &Path, ids: &[i64], buoc: Buoc) {
    if ids.is_empty() {
        return;
    }
    if let Err(e) = mo(db).and_then(|c| danh_dau(&c, ids, buoc)) {
        logging::error(
            "tin_den_danh_dau_failed",
            json!({ "ids": ids, "buoc": buoc.as_str(), "err": e.to_string() }),
        );
    }
}

/// Mọi tin chưa khép (`nhan` · `chay`), cũ trước mới sau.
pub fn chua_xong(conn: &Connection) -> Result<Vec<Tin>> {
    let mut st = conn.prepare(
        "SELECT update_id, nhan_luc, gui_luc, loai, chu, goc, buoc FROM tin_den
         WHERE buoc IN ('nhan', 'chay') ORDER BY update_id",
    )?;
    let rows = st.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<i64>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (update_id, nhan_luc, gui_luc, loai, chu, goc, buoc) = row?;
        // Bản gốc hỏng / bước lạ thì KHÔNG bỏ qua im: đó là một tin của chủ
        // máy mà sổ không còn đọc được.
        let goc = match serde_json::from_str(&goc) {
            Ok(v) => v,
            Err(e) => {
                logging::error(
                    "tin_den_goc_unreadable",
                    json!({ "update_id": update_id, "err": e.to_string() }),
                );
                Value::Null
            }
        };
        let Some(buoc) = Buoc::tu_chuoi(&buoc) else {
            logging::error(
                "tin_den_buoc_unknown",
                json!({ "update_id": update_id, "buoc": buoc }),
            );
            continue;
        };
        out.push(Tin {
            update_id,
            nhan_luc,
            gui_luc,
            loai,
            chu,
            goc,
            buoc,
        });
    }
    Ok(out)
}

/// Dọn tin đã khép cũ hơn [`GIU_NGAY`] ngày. Tin chưa khép thì không bao giờ
/// dọn — nó là việc chưa ai trả lời.
pub fn don_cu(conn: &Connection, now: chrono::DateTime<chrono::Utc>) -> Result<usize> {
    let moc = (now - chrono::Duration::days(GIU_NGAY))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    Ok(conn.execute(
        "DELETE FROM tin_den WHERE buoc IN ('xong', 'do', 'bo') AND nhan_luc < ?1",
        params![moc],
    )?)
}

/// Tuổi của một tin tính từ lúc HUBA nhận (giây). Mốc không đọc được ⟹ `None`.
pub fn tuoi_nhan(t: &Tin, now: chrono::DateTime<chrono::Utc>) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(&t.nhan_luc)
        .ok()
        .map(|d| (now - d.with_timezone(&chrono::Utc)).num_seconds())
}

/// Lượt khởi động lại phân loại tin chưa khép ra sao.
#[derive(Debug, Default)]
pub struct KhoiPhuc {
    /// Chưa bắt đầu chạy, còn đủ mới ⟹ giao lại cho luồng thợ.
    pub chay_lai: Vec<Tin>,
    /// Đang chạy dở lúc huba tắt ⟹ KHÔNG chạy lại, báo.
    pub do_dang: Vec<Tin>,
    /// Chưa bắt đầu chạy nhưng đã quá cũ ⟹ không chạy, báo.
    pub qua_cu: Vec<Tin>,
}

/// Chia tin chưa khép thành ba nhóm. Thuần — không ghi sổ, không gửi gì.
///
/// "Quá cũ" đo từ lúc HUBA nhận, không từ lúc gửi: nút bấm không mang mốc gửi
/// (`callback_query` không có trường nào cho lúc bấm), còn tin chữ thì cổng
/// tuổi theo mốc gửi vẫn chạy lại ở `handle_update` khi nó được giao lại.
pub fn phan_loai(tin: Vec<Tin>, now: chrono::DateTime<chrono::Utc>, too_old_sec: i64) -> KhoiPhuc {
    let mut k = KhoiPhuc::default();
    for t in tin {
        match t.buoc {
            Buoc::Chay => k.do_dang.push(t),
            // Bản gốc không đọc được thì không có gì để chạy lại; để nó ở `nhan`
            // là lượt khởi động nào cũng gặp lại nó. Khép `bo` và báo bằng `chu`.
            Buoc::Nhan if t.goc.is_null() => k.qua_cu.push(t),
            Buoc::Nhan => match tuoi_nhan(&t, now) {
                Some(s) if s <= too_old_sec => k.chay_lai.push(t),
                // Mốc hỏng thì coi là cũ: chạy một lệnh không biết tuổi là
                // chạy mù; báo ra thì chủ máy còn tự gửi lại được.
                _ => k.qua_cu.push(t),
            },
            _ => {}
        }
    }
    k
}

fn trich(chu: &str) -> String {
    let c = chu.trim();
    if c.is_empty() {
        "(không có chữ)".into()
    } else {
        crate::exec::truncate(c, 200)
    }
}

/// Câu báo của lượt khởi động lại. `None` khi không có gì để nói — huba chỉ nói
/// khi có thay đổi (luật 11), và một lượt khởi động lại sạch không phải tin.
pub fn cau_bao_khoi_dong(k: &KhoiPhuc, too_old_sec: i64) -> Option<String> {
    if k.chay_lai.is_empty() && k.do_dang.is_empty() && k.qua_cu.is_empty() {
        return None;
    }
    let mut s = String::from("⚠ huba vừa khởi động lại, còn tin của Hà chưa xong:");
    if !k.do_dang.is_empty() {
        s.push_str(&format!(
            "\n\n• {} lệnh đang chạy DỞ lúc huba tắt — KHÔNG chạy lại (chạy lại có thể gõ hai lần vào phiên). Xem lại rồi gửi lại nếu cần:",
            k.do_dang.len()
        ));
        for t in &k.do_dang {
            s.push_str(&format!("\n  «{}»", trich(&t.chu)));
        }
    }
    if !k.qua_cu.is_empty() {
        s.push_str(&format!(
            "\n\n• {} tin chưa kịp chạy, nay đã quá {} phút — KHÔNG chạy. Gửi lại nếu vẫn cần:",
            k.qua_cu.len(),
            too_old_sec / 60
        ));
        for t in &k.qua_cu {
            s.push_str(&format!("\n  «{}»", trich(&t.chu)));
        }
    }
    if !k.chay_lai.is_empty() {
        s.push_str(&format!(
            "\n\n• {} tin chưa kịp chạy — chạy lại NGAY bây giờ:",
            k.chay_lai.len()
        ));
        for t in &k.chay_lai {
            s.push_str(&format!("\n  «{}»", trich(&t.chu)));
        }
    }
    s.push_str(&format!(
        "\n\n(Mọi tin đến được giữ {GIU_NGAY} ngày trong data/huba.sqlite, bảng tin_den.)"
    ));
    Some(s)
}

/// Phần KHÔNG MẠNG của lượt khởi động: dọn tin cũ, phân loại tin chưa khép, KHÉP
/// SỔ cho cái không chạy lại (`do` · `bo`), rồi trả `(câu báo, các update phải
/// giao lại cho thợ)`. Bên gọi gửi câu báo và giao update.
///
/// Khép sổ TRƯỚC khi nói và trước khi chạy lại: huba chết thêm lần nữa ngay
/// sau đây thì lượt sau không báo lại cùng một tin.
pub fn khoi_phuc(
    conn: &Connection,
    now: chrono::DateTime<chrono::Utc>,
    too_old_sec: i64,
) -> (Option<String>, Vec<Value>) {
    match don_cu(conn, now) {
        Ok(n) if n > 0 => logging::info("tin_den_pruned", json!({ "rows": n })),
        Ok(_) => {}
        Err(e) => logging::error("tin_den_prune_failed", json!({ "err": e.to_string() })),
    }
    let tin = match chua_xong(conn) {
        Ok(t) => t,
        Err(e) => {
            logging::error(
                "tin_den_read_failed",
                json!({ "err": e.to_string(),
                        "effect": "KHÔNG chạy lại được tin lần trước bỏ dở" }),
            );
            return (None, Vec::new());
        }
    };
    let k = phan_loai(tin, now, too_old_sec);
    let ids = |v: &[Tin]| v.iter().map(|t| t.update_id).collect::<Vec<_>>();
    logging::info(
        "tin_den_recovered",
        json!({ "chay_lai": ids(&k.chay_lai), "do_dang": ids(&k.do_dang), "qua_cu": ids(&k.qua_cu) }),
    );
    for (v, b) in [(&k.do_dang, Buoc::Do), (&k.qua_cu, Buoc::Bo)] {
        if let Err(e) = danh_dau(conn, &ids(v), b) {
            logging::error(
                "tin_den_danh_dau_failed",
                json!({ "ids": ids(v), "buoc": b.as_str(), "err": e.to_string() }),
            );
        }
    }
    let cau = cau_bao_khoi_dong(&k, too_old_sec);
    (cau, k.chay_lai.into_iter().map(|t| t.goc).collect())
}

// ─── lô lệnh đang chạy ───────────────────────────────────────────────────

/// Lô tin mà `execute_telegram_commands` đang chạy: `(tệp DB, các update_id)`.
///
/// Một ô là đủ: lô Telegram chạy dưới `pipeline::CMD_LOCK`, nên không bao giờ có
/// hai lô cùng lúc. Có ô này là vì [`khep_lo_truoc_khi_tu_khoi_dong`].
static LO_DANG_CHAY: Mutex<Option<(PathBuf, Vec<i64>)>> = Mutex::new(None);

/// Lô bắt đầu: đánh dấu `chay` rồi nhớ lô.
pub fn bat_dau_lo(db: &Path, ids: Vec<i64>) {
    if ids.is_empty() {
        return;
    }
    danh_dau_tai(db, &ids, Buoc::Chay);
    *LO_DANG_CHAY.lock().unwrap_or_else(|e| e.into_inner()) = Some((db.to_path_buf(), ids));
}

/// Lô chạy hết: đánh dấu `xong` rồi quên lô.
pub fn ket_thuc_lo() {
    let lo = LO_DANG_CHAY
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some((db, ids)) = lo {
        danh_dau_tai(&db, &ids, Buoc::Xong);
    }
}

/// Khép lô đang chạy NGAY TRƯỚC khi huba tự khởi động lại chính nó.
///
/// 🔴 `/upgrade` (và ▶️ trên dòng `install_update.sh`) gọi `launchctl kickstart
/// -k` ngay TRONG lô — tiến trình chết trước khi lô kịp khép, nên không có cú
/// khép này thì mọi lượt nâng cấp để lại một tin `chay` và lượt khởi động sau
/// báo oan *"lệnh đang chạy dở"*. Cái chết ấy chính là KẾT QUẢ của lệnh, không
/// phải một lần gián đoạn. Cùng lý lẽ với `so_viec_khep` ngay trước cú khởi
/// động lại ở nhánh `is_self_rebuild`.
pub fn khep_lo_truoc_khi_tu_khoi_dong() {
    let lo = LO_DANG_CHAY
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some((db, ids)) = lo {
        logging::info(
            "tin_den_khep_truoc_khi_khoi_dong_lai",
            json!({ "ids": ids }),
        );
        danh_dau_tai(&db, &ids, Buoc::Xong);
    }
}
