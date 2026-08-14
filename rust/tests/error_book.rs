//! Một VÒNG phải biết được nó có sạch không.
//!
//! 🔴 Vá ngày 2026-08-14 cho một lỗ do chính lượt gỡ tfl5 mở ra. Bảng `runs`
//! từng được chặng hỏi vòng ghi, và chặng ấy đã đi cùng phòng chat.
//! `run_once` ghi thay, nhưng nó **gần như không bao giờ trả `Err`**: mọi handler
//! tự nuốt lỗi thành một câu trả lời cho người gõ. Hàng nào cũng `ok` ⟹ khối ấy
//! rỗng vĩnh viễn ⟹ đúng cái phép đo mù mà repo này lên án ở hai chỗ.
//!
//! Nên phép đo đổi NGUỒN: đếm dòng `error` trong nhật ký. Luật 3 của dự án đã
//! bắt mọi đường lỗi phải ghi một dòng ở đó, nên đây không phải một phép xấp xỉ
//! — nó là cùng một mệnh đề, đọc từ đầu kia.
//!
//! 📐 **Nó đo cái gì, đo bằng số thật** (đếm trên `logs/hub.log`, 2026-08-14):
//! 83.060 dòng `info` · **1.626 `warn`** · **120 `error`**. Tức khối này KHÔNG
//! phải "mọi trục trặc" — phần lớn trục trặc của hub sống ở mức `warn`, và cố ý
//! sống ở đó (ví dụ `claude_agents_list_failed`: không liệt kê được phiên thì đã
//! có `blind`/`notes` và `/accounts` nói ra, không cần chuông thứ hai). Đọc một
//! khối rỗng là "không có LỖI", không phải "không có gì đáng xem".
//!
//! Và trong 120 dòng `error` ấy, hai nguồn to nhất — `web_ui_failed` (42) và
//! `adapter_poll_failed` (25) — thuộc hai nhánh vừa bị xoá hôm nay. Thứ còn lại
//! là `telegram_poll_rejected`, `telegram_ack_failed`,
//! `session_change_telegram_failed`, `claude_call_failed`, `hubd_fatal`: đúng
//! những thứ đáng hiện lên màn khi hub im tiếng mà không rõ vì sao.
//!
//! ⚠ Bộ đếm là TOÀN CỤC của tiến trình, nên hai bài kiểm dưới đây phải đi lần
//! lượt: `cargo test` chạy các bài trong cùng một tệp song song, và bản nháp đầu
//! của chính hai bài này đã đỏ vì đếm phải dòng lỗi của nhau (`left: 2, right:
//! 1`). Đó là lỗi của phép đo, không phải của mã — và nó đáng ghi lại, vì một
//! phép đo dùng biến toàn cục thì luôn có cái bẫy ấy.

use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn an_error_line_is_counted_and_a_warning_is_not() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let before = hub::logging::error_count();
    hub::logging::error("test_loi_gia", serde_json::json!({ "vi_sao": "để đếm" }));
    assert_eq!(
        hub::logging::error_count(),
        before + 1,
        "một dòng error phải được đếm"
    );
    assert_eq!(
        hub::logging::last_error_msg().as_deref(),
        Some("test_loi_gia"),
        "tên sự kiện gần nhất phải giữ được — nó là thứ /doctor đọc"
    );

    // `warn` KHÔNG phải lỗi. Nếu tính nó thì mọi vòng đều đỏ, và một bảng đỏ
    // liên tục mù y hệt một bảng xanh liên tục — chỉ khác là nó còn dạy người
    // đọc thói quen bỏ qua.
    let before = hub::logging::error_count();
    hub::logging::warn("test_canh_bao", serde_json::json!({}));
    assert_eq!(
        hub::logging::error_count(),
        before,
        "warn bị tính thành error thì khối 'lỗi gần đây' thành khối 'mọi thứ'"
    );
}

/// 🔴 Chỉ **tên sự kiện** được giữ lại, KHÔNG bao giờ nội dung `fields`.
///
/// Đây là ranh giới bảo mật, không phải tiết kiệm bộ nhớ. Chuỗi này đi vào một
/// hàng `runs`, rồi từ hàng ấy lên màn điện thoại qua `/doctor`. `msg` là hằng
/// chuỗi viết trong mã; `fields` mang dữ liệu chạy thật — đường dẫn, câu lỗi của
/// thư viện, và **đã từng mang nguyên khoá bot** (đo 2026-08-11: 28 dòng log
/// chứa token vì `reqwest` in cả URL vào câu lỗi; đó là lý do `logging::redact`
/// tồn tại). Cho `fields` đi cùng là mở lại đúng con đường ấy, lần này chảy
/// vòng qua cơ sở dữ liệu — nơi `redact` không đứng gác.
#[test]
fn only_the_event_name_survives_never_the_fields() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    hub::logging::error(
        "test_loi_co_bi_mat",
        serde_json::json!({
            "url": "https://api.telegram.org/bot99:AAA-token-that-must-not-travel/x",
            "path": "/Users/hanguyen/thu-muc-rieng",
        }),
    );
    let kept = hub::logging::last_error_msg().expect("phải có tên sự kiện");
    assert_eq!(kept, "test_loi_co_bi_mat");
    for secret in [
        "token-that-must-not-travel",
        "thu-muc-rieng",
        "api.telegram.org",
    ] {
        assert!(
            !kept.contains(secret),
            "`{secret}` đi theo dòng lỗi vào sổ, rồi lên màn: {kept}"
        );
    }
}

/// 🔴 `/doctor` phải NÓI ĐƯỢC "có lỗi gần đây" — và nói được cả phạm vi nó soi.
///
/// Viết ngày 2026-08-14 sau khi tôi báo sai ba lần rằng `/doctor` đã đọc bảng
/// `runs`. Nó chưa hề: `runtime::errors_block` nằm trong `runtime::snapshot`,
/// và hàm ấy có đúng một chỗ gọi — `portal.rs`, tệp chết cùng trang tfl5.
///
/// Hai vế, và vế thứ hai mới là vế dễ mất: một dòng "không có lỗi" KHÔNG được
/// đọc thành "mọi thứ ổn". Phần lớn trục trặc của hub sống ở mức `warn` và cố ý
/// không lên đây, nên câu trả lời phải tự khai phạm vi của nó.
#[test]
fn doctor_says_what_it_found_and_what_it_looked_at() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = hub::db::Db::open(&dir.path().join("hub.sqlite")).expect("open db");

    // Sổ trắng: phải nói rõ soi 40 vòng, và nói rõ `warn` không tính.
    let clean = hub::pipeline::recent_errors_line(&db);
    assert!(clean.contains("không có lỗi"), "{clean}");
    assert!(
        clean.contains("40") && clean.contains("warn"),
        "một dòng 'không có lỗi' phải tự khai phạm vi, không thì nó đọc thành \
         'mọi thứ ổn': {clean}"
    );

    // Một vòng bẩn thì phải hiện lên, kèm câu lỗi.
    let id = db.start_run("cycle", "cycle").expect("start");
    db.finish_run(
        id,
        hub::db::RunFinish {
            ok: false,
            n_new: 0,
            err: Some("2 lỗi trong vòng này, gần nhất: telegram_ack_failed".into()),
            skipped: None,
        },
    )
    .expect("finish");
    let dirty = hub::pipeline::recent_errors_line(&db);
    assert!(dirty.contains("lỗi gần đây"), "{dirty}");
    assert!(
        dirty.contains("telegram_ack_failed"),
        "phải mang theo TÊN sự kiện, không thì người đọc vẫn phải mở log: {dirty}"
    );
    assert!(
        !dirty.contains("không có lỗi"),
        "có lỗi mà vẫn in câu xanh là đúng cái phép đo mù đang chữa: {dirty}"
    );
}
