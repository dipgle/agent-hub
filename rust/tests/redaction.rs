use hub::redaction::{compile_extra, is_external_channel, leak_scan};

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
        (
            "xem /Users/hanguyen/Documents/projects/tfl5",
            "local_filesystem_path",
        ),
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

#[test]
fn only_channels_that_leave_the_machine_are_gated() {
    assert!(is_external_channel("github"));
    assert!(is_external_channel("email"));
    assert!(is_external_channel("telegram"));
    assert!(
        !is_external_channel("notify"),
        "the local brief may contain internal detail"
    );
    assert!(!is_external_channel("devlog"));
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
