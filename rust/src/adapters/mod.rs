//! Từ vựng của KÊNH: một mệnh lệnh gõ trên điện thoại trông như thế nào khi nó
//! tới được `pipeline`.
//!
//! Was five: GitHub notifications, project devlogs, email and Telegram all fed
//! an inbox that a bounded `claude -p` call triaged. That product is gone
//! (2026-08-08); `git show backup/inbox-adapters` still has the four ingest
//! adapters.
//!
//! 🔴 Rồi còn một — phòng chat tfl5 — và ngày 2026-08-14 còn KHÔNG. Kênh duy
//! nhất nay là Telegram, và nó khác cả bốn cái cũ ở một điểm đổi được hình dạng
//! của tệp này: **nó không bị hỏi vòng, nó tự đẩy tới**. Nên `PollResult` (số
//! dòng đã lướt qua, con trỏ kiếm được, `Skip` vì thiếu khoá) đi theo chặng hỏi
//! vòng — xem chỗ `pipeline::ingest` từng đứng. Còn lại đúng phần không phụ
//! thuộc kênh nào: một mệnh lệnh là gì, và trả lời nó ở đâu.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Help,
    /// Dựng lại chính hub từ mã hiện tại (`runtime::self_install`).
    ///
    /// 🔴 Hà 2026-08-13: *"tại sao không phải là luồng chạy độc lập trên rust,
    /// tức là mọi lệnh và luồng xử lý phải nằm trong binary"*. Trước route này,
    /// mỗi bản vá của hub đều đòi một người ngồi ở máy gõ `deploy/install.sh` —
    /// tức cây cầu tự nó có một đoạn chỉ đi được khi chủ máy đang ở nhà.
    Upgrade,
    /// Run a full cycle now (the console's "Chạy 1 vòng").
    Run,
    /// Probe channels + tools for real, ignoring the cached reading (the
    /// console's "Kiểm tra").
    Doctor,
    /// Set ONE config field: `arg` is "<dotted.key> <value>".
    SetConfig,
    /// Focus one Claude CLI session so the next snapshot carries its full
    /// stream. `arg` is the session id, or "-" to stop following.
    ///
    /// Focus rather than "fetch": the page cannot call this machine, so the
    /// only way it sees anything is what hubd pushes — and pushing every
    /// session's whole transcript every cycle would be megabytes for the one
    /// session being read.
    Session,
    /// Close the books on a Claude session and open a new one that continues
    /// its thread. `arg` is the session id, or empty for the focused one.
    Handover,
    /// Start a background session for a project. `arg` is "<project> <việc>".
    New,
    /// Stop the focused background session, keeping its conversation.
    Stop,
    /// Continue the focused background session IN PLACE — a real next turn on
    /// the same thread, unlike `Ask` which forks. `arg` is what to say.
    Tell,
    /// `/type <chữ>` — gõ THẲNG vào cửa sổ terminal của phiên đang theo.
    ///
    /// Khác `Tell` ở chỗ căn bản: `Tell` chạy `claude --resume`, tức mở một
    /// lượt mới trên nhật ký và chỉ dùng được cho phiên nền ĐÃ DỪNG. `Type` gõ
    /// phím vào phiên **đang chạy**, kể cả phiên interactive Hà mở tay — đó là
    /// đường DUY NHẤT trả lời một hộp chọn đang chờ, thứ không nằm trong nhật
    /// ký nên không có API nào chạm tới.
    ///
    /// ⚠ Đường này **bỏ qua `DENIED_TOOLS`**: chữ gõ vào terminal không đi qua
    /// bộ khoá nào. Hà chốt 2026-08-09 sau khi được nêu rõ đánh đổi.
    Type,
    /// `/shot` — chụp cửa sổ terminal của phiên đang theo và đẩy ảnh lên màn.
    ///
    /// Đây là đường DUY NHẤT nhìn thấy thứ đang hiện trên màn mà chưa vào nhật
    /// ký: hộp chọn đang chờ, thanh tiến trình, lỗi vừa in ra. `sessions::
    /// stream` đọc tệp, mà tệp chỉ có sau khi lượt kết thúc.
    Shot,
    /// `/key <tên phím>` — một phím điều khiển: up · down · enter · esc · tab ·
    /// space · 1-9. Hộp chọn của `claude` đi bằng mũi tên, gửi chữ "xuống" vào
    /// đó thì nó gõ ra chữ chứ không di chuyển.
    Key,
    /// `/pick <câu>.<lựa chọn>` — trả lời MỘT CÂU BẤT KỲ của bảng hỏi nhiều câu.
    ///
    /// 🔴 Hà 2026-08-13: *"chọn option xong thì vẫn còn bước nữa nên không pass
    /// qua được"* · *"có nhiều option thì phải có cơ chế chọn được nhiều"*.
    /// `/key` gửi đúng một phím vào câu ĐANG MỞ, nên từ điện thoại chỉ trả lời
    /// được câu đầu; các câu sau nằm sau một phím mũi tên mà `arrow_verdict` —
    /// đúng luật của nó — từ chối gửi khi màn đang có hộp chọn. Kết quả: bảng
    /// nhiều câu không bao giờ đủ ô để gửi, và điện thoại không có đường nào đi
    /// tiếp. Route này đi được vì nó KHÔNG gửi mũi tên trần: nó ghép cả dãy
    /// (mũi tên + số) vào MỘT `do script`, nên chỉ có đúng một dấu xuống dòng,
    /// ở cuối, chỗ mình chọn.
    Pick,
    /// `/run_<n>` — chạy lệnh thứ `n` trong sổ lệnh vừa thấy trên màn.
    ///
    /// Anh em sinh đôi của nút `run:<n>`, khác đúng một chỗ và chỗ ấy là cả lý
    /// do nó tồn tại: nó là **chữ nằm trong tin**, nên Telegram tự tô sáng và
    /// chạm là chạy — không cần một khối nút ở cuối tin. `arg` là con số; dòng
    /// lệnh thật nằm trong sổ (`pipeline::quick_cmd`), nên nó KHÔNG thể bị cắt
    /// cụt như một cái nhãn nút.
    RunQuick,
    /// Ask the focused session a question WITHOUT interrupting it. `arg` is the
    /// question; the target is whatever `/session` is following.
    ///
    /// The target is implicit on purpose: this is typed on a phone while
    /// looking at one session's stream, and asking a person to retype a uuid
    /// there is asking them not to use the feature.
    Ask,
    /// `/runin <id> <lệnh>` — hub chạy lệnh, rồi DÁN KẾT QUẢ vào phiên.
    ///
    /// 🔴 Hà 2026-08-13, sau khi biết dấu `!` chưa bao giờ bật chế độ bash:
    /// *"có lẽ nên gọi lệnh ở command khác rồi lấy kết quả dán gửi lại vào
    /// phiên"*. Đây là đường thứ ba, và nó tốt hơn cả hai đường đang có:
    ///
    /// | | ai chạy | tốn hạn mức | có tty | phải ngồi ở máy |
    /// |---|---|---|---|---|
    /// | `▶` nhờ phiên chạy | phiên (`claude`) | CÓ | không | không |
    /// | `🖥` cửa sổ mới | shell thật | không | CÓ | CÓ |
    /// | `/runin` | **hub** | **không** | không | **không** |
    ///
    /// Vì sao nó đáng có: nhờ phiên chạy là trả tiền hạn mức cho một việc
    /// `zsh -lc` làm được miễn phí, và bắt cả một lượt suy nghĩ chạy chỉ để gọi
    /// một dòng lệnh. Còn cửa sổ mới thì kết quả nằm trên màn hình máy — phiên
    /// KHÔNG thấy, nên nó không đi tiếp được.
    ///
    /// `/runin` tách đôi đúng chỗ: **máy chạy**, **phiên đọc**. Phiên nhận
    /// nguyên dòng lệnh kèm mã thoát và đầu ra, tức đủ để làm bước sau.
    ///
    /// Cùng hàng rào với `/cmd` (cùng `zsh -lc`, cùng gốc workspace, cùng trần
    /// thời gian) và thêm một cửa: đầu ra phải qua `redaction::file_risk` trước
    /// khi vào phiên — nó sẽ nằm lại trong nhật ký phiên, tức trên đĩa, mãi mãi.
    RunIn,
    /// `/close [id]` — ĐÓNG HẲN: thoát CLI rồi đóng luôn cửa sổ Terminal.
    ///
    /// 🔴 Hà 2026-08-13: *"chưa có lệnh đóng phiên đóng luôn cửa sổ?"* → *"đang
    /// đứng ở phiên nào thì cần lệnh đóng hẳn"* → *"ah stop là dừng rồi vậy
    /// dùng close"* → *"nếu trống thì đóng phiên đang đứng còn có id thì đóng
    /// phiên theo id"*.
    ///
    /// Tách khỏi `Stop` vì một động từ đang gánh hai kết cục khác hẳn nhau về
    /// mức mất mát: `/stop` dừng một phiên NỀN và giữ nguyên hội thoại (`/tell`
    /// nói tiếp được), còn với phiên có cửa sổ thì nó lại thoát CLI và đóng cửa
    /// sổ. Người bấm không có cách nào biết mình sắp nhận cái nào.
    ///
    /// Quy trình đóng **giữ nguyên như cũ** (Hà: *"trước khi đóng phải chờ cli
    /// chạy nốt mới đóng hẳn"*): gõ `/exit`, chờ tới 30 giây cho `claude` chạy
    /// hết lượt đang xếp hàng, còn bận thì TỪ CHỐI đóng — đóng lúc ấy bật hộp
    /// thoại "terminate running processes", thứ khoá mồm mọi lệnh sau nó.
    Close,
    /// `/accounts` — ba tài khoản `claude` trên máy này: phiên nào đang chạy
    /// bằng tài khoản nào, còn bao nhiêu hạn mức, và **`/new` không nói `@acc`
    /// thì rơi vào tài khoản nào**.
    ///
    /// Hà 2026-08-12: *"chưa có lệnh xem danh sách acc"* → *"vậy lệnh new chọn
    /// acc kiểu gì? hay đang để random?"*. Hai câu ấy là một câu: chọn tài
    /// khoản là một quyết định có hậu quả (hạn mức tuần cạn thì phiên mới chết
    /// giữa chừng), mà dữ liệu để quyết định chỉ nằm trên tab Sức khoẻ — thứ
    /// không với tới được khi đang gõ trên Telegram.
    Accounts,
}

#[derive(Debug, Clone)]
pub struct ChannelCommand {
    pub kind: CommandKind,
    /// Bước phụ do hub tự xếp hàng ⟹ chạy xong thì KHÔNG trả lời.
    pub quiet: bool,
    /// The id the command acts on: a DECISION id for `Approve`/`Reject`, a
    /// MESSAGE id for `Close`/`Reply` (the two CLI verbs take message ids), and
    /// 0 when the command needs neither (`Help`).
    pub decision_id: i64,
    /// Free text following the id — a reject reason, say.
    pub arg: String,
    /// Where to acknowledge the press (chat id).
    pub chat_id: String,
    /// Telegram requires answering the callback or the button spins forever.
    pub callback_id: String,
    /// The message carrying the buttons, so it can be edited after the action.
    pub message_id: Option<i64>,
}

// 🔴 `Skip` đã bỏ 2026-08-14. Nó là "thiếu khoá thì bỏ qua CÓ GHI SỔ, không
// phải chết máy" — luật #4, và luật ấy còn nguyên; chỉ là nay nó được thi hành
// ngay tại kênh (`telegram::Inbox::start` không thấy khoá thì không dựng luồng
// và nói ra), chứ không còn đi qua một dòng `runs` của chặng hỏi vòng.

/// Kênh có sống không, viết cho người đọc. `telegram::health` dựng nó; `/doctor`
/// là chỗ đọc.
#[derive(Debug, Clone)]
pub struct Health {
    pub ok: bool,
    pub detail: String,
}
