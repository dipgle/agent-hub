//! `trust_dialog_tick` phải chấm trên ẢNH CHỤP CỦA VÒNG, không mở lượt dò riêng.
//!
//! 🔴 Vì sao có tệp này. Đo trên `~/Library/Logs/hubd.err`, 31 giờ (2026-09-15
//! 17:35 → 09-16 00:46), lúc máy này có **40 cửa sổ Terminal** mở cùng lúc:
//!
//! ```text
//! trust_dialog_screen_blind            2111 lượt  (2046 vì "hết ngân sách")
//! keys_screen_read_failed              2394 lượt
//!   trong đó chuỗi lặp window_of_from_cache → keys_screen_read_failed
//!   → trust_dialog_screen_blind        1946 lượt
//! probe_budget_spent                    432 / 1352 vòng (32%), vòng nặng nhất
//!                                       bỏ 50 phép dò
//! ```
//!
//! Gốc: bản cũ gọi `answer_trust_dialog` cho TỪNG tab `claude`, mà hàm ấy là
//! `window_of` + `screen_text` — hai lượt `osascript` mỗi tab. 40 tab ⟹ 81 lời
//! gọi mỗi nhịp 30 giây, trên một ngân sách 10 giây (`keys::PROBE_BUDGET_MS`).
//! Nó không bấm được cho ai, và còn tiêu hết phần của các cỗ máy chạy sau.
//!
//! Giá đo được của một lượt dò GỘP, trên đúng máy ấy, 3 lượt liền:
//! `terminal_tabs()` 4,26/4,17/4,32s · `terminal_screens()` 5,34/4,83/17,19s.
//!
//! Bài kiểm soi phần THUẦN (`pipeline::quet_hop_tin_thu_muc`) — phần đi hỏi
//! Terminal thì không bài kiểm nào chạm tới được.

use huba::keys::Tab;
use huba::pipeline::{quet_hop_tin_thu_muc, TrustQuet, TRUST_FRESH_MAX};

/// Dòng chân của hộp chọn — không có nó thì `keys::parse_choices` từ chối cả
/// màn (xem `has_chooser_footer`), nên mọi màn mẫu ở đây đều phải mang nó.
const CHAN: &str = "Enter to confirm · Esc to cancel";

fn man_hop_tin_thu_muc() -> String {
    format!(
        "Do you trust the files in this folder?\n\
         \n\
         /Users/hanguyen/projects/dwork\n\
         \n\
         ❯ 1. Yes, I trust this folder\n  \
           2. No, exit\n\
         {CHAN}\n"
    )
}

fn man_dang_lam_viec() -> String {
    "● Đang chạy bài kiểm…\n  ⎿ 127 binary, 0 fail\n\n> \n".to_string()
}

/// Một hộp chọn KHÁC — huba không được trả lời thay chủ máy ở đây.
fn man_hop_khac() -> String {
    format!(
        "Set up auto mode for your environment?\n\
         ❯ 1. Set it up\n  \
           2. Not now\n\
         {CHAN}\n"
    )
}

fn tab(tty: &str, cli: &str, screen: Option<&str>) -> Tab {
    Tab {
        tty: tty.to_string(),
        busy: true,
        procs: vec!["login".into(), "-zsh".into(), cli.into()],
        screen: screen.map(str::to_string),
        title: String::new(),
    }
}

fn quet(tabs: &[Tab]) -> (Vec<String>, usize, usize, usize) {
    match quet_hop_tin_thu_muc(Some(tabs)) {
        TrustQuet::Quet {
            ung_vien,
            mu,
            bo_qua,
            tong_claude,
        } => (ung_vien, mu, bo_qua, tong_claude),
        TrustQuet::ChuaDoDuoc => panic!("có bảng tab mà vẫn khai CHƯA ĐO ĐƯỢC"),
    }
}

#[test]
fn tab_dang_ket_o_hop_tin_thu_muc_thi_phai_thanh_ung_vien() {
    let man = man_hop_tin_thu_muc();
    let (ung_vien, mu, bo_qua, tong) = quet(&[tab("ttys009", "claude", Some(&man))]);
    assert_eq!(
        ung_vien,
        vec!["ttys009".to_string()],
        "đúng cái cửa sổ đang kẹt mà không hỏi lại thì cỗ máy này mất lý do tồn tại"
    );
    assert_eq!((mu, bo_qua, tong), (0, 0, 1));
}

/// ĐÂY LÀ BÀI KIỂM CHÍNH: 39 tab bình thường + 1 tab kẹt ⟹ hỏi lại ĐÚNG 1.
///
/// Mỗi ứng viên tốn 2 lượt `osascript`; con số này chính là con số đã kéo
/// `probe_budget_spent` lên 32% số vòng.
#[test]
fn bon_muoi_tab_thi_chi_mot_tab_dang_duoc_hoi_lai() {
    let lam_viec = man_dang_lam_viec();
    let hop = man_hop_tin_thu_muc();
    let mut tabs: Vec<Tab> = (0..39)
        .map(|i| tab(&format!("ttys{i:03}"), "claude", Some(&lam_viec)))
        .collect();
    tabs.push(tab("ttys039", "claude", Some(&hop)));

    let (ung_vien, mu, bo_qua, tong) = quet(&tabs);
    assert_eq!(tong, 40, "mẫu số phải là cả 40 tab claude");
    assert_eq!(ung_vien, vec!["ttys039".to_string()]);
    assert_eq!(
        (mu, bo_qua),
        (0, 0),
        "chấm được hết thì không được khai là mù"
    );
}

#[test]
fn hop_chon_khac_thi_khong_dung_toi() {
    let man = man_hop_khac();
    let (ung_vien, ..) = quet(&[tab("ttys004", "claude", Some(&man))]);
    assert!(
        ung_vien.is_empty(),
        "chỉ hộp tin-thư-mục mới được bấm hộ — hộp khác là trả lời thay chủ máy"
    );
}

#[test]
fn tab_khong_chay_claude_thi_khong_phai_viec_cua_no() {
    let man = man_hop_tin_thu_muc();
    let (ung_vien, mu, bo_qua, tong) = quet(&[tab("ttys007", "vim", Some(&man))]);
    assert!(ung_vien.is_empty());
    assert_eq!(
        (mu, bo_qua, tong),
        (0, 0, 0),
        "tab không chạy claude thì không nằm trong mẫu số của phép đo này"
    );
}

/// Màn không về (`None`) hay về rỗng (khung trắng) là **CHƯA CHẤM ĐƯỢC**, nên
/// phải hỏi lại — không được im lặng bỏ qua một cửa sổ có thể đang kẹt.
#[test]
fn man_trong_va_man_khong_ve_deu_phai_hoi_lai() {
    let (ung_vien, mu, bo_qua, tong) = quet(&[
        tab("ttys001", "claude", None),
        tab("ttys002", "claude", Some("   \n\n")),
    ]);
    assert_eq!(ung_vien, vec!["ttys001".to_string(), "ttys002".to_string()]);
    assert_eq!((mu, bo_qua, tong), (2, 0, 2));
}

/// Trần chỉ cắt phần CHƯA CHẤM ĐƯỢC, và phần bị cắt phải ĐẾM ĐƯỢC.
///
/// Tab đã THẤY hộp thì không bao giờ bị cắt: trần sinh ra để chặn đường
/// hỏi-lại-từng-tab phình trở lại, không phải để bỏ rơi cửa sổ đang kẹt.
#[test]
fn tran_cat_phan_chua_cham_duoc_va_noi_ra_phan_bi_cat() {
    let hop = man_hop_tin_thu_muc();
    let mut tabs: Vec<Tab> = (0..20)
        .map(|i| tab(&format!("ttys{i:03}"), "claude", None))
        .collect();
    tabs.push(tab("ttys099", "claude", Some(&hop)));

    let (ung_vien, mu, bo_qua, tong) = quet(&tabs);
    assert_eq!(tong, 21);
    assert_eq!(mu, 20);
    assert_eq!(bo_qua, 20 - TRUST_FRESH_MAX, "phần bị trần cắt phải đếm được");
    assert_eq!(
        ung_vien.len(),
        TRUST_FRESH_MAX + 1,
        "trần + đúng một tab đã thấy hộp"
    );
    assert!(
        ung_vien.contains(&"ttys099".to_string()),
        "tab ĐÃ THẤY hộp không bao giờ được trần cắt"
    );
}

/// "Chưa đo được" là một trạng thái RIÊNG (luật 13②) — không được đọc thành
/// "quét xong, không cửa sổ nào kẹt".
#[test]
fn anh_chup_khong_mang_bang_tab_thi_khong_duoc_ket_luan_gi() {
    assert_eq!(
        quet_hop_tin_thu_muc(None),
        TrustQuet::ChuaDoDuoc,
        "không có bảng tab mà trả về một lượt quét RỖNG là dựng lại đúng cái bẫy \
         screen_of → None đã trả giá"
    );
    // …và một bảng tab RỖNG thì khác hẳn: đã đo, và máy thật sự không có tab nào.
    assert!(matches!(
        quet_hop_tin_thu_muc(Some(&[])),
        TrustQuet::Quet { tong_claude: 0, .. }
    ));
}

/// Cỗ máy này KHÔNG được tự mở lượt dò Terminal nữa — soi thẳng mã nguồn.
///
/// Cùng lối với `tests/cycle_wiring.rs`: hành vi "có hỏi Terminal thêm một lượt
/// không" không quan sát được từ trong tiến trình, mà đó chính là chỗ chết. Bài
/// kiểm này ĐỎ ĐƯỢC — trả `terminal_tabs()` về trong thân hàm là đỏ.
#[test]
fn tick_khong_duoc_mo_luot_do_rieng() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/pipeline.rs"
    ))
    .expect("đọc được src/pipeline.rs");
    for (ten, vi_sao) in [
        (
            "pub fn trust_dialog_tick(",
            "40 tab × 2 lượt osascript mỗi 30 giây — đúng cái giá vừa gỡ",
        ),
        (
            "pub fn orphan_windows_tick(",
            "thêm 4,8–17,2 giây dò gộp trên một ngân sách vòng 10 giây",
        ),
    ] {
        let start = src
            .find(ten)
            .unwrap_or_else(|| panic!("không còn hàm `{ten}` — đổi tên thì sửa cả bài kiểm này"));
        let rest = &src[start..];
        // 🔴 Lấy mục kế tiếp GẦN NHẤT, không phải mục `pub fn` gần nhất — lượt
        // chạy đầu của chính bài kiểm này đỏ vì thế: thân `orphan_windows_tick`
        // bị cắt tận `pub fn trust_dialog_tick`, nên nó nuốt trọn khối tài liệu
        // của hàm ấy và "bắt" được chữ `keys::terminal_tabs()` nằm trong một câu
        // kể chuyện. Một phép đo trỏ nhầm chỗ thì cái đỏ của nó cũng vô nghĩa
        // như cái xanh. Tính cả `pub const`/`fn` để không mục nào lọt qua.
        let end = ["\npub fn ", "\nfn ", "\npub const ", "\npub enum "]
            .iter()
            .filter_map(|m| rest[1..].find(m))
            .min()
            .map(|i| i + 1)
            .unwrap_or(rest.len());
        let than = &rest[..end];
        for cam in ["keys::terminal_tabs(", "keys::terminal_screens("] {
            assert!(
                !than.contains(cam),
                "`{ten}` gọi lại `{cam}` ⟹ {vi_sao}. Bảng tab phải lấy từ \
                 `SessionsSnapshot::tabs` — ảnh chụp của chính vòng ấy."
            );
        }
        assert!(
            than.contains("live.tabs"),
            "`{ten}` phải đọc bảng tab từ ảnh chụp của vòng (`live.tabs`)"
        );
    }
}
