//! Sổ tin đến (`huba::tin_den`) — lệnh của chủ máy sống qua máy tắt / hubad chết.
//!
//! 🔴 Vì sao (02/10, đề xuất phiên dhub theo lời Hà *"sao không có log dự
//! phòng"*): `telegram.rs` tiến con dấu `offset` rồi mới giao update cho thợ, nên
//! từ đó tới lúc lệnh chạy xong mệnh lệnh chỉ sống trong bộ nhớ. Các ca dưới đây
//! dựng đúng những lần "chết" ở từng khúc của đường ấy và hỏi: lượt khởi động
//! sau làm gì với tin ấy.

use huba::tin_den::{self, Buoc};
use serde_json::{json, Value};

fn so() -> (rusqlite::Connection, tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = dir.path().join("huba.sqlite");
    let c = tin_den::mo(&p).expect("mở sổ");
    (c, dir, p)
}

fn tin_chu(id: i64, chu: &str, gui: i64) -> Value {
    json!({ "update_id": id,
            "message": { "message_id": id * 10, "date": gui, "chat": { "id": 1 }, "text": chu } })
}

fn buoc_cua(c: &rusqlite::Connection, id: i64) -> String {
    c.query_row("SELECT buoc FROM tin_den WHERE update_id = ?1", [id], |r| {
        r.get(0)
    })
    .expect("dòng phải có trong sổ")
}

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

/// Ghi xong là nằm TRÊN ĐĨA: một kết nối khác (tức một tiến trình khởi động
/// lại) đọc được nguyên văn, kể cả bản gốc đủ để chạy lại.
#[test]
fn ghi_nhan_xong_la_tien_trinh_khac_doc_duoc() {
    let (c, _d, p) = so();
    let u = tin_chu(501, "Trả lời câu 3: chọn phương án B", now().timestamp());
    assert!(tin_den::ghi_nhan(&c, &u).unwrap(), "lần đầu phải ghi mới");
    drop(c);
    let c2 = tin_den::mo(&p).unwrap();
    let t = tin_den::chua_xong(&c2).unwrap();
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].update_id, 501);
    assert_eq!(t[0].buoc, Buoc::Nhan);
    assert_eq!(t[0].chu, "Trả lời câu 3: chọn phương án B");
    assert_eq!(t[0].goc, u, "bản gốc phải đủ để chạy lại y hệt");
}

/// Telegram giao lại một tin đã nằm trong sổ ⟹ `Ok(false)`, không ghi đè bước.
#[test]
fn giao_lai_tin_da_co_khong_ghi_de_buoc() {
    let (c, _d, _p) = so();
    let u = tin_chu(7, "/shot", now().timestamp());
    assert!(tin_den::ghi_nhan(&c, &u).unwrap());
    tin_den::danh_dau(&c, &[7], Buoc::Chay).unwrap();
    assert!(
        !tin_den::ghi_nhan(&c, &u).unwrap(),
        "lần hai phải báo ĐÃ CÓ"
    );
    assert_eq!(
        buoc_cua(&c, 7),
        "chay",
        "giao lại không được kéo tin về `nhan`"
    );
}

/// Bước khép (`do` · `bo`) không bao giờ bị ghi đè — sổ đã báo chủ máy "không
/// chạy" thì không được lặng lẽ thành `xong`.
#[test]
fn buoc_khep_khong_bi_ghi_de() {
    let (c, _d, _p) = so();
    for id in [1, 2, 3] {
        tin_den::ghi_nhan(&c, &tin_chu(id, "x", now().timestamp())).unwrap();
    }
    // 1: nhan → bo → (xong/chay không ăn)
    assert_eq!(tin_den::danh_dau(&c, &[1], Buoc::Bo).unwrap(), 1);
    assert_eq!(tin_den::danh_dau(&c, &[1], Buoc::Xong).unwrap(), 0);
    assert_eq!(tin_den::danh_dau(&c, &[1], Buoc::Chay).unwrap(), 0);
    assert_eq!(buoc_cua(&c, 1), "bo");
    // 2: nhan → chay → do → (xong không ăn)
    tin_den::danh_dau(&c, &[2], Buoc::Chay).unwrap();
    assert_eq!(tin_den::danh_dau(&c, &[2], Buoc::Do).unwrap(), 1);
    assert_eq!(tin_den::danh_dau(&c, &[2], Buoc::Xong).unwrap(), 0);
    assert_eq!(buoc_cua(&c, 2), "do");
    // 3: nhan → xong → chay (lệnh thứ hai của cùng một update) → xong
    tin_den::danh_dau(&c, &[3], Buoc::Xong).unwrap();
    assert_eq!(tin_den::danh_dau(&c, &[3], Buoc::Chay).unwrap(), 1);
    tin_den::danh_dau(&c, &[3], Buoc::Xong).unwrap();
    assert_eq!(buoc_cua(&c, 3), "xong");
    // `do` chỉ từ `chay`: một tin chưa từng chạy không được báo là "chạy dở".
    tin_den::ghi_nhan(&c, &tin_chu(4, "x", now().timestamp())).unwrap();
    assert_eq!(tin_den::danh_dau(&c, &[4], Buoc::Do).unwrap(), 0);
}

/// CA CHÍNH — chết ở ba khúc khác nhau, khởi động lại làm ba việc khác nhau.
#[test]
fn khoi_dong_lai_chay_lai_cai_chua_chay_va_khong_chay_lai_cai_dang_chay() {
    let (c, _d, p) = so();
    let t = now().timestamp();
    // A: đã ghi đĩa, chưa ai đụng (chết ngay sau khi tiến `offset`).
    tin_den::ghi_nhan(&c, &tin_chu(10, "câu trả lời A", t)).unwrap();
    // B: lô đã BẮT ĐẦU chạy rồi chết (gõ dở vào phiên).
    tin_den::ghi_nhan(&c, &tin_chu(11, "/type abc câu B", t)).unwrap();
    tin_den::danh_dau(&c, &[11], Buoc::Chay).unwrap();
    // C: chạy xong trước khi chết.
    tin_den::ghi_nhan(&c, &tin_chu(12, "/shot", t)).unwrap();
    tin_den::danh_dau(&c, &[12], Buoc::Xong).unwrap();
    drop(c);

    let c = tin_den::mo(&p).unwrap();
    let (cau, chay_lai) = tin_den::khoi_phuc(&c, now(), 900);
    let ids: Vec<i64> = chay_lai
        .iter()
        .filter_map(|u| u.get("update_id").and_then(Value::as_i64))
        .collect();
    assert_eq!(ids, vec![10], "CHỈ tin chưa bắt đầu chạy được chạy lại");
    assert_eq!(buoc_cua(&c, 11), "do", "tin đang chạy dở phải khép `do`");
    assert_eq!(buoc_cua(&c, 12), "xong");
    assert_eq!(
        buoc_cua(&c, 10),
        "nhan",
        "tin chạy lại giữ `nhan` cho tới khi lô chạy nó"
    );
    let cau = cau.expect("có tin chưa xong thì phải nói");
    assert!(cau.contains("«/type abc câu B»"), "{cau}");
    assert!(cau.contains("KHÔNG chạy lại"), "{cau}");
    assert!(cau.contains("«câu trả lời A»"), "{cau}");
    assert!(!cau.contains("/shot"), "tin đã xong không được nhắc: {cau}");

    // Lượt khởi động THỨ HAI không báo lại tin đã báo.
    let (cau2, chay_lai2) = tin_den::khoi_phuc(&c, now(), 900);
    assert!(!cau2.unwrap_or_default().contains("câu B"));
    assert_eq!(chay_lai2.len(), 1, "A vẫn chưa chạy thì vẫn còn đó");
}

/// Tin chưa chạy nhưng đã nằm quá lâu ⟹ không chạy, khép `bo`, NÓI ra.
#[test]
fn tin_qua_cu_khong_chay_ma_bao() {
    let (c, _d, _p) = so();
    tin_den::ghi_nhan(&c, &tin_chu(20, "câu gõ từ tối qua", now().timestamp())).unwrap();
    let mai = now() + chrono::Duration::seconds(901);
    let (cau, chay_lai) = tin_den::khoi_phuc(&c, mai, 900);
    assert!(chay_lai.is_empty(), "quá ngưỡng thì không chạy");
    assert_eq!(buoc_cua(&c, 20), "bo");
    let cau = cau.expect("bỏ một tin của chủ máy thì phải nói");
    assert!(
        cau.contains("«câu gõ từ tối qua»") && cau.contains("KHÔNG chạy"),
        "{cau}"
    );
    // Ngay dưới ngưỡng thì vẫn chạy — cổng không được rộng hơn ngưỡng đã khai.
    tin_den::ghi_nhan(&c, &tin_chu(21, "vừa gõ", now().timestamp())).unwrap();
    let (_, chay_lai) = tin_den::khoi_phuc(&c, now() + chrono::Duration::seconds(890), 900);
    assert_eq!(chay_lai.len(), 1);
}

/// Khởi động lại sạch ⟹ im (luật 11: chỉ nói khi có thay đổi).
#[test]
fn khoi_dong_sach_thi_im() {
    let (c, _d, _p) = so();
    tin_den::ghi_nhan(&c, &tin_chu(30, "/shot", now().timestamp())).unwrap();
    tin_den::danh_dau(&c, &[30], Buoc::Xong).unwrap();
    let (cau, chay_lai) = tin_den::khoi_phuc(&c, now(), 900);
    assert!(cau.is_none(), "{cau:?}");
    assert!(chay_lai.is_empty());
}

/// Dọn đúng thứ đã khép và đã cũ; tin chưa khép thì cũ mấy cũng giữ.
#[test]
fn don_cu_chi_don_tin_da_khep() {
    let (c, _d, _p) = so();
    for (id, b) in [
        (40, None),
        (41, Some(Buoc::Chay)),
        (42, Some(Buoc::Xong)),
        (43, Some(Buoc::Bo)),
    ] {
        tin_den::ghi_nhan(&c, &tin_chu(id, "x", now().timestamp())).unwrap();
        if let Some(b) = b {
            tin_den::danh_dau(&c, &[id], b).unwrap();
        }
    }
    // Hôm nay: chưa dọn gì.
    assert_eq!(tin_den::don_cu(&c, now()).unwrap(), 0);
    // Quá hạn giữ: dọn 42 · 43, giữ 40 (nhan) · 41 (chay).
    let sau = now() + chrono::Duration::days(tin_den::GIU_NGAY) + chrono::Duration::hours(1);
    assert_eq!(tin_den::don_cu(&c, sau).unwrap(), 2);
    let con: Vec<i64> = tin_den::chua_xong(&c)
        .unwrap()
        .iter()
        .map(|t| t.update_id)
        .collect();
    assert_eq!(con, vec![40, 41]);
}

/// Lượt ghi BỀN: kết nối của sổ thật sự bật `fullfsync` + `synchronous=FULL`.
/// Ca phải đỡ là MẤT ĐIỆN — `fsync` trần trên macOS dừng ở bộ đệm của ổ.
#[test]
fn so_ghi_ben_qua_mat_dien() {
    let (c, _d, _p) = so();
    let ff: i64 = c.query_row("PRAGMA fullfsync", [], |r| r.get(0)).unwrap();
    let sy: i64 = c.query_row("PRAGMA synchronous", [], |r| r.get(0)).unwrap();
    assert_eq!(ff, 1, "fullfsync phải BẬT");
    assert_eq!(sy, 2, "synchronous phải FULL (2)");
}

/// Chữ cho người đọc sổ: câu gõ · dữ liệu nút · tên tệp kèm chú thích.
#[test]
fn tom_tat_nhan_ra_tin_cua_minh() {
    let (l, c, g) = tin_den::tom_tat(&tin_chu(1, "xin chào", 1_700_000_000));
    assert_eq!((l, c.as_str(), g), ("chu", "xin chào", Some(1_700_000_000)));
    let (l, c, g) = tin_den::tom_tat(&json!({ "update_id": 2,
        "callback_query": { "id": "q", "data": "key:abc 2", "from": { "id": 1 } } }));
    assert_eq!((l, c.as_str(), g), ("nut", "key:abc 2", None));
    let (l, c, _) = tin_den::tom_tat(&json!({ "update_id": 3,
        "message": { "date": 1, "chat": { "id": 1 }, "caption": "xem giúp",
                     "document": { "file_id": "f", "file_name": "bao-cao.pdf" } } }));
    assert_eq!((l, c.as_str()), ("tep", "bao-cao.pdf — xem giúp"));
    let (l, c, _) = tin_den::tom_tat(&json!({ "update_id": 4,
        "message": { "date": 1, "chat": { "id": 1 }, "photo": [ { "file_id": "p" } ] } }));
    assert_eq!((l, c.as_str()), ("tep", "(ảnh)"));
}

/// Lô lệnh: bắt đầu ⟹ `chay`, hết ⟹ `xong`; huba TỰ khởi động lại giữa lô
/// (`/upgrade`) ⟹ khép `xong` TRƯỚC cú chết, để lượt sau không báo oan "đang
/// chạy dở". Một bài duy nhất vì ô lô là biến toàn cục của tiến trình.
#[test]
fn lo_lenh_va_tu_khoi_dong_lai() {
    let (c, _d, p) = so();
    for id in [50, 51] {
        tin_den::ghi_nhan(&c, &tin_chu(id, "/upgrade", now().timestamp())).unwrap();
    }
    // Lô thường.
    tin_den::bat_dau_lo(&p, vec![50]);
    assert_eq!(buoc_cua(&c, 50), "chay");
    tin_den::ket_thuc_lo();
    assert_eq!(buoc_cua(&c, 50), "xong");
    // Lô `/upgrade`: khép trước cú chết; lô không bao giờ tới `ket_thuc_lo`.
    tin_den::bat_dau_lo(&p, vec![51]);
    tin_den::khep_lo_truoc_khi_tu_khoi_dong();
    assert_eq!(buoc_cua(&c, 51), "xong");
    let (cau, chay_lai) = tin_den::khoi_phuc(&c, now(), 900);
    assert!(
        cau.is_none(),
        "/upgrade không được để lại lời báo oan: {cau:?}"
    );
    assert!(
        chay_lai.is_empty(),
        "/upgrade mà chạy lại là tự khởi động lại mãi"
    );
    // Không có lô nào ⟹ khép là việc không làm gì, không hỏng.
    tin_den::khep_lo_truoc_khi_tu_khoi_dong();
    tin_den::ket_thuc_lo();
}
