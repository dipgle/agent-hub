//! Phép DÒ chạy thật cho bảng hỏi nhiều câu (`AskUserQuestion`).
//!
//! Gắn `#[ignore]` vì nó đọc màn của một cửa sổ Terminal THẬT đang mở. Gọi tay:
//!
//! ```text
//! HUB_LIVE_TTY=ttys007 cargo test --offline --test ask_table_live -- --ignored --nocapture
//! ```
//!
//! Vì sao phải dò thay vì tra tài liệu: thanh tab của bảng hỏi vẽ trạng thái
//! từng câu bằng một ký tự ô (`☐` chưa trả lời / `⊠` đã trả lời trong ảnh Hà
//! gửi 2026-08-13), mà ảnh chụp điện thoại KHÔNG phân biệt được `⊠` với `☒`
//! hay `☑`. Đếm nhầm họ ký tự là dựng một phép đo luôn trả 0 — đúng cái bẫy
//! `OPERATING-CHARTER.md` §2d gọi là phép đo mù. Nên hàm đếm phải viết theo ký
//! tự ĐO ĐƯỢC trên máy này, và đây là chỗ đo.
//!
//! ⚠ Bài kiểm ĐẦU chỉ đọc. Bài kiểm CUỐI (`pressing_enter…`) GỬI PHÍM THẬT, và
//! nó đòi thêm `HUB_LIVE_PRESS=1` — xem chú thích tại chỗ để biết vì sao cái cờ
//! ấy phải có (2026-08-19: chạy cả tệp bằng `--ignored` đã trả lời hộ một câu
//! trong bảng hỏi thật của phiên amm).

#[test]
#[ignore = "đọc màn một cửa sổ Terminal thật — chạy tay bằng --ignored"]
fn what_characters_does_the_question_table_actually_draw() {
    let tty = std::env::var("HUB_LIVE_TTY").unwrap_or_else(|_| "ttys007".to_string());
    let look = huba::keys::look(&tty, 40);
    let body = match look {
        huba::keys::Look::Saw { body, choices } => {
            println!("hộp chọn đọc được: {} mục", choices.len());
            for (n, label) in &choices {
                println!("  {n}. {label}");
            }
            body
        }
        // 🪦 Nhánh `Withheld` gỡ 2026-08-16 — huba thôi giấu chữ với chính nó.
        huba::keys::Look::Blind { why } => panic!("không đọc được màn: {why}"),
    };
    // Phép đo phải trỏ đúng chỗ: hàm đọc bảng chạy trên MÀN THẬT, không chỉ
    // trên hằng số chép tay trong test đơn vị.
    match huba::keys::ask_table(&body) {
        Some(t) => {
            println!(
                "bảng đọc từ màn thật: {} câu · còn trống {} · nhãn {:?}",
                t.answered.len(),
                t.left(),
                t.headers
            );
            assert!(!t.answered.is_empty(), "thanh tab có ô thì phải đếm ra ô");
        }
        None => println!("(màn này không có thanh tab — phiên đang hỏi bảng MỘT câu)"),
    }
    // Chỉ in NHỮNG DÒNG mang ký tự lạ, và cắt ngắn: đây là màn làm việc thật
    // của chủ máy, không phải bãi đổ nhật ký (điều 5).
    println!("--- các dòng có ký tự ngoài ASCII ---");
    for line in body.lines() {
        if !line.is_ascii() {
            let shown: String = line.chars().take(90).collect();
            println!("{shown}");
            let marks: Vec<String> = line
                .chars()
                .filter(|c| !c.is_ascii())
                .map(|c| format!("{c} U+{:04X}", c as u32))
                .collect();
            println!("      ↳ {}", marks.join(" · "));
        }
    }
}

/// Phép đo: cửa sổ ĐANG CÓ đọc ra mấy tab, và huba có KHAI khi bản đọc cụt không?
///
/// 🔴 Bài kiểm này đổi hình dạng 2026-08-30. Bản cũ nới cửa sổ hết cỡ rồi so hai
/// bản đọc (`screen_text_tall` + `wider_table`) — nó đo đúng thứ huba làm hồi ấy.
/// Hà: *"đừng thay đổi kích thước của cửa sổ terminal nữa, bỏ hết các chỗ đi"*.
/// Nên phép đo cũng phải đổi theo, không thì nó đo một hành vi không còn ai chạy.
///
/// Cái còn phải đo: bề ngang cửa sổ vẫn quyết định huba đọc ra mấy tab — chỉ là
/// nay bề ngang ấy do CHỦ MÁY đặt, không phải huba. Nên câu hỏi thành: *"đọc ra
/// bao nhiêu, và khi con số ấy cụt thì huba có nói không"*.
///
/// Chỉ ĐỌC. Một lượt đọc, không phím nào, không đụng vào cỡ cửa sổ.
///
/// ```text
/// HUB_LIVE_TTY=ttysNNN cargo test --offline --test ask_table_live -- --ignored narrow --nocapture
/// ```
#[test]
#[ignore = "đọc màn một cửa sổ Terminal thật — chạy tay bằng --ignored"]
fn a_narrow_window_cuts_the_tab_bar() {
    let tty = std::env::var("HUB_LIVE_TTY").expect("cần HUB_LIVE_TTY");
    let w = huba::keys::window_of(&tty)
        .expect("hỏi được Terminal")
        .expect("tty phải gắn một cửa sổ");

    let man = huba::keys::screen_text(w).expect("đọc được màn");
    let dem = |t: &Option<huba::keys::AskTable>| t.as_ref().map(|t| t.answered.len()).unwrap_or(0);

    // `want = 0` ⟹ không đòi hỏi gì ⟹ đây là con số THẬT cửa sổ này cho đọc.
    let (bang, im) = huba::keys::ask_table_seen(&man, 0);
    println!(
        "màn hiện tại: {} ký tự · {} tab · nhãn {:?}",
        man.chars().count(),
        dem(&bang),
        bang.as_ref().map(|t| t.headers.clone())
    );
    assert!(
        im.is_none(),
        "không đòi hỏi gì mà vẫn kêu ⟹ lời cảnh báo là lời luôn bật, tức vô nghĩa"
    );
    // In nguyên văn dòng mang mũi tên — đây mới là bằng chứng, chứ con số đếm ra
    // thì đã đi qua chính cái hàm đang cần kiểm.
    for line in man.lines().filter(|l| l.contains('←') || l.contains('→')) {
        println!("thanh tab ⟶ {line}");
    }

    // ĐỐI CHỨNG NGƯỢC, chạy được mà không cần biết trên màn có gì: đòi nhiều hơn
    // số đọc được đúng một tab ⟹ BẮT BUỘC phải có lời. Không có lời ở đây nghĩa
    // là huba sẽ đếm hụt trong im lặng ngay trên cửa sổ thật này.
    let doi_them = dem(&bang) + 1;
    let (_, phai_keu) = huba::keys::ask_table_seen(&man, doi_them.max(2));
    assert!(
        phai_keu.is_some(),
        "đòi {doi_them} tab mà chỉ đọc ra {} thì phải KÊU",
        dem(&bang)
    );
    println!(
        "=> đối chứng ngược: đòi {doi_them} tab ⟹ {}",
        phai_keu.unwrap()
    );
    if dem(&bang) == 0 {
        println!("(cửa sổ này không có bảng hỏi nhiều câu nào đang mở — phần đếm chưa nói gì)");
    }
}

/// Phép đo: một cú Enter RỜI có submit được ô nhập đang có chữ không?
///
/// 🔴 Hà 2026-08-14: *"Vậy là tôi bấm enter không có tác dụng rồi"* — huba báo
/// *"✓ đã bấm 'enter'"* mà chữ vẫn nằm nguyên trong ô. `do script` luôn kèm một
/// dấu xuống dòng và TUI đọc cả lượt ghi như một cú DÁN, nên "gửi mỗi newline"
/// có thể chỉ là dán một dòng trống vào nội dung.
///
/// 🔴 HAI LẦN VÁ, 2026-08-19 — bài kiểm này vừa bấm nhầm vào việc thật.
///
/// Tệp mở đầu bằng dòng *"Nó KHÔNG bấm gì cả: chỉ đọc"*, và câu ấy đúng cho bài
/// kiểm ở trên. Bài kiểm NÀY thì gửi Enter thật. Nên chạy cả tệp bằng
/// `--ignored` — thao tác hiển nhiên nhất — là bắn một phím vào cửa sổ đang mở.
/// Đúng chuyện đã xảy ra: đang dò bảng hỏi của phiên `[AI/tcc/amm]` mà Hà để
/// sẵn, cú Enter ấy **trả lời hộ câu 1** (`☐ RPC pool` → `☒`, lựa chọn 1).
///
/// Hai chỗ hỏng, cả hai đều là hình dạng quen của repo này:
/// 1. **Một tệp, hai lời hứa.** Dòng đầu tệp nói "chỉ đọc" nên người đọc tin
///    nó; cái bấm nấp ở cuối. Nay phải khai thêm `HUB_LIVE_PRESS=1` mới bấm —
///    cờ RIÊNG, không dùng chung với `HUB_LIVE_TTY`, để "chạy cả tệp" không bao
///    giờ còn là một cú bấm.
/// 2. **Phép đo mù.** Nó chấm kết quả bằng `input_box_text` — Ô NHẬP — trong
///    khi màn đang mở HỘP CHỌN, chỗ ô nhập không bao giờ đổi. Nên nó in
///    *"Enter KHÔNG tác dụng"* trong khi thanh tab vừa lật `☐`→`☒`: một câu
///    sai, tự tin, về đúng chuyện nguy hiểm nhất. Nay đo cả thanh tab
///    (`ask_table`) — thứ thật sự đổi — và in ra cả hai.
///
/// Chạy tay, và nó GỬI THẬT:
/// `HUB_LIVE_TTY=ttysNNN HUB_LIVE_PRESS=1 cargo test --test ask_table_live -- --ignored press`
#[test]
#[ignore = "gửi Enter thật vào một cửa sổ thật"]
fn pressing_enter_actually_submits_the_input_box() {
    let tty = std::env::var("HUB_LIVE_TTY").expect("cần HUB_LIVE_TTY");
    if std::env::var("HUB_LIVE_PRESS").ok().as_deref() != Some("1") {
        println!("BỎ QUA — bài kiểm này GỬI PHÍM THẬT. Đặt HUB_LIVE_PRESS=1 nếu đúng là muốn bấm.");
        return;
    }
    let w = huba::keys::window_of(&tty)
        .expect("hỏi được Terminal")
        .expect("tty phải gắn một cửa sổ");
    let read = |tty: &str| {
        let body = huba::keys::screen_of(tty, 40).expect("đọc được màn").0;
        let box_text = huba::keys::input_box_text(&body);
        let table = huba::keys::ask_table(&body);
        (box_text, table)
    };
    let (box_before, tab_before) = read(&tty);
    println!("ô nhập TRƯỚC: {box_before:?}");
    println!("thanh tab TRƯỚC: {tab_before:?}");
    huba::keys::press(w, "enter").expect("gửi được phím");
    std::thread::sleep(std::time::Duration::from_millis(2500));
    let (box_after, tab_after) = read(&tty);
    println!("ô nhập SAU : {box_after:?}");
    println!("thanh tab SAU : {tab_after:?}");
    println!(
        "=> ô nhập {} · thanh tab {}",
        if box_before != box_after {
            "CÓ đổi"
        } else {
            "không đổi"
        },
        if tab_before != tab_after {
            "CÓ đổi (Enter đã CHỐT một câu)"
        } else {
            "không đổi"
        }
    );
}
