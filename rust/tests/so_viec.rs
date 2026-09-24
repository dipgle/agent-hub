//! Sổ việc một cửa (Redis Streams) + bộ nói RESP2 của nó.
//!
//! Hai tầng bài: THUẦN (mã hoá/giải mã RESP, bước, khởi phục, bóc câu trả lời
//! `XREADGROUP`) chạy trong cổng; CHẠY THẬT trên Redis của máy (`#[ignore]`, chạy
//! tay) đi trọn vòng đời một việc và giả lập huba khởi động lại giữa chừng —
//! đúng ca 23/09 18:15:45Z (hai hòm thư mất vì daemon khởi động lại).
//!
//! ```text
//! cargo test --offline --test so_viec -- --ignored --nocapture
//! ```

use std::io::Cursor;

use huba::redis_mini::{encode, parse, Redis, Resp};
use huba::so_viec::{
    bao_co_the_da_chay, giay_tu, khoa_hom_thu, khoi_phuc, muc_tu_xread, tra_gi, Buoc, KhoiPhuc, So,
    TraGi,
};

fn doc(s: &str) -> Resp {
    parse(&mut Cursor::new(s.as_bytes().to_vec())).expect("RESP hợp lệ")
}

#[test]
fn resp_ma_hoa_mang_bulk() {
    assert_eq!(
        encode(&["SET", "k", "a b"]),
        b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$3\r\na b\r\n".to_vec()
    );
    // Độ dài tính theo BYTE — chữ Việt có dấu tốn 2–3 byte mỗi ký tự.
    assert_eq!(encode(&["đ"]), b"*1\r\n$2\r\n\xc4\x91\r\n".to_vec());
}

#[test]
fn resp_giai_ma_du_nam_kieu() {
    assert_eq!(doc("+OK\r\n"), Resp::Simple("OK".into()));
    assert_eq!(doc("-ERR x\r\n"), Resp::Error("ERR x".into()));
    assert_eq!(doc(":42\r\n"), Resp::Int(42));
    assert_eq!(doc("$-1\r\n"), Resp::Bulk(None));
    assert_eq!(doc("*-1\r\n"), Resp::Array(None));
    // Bulk mang CRLF BÊN TRONG — đọc theo độ dài, không theo dòng.
    assert_eq!(
        doc("$4\r\na\r\nb\r\n"),
        Resp::Bulk(Some(b"a\r\nb".to_vec()))
    );
    assert_eq!(
        doc("*2\r\n:1\r\n*1\r\n$1\r\nx\r\n"),
        Resp::Array(Some(vec![
            Resp::Int(1),
            Resp::Array(Some(vec![Resp::Bulk(Some(b"x".to_vec()))]))
        ]))
    );
}

#[test]
fn resp_hong_thi_bao_loi_khong_doan() {
    assert!(
        parse(&mut Cursor::new(b"?x\r\n".to_vec())).is_err(),
        "kiểu lạ"
    );
    assert!(
        parse(&mut Cursor::new(b"$5\r\nab\r\n".to_vec())).is_err(),
        "bulk cụt"
    );
    assert!(
        parse(&mut Cursor::new(b":1".to_vec())).is_err(),
        "thiếu CRLF"
    );
    assert!(
        parse(&mut Cursor::new(Vec::new())).is_err(),
        "kết nối đóng giữa chừng"
    );
}

#[test]
fn buoc_khu_hoi_va_khoi_phuc() {
    let tat_ca = [
        Buoc::Nhan,
        Buoc::Doc,
        Buoc::Chay,
        Buoc::Xong,
        Buoc::Gui,
        Buoc::DaGui,
        Buoc::Loi,
        Buoc::Bo,
    ];
    for b in tat_ca {
        assert_eq!(Buoc::parse(b.as_str()), Some(b), "{b:?}");
    }
    assert_eq!(Buoc::parse("la"), None);
    // Luật cốt lõi: việc ĐÃ BẮT ĐẦU chạy thì không bao giờ tự chạy lại.
    assert_eq!(khoi_phuc(Buoc::Nhan), KhoiPhuc::ChayLai);
    assert_eq!(khoi_phuc(Buoc::Doc), KhoiPhuc::ChayLai);
    assert_eq!(khoi_phuc(Buoc::Chay), KhoiPhuc::BaoCoTheDaChay);
    assert_eq!(khoi_phuc(Buoc::Xong), KhoiPhuc::DaCoKetQua);
    assert_eq!(khoi_phuc(Buoc::Gui), KhoiPhuc::DaCoKetQua);
    for b in [Buoc::DaGui, Buoc::Loi, Buoc::Bo] {
        assert_eq!(khoi_phuc(b), KhoiPhuc::KhongLamGi);
        assert!(b.da_khep());
    }
    let s = bao_co_the_da_chay("git push origin x", Some("2026-09-23T18:15:45Z"));
    assert!(
        s.contains("KHÔNG tự chạy lại") && s.contains("git push origin x"),
        "{s}"
    );
}

#[test]
fn boc_cau_tra_loi_xreadgroup() {
    // [[stream, [[muc, [id, 7]], [muc2, [id, 8]]]]]
    let tl = doc("*1\r\n*2\r\n$6\r\nstream\r\n*2\r\n\
         *2\r\n$3\r\n1-0\r\n*2\r\n$2\r\nid\r\n$1\r\n7\r\n\
         *2\r\n$3\r\n2-0\r\n*2\r\n$2\r\nid\r\n$1\r\n8\r\n");
    assert_eq!(
        muc_tu_xread(&tl),
        vec![("1-0".to_string(), 7), ("2-0".to_string(), 8)]
    );
    // Không có gì mới ⟹ `*-1` ⟹ rỗng, không phải lỗi.
    assert!(muc_tu_xread(&doc("*-1\r\n")).is_empty());
}

#[test]
fn ben_tra_khep_cho_lai_hay_tra() {
    // Việc đã khép ⟹ chỉ XACK, dù gọi ngay hay gọi từ vòng.
    for b in [Buoc::DaGui, Buoc::Loi, Buoc::Bo] {
        assert_eq!(tra_gi(b, None, true, 30), TraGi::Khep, "{b:?}");
        assert_eq!(tra_gi(b, Some(1), false, 30), TraGi::Khep, "{b:?}");
    }
    // Có kết quả, chưa trả lượt nào ⟹ trả.
    assert_eq!(tra_gi(Buoc::Xong, None, false, 30), TraGi::Tra);
    // Vừa thử 5 giây trước ⟹ vòng chạy để yên (không gõ dồn vào Terminal câm)…
    assert_eq!(tra_gi(Buoc::Gui, Some(5), false, 30), TraGi::ChoLai);
    // …nhưng luồng vừa có kết quả thì trả luôn.
    assert_eq!(tra_gi(Buoc::Gui, Some(5), true, 30), TraGi::Tra);
    // Đủ nhịp ⟹ trả lại.
    assert_eq!(tra_gi(Buoc::Gui, Some(30), false, 30), TraGi::Tra);
    // Mục kết quả mà việc chưa có kết quả ⟹ sổ hỏng, KHÔNG dán.
    for b in [Buoc::Nhan, Buoc::Doc, Buoc::Chay] {
        assert_eq!(tra_gi(b, None, true, 30), TraGi::SaiBuoc, "{b:?}");
    }
}

#[test]
fn giay_tu_moc_iso_va_moc_hong() {
    // Đúng khuôn `logging::now_iso` (RFC 3339, mili giây, `Z`).
    let moc = "2026-09-23T18:15:45.123Z";
    let t = chrono::DateTime::parse_from_rfc3339(moc)
        .unwrap()
        .timestamp();
    assert_eq!(giay_tu(moc, t + 90), Some(90));
    // Mốc hỏng KHÔNG được đọc thành 0 — `None` để chỗ gọi tự chọn phía an toàn.
    assert_eq!(giay_tu("", t), None);
    assert_eq!(giay_tu("hôm qua", t), None);
}

#[test]
fn khoa_hom_thu_phan_biet_hai_lan_ghi_cung_tep() {
    let p = "/private/tmp/claude-501/x/sid/scratchpad/huba-run.txt";
    let a = khoa_hom_thu(p, 1_000, 20);
    // Cùng lần ghi ⟹ cùng khoá (đó là cả điểm: vòng sau đọc lại không nhận lần hai).
    assert_eq!(a, khoa_hom_thu(p, 1_000, 20));
    // Phiên ghi lệnh THỨ HAI vào đúng tệp ấy ⟹ khoá khác, dù cùng cỡ.
    assert_ne!(a, khoa_hom_thu(p, 1_001, 20));
    assert_ne!(a, khoa_hom_thu(p, 1_000, 21));
    assert!(a.starts_with(p));
}

// ───────────── CHẠY THẬT trên Redis của máy ─────────────

fn so_thu() -> So {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    So {
        pre: format!("huba_test:{}:{t}:", std::process::id()),
        nguoi_doc: "thu".to_string(),
    }
}

fn don(r: &mut Redis, so: &So) {
    let khoa = r
        .cmd(&["KEYS", &format!("{}*", so.pre)])
        .map(|v| v.items())
        .unwrap_or_default();
    for k in khoa {
        if let Some(k) = k.as_str() {
            let _ = r.cmd(&["DEL", &k]);
        }
    }
}

#[test]
#[ignore = "cần Redis thật ở HUB_REDIS_ADDR (mặc định 127.0.0.1:6379)"]
fn vong_doi_mot_viec_va_khoi_dong_lai() {
    let mut r = Redis::connect().expect("kết nối được Redis");
    let so = so_thu();
    so.chuan_bi(&mut r).unwrap();
    so.chuan_bi(&mut r)
        .expect("chuẩn bị lần hai phải nuốt BUSYGROUP");

    // CỬA NHẬN — và chống nhận trùng theo khoá.
    let id = so
        .nhan(
            &mut r,
            "hom_thu",
            Some("/tmp/x/huba-run.taken-1"),
            "sid-1",
            "echo 1",
            true,
        )
        .unwrap()
        .expect("lần đầu phải nhận");
    assert!(so
        .nhan(
            &mut r,
            "hom_thu",
            Some("/tmp/x/huba-run.taken-1"),
            "sid-1",
            "echo 1",
            true
        )
        .unwrap()
        .is_none());
    assert_eq!(so.lay(&mut r, id).unwrap().unwrap().buoc, Buoc::Nhan);

    // BÊN THỰC THI đọc → doc → chay → kết quả.
    let moi = so.doc_viec_moi(&mut r, 10).unwrap();
    assert_eq!(moi.len(), 1);
    let (muc, id2) = moi[0].clone();
    assert_eq!(id2, id);
    so.ghi_vao(&mut r, id, &muc).unwrap();
    so.danh_dau(&mut r, id, Buoc::Doc, None).unwrap();

    // ── GIẢ LẬP KHỞI ĐỘNG LẠI trước khi chạy: mục vẫn treo, và vẫn là CỦA MÌNH.
    let treo = so.viec_con_treo(&mut r, 10).unwrap();
    assert_eq!(
        treo,
        vec![(muc.clone(), id)],
        "việc đã đọc mà chưa xong phải còn treo"
    );
    assert_eq!(
        khoi_phuc(so.lay(&mut r, id).unwrap().unwrap().buoc),
        KhoiPhuc::ChayLai
    );

    so.danh_dau(&mut r, id, Buoc::Chay, None).unwrap();
    so.ghi_pid(&mut r, id, 4242).unwrap();
    let v = so.lay(&mut r, id).unwrap().unwrap();
    assert_eq!((v.buoc, v.pid), (Buoc::Chay, Some(4242)));
    assert_eq!(khoi_phuc(v.buoc), KhoiPhuc::BaoCoTheDaChay);

    so.ghi_ket_qua(&mut r, id, Some(0), "[huba chạy hộ]\n$ echo 1\n1")
        .unwrap();
    assert!(
        so.viec_con_treo(&mut r, 10).unwrap().is_empty(),
        "có kết quả rồi thì mục hang_viec phải được XACK"
    );

    // BÊN TRẢ đọc; GIẢ LẬP chết trước khi trả xong ⟹ lấy lại được bằng XAUTOCLAIM.
    let kq = so.doc_ket_qua_moi(&mut r, 10).unwrap();
    assert_eq!(kq.len(), 1);
    assert_eq!(so.bat_dau_gui(&mut r, id).unwrap(), 1);
    let lai = so.ket_qua_treo(&mut r, 0, 10).unwrap();
    assert_eq!(lai.len(), 1, "kết quả đã đọc mà chưa trả phải lấy lại được");
    assert_eq!(so.bat_dau_gui(&mut r, id).unwrap(), 2);
    so.danh_dau(&mut r, id, Buoc::DaGui, None).unwrap();
    so.da_tra(&mut r, &lai[0].0).unwrap();
    assert!(
        so.ket_qua_treo(&mut r, 0, 10).unwrap().is_empty(),
        "đã trả thì hết treo"
    );
    let cuoi = so.lay(&mut r, id).unwrap().unwrap();
    assert_eq!(cuoi.buoc, Buoc::DaGui);
    assert_eq!(cuoi.ma_thoat, Some(0));
    assert_eq!(cuoi.lan_gui, 2);
    // Hai mốc bên trả dùng (hạn bỏ cuộc, nhịp trả lại) phải đọc về được.
    assert!(
        cuoi.xong_luc.is_some() && cuoi.gui_luc.is_some(),
        "{cuoi:?}"
    );

    don(&mut r, &so);
}

/// Redis máy này `appendonly no` ⟹ một lần nó khởi động lại có thể mất nhóm
/// đọc. `So::ket_noi` dựng lại nhóm mỗi lần mở, nên sổ tự lành; đối chứng
/// ngược: mở trần (`Redis::connect`, không dựng nhóm) thì `XREADGROUP` phải hỏng
/// `NOGROUP` — tức phép thử này đỏ được.
#[test]
#[ignore = "cần Redis thật ở HUB_REDIS_ADDR (mặc định 127.0.0.1:6379)"]
fn mat_nhom_doc_thi_ket_noi_tu_dung_lai() {
    let so = so_thu();
    let mut r = so.ket_noi().expect("mở sổ");
    // Giả lập Redis mất dữ liệu: xoá cả hai stream (kéo theo nhóm của chúng).
    for s in ["hang_viec", "ket_qua"] {
        r.cmd(&["DEL", &format!("{}{s}", so.pre)]).unwrap();
    }
    let mut tran = Redis::connect().unwrap();
    let hong = so.doc_viec_moi(&mut tran, 1);
    assert!(
        hong.as_ref()
            .err()
            .is_some_and(|e| e.to_string().contains("NOGROUP")),
        "đối chứng ngược: mất nhóm mà đọc vẫn qua ⟹ phép thử mù: {hong:?}"
    );
    let mut lai = so.ket_noi().expect("mở lại sổ");
    assert!(
        so.doc_viec_moi(&mut lai, 1).unwrap().is_empty(),
        "ket_noi phải dựng lại nhóm"
    );
    assert!(so.ket_qua_treo(&mut lai, 0, 1).unwrap().is_empty());
    don(&mut lai, &so);
}
