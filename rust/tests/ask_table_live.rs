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

/// Phép đo: cửa sổ HẸP có cắt mất tab của thanh tab không, và nới thì lấy lại
/// được bao nhiêu?
///
/// 🔴 Vì sao phép đo này phải có: `/shot` nới cửa sổ khi màn bị mép cắt rồi
/// dựng nút tab từ **bản rộng** (`pipeline.rs`, nhật ký `shot_grew_window`),
/// còn `/tab` với `/pick` đọc bằng `keys::look` → `keys::screen_text`, **không
/// nới**. Hai bề ngang, hai con số, cho cùng một cái bảng — nên huba có thể trả
/// lời *"bảng chỉ có 2 câu, không có câu 3"* về đúng cái nút chính nó vừa dựng.
///
/// `keys::ask_table_wide` vá chỗ ấy, và bài kiểm đơn vị của nó chỉ chấm được
/// phần QUYẾT ĐỊNH (chuỗi vào, chuỗi ra). Phần còn lại — *"cửa sổ hẹp thật sự
/// cắt cái gì"* — chỉ đo được trên một cửa sổ thật đang mở một bảng thật. Đây
/// là chỗ đo, và nó chỉ ĐỌC: hai lượt đọc, không một phím nào.
///
/// ⚠ Nó CÓ đổi kích thước cửa sổ (rồi trả lại) — đúng thứ `screen_text_tall`
/// làm mỗi lần `/shot` gặp màn bị cắt.
///
/// ```text
/// HUB_LIVE_TTY=ttysNNN cargo test --offline --test ask_table_live -- --ignored narrow --nocapture
/// ```
#[test]
#[ignore = "đọc màn một cửa sổ Terminal thật, hai lần, có đổi kích thước — chạy tay bằng --ignored"]
fn a_narrow_window_cuts_the_tab_bar() {
    let tty = std::env::var("HUB_LIVE_TTY").expect("cần HUB_LIVE_TTY");
    let w = huba::keys::window_of(&tty)
        .expect("hỏi được Terminal")
        .expect("tty phải gắn một cửa sổ");

    let hep = huba::keys::screen_text(w).expect("đọc được màn");
    let t_hep = huba::keys::ask_table(&hep);
    let rong = huba::keys::screen_text_tall(w, huba::keys::GROW_ASK).expect("nới được cửa sổ");
    let t_rong = huba::keys::ask_table(&rong);

    let dem = |t: &Option<huba::keys::AskTable>| t.as_ref().map(|t| t.answered.len()).unwrap_or(0);
    println!(
        "HẸP : {} ký tự · {} tab · nhãn {:?}",
        hep.chars().count(),
        dem(&t_hep),
        t_hep.as_ref().map(|t| t.headers.clone())
    );
    println!(
        "RỘNG: {} ký tự · {} tab · nhãn {:?}",
        rong.chars().count(),
        dem(&t_rong),
        t_rong.as_ref().map(|t| t.headers.clone())
    );
    // In nguyên văn dòng mang mũi tên ở CẢ HAI bản — đây mới là bằng chứng, chứ
    // con số đếm ra thì đã đi qua chính cái hàm đang cần kiểm.
    for (ten, doc) in [("HẸP", &hep), ("RỘNG", &rong)] {
        for line in doc.lines().filter(|l| l.contains('←') || l.contains('→')) {
            println!("{ten} ⟶ {line}");
        }
    }

    let (lay, doi) = huba::keys::wider_table(t_hep.clone(), t_rong.clone());
    println!(
        "=> nới {} · chốt lại {} tab",
        if doi {
            "LẤY LẠI ĐƯỢC tab"
        } else {
            "không thêm gì"
        },
        dem(&lay)
    );
    if dem(&t_hep) == 0 && dem(&t_rong) == 0 {
        println!("(cửa sổ này không có bảng hỏi nhiều câu nào đang mở — phép đo chưa nói gì)");
    }
    // Không assert số tab: cửa sổ thật có thể đang không mở bảng nào. Thứ DUY
    // NHẤT bài kiểm này khoá được mà không cần biết trên màn có gì: nới rồi thì
    // không được đọc ra ÍT chữ hơn — nếu có, cửa nới đang làm hỏng bản đọc.
    assert!(
        rong.chars().count() >= hep.chars().count(),
        "nới cửa sổ mà đọc ra ÍT chữ hơn ({} < {}) — cửa nới đang phá bản đọc",
        rong.chars().count(),
        hep.chars().count()
    );
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
