//! Mọi lượt gọi Telegram dùng MỘT client (giữ kết nối), không dựng mới mỗi lượt.
//!
//! 🔴 2026-09-24, Hà: *"từ lúc gửi đến lúc nhận phản hồi mất 20s là quá chậm"*.
//! Đo đường mạng tới `api.telegram.org` (5 lượt cùng phút): kết nối 0,22–2,43 s ·
//! TLS 0,45–3,73 s · tổng 1,1–4,9 s, một lượt hỏng sau 7,7 s. `Inbox::client()`
//! bản cũ dựng client mới cho 9 chỗ gọi (gửi · sửa · thả dấu…) ⟹ mỗi lượt trả lại
//! nguyên phí bắt tay. Bài đọc MÃ NGUỒN (cùng kiểu `hai_hang_lenh.rs`) vì thứ cần
//! khoá là hình dạng "dựng một lần, dùng lại", không có giá trị trả về nào mang nó.

fn nguon() -> String {
    let p = std::env::var("HUBA_TELEGRAM_SRC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/src/telegram.rs").to_string());
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Thân `fn client(&self)` — tới dấu `}` thụt 4 ô đầu tiên (hàm trong `impl`).
fn than_client(src: &str) -> Option<&str> {
    let dau = src.find("fn client(&self)")?;
    let cuoi = src[dau..].find("\n    }\n")?;
    Some(&src[dau..dau + cuoi])
}

/// Client có được giữ trong một `static` TRƯỚC chỗ dựng không. `None` = thiếu mỏ
/// neo (không có hàm, hoặc không dựng gì) ⟹ bài ĐỎ, không xanh.
fn dung_lai(than: &str) -> Option<bool> {
    let dung = than.find("Client::builder()")?;
    Some(than[..dung].contains("static "))
}

#[test]
fn telegram_dung_mot_client_chung() {
    let src = nguon();
    let than = than_client(&src).expect("KHÔNG ĐO ĐƯỢC: mất `fn client(&self)` trong telegram.rs");
    assert_eq!(
        dung_lai(than),
        Some(true),
        "`Inbox::client()` phải giữ client trong một `static` rồi dùng lại — dựng mới mỗi lượt là trả phí TCP+TLS mỗi lượt:\n{than}"
    );
}

/// ĐỐI CHỨNG NGƯỢC: hình dạng CŨ (dựng mới mỗi lượt) phải ra `Some(false)`.
#[test]
fn doi_chung_nguoc_hinh_dang_cu() {
    let cu = "    fn client(&self) -> Option<reqwest::blocking::Client> {\n        reqwest::blocking::Client::builder()\n            .timeout(Duration::from_secs(40))\n            .build()\n            .ok()\n    }\n";
    assert_eq!(dung_lai(than_client(cu).unwrap()), Some(false));
    // Gỡ hẳn chỗ dựng ⟹ KHÔNG-ĐO-ĐƯỢC, không phải "đúng".
    let rong = "    fn client(&self) -> Option<X> {\n        None\n    }\n";
    assert_eq!(dung_lai(than_client(rong).unwrap()), None);
}
