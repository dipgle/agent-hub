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
//! Cái thứ tư còn tệ hơn, vì nó chưa từng tồn tại: **`setMyCommands`**. hub
//! chưa bao giờ khai lệnh của mình với Telegram, nên gõ `/` trong buồng chat
//! không gợi ý gì và menu ☰ trống trơn — cả một tầng giao diện có sẵn, miễn
//! phí, bỏ không suốt từ đầu.
//!
//! Bảng này giữ đúng những gì Telegram ràng buộc, và test khoá lại: tên lệnh
//! **≤32 ký tự**, chỉ **chữ Latin thường, số, gạch dưới** (tài liệu Bot API,
//! mục *Commands*) — luật ấy không phải sở thích của hub mà là điều kiện để
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
        name: "shot",
        aliases: &["chup"],
        kind: CommandKind::Shot,
        arg: Arg::Rest,
        usage: "[id]",
        help: "Đọc màn đang hiện của phiên",
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
        usage: "[id]",
        help: "Đóng sổ, mở phiên mới nối tiếp",
        listed: false,
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
        help: "Dựng lại hub từ mã hiện tại rồi khởi động lại",
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
         hub tự chèn vào tin — chạm là chạy.",
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
/// `count_of` trả số lượt đã dùng của một route. Hoà nhau thì giữ NGUYÊN thứ tự
/// trong bảng: sắp xếp ổn định, nên menu không nhảy chỗ vì hai lệnh cùng đếm 0 —
/// một cái menu tự đổi chỗ mỗi lượt là cái menu không ai nhớ nổi.
pub fn for_telegram_by_usage(
    count_of: impl Fn(&Route) -> u64,
) -> Vec<(&'static str, &'static str)> {
    let mut rows: Vec<&Route> = ROUTES.iter().filter(|r| r.listed).collect();
    rows.sort_by_key(|r| std::cmp::Reverse(count_of(r)));
    rows.into_iter().map(|r| (r.name, r.help)).collect()
}

/// Route nào mang `kind` này — dùng để quy một lượt chạy về đúng (các) tên lệnh.
pub fn routes_of_kind(kind: CommandKind) -> impl Iterator<Item = &'static Route> {
    ROUTES.iter().filter(move |r| r.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Luật của Telegram, không phải sở thích của hub: tên lệnh ≤32 ký tự, chỉ
    /// chữ thường Latin + số + gạch dưới. Sai luật thì Telegram thôi tô sáng nó
    /// trong tin — mà tô sáng chính là cách hub cho bấm từ trong chữ.
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
