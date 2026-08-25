//! Chữ trong ô nhập: cả dòng là đích chạm, và không còn nút xoá.
//!
//! 🔴 Hà 2026-08-25: *"chỉnh nốt nút enter chỗ ô chờ gợi ý bao chọn cả text cho
//! dễ bấm, nút xóa ô nhập không cần thiết vì có lệnh xóa rồi"*.
//!
//! Cùng một lỗi với dòng lệnh hôm 23/08, chỉ khác chỗ: neo LÀ cả dòng, mà đích
//! chạm to bằng đúng hai ký tự của `⏎`.
//!
//! Và `⌫` đi hẳn: hai đích chạm cạnh nhau, một bên GỬI một bên XOÁ, **cả hai
//! đều không lùi lại được** — bỏ được cái nào là bớt một cú bấm nhầm không sửa
//! được.

use huba::pipeline::html_with_links;

const SEND: &str = "https://t.me/b?start=send_bab47095";

fn box_anchor(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    vec![(text.to_string(), vec![(SEND.to_string(), "⏎".to_string())])]
}

/// Cả dòng nằm trong `<a>`, icon đi VÀO TRONG thẻ.
#[test]
fn the_whole_prompt_line_becomes_the_tap_target() {
    let man = "📷 Màn của [tfl5]:\n❯ chạy deploy đi\n⏵⏵ auto mode on";
    let (html, linked, unlinked) = html_with_links(man, &box_anchor("chạy deploy đi"));
    assert_eq!(linked, 1, "{html}");
    assert!(unlinked.is_empty());
    assert!(
        html.contains(&format!("<a href=\"{SEND}\">⏎ chạy deploy đi</a>")),
        "đích chạm không phủ trọn chữ trong ô:\n{html}"
    );
}

/// 🔴 Chữ thường KHÔNG được bọc `<code>`.
///
/// `<code>` là của dòng lệnh — nó vẽ ranh giới và chặn tự-nối-liên-kết. Bọc một
/// câu tiếng Việt vào đó là biến nó thành thứ trông như mã. Ca này chỉ nổ khi
/// không dựng được liên kết (chưa biết tên bot), nên nó dễ mục nhất.
#[test]
fn plain_text_is_never_wrapped_in_code() {
    let man = "❯ chạy deploy đi\n";
    let anchors = vec![("chạy deploy đi".to_string(), Vec::new())];
    let (html, linked, unlinked) = html_with_links(man, &anchors);
    assert_eq!(linked, 0);
    assert_eq!(unlinked, vec![0], "neo không bấm được phải được nói ra");
    assert!(
        !html.contains("<code>"),
        "bọc chữ thường vào <code>:\n{html}"
    );
    assert!(html.contains("chạy deploy đi"), "nuốt mất chữ:\n{html}");
}

/// 🔄 ĐẢO CHIỀU 2026-08-25 — đường dẫn tệp giữa dòng NAY được bọc cả cụm.
///
/// Bản gốc của bài này (viết sáng cùng ngày) khoá đúng cái hẹp, và ghi rõ lý
/// do: *"Nới ra là đổi hình dạng của nút 📎 (đường dẫn giữa câu) và nhãn lựa
/// chọn (sau số thứ tự) — những chỗ **chưa ai hỏi**"*.
///
/// Chiều ấy hết hiệu lực vì đã có người hỏi. Hà, chiều 25/08, kèm ảnh một tin
/// có hai lần `FEATURE-GAPS.md`: *"icon tải tệp gắn không đúng chỗ trong nội
/// dung tin vậy, và cũng chưa bao text đường dẫn file"*. Hai lời một gốc: không
/// bọc thì `cmd_part` rỗng, nên cái 📎 rơi xuống cuối dòng nguồn — mà Telegram
/// bẻ lại đoạn văn, nên "cuối dòng nguồn" hiện ra giữa câu.
///
/// Bài kiểm ở lại, chỉ đổi chiều: nới quá tay hay thu về như cũ đều phải đỏ.
#[test]
fn a_file_path_in_the_middle_of_a_line_is_wrapped_whole() {
    let man = "  ⎿  Wrote 45 lines to .tmp/commit-msg.txt\n";
    let anchors = vec![(
        ".tmp/commit-msg.txt".to_string(),
        vec![("https://t.me/b?start=f_3".to_string(), "📎".to_string())],
    )];
    let (html, linked, _) = html_with_links(man, &anchors);
    assert_eq!(linked, 1, "{html}");
    assert!(
        html.contains("<a href=\"https://t.me/b?start=f_3\">📎 .tmp/commit-msg.txt</a>"),
        "đường dẫn không được bọc ⟹ đích chạm to đúng hai ký tự, và 📎 rơi \
         xuống cuối dòng:\n{html}"
    );
    assert!(
        !html.contains(">📎</a>"),
        "📎 vẫn đứng một mình ⟹ đã rơi về cuối dòng:\n{html}"
    );
}

/// 🔴 HÀNG RÀO NGƯỢC — cái nới 25/08 chỉ nới cho ĐƯỜNG DẪN, không cho nhãn lựa
/// chọn. Nhãn lựa chọn đứng sau số thứ tự (`1. Vá ACL`), và đích chạm ☑ của nó
/// cố ý nằm ở ĐẦU dòng (Hà 17/08: *"Chèn phía trước số mỗi dòng"*) — mắt chạy
/// dọc cột số để chọn, nên bọc cả cụm là kéo nó ra khỏi cột ấy.
///
/// Thiếu bài này thì "nới cho 📎" và "nới cho tất cả" trông giống hệt nhau.
#[test]
fn a_choice_label_after_its_number_keeps_the_old_shape() {
    let man = "  1. Vá ACL trước, đăng nhập sau\n";
    let anchors = vec![(
        "Vá ACL trước, đăng nhập sau".to_string(),
        vec![(
            "https://t.me/b?start=k_ab12_1".to_string(),
            "\t☑".to_string(),
        )],
    )];
    let (html, linked, _) = html_with_links(man, &anchors);
    assert_eq!(linked, 1, "{html}");
    assert!(
        !html.contains("☑ Vá ACL trước, đăng nhập sau</a>"),
        "nhãn lựa chọn bị bọc cả cụm — bản vá 📎 nới lan sang chỗ chưa ai hỏi:\n{html}"
    );
    assert!(
        html.contains("1. Vá ACL trước"),
        "nuốt mất số thứ tự hoặc nhãn:\n{html}"
    );
}

/// Neo quá ngắn thì không bọc: một dòng hai ba ký tự trùng được với đủ thứ.
#[test]
fn a_very_short_anchor_does_not_take_the_whole_line() {
    let man = "ok\n";
    let (html, _, _) = html_with_links(man, &box_anchor("ok"));
    assert!(
        !html.contains("<a href=\"https://t.me/b?start=send_bab47095\">⏎ ok</a>"),
        "neo 2 ký tự mà vẫn bọc cả dòng:\n{html}"
    );
}
