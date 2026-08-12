//! Bắt lúc một phiên **vừa xong việc** hoặc **vừa tắt hẳn**, và nói ra ĐÚNG MỘT LẦN.
//!
//! # Vì sao có tệp này
//!
//! Hà 2026-08-10: *"có bắt được trường hợp đang chạy và dừng lại hoàn toàn
//! không? nếu có thì thể hiện được trên ui và gửi vào tele"*.
//!
//! Đây là thứ ảnh chụp KHÔNG trả lời được, dù nó mang đủ dữ liệu: ảnh chụp nói
//! *"lúc này phiên đang rảnh"*, còn cái người ta cần biết là *"nó VỪA chuyển từ
//! chạy sang rảnh"* — một sự kiện, không phải một trạng thái. Sự kiện chỉ hiện
//! ra khi có hai lượt đo đặt cạnh nhau, nên phải có sổ ghi lượt trước.
//!
//! # Ba luật của cái loa này
//!
//! 1. **Nói một lần.** Vòng chạy lặp mỗi ~10 giây; báo theo trạng thái thay vì
//!    theo chuyển-trạng-thái là một cái điện thoại rung mãi không thôi, và một
//!    cái loa như thế thì người ta tắt — mất luôn cả những lần đáng nghe.
//! 2. **Lượt đầu im.** Khi hub vừa khởi động lại, sổ trống nên MỌI phiên đều
//!    "mới thấy lần đầu". Báo hết là một tràng tin cho những việc xảy ra lúc
//!    hub còn chưa chạy. Lượt đầu chỉ ghi sổ, không nói gì.
//! 3. **Biến mất cũng là kết thúc.** `claude agents` bỏ một phiên đã dừng khỏi
//!    danh sách sau vài giây (đã ghi ở `pipeline::STOPPED_KEY`), nên phần lớn
//!    lần "tắt hẳn" KHÔNG đi qua trạng thái `dead` — nó chỉ đơn giản là không
//!    còn trong danh sách nữa. Chỉ rình `host == "dead"` là bỏ lọt gần hết.

use std::collections::BTreeMap;

use serde_json::json;

use crate::logging;
use crate::sessions::LiveSession;

/// Trạng thái của một phiên, rút gọn còn đúng thứ cần để so hai lượt.
pub const WORKING: &str = "working";
pub const IDLE: &str = "idle";
pub const DEAD: &str = "dead";

/// Chạy ngắn hơn chừng này thì XONG không phải là tin.
///
/// Đo thật ngay lượt đầu bật loa (2026-08-10): `hub-bd` bắn "vừa chạy xong" hai
/// lần cách nhau 75 giây, và cả hai đều ĐÚNG — nó chạy hai lượt ngắn thật. Đúng
/// mà vẫn sai chỗ: một phiên đang có người ngồi gõ sẽ kêu một tiếng mỗi lượt,
/// mà người ấy đang nhìn thẳng vào nó. Cái loa này có giá trị ở phiên KHÔNG ai
/// nhìn — nơi một lượt chạy dài rồi dừng là thứ đáng gọi người ta quay lại.
///
/// 120 giây: dài hơn một lượt hỏi-đáp thường, ngắn hơn một việc đáng chờ.
pub const MIN_RUN_SEC: i64 = 120;

/// Sống ngắn hơn chừng này thì TẮT cũng không phải là tin.
///
/// 🔴 Đo 2026-08-12 từ chính log của hub: **20 tin "đã tắt hẳn" trong 4 tiếng,
/// mỗi tin một id khác nhau**, đều đặn 7–12 phút một lần. Không phải một phiên
/// báo lặp — là một dòng phiên sinh ra rồi chết, và thủ phạm là **phép dò hạn
/// mức của chính hub**: `claude -p "/usage"` mỗi 5 phút đẻ ra một phiên thật,
/// hiện vài giây trong `claude agents` rồi biến mất. Cái loa làm đúng luật đã
/// viết; luật thiếu vế "sống bao lâu".
///
/// Dùng chung con số với `MIN_RUN_SEC` là có chủ ý: cùng một câu hỏi ("việc này
/// có đủ dài để đáng gọi người ta không"), nên đừng để hai con số trôi khác nhau.
pub const MIN_LIFE_SEC: i64 = MIN_RUN_SEC;

/// Ghi trong sổ: `working@<epoch giây>` để biết nó chạy được bao lâu rồi.
fn working_since(mark: &str) -> Option<i64> {
    mark.strip_prefix("working@")?.parse().ok()
}

/// Những gì sổ phải nhớ về một phiên để nói ĐÚNG lúc nó biến mất.
///
/// Hà 2026-08-10: *"tắt hẳn là sao? ý chung chung thế… tắt hẳn là phải thoát
/// khỏi cli mới đúng, tắt hẳn terminal"*. Đúng — và cùng một lỗi với câu "đang
/// đứng ở dấu nhắc" đã vá trước đó: nói một điều hub không biết.
///
/// "Biến khỏi danh sách `claude agents`" xảy ra vì ít nhất ba lý do khác hẳn
/// nhau: phiên NỀN bị dừng (chẳng liên quan terminal), `claude` thoát mà cửa sổ
/// vẫn mở, hoặc cửa sổ terminal đóng luôn. Phân biệt được cả ba — nhưng phải
/// giữ `tty` và `kind` TỪ TRƯỚC, vì lúc phiên biến mất thì hàng của nó cũng đi
/// theo và không còn gì để hỏi.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mark {
    /// `working@<epoch>` · `idle`
    pub s: String,
    /// tty lúc còn thấy nó — rỗng nếu phiên không gắn cửa sổ nào.
    #[serde(default)]
    pub y: String,
    /// `interactive` · `background`…
    #[serde(default)]
    pub k: String,
    /// Phiên CHA, nếu phiên này do một phiên khác đẻ ra (rỗng = phiên gốc).
    ///
    /// Hà 2026-08-11: *"phiên con được gọi từ phiên cha mà tắt cũng đang gửi
    /// qua tele, có cần không?"* — không. Phiên con kết thúc bình thường là
    /// một CHI TIẾT trong lượt làm việc của phiên cha, và phiên cha sẽ tự báo
    /// khi nó xong; hai tin cho một việc thì tin nào cũng mất giá.
    /// Cùng lý do phải nhớ `tty`: lúc phiên biến mất thì hàng của nó đi theo,
    /// nên quan hệ cha-con phải nằm trong sổ TỪ TRƯỚC.
    #[serde(default)]
    pub p: String,
    /// Lần ĐẦU hub thấy phiên này (epoch giây). 0 = sổ cũ, chưa có trường này.
    ///
    /// 🔴 Hà 2026-08-12: *"tại sao cứ báo phiên đã tắt liên tục"*. Đo log: 20
    /// tin trong 4 tiếng, **mỗi tin một id khác nhau**, đều đặn 7–12 phút một
    /// lần — tức không phải một phiên báo lặp, mà là một dòng phiên sinh ra rồi
    /// chết. Đó chính là **phép dò hạn mức của hub**: `claude -p "/usage"` chạy
    /// mỗi 5 phút, và mỗi lượt đẻ ra một phiên thật, mang một id thật, hiện ra
    /// trong `claude agents` vài giây rồi biến mất. Cái loa làm đúng luật đã
    /// viết ("rời khỏi danh sách = đã kết thúc") — luật ấy thiếu một vế.
    ///
    /// Vế thiếu: **một phiên sống vài giây thì cái chết của nó không phải tin**,
    /// cùng lý do `MIN_RUN_SEC` tồn tại cho "vừa chạy xong".
    #[serde(default)]
    pub f: i64,
    /// Phiên do CHÍNH hub mở (`/new`).
    ///
    /// Ngoại lệ của cửa thời lượng trên: phiên chủ máy vừa mở từ điện thoại mà
    /// chết trong 30 giây là **đúng thứ phải báo** — nó chết chứ không phải nó
    /// xong, và người mở đang chờ nó chạy.
    #[serde(default)]
    pub h: bool,
    /// Tên phiên, và dự án nó đang làm — nhớ TỪ TRƯỚC vì lúc báo thì đã muộn.
    ///
    /// 🔴 Hà 2026-08-12: *"không biết nó là phiên nào rất mơ hồ"*. Tin cũ đọc là
    /// `⏹ phiên 8db91183 đã tắt hẳn` — một id ngắn, thứ không nói được gì cho
    /// người đang cầm điện thoại. Lý do nó chỉ có id: khi phiên **rời khỏi danh
    /// sách** thì hàng của nó đi theo, không còn chỗ nào hỏi tên nữa. Cùng lý do
    /// sổ phải nhớ `tty` và `p` — nhớ luôn tên và dự án.
    #[serde(default)]
    pub n: String,
    #[serde(default)]
    pub d: String,
}

/// Một chuyện vừa xảy ra, đáng để làm phiền chủ máy.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Đang chạy → đứng lại ở dấu nhắc. Lượt việc đã xong.
    Finished { id: String, name: String, ran_sec: i64 },
    /// Rời khỏi danh sách hoặc mất tiến trình. **Không tự nhận là "tắt hẳn"** —
    /// chỗ gọi còn phải dò cửa sổ terminal mới biết nói câu nào (xem `Mark`).
    Ended {
        id: String,
        name: String,
        was_working: bool,
        tty: String,
        kind: String,
        /// Phiên cha, nếu có — xem `Mark::p`.
        parent: String,
    },
}

/// Phiên đang thật sự ở trạng thái nào lúc nó im — NHÌN, không đoán.
///
/// Hà 2026-08-10, đọc tin trên Telegram: *"rõ ràng là lỗi mà sao tele tôi nhận
/// được lại là phiên đang đứng ở dấu nhắc, chờ lượt sau"* — và *"toàn thông báo
/// giống nhau"*. Cả hai đều đúng, và vế đầu nặng hơn: câu ấy là một KHẲNG ĐỊNH
/// hub không hề biết. Thứ hub biết là "nhật ký thôi lớn lên"; mà nhật ký cũng
/// thôi lớn lên khi phiên kẹt ở hộp thoại, khi lỗi, khi hết hạn mức.
///
/// Nên lúc CHUYỂN trạng thái — chuyện hiếm, vài lần một giờ — hub bỏ ra đúng
/// một lần đọc màn cho riêng phiên ấy. Đọc màn cho MỌI phiên MỖI vòng mới là
/// thứ từng kéo một vòng lên 90 giây; một lần cho một phiên lúc nó vừa im thì
/// gần như không tốn gì, và nó đổi một câu đoán thành một câu nhìn thấy.
#[derive(Debug, Clone, PartialEq)]
pub enum Idle {
    /// Màn đang có hộp chọn: phiên KHÔNG rảnh, nó đang chờ người trả lời.
    ///
    /// `options` mang NGUYÊN VĂN từng lựa chọn khi đọc được màn; rỗng khi màn bị
    /// giữ lại vì có dấu hiệu bí mật (`keys::Look::Withheld`) — lúc ấy chỉ còn
    /// con số, và câu nói phải khai luôn là vì sao.
    ///
    /// Hà 2026-08-11: *"cần thêm thông tin mô tả liên quan tới lựa chọn đó mới
    /// hợp lý"*. Đúng: `keys::parse_choices` đã bóc được chữ của từng lựa chọn
    /// từ 2026-08-10, mà tin báo lại chỉ mang con số — một cái chuông nói "có 3
    /// lựa chọn" thì vẫn bắt người ta mở máy ra mới biết chọn gì, tức nó chưa
    /// tiết kiệm cho ai một bước nào.
    Asking { n: usize, options: Vec<String> },
    /// Đứng ở dấu nhắc thật.
    Prompt,
    /// Không đọc được màn — nói đúng chừng ấy, đừng đoán hộ.
    Unknown,
}

impl Change {
    /// Câu nói cho phòng chat và cho Telegram — cùng một câu, vì hai nơi ấy
    /// phải kể cùng một chuyện. Khác câu là sau này không ai đối chiếu được.
    ///
    /// `idle` là thứ NHÌN THẤY trên màn lúc phiên im; `tail` là câu cuối phiên
    /// nói ra. Không có `tail` thì mọi tin giống hệt nhau và người ta thôi đọc
    /// — đó là lời phàn nàn thứ hai của Hà, và nó đúng.
    pub fn say(&self, idle: &Idle, tail: Option<&str>) -> String {
        match self {
            Change::Finished { name, ran_sec, .. } => {
                // Một câu phải trả lời được: CÓ CẦN MÌNH LÀM GÌ KHÔNG.
                let what = match idle {
                    // Đọc được chữ ⟹ ĐƯA CHỮ RA. Nó đã qua cổng quét rò rỉ ở
                    // `keys::look` (màn có dấu hiệu bí mật thì rơi sang nhánh
                    // dưới), nên đây không phải chỗ để cẩn thận thêm lần nữa —
                    // chỉ cắt cho vừa một cái chuông: 5 dòng, mỗi dòng 80 ký tự.
                    Idle::Asking { n, options } if !options.is_empty() => {
                        let lines: Vec<String> = options
                            .iter()
                            .take(5)
                            .enumerate()
                            .map(|(i, o)| format!("{}. {}", i + 1, crate::exec::truncate(o, 80)))
                            .collect();
                        let more = if *n > lines.len() {
                            format!("\n… và {} lựa chọn nữa", n - lines.len())
                        } else {
                            String::new()
                        };
                        format!(
                            "⚠ {name} dừng lại HỎI — cần bạn chọn:\n{}{more}",
                            lines.join("\n")
                        )
                    }
                    // Không đọc được chữ thì nói RÕ vì sao chỉ có con số, đừng
                    // để người ta tưởng hub keo kiệt thông tin.
                    Idle::Asking { n, .. } => format!(
                        "⚠ {name} dừng lại HỎI ({n} lựa chọn) — màn có dấu hiệu bí mật nên hub \
                         không đưa nội dung ra; mở phiên trên máy để đọc"
                    ),
                    Idle::Prompt => format!(
                        "⏸ {name} dừng, đang chờ bạn — sau {} phút chạy",
                        ran_sec / 60
                    ),
                    Idle::Unknown => format!(
                        "⏸ {name} dừng sau {} phút — không đọc được màn nên chưa rõ nó chờ gì",
                        ran_sec / 60
                    ),
                };
                match tail {
                    Some(t) if !t.trim().is_empty() => format!("{what}\n\n«{}»", t.trim()),
                    _ => what,
                }
            }
            // `Ended` KHÔNG dựng câu ở đây: câu đúng phụ thuộc vào việc cửa
            // sổ terminal còn hay mất, mà đó là một phép dò (I/O). Xem
            // `pipeline::announce_changes`.
            Change::Ended { name, .. } => format!("⏹ {name} — kết cục chưa xác định"),
        }
    }
}

/// Gọi tên một phiên ĐÃ BIẾN MẤT, bằng những gì sổ còn giữ.
///
/// Ba dữ kiện xếp theo thứ người ta nhận ra: **tên** phiên · **dự án** nó đang
/// làm · id ngắn để gõ tiếp lệnh. Thiếu tên (sổ ghi từ bản cũ) thì nói thẳng
/// "phiên <id>" — một id không có nhãn còn đỡ hơn một cái tên bịa.
pub fn name_from_mark(id: &str, mark: &Mark) -> String {
    let short = &id[..id.len().min(8)];
    match (mark.n.trim(), mark.d.trim()) {
        ("", _) => format!("phiên {short}"),
        (name, "") => format!("{name} ({short})"),
        (name, folder) => format!("{name} · {folder} ({short})"),
    }
}

/// Trạng thái rút gọn của một phiên trong ảnh chụp lúc này.
fn state_of(s: &LiveSession) -> &'static str {
    if s.host == "dead" {
        DEAD
    } else if s.working {
        WORKING
    } else {
        IDLE
    }
}

/// So sổ cũ với ảnh chụp mới → những chuyện đáng nói + sổ mới.
///
/// Thuần: không đọc đĩa, không gọi mạng, nên kiểm được đủ mọi ca mà không cần
/// một cái máy đang chạy `claude`.
///
/// `first_run` (sổ cũ rỗng) trả về **không sự kiện nào** — xem luật 2 ở đầu tệp.
pub fn changes(
    prev: &BTreeMap<String, Mark>,
    now: &[LiveSession],
    epoch_sec: i64,
) -> (Vec<Change>, BTreeMap<String, Mark>) {
    let mut next: BTreeMap<String, Mark> = BTreeMap::new();
    let mut out: Vec<Change> = Vec::new();
    let first_run = prev.is_empty();

    for s in now {
        let state = state_of(s);
        let before = prev.get(&s.session_id);
        let was_working = before.is_some_and(|b| b.s.starts_with(WORKING));
        // Mốc bắt đầu chạy: giữ nguyên nếu đang chạy tiếp, đặt mới nếu vừa bắt
        // đầu. Không có mốc thì lấy lúc này — thiếu chính xác một lượt, và lượt
        // ấy sẽ bị coi là ngắn, tức im. Thà lỡ một tin còn hơn một tin sai.
        let since = if was_working {
            before.and_then(|b| working_since(&b.s)).unwrap_or(epoch_sec)
        } else {
            epoch_sec
        };
        // Phiên đã chết vẫn nằm trong danh sách vài giây; đừng ghi nó vào sổ
        // mới, nếu không lần sau nó lại "biến mất" và báo tắt lần thứ hai.
        match state {
            WORKING | IDLE => {
                next.insert(
                    s.session_id.clone(),
                    Mark {
                        s: if state == WORKING {
                            format!("{WORKING}@{since}")
                        } else {
                            IDLE.to_string()
                        },
                        y: s.tty.clone(),
                        k: s.kind.clone(),
                        p: s.parent_session_id.clone().unwrap_or_default(),
                        // Giữ nguyên mốc lần đầu; sổ cũ (f = 0) coi như thấy từ
                        // bây giờ, tức lượt sau nó mới đủ tuổi để báo — thà lỡ
                        // một tin còn hơn một tin sai.
                        f: before.map(|b| b.f).filter(|f| *f > 0).unwrap_or(epoch_sec),
                        h: s.started_by_hub,
                        n: s.name.clone(),
                        d: s.folder.clone(),
                    },
                );
            }
            _ => {}
        }
        if first_run {
            continue;
        }
        if was_working && state == IDLE {
            // Cửa thời lượng: chạy chớp nhoáng thì không phải tin.
            if epoch_sec - since >= MIN_RUN_SEC {
                out.push(Change::Finished {
                    id: s.session_id.clone(),
                    name: s.name.clone(),
                    ran_sec: epoch_sec - since,
                });
            }
        } else if before.is_some() && state == DEAD {
            out.push(Change::Ended {
                id: s.session_id.clone(),
                name: s.name.clone(),
                was_working,
                tty: s.tty.clone(),
                kind: s.kind.clone(),
                parent: s.parent_session_id.clone().unwrap_or_default(),
            });
        }
    }

    // Rời khỏi danh sách = đã kết thúc. Đây mới là đường CHÍNH, không phải `dead`.
    if !first_run {
        let seen: Vec<&String> = now.iter().map(|s| &s.session_id).collect();
        for (id, mark) in prev {
            if seen.contains(&id) {
                continue;
            }
            // Cửa TUỔI THỌ — xem `Mark::f`. Phiên sống chớp nhoáng (phép dò hạn
            // mức của chính hub, một `claude -p` bất kỳ) chết đi không phải tin;
            // phiên do hub mở thì luôn báo, vì ở đó chết ≠ xong.
            let lived = epoch_sec - mark.f;
            if mark.f > 0 && lived < MIN_LIFE_SEC && !mark.h {
                logging::info(
                    "session_end_muted",
                    json!({ "session": id, "lived_sec": lived,
                            "why": "phiên sống chớp nhoáng — cái chết của nó không phải tin" }),
                );
                continue;
            }
            out.push(Change::Ended {
                id: id.clone(),
                // Tên lấy TỪ SỔ (xem `Mark::n`): hàng của phiên đã đi mất cùng
                // danh sách. Sổ cũ chưa có tên thì đành id ngắn — nhưng nói rõ
                // đó là id, đừng để người đọc tưởng đấy là tên.
                name: name_from_mark(id, mark),
                was_working: mark.s.starts_with(WORKING),
                // ĐÂY là lý do sổ phải nhớ `tty`: hàng của phiên đã biến mất,
                // nên không còn chỗ nào hỏi nó chạy ở cửa sổ nào.
                tty: mark.y.clone(),
                kind: mark.k.clone(),
                parent: mark.p.clone(),
            });
        }
    }

    (out, next)
}
