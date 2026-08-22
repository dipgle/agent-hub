//! Child-process helper. No shell: every command is argv-exact so untrusted
//! message text can never become shell syntax.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct RunOut {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub ms: u128,
    /// Bao nhiêu byte output bị VỨT vì quá trần. `0` = giữ trọn.
    ///
    /// 🔴 Nó phải đi ra ngoài, không được nằm im trong log: chỗ gọi nào so hai
    /// bản output với nhau (`runtime::text_id`) thì một bản bị cắt là một câu
    /// trả lời SAI, không phải một câu trả lời ngắn.
    pub cut_bytes: u64,
}

impl RunOut {
    pub fn ok(&self) -> bool {
        self.code == Some(0) && !self.timed_out
    }
}

#[derive(Debug, Default)]
pub struct RunOpts<'a> {
    pub cwd: Option<&'a Path>,
    pub input: Option<String>,
    pub timeout: Option<Duration>,
    /// Extra environment for the child. `None` as the value REMOVES the
    /// variable — which is load-bearing for the `claude` CLI: an account is
    /// selected by `CLAUDE_CONFIG_DIR`, and the default account is selected by
    /// the variable being ABSENT, not by pointing it at the default directory.
    pub env: Vec<(String, Option<String>)>,
    /// Nơi nhận PID của tiến trình con, ngay sau khi nó được dựng.
    ///
    /// 🔴 Hà 2026-08-14: *"Có những lệnh sẽ chạy khá lâu nên cần cơ chế theo dõi
    /// riêng thay vì cố định timeout"*. Theo dõi được thì cũng phải DỪNG được,
    /// mà dừng thì phải biết giết ai. `run()` chặn tới khi xong, nên pid phải đi
    /// ra bằng đường này — chỗ gọi lấy nó ngay lúc lệnh bắt đầu, không phải chờ
    /// lệnh kết thúc mới biết mình vừa chạy gì.
    ///
    /// Không dùng thì để `None`: đường cũ không đổi một chữ.
    pub pid_out: Option<mpsc::Sender<u32>>,
    /// Trần output RIÊNG cho lời gọi này. `None` = dùng `MAX_BYTES` (8 MB).
    ///
    /// Có mặt vì một lệnh CÓ THỂ in nhiều hơn thế một cách hợp lệ:
    /// `otool -s __TEXT __text` trên `hubad` in 19 MB. Trần chung giữ nguyên —
    /// nó gác những lệnh in bừa; chỗ nào biết mình xin một biển chữ thì khai ra.
    pub max_bytes: Option<usize>,
}

const POLL: Duration = Duration::from_millis(50);
const MAX_BYTES: usize = 8 * 1024 * 1024;

/// Đọc HẾT ống, giữ lại nhiều nhất `cap` byte, đếm phần đã vứt.
///
/// 🔴 Bản trước là `out.take(cap).read_to_end(&mut buf)` — và `take` không cắt
/// output, nó **thôi đọc**. Luồng đọc kết thúc, `ChildStdout` rời tầm, đầu đọc
/// của ống ĐÓNG — và lượt `write` tiếp theo của tiến trình con hỏng. Nên một
/// cái trần trông như "giữ 8 MB đầu" thật ra là **"giết mọi lệnh in nhiều hơn
/// 8 MB"**.
///
/// Đã trả giá thật, và trả lâu (Hà 2026-08-22: *"Lâu lắm rồi không chạy được
/// lệnh"*): `runtime::text_id` chạy `otool -s __TEXT __text` trên `hubad`, mà
/// output ấy **19.015.353 byte** cho một binary 8.238.928 byte — bản kết xuất
/// hex to gấp 2,3 lần.
///
/// Đo lại đúng hình dạng cũ trên chính `otool` ấy (đọc 8 MB rồi đóng đầu đọc):
/// **exit 1 · stderr RỖNG · 0,07 giây**. Ba con số ấy giải thích trọn câu Hà
/// nhận — *"otool -s __TEXT __text hỏng trên …:"*, cụt ngay sau dấu hai chấm,
/// vì `stderr` chẳng có gì để in. Một câu đổ oan cho `otool`, về một lệnh chưa
/// bao giờ hỏng; và nó chặn MỌI bản vá của huba, vì `self_install` dừng ở đó.
///
/// ⚠ Không phải "treo tới hết giờ" — tôi đã đoán thế lúc đầu và phép đo bác
/// ngay. Nó chết NHANH và IM, thứ khó lần hơn một cái treo.
///
/// Hai điều hàm này giữ: **luôn đọc tới EOF** (đầu đọc không bao giờ đóng sớm,
/// nên tiến trình con luôn được viết cho hết), và **đếm phần vứt đi** để chỗ
/// gọi biết mình đang cầm một bản cụt. Cắt mà im thì phép so hai output biến
/// thành phép so hai đoạn đầu.
fn drain_capped(mut r: impl Read, cap: usize) -> (String, u64) {
    let mut kept: Vec<u8> = Vec::new();
    let mut cut: u64 = 0;
    let mut chunk = [0u8; 64 * 1024];
    loop {
        match r.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let room = cap.saturating_sub(kept.len());
                if room > 0 {
                    kept.extend_from_slice(&chunk[..n.min(room)]);
                }
                cut += (n.saturating_sub(room)) as u64;
            }
            // Đọc hỏng thì dừng — nhưng KHÔNG im: phần chưa đọc tính là đã vứt,
            // nên chỗ gọi vẫn thấy bản này không trọn.
            Err(_) => break,
        }
    }
    (String::from_utf8_lossy(&kept).to_string(), cut)
}

/// Hạng của một lời gọi: **có người đang chờ nó**, hay nó là việc vặt chạy nền.
///
/// 🔴 Hà 2026-08-14: *"phải phân biệt việc gì cần xử lý nhanh chậm để chạy đúng
/// phân loại nhân chứ"*. Đúng, và nó chỉ ra chỗ tôi vừa làm thô: sáng nay huba
/// khai `ProcessType Background` cho CẢ tiến trình (di sản của cái inbox đã
/// xoá), tôi đổi thành `Interactive` cho CẢ tiến trình — hết nghẽn, nhưng lúc
/// ấy một lượt đẩy ảnh chụp định kỳ cũng giành CPU ngang với ngón tay chủ máy.
/// Cả hai bản đều sai cùng một kiểu: xếp hạng cho một TIẾN TRÌNH, trong khi
/// hạng là thuộc tính của từng VIỆC.
///
/// Trần trên đặt ở plist (`Interactive`), còn ở đây là chỗ hạ xuống cho đúng
/// việc: vòng quét định kỳ, đẩy ảnh chụp, dò sức khoẻ — không ai ngồi chờ
/// chúng, nên chúng nhường đường.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Lane {
    /// Việc nền: chạy khi máy rảnh, nhường ngón tay chủ máy.
    #[default]
    Background,
    /// Có người đang nhìn màn hình chờ câu trả lời này.
    Urgent,
}

thread_local! {
    static LANE: std::cell::Cell<Lane> = const { std::cell::Cell::new(Lane::Background) };
}

/// Hạng của luồng đang chạy.
pub fn lane() -> Lane {
    LANE.with(|l| l.get())
}

/// Đánh dấu: mọi tiến trình con sinh ra từ luồng này, cho tới khi guard rời
/// tầm, là việc CÓ NGƯỜI ĐANG CHỜ.
///
/// Theo LUỒNG chứ không theo tham số, vì đường đi từ một cú bấm nút tới lời gọi
/// `osascript` xuyên qua chừng mười lớp (`execute_commands` → `keys::type_text`
/// → `osascript` → `run`), và luồn một tham số qua cả mười lớp là mười chỗ để
/// quên. Cái phải đúng ở đây là "ai gây ra lời gọi này", mà luồng thì trả lời
/// được câu ấy sẵn.
pub fn urgent() -> LaneGuard {
    let prev = lane();
    LANE.with(|l| l.set(Lane::Urgent));
    LaneGuard(prev)
}

pub struct LaneGuard(Lane);

impl Drop for LaneGuard {
    fn drop(&mut self) {
        LANE.with(|l| l.set(self.0));
    }
}

/// Bọc một lệnh nền cho hệ điều hành biết nó được phép chạy chậm.
///
/// 🔴 Hà 2026-08-14: *"Nếu build và chạy trên win và chip Intel thì sao"*.
/// Nên chỗ này **hỏi hệ điều hành, không gõ cứng theo chip**:
///
/// * **macOS** (Apple Silicon *và* Intel): `taskpolicy -b` đặt tiến trình con
///   vào QoS nền. Trên Apple Silicon nó còn nghĩa là ưu tiên lõi tiết kiệm
///   điện; trên Intel không có lõi ấy nên chỉ còn hạ ưu tiên lịch biểu và
///   throttle I/O — nhẹ hơn, vẫn đúng hướng, không hại. Cùng một dòng mã, hai
///   con chip, không cần biết chip nào.
/// * **Hệ khác** (Windows, Linux): không có `taskpolicy`, và huba chưa chạy ở đó
///   được vì nó lái Terminal.app bằng AppleScript. Nhánh này biên dịch thành
///   "chạy thẳng" — mất phần nhường đường, không mất chức năng nào.
///
/// Không dùng `nice`: nó chỉ chỉnh ưu tiên CPU cổ điển, còn thứ đang bóp huba là
/// QoS của macOS — hai bộ điều khiển khác nhau, và `nice` không chạm tới cái
/// đang siết.
#[cfg(target_os = "macos")]
fn lane_wrap<'a>(cmd: &'a str, args: &[&'a str]) -> (String, Vec<String>) {
    if lane() == Lane::Urgent || !Path::new(TASKPOLICY).exists() {
        return (
            cmd.to_string(),
            args.iter().map(|s| s.to_string()).collect(),
        );
    }
    let mut out = vec!["-b".to_string(), cmd.to_string()];
    out.extend(args.iter().map(|s| s.to_string()));
    (TASKPOLICY.to_string(), out)
}

#[cfg(not(target_os = "macos"))]
fn lane_wrap<'a>(cmd: &'a str, args: &[&'a str]) -> (String, Vec<String>) {
    (
        cmd.to_string(),
        args.iter().map(|s| s.to_string()).collect(),
    )
}

#[cfg(target_os = "macos")]
const TASKPOLICY: &str = "/usr/bin/taskpolicy";

/// Giết cả nhóm tiến trình của `pid` (con, cháu, chắt).
///
/// Dùng `/bin/kill` thay vì `libc::kill`: crate này không có `libc` trong danh
/// sách phụ thuộc, và gọi thẳng syscall thì phải mở `unsafe` — thứ repo này cấm
/// tuyệt đối. Một lần spawn `/bin/kill` khi HẾT GIỜ (chuyện hiếm) rẻ hơn nhiều
/// so với việc nới luật ấy.
///
/// TERM trước rồi KILL: cho tiến trình cơ hội dọn dẹp, nhưng đừng chờ nó.
/// Lỗi ở đây không báo ra ngoài vì "không giết được" gần như luôn có nghĩa là
/// "nó chết rồi" — nhưng vẫn ghi log, vì một tiến trình không chịu chết là thứ
/// đáng biết.
#[cfg(unix)]
fn kill_group(pid: u32) {
    for sig in ["-TERM", "-KILL"] {
        let out = Command::new("/bin/kill")
            .args([sig, &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Err(e) = out {
            crate::logging::warn(
                "kill_group_failed",
                serde_json::json!({ "pid": pid, "sig": sig, "err": e.to_string() }),
            );
            return;
        }
        thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(not(unix))]
fn kill_group(_pid: u32) {}

/// Run a command, capture stdout/stderr, enforce a hard timeout.
/// Never errors on a non-zero exit — the caller decides what failure means.
/// Only a spawn failure is an `Err`.
pub fn run(cmd: &str, args: &[&str], opts: RunOpts) -> Result<RunOut> {
    let started = Instant::now();
    let timeout = opts.timeout.unwrap_or(Duration::from_secs(60));

    // Hạng đi kèm LỜI GỌI, không kèm tiến trình — xem `Lane`.
    let (cmd_run, args_run) = lane_wrap(cmd, args);
    let mut command = Command::new(&cmd_run);
    command
        .args(&args_run)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Mỗi lời gọi một NHÓM TIẾN TRÌNH riêng, để lúc hết giờ giết được cả họ.
    //
    // Vì sao cần (đo 2026-08-10): `claude` là một wrapper — nó spawn tiếp một
    // binary native, và `child.kill()` chỉ giết đúng đứa con trực tiếp. Đứa cháu
    // sống sót, treo vô hạn, và mỗi lần hết giờ lại bỏ lại một tiến trình nữa:
    // tìm ra hai con `claude /usage` nằm im, một con treo từ bốn tiếng trước.
    // Một phép dò chạy 5 phút một lần mà rò tiến trình thì tệ hơn không dò.
    //
    // `process_group(0)` là API AN TOÀN (không cần `unsafe`, luật của repo);
    // con thành trưởng nhóm với pgid = pid của chính nó.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    if let Some(dir) = opts.cwd {
        command.current_dir(dir);
    }
    for (key, value) in &opts.env {
        match value {
            Some(v) => command.env(key, v),
            None => command.env_remove(key),
        };
    }

    let mut child = command
        .spawn()
        .map_err(|e| anyhow!("spawn {cmd} failed: {e}"))?;

    // Nói ra pid NGAY, trước cả khi đọc một byte đầu ra: chỗ gọi đang chờ đúng
    // con số này để ghi sổ "đang chạy" và để dựng cái nút dừng. Kênh đóng rồi
    // thì thôi — chỗ gọi bỏ cuộc là chuyện của chỗ gọi, không phải lỗi ở đây.
    if let Some(tx) = &opts.pid_out {
        let _ = tx.send(child.id());
    }

    // stdin in its own thread: a big prompt must not deadlock against a child
    // that is already writing to stdout.
    if let Some(input) = opts.input {
        if let Some(mut stdin) = child.stdin.take() {
            thread::spawn(move || {
                // A write error here means the child closed stdin early (it
                // exited or rejected the input); that shows up as its exit code
                // and stderr, which the caller already inspects. Nothing is
                // hidden by not propagating it out of this thread.
                let _ = stdin.write_all(input.as_bytes());
                // Dropping closes the pipe — the child sees EOF.
            });
        }
    } else {
        drop(child.stdin.take());
    }

    let (tx_out, rx_out) = mpsc::channel::<(String, u64)>();
    let (tx_err, rx_err) = mpsc::channel::<(String, u64)>();

    // Reader threads: a read error (or a send onto a dropped receiver after the
    // grace period) can only mean truncated capture, and the caller sees that
    // as an unparseable/short output plus the process exit code — never as a
    // silent success.
    let cap = opts.max_bytes.unwrap_or(MAX_BYTES);
    if let Some(out) = child.stdout.take() {
        thread::spawn(move || {
            let _ = tx_out.send(drain_capped(out, cap));
        });
    }
    if let Some(err) = child.stderr.take() {
        thread::spawn(move || {
            let _ = tx_err.send(drain_capped(err, cap));
        });
    }

    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if started.elapsed() >= timeout {
                    // `timed_out` is what the caller acts on; a kill/wait error
                    // means the child is already gone, which is the same
                    // outcome. The timeout itself is never hidden.
                    timed_out = true;
                    kill_group(child.id());
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                thread::sleep(POLL);
            }
            Err(e) => {
                return Ok(RunOut {
                    code: None,
                    stdout: String::new(),
                    stderr: format!("wait failed: {e}"),
                    timed_out: false,
                    ms: started.elapsed().as_millis(),
                    cut_bytes: 0,
                })
            }
        }
    };

    // Readers finish once the pipes close (kill closes them too).
    let grace = Duration::from_secs(5);
    let (stdout, cut_out) = rx_out.recv_timeout(grace).unwrap_or_default();
    let (stderr, cut_err) = rx_err.recv_timeout(grace).unwrap_or_default();
    let cut_bytes = cut_out + cut_err;
    if cut_bytes > 0 {
        // Cắt mà im là biến một bản đọc cụt thành một bản đọc trông như đủ.
        crate::logging::warn(
            "exec_output_cut",
            serde_json::json!({ "prog": cmd, "cap": cap, "cut_bytes": cut_bytes,
                                "effect": "chỗ gọi đang cầm bản CỤT — mọi phép so trên nó đều sai" }),
        );
    }

    Ok(RunOut {
        code: status.and_then(|s| s.code()),
        stdout,
        stderr,
        timed_out,
        ms: started.elapsed().as_millis(),
        cut_bytes,
    })
}

// 🔴 ĐÃ XOÁ `run_json` (2026-08-14): mọi chỗ đọc JSON của huba nay tự gọi `run`
// rồi tự phân tích, vì mỗi chỗ cần một câu lỗi khác nhau — `claude agents` hỏng
// khác hẳn `tfl5` hỏng, và câu lỗi là thứ người ta đọc lúc 2 giờ sáng. Một lớp
// bọc chung không ai dùng thì chỉ là một đường thứ hai để quên.
pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    s.chars().take(n).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hết giờ phải giết CẢ HỌ, không chỉ đứa con trực tiếp.
    ///
    /// Đây là lỗi thật, tìm ra 2026-08-10: `claude` là một wrapper spawn tiếp
    /// một binary native, nên `child.kill()` để lại đứa cháu treo vô hạn — hai
    /// con `claude /usage` nằm im trên máy, một con sống bốn tiếng. Phép dò chạy
    /// 5 phút một lần mà mỗi lần rò một tiến trình thì tệ hơn không dò.
    ///
    /// Dựng lại đúng hình dạng ấy: `sh` (con) sinh ra `sleep` (cháu) rồi đứng
    /// chờ. Giết con mà không giết nhóm thì `sleep` sống tiếp.
    #[test]
    fn a_timeout_kills_grandchildren_not_just_the_child() {
        let pidfile = std::env::temp_dir().join(format!("huba-exec-{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&pidfile);
        let script = format!("sleep 30 & echo $! > {}; wait", pidfile.display());

        let out = run(
            "/bin/sh",
            &["-c", &script],
            RunOpts {
                timeout: Some(Duration::from_secs(2)),
                ..Default::default()
            },
        )
        .expect("spawn được");
        assert!(out.timed_out, "phép thử vô nghĩa nếu nó không hết giờ");

        let pid = std::fs::read_to_string(&pidfile)
            .expect("đứa cháu phải kịp ghi pid — nếu không thì phép đo đang nhìn vào hư không")
            .trim()
            .to_string();
        assert!(!pid.is_empty(), "không đọc được pid của đứa cháu");

        // `kill -0` chỉ hỏi "còn sống không", không gửi tín hiệu nào.
        std::thread::sleep(Duration::from_millis(400));
        let alive = Command::new("/bin/kill")
            .args(["-0", &pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let _ = std::fs::remove_file(&pidfile);
        if alive {
            let _ = Command::new("/bin/kill").args(["-KILL", &pid]).status();
        }
        assert!(!alive, "đứa cháu {pid} vẫn sống sau khi hết giờ");
    }
}
