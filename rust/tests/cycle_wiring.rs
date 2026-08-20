//! Cỗ máy chạy mỗi vòng phải CÓ NGƯỜI GỌI — bài kiểm cho một lỗi trình dịch
//! không bao giờ bắt được.
//!
//! 🔴 Vì sao có tệp này. Ngày 2026-08-14 huba gỡ trang tfl5: `lib.rs` bỏ
//! `mod portal`, và **ba** hàm chạy-mỗi-vòng mất chỗ gọi DUY NHẤT của chúng
//! (`announce_changes`, `close_pending_tick`, `trust_dialog_tick`). Không một
//! cảnh báo nào — `pub fn` trong `pub mod` thì `dead_code` im, `cargo test`
//! xanh 263/263, `cargo clippy` 0. Hậu quả đo được trên `logs/huba.log`:
//! `session_change` **439 lượt, lần cuối 14/08 13:10:40**, rồi im hơn một
//! ngày; sổ `closing:windows` giữ hai hàng `c: 0` (chưa từng được ngó lại);
//! và Hà nhìn thấy hậu quả bằng mắt: *"tại sao chuyển phiên mới rồi mà phiên cũ
//! vẫn còn cửa sổ chưa đóng?"*.
//!
//! Cùng một hình dạng với `errors_block` đã ghi trong `CLAUDE.md` — chỗ gọi duy
//! nhất là `portal.rs`, tệp đã chết. Ba lần nữa, trong cùng một commit.
//!
//! Nên bài kiểm này soi **mã nguồn**, không soi hành vi: hành vi của một hàm
//! không ai gọi thì không quan sát được từ bên trong tiến trình, mà đó chính là
//! chỗ chết. Nó ĐỎ ĐƯỢC: bỏ một dòng gọi khỏi `run_once` là đỏ.

/// Thân hàm `pub fn run_once`, cắt từ chính tệp nguồn.
fn run_once_body() -> String {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs"))
        .expect("đọc được src/pipeline.rs");
    let start = src
        .find("pub fn run_once(")
        .expect("không còn hàm `run_once` — nếu đổi tên thì sửa cả bài kiểm này");
    // Vòng chạy kết thúc ở hàm kế tiếp cùng cấp; không có thì tới hết tệp.
    let rest = &src[start..];
    let end = rest[1..]
        .find("\npub fn ")
        .or_else(|| rest[1..].find("\nfn "))
        .map(|i| i + 1)
        .unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn every_per_cycle_machine_is_called_from_run_once() {
    let body = run_once_body();
    // Mỗi dòng dưới đây là một cỗ máy chỉ chạy được nếu vòng chạy gọi nó, kèm
    // thứ mất đi khi nó câm — viết ra để lượt sau ai gỡ thì phải gỡ có ý thức.
    for (call, what_dies) in [
        (
            "execute_telegram_commands(",
            "mọi mệnh lệnh gõ trên Telegram (huba thành câm hoàn toàn)",
        ),
        (
            "announce_changes(",
            "luật 11 — 'vừa xong' / 'vừa tắt' không còn ai nói",
        ),
        (
            "close_pending_tick(",
            "cửa sổ hụt đóng nằm lại trong sổ mãi mãi (Hà nhìn thấy 15/08)",
        ),
        (
            "trust_dialog_tick(",
            "cửa sổ kẹt ở hộp tin-thư-mục không ai bấm hộ",
        ),
        ("auto_handover(", "phiên đầy ngữ cảnh không được đóng sổ hộ"),
        ("prune_sent(", "tin Telegram quá hạn không ai dọn"),
    ] {
        assert!(
            body.contains(call),
            "`run_once` không còn gọi `{call}` ⟹ mất: {what_dies}"
        );
    }
}

#[test]
fn the_dead_page_module_stays_out_of_the_build() {
    // `portal.rs` và `live.rs` còn nằm trên đĩa (chờ `git rm` — Claude bị chặn),
    // nhưng chúng KHÔNG được quay lại `lib.rs`: khai lại là dựng lại đúng cái
    // chỗ gọi giả đã giấu ba cỗ máy trên suốt một ngày.
    let lib = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
        .expect("đọc được src/lib.rs");
    for dead in ["mod portal", "mod live"] {
        assert!(
            !lib.contains(dead),
            "`{dead}` quay lại `lib.rs` — nếu thật sự cần thì phải nói rõ ai gọi ai, \
             đừng để nó lại làm chỗ gọi duy nhất cho việc chạy mỗi vòng"
        );
    }
}
