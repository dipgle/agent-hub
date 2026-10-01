//! Windows — cùng vai trò với `keys.rs` trên macOS: gõ vào console của một
//! phiên, và đọc lại nó. **CHƯA CHẠY TRÊN WINDOWS THẬT** — phiên viết tệp này
//! chạy trên macOS, không có máy Windows nào để đo. Phép đo đã dựng sẵn:
//! `huba windows-tu-kiem` (xem [`tu_kiem`]) mở console riêng, gõ một mốc, đọc
//! lại — chạy nó trên máy thật TRƯỚC khi tin bất kỳ dòng nào dưới đây.
//!
//! # Mô hình: CONSOLE, không phải cửa sổ (viết lại 01/10)
//!
//! Bản 08/09 đi đường cửa sổ: `SetForegroundWindow` + `SendInput` để gõ, UI
//! Automation để đọc. Ba chỗ hỏng đã đọc ra được từ mã (chưa từng chạy):
//! * **Tiến trình nền không được giành tiêu điểm.** hubd chạy từ Task Scheduler,
//!   không có cửa sổ; luật khoá-tiền-cảnh của Windows thường từ chối nó — và khi
//!   bị từ chối, `SendInput` gửi phím vào **cửa sổ đang ở trước**, tức gõ nhầm.
//! * **Windows Terminal dùng MỘT tiến trình cho MỌI cửa sổ**, nên cửa sổ không
//!   nối được với phiên nào bằng cây tiến trình; và một cửa sổ nhiều tab thì
//!   `SendInput` rơi vào tab đang chọn, không phải tab của phiên.
//! * UIA hỏi `TextPattern` trên cửa sổ gốc, không có đường lùi như chú thích hứa.
//!
//! Đường console tránh cả ba: `AttachConsole(pid)` gắn huba vào ĐÚNG console của
//! phiên (mỗi tab Windows Terminal là một console riêng), rồi
//! `WriteConsoleInputW` đặt phím thẳng vào hàng nhập của console ấy và
//! `ReadConsoleOutputCharacterW` đọc chữ đang hiện — không cần tiêu điểm, không
//! cần cửa sổ ở trước, và UIPI không chen vào vì không có thông điệp cửa sổ nào.
//! "Cửa sổ" của huba trên Windows vì thế là **pid shell gốc của console**
//! (`con<pid>` trong sổ — xem `win_procs.rs`).
//!
//! # Chưa đo được — `huba windows-tu-kiem` đo phần lớn
//!
//! * Gõ + đọc qua console trên console cổ điển (conhost) VÀ trên ConPTY (tab
//!   Windows Terminal) — tự kiểm ③ ④.
//! * `claude` (Node, đọc phím bằng `ReadConsoleInputW`) nhận phím mũi tên và
//!   khối dán `ESC[200~…ESC[201~` từ `WriteConsoleInputW` — tự kiểm KHÔNG đo
//!   được (cần một phiên `claude` thật); lượt `/new` thật đầu tiên là phép đo.
//! * `AttachConsole` khiến hubd nhận tín hiệu của console ấy trong lúc đang gắn
//!   (vài mili giây). Ctrl+C bị bỏ qua ([`bo_qua_tin_hieu`]); đóng hẳn console
//!   đúng khoảnh khắc ấy thì Windows có thể kết thúc hubd — Task Scheduler
//!   khởi động lại (`install_update.ps1`, `RestartCount`).

use std::sync::Mutex;
use std::time::Duration;

use anyhow::{anyhow, Result};

use windows::core::{w, BOOL};
use windows::Wdk::System::Threading::{NtQueryInformationProcess, ProcessCommandLineInformation};
use windows::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, HWND, STILL_ACTIVE, UNICODE_STRING,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    AttachConsole, FreeConsole, GetConsoleScreenBufferInfo, GetConsoleTitleW, GetConsoleWindow,
    ReadConsoleOutputCharacterW, SetConsoleCtrlHandler, WriteConsoleInputW,
    CONSOLE_SCREEN_BUFFER_INFO, COORD, INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT, KEY_EVENT_RECORD,
    KEY_EVENT_RECORD_0, LEFT_CTRL_PRESSED,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::VkKeyScanW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetForegroundWindow, SetForegroundWindow, GA_ROOTOWNER,
};

use crate::keys::{Closed, Tab, TabState};
use crate::win_procs::{self, Row};

/// A Win32 clean-up call whose failure changes nothing for the caller — but a
/// leaked handle or a console left attached must still be SAID (rule 3).
fn ghi_neu_hong(buoc: &str, r: windows::core::Result<()>) {
    if let Err(e) = r {
        crate::logging::warn(
            "win_cleanup_failed",
            serde_json::json!({ "step": buoc, "err": e.to_string() }),
        );
    }
}

// ── Bảng tiến trình ────────────────────────────────────────────────────────

/// Mọi tiến trình trên máy, đọc MỘT LẦN (ToolHelp32). Dòng lệnh chỉ đọc cho
/// tiến trình console ([`win_procs::CONSOLE_CLIENTS`]) — đó là tập duy nhất
/// huba cần soi chữ (`claude`, `node …cli.js`, `bash … shell-snapshots`), và mở
/// mọi tiến trình trên máy mỗi ảnh chụp là trả giá cho ~300 lời gọi vô ích.
pub fn process_table() -> Result<Vec<Row>> {
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
            .map_err(|e| anyhow!("CreateToolhelp32Snapshot thất bại: {e}"))?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            let n = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            let exe = String::from_utf16_lossy(&entry.szExeFile[..n]);
            let pid = entry.th32ProcessID;
            let cmd = if win_procs::is_console_client(&exe) {
                command_line(pid).unwrap_or_default()
            } else {
                String::new()
            };
            out.push(Row {
                pid: pid as i64,
                ppid: entry.th32ParentProcessID as i64,
                exe,
                cmd,
            });
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        ghi_neu_hong("CloseHandle(snapshot)", CloseHandle(snap));
    }
    if out.is_empty() {
        return Err(anyhow!(
            "ToolHelp trả về 0 tiến trình — không phải một máy trống"
        ));
    }
    Ok(out)
}

/// Dòng lệnh của tiến trình khác — `NtQueryInformationProcess`
/// (`ProcessCommandLineInformation`, Windows 8.1+). `None` khi không mở được
/// (tiến trình của người dùng khác / được bảo vệ) — chỗ gọi rơi về tên tệp.
fn command_line(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut need = 0u32;
        // Size probe: EXPECTED to fail (buffer too small) — the answer is
        // `need`, the status only explains a `need` that came back unusable.
        let do_co = NtQueryInformationProcess(
            h,
            ProcessCommandLineInformation,
            std::ptr::null_mut(),
            0,
            &mut need,
        );
        if need == 0 || need > 1 << 20 {
            crate::logging::debug(
                "win_cmdline_unreadable",
                serde_json::json!({ "pid": pid, "status": do_co.0, "need": need }),
            );
            ghi_neu_hong("CloseHandle(process)", CloseHandle(h));
            return None;
        }
        // `u64` để bộ đệm căn 8 byte — `UNICODE_STRING` mang con trỏ.
        let mut buf = vec![0u64; (need as usize).div_ceil(8)];
        let st = NtQueryInformationProcess(
            h,
            ProcessCommandLineInformation,
            buf.as_mut_ptr().cast(),
            need,
            &mut need,
        );
        ghi_neu_hong("CloseHandle(process)", CloseHandle(h));
        if st.is_err() {
            return None;
        }
        let us = &*(buf.as_ptr() as *const UNICODE_STRING);
        if us.Buffer.is_null() || us.Length == 0 {
            return None;
        }
        let units = std::slice::from_raw_parts(us.Buffer.0, us.Length as usize / 2);
        Some(String::from_utf16_lossy(units))
    }
}

/// Tiến trình còn chạy không. pid ĐƯỢC DÙNG LẠI trên Windows như trên macOS —
/// chỗ gọi nào cần chắc "vẫn là nó" thì so thêm bảng tiến trình.
pub fn process_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid as u32) else {
            return false;
        };
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code).is_ok();
        ghi_neu_hong("CloseHandle(process)", CloseHandle(h));
        ok && code == STILL_ACTIVE.0 as u32
    }
}

// ── Gắn vào console của một tiến trình ─────────────────────────────────────

/// `AttachConsole` là trạng thái của CẢ tiến trình: hai luồng gắn cùng lúc thì
/// một luồng đọc nhầm console của luồng kia. Mọi lượt gắn đi qua khoá này.
static CONSOLE: Mutex<()> = Mutex::new(());

unsafe extern "system" fn bo_qua_tin_hieu(_kind: u32) -> BOOL {
    BOOL(1)
}

struct Gan {
    conin: HANDLE,
    conout: HANDLE,
}

impl Drop for Gan {
    fn drop(&mut self) {
        unsafe {
            ghi_neu_hong("CloseHandle(CONIN$)", CloseHandle(self.conin));
            ghi_neu_hong("CloseHandle(CONOUT$)", CloseHandle(self.conout));
            ghi_neu_hong("FreeConsole", FreeConsole());
        }
    }
}

/// Chạy `f` trong lúc đang gắn vào console của `pid`, rồi THẢ ra ngay.
///
/// Thất bại khi chính tiến trình này đã có console riêng (`huba.exe` chạy từ
/// một terminal) — khi ấy KHÔNG thả console của mình ra để gắn sang (mất chỗ
/// in kết quả), mà báo rõ: việc này là của hubd (không console) hoặc của lượt
/// tự kiểm chạy tách rời (`tu_kiem`).
fn with_console<T>(pid: i64, f: impl FnOnce(&Gan) -> Result<T>) -> Result<T> {
    let _khoa = CONSOLE.lock().unwrap_or_else(|p| p.into_inner());
    unsafe {
        AttachConsole(pid as u32).map_err(|e| {
            anyhow!(
                "không gắn được vào console của pid {pid}: {e} — tiến trình đã thoát, \
                 hoặc chính huba đang có console riêng (chạy từ terminal thì việc này là của hubd)"
            )
        })?;
        // Đăng ký MỖI lượt là đúng: danh sách bộ xử lý gắn với console đang gắn.
        // Failing here leaves hubd killable by a Ctrl+C typed into that console.
        ghi_neu_hong(
            "SetConsoleCtrlHandler",
            SetConsoleCtrlHandler(Some(bo_qua_tin_hieu), true),
        );
        let mo = |ten| {
            CreateFileW(
                ten,
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        };
        let conin = match mo(w!("CONIN$")) {
            Ok(h) => h,
            Err(e) => {
                ghi_neu_hong("FreeConsole", FreeConsole());
                return Err(anyhow!("gắn được nhưng không mở được CONIN$: {e}"));
            }
        };
        let conout = match mo(w!("CONOUT$")) {
            Ok(h) => h,
            Err(e) => {
                ghi_neu_hong("CloseHandle(CONIN$)", CloseHandle(conin));
                ghi_neu_hong("FreeConsole", FreeConsole());
                return Err(anyhow!("gắn được nhưng không mở được CONOUT$: {e}"));
            }
        };
        let gan = Gan { conin, conout };
        f(&gan)
    }
}

/// Console của `pid` có CỬA SỔ không (cửa sổ console cổ điển, hoặc cửa sổ giả
/// của ConPTY). Một `claude -p` chạy với `CREATE_NO_WINDOW` — phép dò hạn mức
/// của chính huba — có console mà không có cửa sổ: KHÔNG phải một phiên trên màn.
pub fn has_console_window(pid: i64) -> bool {
    with_console(pid, |_| Ok(!unsafe { GetConsoleWindow() }.0.is_null())).unwrap_or(false)
}

fn read_screen(g: &Gan) -> Result<String> {
    let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
    unsafe { GetConsoleScreenBufferInfo(g.conout, &mut info) }
        .map_err(|e| anyhow!("GetConsoleScreenBufferInfo thất bại: {e}"))?;
    let win = info.srWindow;
    let width = (win.Right - win.Left + 1).max(0) as usize;
    let mut lines: Vec<String> = Vec::new();
    for y in win.Top..=win.Bottom {
        let mut buf = vec![0u16; width];
        let mut read = 0u32;
        unsafe {
            ReadConsoleOutputCharacterW(g.conout, &mut buf, COORD { X: win.Left, Y: y }, &mut read)
        }
        .map_err(|e| anyhow!("ReadConsoleOutputCharacterW thất bại ở dòng {y}: {e}"))?;
        let n = (read as usize).min(buf.len());
        lines.push(String::from_utf16_lossy(&buf[..n]).trim_end().to_string());
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    Ok(lines.join("\n"))
}

fn console_title() -> String {
    let mut buf = vec![0u16; 1024];
    let n = unsafe { GetConsoleTitleW(&mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n.min(buf.len())])
}

// ── Sự kiện phím ───────────────────────────────────────────────────────────

const ENHANCED_KEY: u32 = 0x0100;
const SHIFT_PRESSED: u32 = 0x0010;

fn key_record(down: bool, vk: u16, ch: u16, state: u32) -> INPUT_RECORD {
    INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: BOOL(down as i32),
                wRepeatCount: 1,
                wVirtualKeyCode: vk,
                wVirtualScanCode: 0,
                uChar: KEY_EVENT_RECORD_0 { UnicodeChar: ch },
                dwControlKeyState: state,
            },
        },
    }
}

fn press(vk: u16, ch: u16, state: u32) -> [INPUT_RECORD; 2] {
    [
        key_record(true, vk, ch, state),
        key_record(false, vk, ch, state),
    ]
}

/// Một đơn vị UTF-16 thành cặp phím. Ký tự có trên bố cục bàn phím đang dùng
/// thì mang luôn mã phím ảo (`VkKeyScanW`) — vài chương trình .NET (PSReadLine)
/// đọc mã ấy; chữ tiếng Việt không có trên bố cục thì để mã 0, chỉ mang ký tự.
fn char_records(unit: u16) -> [INPUT_RECORD; 2] {
    let scan = unsafe { VkKeyScanW(unit) };
    if scan == -1 {
        return press(0, unit, 0);
    }
    let vk = (scan as u16) & 0xff;
    let shift = if (scan as u16) & 0x0100 != 0 {
        SHIFT_PRESSED
    } else {
        0
    };
    press(vk, unit, shift)
}

fn text_records(text: &str) -> Vec<INPUT_RECORD> {
    text.encode_utf16().flat_map(char_records).collect()
}

/// Tên phím, đúng vốn từ `keys::key_payload`.
fn key_records(name: &str) -> Result<Vec<INPUT_RECORD>> {
    let r = match name {
        "enter" => press(0x0D, 13, 0),
        "esc" => press(0x1B, 27, 0),
        "up" => press(0x26, 0, ENHANCED_KEY),
        "down" => press(0x28, 0, ENHANCED_KEY),
        "left" => press(0x25, 0, ENHANCED_KEY),
        "right" => press(0x27, 0, ENHANCED_KEY),
        "tab" => press(0x09, 9, 0),
        "space" => press(0x20, 32, 0),
        "backspace" => press(0x08, 8, 0),
        // Ctrl+C = ký tự ETX kèm trạng thái Ctrl giữ — ở chế độ đọc thô (Node),
        // chương trình nhận đúng byte 3 như tty macOS.
        "ctrl-c" | "ctrlc" | "^c" => press(0x43, 3, LEFT_CTRL_PRESSED),
        d if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => {
            return Ok(text_records(d));
        }
        other => return Err(anyhow!("không biết phím '{other}'")),
    };
    Ok(r.to_vec())
}

fn write_input(g: &Gan, records: &[INPUT_RECORD]) -> Result<()> {
    // Từng khúc vừa phải: hàng nhập của console có trần, và một khối dán dài
    // đẩy một lần dễ bị cắt ngang.
    for chunk in records.chunks(256) {
        let mut written = 0u32;
        unsafe { WriteConsoleInputW(g.conin, chunk, &mut written) }
            .map_err(|e| anyhow!("WriteConsoleInputW thất bại: {e}"))?;
        if written as usize != chunk.len() {
            return Err(anyhow!(
                "WriteConsoleInputW chỉ nhận {written}/{} sự kiện",
                chunk.len()
            ));
        }
    }
    Ok(())
}

// ── Mặt tiếp xúc mà `keys.rs` gọi tới ──────────────────────────────────────

/// `con12345` → `12345` nếu shell gốc ấy còn sống. Số trần (HWND của bản cũ
/// trong sổ) ⟹ `None`, không đoán.
pub fn window_of(tty: &str) -> Result<Option<i64>> {
    Ok(win_procs::parse_tty(tty).filter(|pid| process_alive(*pid)))
}

/// Mỗi console là một "tab" — không có tab nào khác cùng tên để mà nhầm.
pub fn window_of_any(tty: &str) -> Result<Option<i64>> {
    window_of(tty)
}

/// Tên "tab" đang chọn của console `window` — chính nó.
pub fn selected_tab_tty(window: i64) -> Result<String> {
    if process_alive(window) {
        Ok(win_procs::tty_of_root(window))
    } else {
        Err(anyhow!("shell gốc {window} không còn"))
    }
}

pub fn window_gone(window: i64) -> Result<bool> {
    Ok(!process_alive(window))
}

/// Chữ đang hiện trên console (khung nhìn `srWindow`).
pub fn screen_text(window: i64) -> Result<String> {
    with_console(window, read_screen)
}

/// Console cổ điển giữ cả phần đã cuộn trong bộ đệm, nhưng ConPTY (tab Windows
/// Terminal) chỉ giữ khung nhìn — chưa đo. Trả đúng khung nhìn, không giả vờ đã
/// cuộn (luật 13②).
pub fn screen_scrollback(window: i64, _steps: usize, _du: impl Fn(&str) -> bool) -> Result<String> {
    screen_text(window)
}

/// Gõ một khối chữ — KHÔNG tự bấm Enter (hợp đồng của `keys::type_and_send`).
/// Khối nhiều dòng đi dưới dạng DÁN (`ESC[200~ … ESC[201~`), như terminal thật
/// làm khi dán: gửi từng dòng kèm Enter là gửi đi từng mảnh của đề bài.
pub fn type_into(window: i64, text: &str) -> Result<()> {
    let records = if text.contains('\n') || text.contains('\r') {
        let body = text.replace("\r\n", "\r").replace('\n', "\r");
        text_records(&format!("\u{1b}[200~{body}\u{1b}[201~"))
    } else {
        text_records(text)
    };
    with_console(window, |g| write_input(g, &records))
}

/// Gửi một dãy phím RỜI. Không có CR nào tự kèm như `do script` của macOS, nên
/// không cần nhóm theo lượt ghi — nhịp nghỉ 30 ms giữa các phím để TUI kịp vẽ.
pub fn press_writes(window: i64, writes: &[Vec<String>]) -> Result<()> {
    for group in writes {
        for key in group {
            if key == "clear" {
                continue; // không phải một phím thật — xem `clear_box`.
            }
            let records = key_records(key)?;
            with_console(window, |g| write_input(g, &records))?;
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    Ok(())
}

pub fn send_bare(window: i64, keys: &[String]) -> Result<()> {
    press_writes(window, &[keys.to_vec()])
}

/// Bận/rảnh đọc từ CÂY TIẾN TRÌNH: shell gốc còn con nào chạy không — đúng
/// nghĩa `busy of tab` của Terminal.app, không đoán từ dấu nhắc trên màn.
pub fn tab_state(window: i64) -> Result<TabState> {
    if !process_alive(window) {
        return Ok(TabState::Gone);
    }
    let rows = process_table()?;
    match win_procs::process_count(&rows, window) {
        None => Ok(TabState::Gone),
        Some(1) => Ok(TabState::Idle),
        Some(_) => Ok(TabState::Busy),
    }
}

pub fn tab_proc_count(window: i64) -> Result<usize> {
    let rows = process_table()?;
    win_procs::process_count(&rows, window)
        .ok_or_else(|| anyhow!("shell gốc {window} không còn trong bảng tiến trình"))
}

/// `Ok(None)` = console đã HẾT — cùng hợp đồng với macOS (`keys.rs`).
pub fn tab_process_count(window: i64) -> Result<Option<usize>> {
    let rows = process_table()?;
    Ok(win_procs::process_count(&rows, window))
}

/// Cỡ console tính bằng KÝ TỰ `(hàng, cột)` — cùng đơn vị với macOS.
pub fn window_size(window: i64) -> Result<(i64, i64)> {
    with_console(window, |g| {
        let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
        unsafe { GetConsoleScreenBufferInfo(g.conout, &mut info) }
            .map_err(|e| anyhow!("GetConsoleScreenBufferInfo thất bại: {e}"))?;
        let w = info.srWindow;
        Ok(((w.Bottom - w.Top + 1) as i64, (w.Right - w.Left + 1) as i64))
    })
}

/// Cửa sổ chứa console: cửa sổ console cổ điển chính nó; cửa sổ giả của ConPTY
/// thì chủ của nó là cửa sổ Windows Terminal (WT gán chủ từ 1.17 — CHƯA đo).
fn host_window(window: i64) -> Result<HWND> {
    with_console(window, |_| {
        let h = unsafe { GetConsoleWindow() };
        if h.0.is_null() {
            return Err(anyhow!("console của {window} không có cửa sổ"));
        }
        let owner = unsafe { GetAncestor(h, GA_ROOTOWNER) };
        Ok(if owner.0.is_null() { h } else { owner })
    })
}

/// Đưa cửa sổ chứa console lên trước. KHÔNG còn là bước bắt buộc trước khi gõ
/// (gõ đi thẳng vào console) — chỉ dùng khi chủ máy xin xem cửa sổ.
pub fn bring_to_front(window: i64) -> Result<()> {
    let h = host_window(window)?;
    if unsafe { SetForegroundWindow(h) }.as_bool() {
        Ok(())
    } else {
        Err(anyhow!(
            "Windows từ chối đưa cửa sổ lên trước (luật khoá tiền cảnh với tiến trình nền)"
        ))
    }
}

pub fn focus_window(window: i64) -> Result<()> {
    bring_to_front(window)
}

/// Console của cửa sổ đang ở trước — không đọc được từ HWND ngược về shell
/// gốc mà không quét mọi console, nên KHÔNG đoán: `None`.
pub fn front_window() -> Result<Option<i64>> {
    let _ = unsafe { GetForegroundWindow() };
    Ok(None)
}

pub fn terminal_pid() -> Result<i32> {
    Err(anyhow!("không có khái niệm PID Terminal.app trên Windows"))
}

/// Đóng console: gõ `exit` + Enter vào shell gốc ĐANG RẢNH, chờ nó thoát.
/// Shell thoát thì Windows Terminal tự đóng tab (`closeOnExit` mặc định) và
/// console cổ điển tự đóng cửa sổ. KHÔNG gửi `WM_CLOSE` tới cửa sổ Windows
/// Terminal: một cửa sổ có thể chứa tab của phiên KHÁC.
pub fn close_window(window: i64) -> Result<Closed> {
    if !process_alive(window) {
        return Ok(Closed::Gone);
    }
    if tab_state(window)? == TabState::Busy {
        anyhow::bail!("console {window} còn chương trình đang chạy — không gõ exit đè lên");
    }
    type_into(window, "exit")?;
    press_writes(window, &[vec!["enter".to_string()]])?;
    for _ in 0..25 {
        std::thread::sleep(Duration::from_millis(200));
        if !process_alive(window) {
            return Ok(Closed::Gone);
        }
    }
    anyhow::bail!("đã gõ exit nhưng shell {window} vẫn chạy sau 5 giây")
}

pub fn close_hidden_again(window: i64) -> Result<bool> {
    let _ = window;
    Ok(false)
}

pub fn screen_locked() -> Option<bool> {
    None
}

pub fn photograph_window(window: i64, path: &std::path::Path) -> Result<()> {
    let _ = (window, path);
    Err(anyhow!(
        "chưa cài: chụp ảnh cửa sổ trên Windows — dùng /shot (chữ)"
    ))
}

pub fn frame_is_blank(path: &std::path::Path) -> Option<bool> {
    let _ = path;
    None
}

/// Xoá ô nhập bằng Backspace đủ số ký tự đang hiện, rồi PHÁN bằng lượt đọc lại
/// — cùng hợp đồng [`crate::keys::Cleared`] với macOS.
pub fn clear_box(window: i64) -> Result<crate::keys::Cleared> {
    use crate::keys::{box_state, BoxState, Cleared};
    let n = match box_state(&screen_text(window)?) {
        BoxState::NoBox => return Ok(Cleared::NoBox),
        BoxState::Empty => return Ok(Cleared::Clean),
        BoxState::Text(n) => n,
    };
    let backs: Vec<INPUT_RECORD> = (0..n).flat_map(|_| press(0x08, 8, 0)).collect();
    with_console(window, |g| write_input(g, &backs))?;
    std::thread::sleep(Duration::from_millis(150));
    Ok(match box_state(&screen_text(window)?) {
        BoxState::Empty => Cleared::Clean,
        BoxState::NoBox => Cleared::NoBox,
        BoxState::Text(_) => Cleared::TextLeft,
    })
}

pub fn clear_queue(window: i64) -> Result<(usize, usize)> {
    let _ = window;
    Err(anyhow!("chưa cài: xoá hàng chờ trên Windows"))
}

/// Gõ phím vào console không đòi quyền hệ điều hành nào — luôn `true`.
pub fn trusted() -> bool {
    true
}

/// Mọi console đáng liệt kê, kèm chữ trên màn khi `with_screens` — vai trò của
/// `terminal_tabs`/`terminal_screens` trên macOS. Console KHÔNG có cửa sổ (phép
/// dò `claude -p` của chính huba) bị loại.
pub fn terminal_tabs(with_screens: bool) -> Result<Vec<Tab>> {
    let rows = process_table()?;
    let mut tabs = Vec::new();
    for c in win_procs::consoles(&rows) {
        let read = with_console(c.root, |g| {
            if unsafe { GetConsoleWindow() }.0.is_null() {
                return Ok(None);
            }
            let screen = if with_screens {
                Some(read_screen(g)?)
            } else {
                None
            };
            Ok(Some((screen, console_title())))
        });
        let (screen, title) = match read {
            Ok(Some(x)) => x,
            Ok(None) => continue,
            Err(e) => {
                crate::logging::warn(
                    "windows_console_unreadable",
                    serde_json::json!({ "root": c.root, "err": e.to_string() }),
                );
                continue;
            }
        };
        tabs.push(Tab {
            tty: win_procs::tty_of_root(c.root),
            busy: c.busy,
            procs: c.procs,
            screen,
            title,
        });
    }
    Ok(tabs)
}

// ── Mở một console mới cho `/new` ──────────────────────────────────────────

/// Mở một cửa sổ MỚI chạy `script` (PowerShell) và trả `(pid shell gốc,
/// "con<pid>")`.
///
/// `-EncodedCommand`: kịch bản đi dưới dạng base64, nên `;` (dấu tách lệnh của
/// chính `wt.exe`), nháy, ngoặc trong mẫu `--disallowedTools` không còn ký tự
/// nào để vỡ. Một dòng chú thích mang mã ngẫu nhiên làm chuỗi mã hoá DUY NHẤT —
/// nhờ đó tìm lại đúng `powershell.exe` vừa mở trong bảng tiến trình (pid của
/// `wt.exe` vô dụng: nó giao việc cho tiến trình chủ rồi thoát).
///
/// Không có `wt.exe` (Windows 10 chưa cài Windows Terminal) ⟹ mở console cổ
/// điển bằng `cmd /c start`.
pub fn open_window(script: &str) -> Result<(i64, String)> {
    let nonce = format!(
        "{:x}{:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let enc = win_procs::ps_encoded(&format!("{script}\n# huba {nonce}\n"));
    // `-ExecutionPolicy Bypass` CHỈ cho tiến trình này: bản `claude` cài bằng npm
    // là một shim `claude.ps1`, và chính sách mặc định của Windows (`Restricted`)
    // chặn mọi `.ps1` — cửa sổ sẽ mở ra với một dòng đỏ thay vì một phiên.
    let ps_args = [
        "-NoLogo",
        "-NoExit",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        enc.as_str(),
    ];
    let via_wt = std::process::Command::new("wt.exe")
        .args(["-w", "new", "powershell.exe"])
        .args(ps_args)
        .spawn();
    let how = match via_wt {
        Ok(_) => "wt",
        Err(e) => {
            crate::logging::warn(
                "windows_wt_unavailable",
                serde_json::json!({ "err": e.to_string(), "fallback": "cmd /c start powershell" }),
            );
            std::process::Command::new("cmd.exe")
                .args(["/d", "/c", "start", "", "powershell.exe"])
                .args(ps_args)
                .spawn()
                .map_err(|e| anyhow!("không mở được cả wt.exe lẫn console cổ điển: {e}"))?;
            "conhost"
        }
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
        let rows = process_table()?;
        if let Some(r) = rows
            .iter()
            .find(|r| r.exe.eq_ignore_ascii_case("powershell.exe") && r.cmd.contains(&enc))
        {
            crate::logging::info(
                "windows_console_opened",
                serde_json::json!({ "pid": r.pid, "via": how }),
            );
            return Ok((r.pid, win_procs::tty_of_root(r.pid)));
        }
    }
    Err(anyhow!(
        "đã gọi {how} nhưng không thấy powershell.exe mang đúng kịch bản sau 10 giây"
    ))
}

// ── Tự kiểm trên máy Windows thật ──────────────────────────────────────────

/// Kết quả một mục tự kiểm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ket {
    Dat(String),
    Hong(String),
    /// Không đo được — KHÔNG phải đạt (luật 13②).
    Kdd(String),
}

/// Chạy từng mục đo; gọi từ một tiến trình KHÔNG có console (xem `main.rs`
/// `windows-tu-kiem`: lượt chạy tách rời bằng `DETACHED_PROCESS`).
pub fn tu_kiem() -> Vec<(String, Ket)> {
    let mut out: Vec<(String, Ket)> = Vec::new();

    // ① Bảng tiến trình đọc được, và có chính mình với dòng lệnh đúng.
    match process_table() {
        Ok(rows) => {
            let me = std::process::id() as i64;
            let mine = rows.iter().find(|r| r.pid == me);
            out.push((
                "① bảng tiến trình".into(),
                match mine {
                    Some(r) if r.cmd.contains("windows-tu-kiem") => Ket::Dat(format!(
                        "{} tiến trình, đọc được dòng lệnh của chính mình",
                        rows.len()
                    )),
                    Some(r) => Ket::Hong(format!("thấy pid mình nhưng dòng lệnh lạ: {:?}", r.cmd)),
                    None => Ket::Hong(format!("{} tiến trình nhưng KHÔNG có pid {me}", rows.len())),
                },
            ));
            let claudes: Vec<String> = rows
                .iter()
                .filter(|r| {
                    win_procs::is_console_client(&r.exe)
                        && crate::sessions::is_claude_process(if r.cmd.is_empty() {
                            &r.exe
                        } else {
                            &r.cmd
                        })
                })
                .map(|r| {
                    let by: std::collections::HashMap<i64, &Row> =
                        rows.iter().map(|x| (x.pid, x)).collect();
                    let root = win_procs::console_root(&by, r.pid);
                    format!(
                        "pid {} ({}) gốc {} [{}] cửa sổ={}",
                        r.pid,
                        r.exe,
                        root,
                        by.get(&root).map(|x| x.exe.as_str()).unwrap_or("?"),
                        has_console_window(r.pid)
                    )
                })
                .collect();
            out.push((
                "② phiên claude đang chạy".into(),
                if claudes.is_empty() {
                    Ket::Kdd(
                        "không có phiên claude nào đang chạy — mở một phiên rồi chạy lại để đo"
                            .into(),
                    )
                } else {
                    Ket::Dat(claudes.join(" · "))
                },
            ));
        }
        Err(e) => out.push(("① bảng tiến trình".into(), Ket::Hong(e.to_string()))),
    }

    // ③ Console cổ điển: mở, gõ mốc, đọc lại.
    out.push((
        "③ console cổ điển: gõ + đọc".into(),
        thu_console(|| {
            let child = std::process::Command::new("powershell.exe")
                .args(["-NoLogo", "-NoProfile", "-NoExit"])
                .creation_flags_new_console()
                .spawn()
                .map_err(|e| anyhow!("không mở được powershell: {e}"))?;
            Ok(child.id() as i64)
        }),
    ));

    // ④ Windows Terminal (ConPTY): đúng đường `/new` dùng.
    out.push((
        "④ Windows Terminal (ConPTY) qua open_window: gõ + đọc".into(),
        thu_console(|| {
            let (pid, _) = open_window("Write-Output 'huba-san-sang'")?;
            Ok(pid)
        }),
    ));

    // ⑤ claude chạy được, và tìm thấy bằng đường nào.
    out.push((
        "⑤ claude --version".into(),
        match crate::exec::run(
            "claude",
            &["--version"],
            crate::exec::RunOpts {
                timeout: Some(Duration::from_secs(30)),
                ..Default::default()
            },
        ) {
            Ok(r) if r.code == Some(0) => Ket::Dat(r.stdout.trim().to_string()),
            Ok(r) => Ket::Hong(format!("thoát {:?}: {}", r.code, r.stderr.trim())),
            Err(e) => Ket::Hong(e.to_string()),
        },
    ));

    // ⑥ Tên thư mục nhật ký khớp luật `transcript_slug`.
    out.push(("⑥ thư mục nhật ký ~/.claude/projects".into(), kiem_slug()));
    out
}

trait NewConsole {
    fn creation_flags_new_console(&mut self) -> &mut Self;
}

impl NewConsole for std::process::Command {
    fn creation_flags_new_console(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        self.creation_flags(CREATE_NEW_CONSOLE)
    }
}

/// Đóng cửa sổ thử trên một nhánh HỎNG — và nói ra nếu đóng cũng hỏng: một cửa
/// sổ thử nằm lại trên màn là thứ người chạy tự kiểm phải biết.
fn dong_thu(pid: i64) -> String {
    match close_window(pid) {
        Ok(_) => String::new(),
        Err(e) => format!(" · đóng cửa sổ thử cũng HỎNG: {e}"),
    }
}

/// Mở một console bằng `mo`, rồi đo đủ bốn việc huba làm với console của một
/// phiên: có cửa sổ · đọc được màn · gõ `echo <mốc>` + Enter rồi thấy mốc
/// đứng RIÊNG một dòng (dòng kết quả, khác dòng lệnh vừa gõ) · đóng được.
fn thu_console(mo: impl FnOnce() -> Result<i64>) -> Ket {
    let pid = match mo() {
        Ok(p) => p,
        Err(e) => return Ket::Kdd(format!("không mở được console để đo: {e}")),
    };
    let mut ghi = Vec::new();
    // Chờ shell dựng xong dấu nhắc.
    let mut man = String::new();
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(250));
        if let Ok(s) = screen_text(pid) {
            if s.contains("PS ") {
                man = s;
                break;
            }
        }
    }
    if man.is_empty() {
        let co_cua_so = has_console_window(pid);
        let dong = dong_thu(pid);
        return Ket::Hong(format!(
            "console pid {pid}: không đọc được dấu nhắc PowerShell sau 10 giây (có cửa sổ: {co_cua_so}){dong}"
        ));
    }
    ghi.push(format!("cửa sổ={}", has_console_window(pid)));
    if let Ok((h, c)) = window_size(pid) {
        ghi.push(format!("cỡ {h}×{c}"));
    }
    let moc = format!("HUBA{:x}", std::process::id() ^ 0x5a5a);
    let go = type_into(pid, &format!("echo {moc}"))
        .and_then(|_| press_writes(pid, &[vec!["enter".to_string()]]));
    if let Err(e) = go {
        let dong = dong_thu(pid);
        return Ket::Hong(format!("gõ hỏng: {e}{dong}"));
    }
    let mut thay = false;
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(250));
        if let Ok(s) = screen_text(pid) {
            if s.lines().any(|l| l.trim() == moc) {
                thay = true;
                break;
            }
        }
    }
    if !thay {
        let dong = dong_thu(pid);
        return Ket::Hong(format!(
            "gõ xong nhưng KHÔNG thấy dòng kết quả `{moc}` trên màn ({}){dong}",
            ghi.join(", ")
        ));
    }
    ghi.push("gõ + Enter + đọc lại: thấy mốc".into());
    match close_window(pid) {
        Ok(Closed::Gone) => ghi.push("đóng: shell đã thoát".into()),
        Ok(Closed::Hidden) => ghi.push("đóng: chỉ ẩn".into()),
        Err(e) => return Ket::Hong(format!("{} — nhưng ĐÓNG hỏng: {e}", ghi.join(", "))),
    }
    Ket::Dat(ghi.join(", "))
}

/// So tên thư mục nhật ký có thật với `transcript_slug(cwd)` — nhật ký nào mang
/// `cwd` thì đối chiếu được.
fn kiem_slug() -> Ket {
    let Some(home) = crate::config::home_dir() else {
        return Ket::Kdd("không biết thư mục nhà".into());
    };
    let root = home.join(".claude").join("projects");
    let Ok(dirs) = std::fs::read_dir(&root) else {
        return Ket::Kdd(format!("không có {}", root.display()));
    };
    let (mut khop, mut lech, mut vi_du) = (0usize, 0usize, Vec::new());
    for d in dirs.flatten().take(40) {
        let Some(f) = std::fs::read_dir(d.path()).ok().and_then(|mut it| {
            it.find_map(|e| {
                let p = e.ok()?.path();
                (p.extension().is_some_and(|x| x == "jsonl")).then_some(p)
            })
        }) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        let Some(cwd) = text.lines().find_map(|l| {
            serde_json::from_str::<serde_json::Value>(l)
                .ok()?
                .get("cwd")?
                .as_str()
                .map(str::to_string)
        }) else {
            continue;
        };
        let name = d.file_name().to_string_lossy().to_string();
        if crate::sessions::transcript_slug(&cwd) == name {
            khop += 1;
        } else {
            lech += 1;
            if vi_du.len() < 3 {
                vi_du.push(format!("{cwd} → {name}"));
            }
        }
    }
    match (khop, lech) {
        (0, 0) => Ket::Kdd("không có nhật ký nào mang cwd để đối chiếu".into()),
        (_, 0) => Ket::Dat(format!("{khop}/{khop} thư mục khớp luật")),
        _ => Ket::Hong(format!("{khop} khớp, {lech} lệch: {}", vi_du.join(" | "))),
    }
}
