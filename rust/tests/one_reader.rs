//! Luật 1 của kênh Telegram — **đúng MỘT nơi đọc `getUpdates`** — kiểm được.
//!
//! 🔴 Vì sao có tệp này. Luật ấy đã nằm ở đầu `telegram.rs` từ 2026-08-11, và
//! đã bị vi phạm suốt bởi chính đoạn mã viết ra để giữ nó: `confirm::ask` mở
//! vòng đọc thứ hai rồi chặn vòng chính bằng cờ `busy`. Cờ bật được, nhưng vòng
//! chính lúc ấy đang nằm giữa một long-poll 20 giây và không ai gọi nó về — nên
//! trong tối đa 20 giây, hai vòng cùng hỏi.
//!
//! Đo trên `logs/huba.log` ngày 2026-08-16, TRƯỚC khi vá: **11 lượt**
//! `telegram_poll_rejected` (*"Conflict: terminated by other getUpdates
//! request"*), 5 trong số đó nằm gọn trong 10 phút Hà đóng mấy cửa sổ trần từ
//! điện thoại. Mỗi lượt kèm một giấc ngủ phạt 30 giây của vòng đọc chính, tức
//! 30 giây huba điếc ngay sau mỗi câu hỏi xác nhận.
//!
//! Đó là bằng chứng ĐỎ của bài kiểm này, và nó đến từ máy thật chứ không từ một
//! bản dựng: hành vi "hai tiến trình giành nhau một long-poll" không quan sát
//! được trong một bài kiểm không có mạng. Nên ở đây soi **mã nguồn** — cùng lối
//! với `tests/cycle_wiring.rs`, và cùng lý do.

fn src(name: &str) -> String {
    let p = format!("{}/src/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("đọc được {p}: {e}"))
}

/// Danh sách tệp được phép gọi `getUpdates`, kèm lý do. Thêm tên vào đây là một
/// quyết định, không phải một dòng dọn dẹp.
const READERS: [(&str, &str); 2] = [
    (
        "telegram.rs",
        "vòng đọc DUY NHẤT của huba (`Inbox::read_forever`)",
    ),
    (
        "confirm.rs",
        "đường LÙI cho lúc không có hòm thư nền — CLI một lượt, kênh tắt",
    ),
];

#[test]
fn nobody_else_opens_a_second_read_loop() {
    let dir = format!("{}/src", env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    for e in std::fs::read_dir(&dir).expect("đọc được src/") {
        let p = e.expect("mục tin").path();
        if p.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let name = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if READERS.iter().any(|(f, _)| *f == name) {
            continue;
        }
        let body = std::fs::read_to_string(&p).expect("đọc được tệp nguồn");
        // Chỉ tính LỜI GỌI, không tính chữ trong chú thích hay chuỗi hướng dẫn
        // (`setup.rs` in ra một đường dẫn getUpdates cho chủ máy tự mở).
        if body.contains("api(\"getUpdates\")") {
            offenders.push(name);
        }
    }
    assert!(
        offenders.is_empty(),
        "{offenders:?} mở vòng đọc thứ hai. Hai vòng đọc song song thì Telegram \
         từ chối một bên (Conflict) và cú bấm rơi vào nhầm vòng — xem luật 1 ở \
         đầu telegram.rs. Được phép: {READERS:?}"
    );
}

/// Vòng đọc chính không được có cửa "đứng im nhường ai đó" nữa: cửa ấy chính là
/// cái cờ `busy` đã không giữ nổi luật 1.
#[test]
fn the_main_loop_no_longer_parks_itself() {
    let tg = src("telegram.rs");
    assert!(
        !tg.contains("busy: Arc<AtomicBool>"),
        "cờ `busy` quay lại — nó không chặn được một long-poll đang chạy dở, \
         nên nó chỉ tạo cảm giác an toàn cộng một lượt Conflict mỗi câu hỏi"
    );
    assert!(
        tg.contains("fn deliver_confirm"),
        "mất `deliver_confirm` ⟹ cú bấm xác nhận không còn đường tới người đang chờ"
    );
}

/// Cú bấm phải được GIAO trước khi bị đem đi xử lý như một cái nút thường —
/// nếu không, `handle_update` sẽ trả lời *"câu hỏi đã đóng sổ"* cho đúng cú bấm
/// mà `confirm::ask` đang ngồi chờ.
#[test]
fn handle_update_delivers_before_it_declares_the_question_closed() {
    let all = src("telegram.rs");
    // Đo BÊN TRONG `handle_update`, không đo cả tệp: chú thích đầu tệp cũng nhắc
    // `telegram_confirm_button_late`, và bản đầu của bài kiểm này bắt được chính
    // dòng chú thích ấy rồi báo đỏ. Assert đỏ thì kiểm phép đo trước.
    let start = all
        .find("fn handle_update(")
        .expect("không còn `handle_update` — đổi tên thì sửa cả bài kiểm này");
    let tg = &all[start..];
    let deliver = tg
        .find("self.deliver_confirm(data)")
        .expect("`handle_update` phải giao cú bấm cho người đang chờ");
    let late = tg
        .find("telegram_confirm_button_late")
        .expect("vẫn phải còn câu trả lời cho cú bấm tới muộn");
    assert!(
        deliver < late,
        "nhánh 'đã đóng sổ' đứng TRƯỚC chỗ giao hàng ⟹ mọi cú bấm đều thành 'muộn'"
    );
}

/// Đường lùi phải đọc được lời TỪ CHỐI của Telegram. Bản cũ đọc thẳng `result`,
/// nên một lời từ chối ra đúng hình dạng "không có update nào" và hàm ngồi hết
/// 90 giây rồi kết luận *"không ai bấm"* — lỗi im lặng, ở ngay chỗ đang hỏi chủ
/// máy có cho phép hay không.
#[test]
fn the_fallback_reads_a_refusal_as_a_refusal() {
    let cf = src("confirm.rs");
    assert!(
        cf.contains("poll_rejected(&resp)"),
        "đường lùi không kiểm `poll_rejected` ⟹ Conflict/token sai đọc ra thành \
         'không ai bấm', và huba im lặng không làm gì"
    );
}
