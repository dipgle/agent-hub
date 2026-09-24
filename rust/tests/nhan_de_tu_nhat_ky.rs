//! Nhãn "đang làm gì" của phiên đọc từ bản ghi `ai-title` trong NHẬT KÝ, không
//! còn từ nhan đề tab Terminal.
//!
//! 🔴 2026-09-24: `sample` Terminal lúc nó không trả lời Apple Event — 54 % mẫu luồng
//! chính trong `-[NSWindow _dosetTitle:]` (~13 phiên `claude` đổi nhan đề liên
//! tục). Cùng chữ nhan đề nằm trong nhật ký: `{"type":"ai-title","aiTitle":"…"}`
//! (đo: 4/4 nhật ký soi, ghi lại 29–92 lần mỗi tệp, bản cuối trong ~20 KB đuôi), và
//! nhật ký thì đọc được cả lúc lượt dò Terminal hỏng.
//! ⚠ `CLAUDE_CODE_DISABLE_TERMINAL_TITLE=1` (Hà duyệt cùng ngày) tắt LUÔN `ai-title`
//! — đo trên phiên đầu mở sau khi bật: 0 bản ghi sau 8 lượt. Xem `LiveSession::doing`.

use std::collections::HashSet;

use huba::sessions::parse_tail;

fn khong_nen() -> HashSet<String> {
    HashSet::new()
}

const NGUOI: &str = r#"{"type":"user","message":{"role":"user","content":"hỏi"}}"#;
const MAY: &str = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"trả lời"}]}}"#;
const CHE_DO: &str = r#"{"type":"permission-mode","permissionMode":"auto"}"#;

fn nhan_de(t: &str) -> String {
    format!(r#"{{"type":"ai-title","aiTitle":"{t}","sessionId":"x"}}"#)
}

#[test]
fn lay_nhan_de_moi_nhat() {
    let tail = [
        nhan_de("Cũ"),
        NGUOI.to_string(),
        CHE_DO.to_string(),
        nhan_de("Sổ việc Redis và lỗi kênh Telegram"),
        MAY.to_string(),
    ]
    .join("\n");
    let t = parse_tail(&tail, &khong_nen());
    assert_eq!(
        t.ai_title.as_deref(),
        Some("Sổ việc Redis và lỗi kênh Telegram")
    );
    assert_eq!(t.last_text.as_deref(), Some("trả lời"));
    assert_eq!(t.permission_mode.as_deref(), Some("auto"));
}

/// Nhan đề nằm XA phía sau lượt mới nhất và chế độ — nhánh "chỉ còn tìm nhan đề"
/// (bỏ qua không parse) vẫn phải với tới nó.
#[test]
fn nhan_de_nam_xa_van_tim_thay() {
    let mut dong = vec![
        "{\"type\":\"ai-ti".to_string(), // dòng cụt đầu khung đọc: phải bỏ qua, không vỡ
        nhan_de("Dev-api-v1 bàn giao và Redis"),
        CHE_DO.to_string(),
    ];
    for _ in 0..200 {
        dong.push(NGUOI.to_string());
        dong.push(MAY.to_string());
    }
    let t = parse_tail(&dong.join("\n"), &khong_nen());
    assert_eq!(t.ai_title.as_deref(), Some("Dev-api-v1 bàn giao và Redis"));
    assert_eq!(t.last_text.as_deref(), Some("trả lời"));
}

#[test]
fn khong_co_nhan_de_thi_none_va_phan_con_lai_nguyen() {
    let rong = nhan_de("   ");
    let tail = [NGUOI, CHE_DO, MAY, rong.as_str()].join("\n");
    let t = parse_tail(&tail, &khong_nen());
    assert_eq!(t.ai_title, None, "nhan đề rỗng không phải một nhan đề");
    assert_eq!(t.last_text.as_deref(), Some("trả lời"));
    assert_eq!(t.permission_mode.as_deref(), Some("auto"));
}

// ── Chỗ nối: đọc MÃ NGUỒN ─────────────────────────────────────────────────────

fn nguon() -> String {
    let p = std::env::var("HUBA_SESSIONS_SRC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/src/sessions.rs").to_string());
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Hàng phiên lấy `doing` từ `ai_title`, và `mark_doing` (nhan đề tab) không ghi
/// đè lên nó. `None` = thiếu mỏ neo ⟹ ĐỎ.
fn noi_dung(src: &str) -> Option<bool> {
    let gan = src.contains("row.doing = parsed.ai_title");
    let dau = src.find("fn mark_doing(")?;
    let cuoi = dau + src[dau..].find("\n}\n")?;
    let chi_lui = src[dau..cuoi].contains("!r.doing.is_empty()");
    Some(gan && chi_lui)
}

#[test]
fn hang_phien_lay_nhan_de_tu_nhat_ky_tab_chi_la_duong_lui() {
    assert_eq!(noi_dung(&nguon()), Some(true));
}

/// ĐỐI CHỨNG NGƯỢC: hình dạng trước 24/09 — chỉ có nhan đề tab, ghi đè vô điều kiện.
#[test]
fn doi_chung_nguoc_chi_co_nhan_de_tab() {
    let cu = "fn mark_doing(rows: &mut [LiveSession], tabs: &[Tab]) {\n    for r in rows.iter_mut() {\n        if r.tty.is_empty() {\n            continue;\n        }\n        r.doing = alive_tab(tabs, &r.tty).unwrap_or_default();\n    }\n}\n";
    assert_eq!(noi_dung(cu), Some(false));
    assert_eq!(noi_dung("fn khac() {}\n"), None);
}
