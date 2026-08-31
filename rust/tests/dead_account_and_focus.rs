//! Ba lệnh Hà chốt 2026-08-31, mỗi lệnh một cổng.
//!
//! ① *"Gõ lệnh này mở phiên mới tự nhảy vào acc1 trong khi tài khoản acc1 bị out
//!    rồi `/new dwork/a-dsign`"*
//! ② *"Bấm \"xem màn\" cũng không tự nhảy vào phiên đó là sao?"*
//! ③ (nền của ①) tài khoản chết theo kiểu **tổ chức khoá**, không phải hết hạn
//!    mức — và không sổ nào ghi nó.
//!
//! 🔴 Vì sao ③ đáng một cổng riêng: đo trên máy này cùng lúc Hà gửi ảnh,
//! `~/.claude.json` của acc1 vẫn ghi `seven_day 92%` với `fetchedAtMs` **28/08**
//! (già ba ngày) và `resets_at` đã qua từ 30/08 — nên [`huba::quota`] xếp acc1
//! vào `Unknown`, tức *"chưa đo được"*, KHÔNG phải *"đã chết"*. Luật chọn tài
//! khoản mới dựng sáng cùng ngày cũng không chặn được ca này. Dòng chữ trên MÀN
//! là nguồn duy nhất.

use huba::quota::{Rank, Ranked};
use huba::sessions::LiveSession;
use huba::watch::suggest_account;

/// Nguyên văn màn của phiên `projects-a4`, ảnh Hà gửi 2026-08-31 08:29.
const MAN_KHOA: &str = "\
Claude Code v2.1.228
Opus 5 (1M context) with xhigh effort · Claude Max
~/projects
❯ dwork/a-dsign
Your organization has disabled Claude subscription access for Claude Code · Use an Anthropic API key instead, or ask your admin to enable access
✻ Brewed for 0s";

/// ③ Đọc được dòng khoá, và KHÔNG kêu oan trên một màn đang bàn về nó.
#[test]
fn doc_duoc_dong_to_chuc_khoa_va_khong_keu_oan() {
    let thay = huba::keys::account_blocked_on_screen(MAN_KHOA)
        .expect("màn thật có câu khoá mà không đọc ra");
    assert!(
        thay.contains("disabled") && thay.contains("subscription access"),
        "giữ nguyên văn để chủ máy đọc được lý do: {thay:?}"
    );

    // ĐỐI CHỨNG NGƯỢC ①: màn thường không được đọc ra là bị khoá.
    assert_eq!(
        huba::keys::account_blocked_on_screen("❯ đang chạy test\n✻ Brewed for 3s"),
        None
    );
    // ĐỐI CHỨNG NGƯỢC ②: một phiên đang BÀN về chuyện bị khoá — đúng phiên này —
    // nhắc chữ `disabled` trong một câu văn dài thì KHÔNG phải trạng thái.
    let ban_luan = "Tôi vừa đọc ra rằng tổ chức đã disabled quyền subscription access \
                    của acc1, nên tôi thêm một phép đo mới cho nó và một cổng đứng sau, \
                    kèm đối chứng ngược để nó không kêu oan trên chính màn này.";
    assert_eq!(
        huba::keys::account_blocked_on_screen(ban_luan),
        None,
        "câu văn dài nhắc tới nó không phải một dòng trạng thái"
    );
}

fn phien(acc: &str, dead: bool) -> LiveSession {
    LiveSession {
        host: "terminal".to_string(),
        account: acc.to_string(),
        account_dead: dead.then(|| "Your organization has disabled…".to_string()),
        ..Default::default()
    }
}

/// ① Tài khoản đã CHẾT thì không bao giờ được chọn — kể cả khi hạng của nó là
/// `Unknown`, tức thứ hạng "còn cửa hơn" một tài khoản đã kịch trần.
///
/// Đây đúng ca acc1: sổ ghi số cũ ba ngày ⟹ `Unknown`; nếu chỉ xếp theo hạng thì
/// `Unknown` đứng TRƯỚC `Full`, và `/new` lại nhảy vào đúng tài khoản đã chết.
#[test]
fn tai_khoan_bi_khoa_khong_bao_gio_duoc_chon() {
    let hang = vec![
        Ranked {
            name: "acc1".into(),
            rank: Rank::Unknown,
        },
        Ranked {
            name: "acc2".into(),
            rank: Rank::Free(22),
        },
        Ranked {
            name: "acc3".into(),
            rank: Rank::Full,
        },
    ];
    let live = [phien("acc1", true), phien("acc2", false)];
    assert_eq!(
        suggest_account("", &hang, &live).as_deref(),
        Some("acc2"),
        "acc1 màn báo bị khoá ⟹ phải nhảy qua, dù hạng của nó (`Unknown`) đứng trước acc3"
    );

    // ĐỐI CHỨNG NGƯỢC: bỏ dấu chết đi thì acc1 vẫn KHÔNG được chọn (acc2 rộng
    // cửa hơn) — nên bài trên phải chấm bằng một ca mà dấu chết THẬT SỰ đổi kết
    // quả. Ca ấy là đây: chỉ còn acc1 và acc3.
    let hai = vec![
        Ranked {
            name: "acc1".into(),
            rank: Rank::Unknown,
        },
        Ranked {
            name: "acc3".into(),
            rank: Rank::Full,
        },
    ];
    assert_eq!(
        suggest_account("", &hai, &[phien("acc1", false)]).as_deref(),
        Some("acc1"),
        "acc1 còn sống ⟹ `Unknown` vẫn hơn `Full`"
    );
    assert_eq!(
        suggest_account("", &hai, &[phien("acc1", true)]),
        None,
        "acc1 chết + acc3 kịch trần ⟹ KHÔNG có gì để gợi ý, và nói thẳng là không có"
    );
}
