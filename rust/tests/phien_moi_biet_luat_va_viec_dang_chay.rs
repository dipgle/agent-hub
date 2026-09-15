//! **Phiên mở bằng huba phải biết hai thứ mà nó đang không biết**: luật riêng
//! của dự án, và những gì phiên trước để lại đang chạy.
//!
//! 🔴 Hà 2026-09-15, hai câu liền nhau: *"khi một phiên bị hết hạn mức thì việc
//! chuyển phiên mới có nạp lại luật như cách tôi mở bằng tay không?"* · *"rồi
//! phiên cũ cũng có các luồng mà nó khởi tạo thì sao, phiên mới có biết để chạy
//! lại?"*
//!
//! Đo được lúc ấy, cả hai đều là KHÔNG:
//!
//! * **Luật**: nhật ký `975277d1` (cwd `~/projects`, 874 dòng) có nội dung
//!   `huba/CLAUDE.md` xuất hiện lần đầu ở **dòng 27** — luật dự án con không nạp
//!   lúc khởi động, nó chỉ vào khi phiên CHẠM tệp trong cây ấy. Đối chứng cùng
//!   lượt: `6b16e2a0`, sinh ra từ lượt tự bàn giao 11:10 cùng ngày, cùng cwd
//!   gốc, có **0** dấu vết — nó chưa chạm tệp nào nên chưa bao giờ nạp.
//! * **Việc đang chạy**: `HANDOVER_PROMPT` hỏi 4 mục và **không mục nào** hỏi về
//!   tiến trình nền hay subagent; `sessions.rs` không có một chỗ nào theo dõi
//!   tiến trình con của phiên (tìm `nohup`/`disown`/`child process` = 0); và
//!   `pending_for_display` ép số subagent về 0 khi phiên đã chết.
//!
//! Một chi tiết đã suýt làm vá nhầm chỗ, ghi lại để đừng ai đi lại: đường BÀN
//! GIAO (`start_fresh_after_handover`) **giữ nguyên `cwd` của phiên cũ**, nên nó
//! không tự sinh ra lỗ này — nó **DI TRUYỀN** lỗ từ `/new`, chỗ duy nhất ép
//! `workspace_root`. Vá ở `start_background` là vá ở gốc.

use std::fs;
use std::path::PathBuf;

use huba::sessions::nhac_luat_du_an;

fn thu_muc_tam(ten: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("huba-thu-{}-{ten}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).expect("dựng được thư mục tạm");
    d
}

#[test]
fn co_claude_md_thi_nhac_doc() {
    let d = thu_muc_tam("co-luat");
    fs::write(d.join("CLAUDE.md"), "# luật của dự án thử\n").unwrap();
    let ra = nhac_luat_du_an("[duan] sửa lịch", &d);
    assert!(
        ra.starts_with("[duan] sửa lịch"),
        "đề bài gốc phải còn: {ra}"
    );
    assert!(ra.contains("TRƯỚC KHI LÀM"), "thiếu câu nhắc: {ra}");
    assert!(
        ra.contains(&d.join("CLAUDE.md").display().to_string()),
        "phải nêu ĐƯỜNG DẪN thật, không nói chung chung: {ra}"
    );
}

/// Chiều ngược, và nó là chiều dễ quên: dự án KHÔNG có `CLAUDE.md` thì đừng bịa
/// ra một tệp không tồn tại. Một câu nhắc đọc tệp ma là dạy phiên mới mở đầu
/// bằng một lượt đọc hỏng.
#[test]
fn khong_co_claude_md_thi_khong_bia_ra() {
    let d = thu_muc_tam("khong-luat");
    let ra = nhac_luat_du_an("[duan] sửa lịch", &d);
    assert_eq!(ra, "[duan] sửa lịch", "không được thêm gì: {ra}");
}

/// Đề bài RỖNG phải ở nguyên rỗng — `start_background` không truyền tham số vị
/// trí nào khi rỗng, và đó là cách "mở cửa sổ rồi gõ sau" hoạt động. Nhét câu
/// nhắc vào đây là biến nó thành một phiên có đề bài, tức đổi hạng phiên.
#[test]
fn de_bai_rong_van_rong_du_co_claude_md() {
    let d = thu_muc_tam("rong");
    fs::write(d.join("CLAUDE.md"), "# luật\n").unwrap();
    assert_eq!(nhac_luat_du_an("", &d), "");
    assert_eq!(nhac_luat_du_an("   ", &d), "");
}

// ─────────────────────── bản bàn giao ───────────────────────
//
// `HANDOVER_PROMPT` là `const` riêng tư, nên đọc thẳng mã nguồn — đúng nếp
// `focus_stays_where_the_owner_put_it.rs` đã dùng: khoá một câu chữ có thật
// trong tệp, không phải nới API chỉ để kiểm được.

const NGUON: &str = include_str!("../src/sessions.rs");

fn khoi_han_giao() -> &'static str {
    let bat_dau = NGUON
        .find("const HANDOVER_PROMPT")
        .expect("không thấy HANDOVER_PROMPT — tên đã đổi thì sửa bài này");
    let con_lai = &NGUON[bat_dau..];
    let het = con_lai.find("\";").expect("khối chuỗi không kết thúc") + 2;
    &con_lai[..het]
}

#[test]
fn ban_giao_hoi_ve_viec_dang_chay_nen() {
    let k = khoi_han_giao();
    assert!(
        k.contains("CHẠY NỀN"),
        "bản bàn giao vẫn không hỏi gì về việc đang chạy nền: {k}"
    );
    assert!(
        k.contains("subagent"),
        "subagent chết theo tiến trình cha — không hỏi thì không ai chạy lại: {k}"
    );
    assert!(
        k.contains("LỆNH chạy lại"),
        "biết là có việc đang chạy mà không có lệnh chạy lại thì vẫn kẹt: {k}"
    );
}

/// Vắng mặt phải được KHAI, không được suy ra từ im lặng. Không có mục này thì
/// một bản bàn giao quên mục 5 đọc y hệt một bản bàn giao khai "không có gì".
#[test]
fn khong_co_viec_nen_thi_phai_noi_ra_chu_khong_im() {
    assert!(
        khoi_han_giao().contains("không có"),
        "phải bắt ghi rõ \"không có\" — im lặng và quên đọc giống hệt nhau"
    );
}

/// Số mục trong câu mở đầu phải khớp số mục đánh số bên dưới. Đây là chỗ trôi
/// êm nhất khi ai đó thêm mục 6: câu vẫn đọc trôi chảy, và mô hình vẫn viết 5.
#[test]
fn so_muc_khai_o_dau_khop_so_muc_that() {
    let k = khoi_han_giao();
    let dem = (1..=9).filter(|i| k.contains(&format!("{i}. "))).count();
    assert_eq!(dem, 5, "đếm được {dem} mục đánh số trong khối");
    assert!(
        k.contains("đúng 5 mục"),
        "câu mở đầu vẫn khai số mục cũ trong khi bên dưới có {dem} mục"
    );
}
