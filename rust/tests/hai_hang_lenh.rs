//! HAI hàng lệnh: lệnh `/` chủ máy gõ trên Telegram ≠ việc do PHIÊN nhờ chạy.
//!
//! 🔴 Hà 2026-09-23: *"Lệnh ở đây là lệnh "/" của hub nhận từ tele, chứ không
//! phải để dán vào phiên, như vậy phải có 2 hàng đợi trên luồng xử lý từ tele
//! mới đúng chứ"*. Đo cùng ngày (`logs/huba.log`): **179/342** lệnh trong hàng
//! Telegram là `/runin` do hòm thư `huba-run.txt` của các phiên nhét vào (794
//! lượt trong 7 ngày), chạy chung luồng `telegram-now` và chung `CMD_LOCK` với
//! lệnh của Hà. Mỗi lượt ấy còn chụp toàn bộ chữ của mọi tab Terminal HAI lần
//! (một ở vòng quét hòm thư, một ở nhánh `RunIn`) — ~6 giây/lượt, 37 giây lúc
//! máy nặng — chỉ để tra `tty` của MỘT phiên.
//!
//! Bài này đọc MÃ NGUỒN (cùng kiểu `no_window_resizing.rs`), vì thứ cần khoá là
//! một hình dạng kiến trúc — "việc của phiên không vào hàng của chủ máy" — chứ
//! không phải một giá trị trả về. Mỗi phép dò có đối chứng ngược ở cuối tệp.

/// Mã nguồn `pipeline.rs`, đọc LÚC CHẠY. `HUBA_PIPELINE_SRC` trỏ sang một bản
/// khác để chạy đối chứng trên mã CŨ mà không phải dựng lại thư viện
/// (`git show <sha>:rust/src/pipeline.rs > /tmp/cu.rs`).
fn nguon() -> String {
    let p = std::env::var("HUBA_PIPELINE_SRC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs").to_string());
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Thân một hàm cấp cao nhất: từ `fn <tên>(` tới dấu `}` đứng đầu dòng đầu tiên
/// sau đó (mã đã qua `cargo fmt`). `None` = không tìm thấy ⟹ chỗ gọi ĐỎ.
fn than_ham<'a>(src: &'a str, ten: &str) -> Option<&'a str> {
    let dau = src.find(&format!("fn {ten}("))?;
    let cuoi = src[dau..].find("\n}\n")?;
    Some(&src[dau..dau + cuoi + 2])
}

/// Thân nhánh `CommandKind::RunIn =>` — tới nhánh kế tiếp `CommandKind::Win =>`.
fn than_nhanh_runin(src: &str) -> Option<&str> {
    let dau = src.find("CommandKind::RunIn => {")?;
    let cuoi = src[dau..].find("CommandKind::Win =>")?;
    Some(&src[dau..dau + cuoi])
}

/// Vòng quét hòm thư có nhét việc vào hàng TELEGRAM không.
fn vao_hang_telegram(than: &str) -> bool {
    than.contains("push_text")
}

/// Có chụp toàn bộ danh sách phiên (đọc chữ mọi tab Terminal) không.
fn chup_toan_man(than: &str) -> bool {
    than.contains("sessions::snapshot(")
}

/// Trong `than`, sổ có được hỏi TRƯỚC ảnh chụp không.
fn so_truoc_anh_chup(than: &str) -> bool {
    match (
        than.find("window_target_from_book"),
        than.find("sessions::snapshot("),
    ) {
        (Some(so), Some(anh)) => so < anh,
        (Some(_), None) => true,
        _ => false,
    }
}

#[test]
fn hom_thu_cua_phien_khong_vao_hang_cua_chu_may() {
    let src = nguon();
    let than = than_ham(&src, "runin_inbox_tick")
        .expect("KHÔNG ĐO ĐƯỢC: không tìm thấy `runin_inbox_tick` — đổi tên thì sửa bài này");
    assert!(
        !vao_hang_telegram(than),
        "hòm thư của phiên lại nhét vào hàng Telegram của chủ máy:\n{than}"
    );
    assert!(
        than.contains("queue_session_run("),
        "hòm thư phải xếp vào hàng THỨ HAI (`queue_session_run`):\n{than}"
    );
    assert!(
        !chup_toan_man(than),
        "vòng quét hòm thư lại chụp toàn bộ Terminal cho mỗi tệp:\n{than}"
    );
}

#[test]
fn runin_tra_so_truoc_roi_moi_chup_man() {
    let src = nguon();
    let than = than_nhanh_runin(&src).expect(
        "KHÔNG ĐO ĐƯỢC: không tìm thấy nhánh `CommandKind::RunIn` — đổi hình dạng thì sửa bài này",
    );
    assert!(
        so_truoc_anh_chup(than),
        "`RunIn` phải hỏi SỔ (`window_target_from_book`) trước khi chụp toàn bộ Terminal"
    );
}

#[test]
fn hang_thu_hai_khong_giu_khoa_va_khong_chen_hang_gap() {
    let src = nguon();
    let than = than_ham(&src, "execute_session_runs")
        .expect("KHÔNG ĐO ĐƯỢC: không tìm thấy `execute_session_runs`");
    assert!(
        !than.contains("CMD_LOCK"),
        "hàng của phiên giữ `CMD_LOCK` ⟹ lệnh của chủ máy lại phải xếp sau nó"
    );
    assert!(
        !than.contains("exec::urgent("),
        "hàng của phiên chạy hạng GẤP ⟹ nó thôi nhường lượt Terminal cho chủ máy"
    );
    // Và lệnh của chủ máy thì VẪN giữ cả hai — không phải gỡ khoá cho cả hai hàng.
    let chu = than_ham(&src, "execute_telegram_commands")
        .expect("KHÔNG ĐO ĐƯỢC: không tìm thấy `execute_telegram_commands`");
    assert!(chu.contains("CMD_LOCK") && chu.contains("exec::urgent("));
}

/// ĐỐI CHỨNG NGƯỢC: bơm hình dạng CŨ (đo được trong git trước lượt vá) vào
/// từng phép dò — mỗi cái phải bắt được. Không có bước này thì một phép dò luôn
/// trả `false` cũng làm cả tệp xanh.
#[test]
fn doi_chung_nguoc_cac_phep_do_bat_duoc_hinh_dang_cu() {
    let cu = "fn runin_inbox_tick(db: &Db, cfg: &Config) -> usize {\n    let alive = crate::sessions::snapshot(cfg);\n    tg.push_text_quiet(&format!(\"/runin {sid} {cmd}\"));\n}\n";
    let than = than_ham(cu, "runin_inbox_tick").expect("phải cắt được thân hàm giả");
    assert!(vao_hang_telegram(than), "phép dò hàng Telegram mù");
    assert!(chup_toan_man(than), "phép dò chụp toàn màn mù");

    // Nhánh RunIn bản cũ: chụp màn, không hỏi sổ.
    let runin_cu = "CommandKind::RunIn => {\n let live = crate::sessions::snapshot(cfg);\n }\n CommandKind::Win => {";
    let t = than_nhanh_runin(runin_cu).expect("phải cắt được nhánh giả");
    assert!(!so_truoc_anh_chup(t), "phép dò thứ tự sổ/ảnh chụp mù");
    // Chiều lành: sổ trước ⟹ phải qua.
    let runin_moi = "CommandKind::RunIn => {\n window_target_from_book(&v, &want);\n crate::sessions::snapshot(cfg);\n }\n CommandKind::Win => {";
    assert!(so_truoc_anh_chup(than_nhanh_runin(runin_moi).unwrap()));
    // Không tìm thấy thì phải là `None` (⟹ bài ĐỎ), không phải một thân rỗng.
    assert!(than_ham("fn khac() {\n}\n", "runin_inbox_tick").is_none());
}
