//! Ô nhập là phần NẰM GIỮA HAI VIỀN — thứ vẽ dưới viền dưới không phải chữ của
//! người gõ.
//!
//! 🔴 Hà 2026-09-23: *"chỉnh lại lệnh clean để xóa toàn bộ ô nhập của phiên,
//! hiện tại xóa mỗi dòng thì phải"*. Đo cùng ngày, `logs/huba.log`:
//!
//! - 07:54:41Z và 07:55:13Z — `/clean` hai lượt vào `[dwork]`, cả hai kết thúc
//!   bằng `keys_clear_gave_up left_seen=194` (16 lô × 400 DEL), và câu trả lời
//!   tự mâu thuẫn *"ô nhập đã sạch. ⚠ ô nhập vẫn còn chữ"*;
//! - màn chụp ngay sau (`fixtures/man-bang-subagent-duoi-o-nhap-2026-09-23.txt`):
//!   ô nhập TRỐNG. 194 ký tự kia là **bảng subagent** TUI vẽ dưới dòng chân
//!   (`⏺ main` · `◯ general-purpose  Counting push …  14m 8s · ↓ 165.4k tokens`),
//!   và `input_box_text` đọc từ viền trên tới HẾT MÀN nên gói luôn bảng ấy vào
//!   "chữ trong ô";
//! - cùng gốc, vòng tự gỡ kẹt: `auto_unstick_box_firing` vào chính phiên ấy lúc
//!   07:56–07:59Z với `text_len` 194/209 — bấm Enter vào một ô trống.
//!
//! Từ 29/08 tới hôm ấy: 15 lượt `keys_clear_gave_up`, `left_seen` từ 114 tới
//! 2211 — con số không nhúc nhích qua nhiều lượt xoá liền nhau (6 lượt 18/09
//! đều 2211), tức thứ bị đếm là thứ DEL không chạm tới được.

use huba::keys::input_box_text;

const MAN_THAT: &str = include_str!("fixtures/man-bang-subagent-duoi-o-nhap-2026-09-23.txt");

/// Dòng dấu nhắc của ô trong màn thật, kèm dòng trống bên dưới nó — thay khúc
/// này là đổ chữ vào ĐÚNG ô ấy, giữ nguyên hai viền và bảng subagent.
///
/// ⚠ Sau `❯` là NBSP (`c2 a0`, đo bằng `xxd` trên tệp mẫu), không phải dấu
/// cách thường — viết `"❯ "` thì khúc này không khớp và bài kiểm đo nhầm thứ.
const DAU_NHAC_TRONG: &str = "❯\u{a0}\n\n";

/// 🔴 Ca đã xảy ra thật: ô trống, bảng subagent bên dưới ⟹ KHÔNG có chữ.
#[test]
fn o_trong_ma_co_bang_subagent_ben_duoi_thi_khong_co_chu() {
    assert!(
        MAN_THAT.contains("◯ general-purpose") && MAN_THAT.contains(DAU_NHAC_TRONG),
        "tệp mẫu phải còn nguyên bảng subagent và dấu nhắc trống — không thì bài kiểm đang đo nhầm thứ"
    );
    assert_eq!(
        input_box_text(MAN_THAT),
        None,
        "bảng subagent dưới dòng chân bị đọc thành chữ trong ô — đúng lỗi 23/09, \
         `/clean` bắn 6400 DEL rồi vẫn khai 'còn chữ'"
    );
}

/// Chiều còn lại: có chữ thật (nhiều dòng) trong ô thì đọc ĐÚNG chữ ấy — không
/// sót dòng nào, không dính bảng subagent. Một cái phanh làm hàm luôn trả
/// `None` cũng làm bài trên xanh.
#[test]
fn chu_nhieu_dong_trong_o_doc_du_va_khong_dinh_bang_ben_duoi() {
    let man = MAN_THAT.replacen(
        DAU_NHAC_TRONG,
        "❯ dòng một của câu đang gõ\n  dòng hai của câu đang gõ\n  dòng ba\n",
        1,
    );
    assert_eq!(
        input_box_text(&man).as_deref(),
        Some("dòng một của câu đang gõ dòng hai của câu đang gõ dòng ba"),
    );
}

/// Một vạch NGẮN nằm trong nội dung (đầu ra lệnh dán vào ô hay có) không phải
/// viền dưới: viền dưới rộng ĐÚNG bằng viền trên. Cắt ở vạch đầu tiên gặp là
/// giấu nửa sau của ô — và `clear_box` sẽ dừng khi còn chữ.
#[test]
fn vach_ngan_trong_noi_dung_khong_phai_vien_duoi() {
    let man = "\
────────────────────────────────────────
❯ [huba chạy hộ]
  ────────────
  phần sau vạch vẫn là chữ trong ô
────────────────────────────────────────
  ⏵⏵ auto mode on · 2 shells

  ⏺ main
  ◯ general-purpose  Soát cây làn                              3m 1s · ↓ 12k tokens";
    // Dòng toàn vạch thì bị bỏ như mọi vạch khác (luật lọc có từ trước); thứ
    // bài này chấm là chữ SAU vạch vẫn còn, và bảng dưới viền thì không.
    assert_eq!(
        input_box_text(man).as_deref(),
        Some("[huba chạy hộ] phần sau vạch vẫn là chữ trong ô"),
    );
}

/// Ô đóng khung `╭ … ╰`: viền và thứ dưới `╰` đều không phải chữ.
#[test]
fn o_khung_bo_goc_chi_doc_phan_giua() {
    let man = "\
╭──────────────────────────────────────╮
│ > câu đang gõ                        │
╰──────────────────────────────────────╯
  ⏺ main
  ◯ general-purpose  Soát cây làn   3m 1s";
    let doc = input_box_text(man).unwrap_or_default();
    assert!(
        doc.contains("câu đang gõ"),
        "phải đọc ra chữ trong khung: {doc:?}"
    );
    for ngoai in ['╭', '╰', '⏺', '◯'] {
        assert!(
            !doc.contains(ngoai),
            "viền và bảng dưới `╰` lọt vào chữ trong ô ({ngoai}): {doc:?}"
        );
    }
}

/// Viền dưới bị mép màn cắt mất (khối dán dài hơn phần màn còn trống) thì vẫn
/// đọc được phần nhìn thấy — không có viền dưới không có nghĩa là không có chữ.
#[test]
fn khong_thay_vien_duoi_van_doc_phan_nhin_thay() {
    let man = "\
────────────────────────────────────────
❯ dòng một
  dòng hai";
    assert_eq!(input_box_text(man).as_deref(), Some("dòng một dòng hai"));
}
