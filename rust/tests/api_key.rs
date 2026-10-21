//! `/accounts <tên> -k <key>` — một tài khoản Claude chạy bằng API key (Hà 2026-10-21:
//! *"Tôi có api key và muốn dùng cho acc5"* → *"Tạo thêm lệnh chuyển sang sử dụng api"*).
//!
//! Bốn điều phải giữ, mỗi điều có đối chứng ngược (luật 13①):
//! ① key KHÔNG chạm đĩa của huba và không lên câu trả lời — `tin_den` lưu bản che, log che
//!   theo hình dạng, câu trả lời chỉ mang 4 ký tự cuối;
//! ② cất xong phải được CLI xác nhận, không thì trả NGUYÊN BYTE hai tệp;
//! ③ máy chủ từ chối / không kiểm được ⟹ không ghi gì;
//! ④ huba KHÔNG BAO GIỜ tự chọn tài khoản API (luật 8 — tốn tiền phải là cú bấm của người).

mod common;

#[cfg(unix)]
use huba::account_login::{danh_tinh_tat_ca, nhom_trung};
#[cfg(unix)]
use huba::api_key::KiemKey;
use huba::api_key::{duoi, hinh_dang_sai};
#[cfg(unix)]
use huba::config::{ClaudeAccountCfg, Config};
use huba::logging::{che_api_key, redact};
use huba::pipeline::{accounts_order, AccountsOrder};
use huba::quota::{Rank, Ranked};

/// Key GIẢ đúng hình dạng (`sk-ant-api03-` + 95 ký tự, đuôi `WXYZ`).
fn key_gia() -> String {
    format!("sk-ant-api03-{}WXYZ", "Ab9_-".repeat(19))
}

// ───────────── cú pháp ─────────────────────────────────────────────────────────

#[test]
fn cu_phap_k() {
    let k = key_gia();
    assert_eq!(
        accounts_order(&format!("acc5 -k {k}")),
        AccountsOrder::ApiKey("acc5".into(), Some(k.clone()))
    );
    assert_eq!(
        accounts_order(&format!("-k acc5 {k}")),
        AccountsOrder::ApiKey("acc5".into(), Some(k.clone()))
    );
    for bo in ["xoa", "xoá", "XOA", "off"] {
        assert_eq!(
            accounts_order(&format!("acc5 -k {bo}")),
            AccountsOrder::ApiKey("acc5".into(), None),
            "{bo}"
        );
    }
    // `-k` trần KHÔNG được hiểu là bỏ key: gõ dở rồi bấm gửi không được xoá gì.
    assert_eq!(accounts_order("acc5 -k"), AccountsOrder::Usage);
    // Điện thoại bẻ dòng giữa key ⟹ giữ dấu cách để hinh_dang_sai nói đúng chỗ sai.
    match accounts_order("acc5 -k sk-ant-api03-aaaa bbbb") {
        AccountsOrder::ApiKey(_, Some(k)) => assert!(hinh_dang_sai(&k).is_some()),
        x => panic!("{x:?}"),
    }
}

#[test]
fn hinh_dang_key() {
    assert_eq!(hinh_dang_sai(&key_gia()), None);
    assert_eq!(hinh_dang_sai(&format!("  {}\n", key_gia())), None);
    let admin = key_gia().replace("sk-ant-api03-", "sk-ant-admin01-");
    assert!(hinh_dang_sai(&admin)
        .unwrap()
        .contains("không phải API key"));
    assert!(hinh_dang_sai("sk-ant-api03-ngan")
        .unwrap()
        .contains("quá ngắn"));
    // Bản đã che (đường chạy lại từ sổ sau khởi động lại) nói đúng câu của nó.
    let che = che_api_key(&key_gia());
    assert!(hinh_dang_sai(&che).unwrap().contains("ĐÃ CHE"));
    // Lời từ chối không mang key.
    assert!(!hinh_dang_sai(&admin).unwrap().contains(&admin));
    assert_eq!(duoi(&key_gia()), "…WXYZ");
}

// ───────────── ① che key: log + sổ tin_den ─────────────────────────────────────

#[test]
fn che_key_theo_hinh_dang() {
    let k = key_gia();
    let dong = format!("/accounts acc5 -k {k}");
    let che = che_api_key(&dong);
    assert!(!che.contains(&k), "{che}");
    assert_eq!(che, "/accounts acc5 -k sk-ant-…WXYZ");
    // Che hai lần = che một lần (bản đã che không bị che tiếp thành rác).
    assert_eq!(che_api_key(&che), che);
    // Trong JSON (cột `goc` của tin_den) cũng che được.
    let json = serde_json::json!({ "message": { "text": dong } }).to_string();
    assert!(!che_api_key(&json).contains(&k));
    // Đối chứng: câu hướng dẫn nhắc `sk-ant-api` KHÔNG phải key ⟹ giữ nguyên.
    let huong_dan = "phải mở đầu bằng sk-ant-api, key này đuôi …WXYZ";
    assert_eq!(che_api_key(huong_dan), huong_dan);
    // Mọi dòng log đi qua `redact` ⟹ che luôn, cạnh phép che token bot cũ.
    let loi = format!("POST https://api.telegram.org/bot123:ABC/sendMessage {k}");
    let r = redact(&loi);
    assert!(!r.contains(&k) && !r.contains("123:ABC"), "{r}");
}

#[test]
fn tin_den_luu_ban_da_che() {
    let dir = tempfile::tempdir().unwrap();
    let c = huba::tin_den::mo(&dir.path().join("huba.sqlite")).unwrap();
    let k = key_gia();
    let u = serde_json::json!({ "update_id": 7,
        "message": { "message_id": 70, "date": 1, "chat": { "id": 1 },
                     "text": format!("/accounts acc5 -k {k}") } });
    assert!(huba::tin_den::ghi_nhan(&c, &u).unwrap());
    let (chu, goc): (String, String) = c
        .query_row("SELECT chu, goc FROM tin_den WHERE update_id=7", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(!chu.contains(&k) && !goc.contains(&k), "{chu} | {goc}");
    // Đối chứng: sổ vẫn giữ đủ để người đọc nhận ra tin của mình.
    assert_eq!(chu, "/accounts acc5 -k sk-ant-…WXYZ");
}

// ───────────── ② ③ cất key: thư mục tạm + `claude` giả ─────────────────────────

/// `claude` GIẢ cho `auth status`: `dung` ⟹ khai `api_key_helper` khi settings.json có
/// `apiKeyHelper` (y hệt CLI 2.1.280 đo 21/10); `khong_nhan` ⟹ luôn khai `claude.ai`.
#[cfg(unix)]
fn claude_gia(d: &std::path::Path, kieu: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let p = d.join(format!("claude-{kieu}.sh"));
    let than = match kieu {
        "dung" => {
            "if grep -q apiKeyHelper \"$CLAUDE_CONFIG_DIR/settings.json\" 2>/dev/null; then\n\
             printf '{\"loggedIn\":true,\"authMethod\":\"api_key_helper\",\"apiKeySource\":\"apiKeyHelper\"}\\n'\n\
             else printf '{\"loggedIn\":false,\"authMethod\":\"none\"}\\n'; exit 1; fi\n"
        }
        _ => "printf '{\"loggedIn\":true,\"authMethod\":\"claude.ai\",\"apiKeySource\":\"none\"}\\n'\n",
    };
    std::fs::write(&p, format!("#!/bin/bash\n{than}")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p.display().to_string()
}

#[cfg(unix)]
fn cfg_mot(dir: &std::path::Path, cli: String, locked: bool) -> Config {
    let mut c = common::cfg_for_tests();
    c.claude_cli = cli;
    c.claude_accounts = vec![ClaudeAccountCfg {
        name: "acc5".into(),
        config_dir: Some(dir.display().to_string()),
        launch: None,
        locked,
    }];
    c
}

#[cfg(unix)]
const SETTINGS_CU: &str = "{\n  \"theme\": \"dark\",\n  \"switchModelsOnFlag\": true\n}\n";

#[cfg(unix)]
#[test]
fn dat_key_dung_ghi_hai_tep_va_cli_xac_nhan() {
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    let acc5 = t.path().join("acc5");
    std::fs::create_dir_all(&acc5).unwrap();
    std::fs::write(acc5.join("settings.json"), SETTINGS_CU).unwrap();
    let c = cfg_mot(&acc5, claude_gia(t.path(), "dung"), true);
    let k = key_gia();
    let ack = huba::api_key::dat(&c, "acc5", &k, &|_| KiemKey::Dung);

    assert!(
        ack.starts_with("🔑 acc5 nay chạy bằng API key …WXYZ"),
        "{ack}"
    );
    assert!(!ack.contains(&k), "câu trả lời mang nguyên key: {ack}");
    assert!(ack.contains("vẫn KHOÁ"), "{ack}");
    // Tệp key: đúng nội dung, quyền 600.
    let p = acc5.join(huba::api_key::TEP_KEY);
    assert_eq!(std::fs::read_to_string(&p).unwrap(), k);
    assert_eq!(
        std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o600
    );
    // settings.json: giữ khoá cũ, thêm đúng helper của huba.
    let s: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(acc5.join("settings.json")).unwrap())
            .unwrap();
    assert_eq!(s["theme"], "dark");
    assert_eq!(s["switchModelsOnFlag"], true);
    assert_eq!(s["apiKeyHelper"], huba::api_key::helper_cua_huba(&acc5));
    assert_eq!(huba::api_key::nhan(&acc5).as_deref(), Some("API key …WXYZ"));

    // `-k xoa`: trả về gói, giữ khoá cũ, xoá tệp key.
    let bo = huba::api_key::go(&c, "acc5");
    assert!(bo.contains("thôi dùng API key"), "{bo}");
    assert!(!p.exists());
    let s: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(acc5.join("settings.json")).unwrap())
            .unwrap();
    assert!(s.get("apiKeyHelper").is_none());
    assert_eq!(s["theme"], "dark");
    assert_eq!(huba::api_key::nhan(&acc5), None);
}

/// ② CLI không xác nhận ⟹ trả NGUYÊN BYTE settings.json, không để lại tệp key.
#[cfg(unix)]
#[test]
fn cli_khong_nhan_thi_tra_nguyen_trang() {
    let t = tempfile::tempdir().unwrap();
    let acc5 = t.path().join("acc5");
    std::fs::create_dir_all(&acc5).unwrap();
    std::fs::write(acc5.join("settings.json"), SETTINGS_CU).unwrap();
    let c = cfg_mot(&acc5, claude_gia(t.path(), "khong_nhan"), true);
    let ack = huba::api_key::dat(&c, "acc5", &key_gia(), &|_| KiemKey::Dung);
    assert!(
        ack.contains("CHƯA đổi gì") && ack.contains("nguyên trạng"),
        "{ack}"
    );
    assert!(ack.contains("claude.ai"), "phải nói CLI khai gì: {ack}");
    assert_eq!(
        std::fs::read_to_string(acc5.join("settings.json")).unwrap(),
        SETTINGS_CU
    );
    assert!(!acc5.join(huba::api_key::TEP_KEY).exists());
    assert_eq!(huba::api_key::nhan(&acc5), None);
}

/// ③ Máy chủ từ chối hay không kiểm được ⟹ không ghi một byte nào.
#[cfg(unix)]
#[test]
fn may_chu_tu_choi_thi_khong_ghi_gi() {
    let t = tempfile::tempdir().unwrap();
    let acc5 = t.path().join("acc5");
    std::fs::create_dir_all(&acc5).unwrap();
    std::fs::write(acc5.join("settings.json"), SETTINGS_CU).unwrap();
    let c = cfg_mot(&acc5, claude_gia(t.path(), "dung"), true);
    let k = key_gia();
    for kq in [
        KiemKey::Sai("máy chủ trả 401 invalid x-api-key".into()),
        KiemKey::KhongKiemDuoc("không gọi được api.anthropic.com".into()),
    ] {
        let ack = huba::api_key::dat(&c, "acc5", &k, &|_| kq.clone());
        assert!(ack.contains("CHƯA đổi gì") && !ack.contains(&k), "{ack}");
        assert_eq!(
            std::fs::read_to_string(acc5.join("settings.json")).unwrap(),
            SETTINGS_CU
        );
        assert!(!acc5.join(huba::api_key::TEP_KEY).exists());
    }
    // Hình dạng sai thì còn không hỏi máy chủ.
    let ack = huba::api_key::dat(
        &c,
        "acc5",
        "sk-ant-admin01-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        &|_| panic!("hình dạng sai mà vẫn hỏi máy chủ"),
    );
    assert!(ack.contains("không phải API key"), "{ack}");
}

/// settings.json hỏng JSON ⟹ KHÔNG ghi đè (đó là tệp chủ máy tự sửa).
#[cfg(unix)]
#[test]
fn settings_hong_thi_khong_ghi_de() {
    let t = tempfile::tempdir().unwrap();
    let acc5 = t.path().join("acc5");
    std::fs::create_dir_all(&acc5).unwrap();
    std::fs::write(acc5.join("settings.json"), "{ hỏng").unwrap();
    let c = cfg_mot(&acc5, claude_gia(t.path(), "dung"), false);
    let ack = huba::api_key::dat(&c, "acc5", &key_gia(), &|_| KiemKey::Dung);
    assert!(ack.contains("hỏng JSON"), "{ack}");
    assert_eq!(
        std::fs::read_to_string(acc5.join("settings.json")).unwrap(),
        "{ hỏng"
    );
    assert!(!acc5.join(huba::api_key::TEP_KEY).exists());
}

/// `-k xoa` không đụng helper KHÔNG phải của huba.
#[cfg(unix)]
#[test]
fn khong_go_helper_cua_nguoi_khac() {
    let t = tempfile::tempdir().unwrap();
    let acc5 = t.path().join("acc5");
    std::fs::create_dir_all(&acc5).unwrap();
    let cua_chu_may = "{\"apiKeyHelper\": \"~/bin/lay-key.sh\"}\n";
    std::fs::write(acc5.join("settings.json"), cua_chu_may).unwrap();
    let c = cfg_mot(&acc5, claude_gia(t.path(), "dung"), false);
    let ack = huba::api_key::go(&c, "acc5");
    assert!(ack.contains("KHÔNG phải của huba"), "{ack}");
    assert_eq!(
        std::fs::read_to_string(acc5.join("settings.json")).unwrap(),
        cua_chu_may
    );
    assert_eq!(
        huba::api_key::nhan(&acc5).as_deref(),
        Some("API key (apiKeyHelper ngoài huba)")
    );
}

// ───────────── ④ không tự chọn + danh tính ────────────────────────────────────

#[cfg(unix)]
#[test]
fn tai_khoan_api_khong_bao_gio_duoc_tu_chon() {
    let t = tempfile::tempdir().unwrap();
    let acc5 = t.path().join("acc5");
    std::fs::create_dir_all(&acc5).unwrap();
    let mut c = cfg_mot(&acc5, claude_gia(t.path(), "dung"), false);
    assert!(!huba::quota::dung_api_key(&c, "acc5"), "chưa đặt key");
    let ack = huba::api_key::dat(&c, "acc5", &key_gia(), &|_| KiemKey::Dung);
    assert!(
        ack.contains("KHÔNG tự chọn"),
        "mở khoá thì phải nói rõ: {ack}"
    );
    assert!(huba::quota::dung_api_key(&c, "acc5"));
    let hang = huba::quota::rank_all(&c, chrono::Utc::now().timestamp_millis());
    assert_eq!(hang[0].rank, Rank::ApiKey);
    // Khoá vẫn thắng: quyết định của chủ máy đè mọi phép đo.
    c.claude_accounts[0].locked = true;
    let hang = huba::quota::rank_all(&c, chrono::Utc::now().timestamp_millis());
    assert_eq!(hang[0].rank, Rank::Locked);
}

/// Cửa chọn: tài khoản API là cái DUY NHẤT còn chỗ mà vẫn không được chọn.
#[test]
fn goi_y_khong_bao_gio_tra_tai_khoan_api() {
    let hang = vec![
        Ranked {
            name: "acc3".into(),
            rank: Rank::Full,
        },
        Ranked {
            name: "acc5".into(),
            rank: Rank::ApiKey,
        },
    ];
    assert_eq!(
        huba::watch::suggest_account("acc3", &hang, &[], 12 * 60),
        None
    );
    // Đối chứng: cùng hàng ấy mà acc5 là gói chưa đo ⟹ được chọn.
    let goi = vec![
        Ranked {
            name: "acc3".into(),
            rank: Rank::Full,
        },
        Ranked {
            name: "acc5".into(),
            rank: Rank::Unknown,
        },
    ];
    assert_eq!(
        huba::watch::suggest_account("acc3", &goi, &[], 12 * 60).as_deref(),
        Some("acc5")
    );
}

/// Ca thật 21/10: acc5 còn đăng nhập gói CŨ trùng acc3 (trogiup). Chạy bằng API key thì
/// `/accounts` phải in nhãn key và THÔI báo trùng.
#[cfg(unix)]
#[test]
fn danh_tinh_in_nhan_key_va_thoi_bao_trung() {
    let t = tempfile::tempdir().unwrap();
    let so = "{\"oauthAccount\":{\"emailAddress\":\"trogiup.gdk@gmail.com\"}}";
    let mut accs = Vec::new();
    for ten in ["acc3", "acc5"] {
        let d = t.path().join(ten);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(".claude.json"), so).unwrap();
        accs.push(ClaudeAccountCfg {
            name: ten.into(),
            config_dir: Some(d.display().to_string()),
            launch: None,
            locked: false,
        });
    }
    let mut c = common::cfg_for_tests();
    c.claude_cli = claude_gia(t.path(), "dung");
    c.claude_accounts = accs;
    // Đối chứng: trước khi đặt key thì đúng là trùng.
    assert_eq!(nhom_trung(&danh_tinh_tat_ca(&c)).len(), 1);
    let ack = huba::api_key::dat(&c, "acc5", &key_gia(), &|_| KiemKey::Dung);
    assert!(ack.starts_with("🔑"), "{ack}");
    let ids = danh_tinh_tat_ca(&c);
    assert_eq!(
        ids[1],
        ("acc5".to_string(), Some("API key …WXYZ".to_string()))
    );
    assert!(nhom_trung(&ids).is_empty(), "{ids:?}");
}
