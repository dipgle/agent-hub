//! Tin báo "hết hạn mức" phải trỏ vào một tài khoản CÒN CHẠY ĐƯỢC.
//!
//! 🔴 Hà 2026-08-30, hỏi giữa lúc bốn phiên acc3 đang đứng im: *"phiên đang lỗi
//! hoặc gần hết token muốn chủ động chuyển ngữ cảnh sang phiên mới với acc khác
//! thì dùng lệnh nào"*.
//!
//! Câu hỏi ấy là một phép đo, và huba trượt nó hai lần:
//!
//! ① Tin báo tự động ĐÃ có sẵn câu trả lời, nhưng nó gõ cứng `/handover -a acc1`
//!    từ lúc dựng (28/08). Ba ngày liền câu ấy đúng — vì acc1 tình cờ chưa lần
//!    nào là cái bị chặn. Một hằng số đúng nhờ hoàn cảnh chứ không nhờ lý lẽ:
//!    ngày acc1 hết hạn mức, huba bảo chủ máy chuyển sang đúng tài khoản vừa
//!    chết, và câu ấy đọc lên vẫn trơn tru như mọi câu khác.
//!
//! ② Mà sáng ấy tin báo còn không tới: bốn phiên kia đọc ra `❌ dừng vì LỖI` chứ
//!    không phải `🚫 HẾT HẠN MỨC` — xem `settle_limit`. Nên Hà phải tự đi hỏi
//!    cú pháp mà huba đã biết sẵn.

use huba::sessions::{settle_limit, state_of, LiveSession, ST_ERR, ST_LIMIT};
use huba::watch::{suggest_account, Change, Idle};

fn acc(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

fn phien(account: &str, limited: Option<&str>) -> LiveSession {
    LiveSession {
        host: "terminal".to_string(),
        account: account.to_string(),
        limited: limited.map(str::to_string),
        ..Default::default()
    }
}

/// 🔴 ĐỐI CHỨNG NGƯỢC cho cả tệp: bản gõ cứng `acc1` làm bài này ĐỎ, và nó là
/// bài duy nhất phân biệt được hai bản. Không có nó thì mọi bài dưới vẫn xanh
/// trên bản cũ, tức cổng không đo gì cả.
#[test]
fn the_dead_account_is_never_the_one_suggested() {
    let ba = acc(&["acc1", "acc2", "acc3"]);
    for chan in ["acc1", "acc2", "acc3"] {
        let goi_y = suggest_account(chan, &ba, &[phien(chan, Some("resets 1pm"))])
            .expect("còn hai tài khoản khác thì phải gợi ý được một cái");
        assert_ne!(
            goi_y, chan,
            "gợi ý chuyển sang chính tài khoản vừa bị chặn — cú chạm ấy tốn một lượt \
             gọi `claude` để nhận lại đúng câu chặn cũ"
        );
    }
}

/// Cửa thứ hai, và nó đo được: sáng 30/08 acc3 đứng `weekly limit · resets Sep 1`
/// trên BỐN phiên cùng lúc. Một tài khoản như thế còn sống trong cấu hình nhưng
/// đã chết trên thực tế — gợi ý sang đó là cú chạm vô ích thứ hai.
#[test]
fn an_account_already_blocked_elsewhere_is_skipped() {
    let ba = acc(&["acc1", "acc2", "acc3"]);
    let dang_song = [
        phien("acc2", Some("resets 10:30pm")),
        phien("acc1", Some("resets Sep 1 at 1pm")),
        phien("acc3", None),
    ];
    assert_eq!(
        suggest_account("acc2", &ba, &dang_song).as_deref(),
        Some("acc3"),
        "acc1 đang có phiên bị chặn ⟹ phải nhảy qua nó, dù nó đứng trước trong cấu hình"
    );
}

/// "Không có ai để gợi ý" là một CÂU TRẢ LỜI, không phải một chỗ trống — §13②.
/// Bịa ra một cái tên ở đây thì tốn của chủ máy đúng cái thứ đang thiếu.
#[test]
fn nothing_free_says_so_instead_of_naming_a_dead_account() {
    let ba = acc(&["acc1", "acc2", "acc3"]);
    let het = [
        phien("acc1", Some("resets 1pm")),
        phien("acc2", Some("resets 2pm")),
        phien("acc3", Some("resets 3pm")),
    ];
    assert_eq!(
        suggest_account("acc1", &ba, &het),
        None,
        "cả ba đều chặn thì không được trả về cái nào"
    );
    assert_eq!(
        suggest_account("acc1", &acc(&["acc1"]), &[]),
        None,
        "máy chỉ có MỘT tài khoản thì cũng không có gì để gợi ý"
    );
}

/// Câu chủ máy ĐỌC — chỗ duy nhất cái gợi ý có tác dụng. Đo cả hai kết cục:
/// có gợi ý thì câu lệnh chạy dán được, không có thì vẫn phải còn HÌNH DẠNG
/// câu lệnh (anh có thể biết một tài khoản huba chưa nhìn thấy; mất luôn cú
/// pháp thì anh phải đi tra, đúng lúc đang ở xa).
#[test]
fn the_sentence_carries_a_command_either_way() {
    let co = Change::Limited {
        id: "0789cefe-1111-2222-3333-444455556666".to_string(),
        name: "[dwork]".to_string(),
        acc: "acc3".to_string(),
        when: "resets Sep 1 at 1pm (Asia/Saigon)".to_string(),
        goi_y: Some("acc2".to_string()),
    };
    let cau = co.say(&Idle::Unknown, None);
    assert!(
        cau.contains("/handover -a acc2 0789cefe"),
        "phải là một dòng dán được, đủ cờ và đủ id: {cau:?}"
    );
    assert!(
        !cau.contains("acc3 0789cefe"),
        "không được trỏ về chính tài khoản đang chặn: {cau:?}"
    );

    let khong = Change::Limited {
        id: "0789cefe-1111-2222-3333-444455556666".to_string(),
        name: "[dwork]".to_string(),
        acc: "acc3".to_string(),
        when: "resets Sep 1 at 1pm (Asia/Saigon)".to_string(),
        goi_y: None,
    };
    let cau = khong.say(&Idle::Unknown, None);
    assert!(
        cau.contains("/handover -a"),
        "mất gợi ý thì vẫn phải giữ hình dạng câu lệnh: {cau:?}"
    );
    assert!(
        cau.contains("KHÔNG thấy tài khoản nào đang rảnh"),
        "phải nói ra là huba không biết, đừng im lặng bỏ nửa câu: {cau:?}"
    );
    for ten in ["acc1", "acc2", "acc3"] {
        assert!(
            !cau.contains(&format!("-a {ten}")),
            "không có gợi ý mà vẫn gõ ra một cái tên là bịa: {cau:?}"
        );
    }
}

/// ② — dòng hết hạn mức tới bằng đường NHẬT KÝ vẫn phải đọc ra `🚫`, không phải `❌`.
///
/// Nguyên văn của bốn phiên acc3 sáng 30/08, lấy y như nó nằm trong `error`.
#[test]
fn a_limit_arriving_as_a_journal_error_is_still_a_limit() {
    let (loi, chan) = settle_limit(
        Some("You've hit your weekly limit · resets Sep 1 at 1pm (Asia/Saigon)".to_string()),
        None,
    );
    assert!(
        chan.is_some_and(|k| k.contains("Sep 1")),
        "phải nhận ra là hạn mức, và giữ nguyên GIỜ MỞ LẠI"
    );
    assert_eq!(
        loi, None,
        "phải BỎ vế `error`: `state_of` xét `error` trước `limited`, nên để cả hai \
         cùng Some là giữ nguyên đúng cái đọc sai vừa vá"
    );

    let mut s = LiveSession {
        host: "terminal".to_string(),
        error: Some("You've hit your weekly limit · resets Sep 1 at 1pm".to_string()),
        ..Default::default()
    };
    assert_eq!(
        state_of(&s).0,
        ST_ERR,
        "chưa xếp lại thì nó vẫn là ❌ — nếu không thì bài dưới xanh vô nghĩa"
    );
    let (loi, chan) = settle_limit(s.error.take(), None);
    s.error = loi;
    s.limited = chan;
    assert_eq!(
        state_of(&s).0,
        ST_LIMIT,
        "sau khi xếp lại phải là 🚫 — chỉ tin 🚫 mới mang câu `/handover -a …`"
    );
}

/// 🔴 ĐỐI CHỨNG NGƯỢC của ②: một lỗi API THẬT không được đội lốt hạn mức. Nếu
/// `settle_limit` nuốt mọi thứ thành `limited` thì `❌` biến mất khỏi sản phẩm và
/// chủ máy thôi được báo về những cái chết thật sự thử lại được.
#[test]
fn a_real_api_error_stays_an_error() {
    for that in [
        "API Error: 500 Internal Server Error",
        "Request timed out.",
        "API Error: 401 token đã bị thu hồi",
    ] {
        let (loi, chan) = settle_limit(Some(that.to_string()), None);
        assert_eq!(chan, None, "{that:?} không phải hạn mức");
        assert_eq!(loi.as_deref(), Some(that), "phải giữ nguyên dòng lỗi thật");
    }
    // Và màn đọc được thì nó vẫn thắng: nguồn TRỰC TIẾP hơn nhật ký.
    let (loi, chan) = settle_limit(
        Some("API Error: 500".to_string()),
        Some("resets 10:30pm".to_string()),
    );
    assert_eq!(chan.as_deref(), Some("resets 10:30pm"));
    assert_eq!(
        loi, None,
        "đọc thẳng trên màn là bằng chứng chắc hơn, và hai vế không được cùng Some"
    );
}
