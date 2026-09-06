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
    let head = tieu_de(&[a.clone(), b.clone()]);

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
    let head = tieu_de(&[a, b]);
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
    assert!(
        huba::sessions::is_orphan(&mo_coi),
        "cha đã thoát mà không gọi là mồ côi"
    );

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

    let note = huba::pipeline::offscreen_note(&[go_duoc("aaa", "ttys000"), mo_coi, mo_coi2]);

    assert!(note.contains("2 mồ côi"), "{note}");
    assert!(
        note.contains("/stop b4182616") && note.contains("/stop f0d2465a"),
        "dòng mồ côi không mang sẵn lệnh dọn cho từng cái: {note}"
    );
    // GỌN là một yêu cầu đo được, không phải cảm giác: hai mồ côi ⟹ MỘT dòng.
    let so_dong = note.trim().lines().count();
    assert_eq!(so_dong, 1, "hai mồ côi mà tốn {so_dong} dòng: {note}");
    assert!(
        !note.contains("aaa"),
        "phiên đang mở cửa sổ bị hạ xuống làm con: {note}"
    );
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

    assert!(
        note.contains("1 mồ côi") && note.contains("/stop b4182616"),
        "{note}"
    );
    assert!(note.contains("đừng dọn"), "{note}");
    assert!(
        note.contains("dưới [dwork]"),
        "con không được xếp dưới cha: {note}"
    );
    assert!(note.contains("chưa đọc được cha"), "{note}");
    // 🔴 Vế quan trọng nhất: chỉ ĐÚNG MỘT id được đề nghị dọn.
    assert_eq!(
        note.matches("/stop ").count(),
        1,
        "đề nghị dọn cả thứ còn cha hoặc chưa đo được: {note}"
    );
    assert!(
        !note.contains("/stop f0d2465a"),
        "suýt dọn nhầm con của [dwork]: {note}"
    );
    assert!(
        !note.contains("/stop cccccccc"),
        "suýt dọn nhầm thứ chưa đo được: {note}"
    );
}

/// ĐỐI CHỨNG NGƯỢC: mọi phiên đều có cửa sổ ⟹ KHÔNG có dòng chân nào.
#[test]
fn nothing_offscreen_means_no_footer_at_all() {
    let note =
        huba::pipeline::offscreen_note(&[go_duoc("aaa", "ttys000"), go_duoc("bbb", "ttys001")]);
    assert!(note.is_empty(), "dựng một dòng chân rỗng nghĩa: {note:?}");
}

// ─────────────────────────────────────────────────────────────────────────────
// Tên làn phải có một nguồn ĐO ĐƯỢC, không chỉ lời tự khai
// ─────────────────────────────────────────────────────────────────────────────

/// Hà 06/09, ảnh năm hàng `[dwork]` giống hệt nhau: *"Các phiên dwork mất tên
/// làn rồi, ko phân biệt được"*. Đo ba nhật ký: 0 lời tự khai trong 256 KB cuối.
/// Nhánh git thì đo được — và bốn ca dưới đây là bốn hàng thật trong sổ ràng
/// buộc phiên lúc 07:2x.
#[test]
fn the_lane_is_read_from_the_git_branch() {
    assert_eq!(
        huba::sessions::lane_from_branch("lan/a-chung").as_deref(),
        Some("a-chung")
    );
    assert_eq!(
        huba::sessions::lane_from_branch("lan/a-ddoc").as_deref(),
        Some("a-ddoc")
    );
    assert_eq!(
        huba::sessions::lane_from_branch("lan/a-dci").as_deref(),
        Some("a-dci")
    );
}

/// ĐỐI CHỨNG NGƯỢC: cây CHÍNH không phải một làn — `[dwork]`, không `[dwork/main]`.
#[test]
fn the_trunk_is_not_a_lane() {
    assert_eq!(huba::sessions::lane_from_branch("main"), None);
    assert_eq!(huba::sessions::lane_from_branch("master"), None);
    // Nhánh chưa đọc được ⟹ KHÔNG có làn, và không được thành chuỗi rỗng đội lốt.
    assert_eq!(huba::sessions::lane_from_branch(""), None);
    assert_eq!(huba::sessions::lane_from_branch("   "), None);
    assert_eq!(huba::sessions::lane_from_branch("HEAD"), None);
    // Dấu `/` thừa không được đẻ ra một làn RỖNG — đó mới là nguy cơ thật.
    //
    // Bản đầu của bài kiểm này đòi `None`, và nó SAI: `lan/` cắt đuôi ra nhánh
    // tên `lan`, một làn hợp lệ theo luật chung (nhánh không phải trunk ⟹ làn là
    // đoạn cuối). Git cũng không tạo nổi tên nhánh có `/` ở cuối, nên ca này chỉ
    // là hình dạng méo. Thứ phải khoá là: không bao giờ ra `Some("")`.
    assert_ne!(
        huba::sessions::lane_from_branch("lan/").as_deref(),
        Some("")
    );
    assert_ne!(huba::sessions::lane_from_branch("///").as_deref(), Some(""));
}

// ─────────────────────────────────────────────────────────────────────────────
// Mồ côi: nói MỘT lần, và chỉ khi đã ĐO được cả hai vế
// ─────────────────────────────────────────────────────────────────────────────

use huba::watch::{changes, Change, Mark};
use std::collections::BTreeMap;

const NOW: i64 = 1_788_600_000;

fn mo_coi_that(id: &str) -> LiveSession {
    let mut s = nen(id);
    s.spawned_by_pid = Some(39500);
    s.spawner_alive = Some(false);
    s.name = "Fix mobile font size".into();
    s.folder = "fbot".into();
    s
}

/// Sổ đã thấy phiên này vòng trước, chưa từng báo mồ côi.
fn so_da_thay(id: &str) -> BTreeMap<String, Mark> {
    let mut m = BTreeMap::new();
    m.insert(
        id.to_string(),
        Mark {
            s: "idle".into(),
            f: NOW - 3600,
            m: false,
            ..Default::default()
        },
    );
    m
}

/// Hà 06/09: hai phiên mồ côi giữ 1203 MB, im 4,5 tiếng, không tự dừng. Nó phải
/// được BÁO — nhưng đúng một lần (luật 11: nói khi có THAY ĐỔI, không tụng lại).
#[test]
fn an_orphan_is_announced_exactly_once() {
    let s = mo_coi_that("b4182616-0000-0000-0000-000000000000");
    let prev = so_da_thay(&s.session_id);

    let (lan1, so_moi) = changes(&prev, std::slice::from_ref(&s), NOW, &[]);
    let bao: Vec<&Change> = lan1
        .iter()
        .filter(|c| matches!(c, Change::Orphaned { .. }))
        .collect();
    assert_eq!(bao.len(), 1, "không báo mồ côi: {lan1:?}");
    match bao[0] {
        Change::Orphaned { spawner, .. } => {
            assert_eq!(*spawner, 39500, "báo mồ côi mà không nêu pid cha")
        }
        _ => unreachable!(),
    }
    // Câu nói ra phải mang sẵn đường dọn — tin không kèm cách đi tiếp thì chủ
    // máy vẫn phải tự nhớ cú pháp, đúng lúc đang ở xa.
    let cau = bao[0].say(&huba::watch::Idle::Prompt, None);
    assert!(
        cau.contains("/stop b4182616"),
        "báo mà không kèm cách dọn: {cau}"
    );

    // 🔴 VÒNG HAI: sổ đã ghi `m = true` ⟹ IM. Một cái loa kêu mỗi 20 giây là
    // cái loa bị tắt tiếng, kéo theo mọi tin đáng đọc.
    let (lan2, _) = changes(&so_moi, &[s], NOW + 30, &[]);
    assert!(
        !lan2.iter().any(|c| matches!(c, Change::Orphaned { .. })),
        "tụng lại tin mồ côi ở vòng sau: {lan2:?}"
    );
}

/// ĐỐI CHỨNG NGƯỢC — ba ca KHÔNG được báo, mỗi ca một lý do khác nhau.
#[test]
fn three_things_that_look_like_orphans_but_are_not() {
    let id = "b4182616-0000-0000-0000-000000000000";

    // ① Chưa đọc được cha ⟹ chưa đo được, không phải mồ côi.
    let mut chua_do = mo_coi_that(id);
    chua_do.spawner_alive = None;
    let (e, _) = changes(&so_da_thay(id), &[chua_do], NOW, &[]);
    assert!(
        !e.iter().any(|c| matches!(c, Change::Orphaned { .. })),
        "dán nhãn mồ côi cho một phiên chỉ vì không đọc nổi argv: {e:?}"
    );

    // ② Cha còn sống ⟹ là CON, không phải mồ côi.
    let mut con = mo_coi_that(id);
    con.spawner_alive = Some(true);
    let (e, _) = changes(&so_da_thay(id), &[con], NOW, &[]);
    assert!(
        !e.iter().any(|c| matches!(c, Change::Orphaned { .. })),
        "{e:?}"
    );

    // ③ CÒN CỬA SỔ trên màn ⟹ chủ máy nhìn thấy nó, tự đứng được.
    let mut co_man = mo_coi_that(id);
    co_man.tty = "ttys003".into();
    co_man.host = "terminal".into();
    let (e, _) = changes(&so_da_thay(id), &[co_man], NOW, &[]);
    assert!(
        !e.iter().any(|c| matches!(c, Change::Orphaned { .. })),
        "{e:?}"
    );
}

/// Sổ RỖNG = huba vừa dậy, không phải mọi thứ vừa đổi (luật 11). Vòng đầu im.
#[test]
fn the_first_round_after_a_restart_says_nothing() {
    let s = mo_coi_that("b4182616-0000-0000-0000-000000000000");
    let (e, _) = changes(&BTreeMap::new(), &[s], NOW, &[]);
    assert!(
        !e.iter().any(|c| matches!(c, Change::Orphaned { .. })),
        "vòng đầu sau khi khởi động lại đã rung chuông: {e:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// `/shot` phiên nền: chữ cũ phải TỰ KHAI là chữ cũ
// ─────────────────────────────────────────────────────────────────────────────

/// Ca nguyên hình của Hà: bấm 📷 lúc 04:10, lời cuối ghi lúc 01:45.
#[test]
fn old_words_must_say_how_old_they_are() {
    // 2026-09-05T18:45:09Z = 01:45 giờ máy; đo lúc 04:10 cùng ngày ⟹ 2h25.
    let luc_bam = chrono::DateTime::parse_from_rfc3339("2026-09-05T21:10:00Z")
        .unwrap()
        .timestamp_millis();
    let cau = huba::pipeline::bg_shot_say(
        "[fbot]",
        "Quality-gate báo đỏ ở mục bí mật.",
        Some("2026-09-05T18:45:09.664Z"),
        luc_bam,
    );

    assert!(cau.contains("2h trước"), "không đóng mốc tuổi: {cau}");
    // Và nói THẲNG nó không phải màn — đây mới là chỗ chữa cái hiểu nhầm, chứ
    // không phải cái mốc: một đoạn chữ đặt sau biểu tượng máy ảnh đọc lên là màn.
    assert!(cau.contains("KHÔNG phải màn"), "{cau}");
    assert!(
        cau.contains("Quality-gate báo đỏ"),
        "mất luôn nội dung: {cau}"
    );
}

/// ĐỐI CHỨNG NGƯỢC — hai ca không được đọc thành "2h trước".
#[test]
fn a_missing_timestamp_never_becomes_a_fake_age() {
    let bay_gio = chrono::Utc::now().timestamp_millis();

    // ① Không có mốc ⟹ IM, không bịa. Đây là con bug cũ mặc bộ đồ khác.
    let khong_moc = huba::pipeline::bg_shot_say("[fbot]", "câu gì đó", None, bay_gio);
    assert!(
        !khong_moc.contains("trước"),
        "bịa ra tuổi từ chỗ không có mốc: {khong_moc}"
    );
    assert!(!khong_moc.contains("vừa xong"), "{khong_moc}");

    // ② Mốc HỎNG (không parse nổi) cũng là "chưa đo được", không phải "vừa xong".
    let moc_hong = huba::pipeline::bg_shot_say("[fbot]", "câu gì đó", Some("hôm qua"), bay_gio);
    assert!(
        !moc_hong.contains("vừa xong"),
        "mốc hỏng bị đọc thành mới tinh: {moc_hong}"
    );
    assert!(!moc_hong.contains("trước"), "{moc_hong}");

    // ③ ĐỐI CHỨNG DƯƠNG: mốc vừa xong thì PHẢI nói vừa xong — dưới một phút là
    //    tin đáng giá nhất ở đây, và `quiet_for` cố ý im ở ngưỡng ấy.
    let vua_xong = huba::pipeline::bg_shot_say(
        "[fbot]",
        "câu gì đó",
        Some(&chrono::Utc::now().to_rfc3339()),
        bay_gio,
    );
    assert!(vua_xong.contains("vừa xong"), "{vua_xong}");
}

/// Chạy trên NHẬT KÝ THẬT của phiên `[fbot]` trên máy này — không fixture.
///
/// `#[ignore]` vì nó đọc đĩa và phụ thuộc máy. Chạy tay:
/// `cargo test --offline --test the_list_says_what_you_can_do -- --ignored --nocapture`
#[test]
#[ignore = "đọc nhật ký thật trên máy — chạy tay bằng --ignored"]
fn the_real_fbot_journal_still_carries_its_moment() {
    let cfg = huba::config::load(None).expect("đọc được cấu hình");
    let (said, at) = huba::sessions::last_say_at_by_id(
        &cfg,
        "b4182616-4c7b-440d-bf8e-d2b9959df6c0",
        huba::sessions::SAY_MAX,
    )
    .expect("nhật ký [fbot] phải còn trên đĩa");

    let at = at.expect("dòng lời cuối phải mang timestamp");
    println!("lời cuối ghi lúc: {at}");
    let cau = huba::pipeline::bg_shot_say(
        "[fbot]",
        &said,
        Some(&at),
        chrono::Utc::now().timestamp_millis(),
    );
    println!("---- 8< ---- (đây là chữ Telegram nhận)\n{cau}\n---- >8 ----");
    assert!(cau.contains("KHÔNG phải màn"), "{cau}");
    assert!(
        cau.contains("trước"),
        "nhật ký thật mà không ra tuổi: {cau}"
    );
}
