//! Ô màu dự án phải TRUNG TÍNH — và cái nhãn không bao giờ được đeo hai ô.
//!
//! 🔴 Hà 2026-08-19, đọc một dòng huba vừa gửi:
//! `⏹ Đã đóng hẳn 🟥 [dwork]·f33ae528 — CLI chạy nốt rồi thoát, cửa sổ terminal
//! đã đóng (chờ 67s).` — *"sao nội dung lại thừa và mâu thuẫn nhau thế"*.
//!
//! Ô đỏ ở đó chỉ là **nhãn dự án**, vô can với việc đóng phiên; nhưng đỏ trong
//! mọi dòng chữ khác của huba đã có nghĩa *hỏng / cần chú ý*, nên một tin THÀNH
//! CÔNG mở đầu bằng nó thì tự cãi nhau. Màu mang nghĩa không được làm màu trang
//! trí. Bộ ô nay còn năm màu trung tính; đen/trắng cũng ra vì chúng là màu NỀN
//! của Telegram (một trong hai luôn chìm), và ⬜ giữ việc riêng: *"không biết dự
//! án nào"*.
//!
//! Bộ đo này ghim ba điều, và điều thứ ba là chỗ dễ vỡ nhất khi ai đó đổi bảng
//! màu lần sau: **`without_dot` phải gỡ được cả ô của bộ CŨ**, vì sổ theo dõi
//! (`watch::Mark::l`) còn giữ nhãn đúc bằng chúng.

use huba::sessions::{project_dot, without_dot};

/// Những dự án thật trong workspace, cộng vài chuỗi cạnh.
const TEN: [&str; 14] = [
    "dwork",
    "huba",
    "tfl5",
    "sdvi",
    "tcc",
    "amm",
    "social",
    "mailler",
    "codetrail",
    "onghut",
    "games",
    "anpha1",
    "init-project",
    "a",
];

const MANG_NGHIA: [&str; 2] = ["🟥", "🟨"];
const MAU_NEN: [&str; 2] = ["⬛", "⬜"];

#[test]
fn khong_du_an_nao_deo_o_do_hay_vang() {
    for t in TEN {
        let dot = project_dot(t);
        for xau in MANG_NGHIA {
            assert_ne!(
                dot, xau,
                "«{t}» nhận ô {xau} — màu mang nghĩa (hỏng/cảnh báo) không được làm nhãn dự án"
            );
        }
        assert!(
            !MAU_NEN.contains(&dot),
            "«{t}» nhận ô {dot} — đen/trắng là màu nền, một trong hai luôn chìm"
        );
    }
}

/// Cùng tên ⟹ cùng ô, mọi lần gọi, mọi tiến trình. Đây là lý do hàm băm tồn tại
/// (phát theo thứ tự gặp thì mỗi lần hubad khởi động lại là một bảng màu khác).
#[test]
fn cung_ten_thi_cung_o() {
    for t in TEN {
        let a = project_dot(t);
        for _ in 0..50 {
            assert_eq!(project_dot(t), a, "«{t}» đổi màu giữa hai lượt gọi");
        }
        // Khoảng trắng hai đầu không phải một dự án khác.
        assert_eq!(
            project_dot(&format!("  {t} ")),
            a,
            "«{t}» đổi màu vì khoảng trắng"
        );
    }
}

/// Không biết dự án nào thì nói đúng thế — và ô ấy không được trùng ô của một
/// dự án thật, nếu không "không biết" đọc ra thành một cái tên.
#[test]
fn khong_biet_du_an_thi_o_trang_rieng() {
    assert_eq!(project_dot(""), "⬜");
    assert_eq!(project_dot("   "), "⬜");
    for t in TEN {
        assert_ne!(
            project_dot(t),
            "⬜",
            "«{t}» chiếm mất ô dành cho 'không biết'"
        );
    }
}

/// 🔴 Chỗ dễ vỡ nhất: nhãn CŨ trong sổ. `shown` tự gắn ô vào bất cứ nhãn nào nó
/// nhận, nên nếu `without_dot` không nhận ra ô của bộ cũ thì ra `🟦 🟥 [dwork]`.
#[test]
fn go_duoc_ca_o_cua_bo_cu() {
    for cu in ["🟥", "🟨", "⬜", "🟦", "🟩", "🟧", "🟪", "🟫"] {
        assert_eq!(
            without_dot(&format!("{cu} [dwork]·f33ae528")),
            "[dwork]·f33ae528",
            "không gỡ được ô {cu} — nhãn sẽ đeo hai ô"
        );
    }
    // Không có ô thì trả nguyên, không cắt nhầm chữ đầu.
    assert_eq!(without_dot("[dwork]·f33ae528"), "[dwork]·f33ae528");
    assert_eq!(without_dot("  [huba]"), "[huba]");
    // Gỡ đúng MỘT ô: hai ô liền nhau là dấu hiệu của một lỗi khác, đừng che nó đi.
    assert_eq!(without_dot("🟦 🟥 [dwork]"), "🟥 [dwork]");
}
