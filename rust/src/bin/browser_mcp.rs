//! `browser-mcp` — a minimal, hand-written MCP (Model Context Protocol) server
//! over stdio, exposing `huba::browser` (control of the owner's REAL,
//! already-logged-in Chrome) as tools a Claude Code session can call directly.
//!
//! No `rmcp`/MCP crate: the offline cargo cache carries none, and the build is
//! `cargo build --offline` — this repo's own rule (`Cargo.toml`'s own header)
//! is to hand-write a protocol with `std` rather than pull in machinery a
//! deliberately-synchronous process does not want (see `setup.rs`'s own
//! `TcpListener` instead of a web framework). One JSON-RPC 2.0 message per
//! line on stdin/stdout, synchronous, no async runtime.
//!
//! stdout carries ONLY JSON-RPC frames — a stray `println!` here looks like a
//! malformed frame to whatever spawned this process. Anything for a human
//! (debug, a bad line) goes to stderr instead.
//!
//! Kept deliberately narrow: seven tools, one per `browser::*` primitive that
//! already exists for a reason a human reviewed. No "run arbitrary JS" tool —
//! `browser.rs` itself explains, at `dia_chi_hop_le` and at `sc_click`/
//! `sc_fill`, why that door does not exist even for the phone; it must not
//! reopen here for an agent either.

use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicU64, Ordering};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde_json::{json, Value};

use huba::browser;

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("browser_mcp: lỗi đọc stdin: {e}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(&line) {
            Ok(req) => handle(req),
            Err(e) => {
                eprintln!("browser_mcp: dòng vào không phải JSON: {e}");
                continue;
            }
        };
        if let Some(resp) = resp {
            let s = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string());
            if writeln!(stdout, "{s}").is_err() {
                // Đầu đọc bên kia đã đóng — không còn ai nhận, dừng vòng lặp.
                break;
            }
            let _ = stdout.flush();
        }
    }
}

/// Một request JSON-RPC hỏng KHÔNG được giết cả tiến trình — mọi `unwrap` nguy
/// hiểm phải dừng lại ở việc làm hỏng ĐÚNG một response.
fn handle(req: Value) -> Option<Value> {
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");

    // Đúng luật JSON-RPC 2.0: KHÔNG có field `id` ⟹ đây là một THÔNG BÁO, và
    // thông báo không bao giờ được trả lời — bất kể method là gì.
    let id = match req.get("id") {
        Some(v) => v.clone(),
        None => {
            if method != "notifications/initialized" {
                eprintln!("browser_mcp: thông báo không rõ ({method}) bị bỏ qua");
            }
            return None;
        }
    };

    match method {
        "initialize" => Some(ok(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "browser-mcp", "version": "0.1.0" }
            }),
        )),
        "tools/list" => Some(ok(id, json!({ "tools": tool_defs() }))),
        "tools/call" => {
            let empty = json!({});
            let params = req.get("params").unwrap_or(&empty);
            match dispatch_tool_call(params) {
                Ok(result) => Some(ok(id, result)),
                Err(e) => Some(err(id, e.code, &e.message)),
            }
        }
        other => Some(err(id, -32601, &format!("Method not found: {other}"))),
    }
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn err(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Lỗi ở TẦNG GIAO THỨC (tool không có tên, tham số sai kiểu) — khác hẳn một
/// `Err(Loi)` từ `browser::*`, thứ luôn trả về `isError: true` bên trong một
/// `result` bình thường, không phải một lỗi JSON-RPC.
struct RpcErr {
    code: i64,
    message: String,
}

fn invalid_params(message: String) -> RpcErr {
    RpcErr {
        code: -32602,
        message,
    }
}

/// Bảy tool, ĐÚNG bấy nhiêu — không có tool "chạy JS tuỳ ý".
fn tool_defs() -> Value {
    json!([
        {
            "name": "browser_tabs",
            "description": "Liệt kê mọi tab đang mở trong Chrome THẬT của chủ máy, mọi cửa sổ — không mở tab mới, chỉ đọc.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        },
        {
            "name": "browser_open",
            "description": "Mở một địa chỉ web trong một TAB MỚI của cửa sổ Chrome trước mặt. Chỉ nhận `http`/`https` — không nhận `file:`/`javascript:`.",
            "inputSchema": {
                "type": "object",
                "properties": { "url": { "type": "string", "description": "Địa chỉ cần mở, có hoặc không có lược đồ (vd `mail.google.com`)." } },
                "required": ["url"]
            }
        },
        {
            "name": "browser_switch",
            "description": "Chuyển sang một tab đã mở (theo số cửa sổ + số thứ tự tab) và đưa cửa sổ ấy ra trước mặt.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "win": { "type": "integer", "description": "Số thứ tự cửa sổ (1-based)." },
                    "idx": { "type": "integer", "description": "Số thứ tự tab trong cửa sổ ấy (1-based)." }
                },
                "required": ["win", "idx"]
            }
        },
        {
            "name": "browser_read_text",
            "description": "Đọc toàn bộ chữ (`innerText`) của trang đang mở ở tab hiện tại — văn bản thật của trang, không phải ảnh chụp.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        },
        {
            "name": "browser_click",
            "description": "Bấm vào một phần tử trên trang hiện tại, theo CSS selector. KHÔNG BAO GIỜ nhận mã JavaScript.",
            "inputSchema": {
                "type": "object",
                "properties": { "selector": { "type": "string", "description": "CSS selector của phần tử cần bấm." } },
                "required": ["selector"]
            }
        },
        {
            "name": "browser_fill",
            "description": "Điền một giá trị vào ô nhập trên trang hiện tại, theo CSS selector.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "CSS selector của ô nhập." },
                    "value": { "type": "string", "description": "Giá trị cần điền vào ô." }
                },
                "required": ["selector", "value"]
            }
        },
        {
            "name": "browser_screenshot",
            "description": "Chụp ảnh cửa sổ Chrome hiện tại (đưa ra trước mặt trước khi chụp), trả về ảnh PNG.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        }
    ])
}

fn dispatch_tool_call(params: &Value) -> Result<Value, RpcErr> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params("thiếu tham số `name`".to_string()))?;
    let empty = json!({});
    let args = params.get("arguments").unwrap_or(&empty);

    match name {
        "browser_tabs" => Ok(result_tabs()),
        "browser_open" => {
            let url = req_str(args, "url")?;
            Ok(result_open(&url))
        }
        "browser_switch" => {
            let win = req_uint(args, "win")?;
            let idx = req_uint(args, "idx")?;
            Ok(result_switch(win, idx))
        }
        "browser_read_text" => Ok(result_read_text()),
        "browser_click" => {
            let selector = req_str(args, "selector")?;
            Ok(result_click(&selector))
        }
        "browser_fill" => {
            let selector = req_str(args, "selector")?;
            let value = req_str(args, "value")?;
            Ok(result_fill(&selector, &value))
        }
        "browser_screenshot" => Ok(result_screenshot()),
        other => Err(invalid_params(format!("không có tool tên `{other}`"))),
    }
}

fn req_str(args: &Value, key: &str) -> Result<String, RpcErr> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| invalid_params(format!("thiếu hoặc sai kiểu tham số `{key}` (cần chuỗi)")))
}

fn req_uint(args: &Value, key: &str) -> Result<usize, RpcErr> {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .ok_or_else(|| invalid_params(format!("thiếu hoặc sai kiểu tham số `{key}` (cần số nguyên)")))
}

/// Kết quả tool dạng CHỮ, đúng format MCP: `{content:[...], isError}`.
fn text_result(text: String, is_error: bool) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error
    })
}

/// Một `Err(Loi)` từ `browser::*` LUÔN là "tool chạy nhưng thất bại"
/// (`isError: true` bên trong một result bình thường) — KHÔNG BAO GIỜ là lỗi
/// JSON-RPC tầng giao thức, vì giao thức không hề hỏng: tool đã chạy trọn.
fn loi_result(loi: browser::Loi) -> Value {
    text_result(loi.to_string(), true)
}

fn result_tabs() -> Value {
    match browser::tabs() {
        Ok(tabs) if tabs.is_empty() => text_result("Không có tab nào đang mở.".to_string(), false),
        Ok(tabs) => {
            let lines: Vec<String> = tabs
                .iter()
                .map(|t| {
                    let dau = if t.active { " ●" } else { "" };
                    format!("{}.{}{dau} {} · {}", t.win, t.idx, t.url, t.title)
                })
                .collect();
            text_result(lines.join("\n"), false)
        }
        Err(e) => loi_result(e),
    }
}

fn result_open(url: &str) -> Value {
    match browser::mo(url) {
        Ok(tab) => text_result(
            format!("Đã mở {}.{} — {} · {}", tab.win, tab.idx, tab.url, tab.title),
            false,
        ),
        Err(e) => loi_result(e),
    }
}

fn result_switch(win: usize, idx: usize) -> Value {
    match browser::chon(win, idx) {
        Ok(tab) => text_result(
            format!("Đã chuyển sang {}.{} — {} · {}", tab.win, tab.idx, tab.url, tab.title),
            false,
        ),
        Err(e) => loi_result(e),
    }
}

fn result_read_text() -> Value {
    match browser::chu_trang() {
        Ok(text) => text_result(text, false),
        Err(e) => loi_result(e),
    }
}

fn result_click(selector: &str) -> Value {
    match browser::bam(selector) {
        Ok(true) => text_result(format!("Đã bấm vào `{selector}`."), false),
        // `Ok(false)` là "không thấy phần tử" — một lỗi từ góc nhìn của agent
        // gọi tool, dù không phải `Err` ở tầng Rust.
        Ok(false) => text_result(
            format!("không tìm thấy phần tử khớp selector `{selector}`"),
            true,
        ),
        Err(e) => loi_result(e),
    }
}

fn result_fill(selector: &str, value: &str) -> Value {
    match browser::dien(selector, value) {
        Ok(true) => text_result(format!("Đã điền vào `{selector}`."), false),
        Ok(false) => text_result(
            format!("không tìm thấy phần tử khớp selector `{selector}`"),
            true,
        ),
        Err(e) => loi_result(e),
    }
}

/// Mốc đủ duy nhất TRONG MỘT LẦN CHẠY tiến trình này — không cần `Date.now`
/// hay số ngẫu nhiên, chỉ cần không trùng một tệp tạm khác đang mở trong cùng
/// một tiến trình (pid) tại cùng một thời điểm.
static SHOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn result_screenshot() -> Value {
    let n = SHOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("browser-mcp-{}-{n}.png", std::process::id()));

    let result = match browser::chup_anh(&path) {
        Ok(()) => match std::fs::read(&path) {
            Ok(bytes) => {
                let data = B64.encode(&bytes);
                json!({
                    "content": [{ "type": "image", "data": data, "mimeType": "image/png" }],
                    "isError": false
                })
            }
            Err(e) => text_result(format!("Đã chụp ảnh nhưng không đọc lại được tệp tạm: {e}"), true),
        },
        Err(e) => loi_result(e),
    };
    // Dọn tệp tạm dù đọc thành công hay thất bại — đừng để rác trong temp dir.
    let _ = std::fs::remove_file(&path);
    result
}
