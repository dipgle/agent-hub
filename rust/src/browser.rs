//! Lái **trình duyệt THẬT của chủ máy** từ điện thoại.
//!
//! Hà 2026-08-23: *"Cổng điều khiển browser thế nào rồi"*. Trước lượt này:
//! không route, không dòng nào trong `PLAN.md`, không sự kiện nào trong devlog.
//!
//! ## Vì sao AppleScript chứ không phải CDP
//!
//! Phép thử cầu nối (`CLAUDE.md`): ngồi ở máy thì Hà làm gì? Mở đúng cái Chrome
//! đang đăng nhập sẵn của mình. Đo trên máy này 23/08:
//!
//! * Chrome 151 đang chạy, pid `94894`, **profile mặc định** — tức mọi phiên
//!   đăng nhập nằm ở đó.
//! * **Không cổng gỡ lỗi nào mở** (`127.0.0.1:9222/9223/9229` đều câm), và
//!   Chrome từ chối bật CDP trên profile mặc định — muốn CDP là phải dựng một
//!   profile RIÊNG, tức một trình duyệt KHÁC: không đăng nhập, không cookie,
//!   không phải thứ Hà đang nhìn. Đó là một thế giới thứ hai, không phải cây
//!   cầu bắc sang thế giới đang có.
//! * `Contents/Resources/scripting.sdef` (16 KB, `NSAppleScriptEnabled=true`)
//!   khai đủ mặt cần: `open location` · `make new tab` · `active tab` · `URL` ·
//!   `title` · `loading` · `go back`/`go forward` · `reload` ·
//!   `execute … javascript`.
//!
//! Nên đường đi là AppleScript, y như `keys.rs` lái Terminal — cùng một cơ chế,
//! cùng một loại quyền, và huba không phải học một giao thức thứ hai.
//!
//! ## Hai cánh cửa quyền, và cả hai đều do NGƯỜI mở
//!
//! Đọc `TCC.db` ngày 23/08, hàng của bản đang cài:
//!
//! ```text
//! ~/Library/Application Support/hub/bin/hubd → com.apple.Terminal   = 2 (đã cấp)
//! ~/Library/Application Support/hub/bin/hubd → com.google.Chrome    = KHÔNG CÓ DÒNG NÀO
//! ```
//!
//! Một tiến trình nền chỉ **hiện ra** trong danh sách Tự động hoá sau khi nó đã
//! THỬ (bài học `cgkeys` trong `CLAUDE.md`), nên lượt đầu tiên hỏng là chuyện
//! đã biết trước — việc của mã ở đây là biến nó thành một câu chỉ đúng chỗ bấm,
//! chứ không phải một dòng `osascript hỏng:` cụt lủn.
//!
//! Cửa thứ hai chỉ chắn `execute javascript`: Chrome tắt sẵn *View → Developer
//! → Allow JavaScript from Apple Events*. Không tra được bằng tệp cấu hình —
//! đã tìm cả `Default/Preferences`, `Local State` và `defaults read
//! com.google.Chrome`, **không khoá nào** mang nghĩa ấy. Nên nó phải được phát
//! hiện lúc CHẠY, và [`Loi::JsTat`] là chỗ dịch nó thành đường bấm.

use std::time::Duration;

use serde_json::json;

use crate::exec::{run, RunOpts};
use crate::logging;

/// Tên trình duyệt trong AppleScript. Một hằng vì nó xuất hiện trong mọi đoạn
/// script, và một lượt đổi tên gõ tay sót một chỗ là một lỗi chạy mới im tiếng.
const CHROME: &str = "Google Chrome";

/// `osascript` mất vài trăm ms. 20s rộng rãi, và một cái treo ở đây giữ cả vòng
/// chạy của daemon — cùng con số với `keys::OSA_TIMEOUT`.
const OSA_TIMEOUT: Duration = Duration::from_secs(20);

/// Ký tự ngăn ô khi AppleScript trả một bảng về.
///
/// Dùng TAB chứ không dùng dấu phẩy hay `|`: tiêu đề trang có cả hai thứ ấy
/// (`Gmail | Hộp thư`), còn ký tự tab thì Chrome đã gột khỏi tiêu đề trước khi
/// vẽ lên tab. URL thì không bao giờ chứa nó — dấu tab phải mã hoá thành `%09`.
const O: char = '\t';

/// Chuyện có thể hỏng, và mỗi thứ một câu KHÁC NHAU.
///
/// 🔴 Gộp cả ba vào một câu *"không lái được Chrome"* là đúng cái hình dạng đã
/// chặn mọi bản vá của huba suốt nhiều ngày (`exec::drain_capped`, 22/08): ba
/// nguyên nhân, ba chỗ sửa, một câu chữ — người đọc đi tìm một sự cố không có.
#[derive(Debug)]
pub enum Loi {
    /// Chrome không chạy. KHÔNG tự mở nó: `tell application` sẽ khởi động
    /// Chrome, và một cái cửa sổ tự bật lên vì ai đó lỡ gõ `/web` là thứ ngồi ở
    /// máy không bao giờ xảy ra.
    Tat,
    /// macOS chưa cho `hubad` gửi lệnh sang Chrome.
    ChuaCapQuyen,
    /// Chrome đang tắt *Allow JavaScript from Apple Events*.
    JsTat,
    /// Khung hình chụp ra RỖNG (đen) — màn đang khoá HOẶC thiếu quyền Screen
    /// Recording (KHÁC quyền Tự động hoá — `mang_ra_truoc` đã qua rồi mới tới
    /// đây, nên Apple Events chắc chắn còn dùng được). Cả hai nguyên nhân đều
    /// KHÔNG cản đường đọc chữ (`chu_trang`/`tabs`) — chữ đi qua Apple Events
    /// riêng cho Chrome, không đụng khung hình toàn màn, nên macOS không khoá
    /// nó theo màn hình. Hà hỏi thẳng 2026-09-04: *"màn khóa vẫn xem được tình
    /// trạng trình duyệt chứ đưa ra thông báo làm gì"* — nên caller (route
    /// `/web anh`, tool MCP) phải coi biến thể này là TÍN HIỆU rơi về chữ,
    /// không phải một lỗi cụt. Mang theo lý do người-đọc-được.
    AnhTrong(String),
    /// Mọi thứ còn lại — giữ NGUYÊN VĂN, không dịch, không đoán.
    Khac(String),
}

impl std::fmt::Display for Loi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Loi::Tat => write!(f, "Chrome đang tắt — mở nó rồi gọi lại."),
            Loi::ChuaCapQuyen => write!(
                f,
                "macOS chưa cho huba gửi lệnh sang Chrome.\n\
                 Cài đặt → Quyền riêng tư & Bảo mật → Tự động hoá → mục `hubd` → \
                 bật `Google Chrome`.\n\
                 (Lượt hỏng này CHÍNH LÀ thứ làm `hubd` hiện ra trong danh sách ấy — \
                 gọi lại sau khi tích.)"
            ),
            Loi::JsTat => write!(
                f,
                "Chrome đang chặn JavaScript gọi từ AppleScript, nên đọc được tiêu đề \
                 và địa chỉ nhưng KHÔNG đọc được nội dung trang.\n\
                 Trên máy: Chrome → menu View → Developer → tích `Allow JavaScript from \
                 Apple Events`."
            ),
            Loi::AnhTrong(s) => write!(f, "{s}"),
            Loi::Khac(s) => write!(f, "{s}"),
        }
    }
}

/// Đọc lời từ chối của `osascript` thành đúng một trong bốn thứ trên.
///
/// ⚠ Ba mẫu chữ dưới đây lấy từ tài liệu và từ chính chuỗi macOS trả, **chưa đo
/// được cả ba trên máy này** — hàng `hubd → com.google.Chrome` còn trống nên
/// lượt đầu tiên chạy thật mới sinh ra chúng. Vì thế nhánh cuối giữ NGUYÊN VĂN
/// lỗi: đoán sai một mẫu thì người đọc vẫn thấy câu thật của macOS, chứ không
/// nhận một lời phân loại sai kèm hướng dẫn sai. Đo được rồi thì siết lại đây.
fn doc_loi(stderr: &str) -> Loi {
    let s = stderr.trim();
    let low = s.to_lowercase();
    // -1743 = "Not authorized to send Apple events to <app>".
    if low.contains("-1743") || low.contains("not authorized to send apple events") {
        return Loi::ChuaCapQuyen;
    }
    // Chrome tự trả câu này khi mục menu kia chưa tích.
    if low.contains("javascript") && (low.contains("applescript") || low.contains("apple event")) {
        return Loi::JsTat;
    }
    // -600 / "Application isn't running".
    if low.contains("-600") || low.contains("isn't running") || low.contains("is not running") {
        return Loi::Tat;
    }
    Loi::Khac(crate::exec::truncate(s, 200))
}

/// Chuỗi cho AppleScript — chỉ hai ký tự phải thoát, và bỏ sót một cái thì
/// script hoặc hỏng cú pháp, hoặc ĐỔI NGHĨA. Cùng luật với `keys::as_string`.
fn as_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn osa(script: &str) -> Result<String, Loi> {
    let out = run(
        "osascript",
        &["-e", script],
        RunOpts {
            timeout: Some(OSA_TIMEOUT),
            ..Default::default()
        },
    )
    .map_err(|e| Loi::Khac(e.to_string()))?;
    if out.timed_out {
        return Err(Loi::Khac(format!(
            "osascript quá {}s — Chrome đang treo hay đang có hộp thoại?",
            OSA_TIMEOUT.as_secs()
        )));
    }
    if out.code != Some(0) {
        let loi = doc_loi(&out.stderr);
        // Rule 3 — không đường lỗi nào đi qua đây mà không để lại dấu.
        logging::warn(
            "browser_osascript_failed",
            json!({ "why": format!("{loi:?}"), "stderr": crate::exec::truncate(out.stderr.trim(), 200) }),
        );
        return Err(loi);
    }
    let body = out.stdout.trim_end_matches('\n').to_string();
    if body.trim() == OFF {
        return Err(Loi::Tat);
    }
    Ok(body)
}

/// Thứ script trả về khi Chrome không chạy. Một chuỗi mốc chứ không phải chuỗi
/// rỗng: "Chrome tắt" và "Chrome mở nhưng không có tab nào" là hai câu khác
/// nhau, và trả rỗng cho cả hai là biến cái sau thành cái trước.
const OFF: &str = "\u{1}OFF\u{1}";

/// Mở đầu mọi script: hỏi Chrome có chạy không mà KHÔNG khởi động nó.
///
/// `application "X" is running` là câu duy nhất hỏi được điều ấy; `tell
/// application "X"` thì tự mở Chrome lên — trên máy của người khác, vì một tin
/// nhắn gõ nhầm.
fn script(than: &str) -> String {
    format!(
        "if application {app} is running then\n\
           tell application {app}\n{than}\n  end tell\n\
         else\n  return {off}\nend if",
        app = as_string(CHROME),
        off = as_string(OFF),
    )
}

/// Một tab, đúng những gì đọc được mà không cần tới JavaScript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    /// Cửa sổ thứ mấy (1-based, theo thứ tự Chrome xếp).
    pub win: usize,
    /// Tab thứ mấy trong cửa sổ ấy (1-based) — đúng số Cmd+1…8 bấm tới.
    pub idx: usize,
    pub title: String,
    pub url: String,
    /// Có phải tab đang mở của cửa sổ ấy không.
    pub active: bool,
}

/// Bảng tab, phân tích tách khỏi chỗ gọi `osascript` để kiểm được bằng test.
///
/// Hàng hỏng thì BỎ QUA hàng ấy chứ không giết cả danh sách: một tiêu đề lạ
/// không được phép làm cả cái cổng câm. Nhưng bỏ qua thì phải ĐẾM — số hàng
/// đọc được so với số dòng nhận về là thứ duy nhất phân biệt "Chrome có 1 tab"
/// với "huba đọc hỏng 12 hàng".
pub fn doc_bang(raw: &str) -> (Vec<Tab>, usize) {
    let mut tabs = Vec::new();
    let mut hong = 0usize;
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let o: Vec<&str> = line.splitn(5, O).collect();
        if o.len() < 5 {
            hong += 1;
            continue;
        }
        match (
            o[0].trim().parse::<usize>(),
            o[1].trim().parse::<usize>(),
            o[2].trim().parse::<usize>(),
        ) {
            (Ok(win), Ok(idx), Ok(act)) => tabs.push(Tab {
                win,
                idx,
                title: o[4].trim().to_string(),
                url: o[3].trim().to_string(),
                active: act == idx,
            }),
            _ => hong += 1,
        }
    }
    (tabs, hong)
}

/// Mọi tab đang mở, mọi cửa sổ.
/// Đoạn hỏi cả bảng tab. THUẦN, để soi được bằng test — `keys::window_script`
/// tách ra vì đúng lý do này: lỗi đầu tiên của tính năng ấy là một dòng thừa ở
/// cuối chuỗi, và nó chỉ lộ khi chạy thật vì không gì soi chuỗi sinh ra.
pub fn sc_tabs() -> String {
    script(&format!(
        "    set r to \"\"\n\
             set w to 0\n\
             repeat with cs in windows\n\
               set w to w + 1\n\
               set a to active tab index of cs\n\
               set t to 0\n\
               repeat with tb in tabs of cs\n\
                 set t to t + 1\n\
                 set r to r & w & {sep} & t & {sep} & a & {sep} & (URL of tb) & {sep} & (title of tb) & linefeed\n\
               end repeat\n\
             end repeat\n\
             return r",
        sep = as_string(&O.to_string()),
    ))
}

/// Mọi tab đang mở, mọi cửa sổ.
pub fn tabs() -> Result<Vec<Tab>, Loi> {
    let raw = osa(&sc_tabs())?;
    let (tabs, hong) = doc_bang(&raw);
    if hong > 0 {
        // Đọc hỏng mà im lặng thì danh sách ngắn đi một cách vô hình.
        logging::warn(
            "browser_tab_rows_unreadable",
            json!({ "hong": hong, "doc_duoc": tabs.len() }),
        );
    }
    Ok(tabs)
}

/// Tab đang mở của cửa sổ trước mặt.
pub fn front() -> Result<Tab, Loi> {
    let all = tabs()?;
    all.into_iter()
        .find(|t| t.win == 1 && t.active)
        .ok_or_else(|| Loi::Khac("Chrome đang mở nhưng không có cửa sổ nào.".into()))
}

/// Địa chỉ nhận được từ điện thoại có được phép mở không.
///
/// 🔴 Đây là một CỔNG, không phải một phép làm sạch. Chuỗi này đi thẳng từ một
/// tin nhắn vào trình duyệt **đang đăng nhập mọi thứ** của chủ máy, nên hai họ
/// phải chặn từ đây:
///
/// * `file:` — biến `/web` thành một máy đọc trộm ổ đĩa, và nó sẽ trả về qua
///   ảnh chụp màn.
/// * `javascript:` — chạy mã tuỳ ý trong phiên đăng nhập ấy. Đúng thứ luật 1
///   của `CLAUDE.md` gọi là "không có tường".
///
/// Cho qua đúng `http`/`https`. Không có lược đồ thì THÊM `https://` — người ta
/// gõ `mail.google.com` chứ không gõ lược đồ, và bắt họ gõ đủ là bắt họ nhớ một
/// luật của máy.
pub fn dia_chi_hop_le(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.contains(char::is_whitespace) {
        return None;
    }
    let low = s.to_lowercase();
    if low.starts_with("http://") || low.starts_with("https://") {
        return Some(s.to_string());
    }
    // Có dấu hai chấm TRƯỚC dấu gạch chéo đầu tiên ⟹ nó khai một lược đồ, và
    // lược đồ ấy không phải hai cái trên. `example.com:8080/x` thì dấu hai chấm
    // nằm sau tên miền nên vẫn là địa chỉ web bình thường.
    let truoc_gach = s.split('/').next().unwrap_or(s);
    if truoc_gach.contains(':') {
        let cong_so = truoc_gach
            .rsplit(':')
            .next()
            .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
        if !cong_so {
            return None;
        }
    }
    // Phải trông như một tên miền: có dấu chấm, hoặc là `localhost`.
    if !s.contains('.') && !low.starts_with("localhost") {
        return None;
    }
    Some(format!("https://{s}"))
}

/// Mở một địa chỉ trong một tab MỚI của cửa sổ trước mặt.
///
/// Tab mới chứ không đè lên tab đang xem: ngồi ở máy thì người ta mở tab mới,
/// và đè lên trang chủ máy đang đọc dở là một thao tác không lùi lại được.
pub fn mo(url: &str) -> Result<Tab, Loi> {
    let url = dia_chi_hop_le(url)
        .ok_or_else(|| Loi::Khac("Địa chỉ không hợp lệ — chỉ mở `http`/`https`.".to_string()))?;
    osa(&sc_open(&url))?;
    logging::info("browser_opened", json!({ "url": url }));
    // Đọc lại từ Chrome thay vì tin vào chuỗi mình vừa gửi: trang có thể
    // chuyển hướng, và một câu "đã mở <url>" nói về địa chỉ ĐÃ GÕ chứ không
    // phải địa chỉ đang hiện là một câu sai ngay khi nó hữu ích nhất.
    front()
}

/// Chuyển sang một tab, và đưa cửa sổ ấy ra trước mặt.
pub fn chon(win: usize, idx: usize) -> Result<Tab, Loi> {
    osa(&sc_select(win, idx))?;
    logging::info("browser_tab_selected", json!({ "win": win, "idx": idx }));
    tabs()?
        .into_iter()
        .find(|t| t.win == win && t.idx == idx)
        .ok_or_else(|| Loi::Khac(format!("không còn tab {win}.{idx}")))
}

/// Nội dung trang, dưới dạng CHỮ.
///
/// Vì sao cần, khi đã có ảnh chụp màn: một tấm PNG qua Telegram không tìm được,
/// không sao chép được, không đọc nổi phần nằm dưới màn hình — mà trang web thì
/// dài hơn khung nhìn gần như luôn luôn. Đây là bản `history of tab` mà phía
/// Terminal KHÔNG có (xem `CLAUDE.md` §13: TUI vẽ đè nên bộ đệm cuộn rỗng);
/// trình duyệt thì giữ nguyên cây DOM, nên chỗ này lấy được trọn trang.
///
/// Script cố định, KHÔNG nhận mã từ tin nhắn — xem [`dia_chi_hop_le`] về việc
/// vì sao một đường chạy mã tuỳ ý không được phép tồn tại ở đây.
pub fn chu_trang() -> Result<String, Loi> {
    osa(&sc_text())
}

/// Mở một địa chỉ ở tab cuối, rồi chuyển sang chính nó.
pub fn sc_open(url: &str) -> String {
    script(&format!(
        "    if (count of windows) is 0 then make new window\n\
             make new tab at end of tabs of front window with properties {{URL:{u}}}\n\
             set active tab index of front window to (count of tabs of front window)\n\
             return {u}",
        u = as_string(url),
    ))
}

/// Chuyển tab, và đưa cửa sổ ấy ra trước mặt.
pub fn sc_select(win: usize, idx: usize) -> String {
    script(&format!(
        "    set cs to window {win}\n\
             set active tab index of cs to {idx}\n\
             set index of cs to 1\n\
             activate\n\
             return (URL of active tab of cs)"
    ))
}

/// Đọc nội dung trang. Script CỐ ĐỊNH — không nhận mã từ tin nhắn.
pub fn sc_text() -> String {
    script(&format!(
        "    execute front window's active tab javascript {js}",
        js = as_string("document.body ? document.body.innerText : ''"),
    ))
}

/// Đưa Chrome ra trước mặt, KHÔNG tự mở nó nếu đang tắt.
///
/// Script thân chỉ có `activate` — gate của [`script`] đã lo phần "Chrome có
/// đang chạy không". Cùng lý do `mo`/`chon` không tự khởi động Chrome: một cửa
/// sổ tự bật lên vì ai đó lỡ gõ một lệnh từ điện thoại là thứ ngồi ở máy không
/// bao giờ xảy ra.
pub fn mang_ra_truoc() -> Result<(), Loi> {
    osa(&script("  activate\n  return \"ok\""))?;
    Ok(())
}

/// Chụp ảnh MÀN HÌNH của máy, sau khi đưa Chrome ra trước mặt.
///
/// Nhịp giống hệt `keys::photograph_window` (đưa cửa sổ ra trước → đợi 700ms →
/// `screencapture -x` → kiểm ảnh đen) — cùng một lý do: `screencapture` trả
/// exit 0 ngay cả khi macOS âm thầm từ chối vì thiếu quyền Screen Recording, và
/// một màn ĐANG KHOÁ ra đúng tấm ảnh đen y hệt. Tái dùng nguyên văn
/// `keys::frame_is_blank` + `keys::screen_locked` + `keys::blank_frame_reason`
/// thay vì viết lại phép đo ấy: nó đã trả giá một lần (đo 2026-08-18) để phân
/// biệt hai nguyên nhân cùng ra một tấm ảnh đen, và trả về đúng kiểu lỗi
/// [`Loi`] của tệp này thay vì `anyhow::Result` như bên `keys.rs`.
/// Lý do ảnh ra rỗng — bằng đúng chữ của Chrome/`/web`, KHÔNG mượn câu của
/// `keys::blank_frame_reason` (câu ấy nói về PHIÊN, nhắc `/anh`/`/shot` —
/// nhắc nhầm lệnh khi đây là `/web anh`/`browser_screenshot` là tự làm rối
/// người đọc). Cả ba nhánh đều nói rõ: đọc CHỮ vẫn được, khoá màn không cản.
fn ly_do_anh_trong(locked: Option<bool>) -> String {
    match locked {
        Some(true) => "Chrome không cho ẢNH vì màn hình đang khoá — macOS chặn NỘI DUNG khung \
             hình khi khoá màn, với MỌI tiến trình, không phân biệt đã có quyền hay chưa (đây là \
             giới hạn của macOS, không phải huba chặn). Đọc CHỮ vẫn được: nó đi qua Apple Events \
             riêng cho Chrome, không đụng khung hình toàn màn."
            .to_string(),
        Some(false) => "Chrome không cho ẢNH vì thiếu quyền Screen Recording (khác quyền Tự động \
             hoá đang dùng để đọc chữ) — System Settings → Privacy & Security → Screen Recording \
             → bật cho tiến trình đang gọi. Đọc CHỮ vẫn được trong lúc chờ cấp quyền này."
            .to_string(),
        None => "Chrome không cho ẢNH, và huba không đo được lý do (màn khoá hay thiếu quyền \
             Screen Recording). Đọc CHỮ vẫn được — dùng nó trong lúc chờ kiểm tay."
            .to_string(),
    }
}

pub fn chup_anh(path: &std::path::Path) -> Result<(), Loi> {
    // Chụp mù thì vô nghĩa: một Chrome đang nằm sau cửa sổ khác cho ra ảnh
    // không trả lời được câu người dùng hỏi. Lỗi ở đây thì trả THẲNG, không
    // chụp liều.
    mang_ra_truoc()?;
    std::thread::sleep(Duration::from_millis(700));
    let out = run(
        "screencapture",
        &["-x", &path.display().to_string()],
        RunOpts {
            timeout: Some(Duration::from_secs(20)),
            ..Default::default()
        },
    )
    .map_err(|e| Loi::Khac(e.to_string()))?;
    if out.code != Some(0) || !path.exists() {
        logging::warn(
            "browser_screenshot_failed",
            json!({ "code": out.code, "stderr": crate::exec::truncate(out.stderr.trim(), 160) }),
        );
        return Err(Loi::Khac(format!(
            "screencapture không chụp được ({}).",
            crate::exec::truncate(out.stderr.trim(), 160)
        )));
    }
    // Chụp được ≠ chụp thấy — xem doc-comment của `keys::frame_is_blank`.
    let blank = crate::keys::frame_is_blank(path);
    if blank == Some(true) {
        let locked = crate::keys::screen_locked();
        let _ = std::fs::remove_file(path);
        let ly_do = ly_do_anh_trong(locked);
        logging::warn(
            "browser_screenshot_blank",
            json!({ "screen_locked": locked }),
        );
        return Err(Loi::AnhTrong(ly_do));
    }
    logging::info(
        "browser_screenshot",
        json!({ "path": path.display().to_string() }),
    );
    Ok(())
}

/// Click một phần tử qua CSS selector. Script CỐ ĐỊNH — `selector` chỉ là DỮ
/// LIỆU, không bao giờ là mã: nhúng qua `serde_json::to_string` (escape JS an
/// toàn) rồi cả khối JS mới đi qua `as_string` (escape AppleScript). Hai tầng
/// thoát, không được gộp hay bỏ tầng nào — cùng triết lý với [`dia_chi_hop_le`]
/// về việc vì sao một đường chạy mã tuỳ ý không được phép tồn tại ở tệp này.
pub fn sc_click(selector: &str) -> String {
    let sel = serde_json::to_string(selector).unwrap_or_else(|_| "null".to_string());
    let js = format!(
        "(function(){{var el=document.querySelector({sel});if(!el)return \"0\";el.click();return \"1\";}})()"
    );
    script(&format!(
        "    execute front window's active tab javascript {j}",
        j = as_string(&js),
    ))
}

/// Click, đọc kết quả `"1"`/`"0"` từ [`sc_click`].
///
/// `Ok(false)` = script chạy trót lọt nhưng KHÔNG tìm thấy phần tử — khác hẳn
/// `Err` (script hỏng/Chrome tắt/thiếu quyền), và phải NÓI RA
/// (`browser_click_missed`) chứ không lẫn vào "click trúng nhưng site không
/// phản ứng".
pub fn bam(selector: &str) -> Result<bool, Loi> {
    // 🔴 CHẶN CHỦ ĐỘNG khi màn khoá — KHÔNG chờ macOS tự chặn, vì nó KHÔNG
    // chặn: `execute javascript` đi qua Apple Events, không qua khung hình,
    // nên vẫn chạy được khi màn khoá (đúng cơ chế Terminal `do script` đã đo
    // là "khoá màn không cản"). Đúng lúc `chup_anh` mù (ảnh ra đen) thì phải
    // ĐỒNG THỜI không làm gì — "mù nhưng vẫn động tay được" tệ hơn cả hai thái
    // cực. `Some(false)`/`None` (mở/không đo được) thì đi tiếp như cũ.
    if crate::keys::screen_locked() == Some(true) {
        return Err(Loi::Khac(
            "Màn đang khoá — huba từ chối click để tránh thao tác mà Hà không \
             nhìn thấy được kết quả (ảnh chụp lúc này luôn ra đen, không kiểm \
             lại được). Mở khoá máy rồi thử lại."
                .to_string(),
        ));
    }
    let out = osa(&sc_click(selector))?;
    let trung = out.trim() == "1";
    if trung {
        logging::info("browser_click", json!({ "selector": selector }));
    } else {
        logging::warn("browser_click_missed", json!({ "selector": selector }));
    }
    Ok(trung)
}

/// Điền một ô nhập qua CSS selector, dùng "native value setter" để input kiểu
/// React/controlled-component nhận đúng sự kiện — set `.value` trần không đủ:
/// React theo dõi thay đổi qua setter GỐC của prototype, không phải property
/// đã bị ghi đè trên instance. Cùng hai tầng thoát với [`sc_click`]:
/// `selector`/`value` chỉ là DỮ LIỆU, không bao giờ là mã.
pub fn sc_fill(selector: &str, value: &str) -> String {
    let sel = serde_json::to_string(selector).unwrap_or_else(|_| "null".to_string());
    let val = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    let js = format!(
        "(function(){{var el=document.querySelector({sel});if(!el)return \"0\";var proto=el.tagName===\"TEXTAREA\"?window.HTMLTextAreaElement.prototype:window.HTMLInputElement.prototype;var d=Object.getOwnPropertyDescriptor(proto,\"value\");var setter=d&&d.set;if(setter){{setter.call(el,{val});}}else{{el.value={val};}}el.dispatchEvent(new Event(\"input\",{{bubbles:true}}));el.dispatchEvent(new Event(\"change\",{{bubbles:true}}));return \"1\";}})()"
    );
    script(&format!(
        "    execute front window's active tab javascript {j}",
        j = as_string(&js),
    ))
}

/// Điền, đọc kết quả `"1"`/`"0"` từ [`sc_fill`].
///
/// KHÔNG log `value` — nó có thể mang một mật khẩu chủ máy vừa gõ trên điện
/// thoại; chỉ `selector` là an toàn để lưu lại. `Ok(false)` cũng phải NÓI RA
/// (`browser_fill_missed`), cùng lý do với [`bam`].
pub fn dien(selector: &str, value: &str) -> Result<bool, Loi> {
    // Cùng chốt với `bam` — xem chú thích ở đó. Đứng TRƯỚC `osa()` nên `value`
    // (có thể mang mật khẩu Hà vừa gõ) không bao giờ chạm tới Chrome khi
    // không ai nhìn thấy màn để kiểm lại.
    if crate::keys::screen_locked() == Some(true) {
        return Err(Loi::Khac(
            "Màn đang khoá — huba từ chối điền để tránh thao tác mà Hà không \
             nhìn thấy được kết quả (ảnh chụp lúc này luôn ra đen, không kiểm \
             lại được). Mở khoá máy rồi thử lại."
                .to_string(),
        ));
    }
    let out = osa(&sc_fill(selector, value))?;
    let trung = out.trim() == "1";
    if trung {
        logging::info("browser_fill", json!({ "selector": selector }));
    } else {
        logging::warn("browser_fill_missed", json!({ "selector": selector }));
    }
    Ok(trung)
}
