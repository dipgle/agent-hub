//! Danh sách phiên phải nói THỨ NGÓN TAY LÀM ĐƯỢC, không phải "còn sống".
//!
//! 🔴 Hà 2026-09-06, ảnh chụp `📋 2 phiên đang sống · đều tự duyệt`, bấm vào cả
//! hai và không làm được gì với cái nào: *"Ghi là phiên đang sống nhưng lại chỉ
//! là lịch sử, thật buồn cười, tôi có cần cái này đâu"* — rồi sau đó: *"Ko thể
//! hiểu nổi, bạn làm tôi thấy mất niềm tin quá"*.
//!
//! Đo lại đúng lúc ấy thì cả hai hàng ĐỀU là tiến trình còn sống (`ps` xác
//! nhận, pid 6112 và 68554). Nhưng cả hai `can_type=false`, `working=false`, im
//! 154 và 176 phút, và một hàng có lời cuối đúng là *"Đã ghi và đóng phiên"*.
//!
//! Bài học, và cũng là thứ tệp này gác: **"còn sống" đo sự tồn tại của một tiến
//! trình, còn người đọc hỏi một câu khác hẳn — "tôi mở nó ra thì làm được gì".**
//! Một con số không bao giờ ở trạng thái ngược với câu hỏi ấy thì không phải
//! phép đo (`CLAUDE.md` §13).
//!
//! Mỗi cổng ở đây đi kèm ĐỐI CHỨNG NGƯỢC: cấy ca "mọi phiên gõ được" thì câu cũ
//! phải quay lại nguyên văn. Không có vế ấy thì bài kiểm chỉ chứng minh chuỗi
//! mới có tồn tại, chứ không chứng minh nó BIẾT phân biệt.

use huba::sessions::LiveSession;

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Hàng gõ được: có tty thật, và phép dò tab tìm thấy cửa sổ của nó.
fn go_duoc(id: &str, tty: &str) -> LiveSession {
    LiveSession {
        session_id: id.into(),
        host: "terminal".into(),
        tty: tty.into(),
        can_type: true,
        ..Default::default()
    }
}

/// Hàng NỀN — đúng hình dạng `[fbot]` và `[dwork/a-ddoc]` hôm 06/09: `ps` in
/// `??` vì bg-pty-host `setsid`, nên không có cửa sổ nào TỒN TẠI để mà gõ.
fn nen(id: &str) -> LiveSession {
    LiveSession {
        session_id: id.into(),
        host: "background".into(),
        tty: "??".into(),
        can_type: false,
        ..Default::default()
    }
}

fn tieu_de(rows: &[LiveSession]) -> String {
    huba::pipeline::session_list_text(rows, "", now())
        .lines()
        .next()
        .expect("danh sách phải có ít nhất một dòng")
        .to_string()
}

/// Ca của Hà, nguyên hình: một phiên gõ được, hai phiên nền.
#[test]
fn a_mixed_list_stops_calling_them_all_alive() {
    let rows = vec![
        go_duoc("c3ea24fe", "ttys000"),
        nen("b4182616"),
        nen("f0d2465a"),
    ];
    let head = tieu_de(&rows);

    assert!(
        head.contains("1 gõ được") && head.contains("2 chỉ đọc"),
        "tiêu đề không tách được hai loại: {head}"
    );
    // Và câu cũ phải BIẾN MẤT, không chỉ bị đẩy sang một bên: chừng nào nó còn
    // đứng đó thì con số vẫn hứa đúng cái nó không giao được.
    assert!(
        !head.contains("đang sống"),
        "vẫn còn hứa 'đang sống' cho một danh sách 2/3 không gõ được: {head}"
    );
}

/// ĐỐI CHỨNG NGƯỢC của bài trên — không có nó thì bài trên chỉ chứng minh chuỗi
/// mới có tồn tại, chứ không chứng minh nó phân biệt được gì.
#[test]
fn all_typeable_keeps_the_old_sentence() {
    let rows = vec![
        go_duoc("aaa", "ttys000"),
        go_duoc("bbb", "ttys001"),
        go_duoc("ccc", "ttys002"),
    ];
    let head = tieu_de(&rows);

    assert!(
        head.contains("3 phiên đang sống"),
        "mọi phiên gõ được mà câu cũ không quay lại: {head}"
    );
    assert!(
        !head.contains("gõ được ·") && !head.contains("chỉ đọc"),
        "không có gì để tách mà vẫn tách, thành một dòng chữ thừa: {head}"
    );
}

/// "Dò hỏng" KHÔNG được đọc thành "không phiên nào gõ được".
///
/// `mark_can_type` để nguyên `can_type=false` cho MỌI hàng khi danh sách tab
/// rỗng (`osascript` hỏng, Terminal chưa mở, quyền bị rút). Nếu tiêu đề tin
/// thẳng con số ấy thì một lượt dò hỏng in ra `0 gõ được` — đọc y hệt một sự
/// thật, trong khi nó là **chưa đo được**. Đó đúng là con bug đang sửa, mặc bộ
/// đồ khác, nên nó phải có cổng riêng.
#[test]
fn a_failed_tab_probe_is_not_the_same_as_nothing_typeable() {
    // Hai hàng CÓ tty thật (cửa sổ tồn tại), nhưng phép dò không trả về tab nào.
    let mut a = go_duoc("aaa", "ttys000");
    let mut b = go_duoc("bbb", "ttys001");
    a.can_type = false;
    b.can_type = false;
    let head = tieu_de(&vec![a.clone(), b.clone()]);

    assert!(
        !head.contains("0 gõ được"),
        "một lượt dò hỏng bị in ra như một sự thật: {head}"
    );
    assert!(
        head.contains("2 phiên đang sống"),
        "chưa đo được thì phải giữ nguyên câu cũ, không tô màu: {head}"
    );

    // ĐỐI CHỨNG NGƯỢC: chỉ cần MỘT hàng `can_type=true` là có bằng chứng phép
    // dò CÓ chạy — lúc ấy `false` của hàng kia mới là số đo, và phải được tin.
    // (Đây cũng đúng ca terminal tích hợp VS Code: `host: "terminal"`, tty thật,
    // nhưng Terminal.app không biết cái tty ấy.)
    b.can_type = true;
    let head = tieu_de(&vec![a, b]);
    assert!(
        head.contains("1 gõ được") && head.contains("1 chỉ đọc"),
        "phép dò đã chạy mà số đo của nó vẫn bị bỏ: {head}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Lời cuối phải mang theo TUỔI của nó
// ─────────────────────────────────────────────────────────────────────────────

/// Nhật ký có `timestamp` trên mỗi dòng; bản cũ đọc chữ rồi bỏ mốc lại trên sàn,
/// nên `/shot` phiên nền dán một câu già ba tiếng vào sau chữ "📷" mà không một
/// dấu hiệu nào cho biết nó cũ.
#[test]
fn the_last_word_carries_the_moment_it_was_written() {
    let tail = concat!(
        r#"{"type":"assistant","timestamp":"2026-09-05T18:45:09.664Z","message":{"role":"assistant","content":[{"type":"text","text":"Quality-gate báo đỏ ở mục bí mật."}]}}"#,
        "\n",
        r#"{"type":"assistant","timestamp":"2026-09-05T18:45:11.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#,
    );
    let (said, at) = huba::sessions::last_prose_at(tail, 2000).expect("phải có lời cuối");

    assert_eq!(said, "Quality-gate báo đỏ ở mục bí mật.");
    // Mốc phải là của DÒNG CÓ CHỮ, không phải của dòng cuối tệp: lượt cuối là
    // một `tool_use` thuần, và lấy mốc của nó là nói sai tuổi của câu đang hiện.
    assert_eq!(
        at.as_deref(),
        Some("2026-09-05T18:45:09.664Z"),
        "lấy nhầm mốc của lượt gọi công cụ đứng sau"
    );
}

/// ĐỐI CHỨNG NGƯỢC: dòng KHÔNG khai `timestamp` thì vế mốc phải là `None`.
///
/// "Không biết tuổi" là một trạng thái RIÊNG. Bịa cho nó một mốc (giờ hiện tại,
/// hay mốc của dòng khác) chính là con bug vừa sửa, chỉ đổi chỗ đứng.
#[test]
fn a_line_without_a_timestamp_says_it_does_not_know() {
    let tail = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Đã cài xong, daemon chạy lại rồi."}]}}"#;
    let (said, at) = huba::sessions::last_prose_at(tail, 2000).expect("phải có lời cuối");

    assert_eq!(said, "Đã cài xong, daemon chạy lại rồi.");
    assert_eq!(at, None, "không có mốc mà vẫn bịa ra một cái: {at:?}");
}

/// Cửa cũ không được vỡ: `last_prose` vẫn trả đúng chữ như trước.
#[test]
fn the_old_door_still_returns_just_the_words() {
    let tail = r#"{"type":"assistant","timestamp":"2026-09-05T18:45:09.664Z","message":{"role":"assistant","content":[{"type":"text","text":"Chạy nốt bộ test đây."}]}}"#;
    assert_eq!(
        huba::sessions::last_prose(tail, 2000).as_deref(),
        Some("Chạy nốt bộ test đây."),
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Không có cửa sổ ⟹ nó là CON của ai đó, không phải phiên ngang hàng
// ─────────────────────────────────────────────────────────────────────────────

/// Hà 2026-09-06: *"lệnh chạy khác phát sinh từ mcp làm gì có cửa sổ để nhìn
/// thấy, nó đều là con hết chứ"*. Mối cha–con thật nằm trong argv của
/// `claude daemon run`, dạng đo được trên máy này lúc 05:0x.
#[test]
fn the_spawner_pid_is_read_from_the_real_command_line() {
    let that = concat!(
        "/Users/hanguyen/.npm-global/lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe ",
        "daemon run --origin transient ",
        r#"--spawned-by {"label":"claude","cwd":"/Users/hanguyen/projects/fbot","pid":39500}"#
    );
    assert_eq!(huba::sessions::spawned_by_pid_in(that), Some(39500));
}

/// ĐỐI CHỨNG NGƯỢC ①: dòng lệnh CÓ `"pid":` nhưng KHÔNG có `--spawned-by` thì
/// không được vơ lấy con số ấy. Đây là ca dễ hỏng nhất — argv của CLI đầy JSON.
#[test]
fn a_pid_outside_spawned_by_is_not_a_parent() {
    let that = r#"claude.exe --config {"telemetry":{"pid":777}} --session-id abc"#;
    assert_eq!(huba::sessions::spawned_by_pid_in(that), None);
}

/// ĐỐI CHỨNG NGƯỢC ②: có `--spawned-by` mà không có khoá `pid` ⟹ None, không 0.
#[test]
fn spawned_by_without_a_pid_key_reads_as_unknown() {
    let that = r#"claude.exe daemon run --spawned-by {"label":"claude","cwd":"/x"}"#;
    assert_eq!(huba::sessions::spawned_by_pid_in(that), None);
}

/// MỒ CÔI phải là ba trạng thái chứ không phải hai — và "chưa đo được" KHÔNG
/// được đọc thành "cha đã chết".
#[test]
fn unknown_parentage_is_not_the_same_as_orphaned() {
    let mut mo_coi = nen("b4182616");
    mo_coi.spawned_by_pid = Some(39500);
    mo_coi.spawner_alive = Some(false);
    assert!(huba::sessions::is_orphan(&mo_coi), "cha đã thoát mà không gọi là mồ côi");

    // ĐỐI CHỨNG: chưa đọc được `--spawned-by` ⟹ KHÔNG phải mồ côi.
    let mut chua_do = nen("b4182616");
    chua_do.spawner_alive = None;
    assert!(
        !huba::sessions::is_orphan(&chua_do),
        "dán nhãn mồ côi cho một phiên chỉ vì không đọc nổi argv của nó"
    );

    // ĐỐI CHỨNG: cha còn sống ⟹ là CON, không phải mồ côi.
    let mut con = nen("b4182616");
    con.spawner_alive = Some(true);
    assert!(!huba::sessions::is_orphan(&con));

    // ĐỐI CHỨNG: có cửa sổ trên màn thì không bao giờ là mồ côi, kể cả khi
    // cha đã thoát — nó tự đứng được, chủ máy nhìn thấy nó.
    let mut co_man = go_duoc("aaa", "ttys000");
    co_man.spawner_alive = Some(false);
    assert!(!huba::sessions::is_orphan(&co_man));
}

/// `on_screen` phải đọc tty, KHÔNG đọc `can_type` — dò tab hỏng thì mọi hàng
/// `can_type=false`, và lấy nó làm cửa sẽ xoá sạch danh sách trong im lặng.
#[test]
fn on_screen_survives_a_failed_tab_probe() {
    let mut vscode = go_duoc("aaa", "ttys000");
    vscode.can_type = false; // Terminal.app không biết tty này (hoặc dò hỏng)
    assert!(
        huba::sessions::on_screen(&vscode),
        "một lượt dò hỏng vừa làm phiên biến mất khỏi danh sách"
    );
    assert!(!huba::sessions::on_screen(&nen("b4182616")));
}

/// Dòng chân phải GỌN bằng đúng việc làm được, mà vẫn không gộp ba hình dạng.
///
/// Hà 06/09: *"Màn mồ côi để làm gì, có thao tác được đâu"*. Với mồ côi thì chỉ
/// còn một việc — dừng nó — nên khối ấy co lại thành một dòng mang sẵn lệnh dọn.
/// Nhưng con của một phiên CÒN SỐNG thì tuyệt đối không được lẫn vào đó.
#[test]
fn the_footer_is_one_line_of_the_only_thing_you_can_do() {
    let mut mo_coi = nen("b4182616");
    mo_coi.spawned_by_pid = Some(39500);
    mo_coi.spawner_alive = Some(false);

    let mut mo_coi2 = nen("f0d2465a");
    mo_coi2.spawned_by_pid = Some(28204);
    mo_coi2.spawner_alive = Some(false);

    let note = huba::pipeline::offscreen_note(&[
        go_duoc("aaa", "ttys000"),
        mo_coi,
        mo_coi2,
    ]);

    assert!(note.contains("2 mồ côi"), "{note}");
    assert!(
        note.contains("/stop b4182616") && note.contains("/stop f0d2465a"),
        "dòng mồ côi không mang sẵn lệnh dọn cho từng cái: {note}"
    );
    // GỌN là một yêu cầu đo được, không phải cảm giác: hai mồ côi ⟹ MỘT dòng.
    let so_dong = note.trim().lines().count();
    assert_eq!(so_dong, 1, "hai mồ côi mà tốn {so_dong} dòng: {note}");
    assert!(!note.contains("aaa"), "phiên đang mở cửa sổ bị hạ xuống làm con: {note}");
}

/// Con của một phiên CÒN SỐNG không được lẫn vào dòng dọn — và "chưa đọc được
/// cha" cũng vậy. Gộp cho gọn thêm một dòng nữa là dọn nhầm thứ đang có người dùng.
#[test]
fn a_child_with_a_living_parent_is_never_offered_for_cleanup() {
    let mut co_cha = nen("f0d2465a");
    co_cha.parent_name = Some("[dwork]".into());

    let chua_do = nen("cccccccc"); // spawner_alive = None

    let mut mo_coi = nen("b4182616");
    mo_coi.spawned_by_pid = Some(39500);
    mo_coi.spawner_alive = Some(false);

    let note = huba::pipeline::offscreen_note(&[co_cha, chua_do, mo_coi]);

    assert!(note.contains("1 mồ côi") && note.contains("/stop b4182616"), "{note}");
    assert!(note.contains("đừng dọn"), "{note}");
    assert!(note.contains("dưới [dwork]"), "con không được xếp dưới cha: {note}");
    assert!(note.contains("chưa đọc được cha"), "{note}");
    // 🔴 Vế quan trọng nhất: chỉ ĐÚNG MỘT id được đề nghị dọn.
    assert_eq!(
        note.matches("/stop ").count(),
        1,
        "đề nghị dọn cả thứ còn cha hoặc chưa đo được: {note}"
    );
    assert!(!note.contains("/stop f0d2465a"), "suýt dọn nhầm con của [dwork]: {note}");
    assert!(!note.contains("/stop cccccccc"), "suýt dọn nhầm thứ chưa đo được: {note}");
}

/// ĐỐI CHỨNG NGƯỢC: mọi phiên đều có cửa sổ ⟹ KHÔNG có dòng chân nào.
#[test]
fn nothing_offscreen_means_no_footer_at_all() {
    let note = huba::pipeline::offscreen_note(&[
        go_duoc("aaa", "ttys000"),
        go_duoc("bbb", "ttys001"),
    ]);
    assert!(note.is_empty(), "dựng một dòng chân rỗng nghĩa: {note:?}");
}
