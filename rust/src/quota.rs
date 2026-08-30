//! Tài khoản nào CÒN NHIỀU hạn mức nhất — đọc từ sổ của chính CLI, không spawn.
//!
//! 🔴 Hà 2026-08-30: *"kiểm tra lại cách quản lý hết tokens tài khoản cho triệt
//! để đi, hiện tại mở phiên mới ở acc khác chưa kiểm soát được acc đó có đang còn
//! nhiều tokens nhất không"*.
//!
//! Anh đúng, và chỗ hỏng đo được ngay lúc anh nói. Luật cũ trong
//! [`crate::watch::suggest_account`] là *"tên đầu tiên trong `huba.config.json`
//! mà không phải cái vừa chết và không đang thấy một phiên bị chặn"* — tức thứ
//! tự CẤU HÌNH, không phải thứ tự CÒN CHỖ. Đọc trên máy này đúng lúc ấy:
//! **acc1 92% tuần · acc2 22% · acc3 100%**. Luật cũ trả `acc1`.
//!
//! Và nó còn mù một cách tệ hơn: cửa duy nhất nó biết về "đang bị chặn" là
//! `LiveSession.limited`, thứ chỉ tồn tại khi tài khoản ấy ĐANG có một phiên
//! đứng đó với dòng hạn mức trên màn. Đóng bốn cửa sổ acc3 đi thì acc3 lập tức
//! đọc lên như một tài khoản rảnh — trong khi nó bị chặn cứng tới 1/9.
//!
//! # Nguồn: `<config_dir>/.claude.json`, khoá `cachedUsageUtilization`
//!
//! Chính CLI ghi nó mỗi lượt nó nói chuyện với API. Đo trên máy này 30/08 (ba
//! tệp, ba tài khoản, đọc bằng `python3 -c json.load`):
//!
//! ```text
//! ~/.claude.json          seven_day 92%  resets 2026-08-30T09:00Z  fetched 08-28 23:47
//! ~/.claude-acc2/.claude.json  22%  resets 2026-09-02T10:59Z  fetched 08-30 22:32
//! ~/.claude-acc3/.claude.json 100%  resets 2026-09-01T05:59Z  fetched 08-30 22:33
//! ```
//!
//! `resets_at` của acc3 = 1/9 05:59Z = **1/9 13:00 giờ Sài Gòn** — đúng nguyên
//! văn dòng `keys::session_limit_on_screen` cào được từ bốn cửa sổ sáng hôm ấy.
//! Tệp và màn khớp nhau; tệp chỉ hơn ở chỗ **không cần một cửa sổ nào đang mở**.
//!
//! Vì sao KHÔNG đi đường `claude -p "/usage"`: nó treo tới trần 60 giây, 0 byte,
//! chưa tìm ra thủ phạm (`PLAN.md`, mục còn nợ; `usage_probe_unparsed` lần cuối
//! 14/08). Đọc tệp tốn ~0 giây, không tốn một lượt quota nào, và không có gì để
//! mà treo.
//!
//! # Cái nó KHÔNG đo được, ghi ra để đừng ai tưởng là kín
//!
//! * **Số token tuyệt đối.** `limit_dollars` · `used_dollars` ·
//!   `remaining_dollars` đều `null` trên cả ba tài khoản. Chỉ có phần trăm.
//! * **Số MỚI theo yêu cầu.** Tệp chỉ đổi khi chính CLI của tài khoản ấy chạy.
//!   Muốn ép nó tươi lại thì phải chạy `claude` dưới tài khoản ấy — tức về đúng
//!   phép dò đang treo.
//! * **Việc tiêu hạn mức ở nơi khác** (claude.ai trên trình duyệt) không đi qua
//!   tệp này.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::logging;

/// Bản đọc hạn mức của MỘT tài khoản.
///
/// Mọi trường đều `Option`, có chủ ý: **"không đọc được" là một trạng thái
/// riêng**, không phải 0% (luật 13②). Một tài khoản đọc ra `None` mà bị xếp
/// ngang hàng với một tài khoản đo được 0% là đúng cái lỗi khiến luật cũ chọn
/// nhầm — chỉ khác chiều.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Quota {
    pub account: String,
    /// `seven_day.utilization` — phần trăm hạn mức TUẦN đã dùng.
    pub week_pct: Option<i64>,
    pub week_resets_at: Option<String>,
    /// `five_hour.utilization` — cửa sổ 5 tiếng.
    pub hour5_pct: Option<i64>,
    pub hour5_resets_at: Option<String>,
    /// Lúc CLI ghi con số này (ms từ epoch).
    pub fetched_at_ms: Option<i64>,
    /// Vì sao không đọc được — `None` là đọc được. Chuỗi này đi ra tin nhắn.
    pub why_unknown: Option<String>,
}

/// Xếp hạng một tài khoản để CHỌN. Thứ tự của `derive(Ord)` chính là thứ tự ưu tiên.
///
/// 🔴 Ba bậc chứ không phải hai, và bậc giữa là bậc đắt nhất phải giữ:
///
/// * [`Rank::Free`] — đo được và còn chỗ. Số là phần trăm ĐÃ DÙNG của cửa sổ
///   chật nhất, nên nhỏ hơn là rộng cửa hơn.
/// * [`Rank::Unknown`] — **không đo được**. Nó đứng SAU mọi tài khoản đo được
///   còn chỗ (đừng đoán bừa khi đã có số thật trong tay) và TRƯỚC tài khoản đo
///   được là đã kịch trần (một ẩn số vẫn hơn một cánh cửa đã đóng).
/// * [`Rank::Full`] — đo được, đã kịch trần. Không bao giờ chọn khi còn đường khác.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    Free(i64),
    Unknown,
    Full,
}

impl Rank {
    /// Câu ngắn cho người đọc — đi thẳng vào tin Telegram, nên không có dấu ngoặc.
    pub fn say(&self) -> String {
        match self {
            Rank::Free(p) => format!("đã dùng {p}%"),
            Rank::Unknown => "chưa đo được".to_string(),
            Rank::Full => "ĐÃ KỊCH TRẦN".to_string(),
        }
    }
}

impl Quota {
    /// Một dòng cho người đọc — số, và TUỔI của số ấy.
    ///
    /// 🔴 Tuổi không phải phần trang trí. Bản đọc của acc1 trên máy này già hai
    /// ngày, và một con số già hai ngày đọc lên y hệt một con số vừa đo xong nếu
    /// không ai nói ra. `/accounts` là chỗ DUY NHẤT chủ máy soi lại được luật
    /// chọn tài khoản, nên nó phải in cả cái để soi.
    pub fn say(&self, now_ms: i64) -> String {
        if let Some(why) = &self.why_unknown {
            return format!("chưa đo được — {why}");
        }
        let mut p: Vec<String> = Vec::new();
        if let Some(w) = self.week_pct {
            p.push(format!("tuần {w}%"));
        }
        if let Some(h) = self.hour5_pct {
            p.push(format!("5 tiếng {h}%"));
        }
        if p.is_empty() {
            p.push("sổ không có con số nào".to_string());
        }
        p.push(format!("hạng: {}", rank(self, now_ms).say()));
        if let Some(t) = self.fetched_at_ms {
            let phut = (now_ms - t).max(0) / 60_000;
            p.push(match phut {
                0 => "đo vừa xong".to_string(),
                1..=90 => format!("đo {phut} phút trước"),
                _ => format!("đo {} tiếng trước", phut / 60),
            });
        }
        p.join(" · ")
    }
}

/// Một tài khoản kèm hạng của nó — thứ [`crate::watch::suggest_account`] cần.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ranked {
    pub name: String,
    pub rank: Rank,
}

/// Đường tới sổ của một tài khoản: `<config_dir>/.claude.json`.
///
/// ⚠ Tài khoản MẶC ĐỊNH không phải `~/.claude/.claude.json` mà là
/// **`~/.claude.json`** — ở gốc `$HOME`. Đo trên máy này: tệp gốc có 84 khoá kèm
/// `oauthAccount` và `cachedUsageUtilization`, còn `~/.claude/.claude.json` có 7
/// khoá, không có khoá nào trong hai khoá ấy, và đứng im từ 6/8. Nhầm chỗ thì
/// hàm này đọc ra `None` vĩnh viễn — một phép đo luôn im lặng, đúng dạng hỏng
/// khó thấy nhất.
pub fn book_path(dir: Option<&Path>) -> PathBuf {
    match dir {
        Some(d) => d.join(".claude.json"),
        None => match std::env::var_os("HOME") {
            Some(h) => PathBuf::from(h).join(".claude.json"),
            None => PathBuf::from(".claude.json"),
        },
    }
}

/// Đọc hạn mức của một tài khoản. Hỏng ở bất kỳ bậc nào cũng KÊU, không im.
pub fn read(account: &str, dir: Option<&Path>) -> Quota {
    let path = book_path(dir);
    let trong = |why: String| Quota {
        account: account.to_string(),
        week_pct: None,
        week_resets_at: None,
        hour5_pct: None,
        hour5_resets_at: None,
        fetched_at_ms: None,
        why_unknown: Some(why),
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            logging::warn(
                "quota_book_unreadable",
                json!({ "account": account, "path": path.display().to_string(),
                        "err": e.to_string() }),
            );
            return trong(format!("không đọc được {}", path.display()));
        }
    };
    let doc: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            logging::warn(
                "quota_book_unparsed",
                json!({ "account": account, "path": path.display().to_string(),
                        "err": e.to_string() }),
            );
            return trong("sổ tài khoản không đọc ra JSON".to_string());
        }
    };
    let Some(u) = doc.get("cachedUsageUtilization") else {
        // KHÔNG phải lỗi: một tài khoản chưa gọi API lần nào thì chưa có khoá này.
        logging::info(
            "quota_not_cached_yet",
            json!({ "account": account, "path": path.display().to_string() }),
        );
        return trong("CLI chưa ghi số hạn mức nào cho tài khoản này".to_string());
    };
    let buckets = u.get("utilization");
    let doc_pct = |ten: &str| -> (Option<i64>, Option<String>) {
        let b = buckets.and_then(|b| b.get(ten));
        (
            b.and_then(|b| b.get("utilization")).and_then(Value::as_i64),
            b.and_then(|b| b.get("resets_at"))
                .and_then(Value::as_str)
                .map(str::to_string),
        )
    };
    let (week_pct, week_resets_at) = doc_pct("seven_day");
    let (hour5_pct, hour5_resets_at) = doc_pct("five_hour");
    Quota {
        account: account.to_string(),
        week_pct,
        week_resets_at,
        hour5_pct,
        hour5_resets_at,
        fetched_at_ms: u.get("fetchedAtMs").and_then(Value::as_i64),
        why_unknown: None,
    }
}

/// Một cửa sổ hạn mức đọc ra bao nhiêu — hay không đọc được.
///
/// 🔴 Chỗ này là cả cái ruột của phép đo, và nó có một bẫy đã suýt lọt: bản đọc
/// **CŨ HƠN mốc mở lại của chính nó** thì con số ấy không còn nói gì. Đo trên
/// máy này 30/08: acc1 ghi `92%` lúc 28/08 23:47, `resets_at` là 30/08 09:00Z —
/// tức đồng hồ đã quay vòng trước cả lúc đọc câu này. Lấy `92` mà xếp hạng là
/// loại một tài khoản có thể đang rỗng.
///
/// Và đúng chiều ngược cũng phải giữ: **`None` KHÔNG được đọc thành 0**. Hai câu
/// *"đo được 0%"* và *"không đo được"* dẫn tới hai hành động khác nhau ở phía
/// chủ máy, nên chúng không được nhìn giống nhau (luật 13②).
fn cua_so(pct: Option<i64>, resets_at: Option<&str>, now_ms: i64) -> Option<i64> {
    let pct = pct?;
    match resets_at {
        // Không có mốc mở lại: cửa sổ này chưa chạy. Chỉ tin khi số là 0 —
        // một con số >0 mà không có đồng hồ đi kèm thì huba không biết nó thuộc
        // về chu kỳ nào, và đoán ở đây là đoán về đúng thứ đang cần chắc chắn.
        None => (pct == 0).then_some(0),
        Some(t) => match chrono::DateTime::parse_from_rfc3339(t) {
            Ok(khi) if khi.timestamp_millis() <= now_ms => None,
            Ok(_) => Some(pct),
            // Mốc đọc không ra thì coi như không đo được — fail-closed.
            Err(_) => None,
        },
    }
}

/// Xếp hạng một bản đọc tại thời điểm `now_ms`.
pub fn rank(q: &Quota, now_ms: i64) -> Rank {
    let w = cua_so(q.week_pct, q.week_resets_at.as_deref(), now_ms);
    let h = cua_so(q.hour5_pct, q.hour5_resets_at.as_deref(), now_ms);
    // KỊCH TRẦN thắng mọi thứ, kể cả một cửa sổ khác không đọc được: một cánh
    // cửa đã đóng thì phần còn lại không cứu được.
    if w.is_some_and(|p| p >= 100) || h.is_some_and(|p| p >= 100) {
        return Rank::Full;
    }
    match (w, h) {
        // Cửa sổ CHẬT NHẤT quyết định — nó là cái sẽ chặn trước.
        (Some(a), Some(b)) => Rank::Free(a.max(b)),
        _ => Rank::Unknown,
    }
}

/// Hạng của mọi tài khoản trong cấu hình, theo đúng thứ tự đã khai.
///
/// Thứ tự giữ nguyên vì nó là cách phá hoà: hai tài khoản cùng hạng thì lấy cái
/// chủ máy xếp trước, chứ không lấy cái tình cờ nằm trước trong một `HashMap`.
pub fn rank_all(cfg: &crate::config::Config, now_ms: i64) -> Vec<Ranked> {
    cfg.claude_accounts_or_ambient()
        .iter()
        .map(|a| {
            let dir = a
                .config_dir
                .as_deref()
                .filter(|d| !d.is_empty())
                .map(|d| crate::config::expand_home(Path::new(d)));
            let q = read(&a.name, dir.as_deref());
            let r = rank(&q, now_ms);
            logging::info(
                "quota_read",
                json!({ "account": a.name, "week_pct": q.week_pct,
                        "week_resets_at": q.week_resets_at, "hour5_pct": q.hour5_pct,
                        "fetched_at_ms": q.fetched_at_ms, "why_unknown": q.why_unknown,
                        "rank": r.say() }),
            );
            Ranked {
                name: a.name.clone(),
                rank: r,
            }
        })
        .collect()
}

/// Giờ UTC hiện tại theo ms — tách ra để bài kiểm bơm được một mốc cố định.
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(week: Option<i64>, week_r: Option<&str>, h5: Option<i64>, h5_r: Option<&str>) -> Quota {
        Quota {
            account: "acc".into(),
            week_pct: week,
            week_resets_at: week_r.map(str::to_string),
            hour5_pct: h5,
            hour5_resets_at: h5_r.map(str::to_string),
            fetched_at_ms: None,
            why_unknown: None,
        }
    }

    /// Mốc giả: 2026-08-30T15:00:00Z.
    const NOW: i64 = 1_788_102_000_000;

    #[test]
    fn cua_so_chat_nhat_la_cua_quyet_dinh() {
        // Tuần rộng (10%) mà 5 tiếng đã 80% ⟹ cái chặn trước là 80.
        let r = rank(
            &q(
                Some(10),
                Some("2026-09-02T10:59:59+00:00"),
                Some(80),
                Some("2026-08-30T18:00:00+00:00"),
            ),
            NOW,
        );
        assert_eq!(r, Rank::Free(80));
    }

    /// 🔴 Đúng ca acc1 trên máy này 30/08: `92%` ghi lúc 28/08, mà đồng hồ tuần
    /// mở lại lúc 30/08 09:00Z — trước cả lúc đọc. Con số ấy KHÔNG còn nói gì.
    #[test]
    fn ban_doc_cu_hon_moc_mo_lai_thi_khong_con_la_mot_phep_do() {
        let r = rank(
            &q(Some(92), Some("2026-08-30T09:00:00+00:00"), Some(0), None),
            NOW,
        );
        assert_eq!(
            r,
            Rank::Unknown,
            "đồng hồ đã quay vòng ⟹ 92% là số của chu kỳ trước"
        );
    }

    #[test]
    fn khong_do_duoc_khong_phai_la_khong_phan_tram() {
        assert_eq!(rank(&q(None, None, None, None), NOW), Rank::Unknown);
        // Và nó phải đứng SAU một tài khoản đo được, dù tài khoản ấy đã dùng 99%.
        assert!(
            Rank::Free(99) < Rank::Unknown,
            "có số thật thì đừng nhường chỗ cho một ẩn số"
        );
        // …nhưng TRƯỚC một tài khoản chắc chắn đã kịch trần.
        assert!(Rank::Unknown < Rank::Full);
    }

    /// Kịch trần thắng cả một cửa sổ không đọc được: cửa đã đóng thì thôi.
    #[test]
    fn kich_tran_thang_moi_thu() {
        let r = rank(
            &q(Some(100), Some("2026-09-01T05:59:59+00:00"), None, None),
            NOW,
        );
        assert_eq!(r, Rank::Full);
    }

    /// Cửa sổ 5 tiếng chưa chạy thì CLI ghi `0` + `resets_at: null` — đo được là
    /// 0, không phải "không đo được". Đây là hình dạng thật của acc2/acc3 trên
    /// máy này, nên đọc sai chỗ này là làm cả hai tài khoản rảnh thành ẩn số.
    #[test]
    fn cua_so_chua_chay_thi_0_la_mot_con_so_that() {
        let r = rank(
            &q(Some(22), Some("2026-09-02T10:59:59+00:00"), Some(0), None),
            NOW,
        );
        assert_eq!(r, Rank::Free(22));
    }

    /// Số >0 mà không có đồng hồ đi kèm: không biết nó thuộc chu kỳ nào ⟹ ẩn số.
    #[test]
    fn so_khac_0_ma_khong_co_dong_ho_thi_khong_tin() {
        assert_eq!(rank(&q(Some(40), None, Some(0), None), NOW), Rank::Unknown);
    }

    /// Thứ tự ba bậc là thứ tự CHỌN — khoá lại bằng một phép sắp xếp thật, vì
    /// `derive(Ord)` im lặng đổi nghĩa nếu ai đó xếp lại mấy nhánh enum.
    #[test]
    fn thu_tu_ba_bac_la_thu_tu_chon() {
        let mut v = vec![Rank::Full, Rank::Unknown, Rank::Free(90), Rank::Free(5)];
        v.sort();
        assert_eq!(
            v,
            vec![Rank::Free(5), Rank::Free(90), Rank::Unknown, Rank::Full]
        );
    }
}
