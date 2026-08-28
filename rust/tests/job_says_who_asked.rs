//! Câu nhắc "vẫn đang chạy" phải nói AI nhờ việc ấy.
//!
//! 🔴 Hà 2026-08-29, gửi kèm một ảnh Telegram kín đặc một màn hình:
//! *"Sao lắm lệnh chạy thế, mãi không dừng, mà ko biết lệnh của phiên nào gọi"*.
//!
//! Ba câu, ba chuyện khác nhau, và đo được cả ba trước khi sửa một dòng nào:
//!
//! * **"lắm lệnh"** — chỉ có **BA** việc chạy thật lúc ấy (job #3·#4·#5, pid
//!   60751·81437·93966, `ps -o ppid` chỉ về **22244** tức chính daemon huba).
//!   Nhưng `LONG_JOB_TICK_SEC = 90` đẻ một tin MỚI mỗi 90 giây ⟹ ~70 tin. Cái
//!   anh đếm được là TIN, cái anh tưởng mình đếm là VIỆC.
//! * **"mãi không dừng"** — cả ba kẹt ở cùng một chỗ: `docker info` **không có
//!   `timeout`**. Vòng `for i in $(seq 1 60); do docker info …; sleep 5; done`
//!   được viết để bỏ cuộc sau 5 phút, nhưng nó không bao giờ sang được vòng 2:
//!   `docker info` pid 60756 sống **45 phút 53**. Đó là lệnh của phiên khác
//!   soạn, không phải mã huba — nhưng phanh cuối `LONG_JOB_MAX_SEC = 3600` mới
//!   dọn chúng lúc gần một tiếng.
//! * **"ko biết phiên nào gọi"** — của huba, và là thứ tệp này khoá.
//!
//! Chỗ hỏng đúng là chỗ đã cắn hôm qua với `FreshWindow::old_kept`: sổ việc
//! `Job` có sẵn `session`, `sessions::label_sessions` có sẵn nhãn `[tfl5]`, mà
//! người báo tin chỉ đọc `started.elapsed()`. Dữ kiện nằm sẵn trong tay, không
//! ai đọc.

use huba::pipeline::job_who;

/// Có nhãn thì dùng nhãn — đó là thứ chủ máy nhận ra khi đang nhìn điện thoại.
#[test]
fn a_labelled_job_is_named_by_its_label() {
    assert_eq!(job_who("[tfl5]", "5a7f2f4a-1111-2222"), "[tfl5]");
    assert_eq!(
        job_who("[dwork/A-DSIGN]", "5a7f2f4a-1111-2222"),
        "[dwork/A-DSIGN]",
        "nhãn có làn thì giữ nguyên cả làn — hai làn dwork chạy song song là \
         chuyện thường, gộp về '[dwork]' là lại không biết phiên nào"
    );
}

/// ĐỐI CHỨNG NGƯỢC thứ nhất: KHÔNG có nhãn thì vẫn phải nói được gì đó.
///
/// Đây là vế dễ làm hỏng nhất — nhãn do `label_sessions` tính, mà một việc có
/// thể mở sổ TRƯỚC lượt gán nhãn. Rơi về rỗng ở đây là quay lại đúng cái tin
/// vô danh Hà vừa phàn nàn.
#[test]
fn an_unlabelled_job_falls_back_to_the_uuid_never_to_nothing() {
    let ai = job_who("", "5a7f2f4a-1111-2222-3333-444444444444");
    assert_eq!(ai, "[5a7f2f4a]", "đọc ra: {ai:?}");
    assert!(
        !ai.is_empty() && ai != "[]",
        "một cặp ngoặc trống nói rằng huba biết điều gì đó rồi bỏ trống: {ai:?}"
    );
}

/// ĐỐI CHỨNG NGƯỢC thứ hai: KHÔNG biết gì cả cũng phải là một câu, không phải
/// một khoảng trắng. `[]` hay "" lọt ra Telegram thì câu nhắc đọc thành
/// "⏳  vẫn đang chạy (16 phút)" — đúng hình dạng cũ, chỉ khác là nay có một
/// khoảng trắng thừa để không ai nhận ra bản vá đã hỏng.
#[test]
fn a_job_with_neither_label_nor_id_still_says_something() {
    let ai = job_who("", "");
    assert!(
        !ai.is_empty() && ai != "[]",
        "phải là một câu đọc được: {ai:?}"
    );
    assert!(
        ai.contains("không rõ"),
        "và phải nói thẳng là KHÔNG RÕ, đừng đội lốt một cái tên: {ai:?}"
    );
}

/// Hàm này chỉ có nghĩa nếu nó ĐỔI ĐƯỢC kết quả theo đầu vào (§13①). Ba đầu
/// vào khác nhau ⟹ ba câu khác nhau; một bản dựng hằng bất kỳ làm đỏ bài này.
#[test]
fn the_three_cases_are_three_different_answers() {
    let co_nhan = job_who("[tfl5]", "5a7f2f4a");
    let chi_uuid = job_who("", "5a7f2f4a");
    let khong_gi = job_who("", "");
    assert_ne!(co_nhan, chi_uuid);
    assert_ne!(chi_uuid, khong_gi);
    assert_ne!(co_nhan, khong_gi);
}
