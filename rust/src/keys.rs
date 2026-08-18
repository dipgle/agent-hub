//! Gõ vào cửa sổ terminal của một phiên, và chụp lại cửa sổ ấy.
//!
//! # Vì sao có tệp này
//!
//! Cho tới 2026-08-09 hub **không gõ được** vào phiên interactive: `claude` từ
//! chối `--resume` một phiên đang chạy, và không có primitive nào nhét chữ vào
//! đó (`CLAUDE.md` điều 10). Hệ quả thực tế: một phiên dừng lại hỏi *"chọn
//! phương án nào?"* thì từ điện thoại **không thấy và không trả lời được** —
//! bản ghi câu hỏi chỉ vào nhật ký SAU khi lượt kết thúc, nên nó vô hình cả với
//! `sessions::stream`.
//!
//! Hà chốt 2026-08-09, sau khi tôi nêu rõ đánh đổi: cho hub **gõ tự do** vào
//! phiên. Đây là quyết định của chủ máy, và nó **bỏ qua `DENIED_TOOLS`** —
//! chữ gõ thẳng vào terminal không đi qua bộ khoá nào. Ghi rõ ở đây để không ai
//! đọc mã sau này tưởng đó là sơ suất.
//!
//! # Hàng rào còn giữ (không phải về quyền, mà về ĐÚNG ĐÍCH)
//!
//! * Chỉ gõ vào cửa sổ **ghép được với một phiên có thật** qua `tty`. Không ghép
//!   được thì từ chối — gõ vào cửa sổ lạ là gõ vào việc của người khác.
//! * Mọi lần gõ đều **log** (`keys_typed`) kèm phiên và độ dài chuỗi. Nội dung
//!   không log: nó là chữ của chủ máy, và log là tệp nằm lâu.
//! * Lệnh đi qua phòng chat như mọi động từ khác, nên có dấu vết ở nơi đọc được.
//!
//! # Cái giá phải nói trước
//!
//! `System Events` gõ vào **cửa sổ đang ở trước**, nên hub phải kéo cửa sổ ấy
//! lên trước khi gõ. Tức là gõ từ điện thoại sẽ **giật tiêu điểm** trên máy.
//! Không có đường vòng: đó là cách macOS cho gõ vào một tiến trình interactive.

use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::json;

use crate::exec::{run, RunOpts};
use crate::logging;

/// `osascript` mất vài trăm ms; 20s là quá rộng rãi, và một cái treo ở đây sẽ
/// giữ cả vòng chạy của daemon.
const OSA_TIMEOUT: Duration = Duration::from_secs(20);

fn osascript(script: &str) -> Result<String> {
    let out = run(
        "osascript",
        &["-e", script],
        RunOpts {
            timeout: Some(OSA_TIMEOUT),
            ..Default::default()
        },
    )?;
    if out.timed_out {
        return Err(anyhow!("osascript quá {}s", OSA_TIMEOUT.as_secs()));
    }
    if out.code != Some(0) {
        return Err(anyhow!(
            "osascript hỏng: {}",
            crate::exec::truncate(out.stderr.trim(), 200)
        ));
    }
    Ok(out.stdout.trim().to_string())
}

/// Chuỗi cho AppleScript: chỉ có hai ký tự phải thoát, và bỏ sót một cái là
/// script hỏng cú pháp — hoặc tệ hơn, đổi nghĩa.
fn as_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Đoạn AppleScript hỏi cửa sổ nào đang chạy `dev`.
///
/// Tách ra thành hàm THUẦN để kiểm được bằng test: lỗi đầu tiên của tính năng
/// này là một dòng `return ""` thừa ở cuối, bị cú pháp chuỗi thô của Rust nuốt
/// mất dấu nháy, và nó chỉ lộ khi chạy thật vì không có gì soi chuỗi sinh ra.
fn window_script(dev: &str) -> String {
    // `try` quanh vòng trong, KHÔNG phải cho đẹp: Terminal có những cửa sổ
    // không có tab nào (cửa sổ cài đặt, inspector), và hỏi `tabs of w` ở đó thì
    // cả script chết giữa chừng — *"Can't get every tab of item 1 of every
    // window. (-1728)"*, đo 2026-08-10. Một cửa sổ lạ không được phép làm hỏng
    // việc tìm cửa sổ đúng.
    // 🔴 Khớp tty KHÔNG đủ — đo 2026-08-11.
    //
    // Terminal giữ lại `tty` của một tab **đã chết** (shell thoát, tab hiện
    // `[Process completed]`), còn macOS thì **dùng lại số tty**. Trên máy này,
    // cùng lúc có BA cửa sổ khai `/dev/ttys005`:
    //
    // ```text
    // win=54312 tty=/dev/ttys005 busy=false proc=          ← xác
    // win=54299 tty=/dev/ttys005 busy=false proc=          ← xác
    // win=54478 tty=/dev/ttys005 busy=false proc=login-zsh ← thật
    // ```
    //
    // Bản trước trả về cửa sổ ĐẦU TIÊN khớp, tức rất dễ trúng một cái xác. Hậu
    // quả không nhẹ: màn hình đẩy lên điện thoại là màn của phiên CŨ (Hà thấy
    // `accept edits on` trong khi phiên thật đang `auto mode on`), `/type` gõ
    // vào cửa sổ chết, `can_type` khai bừa là gõ được, và `tab_state` — thứ
    // quyết định có dám đóng cửa sổ không — đọc một tab không còn ai ở đó.
    //
    // Lọc bằng thứ Terminal trả lời được: tab CÒN TIẾN TRÌNH. Trong các tab còn
    // sống, ưu tiên tab đang chạy chương trình (`busy`) — một phiên `claude`,
    // kể cả lúc đứng ở dấu nhắc, luôn `busy = true`; một shell trống thì không.
    format!(
        r#"tell application "Terminal"
  set alive to missing value
  repeat with w in every window
    try
      repeat with t in tabs of w
        if tty of t is {} and (count of (processes of t)) > 0 then
          if busy of t then return id of w
          if alive is missing value then set alive to id of w
        end if
      end repeat
    end try
  end repeat
  if alive is not missing value then return alive
end tell"#,
        as_string(dev)
    )
}

/// Chữ vừa gõ đã đi đâu — đọc từ màn, không phải từ mã trả về của osascript.
///
/// Đây là bài học đắt nhất của cả tính năng này (2026-08-10): `osascript` trả
/// về 0, log ghi `keys_typed`, hub báo "⌨ đã bấm" — mà Hà **không thấy hiện
/// tượng gì**. Vì `do script` chỉ nói "đã đẩy được byte vào tab", nó không nói
/// chương trình bên trong làm gì với byte ấy. Muốn biết thì phải NHÌN.
#[derive(Debug, PartialEq, Eq)]
pub enum Landed {
    /// `claude` đang chạy dở nên xếp chữ vào hàng chờ, sẽ xử lý khi xong.
    Queued,
    /// Phiên đang làm việc (có đồng hồ) — chữ đã khởi động một lượt.
    Running,
    /// 🔴 Chữ VẪN NẰM trong ô nhập — tức CHƯA gửi được.
    ///
    /// Hà 2026-08-15, ảnh chụp ô nhập của `[dwork]` mang **hai tin dính liền**:
    /// *"sao nội dung lại bị lặp thế này"*. Đọc log ra đúng chuyện đã xảy ra:
    /// tin trước gõ xong, hub bắn hai Enter, đọc màn, rồi trả lời `✓ đã gửi` —
    /// trong khi chữ vẫn nằm nguyên trong ô. Tin sau gõ tiếp vào đúng ô ấy, nối
    /// đuôi, và cuối cùng cả hai đi **làm một tin**.
    ///
    /// Gốc là một PHÉP ĐO MÙ: `landed` chỉ biết ba trạng thái *hàng chờ · đang
    /// chạy · rảnh*, mà "rảnh" ở đây có hai nghĩa ngược nhau — *đã gửi xong* và
    /// *chưa gửi được*. Không có trạng thái này thì mọi màn không-bận đều đọc
    /// thành thành công, và hub **không thể** nói sai theo hướng nào khác.
    ///
    /// 📌 `still_in_box` đã có từ 12-08 và làm đúng việc của nó; nó chỉ không
    /// được ai hỏi sau khi bấm Enter. *Một hàm đúng không được gọi thì bằng
    /// không* — và chỗ nó vắng mặt là chỗ hub tự khen mình.
    InBox,
    /// Phiên đang đứng ở dấu nhắc, ô nhập TRỐNG — chữ đã đi.
    Idle,
}

/// Phần màn hình thuộc **ô nhập** — khối đóng khung cuối cùng.
///
/// `claude` vẽ ô nhập bằng khung `╭─╮ │ ╰─╯` ở đáy màn. Lấy từ dấu `╭` cuối cùng
/// trở đi là được đúng ô ấy (kèm dòng gợi ý dưới chân nó, vô hại). Không thấy
/// khung nào — chủ đề khác, cửa sổ hẹp — thì lùi về **4 dòng không rỗng cuối**:
/// vẫn là vùng đáy, chỉ kém sắc nét hơn, và thà kém sắc nét còn hơn soi cả màn
/// rồi đọc phần hội thoại thành nội dung ô nhập.
/// Phần màn TRƯỚC ô nhập — tức nội dung phiên đã nói, không kể chữ đang chờ gõ.
///
/// 🔴 Hà 2026-08-17, ảnh `/shot` `[codetrail]` có dòng `❯ vá luôn runner.sh 📎`:
/// *"Tại sao lại gắn nút tải file vào ô chát gợi ý mờ"*. Đúng: ô nhập là chỗ để
/// GỬI, và đích chạm của nó đã có rồi (⏎ · ⌫). Một cái 📎 mọc lên giữa câu chưa
/// gửi vừa thừa vừa gây hiểu nhầm — nó trông như thể chữ ấy đã là nội dung.
///
/// Không có khung ô nhập ⟹ trả cả màn: thà giữ nguyên hành vi cũ còn hơn cắt
/// mất phần thân vì một phép đoán.
pub fn body_before_box(screen: &str) -> &str {
    match screen.rfind('╭') {
        Some(i) => &screen[..i],
        None => screen,
    }
}

pub fn box_region(screen: &str) -> String {
    if let Some(i) = screen.rfind('╭') {
        return screen[i..].to_string();
    }
    let mut tail: Vec<&str> = screen
        .lines()
        .filter(|l| !l.trim().is_empty())
        .rev()
        .take(4)
        .collect();
    tail.reverse();
    tail.join("\n")
}

// 🪦 `asks_for_go_ahead(screen)` — gỡ 2026-08-16 cùng cái nút nó nuôi
// (`pipeline.rs`, bia mộ ở đó). Nó so màn với bảy cụm chữ mời (*"nói một
// tiếng"*, *"có muốn"*…) để dựng nút gửi hai chữ "làm đi" vào phiên. Hà: *"1
// xóa nút đó đi không cần nữa"*. Gỡ cả hàm chứ không để lại một phép đo không
// ai đọc: một hàm còn đó là một lời mời dựng lại cái nút ấy ở chỗ khác.

/// Chữ đang NẰM SẴN trong ô nhập, nếu có — thứ chỉ cần một Enter là gửi đi.
///
/// 🔴 Hà 2026-08-13, gửi ảnh một màn `/shot`: *"như ảnh vừa gửi có gợi ý nội
/// dung chat cần có cách bấm nhanh để gửi nó"*. Đúng: màn ấy có sẵn dòng
/// `❯ làm quota phép đi` nằm trong ô — chữ đã tới nơi, chỉ thiếu cú Enter — mà
/// từ điện thoại thì không có cách nào bấm cú ấy ngoài gõ lại cả câu.
///
/// Trả về chữ đã dọn (bỏ khung, dấu nhắc, dòng trạng thái), để chỗ gọi vừa
/// dựng được nhãn nút vừa biết có đáng dựng hay không.
pub fn input_box_text(screen: &str) -> Option<String> {
    let mut buf = String::new();
    for line in box_region(screen).lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        // Đường kẻ khung.
        if t.chars().all(|c| "─━-—_═".contains(c)) {
            continue;
        }
        // Dòng chân (chế độ quyền, gợi ý phím) và dòng tip — không phải chữ của
        // người gõ.
        if t.contains("auto mode on")
            || t.contains("esc to interrupt")
            || t.contains("shift+tab")
            || t.starts_with("Tip:")
            || t.starts_with("⎿")
        {
            continue;
        }
        let t = t
            .trim_start_matches(['❯', '>', '│', '┃', '|'])
            .trim_matches(|c: char| c.is_whitespace())
            .trim_end_matches(['│', '┃', '|'])
            .trim();
        if t.is_empty() {
            continue;
        }
        if !buf.is_empty() {
            buf.push(' ');
        }
        buf.push_str(t);
    }
    let out = buf.trim().to_string();
    (out.chars().count() >= 2).then_some(out)
}

/// Chữ vừa gõ CÒN NẰM trong ô nhập, hay đã đi?
///
/// 🔴 Hà đo 2026-08-12: *"nhận được text nhưng không tự gửi"*. Chú thích của
/// `do_script` (và cả CLAUDE.md) tin rằng `do script` luôn kèm một dấu xuống
/// dòng nên "gõ xong là gửi" — điều đó đúng với một cái shell, mà **sai với ô
/// nhập của `claude`**: chữ và dấu xuống dòng đi trong CÙNG MỘT lượt ghi, và
/// TUI đọc lượt ấy như một cú DÁN, tức nuốt luôn dấu xuống dòng vào nội dung.
/// Nên phải NHÌN xem chữ có còn nằm đó không rồi mới gửi Enter rời.
///
/// So sánh sau khi bóp bỏ khoảng trắng và khung viền: ô nhập ngắt dòng theo bề
/// ngang cửa sổ, nên một câu dài nằm trong ô sẽ bị cắt làm nhiều đoạn có `│` xen
/// vào — so nguyên văn thì trượt sạch. Cần **16 ký tự cuối** làm dấu vân tay:
/// đủ đặc trưng để không trùng ngẫu nhiên với chữ khác trên màn, mà vẫn ngắn hơn
/// một dòng của ô nhập.
pub fn still_in_box(screen: &str, typed: &str) -> bool {
    let squash = |s: &str| -> String {
        s.chars()
            .filter(|c| !c.is_whitespace() && !"│┃|>❯".contains(*c))
            .collect()
    };
    // ⚠ CHỈ soi trong Ô NHẬP, không soi cả màn.
    //
    // Đây là chỗ phép đo suýt trỏ sai: gửi đi RỒI thì `claude` in lại chính câu
    // ấy vào phần hội thoại phía trên — chữ vẫn còn trên màn, mà ý nghĩa ngược
    // hẳn. Soi cả màn thì hub đọc "đã gửi" thành "còn nằm trong ô", rồi bắn một
    // Enter thừa và báo sai cho chủ máy. Ô nhập là khối đóng khung cuối cùng.
    let screen = box_region(screen);
    let t = squash(typed);
    let n = t.chars().count();
    // Chữ quá ngắn ("2", "ok") không đủ đặc trưng: nó nằm sẵn trong mọi màn.
    // Thà bỏ sót một lần gửi Enter còn hơn bắn Enter vì một chữ trùng.
    if n < 6 {
        return false;
    }
    // 🔴 HAI ĐẦU, không riêng đuôi — 2026-08-16. Hà, ảnh chụp phiên `[games]`:
    // *"Sao lại có lệnh ở ô chờ trong tin thế này"*. Log cùng lúc:
    // `run_quick n=abdd2611` rồi `runin_ran code=0 ms=55824` — lệnh CHẠY XONG
    // trên máy; thứ kẹt là bước dán KẾT QUẢ vào phiên.
    //
    // Khối dán ấy nhiều dòng (`[hub chạy hộ]` · `$ <lệnh>` · đầu ra). Ô nhập
    // của TUI chỉ hiện được phần ĐẦU, còn dấu vân tay ở đây lấy 16 ký tự CUỐI —
    // nên phép đo đọc ra "chữ đã rời ô", `type_and_send` không bấm Enter, và cả
    // khối nằm lại. Một phép đo đúng cho câu ngắn, mù với khối dài, và nó mù
    // đúng về phía im lặng.
    let tail: String = t.chars().skip(n.saturating_sub(16)).collect();
    let head: String = t.chars().take(16).collect();
    let seen = squash(&screen);
    // 🔴 KHỐI DÁN KHÔNG CÒN LÀ CHỮ CỦA CHÍNH NÓ — 2026-08-16. Hà, ảnh chụp phiên
    // `[mailler]`: *"Chạy lệnh xong dán vào ô chat không gửi đi"* · *"Thiếu
    // enter"*. Trên màn, ô nhập hiện `[Pasted text #4 +3 lines][Pasted text #5]`
    // — TUI của `claude` RÚT GỌN mọi khối dán nhiều dòng thành một cái nhãn, nên
    // cả 16 ký tự đầu lẫn 16 ký tự cuối của khối đều không có mặt để mà tìm.
    //
    // Phép đo vì thế đọc ra "chữ đã rời ô", `type_and_send` không bấm Enter
    // rời, và cả kết quả lệnh nằm lại trong ô — im lặng, đúng cái hình dạng bản
    // vá "hai đầu" ở trên vừa mới sửa cho một ca khác cùng ngày.
    //
    // Chỉ áp cho khối NHIỀU DÒNG: đó đúng là thứ TUI rút gọn, và giới hạn ấy
    // giữ cho một câu một dòng không bao giờ ăn nhầm cái nhãn của lượt trước.
    if typed.contains('\n') && seen.contains("[Pastedtext") {
        return true;
    }
    seen.contains(&tail) || seen.contains(&head)
}

/// Phân loại thuần từ chữ trên màn, để test được không cần Terminal.
pub fn landed(screen: &str, typed: &str) -> Landed {
    // Chính `claude` in dòng này khi có tin trong hàng chờ (đo trên máy:
    // "Press up to edit queued messages").
    if screen.contains("queued message") {
        return Landed::Queued;
    }
    // 🔴 HỎI Ô NHẬP TRƯỚC KHI HỎI ĐỒNG HỒ, và thứ tự này là cả bản vá.
    //
    // Một phiên có thể VỪA bận VỪA còn chữ trong ô (nó đang chạy lượt trước,
    // chữ mới chưa đi). Hỏi `is_busy` trước thì ca ấy đọc thành `Running` —
    // nghe như "chữ đã khởi động một lượt", đúng câu SAI đã gửi cho Hà.
    if still_in_box(screen, typed) {
        return Landed::InBox;
    }
    if is_busy(screen) {
        return Landed::Running;
    }
    Landed::Idle
}

/// Mở một cửa sổ Terminal MỚI và chạy `cmd` trong đó; trả về `(id cửa sổ, tty)`.
///
/// Vì sao hub cần biết mở cửa sổ (Hà 2026-08-11: *"cli claude cài trên máy tôi,
/// hub là cầu kết nối ra ui"*): một phiên `--bg` là hạng phiên **chủ máy không
/// bao giờ tự tạo ra** khi ngồi trước máy — không cửa sổ, không màn sống, muốn
/// nói chen vào phải dừng nó trước. Cầu nối thì phải bắc sang đúng thứ có thật
/// ở đầu bên kia, nên `/new` mở đúng cái người ta sẽ mở: một cửa sổ.
///
/// `tty` lấy NGAY sau khi dựng, vì đó là thứ duy nhất ghép được cửa sổ này với
/// hàng mà `claude agents` sắp khai ra — tên phiên thì `claude` tự đặt, và id
/// thì chưa tồn tại lúc này.
pub fn open_window(cmd: &str) -> Result<(i64, String)> {
    // 🔴 Bản cũ hỏi `id of window 1` — **cửa sổ đang ở TRƯỚC, không phải cửa sổ
    // vừa mở**. `do script` trả về một TAB, và tab ấy có thể nằm ở cửa sổ nào
    // cũng được. Trả giá thật 2026-08-13 08:36, lượt tự đóng sổ của `[AI/tfl5]`:
    //
    //   ⚠ chưa mở được cửa sổ mới (osascript hỏng: execution error:
    //     Can't make id of window 1 of application "Terminal" into type text. (-1700))
    //
    // Hà thấy hậu quả trước tôi: *"mở phiên mới rồi mà cửa sổ cũ vẫn còn nguyên
    // trong cli là sao vậy?"* — vì `do script` **đã dựng xong cửa sổ** rồi mới
    // chết ở dòng đọc id. Kết cục: một cửa sổ mới mồ côi (hub không biết nó
    // tồn tại), cửa sổ cũ vẫn nguyên, một lượt fork đã tiêu, và phiên bị ghi
    // vào sổ "đã đóng" nên sẽ không bao giờ được giúp lại.
    //
    // Điều đáng học không phải "AppleScript khó tính": tác giả cũ ĐÃ lường
    // đúng hình dạng hỏng này — có hẳn một chốt ngay dưới, *"id chỉ để ghi sổ,
    // đừng cho nó làm hỏng cả việc"*. Nhưng chốt đặt ở phía Rust, trong khi lỗi
    // ném ra từ phía AppleScript, nên nó không bao giờ chạy tới. **Một cái chốt
    // đặt sau chỗ hỏng là một cái chốt không tồn tại.**
    //
    // Nay chỉ hỏi thứ BẮT BUỘC phải có — `tty` của chính tab vừa tạo — rồi lấy
    // id bằng `window_of`, đúng hàm đã dùng ở mọi chỗ khác. Hỏi ít đi một thứ,
    // và không còn chỗ nào cho `window 1` sai.
    let script = format!(
        r#"tell application "Terminal"
  set w to do script {}
  delay 1
  return tty of w
end tell"#,
        as_string(cmd)
    );
    let tty = osascript(&script)?.trim().to_string();
    if tty.is_empty() {
        return Err(anyhow!("Terminal mở cửa sổ nhưng không khai tty"));
    }
    // Id lấy sau, và hỏng thì thôi: cửa sổ đã dựng rồi, ném lỗi ở đây là bỏ lại
    // đúng một cửa sổ mồ côi — cái giá vừa trả hôm nay.
    let id = window_of(&tty).ok().flatten().unwrap_or(0);
    Ok((id, tty))
}

/// Cửa sổ Terminal đang chạy `tty` này, nếu có.
///
/// `Terminal` công bố `tty` của từng tab qua AppleScript (đo 2026-08-09:
/// `/dev/ttys005, /dev/ttys000, …`), và hub đã biết `tty` của từng phiên từ
/// `ps -o tty=`. Ghép hai đầu ấy lại là ra đúng cửa sổ của phiên.
pub fn window_of(tty: &str) -> Result<Option<i64>> {
    if tty.is_empty() || tty == "??" || tty == "-" {
        return Ok(None);
    }
    // `ps` in `ttys005`, AppleScript trả `/dev/ttys005`.
    let dev = if tty.starts_with("/dev/") {
        tty.to_string()
    } else {
        format!("/dev/{tty}")
    };
    let script = window_script(&dev);
    // Không tìm thấy thì script chạy hết mà không `return` gì — `osascript` in
    // ra chuỗi rỗng, `parse` hỏng, và ta được `None`. Đó là câu trả lời đúng.
    //
    // ⚠ Bản đầu có thêm dòng `return ""` ở cuối để nói rõ "không thấy". Trong
    // chuỗi thô `r#"…"#` của Rust, dấu `"` đầu là nội dung còn `"#` đóng chuỗi,
    // nên AppleScript nhận được `return "` treo lửng và hỏng ngay ở dòng đầu:
    // *"Expected string but found end of script. (-2741)"*. Một dòng thừa viết
    // cho dễ hiểu lại làm hỏng cả tính năng — và nó chỉ lộ khi CHẠY THẬT, vì
    // test của tệp này chỉ kiểm phần thoát chuỗi.
    match osascript(&script) {
        Ok(out) => {
            let id = out.trim().parse::<i64>().ok();
            if let Some(w) = id {
                remember_window(&dev, w);
            }
            Ok(id)
        }
        // 🔴 Hà 2026-08-14, gõ một câu vào phiên và nhận `⚠ không tìm được cửa
        // sổ: osascript quá 20s`. Cửa sổ ấy vẫn nằm đó — thứ hỏng là phép HỎI:
        // máy đang tải nặng thì một lời gọi AppleScript vượt trần 20 giây.
        //
        // Mà id cửa sổ là thứ KHÔNG ĐỔI suốt đời cửa sổ, còn `window_of` thì bị
        // gọi ở mọi `/type`, `/key`, `/shot` — tức hub hỏi lại Terminal hàng
        // chục lần một điều không đổi, đúng lúc Terminal đang bận nhất. Nhớ lấy
        // câu trả lời cũ và dùng khi phép hỏi hết giờ: sai lầm tệ nhất của bản
        // nhớ (cửa sổ đã đóng) chỉ dẫn tới một `do script` hỏng có thông báo,
        // còn hỏng như hiện nay là mất hẳn đường gõ.
        Err(e) => match recall_window(&dev) {
            Some(w) => {
                logging::warn(
                    "window_of_from_cache",
                    json!({ "tty": dev, "window": w, "err": e.to_string(),
                            "why": "hỏi Terminal hết giờ — dùng id cửa sổ đã nhớ" }),
                );
                Ok(Some(w))
            }
            None => Err(e),
        },
    }
}

/// Cửa sổ mang tty này, **KỂ CẢ tab đã chết** (shell thoát, `[Process
/// completed]`, 0 tiến trình).
///
/// 🔴 Hà 2026-08-17: *"Ko còn sao vẫn liệt kê, hay nó ở tab con"*. Anh bấm ◻ ở
/// một hàng `/terminal` và nhận `⚠ không còn cửa sổ terminal nào chạy ttys014`
/// — trong khi cửa sổ ấy đang nằm ngay đó. Đo ra ngay: `tabs_script` (thứ DỰNG
/// danh sách) đọc **mọi** tab, còn [`window_of`] (thứ THI HÀNH) lọc
/// `count of processes > 0`. Hai bộ liệt kê, một câu hỏi, hai câu trả lời — nên
/// hub vẽ ra một cái nút trỏ vào chỗ chính nó nói là không tồn tại. Cùng hình
/// dạng với lỗi "lệnh in hai biến thể" cùng ngày.
///
/// Cái lọc ấy KHÔNG sai và không được gỡ: tab chết vẫn khai tty cũ, macOS thì
/// dùng lại số tty, nên gõ chữ theo tty trần là gõ vào một cái xác (đo
/// 2026-08-11, ba cửa sổ cùng khai `/dev/ttys005`). Nhưng ĐÓNG thì ngược lại:
/// cái xác chính là thứ cần đóng. Nên tách hai đường, và chỉ đường ĐÓNG được
/// nhìn thấy tab chết.
///
/// Thứ tự ưu tiên giữ nguyên như [`window_of`] — tab đang chạy chương trình,
/// rồi tab còn sống — và chỉ khi không còn gì sống mới nhận tab chết.
pub fn window_of_any(tty: &str) -> Result<Option<i64>> {
    if tty.is_empty() || tty == "??" || tty == "-" {
        return Ok(None);
    }
    let dev = if tty.starts_with("/dev/") {
        tty.to_string()
    } else {
        format!("/dev/{tty}")
    };
    Ok(osascript(&window_any_script(&dev))?
        .trim()
        .parse::<i64>()
        .ok())
}

/// Tách ra để KIỂM ĐƯỢC — cùng lý do với [`window_script`]: ba lỗi AppleScript
/// đắt nhất của tệp này đều nằm trong một chuỗi mà không bài kiểm nào chạm tới.
fn window_any_script(dev: &str) -> String {
    format!(
        r#"tell application "Terminal"
  set alive to missing value
  set dead to missing value
  repeat with w in every window
    try
      repeat with t in tabs of w
        if tty of t is {} then
          if (count of (processes of t)) > 0 then
            if busy of t then return id of w
            if alive is missing value then set alive to id of w
          else
            if dead is missing value then set dead to id of w
          end if
        end if
      end repeat
    end try
  end repeat
  if alive is not missing value then return alive
  if dead is not missing value then return dead
end tell"#,
        as_string(dev)
    )
}

/// Tab của cửa sổ ấy còn tiến trình nào không — `0` nghĩa là shell đã thoát.
///
/// Dùng để biết có gì để `exit` hay không: gõ `exit` vào một tab `[Process
/// completed]` là gõ vào chỗ không ai đọc, rồi hub ngồi chờ một cú thoát không
/// bao giờ tới.
pub fn tab_proc_count(window: i64) -> Result<usize> {
    let out = osascript(&format!(
        r#"tell application "Terminal"
  try
    return (count of processes of selected tab of window id {window}) as text
  on error
    return "0"
  end try
end tell"#
    ))?;
    Ok(out.trim().parse::<usize>().unwrap_or(0))
}

/// Sổ nhớ `tty → id cửa sổ`. Bé, và cố ý không có hạn dùng: id chỉ đổi khi cửa
/// sổ đóng, và lúc ấy `do script` hỏng ra lỗi rõ ràng chứ không im.
static WINDOW_CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, i64>>> =
    std::sync::OnceLock::new();

fn remember_window(dev: &str, w: i64) {
    let m = WINDOW_CACHE.get_or_init(Default::default);
    if let Ok(mut g) = m.lock() {
        g.insert(dev.to_string(), w);
    }
}

fn recall_window(dev: &str) -> Option<i64> {
    let m = WINDOW_CACHE.get_or_init(Default::default);
    let g = m.lock().ok()?;
    g.get(dev).copied()
}

/// MỌI tty mà Terminal.app đang giữ — một lời gọi cho cả danh sách.
///
/// Đây là câu trả lời cho "hub gõ vào phiên nào được": `type_into` đi qua
/// `do script` của Terminal, nên phiên nào Terminal không giữ thì hub không có
/// tay nào chạm tới — dù `ps` khai nó có tty đàng hoàng. Phiên trong terminal
/// tích hợp của VS Code (hay iTerm, hay tmux tách rời) rơi đúng vào ca đó.
///
/// Vì sao hỏi CẢ danh sách thay vì hỏi từng phiên: một vòng có 5-15 phiên, mà
/// mỗi `osascript` mất ~50-150ms. Hỏi từng phiên là con đường đã một lần kéo
/// một vòng từ ~18 giây lên 90 giây (đọc màn cho mọi phiên, 2026-08-10); hỏi
/// một lần rồi tra trong tập thì thêm đúng một lời gọi cho cả vòng.
/// Một tab Terminal, như chính Terminal khai ra.
#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    /// `ttys004`, đã bỏ `/dev/`.
    pub tty: String,
    /// Terminal tự trả lời "tab này còn chương trình nào đang chạy không".
    pub busy: bool,
    /// Tên các tiến trình trong tab, thường là `login`, `-zsh`, rồi CLI.
    pub procs: Vec<String>,
    /// CHỮ đang hiện trên màn của chính tab này — và `None` KHÔNG phải màn trống.
    ///
    /// Hai kết cục khác nhau, nên hai giá trị khác nhau (cùng luật với `Look`):
    /// `None` = lượt dò này **không xin** chữ (`terminal_tabs`), `Some("")` =
    /// xin rồi và màn thật sự trống. Gộp chúng vào một chuỗi rỗng là dựng đúng
    /// cái bẫy `screen_of → None` đã trả giá ở luật 13.
    pub screen: Option<String>,
}

impl Tab {
    /// Tab này có đang chạy một CLI trợ lý không — hay chỉ là một dấu nhắc?
    ///
    /// Đo bằng DANH SÁCH TIẾN TRÌNH chứ không bằng `busy`: `busy` là true cho
    /// cả một `git log` đang mở pager, và false cho một `claude` vừa trả lời
    /// xong. Hai câu hỏi khác nhau, và chỉ câu này quyết định tab ấy có phải
    /// một phiên trợ lý hay không.
    ///
    /// Shell thì bỏ qua: `login`, `-zsh`, `zsh`, `bash`, `-bash`, `sh`. Còn lại
    /// gì thì đó là thứ đang chạy.
    pub fn cli(&self) -> Option<&str> {
        self.procs
            .iter()
            .map(|p| p.trim_start_matches('-'))
            .find(|p| !matches!(*p, "login" | "zsh" | "bash" | "sh" | "tcsh" | "fish" | ""))
            .map(|p| p.trim())
    }
}

/// Mọi tab Terminal đang mở, kèm thứ đang chạy trong đó.
///
/// 🔴 Hà 2026-08-13: *"mỗi cửa sổ terminal là một phiên thì sẽ quản lý được
/// phiên nào đang chạy cli phiên nào không"* · *"vào phiên (terminal) chưa chạy
/// gì → gõ lệnh bình thường như đang gõ ở terminal là được rồi"*.
///
/// Đây là phép đo mà mô hình ấy đứng lên: cho tới nay hub đi từ `claude agents`
/// rồi mới tìm cửa sổ (`window_of` theo tty), nên một cửa sổ **không chạy CLI**
/// là thứ hub không có cách nào biết là có tồn tại. Ngồi trước máy thì nó nằm
/// ngay đó, mở sẵn, gõ được — tức đúng định nghĩa một LỖ HỔNG của cây cầu.
///
/// Trả cả tab lẫn tiến trình trong MỘT lượt `osascript`: hỏi hai lần là hai ảnh
/// chụp lệch nhau, và giữa hai lần ấy một cửa sổ đóng được.
pub fn terminal_tabs() -> Result<Vec<Tab>> {
    probe_tabs(false)
}

/// Như `terminal_tabs`, và kèm CHỮ đang hiện trên màn từng tab — vẫn MỘT lượt dò.
///
/// 🔴 Vì sao gộp vào một lượt (đo 2026-08-16, 11 phiên trên máy này). Ảnh chụp
/// cũ hỏi Terminal **hai lần cho mỗi phiên đang chạy** — `window_of` rồi
/// `screen_text` bên trong `look` — cộng hai lượt nữa cho cả vòng
/// (`terminal_tabs` + `terminal_ttys`). Đo tay: mỗi lượt `osascript` 0,6–1,3
/// giây, nên một ảnh chụp có 7 phiên bận tốn **7–14 giây**, đúng con số Hà đợi
/// khi bấm một nút. Một lượt duy nhất lấy CẢ danh sách tab lẫn chữ 11 màn:
/// **1,3 giây**, và nó không phình theo số phiên bận nữa.
///
/// Và nó ĐÚNG HƠN, không chỉ nhanh hơn: `screen_text` đọc *"contents of selected
/// tab of window id N"* — tức tab ĐANG ĐƯỢC CHỌN của cửa sổ ấy, không phải tab
/// mang tty mình hỏi. Cửa sổ hai tab thì nó đọc nhầm màn, im lặng. Ở đây chữ đi
/// kèm chính cái tab đã khai tty, nên không còn chỗ cho nhầm lẫn ấy.
pub fn terminal_screens() -> Result<Vec<Tab>> {
    probe_tabs(true)
}

fn probe_tabs(with_screens: bool) -> Result<Vec<Tab>> {
    let out = osascript(&tabs_script(with_screens))?;
    let (tabs, skipped) = parse_tabs(&out, with_screens);
    // Bỏ qua một cửa sổ là chuyện thường (bảng cài đặt của Terminal cũng nằm
    // trong `every window`). Bỏ qua mà KHÔNG đọc được cái nào thì không —
    // đó đúng hình dạng con bug vừa vá, và nó phải kêu để lần sau khỏi mất
    // hai ngày.
    if skipped > 0 {
        if tabs.is_empty() {
            logging::error(
                "terminal_tabs_all_skipped",
                json!({ "skipped": skipped,
                        "why": "mọi cửa sổ đều ném lỗi — danh sách rỗng KHÔNG có nghĩa là máy không có cửa sổ nào" }),
            );
        } else {
            logging::info(
                "terminal_tabs_skipped",
                json!({ "skipped": skipped, "read": tabs.len() }),
            );
        }
    }
    Ok(tabs)
}

/// Đoạn AppleScript hỏi mọi tab — kèm chữ trên màn khi `with_screens`.
///
/// Tách thuần để KIỂM ĐƯỢC, cùng lý do với `window_script`.
fn tabs_script(with_screens: bool) -> String {
    // 🔴 BA LỖI TRONG BỐN DÒNG APPLESCRIPT, và cả ba đều CÂM.
    // 08-15. Hà: *"lệnh terminal chưa đúng … đang có 2 cửa sổ không chạy gì"*.
    // Đo bằng tay đúng lúc ấy: 6 tab, 2 trần (`ttys000`, `ttys002`) — còn hàm
    // này trả về **danh sách RỖNG**, `Ok(vec![])`, không một tiếng nào.
    //
    // 1. **`tab` bên trong `tell application "Terminal"` KHÔNG phải ký tự tab.**
    //    Terminal có hẳn một lớp tên `tab`, và tên lớp thắng hằng số của
    //    AppleScript. Nên `… & tab & …` là nối một *class specifier* vào chuỗi
    //    ⟹ ném lỗi. Nay `ASCII character 9`, đặt vào biến, không còn chỗ cho
    //    cái tên ấy va nhau.
    // 2. **`(p as string)` trên một phần tử `processes` cũng ném lỗi**
    //    (`Can't make item 1 of «class prcs» … into type string`). Đúng cách là
    //    coerce CẢ DANH SÁCH kèm `text item delimiters` — và nó vá luôn cái bẫy
    //    mà chú thích cũ đã cảnh báo: `processes of t as string` dán liền tên
    //    tiến trình (`login-zshclaude`), đọc ra một tên không có thật.
    //
    // 3. **`contents of t` KHÔNG đọc ra chữ trên màn** (đo 2026-08-16, cả 11
    //    tab cùng ném *"Can't make «class ttab» 1 of window id … into type
    //    text"*). Vì `t` là một THAM CHIẾU, mà `contents of <tham chiếu>` là
    //    toán tử giải-tham-chiếu của AppleScript — nó trả về chính cái tab, chứ
    //    không phải thuộc tính `contents` của lớp tab. Cùng một họ bẫy với mục
    //    1: ở đó tên lớp thắng hằng số, ở đây toán tử thắng thuộc tính. Đường
    //    đi được là địa chỉ đầy đủ: `contents of tab k of window id wid`.
    //
    // 📌 Bài học đắt hơn cả ba lỗi: **một `try` không có `on error` là một cái
    // máy nuốt lỗi**. Nó dựng lên đúng lý do — có một "cửa sổ" trong danh sách
    // không phải cửa sổ thật, và `every tab of` nó ném `-1728` (đo được: item
    // 4). Nhưng vì lỗi ở mục 1 xảy ra với MỌI cửa sổ, cái `try` ấy nuốt sạch
    // rồi trả về một danh sách rỗng — và "không có cửa sổ nào" là một câu trả
    // lời hoàn toàn hợp lý, nên không ai nghi ngờ nó. Cùng họ với
    // `screen_of → None` gộp ba kết cục (luật 13).
    //
    // Nay đếm số cửa sổ bỏ qua và ghi log; bỏ qua HẾT mà vẫn có cửa sổ thì đó
    // là một sự cố, không phải một cái máy rảnh.
    //
    // 🔴 KHUNG BẢN TIN: mỗi tab là một dòng đầu `tty⇥busy⇥procs⇥số-dòng-màn`,
    // rồi ĐÚNG bấy nhiêu dòng chữ màn. Đếm dòng chứ không cắt theo dấu phân
    // cách, vì chữ trên màn là chữ của người khác: bất cứ dấu nào tôi chọn làm
    // ranh giới đều có thể đang nằm sẵn trên một màn nào đó, và hôm nó nằm đó
    // thì phép đọc lệch mà không ai biết. Số dòng thì không giả được — đã đối
    // chiếu trên bản chụp thật 11 tab / 304 dòng, mọi dòng đầu rơi đúng chỗ.
    let screens = if with_screens {
        r#"
        set c to contents of tab k of window id wid
        set np to (count of paragraphs of c)"#
    } else {
        r#"
        set c to ""
        set np to 0"#
    };
    let body = if with_screens {
        "\n        set acc to acc & c & linefeed"
    } else {
        ""
    };
    format!(
        r##"tell application "Terminal"
  set TAB9 to ASCII character 9
  set acc to ""
  set skipped to 0
  repeat with w in every window
    try
      if visible of w then
      set wid to id of w
      repeat with k from 1 to (count of tabs of w)
        set tb to tab k of window id wid
        set AppleScript's text item delimiters to "|"
        set ps to (processes of tb) as text
        set AppleScript's text item delimiters to ""{screens}
        set acc to acc & (tty of tb) & TAB9 & (busy of tb) & TAB9 & ps & TAB9 & np & linefeed{body}
      end repeat
      end if
    on error
      set skipped to skipped + 1
    end try
  end repeat
  return acc & "#skipped" & TAB9 & skipped & linefeed
end tell"##
    )
}

/// Đọc kết quả AppleScript → `(danh sách tab, số cửa sổ bị bỏ qua)`.
///
/// Tách thuần để KIỂM ĐƯỢC: ba lỗi AppleScript ở trên nằm gọn trong một chuỗi
/// mà không một bài kiểm nào chạm tới — vì cả hàm đòi một Terminal thật. Bản
/// chép ở `tests/` là kết quả THẬT đo trên máy này.
///
/// `with_screens` phải khớp với lượt dò đã sinh ra `out`: nó quyết định
/// `screen` là `None` (không xin) hay `Some("")` (xin rồi, màn trống).
pub fn parse_tabs(out: &str, with_screens: bool) -> (Vec<Tab>, usize) {
    let mut skipped = 0usize;
    let mut tabs: Vec<Tab> = Vec::new();
    let mut lines = out.lines();
    while let Some(l) = lines.next() {
        let mut f = l.split('\t');
        let tty = f.next().unwrap_or_default().trim();
        if tty == "#skipped" {
            skipped = f.next().unwrap_or("0").trim().parse().unwrap_or(0);
            continue;
        }
        if tty.is_empty() {
            continue;
        }
        let busy = f.next().unwrap_or("false").trim() == "true";
        let procs = f
            .next()
            .unwrap_or_default()
            .split('|')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        // Số dòng màn là KHUNG của bản tin, không phải một trường tuỳ ý: đọc
        // hỏng nó thì mọi dòng sau đọc lệch. Nên nó không được im — một khung
        // sai đọc ra một danh sách tab trông vẫn hợp lý.
        let frame = f.next().unwrap_or_default().trim();
        let n: usize = match frame.parse() {
            Ok(n) => n,
            Err(e) => {
                logging::warn(
                    "terminal_tab_frame_unreadable",
                    json!({ "tty": tty, "frame": frame, "err": e.to_string(),
                            "effect": "không đọc được số dòng màn — coi như tab không có chữ, các tab sau có thể lệch khung" }),
                );
                0
            }
        };
        let mut screen: Vec<&str> = Vec::with_capacity(n);
        for i in 0..n {
            match lines.next() {
                Some(x) => screen.push(x),
                None => {
                    logging::warn(
                        "terminal_tab_screen_truncated",
                        json!({ "tty": tty, "want": n, "got": i,
                                "effect": "bản tin cụt — màn của tab này đọc thiếu dòng" }),
                    );
                    break;
                }
            }
        }
        tabs.push(Tab {
            tty: tty.trim_start_matches("/dev/").to_string(),
            busy,
            procs,
            screen: with_screens.then(|| screen.join("\n")),
        });
    }
    (tabs, skipped)
}

/// Tab ĐANG SỐNG mang tty ấy — `None` nghĩa là hub không có tay nào chạm tới.
///
/// Đây là bản tra-trong-tập của `window_script`, và nó phải mang NGUYÊN luật ấy
/// sang, không phải một phép so tty đơn giản: Terminal giữ lại `tty` của tab đã
/// chết, còn macOS thì DÙNG LẠI số tty — nên "khớp tty" trả về một cái xác dễ
/// như trả về tab thật. Lọc theo tab còn tiến trình, và trong đám còn sống thì
/// ưu tiên tab đang chạy chương trình (`busy`), y hệt bản AppleScript.
///
/// 🪦 `terminal_ttys()` — một lượt `osascript` thứ hai chỉ để hỏi lại đúng cái
/// tập mà `terminal_tabs` vừa trả về (0,6 giây mỗi ảnh chụp, mọi ảnh chụp). Gỡ
/// 2026-08-16: hai phép đo về cùng một thứ, hỏi hai lần, là hai câu trả lời có
/// thể lệch nhau — và giữa hai lượt hỏi thì một cửa sổ đóng được.
pub fn alive_tab<'a>(tabs: &'a [Tab], tty: &str) -> Option<&'a Tab> {
    let dev = tty.trim_start_matches("/dev/");
    let mut alive = None;
    for t in tabs.iter().filter(|t| t.tty == dev && !t.procs.is_empty()) {
        if t.busy {
            return Some(t);
        }
        if alive.is_none() {
            alive = Some(t);
        }
    }
    alive
}

/// Tab của cửa sổ này còn chương trình nào đang chạy không — **và cửa sổ ấy còn
/// không**. Ba câu trả lời, vì đó là ba sự thật khác nhau về thế giới.
///
/// Đây là câu hỏi PHÂN BIỆT "thoát CLI xong" với "vẫn đang thoát": `ps` biến
/// mất trước khi shell kịp in dấu nhắc, còn `busy` là chính Terminal trả lời về
/// tab của nó. Dùng nó để CHỜ, chứ đừng đoán bằng `sleep`.
///
/// 🔴 Bản cũ là `tab_busy -> Result<bool>`, và nó gộp **"cửa sổ không còn"** vào
/// nhánh `Err` — cùng một họ lỗi với `keys::look` gộp ba kết cục vào `None`, và
/// lần này cái giá đo được nằm nguyên trong log: **190 dòng
/// `close_check_failed` trong 5 tiếng** (`~/Library/Logs/hubd.err`,
/// 08:44:50Z → 13:47:06Z), tất cả về đúng một cửa sổ 2131 của phiên
/// `win-ttys002`, tất cả cùng một câu:
/// *"Can't make «class busy» of «class tcnt» of window id 2131 … into type
/// text. (-1700)"*. Cửa sổ ấy đã đóng từ lâu; `selected tab` của một cửa sổ
/// không còn tab trả về `missing value`, ép sang chữ thì -1700, và
/// `close_pending_tick` đọc `Err` đúng như luật của nó — *"hỏi không được là hub
/// mù, không phải cửa sổ đã đóng"* — nên giữ nguyên mục trong sổ, hỏi lại sau 30
/// giây, mãi mãi. Luật ấy KHÔNG sai; cái sai là bắt nó phán trên một phép đo
/// không biết nói "không còn".
///
/// Nên câu hỏi "còn tab nào không" phải đứng TRƯỚC, trong cùng một lượt hỏi
/// (hai lượt là hai câu trả lời có thể lệch nhau — bài học của `terminal_ttys`).
/// `on error` ở đây chỉ bắt được lỗi PHÂN GIẢI ĐỐI TƯỢNG của Terminal, tức
/// "không có cửa sổ ấy"; còn `osascript` chết, Terminal câm, quyền bị rút thì
/// vẫn về `Err` như cũ — hub vẫn mù đúng chỗ đáng mù.
///
/// Đo thật trước khi tin, 2026-08-17 (`osascript` gọi tay, bốn hình dạng cửa sổ):
/// 2221 cửa sổ đang chạy `claude` → `true` · 2131 cửa sổ trong sổ chờ đóng →
/// `gone` · 2158 cửa sổ đã ẩn còn tab chết → `false` · 2122 cửa sổ 0 tab →
/// `gone` · 99999 id không tồn tại → `gone`.
pub fn tab_state(window: i64) -> Result<TabState> {
    let out = osascript(&format!(
        r#"tell application "Terminal"
  try
    if (count of tabs of window id {window}) is 0 then return "gone"
    return (busy of (selected tab of window id {window})) as text
  on error
    return "gone"
  end try
end tell"#
    ))?;
    match out.trim() {
        "true" => Ok(TabState::Busy),
        "false" => Ok(TabState::Idle),
        "gone" => Ok(TabState::Gone),
        // Không đoán. Một câu trả lời lạ là hub mù, và mù thì phải nói là mù —
        // đoán bừa "rảnh" ở đây là đóng nhầm một cửa sổ đang chạy dở.
        other => anyhow::bail!(
            "Terminal trả lời lạ cho câu hỏi tab của cửa sổ {window} còn bận không: {:?}",
            crate::exec::truncate(other, 80)
        ),
    }
}

/// Ba kết cục của một lượt hỏi [`tab_state`] — xem chú thích của hàm ấy để biết
/// vì sao kết cục thứ ba phải là một GIÁ TRỊ chứ không phải một lỗi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabState {
    /// Tab còn chương trình đang chạy: chưa thoát xong.
    Busy,
    /// Tab đứng ở dấu nhắc: đóng được.
    Idle,
    /// Không còn cửa sổ ấy (hoặc nó không còn tab nào) — việc đã XONG, dù không
    /// phải hub làm.
    Gone,
}

/// Thoát CLI rồi ĐÓNG cửa sổ — định nghĩa "tắt hẳn" của chủ máy.
///
/// Hà 2026-08-11: *"tắt hẳn là thoát cli và đóng terminal"*, và trước đó:
/// *"phải thoát cli trước rồi đóng thì mới không bị hỏi chứ"*. Thứ tự ấy không
/// phải phép lịch sự: đóng một cửa sổ còn chương trình chạy sẽ bật hộp thoại
/// *"Do you want to terminate running processes?"* — một modal của Terminal,
/// và modal thì **khoá luôn mọi lệnh automation sau đó**. Sai thứ tự là tự bịt
/// mồm mình.
///
/// Hai sự thật đo được ngày 2026-08-11, mỗi cái bác một cách làm tắt:
/// · gõ `/exit` là CLI thoát thật (pid biến mất, `busy` về `false`) — nên không
///   cần `kill`, và `kill` thì mất phần ghi sổ cuối phiên;
/// · cửa sổ **không tự đóng** khi shell thoát (hồ sơ Terminal giữ nó lại kèm
///   dòng `[Process completed]`) — nên bước đóng là bắt buộc, không thừa.
/// Gõ `/exit` vào cửa sổ, và CHỈ thế — không chờ, không đóng.
///
/// 🔴 Hà 2026-08-13: *"30 giây kiểm tra 1 lần nếu chưa xong thì chờ tiếp"*.
/// `quit_and_close` bên dưới chờ tại chỗ rồi **bỏ cuộc sau 30 giây**, và bỏ
/// cuộc là câu trả lời sai cho câu hỏi thật: một lượt `claude` chạy hai mươi
/// phút thì cửa sổ ấy vẫn phải đóng, chỉ là muộn hơn. Nhưng chờ tại chỗ lâu
/// hơn thì hỏng kiểu khác — `execute_commands` giữ `CMD_LOCK`, nên một lượt
/// chờ dài **khoá cả vòng chạy**: không tin báo, không lệnh nào khác đi được.
///
/// Nên tách đôi: gõ `/exit` xong thì ghi sổ và trả lời ngay; việc canh chừng
/// giao cho vòng chạy (`pipeline::close_pending_tick`), đúng cùng cỗ máy "so
/// hai lượt" mà `watch.rs` đã dùng. Chờ bao lâu cũng được vì không ai phải ngồi
/// giữ chỗ.
/// 🔴 `/exit` phải đi bằng ĐÚNG đường mọi chữ khác đi — Hà 2026-08-15:
/// *"ở phiên tfl5 có thấy lệnh exit nào đâu"*.
///
/// Anh nhìn nhầm cửa sổ (hub nhắm `ttys004`, không phải phiên tfl5), nhưng câu
/// hỏi ấy lôi ra một lỗi thật, và nó là **luật 13 bị bỏ sót đúng một chỗ**: bản
/// cũ ở đây gọi thẳng `osascript(do_script(w, "/exit"))`. `do script` đẩy chữ
/// và dấu xuống dòng trong CÙNG một lượt ghi ⟹ TUI của `claude` đọc cả cụm như
/// một cú DÁN và **nuốt dấu xuống dòng** ⟹ `/exit` nằm lại trong ô nhập, chưa
/// bao giờ được gửi. Đó đúng là con bug đã trả giá cả tối 12/08 cho `/type`,
/// đã có sẵn thuốc (`type_and_send`: cú Enter RỜI), mà đường đóng phiên thì
/// không ai nối vào.
///
/// Và cái làm nó sống lâu: đường này **không ghi một dòng log nào**, nên trong
/// sổ không có gì để đối chứng — chỗ gọi cứ thế viết *"đã gõ /exit"* như một sự
/// thật đã quan sát, trong khi tất cả những gì nó biết là `osascript` trả 0.
/// `osascript` trả 0 chỉ chứng minh **bytes đã tới tab** (CLAUDE.md, luật 13).
pub fn send_exit(window: i64) -> Result<()> {
    type_and_send(window, "/exit")?;
    crate::logging::info(
        "keys_exit_sent",
        serde_json::json!({ "window": window,
                            "why": "đã đẩy `/exit` + Enter rời; osascript trả 0 chỉ nói bytes tới tab, KHÔNG nói phiên đã nhận" }),
    );
    Ok(())
}

/// Đưa cửa sổ của phiên ra TRƯỚC rồi chụp ảnh màn hình thật ra `path`.
///
/// 🔴 Hà 2026-08-17: *"Thêm lệnh chụp ảnh màn hình để tôi xem thực sự đang có gì
/// trên màn hình"*, rồi ngay sau đó: *"Focus tới phiên thật"*. Câu thứ hai là cả
/// thiết kế của hàm này — một tấm ảnh chụp bừa cả màn hình chỉ nói được "máy
/// đang mở gì đó", còn thứ anh hỏi là *phiên ẤY* đang hiện gì.
///
/// Đưa ra trước rồi chụp CẢ màn (không cắt riêng cửa sổ): thứ đè lên cửa sổ —
/// hộp thoại của macOS, một cửa sổ khác, thanh thông báo — chính là cái mà chữ
/// đọc từ tab không bao giờ thấy, và cũng chính là thứ hay làm phiên đứng im.
///
/// ⚠ `screencapture` đòi quyền **Screen Recording**, và trên máy này nó đang bị
/// từ chối (đo 2026-08-17: *"could not create image from display"*, exit 1).
/// Nên hàm này KHÔNG được nuốt lỗi: nó nói đúng cái thiếu và chỗ cấp.
/// Bao nhiêu tin đang nằm trong HÀNG CHỜ của TUI — đọc từ chính màn hình.
///
/// 🔴 Hà 2026-08-18: *"Thêm lệnh clean xóa hết ở chờ"* (và chốt sau đó: hàng chờ
/// của PHIÊN, không phải tin Telegram). Đo trên màn thật cùng ngày, gõ hai dòng
/// vào một phiên đang bận:
///
/// ```text
///   ❯ (bo qua - dong do hang cho cua hub A)
///   ❯ (bo qua - dong do hang cho cua hub B)
/// ──────────────────────────────────────────
/// ❯ Press up to edit queued messages
/// ──────────────────────────────────────────
///   ⏵⏵ auto mode on · 2 shells · esc to interrupt · ↓ to manage
/// ```
///
/// Hai hình dạng, và phải dùng CẢ HAI: dòng quảng cáo `queued message` nói
/// *có hay không*, còn các dòng `❯` **thụt lề** nói *bao nhiêu*. Dấu nhắc của ô
/// nhập cũng bắt đầu bằng `❯` nhưng KHÔNG thụt lề — nhầm hai cái là đếm luôn cả
/// ô nhập, tức một hàng chờ rỗng đọc ra "còn 1" và `/clean` quay mãi.
pub fn queued_count(screen: &str) -> usize {
    // 🔴 HỎI Ở ĐÁY MÀN, KHÔNG QUÉT CẢ MÀN. Dòng quảng cáo `Press up to edit
    // queued messages` là thứ TUI vẽ TRONG khung ô nhập — nhưng chính chữ ấy
    // cũng nằm rải rác trong phần hội thoại của một phiên đang BÀN về cơ chế
    // hàng chờ (phiên `[hub]` nói về nó cả ngày, và đoạn văn ấy cuộn trên màn
    // hàng giờ sau đó).
    //
    // Quét cả màn thì `/clean` đọc ra "có hàng chờ" ở một phiên chẳng có gì để
    // dọn, rồi gửi `↑` — và `↑` khi hàng chờ rỗng KHÔNG phải là không làm gì:
    // nó kéo câu CŨ trong lịch sử vào ô nhập, tức hub tự chèn chữ vào chỗ chủ
    // máy đang gõ.
    if !box_region(screen).contains("queued message") {
        return 0;
    }
    screen
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            l.starts_with(' ') && t.starts_with('❯') && t.len() > 1
        })
        .count()
}

/// Màn hình có ĐANG KHOÁ không — `Some(true)` = đang ở màn hình đăng nhập.
///
/// 🔴 Hà 2026-08-18, ngay sau khi tôi khai ảnh đen là "gần như luôn là thiếu
/// quyền": *"Máy đang ở màn hình chờ đăng nhập không chụp được ảnh?"* — và câu
/// ấy đúng. Đo cùng lúc: `ioreg -n Root -d1` trả
/// `"CGSSessionScreenIsLocked"=Yes` kèm `CGSSessionScreenLockedTime`. Hai
/// nguyên nhân cho CÙNG một tấm ảnh đen (màn khoá · chưa cấp Screen Recording),
/// nên một câu trả lời đoán bừa một trong hai là bắt chủ máy đi kiểm hộ.
///
/// `None` = không đo được (không có `ioreg`, hoặc nó đổi định dạng). Không đo
/// được phải khác "đã đo, không khoá" — đúng luật 3.
pub fn screen_locked() -> Option<bool> {
    let out = crate::exec::run(
        "ioreg",
        &["-n", "Root", "-d1"],
        crate::exec::RunOpts {
            timeout: Some(std::time::Duration::from_secs(10)),
            ..Default::default()
        },
    )
    .ok()?;
    if out.code != Some(0) {
        logging::warn(
            "screen_lock_probe_failed",
            json!({ "code": out.code, "why": "không hỏi được ioreg — KHÔNG kết luận là màn đang mở" }),
        );
        return None;
    }
    lock_verdict(&out.stdout)
}

/// Phần THUẦN của [`screen_locked`]: đọc verdict ra khỏi chữ `ioreg` in.
///
/// Tách ra để bài kiểm chạm được — chuỗi thật nằm trong `IOConsoleUsers`, một
/// dòng dài chứa cả chục khoá, và `CGSSessionScreenIsLocked` chỉ CÓ MẶT khi
/// phiên đã từng khoá. Vắng mặt = chưa khoá lần nào, tức đang mở.
pub fn lock_verdict(ioreg_out: &str) -> Option<bool> {
    if !ioreg_out.contains("IOConsoleUsers") {
        return None;
    }
    Some(ioreg_out.contains("\"CGSSessionScreenIsLocked\"=Yes"))
}

/// Ảnh ra đen thì NÓI ĐÚNG VÌ SAO — hàm thuần, kiểm được cả ba ngả.
///
/// Ba câu khác nhau cho ba trạng thái, vì việc chủ máy phải làm khác nhau: mở
/// khoá máy · cấp một quyền · hoặc đi kiểm hộ vì hub không đo được. Gộp cả ba
/// thành "gần như luôn là quyền" là đúng cái tôi vừa làm sai sáng nay.
pub fn blank_frame_reason(locked: Option<bool>) -> String {
    match locked {
        Some(true) => "ảnh ra ĐEN vì máy đang ở **màn hình đăng nhập** (đo: \
             `CGSSessionScreenIsLocked=Yes`). macOS không cho chụp gì ngoài khung trống khi màn \
             đã khoá — không phải chuyện quyền. Mở khoá máy rồi `/anh` lại. Cần biết phiên đang \
             hiện gì ngay bây giờ thì `/shot`: chữ trên màn đọc qua Terminal, khoá màn không cản."
            .to_string(),
        Some(false) => "chụp được nhưng KHUNG HÌNH RỖNG — ảnh toàn đen (đo: co còn 1 điểm ảnh ⟹ \
             #000000), mà màn **không** khoá (`CGSSessionScreenIsLocked` vắng mặt). \
             `screencapture` vẫn trả exit 0, nên đây là cách macOS từ chối im lặng khi thiếu \
             quyền **Screen Recording**: System Settings → Privacy & Security → Screen & System \
             Audio Recording → `+` → ⌘⇧G → dán `~/Library/Application Support/hub/bin/hubd`, bật \
             công tắc, rồi chạy `install_update.sh` (macOS chỉ cấp quyền mới cho tiến trình khởi \
             động lại)."
            .to_string(),
        None => "ảnh ra ĐEN, và hub **không đo được** máy có đang khoá màn không (`ioreg` không \
             trả lời). Hai khả năng, kiểm theo thứ tự: máy đang ở màn hình đăng nhập (mở khoá rồi \
             `/anh` lại), hoặc thiếu quyền Screen Recording cho \
             `~/Library/Application Support/hub/bin/hubd`."
            .to_string(),
    }
}

/// Khung hình ấy có RỖNG không — `Some(true)` = toàn một màu đen.
///
/// 🔴 Hà 2026-08-18: *"Chụp ảnh ra den xì"* · *"Phiên đang mở"*. Đo trên máy
/// cùng lúc: `screencapture -x` **exit 0**, ghi ra một tệp 112 KB, ảnh
/// 3024×1964 — và mọi điểm ảnh lấy mẫu đều bằng nhau, luma 0,0. Tức macOS
/// KHÔNG từ chối nữa (hôm 17/08 nó còn trả *"could not create image from
/// display"*); nó trả một khung trống. Đó là hình dạng của một lời từ chối IM
/// LẶNG: thiếu quyền Screen Recording thì khung hình không có cửa sổ nào.
///
/// Không đo được thì trả `None` và GHI LOG — "không kiểm được" phải khác
/// "kiểm rồi, ảnh ổn", nếu không thì một hôm `sips` biến mất là hub lại lặng lẽ
/// gửi ảnh đen đi.
///
/// Phép đo: `sips` co ảnh còn **1 điểm** rồi ghi ra BMP (không nén, byte điểm
/// ảnh nằm thẳng trong tệp) — không cần thư viện giải mã ảnh nào. Nó ĐỎ ĐƯỢC:
/// cùng lệnh ấy trên `DefaultDesktop.heic` trả `93 7a 43`, còn trên tấm ảnh đen
/// vừa chụp trả `00 00 00`.
pub fn frame_is_blank(path: &std::path::Path) -> Option<bool> {
    let bmp = path.with_extension("probe.bmp");
    let out = crate::exec::run(
        "sips",
        &[
            "-s",
            "format",
            "bmp",
            "--resampleHeightWidth",
            "1",
            "1",
            &path.display().to_string(),
            "--out",
            &bmp.display().to_string(),
        ],
        crate::exec::RunOpts {
            timeout: Some(std::time::Duration::from_secs(10)),
            ..Default::default()
        },
    )
    .ok()?;
    let bytes = std::fs::read(&bmp).ok();
    let _ = std::fs::remove_file(&bmp);
    let bytes = match (out.code, bytes) {
        (Some(0), Some(b)) if b.len() >= 4 => b,
        (code, _) => {
            logging::warn(
                "screenshot_blank_check_failed",
                json!({ "code": code, "why": "không đo được ảnh — KHÔNG kết luận là ảnh ổn" }),
            );
            return None;
        }
    };
    // BMP xuôi từ dưới lên, một điểm ảnh: ba byte màu nằm ở cuối tệp (byte thứ
    // tư là alpha hoặc chèn cho chẵn 4 — bỏ qua).
    let tail = &bytes[bytes.len() - 4..];
    let sum: u32 = tail[..3].iter().map(|b| u32::from(*b)).sum();
    Some(sum == 0)
}

pub fn photograph_window(window: i64, path: &std::path::Path) -> Result<()> {
    // Không đưa ra trước được thì vẫn chụp — một tấm ảnh cả màn hình còn hơn
    // không có gì, và câu trả lời sẽ nói rõ là chưa focus được.
    let focused = osascript(&format!(
        r#"tell application "Terminal"
  set index of window id {window} to 1
  activate
end tell"#
    ))
    .is_ok();
    if focused {
        std::thread::sleep(std::time::Duration::from_millis(700));
    }
    let out = crate::exec::run(
        "screencapture",
        &["-x", &path.display().to_string()],
        crate::exec::RunOpts {
            timeout: Some(std::time::Duration::from_secs(20)),
            ..Default::default()
        },
    )?;
    if out.code != Some(0) || !path.exists() {
        anyhow::bail!(
            "screencapture không chụp được ({}). Gần như luôn là quyền **Screen Recording**: \
             System Settings → Privacy & Security → Screen Recording → bật cho `hubd` \
             (~/Library/Application Support/hub/bin/hubd), rồi `/anh` lại. \
             Đo 2026-08-17: chưa cấp thì nó trả đúng câu 'could not create image from display'.",
            crate::exec::truncate(out.stderr.trim(), 160)
        );
    }
    // 🔴 Chụp được ≠ chụp thấy. Xem `frame_is_blank`: thiếu quyền thì macOS trả
    // một khung TRỐNG với exit 0, nên không đo là gửi đi một tấm ảnh đen kèm
    // câu "đây, màn hình của phiên".
    let blank = frame_is_blank(path);
    if blank == Some(true) {
        let locked = screen_locked();
        let _ = std::fs::remove_file(path);
        logging::warn(
            "screenshot_blank",
            json!({ "window": window, "screen_locked": locked }),
        );
        anyhow::bail!("{}", blank_frame_reason(locked));
    }
    logging::info(
        "screenshot_taken",
        json!({ "window": window, "focused": focused,
                "blank_checked": blank.is_some(),
                "bytes": std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) }),
    );
    Ok(())
}

/// Cửa sổ ấy ĐÃ ĐÓNG chưa — và phép đo phải trỏ đúng chỗ.
///
/// 🔴 `id of every window` KHÔNG trả lời được câu này, dù nó là chỗ đầu tiên ai
/// cũng nhìn: một cửa sổ đã đóng vẫn nằm trong danh sách ấy. Đo 2026-08-17, hai
/// cửa sổ vừa đóng xong: `2169 tabs=0 visible=false` · `2170 tabs=0
/// visible=false` — id còn nguyên, cửa sổ thì đã biến mất khỏi màn hình.
///
/// Tôi đã tin cái danh sách ấy đúng một lượt, và nó dẫn tôi tới một kết luận
/// SAI ("máy khoá màn hình nên `close` không chạy") mà tôi đã nói ra với chủ
/// máy trước khi kiểm lại. Thứ nói đúng chuyện là **số tab** và **`visible`**.
pub fn window_gone(window: i64) -> Result<bool> {
    let out = osascript(&format!(
        r#"tell application "Terminal"
  try
    return ((count of tabs of window id {window}) as text) & "/" & ((visible of window id {window}) as text)
  on error
    return "0/false"
  end try
end tell"#
    ))?;
    let t = out.trim();
    Ok(t.starts_with("0/") || t.ends_with("false"))
}

/// Đóng cửa sổ, **rồi hỏi lại xem nó đã đi chưa**. Gọi khi ĐÃ biết tab không
/// còn bận — xem `tab_state`.
///
/// 🔴 `osascript` TRẢ 0 CHO MỘT LỆNH KHÔNG LÀM GÌ — đo 2026-08-17, và đây là lần
/// thứ hai cùng một cái bẫy trong tệp này (lần trước: `do script` trả 0 chỉ nói
/// bytes tới tab, xem `send_exit`). Bản cũ trả `Ok(())` ngay sau lời gọi, nên
/// nút ◻ *"đóng nó"* của route `/terminal` báo xong trong khi cửa sổ còn y
/// nguyên. Hà, đúng lúc ấy: *"Nút tắt nhanh đâu"* — anh bấm, không có gì xảy ra.
///
/// Đo được (cửa sổ nháp, cùng ngày), và nó KHÔNG phải chuyện khoá màn hình —
/// giả thuyết ấy tôi nói ra rồi phải rút lại sau khi đo đúng chỗ:
/// · cửa sổ vừa mở, shell còn sống ⟹ `close` ĐÓNG được;
/// · cửa sổ vừa `exit`, tab `[Process completed]`, 0 tiến trình ⟹ ĐÓNG được;
/// · **năm cửa sổ có `claude` bị `kill`** rồi shell thoát ⟹ `close` chạy êm,
///   trả 0, và cửa sổ đứng nguyên (`tabs=1 visible=true`), thử đủ bốn cách viết
///   (`close window id`, `close (first window whose id is …)`,
///   `tell window id … to close`, `close … saving no`) và cả khi Terminal đang
///   là app trước. Cùng lúc ấy `set custom title` trên CHÍNH cửa sổ đó lại ăn
///   ngay — tức đường Apple Event vẫn thông, chỉ riêng động từ `close` không có
///   hiệu lực với những cửa sổ ấy.
///
/// Vì sao thì chưa biết (Accessibility không cho đọc cửa sổ của Terminal, và
/// chụp màn hình cũng bị chặn, nên không nhìn được có hộp thoại nào đang treo
/// trên chúng không). Chưa biết thì KHÔNG đoán trong câu báo lỗi — chỉ kể thứ
/// đo được và chỉ đường ⌘W.
pub fn close_window(window: i64) -> Result<Closed> {
    osascript(&format!(
        r#"tell application "Terminal" to close (first window whose id is {window})"#
    ))?;
    for _ in 0..6 {
        std::thread::sleep(std::time::Duration::from_millis(300));
        match window_gone(window) {
            Ok(true) => return Ok(Closed::Gone),
            Ok(false) => {}
            Err(e) => return Err(e.context(
                "đã gửi lệnh đóng nhưng không hỏi lại được cửa sổ — KHÔNG biết nó đã đóng hay chưa",
            )),
        }
    }
    // 🔴 ĐÓNG KHÔNG ĐƯỢC THÌ ẨN, và nói đúng là ĐÃ ẨN — Hà bấm ⏹ bốn lượt liền
    // rồi nhận bốn câu *"chưa đóng được"* (17/08). Câu ấy thật thà, nhưng thật
    // thà xong thì cửa sổ rác vẫn nằm đó và danh sách vẫn dài ra.
    //
    // Đo cùng lúc trên CHÍNH những cửa sổ ấy: `close` không ăn, mà
    // `set visible to false` ăn ngay. Nên hub làm được đúng một nửa việc — và
    // một nửa nói ra được vẫn hơn không nửa nào.
    //
    // Ẩn KHÔNG phải đóng: cửa sổ vẫn còn trong menu Window của Terminal (⌘W khi
    // ngồi máy), nhưng nó rời khỏi mắt và rời khỏi mọi danh sách của hub
    // (`tabs_script` bỏ cửa sổ đã ẩn) — nên câu trả lời phải nói cả hai điều ấy.
    let hidden = osascript(&format!(
        r#"tell application "Terminal"
  try
    set visible of window id {window} to false
    return "an"
  on error errm
    return "hong: " & errm
  end try
end tell"#
    ))
    .map(|s| s.trim() == "an")
    .unwrap_or(false);
    if hidden {
        logging::info(
            "window_hidden_not_closed",
            json!({ "window": window,
                    "why": "close chạy êm mà cửa sổ không đóng — ẩn được thì ẩn, và NÓI là ẩn" }),
        );
        return Ok(Closed::Hidden);
    }
    anyhow::bail!(
        "Terminal nhận lệnh đóng (osascript trả 0) mà cửa sổ vẫn còn tab và vẫn hiện sau ~2 giây, \
         ẩn nó đi cũng không xong. Đo 2026-08-17: phần lớn cửa sổ đóng được bình thường, nhưng có \
         những cửa sổ — đo được trên năm cái từng chạy một CLI bị `kill` — thì `close` chạy êm mà \
         không đóng, đủ mọi cách viết. Chưa rõ vì sao. Đóng tay bằng ⌘W thì được."
    )
}

/// Tab của cửa sổ này còn TIẾN TRÌNH nào không — `None` là không còn cửa sổ.
///
/// Khác `tab_state`: `busy` trả lời *"có chương trình nào đang chạy"* và một
/// shell trống thì `false`, nên `Idle` KHÔNG phân biệt được "cửa sổ chết, chỉ
/// còn dòng `[Process completed]`" với "cửa sổ mới mở, đang ở dấu nhắc". Đúng
/// cái phân biệt ấy là cổng an toàn cho mọi lượt đóng LẠI: id cửa sổ của
/// Terminal đánh lại từ số nhỏ sau khi Terminal khởi động lại (đo được ngay
/// trên máy này: id `156` nằm cạnh đám `21xx`), nên một mục cũ trong sổ có thể
/// trỏ vào một cửa sổ MỚI — cùng họ với bài học "tty là con số ĐƯỢC DÙNG LẠI".
///
/// Đo 2026-08-17: cửa sổ chết `2150` → `0` · hai cửa sổ phiên đang sống →
/// `6` (`login-zsh claude project-agent node caffeinate`). Một phiên thật luôn
/// có ít nhất cái shell, nên `Some(0)` là "không còn gì để mất".
pub fn tab_process_count(window: i64) -> Result<Option<usize>> {
    let out = osascript(&format!(
        r#"tell application "Terminal"
  try
    return (count of processes of (selected tab of window id {window})) as text
  on error
    return "gone"
  end try
end tell"#
    ))?;
    let t = out.trim();
    if t == "gone" {
        return Ok(None);
    }
    match t.parse::<usize>() {
        Ok(n) => Ok(Some(n)),
        Err(_) => anyhow::bail!("Terminal trả lời lạ cho số tiến trình của cửa sổ {window}: {t:?}"),
    }
}

/// Thử ĐÓNG LẠI một cửa sổ đã ẩn, và đo bằng thứ đo đúng chuyện.
///
/// 🔴 KHÔNG dùng `window_gone` ở đây, dù tên nó nghe đúng việc: nó coi
/// `visible = false` **là đã đi** (có chủ ý — cửa sổ ẩn rời khỏi mọi danh sách
/// của hub). Với một cửa sổ vốn ĐANG ẩn thì phép đo ấy trả `true` ngay lượt
/// đầu, tức hub sẽ báo *"đã đóng hẳn"* cho một cửa sổ còn nguyên. Cùng một cái
/// bẫy "phép đo trỏ nhầm chỗ" đã trả giá hai lần trong tệp này; câu hỏi ở đây
/// là *"cửa sổ ấy còn tồn tại không"*, và `tab_state` là chỗ trả lời nó.
///
/// Vì sao có hàm này: 17/08 lúc 10:20Z, năm cửa sổ từ chối `close` (chạy êm,
/// trả 0, cửa sổ đứng nguyên) nên hub ẩn chúng đi. Bốn tiếng sau, ĐÚNG những
/// cửa sổ ấy, ĐÚNG lệnh ấy, gọi tay: cả ba cái thử đều đóng ngay lượt đầu
/// (`1/false` → `0/false`). Nên lời từ chối kia là NHẤT THỜI, không phải thuộc
/// tính của mấy cửa sổ đó — và thứ chữa một lời từ chối nhất thời là thử lại,
/// chứ không phải bỏ nó nằm đó khuất mắt mãi mãi.
pub fn close_hidden_again(window: i64) -> Result<bool> {
    osascript(&format!(
        r#"tell application "Terminal" to close (first window whose id is {window})"#
    ))?;
    for _ in 0..6 {
        std::thread::sleep(std::time::Duration::from_millis(300));
        if matches!(tab_state(window)?, TabState::Gone) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Hai kết cục KHÁC NHAU của một lượt đóng, nên hai giá trị.
pub enum Closed {
    /// Cửa sổ đã biến mất — đo bằng số tab + `visible` (xem [`window_gone`]).
    Gone,
    /// `close` không ăn nhưng ẩn được: khuất mắt và khuất khỏi danh sách của
    /// hub, mà vẫn còn trong menu Window của Terminal cho tới khi ⌘W.
    Hidden,
}

pub fn quit_and_close(window: i64) -> Result<Closed> {
    // Một đường gõ `/exit` duy nhất — xem `send_exit`. Trước 2026-08-15 đây là
    // bản chép tay THỨ HAI của cùng một dòng `do script`, và nó là bản thiếu cú
    // Enter rời.
    send_exit(window)?;
    // Chờ CLI nhả tab. 20 × 500ms: `/exit` thường xong trong ~3 giây, nhưng một
    // phiên đang giữa lượt phải kết thúc lượt ấy trước.
    // 60 × 500ms = 30 giây. Rộng hơn "vừa đủ cho một phiên rảnh" là có chủ ý:
    // `claude` XẾP HÀNG chữ khi đang giữa lượt, nên `/exit` chỉ có hiệu lực sau
    // khi lượt ấy chạy xong. Bấm "Tắt hẳn" đúng lúc phiên đang chạy là chuyện
    // thường, và bỏ cuộc sau 10 giây thì biến một việc chỉ cần chờ thành một
    // lời báo hỏng.
    let mut still_busy = true;
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        match tab_state(window) {
            Ok(TabState::Idle) => {
                still_busy = false;
                break;
            }
            Ok(TabState::Busy) => {}
            // Cửa sổ biến mất trong lúc chờ là việc ĐÃ XONG, không phải hub mù:
            // Terminal tự dọn cửa sổ khi shell thoát (tuỳ hồ sơ), và chủ máy
            // ngồi ngay đấy bấm ⌘W cũng ra đúng kết cục này.
            Ok(TabState::Gone) => {
                logging::info(
                    "close_window_gone_while_waiting",
                    json!({ "window": window,
                            "why": "cửa sổ không còn trong lúc chờ /exit — đã đóng, không phải hỏi không được" }),
                );
                return Ok(Closed::Gone);
            }
            // Không hỏi được thì DỪNG, đừng đóng liều: đóng khi chưa chắc đã
            // thoát là đúng cái bật hộp thoại.
            Err(e) => return Err(e.context("không hỏi được tab còn bận không")),
        }
    }
    if still_busy {
        // 🔴 Câu này phải kể thứ ĐO ĐƯỢC, không phải thứ giả định.
        //
        // Bản cũ mở đầu bằng *"đã gõ /exit"* — một mệnh đề về hành động, phát ra
        // từ chỗ chỉ biết `osascript` trả 0. Hà đọc lại câu ấy rồi đi mở cửa sổ
        // ra soi và không thấy `/exit` đâu (2026-08-15), tức nó tiêu đúng thứ
        // đắt nhất một dòng log có: lòng tin. Thứ hàm này THẬT SỰ đo được chỉ là
        // `tab_state` — nên nó chỉ được nói chừng ấy, và chỉ đường tới dòng log
        // `keys_exit_sent` cho phần còn lại.
        anyhow::bail!(
            "sau 30 giây `tab_state` vẫn `Busy` — tức **CLI chưa thoát** (`busy` chỉ về `false` khi tiến trình trong tab kết thúc; xem chú thích của hàm này). Hai lý do có thể: phiên đang giữa một lượt nên `claude` xếp `/exit` vào hàng chờ, HOẶC dòng ấy chưa được gửi đi. Cửa sổ giữ nguyên (đóng lúc này sẽ bật hộp thoại 'terminate running processes'). Cắt lượt đang chạy bằng `/key esc` rồi `/close`, hoặc chờ phiên rảnh. Lệnh thoát có được đẩy đi hay không: xem log `keys_exit_sent`"
        );
    }
    // Một đường đóng duy nhất — và nó tự KIỂM. Bản trước chép tay lại dòng
    // `close` ở đây, nên khi `close_window` học được phép kiểm thì nhánh này vẫn
    // mù: cùng hình dạng "bản chép tay thứ hai là bản thiếu" đã trả giá ở
    // `send_exit`.
    close_window(window)
}

/// Gõ `text` vào cửa sổ của phiên, rồi Enter.
///
/// `enter = false` cho các lựa chọn cần phím riêng (mũi tên, Esc) — xem
/// [`press`].
/// Đẩy chữ vào ô nhập. **Chỉ gõ** — không đảm bảo nó được GỬI.
///
/// 🔴 Hàm này từng mang một tham số `enter: bool` mà dòng đầu thân hàm là
/// `let _ = enter;` — tức một lời hứa trong chữ ký, bỏ qua trong mã. Gỡ
/// 2026-08-15, sau khi chính tôi đọc `type_into(w, task, true)` ở một chỗ gọi
/// MỚI và tin là nó đã bấm Enter hộ. Một tham số nói dối nguy hơn một tham số
/// thiếu: cái thiếu thì trình dịch kêu, cái nói dối thì không.
pub fn type_into(window: i64, text: &str) -> Result<()> {
    osascript(&do_script(window, &as_string(text)))?;
    Ok(())
}

/// XOÁ SẠCH ô nhập của một cửa sổ — bằng đúng số phím xoá, như người ngồi máy.
///
/// 🔴 Hà 2026-08-16: *"chèn 2 nút 1 nút enter 1 nút xóa"* · *"còn lăn tăn nó là
/// text mờ hay tỏ thì thêm 1 nút xóa bên cạnh nữa để tự thao tác"*.
///
/// Bốn phép đo trên TUI thật (cửa sổ nháp, 2026-08-16) dẫn tới đúng cách này —
/// ba cách "hiển nhiên" hơn đều KHÔNG ăn, và đó là phần đáng nhớ:
/// · `Ctrl+C` (ETX): chữ y nguyên, không xoá cũng không gửi;
/// · `Ctrl+U` (kill-line) và `Ctrl+A`+`Ctrl+K`: y nguyên;
///   ⟹ qua `do script`, ký tự điều khiển KHÔNG hành xử như phím — cả lượt ghi
///   vào TUI như một cú DÁN, nên chúng chỉ là byte trong nội dung.
/// · `DEL` (127) lặp đúng số ký tự đang có: **ô sạch**.
///
/// Và cái chốt làm nó an toàn: **`ESC` ở CUỐI payload chặn được cái CR** mà
/// `do script` luôn kèm (đo: `"chữ" & ESC` ⟹ chữ nằm lại trong ô, không gửi).
/// Không có nó thì mọi lượt xoá kết thúc bằng một cú Enter — tức nút "xoá" hoá
/// thành nút "gửi", đúng thứ không lùi lại được.
///
/// Trả `Ok(true)` khi ô đã sạch, `Ok(false)` khi còn chữ — KHÔNG tự khen: chỗ
/// gọi phải nói đúng thứ đã xảy ra.
pub fn clear_box(window: i64) -> Result<bool> {
    let before = screen_text(window)?;
    let text = input_box_text(&before).unwrap_or_default();
    let n = text.chars().count();
    if n == 0 {
        return Ok(true);
    }
    // Thừa vài phím: con trỏ có thể không ở cuối, và một ô nhiều dòng đếm ra
    // ngắn hơn thực tế. DEL thừa vào ô trống thì không làm gì.
    let dels: String = std::iter::repeat_n('\u{7f}', n + 8).collect();
    osascript(&do_script(
        window,
        &format!("({} & (ASCII character 27))", as_string(&dels)),
    ))?;
    std::thread::sleep(std::time::Duration::from_millis(600));
    let after = screen_text(window)?;
    let left = input_box_text(&after).unwrap_or_default();
    Ok(left.trim().is_empty())
}

/// Trần số vòng của [`clear_queue`] — hàng chờ dài hơn thế thì nói ra, đừng quay mãi.
const CLEAN_MAX_ROUNDS: usize = 25;

/// Xoá SẠCH hàng chờ của một phiên: `(đã xoá, còn lại)`.
///
/// 🔴 Hà 2026-08-18: *"Thêm lệnh clean xóa hết ở chờ"*. Đường đi là cái mà chính
/// TUI quảng cáo trên màn — *"Press up to edit queued messages"*: `↑` lấy tin
/// cuối RA KHỎI hàng và đặt vào ô nhập, rồi xoá ô ấy đi là tin biến mất.
///
/// Ba điều hàm này KHÔNG làm, mỗi điều vá một cách hỏng đã trả giá ở chỗ khác:
///
/// - **Không tin cú bấm.** Mỗi vòng ĐỌC LẠI màn và đếm ([`queued_count`]); hàng
///   chờ không giảm thì dừng ngay và nói ra, chứ không quay đủ 25 vòng rồi báo
///   một câu chung chung. Cùng luật với `close_step`: phán đoán dựa trên phép
///   đo sau mỗi bước, không dựa trên mã trả về của thứ vừa gửi đi.
/// - **Không để cái CR của `do script` gửi tin đi lại.** Terminal kèm một CR vào
///   cuối MỌI lượt ghi (xem `press_writes`), mà lúc ấy ô nhập vừa NHẬN nội dung
///   tin vừa lấy ra — một CR ở đó là gửi lại đúng cái mình định xoá. Nên `↑`,
///   dãy DEL và `ESC` đi trong **CÙNG một lượt ghi**: `ESC` cuối payload chặn
///   được CR ở ô nhập (đo trong `clear_box`).
/// - **Không đụng vào lượt đang chạy.** Chỉ hàng chờ; phiên vẫn chạy tiếp việc
///   nó đang làm. Muốn cắt lượt ấy thì đó là `/key esc`, một lệnh khác.
pub fn clear_queue(window: i64) -> Result<(usize, usize)> {
    let mut left = queued_count(&screen_text(window)?);
    let start = left;
    if left == 0 {
        return Ok((0, 0));
    }
    for _ in 0..CLEAN_MAX_ROUNDS {
        // Nhiều DEL hơn hẳn một tin dài: DEL thừa vào ô trống không làm gì
        // (cùng lý do `clear_box` bắn dư 8 cái).
        let dels: String = std::iter::repeat_n('\u{7f}', 600).collect();
        let payload = format!(
            "((ASCII character 27) & \"[A\" & {} & (ASCII character 27))",
            as_string(&dels)
        );
        osascript(&do_script(window, &payload))?;
        std::thread::sleep(std::time::Duration::from_millis(400));
        let now = queued_count(&screen_text(window)?);
        if now >= left {
            // Không giảm ⟹ `↑` không lấy được tin ra. Nói ra chỗ đứng thật.
            logging::warn(
                "clean_queue_stuck",
                json!({ "window": window, "left": now, "removed": start - now,
                        "why": "hàng chờ không giảm sau một vòng ↑+xoá ô" }),
            );
            return Ok((start - now, now));
        }
        left = now;
        if left == 0 {
            break;
        }
    }
    logging::info(
        "clean_queue_done",
        json!({ "window": window, "removed": start - left, "left": left }),
    );
    Ok((start - left, left))
}

/// Gõ chữ **và gửi đi** — một chỗ duy nhất giữ cú Enter rời.
///
/// 🔴 Luật 13, trả giá cả tối 2026-08-12: `do script` đẩy chữ + dấu xuống dòng
/// trong CÙNG một lượt ghi, và TUI của `claude` đọc lượt ấy như một cú **DÁN** —
/// dấu xuống dòng rơi vào NỘI DUNG thay vì kết thúc nó. Chữ ký của lỗi là câu
/// Hà tả: *"gửi xong im lặng mãi, gửi lần nữa lại gộp thành 1 tin rồi enter"*.
///
/// Nên phải là hai lượt ghi RỜI, giãn nhau (400ms rồi 1000ms): pty giữ đúng thứ
/// tự, và cú thứ hai tới như một phím thật.
///
/// 🔴 NHÌN RỒI MỚI BẤM — 2026-08-16. Bản trước bắn hai Enter **vô điều kiện**,
/// dựa trên câu "Enter thừa vào ô TRỐNG thì `claude` không làm gì, nên lặp lại
/// an toàn theo đúng nghĩa idempotent". Câu ấy sai ở chính chỗ nó tự tin nhất:
/// Enter khi ấy là LF (xem `key_payload`), và LF vào ô trống **không phải là
/// không làm gì** — nó chèn một dòng trống, ô nhập thành nhiều dòng, gợi ý mờ
/// biến mất. Hai cú vô điều kiện ⟹ hai dòng rác sau MỖI lần gõ, và Hà đọc ra
/// chúng trong ảnh chụp trước khi tôi đọc ra chúng trong mã.
///
/// Và cái cớ để bắn vô điều kiện cũng không còn: đo lại hôm nay trên TUI thật,
/// `do script "chữ"` **tự gửi** (CR nằm sẵn trong dấu mà `do script` kèm) — cả
/// chữ ngắn lẫn câu 45 ký tự. Nên cú Enter rời không phải luật, nó là THUỐC cho
/// đúng ca chữ nằm lại: hỏi `still_in_box` rồi mới bấm, đúng như CLAUDE.md §13
/// đã tả mà mã thì chưa làm.
///
/// Ba cửa, cả ba đều đo được chứ không đoán: chữ còn nằm trong ô · màn không có
/// hộp chọn (ở đó Enter là CHỐT, và chốt hộ chủ máy là thứ không lùi lại được) ·
/// đọc được màn (đọc không được thì KHÔNG bấm — mù mà vẫn ra tay là đúng cái
/// bẫy `Look::Blind` sinh ra để chặn).
///
/// 📌 Route `/type` giữ vòng lặp riêng của nó (`pipeline.rs`, nhánh `!is_key`)
/// vì nó còn ghi log từng lượt bấm; hai chỗ phải kể CÙNG một câu chuyện — sửa
/// một bên thì sang bên kia đọc lại.
pub fn type_and_send(window: i64, text: &str) -> Result<()> {
    type_into(window, text)?;
    for wait_ms in [400u64, 1000] {
        std::thread::sleep(std::time::Duration::from_millis(wait_ms));
        let screen = match screen_text(window) {
            Ok(s) => s,
            Err(e) => {
                // Không đọc được màn thì không bấm, và nói ra: một Enter bắn mù
                // không lùi lại được.
                logging::warn(
                    "keys_send_check_blind",
                    json!({ "window": window, "err": e.to_string(),
                            "effect": "không kiểm được chữ đã đi chưa — KHÔNG bấm Enter lượt này" }),
                );
                return Ok(());
            }
        };
        if !still_in_box(&screen, text) {
            // Chữ đã rời ô nhập — `do script` gửi được ngay, đường thường.
            return Ok(());
        }
        if !parse_choices(&screen).is_empty() {
            logging::warn(
                "keys_send_held_dialog",
                json!({ "window": window,
                        "effect": "chữ còn trong ô nhập nhưng màn đang có hộp chọn — Enter ở đó là CHỐT, nên không bấm" }),
            );
            return Ok(());
        }
        press(window, "enter")?;
    }
    Ok(())
}

/// Gửi chữ vào cửa sổ bằng `do script` — API CỦA CHÍNH Terminal.
///
/// Đường cũ đi qua `System Events keystroke`, và macOS chặn thẳng:
/// *"osascript is not allowed to send keystrokes (1002)"*. Cấp Accessibility
/// cho `hubd` không gỡ được, vì thứ gọi AXAPI là `/usr/bin/osascript` —
/// một binary hệ thống, không gán quyền cho nó qua đường daemon được (đo
/// 2026-08-10: cấp quyền rồi khởi động lại daemon, vẫn 1002).
///
/// `do script` thì khác hẳn: nó là scripting API của Terminal, chỉ cần quyền
/// **Automation** — thứ hub đã có, bằng chứng là nó đang đọc được `contents of
/// selected tab`. Nó đẩy chữ vào đúng tab như người gõ, kể cả khi có chương
/// trình đang chạy phía trước.
///
/// ⚠ `do script` LUÔN kèm một dấu xuống dòng — không tắt được. Với ô nhập của
/// `claude` thì đó đúng là điều ta muốn (gõ xong là gửi), nhưng nó cũng có
/// nghĩa: không có cách "gõ mà chưa gửi" qua đường này.
fn do_script(window: i64, applescript_string: &str) -> String {
    format!(
        r#"tell application "Terminal"
  do script {applescript_string} in selected tab of window id {window}
end tell"#
    )
}

/// Một phím điều khiển: `up` `down` `enter` `esc` `tab` `space`, hoặc `1`–`9`.
///
/// Hộp chọn của `claude` đi bằng mũi tên + Enter, và gửi chữ "xuống" vào đó thì
/// nó gõ ra chữ chứ không di chuyển.
pub fn press(window: i64, keyname: &str) -> Result<()> {
    press_writes(window, &[vec![keyname.to_string()]])
}

/// Chữ này có phải TÊN MỘT PHÍM không — hỏi chính bảng phím, không chép lại.
///
/// Dùng cho `/type <nút> [id phiên]` (Hà 2026-08-16). Một bản danh sách thứ hai
/// ở chỗ khác là một bản sẽ lệch: thêm phím mới ở `key_payload` mà quên bên kia
/// thì `/type <phím mới>` lặng lẽ đi làm một dòng chữ gõ vào phiên.
pub fn is_key_name(s: &str) -> bool {
    // `clear` không có trong bảng phím vì nó KHÔNG phải một phím (nó đọc màn
    // rồi bắn đúng bấy nhiêu DEL — xem `clear_box`), nhưng với người gõ
    // `/type clear` thì nó là một cái nút như mọi cái khác.
    s == "clear" || key_payload(s).is_ok()
}

/// Payload AppleScript cho một phím — dùng chung cho [`press`] và [`press_seq`].
fn key_payload(keyname: &str) -> Result<String> {
    // Ký tự điều khiển gửi qua `do script` như mọi chuỗi khác. Mũi tên là dãy
    // thoát ANSI: ESC [ A/B/C/D — đúng thứ terminal nhận khi người ta bấm.
    Ok(match keyname {
        // 🔴 CHÚ THÍCH ĐÃ ĐÚNG, MÃ THÌ SAI — suốt từ 08-14 tới 08-16. Ba dòng
        // ngay dưới đây vẫn nói "CR (ASCII 13) là thứ terminal thật gửi khi
        // người ta bấm Return, nên gửi đúng ký tự ấy", mà thứ đứng sau dấu `=>`
        // lại là **ASCII 10 (LF)**. Không trình dịch nào kêu, không bài kiểm nào
        // chạm — payload là một chuỗi AppleScript, với Rust nó chỉ là chữ.
        //
        // Hà 2026-08-16, kèm ảnh chụp buồng chat: *"Nút enter này vẫn chưa có
        // tác dụng"* · *"Làm mất luôn gợi ý mờ"*. Hai câu ấy là hai nửa của
        // đúng một byte sai, và cả hai đo lại được:
        //
        // · `do script` tự kèm **CR**, không phải LF (đo bằng một cửa sổ nháp
        //   chạy `stty raw -echo; cat -vet`: payload rỗng ⟹ đúng một `^M`;
        //   `"abc" & CR` ⟹ `abc^M^M`). Nên CLAUDE.md §13 nói "always appends a
        //   newline" là đúng ý mà sai byte.
        // · Vậy `(ASCII character 10)` bắn ra **LF rồi CR**. Đo trên TUI thật:
        //   LF **chèn một dòng trống vào ô nhập** — ô nhập thành nhiều dòng, góc
        //   phải hiện `ctrl+g to edit in Vim`, và **gợi ý mờ biến mất** vì ô
        //   không còn rỗng. Sau đó CR gặp một nội dung chỉ toàn khoảng trắng nên
        //   `claude` không gửi gì. Đúng cả hai câu Hà nói, theo đúng thứ tự.
        //
        // CR (ASCII 13) là thứ terminal thật gửi khi người ta bấm Return, nên
        // gửi đúng ký tự ấy thay vì trông chờ vào cái xuống dòng `do script`
        // tự thêm.
        "enter" => "(ASCII character 13)".to_string(),
        "esc" => "(ASCII character 27)".to_string(),
        "up" => "((ASCII character 27) & \"[A\")".to_string(),
        "down" => "((ASCII character 27) & \"[B\")".to_string(),
        "right" => "((ASCII character 27) & \"[C\")".to_string(),
        "left" => "((ASCII character 27) & \"[D\")".to_string(),
        "tab" => "(ASCII character 9)".to_string(),
        "space" => as_string(" "),
        d if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => as_string(d),
        other => return Err(anyhow!("không biết phím '{other}'")),
    })
}

/// Nhịp nghỉ giữa hai LƯỢT GHI.
///
/// Đủ để TUI vẽ xong lượt trước (đo trên máy: một bước điều hướng của `claude`
/// mất chừng 30–60 ms), và đủ ngắn để ba lượt không thành một lượt chờ dài.
const SEQ_GAP_MS: u64 = 120;

/// Gửi từng LƯỢT GHI một — mỗi lượt là đúng một `do script`.
///
/// 🔴 ĐƠN VỊ Ở ĐÂY LÀ **LƯỢT GHI, KHÔNG PHẢI PHÍM**, và đó là cả bài học của
/// 2026-08-17. Terminal kèm một **CR** vào cuối MỌI `do script`, không tắt được
/// (xem `do_script`) — nên "mỗi phím một lượt ghi" nghĩa là **mỗi phím kéo theo
/// một cú Enter**, và trên hộp chọn nhiều, Enter là một cú BẬT/TẮT.
///
/// Hà đọc ra hậu quả trước khi tôi đọc ra nó trong mã: *"Bấm cái nọ mất cái kia
/// ảo lắm"*. Nhật ký 12:39–12:40 khớp từng cú, không phải cảm giác: bấm mục 1 ⟹
/// mục 2 mất dấu; bấm mục 2 ⟹ mục 1 mất dấu; bấm mục 3 ⟹ mục 2 mất dấu. Cùng
/// một hình dạng cả ba lần — cái ô bị mất luôn là ô con trỏ **vừa rời khỏi**.
///
/// Đo lại trên hộp thật (cửa sổ nháp riêng, 3 vòng, 18 lượt ghi, cùng ngày):
/// · một lượt ghi = **đúng một** cú bật/tắt, rơi vào **dòng con trỏ ĐANG ĐỨNG**;
/// · payload có mũi tên thì lượt ấy **vừa bật/tắt dòng đang đứng, vừa dời đi** —
///   `↓` đứng ở mục 1 ⟹ mục 1 đổi dấu, con trỏ sang mục 2;
/// · k mũi tên trong CÙNG một lượt ghi dời đủ k bước (3 `↓`: mục 1 → mục 4) mà
///   vẫn chỉ một cú bật/tắt — nên đi xa không đắt hơn đi gần;
/// · hai CR trong cùng một lượt ghi chỉ ra MỘT cú bật/tắt (chúng gộp làm một);
/// · **không chặn được cái CR ấy**: `ESC` cuối payload không chặn (nó chặn được
///   ở Ô NHẬP — xem `clear_box` — mà không chặn ở hộp chọn), CSI cụt (`ESC[`)
///   cũng không, và phím số thì hộp chọn nhiều không nhận (đo lại: gửi `"4"`
///   trong khi đứng ở mục 1 ⟹ mục 4 không nhúc nhích, chỉ mục 1 đổi dấu).
///
/// Cả năm phép đo ấy hợp thành một luật dùng được: **một lượt ghi là một cú
/// Enter, cộng thêm quãng đường mà mũi tên trong lượt ấy đi được**. Xem
/// [`nav_plan`] để biết cách xếp ba lượt sao cho chỉ đúng một ô đổi dấu.
///
/// Hàm này KHÔNG tự quyết được dãy ấy có an toàn không — chỗ gọi phải tự chịu
/// trách nhiệm, cùng luật với `arrow_verdict`.
pub fn press_writes(window: i64, writes: &[Vec<String>]) -> Result<()> {
    if writes.is_empty() {
        return Err(anyhow!("không có lượt ghi nào để gửi"));
    }
    for keys in writes {
        osascript(&do_script(window, &write_payload(keys)?))?;
        std::thread::sleep(std::time::Duration::from_millis(SEQ_GAP_MS));
    }
    Ok(())
}

/// Payload AppleScript của MỘT lượt ghi: nối payload từng phím lại.
///
/// Dãy RỖNG là một lượt hợp lệ và có nghĩa — chuỗi rỗng, tức lượt ấy chỉ mang
/// đúng cái CR mà `do script` kèm sẵn.
fn write_payload(keys: &[String]) -> Result<String> {
    let mut out = String::from("\"\"");
    for k in keys {
        out.push_str(" & ");
        out.push_str(&key_payload(k)?);
    }
    Ok(format!("({out})"))
}

// 🔴 ĐÃ XOÁ: cả nhánh CHỤP ẢNH MÀN HÌNH (`capture` → PNG, `capture_base64` →
// base64, và bộ mã hoá `b64` đi kèm), 2026-08-14.
//
// Nó từng là "đường DUY NHẤT hub nhìn thấy câu hỏi đang chờ", cho tới khi Hà
// hỏi *"sao lại đẩy ảnh, dựng lại đúng option chứ?"* (08-10) và hoá ra Terminal
// cho đọc thẳng `contents of selected tab` — chữ thuần, không OCR, không vài
// trăm KB base64, và chữ thì đi qua được cổng quét rò rỉ còn ảnh thì không.
// Từ hôm ấy `screen_text` làm hết việc, còn ba hàm kia nằm lại **không một chỗ
// gọi nào** suốt bốn ngày.
//
// Chúng ra đi vì đúng câu Hà hỏi hôm nay: *"Tức là bạn đang chụp ảnh thay vì
// lấy text thuần à"*. Mã chết mang tên `capture` thì câu trả lời "hub không
// chụp ảnh" luôn có một dấu hỏi treo phía sau, kể cả khi nó đúng. Nó cũng là
// thứ duy nhất còn đòi quyền **Screen Recording** — bỏ đi là bớt luôn một quyền
// hệ thống hub không dùng tới.

/// CHỮ đang hiện trên màn của cửa sổ ấy.
///
/// Hà 2026-08-10: *"sao lại đẩy ảnh, dựng lại đúng option chứ? thì mới có nhiều
/// lựa chọn thao tác hơn, ví dụ dùng chuột để chọn"*. Đúng — ảnh chỉ NHÌN được,
/// còn cái người ta cần là BẤM được.
///
/// Và hoá ra không cần ảnh: Terminal cho đọc thẳng `contents of selected tab`,
/// tức đúng chữ đang hiện. Không OCR, không vài trăm KB base64, và chữ thì đi
/// qua được `redaction::leak_scan` — ảnh thì không.
pub fn screen_text(window: i64) -> Result<String> {
    let script = format!(
        r#"tell application "Terminal"
  return contents of selected tab of window id {window}
end tell"#
    );
    osascript(&script)
}

/// Trần độ dài một dòng lệnh đáng dựng nút.
///
/// Nguồn của hàm này là chữ phiên VIẾT RA, đọc thẳng từ nhật ký `.jsonl` —
/// nguyên văn, không đi qua bề ngang cửa sổ nào — nên ở đây một dòng dài là
/// một dòng dài THẬT, không phải một mẩu cụt. Vẫn có trần, không bỏ hẳn: bài
/// học 2026-08-14 (*"Cả 1 khối lệnh dài này thì không được tạo nút"*) là về
/// một khối 380 ký tự. 200 nhận trọn một lệnh triển khai có đường dẫn tuyệt
/// đối, và vẫn chặn cả khối.
pub const BTN_CMD_REPORT_MAX: usize = 200;

/// Những DÒNG LỆNH nằm trong chữ phiên viết ra — thứ bấm một cái là chạy được.
///
/// Hà 2026-08-12: *"phiên hiện ra rõ ràng có lệnh để chạy trên terminal … nếu có
/// lệnh như vậy thì hiển thị luôn lệnh gửi nhanh"*. Phiên `claude` thường kết
/// một lượt bằng đúng một câu lệnh cho chủ máy gõ; đọc được nó trên điện thoại
/// mà vẫn phải gõ tay lại từng ký tự thì cây cầu mới đi được một chiều.
///
/// Nhận diện theo HÌNH DẠNG, không theo ngữ nghĩa: bỏ dấu nhắc ở đầu (`$`, `❯`,
/// `>`), rồi đòi từ đầu tiên là một lệnh quen hoặc một đường dẫn `./…`. Cố ý
/// hẹp — đoán rộng ở đây nghĩa là đưa lên màn một cái nút chạy nhầm thứ.
///
/// Giữ tối đa `max` dòng CUỐI (mới nhất), bỏ trùng.
///
/// 🔴 **Nguồn MÀN đã đi hẳn, 2026-08-15.** Hàm này từng có một người anh em
/// (`commands_on_screen`) đọc `contents of selected tab`, và một nửa tệp này
/// từng là bộ máy vá cho đúng một sự thật: **chữ trên màn là chữ đã đi qua một
/// cửa sổ** — bẻ theo bề ngang, cắt bằng `…`, trộn với khung vẽ của TUI. Nó
/// gồm một trần ngắn (60), một phép đo bề ngang, một phép đoán "dòng sau có bị
/// đẩy xuống không", và một hàm nối đuôi. Mỗi ca sai vá thêm một luật, nên luật
/// càng nhiều thì ca sai càng nhiều — và những cái nút chạy SAI đều đến từ đó:
/// `bash …/deploy.sh` thiếu tham số (mẩu cụt lọt trần 60), `git for-each-ref …
/// | xargs` (mẩu của một khối 380 ký tự). Nay lệnh lấy từ SỔ
/// (`sessions::commands_of`), màn chỉ còn để nhìn.
/// `curl`/`wget` mà KHÔNG có đích thì không phải một lệnh chạy được.
///
/// 🔴 Hà 2026-08-16, ảnh chụp một tin [tfl5] có hai icon liền nhau: *"Chỗ này là
/// 1 lệnh hay 2, tại sao bóc tách lệnh lại khó khăn thế"*. Đọc sổ nút ra ngay:
/// cùng một phiên có `curl -s --max-time 15 "https://cpanel.tafalo.com/healthz…"`
/// (lệnh thật) **và** `curl /healthz` — mẩu thứ hai là cách phiên NÓI TẮT trong
/// câu văn, không phải lệnh: `curl` không có host thì chỉ trả `URL using
/// bad/illegal format`. Nên đứng cạnh nhau chúng trông như hai việc, mà chỉ có
/// một, và cái thứ hai bấm vào thì chạy hỏng.
///
/// Hàng rào chung (`KNOWN` + "phải có tham số") không bắt được ca này vì
/// `curl /healthz` thoả cả hai. Câu hỏi đúng cho một lệnh MẠNG là câu cụ thể
/// hơn: *nó có nói đi đâu không*. Cố ý chỉ soi `curl`/`wget` — đó là ca đo
/// được; `ssh`/`scp` luôn mang host trong chính cú pháp của chúng.
fn network_without_target(verb: &str, line: &str) -> bool {
    if !matches!(verb, "curl" | "wget") {
        return false;
    }
    !line.split_whitespace().skip(1).any(|t| {
        let t = t.trim_matches(['"', '\'']);
        t.contains("://")
            || t.starts_with("localhost")
            || t.starts_with("127.0.0.1")
            // `cpanel.tafalo.com/healthz` — có tên miền, không phải cờ, không
            // phải một đường dẫn trần.
            || (!t.starts_with('-') && !t.starts_with('/') && t.contains('.'))
    })
}

pub fn commands_in_report(text: &str, max: usize) -> Vec<String> {
    let max_len = BTN_CMD_REPORT_MAX;
    let screen = text;
    // `gh` vào danh sách 2026-08-13, vì đó là cái tên trong câu Hà hỏi: màn nói
    // *"Next action is yours: merge PR #54"* và không có nút nào. `gh` là công
    // cụ merge trên máy này — cùng loại với `git`, đã nằm sẵn ở đây từ đầu.
    /// `cd <thư mục> && <lệnh>` — dạng phổ biến nhất, và nó KHÔNG bắt đầu bằng một
    /// động từ trong danh sách.
    ///
    /// 🔴 Hà 2026-08-13, ảnh chụp một tin báo có nguyên dòng
    /// `cd ~/projects/AI/codetrail && git push` mà không cái nút nào. Danh sách
    /// `KNOWN` cố tình hẹp — nó là hàng rào, không phải bảng tra — nên `cd` không
    /// nằm trong đó, và từ đầu tiên của dòng là `cd` ⟹ 0 nút.
    ///
    /// Không nới hàng rào bằng cách nhét `cd` vào `KNOWN`: `cd` một mình chẳng chạy
    /// gì, mà thêm nó là mở cửa cho mọi dòng bắt đầu bằng `cd`. Thay vào đó, nhận
    /// đúng HÌNH DẠNG: `cd <gì đó> &&|; <phần còn lại>` thì đem **phần còn lại** đi
    /// hỏi cùng cái hàng rào ấy. Hàng rào không đổi, chỉ hỏi đúng chỗ.
    fn after_cd(line: &str) -> Option<&str> {
        let rest = line.strip_prefix("cd ")?;
        let (_dir, tail) = rest.split_once("&&").or_else(|| rest.split_once(';'))?;
        let tail = tail.trim();
        (!tail.is_empty()).then_some(tail)
    }

    const KNOWN: &[&str] = &[
        "git",
        "gh",
        // 🔴 `printf` vào danh sách 2026-08-17, ảnh Hà gửi: *"Trong nội dung có
        // lệnh nhưng ko có nút"*. Hai dòng ấy là
        // `printf '@update-be …\n' > ~/projects/dwork/scripts/.cmd-queue/…cmd`
        // — tức đúng cách xếp việc vào file-queue daemon mà CLAUDE.md của
        // workspace dựng ra, và là dòng phiên BẢO CHỦ MÁY chạy. Không có nút thì
        // cây cầu hụt đúng nhịp cuối: anh phải tự gõ lại một dòng dài trên điện
        // thoại.
        //
        // Hàng rào vẫn là hàng rào: `printf` là lệnh có thật, chạy được, và
        // không phải một động từ tiếng Anh lọt vào giữa câu văn như `echo`.
        "printf",
        "npm",
        "npx",
        "node",
        "cargo",
        "bash",
        "sh",
        "zsh",
        "python3",
        "pip3",
        "docker",
        "make",
        "curl",
        "rsync",
        "scp",
        "ssh",
        "sqlite3",
        "pnpm",
        "yarn",
        "deno",
        "go",
        "rustup",
        "brew",
        "launchctl",
        "osascript",
        "open",
        "code",
        "tail",
        "grep",
        "rg",
        "find",
        "ls",
    ];
    let mut out: Vec<String> = Vec::new();
    let rows: Vec<&str> = screen.lines().collect();
    for raw in rows.iter() {
        let raw = *raw;
        // Câu đang CẤM một lệnh thì không phải câu mời chạy nó.
        if forbids(raw) {
            continue;
        }
        // 🔴 Ở ĐÂY TỪNG CÓ BỘ MÁY NỐI DÒNG BỊ BẺ — gỡ 2026-08-15.
        //
        // Nó dựng lên vì một sự thật đo được trên màn thật của `[tfl5]`
        // (08-14): ba dòng liền nhau `git … push origin main` / `bash
        // …/scripts/deploy.sh` / `static-cache-refresh-0814`, tức dòng giữa là
        // NỬA ĐẦU của một lệnh bị cửa sổ bẻ. Bấm vào nửa ấy là chạy một lệnh
        // triển khai THIẾU tham số, trên một dự án thật.
        //
        // Bộ máy ấy ĐÚNG, và vẫn là câu trả lời sai: nó đi chữa hậu quả của
        // việc đọc nhầm nguồn. Chữ trên màn là chữ đã đi qua một cửa sổ; không
        // phép đo nào dựng lại được thứ cửa sổ đã cắt. Nay nguồn đổi — lệnh lấy
        // nguyên văn từ nhật ký (`sessions::commands_of`) — nên cả cái bệnh lẫn
        // thuốc cùng đi: trần 60, phép đo bề ngang, phép đoán "dòng sau có bị
        // đẩy xuống không", và hàm nối đuôi.
        let mut line = raw.trim();
        // Dấu nhắc và dấu trang trí của TUI đứng trước lệnh.
        //
        // 🔴 `!` vào danh sách 2026-08-13, và nó là chỗ mỉa mai nhất trong tệp
        // này: `!<lệnh>` là **quy ước của chính hub** — nút `▶` gõ đúng hình
        // dạng ấy vào phiên để lệnh chạy TRONG phiên. Phiên học theo, viết
        // `! git -C … push origin main` trong báo cáo, và hub **không nhận ra
        // quy ước của chính mình**: `!` không có trong danh sách bóc nên từ đầu
        // tiên là `!`, không phải `git` ⟹ 0 nút. Hà bắt được bằng ảnh chụp:
        // *"rõ ràng có lệnh chạy trong nội dung nhưng lại không có nút để chạy
        // nó"*.
        //
        // Bóc cả hai dạng: `! git …` và `!git …`.
        for p in ["$ ", "❯ ", "> ", "⏵ ", "% ", "• ", "- ", "! ", "!"] {
            if let Some(rest) = line.strip_prefix(p) {
                line = rest.trim();
            }
        }
        // 🔴 Lệnh NẰM TRONG câu văn: đo được ngay lượt `/shot` thật đầu tiên
        // (2026-08-12 21:15) — màn có dòng
        // "`git push origin main` (a plain push to main) executed from a
        // nested-repo", và bản đầu bóc dấu nháy đầu rồi nuốt luôn cả câu phía
        // sau, ra một cái nút chạy nhầm thứ. Trong dấu nháy ngược thì CHỈ lấy
        // phần trong dấu nháy.
        let owned;
        let line = if let Some(after_tick) = line.strip_prefix('`') {
            match after_tick.split_once('`') {
                Some((inner, _)) => {
                    owned = inner.trim().to_string();
                    owned.as_str()
                }
                None => line.trim_start_matches('`').trim(),
            }
        } else {
            line
        };
        let line = line.trim();
        if line.len() < 4 || line.len() > 300 {
            continue;
        }
        // 🔴 Hà 2026-08-14: *"Cả 1 khối lệnh dài này thì không được tạo nút"* —
        // sau khi một cái nút chạy `git for-each-ref … | xargs -n1 git`, tức
        // KHÔNG phải lệnh in trong tin: nó là mẩu cụt của một khối 380 ký tự,
        // cắt đúng chỗ terminal bẻ dòng (~80 cột trừ thụt lề). Mẩu ấy cân bằng
        // nháy và mở đầu bằng một lệnh quen, nên mọi hàng rào phía dưới đều
        // thấy nó hợp lệ. Nó chạy thật, và lần ấy vô hại chỉ vì `refs/original`
        // chưa tồn tại.
        //
        // Từ 08-15 nguồn không còn bị bẻ, nên trần này thôi làm cái việc "sợ
        // mẩu cụt" và chỉ còn làm đúng một việc: một KHỐI không phải một lệnh.
        // 200 nhận trọn lệnh triển khai có đường dẫn tuyệt đối, chặn cả khối.
        if line.len() > max_len {
            continue;
        }
        // MỘT bộ luật cho cả hai lượt quét — xem `looks_like_prose`.
        if looks_like_prose(line) {
            continue;
        }
        let Some(first) = line.split_whitespace().next() else {
            continue;
        };
        // `cd X && <lệnh>`: hỏi cùng hàng rào, nhưng hỏi phần sau `&&`.
        let verb = after_cd(line)
            .and_then(|t| t.split_whitespace().next())
            .unwrap_or(first);
        let looks_like = KNOWN.contains(&verb) || verb.starts_with("./");
        if !looks_like {
            continue;
        }
        // Phải có ít nhất một tham số: `git` trần không phải một lệnh để chạy.
        if line.split_whitespace().count() < 2 {
            continue;
        }
        if network_without_target(verb, line) {
            continue;
        }
        // 🪦 CỔNG `destructive` GỠ 2026-08-16 — Hà: *"đã qua hub thì đừng có
        // chặn gì cả, các chỗ chặn bỏ hết cho tôi"* · *"tôi ở tele là phải gọi
        // lệnh thao tác như ngồi máy thì chặn khác gì chặt tay, cần kênh tele
        // để làm gì?"*.
        //
        // Nó từng bỏ mọi dòng `rm`, `git reset --hard`, `kill`… nên chúng hiện
        // ra trên điện thoại mà không có đường bấm, IM LẶNG. Hôm nay tôi vá cái
        // im lặng ấy bằng một dòng giải thích, và anh hỏi đúng câu phải hỏi:
        // *"thằng nào chặn thế, tôi có yêu cầu vậy à"*. Không ai yêu cầu.
        //
        // Cái rào thật của dự án này vẫn nguyên và nó nằm ở chỗ khác:
        // `sessions::DENIED_TOOLS` gác thứ một PHIÊN TỰ CHẠY được phép làm —
        // đó là gác một cỗ máy, không phải gác bàn tay chủ máy.
        if !out.iter().any(|x| x == line) {
            out.push(line.to_string());
        }
    }
    // …và lệnh nằm TRONG DẤU NHÁY giữa câu văn.
    //
    // 🔴 Hà 2026-08-12: *"nội dung của phiên có lệnh script cần chạy đã có tính
    // năng bấm chạy luôn chưa"*. Có, nhưng luật trên đòi lệnh **đứng đầu dòng**
    // — đúng cho một màn terminal, sai cho một BÁO CÁO. Đo trên tin báo thật
    // (`hanguyen-8e`, 33 phút chạy): nó nhắc `git fetch` và `cargo test
    // --all-targets` mà cả hai nằm giữa câu, nên ra **0 nút**.
    //
    // Dấu nháy ngược là một ranh giới CHÍNH XÁC, và chính nó vá luôn cái bẫy đã
    // trả giá hôm 08-12 tối: bản đầu bóc dấu nháy mở rồi nuốt cả câu phía sau
    // (`git push origin main` (a plain push to main) executed from…) ⟹ một nút
    // chạy nhầm thứ. Cắt đúng trong cặp nháy thì không còn chỗ cho câu văn lọt.
    let segs: Vec<&str> = text.split('`').collect();
    for i in (1..segs.len()).step_by(2) {
        let span = segs[i];
        // Chữ đứng NGAY TRƯỚC dấu nháy, trong cùng dòng ấy — chỗ câu cấm nằm.
        let before = segs[i - 1];
        let prefix = before.rsplit('\n').next().unwrap_or(before);
        if forbids(prefix) {
            continue;
        }
        let Some(cmd) = unwrap_terminal_wrap(span) else {
            continue;
        };
        let cmd = cmd.as_str();
        if cmd.len() < 4 || cmd.len() > 300 {
            continue;
        }
        // CÙNG bộ luật với lượt quét theo dòng — xem `looks_like_prose`. Thiếu
        // đúng cửa này là chỗ dòng trang trí của hub lọt ra shell.
        if looks_like_prose(cmd) || forbids(cmd) {
            continue;
        }
        let Some(first) = cmd.split_whitespace().next() else {
            continue;
        };
        let verb = after_cd(cmd)
            .and_then(|t| t.split_whitespace().next())
            .unwrap_or(first);
        if !(KNOWN.contains(&verb) || verb.starts_with("./")) {
            continue;
        }
        if cmd.split_whitespace().count() < 2 {
            continue;
        }
        // 🪦 `destructive` gỡ 2026-08-16 — xem chỗ gọi trên và `usable_command`.
        if !out.iter().any(|x| x == cmd) {
            out.push(cmd.to_string());
        }
    }
    dedupe_same_script(&mut out);
    if out.len() > max {
        out.drain(..out.len() - max);
    }
    out
}

/// Hai cách VIẾT của cùng một lệnh thì chỉ giữ một — bản cụ thể hơn.
///
/// 🔴 Hà 2026-08-14, ảnh chụp một tin báo mang ba nút: *"sao lắm nút lệnh
/// thế"*. Hai trong ba là `bash ./deploy.sh` và `bash scripts/deploy.sh` — cùng
/// một việc, viết hai kiểu ở hai chỗ khác nhau trong cùng một báo cáo. Trên màn
/// 390px, hai cái nút gần giống nhau không cho thêm lựa chọn nào; chúng bắt
/// người đọc dừng lại đoán xem cái nào mới đúng.
///
/// ⚠ Hẹp có chủ ý, và bản đầu đã quá rộng: nó gộp theo TÊN TỆP, nên ba dòng
/// `bash ./dci-deploy-be.sh module/` · `… dci/leave-quota/` · `… dci/config/…`
/// (ba việc khác nhau, chỉ trùng script) rút còn một — có test cũ bắt được
/// ngay. Luật đúng là so CẢ DÒNG sau khi rút đường dẫn script về tên tệp: chỉ
/// gộp khi hai dòng khác nhau đúng ở chỗ viết đường dẫn, còn tham số thì y hệt.
fn dedupe_same_script(out: &mut Vec<String>) {
    fn shape(cmd: &str) -> String {
        cmd.split_whitespace()
            .map(|w| {
                if w.ends_with(".sh")
                    || w.ends_with(".mjs")
                    || w.ends_with(".py")
                    || w.ends_with(".js")
                {
                    w.rsplit('/').next().unwrap_or(w)
                } else {
                    w
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
    let mut keep: Vec<String> = Vec::new();
    for cmd in out.iter() {
        let s = shape(cmd);
        match keep.iter().position(|k| shape(k) == s) {
            // Cùng một lệnh, hai cách viết: giữ bản nói rõ tệp nằm đâu.
            Some(i) => {
                if cmd.len() > keep[i].len() {
                    keep[i] = cmd.clone();
                }
            }
            None => keep.push(cmd.clone()),
        }
    }
    *out = keep;
}

/// Đuôi file hub CHẮC CHẮN không gửi — thứ cổng quét rò không đọc nổi.
///
/// 🔴 Đây từng là một danh sách TRẮNG, và nó sai ngay trong lần dùng đầu tiên
/// (2026-08-13): tôi mời Hà bấm thử vào `hub.env.example`, đuôi `.example`
/// không có trong danh sách ⟹ **không có nút nào hiện ra**. Danh sách trắng
/// bao giờ cũng thiếu — `.example`, `.gitignore`, `Makefile`, `LICENSE`, một
/// file không đuôi — trong khi câu hỏi thật chỉ có một: *cổng quét rò đọc được
/// nội dung này không?*
///
/// 🪦 Câu trả lời cũ: *"`send_document` đọc bằng `read_to_string`, file nhị phân
/// tự rơi ở đó"*. Hết đúng từ 2026-08-18 — cửa ấy dựng để phục vụ luật 5 bản
/// CŨ (*"thứ gì rời khỏi máy phải soi được"*), mà luật ấy đã đổi từ 16/08: quét
/// thì GHI, không chặn. Nên nó chỉ còn chặn được đúng bản in của chủ máy.
///
/// Danh sách dưới đây vì thế đổi vai: nó không còn nói "đọc được thành chữ",
/// nó nói **đuôi nào đủ quen để một đường TƯƠNG ĐỐI trên màn được coi là tệp** —
/// gồm cả `.docx`/`.pdf`. Câu "có thật không" vẫn để ĐĨA trả lời.
///
/// Danh sách trắng, cố ý hẹp: đường tương đối không có `/` đầu để phân biệt với
/// câu văn, nên thứ duy nhất còn phân biệt được là cái đuôi. Rộng tay ở đây thì
/// mỗi dấu chấm giữa câu thành một cái nút; thiếu một đuôi thì cùng lắm là một
/// tệp không có nút, và đường tuyệt đối vẫn chạy như cũ.
pub const TEXT_FILE_EXT: &[&str] = &[
    "md",
    "txt",
    "rs",
    "toml",
    "json",
    "hjson",
    "yaml",
    "yml",
    "sh",
    "zsh",
    "bash",
    "mjs",
    "cjs",
    "js",
    "ts",
    "tsx",
    "jsx",
    "css",
    "scss",
    "html",
    "htm",
    "py",
    "rb",
    "go",
    "java",
    "kt",
    "sql",
    "plist",
    "lock",
    "log",
    "conf",
    "ini",
    "xml",
    "csv",
    "env",
    "gitignore",
    "service",
    // 🔴 BẢN IN — thêm 2026-08-18. Hà: *"Có file docx nhưng không có nút tải"*.
    // Danh sách này sinh ra hồi hub chỉ gửi được tệp chữ, nên nó vô tình đúng
    // bằng "những gì đọc thành UTF-8 được". Cửa ấy đã bỏ (`telegram::document_body`),
    // và thứ rơi ra ngoài chính là **bản in** — `.docx` đúng chuẩn văn bản hành
    // chính, `.xlsx` bảng số, `.pdf` bản gửi đi. Tức mỗi phiên làm xong việc thì
    // hub gửi được bản nháp mà không gửi được bản thành phẩm.
    "docx",
    "xlsx",
    "pptx",
    "doc",
    "xls",
    "odt",
    "ods",
    "pdf",
];

/// Đuôi KHÔNG dựng nút — không phải "không gửi được".
///
/// 🔴 Tên cũ (`UNSENDABLE_EXT`) nói sai từ 2026-08-18: `send_document` nay gửi
/// được byte thô, nên ảnh và kho nén cũng tới nơi nếu có ai bấm. Cái danh sách
/// này chỉ còn một việc: **đừng tự mọc nút** cho thứ không ai đọc trên điện
/// thoại, và đừng biến mọi đường dẫn ảnh trong một báo cáo thành một hàng nút.
/// Ảnh màn hình đã có đường riêng (`/anh`, `send_photo`).
pub const NO_BUTTON_EXT: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "heic", "bmp", "ico", "tiff", "zip", "gz", "tgz", "bz2",
    "xz", "7z", "rar", "dmg", "pkg", "app", "sqlite", "db", "bin", "exe", "dylib", "so", "o", "a",
    "rlib", "wasm", "mp3", "mp4", "mov", "wav", "m4a", "avi", "webm", "ttf", "otf", "woff",
    "woff2",
];

/// Những ĐƯỜNG DẪN FILE hiện trên màn — thứ bấm một cái là nhận được file.
///
/// 🔴 Hà 2026-08-13: *"các nội dung có path file thì nên cho click vào nhận
/// được file để mở trực tiếp trên tele"*. Trước đó cây cầu này một chiều: hub
/// **nhận** được tệp từ Telegram (`getFile`, từ 79ee269) nhưng không gửi ra
/// được cái nào — nên một báo cáo nhắc tới `ARCHITECTURE.md` là nhắc tới thứ
/// người đọc trên điện thoại không mở nổi.
///
/// Nhận theo HÌNH DẠNG như `commands_on_screen`: đường TUYỆT ĐỐI (`/…`, `~/…`)
/// mang tên tệp, **hoặc** đường TƯƠNG ĐỐI có đuôi văn bản đã biết
/// (`TEXT_FILE_EXT`). Không nhận đuôi nhị phân.
///
/// 🔴 Đường tương đối được nhận từ 2026-08-16. Hà, đọc một bản *"Xem đầy đủ"*
/// có nhắc `docs/flow-boc-tach-lenh.md`: *"nhận được tin có file nhưng chưa có
/// nút tải hay xem"* · *"Có file .md đấy"*.
///
/// Luật cũ bỏ qua chúng vì *"`src/main.rs` trên màn không nói được nó nằm trong
/// dự án nào, đoán sai là gửi nhầm file của dự án khác"* — lo đúng, chỗ sai:
/// câu ấy đo bằng HÌNH DẠNG một thứ chỉ trả lời được bằng ĐĨA. Nay hub biết thư
/// mục của từng phiên (`pipeline::session_root`), nên đường tương đối được giải
/// theo đúng cây của phiên đã nhắc tới nó, và `sendable_file` vứt bỏ những gì
/// không phải tệp thật nằm trong cây ấy. Không tồn tại thì không có nút — chứ
/// không phải đoán rồi gửi nhầm.
///
/// Đổi lại, đuôi phải nằm trong danh sách trắng: một câu văn đầy dấu chấm và
/// dấu gạch chéo (`12/08`, `v.v.`) thì không được thành một cái nút.
///
/// Phần "có THẬT không" KHÔNG hỏi ở đây: hàm này thuần, không chạm đĩa. Nó được
/// hỏi đúng một lần, ở `pipeline::sendable_file`.
///
/// 🪦 Và câu *"có đọc được thành chữ không"* thì thôi hỏi hẳn (2026-08-18): nó
/// từng là cửa thật ở `send_document`, dựng cho một hàng rào đã gỡ từ 16/08.
/// Bản in `.docx`/`.pdf` nay gửi được — xem `telegram::document_body`.
pub fn paths_on_screen(text: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.lines() {
        if forbids(raw) {
            continue;
        }
        // Cắt theo khoảng trắng và các dấu bao quanh hay gặp trong câu văn.
        for tok in raw.split(|c: char| c.is_whitespace() || "`\"'()[]{}<>,;".contains(c)) {
            // 🔴 Đường dẫn BỊ CẮT CỤT không phải một đường dẫn.
            //
            // Hà 2026-08-13, ảnh chụp Telegram: ba nút 📎 dưới một tin, trong đó
            // `Cargo.toml` và `Cargo.toml…` — hai nút, một file. Cái thứ hai
            // sinh ra từ chính màn hình: TUI cắt dòng lệnh dài rồi dán `…` vào
            // cuối (`--manifest-path …/rust/Cargo.toml… (46s · 2 lines)`), và
            // vòng quét này đọc `Cargo.toml…` thành một đường dẫn khác.
            //
            // Bỏ hẳn, đừng gọt `…` rồi dùng: gọt xong thì đúng ở ca này mà SAI
            // ở ca `…/rust/Car…` — cắt cụt giữa tên file thì phần còn lại là
            // một đường dẫn hợp lệ về hình dạng và trỏ vào hư không. Một cái
            // nút không bao giờ mở được là một lời hứa suông trên màn hình.
            if tok.contains('…') || tok.contains("...") {
                continue;
            }
            let t = tok.trim_end_matches(['.', ':', '?', '!', ',']);
            if t.len() < 4 {
                continue;
            }
            let absolute = t.starts_with('/') || t.starts_with("~/");
            // Phải có TÊN FILE, không phải một thư mục: đoạn cuối có dấu chấm.
            let Some(last) = t.rsplit('/').next().filter(|l| l.contains('.')) else {
                continue;
            };
            let ext = last.rsplit('.').next().unwrap_or_default().to_lowercase();
            if NO_BUTTON_EXT.contains(&ext.as_str()) {
                continue;
            }
            // Đường TƯƠNG ĐỐI (kể cả TÊN TỆP TRẦN) chỉ cần mang đuôi văn bản
            // đã biết — câu hỏi "có thật không" để ĐĨA trả lời.
            //
            // 🔴 Hà 2026-08-17, ảnh `/shot` phiên `[dwork]`: *"Trong nội dung có
            // file *.md chưa chèn link tải, phải tìm được file ở đĩa"*. Màn ấy
            // có `TODO.md`, `active-context.md` viết trần, và
            // `docs/chuyen-doi-thiet-ke-2026-08-16h.md` bị cửa sổ **bẻ đôi** —
            // `docs/` nằm cuối một dòng, tên tệp nằm ở dòng sau. Đòi token phải
            // chứa `/` là loại sạch cả ba, mà cả ba đều là tệp có thật.
            //
            // Hàng rào không nằm ở hình dạng nữa mà ở `pipeline::sendable_file`:
            // giải theo cây của đúng phiên, và không thấy thì ĐI TÌM trong cây
            // ấy — khớp đúng một tệp mới dựng nút, nhiều khớp thì thôi (đoán
            // giữa hai tệp cùng tên là gửi nhầm). `Node.js` vẫn không thành nút,
            // chỉ khác là nay nó chết vì không có tệp nào tên vậy, chứ không
            // phải vì thiếu dấu gạch chéo.
            if !absolute && !TEXT_FILE_EXT.contains(&ext.as_str()) {
                continue;
            }
            if !out.iter().any(|x| x == t) {
                out.push(t.to_string());
            }
        }
    }
    if out.len() > max {
        out.drain(..out.len() - max);
    }
    out
}

/// Đây là CÂU VĂN chứ không phải một dòng lệnh?
///
/// 🔴 Hà 2026-08-13, ảnh chụp màn phiên codetrail: *"bấm vào nút chạy lệnh thì
/// bị dính text ngoài như này"*. Thứ hub gõ vào phiên là:
///
/// ```text
/// ! ▶ Lệnh thấy trên màn (bấm nút dưới để gõ `!` vào chính phiên): • git -C … push origin main
/// (eval):1: no matches found: (bấm nút dưới để gõ  vào chính phiên):
/// ```
///
/// Tức **hub đọc lại chính dòng trang trí của nó** rồi biến thành lệnh. Cú push
/// không hề chạy, mà nhìn thì như đã bấm.
///
/// Hai lỗ cùng lúc, và cái thứ hai mới đáng sợ:
/// * `/shot` tự đính dòng *"▶ Lệnh thấy trên màn…"* vào bản trả lời, rồi lượt
///   quét sau đọc luôn cả dòng ấy — **một vòng tự ăn chính mình**.
/// * Lượt quét trong DẤU NHÁY thiếu sạch các cửa lọc câu văn mà lượt quét theo
///   DÒNG đã có từ lâu (`" ("`, `", "`, dấu câu cuối). Hai lượt quét, hai bộ
///   luật khác nhau, và không ai nhìn thấy sự lệch cho tới khi nó gõ ra shell.
///
/// Nay một bộ luật, dùng cho cả hai lượt — kể cả một cửa nhận ra CHÍNH chữ hub
/// in ra màn.
fn looks_like_prose(s: &str) -> bool {
    // Câu văn thường mang mệnh đề trong ngoặc hoặc dấu phẩy; dòng lệnh thật thì
    // hiếm khi có. Thà bỏ sót một nút còn hơn dựng một cái nút chạy nhầm thứ.
    s.contains(" (")
        || s.contains(", ")
        || s.ends_with('.')
        || s.ends_with(':')
        || s.ends_with('?')
        // …và chữ của CHÍNH hub trên màn thì tuyệt đối không phải lệnh.
        || s.contains("Lệnh thấy trên màn")
        || s.contains("bấm nút")
        // 🔴 DẤU CỦA VĂN XUÔI, không bao giờ có trong một dòng shell.
        //
        // Hà 2026-08-13, ảnh chụp nút `▶ cargo test 258 · clippy 0 warning`:
        // *"Thực sự mấy cái nút đọc không dám bấm vì không thể hiểu nó làm
        // gì"*. Anh đúng, và cái nút ấy là **câu trong báo cáo của chính
        // tôi** — một dòng tổng kết, không phải lệnh. Bấm vào là chạy
        // `cargo test 258 · clippy 0 warning`, một thứ vô nghĩa.
        //
        // `looks_like_prose` bắt dấu phẩy và dấu ngoặc, nhưng câu ấy dùng
        // dấu chấm giữa `·` để ngăn vế — thói quen viết của chính tôi, và
        // nó lọt sạch mọi cửa. Ba ký tự dưới đây là dấu ĐÁNH MÁY của văn
        // xuôi: `·` `—` `…`. Không dòng lệnh thật nào mang chúng, nên bắt
        // chúng không bỏ sót cái nút nào có thật.
        || s.contains('·')
        || s.contains('—')
        || s.contains('…')
        // Kết bằng một con số trần: `cargo test 258` là câu ĐẾM, không phải
        // lệnh. Cùng họ với luật trên — văn xuôi đội lốt lệnh.
        || ends_with_bare_number(s)
}

/// `cargo test 258` · `git push 42` — từ CUỐI là một con số trần.
///
/// Tham số của một lệnh thật là tên, cờ, hay đường dẫn; số trần đứng cuối gần
/// như luôn là một con số đếm trong câu văn. Bỏ sót vài lệnh hiếm có hình dạng
/// ấy là cái giá rẻ: bỏ sót thì gõ tay, bấm nhầm thì không có nút hoàn tác.
fn ends_with_bare_number(s: &str) -> bool {
    s.split_whitespace()
        .next_back()
        .is_some_and(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()))
}

/// Câu này đang CẤM một lệnh, chứ không mời chạy nó?
///
/// 🔴 Trả giá ngay trong ngày đặt tính năng, 2026-08-13: một bộ gác lệnh từ
/// chối `git filter-branch` và in ra câu giải thích **chứa chính lệnh ấy trong
/// dấu nháy**. hub đọc màn, thấy hình dạng một lệnh, và gửi cho Hà ba cái nút —
/// trong đó có `▶ git filter-branch --force`. Tức tính năng "bấm là chạy" vừa
/// biến một lời cảnh báo thành **một cú bấm là làm đúng cái điều bị cấm**.
///
/// Bài học không phải "thêm một cửa nữa" mà là: nhận diện theo hình dạng thì
/// không đọc được Ý — và ý duy nhất bắt buộc phải đọc là *"đừng chạy cái này"*.
/// Dấu hiệu lấy hẹp và rõ, chấp nhận bỏ sót vài cái nút đúng, vì cán cân ở đây
/// lệch hẳn: bỏ sót thì gõ tay, bấm nhầm thì không có nút hoàn tác.
fn forbids(context: &str) -> bool {
    const MARKS: &[&str] = &[
        "block",
        "⚠",
        "❌",
        "🔴",
        "never",
        "do not",
        "don't",
        "denied",
        "refus",
        "dangerous",
        "đừng",
        "cấm",
        "không được",
        "không nên",
        "từ chối",
        "nguy hiểm",
        "thay vì",
        // 🔴 Thêm 2026-08-14, sau khi Hà bấm một nút và nhận về một lệnh xoá
        // trần: *"Nút lệnh chạy ko đúng"*. Chữ ấy hub bắt được từ một THÔNG BÁO
        // CHẶN của hook — *"the command runs … which permanently deletes tracked
        // source files … Safer form: …"*. Cả đoạn là lời CẤM một lệnh, và hub
        // đọc nó thành lời MỜI chạy lệnh ấy.
        //
        // Mấy mẫu dưới là chữ ký của loại văn bản đó, không phải của một câu
        // mời: một lời cảnh báo bao giờ cũng nói hậu quả ("permanently",
        // "irreversible") hoặc đưa đường thay thế ("safer form"), còn
        // "pretooluse" thì đúng tên cái cổng đã chặn.
        "safer form",
        "permanently",
        "irreversible",
        "pretooluse",
        "hook stopped",
    ];
    let c = context.to_lowercase();
    MARKS.iter().any(|m| c.contains(m))
}

// 🪦 `destructive(cmd)` — GỠ HẲN 2026-08-16.
//
// Nó giữ một danh sách `rm`, `git reset --hard`, `git clean`, `kill`,
// `drop table`, `launchctl bootout`… và MỌI dòng khớp danh sách ấy đều không
// được dựng nút. Lý do viết ra hồi 14/08 nghe rất hợp lý: *"một cái nút mời
// làm việc không lùi lại được là một cái bẫy"*.
//
// Hà gỡ nó bằng một câu ngắn hơn cả cái lý do ấy — 2026-08-16:
//
//   *"tôi ở tele là phải gọi lệnh thao tác như ngồi máy thì chặn khác gì chặt
//   tay, cần kênh tele để làm gì?"*
//
// Đó chính là phép thử của cả dự án, phát biểu ngược lại. `CLAUDE.md` viết:
// *"Anything he can do at the terminal but not from the phone is a gap."*
// Ngồi ở máy anh gõ `rm` không ai hỏi câu nào; hub từ chối dựng nút cho đúng
// dòng ấy nên nó tự tay tạo ra một khoảng cách — rồi im lặng về việc đó, nên
// từ điện thoại nhìn ra y hệt "hub không đọc được lệnh".
//
// Cái rào THẬT của dự án không nằm ở đây và không đổi:
// `sessions::DENIED_TOOLS` gác thứ một PHIÊN TỰ CHẠY được phép làm (luật 1).
// Gác một cỗ máy chạy không người trông là một chuyện; gác bàn tay chủ máy là
// một chuyện khác, và tệp này đã lẫn hai chuyện ấy suốt hai ngày.

/// Nối lại một lệnh bị MÀN HÌNH bẻ dòng — hoặc từ chối, nếu không chắc.
///
/// 🔴 Hà 2026-08-13, ảnh chụp Telegram: *"Không có lệnh merge mà bấm"*. Màn của
/// phiên tfl5 lúc 11:15 kết bằng đúng một dòng lệnh để gõ, và hub không dựng nổi
/// một cái nút nào cho nó. Lấy nguyên chữ hub đã gửi ra khỏi nhật ký thì thấy
/// ngay vì sao — lệnh dài hơn bề ngang cửa sổ nên TUI bẻ nó làm hai:
///
/// ```text
/// deploy with `bash scripts/deploy.sh walk-fixes-0813 --expect-symbol
///   renderChatPending`. (disable recaps in /config)
/// ```
///
/// …và cổng `contains('\n')` (viết 08-12) vứt thẳng. Cổng ấy đúng ý mà sai
/// hình: nó định loại KHỐI CHỮ nhiều dòng, nhưng thứ nó loại được nhiều nhất
/// lại là **lệnh dài** — đúng những lệnh đáng có nút nhất, vì không ai cần một
/// cái nút để gõ `ls`.
///
/// Nối lại thì phải nối cho ĐÚNG, nên chỉ nối khi chỗ bẻ rơi vào **ranh giới
/// từ**: đo trên chữ thật, TUI của `claude` bẻ sau dấu cách rồi thụt đầu dòng
/// tiếp (`--expect-symbol ␣\n␣␣renderChatPending`, `git rev-parse ␣\n␣␣␣␣␣
/// --show-toplevel`). Bẻ GIỮA một từ thì nối lại là bịa ra một lệnh khác — thà
/// mất một cái nút, nên trường hợp ấy trả `None`.
fn unwrap_terminal_wrap(span: &str) -> Option<String> {
    if !span.contains('\n') {
        return Some(span.trim().to_string());
    }
    let parts: Vec<&str> = span.split('\n').collect();
    // Một lệnh bị bẻ vài lần thì còn là một lệnh; bẻ năm lần thì đây là một khối
    // chữ, và khối chữ không phải thứ để bấm.
    if parts.len() > 4 {
        return None;
    }
    for w in parts.windows(2) {
        // Dòng trống = ngắt đoạn văn, không phải bẻ dòng.
        if w[0].trim().is_empty() || w[1].trim().is_empty() {
            return None;
        }
        let word_boundary =
            w[0].ends_with(char::is_whitespace) || w[1].starts_with(char::is_whitespace);
        if !word_boundary {
            return None;
        }
    }
    Some(
        parts
            .iter()
            .map(|p| p.trim())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string(),
    )
}

/// Hộp chọn đang chờ trên màn, nếu có.
///
/// `claude` vẽ hộp chọn dạng:
///
/// ```text
///   Câu hỏi ở đây?
///   ❯ 1. Phương án một
///     2. Phương án hai
/// ```
///
/// Nhận diện bằng HÌNH DẠNG (`❯` hoặc `N.` đứng đầu dòng), không bằng cách
/// đoán nội dung: câu hỏi là chữ của người khác viết, hình dạng mới là thứ
/// `claude` bảo đảm.
/// Chuỗi phím đưa con trỏ tới dòng **Submit** rồi Enter — cho hộp CHỌN NHIỀU.
///
/// 🔴 Hà 2026-08-17, sau khi bấm đủ bốn lựa chọn rồi `/send_…`: *"Ko qua nổi màn
/// này"*. Đúng, và một dấu Enter trần không bao giờ qua được: trong hộp chọn
/// nhiều, Enter tác động lên DÒNG CON TRỎ ĐANG ĐỨNG — tức bật/tắt đúng cái ô
/// vừa chọn — còn thứ gửi bảng đi là một dòng riêng tên `Submit`, không mang số
/// nên không bấm số tới được.
///
/// Ba cửa, và cả ba đều đo trên chính màn ấy chứ không đoán:
/// · dòng chân phải NÓI `↑/↓ to navigate` — chính TUI khai rằng mũi tên chỉ di
///   chuyển, nên ở đây nó không phạm luật "mũi tên vừa move vừa confirm";
/// · phải thấy dòng `Submit`;
/// · phải thấy con trỏ `❯` để biết đang đứng đâu.
/// Thiếu một cái ⟹ `None`, và chỗ gọi rơi về Enter trần như cũ.
/// 🔴 `Submit` KHÔNG nằm trong vòng đi dọc — đo 2026-08-17, và bản trước đó tin
/// ngược lại nên nó **chốt hụt và lật thêm một ô**.
///
/// Phép đo: hộp 4 lựa chọn, con trỏ ở mục 4, gửi đúng kế hoạch cũ (`↓↓` rồi
/// Enter) để tới dòng `Submit` nằm ngay dưới mục 5. Kết quả: con trỏ **quấn về
/// mục 1** (4→5→1) và cú Enter cuối lật mất dấu của mục 1. Bảng vẫn mở nguyên.
/// Vòng đi dọc chỉ gồm những dòng `<số>. [` — dòng `Submit` và dòng không có ô
/// (`6. Chat about this`) không phải chỗ con trỏ dừng được.
///
/// Đường thật là thanh tab NGANG: `←  ☒ Pick  ✔ Submit  →`. Từ câu đang mở, mỗi
/// `→` sang một tab, và tab cuối là nút gửi. Đo trọn vòng trên hộp thật: tick
/// mục 2 và 4, rồi `[enter] · [→] · [enter]` ⟹ phiên nhận đúng
/// `Chon muc nao? → Beta, Delta`, không thừa không thiếu một mục.
///
/// `at_question` là câu ĐANG MỞ (0 là câu đầu), đọc từ màn bằng
/// [`cursor_on`] — không đếm phím đã bấm, vì chủ máy có thể vừa tự bấm.
pub fn submit_plan(screen: &str, at_question: usize) -> Option<Vec<Vec<String>>> {
    if !has_submit(screen) {
        return None;
    }
    // Không thấy thanh tab (nó có thể nằm ngoài khung màn đọc được) ⟹ coi như
    // bảng MỘT câu: một bước `→`. Đó là hình dạng thường gặp, và nếu đoán sai
    // thì bảng vẫn mở — chỗ gọi đọc lại màn rồi nói đúng thứ đã xảy ra.
    let questions = ask_table(screen).map(|t| t.answered.len()).unwrap_or(1);
    let steps = questions.saturating_sub(at_question).max(1);
    let enter = || vec!["enter".to_string()];
    let rights: Vec<String> = std::iter::repeat_n("right".to_string(), steps).collect();
    Some(vec![enter(), rights, enter()])
}

/// Màn này có nút gửi bấm tới được không — dùng để quyết định có gắn ✅ hay
/// không, nên nó KHÔNG phụ thuộc con trỏ đang đứng đâu.
pub fn has_submit(screen: &str) -> bool {
    is_checkbox_list(screen) && screen.contains("to navigate") && screen.lines().any(is_submit_line)
}

/// Dòng `Submit` của hộp chọn nhiều — mục duy nhất không mang số.
fn is_submit_line(l: &str) -> bool {
    let t = l.trim().trim_start_matches('\u{276f}').trim();
    t == "Submit" || t.ends_with(" Submit")
}

/// Một MỤC con trỏ dừng được: dòng mang `<số>.` **và có ô** `[ ]`.
///
/// Hai thứ bị loại ra, cả hai đều đo được chứ không suy:
/// · **Phần mô tả thụt vào** dưới mỗi lựa chọn không phải mục — bản đầu (sáng
///   17/08) đếm theo DÒNG, nên với hộp mà mỗi lựa chọn kéo hai ba dòng giải
///   thích, con trỏ dừng giữa đường và cú Enter rơi vào một lựa chọn.
/// · **Dòng `Submit` và dòng không có ô** (`6. Chat about this`) — chiều 17/08:
///   con trỏ ở mục 4, hai `↓` để tới `Submit` nằm ngay dưới mục 5, mà con trỏ
///   **quấn về mục 1** (4→5→1). Vòng đi dọc đóng lại ở mục cuối CÓ Ô; `Submit`
///   tới bằng `→` trên thanh tab (xem [`submit_plan`]).
fn is_item_line(l: &str) -> bool {
    let t = l.trim().trim_start_matches('\u{276f}').trim();
    t.split_once('.').is_some_and(|(n, rest)| {
        n.trim()
            .parse::<usize>()
            .is_ok_and(|n| (1..=9).contains(&n))
            && rest.trim_start().starts_with('[')
    })
}

/// Đây có phải hộp CHỌN NHIỀU không — nhận bằng ô `[ ]` / `[✓]` trên nhãn.
///
/// 🔴 Hà 2026-08-17: *"Mà chèn [] là tương ứng chọn được nhiều à"*. Đúng, và nó
/// là dấu hiệu ĐO ĐƯỢC cho một khác biệt đắt: hộp chọn MỘT nhận phím số
/// (`/key <số>`, chạy từ 13/08), còn hộp CHỌN NHIỀU thì KHÔNG — dòng chân của
/// nó chỉ khai `Enter to select · ↑/↓ to navigate`. Đo thật hôm ấy: hub bấm
/// `1`, log ghi "đã bấm '1'", màn không đổi một ô nào; rồi `/shot` lại vẫn thấy
/// `[ ]` trống trơn.
///
/// Nên trong hộp này, "chọn mục n" nghĩa là ĐI TỚI mục n rồi Enter.
pub fn is_checkbox_list(screen: &str) -> bool {
    screen
        .lines()
        .filter(|l| {
            let t = l.trim().trim_start_matches('\u{276f}').trim();
            t.split_once('.').is_some_and(|(n, rest)| {
                n.trim().parse::<usize>().is_ok() && rest.trim_start().starts_with('[')
            })
        })
        .count()
        >= 2
}

/// BA LƯỢT GHI đưa con trỏ tới MỤC ở dòng `target_line` rồi Enter — mà chỉ đúng
/// một ô đổi dấu.
///
/// Vì sao ba, chứ không phải "mấy mũi tên rồi Enter": mỗi lượt ghi tự mang một
/// CR không gỡ được, tức **mỗi lượt là một cú bật/tắt vào dòng đang đứng** (đo:
/// xem [`press_writes`]). Nên bản cũ — một mũi tên một lượt, rồi một Enter —
/// bật/tắt đúng bấy nhiêu ô dọc đường: đi từ mục 2 lên mục 1 thì mục 2 mất dấu,
/// và đó chính là *"bấm cái nọ mất cái kia"*.
///
/// Ba lượt, xếp cho các cú bật/tắt tự triệt tiêu nhau:
/// 1. `enter` tại chỗ ⟹ ô ĐANG ĐỨNG đổi dấu (hỏng có chủ ý, sẽ trả lại ngay);
/// 2. cả k mũi tên trong MỘT lượt ⟹ ô đang đứng đổi dấu **lần hai** (về đúng
///    như cũ) và con trỏ đi trọn k bước;
/// 3. `enter` ⟹ ô ĐÍCH đổi dấu. Một cú, đúng chỗ.
///
/// Con trỏ dừng lại ở mục đích — đúng chỗ tay người sẽ để nó lại — nên bấm lại
/// chính ô ấy chỉ tốn một lượt ghi.
///
/// Đích là dòng `Submit` thì lượt 3 là cú CHỐT, và hai lượt đầu đã lo cho bảng
/// đi đúng bằng những ô chủ máy đã chọn. Bản cũ chốt **sau khi** đã lỡ bật/tắt
/// từng ô nó đi ngang qua — sai lặng lẽ, và không lùi lại được.
///
/// Cửa an toàn giữ nguyên: dòng chân phải khai `↑/↓ to navigate` — chính TUI nói
/// mũi tên chỉ di chuyển. Không có dòng ấy ⟹ `None`, chỗ gọi giữ đường cũ.
pub fn nav_plan(screen: &str, target_line: usize) -> Option<Vec<Vec<String>>> {
    if !screen.contains("to navigate") {
        return None;
    }
    let lines: Vec<&str> = screen.lines().collect();
    let items: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| is_item_line(l))
        .map(|(i, _)| i)
        .collect();
    let cursor_line = lines
        .iter()
        .position(|l| l.trim_start().starts_with('\u{276f}'))?;
    let from = items.iter().position(|i| *i == cursor_line)?;
    let to = items.iter().position(|i| *i == target_line)?;
    let delta = to as isize - from as isize;
    let enter = || vec!["enter".to_string()];
    // Đã đứng sẵn ở đó thì không có quãng đường nào để đi, nên cũng không có ô
    // nào phải trả lại: đúng một lượt ghi, đúng một cú bật/tắt.
    if delta == 0 {
        return Some(vec![enter()]);
    }
    let key = if delta > 0 { "down" } else { "up" };
    let arrows: Vec<String> = std::iter::repeat_n(key.to_string(), delta.unsigned_abs()).collect();
    Some(vec![enter(), arrows, enter()])
}

/// Bao nhiêu ô đã tick / tổng số ô, trên một hộp CHỌN NHIỀU.
///
/// Dùng để ack nói KẾT QUẢ chứ không nói hành động: "đã bấm '3'" chỉ khai rằng
/// phím rời khỏi hub, còn "3/5 ô đã chọn" mới là thứ người ở xa cần biết — và
/// nó bắt được cả ca phím tới nơi nhưng rơi vào mục khác.
pub fn ticked(screen: &str) -> (usize, usize) {
    let marks = tick_marks(screen);
    (marks.iter().filter(|(_, on)| *on).count(), marks.len())
}

/// Từng ô một: `(số mục, đang tick hay không)`, theo đúng thứ tự trên màn.
///
/// Tách ra khỏi [`ticked`] để có thứ con số tổng không nói được: ô NÀO đổi. Một
/// cú bấm đúng làm đổi đúng một ô, nên "mấy ô đổi" là phép đo bắt được đúng lỗi
/// *"bấm cái nọ mất cái kia"* — thứ mà `3/5` vẫn xanh rờn khi hai ô cùng lật.
pub fn tick_marks(screen: &str) -> Vec<(usize, bool)> {
    let mut out = Vec::new();
    for l in screen.lines() {
        let t = l.trim().trim_start_matches('\u{276f}').trim();
        let Some((n, rest)) = t.split_once('.') else {
            continue;
        };
        let Ok(n) = n.trim().parse::<usize>() else {
            continue;
        };
        if let Some(inside) = rest
            .trim_start()
            .strip_prefix('[')
            .and_then(|r| r.split(']').next())
        {
            out.push((n, !inside.trim().is_empty()));
        }
    }
    out
}

/// Những mục có dấu tick KHÁC nhau giữa hai màn.
///
/// Rỗng = không ô nào đổi (cú bấm rơi vào đâu mất). Một phần tử = đúng một ô
/// đổi, dạng duy nhất được coi là lành. Từ hai trở lên = hub vừa lật hộ chủ máy
/// một ô anh không bấm, và chỗ gọi PHẢI nói ra chứ không được nuốt.
///
/// So theo SỐ MỤC chứ không theo vị trí trong danh sách: hộp có thể vẽ lại
/// (mô tả dài ra, dòng gấp khúc), còn số mục thì không đổi giữa hai nhịp.
pub fn ticks_changed(before: &str, after: &str) -> Vec<usize> {
    let a = tick_marks(before);
    let b = tick_marks(after);
    b.iter()
        .filter(|(n, on)| a.iter().any(|(m, was)| m == n && was != on))
        .map(|(n, _)| *n)
        .collect()
}

/// Chuỗi phím để bật/tắt LỰA CHỌN số `n` trong một hộp CHỌN NHIỀU.
///
/// `None` khi màn không phải hộp checkbox hoặc không thấy mục ấy — chỗ gọi giữ
/// đường cũ (gõ thẳng con số), thứ vẫn đúng cho hộp chọn một.
pub fn checkbox_plan(screen: &str, n: usize) -> Option<Vec<Vec<String>>> {
    if !is_checkbox_list(screen) {
        return None;
    }
    let target = screen.lines().position(|l| {
        let t = l.trim().trim_start_matches('\u{276f}').trim();
        t.split_once('.')
            .is_some_and(|(num, _)| num.trim().parse::<usize>() == Ok(n))
    })?;
    nav_plan(screen, target)
}

pub fn parse_choices(screen: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String, usize)> = Vec::new();
    for (idx, line) in screen.lines().enumerate() {
        let t = line.trim();
        // Bỏ dấu con trỏ ❯ nếu có, rồi tìm "<số>." ở đầu.
        let t = t.strip_prefix('❯').map(str::trim_start).unwrap_or(t);
        let Some((num, rest)) = t.split_once('.') else {
            continue;
        };
        let Ok(n) = num.trim().parse::<usize>() else {
            continue;
        };
        if n == 0 || n > 9 {
            continue;
        }
        let label = rest.trim();
        // Một dòng "1." trống không phải lựa chọn; một dòng dài lê thê cũng
        // không — hộp chọn của claude là nhãn ngắn.
        if label.is_empty() || label.len() > 120 {
            continue;
        }
        out.push((n, label.to_string(), idx));
    }
    // Số phải liên tiếp từ 1: "3. xong" trong một đoạn văn không phải hộp chọn.
    if out.is_empty() || out[0].0 != 1 {
        return Vec::new();
    }
    for (i, (n, _, _)) in out.iter().enumerate() {
        if *n != i + 1 {
            return Vec::new();
        }
    }
    // Một lựa chọn duy nhất thì không có gì để chọn.
    if out.len() < 2 {
        return Vec::new();
    }
    // LIỀN DÒNG NHAU. Đây là chỗ hình dạng thật khác hẳn một đoạn văn có đánh
    // số, và bỏ nó ra thì cái chuông kêu nhầm — đo thật 2026-08-11: một câu
    // TRẢ LỜI của phiên có ba gạch đầu dòng "1. / 2. / 3." bị đọc thành hộp
    // chọn, hub bắn `⚠ dừng lại HỎI — cần bạn chọn` kèm nguyên văn ba dòng ấy
    // cho một phiên chẳng hỏi gì ai. Chuông kêu nhầm dạy người ta thôi nghe
    // chuông — đắt ngang một phép đo mù, chỉ hỏng theo chiều ngược lại.
    //
    // `claude` vẽ hộp chọn thành các dòng NGẮN nối tiếp nhau; văn xuôi có đánh
    // số thì giữa hai mục luôn có dòng chữ tràn của mục trước. Cho phép dòng
    // TRỐNG xen giữa (hộp có thể giãn dòng), không cho phép dòng có chữ.
    //
    // 🔴 …TRỪ KHI hộp tự khai nó là hộp — 2026-08-16. Hà, ảnh chụp màn
    // `[AI/mailler]`: *"Màn có option nhưng không có bảng chọn"*. Màn ấy có 5
    // lựa chọn rành rành, và hàm này trả về **0**: hộp `AskUserQuestion` viết
    // mỗi lựa chọn kèm một dòng MÔ TẢ thụt lề bên dưới, nên "liền dòng" không
    // bao giờ đúng cho nó. Luật chống-kêu-nhầm ở trên loại đúng thứ nó phải
    // nhận.
    //
    // Dấu hiệu phân biệt không phải khoảng cách dòng, mà là dòng CHÂN mà chính
    // `claude` vẽ: *"Enter to select · ↑/↓ to navigate · Esc to cancel"*. Một
    // đoạn văn có đánh số không bao giờ mang dòng ấy. Có nó thì mô tả xen giữa
    // là chuyện thường; không có nó thì giữ nguyên luật cũ.
    if !has_chooser_footer(screen) {
        let lines: Vec<&str> = screen.lines().collect();
        for w in out.windows(2) {
            let (from, to) = (w[0].2, w[1].2);
            if lines[from + 1..to].iter().any(|l| !l.trim().is_empty()) {
                return Vec::new();
            }
        }
    }
    out.into_iter().map(|(n, l, _)| (n, l)).collect()
}

/// Màn có đang vẽ CHÂN của một hộp chọn không.
///
/// Đây là câu "hộp tự khai nó là hộp": `claude` in dòng điều hướng ấy ngay dưới
/// danh sách, và không đoạn văn nào có nó. Khớp lỏng theo TỪNG mảnh để không
/// gãy khi CLI đổi dấu phân cách hay thêm phím tắt.
pub fn has_chooser_footer(screen: &str) -> bool {
    // 🔴 CLI có ÍT NHẤT HAI kiểu dòng chân, và bản cũ chỉ biết một — Hà
    // 2026-08-16, ảnh chụp `[dwork]` kẹt hơn ba tiếng ở hộp *"Set up auto mode
    // for your environment?"*. Hai bản đo được trên chính máy này:
    //
    //   Enter to select · ↑/↓ to navigate · Esc to cancel   (hộp khảo sát)
    //   Enter to confirm · Esc to cancel                    (hộp bật auto mode)
    //
    // Bản cũ đòi có chữ *"to select"*, nên với hộp thứ hai nó trả `false` —
    // trong khi `parse_choices` NGAY TRÊN CÙNG MÀN ẤY đọc ra đủ ba lựa chọn.
    // Hai câu trả lời khác nhau cho cùng một câu hỏi, trên cùng một màn: đó là
    // chỗ hỏng, không phải cái footer.
    //
    // Và hậu quả không nhẹ. `prompt_line_text` dùng hàm này làm cổng; cổng mở
    // ⟹ nó quét ngược tìm dòng `❯`, mà lúc ô nhập trống thì dòng `❯` duy nhất
    // là **con trỏ đang trỏ vào một lựa chọn** (`❯ 1. Set it up`). hub đọc đó
    // thành "chữ trong ô nhập", dựng nút `⏎ Gửi`, và một cú Enter lúc màn đang
    // mở hộp chọn thì **XÁC NHẬN lựa chọn số 1** chứ không gửi gì (luật 13).
    // Tức cái nút ấy mời chủ máy bật auto mode mà tưởng mình đang gửi một câu.
    //
    // Đo TỪNG DÒNG chứ không đo cả màn: dòng chân thật là MỘT dòng, còn hai
    // mảnh rời nằm cách nhau hai mươi dòng văn xuôi thì chỉ là trùng chữ.
    screen.lines().any(|line| {
        let l = line.to_lowercase();
        (l.contains("to select") || l.contains("to confirm") || l.contains("để chọn"))
            && (l.contains("to navigate") || l.contains("to cancel") || l.contains("để huỷ"))
    })
}

/// Thanh tab của một bảng hỏi nhiều câu, đọc từ màn.
#[derive(Debug, Clone, PartialEq)]
pub struct AskTable {
    /// Mỗi câu một mục, ĐÚNG thứ tự trái→phải: đã trả lời hay chưa.
    pub answered: Vec<bool>,
    /// Nhãn ngắn từng câu, dùng để ghép với `sessions::Asking` đọc từ nhật ký.
    pub headers: Vec<String>,
}

impl AskTable {
    /// Còn mấy câu trống. `0` nghĩa là bảng đã sẵn sàng gửi.
    pub fn left(&self) -> usize {
        self.answered.iter().filter(|a| !**a).count()
    }
}

/// Bảng hỏi NHIỀU CÂU đang mở trên màn, nếu có.
///
/// 🔴 Hà 2026-08-13: *"chọn option xong thì vẫn còn bước nữa nên không pass qua
/// được"*. Bảng nhiều câu vẽ một thanh tab, và **chỉ thanh ấy** nói ra cái ràng
/// buộc chết người: bảng không gửi đi được chừng nào còn một ô trống.
///
/// Đọc theo KÝ TỰ ĐÃ ĐO trên máy này, không theo ký tự đoán từ ảnh chụp —
/// `rust/tests/ask_table_live.rs` in ra nguyên văn: `←  ☒ Vá ACL  ☐ Đăng nhập
/// ✔ Submit  →`, tức `☒ U+2612` / `☐ U+2610` / `✔ U+2714`. Ảnh chụp điện thoại
/// không phân biệt nổi `☒` với `⊠ U+22A0`, và viết theo cái đoán thì hàm này
/// đếm ra 0 ở mọi màn mà vẫn "chạy đúng" — đúng cái phép đo mù mà
/// `OPERATING-CHARTER.md` §2d dựng ra để tránh.
///
/// Nhận diện cả dòng chứ không bắt từng ký tự rời: phải có mũi tên chỉ dẫn hai
/// đầu (`←` … `→`) VÀ ít nhất một ô. Một dòng văn xuôi lỡ mang chữ `☐` thì
/// không đủ điều kiện, nên hàm không dựng ra một cái bảng không có thật.
pub fn ask_table(screen: &str) -> Option<AskTable> {
    for line in screen.lines() {
        if !line.contains('←') || !line.contains('→') {
            continue;
        }
        let mut answered = Vec::new();
        let mut headers: Vec<String> = Vec::new();
        for ch in line.chars() {
            match ch {
                '☒' | '☑' => {
                    answered.push(true);
                    headers.push(String::new());
                }
                '☐' | '□' => {
                    answered.push(false);
                    headers.push(String::new());
                }
                // `✔ Submit` là NÚT GỬI, không phải một câu hỏi — đếm nó vào
                // thành một câu là khai bảng dài hơn thật, và mọi phép "còn mấy
                // câu trống" lệch theo.
                '✔' | '✓' => break,
                _ => {
                    if let Some(last) = headers.last_mut() {
                        last.push(ch);
                    }
                }
            }
        }
        if answered.is_empty() {
            continue;
        }
        return Some(AskTable {
            answered,
            headers: headers.into_iter().map(|h| h.trim().to_string()).collect(),
        });
    }
    None
}

/// Bảng đang đứng ở CÂU NÀO, ghép bằng chữ chứ không bằng màu.
///
/// Tab đang chọn được vẽ bằng nền tím, mà `contents of tab` trả chữ TRẦN — màu
/// không đi qua đường ấy. Nên vị trí con trỏ phải suy từ thứ đọc được: dưới
/// thanh tab, bảng in nguyên văn câu hỏi đang mở. Ghép câu ấy với danh sách câu
/// đọc từ nhật ký là biết đang đứng ở đâu, không phải đếm phím đã bấm — mà đếm
/// phím thì sai ngay lần đầu chủ máy tự bấm một cái trên bàn phím.
///
/// Bỏ hết khoảng trắng hai bên trước khi so: TUI ngắt dòng câu hỏi theo bề
/// ngang cửa sổ, nên so nguyên văn là trượt đúng những câu dài.
pub fn cursor_on(screen: &str, questions: &[String]) -> Option<usize> {
    let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let flat = squash(screen);
    questions
        .iter()
        .enumerate()
        .filter(|(_, q)| !q.trim().is_empty())
        .find(|(_, q)| flat.contains(&squash(q)))
        .map(|(i, _)| i)
}

/// Phiên có đang chạy dở không, đọc từ màn hình.
///
/// `claude` chạy dở thì in một dòng đếm giờ — `✶ Unravelling… (2m 36s · ↓ 2.0k
/// tokens · …)`. Chữ đầu đổi liên tục (Unravelling, Pondering, Herding…), nên
/// bắt theo chữ là bắt trượt; thứ KHÔNG đổi là cái đồng hồ `(<số>m <số>s ·`.
///
/// Đây là tín hiệu cho câu "đã chạy hết chỗ dở chưa" (Hà 2026-08-10). Nhật ký
/// không trả lời được: nó chỉ ghi SAU khi lượt xong, nên một phiên đang nghĩ ba
/// phút trông y hệt một phiên đã nghỉ.
/// Phiên đang làm gì, bằng đúng chữ terminal đang hiện.
///
/// Hà 2026-08-10: *"ui chưa thể hiện được phiên đang làm gì ví dụ Brewing…;
/// Perambulating"*. Màn danh sách mới nói "đang chạy" — đúng nhưng rỗng; chữ
/// người ta thật sự nhìn là cái động từ đang quay cùng đồng hồ.
///
/// Hình dạng thật, chụp trên máy này: `· Brewing… (10m 43s · ↓ 7.4k tokens)`.
/// Neo vào **cái đồng hồ**, không vào động từ — y như `is_busy`: động từ đổi
/// liên tục (Brewing, Perambulating, Unravelling, Herding…) nên bắt theo chữ là
/// bắt trượt, còn `(<số>m <số>s` thì `claude` giữ nguyên.
#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    /// "Brewing", "Perambulating"… — đã bỏ dấu chấm lửng và ký hiệu quay.
    pub verb: String,
    pub elapsed_sec: u64,
}

impl Activity {
    /// Câu ngắn cho thẻ phiên: `Brewing… 10m43s`.
    pub fn label(&self) -> String {
        let (m, s) = (self.elapsed_sec / 60, self.elapsed_sec % 60);
        format!("{}… {m}m{s:02}s", self.verb)
    }
}

/// Đọc dòng trạng thái đang quay, nếu có.
pub fn activity(screen: &str) -> Option<Activity> {
    for line in screen.lines().rev() {
        let Some(open) = line.find(" (") else {
            continue;
        };
        let inside = &line[open + 2..];
        // "<số>m <số>s" — cùng cái neo `is_busy` dùng.
        let mins: String = inside.chars().take_while(|c| c.is_ascii_digit()).collect();
        if mins.is_empty() || !inside[mins.len()..].starts_with("m ") {
            continue;
        }
        let after = &inside[mins.len() + 2..];
        let secs: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if secs.is_empty() || !after[secs.len()..].starts_with('s') {
            continue;
        }
        // Động từ = phần trước dấu "(", bỏ ký hiệu quay ở đầu và "…" ở cuối.
        let verb = line[..open]
            .trim()
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .trim()
            .trim_end_matches(['…', '.'])
            .trim()
            .to_string();
        if verb.is_empty() || verb.chars().count() > 24 {
            continue;
        }
        return Some(Activity {
            verb,
            elapsed_sec: mins.parse::<u64>().ok()? * 60 + secs.parse::<u64>().ok()?,
        });
    }
    None
}

/// Phiên có đang chạy một lượt không — hỏi DÒNG CHÂN, thứ TUI luôn vẽ.
///
/// `None` = không tìm thấy dòng chân ⟹ không đo được, và chỗ gọi phải xử lý
/// đúng như thế chứ không được đọc thành "rảnh".
///
/// 🔴 Vì sao không dùng [`is_busy`] cho câu hỏi này: `is_busy` tìm một đồng hồ
/// dạng `(3m 12s ·`, mà TUI có ÍT NHẤT hai kiểu dòng đang-chạy, và kiểu thứ hai
/// không có ngoặc — `✻ Cogitated for 37m 51s · 2 shells still running` (nguyên
/// văn, từ ảnh màn hub gửi đi 18/08). Đọc bằng `is_busy` thì phiên ấy ra "rảnh",
/// và nếu lấy đó làm cớ để lật ngược bằng chứng shell thì bản vá 16/08 chết —
/// bài kiểm `shell_is_not_busy` bắt đúng ca ấy trước khi nó kịp lên máy.
///
/// Dòng chân thì nói thẳng và không đổi theo lượt: đang chạy thì nó mang
/// `esc to interrupt`, đang chờ thì không. Đo trên ĐÚNG dòng ấy chứ không quét
/// cả màn: chữ `esc to interrupt` còn nằm rải rác trong phần hội thoại đã cuộn.
pub fn screen_running(screen: &str) -> Option<bool> {
    let footer = screen.lines().rev().find(|l| {
        l.contains("⏵⏵") || l.contains("auto mode") || l.contains("bypass permissions")
    })?;
    Some(footer.contains("esc to interrupt"))
}

pub fn is_busy(screen: &str) -> bool {
    screen.lines().any(|l| {
        let b = l.as_bytes();
        // tìm "(<số>m <số>s ·" — quét tay cho rẻ, không kéo regex vào đây.
        let mut i = 0;
        while let Some(p) = l[i..].find('(') {
            let rest = &l[i + p + 1..];
            let mut it = rest.chars();
            let mut digits = 0;
            for c in it.by_ref() {
                if c.is_ascii_digit() {
                    digits += 1;
                } else {
                    if digits > 0 && c == 'm' && rest[digits..].starts_with("m ") {
                        // "<số>m " — kiểm tiếp "<số>s"
                        let after = &rest[digits + 2..];
                        let secs = after.chars().take_while(|c| c.is_ascii_digit()).count();
                        if secs > 0 && after[secs..].starts_with('s') {
                            return true;
                        }
                    }
                    break;
                }
            }
            i += p + 1;
            if i >= l.len() {
                break;
            }
        }
        let _ = b;
        false
    })
}

/// Một lần nhìn màn phiên — BA kết cục, không phải hai.
///
/// Vì sao phải là ba: `screen_of` cũ gộp cả ba vào `None`, và chỗ dùng nguy
/// hiểm nhất (`pipeline`, chốt phím mũi tên) đọc `None` thành *"không có hộp
/// chọn"* rồi GỬI. Tức đúng lúc hub mù nhất là lúc nó dám tay nhất — mà chú
/// thích ngay tại chốt ấy nói rõ hậu quả là "không lùi lại được".
#[derive(Debug, Clone, PartialEq)]
pub enum Look {
    /// Nhìn rõ: chữ đang hiện + các lựa chọn (rỗng = không có hộp chọn).
    Saw {
        body: String,
        choices: Vec<(usize, String)>,
    },
    /// Không nhìn được: phiên không có cửa sổ, hoặc Terminal/osascript không
    /// trả lời. Đây KHÔNG phải "không có hộp chọn".
    Blind { why: String },
    // 🪦 `Withheld { choices, risk }` — gỡ 2026-08-16 cùng lượt với cổng sinh
    // ra nó. Nó nghĩa là *"màn có dấu hiệu bí mật nên hub giữ chữ lại, chỉ giữ
    // con số lựa chọn"*, và lý lẽ ấy đúng hồi `/shot` cũng quét rò. Nay `/shot`
    // gửi nguyên màn lên Telegram (gỡ 14/08), nên nhánh này giấu với hub đúng
    // thứ hub vừa công bố — và cái giá là `/pick` từ chối một cú bấm hợp lệ
    // bằng câu *"không đọc được chữ"* về một màn chủ máy đang nhìn tận mắt.
    //
    // Để lại một nhánh không ai sinh ra được thì tệ hơn xoá: người đọc sau sẽ
    // tin rằng hub còn xử lý riêng màn có bí mật. Cùng bài học `portal.rs` để
    // lại ba cỗ máy chết câm (`tests/cycle_wiring.rs`).
}

/// Nhìn màn phiên, nói thật là nhìn được tới đâu.
pub fn look(tty: &str, lines: usize) -> Look {
    let w = match window_of(tty) {
        Ok(Some(w)) => w,
        Ok(None) => {
            return Look::Blind {
                why: "phiên không gắn cửa sổ Terminal nào".into(),
            }
        }
        Err(e) => {
            // Không im lặng: hỏng ở đây làm chốt an toàn phía dưới mất căn cứ.
            logging::warn(
                "keys_window_probe_failed",
                json!({ "tty": tty, "err": e.to_string() }),
            );
            return Look::Blind {
                why: format!("không hỏi được Terminal cửa sổ nào: {e}"),
            };
        }
    };
    let screen = match screen_text(w) {
        Ok(s) => s,
        Err(e) => {
            logging::warn(
                "keys_screen_read_failed",
                json!({ "window": w, "err": e.to_string() }),
            );
            return Look::Blind {
                why: format!("không đọc được chữ trên màn: {e}"),
            };
        }
    };
    look_from_screen(&screen, lines)
}

/// Cùng ba kết cục ấy, đọc từ chữ ĐÃ CÓ SẴN — không hỏi Terminal lần nào.
///
/// Tách ra 2026-08-16 vì ảnh chụp nay lấy chữ mọi tab trong MỘT lượt dò
/// (`terminal_screens`), nên chỗ nào có sẵn chữ thì không phải trả giá hỏi lại.
/// Luật phải nằm ở đúng một chỗ: cổng quét rò rỉ (điều 5) và phép đếm ô chọn là
/// thứ quyết định hub có dám gõ hay không, nên hai bản chép là hai bản sẽ lệch.
pub fn look_from_screen(screen: &str, lines: usize) -> Look {
    let choices = parse_choices(screen);
    // 🔴 THÔI GIỮ CHỮ LẠI VỚI CHÍNH MÌNH — 2026-08-16.
    //
    // Hà, phân biệt hai loại việc: *"lệnh ở đây là lệnh bash chứ không phải
    // route của hub, route get file là yc hub gửi file lên tele thì không liên
    // quan gì tới cli cả"*. Cùng ý ấy soi vào đây thì lộ ra một chỗ vô lý:
    //
    // `/shot` gửi NGUYÊN màn ấy lên Telegram, không quét gì cả (cổng quét rò gỡ
    // ngày 14/08, cùng câu *"hub là cổng làm việc của tôi mà"*). Còn hàm này
    // vẫn giấu ĐÚNG CÁI MÀN ẤY với chính hub, nên `/pick` trả lời *"màn có dấu
    // hiệu bí mật nên hub không đọc được chữ"* về một màn chủ máy vừa nhìn tận
    // mắt trên điện thoại. Giấu một thứ đã công bố thì không bảo vệ được gì —
    // nó chỉ làm hub mù đúng lúc cần thấy nhất, rồi từ chối một cú bấm hợp lệ.
    //
    // Dấu hiệu vẫn được GHI, vì nó là một dữ kiện đáng biết; nó thôi làm một
    // cánh cửa. (`sessions::preview_risk` giữ nguyên chỗ dùng THẬT của nó: phần
    // xem trước nằm lại trong một tài liệu trên server, chỗ mà thiết lập tự xoá
    // của Telegram không với tới.)
    let risk = crate::sessions::preview_risk(screen);
    if !risk.is_empty() {
        logging::info(
            "screen_risk_noted",
            json!({ "risk": risk,
                    "why": "màn có dấu hiệu bí mật — GHI LẠI, không giữ chữ: /shot đã gửi chính màn này" }),
        );
    }
    let tail: Vec<&str> = screen
        .lines()
        .filter(|l| !l.trim().is_empty())
        .rev()
        .take(lines)
        .collect();
    Look::Saw {
        body: tail.into_iter().rev().collect::<Vec<_>>().join("\n"),
        choices,
    }
}

/// Phím mũi tên có được gửi không.
///
/// `do script` LUÔN kèm một dấu xuống dòng, không tắt được — nên trên hộp chọn
/// một phím mũi tên vừa DI vừa CHỐT, và chốt nhầm hộ chủ máy là thứ không lùi
/// lại được. Vậy nên điều kiện để gửi là **biết chắc KHÔNG có hộp chọn**, chứ
/// không phải "không thấy hộp chọn nào".
#[derive(Debug, Clone, PartialEq)]
pub enum Arrow {
    Send,
    RefuseDialog,
    RefuseBlind(String),
}

pub fn arrow_verdict(look: &Look) -> Arrow {
    match look {
        Look::Saw { choices, .. } if choices.is_empty() => Arrow::Send,
        Look::Saw { .. } => Arrow::RefuseDialog,
        Look::Blind { why } => Arrow::RefuseBlind(why.clone()),
    }
}

/// Sau một cú Enter mà màn KHÔNG đổi: có được thử `→` để NHẬN GỢI Ý MỜ không?
///
/// 🔴 Hà 2026-08-16, ảnh chụp buồng chat lúc 11:54: *"Bấm nút enter không nhận,
/// chỗ ô chat có gợi ý, phải bấm nút right trước thì nó mới điền text theo gợi
/// ý"*. Đây là mảnh còn thiếu của một chuyện hub ĐÃ nhận ra mà chưa làm gì:
/// tin trả lời hôm ấy nói đúng chẩn đoán (*"nhiều khả năng là GỢI Ý MỜ của
/// TUI"*) rồi đẩy việc về cho chủ máy — *"muốn gửi câu ấy thì gõ thẳng nó ở
/// đây"*. Một trạng thái đã gọi được tên thì phải có hành động đi kèm, không
/// thì cây cầu dừng lại đúng chỗ nó vừa chỉ ra vấn đề.
///
/// **Thứ tự Enter-TRƯỚC là phần quan trọng nhất, đừng đảo lại cho gọn.** Bấm
/// `→` ngay từ đầu thì nhanh hơn một nhịp, và sai ở đúng ca không lùi được:
/// khi ô đang có chữ THẬT chủ máy gõ dở, `→` **nhận nốt phần gợi ý còn lại** và
/// cú CR đi kèm gửi luôn một câu dài hơn câu anh gõ. Enter trước là một phép
/// đo: màn không đổi ⟹ Enter chẳng có gì để gửi ⟹ ô rỗng thật, chữ đang nhìn
/// thấy là gợi ý. Chỉ sau bằng chứng ấy `→` mới an toàn.
///
/// Cửa hộp chọn dùng chung `arrow_verdict`: `press` nào cũng kèm một CR của
/// `do script`, nên `→` trên hộp chọn vừa DI vừa CHỐT.
/// `None` = ca này không áp dụng (phím khác, hoặc màn ĐÃ đổi nên Enter đã ăn).
/// `Some(verdict)` = có lý do để thử, và đây là phán quyết của cửa mũi tên.
/// Hai kết cục ấy phải phân biệt được: gộp "không cần" vào "bị từ chối" là lại
/// đẻ ra một `None` mang ba nghĩa, đúng con bug `screen_of` từng gây.
pub fn ghost_verdict(key: &str, screen_unchanged: bool, seen: &Look) -> Option<Arrow> {
    if key.trim() != "enter" || !screen_unchanged {
        return None;
    }
    Some(arrow_verdict(seen))
}

/// Chữ trên màn của phiên, đã gác bí mật và cắt gọn — dạng dùng được ngay.
///
/// `None` khi phiên không có cửa sổ, khi không đọc được màn, hoặc khi màn có
/// dấu hiệu chứa bí mật: chữ này rời khỏi máy y như phần xem trước của phiên,
/// nên nó phải đi qua đúng cái cổng ấy (điều 5 trong CLAUDE.md).
///
/// Dạng gộp này chỉ hợp cho chỗ **hiển thị** — không có gì để hiện thì thôi.
/// Chỗ nào phải RA QUYẾT ĐỊNH thì dùng `look` và phân biệt cho đủ ba kết cục.
pub fn screen_of(tty: &str, lines: usize) -> Option<(String, Vec<(usize, String)>)> {
    match look(tty, lines) {
        Look::Saw { body, choices } => Some((body, choices)),
        _ => None,
    }
}

/// Dòng LỖI API đang hiện trên màn, nếu có.
///
/// 🔴 Hà 2026-08-12: *"vừa rồi báo lỗi api mà chưa thấy bắt được"*. Một phiên
/// gặp lỗi API thì nhật ký thôi lớn lên y hệt lúc nó xong việc, nên cái loa gọi
/// đó là *"⏸ dừng, đang chờ bạn"* — đúng hình dạng, sai việc phải làm.
///
/// Mẫu lấy từ NHẬT KÝ THẬT trên máy này (đếm 2026-08-12): `API Error:` 30 lần
/// (rate limit · 401 token bị thu hồi · đứt kết nối giữa chừng · máy ngủ),
/// `Request timed out.` 18 lần. Cố ý hẹp: bắt bằng câu chữ `claude` in ra, chứ
/// không đoán theo "màn có chữ error".
pub fn api_error(screen: &str) -> Option<String> {
    screen
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| l.contains("API Error") || l.contains("Request timed out"))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{
        activity, arrow_verdict, as_string, ghost_verdict, landed, window_any_script,
        window_script, Arrow, Landed, Look,
    };

    /// Đọc ĐÚNG chữ terminal đang hiện, và neo vào đồng hồ chứ không vào động từ.
    ///
    /// Hà 2026-08-10: *"ui chưa thể hiện được phiên đang làm gì ví dụ Brewing…;
    /// Perambulating"*. Động từ đổi liên tục nên bắt theo chữ là bắt trượt —
    /// cùng lý do `is_busy` neo vào `(<số>m <số>s`.
    #[test]
    fn the_spinner_line_is_read_by_its_clock_not_by_its_verb() {
        // Dòng thật, chụp trên máy này 2026-08-10.
        let real = "· Brewing… (10m 43s · ↓ 7.4k tokens)";
        let a = activity(real).expect("phải đọc được");
        assert_eq!(a.verb, "Brewing");
        assert_eq!(a.elapsed_sec, 10 * 60 + 43);
        assert_eq!(a.label(), "Brewing… 10m43s");

        // Động từ khác, ký hiệu quay khác — vẫn đọc được.
        let other = activity("✶ Perambulating… (0m 8s · ↓ 12 tokens)").unwrap();
        assert_eq!(other.verb, "Perambulating");
        assert_eq!(other.label(), "Perambulating… 0m08s");

        // Màn đứng yên thì KHÔNG bịa ra hoạt động nào.
        assert!(activity("❯ ").is_none());
        assert!(activity("  ⏵⏵ auto mode on (shift+tab to cycle) · esc to interrupt").is_none());
        assert!(activity("").is_none());
        // Có ngoặc, có chữ, nhưng không có đồng hồ ⟹ không phải dòng trạng thái.
        assert!(activity("Đã sửa 2 tệp (xem lại 2 chỗ)").is_none());

        // Dòng cuối cùng thắng: màn cuộn thì cái mới nhất nằm dưới.
        let two = "· Cũ… (1m 00s · x)\n· Mới… (2m 05s · y)";
        assert_eq!(activity(two).unwrap().verb, "Mới");
    }

    /// Chốt mũi tên chỉ được mở khi BIẾT CHẮC không có hộp chọn.
    ///
    /// Bug thật, tìm ra 2026-08-10: `screen_of` gộp ba kết cục vào `None`, và
    /// chốt đọc `None` thành "không có hộp chọn" rồi GỬI — tức hỏng về phía
    /// nguy hiểm, và hỏng nặng nhất đúng lúc màn đang hiện một mật khẩu (đó
    /// cũng là một đường trả `None`). `do script` luôn kèm dấu xuống dòng nên
    /// trên hộp chọn, mũi tên vừa di vừa CHỐT; chốt nhầm hộ chủ máy là thứ
    /// không lùi lại được.
    #[test]
    fn an_arrow_goes_only_when_we_are_sure_there_is_no_dialog() {
        let quiet = Look::Saw {
            body: "$ ".into(),
            choices: vec![],
        };
        assert_eq!(arrow_verdict(&quiet), Arrow::Send);

        let asking = Look::Saw {
            body: "Chọn đi?".into(),
            choices: vec![(1, "một".into()), (2, "hai".into())],
        };
        assert_eq!(arrow_verdict(&asking), Arrow::RefuseDialog);

        // Màn có dấu hiệu bí mật nay đi đúng nhánh `Saw` như mọi màn khác —
        // chữ không còn bị giữ lại với chính hub (xem bia mộ `Look::Withheld`).
        // Cái phải giữ nguyên: luật quyết vẫn đọc SỐ LỰA CHỌN, nên một màn có
        // mật khẩu mà đang mở hộp chọn thì vẫn không được gửi mũi tên.
        let secret_asking = Look::Saw {
            body: "mật khẩu: hunter2\nChọn đi?".into(),
            choices: vec![(1, "một".into()), (2, "hai".into())],
        };
        assert_eq!(arrow_verdict(&secret_asking), Arrow::RefuseDialog);

        // Không nhìn được thì KHÔNG gửi — và câu từ chối phải mang theo lý do,
        // không thì người ta không biết bấm lại có ích gì không.
        let blind = Look::Blind {
            why: "osascript hết giờ".into(),
        };
        match arrow_verdict(&blind) {
            Arrow::RefuseBlind(why) => assert!(why.contains("osascript"), "phải nói lý do: {why}"),
            other => panic!("mù mà vẫn gửi: {other:?}"),
        }
    }

    /// `→` để NHẬN GỢI Ý MỜ chỉ được bấm khi Enter đã CHỨNG MINH là ô rỗng.
    ///
    /// Hà 2026-08-16: *"Bấm nút enter không nhận, chỗ ô chat có gợi ý, phải bấm
    /// nút right trước thì nó mới điền text theo gợi ý"*. Cái bẫy của tính năng
    /// này là làm cho nhanh: bấm `→` ngay từ đầu. Ca hỏng của đường tắt ấy là ô
    /// đang có chữ THẬT gõ dở — `→` nhận nốt phần gợi ý còn lại rồi cú CR đi kèm
    /// gửi luôn một câu dài hơn câu chủ máy gõ, và không lùi lại được.
    #[test]
    fn the_ghost_suggestion_key_needs_a_dead_enter_first() {
        let quiet = Look::Saw {
            body: "❯ chạy deploy đi".into(),
            choices: vec![],
        };
        // Enter bấm rồi mà màn đứng yên ⟹ Enter chẳng có gì để gửi ⟹ thử `→`.
        assert_eq!(ghost_verdict("enter", true, &quiet), Some(Arrow::Send));
        // Màn ĐÃ đổi ⟹ Enter ăn rồi, không có gì để chữa.
        assert_eq!(ghost_verdict("enter", false, &quiet), None);
        // Phím khác không liên quan: `3` không đổi màn là chuyện của `3`.
        assert_eq!(ghost_verdict("3", true, &quiet), None);
        assert_eq!(ghost_verdict("clear", true, &quiet), None);

        // Trên hộp chọn thì `→` vừa DI vừa CHỐT — dùng chung cửa với mũi tên.
        let asking = Look::Saw {
            body: "Chọn đi?".into(),
            choices: vec![(1, "một".into()), (2, "hai".into())],
        };
        assert_eq!(
            ghost_verdict("enter", true, &asking),
            Some(Arrow::RefuseDialog)
        );
        // …và mù thì KHÔNG bấm: không đọc được màn không có nghĩa là không có
        // hộp chọn.
        let blind = Look::Blind {
            why: "osascript hết giờ".into(),
        };
        match ghost_verdict("enter", true, &blind) {
            Some(Arrow::RefuseBlind(why)) => assert!(why.contains("osascript"), "{why}"),
            other => panic!("mù mà vẫn bấm →: {other:?}"),
        }
    }

    /// Chuỗi AppleScript sinh ra phải ĐÓNG ĐỦ dấu nháy và kết thúc đúng chỗ.
    ///
    /// Lỗi thật 2026-08-10: một dòng `return ""` ở cuối, viết trong chuỗi thô
    /// `r#"…"#` của Rust, bị cắt thành `return "` — AppleScript hỏng ngay dòng
    /// đầu (*"Expected string but found end of script. (-2741)"*) và cả tính
    /// năng chết. Không phép đo nào bắt được vì test cũ chỉ soi phần thoát chuỗi.
    /// base64 tự viết thì phải kiểm bằng những ca người ta hay sai: độ dài
    /// không chia hết cho 3, và byte 0xFF (dễ lộ lỗi dấu).
    /// Hộp chọn nhận ra bằng HÌNH DẠNG, và phải từ chối những thứ chỉ trông
    /// giống. Đây là chỗ dễ nhận nhầm nhất: mọi đoạn văn đều có thể chứa "1.".
    /// "Đang chạy dở" phải bắt theo ĐỒNG HỒ, không theo chữ — chữ đổi mỗi lần.
    #[test]
    fn landed_reads_the_queue_line_the_cli_actually_prints() {
        // Nguyên văn từ màn thật lúc gõ vào phiên đang chạy.
        let busy_with_queue = "❯ Press up to edit queued messages\n  (2m 5s · ↑ 1.2k tokens)";
        assert_eq!(landed(busy_with_queue, "chữ nào đó"), Landed::Queued);
        // Đang chạy mà chưa có hàng chờ.
        assert_eq!(
            landed("  (1m 2s · esc to interrupt)", "chữ nào đó"),
            Landed::Running
        );
        // Đứng ở dấu nhắc.
        assert_eq!(landed("❯ \n  ⏵⏵ auto mode on", "chữ nào đó"), Landed::Idle);
    }

    #[test]
    fn busy_is_read_from_the_clock_not_the_word() {
        use super::is_busy;
        assert!(is_busy(
            "✶ Unravelling… (2m 36s · ↓ 2.0k tokens · thinking)"
        ));
        assert!(is_busy("✻ Pondering… (12m 4s · ↑ 900 tokens)"));
        assert!(is_busy("· Herding cats… (0m 8s ·)"));
        // Rảnh: dấu nhắc trống, dòng gợi ý, không có đồng hồ.
        assert!(!is_busy(
            "❯ \n⏵⏵ auto mode on (shift+tab to cycle) · esc to interrupt"
        ));
        assert!(!is_busy(""));
        // Ngoặc có số nhưng KHÔNG phải đồng hồ thì không tính.
        assert!(!is_busy("Đã sửa 3 tệp (xem lại 2 chỗ)"));
        assert!(!is_busy("chạy trong (500ms)"));
    }

    /// Nguyên văn màn THẬT, chép từ `tests/ask_table_live.rs` chạy trên phiên
    /// `projects-bd` đang kẹt (2026-08-13). Giữ nguyên ký tự đo được — sửa cho
    /// "gọn" là vứt đúng cái bằng chứng khiến hàm đếm đúng.
    const REAL_TAB_BAR: &str = "←  ☒ Vá ACL  ☐ Đăng nhập  ✔ Submit  →";

    #[test]
    fn the_tab_bar_says_which_questions_are_still_empty() {
        let t = super::ask_table(&format!("chữ ở trên\n{REAL_TAB_BAR}\ncâu hỏi ở dưới"))
            .expect("thanh tab thật phải đọc được");
        assert_eq!(t.answered, vec![true, false], "☒ rồi ☐");
        assert_eq!(t.headers, vec!["Vá ACL", "Đăng nhập"]);
        // `✔ Submit` là nút gửi, KHÔNG phải câu thứ ba.
        assert_eq!(t.left(), 1, "còn đúng một ô trống");
    }

    #[test]
    fn a_line_that_merely_mentions_a_box_is_not_a_table() {
        // Không có mũi tên hai đầu ⟹ không phải thanh tab. Dựng bảng từ một
        // dòng văn xuôi là bịa ra một cái hộp không có thật, rồi gửi phím vào.
        assert!(super::ask_table("tôi đã đánh dấu ☐ vào ô ấy").is_none());
        assert!(
            super::ask_table("← quay lại · tiếp →").is_none(),
            "không có ô nào"
        );
    }

    #[test]
    fn the_cursor_is_found_by_text_even_when_the_question_wraps() {
        // Đúng hình dạng thật: TUI bẻ câu hỏi theo bề ngang cửa sổ.
        let screen = format!(
            "{REAL_TAB_BAR}\nCòn chuyện đăng ký hạ chữ username mà đăng nhập lại so đúng\nnhư gõ?\n❯ 1. Có"
        );
        let qs = vec![
            "Khi ô ACL nhận một chuỗi không trỏ tới ai, server nên xử sao?".to_string(),
            "Còn chuyện đăng ký hạ chữ username mà đăng nhập lại so đúng như gõ?".to_string(),
        ];
        assert_eq!(super::cursor_on(&screen, &qs), Some(1), "đang đứng ở câu 2");
        assert_eq!(super::cursor_on("màn chẳng có câu nào", &qs), None);
    }

    #[test]
    fn choices_are_recognised_by_shape_only() {
        use super::parse_choices;
        let box_ = "Chọn cách đi tiếp?\n❯ 1. Sửa tại chỗ\n  2. Mở phiên mới\n  3. Bỏ qua";
        assert_eq!(
            parse_choices(box_),
            vec![
                (1, "Sửa tại chỗ".to_string()),
                (2, "Mở phiên mới".to_string()),
                (3, "Bỏ qua".to_string())
            ]
        );
        // Không bắt đầu từ 1 thì không phải hộp chọn.
        assert!(parse_choices("3. xong rồi\n4. tiếp").is_empty());
        // Số nhảy cóc cũng không.
        assert!(parse_choices("1. một\n3. ba").is_empty());
        // Đoạn văn có dấu chấm không phải lựa chọn.
        assert!(parse_choices("Tôi đã sửa 2 tệp. Xong.").is_empty());
        // Màn trống thì không có gì.
        assert!(parse_choices("").is_empty());
        // Một mục thì không có gì để chọn.
        assert!(parse_choices("❯ 1. Chỉ có một").is_empty());

        // ĐOẠN VĂN CÓ ĐÁNH SỐ — chuông từng kêu nhầm ở đây (2026-08-11).
        // Một câu TRẢ LỜI của phiên, ba mục đánh số, mỗi mục tràn sang dòng
        // sau: hub đọc thành hộp chọn rồi bắn `⚠ dừng lại HỎI — cần bạn chọn`
        // kèm nguyên văn, cho một phiên chẳng hỏi gì ai. Cái khác nhau giữa
        // hai hình dạng là DÒNG CHỮ TRÀN nằm giữa hai mục.
        let prose = "Chờ anh\n\
                     1. Mở lại quyền Documents — mọi việc còn lại đều cần đọc repo.\n\
                     rồi mới chạy tiếp được kịch bản nghiệm thu.\n\
                     2. Telegram hai chiều là việc chưa làm và tôi chưa tự quyết:\n\
                     máy móc đã có sẵn, nhưng nó chỉ sống trong lúc chờ xác nhận.\n\
                     3. Sau khi có quyền thì chạy /btw thật trên một phiên Terminal.";
        assert!(
            parse_choices(prose).is_empty(),
            "đoạn văn có đánh số KHÔNG phải hộp chọn: {:?}",
            parse_choices(prose)
        );
        // Nhưng hộp thật giãn một dòng trống thì vẫn phải nhận ra.
        let spaced = "❯ 1. Yes\n\n  2. No, tell Claude what to do differently";
        assert_eq!(parse_choices(spaced).len(), 2, "{spaced}");
    }

    /// Đường ĐÓNG phải nhìn thấy được tab đã chết — đường GÕ thì không.
    ///
    /// 🔴 Hà 2026-08-17, bấm ◻ ở một hàng `/terminal`: *"Ko còn sao vẫn liệt kê,
    /// hay nó ở tab con"*. hub trả lời *"không còn cửa sổ terminal nào chạy
    /// ttys014"* trong khi cửa sổ ấy đang mở ngay đó — vì `tabs_script` (thứ
    /// DỰNG danh sách) đọc mọi tab, còn `window_script` (thứ THI HÀNH) lọc
    /// `count of processes > 0`. Đo lại bằng tay hôm ấy: `window_any_script`
    /// trả `2158` cho `/dev/ttys014`, còn `window_script` trả rỗng.
    #[test]
    fn the_close_path_can_see_a_dead_tab_but_the_typing_path_cannot() {
        let any = window_any_script("/dev/ttys014");
        assert!(any.contains(r#"is "/dev/ttys014" then"#), "{any}");
        assert!(
            any.contains("if dead is missing value then set dead to id of w"),
            "phải có nhánh nhận tab KHÔNG còn tiến trình:\n{any}"
        );
        assert!(
            any.find("set alive to id of w").unwrap() < any.find("set dead to id of w").unwrap(),
            "tab còn sống phải được ưu tiên trước cái xác:\n{any}"
        );
        // …còn đường gõ giữ nguyên hàng rào: gõ vào một cái xác là gõ vào chỗ
        // không ai đọc (đo 2026-08-11, ba cửa sổ cùng khai `/dev/ttys005`).
        let typing = window_script("/dev/ttys014");
        assert!(
            !typing.contains("set dead to"),
            "đường gõ KHÔNG được nhận tab chết:\n{typing}"
        );
        assert_eq!(any.matches('"').count() % 2, 0, "dấu nháy lẻ:\n{any}");
        assert_eq!(any.matches("end try").count(), 1, "{any}");
        assert!(any.trim_end().ends_with("end tell"), "{any}");
        assert!(!any.contains("{}"), "{any}");
    }

    #[test]
    fn window_script_is_well_formed() {
        let s = window_script("/dev/ttys005");
        assert!(s.contains(r#"is "/dev/ttys005" and"#), "{s}");
        // Tab ĐÃ CHẾT vẫn khai tty cũ, và tty thì bị dùng lại — ba cửa sổ cùng
        // khai `/dev/ttys005` (đo 2026-08-11, hai trong ba là xác). Khớp tty
        // trần là trả về một cửa sổ ma: màn hình sai lên điện thoại, `/type` gõ
        // vào chỗ không ai đọc.
        assert!(
            s.contains("count of (processes of t)) > 0"),
            "phải lọc tab còn sống:\n{s}"
        );
        assert!(
            s.contains("busy of t"),
            "phải ưu tiên tab đang chạy chương trình:\n{s}"
        );
        assert!(s.trim_end().ends_with("end tell"), "kết thúc sai:\n{s}");
        // Số dấu nháy phải CHẴN — lẻ nghĩa là có một chuỗi treo lửng.
        assert_eq!(s.matches('"').count() % 2, 0, "dấu nháy lẻ:\n{s}");
        // Và không được còn chỗ giữ chỗ nào chưa thay.
        assert!(!s.contains("{}"), "{s}");
        // Cửa sổ không có tab (cài đặt, inspector) phải bị BỎ QUA chứ không
        // làm chết cả script — lỗi -1728 đo được 2026-08-10.
        // Đếm `"try"` trần là sai: nó nằm trong cả `end try`. Mỗi `try` phải có
        // đúng một `end try` — đó mới là điều cần kiểm.
        assert_eq!(s.matches("end try").count(), 1, "{s}");
        assert_eq!(
            s.matches("try").count() - s.matches("end try").count(),
            1,
            "{s}"
        );
    }

    /// Thoát chuỗi sai là script hỏng cú pháp — hoặc đổi nghĩa, thứ tệ hơn.
    #[test]
    fn applescript_strings_escape_quotes_and_backslashes() {
        assert_eq!(as_string("hello"), "\"hello\"");
        assert_eq!(as_string(r#"nói "xin chào""#), "\"nói \\\"xin chào\\\"\"");
        assert_eq!(as_string(r"C:\path"), "\"C:\\\\path\"");
        // Chuỗi rỗng vẫn phải là một chuỗi hợp lệ, không phải hai dấu nháy trần.
        assert_eq!(as_string(""), "\"\"");
    }
}
