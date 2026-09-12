//! Một tài khoản **chưa dùng được** không bao giờ được gợi ý — dù sổ của nó
//! đọc ra con số đẹp nhất máy.
//!
//! 🔴 Ca thật, 2026-09-12, và nó tốn một lượt bàn giao: acc4 vừa được khai vào
//! `huba.config.json`. Đăng nhập xong thì CLI ghi ngay `cachedUsageUtilization`
//! = **0%** — nên `rank` đọc nó thành `Free(0)`, tức **rộng cửa nhất trong bốn
//! tài khoản**. acc2 kịch trần lúc 17:3x ⟹ `suggest_account` chọn acc4 ⟹ cửa sổ
//! `ttys007` mở ra và đứng ở **hộp chọn giao diện lần chạy đầu**, vì thư mục cấu
//! hình mới chưa ai onboard. huba báo trung thực *"phiên CHƯA chào đời sau 12
//! giây"*, nhưng phiên cũ thì đã bị bỏ lại, đang bị chặn.
//!
//! Bài học nằm ở chỗ hai câu nghe giống nhau mà đi hai đường:
//!   · *"chưa đo được hạn mức"*  → `Unknown`  → một cái đồng hồ chữa được;
//!   · *"chưa có ai ngồi vào máy này"* → `NotReady` → chỉ NGƯỜI chữa được.
//! Gộp chúng vào một hạng là đúng con bug này.
//!
//! Hai mốc dưới đây đo hai chiều trên bốn tài khoản THẬT (2026-09-12):
//!
//! |                                  | oauthAccount | hasCompletedOnboarding |
//! |----------------------------------|--------------|------------------------|
//! | acc1 · acc2 · acc3 (đang chạy)   | ✓            | ✓                      |
//! | acc4 **trước** khi đăng nhập     | ✗            | ✗                      |
//! | acc4 **sau** khi đăng nhập       | ✓            | ✗ ← vẫn treo ở wizard  |
//! | acc4 sau khi đóng nốt wizard     | ✓            | ✓ → hạng về `đã dùng 0%` |
//!
//! Hàng thứ ba là hàng đắt nhất: **có credential chưa đủ**. Đo được cùng lúc —
//! `claude -p` trên acc4 trả lời bình thường (tài khoản tốt), trong khi cửa sổ
//! tương tác vẫn đứng. huba mở cửa sổ lúc không có ai ngồi đó để bấm.

use huba::quota::{account_not_ready, rank, Quota, Rank, Ranked};
use huba::sessions::LiveSession;
use huba::watch::suggest_account;

/// 12:45 giờ máy — cùng quy ước với các tệp kiểm khác của `suggest_account`.
const LUC_12_45: u64 = 12 * 60 + 45;

fn so(oauth: bool, onboarded: Option<bool>) -> serde_json::Value {
    let mut d = serde_json::Map::new();
    if oauth {
        // Hình dạng thật: `oauthAccount` là một object 19 khoá. Ruột của nó
        // không ai đọc ở đây — sự CÓ MẶT mới là mốc.
        d.insert(
            "oauthAccount".into(),
            serde_json::json!({ "accountUuid": "x" }),
        );
    }
    if let Some(b) = onboarded {
        d.insert("hasCompletedOnboarding".into(), serde_json::json!(b));
    }
    serde_json::Value::Object(d)
}

fn quota_cua(ten: &str, pct: i64, chua_dung_duoc: Option<&str>) -> Quota {
    Quota {
        account: ten.into(),
        week_pct: Some(pct),
        week_resets_at: Some("2099-01-01T00:00:00+00:00".into()),
        hour5_pct: Some(pct),
        hour5_resets_at: Some("2099-01-01T00:00:00+00:00".into()),
        fetched_at_ms: Some(1_789_212_778_037),
        why_unknown: None,
        chua_dung_duoc: chua_dung_duoc.map(str::to_string),
    }
}

/// Ba trạng thái của acc4 trong đúng một ngày, đọc từ chính hai khoá ấy.
#[test]
fn ba_trang_thai_that_cua_acc4_doc_ra_dung_ba_cau() {
    assert_eq!(
        account_not_ready(&so(false, None)).as_deref(),
        Some("chưa đăng nhập (sổ không có oauthAccount)"),
        "10:33 — tệp do phép dò của huba sinh ra, chưa có lượt đăng nhập nào"
    );

    let giua = account_not_ready(&so(true, None));
    assert!(
        giua.is_some_and(|s| s.contains("lượt chạy đầu chưa xong")),
        "17:4x — ĐÃ có credential mà cửa sổ vẫn đứng: đây là hàng mà một cổng \
         chỉ hỏi 'đăng nhập chưa' sẽ bỏ lọt"
    );

    assert_eq!(
        account_not_ready(&so(true, Some(true))),
        None,
        "18:3x — đóng nốt wizard thì tài khoản dùng được"
    );

    // `hasCompletedOnboarding: false` tường minh cũng là chưa xong — đừng đọc
    // "có khoá" thành "đã xong".
    assert!(account_not_ready(&so(true, Some(false))).is_some());
}

/// Con số đẹp KHÔNG cứu được một tài khoản chưa mở cửa sổ nổi. Đây chính là
/// hình dạng đã cắn: `0%` là hạng rộng cửa nhất có thể có.
#[test]
fn con_so_dep_khong_qua_mat_duoc_cua_nay() {
    assert_eq!(
        rank(&quota_cua("acc4", 0, None), 0),
        Rank::Free(0),
        "đối chứng: cùng con số ấy, tài khoản dùng được thì vẫn là hạng tốt nhất"
    );
    assert_eq!(
        rank(&quota_cua("acc4", 0, Some("chưa đăng nhập")), 0),
        Rank::NotReady,
        "0% mà chưa dùng được thì không phải 'rộng cửa nhất', nó là một cánh cửa chưa mở"
    );
}

/// Cửa cuối: `suggest_account` phải bỏ qua nó, kể cả khi không còn ai khác.
/// "Không có tài khoản nào" là một CÂU TRẢ LỜI (§13②) — còn hơn mở một cửa sổ
/// rồi bỏ lại phiên cũ đang bị chặn.
#[test]
fn khong_bao_gio_goi_y_tai_khoan_chua_dung_duoc() {
    let hang = vec![
        Ranked {
            name: "acc1".into(),
            rank: Rank::Full,
        },
        Ranked {
            name: "acc2".into(),
            rank: Rank::Full,
        },
        Ranked {
            name: "acc4".into(),
            rank: Rank::NotReady,
        },
    ];
    assert_eq!(
        suggest_account("acc2", &hang, &[], LUC_12_45),
        None,
        "acc4 chưa dùng được ⟹ thà nói không biết chuyển đi đâu"
    );

    // ĐỐI CHỨNG NGƯỢC nằm ngay trong bài: đúng bảng ấy, chỉ đổi hạng acc4 thành
    // thứ nó BỊ đọc nhầm thành hôm 12/09 — nếu cửa mới không có thì bài trên
    // xanh vì lý do khác (vd acc4 bị loại bởi một cửa cũ nào đó).
    let nham = vec![
        Ranked {
            name: "acc1".into(),
            rank: Rank::Full,
        },
        Ranked {
            name: "acc2".into(),
            rank: Rank::Full,
        },
        Ranked {
            name: "acc4".into(),
            rank: Rank::Free(0),
        },
    ];
    assert_eq!(
        suggest_account("acc2", &nham, &[], LUC_12_45).as_deref(),
        Some("acc4"),
        "đây là hành vi CŨ — giữ lại để thấy rõ cửa mới đổi đúng cái gì"
    );
}

/// `NotReady` phải đứng cùng họ `Dead`, không cùng họ `Unknown`: một ẩn số vẫn
/// hơn cánh cửa đã đóng, nhưng một cánh cửa CHƯA AI MỞ thì không.
#[test]
fn xep_hang_dung_cho() {
    assert!(Rank::Free(99) < Rank::Unknown);
    assert!(Rank::Unknown < Rank::Full);
    assert!(Rank::Full < Rank::NotReady);
    assert!(Rank::NotReady < Rank::Dead);
}

/// Một phiên bình thường không mang cờ gì — giữ để `LiveSession` còn được dùng
/// ở đây, và để cửa "màn đang báo bị chặn" không vô tình bắt nhầm tài khoản
/// chưa dùng được (hai cửa khác nhau, đừng để chúng dựa vào nhau).
#[test]
fn cua_man_va_cua_so_la_hai_cua_roi_nhau() {
    let hang = vec![
        Ranked {
            name: "acc2".into(),
            rank: Rank::Free(30),
        },
        Ranked {
            name: "acc4".into(),
            rank: Rank::NotReady,
        },
    ];
    let dang_song = [LiveSession {
        account: "acc2".into(),
        host: "terminal".into(),
        ..Default::default()
    }];
    assert_eq!(
        suggest_account("acc1", &hang, &dang_song, LUC_12_45).as_deref(),
        Some("acc2"),
        "acc2 không mang dấu chặn nào ⟹ vẫn chọn được; acc4 bị loại bởi hạng, không bởi màn"
    );
}
