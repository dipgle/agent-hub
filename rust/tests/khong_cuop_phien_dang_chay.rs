//! Cửa sổ vừa mở KHÔNG được nhận bừa một phiên đang chạy ở cửa sổ khác.
//!
//! 🔴 Ca đo được 2026-09-12 18:36, và nó là ca tệ nhất trong họ này vì nó IM
//! LẶNG — không dòng lỗi nào, chỉ có con trỏ của điện thoại lặng lẽ trỏ sai:
//!
//! ```text
//! 11:36:11Z new_window_opened                  tty=ttys017 window=392
//! 11:36:30Z new_session_matched_by_transcript  session=54bd8153… tty=ttys017
//! ```
//!
//! `54bd8153` là phiên `[huba]` đang chạy ở **ttys009**, acc1 — không liên quan
//! gì tới cửa sổ acc4 vừa mở. huba nhắn *"Đã mở cửa sổ Terminal. Phiên
//! 54bd8153 — nay đang theo phiên này"*, nên mọi câu chủ máy gõ tiếp đi thẳng
//! vào một phiên KHÁC đang làm việc dở; còn phiên thật vừa sinh thì không ai
//! biết id. Hà: *"danh sách phiên cũng không thấy phiên mới đâu"*.
//!
//! Vì sao lối đoán ấy sai một cách có hệ thống: nó hỏi *"tệp nhật ký nào vừa
//! được ghi"*, mà `projects` của acc2·acc3·acc4 đều là **liên kết mềm về
//! `~/.claude/projects`** — một gốc chung. Nên một phiên đang gõ liên tục luôn
//! thắng cuộc đua, còn phiên mới thì 15–60 giây nữa mới có tệp.
//!
//! Chú thích trong `wait_for_new_session_id` đã kể đúng con bug này từ
//! 2026-08-15 (ba lượt ghép nhầm liên tiếp) và đã vá bằng cách hỏi tty TRƯỚC.
//! Nhưng lối đoán cũ ở lại làm đường lui — nên nó quay về nguyên vẹn đúng vào
//! ngày phép hỏi-theo-tty chậm một nhịp. **Vá một tầng không nói gì về tầng kia.**

use huba::sessions::id_bound_elsewhere_in;

/// Dựng lại ĐÚNG bảng của 18:36: phiên `[huba]` ở ttys009, cửa sổ vừa mở ttys017.
fn so_that() -> Vec<(String, String)> {
    vec![
        (
            "54bd8153-4dfb-49f1-ad29-6ec1d551c035".into(),
            "ttys009".into(),
        ),
        (
            "6fc47e02-8884-4d72-98db-7f2ad5d30781".into(),
            "ttys011".into(),
        ),
    ]
}

#[test]
fn phien_dang_song_o_cua_so_khac_thi_bi_tu_choi() {
    assert!(
        id_bound_elsewhere_in(
            &so_that(),
            "54bd8153-4dfb-49f1-ad29-6ec1d551c035",
            "ttys017"
        ),
        "đây là cú cướp phiên đã xảy ra thật — phải chặn"
    );
}

/// Đối chứng NGƯỢC, và nó là vế bắt buộc: nếu hàm này trả `true` cho mọi thứ
/// thì bài trên vẫn xanh mà tính năng thì chết hẳn (không phiên mới nào được
/// nhận nữa).
#[test]
fn dung_cua_so_ay_thi_van_nhan() {
    assert!(
        !id_bound_elsewhere_in(
            &so_that(),
            "54bd8153-4dfb-49f1-ad29-6ec1d551c035",
            "ttys009"
        ),
        "chính cửa sổ của nó thì không phải là 'cửa sổ khác'"
    );
    assert!(
        !id_bound_elsewhere_in(
            &so_that(),
            "aaaaaaaa-0000-0000-0000-000000000000",
            "ttys017"
        ),
        "phiên mới toanh chưa có trong sổ ⟹ không có bằng chứng nó thuộc về ai khác ⟹ nhận"
    );
}

/// `/dev/ttysNNN` và `ttysNNN` là cùng một cửa sổ — `ps` in kiểu này, AppleScript
/// kiểu kia. So chuỗi trần ở đây là mở lại đúng cái cửa vừa đóng.
#[test]
fn hai_loi_viet_tty_la_mot_cua_so() {
    let so = vec![("x".to_string(), "/dev/ttys009".to_string())];
    assert!(!id_bound_elsewhere_in(&so, "x", "ttys009"));
    assert!(id_bound_elsewhere_in(&so, "x", "ttys017"));
}

/// `??` không phải một cửa sổ (luật 11b) — một phiên không có tty điều khiển
/// không "chiếm" cửa sổ nào, nên nó không được dùng làm cớ để từ chối.
#[test]
fn khong_co_tty_thi_khong_chiem_cua_so_nao() {
    let so = vec![
        ("x".to_string(), "??".to_string()),
        ("y".to_string(), "".to_string()),
        ("z".to_string(), "-".to_string()),
    ];
    for id in ["x", "y", "z"] {
        assert!(
            !id_bound_elsewhere_in(&so, id, "ttys017"),
            "`{id}` không có cửa sổ thật ⟹ không chặn được ai"
        );
    }
}
