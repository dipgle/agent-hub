use huba::redaction::{compile_extra, leak_scan};

#[test]
fn the_observed_real_leak_is_caught() {
    // Verbatim shape of the evidence line produced by a real triage run on 2026-07-26.
    let draft = "memory: tfl5 security hardening 2026-07 — CÒN OPEN: PG SPOF, HMAC-key rotate";
    let hits = leak_scan(draft, &[]);
    assert!(
        hits.contains(&"internal_notes_citation".to_string()),
        "{hits:?}"
    );
    assert!(
        hits.contains(&"internal_risk_language".to_string()),
        "{hits:?}"
    );
}

#[test]
fn hosts_ips_paths_wikilinks_and_credentials_are_caught() {
    let cases = [
        ("deploy chạy trên vps-a rồi reload", "internal_host"),
        ("node ở 46.250.231.130:41100", "ip_address"),
        ("xem /Users/hanguyen/projects/tfl5", "local_filesystem_path"),
        (
            "chi tiết ở [[tfl5-deploy-nopasswd-scope]]",
            "memory_wikilink",
        ),
        ("dùng bearer token của admin", "credential_word"),
        (
            "token: 123456789:AAHfSjKLmnOPqrstuvwxyz0123456789abc",
            "credential_literal",
        ),
        ("-----BEGIN RSA PRIVATE KEY-----", "private_key_block"),
    ];
    for (text, label) in cases {
        assert!(
            leak_scan(text, &[]).contains(&label.to_string()),
            "{label} missed in: {text}"
        );
    }
}

#[test]
fn an_ordinary_customer_facing_reply_passes_clean() {
    for t in [
        "Cảm ơn bạn đã báo. Mình đã ghi nhận lỗi ở trang đăng nhập và sẽ kiểm tra trong hôm nay.",
        "Tính năng xuất Excel dự kiến có trong bản tới. Mình sẽ thông báo khi xong.",
        "Bạn thử đăng xuất rồi đăng nhập lại giúp mình nhé, nếu vẫn lỗi mình sẽ kiểm tra tiếp.",
    ] {
        assert!(leak_scan(t, &[]).is_empty(), "false positive on: {t}");
    }
}

/// There used to be a per-channel decision here — "does this channel leave the
/// machine?" — and the scan only ran when the answer was yes. The channels it
/// distinguished are gone; what remains (a session preview on its way into a
/// doc on a server) always leaves. The gate is unconditional now, which is one
/// fewer place for a new caller to land on the wrong side of a list.
#[test]
fn a_password_in_a_session_preview_is_caught_without_asking_which_channel() {
    let hits = leak_scan("mật khẩu đăng nhập là Abc@12345", &[]);
    assert!(
        hits.iter().any(|h| h.starts_with("credential")),
        "a stated password must be flagged: {hits:?}"
    );
}

#[test]
fn config_patterns_compile_and_a_broken_one_is_reported_not_swallowed() {
    let compiled = compile_extra(&["tafalo-internal".to_string(), "([unclosed".to_string()]);
    assert_eq!(
        compiled.len(),
        1,
        "the invalid pattern must be dropped, not panic"
    );
    assert!(!leak_scan("dự án tafalo-internal", &compiled).is_empty());
}

/// 🔴 Câu LỖI cũng phải qua cổng — đo 2026-08-11.
///
/// `reqwest` dựng câu lỗi bằng cách in nguyên URL, mà URL Telegram mang token
/// trong đường dẫn. Vòng đọc mới hỏng mạng vài lần là **28 dòng log mang nguyên
/// khoá bot**, trong một tệp nằm lâu trên đĩa. Luật 4 của dự án nói "log TÊN
/// khoá, không bao giờ giá trị" — và chỗ dễ quên nhất chính là đường lỗi.
/// ⚠ Mẫu thử dưới đây là token **BỊA**, cùng hình dạng chứ không cùng giá trị.
/// Bản đầu của test này chép nguyên token THẬT của bot vào đây (bắt được
/// 2026-08-11 trước khi commit, `git log -S` = 0 commit): một tệp test cũng là
/// mã nguồn, và mã nguồn không phải chỗ của bí mật — đúng luật 4 mà chính test
/// này sinh ra để bảo vệ.
#[test]
fn an_error_string_never_carries_the_bot_token() {
    let raw = "error sending request for url (https://api.telegram.org/bot1234567890:AAFakeFakeFakeFakeFakeFakeFakeFake00/getUpdates?offset=1&timeout=20)";
    let safe = huba::logging::redact(raw);
    assert!(!safe.contains("AAFakeFake"), "token còn nguyên: {safe}");
    assert!(!safe.contains("1234567890:"), "token còn nguyên: {safe}");
    // Vẫn phải đọc được là hỏng ở đâu, nếu không thì che xong hoá mù.
    assert!(safe.contains("getUpdates"), "mất mất chỗ hỏng: {safe}");
    assert!(safe.contains("<token>"), "phải nói rõ đã che: {safe}");
    // Chuỗi không có token thì giữ nguyên từng chữ.
    let plain = "không đọc được nhật ký phiên";
    assert_eq!(huba::logging::redact(plain), plain);
}

/// Cân cho TỆP hẹp hơn cân cho phần xem trước — và đó là chủ ý.
///
/// 🔴 Hà 2026-08-13, ngay cú bấm nút 📎 đầu tiên: *"chưa gửi được
/// huba.env.example — giữ lại: có dấu hiệu bí mật (credential_word_vi)"*. Tệp
/// ấy là BẢN MẪU, mọi giá trị đều rỗng; thứ khớp chỉ là chữ "Mật khẩu" trong
/// nhãn ô nhập. Dùng cân của "thư gửi người ngoài" cho một tệp chủ máy gọi
/// đích danh thì chặn gần hết tài liệu — và một cái cổng chặn hết là một cái
/// cổng không ai dùng nữa.
#[test]
fn a_template_file_is_not_a_secret_but_a_filled_in_one_is() {
    use huba::redaction::file_risk;

    // Đúng bản mẫu đã bị chặn oan: có chữ "Mật khẩu", giá trị RỖNG.
    let template = "# Mật khẩu tfl5 của tài khoản bot\nHUB_TFL5_PASSWORD=\nHUB_TFL5_USER=\n";
    assert!(file_risk(template).is_empty(), "{:?}", file_risk(template));

    // Tài liệu bình thường: đường dẫn máy, trích CLAUDE.md, tên host — chủ máy
    // đã có sẵn hết, gửi vào phòng chat của chính anh thì không phải rò rỉ.
    let doc =
        "Xem /Users/hanguyen/projects/AI/huba/CLAUDE.md: mục vps-a, [[abc]], blocker còn treo";
    assert!(file_risk(doc).is_empty(), "{:?}", file_risk(doc));

    // …nhưng GIÁ TRỊ thật thì chặn, cả ba hình dạng.
    assert_eq!(
        file_risk("HUB_TFL5_PASSWORD=abc123xyz"),
        vec!["secret_assignment".to_string()]
    );
    assert_eq!(
        file_risk("Mật khẩu: chim-non-2026"),
        vec!["secret_assignment".to_string()]
    );
    assert!(file_risk("token = sk-abcdefghijklmnop").contains(&"credential_literal".to_string()));
    assert!(file_risk("-----BEGIN RSA PRIVATE KEY-----").contains(&"private_key_block".to_string()));
}

/// Một dòng CHÚ THÍCH không được đọc thành một bí mật.
///
/// 🔴 Hà 2026-08-13, bấm nút 📎 lấy `mailler/scripts/dkim-doctor.sh` và nhận
/// *"⚠ chưa gửi được … giữ lại: có dấu hiệu bí mật (secret_assignment)"*. Cả
/// tệp 237 dòng chỉ có ĐÚNG một dòng khớp, và nó là dòng dưới đây — `pass` ở
/// đây là **giá trị** của `spf=`, còn dấu `=` bị tóm là của `dkim=fail` ở tận
/// cuối câu. Cầu nối `[^\n:=]*` không có trần nên nối được hai thứ chẳng liên
/// quan.
#[test]
fn a_dmarc_comment_is_not_a_leaked_password() {
    use huba::redaction::file_risk;

    let real = "#   The 2026-08-10 DMARC report says spf=pass, dkim=fail for a message this";
    assert!(
        file_risk(real).is_empty(),
        "chặn nhầm một dòng chú thích: {:?}",
        file_risk(real)
    );

    // …mà cửa vẫn phải đóng đúng chỗ nó sinh ra để đóng.
    assert_eq!(
        file_risk("HUB_TFL5_PASSWORD=abc123xyz"),
        vec!["secret_assignment".to_string()],
        "biến môi trường có giá trị vẫn phải chặn"
    );
    assert_eq!(
        file_risk("Mật khẩu đăng nhập: chim-non-2026"),
        vec!["secret_assignment".to_string()],
        "văn xuôi tiếng Việt có khoảng cách vẫn phải chặn"
    );
    assert_eq!(
        file_risk("  \"api_key\": \"sk-live-abcdefgh\""),
        vec!["secret_assignment".to_string()],
        "JSON có nháy kép vẫn phải chặn"
    );
    // Bản mẫu thì không: đó là tệp người mới cần đọc nhất.
    assert!(
        file_risk("HUB_TFL5_PASSWORD=\nHUB_TFL5_USER=\n").is_empty(),
        "chặn cả bản mẫu"
    );
}
