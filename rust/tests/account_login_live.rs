//! `/accounts <tên> -l` chạy THẬT trên một thư mục cấu hình TRỐNG — không đụng tài
//! khoản thật nào.
//!
//! 🔴 Vì sao phải live: bài kiểm thuần (`tests/tai_khoan_chi_tiet.rs`) chỉ nói huba
//! ĐỌC đúng một màn đã chép. Nó không nói `open_window` có mở ra đúng `auth login`,
//! đường dẫn có hiện trong 20 s, `do script` có gửi được mã vào ô dán, và cửa sổ có
//! đóng sạch không — mà đúng mấy chuyện ấy mới quyết định Hà có đăng nhập được từ
//! điện thoại hay không.
//!
//! Mã gõ vào là mã GIẢ ⟹ CLI trả `Login failed: … 400` rồi thoát (đo 11/10). `claude`
//! được bọc một lệnh `open` giả để không bật tab trình duyệt nào trên máy.
//!
//! ```
//! cd ~/projects/huba/rust
//! cargo test --offline --test account_login_live -- --ignored --nocapture
//! ```

mod common;

use std::path::Path;

use huba::config::ClaudeAccountCfg;

fn script(p: &Path, body: &str) {
    std::fs::write(p, body).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
#[ignore = "mở một cửa sổ Terminal thật — chạy tay"]
fn dang_nhap_tron_vong_tren_thu_muc_trong() {
    let d = tempfile::tempdir().unwrap();
    let shim = d.path().join("shim");
    std::fs::create_dir_all(&shim).unwrap();
    script(&shim.join("open"), "#!/bin/bash\nexit 0\n");
    let that = std::process::Command::new("zsh")
        .args(["-lc", "command -v claude"])
        .output()
        .unwrap();
    let that = String::from_utf8_lossy(&that.stdout).trim().to_string();
    assert!(!that.is_empty(), "máy này không có `claude` trên PATH");
    let boc = d.path().join("claude-boc.sh");
    script(
        &boc,
        &format!(
            "#!/bin/bash\nPATH='{}':\"$PATH\" exec '{}' \"$@\"\n",
            shim.display(),
            that
        ),
    );
    let thu = d.path().join("acc-thu");
    std::fs::create_dir_all(&thu).unwrap();

    let mut cfg = common::cfg_for_tests();
    cfg.claude_cli = boc.display().to_string();
    cfg.claude_accounts = vec![ClaudeAccountCfg {
        name: "thu".into(),
        config_dir: Some(thu.display().to_string()),
        ..Default::default()
    }];
    let db = huba::db::Db::open(&d.path().join("huba.sqlite")).unwrap();

    let a = huba::account_login::start(&cfg, &db, "thu");
    println!("── start ──\n{a}");
    assert!(
        a.contains("https://claude.com/cai/oauth/authorize?")
            && a.contains("/accounts thu -l <mã>"),
        "{a}"
    );
    let w: i64 = a
        .split("cửa sổ Terminal ")
        .nth(1)
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .expect("câu dẫn đường phải nêu id cửa sổ");

    // Lượt -l thứ hai khi cửa sổ còn chờ ⟹ nhắc lại, KHÔNG mở cái mới.
    let b = huba::account_login::start(&cfg, &db, "thu");
    assert!(
        b.contains("nhắc lại") && b.contains(&format!("cửa sổ Terminal {w} ")),
        "{b}"
    );

    // Sổ cũ quá 15′ ⟹ lượt -l mới phải TẮT đúng CLI cũ (không gõ chữ vào nó), đóng cửa sổ
    // cũ, mở cửa sổ khác.
    let cu: serde_json::Value =
        serde_json::from_str(&db.cursor_or_log("login:thu").expect("có sổ")).unwrap();
    let tty_cu = cu["tty"]
        .as_str()
        .unwrap()
        .trim_start_matches("/dev/")
        .to_string();
    // 🔴 Chấm theo PID, KHÔNG theo tên tty: lượt chạy đầu của bài này đỏ oan vì cửa sổ MỚI
    // dựng lên đã dùng lại đúng `ttys002` của cửa sổ vừa đóng (luật 11b — tty là con số
    // ĐƯỢC DÙNG LẠI).
    let ps = std::process::Command::new("ps")
        .args(["-t", &tty_cu, "-o", "pid=,command="])
        .output()
        .unwrap();
    let pid_cu = huba::account_login::login_pid(&String::from_utf8_lossy(&ps.stdout))
        .expect("CLI cũ phải đang chạy trước khi huỷ");
    let mut gia = cu.clone();
    gia["at_ms"] = serde_json::json!(0);
    db.set_cursor("login:thu", &gia.to_string()).unwrap();
    let a2 = huba::account_login::start(&cfg, &db, "thu");
    let w2: i64 = a2
        .split("cửa sổ Terminal ")
        .nth(1)
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .expect("câu dẫn đường phải nêu id cửa sổ");
    assert_ne!(w2, w, "phải là cửa sổ MỚI");
    assert!(
        huba::keys::window_gone(w).unwrap_or(false),
        "cửa sổ cũ {w} còn đó"
    );
    let con = std::process::Command::new("ps")
        .args(["-p", &pid_cu.to_string()])
        .status()
        .unwrap()
        .success();
    assert!(!con, "CLI cũ pid {pid_cu} còn sống");
    let w = w2;

    let c = huba::account_login::finish(&cfg, &db, "thu", "thu-ma-gia-123#abc");
    println!("── finish ──\n{c}");
    assert!(c.contains("Login failed"), "{c}");
    assert!(c.contains("đã đóng cửa sổ"), "{c}");
    assert!(
        huba::keys::window_gone(w).unwrap_or(false),
        "cửa sổ {w} còn đó"
    );

    // Sổ đã xoá ⟹ gửi mã lần nữa thì nói thẳng là không có cửa sổ nào chờ.
    let e = huba::account_login::finish(&cfg, &db, "thu", "thu-ma-gia-456#abc");
    assert!(e.contains("không có cửa sổ đăng nhập nào đang chờ"), "{e}");
}
