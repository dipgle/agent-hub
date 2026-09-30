//! Những mảnh THUẦN của bản Windows — chạy được ngay trên macOS.
//!
//! Hà 2026-09-30: *"viết để chạy được window"*. Máy viết mã không có Windows nào,
//! nên phần chạm hệ điều hành (UI Automation, SendInput, Task Scheduler) chỉ được
//! dựng-kiểm chéo (`gate.sh` bước ②b). Phần QUYẾT ĐỊNH thì tách ra thành hàm thuần
//! để chấm ở đây — đúng hai chỗ mà một lỗi sẽ im lặng trên Windows:
//!
//! * **thư mục nhà**: Windows không đặt `HOME`; đọc thẳng `HOME` ⟹ `~\.claude` rơi về
//!   đường TƯƠNG ĐỐI ⟹ danh sách phiên rỗng, không một dòng lỗi.
//! * **pid còn sống**: Windows không có `kill -0`; lượt dò hỏng ⟹ nhánh fail-closed
//!   đọc ra "còn sống" ⟹ hubd tự từ chối khởi động lại, mãi mãi.

use std::ffi::OsString;
use std::path::PathBuf;

use huba::config::home_from;
use huba::exec::tasklist_has_pid;

fn s(x: &str) -> Option<OsString> {
    Some(OsString::from(x))
}

#[test]
fn thu_muc_nha_home_truoc_roi_userprofile() {
    // macOS/Linux, Git Bash trên Windows: HOME có ⟹ dùng HOME.
    assert_eq!(
        home_from(s("/Users/ha"), s("C:\\Users\\ha")),
        Some(PathBuf::from("/Users/ha"))
    );
    // Windows thường: KHÔNG có HOME ⟹ USERPROFILE. Bản cũ trả `None` ở đây.
    assert_eq!(
        home_from(None, s("C:\\Users\\ha")),
        Some(PathBuf::from("C:\\Users\\ha"))
    );
    // HOME đặt mà RỖNG không phải một thư mục nhà.
    assert_eq!(
        home_from(s(""), s("C:\\Users\\ha")),
        Some(PathBuf::from("C:\\Users\\ha"))
    );
    assert_eq!(home_from(None, None), None);
    assert_eq!(home_from(s(""), s("")), None);
}

/// Hình dạng `tasklist /FO CSV /NH` theo tài liệu Microsoft. ⚠ Chưa đối chiếu trên
/// máy Windows thật — `scripts/do-windows.ps1` không đo lệnh này; lần chạy đầu trên
/// máy thật phải xem `pid_check_*` trong `logs/huba.log`.
#[test]
fn tasklist_doc_theo_cot_pid_khong_theo_chuoi_con() {
    let song = "\"hubd.exe\",\"5432\",\"Console\",\"1\",\"12,345 K\"\r\n";
    assert!(tasklist_has_pid(song, "5432"));
    // Vế ngược: `345` nằm TRONG cột bộ nhớ ("12,345 K") — không phải pid ấy.
    assert!(!tasklist_has_pid(song, "345"));
    // Cột "Session#" là "1" — không phải pid 1.
    assert!(!tasklist_has_pid(song, "1"));
    // Không có tiến trình: câu báo dịch theo ngôn ngữ — không được đọc nó.
    assert!(!tasklist_has_pid(
        "INFO: No tasks are running which match the specified criteria.\r\n",
        "5432"
    ));
    assert!(!tasklist_has_pid(
        "Thông tin: Không có tác vụ nào đang chạy khớp với tiêu chí đã chỉ định.\r\n",
        "5432"
    ));
    assert!(!tasklist_has_pid("", "5432"));
}
