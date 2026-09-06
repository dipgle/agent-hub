//! CON TRỎ LÀ CỦA CHỦ MÁY — huba không tự chọn phiên làm việc thay anh.
//!
//! 🔴 Hà 2026-09-03: *"Tại sao việc chuyển phiên tự động lại tự nhảy vào phiên
//! đang làm việc → tôi đang gửi tin đi thì nó nhảy vào đó chứ không phải vào
//! phiên tôi đã chọn ban đầu → việc chọn phiên làm việc chỉ được xuất phát từ
//! phía tôi gửi lệnh"*.
//!
//! Nhật ký `logs/huba.log` cùng chiều hôm ấy — ba lượt trong năm phút, và **hai
//! tin của chủ máy đi lạc**:
//!
//! ```text
//! 19:04:49  auto_limit_firing  c68090e5 [huba]  → 19:05:01 phiên 95180f77 chào đời
//! 19:06:42  telegram_text_as_typing → 95180f77   ← chữ của anh rơi vào [huba]
//! 19:07:01  auto_limit_firing  9df3b075         → 521f5c7c chào đời
//! 19:07:10  telegram_text_as_typing → 521f5c7c   ← rơi tiếp vào [tafalo5]
//! 19:09:32  auto_limit_firing  dbd80185 [fbot]  → 32fe99f0 chào đời
//! 19:10:00  /session 32fe99f0…                   ← anh tự đi tìm lại phiên mình
//! 19:11:12  telegram_text_as_typing → 32fe99f0   ← ĐÚNG câu 59 ký tự ấy, gửi LẠI
//! ```
//!
//! Bàn giao chạy đúng từ đầu tới cuối. Chỗ hỏng là hai dòng
//! `set_cursor(FOCUS_SESSION_KEY, new_id)`: chúng đổi **nơi chữ anh gõ sẽ đi
//! tới** trong một lượt không ai bấm gì.
//!
//! Bài kiểm này đo hai thứ, và cả hai đều là câu CHỮ — vì thứ chủ máy nhận được
//! là câu chữ, không phải một trường trong sổ:
//! * tin tự động phải nói con trỏ đang ở đâu, và ba ca phải đọc KHÁC nhau;
//! * gõ vào một phiên vừa bị bàn giao thì huba phải chỉ ra phiên kế nhiệm, chứ
//!   không bắt anh tự đi tìm cái phiên chính nó vừa mở.

use huba::pipeline::{
    auto_handover_notice, auto_limit_notice, focus_kept, successor_in, FocusKept, HandoverMove,
    Successor,
};
use std::collections::BTreeMap;

const MOI: &str = "32fe99f0-ec86-4dfd-bfc3-4408ded8ded9";
const CU: &str = "dbd80185-cac5-4306-a7fd-8e83db1184bc";

fn tin_ban_giao(focus: FocusKept) -> String {
    auto_handover_notice(
        "[AI/huba]",
        80,
        126,
        &HandoverMove::Opened {
            tty: "ttys007",
            new_id: MOI,
            closed_err: None,
            retrying: false,
            focus,
        },
    )
}

fn tin_het_han_muc(focus: FocusKept) -> String {
    auto_limit_notice(
        "[fbot]",
        "acc1",
        "acc2",
        "resets 9:50pm (Asia/Saigon)",
        &HandoverMove::Opened {
            tty: "ttys007",
            new_id: MOI,
            closed_err: None,
            retrying: false,
            focus,
        },
    )
}

/// CA ĐÃ HỎNG: chủ máy đang theo phiên KHÁC. Con trỏ không được nhúc nhích, và
/// tin phải nói ra chữ anh gõ vẫn đi vào đâu.
#[test]
fn dang_theo_phien_khac_thi_tin_phai_noi_con_tro_khong_doi() {
    for (ten, tin) in [
        ("bàn giao", tin_ban_giao(FocusKept::Elsewhere("[fbot]"))),
        (
            "hết hạn mức",
            tin_het_han_muc(FocusKept::Elsewhere("[fbot]")),
        ),
    ] {
        assert!(
            tin.contains("Con trỏ KHÔNG đổi"),
            "{ten}: phải nói thẳng là con trỏ đứng yên: {tin:?}"
        );
        assert!(
            tin.contains("[fbot]"),
            "{ten}: phải gọi TÊN phiên anh đang theo — không thì anh vẫn phải đi tra: {tin:?}"
        );
        // Câu cũ, và nó là dấu hiệu hỏng chứ không phải một cách nói khác.
        assert!(
            !tin.contains("Đang theo phiên mới"),
            "{ten}: huba lại tự nhận đã kéo con trỏ: {tin:?}"
        );
        // Phiên mới vẫn phải gõ được — bỏ cướp con trỏ không có nghĩa là giấu
        // đường sang.
        assert!(
            tin.contains(MOI),
            "{ten}: mất id phiên mới thì anh không sang được: {tin:?}"
        );
    }
}

/// ĐỐI CHỨNG NGƯỢC ① (§13①): con trỏ đang ở CHÍNH phiên vừa đóng sổ.
///
/// Cùng một `HandoverMove::Opened`, chỉ đổi mỗi `focus`, mà câu y hệt thì cái
/// trường ấy không đo gì cả — nó chỉ là một trường bị bỏ quên.
#[test]
fn con_tro_o_phien_vua_tat_phai_doc_khac_han() {
    let khac = tin_ban_giao(FocusKept::Elsewhere("[fbot]"));
    let vua_tat = tin_ban_giao(FocusKept::OnEnded);
    let chua_theo = tin_ban_giao(FocusKept::Nowhere);

    assert_ne!(khac, vua_tat, "hai ca đòi chủ máy hai việc khác nhau");
    assert_ne!(khac, chua_theo, "ba ca thì phải ba câu");
    assert_ne!(vua_tat, chua_theo, "ba ca thì phải ba câu");

    assert!(
        vua_tat.contains("VỪA ĐÓNG SỔ"),
        "phải nói con trỏ đang trỏ vào cái phiên không còn: {vua_tat:?}"
    );
    // Và ngay cả ở ca này huba vẫn KHÔNG tự chuyển — đó là cả bản vá.
    assert!(
        !vua_tat.contains("Đang theo phiên mới"),
        "kể cả khi con trỏ trỏ vào phiên đã tắt thì vẫn là chủ máy chọn: {vua_tat:?}"
    );
    assert!(
        chua_theo.contains("Chưa theo phiên nào"),
        "chưa chọn gì thì nói thẳng, đừng chọn hộ: {chua_theo:?}"
    );
}

/// Hai cái mồm nói MỘT câu (luật 11): tin bàn giao và tin hết hạn mức phải dùng
/// đúng một câu về con trỏ.
///
/// Trước bản vá chúng lệch nhau ở chỗ khác nhưng **cùng nói sai một câu**, vì
/// câu ấy được chép tay ở hai nơi cách nhau ~350 dòng.
#[test]
fn hai_tin_tu_dong_noi_cung_mot_cau_ve_con_tro() {
    let a = tin_ban_giao(FocusKept::Elsewhere("[fbot]"));
    let b = tin_het_han_muc(FocusKept::Elsewhere("[fbot]"));
    let cau = "Con trỏ KHÔNG đổi — chữ anh gõ vẫn đi vào [fbot].";
    assert!(a.contains(cau), "tin bàn giao lệch câu: {a:?}");
    assert!(b.contains(cau), "tin hết hạn mức lệch câu: {b:?}");
}

/// Phép so thuần: ba ca vào, ba ca ra. Rỗng ≠ trùng ≠ khác.
#[test]
fn focus_kept_phan_biet_du_ba_ca() {
    assert!(matches!(focus_kept("", CU, "[fbot]"), FocusKept::Nowhere));
    assert!(matches!(
        focus_kept("   ", CU, "[fbot]"),
        FocusKept::Nowhere
    ));
    assert!(matches!(focus_kept(CU, CU, "[fbot]"), FocusKept::OnEnded));
    // Khoảng trắng thừa quanh id không được đọc thành "một phiên khác" — đó là
    // cách một con trỏ trỏ đúng phiên vừa tắt lọt qua thành `Elsewhere`.
    assert!(matches!(
        focus_kept(&format!(" {CU} "), CU, "[fbot]"),
        FocusKept::OnEnded
    ));
    match focus_kept(MOI, CU, "[fbot]") {
        FocusKept::Elsewhere(ten) => assert_eq!(ten, "[fbot]"),
        _ => panic!("con trỏ ở phiên khác mà đọc thành trùng/rỗng"),
    }
}

fn so_ke_nhiem() -> BTreeMap<String, Successor> {
    let mut b = BTreeMap::new();
    b.insert(
        CU.to_string(),
        Successor {
            new_id: MOI.to_string(),
            at: 1_788_000_000,
        },
    );
    b
}

/// Gõ vào một phiên vừa bị bàn giao: huba phải biết ai thay nó.
///
/// Nhận cả id NGẮN vì chủ máy gõ id ngắn suốt — huba tự in id 8 ký tự khắp nơi.
#[test]
fn so_ke_nhiem_tra_duoc_ca_id_day_du_lan_id_ngan() {
    let b = so_ke_nhiem();
    assert_eq!(successor_in(&b, CU).as_deref(), Some(MOI));
    assert_eq!(successor_in(&b, &CU[..8]).as_deref(), Some(MOI));
}

/// ĐỐI CHỨNG NGƯỢC ②: sổ không được đoán bừa.
///
/// Một phép tra `contains` hay một tiền tố 3 ký tự sẽ trả về "phiên kế nhiệm"
/// cho một id gõ nhầm — và câu trả lời ấy chỉ chủ máy đi vào một phiên **không
/// liên quan gì** tới cái anh vừa gõ. Thà nói "không thấy phiên".
#[test]
fn so_ke_nhiem_khong_doan_bua() {
    let b = so_ke_nhiem();
    assert_eq!(successor_in(&b, ""), None, "id rỗng không tra được gì");
    assert_eq!(successor_in(&b, "   "), None);
    assert_eq!(
        successor_in(&b, "dbd8018"),
        None,
        "7 ký tự là quá ngắn để chắc — id ngắn của huba luôn 8"
    );
    assert_eq!(
        successor_in(&b, "cac5"),
        None,
        "khúc GIỮA id không phải tiền tố: tra kiểu `contains` là đoán bừa"
    );
    assert_eq!(
        successor_in(&b, "deadbeef"),
        None,
        "id lạ thì trả None, đừng dựng ra một phiên kế nhiệm"
    );
}

/// Mã nguồn của `pipeline.rs`, nhúng lúc BIÊN DỊCH.
///
/// `include_str!` chứ không phải đọc tệp lúc chạy: tệp đổi tên hay dời chỗ thì
/// **không biên dịch được** — hỏng to, thấy ngay. Một `fs::read` sẽ trả `Err`
/// rồi bài kiểm khéo léo bỏ qua, tức đúng cái cổng không đo gì (§13②).
const NGUON: &str = include_str!("../src/pipeline.rs");

/// Số chỗ đặt con trỏ CÒN LẠI trong `pipeline.rs`, khai MẪU SỐ (§13③).
///
/// 9 chỗ, và cả 9 đều đi sau một lệnh của chủ máy: `/new` · `/term run` ·
/// `/handover -a` (hai nhánh) · `/terminal` trần · `shot_<id>` · `/follow` bỏ
/// theo · `/follow <id>` · `s_<id>`.
///
/// Con số này là một CỔNG, không phải trang trí: thêm một đường đặt con trỏ mà
/// không sửa số ⟹ đỏ, và người sửa phải nói ra đường mới ấy xuất phát từ lệnh
/// nào của chủ máy. Đó đúng là câu hỏi Hà đặt ra ngày 03/09.
const SO_CHO_DAT_CON_TRO: usize = 9;

/// Cắt thân một hàm cấp cao nhất: từ dòng khai báo tới dấu `}` ở cột 0.
///
/// Thiếu mỏ neo ⟹ **panic**, không phải bỏ qua: hàm bị đổi tên mà cổng vẫn xanh
/// là cổng nói dối (§13②).
fn than_ham(ten: &str) -> &'static str {
    let dau = NGUON.find(ten).unwrap_or_else(|| {
        panic!("không thấy `{ten}` trong pipeline.rs — cổng này mù rồi, sửa mỏ neo")
    });
    let phan = &NGUON[dau..];
    let cuoi = phan
        .find("\n}\n")
        .unwrap_or_else(|| panic!("không thấy dấu đóng hàm của `{ten}`"));
    &phan[..cuoi]
}

/// 🔴 CỔNG: hai đường TỰ ĐỘNG không được đụng vào con trỏ.
///
/// Đây là hàng rào cho chính hành vi Hà bắt được — và nó khoá thứ mà mấy bài
/// kiểm câu chữ ở trên KHÔNG khoá được: những hàm ấy cần một máy đang chạy
/// `claude` và một cửa sổ Terminal thật, nên không có cách nào gọi chúng trong
/// một bài kiểm. Cổng đọc mã là cổng duy nhất với tới được, nên nó phải tự khai
/// mẫu số và phải chết khi mất mỏ neo.
#[test]
fn duong_tu_dong_khong_duoc_tu_chon_phien() {
    for ten in [
        "fn auto_handover(db: &Db",
        "fn auto_switch_on_limit(db: &Db",
    ] {
        let than = than_ham(ten);
        assert!(
            !than.contains("db.set_cursor(FOCUS_SESSION_KEY"),
            "`{ten}` lại tự đặt con trỏ — đúng cái làm hai tin của chủ máy đi lạc chiều 03/09"
        );
    }

    // MẪU SỐ: cổng phải nhìn thấy CÁI GÌ ĐÓ, không thì nó xanh vì mù.
    let tong = NGUON.matches("db.set_cursor(FOCUS_SESSION_KEY").count();
    assert!(
        tong > 0,
        "không thấy chỗ đặt con trỏ nào — mẫu chuỗi sai, cổng đang đo hư không"
    );
    assert_eq!(
        tong, SO_CHO_DAT_CON_TRO,
        "số đường đặt con trỏ đổi ({tong} ≠ {SO_CHO_DAT_CON_TRO}). Nếu là đường MỚI: nó xuất \
         phát từ lệnh nào của chủ máy? Trả lời được thì sửa hằng số và ghi vào doc của nó."
    );
}
