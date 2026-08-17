//! Bảng hỏi NHIỀU CÂU: đọc nguyên bảng, nói đúng, và đi tới được câu bất kỳ.
//!
//! 🔴 Hà 2026-08-13, ảnh chụp bảng hỏi của `[AI/tfl5]` (`☒ Vá ACL` · `☐ Đăng
//! nhập` · `You have not answered all questions`): *"chọn option xong thì vẫn
//! còn bước nữa nên không pass qua được"* · *"có nhiều option thì phải có cơ
//! chế chọn được nhiều"*.
//!
//! Ba tầng, ba chỗ hỏng khác nhau — nên ba nhóm test:
//! 1. **Nguồn**: `pending_question` từng đọc đúng `questions[0]` rồi tự trấn an
//!    rằng các câu sau "vẫn nằm trên màn cho người mở phiên ra xem". Câu ấy
//!    đúng về vị trí và sai về hậu quả: bảng không gửi được khi còn ô trống.
//! 2. **Câu chữ**: một cái chuông nói "dừng lại HỎI" mà giấu mất chuyện còn hai
//!    câu nữa thì nó mời người ta bấm một cái rồi tưởng xong.
//! 3. **Số học đi tới câu N**: đi nhầm một tab là chốt một lựa chọn cho câu
//!    người ta chưa đọc — không lùi lại được, nên nó phải nằm chỗ test nhìn
//!    thấy chứ không lẫn trong hàm cần Terminal thật.

use hub::pipeline::pick_keys;
use hub::sessions::pending_question;
use hub::watch::{Change, Idle};

/// Đúng hình dạng `claude` ghi vào nhật ký: một `tool_use` mang cả bảng.
fn tail_with_two_questions() -> String {
    serde_json::json!({
        "message": { "content": [ {
            "type": "tool_use",
            "id": "toolu_01",
            "name": "AskUserQuestion",
            "input": { "questions": [
                {
                    "header": "Vá ACL",
                    "question": "Khi ô ACL nhận một chuỗi không trỏ tới ai, server nên xử sao?",
                    "multiSelect": false,
                    "options": [ {"label": "Từ chối, nói rõ token nào"}, {"label": "Vẫn lưu, báo rõ"} ]
                },
                {
                    "header": "Đăng nhập",
                    "question": "Đăng nhập có phân biệt hoa thường không?",
                    "multiSelect": true,
                    "options": [ {"label": "Không phân biệt"}, {"label": "Chặn từ form"} ]
                }
            ] }
        } ] }
    })
    .to_string()
}

#[test]
fn the_whole_table_leaves_the_transcript_not_just_the_first_question() {
    let a = pending_question(&tail_with_two_questions()).expect("bảng đang treo");
    assert_eq!(a.header, "Vá ACL");
    assert_eq!(a.rest.len(), 1, "câu 2 phải đi ra cùng, không bị bỏ lại");
    assert_eq!(a.rest[0].header, "Đăng nhập");
    assert_eq!(a.rest[0].options, vec!["Không phân biệt", "Chặn từ form"]);
    // `multiSelect` đọc theo TỪNG câu: một bảng có thể trộn câu chọn-một với
    // câu chọn-nhiều, và khai sai bản chất là mời người ta bấm hụt.
    assert!(!a.multi, "câu 1 chọn một");
    assert!(a.rest[0].multi, "câu 2 chọn nhiều");
}

#[test]
fn a_table_that_is_answered_says_nothing_is_pending() {
    // Có `tool_result` mang đúng id ⟹ bảng đã trả lời xong ⟹ không còn treo.
    let done = serde_json::json!({
        "message": { "content": [ { "type": "tool_result", "tool_use_id": "toolu_01" } ] }
    })
    .to_string();
    let tail = format!("{}\n{}", tail_with_two_questions(), done);
    assert!(pending_question(&tail).is_none());
}

#[test]
fn the_bell_says_how_many_questions_are_in_the_table() {
    let a = pending_question(&tail_with_two_questions()).unwrap();
    let c = Change::Asking {
        id: "s1".into(),
        name: "projects-bd".into(),
        header: a.header,
        question: a.question,
        options: a.options,
        multi: a.multi,
        rest: a.rest,
    };
    let said = c.say(&Idle::Unknown, None);
    assert!(
        said.contains("câu 1/2"),
        "phải nói đang ở câu mấy trên mấy: {said}"
    );
    assert!(said.contains("Bảng 2 câu"), "{said}");
    assert!(
        said.contains("trả lời HẾT rồi mới gửi được"),
        "phải nói ra cái ràng buộc, không thì bấm một cái rồi tưởng xong: {said}"
    );
    assert!(
        said.contains("Đăng nhập"),
        "câu 2 phải đọc được ngay trên chuông: {said}"
    );
    assert!(
        said.contains("Không phân biệt"),
        "kèm lựa chọn của câu 2: {said}"
    );
}

/// Bảng MỘT câu không mọc thêm chữ: câu nói cũ đã đúng, và thêm "câu 1/1" vào
/// đó chỉ là tiếng ồn trên một màn hình 390px.
#[test]
fn a_single_question_bell_stays_exactly_as_it_was() {
    let c = Change::Asking {
        id: "s1".into(),
        name: "projects-bd".into(),
        header: "Nửa ngày".into(),
        question: "Đơn vắng có khai được nửa ngày không?".into(),
        options: vec!["Có".into(), "Không".into()],
        multi: false,
        rest: Vec::new(),
    };
    let said = c.say(&Idle::Unknown, None);
    assert!(!said.contains("câu 1/"), "{said}");
    assert!(!said.contains("Bảng"), "{said}");
}

#[test]
fn walking_to_another_question_counts_the_arrows_and_never_overshoots() {
    // Đang ở câu 1, muốn trả lời câu 2 → một bước sang phải rồi bấm số.
    assert_eq!(pick_keys(0, 1, 2), vec!["right", "2"]);
    // Đang ở câu 3, muốn quay về câu 1 → hai bước sang trái. Đi bằng `left`
    // chứ không vòng qua phải: vòng qua phải chỉ đúng nếu tab có nối vòng, mà
    // đó là điều chưa ai đo trên máy này.
    assert_eq!(pick_keys(2, 0, 1), vec!["left", "left", "1"]);
    // Đang đứng ĐÚNG câu ấy → không mũi tên nào cả. Một phím mũi tên thừa ở
    // đây là một lần chốt hộ chủ máy vào câu bên cạnh.
    assert_eq!(pick_keys(1, 1, 3), vec!["3"]);
}

/// 🔴 Hà 2026-08-14, sau khi chạm hai dòng lệnh: *"bấm rồi nhưng không được"*.
///
/// Nhưng log nói ngược lại — `pick_sent keys:["1"]` rồi `pick_sent
/// keys:["right","1"]`, và phiên tfl5 sau đó `working:true asking:false`. Tức
/// hai cú bấm ĐÚNG, bảng được trả lời và gửi đi. Cái hỏng là câu chữ: bảng
/// biến mất bị gọi là *"đọc lại KHÔNG thấy bảng đâu"* — nghe như một thất bại,
/// trong khi đó là kết cục TỐT NHẤT.
///
/// Làm đúng rồi báo sai cũng là một lỗi, chỉ hỏng ở khâu cuối: người ta bấm
/// lại một việc đã xong.
#[test]
fn the_send_command_carries_its_id_inside_the_name() {
    // `/key <id> enter` KHÔNG chạm được: Telegram gửi lại mỗi `/key`, tham số
    // sau dấu cách rơi mất — đúng cái Hà gặp lúc 09:06 (*"Chưa hiểu lệnh này"*).
    let a = hub::sessions::Asking {
        header: "Vá ACL".into(),
        question: "Server nên xử sao?".into(),
        options: vec!["Từ chối".into()],
        multi: false,
        rest: vec![hub::sessions::Question {
            header: "Đăng nhập".into(),
            question: "Phân biệt hoa thường?".into(),
            options: vec!["Không".into()],
            multi: false,
        }],
    };
    let txt = hub::pipeline::ask_command_lines("4963b95c-93b0-46e3-baf9-40bbfacbef2f", &a, false);
    assert!(
        txt.contains("/send_4963b95c"),
        "phải là lệnh chạm được: {txt}"
    );
    assert!(
        !txt.contains("/key "),
        "không được dùng dạng có tham số rời: {txt}"
    );
}
