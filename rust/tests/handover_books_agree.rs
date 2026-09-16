//! Hai cuốn sổ của `auto_handover` phải nói cùng một sự thật.
//!
//! 🔴 Hà 2026-08-24: *"Trong danh sách phiên tôi thấy có 1 phiên 64% rồi tại
//! sao chưa tự chuyển, tôi thấy vấn đề này chạy không được ổn định"*.
//!
//! Anh mô tả đúng cả triệu chứng lẫn tính chất. Nó **không** ổn định, và thứ
//! quyết định phiên nào hỏng là **thứ tự chữ cái của uuid**:
//!
//! * `auto_handover:done` là một `Vec`, cắt từ đầu ⟹ đúng là cũ-trước;
//! * `auto_handover:pct` là một `BTreeMap`, cắt bằng `keys().next()` ⟹ bỏ khoá
//!   **nhỏ nhất theo bảng chữ cái**, chẳng liên quan gì tới tuổi.
//!
//! Đo trên DB thật lúc phát hiện: `pct` mở đầu bằng `5a7f2f4a` — mọi khoá bắt
//! đầu bằng `0`–`4` đã biến mất, còn `done` vẫn giữ chúng. Phiên `1ad3e613` rơi
//! đúng khe: **có** trong `done`, **mất** trong `pct` ⟹ mốc hỏi-lại rơi về
//! `unwrap_or(at_percent) + 10` = 70%, nên nó nằm im ở 63% suốt nhiều giờ trong
//! khi log chỉ nói `AlreadyDone`.
//!
//! Hai bản vá, và bài kiểm này khoá cả hai.

use huba::db::Db;
use huba::pipeline::{
    auto_handover_why, doc_so_ban_giao, ghi_so_ban_giao, mo_duoc_khong, AutoWhy, MocBanGiao,
};
use huba::sessions::FreshWindow;

const AT: u8 = 60;

/// Gọi ĐÚNG hàm `auto_handover` gọi — không dựng lại phép quyết định ở đây.
///
/// 🔴 Bản đầu của bài kiểm này chép logic vào chính nó, nên nó xanh kể cả khi
/// mã sản xuất hỏng. Luật §13 của workspace gọi đúng tên: *một cổng không bao
/// giờ đỏ được là một cổng không có*. Bắt được lúc tự soi lại, 2026-08-25.
fn hoi_lai(sid: &str, pct: u8, done: &[&str], done_at: &[(&str, u8)], mo_duoc: bool) -> bool {
    let done: Vec<String> = done.iter().map(|s| s.to_string()).collect();
    let at: std::collections::BTreeMap<String, MocBanGiao> = done_at
        .iter()
        .map(|(k, v)| (k.to_string(), MocBanGiao { pct: *v, mo_duoc }))
        .collect();
    huba::pipeline::already_handed_over(sid, pct, &done, &at)
}

/// Lượt bàn giao **MỞ ĐƯỢC** — luật cũ, mốc hỏi-lại 10 điểm.
fn already_done(sid: &str, pct: u8, done: &[&str], done_at: &[(&str, u8)]) -> bool {
    hoi_lai(sid, pct, done, done_at, true)
}

/// Lượt bàn giao **MỞ HỤT** — không đo được id phiên mới. Mốc hỏi-lại ngắn.
fn already_done_hut(sid: &str, pct: u8, done: &[&str], done_at: &[(&str, u8)]) -> bool {
    hoi_lai(sid, pct, done, done_at, false)
}

/// 🔴 Ca của Hà: có trong `done`, MẤT trong `pct` ⟹ phải hỏi lại, không khoá.
#[test]
fn a_session_whose_recorded_pct_was_evicted_is_reconsidered() {
    let done = ["1ad3e613"];
    let done_at: [(&str, u8); 0] = []; // đã bị cắt khỏi sổ pct
    assert!(
        !already_done("1ad3e613", 63, &done, &done_at),
        "quên mốc cũ mà vẫn khoá ⟹ phiên đứng im tới 70%"
    );
    // …và khi ấy nó phải đi tiếp tới các cửa THẬT, không dừng ở AlreadyDone.
    let why = auto_handover_why(63, AT, false, true, false, false, 0, 300, 120);
    assert_eq!(why, AutoWhy::Do, "{why:?}");
}

/// Còn nhớ mốc thì luật cũ giữ nguyên — bàn giao ở 61% thì im tới 71%.
#[test]
fn a_remembered_pct_still_holds_until_it_climbs_a_step() {
    let done = ["1ad3e613"];
    let at = [("1ad3e613", 61u8)];
    assert!(
        already_done("1ad3e613", 63, &done, &at),
        "63 < 71 thì phải giữ"
    );
    assert!(
        already_done("1ad3e613", 70, &done, &at),
        "70 < 71 thì vẫn giữ"
    );
    assert!(
        !already_done("1ad3e613", 71, &done, &at),
        "leo đủ một mốc thì phải hỏi lại"
    );
}

/// Chưa từng bàn giao thì `pct` không liên quan.
#[test]
fn a_session_never_handed_over_is_never_already_done() {
    let at = [("1ad3e613", 61u8)];
    assert!(!already_done("aaaaaaaa", 99, &[], &at));
    assert!(!already_done("aaaaaaaa", 99, &["1ad3e613"], &at));
}

/// 🔴 Sổ `pct` phải cắt THEO `done`, không theo thứ tự chữ cái.
///
/// Dựng lại đúng phép cắt mới và chứng minh nó không còn phụ thuộc uuid: một
/// khoá bắt đầu bằng `0` vẫn sống chừng nào `done` còn giữ nó, và biến mất đúng
/// lúc `done` bỏ nó — chứ không phải vì nó xếp trước theo bảng chữ cái.
#[test]
fn the_pct_book_follows_the_done_list_not_the_alphabet() {
    let mut at: std::collections::BTreeMap<String, u8> = Default::default();
    for k in ["0aaa", "1ad3e613", "5a7f2f4a", "zzzz"] {
        at.insert(k.to_string(), 61);
    }
    // `done` giữ ba cái, bỏ `5a7f2f4a` — thứ mà phép cắt cũ sẽ GIỮ LẠI.
    let done: Vec<String> = ["0aaa", "1ad3e613", "zzzz"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    at.retain(|k, _| done.contains(k));

    assert!(at.contains_key("0aaa"), "khoá 'nhỏ' bị cắt oan lần nữa");
    assert!(at.contains_key("1ad3e613"), "đúng ca của Hà, lại mất");
    assert!(!at.contains_key("5a7f2f4a"), "đã rời done mà vẫn nằm lại");
    assert_eq!(at.len(), done.len(), "hai sổ phải cùng một tập");
}

/// 🔴 CA THẬT, cấy nguyên si từ nhật ký: lượt `1aac8d22` ngày 2026-09-15.
///
/// `auto_handover_firing` lúc `18:45:33.601Z` ở **80 %**, rồi 83 giây không một
/// dòng nào của phiên ấy — `keys::open_window` ném `Err` sau khi cửa sổ đã dựng
/// xong (`osascript` hết hạn 20 s). Không có id phiên mới ⟹ **mở HỤT**. Bản cũ
/// vẫn đóng dấu "xong" ở bước MỞ, nên `AlreadyDone` khoá tới 90 %; thực tế nó
/// chỉ thoát ra ở **93 %** lúc `00:03:43Z` — **5 giờ 19 phút** với hai cửa sổ
/// cùng sống.
///
/// Đây là ĐỐI CHỨNG NGƯỢC của bản vá: bỏ trường `mo_duoc` đi (hay cho nó luôn
/// `true`) thì `81` và `82` dưới đây đổi màu.
#[test]
fn the_handover_that_opened_no_window_is_asked_again_two_points_later() {
    let done = ["1aac8d22"];
    let at = [("1aac8d22", 80u8)];

    // Vẫn vào sổ: 30 giây sau, ngữ cảnh chưa nhúc nhích ⟹ KHÔNG bắn lại.
    assert!(
        already_done_hut("1aac8d22", 80, &done, &at),
        "lượt hụt không vào sổ ⟹ mỗi vòng ~30s một fork_call (đo được: ước tính 8,30 USD)"
    );
    assert!(
        already_done_hut("1aac8d22", 81, &done, &at),
        "81 < 82 thì vẫn giữ — cửa hỏi-lại HẸP hơn, không phải biến mất"
    );
    // …nhưng không phải 5 giờ nữa.
    assert!(
        !already_done_hut("1aac8d22", 82, &done, &at),
        "leo 2 điểm mà vẫn khoá ⟹ đúng cái bẫy 80%→93% đã đo"
    );

    // ĐỐI CHỨNG: cùng con số, chỉ đổi *mở được hay không*. Nếu hai vế này ra
    // cùng kết quả thì trường `mo_duoc` không điều khiển gì cả, và bài kiểm
    // trên chỉ đang khoá một hằng số.
    assert!(
        already_done("1aac8d22", 82, &done, &at),
        "lượt MỞ ĐƯỢC phải vẫn giữ tới 90 — luật cũ không được nới theo"
    );
    assert!(
        already_done("1aac8d22", 89, &done, &at),
        "89 < 90 thì lượt mở được vẫn giữ"
    );
    assert!(
        !already_done("1aac8d22", 90, &done, &at),
        "lượt mở được leo đủ 10 điểm thì hỏi lại, y như trước bản vá"
    );
}

/// 🔴 ĐƯỜNG TRÒN: ghi bằng người ghi THẬT, đọc bằng người đọc THẬT.
///
/// Bài này ra đời vì tầng đối chứng ngược 2026-09-16 bắt được một lỗ trong chính
/// tầng kiểm vừa viết: cấy mutant *"sổ `done` thôi nhận phiên mới"*
/// (`next.push(sid)` → bỏ đi) mà **cả 7 bài kiểm đều xanh**. Lý do đúng bằng một
/// câu: cả 7 đều **nắn sổ bằng tay** rồi đưa cho `already_handed_over`, nên
/// chúng khoá được người ĐỌC và không hề chạm tới người GHI. Một cuốn sổ có hai
/// đầu; kiểm một đầu là kiểm một nửa.
///
/// ⚠ Và ghi rõ cái bài này KHÔNG chứng minh: nó không chạy `auto_handover`, nên
/// *"lượt ghi nằm SAU lượt mở cửa sổ"* không do nó canh. Thứ canh chỗ ấy là kiểu
/// dữ liệu: tham số `mo_duoc` được tính bằng `mo_duoc_khong(&moved)`, mà `moved`
/// chưa tồn tại ở bước MỞ — dời lời gọi lên trên là **không dịch nổi**.
#[test]
fn the_book_the_writer_leaves_is_the_book_the_reader_believes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&tmp.path().join("t.sqlite")).expect("open");

    // Đúng ca `1aac8d22`: bắn ở 80 %, cửa sổ KHÔNG khai được id.
    ghi_so_ban_giao(&db, "1aac8d22", 80, false, &[]);
    let (done, at) = doc_so_ban_giao(&db);
    assert_eq!(
        done,
        vec!["1aac8d22".to_string()],
        "người ghi bỏ quên phiên"
    );
    assert_eq!(
        at.get("1aac8d22"),
        Some(&MocBanGiao {
            pct: 80,
            mo_duoc: false
        })
    );
    assert!(
        huba::pipeline::already_handed_over("1aac8d22", 81, &done, &at),
        "vừa ghi xong mà vòng sau đã bắn lại ⟹ một fork_call mỗi ~30 giây"
    );
    assert!(
        !huba::pipeline::already_handed_over("1aac8d22", 82, &done, &at),
        "leo 2 điểm mà vẫn khoá ⟹ đúng cái bẫy 5 giờ 19 phút"
    );

    // Lượt sau MỞ ĐƯỢC: cùng cuốn sổ, mốc dài trở lại.
    ghi_so_ban_giao(&db, "d1cdcd48", 80, true, &done);
    let (done, at) = doc_so_ban_giao(&db);
    assert_eq!(done.len(), 2, "sổ phải NỐI, không phải ghi đè");
    assert!(
        huba::pipeline::already_handed_over("d1cdcd48", 89, &done, &at),
        "lượt mở được phải giữ đủ 10 điểm"
    );
    // …và phiên cũ vẫn giữ nguyên mốc NGẮN của nó — hai lượt, hai luật, một sổ.
    assert!(
        !huba::pipeline::already_handed_over("1aac8d22", 82, &done, &at),
        "lượt hụt bị lượt sau ghi đè mất cờ"
    );
}

/// 🔴 PHÉP ĐỌC sinh ra `mo_duoc` — ba kết cục, chỉ một là "mở được".
///
/// Bài kiểm này tồn tại vì bài kiểm phía trên KHÔNG với tới được nó: chúng khoá
/// phép TÍNH của `already_handed_over`, còn đây là phép ĐỌC sinh ra đầu vào. Cấy
/// một lỗi vào đúng chỗ ấy (`Ok(_)` thay cho `Ok(w) if w.new_id.is_some()`) thì
/// mọi bài trên vẫn xanh trơn — và đó chính là hình dạng cả bản vá này đi chữa.
#[test]
fn only_a_session_id_actually_seen_counts_as_opened() {
    let cua_so = |new_id: Option<&str>| FreshWindow {
        tty: "ttys000".to_string(),
        new_id: new_id.map(str::to_string),
        closed_err: None,
        old_kept: false,
        asking: vec![],
    };

    assert!(
        mo_duoc_khong(&Ok(cua_so(Some("d1cdcd48")))),
        "thấy id phiên mới mà vẫn tính là hụt"
    );
    assert!(
        !mo_duoc_khong(&Ok(cua_so(None))),
        "cửa sổ mở nhưng CHƯA khai id — đây là Stalled, không phải xong"
    );
    // Đúng ca `1aac8d22` 2026-09-15: `open_window` ném Err sau khi cửa sổ đã
    // dựng xong. Cửa sổ có thật, nhưng huba KHÔNG cầm được id nào.
    assert!(
        !mo_duoc_khong(&Err(anyhow::anyhow!("osascript quá 20s"))),
        "Err mà tính là mở được ⟹ khoá 10 điểm cho một lượt bỏ lại cửa sổ mồ côi"
    );
}

/// Hai mốc phải KHÁC nhau, và khác đúng chiều: hụt thì hỏi lại SỚM hơn.
///
/// Viết riêng vì bài trên có thể xanh cả khi hai mốc bằng nhau ở một vài điểm
/// rời rạc. Ở đây quét cả dải và đòi một bất biến: với mọi `pct ≥ mốc`, lượt hụt
/// không bao giờ bị giữ lâu hơn lượt mở được.
#[test]
fn a_stalled_handover_is_never_held_longer_than_a_successful_one() {
    let done = ["aaaa"];
    for moc in [0u8, 30, 61, 80, 99] {
        let at = [("aaaa", moc)];
        let mut hut_nha_som_hon = false;
        for pct in moc..=100 {
            let hut = already_done_hut("aaaa", pct, &done, &at);
            let mo = already_done("aaaa", pct, &done, &at);
            assert!(
                !(hut && !mo),
                "mốc {moc}, pct {pct}: lượt HỤT bị giữ trong khi lượt MỞ ĐƯỢC đã được thả"
            );
            if !hut && mo {
                hut_nha_som_hon = true;
            }
        }
        // `moc` sát trần thì `saturating_add` dí cả hai về 100 và không còn khe
        // nào để khác nhau — đó là hành vi đúng, không phải một ca hỏng.
        if moc <= 90 {
            assert!(
                hut_nha_som_hon,
                "mốc {moc}: hai mốc hỏi-lại không khác nhau ở bất kỳ điểm nào ⟹ trường `mo_duoc` vô dụng"
            );
        }
    }
}
