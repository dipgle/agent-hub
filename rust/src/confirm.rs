//! Xác nhận lần hai qua Telegram, cho những lệnh không lùi lại được.
//!
//! Hà 2026-08-10: *"riêng một số lệnh dừng hoặc tắt phiên cần có xác thực qua
//! tele"*, ngay sau khi xin nút Dừng đứng thẳng trên danh sách phiên. Hai yêu
//! cầu ấy đi đôi với nhau: nút càng dễ chạm thì cái chạm nhầm càng đắt.
//!
//! **Đây không phải nhánh hộp thư quay lại.** Nó không đọc tin, không tạo việc,
//! không gọi `claude`, không tiêu một đồng hạn mức nào. Nó gửi đúng một câu hỏi
//! kèm hai cái nút và chờ một cú bấm. Thước đo của `CLAUDE.md` — *"cái này có
//! giúp Hà xem hoặc điều khiển phiên từ điện thoại không?"* — trả lời là có: nó
//! là thứ đứng giữa một ngón tay và một tiến trình đang chạy dở.
//!
//! ## Ba quyết định, và lý do
//!
//! 1. **Bật mà thiếu khoá thì TỪ CHỐI lệnh, không âm thầm cho qua.** Một chốt
//!    chặn tự tháo khi cấu hình sai là một chốt chặn không ai dám tin. Đường
//!    thoát vẫn còn và luôn còn: ngồi vào máy gõ `claude stop`.
//! 2. **Chỉ chủ hòm mới xác nhận được.** Bấm nút mà `from.id` khác `chat_id`
//!    trong cấu hình thì coi như không có — cùng tinh thần với luật §7 (phòng
//!    chat chỉ nhận lệnh của chủ máy).
//! 3. **Chặn vòng chạy trong lúc chờ.** hub cố ý đồng bộ; thêm luồng nền chỉ để
//!    khỏi chờ một việc mà người dùng ĐANG ĐỨNG CHỜ là thêm mảnh chuyển động
//!    không mua được gì. Chờ bằng long-poll của Telegram nên không quay CPU, và
//!    câu trả lời trong phòng chat được gửi TRƯỚC khi chờ để màn không câm.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::config::Config;
use crate::logging;

/// Kết cục của một lần hỏi. Không có biến thể nào nghĩa là "chắc là đồng ý".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Confirmed,
    Declined,
    TimedOut,
    /// Không hỏi được: thiếu khoá, mạng hỏng, Telegram trả lỗi. Kèm lý do để
    /// câu trả lời trong phòng chat nói được ĐÚNG cái đang thiếu.
    Unavailable(String),
}

impl Verdict {
    /// Chỉ đúng một trạng thái cho phép đi tiếp.
    pub fn allows(&self) -> bool {
        matches!(self, Verdict::Confirmed)
    }

    /// Câu nói cho phòng chat khi không đi tiếp được.
    pub fn refusal(&self) -> String {
        match self {
            Verdict::Confirmed => String::new(),
            Verdict::Declined => "✋ Đã huỷ trên Telegram — không dừng phiên nào.".to_string(),
            Verdict::TimedOut => {
                "⌛ Hết hạn chờ xác nhận trên Telegram — không dừng phiên nào. Bấm lại nếu vẫn muốn."
                    .to_string()
            }
            Verdict::Unavailable(why) => format!(
                "⚠ Không hỏi được Telegram nên KHÔNG dừng phiên: {}. \
                 Muốn dừng ngay thì ngồi vào máy gõ `claude stop`.",
                crate::exec::truncate(why, 160)
            ),
        }
    }
}

/// Hỏi chủ máy một câu qua Telegram và chờ bấm nút.
///
/// `what` là câu mô tả việc sắp làm, đã đủ cụ thể để đọc trên điện thoại mà
/// không cần mở gì thêm ("Dừng phiên sdvi-a1b2 (acc2)?").
pub fn ask(cfg: &Config, what: &str) -> Verdict {
    if !cfg.confirm.enabled {
        // Tắt hẳn là một lựa chọn có chủ ý của chủ máy, không phải lỗi. Vẫn ghi
        // lại: một chốt chặn bị tắt mà không để dấu vết là thứ không ai nhớ.
        logging::info("confirm_disabled", json!({ "what": what }));
        return Verdict::Confirmed;
    }
    let (token, chat_id) = match (
        crate::config::secret_from_env(&cfg.confirm.bot_token_env),
        crate::config::secret_from_env(&cfg.confirm.chat_id_env),
    ) {
        (Some(t), Some(c)) => (t, c),
        (t, c) => {
            // Chỉ TÊN khoá vào log, không bao giờ giá trị (luật §4).
            let missing: Vec<&str> = [
                (t.is_none()).then_some(cfg.confirm.bot_token_env.as_str()),
                (c.is_none()).then_some(cfg.confirm.chat_id_env.as_str()),
            ]
            .into_iter()
            .flatten()
            .collect();
            logging::error("confirm_secret_missing", json!({ "keys": missing }));
            return Verdict::Unavailable(format!("thiếu {} trong hub.env", missing.join(" + ")));
        }
    };

    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => return Verdict::Unavailable(e.to_string()),
    };
    let api = |m: &str| format!("https://api.telegram.org/bot{token}/{m}");

    // Mốc nước TRƯỚC khi hỏi: nếu không đặt, một cú bấm cũ còn nằm trong hàng
    // đợi của Telegram sẽ được đọc lại và tính là câu trả lời cho câu hỏi lần
    // này. Xác nhận đi lạc sang một lệnh khác là kiểu hỏng tệ nhất ở đây.
    let mut offset = match watermark(&client, &api("getUpdates")) {
        Ok(o) => o,
        Err(e) => return Verdict::Unavailable(format!("không đọc được getUpdates: {e}")),
    };

    // Nonce chỉ để ghép câu trả lời với câu hỏi, không phải thứ để chống giả
    // mạo — cái chống giả mạo là `from.id`. Không kéo thêm crate ngẫu nhiên vào
    // chỉ vì việc này.
    let nonce = format!("{}", chrono::Utc::now().timestamp_millis());
    let body = json!({
        "chat_id": chat_id,
        "text": format!("🔒 hub xin xác nhận\n\n{what}\n\nKhông bấm gì trong {}s = không làm.", cfg.confirm.timeout_sec),
        "reply_markup": {
            "inline_keyboard": [[
                { "text": "✅ Xác nhận", "callback_data": format!("ok:{nonce}") },
                { "text": "✖ Huỷ",      "callback_data": format!("no:{nonce}") }
            ]]
        }
    });
    let sent = client.post(api("sendMessage")).json(&body).send();
    let message_id = match sent {
        Ok(r) => {
            let v: Value = r.json().unwrap_or_else(|_| json!({}));
            if v.get("ok").and_then(Value::as_bool) != Some(true) {
                let why = v
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("Telegram từ chối sendMessage")
                    .to_string();
                logging::error("confirm_send_failed", json!({ "detail": why }));
                return Verdict::Unavailable(why);
            }
            v.pointer("/result/message_id").and_then(Value::as_i64)
        }
        Err(e) => {
            logging::error("confirm_send_failed", json!({ "err": e.to_string() }));
            return Verdict::Unavailable(e.to_string());
        }
    };
    logging::info(
        "confirm_asked",
        json!({ "what": what, "timeout_sec": cfg.confirm.timeout_sec }),
    );

    let deadline = Instant::now() + Duration::from_secs(cfg.confirm.timeout_sec);
    let verdict = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break Verdict::TimedOut;
        }
        // Long-poll: đứng chờ ở phía Telegram thay vì hỏi lại liên tục. Trần 25s
        // để vòng lặp còn nhìn lại hạn chót của chính nó.
        let wait = left.as_secs().clamp(1, 25);
        let url = format!("{}?offset={}&timeout={}", api("getUpdates"), offset, wait);
        let resp = match client.get(&url).send().and_then(|r| r.json::<Value>()) {
            Ok(v) => v,
            Err(e) => {
                // Một nhịp mạng hỏng KHÔNG phải là câu trả lời "không" — ghi lại
                // rồi thử tiếp cho tới hạn.
                logging::warn("confirm_poll_failed", json!({ "err": e.to_string() }));
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        let empty = vec![];
        let updates = resp
            .get("result")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        for u in updates {
            if let Some(id) = u.get("update_id").and_then(Value::as_i64) {
                offset = offset.max(id + 1);
            }
            let Some(cb) = u.get("callback_query") else {
                continue;
            };
            let data = cb.get("data").and_then(Value::as_str).unwrap_or("");
            if !data.ends_with(&nonce) {
                continue; // câu trả lời của một câu hỏi khác
            }
            let from = cb
                .pointer("/from/id")
                .map(|v| v.to_string())
                .unwrap_or_default();
            if from != chat_id {
                // Người khác trong nhóm bấm nút thì đó chỉ là bấm nút.
                logging::warn("confirm_from_stranger", json!({ "what": what }));
                continue;
            }
            if let Some(cbid) = cb.get("id").and_then(Value::as_str) {
                let _ = client
                    .post(api("answerCallbackQuery"))
                    .json(&json!({ "callback_query_id": cbid }))
                    .send();
            }
            break;
        }
        if let Some(v) = updates.iter().find_map(|u| {
            let cb = u.get("callback_query")?;
            let data = cb.get("data").and_then(Value::as_str)?;
            if !data.ends_with(&nonce) {
                return None;
            }
            let from = cb.pointer("/from/id").map(|v| v.to_string())?;
            if from != chat_id {
                return None;
            }
            Some(if data.starts_with("ok:") {
                Verdict::Confirmed
            } else {
                Verdict::Declined
            })
        }) {
            break v;
        }
    };

    // Ghi kết cục ngay lên chính tin nhắn ấy: hòm Telegram phải đọc được là
    // "đã bấm gì, và hub đã hiểu thế nào", chứ không để lại một câu hỏi treo.
    if let Some(mid) = message_id {
        let stamp = match &verdict {
            Verdict::Confirmed => "✅ Đã xác nhận",
            Verdict::Declined => "✖ Đã huỷ",
            Verdict::TimedOut => "⌛ Hết hạn — hub KHÔNG làm gì",
            Verdict::Unavailable(_) => "⚠ Hỏng đường hỏi",
        };
        let _ = client
            .post(api("editMessageText"))
            .json(&json!({
                "chat_id": chat_id,
                "message_id": mid,
                "text": format!("🔒 {what}\n\n{stamp}"),
            }))
            .send();
    }
    logging::info(
        "confirm_resolved",
        json!({ "what": what, "verdict": format!("{verdict:?}") }),
    );
    verdict
}

/// `update_id` kế tiếp cần đọc, tính từ những gì Telegram còn đang giữ.
fn watermark(client: &reqwest::blocking::Client, url: &str) -> anyhow::Result<i64> {
    let v: Value = client
        .get(format!("{url}?offset=-1&timeout=0"))
        .send()?
        .json()?;
    let next = v
        .get("result")
        .and_then(Value::as_array)
        .and_then(|a| a.last())
        .and_then(|u| u.get("update_id"))
        .and_then(Value::as_i64)
        .map(|id| id + 1)
        .unwrap_or(0);
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chỉ MỘT trạng thái được đi tiếp. Nếu ai đó thêm biến thể mới mà quên,
    /// mặc định phải là "không cho qua" — chốt chặn hỏng theo hướng an toàn.
    #[test]
    fn only_a_real_confirmation_opens_the_gate() {
        assert!(Verdict::Confirmed.allows());
        assert!(!Verdict::Declined.allows());
        assert!(!Verdict::TimedOut.allows());
        assert!(!Verdict::Unavailable("thiếu khoá".into()).allows());
    }

    /// Câu từ chối phải nói ĐƯỢC hai điều: không có gì bị dừng, và đường thoát
    /// nằm ở đâu. Một lời từ chối cụt lủn biến chốt chặn thành ngõ cụt.
    #[test]
    fn a_refusal_says_what_did_not_happen_and_what_to_do() {
        for v in [
            Verdict::Declined,
            Verdict::TimedOut,
            Verdict::Unavailable("thiếu HUB_TELEGRAM_BOT_TOKEN trong hub.env".into()),
        ] {
            let msg = v.refusal();
            assert!(!msg.is_empty(), "{v:?} không có câu trả lời");
            assert!(
                msg.contains("không dừng") || msg.contains("KHÔNG dừng"),
                "{v:?} không nói rõ là chưa dừng gì: {msg}"
            );
        }
        assert!(
            Verdict::Unavailable("x".into()).refusal().contains("claude stop"),
            "lúc hỏng đường hỏi phải chỉ ra đường thoát"
        );
    }

    #[test]
    fn a_confirmation_has_nothing_to_refuse() {
        assert_eq!(Verdict::Confirmed.refusal(), "");
    }
}
