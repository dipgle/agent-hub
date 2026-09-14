//! `/exit` chỉ được đi bằng MỘT đường, và đường ấy phải có cú Enter rời.
//!
//! 🔴 Hà 2026-08-15: *"ở phiên tfl5 có thấy lệnh exit nào đâu"*. Anh soi nhầm
//! cửa sổ — huba nhắm `ttys004` (phiên huba tiền nhiệm), không phải phiên tfl5 —
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

// ── Ai GHI SỔ cũng phải khai đúng mình đã gõ `/exit` hay chưa ───────────────
//
// 🔴 Vì sao bài kiểm này đứng ở đây thay vì trong `close_book.rs`: `close_step`
// là hàm thuần, bài kiểm của nó chứng minh được *"bận + chưa gõ ⇒ SendExit"* —
// nhưng **khối đúng không nói người gọi truyền đúng**. Nếu `defer_close_to_book`
// khai bừa `now` vào ô `exit_sent_at`, sổ lại tin `/exit` đã đi, và con bug
// 13/09 sống lại nguyên vẹn trong khi 7 bài kiểm của `close_book.rs` vẫn xanh.
// Đó đúng là hình dạng mutant đã sống qua vòng đối chứng ngày 13/09 ở một bản vá
// khác (`L5`: chỗ gọi truyền thẳng `false`).
//
// Không kiểm được bằng hành vi: `defer_close_to_book` gọi `keys::window_of`, thứ
// đòi một cửa sổ Terminal thật. Nên kiểm bằng HÌNH DẠNG MÃ, cùng lý lẽ với cả
// tệp này.

fn pipeline_src() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs"))
        .expect("đọc được src/pipeline.rs")
}

/// Thân một hàm, nhận cả `fn` lẫn `pub fn`, cắt tới khai báo hàm KẾ TIẾP ở bất
/// kỳ mức hiển thị nào.
///
/// `body_of` ở trên chỉ biết `pub fn`, nên nó trượt `close_and_say` (hàm riêng
/// tư) và, tệ hơn, với một hàm riêng tư nó sẽ nuốt luôn mấy hàm sau — tức bài
/// kiểm đọc nhầm chỗ gọi của người khác rồi phán về hàm này. Một phép đo neo sai
/// bề mặt thì đỏ-sai-chỗ còn tệ hơn xanh giả.
fn fn_body(src: &str, name: &str) -> String {
    let start = src
        .find(&format!("\nfn {name}("))
        .map(|i| i + 1)
        .or_else(|| src.find(&format!("\npub fn {name}(")).map(|i| i + 1))
        .unwrap_or_else(|| panic!("không còn hàm `{name}` — đổi tên thì sửa cả bài kiểm này"));
    let rest = &src[start..];
    let end = rest[1..]
        .find("\nfn ")
        .into_iter()
        .chain(rest[1..].find("\npub fn "))
        .min()
        .map(|i| i + 1)
        .unwrap_or(rest.len());
    rest[..end].to_string()
}

/// Các đối số của lượt gọi `remember_closing(` ĐẦU TIÊN trong `body`, cắt theo
/// dấu phẩy ở ĐỘ SÂU 0 — không cắt bằng `split(',')` trần, vì `&crate::…::shown(s)`
/// và `json!({…})` đều mang dấu phẩy lồng bên trong.
fn remember_closing_args(body: &str) -> Vec<String> {
    let at = body.find("remember_closing(").expect(
        "thân hàm không còn gọi `remember_closing` — đổi cách ghi sổ thì sửa cả bài kiểm này",
    );
    let rest = &body[at + "remember_closing(".len()..];
    let mut depth = 0i32;
    let mut args = vec![String::new()];
    for ch in rest.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                args.last_mut().unwrap().push(ch);
            }
            ')' if depth == 0 => break,
            ')' | ']' | '}' => {
                depth -= 1;
                args.last_mut().unwrap().push(ch);
            }
            ',' if depth == 0 => args.push(String::new()),
            _ => args.last_mut().unwrap().push(ch),
        }
    }
    args.into_iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect()
}

#[test]
fn nobody_writes_the_book_claiming_an_exit_it_did_not_send() {
    let src = pipeline_src();

    // ① Nhánh BÀN GIAO — `quit_and_close` trả `Err` cho cả hai ca (chết ở
    //    `send_exit`, và gõ được nhưng tab vẫn bận), nên đường này KHÔNG biết.
    //    Khai `0` là khai đúng cái mình biết; khai `now` là dựng lại con bug.
    let defer = fn_body(&src, "defer_close_to_book");
    let args = remember_closing_args(&defer);
    assert_eq!(
        args.len(),
        6,
        "`remember_closing` đổi số đối số ({} cái) — bài kiểm này neo vào ô cuối, sửa mã thì sửa cả đây",
        args.len()
    );
    assert_eq!(
        args[5], "0",
        "`defer_close_to_book` khai `{}` vào ô `exit_sent_at` — đường bàn giao KHÔNG \
         biết `/exit` đã đi hay chưa, khai bừa là sổ thôi gõ hộ và cửa sổ cũ nằm mở \
         như ca 53a0683e ngày 13/09",
        args[5]
    );

    // ② ĐỐI CHỨNG NGƯỢC cùng chỗ: route `/close` thì BIẾT chắc —
    //    `Closing::Exiting` chỉ ra đời sau một `send_exit` trả `Ok` — nên nó
    //    phải khai mốc thật. Một bài kiểm chỉ đòi "mọi chỗ đều khai 0" sẽ xanh
    //    cả khi ai đó làm sổ mù hẳn, tức không phân biệt nổi hai đường.
    let close_say = fn_body(&src, "close_and_say");
    let args2 = remember_closing_args(&close_say);
    assert_eq!(args2.len(), 6, "cùng hợp đồng, cùng 6 ô");
    assert_ne!(
        args2[5], "0",
        "route `/close` vừa gõ `/exit` xong mà khai `0` ⟹ sổ sẽ gõ thêm lần nữa \
         vào đúng cái TUI ấy, và cú thứ hai rơi xuống shell"
    );

    // ③ Nhánh THI HÀNH phải đặt lại CẢ HAI đồng hồ, và không đo được bằng hành
    //    vi (nó gọi `keys::send_exit`, đòi Terminal thật). `c.x` để thôi gõ lần
    //    nữa; `c.t` vì trần `CLOSE_GIVE_UP_SEC` được định nghĩa là *"chờ bao lâu
    //    SAU `/exit`"* — đếm nó từ lúc mục vào sổ, tức từ trước khi `/exit` đi,
    //    là đếm sai thứ, và cửa sổ bị buông sớm đúng bằng quãng đã đợi hư không.
    let tick = fn_body(&src, "close_pending_tick");
    let at = tick.find("CloseStep::SendExit =>").expect(
        "`close_pending_tick` không còn thi hành `SendExit` — sổ lại thành máy chỉ biết CHỜ",
    );
    // Cắt tới RANH GIỚI KÝ TỰ, không cắt theo byte: chú thích quanh nhánh ấy đầy
    // `🔴`/`⟹` (4 byte), và `&s[a..b]` rơi vào giữa một ký tự là panic — bài kiểm
    // chết vì cái thước của nó, không vì thứ nó đo. Bản đầu của đúng dòng này đã
    // đỏ như thế: *"end byte index 8260 is not a char boundary; it is inside 🔴"*.
    let mut end = (at + 1200).min(tick.len());
    while end < tick.len() && !tick.is_char_boundary(end) {
        end += 1;
    }
    let arm = &tick[at..end];
    assert!(
        arm.contains("c.x = now"),
        "gõ xong mà không ghi `c.x` ⟹ lượt sau gõ nữa, mỗi 30 giây một `/exit`"
    );
    assert!(
        arm.contains("c.t = now"),
        "gõ xong mà không đặt lại `c.t` ⟹ trần 600 giây vẫn đếm từ lúc mục vào sổ, \
         nên CLI chỉ còn phần thừa của quãng đã đợi hư không để chạy nốt lượt dở"
    );
}
