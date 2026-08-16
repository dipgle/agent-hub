//! Hai cổng hỏi "màn có đang mở hộp chọn không" — và cả hai phải đứng được
//! trên MÀN THẬT của `[dwork]` ngày 2026-08-16.
//!
//! 🔴 Hà 2026-08-16: *"kiểm tra lại màn phiên dwork đi … ko biết thao tác kiểu
//! gì"*. Phiên ấy kẹt hơn ba tiếng ở hộp *"Set up auto mode for your
//! environment?"* — hub thấy hộp (log `trust_dialog_not_this_box choices=3` từ
//! 12:03) nhưng `has_chooser_footer` trả **false** trên cùng cái màn ấy, vì
//! hộp này dùng dòng chân *"Enter to confirm · Esc to cancel"* thay cho
//! *"Enter to select · ↑/↓ to navigate · Esc to cancel"*.
//!
//! Vì sao đó là lỗi chứ không phải chi tiết: `pipeline::prompt_line_text` lấy
//! hàm này làm cổng. Cổng mù ⟹ nó quét ngược tìm dòng `❯`, và khi ô nhập trống
//! thì dòng `❯` duy nhất là **con trỏ hộp chọn** (`❯ 1. Set it up`). hub đọc
//! thành "chữ trong ô nhập", dựng nút `⏎ Gửi`, và Enter lúc có hộp chọn thì
//! XÁC NHẬN lựa chọn 1 (luật 13) — mời chủ máy bật auto mode trong khi anh
//! tưởng mình đang gửi một câu.
//!
//! Bản chụp dùng ở đây là màn THẬT, đọc bằng chính `keys::screen_of` lúc
//! 15:31, lưu nguyên văn ở `tests/fixtures/`.

fn dwork_screen() -> String {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/screen-dwork-automode-2026-08-16.txt"
    );
    std::fs::read_to_string(p).expect("đọc được bản chụp màn dwork")
}

/// Bản chụp phải còn nguyên cái đặc điểm nó sinh ra để canh — nếu ai đó dọn nó
/// thành một màn "sạch" thì mọi assert dưới đây thành vô nghĩa mà vẫn xanh.
#[test]
fn the_fixture_still_carries_the_footer_that_broke_it() {
    let s = dwork_screen();
    assert!(
        s.contains("Enter to confirm") && !s.contains("Enter to select"),
        "bản chụp mất dòng chân 'Enter to confirm' — nó là toàn bộ lý do tệp này tồn tại"
    );
    assert!(
        s.contains("❯ 1. Set it up"),
        "bản chụp mất con trỏ hộp chọn"
    );
}

#[test]
fn the_confirm_style_footer_counts_as_a_chooser() {
    assert!(
        hub::keys::has_chooser_footer(&dwork_screen()),
        "màn có hộp chọn mà cổng nói không — đây là màn đã làm hub mời bật auto mode"
    );
}

/// Kiểu cũ vẫn phải nhận ra, không được vá cái này làm hỏng cái kia.
#[test]
fn the_select_style_footer_still_counts() {
    let man = "❯ 1. Đổi tên file ở gốc repo\n  2. Chỉ sửa tham chiếu\n\
               Enter to select · ↑/↓ to navigate · Esc to cancel";
    assert!(hub::keys::has_chooser_footer(man));
}

/// Hai mảnh chữ nằm rời nhau trên hai dòng văn xuôi KHÔNG phải một dòng chân.
/// Bản trước đo cả màn nên một phiên đang bàn về "to confirm" ở đoạn này và
/// "to cancel" ở đoạn kia là đủ dựng ra một hộp chọn không có thật.
#[test]
fn two_scattered_phrases_are_not_a_footer() {
    let prose = "Tôi sẽ gửi mail to confirm với khách.\n\
                 …\n\
                 Nếu họ đổi ý thì mình có đường to cancel trước thứ sáu.";
    assert!(!hub::keys::has_chooser_footer(prose));
}

/// Con trỏ hộp chọn KHÔNG được đọc thành chữ trong ô nhập — kiểm trên bản chụp
/// thật, sau khi đã bỏ dòng chân đi, để chứng minh cổng thứ hai tự đứng được.
#[test]
fn the_chooser_cursor_is_never_read_as_typed_text() {
    let s = dwork_screen();
    // Bỏ dòng chân ⟹ cổng thứ nhất mù, đúng như hôm nay. Cổng thứ hai phải giữ.
    let without_footer: String = s
        .lines()
        .filter(|l| !l.contains("Enter to confirm"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !hub::keys::has_chooser_footer(&without_footer),
        "dựng lại đúng cảnh cổng thứ nhất mù"
    );
    // …và bỏ nốt dòng chữ Hà đã gõ, để dòng `❯` duy nhất còn lại là con trỏ.
    let only_cursor: String = without_footer
        .lines()
        .filter(|l| !l.contains("làm việc 1"))
        .collect::<Vec<_>>()
        .join("\n");
    let read = hub::pipeline::prompt_line_text(&only_cursor);
    assert_eq!(
        read, None,
        "hub đọc con trỏ hộp chọn thành chữ trong ô nhập: {read:?} — cú `⏎ Gửi` \
         dựng từ đó sẽ XÁC NHẬN lựa chọn 1"
    );
}

/// Chữ người ta gõ thật thì vẫn phải đọc ra được — cổng mới không được nuốt nó.
#[test]
fn real_typed_text_still_reads() {
    let s = dwork_screen();
    let without_footer: String = s
        .lines()
        .filter(|l| !l.contains("Enter to confirm"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        hub::pipeline::prompt_line_text(&without_footer).as_deref(),
        Some("làm việc 1, deploy dev rồi nghiệm thu UI"),
        "câu Hà gõ nằm ngay đó, hub phải đọc được"
    );
}
