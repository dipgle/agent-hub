//! CỬA SỔ MỒ CÔI — phiên chết rồi, cửa sổ Terminal còn đứng đó.
//!
//! 🔴 Hà 2026-09-03: *"Đợi rất lâu nhưng cửa sổ terminal cũ không đóng mặc dù
//! cli thoát hết rồi, đang ở dấu nhắc lệnh của terminal"*.
//!
//! Đo ngay lúc ấy, và chỗ đo được mới là chỗ đáng vá: cửa sổ của lượt bàn giao
//! vừa xong (`1586`/`ttys007`) **đã đóng** lúc 16:40:07. Nhưng còn **hai** cửa
//! sổ khác đứng nguyên từ hôm trước — `1481`/`ttys008` (mở 01/09 19:05) và
//! `1786`/`ttys003` (mở 02/09 15:25) — cả hai chỉ còn `login` + `-zsh`,
//! `busy=false`, màn dừng ở dòng `claude --resume …`. huba **chỉ** dọn cửa sổ
//! của phiên nó đang bàn giao; cửa sổ có CLI tự chết thì không cuốn sổ nào biết.
//!
//! Hà chốt: **BÁO, KHÔNG TỰ ĐÓNG.** Nên bài kiểm này đo đúng một thứ — phép
//! NHẬN DẠNG. Nó phải bắt được cửa sổ rác thật, và phải bỏ qua cửa sổ chủ máy
//! đang dùng, vì cái giá của một lần nhận nhầm là một tin nhắn giục anh đóng
//! chính cửa sổ anh đang gõ.

use huba::keys::Tab;
use huba::pipeline::is_orphan_window;
use std::collections::BTreeSet;

/// Màn THẬT của cửa sổ `1786`/`ttys003`, chép từ ảnh chụp 03/09 16:45.
///
/// Giữ nguyên văn: đây là bằng chứng, không phải một chuỗi viết cho vừa bài
/// kiểm. Nó cũng ghi lại một chuyện khác đáng nhớ — lượt mở phiên ấy rơi vào
/// `cmdand quote>` (chuỗi nháy chưa đóng), nên cửa sổ này chứa cả một lượt khởi
/// động hỏng lẫn một lượt CLI thoát bình thường.
const MAN_THAT: &str = "\
Last login: Wed Sep  2 15:21:00 on ttys002
hanguyen@Has-MacBook-Pro ~ % cd '/Users/hanguyen/projects' && claude3 --permission-mode auto 'Tiếp quản phiên trước
cmdand quote>
cmdand quote> # BÀN GIAO — làn A-DESIGN (`dwork/dev-dsign`, `lan/a-dsign`)
cmdand quote>

Resume this session with:
  claude --resume 9ba80be4-4c34-4650-9e03-8556e61f480a
hanguyen@Has-MacBook-Pro projects % ";

fn tab(tty: &str, busy: bool, screen: Option<&str>) -> Tab {
    Tab {
        tty: tty.to_string(),
        busy,
        procs: vec!["login".into(), "-zsh".into()],
        screen: screen.map(str::to_string),
        title: "projects — -zsh".to_string(),
    }
}

fn khong_phien() -> BTreeSet<String> {
    BTreeSet::new()
}

/// Ca đã đo được trên máy thật: hai cửa sổ rác, và huba mù với cả hai.
#[test]
fn cua_so_con_dau_nhac_sau_khi_cli_thoat_la_mo_coi() {
    assert!(
        is_orphan_window(&tab("ttys003", false, Some(MAN_THAT)), &khong_phien()),
        "đây đúng cửa sổ Hà nhìn thấy — không nhận ra nó thì cả bản vá chẳng đo gì"
    );
}

/// ĐỐI CHỨNG NGƯỢC ①: tab CÒN BẬN không phải rác — đó là việc của sổ đóng.
///
/// Thiếu bài này thì một phép nhận dạng trả `true` cứng cũng xanh.
#[test]
fn tab_con_ban_khong_phai_rac() {
    assert!(
        !is_orphan_window(&tab("ttys003", true, Some(MAN_THAT)), &khong_phien()),
        "còn bận ⟹ `close_pending_tick` lo, đừng gọi là rác"
    );
}

/// ĐỐI CHỨNG NGƯỢC ②: **một tty được DÙNG LẠI** (luật 11b).
///
/// Ca thật 2026-08-12: chủ máy thoát CLI rồi gõ `claude` lại trong CHÍNH cửa sổ
/// ấy; scrollback vẫn còn dòng `claude --resume` của phiên trước. Hỏi màn thì ra
/// "rác", hỏi sổ phiên thì ra "đang chạy" — và sổ phiên mới là câu trả lời.
#[test]
fn tty_dang_co_phien_song_thi_khong_bao_gio_la_rac() {
    let mut song = BTreeSet::new();
    song.insert("ttys003".to_string());
    assert!(
        !is_orphan_window(&tab("ttys003", false, Some(MAN_THAT)), &song),
        "cửa sổ đang có phiên sống mà bị gọi là rác ⟹ giục chủ máy đóng phiên đang chạy"
    );
}

/// ĐỐI CHỨNG NGƯỢC ③: cửa sổ CHỦ MÁY tự mở, chưa từng chạy phiên nào.
///
/// Đây là lý do phép đo bắt bằng BẰNG CHỨNG DƯƠNG (`claude --resume` trên màn)
/// chứ không bằng sự vắng mặt của tiến trình `claude`: hai cửa sổ dưới đây có
/// `procs` y hệt nhau, `busy` y hệt nhau, và chỉ khác nhau ở chỗ một cái đã
/// từng có phiên. Đo bằng sự vắng mặt thì cả hai đều "rác", và huba sẽ đi giục
/// chủ máy đóng cái cửa sổ anh vừa mở để gõ.
#[test]
fn cua_so_chu_may_vua_mo_khong_phai_rac() {
    let man = "Last login: Thu Sep  3 16:40:00 on ttys009\nhanguyen@Has-MacBook-Pro ~ % ";
    assert!(
        !is_orphan_window(&tab("ttys009", false, Some(man)), &khong_phien()),
        "chưa từng có phiên nào ở đây ⟹ không phải việc của huba"
    );
}

/// ĐỐI CHỨNG NGƯỢC ④: `None` là **CHƯA ĐO ĐƯỢC**, không phải "màn sạch".
///
/// `Tab::screen` cố ý giữ hai chuyện khác nhau: `None` = lượt dò không xin chữ
/// (`terminal_tabs`), `Some("")` = xin rồi và màn thật sự trống. Gộp chúng lại
/// là dựng đúng cái bẫy `screen_of → None` của luật 13 — và ở đây nó fail-closed
/// đúng chiều: không đọc được thì KHÔNG kết luận, chứ không đoán bừa một hướng.
#[test]
fn khong_doc_duoc_man_thi_khong_ket_luan() {
    assert!(
        !is_orphan_window(&tab("ttys003", false, None), &khong_phien()),
        "`None` là chưa đo được — không được đọc thành 'có rác' hay 'không rác'"
    );
    assert!(
        !is_orphan_window(&tab("ttys003", false, Some("")), &khong_phien()),
        "màn trống thì không có bằng chứng nào cả"
    );
}

/// ĐỐI CHỨNG NGƯỢC ⑤ — CÁI BẪY THẬT: phiên đang **BÀN VỀ** lệnh khôi phục.
///
/// Cùng bài học với `keys::account_blocked_on_screen`: một phiên đang nói về
/// chuyện khôi phục phiên — **chính phiên viết bài kiểm này**, cả buổi chiều
/// 03/09 — chép nguyên văn `claude --resume <id>` vào giữa một câu tiếng Việt.
/// Màn dưới đây CHỨA đúng chuỗi ấy, nên một phép đo bằng `contains` sẽ xanh rồi
/// đi mách chủ máy đóng cửa sổ anh đang gõ.
///
/// Neo vào ĐẦU DÒNG thì cùng chuỗi ấy đứng giữa câu không còn khớp — và đó là
/// khác biệt duy nhất giữa bài này với bài đầu tiên.
#[test]
fn cau_van_nhac_toi_lenh_resume_khong_phai_bang_chung() {
    let man = "\
Nếu cửa sổ ấy chết thì mở lại bằng claude --resume 9ba80be4-4c34-4650 nhé anh.
Tôi đang chờ cổng chạy xong rồi báo lại.
❯ ";
    assert!(
        man.contains("claude --resume "),
        "bài kiểm này chỉ có nghĩa khi màn THẬT SỰ chứa chuỗi ấy — không thì nó \
         chẳng bẫy được phép đo nào"
    );
    assert!(
        !is_orphan_window(&tab("ttys003", false, Some(man)), &khong_phien()),
        "nhận theo `contains` ⟹ mách nhầm về đúng cửa sổ đang làm việc"
    );
}

/// Và vế còn lại của cùng cái neo: dấu THẬT bị thụt lề hai khoảng trắng
/// (`claude` in ra như thế), nên neo không được chặt tới mức đòi cột 0.
#[test]
fn dau_that_bi_thut_le_van_phai_bat_duoc() {
    assert!(
        MAN_THAT.contains("\n  claude --resume "),
        "bản chép phải giữ đúng hai khoảng trắng thụt lề của `claude`"
    );
    assert!(
        is_orphan_window(&tab("ttys003", false, Some(MAN_THAT)), &khong_phien()),
        "neo chặt tới mức đòi cột 0 thì trượt đúng dấu thật"
    );
}
