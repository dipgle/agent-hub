//! BẢNG LỆNH — một nguồn sự thật cho mọi thứ liên quan tới một cái tên lệnh.
//!
//! 🔴 Hà 2026-08-14: *"Tại sao không tạo lib lệnh để map khi nhận"*.
//!
//! Không có lý do tốt nào; đó là nợ tích tụ. Trước tệp này, một cái tên lệnh
//! sống ở **ba chỗ rời nhau**: 26 nhánh `match` trong `verbs::parse_command`,
//! một khối chữ `/help` gõ tay trong `pipeline.rs`, và bảng `callback_data` ở
//! `telegram.rs`. Ba chỗ không ai bắt phải giống nhau, nên chúng lệch theo thời
//! gian — và lệch ở đây có một hình dạng rất khó thấy: lệnh vẫn parse, vẫn
//! chạy, chỉ **không ai biết nó tồn tại**. `/pick` là ví dụ sống: nó ra đời
//! sáng nay, hoạt động ngay, và `/help` không hề nhắc tới nó cho tới khi tôi
//! nhớ ra phải sửa tay dòng thứ 27.
//!
//! Cái thứ tư còn tệ hơn, vì nó chưa từng tồn tại: **`setMyCommands`**. huba
//! chưa bao giờ khai lệnh của mình với Telegram, nên gõ `/` trong buồng chat
//! không gợi ý gì và menu ☰ trống trơn — cả một tầng giao diện có sẵn, miễn
//! phí, bỏ không suốt từ đầu.
//!
//! Bảng này giữ đúng những gì Telegram ràng buộc, và test khoá lại: tên lệnh
//! **≤32 ký tự**, chỉ **chữ Latin thường, số, gạch dưới** (tài liệu Bot API,
//! mục *Commands*) — luật ấy không phải sở thích của huba mà là điều kiện để
//! Telegram chịu tô sáng cái tên ấy trong một tin nhắn.

use crate::adapters::CommandKind;

/// Phần chữ đi sau tên lệnh được đọc thế nào.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arg {
    /// Không nhận gì: chữ đi sau bị bỏ qua (`/doctor`, `/sessions`).
    None,
    /// Nguyên văn phần còn lại, được phép rỗng (`/session [id]`, `/close [id]`).
    Rest,
    /// Nguyên văn phần còn lại, và **bắt buộc** (`/win <lệnh>`, `/key <phím>`).
    RestRequired,
    /// Luật riêng, `parse_command` tự xử — khai ở đây để `/help` và
    /// `setMyCommands` vẫn thấy nó (`/new` đọc cờ, `/runin` đòi id đứng trước).
    Custom,
    /// Tham số CỐ ĐỊNH, gõ sẵn trong bảng: cái tên lệnh đã là toàn bộ ý định.
    ///
    /// 🔴 Hà 2026-08-17: *"Thêm vào menu lệnh để bấm chọn nhanh: type right;
    /// type enter"* · *"Để bấm gửi luôn trong ô chờ mờ"* · *"Đỡ phải chèn nút"*.
    ///
    /// Menu ☰ của Telegram chỉ khai được TÊN lệnh, không khai được tham số —
    /// nên `/key right` không bao giờ vào menu được. Một route mang sẵn tham số
    /// thì vào được, và đó là cách rẻ nhất để thay một cái nút: menu luôn ở đó,
    /// không chiếm dòng nào trong tin, không phụ thuộc tin nào còn trên màn.
    Fixed(&'static str),
}

/// Một lệnh: tên, các tên gọi khác, việc nó làm, và câu mô tả DUY NHẤT.
pub struct Route {
    /// Tên chính — cũng là tên đăng ký với Telegram. Phải hợp lệ (xem test).
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub kind: CommandKind,
    pub arg: Arg,
    /// Cú pháp hiện trong `/help`, ví dụ `<câu hỏi>` hay `[id]`.
    pub usage: &'static str,
    /// Một câu, dùng cho CẢ `/help` LẪN menu lệnh của Telegram.
    pub help: &'static str,
    /// Có khai với Telegram không. Lệnh ít dùng để `false` cho menu khỏi loãng —
    /// nó vẫn parse như thường, chỉ không nằm trong danh sách gợi ý.
    pub listed: bool,
}

/// Mọi lệnh phòng chat hiểu. Thứ tự ở đây là thứ tự hiện trong `/help`.
pub const ROUTES: &[Route] = &[
    Route {
        name: "session",
        aliases: &["phien"],
        kind: CommandKind::Session,
        arg: Arg::Rest,
        usage: "[id]",
        help: "Trống = cửa sổ đang chạy CLI; kèm id để theo; '-' để thôi theo",
        listed: true,
    },
    // 🔴 Hà 2026-08-14: *"trong ds lệnh bỏ sessions đi vì lệnh session trống
    // thay thế rồi"*. Bỏ khỏi DANH SÁCH, không bỏ khỏi bộ phân tích: hai dòng
    // gần giống nhau trong một menu bảy chữ thì tốn chỗ và bắt người đọc so
    // từng chữ, nhưng `/sessions` đã nằm trong tay quen (và trong mấy trăm tin
    // cũ) — gỡ hẳn là biến một lệnh đang chạy thành "không hiểu lệnh này".
    Route {
        name: "sessions",
        aliases: &["phiens", "danhsach"],
        kind: CommandKind::Session,
        arg: Arg::None,
        usage: "",
        help: "Danh sách phiên đang sống (như /session trống)",
        listed: false,
    },
    Route {
        name: "ctrlc",
        // 🔴 `ctrl_c`, KHÔNG phải `ctrl-c` — sửa 2026-08-30. Tên lệnh Telegram chỉ
        // nhận chữ thường + số + gạch DƯỚI, nên bản 27/08 làm
        // `every_command_name_is_one_telegram_will_highlight` ĐỎ ngay lượt cài, và
        // nó nằm đỏ ba ngày vì lượt ấy chỉ chạy "10 suite vùng ảnh hưởng" chứ
        // không chạy suite của thư viện. Cái giá thật không nằm ở bài kiểm: một
        // cái tên sai luật thì Telegram thôi tô sáng nó, mà tô sáng chính là cách
        // huba cho bấm từ trong chữ.
        //
        // Dấu gạch NGANG vẫn sống ở `Arg::Fixed("ctrl-c")` ngay dưới, và ở đó nó
        // đúng: đấy là tên PHÍM gửi cho `keys`, không phải tên lệnh cho Telegram.
        aliases: &["ctrl_c", "refresh", "lamtuoi"],
        kind: CommandKind::Key,
        // 🔴 `Arg::Fixed`, KHÔNG phải một route có handler riêng — và đó là cả
        // bài học của ngày 27/08. Bản trước dựng hẳn `CommandKind::Refresh` +
        // một nhánh xử lý riêng để nới cửa sổ; Hà thử rồi bác: *"Cách bạn xử lý
        // màn bị treo không được, cứ để nó là lệnh ctrl+c cho tôi"*. Thứ anh
        // muốn là phím anh vẫn bấm khi ngồi trước máy — mà `/key` đã gửi phím
        // được từ lâu, nên việc duy nhất còn thiếu là một cái TÊN gọi nó ra.
        // Menu ☰ của Telegram không khai được tham số, nên `/key ctrl-c` không
        // vào menu được; một route mang sẵn tham số thì vào được.
        arg: Arg::Fixed("ctrl-c"),
        usage: "",
        help: "Gửi Ctrl+C vào phiên — NGẮT lượt đang chạy dở",
        listed: true,
    },
    Route {
        name: "shot",
        aliases: &["chup"],
        kind: CommandKind::Shot,
        arg: Arg::Rest,
        usage: "[id]",
        help: "Đọc màn đang hiện của phiên",
        listed: true,
    },
    Route {
        // 🔴 `focus` là alias, không phải tên chính — nhưng nó là từ chủ máy
        // thật sự gõ. Đo được: `telegram_command_queued {"head":"/focus"}` lúc
        // 2026-08-22T03:36:46Z, rồi `telegram_not_a_command` 12 giây sau. Tên
        // chính để `front` vì nó nói ĐÚNG việc (đưa ra trước mặt), còn alias
        // nhận lấy cái từ người ta với tay tới.
        name: "front",
        aliases: &["focus", "truoc"],
        kind: CommandKind::Front,
        arg: Arg::Rest,
        usage: "[id]",
        help: "Đưa cửa sổ của phiên ra trước mặt (không chụp, không gõ gì)",
        listed: true,
    },
    Route {
        name: "anh",
        aliases: &["photo", "screenshot"],
        kind: CommandKind::Photo,
        arg: Arg::Rest,
        usage: "[id]",
        help: "ẢNH THẬT của màn hình (đưa cửa sổ phiên ra trước rồi chụp)",
        listed: true,
    },
    Route {
        name: "ask",
        aliases: &["hoi"],
        kind: CommandKind::Ask,
        arg: Arg::Custom,
        usage: "<câu hỏi>",
        help: "Hỏi bên lề, phiên gốc không bị đụng",
        listed: true,
    },
    Route {
        name: "new",
        aliases: &["moi"],
        kind: CommandKind::New,
        // 🔴 `Custom` → `Rest` ngày 2026-08-14: đề bài KHÔNG bắt buộc. Chạm vào
        // dòng này trong menu Telegram chỉ gửi đúng `/new`, nên một lệnh
        // `listed: true` mà đòi tham số là một lệnh không bấm được.
        arg: Arg::Rest,
        // 🔴 Hà 2026-08-15: *"lệnh sẽ như thế này `/new [acc] [text]`"*. Mỗi
        // tham số thêm một BƯỚC, và đó là cả cấu trúc: trống = cửa sổ trần ·
        // kèm tài khoản = dựng CLI · kèm chữ = gõ đề bài vào.
        usage: "[acc] [việc]",
        help: "Trống = cửa sổ trần · +acc = dựng CLI · +việc = gõ luôn đề bài",
        listed: true,
    },
    Route {
        name: "type",
        aliases: &["go"],
        kind: CommandKind::Type,
        arg: Arg::RestRequired,
        usage: "<chữ>",
        help: "Gõ chữ vào cửa sổ phiên",
        listed: false,
    },
    Route {
        name: "key",
        aliases: &["phim"],
        kind: CommandKind::Key,
        arg: Arg::RestRequired,
        usage: "<up|down|left|right|enter|esc|tab|space|1-9>",
        help: "Bấm một phím vào cửa sổ phiên",
        listed: false,
    },
    // Hai phím dùng nhiều nhất, mỗi phím một cái tên gõ được và một chỗ trong
    // menu ☰ — xem `Arg::Fixed`. Chúng đi ĐÚNG route `/key`, không đẻ nhánh xử
    // lý nào: cùng phép đọc màn, cùng cổng an toàn (mũi tên chỉ gửi khi chứng
    // minh được màn không có hộp chọn).
    Route {
        name: "enter",
        aliases: &["gui"],
        kind: CommandKind::Key,
        arg: Arg::Fixed("enter"),
        usage: "",
        help: "Gửi chữ đang nằm trong ô nhập của phiên",
        listed: true,
    },
    Route {
        name: "right",
        aliases: &["goiy"],
        kind: CommandKind::Key,
        arg: Arg::Fixed("right"),
        usage: "",
        help: "Nhận gợi ý mờ vào ô nhập (→), rồi /enter để gửi",
        listed: true,
    },
    Route {
        name: "pick",
        aliases: &["chon"],
        kind: CommandKind::Pick,
        arg: Arg::RestRequired,
        usage: "<câu>.<lựa chọn>",
        help: "Trả lời một câu của bảng hỏi nhiều câu",
        listed: true,
    },
    Route {
        name: "tab",
        aliases: &["cau"],
        kind: CommandKind::Tab,
        arg: Arg::RestRequired,
        usage: "<số>",
        help: "Sang tab (câu) ấy của bảng hỏi — chỉ đi, không chọn",
        listed: true,
    },
    Route {
        name: "clean",
        aliases: &["don", "xoacho"],
        kind: CommandKind::Clean,
        arg: Arg::Rest,
        usage: "[id]",
        help: "Xoá hàng chờ VÀ ô nhập của phiên",
        listed: true,
    },
    Route {
        name: "clear",
        aliases: &["xoao", "xoaonhap"],
        kind: CommandKind::Clear,
        arg: Arg::Rest,
        usage: "[id]",
        help: "Chỉ xoá chữ trong ô nhập — hàng chờ giữ nguyên",
        listed: true,
    },
    Route {
        name: "stop",
        aliases: &["dung"],
        kind: CommandKind::Stop,
        arg: Arg::Rest,
        usage: "[id]",
        help: "Dừng phiên nền, hội thoại vẫn giữ",
        listed: false,
    },
    Route {
        name: "close",
        aliases: &["dong", "dongphien"],
        kind: CommandKind::Close,
        arg: Arg::Rest,
        usage: "[id]",
        help: "Đóng hẳn phiên và cửa sổ của nó",
        listed: false,
    },
    Route {
        name: "handover",
        aliases: &["bangiao"],
        kind: CommandKind::Handover,
        arg: Arg::Rest,
        usage: "[-a acc] [id]",
        // 🔴 `listed: true` từ 28/08. Nó ở ngoài menu suốt vì "ít dùng", mà
        // chính sự vô hình ấy là thứ `CLAUDE.md` đã ghi một lần rồi ở `/win`:
        // con số "0 lượt dùng" đo SỰ VÔ HÌNH, không đo sự vô dụng. Nay nó là
        // đường thoát khi một tài khoản hết hạn mức — đúng lúc chủ máy đang ở
        // xa và cần tìm ra nó trong menu.
        help: "Đóng sổ phiên, mở phiên mới nối tiếp (-a acc2 để đổi tài khoản)",
        listed: true,
    },
    Route {
        name: "runin",
        aliases: &[],
        kind: CommandKind::RunIn,
        arg: Arg::Custom,
        usage: "<id> <dòng lệnh>",
        help: "Máy chạy, kết quả dán vào phiên ấy",
        listed: false,
    },
    Route {
        name: "upgrade",
        aliases: &["capnhat"],
        kind: CommandKind::Upgrade,
        arg: Arg::None,
        usage: "",
        help: "Dựng lại huba từ mã hiện tại rồi khởi động lại",
        listed: true,
    },
    Route {
        name: "terminal",
        aliases: &["win", "cuaso", "tty"],
        kind: CommandKind::Win,
        // 🔴 `Rest`, không phải `RestRequired`: trơn = XEM DANH SÁCH (Hà
        // 2026-08-15). `RestRequired` trả `None` cho `/terminal` trơn — tức gõ
        // đúng tên một route rồi nhận lại sự im lặng.
        arg: Arg::Rest,
        usage: "",
        help: "Liệt kê cửa sổ Terminal trần (không chạy gì). Mở mới: /new",
        listed: true,
    },
    Route {
        name: "web",
        aliases: &["browser", "trinhduyet"],
        kind: CommandKind::Web,
        // `Rest`, cùng lý do với `terminal`: gõ trơn = XEM, và một route
        // `listed: true` thì "gõ trơn" là cách một ngón tay chạm tới được.
        arg: Arg::Rest,
        usage: "[địa chỉ | <cửa sổ>.<tab> | an …]",
        help: "Chrome TRÊN MÁY: trống = các tab; địa chỉ = mở. `an` = trình duyệt ẩn của huba",
        listed: true,
    },
    Route {
        name: "accounts",
        aliases: &["acc", "taikhoan"],
        kind: CommandKind::Accounts,
        arg: Arg::None,
        usage: "",
        help: "Các tài khoản Claude trên máy",
        listed: true,
    },
    Route {
        name: "set",
        aliases: &["cauhinh"],
        kind: CommandKind::SetConfig,
        // Luật riêng: đòi CẢ khoá lẫn giá trị. `/set autonomy.default` (thiếu
        // vế sau) phải KHÔNG parse — một lệnh đổi cấu hình mà nuốt nửa câu là
        // một lệnh đổi cấu hình sang giá trị rỗng. Test `tfl5.rs` giữ chỗ này.
        arg: Arg::Custom,
        usage: "<khoá> <giá trị>",
        help: "Đổi một mục cấu hình",
        listed: false,
    },
    Route {
        name: "doctor",
        aliases: &["health"],
        kind: CommandKind::Doctor,
        arg: Arg::None,
        usage: "",
        help: "Kiểm kênh, khoá, công cụ",
        listed: true,
    },
    // 🔴 `/ingest` (`/poll`) đã bỏ 2026-08-14: nó đọc PHÒNG CHAT, và phòng chat
    // đi rồi. Telegram không có gì để đọc-ngay — nó tự đẩy tới. Bỏ hẳn khỏi bảng
    // chứ không để `listed: false`: một lệnh ẩn vẫn là một lệnh gõ được, và nó
    // sẽ trả lời bằng một câu vô nghĩa.
    Route {
        name: "run",
        aliases: &["cycle"],
        kind: CommandKind::Run,
        arg: Arg::None,
        usage: "",
        help: "Chạy một vòng ngay",
        listed: false,
    },
    Route {
        name: "help",
        aliases: &["?"],
        kind: CommandKind::Help,
        arg: Arg::None,
        usage: "",
        help: "Danh sách lệnh",
        listed: true,
    },
];

/// Tra một động từ (đã hạ chữ thường, không có dấu `/`) ra route của nó.
pub fn lookup(verb: &str) -> Option<&'static Route> {
    ROUTES
        .iter()
        .find(|r| r.name == verb || r.aliases.contains(&verb))
}

/// Chữ cho `/help`, sinh TỪ BẢNG — nên một lệnh mới không thể ra đời mà thiếu
/// dòng của nó, và không dòng nào tả một lệnh đã chết.
pub fn help_text() -> String {
    let mut out = String::from("Lệnh dùng được trong phòng này:\n");
    for r in ROUTES {
        out.push_str(&format!("/{}", r.name));
        if !r.usage.is_empty() {
            out.push(' ');
            out.push_str(r.usage);
        }
        out.push_str(" — ");
        out.push_str(r.help);
        out.push('\n');
    }
    out.push_str(
        "\nChọn phiên xong thì CHỮ THƯỜNG gõ ở đây đi thẳng vào phiên ấy.\n\
         Lệnh dạng /pick_<id>_<câu>_<lựa chọn> và /run_<n> là chữ chạm-được \
         huba tự chèn vào tin — chạm là chạy.",
    );
    out
}

/// Danh sách khai với Telegram (`setMyCommands`).
pub fn for_telegram() -> Vec<(&'static str, &'static str)> {
    ROUTES
        .iter()
        .filter(|r| r.listed)
        .map(|r| (r.name, r.help))
        .collect()
}

/// Cùng danh sách ấy, **xếp theo tần suất dùng** — nhiều nhất lên đầu.
///
/// 🔴 Hà 2026-08-17: *"Menu có sắp xếp tự động theo tần suất tương tác được
/// không"*. Được, và rẻ: Telegram hiện menu ☰ **đúng thứ tự** danh sách gửi lên
/// `setMyCommands`, nên "sắp xếp lại menu" chỉ là gửi lại danh sách theo thứ tự
/// khác — một lượt HTTP, và chỉ khi thứ tự thật sự đổi.
///
/// `score_of` trả ĐIỂM dùng của một route. Hoà nhau thì giữ NGUYÊN thứ tự trong
/// bảng: sắp xếp ổn định, nên menu không nhảy chỗ vì hai lệnh cùng đếm 0 — một
/// cái menu tự đổi chỗ mỗi lượt là cái menu không ai nhớ nổi.
///
/// 🔴 Câu ấy viết ra 17/08 và nó ĐÚNG, chỉ hụt đúng một chữ: nó lo cho hoà NHAU
/// mà quên SÁT nhau. Hà 2026-08-19: *"menu đang theo flow nào mà tôi thấy cứ
/// nhảy loạn"*. Nên hàm này thôi tự quyết thứ tự — nó trả về ĐIỂM kèm hàng, và
/// chỗ gọi (`pipeline::menu_settled_order`) mới là nơi quyết có đổi chỗ hay
/// không. Xếp hạng và HÃM xếp hạng là hai việc, tách ra mới đo được từng cái.
pub fn for_telegram_scored(
    score_of: impl Fn(&Route) -> u64,
) -> Vec<(&'static str, &'static str, u64)> {
    let mut rows: Vec<(&'static str, &'static str, u64)> = ROUTES
        .iter()
        .filter(|r| r.listed)
        .map(|r| (r.name, r.help, score_of(r)))
        .collect();
    rows.sort_by_key(|(_, _, s)| std::cmp::Reverse(*s));
    rows
}

/// Route nào mang `kind` này — dùng để quy một lượt chạy về đúng (các) tên lệnh.
pub fn routes_of_kind(kind: CommandKind) -> impl Iterator<Item = &'static Route> {
    ROUTES.iter().filter(move |r| r.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Luật của Telegram, không phải sở thích của huba: tên lệnh ≤32 ký tự, chỉ
    /// chữ thường Latin + số + gạch dưới. Sai luật thì Telegram thôi tô sáng nó
    /// trong tin — mà tô sáng chính là cách huba cho bấm từ trong chữ.
    #[test]
    fn every_command_name_is_one_telegram_will_highlight() {
        for r in ROUTES {
            for n in std::iter::once(&r.name).chain(r.aliases.iter()) {
                // `?` là lối gõ tắt cũ, không khai với Telegram — nhưng vẫn phải
                // parse được, nên nó là ngoại lệ DUY NHẤT và có tên ở đây.
                if *n == "?" {
                    continue;
                }
                assert!(n.len() <= 32, "tên quá dài: {n}");
                assert!(
                    n.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                    "tên có ký tự Telegram không nhận: {n}"
                );
            }
        }
    }

    /// 🔴 Cái từ chủ máy GÕ RA phải dẫn tới đâu đó.
    ///
    /// Đo được, không suy: `telegram_command_queued {"head":"/focus"}` lúc
    /// 2026-08-22T03:36:46Z, rồi `telegram_not_a_command` 12 giây sau. Hà gõ
    /// `/focus` trên điện thoại vì đó là từ tự nhiên cho việc *"đưa phiên ấy nổi
    /// lên"*, và huba không có route nào tên thế.
    ///
    /// Bài kiểm khoá CẢ HAI tên: `front` (tên chính, nói đúng việc) và `focus`
    /// (từ người ta với tay tới). Bỏ alias đi là làm đỏ một bài kiểm CÓ CHỦ,
    /// không phải dọn một dòng thừa.
    #[test]
    fn front_and_focus_both_lead_somewhere() {
        for name in ["front", "focus"] {
            let r = lookup(name).unwrap_or_else(|| {
                panic!("`/{name}` không dẫn tới route nào — đúng ca 22/08, xem chú thích")
            });
            assert_eq!(r.kind, CommandKind::Front, "`/{name}` phải là route Front");
            assert_eq!(r.arg, Arg::Rest, "`/{name} [id]` — id được phép rỗng");
        }
        // Và nó phải VÀO MENU: một route `listed: false` thì chủ máy không có
        // cách nào biết nó tồn tại — đúng bài học đã trả giá với `win`
        // (0 lượt dùng suốt 3 tuần vì vô hình, không vì vô dụng).
        assert!(
            lookup("front").is_some_and(|r| r.listed),
            "`/front` phải nằm trong menu ☰"
        );
    }

    #[test]
    fn no_two_routes_answer_to_the_same_name() {
        let mut seen: Vec<&str> = Vec::new();
        for r in ROUTES {
            for n in std::iter::once(&r.name).chain(r.aliases.iter()) {
                assert!(!seen.contains(n), "tên trùng: {n}");
                seen.push(n);
            }
        }
    }

    #[test]
    fn help_lists_every_route_and_lookup_finds_them_all() {
        let h = help_text();
        for r in ROUTES {
            assert!(
                h.contains(&format!("/{}", r.name)),
                "thiếu trong help: {}",
                r.name
            );
            assert_eq!(lookup(r.name).map(|x| x.name), Some(r.name));
            for a in r.aliases {
                assert_eq!(lookup(a).map(|x| x.name), Some(r.name), "alias {a}");
            }
        }
        assert!(lookup("khongcolenhnay").is_none());
    }
}
