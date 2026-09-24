//! "Đã dán" phải là lời khai CỦA NHẬT KÝ phiên đích, không phải của màn.
//!
//! 🔴 2026-09-24: đo trên 98 lượt huba khai `so_viec_da_tra`, **21 lượt khối kết
//! quả chỉ vào hội thoại sau hơn 60 s** (tới 4914 s) — nó nằm trong ô nhập, chưa
//! gửi, trong khi `type_and_send` đã trả `Gone`. Ca rõ nhất: kết quả `git push`
//! của phiên huba dán lúc 02:35:56Z, vào hội thoại lúc 04:34:33Z, dính liền với
//! câu Hà gõ sau đó. `pipeline::khoi_da_vao_nhat_ky` là phép đo thay cho màn;
//! bài này khoá nó trên HÌNH DẠNG THẬT của nhật ký (`user` chuỗi · `user` mảng ·
//! `queue-operation enqueue`).

use huba::pipeline::khoi_da_vao_nhat_ky;

const LENH: &str = "git -C /Users/hanguyen/projects/huba push origin main";

fn user_chuoi(text: &str) -> String {
    serde_json::json!({ "type": "user", "message": { "role": "user", "content": text },
                        "timestamp": "2026-09-24T04:34:33.000Z" })
    .to_string()
}

#[test]
fn nhan_luot_user_dang_chuoi_va_dang_mang() {
    let khoi = format!("[huba chạy hộ]\n$ {LENH}\n✅ xong (3.1s)\n[stderr] To https://github.com/dipgle/agent-hub.git");
    assert!(khoi_da_vao_nhat_ky(&user_chuoi(&khoi), LENH));
    let mang = serde_json::json!({ "type": "user", "message": { "role": "user",
        "content": [ { "type": "text", "text": khoi } ] } })
    .to_string();
    assert!(khoi_da_vao_nhat_ky(&mang, LENH));
    // Khối hỏng (không qua tty) mang nhãn dài hơn — vẫn là khối của huba.
    let hong = format!("[huba chạy hộ · không qua tty]\n$ {LENH}\n⚠ không chạy được: x");
    assert!(khoi_da_vao_nhat_ky(&user_chuoi(&hong), LENH));
}

#[test]
fn nhan_hang_cho_khi_phien_dang_ban() {
    let dong = serde_json::json!({ "type": "queue-operation", "operation": "enqueue",
        "content": format!("[huba chạy hộ]\n$ {LENH}\n✅ xong") })
    .to_string();
    assert!(khoi_da_vao_nhat_ky(&dong, LENH));
    // `dequeue`/`remove` không phải bằng chứng nhận — chỉ `enqueue`.
    let bo = serde_json::json!({ "type": "queue-operation", "operation": "remove",
        "content": format!("[huba chạy hộ]\n$ {LENH}") })
    .to_string();
    assert!(!khoi_da_vao_nhat_ky(&bo, LENH));
}

/// ĐỐI CHỨNG NGƯỢC: những thứ TRÔNG giống mà không phải lượt nhận khối.
#[test]
fn khong_nhan_nham() {
    // Lệnh KHÁC (lượt cũ của một lệnh khác) ⟹ không tính.
    let khac = "[huba chạy hộ]\n$ git -C /x push origin lan/y\n✅ xong";
    assert!(!khoi_da_vao_nhat_ky(&user_chuoi(khac), LENH));
    // Phiên tự NHẮC tới lệnh trong lời của nó (assistant) ⟹ không phải nhận.
    let tu_noi = serde_json::json!({ "type": "assistant", "message": { "content": [
        { "type": "text", "text": format!("[huba chạy hộ]\n$ {LENH}") } ] } })
    .to_string();
    assert!(!khoi_da_vao_nhat_ky(&tu_noi, LENH));
    // Phiên tự GHI hòm thư (tool_use Write mang chính dòng lệnh) ⟹ không tính.
    let ghi = serde_json::json!({ "type": "user", "message": { "content": [
        { "type": "tool_result", "content": format!("huba chạy hộ… $ {LENH}") } ] } })
    .to_string();
    assert!(!khoi_da_vao_nhat_ky(&ghi, LENH));
    // Nhật ký rỗng / dòng hỏng ⟹ không.
    assert!(!khoi_da_vao_nhat_ky("", LENH));
    assert!(!khoi_da_vao_nhat_ky("{không phải json huba chạy hộ", LENH));
}

/// Neo trên ĐÚNG dòng thật của nhật ký phiên huba 04:34:33Z (rút gọn trường thừa),
/// để phép đo không chỉ đúng với khuôn tôi tự dựng.
#[test]
fn dong_that_cua_ca_0234z() {
    let that = r#"{"parentUuid":"x","isSidechain":false,"type":"user","message":{"role":"user","content":"[huba chạy hộ]\n$ git -C /Users/hanguyen/projects/huba push origin main\n✅ xong (3.1s)\n[stderr] To https://github.com/dipgle/agent-hub.git\n   fdfcee4..3e0cbfd  main -> main\nSao giờ chậm hơn cả trước khi sửa"},"timestamp":"2026-09-24T04:34:33.000Z"}"#;
    assert!(khoi_da_vao_nhat_ky(that, LENH));
}
