//! `parse_choices` chạy trên MÀN THẬT, không phải chuỗi tự bịa.
//!
//! 🔴 Vì sao phải có tệp này dù đã có unit test. Hà 2026-08-21 bấm `/shot` sau
//! khi cài bản vá và báo *"không thấy nút ☑ nào"* — nghe như đã xong. Nhưng đọc
//! log lượt ấy thì màn **không có một dòng đánh số nào**: không có gì để gắn
//! nút, nên "không thấy ☑" chẳng chứng minh điều gì. Đó đúng là phép đo mù mà
//! `CLAUDE.md` cấm — một assert không thể đỏ vì sản phẩm hỏng.
//!
//! Nên bài kiểm này lấy đầu vào từ `logs/huba.log`: một màn `/shot` CÓ THẬT
//! (2026-08-18) mang 8 dòng đánh số, không con trỏ `❯`, không dòng chân hộp
//! chọn — tức đúng hình dạng đã khiến huba gắn ☑ vào văn xuôi rồi báo với chủ
//! máy một nguyên nhân không tồn tại (*"hộp này có thể không nhận phím số"*).
//!
//! Nó ĐỎ ĐƯỢC: trả `parse_choices` về luật cũ ("số liên tiếp bắt đầu từ 1") là
//! fixture này đọc ra một hộp chọn 4 mục.

const MAN: &str = include_str!("fixtures/man_van_xuoi_danh_so.txt");

#[test]
fn man_that_co_danh_so_nhung_khong_phai_hop_chon() {
    // Điều kiện tiên quyết: fixture phải THẬT SỰ mang hình dạng gây lỗi. Không
    // có ba assert này thì một fixture bị thay bằng màn trống vẫn "xanh", và
    // bài kiểm biến thành thứ nó sinh ra để chống.
    let so_dong_danh_so = MAN
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.split_once('.')
                .is_some_and(|(n, r)| n.trim().parse::<usize>().is_ok() && !r.trim().is_empty())
        })
        .count();
    assert!(
        so_dong_danh_so >= 3,
        "fixture phải có ≥3 dòng đánh số, đang có {so_dong_danh_so}"
    );
    // ⚠ Hỏi ĐÚNG câu mà cửa hỏi: `❯` NGAY TRƯỚC một dòng đánh số. Bản đầu của
    // assert này cấm mọi dòng bắt đầu bằng `❯` và đỏ ngay — vì màn thật có một
    // dòng `❯` trống, tức dấu nhắc ô nhập. Một điều kiện tiên quyết rộng hơn
    // cái nó đi kèm thì loại bỏ chính những fixture thật.
    assert!(
        !MAN.lines().any(|l| {
            l.trim_start()
                .strip_prefix('❯')
                .map(str::trim_start)
                .and_then(|r| r.split_once('.'))
                .is_some_and(|(n, r)| n.trim().parse::<usize>().is_ok() && !r.trim().is_empty())
        }),
        "fixture không được có con trỏ ❯ trên dòng đánh số — nó phải là VĂN XUÔI"
    );
    assert!(
        !huba::keys::has_chooser_footer(MAN),
        "fixture không được có dòng chân hộp chọn"
    );

    let c = huba::keys::parse_choices(MAN);
    assert!(
        c.is_empty(),
        "màn văn xuôi bị đọc thành hộp chọn ({} lựa chọn): {:?}",
        c.len(),
        c
    );
}
