//! Cổng trình duyệt, chạy THẬT — qua đúng đường Rust gọi, không phải chỉ gọi
//! tay `node web.mjs`.
//!
//! `#[ignore]` vì nó dựng một trình duyệt thật và đi ra Internet. Chạy tay:
//!
//! ```text
//! cargo test --offline --test web_live -- --ignored --nocapture
//! ```
//!
//! 🔴 Vì sao phải có, dù `tests/browser.rs` đã xanh: những bài kia đều THUẦN
//! (chuỗi vào, chuỗi ra). Chúng không chạm được vào ba chỗ đã trả giá thật
//! trong ngày 23/08 — `node` tìm bằng đường tuyệt đối (launchd PATH tối thiểu),
//! trạng thái trang phải SỐNG giữa hai tiến trình, và cái binary nào ra được
//! mạng. Đúng luật của repo: chỉ tính là "chạy được" khi đã chạy trên env thật.

use std::path::PathBuf;

fn hub_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rust/ phải nằm trong cây huba")
        .to_path_buf()
}

#[test]
#[ignore = "dựng trình duyệt thật và đi ra Internet — chạy tay bằng --ignored"]
fn the_browser_opens_reads_and_survives_between_commands() {
    let home = hub_home();

    // Bắt đầu từ trạng thái sạch: lượt trước còn sống thì bài kiểm này đo nhờ
    // công của nó, chứ không đo công của chính nó.
    let (ack, _) = huba::web::route(&home, "tắt");
    println!("tắt trước: {ack}");

    // ① Mở — và lượt này phải TỰ DỰNG trình duyệt, vì vừa tắt xong.
    let (ack, anh) = huba::web::route(&home, "example.com");
    println!("mở: {ack}");
    assert!(ack.contains("Example Domain"), "không mở được: {ack}");
    // Ảnh là thứ người cầm điện thoại cần trước tiên.
    let anh = anh.expect("phải có ảnh trang");
    let co = std::fs::metadata(&anh).map(|m| m.len()).unwrap_or(0);
    assert!(co > 1000, "ảnh {} chỉ {co} byte", anh.display());

    // ② Đọc — một TIẾN TRÌNH node KHÁC. Trang phải còn nguyên; đây chính là vế
    //    mà bản đầu của `web.mjs` làm hỏng (tự dựng thêm một trình duyệt nữa
    //    lên cùng hồ sơ ⟹ cả hai treo, cổng vẫn LISTEN mà không ai trả lời).
    let (ack, _) = huba::web::route(&home, "doc");
    println!(
        "đọc: {}",
        ack.lines().take(3).collect::<Vec<_>>().join(" / ")
    );
    assert!(
        ack.contains("This domain is for use in documentation"),
        "mất trang giữa hai lệnh: {ack}"
    );

    // ③ Bấm theo CHỮ NGƯỜI ĐỌC THẤY — không selector, vì người ra lệnh đang
    //    nhìn một tấm ảnh chứ không nhìn cây DOM.
    let (ack, _) = huba::web::route(&home, "bấm Learn more");
    println!("bấm: {ack}");
    assert!(ack.contains("iana.org"), "cú bấm không dẫn đi đâu: {ack}");

    // ④ Địa chỉ bậy vẫn phải chết ở cổng, kể cả khi trình duyệt đang sống.
    let (ack, _) = huba::web::route(&home, "file:///etc/passwd");
    assert!(!ack.contains("root:"), "ĐỌC TRỘM ĐƯỢC Ổ ĐĨA: {ack}");
    println!("cổng địa chỉ: {ack}");

    // ⑤ 🔴 TẮT RỒI MỞ LẠI NGUỘI — Hà 2026-08-23: *"Làm sao giữ được cache, vì
    //    tôi thấy mỗi lần mở lại mất hết trạng thái cũ"*. Đo ra hai vế khác
    //    nhau: cookie/localStorage KHÔNG mất (công của `--user-data-dir`, đo
    //    riêng bằng tay), còn thứ mất là ĐANG ĐỨNG Ở TRANG NÀO — Chrome khởi
    //    động về `about:blank`. Đây là chỗ ghim vế thứ hai.
    let (ack, _) = huba::web::route(&home, "tắt");
    println!("tắt: {ack}");
    let (ack, _) = huba::web::route(&home, "");
    println!("mở lại nguội: {ack}");
    assert!(
        ack.contains("iana.org"),
        "mở lại không quay về trang cũ: {ack}"
    );

    // ⑥ GÕ CHỮ + ENTER — vế mở ra việc đăng nhập, thứ trước 23/08 không có.
    //    Đo trên một ô tìm kiếm thật chứ không trên trang tự dựng: ô ấy nghe
    //    từng phím (gợi ý tức thời) và điều hướng kiểu SPA — đúng hai chỗ một
    //    bộ gõ giả sẽ đi qua êm mà bộ thật thì vấp.
    let (ack, _) = huba::web::route(&home, "duckduckgo.com");
    println!("mở ô tìm: {ack}");
    let (ack, _) = huba::web::route(&home, "gõ huba telegram bridge");
    println!("gõ: {ack}");
    let (ack, _) = huba::web::route(&home, "enter");
    println!("enter: {ack}");
    assert!(
        ack.contains("huba telegram bridge"),
        "cú Enter không dẫn tới trang kết quả — hoặc lệnh trả về TRƯỚC khi trang \
         chuyển xong (bẫy SPA, xem `press` trong web.mjs): {ack}"
    );

    let (ack, _) = huba::web::route(&home, "tắt");
    println!("dọn: {ack}");
}
