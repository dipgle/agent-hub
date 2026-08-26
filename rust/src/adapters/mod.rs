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
    /// Dựng lại chính huba từ mã hiện tại (`runtime::self_install`).
    ///
    /// 🔴 Hà 2026-08-13: *"tại sao không phải là luồng chạy độc lập trên rust,
    /// tức là mọi lệnh và luồng xử lý phải nằm trong binary"*. Trước route này,
    /// mỗi bản vá của huba đều đòi một người ngồi ở máy gõ `install_update.sh` —
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
    /// only way it sees anything is what hubad pushes — and pushing every
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
    /// `/anh` — ẢNH THẬT của màn hình, không phải chữ đọc từ tab.
    ///
    /// 🔴 Hà 2026-08-17: *"Thêm lệnh chụp ảnh màn hình để tôi xem thực sự đang
    /// có gì trên màn hình"* · *"Focus tới phiên thật"*.
    ///
    /// `/shot` đọc CHỮ của tab (`contents of selected tab`) — đủ cho hộp chọn và
    /// dòng lệnh, mà mù với mọi thứ không phải chữ: màu, con trỏ đang ở đâu, hộp
    /// thoại của macOS đè lên cửa sổ, và cả cái phần bị cuộn ra ngoài khung. Khi
    /// chữ đọc về "không nói lên điều gì" thì cây cầu phải có đường thứ hai.
    ///
    /// Nhánh chụp ảnh cũ bị xoá 2026-08-14 vì hồi ấy nó là đường DUY NHẤT và nó
    /// đắt (base64 vài trăm KB, đòi quyền Screen Recording). Nay nó quay lại
    /// đúng vai: đường phụ, chỉ chạy khi chủ máy gõ, và gửi PNG thật qua
    /// `sendPhoto` chứ không nhồi base64 vào chữ.
    Photo,
    /// `/front [id]` — đưa cửa sổ Terminal của phiên ấy ra TRƯỚC MẶT. Không
    /// chụp, không gõ, không gửi phím nào.
    ///
    /// 🔴 Hà 2026-08-22, sau khi gõ `/focus` trên điện thoại và không thấy gì:
    /// *"vậy muốn một phiên nổi lên thì làm thế nào"*. Trước route này câu trả
    /// lời là *"gõ `/anh`"* — tức muốn nhìn cửa sổ bằng MẮT THẬT thì phải trả
    /// giá bằng một tấm PNG đi qua Telegram, và phải có quyền Screen Recording.
    ///
    /// Đúng phép thử cầu nối trong `CLAUDE.md`: ngồi ở máy thì chỉ cần bấm vào
    /// cửa sổ. Cái gì làm được ở máy mà điện thoại không làm được là một GAP,
    /// và gap này đóng bằng đúng một lời gọi AppleScript.
    ///
    /// `focus` là một alias có chủ ý — đó là từ chủ máy gõ ra khi cần việc này.
    Front,
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
    /// `/tab <số>` — DI con trỏ sang tab ấy của bảng hỏi nhiều câu, rồi trả về
    /// chính màn ấy. KHÔNG chọn gì cả.
    ///
    /// 🔴 Hà 2026-08-19: *"mặc định thao tác trên máy muốn chuyển tab thì bấm
    /// phím phải trái, giờ qua tele thì có nút bấm ở chính tab để nhận như
    /// click chuột"*.
    ///
    /// Route này không dựng được suốt ba ngày, và lý do là một bức tường thật:
    /// mọi lượt ghi qua `do script` kèm một CR không tắt được, nên trên hộp
    /// chọn "sang phải" luôn kèm "chốt câu đang mở". Nó đứng được từ hôm nay là
    /// nhờ [`crate::cgkeys`] — phím rời gửi thẳng vào tiến trình Terminal, không
    /// qua `do script`, nên không có CR nào đi kèm. Đo: 12 lượt phím ngang,
    /// `answered` không nhúc nhích.
    ///
    /// Khác [`CommandKind::Pick`] ở chỗ căn bản: `Pick` **trả lời** một câu (và
    /// vì thế cần biết con trỏ đang ở đâu — thứ chỉ nhật ký nói được, mà nhật ký
    /// thì TRỐNG chừng nào bảng còn treo). `Tab` chỉ ĐI, và nó về mốc rồi đếm
    /// nên không cần biết đang đứng đâu — xem `keys::tab_keys`.
    Tab,
    /// `/clean [id]` — xoá SẠCH hàng chờ của phiên: những dòng đã gõ vào lúc nó
    /// đang bận và còn nằm đợi lượt sau.
    ///
    /// 🔴 Hà 2026-08-18: *"Thêm lệnh clean xóa hết ở chờ"*. Từ điện thoại,
    /// một câu gõ nhầm vào phiên đang chạy là một câu **không rút lại được**:
    /// nó sẽ chạy khi phiên rảnh, có khi nửa tiếng sau, và lúc ấy chẳng ai ngồi
    /// đó mà đọc. Ngồi ở máy thì bấm `↑` rồi xoá — đúng phép thử cây cầu, nên
    /// điện thoại phải làm được đúng chừng ấy.
    ///
    /// KHÔNG cắt lượt đang chạy (đó là `/key esc`): chỉ dọn phần chưa bắt đầu.
    ///
    /// 🔴 …VÀ DỌN NỐT Ô NHẬP — Hà 2026-08-26: *"Sửa lại lệnh clean và thêm lệnh
    /// clear để cùng có tác dụng xóa text ở ô chat"*.
    ///
    /// Bản cũ dừng ngay sau khi dọn hàng chờ, nên chữ vẫn nằm lại trong ô — mà
    /// chính `clear_queue` là thứ kéo nó vào đấy: nó bấm `↑` để lôi từng tin
    /// trong hàng chờ NGƯỢC VÀO ô nhập rồi xoá. Tin cuối cùng được lôi ra nằm
    /// lại, và người gõ `/clean` đọc thành "dọn chưa sạch".
    ///
    /// Nên thứ tự bắt buộc là **hàng chờ trước, ô nhập sau**; làm ngược lại là
    /// xoá một cái ô sắp được đổ đầy trở lại.
    Clean,
    /// `/clear [id]` — chỉ xoá **ô nhập**, không đụng hàng chờ.
    ///
    /// 🔴 Hà 2026-08-26, cùng câu trên. Phép xoá ô nhập vốn đã có
    /// (`keys::clear_box`) nhưng chỉ gọi được qua `/key clear` hoặc một liên kết
    /// `clr_<sid>` — tức nó nấp sau một lệnh nói về chuyện khác.
    ///
    /// Giữ RIÊNG với `/clean` chứ không gộp, vì hậu quả khác nhau: `/clear` chỉ
    /// bỏ chữ chưa gửi, còn `/clean` bỏ cả những tin ĐÃ xếp hàng chờ chạy — thứ
    /// mất đi thì không lấy lại được.
    Clear,
    /// `/run_<n>` — chạy lệnh thứ `n` trong sổ lệnh vừa thấy trên màn.
    ///
    /// Anh em sinh đôi của nút `run:<n>`, khác đúng một chỗ và chỗ ấy là cả lý
    /// do nó tồn tại: nó là **chữ nằm trong tin**, nên Telegram tự tô sáng và
    /// chạm là chạy — không cần một khối nút ở cuối tin. `arg` là con số; dòng
    /// lệnh thật nằm trong sổ (`pipeline::quick_cmd`), nên nó KHÔNG thể bị cắt
    /// cụt như một cái nhãn nút.
    RunQuick,
    /// `term_<mã>` — CÙNG dòng lệnh của `RunQuick`, chạy ở một CỬA SỔ riêng.
    ///
    /// 🔴 Hà 2026-08-16: *"tách thành 2 nút này để người dùng chủ động chọn"*.
    /// Hai kiểu chạy khác nhau ở thứ chúng để lại: `▶️` (RunQuick) chờ lệnh
    /// xong rồi dán bản tóm tắt vào phiên — tốt cho một lệnh ngắn có kết quả
    /// đáng đọc; `🖥` mở một cửa sổ Terminal và gõ lệnh vào đó — tốt cho lệnh
    /// dài, lệnh hỏi lại, hay lệnh chủ máy muốn ngồi nhìn. Ngồi ở máy thì hai
    /// việc ấy cũng là hai việc, nên cây cầu phải mang sang đủ cả hai.
    RunInTerminal,
    /// 📎 Gửi về Telegram một TỆP mà phiên vừa nhắc tới. `arg` là chỉ số trong
    /// sổ tệp (`pipeline::remember_files`).
    ///
    /// 🔴 Hà 2026-08-16: *"chưa chèn link tải file xuất hiện trong nội dung
    /// phiên gửi lên tele"*. Cái nút `file:<n>` ở đáy tin đã có từ 13/08; thứ
    /// còn thiếu là ĐÍCH CHẠM NẰM GIỮA CHỮ, ngay tại tên tệp — cùng bài học với
    /// dòng lệnh (*"Chèn ngay sau câu lệnh chứ không phải 1 nút ở cuối"*).
    /// Telegram không đặt nút vào giữa chữ được, chỉ đặt được liên kết, và liên
    /// kết ấy quay về bot bằng `/start f_<n>` — nên nó phải là một động từ.
    ///
    /// Không mở thêm cửa nào: cả hai lối vào cùng gọi
    /// `telegram::Inbox::send_quick_file`.
    SendFile,
    /// Ask the focused session a question WITHOUT interrupting it. `arg` is the
    /// question; the target is whatever `/session` is following.
    ///
    /// The target is implicit on purpose: this is typed on a phone while
    /// looking at one session's stream, and asking a person to retype a uuid
    /// there is asking them not to use the feature.
    Ask,
    /// `/runin <id> <lệnh>` — huba chạy lệnh, rồi DÁN KẾT QUẢ vào phiên.
    ///
    /// 🔴 Hà 2026-08-13, sau khi biết dấu `!` chưa bao giờ bật chế độ bash:
    /// *"có lẽ nên gọi lệnh ở command khác rồi lấy kết quả dán gửi lại vào
    /// phiên"*. Đây là đường thứ ba, và nó tốt hơn cả hai đường đang có:
    ///
    /// | | ai chạy | tốn hạn mức | có tty | phải ngồi ở máy |
    /// |---|---|---|---|---|
    /// | `▶` nhờ phiên chạy | phiên (`claude`) | CÓ | không | không |
    /// | `🖥` cửa sổ mới | shell thật | không | CÓ | CÓ |
    /// | `/runin` | **huba** | **không** | không | **không** |
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
    /// 🔴 KHÔI PHỤC 2026-08-15, vài phút sau khi bị gỡ — và lý do gỡ nó là một
    /// LỖI PHÉP ĐO của tôi. Tôi lấy "0 lượt dùng trong toàn bộ log" làm bằng
    /// chứng rằng route này thừa. Hà chỉ ra chỗ sai: *"cái tên win hơi mơ hồ mà
    /// bạn cũng không đưa vào help nên tôi ko hề biết"*.
    ///
    /// Đúng: nó `listed: false` nên KHÔNG vào menu ☰ và không hiện khi gõ `/`;
    /// `/help` có in nó, nhưng lẫn trong 24 dòng phải chủ động đi tìm. Con số 0
    /// ấy đo **sự vô hình**, không đo sự vô dụng — một thứ không ai nhìn thấy
    /// thì đương nhiên không ai gọi, và điều đó chẳng nói gì về việc nó có đáng
    /// giữ hay không.
    ///
    /// Nay tên là `/terminal` (giữ `win`, `cuaso` làm alias) và `listed: true`.
    /// `/win <lệnh>` — chạy trong một **cửa sổ Terminal thật**, không phải nền.
    ///
    /// 🔴 Hà 2026-08-13, gửi ảnh chụp lời một phiên khác: *"`!` trong Claude
    /// Code không cấp tty, nên `ssh -t` không xin được — không phải lỗi sudo
    /// hay script. Cần một cửa sổ terminal thật, dán đúng dòng này rồi gõ mật
    /// khẩu"*, rồi chốt: *"với lệnh này chỉ chạy được trong terminal không chạy
    /// được trong cli nên cần thêm cách tạo nút"*.
    ///
    /// `/cmd` sinh tiến trình con KHÔNG có tty, nên mọi thứ đòi bàn phím —
    /// `sudo`, `ssh -t`, `passwd`, một `read -s` — chết ngay ở dòng hỏi, và
    /// chết theo kiểu khó đọc (*"a terminal is required"*), chứ không phải kiểu
    /// "lệnh sai". Đây không phải lỗi vá được trong `/cmd`: **cái thiếu là một
    /// cái tty**, mà tty thì chỉ cửa sổ mới có.
    ///
    /// Nên đúng như luật cầu nối: ngồi ở máy anh sẽ mở một cửa sổ rồi dán vào
    /// đó. `/win` làm đúng thế — `keys::open_window`, cùng đường `/new` đã đi.
    /// Cửa sổ **ở lại** sau khi lệnh chạy xong: đó là chỗ gõ mật khẩu, và cũng
    /// là chỗ đọc kết quả.
    ///
    /// Hai đường này KHÔNG thay nhau: `/cmd` trả kết quả về điện thoại (đọc
    /// được từ xa, không cần đứng dậy), `/win` cần người ngồi trước máy. Nút
    /// `🖥` chỉ mọc kèm KẾT QUẢ của `/cmd`, tức đúng lúc đã biết đường kia
    /// không đi được.
    Win,
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
    /// `/web` — lái CHROME THẬT của chủ máy: xem tab nào đang mở, mở một địa
    /// chỉ, chuyển tab, đọc nội dung trang thành chữ.
    ///
    /// 🔴 Hà 2026-08-23: *"Cổng điều khiển browser thế nào rồi"*. Đúng một
    /// "gap" theo phép thử cầu nối (`CLAUDE.md`): ngồi ở máy thì mở Chrome là
    /// một cú click, còn từ điện thoại thì trước lượt này KHÔNG có đường nào.
    ///
    /// `arg` trống = danh sách tab · một địa chỉ = mở · `<cửa sổ>.<tab>` =
    /// chuyển sang tab ấy · `doc`/`text` = đọc trang thành chữ.
    Web,
}

#[derive(Debug, Clone)]
pub struct ChannelCommand {
    pub kind: CommandKind,
    /// Bước phụ do huba tự xếp hàng ⟹ chạy xong thì KHÔNG trả lời.
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
