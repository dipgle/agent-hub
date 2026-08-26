//! Bộ phân tích MỆNH LỆNH: chữ người gõ → một route.
//!
//! 🔴 Tách khỏi `adapters/tfl5.rs` ngày 2026-08-14, khi Hà chốt bỏ hẳn trang
//! tfl5: *"tạm thời không dùng tfl5 để xem cứ xóa hết đi"*.
//!
//! Hàm này nằm nhầm chỗ suốt từ đầu — nó mang tên một KÊNH, trong khi Telegram,
//! phòng chat và cả `/help` đều gọi nó. Bỏ kênh mà không tách trước là bỏ luôn
//! bộ phân tích của kênh còn lại; đó là lý do lượt cắt bắt đầu từ đây chứ không
//! từ chỗ dễ thấy nhất.
//!
//! Ở đây không có mạng, không có kênh: vào là chữ, ra là `(route, id, phần còn
//! lại)`. Nhờ thế các bài kiểm của nó chạy được mà không cần một máy chủ nào —
//! và đó cũng là lý do chúng còn nguyên vẹn sau khi cả cái kênh biến mất.

use crate::adapters::CommandKind;

/// BÀN PHÍM THƯỜNG TRỰC dưới ô nhập: `(nhãn nút, lệnh nó gửi)`.
///
/// 🔴 Hà 2026-08-26: *"sao pin msg không bấm được nút trực tiếp ở trên à, nó đang
/// cuộn tới tin đó không hợp lý lắm"*.
///
/// Câu trả lời là một giới hạn của Telegram, không phải chỗ huba làm thiếu: BĂNG
/// GIM ở đỉnh buồng chat là chỗ **chỉ để hiện chữ**. Không có API nào gắn nút vào
/// nó, và chạm vào băng thì client luôn nhảy tới tin gốc. Nên dù tin gim mang
/// `inline_keyboard` hay mang cả dòng là một liên kết, từ trên đỉnh vẫn chỉ ra
/// đúng một cú cuộn.
///
/// Thứ Telegram CHO PHÉP luôn nằm trong tầm tay, một chạm, không cuộn là
/// `ReplyKeyboardMarkup` — bàn phím nằm ngay trên ô nhập, sống qua mọi tin cho
/// tới khi có bàn phím khác thay. Nó gửi CHỮ chứ không gửi callback, nên mỗi nút
/// là đúng một route đã có; không đẻ đường xử lý mới.
///
/// ⚠ **MỘT bảng, hai chỗ đọc**: `telegram::persistent_keyboard` dựng nút từ đây,
/// `parse_command` dịch ngược cũng từ đây. Hai bản chép là hai bản sẽ lệch, và
/// lúc lệch thì nút hiện ra nhưng bấm vào huba trả lời *"Chưa hiểu lệnh này"* —
/// đúng con bug `/key enter` đã trả giá sáng cùng ngày. `tests/keyboard.rs` khoá
/// vòng tròn ấy lại.
pub const KEYBOARD: &[(&str, &str)] = &[("📷 Xem màn", "/shot"), ("📋 Phiên", "/session")];

/// `ttys014` — tên một tty như Terminal khai, đã bỏ `/dev/`.
///
/// Hẹp có chủ ý: payload đi thẳng vào `win-<tty>` rồi thành id phiên, nên nhận
/// bừa một chuỗi lạ ở đây là để nó chạy tiếp xuống tận chỗ tra cửa sổ.
fn is_tty_name(s: &str) -> bool {
    s.len() > 4 && s.starts_with("ttys") && s[4..].chars().all(|c| c.is_ascii_alphanumeric())
}

/// Một mệnh lệnh gõ trên kênh, nếu chữ này là lệnh.
///
/// 🔴 **Cổng người đã rời khỏi hàm này**, 2026-08-14, cùng lượt gỡ tfl5. Trước
/// đó nó nhận thêm `(from_user_tid, owner_tids)` và từ chối người lạ — đúng khi
/// còn một PHÒNG CHAT mà ai vào cũng gõ được. Telegram không có hình dạng ấy:
/// cổng của nó là `chat_id`, và `telegram.rs` đã bỏ mọi tin từ buồng khác
/// (`:1326` cho chữ, `:1731` cho nút) trước khi có gì tới được đây.
///
/// Nên chỗ gọi duy nhất phải tự bịa ra người gõ để đi qua chính cái cổng ấy:
/// lấy `first()` của danh sách chủ rồi đem so với danh sách. Một cổng được dựng
/// sao cho không bao giờ từ chối được **trừ khi danh sách rỗng** — và khi ấy nó
/// từ chối MỌI mệnh lệnh, im lặng, chỉ để lại một dòng nhật ký. Tức là cái bẫy
/// nằm đúng ở chỗ nó nhìn giống bảo mật nhất.
///
/// Luật thì không đổi, chỉ đổi chỗ đứng: **một cổng người, ở KÊNH**. Đặt hai
/// cổng ở hai tầng cho cùng một câu hỏi là cách chắc chắn để một hôm nào đó
/// chúng trả lời khác nhau.
///
/// Ở đây không có mạng, không có kênh, và nay cũng không có ai: vào là chữ, ra
/// là `(route, id, phần còn lại)`.
pub fn parse_command(text: &str) -> Option<(CommandKind, i64, String)> {
    let t = text.trim();
    // Nút bàn phím thường trực gửi NHÃN, không gửi lệnh — dịch về đúng route đã
    // có (xem [`KEYBOARD`]). Đặt TRƯỚC cửa `starts_with('/')`, vì nhãn có emoji
    // và không mở đầu bằng dấu gạch chéo.
    let t = KEYBOARD
        .iter()
        .find(|(nhan, _)| *nhan == t)
        .map_or(t, |(_, lenh)| *lenh);
    if !t.starts_with('/') {
        return None;
    }
    let mut parts = t[1..].splitn(3, char::is_whitespace);
    let verb = parts.next().unwrap_or("").to_lowercase();
    // ⭐ LỆNH TỰ TÔ SÁNG — tham số nằm trong TÊN lệnh, không phải sau dấu cách.
    //
    // 🔴 Hà 2026-08-14: *"Sao không dùng Deep Links để định dạng bên trong nội
    // dung văn bản như khối lệnh thay vì tạo 1 cái nút rất khó hiểu"* →
    // *"Hạn chế dùng khối nút ở cuối tin"*.
    //
    // Anh chỉ đúng chỗ tôi đã kết luận sai: tôi bảo Telegram không đặt nút giữa
    // chữ được — đúng với `inline_keyboard`, nhưng nó KHÔNG phải cách duy nhất
    // để bấm. Tài liệu Bot API (mục *Commands*) nói thẳng: *"Highlight commands
    // in messages. When the user taps a highlighted command, that command is
    // immediately sent again."* Tức chỉ cần IN `/lệnh` vào giữa câu là nó thành
    // thứ chạm được, đứng đúng chỗ nó nói tới.
    //
    // Ràng buộc là cả thiết kế: tên lệnh **≤32 ký tự, chỉ Latin/số/gạch dưới**,
    // và chạm chỉ gửi lại ĐÚNG token lệnh — chữ sau dấu cách rơi mất. Nên tham
    // số phải nằm trong tên: `/pick_4963b95c_2_1` (18 ký tự) mang đủ phiên, câu
    // và lựa chọn; `/run_0` trỏ vào sổ lệnh thay vì chép dòng lệnh vào nhãn —
    // và đó cũng là lý do nó không thể bị cắt cụt như một cái nhãn nút.
    /// Mẩu id trong một đích-chạm có hợp lệ không.
    ///
    /// 🔴 CỬA SỔ TRẦN CŨNG LÀ MỘT ĐÍCH — Hà 2026-08-19, ảnh `/shot` cửa sổ
    /// `ttys002` đang hỏi *"Bypass Permissions mode"* với đúng hai lựa chọn, hai
    /// dấu ☑ hiện rành rành: *"Sao khong bam chon được"*. Log trả lời trong một
    /// dòng: `telegram_not_a_command {"head":"/start k_win-ttys_2"}`.
    ///
    /// Hai lỗi chồng lên nhau, và cả hai đều từ giả định *"id nào cũng là uuid"*:
    /// `SessionData::short()` cắt 8 ký tự đầu nên `win-ttys002` thành
    /// `win-ttys` — **mất số tty**, tức mất luôn cái phân biệt cửa sổ này với
    /// cửa sổ khác; rồi bộ đọc đòi 8 ký tự HEX nên `win-ttys` không lọt. Kết
    /// quả: huba VẼ RA hai cái ☑ mà không đường nào nhận chúng — đúng hình dạng
    /// "một cái nút không dẫn vào đâu" mà luật 14 cấm.
    ///
    /// Nên id cửa sổ đi NGUYÊN (`win-ttys002`, 11 ký tự, vẫn thừa chỗ trong 32
    /// ký tự tên lệnh và hợp bộ ký tự deep-link `A-Za-z0-9_-`).
    fn sid_ok(sid: &str) -> bool {
        if sid.is_empty() {
            return false;
        }
        crate::sessions::is_shell_id(sid) || sid.chars().all(|c| c.is_ascii_hexdigit())
    }
    if let Some(rest) = verb.strip_prefix("pick_") {
        // `pick_<8 ký tự đầu id>_<câu>_<lựa chọn>`
        let f: Vec<&str> = rest.split('_').collect();
        if f.len() == 3 && !f[0].is_empty() {
            return Some((CommandKind::Pick, 0, format!("{} {}.{}", f[0], f[1], f[2])));
        }
    }
    // `tab_<8 ký tự đầu id>_<số>` — SANG tab ấy, không chọn gì.
    //
    // 🔴 Hà 2026-08-19: *"có nút bấm ở chính tab để nhận như click chuột"*. Cùng
    // khuôn với `pick_`: tham số nằm trong TÊN lệnh, vì chạm một lệnh tô sáng
    // chỉ gửi lại đúng token lệnh — chữ sau dấu cách rơi mất.
    if let Some(rest) = verb.strip_prefix("tab_") {
        if let Some((sid, n)) = rest.split_once('_') {
            let ok_sid = sid_ok(sid);
            let ok_n = !n.is_empty() && n.chars().all(|c| c.is_ascii_digit());
            if ok_sid && ok_n {
                return Some((CommandKind::Tab, 0, format!("{sid} {n}")));
            }
        }
    }
    // `/start <payload>` — đường VỀ của deep link. Bấm icon `▶️` trong chữ là
    // Telegram gửi đúng câu này, nên payload phải được cởi ra thành lệnh thật.
    // Không có nhánh này thì cái icon chỉ mở lại buồng chat rồi thôi.
    if verb == "start" {
        let payload = t[1..]
            .split_once(char::is_whitespace)
            .map(|(_, r)| r.trim().to_string())
            .unwrap_or_default();
        if payload.is_empty() {
            return None;
        }
        // Đi lại đúng bộ phân tích này, chỉ khác cái vỏ: `/start pick_x_1_2`
        // phải cho ra y hệt `/pick_x_1_2` gõ tay. Một đường, một luật.
        return parse_command(&format!("/{payload}"));
    }
    // `send_<8 ký tự đầu id>` — gửi bảng đi (một dấu Enter vào đúng cửa sổ ấy).
    // Nó là `/key <id> enter` viết dưới dạng CHẠM ĐƯỢC: `/key` có tham số đứng
    // sau dấu cách, mà chạm thì chỉ gửi lại token lệnh — chữ sau rơi mất.
    if let Some(sid) = verb.strip_prefix("send_") {
        if sid_ok(sid) {
            return Some((CommandKind::Key, 0, format!("{sid} enter")));
        }
    }
    // `k_<8 ký tự đầu id>_<số>` — bấm một LỰA CHỌN của đúng phiên ấy.
    //
    // 🔴 Hà 2026-08-16: *"nút chọn phải chèn ngay tại các dòng chọn tại chính
    // chỗ option chứ không phải ném thêm xuống cuối"*. Một cái nút chỉ đặt được
    // dưới đáy tin; thứ đặt được ngay sau dòng "1. …" là một LIÊN KẾT, và liên
    // kết thì phải tự mang đủ phiên + số.
    if let Some(rest) = verb.strip_prefix("k_") {
        if let Some((sid, n)) = rest.split_once('_') {
            let ok_sid = sid_ok(sid);
            let ok_n = n.len() == 1 && n.chars().all(|c| c.is_ascii_digit()) && n != "0";
            if ok_sid && ok_n {
                return Some((CommandKind::Key, 0, format!("{sid} {n}")));
            }
        }
    }
    // `clr_<8 ký tự đầu id>` — XOÁ ô nhập của đúng phiên ấy.
    //
    // 🔴 Hà 2026-08-16: *"còn lăn tăn nó là text mờ hay tỏ thì thêm 1 nút xóa
    // bên cạnh nữa để tự thao tác"*. Đúng chỗ tôi bí: đọc màn về thì chữ mất
    // màu, nên huba không phân biệt được **chữ chủ máy gõ** với **gợi ý mờ** TUI
    // tự bày. Tôi đã lấy đó làm lý do để KHÔNG làm gì — mà người ngồi trước máy
    // thì nhìn một cái là biết. Vậy đừng bắt huba đoán: đưa cả hai đường ra, ai
    // nhìn thấy thì người ấy quyết.
    //
    // Mã phiên nằm TRONG chính cái liên kết (`clr_<sid>`), không lấy theo con
    // trỏ: một tin cũ bấm lại vẫn phải chạm đúng phiên của nó — cùng bài học
    // với `quick_token`.
    if let Some(sid) = verb.strip_prefix("clr_") {
        if sid_ok(sid) {
            return Some((CommandKind::Key, 0, format!("{sid} clear")));
        }
    }
    // `run_<mã>` — bấm icon ▶️ trong chữ. Mã là thứ `pipeline::quick_token` sinh
    // ra: **8 ký tự HEX** (`format!("{h:08x}")`), không phải một số thứ tự.
    //
    // 🔴 Hà 2026-08-16: *"Có mỗi vấn đề nút lệnh này làm mãi không xong"*, kèm
    // ảnh huba đáp *"Chưa hiểu lệnh này"* cho chính cái icon nó vừa gửi. Log nói
    // đúng thủ phạm trong một dòng: `telegram_not_a_command {"head":"/start
    // run_d1704560"}` — nhánh này đòi `is_ascii_digit`, mà `d1704560` có chữ
    // `d`. Nên **gần như MỌI** mã đều rớt: 8 chữ số hex mà không dính lấy một
    // chữ cái a–f là chuyện hiếm (~1/2000).
    //
    // Nó lệch từ lượt đổi nút-theo-chỉ-số sang nút-mang-mã-riêng (`quick_token`,
    // 2026-08-15, để nút của tin cũ không chạy việc của tin mới): một đầu đổi
    // sang hex, đầu đọc ở lại với chữ số, và **bài kiểm round-trip vẫn xanh vì
    // nó tự chọn `run_0`** — một hình dạng không còn ai sinh ra. Nay bài kiểm ấy
    // lấy mã từ chính `quick_token`, nên hai đầu không lệch được nữa mà không ai
    // biết.
    if let Some(n) = verb.strip_prefix("run_") {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some((CommandKind::RunQuick, 0, n.to_string()));
        }
    }
    // `term_<mã>` — CÙNG dòng lệnh ấy, nhưng mở một CỬA SỔ Terminal riêng và gõ
    // nó vào đó, thay vì huba chạy rồi dán kết quả ngược vào phiên.
    //
    // 🔴 Hà 2026-08-16: *"kiếm 1 cái icon terminal để biết nó là bấm chạy
    // terminal riêng chứ không phải chạy xong rồi gửi ngược vào phiên, nên tách
    // thành 2 nút này để người dùng chủ động chọn"*.
    //
    // Hai cách chạy KHÁC NHAU THẬT, và trước lượt này chỉ có một: `▶️` chạy
    // bằng `/bin/zsh -lc` của huba, chờ tới khi xong, rồi dán bản tóm tắt vào
    // phiên. Cách ấy đúng cho một lệnh ngắn có kết quả đáng đọc, và SAI cho một
    // lệnh dài, một lệnh hỏi lại, hay một lệnh chủ máy muốn ngồi nhìn — những
    // thứ mà ngồi ở máy thì người ta mở một cửa sổ. Đúng phép thử cầu nối.
    if let Some(n) = verb.strip_prefix("term_") {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some((CommandKind::RunInTerminal, 0, n.to_string()));
        }
    }
    // `w_<tty>` / `wx_<tty>` — hai đích chạm của MỘT hàng trong `/terminal`, đặt
    // ngay trên dòng của cửa sổ ấy thay vì thành hai cái nút ở đáy.
    //
    // 🔴 Hà 2026-08-17, ảnh một danh sách 8 cửa sổ kéo theo 16 cái nút xếp dọc:
    // *"danh sách đó mỗi cái và nút nằm trên 1 dòng"*. Một cái nút chỉ nằm được
    // dưới đáy tin và Telegram cắt nhãn của nó, nên hai nút `ttys014` giống hệt
    // nhau nằm cạnh nhau mà không nói được cái nào mở cái nào đóng — cùng bài
    // học với ⏎/⌫ của ô nhập và với ☑ của dòng lựa chọn.
    //
    // Vì sao không dùng thẳng `sess:`/`close:` như cái nút: payload của deep
    // link chỉ nhận `[A-Za-z0-9_-]`, mà hai cái ấy mang dấu `:`. Nên chúng đi
    // đúng route cũ, chỉ khác cái vỏ — không thêm một đường ĐI nào.
    if let Some(tty) = verb.strip_prefix("wx_") {
        if is_tty_name(tty) {
            return Some((
                CommandKind::Close,
                0,
                format!("{}{tty}", crate::sessions::SHELL_ID_PREFIX),
            ));
        }
    }
    // `wb_<cửa sổ>_<tab>` — chạm vào một hàng TAB của `/web`.
    //
    // Cùng khuôn với `s_<id phiên>`: tham số nằm trong TÊN lệnh vì chạm một
    // liên kết chỉ gửi lại đúng token ấy, chữ sau dấu cách rơi mất. Dấu `_`
    // ngăn ô ở payload rồi đổi thành `.` khi thành lệnh, vì `.` không nằm trong
    // bộ ký tự Telegram cho phép ở `?start=`.
    if let Some(rest) = verb.strip_prefix("wb_") {
        if let Some((w, t)) = rest.split_once('_') {
            let so = |x: &str| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit());
            if so(w) && so(t) {
                return Some((CommandKind::Web, 0, format!("{w}.{t}")));
            }
        }
    }
    // `s_<id phiên>` — CHẠM VÀO CHÍNH HÀNG của phiên trong danh sách, thay cho
    // một cái nút lặp lại hàng ấy dưới đáy tin.
    //
    // 🔴 Hà 2026-08-22, ảnh chụp buồng chat lúc 21:36: *"Vẫn đang hiện cả danh
    // sách lẫn nút thừa thãi"*. Mỗi phiên hiện HAI lần — một hàng chữ, rồi một
    // cái nút mang đúng tên + tài khoản + icon tình trạng của hàng ấy. Sáu nút
    // ăn gần nửa màn, và Telegram cho nút một chiều cao CỐ ĐỊNH nên rút ngắn
    // nhãn không lấy lại được một pixel nào: chỉ có bỏ hẳn.
    //
    // Id ĐẦY ĐỦ nằm trong payload (uuid 36 ký tự + `s_` = 38 ≤ 64 của
    // Telegram), nên liên kết mang đúng cái lệnh cũ cái nút mang — `/session
    // <uuid>` — chứ không phải một đường thứ hai phải nhớ.
    if let Some(id) = verb.strip_prefix("s_") {
        let ok = crate::sessions::is_shell_id(id)
            || (!id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        if ok {
            return Some((CommandKind::Session, 0, id.to_string()));
        }
    }
    // `shot_<id>` — XEM MÀN của đúng phiên ấy, bằng một LIÊN KẾT trong chữ.
    //
    // 🔴 Hà 2026-08-26: *"nút xem màn bỏ text đi để icon và bao hết text của tin
    // gim"*. Cái nút bàn phím `shot:<id>` vốn đã có, nhưng nút thì đứng RỜI ở
    // đáy tin — không bọc được chữ, nên đích chạm to đúng bằng cái emoji.
    //
    // Anh em sinh đôi của `s_`: cùng hình dạng, cùng phép kiểm id, khác đúng
    // cái việc nó làm. Tách riêng chứ không nhét thêm cờ vào `s_`, vì *"vào
    // phiên"* và *"xem màn"* là hai việc — gộp là dựng một đường mà người đọc
    // không đoán được nó sẽ làm gì.
    if let Some(id) = verb.strip_prefix("shot_") {
        let ok = crate::sessions::is_shell_id(id)
            || (!id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        if ok {
            return Some((CommandKind::Shot, 0, id.to_string()));
        }
    }
    if let Some(tty) = verb.strip_prefix("w_") {
        if is_tty_name(tty) {
            return Some((
                CommandKind::Session,
                0,
                format!("{}{tty}", crate::sessions::SHELL_ID_PREFIX),
            ));
        }
    }
    // `f_<n>` — 📎 tải về một TỆP phiên vừa nhắc tới, bấm ngay tại tên tệp trong
    // chữ. Chỉ số thập phân, cùng sổ với cái nút `file:<n>` ở đáy tin (xem
    // `CommandKind::SendFile`).
    if let Some(n) = verb.strip_prefix("f_") {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
            return Some((CommandKind::SendFile, 0, n.to_string()));
        }
    }
    // `/type <nút> [id phiên]` — Hà 2026-08-16: *"cấu trúc lại lệnh type thành
    // `/type <nút> [id phiên]`, ko có id phiên thì vào phiên đang trỏ tới"*.
    //
    // Vì sao nó đúng hơn `/key <id> <phím>`: id đứng TRƯỚC bắt người gõ phải
    // biết id trước khi biết mình muốn bấm gì, mà chín trên mười lượt là bấm
    // vào đúng phiên đang theo — nên thứ bắt buộc lại là thứ gần như luôn thừa.
    // Chữ thường thì đã không cần động từ nào cả (gõ thẳng là vào phiên đang
    // theo), nên `/type` còn đúng một việc đáng làm: gửi một NÚT.
    //
    // Hẹp có chủ ý, để không nuốt mất một câu chữ: chỉ nhận khi từ đầu là TÊN
    // PHÍM thật (hỏi `keys::is_key_name`, không chép danh sách) và cả tham số
    // chỉ có 1–2 từ. `/type enter vào phiên đi` vẫn là một dòng chữ.
    if matches!(verb.as_str(), "type" | "go") {
        let rest = t[1..]
            .split_once(char::is_whitespace)
            .map(|(_, r)| r.trim())
            .unwrap_or_default();
        let mut it = rest.split_whitespace();
        if let (Some(key), id, None) = (it.next(), it.next(), it.next()) {
            if crate::keys::is_key_name(key) {
                let arg = match id {
                    Some(id) => format!("{id} {key}"),
                    None => key.to_string(),
                };
                return Some((CommandKind::Key, 0, arg));
            }
        }
    }
    // BẢNG LỆNH trả lời trước, cho mọi route có cách đọc tham số CHUẨN — xem
    // `crate::commands`. Nhánh `match` bên dưới chỉ còn giữ những route có luật
    // riêng thật sự (`/new` đọc cờ, `/runin` đòi id đứng trước, `/sessions` cố
    // ý KHÔNG nhận id…), và mỗi cái đều đã khai trong bảng với `Arg::Custom`
    // nên `/help` lẫn menu Telegram vẫn thấy chúng.
    if let Some(r) = crate::commands::lookup(&verb) {
        let rest = || {
            t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default()
        };
        match r.arg {
            crate::commands::Arg::None => return Some((r.kind, 0, String::new())),
            // Tham số nằm sẵn trong bảng (`/enter`, `/right`) — chữ gõ thêm sau
            // tên lệnh bị bỏ qua, vì cái tên đã là toàn bộ ý định.
            crate::commands::Arg::Fixed(v) => return Some((r.kind, 0, v.to_string())),
            crate::commands::Arg::Rest => return Some((r.kind, 0, rest())),
            crate::commands::Arg::RestRequired => {
                let v = rest();
                return (!v.is_empty()).then_some((r.kind, 0, v));
            }
            // Luật riêng: rơi xuống `match` bên dưới.
            crate::commands::Arg::Custom => {}
        }
    }
    match verb.as_str() {
        // `/approve` `/reject` `/close` `/reply` `/act` được phân tích ở đây tới
        // 2026-08-08. Chúng tác động lên một hộp thư không còn tồn tại, và một
        // động từ parse được mà không có handler là tệ nhất trong hai đằng:
        // phòng chat nhận nó, không có gì xảy ra, và không có gì nói ra điều đó.
        // Không parse thì chúng là chữ thường — đúng sự thật.
        // `/ingest` (`/poll`) đã bỏ 2026-08-14: động từ ấy đọc PHÒNG CHAT.
        // Không parse thì nó là chữ thường — đúng sự thật, và đúng luật đã ghi
        // ngay trên đây về `/approve` với cái hộp thư không còn tồn tại.
        "run" | "cycle" => Some((CommandKind::Run, 0, String::new())),
        "doctor" | "health" => Some((CommandKind::Doctor, 0, String::new())),
        // `/accounts` — cũng là một verb không mang id. `acc` để gõ nhanh trên
        // điện thoại, `taikhoan` cho lối gõ không dấu quen thuộc của phòng này.
        "accounts" | "acc" | "taikhoan" => Some((CommandKind::Accounts, 0, String::new())),
        // `/runin <id> <lệnh>` — id đi TRƯỚC, phần còn lại là nguyên văn dòng
        // lệnh. Không có id thì không nhận: cái nút luôn mang id, và một
        // `/runin` gõ tay không id sẽ chạy vào phiên đang theo — đúng con
        // đường đã gõ nhầm phiên tối 2026-08-13.
        "runin" => {
            let rest = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!rest.is_empty() && rest.contains(char::is_whitespace)).then_some((
                CommandKind::RunIn,
                0,
                rest,
            ))
        }
        // `/close [id]` — cùng cách nhận đích với `/stop`: trống thì phiên đang
        // theo, có id thì phiên ấy. Khác `/stop` ở KẾT CỤC, không ở cách nhắm.
        "close" | "dong" | "dongphien" => {
            let want = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            Some((CommandKind::Close, 0, want))
        }
        // `/set <khoá> <giá trị>` — ô id ở đây giữ KHOÁ, nên cắt lại từ chuỗi
        // thô thay vì dùng id đã phân tích.
        "set" | "cauhinh" => {
            let rest = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            // Cần CẢ khoá lẫn giá trị; `/set foo` một mình sẽ tới nơi với giá
            // trị rỗng và xoá trắng trường ấy.
            (!rest.is_empty() && rest.contains(char::is_whitespace)).then_some((
                CommandKind::SetConfig,
                0,
                rest,
            ))
        }
        // 🔴 `/cmd` · `/win` · `/project` đã gỡ ngày 2026-08-15 (Hà: *"Bỏ cả
        // 3"*, và về `/cmd`: *"Không cần cmd vì có terminal là dán vào được"*).
        // Đo trên toàn bộ log: `/win` và `/project` chưa chạy lần nào từ 26/07;
        // `/cmd` đúng một lần, và lần ấy là chạm menu ☰ nên ack là "cần một
        // dòng lệnh" — chưa có dòng shell nào thật sự chạy qua nó.
        //
        // Ba nhánh `match` của chúng vốn đã CHẾT trước cả khi bị gỡ: bảng lệnh
        // ở trên trả lời trước và `return` ngay với `Arg::Rest`, nên chúng
        // không bao giờ tới lượt.
        // `/session <uuid>` — ô id chỉ đọc được số nguyên, mà id phiên là uuid,
        // nên cắt lại như `/project`.
        //
        // **`/sessions` (số nhiều) = xem danh sách** (Hà 2026-08-11: *"mở kênh
        // /sessions để xem danh sách phiên"*). Cùng một route, khác đúng cái tên
        // gõ vào: người ta hỏi "có những phiên nào" bằng số nhiều, và bắt họ
        // nhớ rằng "/session không tham số" mới là danh sách là bắt nhớ một luật
        // của mã. Số nhiều thì KHÔNG nhận id — `/sessions <id>` là câu gõ nhầm,
        // và im lặng theo một phiên vì gõ nhầm thì mọi lệnh sau đó đi sai chỗ.
        "sessions" | "phiens" | "danhsach" => Some((CommandKind::Session, 0, String::new())),
        "session" | "phien" => {
            let want = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            Some((CommandKind::Session, 0, want))
        }
        // `/handover [<uuid>]` — ô id là uuid, nên cắt lại như `/session`.
        "handover" | "bangiao" => {
            let want = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            Some((CommandKind::Handover, 0, want))
        }
        // 🔴 `/new` KHÔNG còn nhánh riêng ở đây, 2026-08-14 (Hà: *"Lệnh new nữa
        // chưa chạy đc"*). Nó đi bằng bảng lệnh, `Arg::Rest` — nhận cả câu, kể
        // cả câu RỖNG.
        //
        // Cổng cũ đòi `rest` có ít nhất một dấu cách. Luật ấy đúng khi từ đầu
        // tiên còn là TÊN DỰ ÁN — nhưng luật kia đã bị bỏ ngày 2026-08-13 (Hà:
        // *"lệnh new chỉ cần tham số sử dụng acc nào và text gửi đi là gì"*),
        // còn cái cổng thì ở lại. Từ đó `/new` gõ trơn bị trả về `None`, rơi vào
        // nhánh "không phải lệnh", và huba đáp *"Chưa hiểu lệnh này"* — về một
        // động từ chính nó vừa khai với Telegram bằng `setMyCommands` và đang
        // hiện trong menu. Đo được **ba lần** trong nhật ký: 13-08 13:27,
        // 14-08 08:13, 14-08 22:27.
        //
        // Chạm vào một lệnh trong menu Telegram chỉ gửi lại ĐÚNG token lệnh —
        // chữ sau dấu cách rơi mất (xem khối `pick_` ở đầu hàm). Nên với một
        // lệnh `listed: true`, "gõ trơn" không phải cách dùng sai: nó là cách
        // dùng MẶC ĐỊNH, cách duy nhất một ngón tay chạm tới được.
        //
        // Và handler đã sẵn sàng từ lâu: `name.is_empty()` ⟹ gốc workspace,
        // `task` rỗng ⟹ mở cửa sổ rồi nói sau — đúng luật 7 của `CLAUDE.md`
        // (*"An empty task is allowed on the window path"*). Chỉ mỗi cái cổng
        // này chưa ai gỡ.
        // `/stop [id]` — trống nghĩa là phiên đang theo.
        "stop" | "dung" => {
            let want = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            Some((CommandKind::Stop, 0, want))
        }
        // 🔴 `/tell` gỡ 2026-08-15. Hà: *"lệnh tell là không cần thiết?"* ·
        // *"vì trên tele tôi chỉ gõ text bình thường thôi"*.
        //
        // Và bằng chứng mạnh hơn con số 0 lượt dùng (con số ấy một mình đã lừa
        // một lần rồi — `/win`, `listed:false`, đo SỰ VÔ HÌNH): `sessions::tell`
        // mở đầu bằng `if session.kind != "background" { bail!(…) }`, mà hạng
        // phiên nền nay chỉ còn sinh ra khi MỞ CỬA SỔ THẤT BẠI. Nó không phải
        // chưa ai gõ — nó gần như không còn mục tiêu để nhắm vào.
        //
        // Khả năng thật của nó (nói tiếp vào một phiên đã tắt) KHÔNG mất: nó về
        // `/new <id>`, mở một cửa sổ chạy `claude --resume <id>` — đúng thứ chủ
        // máy làm khi ngồi ở máy, thay vì một lượt `-p` không cửa sổ và có tiêu
        // hạn mức.
        // `/type <chữ>` — gõ thẳng vào cửa sổ của phiên đang theo.
        "type" | "go" => {
            let what = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!what.is_empty()).then_some((CommandKind::Type, 0, what))
        }
        // `/upgrade` — huba tự dựng lại chính nó từ mã hiện tại.
        "upgrade" | "capnhat" => Some((CommandKind::Upgrade, 0, String::new())),
        // `/shot` — đọc màn của phiên đang theo.
        "shot" | "chup" => Some((CommandKind::Shot, 0, String::new())),
        // `/key <tên phím>` — một phím điều khiển.
        "key" | "phim" => {
            let what = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!what.is_empty()).then_some((CommandKind::Key, 0, what))
        }
        // `/pick [<phiên>] <câu>.<lựa chọn>` — trả lời một câu của bảng nhiều câu.
        "pick" | "chon" => {
            let what = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!what.is_empty()).then_some((CommandKind::Pick, 0, what))
        }
        // `/ask <câu hỏi>` — mọi thứ sau động từ là câu hỏi, nên cắt lại như
        // `/project`. Không id: đích là phiên đang theo, vì câu này được gõ
        // trong lúc đang nhìn phiên ấy.
        "ask" | "hoi" => {
            let question = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            // Một `/ask` rỗng sẽ trả tiền cho một lượt gọi claude để trả lời
            // chỗ trống. Rơi về `None` để nó thành chữ thường và người gõ thấy
            // là nó không được hiểu.
            (!question.is_empty()).then_some((CommandKind::Ask, 0, question))
        }
        _ => None,
    }
}
