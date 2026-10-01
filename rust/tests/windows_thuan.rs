//! Bản Windows — phần THUẦN (không gọi hệ điều hành), kiểm được trên mọi nền.
//!
//! Phần gọi Win32 (`keys_win.rs`) chỉ đo được trên máy Windows thật, bằng
//! `huba windows-tu-kiem`. Ở đây khoá những quyết định đứng TRÊN dữ liệu: bảng
//! tiến trình ⟹ console nào chạy phiên nào, tên "tty" `con<pid>`, và dòng lệnh
//! PowerShell mở phiên — chỗ một lỗi nhỏ biến thành "mọi phiên đã tắt" hay một
//! cửa sổ mở ra với dòng đỏ.

use std::collections::HashMap;
use std::path::Path;

use huba::keys::Tab;
use huba::sessions::{classify_host, is_claude_process, ps_rows_from_windows, terminal_script_ps};
use huba::win_procs::{
    console_root, consoles, parse_tty, process_count, ps51_native_arg, ps_arg, ps_encoded, Row,
};

fn row(pid: i64, ppid: i64, exe: &str, cmd: &str) -> Row {
    Row {
        pid,
        ppid,
        exe: exe.to_string(),
        cmd: cmd.to_string(),
    }
}

/// Một máy Windows tiêu biểu — mỗi nhóm là một hình dạng phải đọc đúng.
fn bang() -> Vec<Row> {
    vec![
        row(900, 4, "winlogon.exe", ""),
        row(1000, 900, "explorer.exe", ""),
        // Windows Terminal: MỘT tiến trình cho mọi cửa sổ; mỗi tab một OpenConsole.
        row(2000, 1000, "WindowsTerminal.exe", ""),
        row(2100, 2000, "OpenConsole.exe", ""),
        // Tab 1 — cửa sổ huba mở (`/new`): powershell → claude.exe → Git Bash.
        row(
            2200,
            2000,
            "powershell.exe",
            "powershell.exe -NoLogo -NoExit -ExecutionPolicy Bypass -EncodedCommand QQA=",
        ),
        row(
            2300,
            2200,
            "claude.exe",
            r#""C:\Users\ha\.local\bin\claude.exe" --permission-mode auto --disallowedTools "Bash(git push:*)""#,
        ),
        row(
            2400,
            2300,
            "bash.exe",
            "bash.exe -c source C:/Users/ha/.claude/shell-snapshots/snapshot-bash-1.sh",
        ),
        // Tab 2 — shell trần, chưa chạy gì.
        row(2500, 2000, "OpenConsole.exe", ""),
        row(2600, 2000, "pwsh.exe", "pwsh.exe -NoLogo"),
        // Console cổ điển mở từ Start, claude cài bằng npm: cmd → cmd /c claude.cmd → node cli.js.
        row(3000, 1000, "cmd.exe", r"C:\WINDOWS\system32\cmd.exe"),
        row(3050, 3000, "conhost.exe", ""),
        row(
            3100,
            3000,
            "cmd.exe",
            r#"C:\WINDOWS\system32\cmd.exe /c ""C:\Users\ha\AppData\Roaming\npm\claude.cmd" ""#,
        ),
        row(
            3200,
            3100,
            "node.exe",
            r#"node "C:\Users\ha\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\cli.js""#,
        ),
        // Phép dò hạn mức của chính hubd — CREATE_NO_WINDOW, không cửa sổ.
        row(4000, 900, "hubd.exe", ""),
        row(
            4100,
            4000,
            "claude.exe",
            "claude -p /usage --output-format json",
        ),
        // claude của tiện ích VS Code.
        row(5000, 1000, "Code.exe", ""),
        row(
            5100,
            5000,
            "claude.exe",
            r"C:\Users\ha\.vscode\extensions\anthropic.claude-code-2.1.0\resources\native-binary\claude.exe",
        ),
        // Bảng hỏng: ppid trỏ vòng (pid dùng lại) — phải dừng, không lặp mãi.
        row(6000, 6001, "node.exe", ""),
        row(6001, 6000, "node.exe", ""),
    ]
}

fn by_pid(rows: &[Row]) -> HashMap<i64, &Row> {
    rows.iter().map(|r| (r.pid, r)).collect()
}

#[test]
fn shell_goc_cua_console() {
    let rows = bang();
    let by = by_pid(&rows);
    assert_eq!(
        console_root(&by, 2300),
        2200,
        "claude.exe trong tab WT ⟹ powershell"
    );
    assert_eq!(
        console_root(&by, 2400),
        2200,
        "Git Bash con của claude ⟹ cùng tab"
    );
    assert_eq!(
        console_root(&by, 3200),
        3000,
        "node (npm) ⟹ cmd gốc của console cổ điển"
    );
    assert_eq!(
        console_root(&by, 4100),
        4100,
        "cha là hubd.exe, không phải shell ⟹ tự nó là gốc"
    );
    assert_eq!(console_root(&by, 2600), 2600);
    // Vòng ppid: phải trả về (bất kỳ đâu trong vòng), không treo.
    let r = console_root(&by, 6000);
    assert!(r == 6000 || r == 6001);
}

#[test]
fn gom_console_dung_tab() {
    let rows = bang();
    let cs = consoles(&rows);
    let get = |root: i64| cs.iter().find(|c| c.root == root).cloned();

    let tab1 = get(2200).expect("tab của phiên huba mở");
    assert_eq!(tab1.procs, vec!["powershell", "claude", "bash"]);
    assert!(tab1.busy);

    let tran = get(2600).expect("shell trần của tab 2 cũng phải được liệt kê (/terminal)");
    assert_eq!(tran.procs, vec!["pwsh"]);
    assert!(!tran.busy, "shell trần là RẢNH");

    let npm = get(3000).expect("console cổ điển chạy claude npm");
    assert!(npm.procs.contains(&"claude".to_string()));
    assert!(
        !npm.procs.contains(&"conhost".to_string()),
        "chủ console không phải 'đang chạy'"
    );
    assert!(npm.busy);

    // Gốc của MỌI tiến trình claude đều có mặt (lọc "có cửa sổ" là việc của API).
    assert!(get(4100).is_some());
    // Không gốc nào lặp.
    let mut roots: Vec<i64> = cs.iter().map(|c| c.root).collect();
    roots.dedup();
    assert_eq!(roots.len(), cs.len());
}

/// `Tab::cli` phải coi shell của Windows là shell — nếu không, một tab
/// `powershell` trần đọc ra "đang chạy CLI powershell".
#[test]
fn tab_windows_nhan_dung_cli() {
    let tab = |procs: &[&str]| Tab {
        tty: "con1".into(),
        busy: false,
        procs: procs.iter().map(|s| s.to_string()).collect(),
        screen: None,
        title: String::new(),
    };
    assert!(tab(&["powershell", "claude", "bash"]).is_claude());
    assert_eq!(tab(&["pwsh"]).cli(), None);
    assert_eq!(tab(&["cmd"]).cli(), None);
    assert_eq!(tab(&["powershell", "git"]).cli(), Some("git"));
}

#[test]
fn tty_cua_tien_trinh_claude() {
    let rows = bang();
    // Phép dò của hubd (4100) và claude của VS Code (5100): console KHÔNG có cửa sổ.
    let co_cua_so = |pid: i64| !matches!(pid, 4100 | 5100);
    let ps: HashMap<i64, (i64, String, String)> = ps_rows_from_windows(&rows, co_cua_so)
        .into_iter()
        .map(|(pid, ppid, tty, cmd)| (pid, (ppid, tty, cmd)))
        .collect();
    assert_eq!(ps[&2300].1, "con2200");
    assert_eq!(ps[&3200].1, "con3000");
    assert_eq!(ps[&3100].1, "con3000", "cmd /c claude.cmd cùng console");
    assert_eq!(
        ps[&4100].1, "",
        "phép dò không cửa sổ ⟹ không tty ⟹ detached"
    );
    assert_eq!(ps[&2400].1, "", "Git Bash không phải phiên");
    assert_eq!(ps[&2200].1, "", "shell không phải phiên");
    // Dòng lệnh rỗng thì rơi về tên tệp — `is_claude_process` vẫn có chữ để soi.
    assert_eq!(ps[&2100].2, "OpenConsole.exe");

    // Nối với `classify_host` như `Procs::host_of` làm.
    assert_eq!(
        classify_host(&ps[&2300].2, "terminal", &ps[&2300].1),
        "terminal"
    );
    assert_eq!(
        classify_host(&ps[&4100].2, "terminal", &ps[&4100].1),
        "detached"
    );
    assert_eq!(classify_host(&ps[&5100].2, "terminal", ""), "editor");
}

#[test]
fn nhan_ra_claude_tren_duong_dan_windows() {
    assert!(is_claude_process(
        r#""C:\Users\ha\.local\bin\claude.exe" --resume x"#
    ));
    assert!(is_claude_process(r"C:\Users\ha\.local\bin\claude.exe"));
    assert!(is_claude_process(
        r#"node "C:\Users\ha\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\cli.js""#
    ));
    assert!(!is_claude_process(r"C:\Windows\System32\notepad.exe"));
    assert!(!is_claude_process("powershell.exe -NoLogo"));
}

#[test]
fn so_tien_trinh_trong_console() {
    let rows = bang();
    assert_eq!(process_count(&rows, 2200), Some(3));
    assert_eq!(process_count(&rows, 2600), Some(1));
    assert_eq!(process_count(&rows, 3000), Some(3), "conhost không tính");
    assert_eq!(
        process_count(&rows, 9999),
        None,
        "gốc không còn ⟹ None (console đã HẾT)"
    );
}

#[test]
fn ten_tty_windows() {
    assert_eq!(parse_tty("con2200"), Some(2200));
    assert_eq!(parse_tty(" con2200 "), Some(2200));
    assert_eq!(parse_tty("ttys004"), None);
    assert_eq!(
        parse_tty("123456"),
        None,
        "HWND của bản cũ trong sổ — không đoán"
    );
    assert_eq!(parse_tty("con0"), None);
    assert_eq!(parse_tty("con"), None);
}

/// Bộ tách đối số theo luật `CommandLineToArgvW` (msvcrt) — đủ cho phép thử trọn
/// vòng: `\` trước `"` thì đếm cặp, `"` lẻ là nháy chữ.
fn msvc_parse_one(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let (mut i, mut in_q) = (0usize, false);
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let mut n = 0;
            while i < chars.len() && chars[i] == '\\' {
                n += 1;
                i += 1;
            }
            if i < chars.len() && chars[i] == '"' {
                out.extend(std::iter::repeat_n('\\', n / 2));
                if n % 2 == 1 {
                    out.push('"');
                    i += 1;
                }
            } else {
                out.extend(std::iter::repeat_n('\\', n));
            }
            continue;
        }
        if c == '"' {
            in_q = !in_q;
            i += 1;
            continue;
        }
        let _ = in_q;
        out.push(c);
        i += 1;
    }
    out
}

/// Cách Windows PowerShell 5.1 dựng đối số cho chương trình ngoài: bọc `"…"`
/// khi có khoảng trắng (hay rỗng), KHÔNG thoát `"` bên trong.
fn ps51_pass(v: &str) -> String {
    if v.is_empty() || v.chars().any(char::is_whitespace) {
        format!("\"{v}\"")
    } else {
        v.to_string()
    }
}

#[test]
fn doi_so_qua_powershell_51_den_nguyen_ven() {
    let ca = [
        "Bash(git push:*)",
        "Bash(rm:*)",
        r#"anh nói "xong rồi" nhé"#,
        r"C:\dir\",
        r"C:\my dir\",
        r#"a\"b"#,
        r#"đường "C:\x\" cuối"#,
        "nhiều\ndòng \"trích\" và \\\\ hai gạch",
        "",
    ];
    for s in ca {
        let qua = ps51_pass(&ps51_native_arg(s));
        assert_eq!(
            msvc_parse_one(&qua),
            s,
            "đối số {s:?} vỡ khi qua PowerShell 5.1: {qua}"
        );
    }
    // Đối chứng ngược: để TRẦN (không thoát) thì ca có `"` phải vỡ — chứng minh
    // phép thử trên đỏ được.
    let tran = ps51_pass(r#"anh nói "xong rồi" nhé"#);
    assert_ne!(msvc_parse_one(&tran), r#"anh nói "xong rồi" nhé"#);
    // Lớp nháy đơn của PowerShell bên ngoài: `'` ⟹ `''`.
    assert_eq!(ps_arg("it's"), "'it''s'");
}

#[test]
fn encoded_command_la_utf16le_base64() {
    // "A" = 41 00 ⟹ "QQA="
    assert_eq!(ps_encoded("A"), "QQA=");
    // Chữ Việt đi trọn vòng.
    use base64::Engine;
    let b = base64::engine::general_purpose::STANDARD
        .decode(ps_encoded("Set-Location 'dự án'; & 'claude'"))
        .unwrap();
    let units: Vec<u16> = b
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    assert_eq!(
        String::from_utf16(&units).unwrap(),
        "Set-Location 'dự án'; & 'claude'"
    );
}

#[test]
fn kich_ban_powershell_mo_phien() {
    let s = terminal_script_ps(
        "& 'claude'",
        Path::new(r"C:\Users\ha\projects"),
        Some(r#"bàn giao "gấp""#),
        Some("abcd-1234"),
    );
    let mut dong = s.lines();
    assert_eq!(
        dong.next(),
        Some(r"Set-Location -LiteralPath 'C:\Users\ha\projects'")
    );
    let lenh = dong.next().unwrap();
    assert!(lenh.starts_with("& 'claude' --permission-mode auto --resume 'abcd-1234' "));
    // Đề bài đứng TRƯỚC --disallowedTools (cờ variadic — luật 10), và đã thoát `"`.
    let de = lenh
        .find(r#"'bàn giao \"gấp\"'"#)
        .expect("đề bài đã thoát dấu nháy kép");
    let cam = lenh.find("--disallowedTools").unwrap();
    assert!(de < cam);
    assert!(
        lenh.contains("'Bash(git push:*)'"),
        "mọi mẫu công cụ được bọc nháy"
    );
    assert!(!s.contains("&&"), "&& là lỗi cú pháp của PowerShell 5.1");
}
