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
    // vào cửa sổ chết, `can_type` khai bừa là gõ được, và `tab_busy` — thứ
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
    /// Phiên đang đứng ở dấu nhắc.
    Idle,
}

/// Phần màn hình thuộc **ô nhập** — khối đóng khung cuối cùng.
///
/// `claude` vẽ ô nhập bằng khung `╭─╮ │ ╰─╯` ở đáy màn. Lấy từ dấu `╭` cuối cùng
/// trở đi là được đúng ô ấy (kèm dòng gợi ý dưới chân nó, vô hại). Không thấy
/// khung nào — chủ đề khác, cửa sổ hẹp — thì lùi về **4 dòng không rỗng cuối**:
/// vẫn là vùng đáy, chỉ kém sắc nét hơn, và thà kém sắc nét còn hơn soi cả màn
/// rồi đọc phần hội thoại thành nội dung ô nhập.
fn box_region(screen: &str) -> String {
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

/// Màn có đang ĐỀ XUẤT một việc và chờ một tiếng "ừ" không?
///
/// 🔴 Hà 2026-08-13, gửi ảnh màn `dwork`: *"một gợi ý tương tự"*. Phiên viết
/// *"Việc tiếp theo tôi làm được ngay mà không chờ ai: dựng phân hệ quota
/// phép… Nói một tiếng nếu anh muốn tôi vào việc đó"* — và chính Hà đã trả lời
/// nó bằng đúng hai chữ **"Làm đi"** (log 01:48:03). Tức việc thường xuyên nhất
/// trên điện thoại là gõ lại một câu đồng ý.
///
/// Nhận theo CÂU CHỮ của lời mời, không đoán theo ngữ nghĩa: một cái nút gửi
/// "làm đi" vào nhầm lúc là một mệnh lệnh không lùi được. Mẫu lấy từ màn thật.
pub fn asks_for_go_ahead(screen: &str) -> bool {
    const INVITES: &[&str] = &[
        "nói một tiếng",
        "muốn tôi",
        "xin xác nhận",
        "anh chốt",
        "anh chọn",
        "hay ưu tiên",
        "có muốn",
    ];
    let low = screen.to_lowercase();
    INVITES.iter().any(|m| low.contains(m))
}

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
    let needle: String = t.chars().skip(n.saturating_sub(16)).collect();
    squash(&screen).contains(&needle)
}

/// Phân loại thuần từ chữ trên màn, để test được không cần Terminal.
pub fn landed(screen: &str) -> Landed {
    // Chính `claude` in dòng này khi có tin trong hàng chờ (đo trên máy:
    // "Press up to edit queued messages").
    if screen.contains("queued message") {
        return Landed::Queued;
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
    // Ngăn cách bằng ký tự hiếm, KHÔNG phải khoảng trắng: `processes of t as
    // string` dán liền các tên (`login-zshclaude`), đọc ra là một tên tiến
    // trình không có thật.
    let out = osascript(
        r#"tell application "Terminal"
  set acc to ""
  repeat with w in every window
    try
      repeat with t in tabs of w
        set ps to ""
        repeat with p in (processes of t)
          set ps to ps & (p as string) & "|"
        end repeat
        set acc to acc & (tty of t) & tab & (busy of t) & tab & ps & linefeed
      end repeat
    end try
  end repeat
  return acc
end tell"#,
    )?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let tty = f.next()?.trim();
            if tty.is_empty() {
                return None;
            }
            let busy = f.next().unwrap_or("false").trim() == "true";
            let procs = f
                .next()
                .unwrap_or_default()
                .split('|')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
            Some(Tab {
                tty: tty.trim_start_matches("/dev/").to_string(),
                busy,
                procs,
            })
        })
        .collect())
}

pub fn terminal_ttys() -> Result<std::collections::HashSet<String>> {
    // Cùng lối phòng thủ với `window_script`: cửa sổ không có tab (bảng cài
    // đặt, inspector) làm cả script chết giữa chừng nếu không bọc `try`.
    // Chỉ đếm tab CÒN TIẾN TRÌNH — cùng lý do với `window_script`: một tab đã
    // chết vẫn khai tty cũ, và tty thì bị dùng lại, nên đếm cả xác là khai bừa
    // "hub gõ được vào phiên này".
    let out = osascript(
        r#"tell application "Terminal"
  set acc to ""
  repeat with w in every window
    try
      repeat with t in tabs of w
        if (count of (processes of t)) > 0 then set acc to acc & (tty of t) & linefeed
      end repeat
    end try
  end repeat
  return acc
end tell"#,
    )?;
    Ok(out
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.trim_start_matches("/dev/").to_string())
        .collect())
}

/// Tab của cửa sổ này còn chương trình nào đang chạy không.
///
/// Đây là câu hỏi PHÂN BIỆT "thoát CLI xong" với "vẫn đang thoát": `ps` biến
/// mất trước khi shell kịp in dấu nhắc, còn `busy` là chính Terminal trả lời về
/// tab của nó. Dùng nó để CHỜ, chứ đừng đoán bằng `sleep`.
pub fn tab_busy(window: i64) -> Result<bool> {
    let out = osascript(&format!(
        r#"tell application "Terminal" to get busy of (selected tab of window id {window}) as text"#
    ))?;
    Ok(out.trim() == "true")
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
pub fn send_exit(window: i64) -> Result<()> {
    osascript(&do_script(window, &as_string("/exit")))?;
    Ok(())
}

/// Đóng cửa sổ. Gọi khi ĐÃ biết tab không còn bận — xem `tab_busy`.
pub fn close_window(window: i64) -> Result<()> {
    osascript(&format!(
        r#"tell application "Terminal" to close (first window whose id is {window})"#
    ))?;
    Ok(())
}

pub fn quit_and_close(window: i64) -> Result<()> {
    osascript(&do_script(window, &as_string("/exit")))?;
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
        match tab_busy(window) {
            Ok(false) => {
                still_busy = false;
                break;
            }
            Ok(true) => {}
            // Không hỏi được thì DỪNG, đừng đóng liều: đóng khi chưa chắc đã
            // thoát là đúng cái bật hộp thoại.
            Err(e) => return Err(e.context("không hỏi được tab còn bận không")),
        }
    }
    if still_busy {
        anyhow::bail!(
            "đã gõ /exit nhưng phiên vẫn đang chạy dở sau 30 giây — `claude` xếp hàng lệnh thoát tới cuối lượt. Cửa sổ vẫn nguyên (đóng lúc này sẽ bật hộp thoại 'terminate running processes'); chờ nó xong rồi bấm lại"
        );
    }
    osascript(&format!(
        r#"tell application "Terminal" to close (first window whose id is {window})"#
    ))?;
    Ok(())
}

/// Gõ `text` vào cửa sổ của phiên, rồi Enter.
///
/// `enter = false` cho các lựa chọn cần phím riêng (mũi tên, Esc) — xem
/// [`press`].
pub fn type_into(window: i64, text: &str, enter: bool) -> Result<()> {
    let _ = enter;
    osascript(&do_script(window, &as_string(text)))?;
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
    press_seq(window, std::slice::from_ref(&keyname))
}

/// Payload AppleScript cho một phím — dùng chung cho [`press`] và [`press_seq`].
fn key_payload(keyname: &str) -> Result<String> {
    // Ký tự điều khiển gửi qua `do script` như mọi chuỗi khác. Mũi tên là dãy
    // thoát ANSI: ESC [ A/B/C/D — đúng thứ terminal nhận khi người ta bấm.
    Ok(match keyname {
        // 🔴 Đo 2026-08-14 (Hà: *"Vậy là tôi bấm enter không có tác dụng rồi"*):
        // gửi CHUỖI RỖNG và để `do script` tự chèn xuống dòng thì ô nhập KHÔNG
        // nhúc nhích — đo trước/sau bằng `input_box_text`, chữ y nguyên. Lý do
        // cùng họ với bài học dấu Enter hồi 08-12: TUI đọc cả lượt ghi như một
        // cú DÁN, và dán một dòng trống thì chỉ là một dòng trống.
        //
        // CR (ASCII 13) là thứ terminal thật gửi khi người ta bấm Return, nên
        // gửi đúng ký tự ấy thay vì trông chờ vào cái xuống dòng `do script`
        // tự thêm.
        "enter" => "(ASCII character 10)".to_string(),
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

/// Nhiều phím trong MỘT lời gọi `do script` — cả dãy đi vào tab như một lần gõ.
///
/// Vì sao phải có, thay vì gọi [`press`] nhiều lần: mỗi `do script` **tự kèm
/// một dấu xuống dòng** và không tắt được (xem `do_script`). Nên gọi ba lần là
/// ba dấu Enter chen vào giữa dãy phím — trên một bảng hỏi, mỗi Enter thừa là
/// một lần CHỐT hộ chủ máy. Gộp lại thì cả dãy chỉ còn đúng một dấu ở cuối, tức
/// số Enter đếm được và nằm ở chỗ mình chọn.
///
/// Hàm này KHÔNG tự quyết được dãy phím ấy có an toàn hay không — chỗ gọi phải
/// tự chịu trách nhiệm, cùng luật với `arrow_verdict`.
pub fn press_seq(window: i64, keys: &[&str]) -> Result<()> {
    if keys.is_empty() {
        return Err(anyhow!("không có phím nào để gửi"));
    }
    let parts: Result<Vec<String>> = keys.iter().map(|k| key_payload(k)).collect();
    // `enter` giữa dãy là chuỗi rỗng nên nối vào không sinh ký tự — đúng ý:
    // dấu xuống dòng DUY NHẤT là cái `do script` kèm ở cuối.
    osascript(&do_script(window, &parts?.join(" & ")))?;
    Ok(())
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

/// Những DÒNG LỆNH đang hiện trên màn — thứ bấm một cái là chạy được.
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
/// Dài hơn chừng này thì KHÔNG dựng nút — trên màn nó đã bị bẻ dòng, và hub
/// đang cầm một mẩu chứ không phải cả lệnh. Xem chỗ dùng trong
/// [`commands_on_screen`].
pub const BTN_CMD_MAX: usize = 60;

/// Trần cho nguồn KHÔNG bị bẻ dòng — chữ của một BÁO CÁO, đọc từ nhật ký.
///
/// 🔴 Hà 2026-08-14, ảnh chụp một bản đầy đủ của `[tfl5]` có hai dòng lệnh mà
/// chỉ dòng ngắn có nút: *"Có lệnh bash sao lại ko có nút bấm chạy cho nó"*.
/// Đo đúng hai dòng ấy:
///   `git -C /Users/hanguyen/projects/AI/tfl5 push origin main`            = 56
///   `bash /Users/hanguyen/projects/AI/tfl5/scripts/deploy.sh static-…`    = 80
/// Trần 60 đứng đúng giữa hai dòng đó. Mà lý do của trần 60 — *"trên màn nó đã
/// bị bẻ dòng, hub đang cầm một mẩu chứ không phải cả lệnh"* — chỉ đúng cho
/// nguồn MÀN. Chữ của một báo cáo đến từ nhật ký `.jsonl`, nguyên văn model
/// viết ra, không đi qua bề ngang cửa sổ nào cả: ở đó một dòng dài là một dòng
/// dài THẬT, và từ chối nó là từ chối đúng những lệnh đáng bấm nhất (đường dẫn
/// tuyệt đối + tham số là hình dạng chuẩn của một lệnh triển khai).
///
/// Vẫn có trần, không bỏ hẳn: bài học sáng cùng ngày (*"Cả 1 khối lệnh dài này
/// thì không được tạo nút"*) là về một khối 380 ký tự. 200 nhận trọn một lệnh
/// deploy có đường dẫn tuyệt đối, và vẫn chặn cả khối.
pub const BTN_CMD_REPORT_MAX: usize = 200;

/// Lệnh trên MÀN — chữ đã qua bề ngang cửa sổ, nên trần ngắn (`BTN_CMD_MAX`).
pub fn commands_on_screen(text: &str, max: usize) -> Vec<String> {
    commands_in(text, max, BTN_CMD_MAX)
}

/// Lệnh trong một BÁO CÁO (chữ lấy từ nhật ký, không bị bẻ dòng).
///
/// Cùng một bộ luật với [`commands_on_screen`] — cùng hàng rào `KNOWN`, cùng
/// `looks_like_prose`, cùng `forbids` — khác đúng MỘT cửa: trần độ dài. Khác
/// nhau ở đâu thì phải nói ra ở đó, chứ không đẻ ra bộ luật thứ hai để rồi hai
/// bên lệch nhau lúc nào không biết.
pub fn commands_in_report(text: &str, max: usize) -> Vec<String> {
    commands_in(text, max, BTN_CMD_REPORT_MAX)
}

fn commands_in(text: &str, max: usize, max_len: usize) -> Vec<String> {
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
/// Đuôi của một dòng lệnh vừa bị cửa sổ bẻ — hoặc `None` nếu không chắc.
///
/// Cố ý HẸP, vì nối nhầm là dựng ra một lệnh chưa ai viết: đuôi phải là một
/// mẩu liền (không dấu cách) hoặc một tham số `-x`/`--x`. Một câu văn ở dòng
/// dưới thì không phải đuôi, và lúc ấy chỗ gọi bỏ luôn cái nút.
fn wrap_tail(next: Option<&str>) -> Option<String> {
    let t = next?.trim();
    if t.is_empty() || t.len() > 60 {
        return None;
    }
    // Dấu nhắc / gạch đầu dòng = một dòng MỚI, không phải phần tiếp. `-` KHÔNG
    // nằm đây: một tham số `-v` hay `--expect-symbol` bắt đầu đúng như thế.
    for p in ["$", "❯", ">", "⏵", "%", "•", "!", "#", "⎿", "│"] {
        if t.starts_with(p) {
            return None;
        }
    }
    let one_token = !t.contains(char::is_whitespace);
    let a_flag = t.starts_with('-');
    (one_token || a_flag).then(|| t.to_string())
}

fn after_cd(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("cd ")?;
    let (_dir, tail) = rest
        .split_once("&&")
        .or_else(|| rest.split_once(';'))?;
    let tail = tail.trim();
    (!tail.is_empty()).then_some(tail)
}

    const KNOWN: &[&str] = &[
        "git", "gh", "npm", "npx", "node", "cargo", "bash", "sh", "zsh", "python3", "pip3",
        "docker", "make", "curl", "rsync", "scp", "ssh", "sqlite3", "pnpm", "yarn", "deno", "go",
        "rustup", "brew", "launchctl", "osascript", "open", "code", "tail", "grep", "rg", "find",
        "ls",
    ];
    let mut out: Vec<String> = Vec::new();
    let rows: Vec<&str> = screen.lines().collect();
    // Bề ngang cửa sổ, đo từ chính màn: dòng dài nhất. Chỉ có nghĩa khi nguồn
    // là MÀN — chữ báo cáo không bị bẻ, nên `wrapped` tắt luật này đi.
    //
    // Bề ngang chỉ ĐO ĐƯỢC khi có đủ dòng để đo: trên một mẩu chữ ba dòng, dòng
    // dài nhất chính là dòng đang xét, và "chỗ trống còn lại" ra 0 cho mọi
    // dòng — phép đo tự khẳng định điều nó định kiểm. Một màn Terminal thật
    // luôn dày hơn thế.
    const MIN_ROWS_FOR_WIDTH: usize = 6;
    let wrapped = max_len == BTN_CMD_MAX && rows.len() >= MIN_ROWS_FOR_WIDTH;
    let width = rows.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    for (li, raw) in rows.iter().enumerate() {
        let raw = *raw;
        // Câu đang CẤM một lệnh thì không phải câu mời chạy nó.
        if forbids(raw) {
            continue;
        }
        // 🔴 MỘT LỆNH CỤT LÀ MỘT LỆNH KHÁC. Đo được 2026-08-14 trên màn thật
        // của `[tfl5]`, ba dòng liền nhau trong câu trả lời `/shot`:
        //     [10] len=58  "  git -C /Users/hanguyen/projects/AI/tfl5 push origin main"
        //     [11] len=57  "  bash /Users/hanguyen/projects/AI/tfl5/scripts/deploy.sh"
        //     [12] len=27  "  static-cache-refresh-0814"
        // Cửa sổ rộng ~58, nên dòng 11 là NỬA ĐẦU của một lệnh bị bẻ — mà nửa
        // ấy dài 55 sau khi trim, tức LỌT trần 60 và ra một cái nút. Sổ ghi lại
        // đúng như thế: `bash …/scripts/deploy.sh`, không có tên bản. Bấm vào
        // là chạy một lệnh triển khai THIẾU tham số, trên một dự án thật.
        //
        // Trần `BTN_CMD_MAX` sinh ra để chặn chuyện này nhưng chỉ chặn được mẩu
        // DÀI; mẩu cụt lại thường NGẮN, nên nó lọt qua đúng cái cửa dựng ra để
        // giữ nó. Phép đo đúng không phải độ dài, mà là **dòng có chạm mép cửa
        // sổ không**.
        //
        // Và phép đo phải trỏ đúng chỗ: dòng bị bẻ KHÔNG dài bằng mép. TUI bẻ
        // theo TỪ, nên nó dừng ở khoảng trắng cuối cùng lọt vào bề ngang —
        // dòng 11 dài 57 trên một cửa sổ rộng 80. Đo "chạm mép" bằng độ dài là
        // đo trượt; tôi đã viết bản ấy trước, và chính phân bố độ dài của màn
        // thật (max 80, ba dòng đạt 80) bác bỏ nó.
        //
        // Dấu hiệu ĐÚNG của word-wrap: **từ đầu tiên của dòng sau không vừa
        // vào chỗ trống còn lại** — 25 ký tự `static-cache-refresh-0814` không
        // lọt vào 23 chỗ trống, nên nó bị đẩy xuống. Một câu văn đi sau một
        // dòng lệnh thì ngược lại: từ đầu của nó thường vừa, tức nó xuống dòng
        // vì người viết muốn thế.
        //
        // Nối lại bằng MỘT dấu cách (word-wrap nuốt đúng một cái). Có dấu hiệu
        // bẻ mà đuôi không nhận ra được ⟹ **THÔI, không dựng nút**: thà thiếu
        // một cái nút còn hơn một cái nút chạy lệnh khác lệnh in ra.
        //
        // Nối LẶP, vì một lệnh dài bị bẻ hai lần vẫn là một lệnh — nhưng có
        // trần: bẻ quá ba lần thì đây là một đoạn văn, không phải một dòng.
        let mut joined = String::new();
        let mut cap = max_len;
        let mut cur = raw;
        let mut k = li;
        if wrapped && width > 0 {
            for _ in 0..3 {
                // Chỗ trống đo trên DÒNG MÀN cuối cùng đã gộp (`rows[k]`),
                // không trên chuỗi đã nối: chuỗi nối dài hơn bề ngang cửa sổ
                // theo đúng định nghĩa, nên đo trên nó thì "chỗ trống" luôn
                // bằng 0 và mọi dòng kế tiếp đều trông như bị đẩy xuống.
                let space_left = width.saturating_sub(rows[k].chars().count());
                let pushed = rows
                    .get(k + 1)
                    .and_then(|n| n.split_whitespace().next())
                    .is_some_and(|w| w.chars().count() + 1 > space_left);
                if !pushed {
                    break;
                }
                match wrap_tail(rows.get(k + 1).copied()) {
                    Some(tail) => {
                        joined = format!("{} {}", cur.trim_end(), tail);
                        // Đã nối xong thì đây là dòng TRỌN VẸN, không còn là
                        // mẩu — nên trần "sợ mẩu cụt" thôi áp: một lệnh
                        // triển khai có đường dẫn tuyệt đối vượt 60 là chuyện
                        // thường, và đúng nó là thứ đáng bấm.
                        cap = BTN_CMD_REPORT_MAX;
                        cur = joined.as_str();
                        k += 1;
                    }
                    // Có dấu hiệu bẻ mà đuôi không nhận ra được ⟹ THÔI —
                    // nhưng chỉ khi CHƯA nối được lần nào. Đã nối xong một
                    // lần thì dòng đang cầm là trọn vẹn, và cái đứng sau nó
                    // chỉ là dòng kế tiếp của màn.
                    None => {
                        if joined.is_empty() {
                            cur = "";
                        }
                        break;
                    }
                }
            }
        }
        if cur.is_empty() {
            continue;
        }
        let max_len = cap;
        let raw = cur;
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
        // Nguồn của hàm này là MÀN — chữ đã bị TUI bẻ theo bề ngang cửa sổ —
        // nên với một lệnh dài, hub không có cách nào biết mình đang cầm cả
        // dòng hay một mẩu. Không đoán: dài quá thì THÔI, và nói ra ở chỗ gọi.
        // Một lệnh dán tay là phiền; một cái nút chạy lệnh khác lệnh in ra là
        // chuyện không lùi lại được.
        //
        // 60 đo từ chỗ thật: lệnh quen nhất ở đây (`cd ~/projects/hub && ./hub
        // self-install` = 40, `launchctl kickstart -k gui/501/com.dipgle.hubd`
        // = 44) đều lọt, còn bề ngang một dòng terminal thì bắt đầu bẻ quanh
        // 72–80 kể cả thụt lề.
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
                if w.ends_with(".sh") || w.ends_with(".mjs") || w.ends_with(".py") || w.ends_with(".js")
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
/// Câu ấy chỉ trả lời được khi MỞ FILE RA, nên nó được trả lời đúng chỗ:
/// `Inbox::send_document` đọc bằng `read_to_string`, và file nhị phân tự rơi ở
/// đó (không phải UTF-8). Danh sách dưới đây chỉ còn làm một việc rẻ tiền: đừng
/// dựng cái nút mà ai cũng biết trước là bấm vào sẽ hỏng.
///
/// Luật gốc không đổi (luật 5): thứ gì rời khỏi máy này phải soi được. Một ảnh
/// chụp màn hình mang nguyên mật khẩu mà mọi phép quét chuỗi đều nói "sạch".
pub const UNSENDABLE_EXT: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "heic", "bmp", "ico", "tiff", "pdf", "zip", "gz", "tgz",
    "bz2", "xz", "7z", "rar", "dmg", "pkg", "app", "sqlite", "db", "bin", "exe", "dylib", "so",
    "o", "a", "rlib", "wasm", "mp3", "mp4", "mov", "wav", "m4a", "avi", "webm", "ttf", "otf",
    "woff", "woff2",
];

/// Những ĐƯỜNG DẪN FILE hiện trên màn — thứ bấm một cái là nhận được file.
///
/// 🔴 Hà 2026-08-13: *"các nội dung có path file thì nên cho click vào nhận
/// được file để mở trực tiếp trên tele"*. Trước đó cây cầu này một chiều: hub
/// **nhận** được tệp từ Telegram (`getFile`, từ 79ee269) nhưng không gửi ra
/// được cái nào — nên một báo cáo nhắc tới `ARCHITECTURE.md` là nhắc tới thứ
/// người đọc trên điện thoại không mở nổi.
///
/// Nhận theo HÌNH DẠNG như `commands_on_screen`, và hẹp y như thế: phải là
/// đường TUYỆT ĐỐI (`/…` hoặc `~/…`), và không mang đuôi nhị phân đã biết.
/// Đường tương đối thì cố ý bỏ qua — `src/main.rs` trên màn không nói được nó
/// nằm trong dự án nào, mà đoán sai ở đây là gửi nhầm file của dự án khác.
///
/// Phần "có đọc được không" KHÔNG hỏi ở đây: hàm này thuần, không chạm đĩa. Nó
/// được hỏi đúng một lần, đúng lúc gửi — xem `UNSENDABLE_EXT`.
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
            let t = tok.trim_end_matches(['.', ':', '?', '!']);
            if !(t.starts_with('/') || t.starts_with("~/")) || t.len() < 4 {
                continue;
            }
            // Phải có TÊN FILE, không phải một thư mục: đoạn cuối có dấu chấm.
            let Some(last) = t.rsplit('/').next().filter(|l| l.contains('.')) else {
                continue;
            };
            let ext = last.rsplit('.').next().unwrap_or_default().to_lowercase();
            if UNSENDABLE_EXT.contains(&ext.as_str()) {
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
        "block", "⚠", "❌", "🔴", "never", "do not", "don't", "denied", "refus", "dangerous",
        "đừng", "cấm", "không được", "không nên", "từ chối", "nguy hiểm", "thay vì",
    ];
    let c = context.to_lowercase();
    MARKS.iter().any(|m| c.contains(m))
}

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
pub fn parse_choices(screen: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String, usize)> = Vec::new();
    for (idx, line) in screen.lines().enumerate() {
        let t = line.trim();
        // Bỏ dấu con trỏ ❯ nếu có, rồi tìm "<số>." ở đầu.
        let t = t.strip_prefix('❯').map(str::trim_start).unwrap_or(t);
        let Some((num, rest)) = t.split_once('.') else { continue };
        let Ok(n) = num.trim().parse::<usize>() else { continue };
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
    let lines: Vec<&str> = screen.lines().collect();
    for w in out.windows(2) {
        let (from, to) = (w[0].2, w[1].2);
        if lines[from + 1..to].iter().any(|l| !l.trim().is_empty()) {
            return Vec::new();
        }
    }
    out.into_iter().map(|(n, l, _)| (n, l)).collect()
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
        let Some(open) = line.find(" (") else { continue };
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
    /// Màn có dấu hiệu chứa bí mật ⟹ chữ bị giữ lại (điều 5 trong CLAUDE.md).
    ///
    /// Nhưng **vẫn biết chắc có mấy lựa chọn**: đó là một con số đếm được từ
    /// hình dạng, không mang chữ nào ra khỏi máy. Giữ được con số ấy nghĩa là
    /// chốt an toàn không phải mù chỉ vì màn đang hiện một mật khẩu — đúng cái
    /// tình huống mà mù là tệ nhất.
    Withheld { choices: usize, risk: Vec<String> },
    /// Không nhìn được: phiên không có cửa sổ, hoặc Terminal/osascript không
    /// trả lời. Đây KHÔNG phải "không có hộp chọn".
    Blind { why: String },
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
            logging::warn("keys_window_probe_failed", json!({ "tty": tty, "err": e.to_string() }));
            return Look::Blind {
                why: format!("không hỏi được Terminal cửa sổ nào: {e}"),
            };
        }
    };
    let screen = match screen_text(w) {
        Ok(s) => s,
        Err(e) => {
            logging::warn("keys_screen_read_failed", json!({ "window": w, "err": e.to_string() }));
            return Look::Blind {
                why: format!("không đọc được chữ trên màn: {e}"),
            };
        }
    };
    let choices = parse_choices(&screen);
    let risk = crate::sessions::preview_risk(&screen);
    if !risk.is_empty() {
        return Look::Withheld {
            choices: choices.len(),
            risk,
        };
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
        // Chữ bị giữ lại nhưng con số thì chắc chắn — vẫn quyết được.
        Look::Withheld { choices: 0, .. } => Arrow::Send,
        Look::Withheld { .. } => Arrow::RefuseDialog,
        Look::Blind { why } => Arrow::RefuseBlind(why.clone()),
    }
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
    use super::{activity, arrow_verdict, as_string, landed, window_script, Arrow, Landed, Look};

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
        let quiet = Look::Saw { body: "$ ".into(), choices: vec![] };
        assert_eq!(arrow_verdict(&quiet), Arrow::Send);

        let asking = Look::Saw {
            body: "Chọn đi?".into(),
            choices: vec![(1, "một".into()), (2, "hai".into())],
        };
        assert_eq!(arrow_verdict(&asking), Arrow::RefuseDialog);

        // Màn có bí mật: CHỮ bị giữ lại, nhưng con số lựa chọn vẫn chắc chắn —
        // nên chốt không bị mù chỉ vì màn đang hiện một mật khẩu.
        let secret_quiet = Look::Withheld { choices: 0, risk: vec!["credential_word_vi".into()] };
        assert_eq!(arrow_verdict(&secret_quiet), Arrow::Send);
        let secret_asking = Look::Withheld { choices: 2, risk: vec!["credential_word_vi".into()] };
        assert_eq!(arrow_verdict(&secret_asking), Arrow::RefuseDialog);

        // Không nhìn được thì KHÔNG gửi — và câu từ chối phải mang theo lý do,
        // không thì người ta không biết bấm lại có ích gì không.
        let blind = Look::Blind { why: "osascript hết giờ".into() };
        match arrow_verdict(&blind) {
            Arrow::RefuseBlind(why) => assert!(why.contains("osascript"), "phải nói lý do: {why}"),
            other => panic!("mù mà vẫn gửi: {other:?}"),
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
        assert_eq!(landed(busy_with_queue), Landed::Queued);
        // Đang chạy mà chưa có hàng chờ.
        assert_eq!(landed("  (1m 2s · esc to interrupt)"), Landed::Running);
        // Đứng ở dấu nhắc.
        assert_eq!(landed("❯ \n  ⏵⏵ auto mode on"), Landed::Idle);
    }

    #[test]
    fn busy_is_read_from_the_clock_not_the_word() {
        use super::is_busy;
        assert!(is_busy("✶ Unravelling… (2m 36s · ↓ 2.0k tokens · thinking)"));
        assert!(is_busy("✻ Pondering… (12m 4s · ↑ 900 tokens)"));
        assert!(is_busy("· Herding cats… (0m 8s ·)"));
        // Rảnh: dấu nhắc trống, dòng gợi ý, không có đồng hồ.
        assert!(!is_busy("❯ \n⏵⏵ auto mode on (shift+tab to cycle) · esc to interrupt"));
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
        assert!(super::ask_table("← quay lại · tiếp →").is_none(), "không có ô nào");
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

    #[test]
    fn window_script_is_well_formed() {
        let s = window_script("/dev/ttys005");
        assert!(s.contains(r#"is "/dev/ttys005" and"#), "{s}");
        // Tab ĐÃ CHẾT vẫn khai tty cũ, và tty thì bị dùng lại — ba cửa sổ cùng
        // khai `/dev/ttys005` (đo 2026-08-11, hai trong ba là xác). Khớp tty
        // trần là trả về một cửa sổ ma: màn hình sai lên điện thoại, `/type` gõ
        // vào chỗ không ai đọc.
        assert!(s.contains("count of (processes of t)) > 0"), "phải lọc tab còn sống:\n{s}");
        assert!(s.contains("busy of t"), "phải ưu tiên tab đang chạy chương trình:\n{s}");
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
        assert_eq!(s.matches("try").count() - s.matches("end try").count(), 1, "{s}");
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

