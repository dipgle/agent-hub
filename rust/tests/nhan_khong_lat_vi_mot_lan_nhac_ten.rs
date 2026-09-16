//! Một phiên NHẮC tên dự án khác thì không được đổi tên theo.
//!
//! 🔴 Hà 2026-09-16, ảnh danh sách Telegram: *"Phiên main chạy một hồi nhắc đến
//! huba thì tên bị nhảy sang luôn, không hiểu đang bắt kiểu gì, như thế này làm
//! sao tôi biết thế nào để quản lý"*. Hàng `1249d1ef` — việc của nó đúng là
//! *"Fix main branch session disconnection"* của **dwork** — hiện ra `[huba]`.
//!
//! Đo trên chính nhật ký ấy (256 KB đuôi, 18 lượt `assistant`), dòng kích hoạt
//! là **lượt nói của chính phiên**, lúc 2026-09-16T07:07:18.882Z:
//!
//! ```text
//! `[huba]` xác nhận chẩn đoán mới (11/162 = 6,8 %, cùng dân số) — và **bắt được…
//! ```
//!
//! Hai phiên đang nhắn cho nhau, nên phiên dwork **nói VỀ** huba — còn bộ dò đọc
//! thành *"tôi LÀ huba"*. Hai cổng cũ đều không đỡ được: chuỗi ấy nằm ở ĐẦU lượt
//! nói, và `huba` là thư mục CÓ THẬT.
//!
//! **Và nhãn sai KHÔNG TỰ LÀNH**: lời khai được ghi vào sổ nhớ, rồi từ đó luôn
//! thắng phép đếm đường dẫn (`recall_declared` đứng trên `counted`).
//!
//! ⚠ HAI ĐƯỜNG SỬA HIỂN NHIÊN ĐÃ BỊ ĐO RA LÀ SAI — đừng thử lại:
//! · *"lấy nhãn xuất hiện nhiều nhất"*: trong 256 KB đuôi ấy có **đúng một** dòng
//!   tự khai, và nó là dòng sai. Đa số của một tập một phần tử vẫn là phần tử ấy.
//! · *"đối chiếu sổ `scripts/.session-bind`"*: phiên ấy **không có** bản ghi nào
//!   — sổ ghi theo lượt GHI TỆP qua hook, mà nó đang chỉ đọc.
//!
//! Cổng thật nằm ở SỐ LẦN: luật workspace bắt phiên tự khai ở **MỌI** lượt nói,
//! nên một cái tên xuất hiện đúng một lần trong cả cửa sổ đuôi là tên được NHẮC.

fn workspace() -> String {
    let home = std::env::var("HOME").expect("HOME");
    format!("{home}/projects")
}

/// Dựng đuôi nhật ký từ các lượt nói của phiên, theo đúng hình dạng thật.
fn duoi(luot: &[&str]) -> String {
    luot.iter()
        .map(|t| {
            serde_json::json!({
                "type": "assistant",
                "message": {"role": "assistant", "content": [{"type": "text", "text": t}]}
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Nguyên văn dòng đã gây lỗi, cắt ngắn.
const NHAC_TEN_HUBA: &str =
    "`[huba]` xác nhận chẩn đoán mới (11/162 = 6,8 %, cùng dân số) — và bắt được một lỗi mẫu số";

/// CA CỦA HÀ: phiên đã mang nhãn `dwork`, rồi nhắc `huba` ĐÚNG MỘT LẦN.
#[test]
fn mot_lan_nhac_ten_khong_lat_duoc_nhan_dang_giu() {
    let ws = workspace();
    let sid = "test-nhac-ten-1249d1ef";

    // ① Phiên tự khai `dwork` như thường lệ ⟹ nhãn vào sổ.
    let truoc = duoi(&[
        "[dwork] Đang đo lại chồng lấn từng cặp bàn giao.",
        "[dwork] Xong, p50 125 giây.",
    ]);
    assert_eq!(
        huba::sessions::folder_for_session(sid, &truoc, &ws, || None).as_deref(),
        Some("dwork"),
        "phiên tự khai dwork mà không nhận được nhãn"
    );

    // ② Rồi nó NHẮC tên huba đúng một lần, ở đầu lượt nói.
    let sau = duoi(&[
        "Đã đo lại: 11/162 lượt bắn không thấy mốc mở cửa sổ.",
        NHAC_TEN_HUBA,
        "Tiếp tục dựng dòng thời gian cho lượt 18:45.",
    ]);
    assert_eq!(
        huba::sessions::folder_for_session(sid, &sau, &ws, || None).as_deref(),
        Some("dwork"),
        "một lần NHẮC tên dự án khác đã lật được nhãn — đúng cái Hà chụp màn 16/09"
    );
}

/// Chiều ngược: phiên THẬT SỰ đổi dự án thì vẫn phải đổi được nhãn.
///
/// Cổng chống lật mà chặn luôn cả lượt đổi thật thì nó không phải cổng, nó là
/// một cái nhãn đông cứng.
#[test]
fn khai_hai_lan_thi_van_lat_duoc_nhan() {
    let ws = workspace();
    let sid = "test-doi-that-9c44d3fe";

    let truoc = duoi(&["[dwork] Đang làm dwork.", "[dwork] Vẫn dwork."]);
    assert_eq!(
        huba::sessions::folder_for_session(sid, &truoc, &ws, || None).as_deref(),
        Some("dwork")
    );

    let sau = duoi(&[
        "[huba] Chuyển sang huba, đang đọc quota.rs.",
        "[huba] Đã vá xong, chạy bài kiểm.",
    ]);
    assert_eq!(
        huba::sessions::folder_for_session(sid, &sau, &ws, || None).as_deref(),
        Some("huba"),
        "khai HAI lần là đổi thật — cổng chống lật không được chặn ca này"
    );
}

/// Phiên CHƯA có nhãn nào thì một lời khai vẫn đủ để GIEO.
///
/// Cổng chỉ chặn việc LẬT, không chặn việc ĐẶT: đổi một nhãn sai lấy một nhãn
/// trống là đổi ngang, không phải sửa.
#[test]
fn phien_chua_co_nhan_thi_mot_lan_khai_van_du_de_gieo() {
    let ws = workspace();
    let sid = "test-gieo-moi-aa11bb22";
    let tail = duoi(&["[huba] Lượt nói đầu tiên của phiên này."]);
    assert_eq!(
        huba::sessions::folder_for_session(sid, &tail, &ws, || None).as_deref(),
        Some("huba"),
        "phiên mới mà phải đợi tới lượt nói thứ hai mới có tên thì cổng đang chặn nhầm"
    );
}

/// Lượt `user` KHÔNG phải lời khai — luật cũ, giữ nguyên, khoá lại vì bản vá
/// này đụng đúng vòng lặp ấy.
#[test]
fn loi_chu_may_go_khong_phai_loi_tu_khai() {
    let ws = workspace();
    let sid = "test-user-noi-cc33dd44";
    let tail = format!(
        "{}\n{}",
        serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": [{"type": "text", "text": "[huba] xem lại hộ tôi"}]}
        }),
        duoi(&["[dwork] Đang làm dwork."])
    );
    assert_eq!(
        huba::sessions::folder_for_session(sid, &tail, &ws, || None).as_deref(),
        Some("dwork"),
        "câu chủ máy gõ nhắc tên dự án khác là chuyện thường — không được tính là lời khai"
    );
}
