//! `/exit` chỉ được đi bằng MỘT đường, và đường ấy phải có cú Enter rời.
//!
//! 🔴 Hà 2026-08-15: *"ở phiên tfl5 có thấy lệnh exit nào đâu"*. Anh soi nhầm
//! cửa sổ — hub nhắm `ttys004` (phiên hub tiền nhiệm), không phải phiên tfl5 —
//! nhưng câu hỏi lôi ra một lỗi thật: `keys::quit_and_close` và `keys::send_exit`
//! mỗi hàm giữ một bản chép tay của `osascript(do_script(w, "/exit"))`, **cả hai
//! đều thiếu cú Enter rời**.
//!
//! Vì sao thiếu Enter là hỏng, chứ không phải chi tiết: `do script` đẩy chữ +
//! dấu xuống dòng trong CÙNG một lượt ghi, TUI của `claude` đọc cả cụm như một
//! cú DÁN và nuốt dấu xuống dòng ⟹ `/exit` nằm lại trong ô nhập. Đúng con bug
//! đã trả giá cả tối 12/08 cho `/type`, đã có sẵn thuốc (`type_and_send`), mà
//! đường đóng phiên không ai nối vào.
//!
//! Không kiểm được bằng hành vi: muốn quan sát thì phải có một cửa sổ Terminal
//! thật đang chạy `claude` và một phiên chịu bị đóng. Nên kiểm bằng HÌNH DẠNG
//! MÃ — thứ duy nhất đứng được ở đây, y như `tests/cycle_wiring.rs`.

fn keys_src() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/keys.rs"))
        .expect("đọc được src/keys.rs")
}

/// Thân một hàm, cắt từ `pub fn <tên>(` tới `pub fn` kế tiếp.
fn body_of(src: &str, name: &str) -> String {
    let start = src
        .find(&format!("pub fn {name}("))
        .unwrap_or_else(|| panic!("không còn hàm `{name}` — đổi tên thì sửa cả bài kiểm này"));
    let rest = &src[start..];
    let end = rest[1..]
        .find("\npub fn ")
        .map(|i| i + 1)
        .unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn the_exit_command_goes_through_the_one_path_that_presses_enter() {
    let src = keys_src();

    let send_exit = body_of(&src, "send_exit");
    assert!(
        send_exit.contains("type_and_send("),
        "`send_exit` không còn đi qua `type_and_send` ⟹ mất cú Enter rời ⟹ `/exit` \
         nằm lại trong ô nhập và phiên không bao giờ thoát (luật 13)"
    );

    let quit = body_of(&src, "quit_and_close");
    assert!(
        quit.contains("send_exit("),
        "`quit_and_close` tự gõ `/exit` lần nữa thay vì gọi `send_exit` — bản chép \
         tay thứ hai chính là bản đã thiếu Enter"
    );

    // Một nguồn duy nhất cho chuỗi ấy: hai chỗ gõ là hai chỗ để quên một luật.
    let n = src.matches(r#"as_string("/exit")"#).count();
    assert_eq!(
        n, 0,
        "còn {n} chỗ đẩy thẳng `/exit` bằng `do script` — đường duy nhất phải là `send_exit`"
    );
}

#[test]
fn the_failure_sentence_claims_only_what_it_measured() {
    // Câu cũ mở đầu bằng *"đã gõ /exit"* — một mệnh đề về HÀNH ĐỘNG, phát ra từ
    // chỗ chỉ biết `osascript` trả 0 (mà `osascript` trả 0 chỉ chứng minh bytes
    // tới tab). Hà đọc câu ấy, đi soi cửa sổ, không thấy `/exit` đâu — một dòng
    // log sai kiểu ấy tiêu đúng thứ đắt nhất nó có: lòng tin.
    // Soi ĐÚNG câu báo hỏng, không soi cả thân hàm: chú thích ngay trên nó có
    // quyền trích lại câu CŨ để kể vì sao nó sai — và bản đầu của bài kiểm này
    // đỏ đúng vì thế. Một phép đo quét quá rộng thì nó đo cả lời kể về lỗi.
    let body = body_of(&keys_src(), "quit_and_close");
    let at = body
        .find("anyhow::bail!(")
        .expect("`quit_and_close` không còn câu báo hỏng nào");
    let quit = &body[at..body[at..].find(");").map(|i| at + i).unwrap_or(body.len())];
    assert!(
        !quit.contains("đã gõ /exit"),
        "câu báo hỏng lại khẳng định một hành động chưa quan sát được"
    );
    assert!(
        quit.contains("tab_state"),
        "câu báo hỏng phải nêu thứ nó THẬT SỰ đo được"
    );
    assert!(
        quit.contains("keys_exit_sent"),
        "…và chỉ đường tới dòng log đối chứng được"
    );
}
