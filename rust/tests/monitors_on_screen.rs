//! Đếm **monitor** của `claude` từ chân màn — và không đếm nhầm lời bàn về nó.
//!
//! 🔴 Hà 2026-08-27: *"Một phiên đang có monitor thì luôn chèn thêm icon eye vào
//! để dễ nhận dạng"* · *"monitor là cái đang chạy nền ở cuối màn hình cơ mà"*.
//!
//! `monitor` là khái niệm của chính `claude`, KHÔNG phải của huba, và nó **không
//! để lại dấu vết nào trong `ps`** — khác hẳn `· 1 shell still running`, thứ mà
//! `Procs::running_shell` bắt được qua tiến trình con `shell-snapshots`. Nên màn
//! là nguồn duy nhất, và một phép đọc màn thì phải chứng minh được nó không đọc
//! bừa.
//!
//! Nửa dưới tệp là ĐỐI CHỨNG NGƯỢC (điều 13①): những màn CÓ chữ "monitor" mà
//! không phải một monitor đang chạy. Thiếu nửa ấy thì `|_| 1` cũng "đạt", và mọi
//! phiên sẽ mọc một con mắt vĩnh viễn.

use huba::keys::monitors_on_screen;

/// Hai dòng chân THẬT, chép từ ảnh Hà gửi lúc 06:56 ngày 27/08.
const CHAN_MAN: &str = "\
tra-loi-* bất kỳ, phiếu/tệp chạm dsign, hoặc nhánh làn đã đẩy được.
Không còn việc nào làm được mà chưa làm — tôi dừng ở đây chờ tin.
✳ Sautéed for 8s · 1 monitor still running
new task? /clear to save 676.9k tokens

❯ đã push rồi, kiểm lại đi

⏵⏵ auto mode on · 1 monitor · ← for agents · ↓ to manage";

#[test]
fn the_real_footer_reports_one_monitor() {
    assert_eq!(
        monitors_on_screen(CHAN_MAN),
        1,
        "chân màn thật khai 1 monitor ở CẢ HAI dòng — đọc ra 0 là mất hẳn dấu hiệu"
    );
}

#[test]
fn several_monitors_are_counted_not_just_flagged() {
    let man = "⏵⏵ auto mode on · 3 monitors · ← for agents";
    assert_eq!(
        monitors_on_screen(man),
        3,
        "chân màn khai bao nhiêu thì đọc bấy nhiêu — gộp thành bool là vứt một dữ kiện \
         đã đọc được rồi"
    );
}

/// Hết monitor thì phải về 0 — nếu không thì con mắt sáng mãi và thôi nhận dạng
/// được gì.
#[test]
fn a_footer_without_monitors_reads_zero() {
    for man in [
        "⏵⏵ auto mode on · ← for agents · ↓ to manage",
        "✳ Sautéed for 8s",
        "",
        "   \n\n  ",
    ] {
        assert_eq!(
            monitors_on_screen(man),
            0,
            "màn {man:?} không khai monitor nào"
        );
    }
}

/// 🔴 ĐỐI CHỨNG NGƯỢC: chữ "monitor" nằm trong HỘI THOẠI, không phải ở chân màn.
///
/// Ca này không phải giả định: chính phiên `[huba]` đã bàn về monitor suốt buổi
/// 27/08, nên màn của nó đầy chữ ấy. Quét cả màn là đếm lời bàn thành việc đang
/// chạy — và lúc ấy con mắt mọc lên đúng phiên KHÔNG có monitor nào.
#[test]
fn prose_about_monitors_is_not_a_running_monitor() {
    let noi_ve = format!(
        "Hà: một phiên đang có 2 monitor thì chèn icon eye\n\
         tôi: đã hiểu, 1 monitor sẽ thành 👁\n{}",
        "dòng hội thoại\n".repeat(12)
    );
    assert_eq!(
        monitors_on_screen(&noi_ve),
        0,
        "chữ 'monitor' trong hội thoại đã cuộn lên trên thì KHÔNG phải chân màn"
    );
}

/// Dòng không có dấu `·` thì không phải chân màn — cả hai dòng thật đều dùng nó
/// làm dấu phân cách.
#[test]
fn a_line_without_the_separator_is_not_a_footer() {
    assert_eq!(monitors_on_screen("tôi đã tắt 1 monitor rồi"), 0);
}

/// Số phải DÍNH liền chữ qua đúng một khoảng trắng — `10monitor` hay `monitor`
/// trần thì không đếm.
#[test]
fn only_a_number_directly_before_the_word_counts() {
    assert_eq!(monitors_on_screen("⏵⏵ auto · monitor · ← for agents"), 0);
    assert_eq!(monitors_on_screen("⏵⏵ auto · 2monitor · ← for agents"), 0);
    assert_eq!(monitors_on_screen("⏵⏵ auto · 2 monitor · ← for agents"), 2);
}
