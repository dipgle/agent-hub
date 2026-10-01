//! Windows: từ BẢNG TIẾN TRÌNH ra "console nào chạy phiên nào" — phần THUẦN.
//!
//! Không gọi hệ điều hành (phần ấy ở `keys_win.rs`), nên biên dịch và KIỂM ĐƯỢC
//! trên mọi nền — kể cả trên chiếc Mac viết ra nó, nơi không có máy Windows nào.
//!
//! # Vì sao "tty" trên Windows là `con<pid>`, và pid ấy là của AI
//!
//! macOS nối phiên với cửa sổ bằng tty (`ps` in sẵn). Windows không có tty, và
//! Windows Terminal dùng **MỘT tiến trình cho MỌI cửa sổ** — nên leo cây tiến
//! trình lên tới `WindowsTerminal.exe` không nói được phiên nằm ở cửa sổ nào.
//! Thứ PHÂN BIỆT được là CONSOLE: mỗi tab (và mỗi cửa sổ console cổ điển) là một
//! console riêng, và huba gõ/đọc thẳng vào console ấy (`AttachConsole` +
//! `WriteConsoleInputW`/`ReadConsoleOutputCharacterW`, xem `keys_win.rs`) — không
//! cần cửa sổ, không cần tiêu điểm.
//!
//! Tên của một console = pid của **shell gốc** của nó: leo từ tiến trình lên
//! chừng nào cha còn là một tiến trình CONSOLE ([`CONSOLE_CLIENTS`] — shell,
//! `node`, `claude`); dừng ở cha đầu tiên KHÔNG phải (`WindowsTerminal.exe`,
//! `explorer.exe`, `Code.exe`, hoặc cha đã chết). Mọi tiến trình trong cùng một
//! tab leo về cùng một gốc ⟹ cùng tên — đúng vai trò của tty. Và phiên `claude`
//! thứ hai gõ lại trong cùng tab vẫn mang tên cũ, nên phép "cửa sổ ấy nay chạy
//! phiên khác" (`window_taken_over`) vẫn đúng.
//!
//! ⚠ CHƯA ĐO trên máy Windows thật: tên tiến trình của `claude` (bản cài gốc
//! `claude.exe`, bản npm `node.exe …\claude-code\cli.js`) đúng theo tài liệu
//! công khai; `huba windows-tu-kiem` in ra bảng thật để đối chiếu.

use std::collections::{HashMap, HashSet};

/// Một hàng bảng tiến trình Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub pid: i64,
    pub ppid: i64,
    /// Tên tệp chạy, như ToolHelp trả: `powershell.exe`.
    pub exe: String,
    /// Dòng lệnh đầy đủ; rỗng khi không đọc được (tiến trình của người dùng
    /// khác/được bảo vệ) — khi ấy chỗ gọi rơi về `exe`.
    pub cmd: String,
}

/// Tiến trình "sống TRONG một console" — leo cây qua chúng mới tới shell gốc.
pub const CONSOLE_CLIENTS: &[&str] = &[
    "powershell.exe",
    "pwsh.exe",
    "cmd.exe",
    "bash.exe",
    "sh.exe",
    "zsh.exe",
    "fish.exe",
    "nu.exe",
    "node.exe",
    "claude.exe",
];

/// Cha của một shell "người dùng tự mở": một tab Windows Terminal, một cửa sổ
/// console mở từ Start/Explorer, hay terminal tích hợp của trình soạn thảo.
pub const TERMINAL_HOSTS: &[&str] = &[
    "windowsterminal.exe",
    "explorer.exe",
    "code.exe",
    "cursor.exe",
];

/// Tiến trình chủ console — mọc kèm mọi console, KHÔNG phải "đang chạy việc".
const CONSOLE_HOSTS: &[&str] = &["conhost.exe", "openconsole.exe"];

fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

pub fn is_console_client(exe: &str) -> bool {
    CONSOLE_CLIENTS.contains(&lower(exe).as_str())
}

/// `con12345` — xem đầu tệp.
pub fn tty_of_root(root: i64) -> String {
    format!("con{root}")
}

/// `con12345` → `12345`. Chuỗi khác (tty macOS, HWND cũ trong sổ) ⟹ `None`.
pub fn parse_tty(tty: &str) -> Option<i64> {
    tty.trim()
        .strip_prefix("con")
        .and_then(|n| n.parse::<i64>().ok())
        .filter(|n| *n > 0)
}

/// Shell gốc của console chứa `pid` — xem đầu tệp. Có TRẦN 64 bậc: một bảng
/// tiến trình có `ppid` trỏ vòng (pid được dùng lại) không được thành vòng lặp
/// vô tận.
pub fn console_root(by_pid: &HashMap<i64, &Row>, pid: i64) -> i64 {
    let mut cur = pid;
    for _ in 0..64 {
        let Some(row) = by_pid.get(&cur) else {
            return cur;
        };
        match by_pid.get(&row.ppid) {
            Some(parent) if parent.pid != cur && is_console_client(&parent.exe) => {
                cur = parent.pid;
            }
            _ => return cur,
        }
    }
    cur
}

/// Tên hiện cho một tiến trình trong danh sách `Tab::procs`: bỏ `.exe`, chữ
/// thường; `node.exe` chạy CLI thì gọi đúng tên `claude` (`Tab::is_claude` so
/// tên ấy, và bản npm KHÔNG mang chữ `claude` trong tên tệp chạy).
pub fn display_name(row: &Row) -> String {
    let cmd = if row.cmd.is_empty() {
        &row.exe
    } else {
        &row.cmd
    };
    if crate::sessions::is_claude_process(cmd) {
        return "claude".to_string();
    }
    let e = lower(&row.exe);
    e.strip_suffix(".exe").unwrap_or(&e).to_string()
}

/// Một console đang mở, gom từ bảng tiến trình.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Console {
    pub root: i64,
    /// Tên các tiến trình trong console, gốc đứng đầu (`powershell`, `claude`…).
    pub procs: Vec<String>,
    /// Có tiến trình nào ngoài shell gốc (và chủ console) đang chạy không —
    /// đúng nghĩa `busy of tab` của Terminal.app.
    pub busy: bool,
}

/// Mọi console ĐÁNG LIỆT KÊ: gốc của mọi tiến trình `claude`, cộng mọi shell mà
/// cha là một [`TERMINAL_HOSTS`] (tab/cửa sổ người dùng tự mở — để `/terminal`
/// thấy được cả cửa sổ chưa chạy CLI, như trên macOS).
///
/// Chưa lọc "console có cửa sổ thật không" — câu ấy chỉ hỏi được bằng API
/// (`keys_win::has_console_window`), và chỗ gọi tự hỏi.
pub fn consoles(rows: &[Row]) -> Vec<Console> {
    let by_pid: HashMap<i64, &Row> = rows.iter().map(|r| (r.pid, r)).collect();
    let mut roots: Vec<i64> = Vec::new();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut push = |root: i64, roots: &mut Vec<i64>| {
        if seen.insert(root) {
            roots.push(root);
        }
    };
    for r in rows {
        let cmd = if r.cmd.is_empty() { &r.exe } else { &r.cmd };
        if crate::sessions::is_claude_process(cmd) && is_console_client(&r.exe) {
            push(console_root(&by_pid, r.pid), &mut roots);
        }
    }
    for r in rows {
        if !is_console_client(&r.exe)
            || lower(&r.exe) == "node.exe"
            || lower(&r.exe) == "claude.exe"
        {
            continue;
        }
        let parent_is_host = by_pid
            .get(&r.ppid)
            .is_some_and(|p| TERMINAL_HOSTS.contains(&lower(&p.exe).as_str()));
        if parent_is_host {
            push(r.pid, &mut roots);
        }
    }
    // Con cháu của từng gốc, theo bảng `ppid` — đi XUỐNG từ gốc.
    let mut children: HashMap<i64, Vec<&Row>> = HashMap::new();
    for r in rows {
        if r.ppid != r.pid {
            children.entry(r.ppid).or_default().push(r);
        }
    }
    roots
        .into_iter()
        .filter_map(|root| {
            let row = by_pid.get(&root)?;
            let mut procs = vec![display_name(row)];
            let mut busy = false;
            let mut stack: Vec<i64> = vec![root];
            let mut visited: HashSet<i64> = HashSet::from([root]);
            while let Some(p) = stack.pop() {
                for c in children.get(&p).map(Vec::as_slice).unwrap_or(&[]) {
                    if !visited.insert(c.pid) {
                        continue;
                    }
                    if CONSOLE_HOSTS.contains(&lower(&c.exe).as_str()) {
                        continue;
                    }
                    busy = true;
                    let name = display_name(c);
                    if !procs.contains(&name) {
                        procs.push(name);
                    }
                    stack.push(c.pid);
                }
            }
            Some(Console { root, procs, busy })
        })
        .collect()
}

/// Số tiến trình trong console của `root` (gốc + con cháu, trừ chủ console) —
/// cùng nghĩa `tab_process_count` của macOS. `None` = gốc không còn.
pub fn process_count(rows: &[Row], root: i64) -> Option<usize> {
    let by_pid: HashMap<i64, &Row> = rows.iter().map(|r| (r.pid, r)).collect();
    by_pid.get(&root)?;
    let mut n = 1usize;
    let mut stack = vec![root];
    let mut visited: HashSet<i64> = HashSet::from([root]);
    while let Some(p) = stack.pop() {
        for r in rows.iter().filter(|r| r.ppid == p && r.pid != p) {
            if visited.insert(r.pid) && !CONSOLE_HOSTS.contains(&lower(&r.exe).as_str()) {
                n += 1;
                stack.push(r.pid);
            }
        }
    }
    Some(n)
}

/// Bọc một chuỗi thành chuỗi nháy đơn của PowerShell: `'` ⟹ `''`.
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Giá trị đối số để Windows PowerShell **5.1** truyền nguyên vẹn sang một
/// chương trình ngoài (`claude.exe`).
///
/// 5.1 KHÔNG tự thoát dấu `"` khi dựng dòng lệnh cho chương trình ngoài — nó
/// chỉ bọc cả đối số trong `"…"` khi đối số có khoảng trắng (hay rỗng). Nên một
/// đề bài chứa `"` tới tay `claude` sẽ vỡ đôi. Thoát trước theo luật
/// `CommandLineToArgvW`: `\` đứng ngay trước `"` thì nhân đôi, `"` ⟹ `\"`, và
/// khi 5.1 sẽ bọc nháy thì `\` ở CUỐI cũng nhân đôi. huba luôn mở `powershell`
/// (5.1), không mở `pwsh` — bản 7.3+ tự thoát và sẽ thoát CHỒNG lên bản này.
pub fn ps51_native_arg(s: &str) -> String {
    let wraps = s.is_empty() || s.chars().any(char::is_whitespace);
    let mut out = String::with_capacity(s.len() + 8);
    let mut backslashes = 0usize;
    for c in s.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    let tail = if wraps { backslashes * 2 } else { backslashes };
    out.extend(std::iter::repeat_n('\\', tail));
    out
}

/// Một đối số cho chương trình ngoài, viết thành chữ PowerShell.
pub fn ps_arg(s: &str) -> String {
    ps_quote(&ps51_native_arg(s))
}

/// `-EncodedCommand`: base64 của UTF-16LE. Đi qua `wt.exe` mà không vướng `;`
/// (dấu tách lệnh của chính `wt.exe`) hay bất kỳ ký tự nào cần thoát.
pub fn ps_encoded(script: &str) -> String {
    use base64::Engine;
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
