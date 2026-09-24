//! Kết quả `/runin` phải vào ĐÚNG phiên đã nhờ — hàng rào ở cửa gõ.
//!
//! 🔴 2026-09-23, main dwork 44 báo: kết quả đẩy `lan/dorg` lúc 17:40Z vào hàng
//! chờ của phiên **dci**. Đo trên nhật ký của chính hai phiên: `2151a7af…jsonl`
//! dòng 1568 `queue-operation enqueue 17:40:17.400` mang khối `… lan/dorg`;
//! `eed44b1d…jsonl` (dorg) không có khối ấy — trong khi huba KHAI *"dán kết quả
//! vào [dwork/dorg]"*. Gốc CHƯA tìm ra (năm mắt xích đo lại đều đúng); thứ dựng
//! được là hỏi thẳng Terminal ngay trước khi gõ: tab đang chọn của cửa sổ ấy có
//! mang tty của phiên đích không (`pipeline::paste_target_ok`).

use huba::pipeline::same_tty;

#[test]
fn cung_tty_bo_tien_to_dev_va_khong_khop_chuoi_rong() {
    assert!(same_tty("/dev/ttys000", "ttys000"));
    assert!(same_tty("ttys001", "/dev/ttys001"));
    assert!(
        !same_tty("/dev/ttys000", "ttys001"),
        "hai phiên dorg/dci của ca 17:40Z"
    );
    assert!(
        !same_tty("", ""),
        "chuỗi rỗng không phải một tty — `\"\" == \"\"` là cái bẫy"
    );
    assert!(!same_tty("/dev/", "ttys000"));
}

fn nguon() -> String {
    let p = std::env::var("HUBA_PIPELINE_SRC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs").to_string());
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Dòng này có phải một chỗ GỌI `type_and_send` để dán kết quả `/runin` không:
/// `block` ở `dan_vao_phien` (cửa chung của `watch_long_job` và bên trả của sổ
/// việc, từ 24/09), `&block` là hình dạng cũ trước khi gom cửa, `&p.b` ở
/// `runin_pending_tick`.
fn la_cho_dan(l: &str) -> bool {
    l.contains("type_and_send(w, block)")
        || l.contains("type_and_send(w, &block)")
        || l.contains("type_and_send(w, &p.b)")
}

/// Các dòng GỌI `type_and_send` để dán kết quả `/runin` — và dòng ấy có mang
/// hàng rào `paste_target_ok(` trong cùng điều kiện nhánh không.
fn cho_dan(src: &str) -> Vec<(bool, String)> {
    src.lines()
        .filter(|l| la_cho_dan(l))
        .map(|l| (l.contains("paste_target_ok("), l.trim().to_string()))
        .collect()
}

/// `type_and_send(w, &p.b)` nằm ở dòng DƯỚI dòng điều kiện sau `cargo fmt` —
/// nên với đường gõ lại thì soi dòng ngay trên.
fn co_hang_rao(src: &str) -> (usize, usize) {
    let dong: Vec<&str> = src.lines().collect();
    let mut tong = 0;
    let mut rao = 0;
    for (i, l) in dong.iter().enumerate() {
        if la_cho_dan(l) {
            tong += 1;
            let tren = if i > 0 { dong[i - 1] } else { "" };
            if l.contains("paste_target_ok(") || tren.contains("paste_target_ok(") {
                rao += 1;
            }
        }
    }
    (tong, rao)
}

#[test]
fn moi_cho_dan_ket_qua_runin_deu_qua_hang_rao() {
    let src = nguon();
    let (tong, rao) = co_hang_rao(&src);
    assert!(
        tong >= 2,
        "KHÔNG ĐO ĐƯỢC: chỉ thấy {tong} chỗ dán kết quả /runin (mong ≥ 2: dán lần đầu + gõ lại) — \
         đổi hình dạng thì sửa bài này: {:?}",
        cho_dan(&src)
    );
    assert_eq!(
        rao,
        tong,
        "có chỗ dán kết quả /runin KHÔNG qua `paste_target_ok`: {:?}",
        cho_dan(&src)
    );
}

/// ĐỐI CHỨNG NGƯỢC: phép dò phải bắt được một chỗ dán không có hàng rào.
#[test]
fn doi_chung_nguoc_phep_do_hang_rao() {
    let khong_rao = "Ok(Some(w)) => match crate::keys::type_and_send(w, &block) {\nOk(Some(w)) => {\ncrate::keys::type_and_send(w, &p.b).map(|d| d)";
    assert_eq!(co_hang_rao(khong_rao), (2, 0), "phép dò mù");
    // Hình dạng MỚI của cửa chung (`dan_vao_phien`, tham số `block: &str`) cũng
    // phải bị bắt khi thiếu rào — không thì gom cửa xong là phép dò mù với nó.
    let cua_chung_khong_rao = "Ok(Some(w)) => match crate::keys::type_and_send(w, block) {";
    assert_eq!(
        co_hang_rao(cua_chung_khong_rao),
        (1, 0),
        "phép dò mù với cửa chung"
    );
    let co_rao = "Ok(Some(w)) if paste_target_ok(w, &s.tty, &s.session_id) => match crate::keys::type_and_send(w, &block) {\nOk(Some(w)) if paste_target_ok(w, &t, &p.s) => {\ncrate::keys::type_and_send(w, &p.b).map(|d| d)";
    assert_eq!(
        co_hang_rao(co_rao),
        (2, 2),
        "phép dò chặn cả hình dạng đúng"
    );
}

/// Thân một hàm cấp cao nhất (mã đã qua `cargo fmt`).
fn than_ham<'a>(src: &'a str, ten: &str) -> Option<&'a str> {
    let dau = src.find(&format!("fn {ten}("))?;
    let cuoi = src[dau..].find("\n}\n")?;
    Some(&src[dau..dau + cuoi + 2])
}

/// Trong `than`, cú dán (`neo_dan`) có đứng SAU một `exec::urgent()` không —
/// `None` khi thiếu một trong hai (KHÔNG-ĐO-ĐƯỢC ⟹ bài ĐỎ).
fn dan_o_hang_gap(than: &str, neo_dan: &str) -> Option<bool> {
    let dan = than.find(neo_dan)?;
    let gap = than[..dan].rfind("exec::urgent()")?;
    Some(gap < dan)
}

/// 🔴 CẢ cú dán phải chạy HẠNG GẤP, không chỉ câu hỏi tty (2026-09-24). Gọi từ
/// vòng chạy (hạng nền) thì câu hỏi tty gấp của `paste_target_ok` mở cửa nhường
/// 2 giây ⟹ `type_and_send` ngay sau nó bị nhường. Đo: việc 5 của sổ việc hỏng
/// 2/2 lượt trả lại với đúng câu "nhường Terminal…"; đường gõ lại cũ từ 21:29Z
/// nhận 2 việc, dán được 0.
#[test]
fn cu_dan_chay_hang_gap_o_ca_hai_cua() {
    let src = nguon();
    let cua = than_ham(&src, "dan_vao_phien").expect("KHÔNG ĐO ĐƯỢC: mất `dan_vao_phien`");
    assert_eq!(
        dan_o_hang_gap(cua, "window_of("),
        Some(true),
        "`dan_vao_phien` phải nâng hạng gấp TRƯỚC khi hỏi Terminal"
    );
    let go_lai =
        than_ham(&src, "runin_pending_tick").expect("KHÔNG ĐO ĐƯỢC: mất `runin_pending_tick`");
    assert_eq!(
        dan_o_hang_gap(go_lai, "type_and_send(w, &p.b)"),
        Some(true),
        "`runin_pending_tick` phải gõ lại ở hạng gấp"
    );
}

#[test]
fn doi_chung_nguoc_cu_dan_hang_gap() {
    let nen = "fn dan_vao_phien(s: &S, block: &str) -> DanVao {\n    match crate::keys::window_of(&s.tty) {}\n}\n";
    assert_eq!(
        dan_o_hang_gap(than_ham(nen, "dan_vao_phien").unwrap(), "window_of("),
        None,
        "thiếu hạng gấp phải ra None ⟹ bài chính ĐỎ"
    );
    // Hạng gấp đứng SAU cú dán thì không tính.
    let sau = "fn runin_pending_tick() {\n    crate::keys::type_and_send(w, &p.b);\n    let _l = crate::exec::urgent();\n}\n";
    assert_eq!(
        dan_o_hang_gap(
            than_ham(sau, "runin_pending_tick").unwrap(),
            "type_and_send(w, &p.b)"
        ),
        None
    );
    let gap = "fn dan_vao_phien(s: &S, block: &str) -> DanVao {\n    let _lane = crate::exec::urgent();\n    match crate::keys::window_of(&s.tty) {}\n}\n";
    assert_eq!(
        dan_o_hang_gap(than_ham(gap, "dan_vao_phien").unwrap(), "window_of("),
        Some(true)
    );
}

/// Câu hỏi tty ở hàng rào phải chạy HẠNG GẤP — không thì ngân sách hỏi Terminal
/// của vòng nền chặn nó, hàng rào dừng ở phía an toàn và kết quả bị dời (đo
/// 23/09 18:16→21:15Z: 23 lượt `runin_paste_target_unverified`, một lượt chờ
/// 613 giây, một lượt bỏ cuộc sau 1072 giây).
fn hoi_tty_hang_gap(src: &str) -> Option<bool> {
    let dau = src.find("fn paste_target_ok(")?;
    let cuoi = src[dau..].find("\n}\n")?;
    let than = &src[dau..dau + cuoi];
    let gap = than.find("exec::urgent()")?;
    let hoi = than.find("selected_tab_tty(")?;
    Some(gap < hoi)
}

#[test]
fn hang_rao_hoi_tty_o_hang_gap() {
    let src = nguon();
    assert_eq!(
        hoi_tty_hang_gap(&src),
        Some(true),
        "`paste_target_ok` phải giữ `exec::urgent()` TRƯỚC khi hỏi `selected_tab_tty`"
    );
}

#[test]
fn doi_chung_nguoc_hang_gap() {
    let nen = "fn paste_target_ok(w: i64) -> bool {\n    match crate::keys::selected_tab_tty(w) { _ => true }\n}\n";
    assert_eq!(
        hoi_tty_hang_gap(nen),
        None,
        "thiếu hạng gấp phải ra None ⟹ bài chính ĐỎ"
    );
    let gap = "fn paste_target_ok(w: i64) -> bool {\n    let _lane = crate::exec::urgent();\n    match crate::keys::selected_tab_tty(w) { _ => true }\n}\n";
    assert_eq!(hoi_tty_hang_gap(gap), Some(true));
}
