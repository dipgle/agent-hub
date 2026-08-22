//! MỌI đường gửi chữ ra Telegram phải đi qua **một** cửa có thử lại.
//!
//! 🔴 Ca thật, 2026-08-22. Vòng thử-lại-khi-lỗi-mạng nằm trong `react()` từ
//! 14/08, kèm đúng lý do của nó — *"một cú trượt mạng đủ để phá cả quy ước chủ
//! máy vừa đặt ra"* — và **không ai chép nó sang `send_text`**. Hà gõ `/focus`
//! lúc `03:36:46.760Z`; huba soạn đúng câu *"Chưa hiểu lệnh này — gõ /help"*,
//! rồi `telegram_ack_failed` lúc `03:37:38.288Z`:
//! `error sending request for url (…/sendMessage)`. Câu trả lời chết trên
//! đường đi, Hà thấy IM LẶNG. Riêng ngày ấy **5 lần**.
//!
//! Không có bài kiểm nào chạy được cái vòng thử lại — nó cần mạng hỏng đúng
//! nhịp, mà địa chỉ API thì gõ cứng (`telegram.rs`, `fn api`). Nên bài kiểm này
//! gác thứ **kiểm được**, và đúng thứ đã hỏng: *có còn đường gửi nào đi tắt
//! không*. Cùng họ `runtime::SIGNING_CN` — hai bản chép thì hai bản sẽ lệch.
//!
//! Nó đọc `src/telegram.rs`, KHÔNG đọc chính nó: một bài kiểm quét chính mình
//! tìm một chuỗi thì luôn tự khớp, và đó là phép đo mù
//! (`OPERATING-CHARTER.md` §2d).

/// Những phương thức CHỞ CHỮ tới chủ máy. Một lượt trượt mạng ở đây là một câu
/// biến mất — khác hẳn `getFile`/`setMyCommands` (khởi động, tự hỏi lại) hay
/// `sendPhoto`/`sendDocument` (multipart, hình dạng khác, lỗi của chúng vẫn đi
/// ra bằng một câu chữ — mà câu chữ ấy nay có thử lại).
const CHO_CHU: &[&str] = &["sendMessage", "editMessageText", "setMessageReaction"];

fn nguon() -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/telegram.rs");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("đọc {}: {e}", p.display()))
}

#[test]
fn every_text_send_goes_through_the_one_retrying_door() {
    let src = nguon();
    // Điều kiện tiên quyết: cái cửa ấy phải CÓ THẬT. Thiếu nó thì mọi assert
    // dưới đây xanh vì không có gì để đếm — đúng hình dạng bài kiểm không đo gì.
    assert!(
        src.contains("fn post_retry("),
        "không còn `post_retry` — cửa duy nhất có thử lại đã biến mất"
    );
    for m in CHO_CHU {
        let di_tat = format!(".post(self.api(\"{m}\"))");
        assert!(
            !src.contains(&di_tat),
            "`{m}` đang gọi thẳng, không qua `post_retry` — một cú trượt mạng là mất câu ấy"
        );
    }
}

/// Và chỉ có ĐÚNG MỘT vòng thử lại trong tệp.
///
/// Vế này khác vế trên: ở trên hỏi *"còn ai đi tắt không"*, ở đây hỏi *"có ai
/// dựng lại bản chép thứ hai không"*. Chính bản chép thứ hai là thứ đã hỏng —
/// `react()` có vòng, `send_text()` không, và hai đường lệch nhau tám ngày.
#[test]
fn there_is_exactly_one_retry_loop() {
    let src = nguon();
    let n = src.matches("for attempt in 0..").count();
    assert_eq!(
        n, 1,
        "có {n} vòng thử lại trong telegram.rs — gom về một chỗ, đừng chép"
    );
}
