//! Terminal.app chết thì mọi phiên trong nó chết theo — và đó là MỘT tin.
//!
//! 🔴 Đo được ngày 2026-09-10, và nó là câu Hà phải tự đi hỏi. Terminal.app
//! (chạy liên tục từ 30/08, pid 1264) bị kernel giết lúc **10:03:57** —
//! `EXC_BREAKPOINT` / `PAC_EXCEPTION` ngay trong `aeProcessAppleEvent`, xem
//! `~/Library/Logs/DiagnosticReports/Terminal-2026-09-10-100434.ips`. Trong
//! đúng một vòng, `sessions_snapshot_ms` đi từ **13 phiên xuống 0**
//! (`hubd.err` 10:03:57.679 → 10:04:02.761), và lúc 10:06 huba bắn **13 tin**
//! `⚫ … đã tắt (thoát CLI, cửa sổ terminal còn mở)` liền nhau.
//!
//! Không tin nào nói vì sao. Câu Hà nhắn lại: *"Máy vừa bị sao mà thoát hết
//! cli"* — trong khi chính huba đã ghi `terminal_alive: false` **năm giây
//! trước** (`hubd.err` 10:03:57.428, `terminal_probe_failed`). Dữ kiện nằm sẵn
//! trong log; chỗ thiếu là sợi dây nối nó vào cái loa.
//!
//! Mười ba tin đúng từng cái một mà vẫn để người đọc không biết chuyện gì xảy
//! ra, vì thứ người ta cần là MỘT NGUYÊN NHÂN chứ không phải mười ba triệu
//! chứng. Cùng họ với hai lần Hà đã bắt trước đây: *"Đóng 1 phiên mà lắm thông
//! báo thế"* (13/08).

use huba::watch::{terminal_fate, terminal_restart_text, TerminalFate};

// ───────────────────────── PHÁN QUYẾT: ba trạng thái, không phải hai ─────────

#[test]
fn a_different_pid_means_terminal_died_and_came_back() {
    assert_eq!(
        terminal_fate(Some(1264), Some(2874)),
        TerminalFate::KhoiDongLai {
            truoc: 1264,
            sau: 2874
        },
        "hai vòng hai pid khác nhau ⟹ Terminal.app đã đi và mở lại; mọi cửa sổ \
         của vòng trước đi theo nó"
    );
}

#[test]
fn the_same_pid_is_not_an_event() {
    assert_eq!(
        terminal_fate(Some(1264), Some(1264)),
        TerminalFate::NguyenVen,
        "cùng một tiến trình ⟹ phiên nào tắt là tự nó tắt, không được gán cớ"
    );
}

#[test]
fn not_measured_is_its_own_state_not_a_quiet_yes() {
    // Đây là ràng buộc ② của luật 13: "không đo được" phải là trạng thái RIÊNG.
    // Gộp nó vào `NguyenVen` thì mỗi lần `pgrep` hết giờ huba lại lặng lẽ khẳng
    // định Terminal vẫn thế — một tín hiệu không bao giờ ở trạng thái ngược lại.
    assert_eq!(
        terminal_fate(None, Some(2874)),
        TerminalFate::ChuaDoDuoc,
        "vòng TRƯỚC không đo được (hubd vừa khởi động, sổ trống) ⟹ chưa đủ tư \
         cách kết luận"
    );
    assert_eq!(
        terminal_fate(Some(1264), None),
        TerminalFate::ChuaDoDuoc,
        "vòng NÀY không đo được (pgrep hết giờ) ⟹ im, đừng báo tử Terminal"
    );
    assert_eq!(terminal_fate(None, None), TerminalFate::ChuaDoDuoc);
}

#[test]
fn the_first_round_after_a_restart_can_never_cry_wolf() {
    // Sổ `terminal:pid` trống là ca THẬT và nó xảy ra mỗi lần cài bản mới: 22
    // lượt khởi chạy hubd bị kernel giết trong 6 ngày (04→10/09), mỗi lượt kéo
    // theo một lần hubd lên lại với sổ vừa đọc lần đầu. Nếu vòng đầu dám kết
    // luận thì mỗi lượt cài đẻ một lời báo động giả.
    assert_eq!(terminal_fate(None, Some(999)), TerminalFate::ChuaDoDuoc);
}

// ───────────────────────── CÂU CHỮ: một tin, đủ cả cớ lẫn danh sách ──────────

#[test]
fn the_one_message_carries_the_cause_and_the_count() {
    let dang_cham = vec!["[dwork/a-ddoc]".to_string(), "[dwork]".to_string()];
    let con_lai = vec!["[onghut]".to_string(), "[mailler]".to_string()];
    let t = terminal_restart_text(1264, 2874, &dang_cham, &con_lai);

    assert!(
        t.contains("1264") && t.contains("2874"),
        "phải mang CẢ HAI pid — đó là bằng chứng, không phải lời kể: {t}"
    );
    assert!(
        t.contains("Terminal.app"),
        "phải gọi đúng tên thủ phạm, không nói chung chung 'các phiên đã tắt': {t}"
    );
    // 🔴 Neo vào "4 phiên", KHÔNG vào chữ số '4' trần. Bản đầu của bài này viết
    // `t.contains('4')` và nó XANH VÔ ĐIỀU KIỆN: pid `1264` đã chứa sẵn một số
    // 4, nên khẳng định ấy đúng kể cả khi câu chữ quên hẳn phần đếm. Một phép
    // đo tự khớp chính nó thì không phải phép đo.
    assert!(
        t.contains("4 phiên"),
        "phải khai TỔNG số phiên bị cuốn theo (2 + 2 = 4): {t}"
    );
}

#[test]
fn the_count_is_counted_not_copied_from_a_pid() {
    // Đối chứng cho bài trên, bằng ba con số RỜI NHAU: hai pid không chứa chữ
    // số nào của tổng, nên "3 phiên" chỉ có thể tới từ phép đếm.
    let con_lai = vec!["[a]".to_string(), "[b]".to_string(), "[c]".to_string()];
    let t = terminal_restart_text(500, 600, &[], &con_lai);
    assert!(
        t.contains("3 phiên"),
        "tổng phải đếm từ hai danh sách, không lấy từ đâu khác: {t}"
    );
}

#[test]
fn sessions_caught_mid_work_are_not_buried_in_the_list() {
    let dang_cham = vec!["[dwork/a-ddoc]".to_string()];
    let con_lai = vec!["[onghut]".to_string(), "[mailler]".to_string()];
    let t = terminal_restart_text(1264, 2874, &dang_cham, &con_lai);

    let i_cham = t.find("[dwork/a-ddoc]").expect("có tên phiên đang chạy dở");
    let i_lai = t.find("[onghut]").expect("có tên phiên còn lại");
    assert!(
        i_cham < i_lai,
        "phiên ĐANG CHẠY DỞ là phần đòi người ta làm gì, phải đứng trước phần \
         chỉ để biết — gộp chung thì việc cần xem lại chìm trong danh sách:\n{t}"
    );
    assert!(
        t.contains("xem lại"),
        "phần đang chạy dở phải NÓI RA là cần xem lại: {t}"
    );
}

#[test]
fn a_wave_with_nobody_mid_work_says_no_such_thing() {
    // Đối chứng ngược cho bài trên: không có phiên nào chạy dở thì KHÔNG được
    // in cái nhãn "xem lại" — một cảnh báo kêu oan là một cảnh báo bị lướt qua.
    let con_lai = vec!["[onghut]".to_string()];
    let t = terminal_restart_text(7, 8, &[], &con_lai);
    assert!(
        !t.contains("xem lại"),
        "không ai đang chạy dở mà vẫn giục 'xem lại' là kêu oan: {t}"
    );
    assert!(t.contains("[onghut]"), "vẫn phải kê tên phiên đã tắt: {t}");
}

#[test]
fn it_points_at_where_the_real_cause_is_readable() {
    // huba KHÔNG phân biệt được "Terminal sập" với "chủ máy tự thoát Terminal"
    // — cả hai đều làm pid đổi. Nên nó không được đoán; nó chỉ ra chỗ đọc được
    // câu trả lời. Ca 10/09 có tệp `Terminal-2026-09-10-100434.ips`; một lượt
    // chủ máy tự thoát thì không có tệp nào.
    let t = terminal_restart_text(1264, 2874, &[], &["[onghut]".to_string()]);
    assert!(
        t.contains("DiagnosticReports"),
        "phải chỉ chỗ đọc báo cáo sự cố để phân biệt SẬP với TỰ THOÁT: {t}"
    );
}

// ───────────── ĐIỂM GỌI: hàm thuần đúng mà thân vòng lặp không hỏi thì vô nghĩa
//
// MẪU SỐ — hai bài dưới đây đo **chuỗi trong mã nguồn** `src/pipeline.rs` và
// `src/sessions.rs`. Chúng KHÔNG chứng minh nhánh ấy được chạy, và không thay
// được một lượt Terminal chết thật. Chúng bịt đúng chỗ mù mà `handover_button`
// rồi `enter_is_a_real_key` đều đã trả giá: enum đúng, người gọi sai, mọi bài
// về enum vẫn xanh trong khi ngoài kia không có gì đổi.

fn nguon(tep: &str) -> String {
    let p = format!("{}/src/{tep}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("đọc được {p}: {e}"))
}

/// Thân `announce_changes` — cắt từ dòng khai báo tới hàm kế tiếp ở cột 0.
/// `None` = không tìm thấy ⟹ ĐỎ: hàm đổi tên mà bài kiểm không hay biết là
/// trạng thái "không đo được", không phải "sạch".
fn than_announce_changes(src: &str) -> Option<&str> {
    let i = src.find("pub fn announce_changes(")?;
    let sau = &src[i..];
    let j = sau[1..]
        .find("\npub fn ")
        .or_else(|| sau[1..].find("\nfn "))
        .map(|k| k + 1)
        .unwrap_or(sau.len());
    Some(&sau[..j])
}

#[test]
fn the_verdict_is_wired_into_the_announcer() {
    let src = nguon("pipeline.rs");
    let than = than_announce_changes(&src).expect(
        "không thấy `pub fn announce_changes(` trong src/pipeline.rs — đổi tên \
         rồi thì bài này KHÔNG đo được nữa",
    );
    assert!(
        than.contains("terminal_fate("),
        "thân `announce_changes` phải hỏi `watch::terminal_fate` — không hỏi thì \
         cả tệp bài kiểm này chỉ chứng minh một hàm không ai gọi:\n{than}"
    );
    assert!(
        than.contains("terminal_restart_text("),
        "…và phải NÓI ra bằng tin gộp; đo mà không nói thì Hà vẫn phải đi hỏi"
    );
    assert!(
        than.contains("TERMINAL_PID_KEY"),
        "phải GHI pid của vòng này xuống sổ, không thì vòng sau luôn `ChuaDoDuoc` \
         và cửa này không bao giờ mở"
    );
}

#[test]
fn the_pid_is_read_in_the_same_snapshot_as_the_sessions() {
    // Hỏi pid ở `announce_changes` thay vì trong `snapshot` là hỏi ở một khoảnh
    // khắc KHÁC với lúc đếm phiên — Terminal khởi động lại xen vào giữa thì cái
    // cớ bị gán vào nhầm vòng. Cùng lý do `ps` và `terminal_screens` được đọc
    // một lượt cho cả ảnh chụp.
    let src = nguon("sessions.rs");
    let i = src
        .find("pub fn snapshot(")
        .expect("không thấy `pub fn snapshot(` trong src/sessions.rs");
    let sau = &src[i..];
    let j = sau[1..]
        .find("\npub fn ")
        .map(|k| k + 1)
        .unwrap_or(sau.len());
    let than = &sau[..j];
    assert!(
        than.contains("out.terminal_pid = terminal_pid()"),
        "`sessions::snapshot` phải đo pid Terminal trong CÙNG lượt với `ps`:\n{than}"
    );
}
