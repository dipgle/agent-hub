//! LUẬT 9 — **không có tiền trên màn**, và đây là chốt canh nó.
//!
//! 🔴 Viết ngày 2026-08-14 để **thay hai chốt vừa mất**. Tới hôm ấy luật này
//! được canh bởi `portal.rs::the_snapshot_carries_no_inbox_and_no_money` và
//! `fe-board-uc.mjs`; cả hai đi cùng trang tfl5, và đi thì luật còn lại một mình
//! với mấy dòng `#[serde(skip_serializing)]` — tức một quy ước, không phải một
//! phép đo. Luật này **đã mọc lại một lần rồi** (trần chi tiêu quay lại dưới
//! dạng bảng giá), nên để nó không có ai canh là chuyện của thời gian.
//!
//! Chốt mới đứng THẤP HƠN chốt cũ, và đó là cái được: bản cũ soi ảnh chụp của
//! một kênh: đúng cho kênh ấy, và chết theo kênh ấy. Bản này soi chính **hình
//! dạng lúc tuần tự hoá** của ba cấu trúc mang giá — nên nó còn đúng với mọi
//! chỗ gửi sau này, kể cả chỗ chưa ai viết.
//!
//! Một câu về cách đọc: `cost_usd` **vẫn được ghi** (sổ `spend`, luật 8 — để câu
//! hỏi ấy TRẢ LỜI ĐƯỢC nếu có ai hỏi). Thứ bị cấm là nó **đi ra ngoài**.

use hub::sessions::{Aside, Handover, LiveSession, SessionsSnapshot, Told};

/// Con số phải KHÁC THƯỜNG: `0.5` hay `1.0` có thể trùng một trường khác rồi
/// làm phép đo báo đỏ oan; chuỗi này thì không thể xuất hiện vì lý do nào khác.
const PRICE: f64 = 7.7311331;
const PRICE_TEXT: &str = "7.7311331";

/// Mọi tên khoá đã từng, hoặc có thể, mang một con số tiền ra khỏi máy này.
const FORBIDDEN: &[&str] = &[
    "cost_usd",
    "cost_days",
    "owner_spend",
    "owner_budget",
    "budget",
    "spend",
    "usd",
    // Hai cái này không phải tiền mà là HỘP THƯ — cùng chuyến đi 2026-08-08, và
    // cùng một lý do để canh: nhánh ấy cũng từng mọc lại.
    "items",
    "counts",
];

fn assert_no_price(what: &str, json: &str) {
    for gone in FORBIDDEN {
        assert!(
            !json.contains(gone),
            "{what}: `{gone}` lọt ra ngoài — luật 9.\n{json}"
        );
    }
    assert!(
        !json.contains(PRICE_TEXT),
        "{what}: con số giá đi ra ngoài dù tên trường đã giấu.\n{json}"
    );
}

/// Ba cấu trúc mang giá: ghi thì có, gửi thì không.
///
/// Mỗi cái được dựng với giá KHÁC 0 trước khi tuần tự hoá — nếu để `Default`
/// thì `cost_usd` bằng 0.0 và bài kiểm này xanh kể cả khi ai đó gỡ
/// `skip_serializing`. Đó đúng là hình dạng "phép đo mù" mà `OPERATING-CHARTER`
/// §2d gọi tên: một assert không bao giờ đỏ được.
#[test]
fn the_three_structs_that_know_a_price_never_publish_it() {
    let h = Handover {
        source_id: "4963b95c".into(),
        source_name: "projects-ff".into(),
        new_session_id: "0a109818".into(),
        checkpoint: "đang sửa cổng người".into(),
        cost_usd: PRICE,
        ts: "2026-08-14T15:48:17Z".into(),
        resume_command: "claude --resume 0a109818".into(),
    };
    // Vế thứ nhất của luật: GHI thì vẫn ghi. Bỏ vế này thì "không có tiền trên
    // màn" đọc thành "không đo tiền nữa", và đó là một luật khác hẳn.
    assert_eq!(h.cost_usd, PRICE, "giá phải còn trong bộ nhớ để ghi sổ");
    let json = serde_json::to_string(&h).expect("handover serialises");
    assert!(
        json.contains("resume_command"),
        "phép đo phải chạm được vào thứ nó đo — bản JSON này rỗng?\n{json}"
    );
    assert_no_price("Handover", &json);

    let a = Aside {
        source_id: "4963b95c".into(),
        source_name: "projects-ff".into(),
        new_session_id: "0a109818".into(),
        question: "đang kẹt ở đâu?".into(),
        answer: "ở cái cổng không từ chối được".into(),
        cost_usd: PRICE,
        ts: "2026-08-14T15:48:17Z".into(),
    };
    assert_eq!(a.cost_usd, PRICE);
    let json = serde_json::to_string(&a).expect("aside serialises");
    assert!(json.contains("question"), "bản JSON này rỗng?\n{json}");
    assert_no_price("Aside", &json);

    let t = Told {
        session_id: "4963b95c".into(),
        source_name: "projects-ff".into(),
        text: "chạy lại test đi".into(),
        answer: "263 xanh".into(),
        cost_usd: PRICE,
        ts: "2026-08-14T15:48:17Z".into(),
    };
    assert_eq!(t.cost_usd, PRICE);
    let json = serde_json::to_string(&t).expect("told serialises");
    assert!(json.contains("answer"), "bản JSON này rỗng?\n{json}");
    assert_no_price("Told", &json);
}

/// Và bản chụp phiên — thứ đi xa nhất khỏi máy này.
///
/// Dựng bằng dữ liệu THẬT chứ không `Default::default()` trơn: một bản chụp
/// rỗng tuần tự hoá ra vài chục ký tự, và một assert "không chứa X" trên vài
/// chục ký tự thì đúng mà vô nghĩa.
#[test]
fn the_sessions_snapshot_carries_no_price_and_no_inbox() {
    let snap = SessionsSnapshot {
        sessions: vec![LiveSession {
            session_id: "4963b95c-93b0-46e3-baf9-40bbfacbef2f".into(),
            name: "projects-ff".into(),
            account: "acc1".into(),
            cwd: "/Users/hanguyen/projects/hub".into(),
            host: "terminal".into(),
            working: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    let json = serde_json::to_string(&snap).expect("snapshot serialises");
    // Phép đo phải chứng minh nó có nhìn thấy gì đó trước khi nói "không thấy".
    assert!(
        json.contains("projects-ff") && json.contains("acc1"),
        "bản chụp không mang nổi phiên nào thì assert vắng mặt là vô nghĩa\n{json}"
    );
    assert_no_price("SessionsSnapshot", &json);
}

/// 🔴 Bài kiểm canh chính BÀI KIỂM TRÊN.
///
/// `assert_no_price` chỉ có giá trị nếu nó thật sự đỏ được. Cả ba bài trên đều
/// mong một câu trả lời VẮNG MẶT, và một phép đo mong sự vắng mặt là loại phép
/// đo dễ hỏng nhất trên đời: sai selector, sai tên trường, sai kiểu — kết quả
/// vẫn là "không thấy gì", vẫn xanh.
///
/// Nên ở đây dựng đúng cái hình dạng bị cấm rồi kiểm rằng nó BỊ BẮT.
#[test]
fn the_guard_above_is_able_to_fail() {
    let leaked = serde_json::json!({
        "sessions": [{ "name": "projects-ff" }],
        "owner_spend": 12.5
    })
    .to_string();
    let caught = std::panic::catch_unwind(|| assert_no_price("cố tình rò", &leaked));
    assert!(
        caught.is_err(),
        "chốt canh không bắt được một trường tiền bày sẵn — nó là phép đo mù"
    );

    // …và bắt được cả khi tên trường đã đổi mà con số thì còn.
    let renamed = serde_json::json!({
        "sessions": [{ "name": "projects-ff" }],
        "gia_tri_mot_luot": PRICE
    })
    .to_string();
    let caught = std::panic::catch_unwind(|| assert_no_price("đổi tên trường", &renamed));
    assert!(
        caught.is_err(),
        "đổi tên trường là đường vòng hiển nhiên nhất — phải bắt bằng chính con số"
    );
}
