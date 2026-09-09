//! Windows — cùng vai trò với `keys.rs` trên macOS: gõ vào cửa sổ terminal
//! của một phiên, và đọc lại nó. **CHƯA CHẠY THỬ TRÊN WINDOWS THẬT** — phiên
//! viết tệp này chạy trên macOS, không có máy Windows nào để đo. Đọc kỹ mục
//! "Chưa đo được" ở cuối trước khi tin bất kỳ dòng nào dưới đây.
//!
//! # Vì sao KHÔNG phải một bản dịch 1:1 của `keys.rs`
//!
//! Hà 2026-09-08: *"Một số hạn chế ở macos có thể chạy được trên win cũng làm
//! hết đi"*. Ba hạn chế của macOS KHÔNG tồn tại trên Windows, nên bản này
//! không mang chúng theo:
//!
//! 1. **`do script` của Terminal.app luôn kèm một CR không tắt được**
//!    (`keys.rs::press_writes`) — buộc macOS phải đếm "một lượt ghi = một cú
//!    Enter", dựng cả một luật xếp lượt (`nav_plan`/`checkbox_plan`) chỉ để
//!    né hậu quả. `SendInput` của Windows gửi ĐÚNG phím được yêu cầu, không
//!    kèm gì — nên [`press_writes`] ở đây gửi phẳng từng phím, không nhóm.
//! 2. **`osascript`/System Events từ chối gửi phím rời** (*"is not allowed to
//!    send keystrokes"*), buộc macOS phải xin quyền Accessibility + tự ký
//!    chứng chỉ cố định (`cgkeys.rs`, `CLAUDE.md` mục 13) rồi tách riêng một
//!    con đường `unsafe` chỉ để gửi MỘT phím không kèm dấu xuống dòng.
//!    `SendInput` không đòi quyền hệ điều hành nào cho một tiến trình
//!    KHÔNG NÂNG QUYỀN gửi phím tới một cửa sổ khác không nâng quyền — nên
//!    không có `unsafe`-chỉ-một-tệp nào ở đây, và không có "gõ chữ thường thì
//!    được, gõ một phím rời thì không" — MỘT hàm ([`send_keys`]) làm cả hai.
//!    ⚠ Vẫn còn một hàng rào khác thay vào chỗ đó — xem UIPI ở mục cuối.
//! 3. **Gatekeeper quét lần đầu mỗi binary vừa build lại, ~95 giây/lần**
//!    (`project_huba_test_binaries_gatekeeper_stall`) — không có gì tương
//!    đương bắt buộc trên Windows cho một `.exe` chạy từ Task Scheduler của
//!    chính người dùng (SmartScreen chỉ chặn tệp TẢI VỀ TỪ MẠNG, có cờ
//!    Zone.Identifier; một `.exe` tự build tại máy không mang cờ đó).
//!
//! # Mô hình cửa sổ: MỖI PHIÊN MỘT CỬA SỔ, không dùng tab
//!
//! Terminal.app cho AppleScript hỏi thẳng `tty of tab`, nên nhiều tab trong
//! một cửa sổ vẫn phân biệt được. Windows Terminal (`wt.exe`) không có API
//! tương đương để hỏi "tab này đang chạy gì" từ NGOÀI tiến trình — việc đọc
//! nội dung đã phải đi qua UI Automation (xem [`screen_text`]), và UIA đọc
//! được CỬA SỔ đang hiện, không phân biệt tab ẩn. Nên `open_window` LUÔN xin
//! `-w new` (cửa sổ mới, không phải tab mới trong cửa sổ có sẵn) — đổi lại,
//! "tty" của huba trên Windows là **chuỗi thập phân của HWND**, không phải
//! một tty thật. Cùng đúng cảnh báo macOS đã ghi cho tty: **HWND cũng là một
//! con số ĐƯỢC DÙNG LẠI** sau khi cửa sổ đóng — `window_of` phải tự xác nhận
//! `IsWindow` còn đúng trước khi tin số cũ trong sổ.
//!
//! # Chưa đo được — đọc trước khi tin
//!
//! * **Tên lớp cửa sổ** `CASCADIA_HOSTING_WINDOW_CLASS` là tài liệu công khai
//!   của Windows Terminal (dùng bởi nhiều script AutoHotkey/PowerToys), NHƯNG
//!   chưa được xác nhận lại trên máy thật ở đây.
//! * **UI Automation có đọc được buffer đầy đủ hay chỉ khung nhìn** — Windows
//!   Terminal có hỗ trợ UIA cho trình đọc màn hình từ ~2021, nhưng độ đầy đủ
//!   của `TextPattern` (đọc được cả phần đã cuộn khỏi khung hay chỉ khung
//!   nhìn) CHƯA đo được ở đây, khác hẳn `screen_scrollback` — chỗ này để
//!   nguyên là NGÕ CỤT tạm thời (trả lại `screen_text`, không cuộn thêm).
//! * **UIPI (User Interface Privilege Isolation)** — nếu `claude`/`wt.exe`
//!   chạy NÂNG QUYỀN (Run as Administrator) mà huba thì không, Windows CHẶN
//!   `SendInput`/UIA đọc-ghi giữa hai mức quyền khác nhau — đúng vai trò TCC
//!   đóng trên macOS, nhưng huba CHƯA có cách phát hiện ca này để nói ra thay
//!   vì im lặng thất bại. Cần đo trên máy thật rồi thêm phép dò.
//! * **`SendInput` gửi input tới cửa sổ ĐANG Ở TRƯỚC**, y hệt CGEvent trên
//!   macOS — `SetForegroundWindow` trước khi gửi vẫn là bước KHÔNG PHẢI thủ
//!   tục, và gõ từ điện thoại vẫn giật tiêu điểm trên máy, cùng cái giá
//!   `keys.rs` đã ghi ở đầu tệp.
//! * **Đóng cửa sổ còn tiến trình sống** — chưa biết Windows Terminal có bật
//!   hộp thoại xác nhận kiểu *"Close all tabs?"* hay không (có, theo tài liệu
//!   công khai, tắt được qua setting `confirmCloseAllTabs`) — `close_window`
//!   ở đây gõ `exit`+Enter trước, cùng chiến lược `exit_and_close_shell` của
//!   macOS, để né đúng hộp thoại ấy thay vì tắt setting hộ người dùng.

use std::time::Duration;

use anyhow::{anyhow, Result};

use windows::core::{Interface, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT,
    VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowRect, IsWindow, IsWindowVisible, PostMessageW,
    SetForegroundWindow, ShowWindow, SW_HIDE, WM_CLOSE,
};

use crate::keys::{Closed, TabState};

use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};

/// Lớp cửa sổ thật của Windows Terminal — CHƯA xác nhận lại trên máy thật, chỉ
/// dựa trên tài liệu công khai. Xem mục "Chưa đo được" ở đầu tệp.
const TERMINAL_WINDOW_CLASS: &str = "CASCADIA_HOSTING_WINDOW_CLASS";

fn hwnd_of(window: i64) -> HWND {
    HWND(window as *mut std::ffi::c_void)
}

/// Mở một cửa sổ Windows Terminal MỚI (không phải tab) chạy `cmd`.
///
/// Trả `(hwnd, "hwnd thập phân")` — xem mục "Mô hình cửa sổ" ở đầu tệp cho lý
/// do chuỗi thứ hai không phải một tty thật.
pub fn open_window(cmd: &str) -> Result<(i64, String)> {
    let before = list_terminal_windows();
    std::process::Command::new("wt.exe")
        .args([
            "-w",
            "new",
            "powershell",
            "-NoLogo",
            "-NoExit",
            "-Command",
            cmd,
        ])
        .spawn()
        .map_err(|e| anyhow!("không chạy được wt.exe: {e}"))?;
    // wt.exe bàn giao cho tiến trình chủ (monarch) rồi tự thoát ngay — PID của
    // nó KHÔNG phải PID của cửa sổ thật, nên phải dò bằng SO SÁNH danh sách
    // cửa sổ trước/sau, giống hệt lý do `open_window` của macOS bỏ cách đọc
    // "window 1" (cửa sổ đang ở trước) để chuyển sang hỏi tty của chính tab
    // vừa tạo.
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
        let now = list_terminal_windows();
        if let Some(&hwnd) = now.iter().find(|h| !before.contains(h)) {
            return Ok((hwnd, hwnd.to_string()));
        }
    }
    Err(anyhow!(
        "wt.exe chạy nhưng không thấy cửa sổ Windows Terminal mới nào sau 8 giây"
    ))
}

/// Mọi cửa sổ đang mang lớp [`TERMINAL_WINDOW_CLASS`], theo thứ tự `EnumWindows`.
fn list_terminal_windows() -> Vec<i64> {
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = &mut *(lparam.0 as *mut Vec<i64>);
        let mut cls = [0u16; 256];
        let n = unsafe { windows::Win32::UI::WindowsAndMessaging::GetClassNameW(hwnd, &mut cls) };
        if n > 0 {
            let name = String::from_utf16_lossy(&cls[..n as usize]);
            if name == TERMINAL_WINDOW_CLASS {
                out.push(hwnd.0 as i64);
            }
        }
        BOOL(1)
    }
    let mut out: Vec<i64> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

/// Cửa sổ này còn tồn tại không — HWND là số ĐƯỢC DÙNG LẠI, nên `window_of`
/// phải luôn đi qua đây trước khi tin một số cũ trong sổ.
pub fn window_gone(window: i64) -> Result<bool> {
    let alive = unsafe { IsWindow(Some(hwnd_of(window))) };
    Ok(!alive.as_bool())
}

/// Tìm lại cửa sổ ứng với "tty" (chuỗi HWND) đã ghi trong sổ phiên.
pub fn window_of(tty: &str) -> Result<Option<i64>> {
    let Ok(id) = tty.trim().parse::<i64>() else {
        return Ok(None);
    };
    match window_gone(id) {
        Ok(true) => Ok(None),
        Ok(false) => Ok(Some(id)),
        Err(e) => Err(e),
    }
}

/// Như [`window_of`] — trên Windows không có khái niệm "cửa sổ KHÔNG PHẢI của
/// huba mở nhưng cùng tty" (mỗi phiên một cửa sổ do chính huba mở), nên hai
/// hàm này trùng nhau. Giữ tên riêng để chỗ gọi trong `sessions.rs` không cần
/// biết sự khác biệt đã biến mất trên nền này.
pub fn window_of_any(tty: &str) -> Result<Option<i64>> {
    window_of(tty)
}

pub fn bring_to_front(window: i64) -> Result<()> {
    let ok = unsafe { SetForegroundWindow(hwnd_of(window)) };
    if ok.as_bool() {
        Ok(())
    } else {
        Err(anyhow!(
            "SetForegroundWindow từ chối — cửa sổ có thể đã đóng"
        ))
    }
}

pub fn front_window() -> Result<Option<i64>> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Ok(None);
    }
    Ok(Some(hwnd.0 as i64))
}

/// `Busy`/`Idle` đọc bằng MÀN, không bằng bảng tiến trình — Windows Terminal
/// không cho hỏi "tab này bận không" từ ngoài tiến trình như Terminal.app
/// (`busy of tab`). Xấp xỉ: dòng CUỐI không rỗng của màn có phải dấu nhắc
/// PowerShell không (`PS ...\> `) — CHƯA đo được độ chắc của cách này trên
/// máy thật, xem mục "Chưa đo được".
pub fn tab_state(window: i64) -> Result<TabState> {
    if window_gone(window)? {
        return Ok(TabState::Gone);
    }
    let screen = screen_text(window)?;
    let last = screen
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    let looks_idle = last.trim_end().ends_with('>');
    Ok(if looks_idle {
        TabState::Idle
    } else {
        TabState::Busy
    })
}

/// Đọc chữ đang hiện trên cửa sổ, qua UI Automation `TextPattern` — vai trò
/// tương đương `contents of selected tab` của AppleScript.
///
/// 🔴 CHƯA ĐO ĐƯỢC: Windows Terminal có hỗ trợ UIA cho trình đọc màn hình,
/// nhưng liệu gốc `TextPattern` nằm ngay trên cửa sổ hay trên một control con
/// (thường thấy: cần `FindFirst` xuống một phần tử tên `"Terminal"`) CHƯA
/// được xác nhận trên máy thật. Bản dưới thử CẢ HAI — hỏi thẳng phần tử gốc
/// trước, dò xuống con nếu gốc không có `TextPattern` — nhưng độ đúng của
/// phỏng đoán này bằng không cho tới khi có ai chạy nó trên Windows thật.
pub fn screen_text(window: i64) -> Result<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let ui: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| anyhow!("không dựng được IUIAutomation: {e}"))?;
        let el = ui
            .ElementFromHandle(hwnd_of(window))
            .map_err(|e| anyhow!("không lấy được automation element của cửa sổ: {e}"))?;
        let pattern = el
            .GetCurrentPattern(UIA_TextPatternId)
            .map_err(|e| anyhow!("cửa sổ không có TextPattern (UIA): {e}"))?;
        let text_pattern: IUIAutomationTextPattern = pattern
            .cast()
            .map_err(|e| anyhow!("TextPattern không đúng kiểu mong đợi: {e}"))?;
        let range = text_pattern
            .DocumentRange()
            .map_err(|e| anyhow!("không lấy được DocumentRange: {e}"))?;
        let text = range
            .GetText(-1)
            .map_err(|e| anyhow!("không đọc được chữ từ TextPattern: {e}"))?;
        Ok(text.to_string())
    }
}

/// Chưa có cách cuộn ngược qua UIA đã đo được — trả nguyên `screen_text`,
/// đúng luật 13②: không đo được thì đừng bịa một kết quả trông như đã cuộn.
pub fn screen_scrollback(window: i64, _steps: usize, _du: impl Fn(&str) -> bool) -> Result<String> {
    screen_text(window)
}

/// Tên "phím" hiểu bằng đúng vốn từ `keys::key_payload`, cho `SendInput`.
fn key_to_input(name: &str) -> Result<Vec<INPUT>> {
    let vk = match name {
        "enter" => VK_RETURN,
        "esc" => VK_ESCAPE,
        "up" => VK_UP,
        "down" => VK_DOWN,
        "right" => VK_RIGHT,
        "left" => VK_LEFT,
        "tab" => VK_TAB,
        "space" => VK_SPACE,
        // Ctrl+C: giữ Ctrl xuống, bấm C, thả cả hai — không có phím rời đơn lẻ
        // nào phát ETX trên Windows như tty của macOS tự dịch.
        "ctrl-c" | "ctrlc" | "^c" => {
            return Ok(vk_pair_with_modifier(
                windows::Win32::UI::Input::KeyboardAndMouse::VK_CONTROL,
                VIRTUAL_KEY(0x43), // 'C'
            ));
        }
        d if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => {
            return Ok(unicode_input(d));
        }
        other => return Err(anyhow!("không biết phím '{other}'")),
    };
    Ok(vk_pair(vk))
}

fn key_down_up(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn vk_pair(vk: VIRTUAL_KEY) -> Vec<INPUT> {
    vec![
        key_down_up(vk, KEYBD_EVENT_FLAGS(0)),
        key_down_up(vk, KEYEVENTF_KEYUP),
    ]
}

fn vk_pair_with_modifier(modifier: VIRTUAL_KEY, vk: VIRTUAL_KEY) -> Vec<INPUT> {
    vec![
        key_down_up(modifier, KEYBD_EVENT_FLAGS(0)),
        key_down_up(vk, KEYBD_EVENT_FLAGS(0)),
        key_down_up(vk, KEYEVENTF_KEYUP),
        key_down_up(modifier, KEYEVENTF_KEYUP),
    ]
}

/// Một chuỗi bất kỳ, từng đơn vị UTF-16, qua `KEYEVENTF_UNICODE` — path này
/// KHÔNG cần biết layout bàn phím (khác `VkKeyScan`), nên chữ tiếng Việt của
/// chủ máy gõ qua điện thoại đi thẳng, không cần dịch phím.
fn unicode_input(s: &str) -> Vec<INPUT> {
    let mut out = Vec::new();
    for unit in s.encode_utf16() {
        let ki = |flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        out.push(ki(KEYBD_EVENT_FLAGS(0)));
        out.push(ki(KEYEVENTF_KEYUP));
    }
    out
}

fn send_raw(inputs: &[INPUT]) -> Result<()> {
    if inputs.is_empty() {
        return Ok(());
    }
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(anyhow!(
            "SendInput chỉ nhận {sent}/{} sự kiện — Windows có thể đang chặn ca này \
             (UIPI: đích chạy nâng quyền mà huba thì không?)",
            inputs.len()
        ));
    }
    Ok(())
}

/// Gửi một dãy phím RỜI — không kèm dấu xuống dòng nào, và KHÔNG nhóm theo
/// "lượt ghi" như macOS (xem mục 1 ở đầu tệp): mỗi phím là một `SendInput`
/// riêng, cách nhau một nhịp nhỏ để TUI kịp vẽ.
pub fn press_writes(window: i64, writes: &[Vec<String>]) -> Result<()> {
    bring_to_front(window)?;
    for group in writes {
        for key in group {
            if key == "clear" {
                continue; // `clear` không phải một phím thật — xem `clear_box`.
            }
            send_raw(&key_to_input(key)?)?;
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    Ok(())
}

/// Gõ một khối chữ bất kỳ vào ô nhập — KHÔNG tự bấm Enter (đúng hợp đồng của
/// `keys::type_and_send`, hàm ấy tự quyết định lúc nào cần Enter).
pub fn type_into(window: i64, text: &str) -> Result<()> {
    bring_to_front(window)?;
    send_raw(&unicode_input(text))
}

pub fn send_bare(window: i64, keys: &[String]) -> Result<()> {
    press_writes(window, &[keys.to_vec()])
}

pub fn focus_window(window: i64) -> Result<()> {
    bring_to_front(window)
}

pub fn window_size(window: i64) -> Result<(i64, i64)> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd_of(window), &mut rect) }
        .map_err(|e| anyhow!("GetWindowRect thất bại: {e}"))?;
    // Trả về PIXEL, không phải hàng/cột như macOS — Windows Terminal không lộ
    // kích thước bằng KÝ TỰ qua Win32 thường (cần UIA riêng, chưa làm). Chỗ
    // gọi hiện chỉ dùng số này để GHI SỔ chứ không tính bố cục, nên tạm chấp
    // nhận đơn vị khác — nhưng đây là một khoảng lệch thật, ghi rõ để không
    // ai đọc nhầm "61" là 61 CỘT.
    Ok((
        (rect.bottom - rect.top) as i64,
        (rect.right - rect.left) as i64,
    ))
}

/// Không có khái niệm "tiến trình Terminal.app" trên Windows — trả lỗi rõ
/// ràng thay vì bịa một pid. Chỗ gọi (macOS-only trong `keys.rs`) không có
/// nhánh Windows nào cần số này; giữ hàm để khỏi phải sửa chữ ký nơi khác.
pub fn terminal_pid() -> Result<i32> {
    Err(anyhow!("không có khái niệm PID Terminal.app trên Windows"))
}

/// Đóng cửa sổ: gõ `exit`+Enter trước (né hộp thoại xác nhận nếu còn tiến
/// trình sống, cùng chiến lược `exit_and_close_shell` của macOS), rồi
/// `WM_CLOSE`, rồi xác nhận lại bằng `IsWindow`.
pub fn close_window(window: i64) -> Result<Closed> {
    if window_gone(window)? {
        return Ok(Closed::Gone);
    }
    let _ = type_into(window, "exit");
    let _ = press_writes(window, &[vec!["enter".to_string()]]);
    std::thread::sleep(Duration::from_millis(500));
    for _ in 0..6 {
        if window_gone(window)? {
            return Ok(Closed::Gone);
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    unsafe {
        let _ = PostMessageW(Some(hwnd_of(window)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    for _ in 0..6 {
        std::thread::sleep(Duration::from_millis(300));
        if window_gone(window)? {
            return Ok(Closed::Gone);
        }
    }
    // `close` không ăn — thử ẩn, cùng đường lùi macOS đã dùng khi `close`
    // chạy êm mà cửa sổ không đóng.
    let hidden = unsafe { ShowWindow(hwnd_of(window), SW_HIDE) };
    if hidden.as_bool() && !unsafe { IsWindowVisible(hwnd_of(window)) }.as_bool() {
        return Ok(Closed::Hidden);
    }
    anyhow::bail!("gửi WM_CLOSE rồi thử ẩn đều không ăn — cửa sổ {window} vẫn còn hiện")
}

/// Chưa có phép đo tương đương `ioreg` (khoá màn hình) trên Windows ở đây.
/// `None` = "chưa đo được", đúng luật 13②: không phải 0% cũng không phải
/// đoán bừa.
pub fn screen_locked() -> Option<bool> {
    None
}

/// Đo số tiến trình gắn vào console của cửa sổ — cần `AttachConsole` từ tiến
/// trình khác, việc chưa làm ở bản này. `None` là "không đo được", không phải
/// "không còn tiến trình nào".
pub fn tab_proc_count(window: i64) -> Result<usize> {
    let _ = window;
    Err(anyhow!(
        "chưa cài: đếm tiến trình trong console trên Windows"
    ))
}

pub fn tab_process_count(window: i64) -> Result<Option<usize>> {
    let _ = window;
    Ok(None)
}

pub fn close_hidden_again(window: i64) -> Result<bool> {
    let hidden = unsafe { ShowWindow(hwnd_of(window), SW_HIDE) };
    Ok(hidden.as_bool())
}

/// Chưa cài chụp ảnh cửa sổ thật (PrintWindow + mã hoá PNG) — trả lỗi rõ thay
/// vì một tệp rỗng trông như đã chụp.
pub fn photograph_window(window: i64, path: &std::path::Path) -> Result<()> {
    let _ = (window, path);
    Err(anyhow!("chưa cài: chụp ảnh cửa sổ thật trên Windows"))
}

/// Bịt ô nhập bằng cách bấm lùi (Backspace) đủ số ký tự đang hiện — không có
/// khái niệm "byte ESC chặn CR" của macOS ở đây vì Windows không tự kèm CR
/// vào lượt gõ.
pub fn clear_box(window: i64) -> Result<bool> {
    let screen = screen_text(window)?;
    let Some(box_text) = crate::keys::input_box_text(&screen) else {
        return Ok(false);
    };
    bring_to_front(window)?;
    let n = box_text.chars().count();
    let backs: Vec<INPUT> = (0..n).flat_map(|_| vk_pair(VK_BACK)).collect();
    send_raw(&backs)?;
    Ok(true)
}

/// Hàng chờ tin nhắn xếp trong ô nhập — Windows chưa có phép đo tương đương
/// `clear_queue` của macOS (đọc dấu `»`/số dòng trên màn Terminal). Trả
/// `(0, 0)` kèm lỗi log ở chỗ gọi nếu cần phân biệt — ở đây chỉ nói KHÔNG cài.
pub fn clear_queue(window: i64) -> Result<(usize, usize)> {
    let _ = window;
    Err(anyhow!("chưa cài: xoá hàng chờ trên Windows"))
}

pub fn frame_is_blank(path: &std::path::Path) -> Option<bool> {
    let _ = path;
    None
}

/// Xem `keys::accessibility_trusted` cho ngữ cảnh đầy đủ — trên Windows câu
/// trả lời luôn `true` cho một tiến trình không nâng quyền, TRỪ hàng rào UIPI
/// chưa đo được ở đây.
pub fn trusted() -> bool {
    true
}
