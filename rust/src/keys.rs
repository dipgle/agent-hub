//! Gõ vào cửa sổ terminal của một phiên, và chụp lại cửa sổ ấy.
//!
//! # Vì sao có tệp này
//!
//! Cho tới 2026-08-09 huba **không gõ được** vào phiên interactive: `claude` từ
//! chối `--resume` một phiên đang chạy, và không có primitive nào nhét chữ vào
//! đó (`CLAUDE.md` điều 10). Hệ quả thực tế: một phiên dừng lại hỏi *"chọn
//! phương án nào?"* thì từ điện thoại **không thấy và không trả lời được** —
//! bản ghi câu hỏi chỉ vào nhật ký SAU khi lượt kết thúc, nên nó vô hình cả với
//! `sessions::stream`.
//!
//! Hà chốt 2026-08-09, sau khi tôi nêu rõ đánh đổi: cho huba **gõ tự do** vào
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
//! `System Events` gõ vào **cửa sổ đang ở trước**, nên huba phải kéo cửa sổ ấy
//! lên trước khi gõ. Tức là gõ từ điện thoại sẽ **giật tiêu điểm** trên máy.
//! Không có đường vòng: đó là cách macOS cho gõ vào một tiến trình interactive.

use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::json;

use crate::exec::{run, RunOpts};
use crate::logging;

/// Trần cho một lượt `osascript` — **tuỳ ai đang chờ**.
///
/// 🔴 Hà 2026-08-25, ảnh một tin mang `⚠ không đọc được màn: osascript quá
/// 20s`: *"Sao quá 20s lại không chụp được màn"*.
///
/// Trần 20 giây cũ là MỘT con số cho mọi lượt gọi, và lý do của nó chỉ đúng cho
/// một nửa: *"một cái treo ở đây sẽ giữ cả vòng chạy của daemon"*. Đúng với
/// vòng quét định kỳ — ở đó không ai đang chờ, và giữ vòng là giữ mọi thứ khác.
/// Sai với `/shot`: lúc ấy chủ máy **đang ngồi nhìn điện thoại chờ đúng câu trả
/// lời này**, nên bỏ cuộc ở giây thứ 20 là đem câu trả lời của anh đi đổi lấy
/// một vòng quét chẳng có việc gì gấp.
///
/// Đo được cái giá: **386 lượt `osascript quá 20s` trong một ngày** (log
/// 23/08), rải khắp mọi phép hỏi Terminal. Terminal bận theo cơn — một lượt
/// chờ dài hơn thường qua được, còn hai lượt ngắn thì trượt cả hai.
///
/// `exec::lane()` đã biết câu trả lời sẵn: nó theo LUỒNG, và mọi đường đi từ
/// một cú bấm đều đã được `exec::urgent()` đánh dấu. Nên chỗ này không cần thêm
/// tham số nào — chỉ cần hỏi.
/// Lượt `osascript` THÀNH CÔNG gần nhất mất bao nhiêu mili-giây.
///
/// Không có sổ, không có khoá: một số duy nhất, ghi bởi mỗi lượt thành công.
/// Sai một nhịp cũng không sao — nó chỉ dùng để CHỌN TRẦN, không để phán gì.
static OSA_LAST_OK_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Trần cho một lượt `osascript`, **tự giãn theo máy** thay vì gõ cứng.
///
/// 🔴 Hà 2026-08-25, tối máy quá tải: *"sao giờ vào phiên nào cung báo quá 20s
/// ko xem được màn vậy?"* — mọi phiên cùng lúc, vì cả 11 phiên dùng CHUNG một
/// phép dò.
///
/// Đo được đêm ấy: `load average` **41,5 trên máy 8 nhân** (quá tải gấp 5), do
/// mấy lượt `cargo test` của chính tôi cộng với Spotlight đi lập chỉ mục
/// `target/`. Ở mức ấy `osascript` không được cấp CPU trong 20 giây. Đo lại lúc
/// load 13: **chính kịch bản ấy mất 1,0 giây**. Nên không có gì hỏng — nó ĐÓI.
///
/// Một con số gõ cứng không trả lời được câu hỏi ấy, vì câu hỏi là *"máy đang
/// bận tới đâu"* — thứ chỉ đo được lúc chạy. Đúng luật Hà đã chốt: vấn đề
/// runtime thì tự điều chỉnh, đừng gõ cứng ngưỡng.
///
/// Nên: lấy lượt THÀNH CÔNG gần nhất làm thước. Máy rảnh thì `last_ok` ~0,3s và
/// trần đứng nguyên ở mức cũ; máy tải nặng thì `last_ok` nở ra và trần nở theo.
/// Máy hồi phục thì lượt thành công kế tiếp kéo nó về — không cần ai reset.
pub fn osa_timeout() -> Duration {
    osa_budget(
        crate::exec::lane(),
        Duration::from_millis(OSA_LAST_OK_MS.load(std::sync::atomic::Ordering::Relaxed)),
    )
}

/// Phần THUẦN của [`osa_timeout`] — tách ra để kiểm được mà không cần máy bận.
///
/// Ba ràng buộc, và cái thứ ba là cái giữ cho nó không thành một cái bẫy khác:
/// ① **sàn** đúng bằng trần cũ (45s có người chờ · 20s vòng nền) — bản vá này
///   chỉ được NỚI, không được rút ngắn thứ đang chạy đúng;
/// ② **hệ số 6 lần** lượt thành công gần nhất: đủ rộng để qua một cơn tải, đủ
///   hẹp để không ngồi chờ một cửa sổ đã chết thật;
/// ③ **trần cứng** — nếu không thì một lượt chậm bất thường đẩy trần lên vô hạn
///   và huba ngồi chờ mãi. Có người đang chờ thì 180s (anh còn bỏ đi được);
///   vòng nền 90s, vì ở đây một lượt treo giữ cả nhịp quét.
pub fn osa_budget(lane: crate::exec::Lane, last_ok: Duration) -> Duration {
    let (san, tran) = match lane {
        // Có người đang chờ: thà chờ thêm còn hơn trả về một câu "không đọc
        // được màn" mà chính người ấy không làm gì được với nó.
        crate::exec::Lane::Urgent => (Duration::from_secs(45), Duration::from_secs(180)),
        // Vòng nền: một lượt treo ở đây giữ cả vòng chạy của daemon.
        crate::exec::Lane::Background => (Duration::from_secs(20), Duration::from_secs(90)),
    };
    (last_ok * 6).clamp(san, tran)
}

fn osascript(script: &str) -> Result<String> {
    let tran = osa_timeout();
    let bat_dau = std::time::Instant::now();
    let out = run(
        "osascript",
        &["-e", script],
        RunOpts {
            timeout: Some(tran),
            ..Default::default()
        },
    )?;
    if out.timed_out {
        // 🔴 NÓI RA VÌ SAO, đừng chỉ nói con số — Hà 2026-08-25 nhận hàng loạt
        // `⚠ không đọc được màn: osascript quá 20s` và không có cách nào biết đó
        // là máy bận hay cửa sổ chết. Hai chuyện ấy cần hai hành động khác nhau.
        //
        // Lượt thành công gần nhất là câu trả lời rẻ nhất: bình thường ~0,3s,
        // nên một con số vài giây đã tự tố cáo máy đang tải nặng.
        let truoc = OSA_LAST_OK_MS.load(std::sync::atomic::Ordering::Relaxed);
        let vi = if truoc >= 2_000 {
            format!(
                " (máy đang tải nặng: lượt đọc gần nhất đã mất {:.1}s)",
                truoc as f64 / 1000.0
            )
        } else {
            String::new()
        };
        return Err(anyhow!("osascript quá {}s{vi}", tran.as_secs()));
    }
    // Chỉ ghi lượt THÀNH CÔNG: một lượt quá hạn không nói được nó "mất bao lâu",
    // nó chỉ nói "lâu hơn trần" — lấy nó làm thước là tự đẩy trần lên mãi.
    OSA_LAST_OK_MS.store(
        bat_dau.elapsed().as_millis().min(u64::MAX as u128) as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
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
/// về 0, log ghi `keys_typed`, huba báo "⌨ đã bấm" — mà Hà **không thấy hiện
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
    /// tin trước gõ xong, huba bắn hai Enter, đọc màn, rồi trả lời `✓ đã gửi` —
    /// trong khi chữ vẫn nằm nguyên trong ô. Tin sau gõ tiếp vào đúng ô ấy, nối
    /// đuôi, và cuối cùng cả hai đi **làm một tin**.
    ///
    /// Gốc là một PHÉP ĐO MÙ: `landed` chỉ biết ba trạng thái *hàng chờ · đang
    /// chạy · rảnh*, mà "rảnh" ở đây có hai nghĩa ngược nhau — *đã gửi xong* và
    /// *chưa gửi được*. Không có trạng thái này thì mọi màn không-bận đều đọc
    /// thành thành công, và huba **không thể** nói sai theo hướng nào khác.
    ///
    /// 📌 `still_in_box` đã có từ 12-08 và làm đúng việc của nó; nó chỉ không
    /// được ai hỏi sau khi bấm Enter. *Một hàm đúng không được gọi thì bằng
    /// không* — và chỗ nó vắng mặt là chỗ huba tự khen mình.
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
pub fn body_before_box(screen: &str) -> String {
    let Some(i) = box_start(screen) else {
        return screen.to_string();
    };
    let head = &screen[..i];
    // 🔴 BỎ ĐÚNG CÁI Ô, KHÔNG BỎ MỌI THỨ SAU NÓ — 2026-08-19.
    //
    // Bản cũ cắt từ ô nhập tới hết, và điều đó vô hại chừng nào chuỗi đưa vào
    // đúng là một ẢNH MÀN (dưới ô chỉ còn dòng trạng thái). Nhưng hai chỗ gọi
    // thật lại đưa vào **tin `/shot` đã dựng xong**: ảnh màn, rồi khối *"Lời
    // cuối nó nói"* huba nối thêm — và khối ấy nằm SAU ô nhập. Cắt tới hết là
    // vứt luôn nó, tức mọi đường dẫn tệp huba tự viết ra đều mất nút 📎.
    //
    // Bài kiểm `file_button_beside_command` bắt được ngay: đường dẫn báo cáo
    // `.html` nằm ở dòng 29, ô nhập ở dòng 19–21.
    let rest = &screen[i..];
    let mut at = 0usize;
    let mut close = None;
    for (k, line) in rest.split_inclusive('\n').enumerate() {
        let t = line.trim();
        let is_rule = t.chars().count() >= 8 && t.chars().all(|c| "─━—▔═".contains(c));
        if k > 0 && (is_rule || t.starts_with('╰')) {
            close = Some(at + line.len());
            break;
        }
        at += line.len();
    }
    match close {
        Some(end) => format!("{head}{}", &rest[end..]),
        // Ô chạy tới hết màn (bị mép dưới cắt) — không còn gì phía sau để giữ.
        None => head.to_string(),
    }
}

/// Ô nhập BẮT ĐẦU ở byte nào — một cái neo, hai chỗ dùng.
///
/// 🔴 **Bản `claude` hiện nay KHÔNG vẽ khung nữa** — đo 2026-08-19 trên màn thật
/// của phiên `[dwork]` (`ttys000`, 25 dòng): `╭` · `╰` · `│` đều **0 lần**, chỉ
/// còn **một dòng kẻ `─` suốt bề ngang** rồi tới `❯`. Cái neo `rfind('╭')` viết
/// từ 08-12 vì thế trượt ở MỌI lượt đọc, và cả họ hàm dựng trên nó lặng lẽ rơi
/// về đường lùi *"bốn dòng cuối"* — một vùng gần đúng, đủ để mọi thứ trông vẫn
/// chạy.
///
/// Cái giá đo được, cùng ngày, cùng một màn ấy: `still_in_box` đọc nhầm vùng ⟹
/// `type_and_send` không bấm Enter ⟹ **cả khối kết quả `▶️` nằm lại trong ô
/// nhập hơn một tiếng**, trong khi huba đã báo *"✅ Đã chạy trên máy rồi dán kết
/// quả vào [dwork]"*; rồi `clear_box` đếm chữ trên chính vùng sai ấy nên bấm ⊠
/// hai lần đều không sạch (`keys_clear_incomplete` ×2). Hà, ảnh chụp: *"nội
/// dung sao bị chèn lung tung ở đâu vào ô chat"*.
///
/// Nên neo phải nhận CẢ HAI hình dạng, và khi cả hai đều không có thì đường lùi
/// vẫn còn đó — chỉ là nó thôi làm đường chính.
fn box_start(screen: &str) -> Option<usize> {
    if let Some(i) = screen.rfind('╭') {
        return Some(i);
    }
    // Dòng kẻ ngang: viền của ô nhập ở bản không khung. Đòi dài (≥ 8) để một
    // dòng gạch ngắn giữa văn bản không cắt nhầm chỗ.
    //
    // 🔴 KHÔNG PHẢI CỨ VẠCH CUỐI CÙNG. Ô nhập nằm GIỮA HAI vạch (bản chụp thật
    // `tests/fixtures/shot-screen-2026-08-18.txt`), nên "vạch cuối" là viền
    // DƯỚI và vùng sau nó chỉ còn dòng chân `⏵⏵ auto mode on` — lấy nó là mất
    // sạch chữ trong ô. Bài kiểm bắt được ngay lượt chạy đầu.
    //
    // Nên đi ngược từ vạch cuối lên, lấy vạch đầu tiên mà vùng SAU nó có một
    // dòng dấu nhắc `❯` thật. `❯` cũng là con trỏ của hộp chọn (`❯ 1. Set it
    // up`), nên dòng `❯ <số>.` không tính — ở đó không có ô nhập nào.
    let mut rules: Vec<usize> = Vec::new();
    let mut at = 0usize;
    for line in screen.split_inclusive('\n') {
        let t = line.trim();
        if t.chars().count() >= 8 && t.chars().all(|c| "─━—▔═".contains(c)) {
            rules.push(at);
        }
        at += line.len();
    }
    rules.into_iter().rev().find(|i| {
        screen[*i..].lines().any(|l| {
            let t = l.trim();
            match t.strip_prefix('❯') {
                Some(rest) => {
                    let rest = rest.trim_start();
                    !rest
                        .split_once('.')
                        .is_some_and(|(n, _)| n.trim().parse::<usize>().is_ok())
                }
                None => false,
            }
        })
    })
}

pub fn box_region(screen: &str) -> String {
    if let Some(i) = box_start(screen) {
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
    // `$` nằm trong bộ bỏ đi cùng `❯`, và đó là một cặp: khối dán mở đầu dòng
    // lệnh bằng `$ `, còn TUI vẽ lại chính dòng ấy dưới dấu nhắc `❯ ` của nó.
    // Không bỏ thì hai chuỗi chỉ khác nhau đúng một ký tự ở đầu — và khác một
    // ký tự là trượt hẳn (đo 2026-08-19, `[dwork]`).
    let squash = |s: &str| -> String {
        s.chars()
            .filter(|c| !c.is_whitespace() && !"│┃|>❯$".contains(*c))
            .collect()
    };
    // ⚠ CHỈ soi trong Ô NHẬP, không soi cả màn.
    //
    // Đây là chỗ phép đo suýt trỏ sai: gửi đi RỒI thì `claude` in lại chính câu
    // ấy vào phần hội thoại phía trên — chữ vẫn còn trên màn, mà ý nghĩa ngược
    // hẳn. Soi cả màn thì huba đọc "đã gửi" thành "còn nằm trong ô", rồi bắn một
    // Enter thừa và báo sai cho chủ máy. Ô nhập là khối đóng khung cuối cùng.
    // 🔴 KHÔNG CÓ Ô NHẬP THÌ KHÔNG CÓ GÌ "CÒN NẰM TRONG Ô" — Hà 2026-08-25:
    // *"gõ 1 lệnh mà có 4 lần enter"*.
    //
    // `box_region` rơi về **bốn dòng cuối** khi `box_start` không thấy khung.
    // Đường lùi ấy đúng cho màn `claude` mà phép dò khung trượt; nó SAI hẳn cho
    // một cửa sổ shell trần: ở đó `do script` đã gửi lệnh rồi, và dòng lệnh vừa
    // gõ **nằm lại trên màn vĩnh viễn** dưới dấu nhắc (`… projects % cd
    // projects`). Nên phép kiểm luôn trả "còn trong ô", và `type_and_send` bắn
    // thêm hai cú Enter — đúng mấy dấu nhắc trống Hà chụp được.
    //
    // Không thấy khung ⟹ trả `false`. Vừa vá đúng ca này, vừa bỏ một cú Enter
    // BẮN MÙ: quyết định bấm Enter dựa trên bốn dòng cuối của một màn không rõ
    // hình dạng chính là thứ `CLAUDE.md` §13 cấm — một Enter lạc thì không lùi
    // lại được (ca `☐ RPC pool` → `☒` của phiên amm).
    if box_start(screen).is_none() {
        return false;
    }
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
    // Khối dán ấy nhiều dòng (`[huba chạy hộ]` · `$ <lệnh>` · đầu ra). Ô nhập
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
    if seen.contains(&tail) || seen.contains(&head) {
        return true;
    }
    // 🔴 VÀ MỘT KHÚC BẤT KỲ, không chỉ hai đầu — 2026-08-19. Ca đo được: khối
    // bốn dòng (`[huba chạy hộ]` · `$ <lệnh>` · `✅ xong (0.1s)` · đầu ra) dán vào
    // một cửa sổ 80×24 ở đáy màn. Ô nhập hiện được đúng **một khúc giữa**: dòng
    // đầu đã cuộn khỏi ô, dòng cuối nằm dưới mép màn, và dòng lệnh ở giữa thì
    // bị GẤP DÒNG nên cũng không còn nguyên vẹn. Cả `head` lẫn `tail` đều vắng
    // mặt ⟹ phép đo trả "chữ đã đi" ⟹ không bấm Enter ⟹ cả khối nằm lại trong ô
    // hơn một tiếng, trong khi huba đã báo *"✅ đã dán kết quả vào phiên"*.
    //
    // Hai đầu — và cả "từng dòng", bản vá đầu tiên tôi viết cho đúng ca này —
    // đều là phép đoán về CHỖ TUI cắt. Cửa sổ trượt thì không đoán: chỉ cần
    // **một khúc 24 ký tự** của khối còn nhìn thấy được là đủ kết luận chữ chưa
    // đi. `squash` bỏ hết khoảng trắng kể cả dấu xuống dòng, nên một dòng bị gấp
    // vẫn nối lại thành chuỗi liền — đúng thứ làm phép so "từng dòng" trượt.
    const WIN: usize = 24;
    const STRIDE: usize = 12;
    let ch: Vec<char> = t.chars().collect();
    if ch.len() < WIN {
        return false;
    }
    (0..=ch.len() - WIN).step_by(STRIDE).any(|i| {
        let w: String = ch[i..i + WIN].iter().collect();
        seen.contains(&w)
    })
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
/// Vì sao huba cần biết mở cửa sổ (Hà 2026-08-11: *"cli claude cài trên máy tôi,
/// huba là cầu kết nối ra ui"*): một phiên `--bg` là hạng phiên **chủ máy không
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
    // chết ở dòng đọc id. Kết cục: một cửa sổ mới mồ côi (huba không biết nó
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

/// PID của Terminal.app — đích của [`crate::cgkeys::post`].
///
/// Hỏi `ps` chứ không hỏi AppleScript: đây là câu hỏi về TIẾN TRÌNH, và
/// osascript là thứ huba đang tìm cách đi vòng qua ở đường phím rời. Hỏi nó một
/// câu nữa là buộc đường mới vào đúng chỗ nghẽn của đường cũ. (Hỏi System Events
/// còn tệ hơn: nó đòi thêm một quyền Automation nữa cho một con số.)
///
/// 🔴 `pgrep -x Terminal` là bản đầu và nó trả RỖNG trên chính máy này (đo
/// 2026-08-19) trong khi `ps aux` thấy tiến trình ấy rõ ràng — nên đừng đổi lại.
/// Khớp theo ĐUÔI đường dẫn: `comm` là
/// `/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal`.
pub fn terminal_pid() -> Result<i32> {
    let out = run(
        "ps",
        &["-Ao", "pid=,comm="],
        RunOpts {
            timeout: Some(osa_timeout()),
            ..Default::default()
        },
    )?;
    for line in out.stdout.lines() {
        let Some((pid, comm)) = line.trim().split_once(char::is_whitespace) else {
            continue;
        };
        // Đuôi `/Terminal` chứ không phải chuỗi con `Terminal`: `iTerm`,
        // `Terminal Helper`, hay một tệp nào đó có chữ ấy trong đường dẫn đều
        // KHÔNG phải cái app huba đang nói chuyện qua AppleScript.
        if comm.trim().ends_with("/MacOS/Terminal") {
            return pid
                .parse::<i32>()
                .map_err(|e| anyhow!("ps trả '{pid}', không đọc ra pid: {e}"));
        }
    }
    Err(anyhow!("Terminal.app không chạy"))
}

/// Đưa cửa sổ ấy lên làm cửa sổ NHẬN PHÍM của Terminal.
///
/// 🔴 Phím rời đi bằng `CGEventPostToPid` tới cả TIẾN TRÌNH, không tới một cửa
/// sổ — Terminal tự phát nó cho cửa sổ đang nhận phím của mình. Nên bước này
/// KHÔNG phải thủ tục lịch sự: bỏ nó là gõ vào cửa sổ nào tình cờ đang được
/// chọn, tức đúng cái lỗi "gõ vào việc của người khác" mà cả tệp này đứng ra
/// chặn.
///
/// Vẫn đi bằng AppleScript, và ở đây thì đúng chỗ: nó chỉ SẮP XẾP cửa sổ, không
/// gửi phím nào, nên cái CR của `do script` không dính dáng gì.
pub fn focus_window(window: i64) -> Result<()> {
    osascript(&format!(
        "tell application \"Terminal\"\n\
           set frontmost of window id {window} to true\n\
         end tell"
    ))?;
    Ok(())
}

/// Dãy phím đưa con trỏ NGANG về đúng tab số `target` (đếm từ 1; `0` = bước
/// `Review your answers` ở cuối).
///
/// 🔴 KHÔNG đếm từ chỗ đang đứng, vì huba KHÔNG BIẾT nó đang đứng đâu: tab hiện
/// hành được vẽ bằng màu nền, mà `contents of tab` trả chữ trần nên màu không đi
/// qua. Cách duy nhất chắc chắn là **về mốc rồi đếm từ mốc**.
///
/// 📐 Mốc ấy có thật, và đây là phép đo dựng nên nó (2026-08-19, bảng 3 câu của
/// phiên `[AI/tcc/amm]`, gửi bằng [`crate::cgkeys`] nên không phím nào chốt gì):
/// * `→` **không quấn vòng**: 6 lượt liên tiếp đều đứng lại ở `Review your
///   answers` — bước cuối bên phải.
/// * `←` cũng không quấn: 6 lượt thì bước 3, 4, 5 đều là *"Mặt ĐỌC của native
///   pool…"* — tức câu số 1, mép trái.
/// * Thứ tự đọc được đúng như thanh tab vẽ: `RPC pool` → `NativeAssets v3` →
///   `Việc tiếp` → `Review`.
/// * `answered` giữ nguyên `[true,false,false]` qua **cả 12 lượt** — bằng chứng
///   phím ngang gửi kiểu này không chốt câu nào.
///
/// Nên: đẩy sát mép trái bằng `tabs + 1` lượt `←` (thừa một lượt cho chắc, vì
/// mép trái nuốt lượt thừa), rồi đi sang phải `target - 1` lượt.
pub fn tab_keys(tabs: usize, target: usize) -> Vec<String> {
    let right = || "right".to_string();
    // Bước Review nằm sát mép PHẢI, nên nó rẻ hơn: cứ đẩy hết sang phải.
    if target == 0 {
        return std::iter::repeat_n(right(), tabs + 1).collect();
    }
    let mut keys: Vec<String> = std::iter::repeat_n("left".to_string(), tabs + 1).collect();
    keys.extend(std::iter::repeat_n(right(), target.saturating_sub(1)));
    keys
}

/// Gửi một dãy phím RỜI vào cửa sổ ấy — không kèm dấu xuống dòng nào.
///
/// Hai bước, và bước đầu không phải thủ tục: [`focus_window`] quyết định phím
/// rơi vào cửa sổ NÀO (xem chú thích ở đó), rồi [`crate::cgkeys::post`] mới đưa
/// phím vào tiến trình Terminal.
pub fn send_bare(window: i64, keys: &[String]) -> Result<()> {
    let pid = terminal_pid()?;
    focus_window(window)?;
    crate::cgkeys::post(pid, keys)
}

/// Mỗi lượt cuộn bao nhiêu dòng, và cuộn tối đa mấy lượt — xem [`screen_scrollback`].
///
/// Nhỏ hơn chiều cao khung một chút để hai khung liên tiếp CHỒNG NHAU: chỗ ghép
/// cần phần chồng ấy để biết nối vào đâu. Bằng đúng chiều cao là hai khung khít
/// nhau, và một dòng rớt giữa hai lượt vẽ thì không gì phát hiện được.
const SCROLL_STEP: i32 = 8;
/// Trần lượt cuộn: 40 × 8 = 320 dòng ngược. Có trần vì đây là cửa sổ chủ máy
/// đang nhìn, và mỗi lượt tốn ~0,1 giây cộng một lượt đọc màn.
const SCROLL_MAX_STEPS: usize = 40;

/// Ghép một khung CŨ HƠN vào phía trên phần đã có.
///
/// Hai khung liên tiếp chồng nhau, nên phép ghép là: tìm đoạn chồng dài nhất
/// giữa ĐUÔI khung cũ và ĐẦU phần đã có, rồi nối phần không chồng lên trước.
/// Không tìm được chỗ chồng thì nối thẳng — thà thừa một khung còn hơn mất một
/// đoạn, và chỗ nối vẫn đọc được.
///
/// So bằng dòng đã cắt khoảng trắng: TUI vẽ lại có thể đổi phần đệm bên phải
/// giữa hai lượt, mà một dấu cách thừa không được phép biến hai bản sao của
/// cùng một dòng thành hai dòng khác nhau.
fn merge_above(older: &[String], have: &[String]) -> Vec<String> {
    let key = |s: &String| s.trim_end().to_string();
    let max = older.len().min(have.len());
    for n in (1..=max).rev() {
        let tail = &older[older.len() - n..];
        let head = &have[..n];
        if tail.iter().map(key).eq(head.iter().map(key)) {
            let mut out: Vec<String> = older[..older.len() - n].to_vec();
            out.extend_from_slice(have);
            return out;
        }
    }
    let mut out = older.to_vec();
    out.extend_from_slice(have);
    out
}

/// Đọc NGƯỢC lịch sử phiên bằng cách cuộn chuột, rồi trả màn về đáy.
///
/// 🔴 Hà 2026-08-20: *"Chỉ cần focus tới cửa sổ di chuột tới khung nhìn cuộn
/// chuột là được"* — sau khi vặn đúng chỗ tôi nói hớ: *"một phiên chạy tầm 50%
/// context thì nó phải dài ít nhất 10 trang màn hình"*.
///
/// Anh đúng. Ba đường trước đều KHÔNG lấy được 10 trang ấy, và mỗi đường hỏng
/// một kiểu (đo cùng ngày, đừng dò lại):
/// * `history of tab` — bộ đệm cuộn của Terminal chạy tốt (shell thường: 504
///   dòng cho 500 dòng in ra), nhưng `claude` không đẩy dòng nào vào đó. Đọc 4
///   lần cách nhau nhiều phút vẫn đúng 43 dòng, đóng băng ở phần trước lúc CLI
///   khởi động.
/// * Menu `View ▸ Scroll to Top` — click được (`MENU-OK`), `contents` không đổi
///   một ký tự. Đó là cuộn của Terminal, và Terminal chẳng giữ gì để cuộn.
/// * Nới cửa sổ hết cỡ ([`screen_text_tall`]) — lấy thêm thật, nhưng đụng trần
///   cứng 61×206 của màn hình.
///
/// Bánh xe thì đi thẳng vào TUI, và TUI có sẵn cả lịch sử trong bộ nhớ nó: đo
/// được 934 → 1391 ký tự sau 10 lượt, đầu khung lùi về một đoạn đã trôi.
///
/// Ba điều hàm này giữ:
/// * **luôn trả màn về đáy**, kể cả khi đọc hỏng giữa chừng — cửa sổ này là cửa
///   sổ chủ máy đang nhìn, bỏ nó ở lưng chừng quá khứ là một lỗi thấy được;
/// * cuộn xuống DƯ (`+4` lượt) chứ không đếm cho khít: đáy nuốt lượt thừa, còn
///   thiếu một lượt là màn nằm lại lưng chừng;
/// * **dừng sớm khi không lấy thêm được gì** — hai lượt liên tiếp không thêm
///   dòng nào nghĩa là đã tới đầu lịch sử, và cuộn tiếp chỉ tốn thời gian của
///   người đang chờ trên điện thoại.
pub fn screen_scrollback(window: i64, steps: usize, du: impl Fn(&str) -> bool) -> Result<String> {
    let steps = steps.min(SCROLL_MAX_STEPS);
    // 🔴 LÀN GẤP, và đây không phải tinh chỉnh cho vui — đo 2026-08-20: vòng
    // cuộn-rồi-đọc mất **6,3 giây mỗi nấc** trong mã, trong khi đúng hai lệnh ấy
    // gõ từ shell mất **0,20 giây**. Gấp ba mươi lần, và thủ phạm là
    // `exec::lane_wrap`: nó bọc mọi lệnh bằng `taskpolicy -b`, tức QoS NỀN —
    // đúng cho vòng quét định kỳ, sai hẳn cho một cú `/shot` có người đang cầm
    // điện thoại chờ. 12 nấc vì thế thành 76 giây.
    let _lane = crate::exec::urgent();
    let pid = terminal_pid()?;
    focus_window(window)?;

    let lines_of = |s: &str| -> Vec<String> { s.lines().map(str::to_string).collect() };
    let mut have = lines_of(&screen_text(window)?);
    let mut dry = 0;
    let mut used = 0;

    for _ in 0..steps {
        if let Err(e) = crate::cgkeys::scroll(pid, SCROLL_STEP, 1) {
            // Không cuộn được thì thôi cuộn — nhưng vẫn phải trả màn về đáy,
            // nên đừng `?` ở đây.
            crate::logging::warn(
                "scroll_read_stopped",
                serde_json::json!({ "window": window, "err": e.to_string(),
                                    "effect": "trả về phần đọc được tới lúc này" }),
            );
            break;
        }
        used += 1;
        std::thread::sleep(std::time::Duration::from_millis(120));
        let Ok(frame) = screen_text(window) else {
            break;
        };
        let before = have.len();
        have = merge_above(&lines_of(&frame), &have);
        // ĐỦ RỒI THÌ DỪNG. Chỗ gọi biết "đủ" nghĩa là gì (thường: lời cuối theo
        // nhật ký đã nằm trọn trong đây), và cuộn thêm sau khi đủ là kéo cửa sổ
        // của chủ máy đi xa hơn mức cần, cho một người đang chờ trên điện thoại.
        if du(&have.join("\n")) {
            break;
        }
        if have.len() == before {
            dry += 1;
            if dry >= 2 {
                break;
            }
        } else {
            dry = 0;
        }
    }

    // Trả về đáy — dư mấy lượt cho chắc. Đây là bước KHÔNG được bỏ qua.
    if used > 0 {
        if let Err(e) = crate::cgkeys::scroll(pid, -SCROLL_STEP, used + 4) {
            crate::logging::error(
                "scroll_restore_failed",
                serde_json::json!({ "window": window, "err": e.to_string(),
                                    "effect": "MÀN CỦA CHỦ MÁY CÒN Ở LƯNG CHỪNG QUÁ KHỨ" }),
            );
        }
    }
    crate::logging::info(
        "scroll_read",
        serde_json::json!({ "window": window, "steps": used, "lines": have.len() }),
    );
    Ok(have.join("\n"))
}

/// Cửa sổ Terminal đang chạy `tty` này, nếu có.
///
/// `Terminal` công bố `tty` của từng tab qua AppleScript (đo 2026-08-09:
/// `/dev/ttys005, /dev/ttys000, …`), và huba đã biết `tty` của từng phiên từ
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
        // gọi ở mọi `/type`, `/key`, `/shot` — tức huba hỏi lại Terminal hàng
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
/// huba vẽ ra một cái nút trỏ vào chỗ chính nó nói là không tồn tại. Cùng hình
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
/// completed]` là gõ vào chỗ không ai đọc, rồi huba ngồi chờ một cú thoát không
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
/// Đây là câu trả lời cho "huba gõ vào phiên nào được": `type_into` đi qua
/// `do script` của Terminal, nên phiên nào Terminal không giữ thì huba không có
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
    /// Nhan đề tab — và với tab đang chạy `claude`, đó là **câu tóm tắt của
    /// chính CLI về việc nó đang làm**.
    ///
    /// Đo 2026-08-19, tám tab trên máy này: `✳ Chốt mockup doc và driver` ·
    /// `◐ Tiếp tục N7 lát 2 kiểm tra khuôn mặt` · `✳ Tiếp tục DS04 quét mã và
    /// nhập xuất XML` · `◑ Continue huba improvements and run quality gate`.
    /// Ba trong số ấy cùng dự án `dwork` — tức đây là thứ phân biệt được đúng
    /// chỗ mà nhãn dự án bó tay (xem `sessions::label_sessions`).
    ///
    /// Nó KHÔNG phải một phép đoán của huba: chính `claude` đặt nhan đề ấy qua
    /// escape OSC, cùng chữ chủ máy đang thấy trên thanh tab. Cùng hạng bằng
    /// chứng với `status`/`state` của `claude agents` — huba chỉ chở đi.
    ///
    /// Rỗng = tab không có nhan đề riêng, hoặc lượt dò không hỏi tới.
    pub title: String,
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

    /// Tab này có đang chạy `claude` không — **so KHÔNG phân biệt hoa thường**.
    ///
    /// 🔴 Đo 2026-08-19, và nó khép lại một câu hỏi treo từ sáng: cửa sổ
    /// `ttys001` chạy một tiến trình tên **`Claude`** (chữ C hoa), tên cửa sổ
    /// `hanguyen — Claude — 80×24`. Ổ đĩa macOS không phân biệt hoa thường nên
    /// `Claude "tiếp social"` gõ tay vẫn phân giải ra đúng binary và chạy thật —
    /// nhưng mọi phép so `== "claude"` của huba thì trượt, nên vòng nào huba cũng
    /// ghi `terminal_tab_busy_unmatched` về đúng cái tab ấy và không bao giờ đọc
    /// tới nó (không nhận ra hộp tin-thư-mục, không đọc được việc đang làm).
    ///
    /// huba KHÔNG bao giờ tự phát chữ hoa (`config.rs:489` · `sessions.rs:4245` ·
    /// `main.rs:245` đều `claude` thường) — nên hình dạng này chỉ tới từ tay
    /// người gõ, và tay người thì không có lý do gì phải gõ đúng chữ thường.
    pub fn is_claude(&self) -> bool {
        self.cli().is_some_and(|c| c.eq_ignore_ascii_case("claude"))
    }

    /// VIỆC tab này đang làm, bằng chữ của chính `claude` — `None` khi không có.
    ///
    /// Ba cửa, và mỗi cửa đóng một ca đã đo được trên máy này (2026-08-19):
    ///
    /// 1. **Tab phải đang chạy `claude`.** Nhan đề của một tab khác là nhan đề
    ///    của thứ khác. Đo: `ttys001` chạy `Claude` (chữ C hoa — macOS phân giải
    ///    không phân biệt hoa thường nên nó vẫn ra đúng binary), nhan đề vẫn là
    ///    `Terminal` mặc định, tức phiên ấy chưa từng đặt tên việc.
    /// 2. **Bỏ dấu quay ở đầu.** `claude` đính chỉ báo đang-chạy vào nhan đề
    ///    (`✳`, `◐`, `◑`, …) và bộ ký tự ấy đổi theo phiên bản CLI — nên bỏ theo
    ///    TÍNH CHẤT (mọi thứ không phải chữ/số đứng đầu), không theo một danh
    ///    sách gõ sẵn sẽ mục.
    /// 3. **`Terminal` trần thì không tính.** Đó là tên hồ sơ mặc định của
    ///    Terminal.app, không phải một câu tóm tắt.
    ///
    /// Trùng nhau thì sao: hàm này không biết, và không cần biết — chỗ gọi
    /// (`sessions::label_sessions`) nhìn CẢ TẬP, nên hai tab cùng nhan đề vẫn
    /// rơi về mã id ngắn. Cùng luật với `label_sessions`: "có trùng ai không" là
    /// câu hỏi của tập, không hàng nào tự trả lời được.
    pub fn doing(&self) -> Option<&str> {
        if !self.is_claude() {
            return None;
        }
        let t = self
            .title
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .trim();
        (!t.is_empty() && t != "Terminal").then_some(t)
    }
}

/// Mọi tab Terminal đang mở, kèm thứ đang chạy trong đó.
///
/// 🔴 Hà 2026-08-13: *"mỗi cửa sổ terminal là một phiên thì sẽ quản lý được
/// phiên nào đang chạy cli phiên nào không"* · *"vào phiên (terminal) chưa chạy
/// gì → gõ lệnh bình thường như đang gõ ở terminal là được rồi"*.
///
/// Đây là phép đo mà mô hình ấy đứng lên: cho tới nay huba đi từ `claude agents`
/// rồi mới tìm cửa sổ (`window_of` theo tty), nên một cửa sổ **không chạy CLI**
/// là thứ huba không có cách nào biết là có tồn tại. Ngồi trước máy thì nó nằm
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
    // 🔴 KHUNG BẢN TIN: mỗi tab là một dòng đầu
    // `tty⇥busy⇥procs⇥số-dòng-màn⇥nhan-đề`, rồi ĐÚNG bấy nhiêu dòng chữ màn.
    // Đếm dòng chứ không cắt theo dấu phân cách, vì chữ trên màn là chữ của
    // người khác: bất cứ dấu nào tôi chọn làm ranh giới đều có thể đang nằm sẵn
    // trên một màn nào đó, và hôm nó nằm đó thì phép đọc lệch mà không ai biết.
    // Số dòng thì không giả được — đã đối chiếu trên bản chụp thật 11 tab / 304
    // dòng, mọi dòng đầu rơi đúng chỗ.
    //
    // Nhan đề đứng CUỐI vì nó cũng là chữ của người khác (xem `parse_tabs`), và
    // dấu xuống dòng trong nó bị dập NGAY TẠI ĐÂY: `str::lines()` của Rust cắt
    // theo `\n`, nên một `\n` lọt vào nhan đề sẽ đẻ ra một hàng tab ma. Escape
    // OSC vốn không mang được `\n` — dập ở đây là để phép đọc không phụ thuộc
    // vào việc ấy còn đúng ở bản `claude` sau.
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
        set ct to ""
        try
          set ct to custom title of tb
          if ct is missing value then set ct to ""
        end try
        set AppleScript's text item delimiters to linefeed
        set ctl to text items of (ct as text)
        set AppleScript's text item delimiters to " "
        set ct to ctl as text
        set AppleScript's text item delimiters to ""
        set acc to acc & (tty of tb) & TAB9 & (busy of tb) & TAB9 & ps & TAB9 & np & TAB9 & ct & linefeed{body}
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
        // Nhan đề đứng SAU khung, và ăn hết phần còn lại của dòng — cố ý.
        //
        // Nó là chữ của người khác (chính `claude` đặt), nên nó không được phép
        // xê dịch một trường nào khác: đứng cuối thì một ký tự TAB lọt vào giữa
        // nhan đề chỉ cắt chính nó, và nối lại là xong. Đứng trước `np` thì cùng
        // ký tự ấy đẩy khung đi một cột và mọi tab sau đọc lệch — đúng cái bẫy
        // mà khung-đếm-dòng sinh ra để tránh. (Dấu xuống dòng thì chặn ở đầu
        // kia, trong AppleScript: xem `tabs_script`.)
        let title = f.collect::<Vec<_>>().join("\t").trim().to_string();
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
            title,
        });
    }
    (tabs, skipped)
}

/// Tab ĐANG SỐNG mang tty ấy — `None` nghĩa là huba không có tay nào chạm tới.
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
/// `close_pending_tick` đọc `Err` đúng như luật của nó — *"hỏi không được là huba
/// mù, không phải cửa sổ đã đóng"* — nên giữ nguyên mục trong sổ, hỏi lại sau 30
/// giây, mãi mãi. Luật ấy KHÔNG sai; cái sai là bắt nó phán trên một phép đo
/// không biết nói "không còn".
///
/// Nên câu hỏi "còn tab nào không" phải đứng TRƯỚC, trong cùng một lượt hỏi
/// (hai lượt là hai câu trả lời có thể lệch nhau — bài học của `terminal_ttys`).
/// `on error` ở đây chỉ bắt được lỗi PHÂN GIẢI ĐỐI TƯỢNG của Terminal, tức
/// "không có cửa sổ ấy"; còn `osascript` chết, Terminal câm, quyền bị rút thì
/// vẫn về `Err` như cũ — huba vẫn mù đúng chỗ đáng mù.
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
        // Không đoán. Một câu trả lời lạ là huba mù, và mù thì phải nói là mù —
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
    /// phải huba làm.
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
/// Anh nhìn nhầm cửa sổ (huba nhắm `ttys004`, không phải phiên tfl5), nhưng câu
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
    let how = type_and_send(window, "/exit")?;
    crate::logging::info(
        "keys_exit_sent",
        serde_json::json!({ "window": window, "landed": format!("{how:?}"),
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
///   ❯ (bo qua - dong do hang cho cua huba A)
///   ❯ (bo qua - dong do hang cho cua huba B)
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
    // hàng chờ (phiên `[huba]` nói về nó cả ngày, và đoạn văn ấy cuộn trên màn
    // hàng giờ sau đó).
    //
    // Quét cả màn thì `/clean` đọc ra "có hàng chờ" ở một phiên chẳng có gì để
    // dọn, rồi gửi `↑` — và `↑` khi hàng chờ rỗng KHÔNG phải là không làm gì:
    // nó kéo câu CŨ trong lịch sử vào ô nhập, tức huba tự chèn chữ vào chỗ chủ
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
/// khoá máy · cấp một quyền · hoặc đi kiểm hộ vì huba không đo được. Gộp cả ba
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
        None => "ảnh ra ĐEN, và huba **không đo được** máy có đang khoá màn không (`ioreg` không \
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
/// "kiểm rồi, ảnh ổn", nếu không thì một hôm `sips` biến mất là huba lại lặng lẽ
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

/// Đưa cửa sổ ấy ra TRƯỚC MẶT — trước cả trong Terminal lẫn trước các ứng dụng
/// khác.
///
/// 🔴 Khác [`focus_window`], và khác ở chỗ quyết định: `focus_window` chỉ đặt
/// `frontmost` của MỘT CỬA SỔ, tức nó trả lời câu *"phím sắp gửi rơi vào cửa sổ
/// nào"*. Hàm này trả lời câu KHÁC — *"con người có nhìn thấy cửa sổ ấy không"*
/// — nên nó phải `activate` chính Terminal: sắp đúng thứ tự trong Terminal mà
/// Terminal vẫn nằm sau Chrome thì trên màn hình **không có gì xảy ra cả**.
///
/// Công thức lấy nguyên từ [`photograph_window`], nơi nó đã chạy thật từ 17/08
/// (ảnh gửi về Telegram đúng cửa sổ phiên). Không viết lại bằng cách khác:
/// hôm nay đã trả giá một lần cho việc có hai bản chép của cùng một phép
/// (`runtime::SIGNING_CN`), nên chỗ này là MỘT hàm, hai người gọi.
pub fn bring_to_front(window: i64) -> Result<()> {
    osascript(&format!(
        r#"tell application "Terminal"
  set index of window id {window} to 1
  activate
end tell"#
    ))?;
    Ok(())
}

/// Cửa sổ nào của Terminal đang đứng trước — để KIỂM, không để đoán.
///
/// `osascript` trả 0 chỉ chứng minh câu lệnh chạy xong, không chứng minh cửa sổ
/// đã ra trước (điều 4 của charter: đừng đọc mã thoát của thứ chỉ khởi chạy).
/// Nên [`bring_to_front`] phải có người chấm bài, và người ấy hỏi lại chính
/// Terminal.
///
/// `None` = Terminal không có cửa sổ nào — một câu trả lời thật, khác hẳn
/// `Err` (không hỏi được).
pub fn front_window() -> Result<Option<i64>> {
    let out = osascript(
        r#"tell application "Terminal"
  if (count of windows) is 0 then return ""
  return id of front window as text
end tell"#,
    )?;
    Ok(out.trim().parse::<i64>().ok())
}

pub fn photograph_window(window: i64, path: &std::path::Path) -> Result<()> {
    // Không đưa ra trước được thì vẫn chụp — một tấm ảnh cả màn hình còn hơn
    // không có gì, và câu trả lời sẽ nói rõ là chưa focus được.
    let focused = bring_to_front(window).is_ok();
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
             System Settings → Privacy & Security → Screen Recording → bật cho `hubad` \
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
/// Cửa sổ SHELL trần: gõ `exit`, chờ tiến trình chết, rồi mới đóng.
///
/// 🔴 Vì sao không gọi thẳng [`close_window`]: đóng một cửa sổ còn tiến trình
/// sống bật hộp thoại *"Do you want to terminate running processes?"* của
/// Terminal, và một modal thì **khoá mọi lệnh automation sau nó** — huba câm
/// cho tới khi có người ngồi xuống bấm. Đây là luật đã đóng khung trong
/// [`quit_and_close`], chỉ khác một chữ: shell thoát bằng `exit`, TUI của
/// `claude` thoát bằng `/exit` — gõ nhầm `/exit` vào dấu nhắc zsh chỉ ra
/// `zsh: no such file or directory` rồi cửa sổ nằm nguyên đó.
///
/// Khác `sessions::close_session` ở chỗ CHỜ: hàm này CHẶN cho tới khi xong,
/// nên chỉ gọi từ một luồng có quyền chờ (`watch_terminal_job`). Đường lệnh thì
/// vẫn phải đi qua sổ chờ đóng (`close_pending_tick`), vì nó giữ `CMD_LOCK` và
/// một cú thoát có thể tốn hàng chục phút.
pub fn exit_and_close_shell(window: i64, cho: std::time::Duration) -> Result<Closed> {
    // Tab đã chết thì không có gì để thoát — gõ `exit` vào `[Process completed]`
    // là gõ vào chỗ không ai đọc.
    if tab_proc_count(window).unwrap_or(1) > 0 {
        type_into(window, "exit")?;
        let han = std::time::Instant::now();
        while han.elapsed() < cho {
            std::thread::sleep(std::time::Duration::from_millis(400));
            if tab_proc_count(window).unwrap_or(1) == 0 {
                break;
            }
        }
    }
    close_window(window)
}

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
    // `set visible to false` ăn ngay. Nên huba làm được đúng một nửa việc — và
    // một nửa nói ra được vẫn hơn không nửa nào.
    //
    // Ẩn KHÔNG phải đóng: cửa sổ vẫn còn trong menu Window của Terminal (⌘W khi
    // ngồi máy), nhưng nó rời khỏi mắt và rời khỏi mọi danh sách của huba
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
/// của huba). Với một cửa sổ vốn ĐANG ẩn thì phép đo ấy trả `true` ngay lượt
/// đầu, tức huba sẽ báo *"đã thoát — cửa sổ đã đóng"* cho một cửa sổ còn nguyên. Cùng một cái
/// bẫy "phép đo trỏ nhầm chỗ" đã trả giá hai lần trong tệp này; câu hỏi ở đây
/// là *"cửa sổ ấy còn tồn tại không"*, và `tab_state` là chỗ trả lời nó.
///
/// Vì sao có hàm này: 17/08 lúc 10:20Z, năm cửa sổ từ chối `close` (chạy êm,
/// trả 0, cửa sổ đứng nguyên) nên huba ẩn chúng đi. Bốn tiếng sau, ĐÚNG những
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
    /// huba, mà vẫn còn trong menu Window của Terminal cho tới khi ⌘W.
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
            // Cửa sổ biến mất trong lúc chờ là việc ĐÃ XONG, không phải huba mù:
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
    // 🔴 NHIỀU VÒNG, vì phép đếm chỉ thấy phần HIỆN RA — Hà 2026-08-19, bấm ⊠
    // hai lần liền trên `[dwork]` và cả hai lần `keys_clear_incomplete`.
    //
    // Trong ô lúc ấy là khối kết quả `▶️` bốn dòng, dài hơn phần màn còn trống,
    // nên `input_box_text` đọc ra ~70 ký tự trong khi nội dung thật vài trăm.
    // Bản cũ bắn đúng-bấy-nhiêu-cộng-tám DEL rồi hỏi lại **một lần** — tức nó
    // luôn xoá thiếu, và luôn kết luận đúng ("chưa sạch") về một việc nó chưa
    // làm xong.
    //
    // Và không đo được TIẾN ĐỘ bằng phần nhìn thấy: DEL xoá lùi từ CON TRỎ, mà
    // con trỏ nằm ở CUỐI khối — tức ở phần khuất dưới mép màn. Xoá thật vẫn
    // không làm dòng nhìn thấy đổi một chữ nào, nên "không đổi ⟹ dừng" là một
    // phép đo mù. Đường đúng là bắn từng lô rồi hỏi lại **ô đã trống chưa**,
    // đúng khuôn `clear_queue`: không tin cú bấm, tin lượt đọc sau nó.
    // 🔴 XOÁ HẾT, không xoá cho có — Hà 2026-08-19: *"Sửa lại lệnh clear thành
    // xóa hết text ở ô chat"*. Trần đặt theo thứ DÀI NHẤT huba có thể tự dán vào
    // đó: khối kết quả `▶️` mang tối đa `CMD_OUT_MAX` = 3000 ký tự cộng phần
    // bọc. 16 lô × 400 phủ 6400 — hơn gấp đôi, và vòng lặp dừng NGAY khi ô đọc
    // ra trống, nên ô một dòng vẫn xong sau đúng một lô.
    const ROUNDS: usize = 16;
    const BATCH: usize = 400;
    for _ in 0..ROUNDS {
        let text = input_box_text(&screen_text(window)?).unwrap_or_default();
        let n = text.chars().count();
        if n == 0 {
            return Ok(true);
        }
        // Thừa vài phím: con trỏ có thể không ở cuối, và một ô nhiều dòng đếm ra
        // ngắn hơn thực tế. DEL thừa vào ô trống thì không làm gì.
        let dels: String = std::iter::repeat_n('\u{7f}', (n + 8).max(BATCH)).collect();
        osascript(&do_script(
            window,
            &format!("({} & (ASCII character 27))", as_string(&dels)),
        ))?;
        std::thread::sleep(std::time::Duration::from_millis(600));
    }
    let left = input_box_text(&screen_text(window)?).unwrap_or_default();
    if !left.trim().is_empty() {
        logging::warn(
            "keys_clear_gave_up",
            json!({ "window": window, "rounds": ROUNDS, "batch": BATCH,
                    "left_seen": left.chars().count(),
                    "effect": "xoá hết ngần ấy lô mà ô nhập vẫn còn chữ — chỗ gọi phải nói là CHƯA sạch" }),
        );
    }
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
pub fn type_and_send(window: i64, text: &str) -> Result<Delivered> {
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
                return Ok(Delivered::Unverified(format!("không đọc được màn: {e}")));
            }
        };
        if !still_in_box(&screen, text) {
            // Chữ đã rời ô nhập — `do script` gửi được ngay, đường thường.
            return Ok(Delivered::Gone);
        }
        if !parse_choices(&screen).is_empty() {
            logging::warn(
                "keys_send_held_dialog",
                json!({ "window": window,
                        "effect": "chữ còn trong ô nhập nhưng màn đang có hộp chọn — Enter ở đó là CHỐT, nên không bấm" }),
            );
            return Ok(Delivered::StillInBox);
        }
        press(window, "enter")?;
    }
    // Bấm hết cả hai cú Enter mà lượt đọc cuối vẫn thấy chữ nằm đó ⟹ ĐỌC LẠI
    // một lần nữa rồi mới phán. Cú Enter thứ hai cần thời gian của nó, và một
    // câu "chưa gửi được" nói ra vì không thèm nhìn lại thì cũng sai y như câu
    // "đã gửi" nói ra vì không thèm nhìn.
    std::thread::sleep(std::time::Duration::from_millis(600));
    match screen_text(window) {
        Ok(s) if !still_in_box(&s, text) => Ok(Delivered::Gone),
        Ok(_) => {
            logging::warn(
                "keys_send_left_in_box",
                json!({ "window": window,
                        "effect": "gõ xong, bấm đủ hai cú Enter, chữ VẪN nằm trong ô nhập — chỗ gọi phải nói đúng như vậy" }),
            );
            Ok(Delivered::StillInBox)
        }
        Err(e) => Ok(Delivered::Unverified(format!("không đọc được màn: {e}"))),
    }
}

/// Chữ huba vừa gõ ĐÃ ĐI CHƯA — ba kết cục, không gộp.
///
/// 🔴 Hà 2026-08-19, ảnh chụp ô nhập `[dwork]` mang nguyên khối kết quả `▶️`:
/// *"nội dung sao bị chèn lung tung ở đâu vào ô chat"*. Log cùng lúc:
/// `runin_ran code=0` rồi huba trả lời *"✅ Đã chạy trên máy rồi dán kết quả
/// vào…"* — trong khi khối ấy nằm nguyên trong ô, chưa gửi, và vẫn còn ở đó
/// **một tiếng sau**.
///
/// `type_and_send` trả `Ok(())` cho cả BA đường ra của nó — gửi được, không
/// bấm vì có hộp chọn, và mù vì không đọc được màn — nên chỗ gọi không có cách
/// nào nói khác đi. Cùng một hình dạng với `Look` và `TabState`: một hàm biết
/// ba chuyện mà chỉ kể được một.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivered {
    /// Chữ đã rời ô nhập — đo bằng chính màn, không phải bằng mã trả về.
    Gone,
    /// Chữ VẪN nằm trong ô nhập. Nó ở đó, người ta nhìn thấy nó, và nó chưa chạy.
    StillInBox,
    /// huba không kiểm được (không đọc được màn) — KHÔNG được đọc thành "xong".
    Unverified(String),
}

/// Gửi chữ vào cửa sổ bằng `do script` — API CỦA CHÍNH Terminal.
///
/// Đường cũ đi qua `System Events keystroke`, và macOS chặn thẳng:
/// *"osascript is not allowed to send keystrokes (1002)"*. Cấp Accessibility
/// cho `hubad` không gỡ được, vì thứ gọi AXAPI là `/usr/bin/osascript` —
/// một binary hệ thống, không gán quyền cho nó qua đường daemon được (đo
/// 2026-08-10: cấp quyền rồi khởi động lại daemon, vẫn 1002).
///
/// `do script` thì khác hẳn: nó là scripting API của Terminal, chỉ cần quyền
/// **Automation** — thứ huba đã có, bằng chứng là nó đang đọc được `contents of
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
// Nó từng là "đường DUY NHẤT huba nhìn thấy câu hỏi đang chờ", cho tới khi Hà
// hỏi *"sao lại đẩy ảnh, dựng lại đúng option chứ?"* (08-10) và hoá ra Terminal
// cho đọc thẳng `contents of selected tab` — chữ thuần, không OCR, không vài
// trăm KB base64, và chữ thì đi qua được cổng quét rò rỉ còn ảnh thì không.
// Từ hôm ấy `screen_text` làm hết việc, còn ba hàm kia nằm lại **không một chỗ
// gọi nào** suốt bốn ngày.
//
// Chúng ra đi vì đúng câu Hà hỏi hôm nay: *"Tức là bạn đang chụp ảnh thay vì
// lấy text thuần à"*. Mã chết mang tên `capture` thì câu trả lời "huba không
// chụp ảnh" luôn có một dấu hỏi treo phía sau, kể cả khi nó đúng. Nó cũng là
// thứ duy nhất còn đòi quyền **Screen Recording** — bỏ đi là bớt luôn một quyền
// hệ thống huba không dùng tới.

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

/// Xin HẾT CỠ — Terminal tự kẹp lại cho vừa màn hình.
///
/// 🔴 Hà 2026-08-20: *"Sao không mở rộng cửa sổ ra hết cỡ"*. Số cũ là 60, gõ
/// cứng từ một phép đo trên MỘT màn hình — nên nó vừa bỏ phí một dòng ở đây
/// (trần thật là 61), vừa bỏ phí bao nhiêu tuỳ màn ở máy khác. Đo 20/08 trên
/// chính cửa sổ này: xin `999` ⟹ Terminal trả về **61**, không một lỗi nào.
/// Nó KẸP giùm, nên "hết cỡ" là một con số đúng ở mọi màn hình, còn một con số
/// đo được thì chỉ đúng ở cái màn đã đo.
///
/// Dùng cho CẢ hai chiều: chiều ngang cũng nới, và đó không phải phần thêm cho
/// đẹp — cột rộng thì dòng dài thôi bị bẻ, nên cùng 61 dòng chứa nhiều chữ hơn
/// hẳn. Đo cùng lượt: 24×80 ⟹ 1081 ký tự · nới cao ⟹ 2689 · nới cả ngang
/// (206 cột) ⟹ **3943**. Gấp 3,6 lần bản gốc, và một phần ba số ấy là nhờ
/// chiều ngang.
pub const GROW_ASK: usize = 999;

/// Đọc màn sau khi NỚI CAO cửa sổ, rồi trả lại đúng chiều cũ.
///
/// 🔴 Hà 2026-08-19, sau khi tự cuộn màn rồi gõ `/shot` lại: *"đúng là bạn đang
/// chỉ lấy được đúng nội dung đang trong khung nhìn, vậy những gì gửi lên tele
/// làm sao đủ nội dung ngữ cảnh được?"* · *"phải có cách khác để mọi thứ trong
/// phiên phải thể hiện đúng đủ khi gửi giống như một bản sao hoàn hảo chứ?"*
///
/// Anh đúng, và hai đường tôi thử trước đều KHÔNG phải câu trả lời — đo, không
/// đoán:
/// * `contents of tab` chỉ trả phần ĐANG HIỆN: 26 đoạn / 1487 ký tự.
/// * `history of tab` (toàn bộ cuộn lại) trả **42 đoạn / 3487 ký tự**, mà 16
///   dòng thêm ấy là dòng đăng nhập shell + câu lệnh mở phiên — **không có một
///   dòng hội thoại nào**. Vì TUI vẽ ĐÈ tại chỗ chứ không đẩy chữ ra khỏi màn,
///   nên bộ đệm cuộn của Terminal không giữ gì cả. Đừng thử lại đường này.
///
/// Đường đi được là đường một người ngồi trước máy sẽ làm: **kéo cửa sổ ra hết
/// cỡ**, để chính CLI vẽ lại đủ, đọc, rồi trả lại như cũ.
///
/// 🔴 Hà 2026-08-20: *"Sao không mở rộng cửa sổ ra hết cỡ"* — và cả hai chiều,
/// không riêng chiều cao. Đo trên cửa sổ thật cùng ngày:
/// `24×80 ⟹ 1081 ký tự` · `nới cao ⟹ 61 dòng, 2689` · `nới cả ngang ⟹ 206 cột,
/// 3943`. Chiều ngang đáng một phần ba số ấy, vì cột rộng thì dòng dài thôi bị
/// bẻ — cùng 61 dòng mà chứa nhiều chữ hơn hẳn.
///
/// Xin [`GROW_ASK`] chứ không xin một con số đo được: Terminal KẸP giùm cho vừa
/// màn hình (xin 999, nhận 61×206, không lỗi), nên cùng một dòng mã lấy đúng
/// tối đa ở mọi màn hình — kể cả cái màn chưa ai đo.
///
/// Bốn điều hàm này giữ, và cả bốn đều ở TRONG một lượt `osascript` — vì nửa
/// chừng mà huba chết thì cửa sổ của chủ máy nằm lại ở chiều lạ:
/// * nhớ CẢ HAI chiều cũ TRƯỚC khi đổi;
/// * `try` bọc đúng khúc đọc, nên lỗi đọc không cướp mất bước trả lại;
/// * trả lại trên MỌI đường ra;
/// * trả **cột trước, dòng sau** — đúng thứ tự đã đo là về lại đúng `24×80`.
pub fn screen_text_tall(window: i64, ask: usize) -> Result<String> {
    let script = format!(
        // ⚠ ĐỊA CHỈ ĐẦY ĐỦ mỗi lần, không gán `tb` rồi `contents of tb`:
        // `contents of <tham chiếu>` là toán tử giải-tham-chiếu của AppleScript,
        // nó trả về chính cái tab chứ không phải chữ trên màn (bẫy đã trả giá
        // 2026-08-16, xem chú thích ở `tabs_script`).
        r#"tell application "Terminal"
  set cr to number of rows of selected tab of window id {window}
  set cc to number of columns of selected tab of window id {window}
  set doc to ""
  try
    set number of rows of selected tab of window id {window} to {ask}
    set number of columns of selected tab of window id {window} to {ask}
    delay 1.2
    set doc to contents of selected tab of window id {window}
  end try
  set number of columns of selected tab of window id {window} to cc
  set number of rows of selected tab of window id {window} to cr
  return doc
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

/// Dấu phiên đặt để nói *"dòng dưới là lệnh, chạy hộ tôi"*.
///
/// 🔴 Hà 2026-08-24: *"có quy tắc nào để bảo claude đánh dấu vào output kết quả
/// là lệnh để bắt tôi chạy không"* — rồi chọn **chỉ dùng dấu, bỏ danh sách cho
/// phép**.
///
/// Vì sao hình dạng này, ba lý do và cả ba đo được:
/// ① `#` là dấu chú thích của shell ⟹ chủ máy copy cả khối dán vào Terminal thì
///    dòng dấu tự bị bỏ qua, lệnh vẫn chạy đúng;
/// ② dấu nằm ở DÒNG RIÊNG nên dòng lệnh không bị bẩn — nút bấm, chữ hiện ra, và
///    thứ copy được đều y hệt nhau;
/// ③ ghép được với [`join_continuations`]: lệnh nối `\` nhiều dòng vẫn đi nguyên
///    khối.
pub const RUN_MARK: &str = "#huba-run";

/// Những lệnh phiên ĐÃ ĐÁNH DẤU — không đoán theo hình dạng.
///
/// Khác [`commands_in_report`] ở đúng một điểm, và điểm ấy là cả thiết kế:
/// hàm kia hỏi *"dòng này TRÔNG giống lệnh không"* để bày một cái nút cho người
/// bấm; hàm này hỏi *"phiên có CỐ Ý bảo chạy không"* để chạy khi không ai nhìn.
/// Hai câu hỏi khác nhau thì không dùng chung một câu trả lời — đó là bài học
/// đã trả giá bằng một lỗ RCE trong bản `autorun_allows` hôm qua.
///
/// 🔴 DẤU PHẢI CHIẾM TRỌN MỘT DÒNG. `echo "#huba-run"` hay một câu văn nhắc tới
/// cái dấu đều KHÔNG kích hoạt được. Nới chỗ này là mở lại đúng cửa vừa đóng:
/// chữ trong `last_text` đến từ web, từ diff, từ tệp phiên vừa đọc.
///
/// ⚠ Cái dấu nói *"mô hình cố ý bảo chạy"*, KHÔNG nói *"chủ máy cho phép"*. Hà
/// biết và chọn mức này (2026-08-24); đừng lặng lẽ thêm một cổng nữa vào đây mà
/// không hỏi, cũng đừng lặng lẽ nới nó ra.
pub fn marked_commands(text: &str, max: usize) -> Vec<String> {
    let rows = join_continuations(text);
    let mut out: Vec<String> = Vec::new();
    let mut armed = false;
    for (raw, joined) in rows.iter() {
        let line = raw.trim();
        if line == RUN_MARK {
            armed = true;
            continue;
        }
        if !armed {
            continue;
        }
        // Dòng trống ngay sau dấu: bỏ qua, đừng tắt dấu — TUI hay chèn một
        // dòng trống giữa hai khối.
        if line.is_empty() {
            continue;
        }
        armed = false;
        // Bóc dấu nhắc/trang trí của TUI, cùng bộ với `commands_in_report` —
        // hai chỗ đọc cùng một màn thì phải bóc cùng một thứ.
        let mut cmd = line;
        for p in ["$ ", "❯ ", "> ", "⏵ ", "% ", "• ", "- ", "! ", "!"] {
            if let Some(rest) = cmd.strip_prefix(p) {
                cmd = rest.trim();
            }
        }
        let cap = if *joined {
            BTN_CMD_BLOCK_MAX
        } else {
            BTN_CMD_REPORT_MAX
        };
        if cmd.len() < 2 || cmd.len() > cap {
            logging::warn(
                "run_mark_line_rejected",
                json!({ "len": cmd.len(), "cap": cap,
                        "why": "dòng sau dấu quá ngắn hoặc quá dài" }),
            );
            continue;
        }
        out.push(cmd.to_string());
        if out.len() >= max {
            break;
        }
    }
    out
}

/// Trần cho một lệnh do NGƯỜI VIẾT nối bằng `\` — rộng hơn hẳn dòng thường.
///
/// Trần 200 của dòng thường trả lời câu *"một KHỐI không phải một lệnh"*. Câu ấy
/// vẫn đúng cho những dòng nằm cạnh nhau tình cờ. Nhưng một chuỗi nối bằng `\`
/// thì tác giả đã nói thẳng nó là MỘT lệnh — không còn gì để đoán, nên trần ở
/// đây đo cái khác: dài tới mức này thì dán vào điện thoại đã thành bức tường.
const BTN_CMD_BLOCK_MAX: usize = 700;

/// Nối những dòng mà người viết đã nối bằng `\` ở cuối dòng.
///
/// 🔴 Hà 2026-08-23, ảnh chụp buồng `[dwork · A-CHUNG]`: *"Khối lệnh chạy gom
/// bị thiếu hẳn `cd` dẫn đến chạy không đúng thư mục"*.
///
/// Nguyên văn khối trên màn:
///
/// ```text
/// cd ~/projects/dwork/dev-chung && \
/// git merge --no-edit origin/main && \
/// bash ~/projects/scripts/dci-cong-tat-ca.sh dev-chung && \
/// …
/// ```
///
/// huba gắn `▶️` vào **đúng dòng thứ hai**, vì nó mở đầu bằng `git` ∈ `KNOWN`.
/// Dòng `cd …` ở trên bị bỏ lại: `after_cd` chỉ nhận `cd X && <lệnh>` khi cả
/// hai vế nằm trên CÙNG một dòng, mà ở đây vế sau `&&` chỉ là dấu `\`. Bấm cái
/// nút ấy là `git merge` **trong thư mục khác** — và trên một cây git thật thì
/// đó không phải một lỗi hiển thị.
///
/// ⚠ ĐÂY KHÔNG PHẢI BỘ NỐI DÒNG ĐÃ GỠ 2026-08-15. Cái ấy đi đoán xem cửa sổ
/// terminal có bẻ dòng hay không — một phép đoán trên chữ đã mất thông tin, và
/// nó đã tạo ra một nút chạy lệnh triển khai THIẾU tham số. Cái này không đoán
/// gì: `\` cuối dòng là **dấu tác giả tự đặt** để nói "câu chưa hết". Nối theo
/// dấu ấy là đọc đúng thứ người ta viết, không phải dựng lại thứ đã mất.
///
/// Trả `(dòng, có_nối_không)`; cờ thứ hai để chỗ gọi nới trần đúng cho những
/// dòng đã được tác giả khai là một lệnh — xem [`BTN_CMD_BLOCK_MAX`].
fn join_continuations(screen: &str) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut acc: Option<String> = None;
    let push_part = |acc: &mut Option<String>, part: &str| match acc.as_mut() {
        Some(a) => {
            a.push(' ');
            a.push_str(part.trim());
        }
        None => *acc = Some(part.trim().to_string()),
    };
    for raw in screen.lines() {
        match raw.trim_end().strip_suffix('\\') {
            Some(head) => push_part(&mut acc, head),
            None => match acc.take() {
                // Dòng cuối của một chuỗi nối: gộp nốt rồi đóng khối.
                Some(mut a) => {
                    a.push(' ');
                    a.push_str(raw.trim());
                    out.push((a, true));
                }
                None => out.push((raw.to_string(), false)),
            },
        }
    }
    // Khối kết thúc bằng `\` treo lơ lửng (màn bị cắt, hoặc tác giả gõ thừa):
    // vẫn trả về, để nó đi qua đúng những hàng rào như mọi dòng khác thay vì
    // biến mất không dấu vết.
    if let Some(a) = acc {
        out.push((a, true));
    }
    out
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
        // 🔴 CẢ HỌ LỆNH ĐỘNG TAY VÀO TỆP — thêm 2026-08-18, và lý do là một cái
        // chặn đã bị gỡ mà cái này thì không.
        //
        // Hà, ảnh chụp `/shot` của `[AI/tfl5]`: *"Có lệnh nhưng lại không có nút
        // chạy"*. Dòng ấy là dòng cuối cùng để đóng sổ một phiên:
        // `rm ~/projects/AI/tfl5/ide/src/__tests__/deploy_domains_by_role.test.jsx`
        // — không ▶️, không 🖥, mà đường dẫn trong nó lại mọc một nút 📎 mời TẢI
        // VỀ đúng cái tệp dòng ấy bảo xoá.
        //
        // Ngày 16/08 cổng `destructive` bị gỡ hẳn (xem bia mộ ở cuối tệp) đúng
        // vì câu này: *"tôi ở tele là phải gọi lệnh thao tác như ngồi máy thì
        // chặn khác gì chặt tay"*. Nhưng `rm` chưa bao giờ có trong danh sách
        // này, và danh sách này được hỏi TRƯỚC cái cổng ấy. Nên cái chặn không
        // biến mất, nó chỉ lùi lên một tầng — và tầng này im lặng hơn hẳn tầng
        // cũ, vì nó không có tên, không có dòng log, chỉ có một `continue`.
        // Cùng hình dạng với ca `.docx` cùng ngày: **gỡ một luật mà quên gỡ
        // những thứ dựng lên để phục vụ nó.**
        //
        // 📐 Đo trước khi chọn, không kê theo trí nhớ: quét toàn bộ `huba.log` từ
        // 14/08, lấy mọi đoạn trong dấu nháy ngược qua được `looks_like_prose`
        // rồi đếm động từ KHÔNG có trong danh sách — `cat` 3 · `cp` 3 · `ps` 2 ·
        // `rm` · `lsof` · `psql`. Phần còn lại của danh sách dưới đây là cùng họ
        // với chúng (đụng tệp, đụng tiến trình), thêm một lượt để lần sau không
        // phải vá từng chữ một.
        //
        // Hàng rào vẫn giữ đúng việc của nó — chống VĂN XUÔI đội lốt lệnh, chứ
        // không phải chống lệnh nguy hiểm. Ba cửa dưới nó không đổi:
        // `looks_like_prose`, `forbids` (câu đang CẤM thì không mời chạy), và
        // "phải có ít nhất một tham số". `sudo` cố ý ĐỨNG NGOÀI: nút ▶️ chạy
        // bằng `zsh -lc` không có tty, nên một dòng `sudo` sẽ treo tới trần thời
        // gian rồi chết câm — xem [[feedback_no_tty_password_prompts]].
        "rm",
        "mv",
        "cp",
        "mkdir",
        "rmdir",
        "touch",
        "ln",
        "chmod",
        "chown",
        "cat",
        "head",
        "wc",
        "sed",
        "awk",
        "sort",
        "diff",
        "du",
        "df",
        "ps",
        "lsof",
        "kill",
        "pkill",
        "killall",
        "xargs",
        "jq",
        "tar",
        "unzip",
        "zip",
        "psql",
        "defaults",
        "which",
    ];
    let mut out: Vec<String> = Vec::new();
    let rows = join_continuations(screen);
    for (raw, joined) in rows.iter() {
        let (raw, joined) = (raw.as_str(), *joined);
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
        // này: `!<lệnh>` là **quy ước của chính huba** — nút `▶` gõ đúng hình
        // dạng ấy vào phiên để lệnh chạy TRONG phiên. Phiên học theo, viết
        // `! git -C … push origin main` trong báo cáo, và huba **không nhận ra
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
        // Dòng do tác giả nối bằng `\` được nới trần — xem [`BTN_CMD_BLOCK_MAX`].
        let hard_max = if joined { BTN_CMD_BLOCK_MAX } else { 300 };
        if line.len() < 4 || line.len() > hard_max {
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
        if line.len() > if joined { BTN_CMD_BLOCK_MAX } else { max_len } {
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
        // 🔴 BÓC LỚP BỌC RỒI MỚI HỎI HÀNG RÀO — Hà 2026-08-24, ảnh `/shot` của
        // `[tfl5]`: *"Sao ko có chạy đc lệnh"*. Dòng ấy là
        // `cd ~/projects/AI/tfl5 && nohup bash -c '…' & echo started` — 180 ký
        // tự, dưới trần 200, không dấu hiệu văn xuôi nào. Chạy thẳng
        // `commands_in_report` trên nó trả về **rỗng**.
        //
        // Vì `after_cd` giao lại `nohup bash -c '…'`, và động từ đầu là `nohup`
        // — không có trong `KNOWN`. Hàng rào cố ý HẸP nên nó đúng khi từ chối
        // `nohup`; cái sai là hỏi nó về từ SAI. `nohup` không chạy gì cả, nó
        // chỉ bọc quanh lệnh thật, y hệt `cd X &&` ở dòng trên.
        //
        // Nên đi đúng đường `after_cd` đã mở: **không nới hàng rào, chỉ hỏi
        // đúng chỗ**. Nhét `nohup` vào `KNOWN` thì mọi dòng mở đầu bằng nó lọt
        // qua mà lệnh thật bên trong chưa ai nhìn.
        //
        // ⚠ `sudo` KHÔNG nằm ở đây, và đó là chủ ý cũ giữ nguyên (xem chú thích
        // của nó bên trên): nó đổi QUYỀN chứ không chỉ đổi cách chạy.
        //
        // Hậu quả thứ hai của cùng cái lỗi, cũng thấy trong ảnh: `upgrade.sh`
        // trong dòng ấy mọc một nút 📎 mời TẢI VỀ. `paths_not_in_commands` chỉ
        // chừa đường dẫn nào NẰM TRONG một lệnh đã nhận ra — lệnh không được
        // nhận thì đường dẫn của nó thành tệp rời. Vá chỗ này là vá cả hai.
        const WRAPPERS: &[&str] = &["nohup", "env", "time", "stdbuf", "caffeinate", "nice"];
        let verb = if WRAPPERS.contains(&verb) {
            let rest = after_cd(line).unwrap_or(line);
            rest.split_whitespace()
                .skip(1)
                // Bỏ qua thứ thuộc về CHÍNH lớp bọc, ba dạng đã gặp:
                // cờ (`env -i`), GIÁ TRỊ của cờ (`nice -n 10` — `10` không mở
                // đầu bằng `-` nên vòng lọc đầu tiên dừng lại ở nó, đo được
                // bằng bài kiểm), và gán biến môi trường (`env FOO=bar cmd`).
                .find(|w| {
                    !w.starts_with('-')
                        && !w.contains('=')
                        && !w.chars().all(|c| c.is_ascii_digit())
                })
                .unwrap_or(verb)
        } else {
            verb
        };
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
        // 🪦 CỔNG `destructive` GỠ 2026-08-16 — Hà: *"đã qua huba thì đừng có
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
        // đúng cửa này là chỗ dòng trang trí của huba lọt ra shell.
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

/// Đuôi file huba CHẮC CHẮN không gửi — thứ cổng quét rò không đọc nổi.
///
/// 🔴 Đây từng là một danh sách TRẮNG, và nó sai ngay trong lần dùng đầu tiên
/// (2026-08-13): tôi mời Hà bấm thử vào `huba.env.example`, đuôi `.example`
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
    // Danh sách này sinh ra hồi huba chỉ gửi được tệp chữ, nên nó vô tình đúng
    // bằng "những gì đọc thành UTF-8 được". Cửa ấy đã bỏ (`telegram::document_body`),
    // và thứ rơi ra ngoài chính là **bản in** — `.docx` đúng chuẩn văn bản hành
    // chính, `.xlsx` bảng số, `.pdf` bản gửi đi. Tức mỗi phiên làm xong việc thì
    // huba gửi được bản nháp mà không gửi được bản thành phẩm.
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
/// được file để mở trực tiếp trên tele"*. Trước đó cây cầu này một chiều: huba
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
/// câu ấy đo bằng HÌNH DẠNG một thứ chỉ trả lời được bằng ĐĨA. Nay huba biết thư
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
/// bị dính text ngoài như này"*. Thứ huba gõ vào phiên là:
///
/// ```text
/// ! ▶ Lệnh thấy trên màn (bấm nút dưới để gõ `!` vào chính phiên): • git -C … push origin main
/// (eval):1: no matches found: (bấm nút dưới để gõ  vào chính phiên):
/// ```
///
/// Tức **huba đọc lại chính dòng trang trí của nó** rồi biến thành lệnh. Cú push
/// không hề chạy, mà nhìn thì như đã bấm.
///
/// Hai lỗ cùng lúc, và cái thứ hai mới đáng sợ:
/// * `/shot` tự đính dòng *"▶ Lệnh thấy trên màn…"* vào bản trả lời, rồi lượt
///   quét sau đọc luôn cả dòng ấy — **một vòng tự ăn chính mình**.
/// * Lượt quét trong DẤU NHÁY thiếu sạch các cửa lọc câu văn mà lượt quét theo
///   DÒNG đã có từ lâu (`" ("`, `", "`, dấu câu cuối). Hai lượt quét, hai bộ
///   luật khác nhau, và không ai nhìn thấy sự lệch cho tới khi nó gõ ra shell.
///
/// Nay một bộ luật, dùng cho cả hai lượt — kể cả một cửa nhận ra CHÍNH chữ huba
/// in ra màn.
fn looks_like_prose(s: &str) -> bool {
    // Câu văn thường mang mệnh đề trong ngoặc hoặc dấu phẩy; dòng lệnh thật thì
    // hiếm khi có. Thà bỏ sót một nút còn hơn dựng một cái nút chạy nhầm thứ.
    s.contains(" (")
        || s.contains(", ")
        || s.ends_with('.')
        || s.ends_with(':')
        || s.ends_with('?')
        // …và chữ của CHÍNH huba trên màn thì tuyệt đối không phải lệnh.
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
/// dấu nháy**. huba đọc màn, thấy hình dạng một lệnh, và gửi cho Hà ba cái nút —
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
        // trần: *"Nút lệnh chạy ko đúng"*. Chữ ấy huba bắt được từ một THÔNG BÁO
        // CHẶN của hook — *"the command runs … which permanently deletes tracked
        // source files … Safer form: …"*. Cả đoạn là lời CẤM một lệnh, và huba
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
// Ngồi ở máy anh gõ `rm` không ai hỏi câu nào; huba từ chối dựng nút cho đúng
// dòng ấy nên nó tự tay tạo ra một khoảng cách — rồi im lặng về việc đó, nên
// từ điện thoại nhìn ra y hệt "huba không đọc được lệnh".
//
// Cái rào THẬT của dự án không nằm ở đây và không đổi:
// `sessions::DENIED_TOOLS` gác thứ một PHIÊN TỰ CHẠY được phép làm (luật 1).
// Gác một cỗ máy chạy không người trông là một chuyện; gác bàn tay chủ máy là
// một chuyện khác, và tệp này đã lẫn hai chuyện ấy suốt hai ngày.

/// Nối lại một lệnh bị MÀN HÌNH bẻ dòng — hoặc từ chối, nếu không chắc.
///
/// 🔴 Hà 2026-08-13, ảnh chụp Telegram: *"Không có lệnh merge mà bấm"*. Màn của
/// phiên tfl5 lúc 11:15 kết bằng đúng một dòng lệnh để gõ, và huba không dựng nổi
/// một cái nút nào cho nó. Lấy nguyên chữ huba đã gửi ra khỏi nhật ký thì thấy
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
/// nó chỉ khai `Enter to select · ↑/↓ to navigate`. Đo thật hôm ấy: huba bấm
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
/// phím rời khỏi huba, còn "3/5 ô đã chọn" mới là thứ người ở xa cần biết — và
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
/// đổi, dạng duy nhất được coi là lành. Từ hai trở lên = huba vừa lật hộ chủ máy
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
        // 🔴 ĐẾM KÝ TỰ, KHÔNG ĐẾM BYTE — Hà 2026-08-25. `str::len()` trả byte,
        // mà chữ Việt có dấu tốn 2-3 byte mỗi ký tự: cùng một trần 120, tiếng
        // Anh được ~120 ký tự còn tiếng Việt chỉ ~50-60. Cái trần này vì thế
        // cắt đúng thứ tiếng mà phiên của chủ máy viết ra.
        if label.is_empty() || label.chars().count() > 120 {
            continue;
        }
        out.push((n, label.to_string(), idx));
    }
    // Số phải liên tiếp từ 1: "3. xong" trong một đoạn văn không phải hộp chọn.
    //
    // 🔴 …trừ khi hộp DÀI HƠN MÀN — Hà 2026-08-19, ảnh `/shot` phiên `[tcc/amm]`:
    // *"Nội dung màn của phiên như thế này thì làm được gì, đọc không hiểu
    // luôn"*. Đo trên chính màn ấy: sáu lựa chọn, mỗi cái kèm bốn dòng mô tả ⟹
    // **dòng số của lựa chọn 1 đã cuộn lên khỏi mép trên**, màn bắt đầu bằng
    // phần đuôi mô tả của nó rồi mới tới `2.`. Luật "bắt đầu từ 1" vì thế trả về
    // RỖNG — không một cái nút nào — nên tin ấy chỉ còn là một khối chữ vỡ dòng
    // mà không bấm được gì. Đúng chữ Hà dùng: *làm được gì*.
    //
    // Cửa an toàn không đổi, nó chỉ chuyển vai: có DÒNG CHÂN của hộp thì đây
    // chắc chắn là hộp (`has_chooser_footer`), và số đọc được là số CLI tự đánh
    // — bấm `2` vẫn tới đúng lựa chọn 2 dù `1.` không còn trên màn. Không có
    // dòng chân thì giữ nguyên luật cũ, vì lúc ấy "bắt đầu từ 1" là thứ duy nhất
    // ngăn một đoạn văn đánh số bị đọc thành hộp chọn.
    // 🔴 DÒNG CHÂN LÀ CỬA, cho MỌI hộp — sửa 2026-08-21 sau ảnh `/shot` của
    // `[tfl5]`: huba gắn ☑ vào ba dòng `1.` `2.` `3.` của một ĐOẠN VĂN, rồi khi
    // Hà bấm thì báo *"đã gửi '2' mà bảng vẫn còn nguyên 2 lựa chọn"* và tự đoán
    // *"hộp này có thể không nhận phím số"*. Không phải hộp nào không nhận phím
    // — không có hộp nào cả.
    //
    // Luật cũ đòi dòng chân CHỈ khi số không bắt đầu từ 1, nên một đoạn văn
    // liệt kê ba việc đi thẳng qua cửa. Mà "bắt đầu từ 1" không phân biệt được
    // hộp chọn với văn xuôi — chỉ dòng chân mới là thứ duy nhất CLI vẽ ra và
    // văn xuôi không thể có.
    //
    // Đánh đổi, nói thẳng: một hộp thật mà dòng chân bị mép màn cắt sẽ mất nút.
    // Chấp nhận, vì hướng hỏng của hai bên không cân nhau — mất nút thì `/shot`
    // vẫn đọc được và `/key` vẫn gõ được; còn gắn nút vào văn xuôi thì một con
    // số rơi vào màn KHÔNG có hộp chọn, và nó có thể đi làm một lượt chat trong
    // phiên của chủ máy. Vả lại dòng chân nằm ở ĐÁY hộp, tức phần sống sót lâu
    // nhất khi màn cuộn, và nay còn có cửa nới-hết-cỡ + cuộn đứng sau.
    // Con trỏ `❯` đứng ngay trước một dòng đánh số: dấu hiệu thứ hai mà CLI vẽ
    // ra và văn xuôi không có. Cần nó vì dòng chân nằm dưới đáy hộp nên hay bị
    // mép màn cắt — đo trên log thật: trong 214 màn có dòng đánh số mà THIẾU
    // dòng chân, **21** vẫn có con trỏ (hộp thật bị cắt) và **193** thì không
    // (văn xuôi). Đòi mỗi dòng chân là giết 21 hộp thật ấy.
    let co_tro = screen.lines().any(|l| {
        let t = l.trim_start();
        t.strip_prefix('❯')
            .map(str::trim_start)
            .and_then(|r| r.split_once('.'))
            .is_some_and(|(n, rest)| {
                n.trim()
                    .parse::<usize>()
                    .is_ok_and(|n| (1..=9).contains(&n))
                    && !rest.trim().is_empty()
            })
    });
    let chan = chooser_footer_line(screen);
    let footer = chan.is_some();
    if out.is_empty() || (!footer && (!co_tro || out[0].0 != 1)) {
        return Vec::new();
    }
    // 🔴 CÓ DÒNG CHÂN ⟹ NEO VÀO NÓ, ĐỪNG CHẤM CẢ MÀN — Hà 2026-08-25, ảnh
    // `/shot` phiên `[dwork]`: *"Có option nhưng không có chọn được"*.
    //
    // Màn ấy có hộp 5 lựa chọn + dòng chân đầy đủ, mà hàm này trả về **0**. Đo
    // bằng byte trên chính màn ấy (lưu ở `tests/fixtures/`): nửa trên màn là
    // văn xuôi của phiên, có ba dòng đánh số — 153, **115**, 185 byte. Trần 120
    // loại được cái thứ nhất và thứ ba; cái **115 byte lọt**, nên `out[0]` là
    // một dòng VĂN XUÔI mang số 2, `first = 2`, và phép kiểm liên tiếp ngay
    // dưới gãy ở phần tử sau (1 ≠ 3) ⟹ vứt sạch cả hộp thật.
    //
    // Tức cái trần độ dài chưa bao giờ là một cái CỔNG — nó là xổ số: hai dòng
    // văn xuôi bị loại nhờ may, dòng thứ ba sống sót và đầu độc cả danh sách.
    // Sửa cái trần cũng không cứu được, vì bất kỳ trần nào cũng có dòng lọt.
    //
    // Hộp chọn là cụm dòng đánh số NGAY TRÊN dòng chân — chính CLI vẽ ra cả
    // hai. Nên: bỏ mọi mục nằm dưới dòng chân, rồi lấy ĐUÔI liên tiếp dài nhất
    // đếm ngược lên. Văn xuôi ở nửa trên màn từ đó không với tới được về mặt
    // CẤU TRÚC, chứ không phải nhờ đoán theo độ dài.
    //
    // Không có dòng chân thì mọi hàng rào cũ giữ nguyên (con trỏ `❯` + bắt đầu
    // từ 1 + liền dòng) — đó là thứ duy nhất ngăn một đoạn văn đánh số bị đọc
    // thành hộp chọn, và nó đã trả giá hai lần (11/08 và 21/08).
    if let Some(chan) = chan {
        out.retain(|(_, _, idx)| *idx < chan);
        if out.is_empty() {
            return Vec::new();
        }
        let mut dau = out.len() - 1;
        while dau > 0 && out[dau - 1].0 + 1 == out[dau].0 {
            dau -= 1;
        }
        out.drain(..dau);
    }
    let first = out[0].0;
    for (i, (n, _, _)) in out.iter().enumerate() {
        if *n != first + i {
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
    // chọn, huba bắn `⚠ dừng lại HỎI — cần bạn chọn` kèm nguyên văn ba dòng ấy
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
    if !footer {
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
    // là **con trỏ đang trỏ vào một lựa chọn** (`❯ 1. Set it up`). huba đọc đó
    // thành "chữ trong ô nhập", dựng nút `⏎ Gửi`, và một cú Enter lúc màn đang
    // mở hộp chọn thì **XÁC NHẬN lựa chọn số 1** chứ không gửi gì (luật 13).
    // Tức cái nút ấy mời chủ máy bật auto mode mà tưởng mình đang gửi một câu.
    //
    // Đo TỪNG DÒNG chứ không đo cả màn: dòng chân thật là MỘT dòng, còn hai
    // mảnh rời nằm cách nhau hai mươi dòng văn xuôi thì chỉ là trùng chữ.
    chooser_footer_line(screen).is_some()
}

/// Dòng chân ấy nằm ở DÒNG nào — `parse_choices` cần VỊ TRÍ, không chỉ có/không.
///
/// Tách ra để hai câu hỏi (*"màn có hộp chọn không"* và *"hộp ấy kết thúc ở
/// đâu"*) đọc CHUNG một phép đo. Hai bản chép của cùng một phép so chuỗi là hai
/// câu trả lời lệch nhau — đúng cái bệnh mà chính hàm này đã mắc một lần
/// (16/08: `has_chooser_footer` nói *không* trong khi `parse_choices` trên cùng
/// màn ấy đọc ra ba lựa chọn).
///
/// Lấy dòng chân CUỐI CÙNG. Một màn có thể còn mang đoạn hội thoại cũ kèm dòng
/// chân đã cuộn qua; hộp đang MỞ là cái ở dưới cùng.
///
/// (`lines()` không phải `ExactSizeIterator` nên không có `rposition` — duyệt
/// xuôi rồi lấy `last` là cùng một thứ, và không phải gom cả màn vào một `Vec`.)
pub fn chooser_footer_line(screen: &str) -> Option<usize> {
    screen
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let l = line.to_lowercase();
            (l.contains("to select") || l.contains("to confirm") || l.contains("để chọn"))
                && (l.contains("to navigate") || l.contains("to cancel") || l.contains("để huỷ"))
        })
        .map(|(i, _)| i)
        .last()
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

/// Bản đọc HẸP này có dấu hiệu THIẾU tab không.
///
/// 🔴 Cùng một cái bảng, huba đọc ra hai con số khác nhau tuỳ đường đi — và
/// đường dựng NÚT nhìn rộng hơn đường THI HÀNH cú bấm:
/// * `/shot` nới cửa sổ hết cỡ khi màn bị mép cắt rồi mới đọc
///   (`pipeline.rs`, nhật ký `shot_grew_window`), và nút tab dựng từ **bản rộng**
///   ấy;
/// * `/tab` với `/pick` đọc bằng [`look`] → [`screen_text`], **không nới lần
///   nào**.
///
/// Nên cái nút `tab 3` đi ra điện thoại từ một bản đọc thấy 3 tab, còn cú bấm
/// vào chính nó lại chấm trên một bản đọc chỉ thấy 2 — và câu trả lời là *"bảng
/// chỉ có 2 câu, không có câu 3"* về đúng cái nút huba vừa tự dựng ra. Cửa sổ
/// hẹp còn một dạng tệ hơn: thanh tab bị BẺ DÒNG thì `←` và `→` thôi chung một
/// dòng, [`ask_table`] trả `None`, và một bảng nhiều câu đọc thành bảng một câu.
///
/// `want` là con số đến từ nguồn KHÁC màn — sổ phiên, hoặc chính số tab chủ máy
/// vừa bấm. Con số thứ hai không phải số bịa: cái nút ấy do huba dựng.
pub fn tab_bar_cut(near: Option<&AskTable>, want: usize) -> bool {
    match near {
        Some(t) => t.answered.len() < want,
        // Không thấy thanh tab nào mà nguồn khác nói bảng có từ 2 câu ⟹ nhiều
        // khả năng thanh tab bị bẻ dòng. Bảng MỘT câu thì đúng là không vẽ
        // thanh tab, nên đừng lấy nó làm cớ đụng vào cửa sổ chủ máy.
        None => want >= 2,
    }
}

/// Hai lần đọc cùng một thanh tab: bản RỘNG chỉ thắng khi nó thật sự thấy
/// nhiều tab hơn.
///
/// "Rộng hơn" không tự động là "đúng hơn": nới xong mà TUI chưa vẽ lại kịp thì
/// bản rộng đọc ra ÍT hơn, và lấy nó là đổi một bản đọc đủ lấy một bản đọc cụt.
/// Cùng đúng luật `shot_grew_window` đang giữ — chỉ nhận bản rộng khi nó THẬT
/// SỰ hơn.
pub fn wider_table(near: Option<AskTable>, far: Option<AskTable>) -> (Option<AskTable>, bool) {
    let n = near.as_ref().map(|t| t.answered.len()).unwrap_or(0);
    let f = far.as_ref().map(|t| t.answered.len()).unwrap_or(0);
    if f > n {
        (far, true)
    } else {
        (near, false)
    }
}

/// Thanh tab đọc từ chữ ĐÃ CÓ; thiếu tab so với `want` thì NỚI CỬA SỔ đọc lại.
///
/// Trả `(bảng, chữ của bản đọc rộng nếu đã nhận nó)` — chỗ gọi cần cả chữ, vì
/// [`cursor_on`] phải chấm trên đúng cái màn mà bảng vừa đọc ra: đếm ô trống
/// trên bản rộng rồi tìm con trỏ trên bản hẹp là hỏi hai cái màn khác nhau.
///
/// Nới là đụng vào cửa sổ chủ máy và tốn thêm ~1,5 giây, nên chỉ nới khi
/// [`tab_bar_cut`] nói có dấu hiệu thiếu — không nới phòng xa.
pub fn ask_table_wide(body: &str, window: i64, want: usize) -> (Option<AskTable>, Option<String>) {
    let near = ask_table(body);
    if !tab_bar_cut(near.as_ref(), want) {
        return (near, None);
    }
    let rong = match screen_text_tall(window, GROW_ASK) {
        Ok(r) if !r.trim().is_empty() => r,
        Ok(_) => {
            logging::warn(
                "tab_bar_grow_empty",
                json!({ "window": window, "want": want,
                        "effect": "nới cửa sổ xong đọc ra rỗng — chấm trên bản đọc hẹp" }),
            );
            return (near, None);
        }
        Err(e) => {
            logging::warn(
                "tab_bar_grow_failed",
                json!({ "window": window, "want": want, "err": logging::err_chain(&e),
                        "effect": "không nới được cửa sổ — chấm trên bản đọc hẹp, có thể thiếu tab" }),
            );
            return (near, None);
        }
    };
    let far = ask_table(&rong);
    let (before, after) = (
        near.as_ref().map(|t| t.answered.len()).unwrap_or(0),
        far.as_ref().map(|t| t.answered.len()).unwrap_or(0),
    );
    let (table, lay_rong) = wider_table(near, far);
    logging::info(
        "tab_bar_regrown",
        json!({ "window": window, "xin": GROW_ASK, "want": want,
                "tabs_before": before, "tabs_after": after, "taken": lay_rong }),
    );
    if lay_rong {
        (table, Some(rong))
    } else {
        (table, None)
    }
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
/// văn, từ ảnh màn huba gửi đi 18/08). Đọc bằng `is_busy` thì phiên ấy ra "rảnh",
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
/// chọn"* rồi GỬI. Tức đúng lúc huba mù nhất là lúc nó dám tay nhất — mà chú
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
    // ra nó. Nó nghĩa là *"màn có dấu hiệu bí mật nên huba giữ chữ lại, chỉ giữ
    // con số lựa chọn"*, và lý lẽ ấy đúng hồi `/shot` cũng quét rò. Nay `/shot`
    // gửi nguyên màn lên Telegram (gỡ 14/08), nên nhánh này giấu với huba đúng
    // thứ huba vừa công bố — và cái giá là `/pick` từ chối một cú bấm hợp lệ
    // bằng câu *"không đọc được chữ"* về một màn chủ máy đang nhìn tận mắt.
    //
    // Để lại một nhánh không ai sinh ra được thì tệ hơn xoá: người đọc sau sẽ
    // tin rằng huba còn xử lý riêng màn có bí mật. Cùng bài học `portal.rs` để
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
/// thứ quyết định huba có dám gõ hay không, nên hai bản chép là hai bản sẽ lệch.
pub fn look_from_screen(screen: &str, lines: usize) -> Look {
    let choices = parse_choices(screen);
    // 🔴 THÔI GIỮ CHỮ LẠI VỚI CHÍNH MÌNH — 2026-08-16.
    //
    // Hà, phân biệt hai loại việc: *"lệnh ở đây là lệnh bash chứ không phải
    // route của huba, route get file là yc huba gửi file lên tele thì không liên
    // quan gì tới cli cả"*. Cùng ý ấy soi vào đây thì lộ ra một chỗ vô lý:
    //
    // `/shot` gửi NGUYÊN màn ấy lên Telegram, không quét gì cả (cổng quét rò gỡ
    // ngày 14/08, cùng câu *"huba là cổng làm việc của tôi mà"*). Còn hàm này
    // vẫn giấu ĐÚNG CÁI MÀN ẤY với chính huba, nên `/pick` trả lời *"màn có dấu
    // hiệu bí mật nên huba không đọc được chữ"* về một màn chủ máy vừa nhìn tận
    // mắt trên điện thoại. Giấu một thứ đã công bố thì không bảo vệ được gì —
    // nó chỉ làm huba mù đúng lúc cần thấy nhất, rồi từ chối một cú bấm hợp lệ.
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
/// ý"*. Đây là mảnh còn thiếu của một chuyện huba ĐÃ nhận ra mà chưa làm gì:
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
mod numbered_prose_tests {
    use super::parse_choices;

    /// Một DANH SÁCH ĐÁNH SỐ trong câu trả lời của phiên KHÔNG phải hộp chọn.
    ///
    /// 🔴 Hà 2026-08-21, ảnh `/shot` phiên `[tfl5]`: huba gắn ☑ vào ba dòng
    /// `1.` `2.` `3.` của một đoạn văn, rồi khi Hà bấm thì báo *"đã gửi '2' mà
    /// bảng vẫn còn nguyên 2 lựa chọn"* và tự đoán *"hộp này có thể không nhận
    /// phím số"*. Không phải hộp nào không nhận phím — **không có hộp nào cả**.
    ///
    /// Luật cũ chỉ đòi "số liên tiếp bắt đầu từ 1", mà một đoạn văn liệt kê ba
    /// việc thì thoả đúng điều đó. Cửa thật phải là DÒNG CHÂN của hộp chọn: nó
    /// là thứ duy nhất chỉ CLI mới vẽ ra.
    ///
    /// Hậu quả không chỉ là một cái nút thừa: bấm nó gửi một con số vào phiên,
    /// và một con số rơi vào màn không có hộp chọn có thể đi làm một lượt chat.
    #[test]
    fn danh_sach_danh_so_trong_van_xuoi_khong_phai_hop_chon() {
        let man = "  Ba việc còn chờ Hà, đều không phải việc mã:\n\
                   \x20 1. Secret ba cổng thanh toán (B4) — không mã nào thay được.\n\
                   \x20 2. Ngày admin.js thôi là đường chính thức.\n\
                   \x20 3. Tenant tự mint token — đã mở khoá.\n\
                   \x20 ⏵⏵ auto mode on · 2 shells · ← 2 agents · ↓ to manage\n";
        assert!(
            parse_choices(man).is_empty(),
            "đoạn văn đánh số bị đọc thành hộp chọn: {:?}",
            parse_choices(man)
        );
    }

    /// …và cửa mới KHÔNG được giết hộp chọn thật: có dòng chân thì vẫn đọc ra đủ.
    #[test]
    fn hop_chon_that_van_doc_duoc() {
        let man = "❯ 1. Vá ACL trước\n\
                   \x20 2. Đăng nhập lại\n\
                   \x20 Enter to select · ↑/↓ to navigate · Esc to cancel\n";
        let c = parse_choices(man);
        assert_eq!(c.len(), 2, "hộp thật phải đọc ra 2 lựa chọn, đọc ra: {c:?}");
    }
}

#[cfg(test)]
mod merge_above_tests {
    use super::merge_above;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    /// Hai khung liên tiếp CHỒNG nhau — đó là lý do [`SCROLL_STEP`] nhỏ hơn
    /// chiều cao khung. Ghép phải nối đúng chỗ chồng, không chép lại nó.
    #[test]
    fn cho_chong_thi_noi_dung_cho_khong_chep_lai() {
        let cu = v(&["một", "hai", "ba", "bốn"]);
        let dang_co = v(&["ba", "bốn", "năm"]);
        assert_eq!(
            merge_above(&cu, &dang_co),
            v(&["một", "hai", "ba", "bốn", "năm"])
        );
    }

    /// TUI vẽ lại có thể đổi phần đệm bên phải giữa hai lượt. Một dấu cách thừa
    /// KHÔNG được biến hai bản sao của cùng một dòng thành hai dòng khác nhau —
    /// nếu không, mỗi lượt cuộn lại nhân đôi một khung.
    #[test]
    fn khoang_trang_cuoi_dong_khong_pha_cho_ghep() {
        let cu = v(&["một", "hai  ", "ba   "]);
        let dang_co = v(&["hai", "ba", "bốn"]);
        assert_eq!(merge_above(&cu, &dang_co), v(&["một", "hai", "ba", "bốn"]));
    }

    /// Không tìm ra chỗ chồng (cuộn quá nhanh, hoặc màn vừa đổi hẳn) thì nối
    /// thẳng: thà thừa một khung còn hơn NUỐT một đoạn — mất chữ là thứ người
    /// đọc không thể tự phát hiện.
    #[test]
    fn khong_co_cho_chong_thi_noi_thang_chu_khong_nuot() {
        let cu = v(&["một", "hai"]);
        let dang_co = v(&["chín", "mười"]);
        assert_eq!(
            merge_above(&cu, &dang_co),
            v(&["một", "hai", "chín", "mười"])
        );
    }

    /// Khung cũ nằm TRỌN trong phần đã có (cuộn không ra thêm gì) ⟹ không thêm
    /// dòng nào. Đây chính là điều kiện dừng `dry` của `screen_scrollback`: nếu
    /// phép ghép ở đây trả về dài hơn, vòng cuộn sẽ không bao giờ tự dừng.
    #[test]
    fn khung_khong_moi_thi_khong_dai_them() {
        let dang_co = v(&["một", "hai", "ba"]);
        let cu = v(&["một", "hai"]);
        assert_eq!(merge_above(&cu, &dang_co), dang_co);
    }
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
        // chữ không còn bị giữ lại với chính huba (xem bia mộ `Look::Withheld`).
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

    /// 🔴 CÙNG MỘT BẢNG, HAI CON SỐ — và cái nút đi ra điện thoại từ bản đọc
    /// RỘNG còn cú bấm vào nó chấm trên bản đọc HẸP.
    ///
    /// `/shot` nới cửa sổ khi màn bị mép cắt rồi dựng nút tab từ bản rộng ấy
    /// (`pipeline.rs`, `shot_grew_window`); `/tab` và `/pick` thì đọc bằng
    /// `look` → `screen_text`, không nới. Nên huba có thể trả lời *"bảng chỉ có
    /// 2 câu, không có câu 3"* về đúng cái nút chính nó vừa dựng.
    #[test]
    fn doc_ra_it_tab_hon_so_la_dau_hieu_bi_cat() {
        let t = super::ask_table(REAL_TAB_BAR).expect("thanh tab thật phải đọc được");
        assert_eq!(t.answered.len(), 2, "bản đọc hẹp thấy 2 tab");
        // Nguồn khác màn nói 3 câu ⟹ nới cửa sổ nhìn lại trước khi từ chối.
        assert!(super::tab_bar_cut(Some(&t), 3));
        // Khớp rồi thì thôi: nới là đụng vào cửa sổ chủ máy, không làm phòng xa.
        assert!(!super::tab_bar_cut(Some(&t), 2));
        assert!(!super::tab_bar_cut(Some(&t), 0));
    }

    /// Cửa sổ hẹp BẺ ĐÔI thanh tab ⟹ `ask_table` trả `None`, tức bảng nhiều câu
    /// đọc thành bảng một câu. Đây là dạng cắt tệ hơn dạng thiếu vài tab: nó
    /// không thiếu một phần, nó mất hẳn cả cái bảng.
    #[test]
    fn thanh_tab_bi_be_dong_thi_khong_doc_ra_bang_nao() {
        let be = "←  ☒ Vá ACL  ☐ Đăng nhập\n  ☐ RPC pool  ✔ Submit  →";
        assert!(
            super::ask_table(be).is_none(),
            "`←` và `→` khác dòng ⟹ không phải một thanh tab đọc được"
        );
        assert!(
            super::tab_bar_cut(None, 3),
            "sổ nói 3 câu mà không thấy tab nào ⟹ nới rồi nhìn lại"
        );
        // Bảng MỘT câu thì đúng là không có thanh tab — không phải cớ để nới.
        assert!(!super::tab_bar_cut(None, 1));
        assert!(!super::tab_bar_cut(None, 0));
    }

    /// Bản rộng chỉ THẮNG khi nó thật sự thấy nhiều tab hơn — nới xong mà TUI
    /// chưa vẽ lại kịp thì giữ nguyên bản hẹp, đừng đổi một bản đọc đủ lấy một
    /// bản đọc cụt.
    #[test]
    fn ban_rong_chi_thang_khi_that_su_hon() {
        let hep = super::ask_table(REAL_TAB_BAR);
        let rong = super::ask_table("←  ☒ Vá ACL  ☐ Đăng nhập  ☐ RPC pool  ✔ Submit  →");
        assert_eq!(rong.as_ref().unwrap().answered.len(), 3);

        let (lay, doi) = super::wider_table(hep.clone(), rong.clone());
        assert!(doi, "2 → 3 tab thì phải nhận bản rộng");
        assert_eq!(lay.unwrap().answered.len(), 3);

        let (lay, doi) = super::wider_table(rong.clone(), hep.clone());
        assert!(!doi, "bản rộng đọc ra ÍT hơn ⟹ giữ bản cũ");
        assert_eq!(lay.unwrap().answered.len(), 3);

        let (lay, doi) = super::wider_table(hep.clone(), None);
        assert!(!doi, "nới xong không thấy bảng nào ⟹ giữ bản cũ");
        assert_eq!(lay.unwrap().answered.len(), 2);

        // Thanh tab bị bẻ dòng ở bản hẹp, nới ra thì đọc được: đúng ca cứu được.
        let (lay, doi) = super::wider_table(None, rong);
        assert!(doi);
        assert_eq!(lay.unwrap().answered.len(), 3);

        assert_eq!(super::wider_table(None, None), (None, false));
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
        // sau: huba đọc thành hộp chọn rồi bắn `⚠ dừng lại HỎI — cần bạn chọn`
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
    /// hay nó ở tab con"*. huba trả lời *"không còn cửa sổ terminal nào chạy
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
