//! "Nó đang chạy dở, nên xem lại" phải nói về lúc phiên CHẾT, không phải lúc
//! huba nhìn nó lần cuối.
//!
//! 🔴 2026-10-01 10:32:16Z huba báo `⚫ [huba]·fb6a2579 đã tắt hẳn — nó đang chạy
//! dở, nên xem lại.` Nhật ký của phiên ấy khép lượt cuối bằng `end_turn` lúc
//! 10:30:39Z; phiên kế nhiệm đóng nó ~10:31. Cờ "chạy dở" đến từ lượt nhìn
//! 10:30:13Z — lúc ấy nó chạy thật — vì hai lượt nhìn cách nhau ~2 phút.
//! Đo trên `logs/huba.log` cùng ngày (`.tmp/do-chay-do.py`): 592 tin "nên xem
//! lại"; nối được 374 về nhật ký (211 không mang id phiên, 7 mất nhật ký). Bỏ
//! `167252e2` (phiên nền nhấp nháy 04–05/09, 319 tin, đã vá bằng
//! `BG_MISS_DEBOUNCE_SEC`) còn 55: **51 (93 %)** về một phiên mà bản ghi hội
//! thoại cuối là `end_turn`; `turn_open_at_death` trả `Some(false)` cho cả 51.
//!
//! Khung bản ghi dưới đây rút từ đuôi nhật ký thật `fb6a2579` (chữ cắt ngắn,
//! thứ tự và kiểu bản ghi giữ nguyên).

use std::collections::HashSet;

use huba::sessions::{parse_tail, NewestTurn};

fn khong_nen() -> HashSet<String> {
    HashSet::new()
}

const THONG_BAO_NEN: &str = r#"{"type":"user","origin":{"kind":"task-notification"},"message":{"role":"user","content":"<task-notification>\n<task-id>b0cr4k2s6</task-id>\n<tool-use-id>toolu_bash</tool-use-id>\n<status>completed</status>\n</task-notification>"},"timestamp":"2026-10-01T10:30:11.315Z"}"#;
const GOI_BASH: &str = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_cat","name":"Bash","input":{"command":"cat x"}}]},"timestamp":"2026-10-01T10:30:19.321Z"}"#;
const KQ_BASH: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_cat","content":"ok"}]},"timestamp":"2026-10-01T10:30:19.348Z"}"#;
const DINH_KEM: &str =
    r#"{"type":"attachment","attachment":{"type":"x"},"timestamp":"2026-10-01T10:30:19.779Z"}"#;
const NGHI: &str = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"end_turn","content":[{"type":"thinking","thinking":""}]},"timestamp":"2026-10-01T10:30:35.457Z"}"#;
const LOI_CUOI: &str = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"[huba] Đã chuyển phiên."}]},"timestamp":"2026-10-01T10:30:39.187Z"}"#;
/// Bảy bản ghi sổ sách CLI ghi SAU lời cuối — không bản nào là hội thoại.
const SO_SACH_SAU: [&str; 7] = [
    r#"{"type":"system","subtype":"stop_hook_summary","hookCount":1,"timestamp":"2026-10-01T10:30:39.265Z"}"#,
    r#"{"type":"system","subtype":"turn_duration","durationMs":28000,"isMeta":false,"timestamp":"2026-10-01T10:30:39.268Z"}"#,
    r#"{"type":"last-prompt","lastPrompt":"x","sessionId":"fb6a2579"}"#,
    r#"{"type":"mode","mode":"normal","sessionId":"fb6a2579"}"#,
    r#"{"type":"permission-mode","permissionMode":"auto","sessionId":"fb6a2579"}"#,
    r#"{"type":"atis-latch","atis":false,"sessionId":"fb6a2579"}"#,
    r#"{"type":"cost-state","totalCostUSD":0,"sessionId":"fb6a2579"}"#,
];

fn duoi_that() -> Vec<&'static str> {
    let mut v = vec![THONG_BAO_NEN, GOI_BASH, KQ_BASH, DINH_KEM, NGHI, LOI_CUOI];
    v.extend(SO_SACH_SAU);
    v
}

// ── Lượt ĐÃ KHÉP ⟹ Some(false) ───────────────────────────────────────────────

/// Đúng ca 10:32:16Z: lời cuối `end_turn`, sau đó toàn sổ sách.
#[test]
fn ca_that_fb6a2579_luot_da_khep() {
    let t = parse_tail(&duoi_that().join("\n"), &khong_nen());
    assert_eq!(
        t.newest_turn,
        Some(NewestTurn {
            role: "assistant".to_string(),
            stop_reason: Some("end_turn".to_string()),
            interrupted: false,
        })
    );
    assert_eq!(t.turn_open(), Some(false));
}

/// Ca `2aebe8dd` (30/09): chủ máy bấm Esc NGẮT lượt rồi đóng phiên. Bản ghi cuối
/// là `user` — nhưng là dấu ngắt, phiên đã về dấu nhắc. Bản trơn mang
/// `interruptedMessageId`; bản "for tool use" của CLI `2.1.280` thường KHÔNG.
#[test]
fn ngat_luot_bang_esc_la_khep() {
    let ngat = r#"{"type":"user","interruptedMessageId":"msg_x","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#;
    let ngat_cong_cu = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user for tool use]"}]}}"#;
    for cuoi in [ngat, ngat_cong_cu] {
        let mut v = vec![GOI_BASH, KQ_BASH, cuoi];
        v.extend(SO_SACH_SAU);
        let t = parse_tail(&v.join("\n"), &khong_nen());
        assert!(
            t.newest_turn.as_ref().is_some_and(|n| n.interrupted),
            "{cuoi}"
        );
        assert_eq!(t.turn_open(), Some(false), "{cuoi}");
    }
}

/// Ca `594a4cd8` (21/09): `/context` gõ tại chỗ sau lượt đã khép — CLI ghi đầu ra
/// thành một bản ghi `user` có `isMeta: true`. Đó không phải một lượt. Sổ sách
/// (`permission-mode`) đứng SAU nó để chạm đúng cửa "ngừng parse sớm".
#[test]
fn ban_ghi_is_meta_khong_phai_luot() {
    let meta = r###"{"type":"user","isMeta":true,"message":{"role":"user","content":"## Context Usage\n\n**Tokens:** 416k / 1m (42%)"}}"###;
    let mut v = duoi_that();
    v.push(meta);
    v.extend(SO_SACH_SAU);
    let t = parse_tail(&v.join("\n"), &khong_nen());
    assert_eq!(
        t.newest_turn.as_ref().map(|n| n.role.as_str()),
        Some("assistant")
    );
    assert_eq!(t.turn_open(), Some(false));
}

/// Agent NỀN đã về (thông báo mang đúng `tool-use-id`) rồi lượt mới khép ⟹ rảnh.
#[test]
fn agent_nen_da_ve_roi_khep_luot_la_khep() {
    let goi = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_ag","name":"Agent","input":{}}]}}"#;
    let kq = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_ag","content":"launched"}]}}"#;
    let ve = r#"{"type":"user","message":{"role":"user","content":"<task-notification>\n<tool-use-id>toolu_ag</tool-use-id>\n</task-notification>"}}"#;
    let nen: HashSet<String> = ["toolu_ag".to_string()].into();
    let t = parse_tail(&[goi, kq, ve, LOI_CUOI].join("\n"), &nen);
    assert_eq!(t.pending_subagents, 0);
    assert_eq!(t.turn_open(), Some(false));
}

// ── Lượt CÒN MỞ ⟹ Some(true) — đối chứng ngược của nhóm trên ────────────────

/// Bị đóng giữa lúc một lệnh đang chạy: bản ghi cuối là `tool_use` TRẦN (không
/// chữ). `last_role` đi lùi tới bản có chữ nên không trả lời được câu này.
#[test]
fn dung_o_tool_use_tran_la_mo() {
    let t = parse_tail(
        &[THONG_BAO_NEN, LOI_CUOI, GOI_BASH].join("\n"),
        &khong_nen(),
    );
    assert_eq!(t.turn_open(), Some(true));
}

/// Kết quả công cụ đã về, model chưa kịp nói tiếp ⟹ đang giữa lượt.
#[test]
fn dung_o_tool_result_la_mo() {
    let t = parse_tail(&[GOI_BASH, KQ_BASH, DINH_KEM].join("\n"), &khong_nen());
    assert_eq!(t.turn_open(), Some(true));
}

/// Chủ máy vừa gửi câu mới sau lượt đã khép ⟹ lượt MỚI đang chạy.
#[test]
fn cau_moi_sau_luot_khep_la_mo() {
    let moi = r#"{"type":"user","message":{"role":"user","content":"làm tiếp"}}"#;
    let mut v = duoi_that();
    v.push(moi);
    let t = parse_tail(&v.join("\n"), &khong_nen());
    assert_eq!(t.turn_open(), Some(true));
}

/// Câu chủ máy GÕ có nhắc tới dấu ngắt (khớp chuỗi con) KHÔNG phải dấu ngắt —
/// đối chứng ngược của `ngat_luot_bang_esc_la_khep`.
#[test]
fn cau_go_nhac_toi_dau_ngat_van_la_mo() {
    let go = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"sao lại có [Request interrupted by user] ở đây?"}]}}"#;
    let mut v = duoi_that();
    v.push(go);
    let t = parse_tail(&v.join("\n"), &khong_nen());
    assert!(t.newest_turn.as_ref().is_some_and(|n| !n.interrupted));
    assert_eq!(t.turn_open(), Some(true));
}

/// Bản ghi `user` KHÔNG có `isMeta` sau lượt khép là lượt mới — đối chứng ngược
/// của `ban_ghi_is_meta_khong_phai_luot`.
#[test]
fn cung_chu_ay_khong_is_meta_la_mo() {
    let khong_meta =
        r###"{"type":"user","message":{"role":"user","content":"## Context Usage"}}"###;
    let mut v = duoi_that();
    v.push(khong_meta);
    v.extend(SO_SACH_SAU);
    let t = parse_tail(&v.join("\n"), &khong_nen());
    assert_eq!(t.turn_open(), Some(true));
}

/// Khép lượt ở dấu nhắc nhưng agent NỀN chưa về ⟹ chết đi là mất việc.
#[test]
fn khep_luot_ma_con_agent_nen_la_mo() {
    let goi = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_ag","name":"Agent","input":{}}]}}"#;
    let kq = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_ag","content":"launched"}]}}"#;
    let nen: HashSet<String> = ["toolu_ag".to_string()].into();
    let t = parse_tail(&[goi, kq, LOI_CUOI].join("\n"), &nen);
    assert_eq!(t.pending_subagents, 1);
    assert_eq!(t.turn_open(), Some(true));
}

// ── KHÔNG ĐO ĐƯỢC ⟹ None, không lẫn vào hai trạng thái trên ─────────────────

#[test]
fn chi_co_so_sach_la_khong_do_duoc() {
    let t = parse_tail(&SO_SACH_SAU.join("\n"), &khong_nen());
    assert_eq!(t.newest_turn, None);
    assert_eq!(t.turn_open(), None);
}

// ── Chỗ nối: đọc MÃ NGUỒN `pipeline.rs` ──────────────────────────────────────

fn nguon() -> String {
    let p = std::env::var("HUBA_PIPELINE_SRC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs").to_string());
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Phép hỏi nhật ký phải đứng TRƯỚC cả hai chỗ đọc `was_working` trong
/// `announce_changes`: khối im phiên con và câu "nên xem lại". `None` = thiếu
/// mỏ neo ⟹ ĐỎ.
fn noi_dung(src: &str) -> Option<bool> {
    let hoi = src.find("turn_open_at_death(cfg, &id)")?;
    let im_con = src.find("phiên con kết thúc bình thường")?;
    let canh_bao = src.find(" — nó đang chạy dở, nên xem lại")?;
    Some(hoi < im_con && hoi < canh_bao)
}

#[test]
fn hoi_nhat_ky_truoc_khi_dung_co_chay_do() {
    assert_eq!(noi_dung(&nguon()), Some(true));
}

/// ĐỐI CHỨNG NGƯỢC: hình dạng trước 01/10 — dùng thẳng cờ của sổ, không hỏi
/// nhật ký; và phép hỏi đặt SAU câu cảnh báo cũng phải đỏ.
#[test]
fn doi_chung_nguoc_dung_co_so_truc_tiep() {
    let cu = "if !parent.is_empty() && !was_working { // phiên con kết thúc bình thường\n}\nlet warn = if *was_working { \" — nó đang chạy dở, nên xem lại\" } else { \"\" };\n";
    assert_eq!(noi_dung(cu), None);
    let sai_cho = format!("{cu}crate::sessions::turn_open_at_death(cfg, &id);\n");
    assert_eq!(noi_dung(&sai_cho), Some(false));
}
