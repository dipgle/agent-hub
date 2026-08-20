//! Bốn chỗ huba đọc SAI thứ nó đang nhìn — Hà, bốn ảnh chụp trong một buổi tối
//! 2026-08-19. Cả bốn cùng một họ: huba cầm đúng dữ kiện mà đọc trên một khuôn
//! đã cũ, rồi kết luận tự tin.
//!
//! 1. *"Màn có option nhưng nhận được bị thiếu và không có cách bấm chọn là
//!    sao"* — hộp chọn cao hơn cửa sổ nên `1.` cuộn khỏi mép trên; luật "số phải
//!    liên tiếp TỪ 1" trả về rỗng ⟹ không một cái nút nào.
//! 2. *"nội dung sao bị chèn lung tung ở đâu vào ô chat"* — khối kết quả `▶️`
//!    nằm lại trong ô nhập vì phép đo "chữ đã đi chưa" chỉ soi hai ĐẦU khối,
//!    trong khi ô chỉ hiện được khúc GIỮA.
//! 3. *"Thông tin đầu phiên bị thiếu mã"* — hàng dựng từ sổ bỏ trống `label`.
//! 4. *"Chuyển phiên xong phiên cũ bị kẹt như này làm sao qua được"* — hộp
//!    *"Background work is running"* của `/exit`.

use huba::keys::{box_region, parse_choices, still_in_box};
use huba::sessions::{
    exit_dialog_choice, exit_dialog_tasks, label_sessions, shown, without_dot, LiveSession,
};

/// Màn THẬT của phiên `[tcc/amm]`, đọc lúc 20:25 ngày 19/08 — đúng cái Hà chụp.
const AMM: &str = include_str!("fixtures/shot-amm-chooser-2026-08-19.txt");

/// Hộp chọn CAO HƠN cửa sổ vẫn là hộp chọn.
#[test]
fn a_chooser_taller_than_the_window_still_has_buttons() {
    let got = parse_choices(AMM);
    let nums: Vec<usize> = got.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        nums,
        vec![2, 3, 4, 5, 6],
        "màn bắt đầu từ lựa chọn 2 vì lựa chọn 1 đã cuộn khỏi mép trên:\n{got:?}"
    );
    assert!(
        got[0].1.starts_with("Bake multisig"),
        "nhãn phải là chữ trên chính dòng số: {:?}",
        got[0].1
    );
}

/// …nhưng nới luật ấy KHÔNG được mở cửa cho một đoạn văn có đánh số.
///
/// Đây là cái giá của bản vá trên, nên nó phải có bài kiểm riêng: cửa an toàn
/// nay là DÒNG CHÂN của hộp, và không có dòng chân thì luật cũ giữ nguyên.
#[test]
fn numbered_prose_without_the_chooser_footer_is_still_not_a_chooser() {
    let prose = "Tôi đã làm ba việc:\n\
                 2. vá cổng ACL\n\
                 lý do: nó nhận zid thay org_id\n\
                 3. thêm bài kiểm\n\
                 4. đẩy lên origin/main\n";
    assert!(
        parse_choices(prose).is_empty(),
        "đoạn văn đánh số 2·3·4 mà không có dòng chân thì KHÔNG phải hộp chọn"
    );
}

/// Ô nhập của bản `claude` hiện nay: nằm GIỮA HAI vạch kẻ, không còn khung.
#[test]
fn the_input_box_sits_between_two_rules() {
    let screen = "phiên vừa trả lời xong\n\
                  ────────────────────────────────\n\
                  ❯ chữ đang chờ gửi\n\
                  ────────────────────────────────\n\
                  ⏵⏵ auto mode on · 2 shells\n";
    let region = box_region(screen);
    assert!(
        region.contains("chữ đang chờ gửi"),
        "vạch CUỐI là viền DƯỚI — lấy nó là mất sạch chữ trong ô:\n{region}"
    );
    assert!(
        !region.contains("phiên vừa trả lời xong"),
        "…mà cũng không được nuốt cả phần hội thoại phía trên:\n{region}"
    );
}

/// Khối dán dài: ô chỉ hiện được khúc GIỮA, nên hai đầu đều vắng mặt.
///
/// 🔴 Đây là bài kiểm tái hiện của lỗi Hà chụp lúc 20:12. Với luật cũ (chỉ soi
/// 16 ký tự đầu + 16 ký tự cuối) nó ĐỎ: cả hai đầu đều không có trên màn, phép
/// đo đọc ra "chữ đã đi", `type_and_send` không bấm Enter, và cả khối nằm lại
/// trong ô nhập hơn một tiếng trong khi huba báo *"✅ đã dán vào phiên"*.
#[test]
fn a_pasted_block_is_found_by_any_chunk_still_visible() {
    let block = "[huba chạy hộ]\n\
                 $ git status --short | grep -E \"^(UU|AA|DU|UD|AU|UA)\" || echo \"(không có)\"\n\
                 ✅ xong (0.1s)\n\
                 (không có)";
    // Màn THẬT (`ttys000`, 20:35): dòng đầu đã cuộn khỏi ô, dòng cuối nằm dưới
    // mép màn, và dòng lệnh ở giữa thì bị GẤP DÒNG — khúc `git status --short |`
    // không còn. Nên không dòng nào của khối còn nguyên vẹn để mà so.
    let screen = "…phần hội thoại…\n\
                  ────────────────────────────────\n\
                  ❯ grep -E \"^(UU|AA|DU|UD|AU|UA)\" || echo \"(không có)\"\n  \
                  ✅ xong (0.1s)\n";
    assert!(
        still_in_box(screen, block),
        "khối vẫn nằm trong ô nhập — một KHÚC còn nhìn thấy được là đủ để biết"
    );
}

/// …và một ô nhập đang giữ chữ của NGƯỜI KHÁC thì không đọc thành chữ của huba.
#[test]
fn someone_elses_text_in_the_box_is_not_mistaken_for_the_paste() {
    let block = "[huba chạy hộ]\n$ cargo test --offline\n✅ xong (92.4s)\n424 passed";
    let screen = "…phần hội thoại…\n\
                  ────────────────────────────────\n\
                  ❯ làm tiếp phần bảo mật của DS04 đi\n\
                  ────────────────────────────────\n\
                  ⏵⏵ auto mode on\n";
    assert!(!still_in_box(screen, block));
}

/// …và ô đã trống thì đừng bắn thêm một cú Enter nào.
#[test]
fn an_empty_box_reads_as_sent() {
    let block = "[huba chạy hộ]\n$ ls -la /tmp/khong-co-that\n❌ exit 1 (0.0s)";
    let screen = "…phần hội thoại…\n\
                  ────────────────────────────────\n\
                  ❯ \n\
                  ⏵⏵ auto mode on\n";
    assert!(!still_in_box(screen, block));
}

/// Hộp *"Background work is running"*: nhận đúng nó, và bấm đúng lựa chọn.
#[test]
fn the_exit_dialog_is_answered_with_stop_tasks() {
    let screen = "  Background work is running\n\
                  The following will stop when you exit:\n\
                  \n\
                  shell · bash /Users/hanguyen/projects/scripts/quality-gate.sh\n\
                  \n\
                  ❯ 1. Exit and stop tasks\n  \
                  2. Move to background and exit\n  \
                  3. Stay\n\
                  \n\
                  Enter to confirm · Esc to cancel\n";
    let choices = parse_choices(screen);
    assert_eq!(choices.len(), 3, "{choices:?}");
    assert_eq!(
        exit_dialog_choice(screen, &choices),
        Some(1),
        "phải là 'Exit and stop tasks' — chọn 2 để lại tiến trình sống, và đóng \
         cửa sổ còn tiến trình sống thì Terminal bật hộp khoá mọi lệnh sau nó"
    );
    let tasks = exit_dialog_tasks(screen);
    assert_eq!(tasks.len(), 1, "{tasks:?}");
    assert!(tasks[0].contains("quality-gate.sh"), "{tasks:?}");
}

/// Hộp chọn KHÁC thì huba không trả lời thay chủ máy.
#[test]
fn another_dialog_is_never_answered_for_the_owner() {
    let screen = "Do you want to make this edit to billing.rs?\n\
                  ❯ 1. Yes\n  \
                  2. Yes, and don't ask again\n  \
                  3. No, tell Claude what to do differently\n\
                  \n\
                  Enter to confirm · Esc to cancel\n";
    let choices = parse_choices(screen);
    assert!(!choices.is_empty(), "vẫn phải đọc ra hộp: {choices:?}");
    assert_eq!(
        exit_dialog_choice(screen, &choices),
        None,
        "không có dòng 'Background work is running' ⟹ KHÔNG phải hộp của /exit"
    );
}

fn row(id: &str, folder: &str, doing: &str) -> LiveSession {
    LiveSession {
        session_id: id.to_string(),
        name: format!("projects-{}", &id[..2]),
        folder: folder.to_string(),
        doing: doing.to_string(),
        ..Default::default()
    }
}

/// Ba phiên `dwork`: nhãn phải nói VIỆC ĐANG LÀM, không phải tám ký tự hex.
#[test]
fn same_project_sessions_are_told_apart_by_what_they_are_doing() {
    let root = std::path::PathBuf::from("/Users/hanguyen/projects");
    let mut rows = vec![
        row("08a90086-x", "dwork", "Chốt mockup doc và driver"),
        row(
            "a14bc255-x",
            "dwork",
            "Tiếp tục DS04 quét mã và nhập xuất XML",
        ),
        row(
            "f33ae528-x",
            "dwork",
            "Tiếp tục N7 lát 2 kiểm tra khuôn mặt",
        ),
        row("51220fa7-x", "AI/tcc/amm", "Chốt hướng authority cho AMM"),
    ];
    label_sessions(&mut rows, &root);
    assert_eq!(rows[0].label, "[dwork]·Chốt mockup doc và driver");
    assert!(
        rows[1].label.starts_with("[dwork]·Tiếp tục DS04"),
        "{}",
        rows[1].label
    );
    // Một mình một dự án thì không cần vế phân biệt nào cả.
    assert_eq!(rows[3].label, "[tcc/amm]");
}

/// Không đọc được việc đang làm thì RƠI VỀ mã id, đừng bịa và đừng để trống.
#[test]
fn without_a_tab_title_the_short_id_is_still_the_fallback() {
    let root = std::path::PathBuf::from("/Users/hanguyen/projects");
    let mut rows = vec![
        row("08a90086-x", "dwork", ""),
        row("a14bc255-x", "dwork", ""),
    ];
    label_sessions(&mut rows, &root);
    assert_eq!(rows[0].label, "[dwork]·08a90086");
    assert_eq!(rows[1].label, "[dwork]·a14bc255");
}

/// Hai phiên cùng một câu việc thì câu ấy KHÔNG phân biệt được — về lại id.
#[test]
fn two_sessions_doing_the_same_thing_fall_back_to_the_id() {
    let root = std::path::PathBuf::from("/Users/hanguyen/projects");
    let mut rows = vec![
        row("08a90086-x", "dwork", "Chạy cổng chất lượng"),
        row("a14bc255-x", "dwork", "Chạy cổng chất lượng"),
    ];
    label_sessions(&mut rows, &root);
    assert_eq!(rows[0].label, "[dwork]·08a90086");
    assert_eq!(rows[1].label, "[dwork]·a14bc255");
}

/// Đích chạm của một CỬA SỔ TRẦN phải nhận được — cả id, không cắt.
///
/// 🔴 Hà 2026-08-19, ảnh hộp *"Bypass Permissions mode"* trên `ttys002` với hai
/// dấu ☑ hiện rành rành: *"Sao khong bam chon được"*. Log:
/// `telegram_not_a_command {"head":"/start k_win-ttys_2"}` — id bị cắt còn 8 ký
/// tự (mất số tty) rồi bộ đọc đòi HEX nên gạt luôn.
#[test]
fn a_bare_window_choice_link_round_trips() {
    use huba::adapters::CommandKind;
    use huba::verbs::parse_command;

    assert_eq!(
        parse_command("/start k_win-ttys002_2"),
        Some((CommandKind::Key, 0, "win-ttys002 2".to_string())),
        "liên kết ☑ của một cửa sổ trần phải cởi ra thành /key đúng cửa sổ ấy"
    );
    assert_eq!(
        parse_command("/start clr_win-ttys002"),
        Some((CommandKind::Key, 0, "win-ttys002 clear".to_string()))
    );
    assert_eq!(
        parse_command("/start send_win-ttys002"),
        Some((CommandKind::Key, 0, "win-ttys002 enter".to_string()))
    );
    // Phiên thật thì vẫn đi bằng 8 ký tự hex như cũ.
    assert_eq!(
        parse_command("/start k_a14bc255_3"),
        Some((CommandKind::Key, 0, "a14bc255 3".to_string()))
    );
    // …và một chuỗi bịa vẫn không lọt.
    assert_eq!(parse_command("/start k_khong-phai_1"), None);
}

/// Nhãn chép từ sổ không được đeo HAI ô màu.
#[test]
fn a_label_taken_from_the_book_keeps_exactly_one_dot() {
    assert_eq!(without_dot("🟥 [dwork]·a14bc255"), "[dwork]·a14bc255");
    assert_eq!(without_dot("[dwork]·a14bc255"), "[dwork]·a14bc255");
    let s = LiveSession {
        folder: "dwork".into(),
        label: without_dot("🟥 [dwork]·a14bc255").to_string(),
        ..Default::default()
    };
    // Không ghim MÀU ở đây — bảng màu là việc của `project_dot` và nó đổi thật
    // (bộ cũ có 🟥/🟨, bộ nay trung tính). Thứ bài kiểm này khoá là HÌNH DẠNG:
    // đúng một ô, đứng đầu, rồi tới nhãn.
    let out = shown(&s);
    assert_eq!(
        out,
        format!("{} [dwork]·a14bc255", huba::sessions::project_dot("dwork")),
        "nhãn chép từ sổ phải đeo đúng MỘT ô màu"
    );
}
