//! Nhãn dự án của một phiên phải là thứ phiên TỰ KHAI, không phải thứ đếm được
//! trong đuôi nhật ký của nó.
//!
//! 🔴 Hà 2026-08-18: *"đang làm phiên onghut giờ mất luôn thành 2 phiên tfl5"*.
//! Phiên `08b1a8e8` là onghut; nó vừa làm một quãng đụng vào `AI/tfl5` (chỗ ghim
//! Playwright), và `folder_from_tail` đếm đường dẫn trong cửa sổ 256 KB rồi lấy
//! cái nhiều nhất. Đếm lại đúng cửa sổ ấy tại đúng thời điểm ấy
//! (byte 8508947..8771091 của nhật ký thật, cắt tại `2026-08-18T08:47:47Z`):
//!
//! | nhãn | số lần nhắc |
//! |---|---|
//! | `AI/tfl5` | 43 |
//! | `onghut` | 24 |
//!
//! ⟹ nhãn lật sang `[tfl5]`, danh sách hiện HAI hàng `[tfl5]` (hàng thật
//! `f58e9e12` + hàng onghut bị đổi tên), và chữ "onghut" biến mất khỏi màn — dù
//! phiên vẫn sống và lời cuối của nó vẫn mở đầu bằng `[onghut]`.
//!
//! Fixture dưới đây cắt ra từ CHÍNH cửa sổ ấy (17 bản ghi thật, giữ nguyên tỉ lệ
//! tfl5 > onghut; riêng lượt nói cuối bị cắt ngắn phần chữ cho dễ đọc).

use std::path::Path;

fn fixture() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/onghut-mislabel-2026-08-18.jsonl");
    std::fs::read_to_string(p).expect("fixture nằm cạnh bài kiểm")
}

/// Workspace thật: phép đối chiếu nhãn phải trỏ vào thư mục CÓ THẬT, nên bài
/// kiểm này chỉ chạy được trên máy có `~/projects/onghut` và `~/projects/AI/tfl5`.
fn workspace() -> String {
    let home = std::env::var("HOME").expect("HOME");
    format!("{home}/projects")
}

/// Ca gốc: phép đếm CHỌN SAI, và nó chọn sai một cách hoàn toàn hợp lý.
///
/// Giữ bài kiểm này lại để lần sau ai đọc mã còn biết vì sao phải có tầng khai
/// báo: bản thân phép đếm không hỏng, nó chỉ trả lời một câu hỏi khác — *"phiên
/// này đụng vào thư mục nào nhiều nhất"* — mà câu ấy không phải *"phiên này là
/// dự án gì"*.
#[test]
fn the_path_census_picks_the_wrong_project_here() {
    let got = hub::sessions::folder_from_tail(&fixture(), &workspace());
    assert_eq!(
        got.as_deref(),
        Some("AI/tfl5"),
        "fixture phải giữ nguyên thế mất cân bằng đã gây lỗi"
    );
}

/// Bản vá: lời phiên TỰ KHAI thắng phép đếm.
#[test]
fn the_session_own_prefix_wins_over_the_census() {
    let got = hub::sessions::folder_declared(&fixture(), &workspace());
    assert_eq!(
        got.as_deref(),
        Some("onghut"),
        "phiên tự mở đầu bằng `[onghut]` mà nhãn vẫn đi theo phép đếm"
    );
}

/// Và đường ghép cuối cùng — thứ dòng danh sách thật sự đọc.
#[test]
fn the_label_the_list_shows_is_the_declared_one() {
    let ws = workspace();
    let got = hub::sessions::folder_for_session("test-onghut-08b1a8e8", &fixture(), &ws, || None);
    assert_eq!(got.as_deref(), Some("onghut"));
    assert_eq!(
        hub::sessions::display_name("hanguyen-9c", "onghut"),
        "[onghut]"
    );
}

/// Không được vá quá tay: phiên KHÔNG khai gì thì phép đếm vẫn là câu trả lời.
///
/// Nhật ký của một phiên có thể toàn lượt gọi công cụ hàng giờ liền (`💬 [dùng
/// Bash]`), lúc ấy không có lời nào để đọc — bỏ phép đếm đi là đổi một lỗi lấy
/// một lỗi khác.
#[test]
fn with_no_declaration_the_census_still_answers() {
    let ws = workspace();
    let tail_without_prose: String = fixture()
        .lines()
        .filter(|l| !l.contains("[onghut] Đã viết lại"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        hub::sessions::folder_declared(&tail_without_prose, &ws),
        None,
        "không có lời nào thì đừng bịa ra một lời khai"
    );
    let got = hub::sessions::folder_for_session("test-no-prose", &tail_without_prose, &ws, || None);
    assert_eq!(
        got.as_deref(),
        Some("AI/tfl5"),
        "mất phép đếm là mất luôn nhãn"
    );
}

/// Một lượt nói chỉ có tên trong ngoặc thì chưa đủ — phải là thư mục CÓ THẬT.
///
/// Câu văn bàn về một thứ trong ngoặc vuông (`[BUG-02]`, `[hôm qua]`) không phải
/// một lời khai dự án, và đối chiếu với đĩa là cổng rẻ nhất chặn được cả lớp ấy.
#[test]
fn a_bracket_that_is_not_a_project_is_not_a_declaration() {
    let ws = workspace();
    for text in [
        "[BUG-02] đã vá",
        "[dùng Bash]",
        "[không-có-thư-mục-nào-tên-này] xong",
        "[Request interrupted by user]",
    ] {
        assert_eq!(
            hub::sessions::declared_label(text, &ws),
            None,
            "nhận nhầm một lời khai: {text}"
        );
    }
    // Còn tên thật thì nhận, kể cả khi được bọc trong dấu nháy ngược như phiên
    // hub vẫn viết (`[hub]`), và kể cả khi nằm trong ngăn kéo `AI/`.
    assert_eq!(
        hub::sessions::declared_label("`[hub]` Ba việc, gọn từng cái.", &ws).as_deref(),
        Some("hub")
    );
    assert_eq!(
        hub::sessions::declared_label("[tfl5] Con số không sai", &ws).as_deref(),
        Some("AI/tfl5")
    );
}

/// Phép đếm KHÔNG được lật cái nhãn phiên đã tự khai.
///
/// Đây là nửa còn lại của lỗi: lời khai có thể trôi ra khỏi cửa sổ 256 KB sau
/// một quãng dài chỉ gọi công cụ. Lúc ấy phép đếm quay lại một mình — và nếu nó
/// được phép ghi đè cái đã khai thì nhãn lật y như cũ, chỉ chậm hơn.
#[test]
fn the_census_may_not_overwrite_a_declared_label() {
    let ws = workspace();
    let sid = "test-memo-onghut";
    // Lượt đo 1: có lời khai ⟹ nhớ "onghut" kèm hạng "tự khai".
    assert_eq!(
        hub::sessions::folder_for_session(sid, &fixture(), &ws, || None).as_deref(),
        Some("onghut")
    );
    // Lượt đo 2: lời khai đã trôi mất, chỉ còn đường dẫn tfl5 ⟹ GIỮ NGUYÊN.
    let tail_without_prose: String = fixture()
        .lines()
        .filter(|l| !l.contains("[onghut] Đã viết lại"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        hub::sessions::folder_for_session(sid, &tail_without_prose, &ws, || None).as_deref(),
        Some("onghut"),
        "phép đếm đã lật cái nhãn phiên tự khai — đúng lỗi 18/08"
    );
}
