//! `/new` lui về phiên nền thì phải NÓI RA — và nút phiên dùng chung bảng icon.
//!
//! 🔴 Hà 2026-08-19: *"Tôi dùng lệnh `/new acc1 tiếp social` sau đó và phiên lại
//! báo không có cửa sổ là sao"*. Lệnh không sai gì cả. Đo trong log:
//!
//! ```text
//! 10:43:05  new_in_terminal_failed  err="osascript quá 20s"  falling_back_to="--bg"
//! ```
//!
//! Tức hub thử mở cửa sổ, `osascript` hết 20 giây không trả lời, nên nó lui về
//! `--bg` — đúng đường lui đã thiết kế. Cái sai là nó lui trong IM LẶNG: câu
//! chào chỉ đổi hai chữ (`⌨ cửa sổ Terminal` → `🌙 phiên nền`), còn lý do thì
//! nằm trong log — chỗ người cầm điện thoại không đọc được. Anh chỉ phát hiện
//! lúc bấm `/shot` và nhận *"không có cửa sổ terminal để gõ"*.
//!
//! Luật 3 (*"không có lỗi im lặng"*) đọc ở tầng NGƯỜI.

use hub::sessions::{LiveSession, ST_ASK, ST_BG, ST_DEAD, ST_ERR, ST_RUN, ST_WAIT};

/// Nút phiên và dòng chữ phải dùng CÙNG MỘT bảng — không phải hai bản chép.
///
/// 🔴 Chú thích cũ ở `session_button_label` hứa đúng câu ấy (*"cùng bộ chấm với
/// `session_list_text`"*) và giữ nó bằng tay: hai bảng `match` gõ song song.
/// Nó gãy ngay lượt đổi đầu tiên — dòng chữ thành `💤` trong khi nút vẫn `🟡`,
/// và Hà đọc được trên ảnh. Bài kiểm này là chỗ lời hứa ấy được cưỡng chế.
#[test]
fn the_button_and_the_line_agree_on_every_state() {
    let mk = |f: &dyn Fn(&mut LiveSession)| {
        let mut s = LiveSession {
            session_id: "aaaaaaaa-0000".into(),
            host: "interactive".into(),
            account: "acc1".into(),
            ..Default::default()
        };
        f(&mut s);
        s
    };
    /// Một ca đo: cách bẻ hàng, và cái icon nó phải cho ra.
    ///
    /// Đặt tên cho kiểu vì `cargo clippy --all-targets -- -D warnings` gọi bản
    /// viết thẳng là `type_complexity` — và luật của kho này là **0 warning**
    /// (`CLAUDE.md`, mục Stack), nên cái tên này rẻ hơn một dòng `allow`.
    type Ca = (Box<dyn Fn(&mut LiveSession)>, &'static str);
    let cases: Vec<Ca> = vec![
        (Box::new(|s: &mut LiveSession| s.working = true), ST_RUN),
        (Box::new(|_: &mut LiveSession| {}), ST_WAIT),
        (Box::new(|s: &mut LiveSession| s.bg_shell = true), ST_BG),
        (
            Box::new(|s: &mut LiveSession| s.asking = Some(Default::default())),
            ST_ASK,
        ),
        (
            Box::new(|s: &mut LiveSession| s.error = Some("bùm".into())),
            ST_ERR,
        ),
        (
            Box::new(|s: &mut LiveSession| s.host = "dead".into()),
            ST_DEAD,
        ),
    ];
    for (f, want) in cases {
        let s = mk(&*f);
        let line = hub::pipeline::session_list_text(std::slice::from_ref(&s), "", 0);
        let button = hub::pipeline::session_button_label(&s);
        assert!(line.contains(want), "dòng chữ thiếu {want}: {line}");
        assert!(button.contains(want), "nút thiếu {want}: {button}");
    }
}

/// Đường lui phải mang theo LÝ DO, và câu chào phải in nó ra.
#[test]
fn the_fallback_carries_its_reason() {
    let started = hub::sessions::Started {
        session_id: "ed3e3d81-99ca".into(),
        project: String::new(),
        cwd: "/Users/hanguyen/projects".into(),
        task: "tiếp social".into(),
        ts: "2026-08-19T10:43:17Z".into(),
        window: false,
        fallback_why: Some("osascript quá 20s".into()),
    };
    // Trường này là thứ câu chào đọc; không có nó thì lý do chỉ nằm trong log.
    assert_eq!(started.fallback_why.as_deref(), Some("osascript quá 20s"));
    assert!(!started.window);

    // …và đường CHÍNH thì không được bịa ra lý do nào.
    let ok = hub::sessions::Started {
        window: true,
        fallback_why: None,
        ..started
    };
    assert!(ok.fallback_why.is_none());
}
