//! Đường dẫn tệp phiên MỜI TẢI phải thành liên kết 📎 bấm được trên Telegram.
//!
//! 🔴 2026-09-24, Hà gửi ảnh một tin `[dwork]`: *"Ảo thật, bảo bấm tải nhưng ko có
//! link"* — phiên viết *"(bấm 📎 để tải về điện thoại):"* rồi xuống dòng đường dẫn
//! `/Users/hanguyen/projects/dwork/dev/.tmp/so-do-quy-trinh-doi-2026-09-24.html`
//! (tệp CÓ THẬT, 24.971 B), mà trên tin không có 📎 nào. Đoạn màn dưới đây là
//! NGUYÊN VĂN chữ `/shot` 07:27:28Z (log `channel_command_handled`).

use huba::keys::commands_in_report;
use huba::pipeline::{render_session_data, SessionData};

/// Câu GỐC trong nhật ký phiên `42090c12` — đường dẫn nằm trong BACKTICK. Đây là
/// dạng chữ huba chuyển khi lấy từ nhật ký (bù phần màn thiếu, "Xem đầy đủ", tin
/// tự phát), khác chữ trên màn (TUI vẽ code span không có backtick).
const PROSE: &str = "Sơ đồ đã vẽ lại xong, gửi anh/chị (bấm 📎 để tải về điện thoại):\n\
`/Users/hanguyen/projects/dwork/dev/.tmp/so-do-quy-trinh-doi-2026-09-24.html`\n\
\n\
Sơ đồ có 3 phần:";

#[test]
fn neo_tep_bam_ca_khi_duong_dan_trong_backtick() {
    huba::telegram::set_bot_username("hub_test_bot");
    let html = render_session_data(PROSE, &data());
    assert!(
        html.contains("start=f_0"),
        "đường dẫn trong backtick KHÔNG có 📎 — chỉ còn một khối chữ đơn cách không bấm được:\n{html}"
    );
}

/// NGUYÊN VĂN chữ `/shot` 07:27:28Z (log `channel_command_handled`, đã soát không
/// mang bí mật). Khuôn tự viết lần đầu KHÔNG tái hiện được ca hỏng (đối chứng
/// ngược đỏ), nên bài này đứng trên chữ thật.
const SHOT_THAT: &str = include_str!("fixtures/shot-dwork-tep-moi-tai-2026-09-24.txt");

/// Các đường dẫn `/shot` đưa vào hàng xét — đúng chuỗi của bản thật
/// (`body_before_box` → `paths_on_screen`).
fn hang_xet(tran: usize) -> Vec<String> {
    huba::keys::paths_on_screen(&huba::keys::body_before_box(SHOT_THAT), tran)
}

#[test]
fn tran_quet_du_de_tep_moi_tai_vao_hang_xet() {
    let quet = hang_xet(huba::pipeline::PATHS_SCAN_MAX);
    assert!(
        quet.iter().any(|p| p == PATH),
        "tệp phiên mời tải không vào hàng xét — 📎 sẽ rơi vào tệp khác: {quet:?}"
    );
}

/// ĐỐI CHỨNG NGƯỢC: trần cũ (4) trên chữ thật tái hiện đúng ca hỏng — hạ trần quét
/// về như cũ thì bài trên phải đỏ.
#[test]
fn doi_chung_nguoc_tran_cu_bo_sot_tep() {
    let cu = hang_xet(4);
    assert_eq!(cu.len(), 4, "chữ thật phải lấp đủ trần cũ: {cu:?}");
    assert!(
        !cu.iter().any(|p| p == PATH),
        "trần 4 mà vẫn thấy tệp ⟹ bài không tái hiện ca hỏng: {cu:?}"
    );
}

/// THĂM DÒ tay: chạy TOÀN VĂN một chữ `/shot` (đọc từ tệp `HUBA_SHOT_FILE`) qua
/// bản vẽ thật, in HTML ra để soi neo 📎 có bám không.
///
/// ```text
/// HUBA_SHOT_FILE=… HUBA_SHOT_PATH=… cargo test --test neo_tep_trong_shot -- --ignored --nocapture
/// ```
#[test]
#[ignore = "thăm dò tay trên một chữ /shot thật"]
fn tham_do_toan_van_shot() {
    let (Ok(f), Ok(p)) = (
        std::env::var("HUBA_SHOT_FILE"),
        std::env::var("HUBA_SHOT_PATH"),
    ) else {
        panic!("KHÔNG ĐO ĐƯỢC: thiếu HUBA_SHOT_FILE / HUBA_SHOT_PATH");
    };
    huba::telegram::set_bot_username("hub_test_bot");
    let text = std::fs::read_to_string(&f).expect("đọc tệp chữ /shot");
    let data = SessionData {
        sid: "42090c12-fd2a-478a-bcc0-057bd5396c11".into(),
        files: vec![(p.clone(), 0)],
        ..Default::default()
    };
    let html = render_session_data(&text, &data);
    let co = html.contains("start=f_0");
    let dong: Vec<&str> = html.lines().collect();
    let k = dong
        .iter()
        .position(|l| l.contains("so-do-quy-trinh"))
        .unwrap_or(0);
    println!(
        "neo 📎 bám: {co} · số dòng: {}\n--- quanh đường dẫn ---\n{}",
        dong.len(),
        dong[k.saturating_sub(2)..(k + 2).min(dong.len())].join("\n")
    );
}

/// THĂM DÒ tay: đi ĐÚNG chuỗi hàm của `/shot` thật (cấu hình thật, DB là BẢN SAO)
/// để biết tệp có vào `files` không, và rơi ở mắt xích nào.
#[test]
#[ignore = "thăm dò tay: cần HUBA_SHOT_FILE, HUBA_CFG, HUBA_DB_COPY, HUBA_SID"]
fn tham_do_chuoi_that_cua_shot() {
    let get = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("KHÔNG ĐO ĐƯỢC: thiếu {k}"));
    let ack = std::fs::read_to_string(get("HUBA_SHOT_FILE")).expect("đọc chữ /shot");
    let cfg = huba::config::load(Some(std::path::Path::new(&get("HUBA_CFG")))).expect("cấu hình");
    let db = huba::db::Db::open(std::path::Path::new(&get("HUBA_DB_COPY"))).expect("DB bản sao");
    let sid = get("HUBA_SID");
    let body = huba::keys::body_before_box(&ack);
    let paths = huba::keys::paths_on_screen(&body, huba::pipeline::PATHS_SCAN_MAX);
    let cmds = huba::pipeline::cmds_of_text(&cfg, &sid, &ack);
    let seen = huba::pipeline::paths_not_in_commands(&ack, &paths, &cmds);
    let files = huba::pipeline::file_anchors(&db, &cfg, &sid, &seen);
    println!("paths_on_screen: {paths:?}");
    println!(
        "cmds_of_text ({}): {:?}",
        cmds.len(),
        cmds.iter().map(|c| &c.line).collect::<Vec<_>>()
    );
    println!("paths_not_in_commands: {seen:?}");
    println!("file_anchors: {files:?}");
}

/// Một dòng CHỈ gồm đường dẫn tới một TỆP (không động từ, không đối số) không
/// phải một dòng lệnh: nhận nhầm thì nó thành ▶️/🖥 (chạy tệp `.html` như một
/// chương trình) và mất 📎 — `paths_not_in_commands` gạt đường dẫn nằm trong
/// dòng lệnh khỏi bộ neo tệp.
#[test]
fn dong_chi_la_duong_dan_tep_khong_phai_lenh() {
    // Cả chữ trên MÀN lẫn câu gốc trong NHẬT KÝ (đường dẫn trong backtick) —
    // `add_prose_cmds` bóc lệnh từ cả hai.
    for (nguon, chu) in [("màn", SCREEN), ("nhật ký", PROSE)] {
        let lenh = commands_in_report(chu, 10);
        assert!(
            !lenh.iter().any(|l| l.contains("so-do-quy-trinh-doi")),
            "{nguon}: dòng đường dẫn tệp bị nhận là LỆNH: {lenh:?}"
        );
    }
}

const PATH: &str = "/Users/hanguyen/projects/dwork/dev/.tmp/so-do-quy-trinh-doi-2026-09-24.html";

const SCREEN: &str = "  Read 1 file, ran 2 shell commands \n\
⏺ [dwork] Sơ đồ đã vẽ lại xong, gửi anh/chị (bấm 📎 để tải về điện thoại):\n\
\x20 /Users/hanguyen/projects/dwork/dev/.tmp/so-do-quy-trinh-doi-2026-09-24.html\n\
\x20 Sơ đồ có 3 phần: \n\
\x20 1. Vòng việc hằng ngày qua bảng việc Redis, đánh dấu 7 điểm đứ\n";

fn data() -> SessionData {
    SessionData {
        sid: "42090c12-fd2a-478a-bcc0-057bd5396c11".into(),
        files: vec![(PATH.to_string(), 0)],
        ..Default::default()
    }
}

#[test]
fn neo_tep_nam_ngay_sau_duong_dan() {
    huba::telegram::set_bot_username("hub_test_bot");
    let html = render_session_data(SCREEN, &data());
    // Hình dạng đúng (đo trên bản vẽ thật): CẢ đường dẫn là liên kết, 📎 đứng đầu.
    let mo = format!("<a href=\"https://t.me/hub_test_bot?start=f_0\">📎 {PATH}</a>");
    assert!(
        html.contains(&mo),
        "đường dẫn tệp không thành liên kết 📎 bấm được:\n{html}"
    );
}
