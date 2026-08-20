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
//! 3. **Chặn vòng chạy trong lúc chờ.** huba cố ý đồng bộ; thêm luồng nền chỉ để
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
    ///
    /// `nothing_done` là việc đã KHÔNG xảy ra, viết như một mệnh đề ngắn:
    /// `"dừng phiên nào"`, `"đóng sổ phiên nào"`. Nó phải nằm trong câu, vì một
    /// lời từ chối không nói rõ cái gì đã không xảy ra sẽ bị đọc thành "hỏng
    /// rồi, chắc mất phiên" — đúng nỗi lo mà chốt chặn này sinh ra để dập.
    pub fn refusal(&self, nothing_done: &str) -> String {
        match self {
            Verdict::Confirmed => String::new(),
            Verdict::Declined => format!("✋ Đã huỷ trên Telegram — không {nothing_done}."),
            Verdict::TimedOut => format!(
                "⌛ Hết hạn chờ xác nhận trên Telegram — không {nothing_done}. Bấm lại nếu vẫn muốn."
            ),
            Verdict::Unavailable(why) => format!(
                "⚠ Không hỏi được Telegram nên KHÔNG {}: {}. \
                 Đường thoát luôn còn: ngồi vào máy và làm thẳng trên terminal.",
                nothing_done,
                crate::exec::truncate(why, 160)
            ),
        }
    }
}

/// Hỏi chủ máy một câu qua Telegram và chờ bấm nút.
///
/// `what` là câu mô tả việc sắp làm, đã đủ cụ thể để đọc trên điện thoại mà
/// không cần mở gì thêm ("Dừng phiên sdvi-a1b2 (acc2)?").
/// Gửi một tin THƯỜNG sang Telegram — không nút, không chờ, không hỏi gì.
///
/// Tách ra khỏi `ask` vì hai việc khác hẳn nhau: `ask` **đứng chờ tới 90 giây**
/// một cú bấm và trả về phán quyết, còn cái này chỉ báo cho người đang không
/// nhìn màn hình. Gọi `ask` để báo tin thì mỗi lời báo sẽ ghim vòng chạy của
/// daemon lại một phút rưỡi.
///
/// Trả `Err` chứ không nuốt: một cái loa hỏng mà im lặng thì tệ hơn không có
/// loa — chỗ gọi phải log.
pub fn tell(cfg: &Config, text: &str) -> Result<(), String> {
    let (token, chat_id) = match (
        crate::config::secret_from_env(&cfg.confirm.bot_token_env),
        crate::config::secret_from_env(&cfg.confirm.chat_id_env),
    ) {
        (Some(t), Some(c)) => (t, c),
        (t, c) => {
            // Chỉ TÊN khoá, không bao giờ giá trị (luật §4).
            let missing: Vec<&str> = [
                (t.is_none()).then_some(cfg.confirm.bot_token_env.as_str()),
                (c.is_none()).then_some(cfg.confirm.chat_id_env.as_str()),
            ]
            .into_iter()
            .flatten()
            .collect();
            return Err(format!("thiếu {} trong huba.env", missing.join(" + ")));
        }
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let r = client
        .post(format!("https://api.telegram.org/bot{token}/sendMessage"))
        .json(&json!({ "chat_id": chat_id, "text": text }))
        .send()
        .map_err(|e| e.to_string())?;
    let v: Value = r.json().unwrap_or_else(|_| json!({}));
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        // Nhặt `message_id` NGAY: đây là lần duy nhất nó tồn tại, và không có
        // nó thì tin này không bao giờ xoá được (xem `telegram::remember_sent`).
        crate::telegram::remember_sent(cfg, &v);
        Ok(())
    } else {
        Err(v
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("Telegram từ chối sendMessage")
            .to_string())
    }
}

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
            return Verdict::Unavailable(format!("thiếu {} trong huba.env", missing.join(" + ")));
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

    // Nonce chỉ để ghép câu trả lời với câu hỏi, không phải thứ để chống giả
    // mạo — cái chống giả mạo là `from.id`. Không kéo thêm crate ngẫu nhiên vào
    // chỉ vì việc này.
    let nonce = format!("{}", chrono::Utc::now().timestamp_millis());

    // KHÔNG mở đường đọc thứ hai — ĐĂNG KÝ ở đường đã có.
    //
    // 🔴 2026-08-16. Bản cũ gọi `inbox.hold()` rồi tự chạy `getUpdates`, tin
    // rằng cái cờ ấy làm vòng nền đứng im. Nó không: `hold()` bật cờ trong khi
    // vòng nền đang nằm giữa một long-poll 20 giây, và không có cách nào gọi
    // một long-poll về. Hai vòng cùng hỏi ⟹ Telegram từ chối một bên
    // (`Conflict: terminated by other getUpdates request`) — 11 lượt trong
    // `logs/huba.log` ngày 16/08, mỗi lượt kèm 30 giây vòng nền ngủ phạt, tức
    // 30 giây huba điếc ngay sau mỗi câu hỏi xác nhận.
    //
    // Nay hàm này không hỏi Telegram câu nào: nó để lại `nonce` ở hòm thư và
    // ngồi chờ. Đăng ký TRƯỚC `sendMessage`, vì khoảng giữa "tin hiện trên điện
    // thoại" và "bắt đầu chờ" là một cửa sổ mất cú bấm.
    let waiting = crate::telegram::inbox().map(|i| i.expect_confirm(&nonce));

    // Đường lùi: KHÔNG có hòm thư (CLI một lượt, kênh tắt) thì tự đọc như cũ.
    // Chỉ nhánh này còn dùng `offset`/`watermark`.
    let mut offset = if waiting.is_some() {
        0
    } else {
        match watermark(&client, &api("getUpdates")) {
            Ok(o) => o,
            Err(e) => return Verdict::Unavailable(format!("không đọc được getUpdates: {e}")),
        }
    };
    let body = json!({
        "chat_id": chat_id,
        "text": format!("🔒 huba xin xác nhận\n\n{what}\n\nKhông bấm gì trong {}s = không làm.", cfg.confirm.timeout_sec),
        "reply_markup": {
            "inline_keyboard": [[
                { "text": "✅ Xác nhận", "callback_data": format!("ok:{nonce}") },
                { "text": "✖ Huỷ",      "callback_data": format!("no:{nonce}") }
            ]]
        }
    });
    // Câu hỏi này là một tin MỚI ở đáy buồng chat ⟹ câu xác nhận trơn đang mở
    // thôi là tin cuối, và sửa nó sau đó là sửa một dòng nằm trên câu hỏi
    // (`telegram::fold_ack`). Đây là cửa gửi duy nhất KHÔNG đi qua `Inbox`, nên
    // nó phải tự gọi — thiếu chỗ này là cả luật hở đúng một lối.
    if let Some(i) = crate::telegram::inbox() {
        i.forget_ack_live();
    }
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
    let verdict = if let Some(w) = &waiting {
        // Đường CHÍNH: vòng đọc của hòm thư giao cú bấm tận tay. Cổng "ai bấm"
        // (`callback_query.from.id`) đã đứng ở đó, trước khi tới sổ chờ — xem
        // `telegram::handle_update`, luật 7 của dự án.
        match w.wait(deadline.saturating_duration_since(Instant::now())) {
            Some(data) if data.starts_with("ok:") => Verdict::Confirmed,
            Some(_) => Verdict::Declined,
            None => Verdict::TimedOut,
        }
    } else {
        confirm_poll(&client, &api, &nonce, &chat_id, what, &mut offset, deadline)
    };

    // Ghi kết cục ngay lên chính tin nhắn ấy: hòm Telegram phải đọc được là
    // "đã bấm gì, và huba đã hiểu thế nào", chứ không để lại một câu hỏi treo.
    if let Some(mid) = message_id {
        let stamp = match &verdict {
            Verdict::Confirmed => "✅ Đã xác nhận",
            Verdict::Declined => "✖ Đã huỷ",
            Verdict::TimedOut => "⌛ Hết hạn — huba KHÔNG làm gì",
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
    // Rút tên khỏi sổ chờ ngay tại đây, không để tới cuối tầm: một cú bấm tới
    // sau lúc này phải nghe câu "đã đóng sổ", chứ không rơi vào một cái ống mà
    // không ai còn đọc.
    drop(waiting);
    verdict
}

/// Đường LÙI: tự đọc `getUpdates` khi tiến trình không có hòm thư nền.
///
/// Chỉ chạy khi `telegram::inbox()` là `None` — CLI một lượt, hoặc kênh tắt.
/// Trong `hubad` thì hòm thư luôn có, nên nhánh này không bao giờ chạy song song
/// với vòng đọc nào: luật 1 vẫn nguyên.
#[allow(clippy::too_many_arguments)]
fn confirm_poll(
    client: &reqwest::blocking::Client,
    api: &dyn Fn(&str) -> String,
    nonce: &str,
    chat_id: &str,
    what: &str,
    offset: &mut i64,
    deadline: Instant,
) -> Verdict {
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Verdict::TimedOut;
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
                logging::warn(
                    "confirm_poll_failed",
                    json!({ "err": crate::logging::redact(&e.to_string()) }),
                );
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        // 🔴 TRẢ LỜI ĐƯỢC ≠ TRẢ LỜI THUẬN. Bản cũ đọc thẳng `result` nên một lời
        // từ chối của Telegram (`Conflict`, token sai) ra đúng hình dạng "không
        // có update nào" — và hàm này ngồi hết 90 giây rồi kết luận *"không ai
        // bấm"*. Một lỗi im lặng, đúng thứ luật 3 cấm, ở ngay cái hàm đang hỏi
        // chủ máy có cho phép hay không. `telegram::poll_rejected` là cùng phép
        // đọc mà vòng đọc chính đã dùng từ trước.
        if let Some(why) = crate::telegram::poll_rejected(&resp) {
            logging::error("confirm_poll_rejected", json!({ "why": why, "what": what }));
            return Verdict::Unavailable(why);
        }
        let empty = vec![];
        let updates = resp
            .get("result")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        for u in updates {
            if let Some(id) = u.get("update_id").and_then(Value::as_i64) {
                *offset = (*offset).max(id + 1);
            }
            let Some(cb) = u.get("callback_query") else {
                continue;
            };
            let data = cb.get("data").and_then(Value::as_str).unwrap_or("");
            if !data.ends_with(nonce) {
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
            return if data.starts_with("ok:") {
                Verdict::Confirmed
            } else {
                Verdict::Declined
            };
        }
    }
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
            Verdict::Unavailable("thiếu HUB_TELEGRAM_BOT_TOKEN trong huba.env".into()),
        ] {
            for what in ["dừng phiên nào", "đóng sổ phiên nào"] {
                let msg = v.refusal(what);
                assert!(!msg.is_empty(), "{v:?} không có câu trả lời");
                assert!(
                    msg.contains(what),
                    "{v:?} không nói rõ việc gì đã KHÔNG xảy ra: {msg}"
                );
            }
        }
        assert!(
            Verdict::Unavailable("x".into())
                .refusal("dừng phiên nào")
                .contains("terminal"),
            "lúc hỏng đường hỏi phải chỉ ra đường thoát"
        );
    }

    #[test]
    fn a_confirmation_has_nothing_to_refuse() {
        assert_eq!(Verdict::Confirmed.refusal("dừng phiên nào"), "");
    }
}
