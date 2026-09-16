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
//! Vì sao tệp là lớp NỀN: đọc nó tốn ~0 giây, không spawn, không tốn một lượt
//! quota nào, và không có gì để mà treo.
//!
//! ⚠ **Dòng này trước viết rằng `claude -p "/usage"` "treo tới trần 60 giây, 0
//! byte" — ĐO LẠI 2026-09-15 thì KHÔNG CÒN ĐÚNG**, và cái sai ấy đắt hơn một
//! dòng chú thích lạc hậu: nó là lý do người đọc sẽ tránh phép dò sống, đúng thứ
//! [`overlay_live`] cần để chữa con số chết.
//!
//! ```text
//! claude -p /usage --output-format json   (acc3)
//! exit 0 · 4,2 giây · stdout 1793 B · num_turns 0 · total_cost_usd 0
//! ```
//!
//! Khớp với log: `usage_probe_unparsed` = 0 và `usage_probe_failed` = 0 trong
//! phần nhật ký quét được (60 MB cuối của 139 MB — **mẫu số chưa phủ hết**, nên
//! đọc là "không thấy lần hỏng nào", không phải "chưa hỏng lần nào").
//!
//! Nên hai lớp ấy **không thay nhau, chúng bổ cho nhau**: tệp trả lời tức thì và
//! luôn có mặt, phép dò trả lời ĐÚNG LÚC NÀY. Xem [`kich_tran_can_xac_nhan`] —
//! chỗ quyết định khi nào phải hỏi tới lớp thứ hai.
//!
//! # Cái nó KHÔNG đo được, ghi ra để đừng ai tưởng là kín
//!
//! * **Số token tuyệt đối.** `limit_dollars` · `used_dollars` ·
//!   `remaining_dollars` đều `null` trên cả ba tài khoản. Chỉ có phần trăm.
//! * **Số MỚI theo yêu cầu — và `claude -p` KHÔNG ép được.** Dòng này trước
//!   viết *"muốn ép nó tươi lại thì phải chạy `claude` dưới tài khoản ấy"*. Đo
//!   hai chiều 2026-09-15 thì câu ấy sai đúng ở chỗ đắt nhất: một lượt
//!   `claude -p` **chạy trót lọt** — exit 0, có câu trả lời, 38–41 giây, KHÔNG
//!   treo — và nó **CÓ ghi** `.claude.json` (`mtime` đổi ở cả hai vế), nhưng
//!   `cachedUsageUtilization` thì không nhúc nhích:
//!
//!   ```text
//!   acc3 (đang có số 3%)  exit 0 · mtime ĐỔI · fetchedAtMs 1789461853704 → y nguyên
//!   acc5 (chưa có số)     exit 0 · mtime ĐỔI · utilization null        → vẫn null
//!   ```
//!
//!   Vế acc3 là vế phải có: một tài khoản ĐÃ có số mà vẫn không tươi lại thì
//!   loại được giả thuyết *"tại acc5 mới quá"*. Và `mtime` đổi loại nốt giả
//!   thuyết *"CLI không ghi gì"* — nó ghi, chỉ không ghi khoá này. ⟹ Khoá ấy do
//!   **phiên tương tác** ghi, không phải mọi lượt CLI nói chuyện với API.
//!
//!   Hệ quả, và đây là chỗ đừng ai đi lại: `Unknown` của một tài khoản vừa dựng
//!   **không có đường rẻ nào chữa**. Không bắn `-p` để hâm số được — phải có
//!   người gõ một câu trong cửa sổ thật. Ca thật: acc5 thêm 15/09, onboard xong
//!   mà sổ vẫn rỗng, nên nó ngồi ở `Unknown` — hạng đứng TRƯỚC `Full`, tức vẫn
//!   được gợi ý khi các tài khoản khác kịch trần, và lần này cửa sổ mở ra chạy
//!   được (khác hẳn ca acc4 12/09, xem [`account_not_ready`]).
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
    /// Tài khoản **chưa mở cửa sổ được**, kèm lý do. `None` = dùng được.
    ///
    /// 🔴 Khác hẳn `why_unknown`, và trộn hai thứ này là con bug ngày 12/09:
    /// `why_unknown` nói *"chưa biết còn bao nhiêu hạn mức"* — một tài khoản
    /// vẫn chạy được, chỉ là sổ chưa có số. Trường này nói *"mở cửa sổ ra là nó
    /// đứng ngay"*, và không đồng hồ nào chữa được, chỉ có người.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chua_dung_duoc: Option<String>,
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
/// * [`Rank::Dead`] — **tổ chức đã khoá**. Khác `Full` ở chỗ đắt nhất: `Full`
///   chờ một cái đồng hồ rồi tự mở, cái này chờ bao lâu cũng vô ích. Không đọc
///   ra từ sổ `.claude.json` bao giờ — nguồn của nó là MÀN, và vì màn chỉ nói
///   khi còn một cửa sổ đang mở nên nó phải được GHI LẠI; xem
///   [`crate::watch::reconcile_dead_book`].
/// * [`Rank::NotReady`] — **chưa dùng được**: chưa đăng nhập, hoặc đăng nhập
///   rồi mà lượt chạy đầu chưa xong (`claude` còn hỏi giao diện). Đứng cạnh
///   `Dead` chứ không cạnh `Unknown`, và đó là cả bản vá 12/09 — xem
///   [`account_not_ready`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    Free(i64),
    Unknown,
    Full,
    NotReady,
    Dead,
}

impl Rank {
    /// Câu ngắn cho người đọc — đi thẳng vào tin Telegram, nên không có dấu ngoặc.
    pub fn say(&self) -> String {
        match self {
            Rank::Free(p) => format!("đã dùng {p}%"),
            Rank::Unknown => "chưa đo được".to_string(),
            Rank::Full => "ĐÃ KỊCH TRẦN".to_string(),
            Rank::NotReady => "CHƯA DÙNG ĐƯỢC".to_string(),
            Rank::Dead => "TỔ CHỨC ĐÃ KHOÁ".to_string(),
        }
    }
}

/// Hạng từ phép dò SỐNG (`runtime::usage_cached`, `/usage` vừa chạy thật) —
/// không cần `resets_at` như [`cua_so`]: phép đo vừa xảy ra đúng lúc này nên
/// không có tuổi để mà cũ.
///
/// `None` = phép dò chưa có số (đang đo lần đầu, hết giờ, hoặc không đọc được
/// câu trả lời) — gọi nơi khác lùi về tệp (`rank`).
pub fn rank_from_live(session_pct: Option<i64>, week_pct: Option<i64>) -> Option<Rank> {
    let p = session_pct.into_iter().chain(week_pct).max()?;
    Some(if p >= 100 { Rank::Full } else { Rank::Free(p) })
}

/// Đè hạng bằng phép dò SỐNG khi có số — tệp chỉ còn dùng lúc chưa đo được.
///
/// Hà 2026-09-07, sau vụ dồn 4 phiên `acc2 → acc1` rồi chính acc1 hết hạn mức
/// một tiếng sau: *"chạy luôn lệnh /usage có hơn không"*. Đúng — huba đã có
/// phép dò này từ 10/08 (`runtime::usage_cached`, cache 5 phút, chạy nền không
/// chặn vòng lặp), nhưng [`crate::watch::suggest_account`] chưa từng đọc nó,
/// chỉ đọc tệp `.claude.json` qua [`rank_all`] — tệp CHỈ đổi khi chính CLI của
/// tài khoản ấy chạm mạng, nên một tài khoản vừa bị khoá mà không phiên nào
/// của nó gọi API tiếp thì tệp đứng yên ở con số CŨ vô thời hạn, đọc lên như
/// còn rộng. Phép dò sống thì hỏi thẳng tài khoản đó, không cần đợi ai.
///
/// `usage_accounts` là khối `"accounts"` trong JSON của `usage_cached` — một
/// object tên tài khoản → `{"session_pct":…, "week_pct":…}` hoặc `{"err":…}`.
pub fn overlay_live(ranked: Vec<Ranked>, usage_accounts: &Value) -> Vec<Ranked> {
    ranked
        .into_iter()
        .map(|mut r| {
            let row = usage_accounts.get(&r.name);
            let pct = |k: &str| row.and_then(|v| v.get(k)).and_then(Value::as_i64);
            if let Some(live) = rank_from_live(pct("session_pct"), pct("week_pct")) {
                r.rank = live;
            }
            r
        })
        .collect()
}

/// Đóng dấu [`Rank::Dead`] lên những tài khoản có tên trong SỔ.
///
/// 🔴 Vì sao là một lượt riêng chứ không nằm trong [`rank_all`]: `rank_all` đọc
/// ĐĨA (`.claude.json` của từng tài khoản) và không biết gì về cơ sở dữ liệu;
/// cuốn sổ thì nằm trong `cursors`. Trộn hai nguồn vào một hàm là buộc mọi bài
/// kiểm của `rank_all` phải dựng một `Db`. Tách ra thì phần "đo" vẫn thuần đĩa,
/// phần "nhớ" thuần sổ, và chỗ gọi ghép hai cái lại — đúng ranh giới
/// `accounts_text` đã tự đặt cho mình.
///
/// Dấu này ĐÈ lên mọi hạng đọc được từ đĩa, cố ý: sổ `.claude.json` của một tài
/// khoản đã bị khoá vẫn ghi `92%` từ ba ngày trước và vẫn xếp ra `Unknown` —
/// một con số đúng về một cánh cửa đã đóng.
pub fn apply_dead_book(
    ranked: Vec<Ranked>,
    dead: &std::collections::BTreeMap<String, String>,
) -> Vec<Ranked> {
    ranked
        .into_iter()
        .map(|mut a| {
            if dead.contains_key(&a.name) {
                a.rank = Rank::Dead;
            }
            a
        })
        .collect()
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
        // 🔴 MỖI CỬA SỔ MANG MỐC CỦA CHÍNH NÓ — Hà 2026-09-16, lượt thứ hai:
        // *"Vẫn thiếu giờ reset của phiên"*. Bản trước in **một** mốc, của cửa
        // sổ CHẬT NHẤT, nên hễ cửa sổ tuần chật hơn là giờ quay vòng của cửa sổ
        // PHIÊN (5 tiếng) biến mất — mà đó mới đúng là con số chủ máy cần khi
        // câu hỏi là *"mở phiên ngay bây giờ được không"*. Hai cửa sổ, hai con
        // số phần trăm đứng cạnh nhau trên cùng một dòng, thì mỗi con số phải
        // kéo theo cái đồng hồ của nó: một dòng in 62% và 19% mà chỉ có một mốc
        // thì người đọc không có cách nào biết mốc ấy thuộc về số nào.
        if let Some(w) = self.week_pct {
            p.push(
                match moc_cua_so(self.week_pct, self.week_resets_at.as_deref(), now_ms, false) {
                    Some(m) => format!("tuần {w}% ↻ {m}"),
                    None => format!("tuần {w}%"),
                },
            );
        }
        if let Some(h) = self.hour5_pct {
            p.push(
                match moc_cua_so(
                    self.hour5_pct,
                    self.hour5_resets_at.as_deref(),
                    now_ms,
                    true,
                ) {
                    Some(m) => format!("5 tiếng {h}% ↻ {m}"),
                    None => format!("5 tiếng {h}%"),
                },
            );
        }
        if p.is_empty() {
            p.push("sổ không có con số nào".to_string());
        }
        let hang = rank(self, now_ms);
        p.push(format!("hạng: {}", hang.say()));
        // 🪦 Ở đây từng có `mở lại {mốc}` cho hàng KỊCH TRẦN (15/09) rồi thêm
        // `reset {cửa sổ chật nhất}` cho hàng còn lại (16/09, lượt đầu). Cả hai
        // đi cùng bản vá trên: khi **mỗi** con số phần trăm đã kéo theo đồng hồ
        // của chính nó, một trường thứ ba ở cuối dòng chỉ lặp lại một trong hai
        // mốc ấy dưới một cái tên khác — và lặp bằng một cái tên khác là cách
        // rẻ nhất để hai chỗ về sau nói lệch nhau.
        //
        // ⚠ Hàng kịch trần KHÔNG mất thông tin: `hạng: ĐÃ KỊCH TRẦN` nói cửa
        // đang đóng, còn `tuần 100% ↻ 17:59 16/09 (còn 9 tiếng)` nói đóng tới
        // bao giờ — và nói ĐÚNG cửa sổ nào đang đóng, thứ mà một dòng "mở lại"
        // gộp chung không phân biệt được khi cả hai cửa sổ cùng chặn.
        // Điều kiện kiểm tra THỨ HAI, đứng cạnh tỉ lệ chứ không thay nó.
        if let Some(canh) = kich_tran_can_xac_nhan(self, now_ms) {
            p.push(format!("⚠ {canh}"));
        }
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
        chua_dung_duoc: None,
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        // 🔴 KHÔNG CÓ TỆP và ĐỌC HỎNG là hai chuyện, và chúng dẫn tới hai kết
        // luận ngược nhau. Không có tệp ⟹ chưa lượt `claude` nào từng chạy
        // trong thư mục ấy ⟹ chắc chắn chưa dùng được, phải fail-closed. Đọc
        // hỏng vì quyền/đĩa ⟹ một trục trặc có thể tự qua, và đóng vĩnh viễn
        // một tài khoản đang tốt vì một lượt đọc hụt thì tệ hơn.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            logging::warn(
                "quota_book_missing",
                json!({ "account": account, "path": path.display().to_string(),
                        "why": "chưa có sổ ⟹ thư mục cấu hình này chưa chạy `claude` lần nào" }),
            );
            let mut q = trong(format!("chưa có {}", path.display()));
            q.chua_dung_duoc = Some("chưa đăng nhập (chưa có sổ .claude.json)".to_string());
            return q;
        }
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
    // Hỏi TRƯỚC câu hạn mức: "tài khoản này mở cửa sổ ra có chạy được không".
    // Hai câu ấy nghe giống nhau ở chỗ cùng ra `None`, nhưng chúng đi hai đường
    // — xem [`account_not_ready`].
    let chua_dung_duoc = account_not_ready(&doc);
    if let Some(why) = &chua_dung_duoc {
        logging::warn(
            "account_not_ready",
            json!({ "account": account, "path": path.display().to_string(), "why": why,
                    "hau_qua": "KHÔNG gợi ý tài khoản này — mở cửa sổ ra là nó đứng" }),
        );
    }
    let Some(u) = doc.get("cachedUsageUtilization") else {
        // KHÔNG phải lỗi: một tài khoản chưa gọi API lần nào thì chưa có khoá này.
        logging::info(
            "quota_not_cached_yet",
            json!({ "account": account, "path": path.display().to_string() }),
        );
        let mut q = trong("CLI chưa ghi số hạn mức nào cho tài khoản này".to_string());
        q.chua_dung_duoc = chua_dung_duoc;
        return q;
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
        chua_dung_duoc,
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
/// Tài khoản này mở cửa sổ ra có chạy được không — đọc từ chính sổ của CLI.
///
/// 🔴 Ca đo được 2026-09-12, và nó tốn một lượt bàn giao thật: acc4 vừa được
/// khai vào config nhưng CHƯA đăng nhập. Sổ của nó không có số hạn mức ⟹ hạng
/// `Unknown` ⟹ mà `Unknown` đứng **TRƯỚC** `Full` (*"một ẩn số vẫn hơn một cánh
/// cửa đã đóng"*) ⟹ đúng lúc acc2 kịch trần, `watch::suggest_account` chọn acc4,
/// mở cửa sổ `ttys007`, và cửa sổ ấy đứng ở hộp chọn giao diện lần đầu. huba
/// báo trung thực *"phiên CHƯA chào đời sau 12 giây"* — nhưng phiên cũ thì đã
/// bị bỏ lại, đang bị chặn, không làm tiếp được gì.
///
/// Câu ấy KHÔNG phải "chưa đo được hạn mức": nó là "chưa có ai ngồi vào máy
/// này". Một đồng hồ chữa được cái thứ nhất; cái thứ hai chỉ người chữa được —
/// nên nó thuộc về họ `Dead`, không thuộc họ `Unknown`.
///
/// Hai mốc, và cả hai đều cần, đo hai chiều trên bốn tài khoản thật:
///
/// | | oauthAccount | hasCompletedOnboarding |
/// |---|---|---|
/// | acc1 · acc2 · acc3 (đang chạy) | ✓ | ✓ |
/// | acc4 **trước** khi đăng nhập | ✗ | ✗ |
/// | acc4 **sau** khi đăng nhập | ✓ | ✗ ← vẫn treo ở hộp chọn giao diện |
///
/// Hàng cuối là hàng dạy được nhiều nhất: có credential **chưa đủ**. Lượt chạy
/// đầu của một thư mục cấu hình mới còn một cái hộp hỏi nữa, và huba mở cửa sổ
/// lúc không có ai ngồi đó để trả lời.
pub fn account_not_ready(doc: &Value) -> Option<String> {
    if doc.get("oauthAccount").is_none() {
        return Some("chưa đăng nhập (sổ không có oauthAccount)".to_string());
    }
    if doc.get("hasCompletedOnboarding").and_then(Value::as_bool) != Some(true) {
        return Some(
            "đăng nhập rồi nhưng lượt chạy đầu chưa xong — `claude` còn hỏi giao diện".to_string(),
        );
    }
    None
}

/// Bản đọc của một cửa sổ đã **CŨ** chưa — thước đo là chính cái đồng hồ của nó.
///
/// 🔴 Hà 2026-09-15: *"thêm vào phần kịch trần thời gian reset để thêm điều kiện
/// kiểm tra chứ chỉ dựa vào tỉ lệ là không đủ"* · *"vì tỉ lệ là số chết chưa
/// chắc đúng"*.
///
/// Đúng, và [`cua_so`] mới hỏi được một NỬA câu: *mốc mở lại đã qua chưa*. Qua
/// rồi thì nó trả `None`; chưa qua thì nó lấy NGUYÊN con số và **không hỏi con
/// số ấy già bao nhiêu**. Mà con số ấy chỉ do **phiên tương tác** ghi — đo hai
/// chiều 15/09: `claude -p` chạy trót lọt (exit 0, 38–41 giây), `mtime` của
/// `.claude.json` ĐỔI, mà `cachedUsageUtilization` không nhúc nhích ở CẢ tài
/// khoản đã có số LẪN tài khoản chưa có. Nên nó nằm im được vô thời hạn: đúng
/// nghĩa "số chết".
///
/// Ca thật đang sống trên máy đúng lúc viết dòng này:
///
/// ```text
/// acc2  fetched 2026-09-14T07:32Z (già 27,8h)  seven_day 100%  resets 09-16T11:00Z (còn 23,7h)
/// ```
///
/// Tuổi 27,8h > còn lại 23,7h ⟹ **CŨ**. Thước "tuổi > thời gian còn lại" là Hà
/// chọn (15/09) và nó **tự co giãn theo vị trí trong cửa sổ**: cùng một bản đọc
/// 3 tiếng là mới toanh với cửa sổ tuần còn 5 ngày, và đã cũ mèm với cửa sổ 5
/// tiếng chỉ còn 30 phút. Một ngưỡng phút cứng thì hoặc quá chặt cho tuần, hoặc
/// quá lỏng cho 5 tiếng — hai thang khác nhau hai bậc độ lớn.
///
/// Không đo được tuổi (`fetched_at_ms` rỗng, hoặc mốc không parse ra) ⟹ **CŨ**,
/// fail-closed. Ở đây "cũ" nghĩa là *đi hỏi lại đi*, KHÔNG phải *loại tài khoản
/// này* — nên fail-closed ở đây không đóng cửa của ai.
fn ban_doc_cu(fetched_at_ms: Option<i64>, resets_at: Option<&str>, now_ms: i64) -> bool {
    let Some(t) = fetched_at_ms else {
        return true;
    };
    let Some(moc) = resets_at.and_then(|r| chrono::DateTime::parse_from_rfc3339(r).ok()) else {
        return true;
    };
    (now_ms - t) > (moc.timestamp_millis() - now_ms)
}

/// Verdict [`Rank::Full`] này có đang tựa vào một con số CŨ không — nói ra bằng số.
///
/// `None` = không phải `Full`, hoặc cửa sổ làm nên verdict ấy còn tươi. `Some(câu)`
/// = nó cũ; câu ấy đi thẳng ra `/accounts` và là tín hiệu cho chỗ gọi đi hỏi
/// [`crate::runtime::usage_cached`].
///
/// 🔴 **Nó KHÔNG hạ hạng, và đó là cả quyết định.** Hà chốt 15/09 giữa ba nhánh
/// hỏng ngược chiều nhau: *dò sống TRƯỚC, **hết cách mới giữ `Full`***. Hạ xuống
/// `Unknown` thì tự phục hồi được, nhưng `Unknown` đứng **TRƯỚC** `Full` trong
/// thứ tự chọn ⟹ huba sẽ mở một cửa sổ chết đúng vào lúc mọi tài khoản khác kịch
/// trần — đúng ca acc4 ngày 12/09, và cái giá của nó là một phiên đang làm việc
/// bị bỏ lại. Nên mặc định ở đây là **GIỮ**, còn đường sửa là [`overlay_live`].
///
/// Chỉ hỏi tuổi của **cửa sổ đã làm nên verdict**, không hỏi cửa sổ kia: một bản
/// đọc có thể tươi với cửa sổ tuần và cũ với cửa sổ 5 tiếng cùng một lúc, và lấy
/// nhầm cửa sổ là cảnh báo sai chỗ.
pub fn kich_tran_can_xac_nhan(q: &Quota, now_ms: i64) -> Option<String> {
    if rank(q, now_ms) != Rank::Full {
        return None;
    }
    for (ten, pct, resets) in [
        ("tuần", q.week_pct, q.week_resets_at.as_deref()),
        ("5 tiếng", q.hour5_pct, q.hour5_resets_at.as_deref()),
    ] {
        if cua_so(pct, resets, now_ms).is_some_and(|p| p >= 100)
            && ban_doc_cu(q.fetched_at_ms, resets, now_ms)
        {
            return Some(match q.fetched_at_ms {
                Some(t) => format!(
                    "kịch trần {ten} theo bản đọc già {} tiếng — cần xác nhận",
                    (now_ms - t).max(0) / 3_600_000
                ),
                None => format!("kịch trần {ten} theo bản đọc không rõ tuổi — cần xác nhận"),
            });
        }
    }
    None
}

/// Mốc mở lại của cửa sổ đang chặn — `None` khi không cửa sổ nào chặn.
///
/// "ĐÃ KỊCH TRẦN" mà không nói **mở lại lúc nào** thì chủ máy không có cơ sở
/// chọn giữa *chờ* và *chuyển tài khoản*, và đó đúng là câu hỏi anh đang đứng
/// trước mỗi lần đọc dòng ấy. Lấy mốc SỚM NHẤT trong những cửa sổ đã ≥100: đó là
/// lúc tài khoản dùng lại được.
pub fn mo_lai_luc(q: &Quota, now_ms: i64) -> Option<String> {
    [
        (q.week_pct, q.week_resets_at.as_deref()),
        (q.hour5_pct, q.hour5_resets_at.as_deref()),
    ]
    .into_iter()
    .filter(|(pct, resets)| cua_so(*pct, *resets, now_ms).is_some_and(|p| p >= 100))
    .filter_map(|(_, resets)| resets)
    .filter_map(|r| chrono::DateTime::parse_from_rfc3339(r).ok())
    .min()
    .map(|t| {
        t.with_timezone(&chrono::Local)
            .format("%H:%M %d/%m")
            .to_string()
    })
}

/// Cửa sổ ĐANG CHẬT NHẤT của một tài khoản **chưa** kịch trần mở lại lúc nào.
///
/// 🔴 Hà 2026-09-16: *"Danh sách acc thiếu giờ sắp reset"*. [`mo_lai_luc`] chỉ
/// trả lời cho tài khoản đã kịch trần — tức đúng lúc câu trả lời không còn dùng
/// để chọn được nữa. Hàm này trả lời cho phần còn lại, và đó mới là chỗ chủ máy
/// đứng khi đọc `/accounts`: *acc này 61% tuần, mở phiên vào có sao không?*
///
/// **Chọn ĐÚNG cửa sổ mà [`rank`] đã dùng để chấm hạng** — cái CHẬT NHẤT, vì nó
/// là cái sẽ chặn trước. Một dòng in "hạng: đã dùng 45%" (lấy từ cửa sổ 5 tiếng)
/// mà kèm mốc reset của cửa sổ TUẦN là hai phép đo nói về hai thứ khác nhau
/// đứng cạnh nhau như một — đúng hình dạng lỗi mà `/accounts` đã tách nhãn "(dò
/// /usage: …)" ra để tránh.
///
/// `None` khi chưa đọc được đủ hai cửa sổ: giữ nguyên luật [`cua_so`] — mốc đã
/// qua hay đọc không ra thì fail-closed, thà im còn hơn in một cái hẹn đã hết
/// hạn ra màn.
pub fn sap_reset(q: &Quota, now_ms: i64) -> Option<String> {
    let w = cua_so(q.week_pct, q.week_resets_at.as_deref(), now_ms);
    let h = cua_so(q.hour5_pct, q.hour5_resets_at.as_deref(), now_ms);
    // Hoà thì lấy TUẦN: nó là cửa sổ đắt hơn (5 tiếng tự mở lại trong ngày),
    // nên khi hai con số bằng nhau thì mốc tuần là mốc đáng biết hơn.
    let (ten, pct, moc, ngan_gon) = match (w, h) {
        (Some(a), Some(b)) if b > a => {
            ("5 tiếng", q.hour5_pct, q.hour5_resets_at.as_deref(), false)
        }
        (Some(_), Some(_)) => ("tuần", q.week_pct, q.week_resets_at.as_deref(), false),
        _ => return None,
    };
    Some(format!(
        "reset {ten} {}",
        moc_cua_so(pct, moc, now_ms, ngan_gon)?
    ))
}

/// Đồng hồ của MỘT cửa sổ hạn mức: `"15:00 20/09 (còn 4 ngày)"`.
///
/// Một nguồn duy nhất cho mọi chỗ in mốc quay vòng — dòng `/accounts`
/// ([`Quota::say`]) và dòng log ([`sap_reset`]) đọc chung hàm này. Hai bản chép
/// của cùng một phép dựng chuỗi là hai câu trả lời cho cùng một câu hỏi, và
/// chúng chỉ cần một lượt sửa lệch nhau là bắt đầu nói khác nhau về cùng một
/// tài khoản.
///
/// `ngan_gon` bỏ phần NGÀY: cửa sổ 5 tiếng luôn quay vòng trong vòng 5 giờ tới
/// (đã qua [`cua_so`] nên mốc chắc chắn còn ở phía trước), nên `09:50` là đủ và
/// `09:50 16/09` chỉ làm dòng dài thêm ở đúng chỗ màn hình điện thoại hẹp nhất.
///
/// `None` = **chưa đọc được cửa sổ này** (thiếu số, mốc hỏng, hay mốc đã qua —
/// luật fail-closed của [`cua_so`]), KHÔNG phải *"cửa sổ này không quay vòng"*.
pub fn moc_cua_so(
    pct: Option<i64>,
    resets_at: Option<&str>,
    now_ms: i64,
    ngan_gon: bool,
) -> Option<String> {
    cua_so(pct, resets_at, now_ms)?;
    let khi = chrono::DateTime::parse_from_rfc3339(resets_at?).ok()?;
    let phut = (khi.timestamp_millis() - now_ms) / 60_000;
    // 🔴 `phut == 0` là *"còn DƯỚI một phút"*, không phải *"đã qua"* — [`cua_so`]
    // ở trên đã loại mọi mốc quá khứ (fail-closed), nên tới được đây là mốc còn
    // ở phía trước, chỉ là gần tới mức phép chia phút làm tròn về 0.
    //
    // Bản đầu viết `..=0 => return None` và **tầng đối chứng ngược bắt được nó**:
    // mutant "bỏ cửa ấy" ra XANH, nghĩa là không bài kiểm nào chạm tới dòng đó —
    // bài kiểm mang tên *"mốc đã qua thì không in gì"* thật ra đang chứng minh
    // cái cửa `cua_so`, không phải cửa này. Và khi đọc lại thì dòng ấy còn SAI
    // hướng: nó làm tài khoản sắp mở lại **biến mất khỏi màn** đúng phút đáng
    // nói nhất.
    let con = match phut {
        ..=0 => "còn dưới 1 phút".to_string(),
        1..=90 => format!("còn {phut} phút"),
        91..=1439 => format!("còn {} tiếng", phut / 60),
        _ => format!("còn {} ngày", phut / 1440),
    };
    let khuon = if ngan_gon { "%H:%M" } else { "%H:%M %d/%m" };
    Some(format!(
        "{} ({con})",
        khi.with_timezone(&chrono::Local).format(khuon)
    ))
}

pub fn rank(q: &Quota, now_ms: i64) -> Rank {
    // Đứng TRƯỚC mọi phép tính phần trăm: còn chưa mở được cửa sổ thì con số
    // hạn mức nói về một cánh cửa chưa ai bước qua.
    if q.chua_dung_duoc.is_some() {
        return Rank::NotReady;
    }
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
    read_all(cfg)
        .into_iter()
        .map(|q| {
            let r = rank(&q, now_ms);
            // 🔴 Điều kiện thứ hai phải có mặt Ở ĐÂY, không chỉ ở `/accounts`.
            // Log là bề mặt pháp y của repo này — mọi mục PLAN.md đều dựng từ
            // nó — nên một cảnh báo chỉ hiện trên điện thoại là cảnh báo mới làm
            // được một nửa (luật 13②: thứ người đọc phán quyết không thấy thì
            // không tính là cảnh báo). Đo 15/09 ngay sau lượt cài đầu tiên: dòng
            // `quota_read` của acc2 in `rank:"ĐÃ KỊCH TRẦN"` trong khi verdict ấy
            // tựa vào bản đọc già 27,8 tiếng, và **không một trường nào nói ra**.
            logging::info(
                "quota_read",
                json!({ "account": q.account, "week_pct": q.week_pct,
                        "week_resets_at": q.week_resets_at, "hour5_pct": q.hour5_pct,
                        "fetched_at_ms": q.fetched_at_ms, "why_unknown": q.why_unknown,
                        "rank": r.say(),
                        // Cùng luật với `can_xac_nhan` ngay dưới: thứ chỉ hiện
                        // trên điện thoại là thứ người đọc log không thấy, và
                        // log mới là bề mặt pháp y của kho này.
                        "sap_reset": sap_reset(&q, now_ms),
                        "can_xac_nhan": kich_tran_can_xac_nhan(&q, now_ms),
                        "mo_lai_luc": mo_lai_luc(&q, now_ms) }),
            );
            Ranked {
                name: q.account,
                rank: r,
            }
        })
        .collect()
}

/// Bản đọc thô của mọi tài khoản, theo đúng thứ tự cấu hình.
///
/// 🔴 Tách khỏi [`rank_all`] vì `runtime::accounts_text` cần bản THÔ (nó in cả
/// phần trăm lẫn tuổi), còn chỗ chọn tài khoản chỉ cần cái hạng. Quan trọng hơn:
/// **đây là chỗ duy nhất chạm đĩa**, nên `accounts_text` giữ nguyên được ranh
/// giới nó đã tự đặt từ đầu (*"phần dựng câu, tách khỏi phần đi đo"*).
///
/// Ranh giới ấy tôi vừa phá một lần trong chính lượt này (30/08): cho
/// `accounts_text` tự gọi `read` là biến một hàm dựng chữ thuần thành hàm đọc
/// `$HOME`, và bài kiểm `usage_still_being_measured_says_so_instead_of_showing_zero`
/// đỏ ngay — nó chấm *"không được bịa 0%"* trên một câu nay mang số thật của
/// máy đang chạy. Cổng bắt đúng chỗ; chép lại đây để đừng ai vá lại kiểu ấy.
pub fn read_all(cfg: &crate::config::Config) -> Vec<Quota> {
    cfg.claude_accounts_or_ambient()
        .iter()
        .map(|a| {
            let dir = a
                .config_dir
                .as_deref()
                .filter(|d| !d.is_empty())
                .map(|d| crate::config::expand_home(Path::new(d)));
            read(&a.name, dir.as_deref())
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
            chua_dung_duoc: None,
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

    // ─────────────────────────────────────────────────────────────────────
    // Phép dò SỐNG (`/usage`) đè lên tệp — Hà 07/09: "chạy luôn /usage có hơn"
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn rank_from_live_lay_cua_so_chat_nhat() {
        assert_eq!(rank_from_live(Some(10), Some(80)), Some(Rank::Free(80)));
        assert_eq!(rank_from_live(Some(100), Some(10)), Some(Rank::Full));
        assert_eq!(rank_from_live(None, Some(30)), Some(Rank::Free(30)));
        assert_eq!(rank_from_live(Some(30), None), Some(Rank::Free(30)));
        assert_eq!(rank_from_live(None, None), None);
    }

    /// Đúng ca gây ra bug 07/09: tệp còn ghi acc1 rảnh (22%), nhưng phiên vừa
    /// dồn vào đã đẩy nó kịch trần THẬT — phép dò sống phải thắng.
    #[test]
    fn overlay_live_de_len_mot_con_so_te_cua_tep() {
        let file_ranked = vec![Ranked {
            name: "acc1".into(),
            rank: Rank::Free(22),
        }];
        let live = json!({ "acc1": { "session_pct": 100, "week_pct": 40 } });
        let out = overlay_live(file_ranked, &live);
        assert_eq!(out[0].rank, Rank::Full);
    }

    /// Phép dò chưa có số (đang đo, hết giờ, hay không đọc được) ⟹ GIỮ NGUYÊN
    /// hạng của tệp — đây là lùi về đường cũ, không phải một hạng bịa mới.
    #[test]
    fn overlay_live_lui_ve_tep_khi_chua_do_duoc() {
        let file_ranked = vec![Ranked {
            name: "acc1".into(),
            rank: Rank::Free(22),
        }];
        let live = json!({ "acc1": { "err": "/usage hết giờ sau 60000ms" } });
        let out = overlay_live(file_ranked.clone(), &live);
        assert_eq!(out, file_ranked);

        let live_rong = json!({});
        let out2 = overlay_live(file_ranked.clone(), &live_rong);
        assert_eq!(
            out2, file_ranked,
            "tài khoản vắng mặt trong phép dò cũng phải lùi về tệp"
        );
    }
}
