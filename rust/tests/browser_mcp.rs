//! Kiểm tầng GIAO THỨC của `browser-mcp` bằng cách spawn chính binary đã
//! build và nói chuyện qua stdin/stdout THẬT — JSON-RPC 2.0, mỗi thông điệp
//! một dòng.
//!
//! KHÔNG chạm Chrome thật: môi trường chạy test (CI/sandbox) không có quyền
//! Automation, nên mọi test ở đây dừng lại TRƯỚC khi `tools/call` gọi vào một
//! tool `browser_*` thật — hoặc gọi một tool mà tham số cố tình sai/thiếu, để
//! phép validate trả lỗi trước khi kịp đụng `browser::*`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

/// Một tiến trình `browser-mcp` con, cộng hai đầu ống đã mở sẵn.
struct Mcp {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Mcp {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_browser-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn browser-mcp");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        Mcp {
            child,
            stdin: Some(stdin),
            stdout,
        }
    }

    fn send(&mut self, v: &Value) {
        let stdin = self
            .stdin
            .as_mut()
            .expect("stdin đã đóng — gọi send() sau close_stdin()?");
        let line = serde_json::to_string(v).expect("serialize request");
        writeln!(stdin, "{line}").expect("write to child stdin");
        stdin.flush().expect("flush child stdin");
    }

    /// Đọc đúng MỘT dòng phản hồi và parse lại thành JSON.
    fn read_response(&mut self) -> Value {
        let mut buf = String::new();
        let n = self
            .stdout
            .read_line(&mut buf)
            .expect("read from child stdout");
        assert!(n > 0, "tiến trình con đóng stdout trước khi trả lời (EOF)");
        serde_json::from_str(buf.trim_end())
            .unwrap_or_else(|e| panic!("dòng trả về không phải JSON hợp lệ: {e}\ndòng: {buf:?}"))
    }

    /// Đóng đầu ghi — tiến trình con đọc `stdin.lock().lines()` sẽ gặp EOF và
    /// tự thoát vòng lặp chính, không cần `kill`.
    fn close_stdin(&mut self) {
        self.stdin.take();
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        // Luôn đóng stdin trước — dù test đã gọi `close_stdin()` hay chưa —
        // rồi `kill`+`wait` như một lưới an toàn: test không được để lại một
        // tiến trình `browser-mcp` mồ côi chạy nền sau khi kết thúc.
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `tools/list` phải trả ĐÚNG bảy tool, đúng bảy cái tên, và mỗi tool phải
/// mang `inputSchema` — một agent gọi tool không có schema thì không biết
/// tham số nào hợp lệ.
#[test]
fn tools_list_reports_exactly_the_seven_tools_with_schemas() {
    let mut mcp = Mcp::spawn();
    mcp.send(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }));
    let resp = mcp.read_response();

    assert_eq!(resp["jsonrpc"], "2.0", "{resp}");
    assert_eq!(resp["id"], 1, "{resp}");
    let tools = resp["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("result.tools không phải mảng: {resp}"));
    assert_eq!(
        tools.len(),
        7,
        "phải đúng 7 tool, không hơn không kém: {tools:?}"
    );

    let expected = [
        "browser_tabs",
        "browser_open",
        "browser_switch",
        "browser_read_text",
        "browser_click",
        "browser_fill",
        "browser_screenshot",
    ];
    for name in expected {
        assert!(
            tools.iter().any(|t| t["name"] == name),
            "thiếu tool `{name}` trong {tools:?}"
        );
    }
    for t in tools {
        assert!(
            t.get("inputSchema").and_then(Value::as_object).is_some(),
            "tool thiếu inputSchema dạng object: {t}"
        );
        assert!(
            t.get("description")
                .and_then(Value::as_str)
                .is_some_and(|d| !d.is_empty()),
            "tool thiếu description: {t}"
        );
    }

    mcp.close_stdin();
}

/// Method hoàn toàn lạ (không phải initialize/tools-list/tools-call/
/// notifications-initialized) phải là lỗi JSON-RPC ĐÚNG mã `-32601`.
#[test]
fn an_unknown_method_answers_method_not_found() {
    let mut mcp = Mcp::spawn();
    mcp.send(&json!({ "jsonrpc": "2.0", "id": 42, "method": "khong_ton_tai" }));
    let resp = mcp.read_response();

    assert_eq!(resp["id"], 42, "{resp}");
    assert_eq!(resp["error"]["code"], -32601, "{resp}");
    assert!(
        resp.get("result").is_none(),
        "lỗi thì không được có `result`: {resp}"
    );

    mcp.close_stdin();
}

/// `notifications/initialized` không có field `id` ⟹ KHÔNG được sinh ra bất
/// kỳ dòng phản hồi nào. Chứng minh bằng cách gửi ngay một request CÓ `id`
/// ngay sau nó, rồi assert dòng ĐẦU TIÊN đọc được ứng đúng với `id` ấy — nếu
/// notification lỡ sinh ra một dòng rác, dòng đó sẽ đứng trước và phép assert
/// dưới đây sẽ đỏ.
#[test]
fn a_bare_notification_produces_no_reply_line_of_its_own() {
    let mut mcp = Mcp::spawn();
    mcp.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    mcp.send(&json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/list" }));

    let first_line = mcp.read_response();
    assert_eq!(
        first_line["id"], 7,
        "dòng ĐẦU TIÊN phải ứng với request id=7, không phải một hồi đáp lạc của notification: {first_line}"
    );
    assert!(first_line.get("result").is_some(), "{first_line}");

    mcp.close_stdin();
}

/// Tham số THIẾU trên một tool có thật phải là lỗi tầng GIAO THỨC
/// (`-32602 Invalid params`), và phải trả về TRƯỚC khi chạm tới
/// `browser::*` — bài kiểm này không cần Chrome, không cần quyền Automation.
#[test]
fn tools_call_missing_a_required_argument_is_invalid_params() {
    let mut mcp = Mcp::spawn();
    mcp.send(&json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "browser_click", "arguments": {} }
    }));
    let resp = mcp.read_response();

    assert_eq!(resp["id"], 9, "{resp}");
    assert_eq!(resp["error"]["code"], -32602, "{resp}");

    mcp.close_stdin();
}

/// Tool không có tên cũng là lỗi tầng giao thức, cùng mã với tham số sai kiểu
/// — cả hai đều là "request không hợp lệ", không phải "tool chạy rồi thất bại".
#[test]
fn tools_call_with_an_unknown_tool_name_is_invalid_params() {
    let mut mcp = Mcp::spawn();
    mcp.send(&json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "browser_xoa_o_cung", "arguments": {} }
    }));
    let resp = mcp.read_response();

    assert_eq!(resp["id"], 10, "{resp}");
    assert_eq!(resp["error"]["code"], -32602, "{resp}");

    mcp.close_stdin();
}

/// `initialize` phải trả đúng hình dạng handshake MCP tối thiểu — client dò
/// `protocolVersion` + `capabilities.tools` trước khi gọi `tools/list`.
#[test]
fn initialize_reports_the_expected_handshake_shape() {
    let mut mcp = Mcp::spawn();
    mcp.send(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }));
    let resp = mcp.read_response();

    assert_eq!(resp["id"], 1, "{resp}");
    assert!(resp["result"]["protocolVersion"].is_string(), "{resp}");
    assert!(
        resp["result"]["capabilities"]["tools"].is_object(),
        "{resp}"
    );
    assert_eq!(
        resp["result"]["serverInfo"]["name"], "browser-mcp",
        "{resp}"
    );

    mcp.close_stdin();
}
