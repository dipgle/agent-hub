//! Một tệp gửi lên mà không nhận được thì câu từ chối phải NÓI VÌ SAO.
//!
//! 🔴 Hà 2026-09-11: *"Sao gửi ảnh qua tele tự nhiên lại lỗi này: ⚠ không hỏi
//! được Telegram đường dẫn của tệp ấy"* — rồi, vì câu ấy không nói gì:
//! *"Chả nhẽ do trong ảnh có chuỗi gì đặc biệt?"*
//!
//! Không phải nội dung tấm ảnh, và điều đáng nhớ là VÌ SAO câu hỏi ấy nảy ra
//! được: `take_file` gộp bốn kết cục khác hẳn nhau (mạng hỏng · Telegram từ
//! chối · thân không phải JSON · JSON rỗng ruột) vào một dòng duy nhất, trong
//! khi Telegram ĐÃ gửi kèm lời giải thích. `reqwest` coi HTTP 400 là một phản
//! hồi chứ không phải `Err`, nên `description` về tới tận nơi rồi bị `and_then`
//! bỏ đi.
//!
//! Đo bằng chính token đang chạy, 2026-09-11: `getMe` → HTTP 200
//! `@ai_angles_bot` (kênh sống); `getFile` với `file_id` sai → **HTTP 400 ·
//! `Bad Request: invalid file_id`**. Mọi mẫu dưới đây là hình dạng THẬT của
//! Telegram, không phải hình dạng nghĩ ra.

use huba::telegram::getfile_verdict;

/// Đường sống: có `file_path` thì trả đúng nó, không kèm gì.
#[test]
fn co_duong_dan_thi_tra_dung_duong_dan() {
    let than = r#"{"ok":true,"result":{"file_id":"AgACAgUAA","file_unique_id":"AQADFhlrGwE8IFV9","file_size":93210,"file_path":"photos/file_42.jpg"}}"#;
    assert_eq!(getfile_verdict(than).as_deref(), Ok("photos/file_42.jpg"));
}

/// Ca đo được hôm nay: Telegram từ chối, và câu của nó phải đi NGUYÊN VĂN ra
/// tới chủ máy. Đây là bài mà bản cũ không thể nào xanh — nó vứt `description`.
#[test]
fn telegram_tu_choi_thi_noi_nguyen_van_cau_cua_telegram() {
    let than = r#"{"ok":false,"error_code":400,"description":"Bad Request: invalid file_id"}"#;
    assert_eq!(
        getfile_verdict(than),
        Err("Bad Request: invalid file_id".to_string()),
        "lý do Telegram đưa là thứ DUY NHẤT chẩn đoán được — không được thay bằng câu chung chung"
    );
}

/// Trần 20 MB bị chặn ngay ở `getFile`, KHÔNG phải ở bước tải. Câu cũ đoán hộ
/// sai chỗ: nó gợi ý "≤ 20 MB" ở bước 2, nơi lỗi ấy không bao giờ tới.
#[test]
fn tep_qua_to_bi_chan_ngay_o_buoc_hoi_duong_dan() {
    let than = r#"{"ok":false,"error_code":400,"description":"Bad Request: file is too big"}"#;
    assert_eq!(
        getfile_verdict(than),
        Err("Bad Request: file is too big".to_string())
    );
}

/// "Không hiểu Telegram nói gì" là một trạng thái RIÊNG, không được đọc thành
/// "Telegram nói không" — hai cái dẫn tới hai việc sửa khác hẳn nhau (§13②).
/// Cả hai ca dưới đây đều phải mang theo CHÍNH cái thân trả về, vì lúc ấy nó là
/// bằng chứng duy nhất còn lại.
#[test]
fn than_khong_doc_duoc_thi_mang_theo_chinh_cai_than_ay() {
    let html = "<html><head><title>502 Bad Gateway</title></head></html>";
    let loi = getfile_verdict(html).unwrap_err();
    assert!(
        loi.contains("502 Bad Gateway"),
        "phải giữ lại thân trả về để còn biết mình vừa nói chuyện với ai: {loi}"
    );

    let rong = r#"{"ok":true,"result":{"file_id":"AgACAgUAA","file_size":93210}}"#;
    let loi = getfile_verdict(rong).unwrap_err();
    assert!(
        loi.contains("file_size"),
        "`ok:true` mà thiếu `file_path` vẫn là một câu KHÔNG dùng được: {loi}"
    );

    // `file_path` rỗng cũng là rỗng ruột — một chuỗi trắng ghép vào URL tải về
    // sẽ thành một lượt GET vào gốc thư mục tệp của bot, hỏng ở tận bước sau.
    let trang = r#"{"ok":true,"result":{"file_path":""}}"#;
    assert!(getfile_verdict(trang).is_err());
}
