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
//! lại)`. Nhờ thế 44 bài kiểm của nó chạy được mà không cần một máy chủ nào — và
//! đó cũng là lý do chúng còn nguyên vẹn sau khi cả cái kênh biến mất.

use serde_json::json;

use crate::adapters::CommandKind;
use crate::logging;

/// Một mệnh lệnh gõ trong phòng chat, nếu chữ này là lệnh VÀ người gõ được
/// quyền ra lệnh cho hub.
///
/// Cửa tin cậy là cả điểm mấu chốt: ở trong phòng là chuyện của kênh, còn mở
/// một phiên trên cái Mac này — hay hỏi nó một câu — là chuyện của chủ máy.
/// Người khác gõ `/new` thì đó chỉ là một người đang gõ chữ.
pub fn parse_command(
    text: &str,
    from_user_tid: &str,
    owner_tids: &[String],
) -> Option<(CommandKind, i64, String)> {
    let t = text.trim();
    if !t.starts_with('/') {
        return None;
    }
    if !owner_tids.iter().any(|u| u == from_user_tid) {
        logging::warn(
            "command_from_non_owner",
            json!({ "from_user_tid": from_user_tid, "head": crate::exec::truncate(t, 40) }),
        );
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
    if let Some(rest) = verb.strip_prefix("pick_") {
        // `pick_<8 ký tự đầu id>_<câu>_<lựa chọn>`
        let f: Vec<&str> = rest.split('_').collect();
        if f.len() == 3 && !f[0].is_empty() {
            return Some((CommandKind::Pick, 0, format!("{} {}.{}", f[0], f[1], f[2])));
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
        return parse_command(&format!("/{payload}"), from_user_tid, owner_tids);
    }
    // `send_<8 ký tự đầu id>` — gửi bảng đi (một dấu Enter vào đúng cửa sổ ấy).
    // Nó là `/key <id> enter` viết dưới dạng CHẠM ĐƯỢC: `/key` có tham số đứng
    // sau dấu cách, mà chạm thì chỉ gửi lại token lệnh — chữ sau rơi mất.
    if let Some(sid) = verb.strip_prefix("send_") {
        if !sid.is_empty() && sid.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some((CommandKind::Key, 0, format!("{sid} enter")));
        }
    }
    if let Some(n) = verb.strip_prefix("run_") {
        if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() {
            return Some((CommandKind::RunQuick, 0, n.to_string()));
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
        "ingest" | "poll" => Some((CommandKind::Ingest, 0, String::new())),
        "run" | "cycle" => Some((CommandKind::Run, 0, String::new())),
        "doctor" | "health" => Some((CommandKind::Doctor, 0, String::new())),
        // `/accounts` — cũng là một verb không mang id. `acc` để gõ nhanh trên
        // điện thoại, `taikhoan` cho lối gõ không dấu quen thuộc của phòng này.
        "accounts" | "acc" | "taikhoan" => Some((CommandKind::Accounts, 0, String::new())),
        // `/cmd <dòng lệnh>` — phần sau động từ là NGUYÊN VĂN dòng lệnh, nên
        // cắt lại từ chuỗi thô: `id` của bộ phân tích chung sẽ nuốt mất chữ đầu.
        "cmd" | "sh" => {
            let rest = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            Some((CommandKind::Cmd, 0, rest))
        }
        // `/runin <id> <lệnh>` — id đi TRƯỚC, phần còn lại là nguyên văn dòng
        // lệnh. Không có id thì không nhận: cái nút luôn mang id, và một
        // `/runin` gõ tay không id sẽ chạy vào phiên đang theo — đúng con
        // đường đã gõ nhầm phiên tối 2026-08-13.
        "runin" => {
            let rest = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!rest.is_empty() && rest.contains(char::is_whitespace))
                .then_some((CommandKind::RunIn, 0, rest))
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
        // `/win <dòng lệnh>` — cùng cách cắt với `/cmd`, khác chỗ CHẠY: một cửa
        // sổ Terminal thật, vì thứ nó thiếu là một cái tty.
        "win" | "cuaso" => {
            let rest = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!rest.is_empty()).then_some((CommandKind::Win, 0, rest))
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
        // `/project` nhận một TÊN, nên cắt lại: ô id vô nghĩa ở đây.
        "project" | "duan" => {
            let name = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            Some((CommandKind::Project, 0, name))
        }
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
        // `/new <dự án> <việc>` — mở một phiên trong dự án ấy.
        "new" | "moi" => {
            let rest = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!rest.is_empty() && rest.contains(char::is_whitespace)).then_some((
                CommandKind::New,
                0,
                rest,
            ))
        }
        // `/stop [id]` — trống nghĩa là phiên đang theo.
        "stop" | "dung" => {
            let want = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            Some((CommandKind::Stop, 0, want))
        }
        // `/tell <nội dung>` — lượt tiếp theo của phiên đang theo.
        "tell" | "noi" => {
            let what = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!what.is_empty()).then_some((CommandKind::Tell, 0, what))
        }
        // `/type <chữ>` — gõ thẳng vào cửa sổ của phiên đang theo.
        "type" | "go" => {
            let what = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!what.is_empty()).then_some((CommandKind::Type, 0, what))
        }
        // `/upgrade` — hub tự dựng lại chính nó từ mã hiện tại.
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
