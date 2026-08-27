//! Mọi route trong bảng phải ĐI TỚI ĐƯỢC bằng chính cái tên nó đăng ký.
//!
//! 🔴 Vì sao tệp này ra đời (2026-08-27, lúc thêm `/refresh`): `commands::ROUTES`
//! sinh ra `/help` và `setMyCommands` — tức cái danh sách Telegram hiện trong menu
//! ☰ — còn việc ĐỌC một câu lệnh thì nằm ở `verbs::parse_command`. Con bug tệ
//! nhất trong họ này là lệnh **hiện ra trong menu**, chủ máy bấm, và huba đáp
//! *"Chưa hiểu lệnh này"*: kênh nhận, không gì xảy ra, không gì nói ra điều đó.
//!
//! 🔴 **MẪU SỐ THẬT — đo được ngay lượt chạy đầu, và nó nhỏ hơn cái tên tệp hứa.**
//! Cấy vào bảng một route không có nhánh parse nào, cổng này **vẫn XANH**. Lý do
//! ở `verbs.rs`: `parse_command` tra `commands::lookup` TRƯỚC, và `lookup` khớp cả
//! `name` lẫn `aliases`, rồi tự đọc tham số cho `Arg::None` · `Fixed` · `Rest` ·
//! `RestRequired`. Với bốn hạng ấy hai bảng **dính liền bằng cấu trúc** — không có
//! khe nào để hở, nên cũng không có gì để đo.
//!
//! Chỗ CÒN hở đúng một chỗ: `Arg::Custom` nghĩa là *"luật riêng, `parse_command`
//! tự xử"* — `lookup` cố ý rơi xuống `match` bên dưới, và nếu nhánh viết tay ở đó
//! thiếu thì route chết trong khi menu vẫn mời bấm. Ba route đang ở diện ấy
//! (`/ask` · `/runin` · `/set`). **Cổng này đo BA route đó**, phần còn lại nó chỉ
//! canh cho ngày ai đó bỏ lối tắt `lookup` đi — và `measured_surface_is_not_empty`
//! bắt phải khai con số ấy ra thay vì để người đọc tưởng nó phủ cả bảng.
//!
//! Chiều ngược lại của cùng một sai lầm cũng đã đo được cùng lượt: nhánh
//! `"refresh" | "lamtuoi"` viết tay trong `verbs.rs` là **mã chết** — bảng trả lời
//! trước, nhánh ấy không bao giờ tới lượt. Đã gỡ, để lại bia mộ tại chỗ.

use huba::commands::{Arg, ROUTES};
use huba::verbs::parse_command;

/// Bao nhiêu ô `<…>` mà `usage` khai — tức route ấy đòi mấy vế.
///
/// 🔴 Vì sao phải đếm thay vì đưa đại một chữ (sửa 27/08, ngay lượt chạy đầu):
/// bảng KHÔNG nói một route `Custom` đòi mấy vế, chỉ `usage` nói.
/// `/runin <id> <dòng lệnh>` và `/set <khoá> <giá trị>` đều **cố ý** từ chối một
/// vế (`/set foo` mà lọt là đổi cấu hình sang giá trị rỗng). Đưa đại `/runin x`
/// rồi kêu "route hỏng" là bài kiểm tự dựng ca sai cho mình — đúng thứ
/// `CLAUDE.md` dặn: assert đỏ thì soi PHÉP ĐO trước khi soi mã.
fn so_o(usage: &str) -> usize {
    usage.matches('<').count()
}

/// Gõ tên route ra thành câu lệnh mà chủ máy sẽ thật sự gõ.
fn thu(name: &str, arg: Arg, usage: &str) -> Option<(huba::adapters::CommandKind, i64, String)> {
    // Route đòi tham số thì gõ tên trần KHÔNG đủ — đó là hành vi ĐÚNG, nên bài
    // kiểm phải đưa cho nó tham số thay vì kết luận route hỏng.
    let cau = match arg {
        Arg::None | Arg::Rest => format!("/{name}"),
        Arg::RestRequired => format!("/{name} x"),
        // Số vế lấy từ chính lời khai của bảng. Không khai ô nào thì bài kiểm
        // KHÔNG đoán — xem `every_custom_route_declares_its_shape`.
        Arg::Custom => {
            let ve = vec!["x"; so_o(usage).max(1)].join(" ");
            format!("/{name} {ve}")
        }
        Arg::Fixed(v) => format!("/{name} {v}"),
    };
    parse_command(&cau)
}

/// KHAI MẪU SỐ: cổng này đo được mấy trên mấy, và đỏ khi con số ấy về 0.
///
/// `Arg::Custom` là phần duy nhất `lookup` không tự lo; hết Custom thì cả tệp này
/// chỉ còn lặp lại một chuyện cấu trúc đã đúng sẵn, và một cổng như thế phải bị
/// gỡ chứ không nằm đấy nhận công.
#[test]
fn measured_surface_is_not_empty() {
    let rieng = ROUTES
        .iter()
        .filter(|r| matches!(r.arg, Arg::Custom))
        .count();
    assert!(
        rieng > 0,
        "{} route trong bảng, 0 route Arg::Custom — không còn nhánh viết tay nào để \
         hở, nên cổng này không đo gì nữa: gỡ nó đi thay vì để nó xanh",
        ROUTES.len()
    );
}

/// "Không đo được" là TRẠNG THÁI RIÊNG, không được lẫn vào màu xanh.
///
/// Một `Arg::Custom` mà `usage` không có ô `<…>` nào thì `thu()` chỉ còn cách
/// đoán — và lúc ấy hai bài kiểm dưới đây xanh mà chẳng chứng minh gì. Bắt nó đỏ
/// ngay tại đây, ở đúng chỗ đọc ra lý do.
#[test]
fn every_custom_route_declares_its_shape() {
    for r in ROUTES {
        if matches!(r.arg, Arg::Custom) {
            assert!(
                so_o(r.usage) >= 1,
                "/{} khai Arg::Custom (luật riêng) nhưng usage {:?} không có ô <…> nào — \
                 bài kiểm không suy ra được câu lệnh mẫu, nên không đo được route này",
                r.name,
                r.usage
            );
        }
    }
}

#[test]
fn every_route_in_the_table_is_reachable_by_its_own_name() {
    assert!(
        !ROUTES.is_empty(),
        "bảng rỗng thì bài kiểm này xanh vô nghĩa"
    );
    for r in ROUTES {
        let got = thu(r.name, r.arg, r.usage);
        assert!(
            got.is_some(),
            "/{} có trong bảng (và trong menu ☰ của Telegram) nhưng `parse_command` \
             không hiểu — bấm vào sẽ nhận 'Chưa hiểu lệnh này'",
            r.name
        );
        assert_eq!(
            got.unwrap().0,
            r.kind,
            "/{} đọc ra một route KHÁC với cái nó khai trong bảng — `/help` sẽ mô tả \
             một việc, cú bấm làm một việc khác",
            r.name
        );
    }
}

#[test]
fn every_alias_lands_on_the_same_route_as_its_name() {
    // MẪU SỐ: vòng lặp lồng trên một danh sách rỗng cũng xanh. Khai số alias đã
    // thật sự thử ra, và bắt nó > 0 — nếu ngày nào bảng không còn alias nào thì
    // bài kiểm này phải ĐỎ để ai đó xoá nó đi, chứ không nằm đấy giả vờ canh.
    let mut da_thu = 0usize;
    for r in ROUTES {
        for a in r.aliases {
            da_thu += 1;
            let got = thu(a, r.arg, r.usage);
            assert!(
                got.is_some(),
                "alias /{a} của /{} không đọc được — một cái tên chết trong tài liệu",
                r.name
            );
            assert_eq!(
                got.unwrap().0,
                r.kind,
                "alias /{a} đi tới route khác với /{}",
                r.name
            );
        }
    }
    assert!(
        da_thu > 0,
        "không thử được alias nào trên {} route — bài kiểm xanh mà mẫu số bằng 0",
        ROUTES.len()
    );
}

/// ĐỐI CHỨNG NGƯỢC: bài kiểm trên chỉ có nghĩa nếu `parse_command` BIẾT từ chối.
///
/// Thiếu vế này thì một bộ phân tích trả `Some(Help)` cho mọi chuỗi cũng làm cả
/// tệp này xanh — và lúc ấy nó không còn đo gì nữa.
#[test]
fn a_name_that_is_not_a_route_is_still_refused() {
    for khong_phai in ["/khongcolenhnay", "/", "/refreshx", "xin chào"] {
        assert!(
            parse_command(khong_phai).is_none(),
            "{khong_phai:?} không phải lệnh — `parse_command` phải trả None, nếu không \
             thì bài kiểm 'mọi route đọc được' ở trên đo bằng không"
        );
    }
}

/// ĐỐI CHỨNG NGƯỢC thứ hai, ở đúng chỗ cổng này còn đo được: một route `Custom`
/// **thiếu nhánh viết tay** phải làm nó ĐỎ.
///
/// Ca cấy thật đã chạy tay ngày 27/08 (thêm hẳn một `Route` vào `ROUTES`); ở đây
/// giữ lại phần diễn được bằng mã: `parse_command` chỉ nhận một cái tên khi tên
/// ấy có đường đi, nên tên `Custom` chưa ai viết nhánh cho thì trả `None` — và
/// `every_route_in_the_table_is_reachable_by_its_own_name` sẽ bắt được nó.
#[test]
fn a_custom_route_without_a_hand_written_arm_would_be_caught() {
    // Đúng hình dạng câu lệnh mà `thu()` sinh cho một `Arg::Custom` hai vế, chỉ
    // khác cái tên: chưa có trong bảng, nên cũng chưa có nhánh nào ở `match`.
    assert!(
        parse_command("/mot_route_custom_chua_ai_viet_nhanh x y").is_none(),
        "một tên Custom không có nhánh viết tay mà vẫn parse được thì cổng này mù \
         đúng chỗ duy nhất nó còn đo"
    );
}
