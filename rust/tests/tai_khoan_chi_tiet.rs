//! `/accounts detail <tên>` — xem MỘT tài khoản, kể cả email.
//!
//! Hà 2026-09-29: *"Lệnh nào để xem chi tiết thông tin một acc? Bao gồm cả
//! email? Ví dụ `/accounts detail acc3`"*. Trước lượt này không có đường nào:
//! email chỉ từng được đọc trong ảnh chụp của trang tfl5 đã gỡ.
//!
//! Danh tính đọc từ `oauthAccount` trong sổ `.claude.json` — đọc TỆP, không
//! spawn `claude`, nên xem được cả tài khoản đang khoá mà không tiêu hạn mức.

mod common;

use std::path::Path;

use huba::config::{ClaudeAccountCfg, Config};
use huba::pipeline::accounts_order;
use huba::quota::{identity, Identity};
use huba::runtime::account_detail_text;
use huba::sessions::{LiveSession, SessionsSnapshot};

const BAY_GIO: i64 = 1_790_000_000_000;

fn so(dir: &Path, noi_dung: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(".claude.json"), noi_dung).unwrap();
}

const SO_DAY_DU: &str = r#"{"oauthAccount":{"emailAddress":"trogiup.gdk@gmail.com",
    "displayName":"Dwork","organizationName":"trogiup's Organization",
    "organizationRole":"admin","billingType":"stripe_subscription"}}"#;

fn cfg_voi(dir3: Option<&Path>, khoa3: bool) -> Config {
    let mut c = common::cfg_for_tests();
    c.claude_accounts = vec![
        ClaudeAccountCfg {
            name: "acc1".into(),
            launch: Some("claude".into()),
            ..Default::default()
        },
        ClaudeAccountCfg {
            name: "acc3".into(),
            config_dir: Some(
                dir3.map(|d| d.display().to_string())
                    .unwrap_or_else(|| "~/.claude-acc3".into()),
            ),
            launch: Some("claude3".into()),
            locked: khoa3,
        },
    ];
    c
}

// ── Đọc danh tính ───────────────────────────────────────────────────────────

#[test]
fn identity_doc_du_nam_truong_tu_oauthaccount() {
    let d = tempfile::tempdir().unwrap();
    so(d.path(), SO_DAY_DU);
    let i = identity(Some(d.path())).unwrap();
    assert_eq!(
        i,
        Identity {
            email: Some("trogiup.gdk@gmail.com".into()),
            display_name: Some("Dwork".into()),
            org: Some("trogiup's Organization".into()),
            role: Some("admin".into()),
            billing: Some("stripe_subscription".into()),
        }
    );
}

/// Ba cách hỏng đo được là ba câu KHÁC nhau — không gộp thành "email trống".
#[test]
fn identity_noi_ro_vi_sao_khong_doc_duoc() {
    let d = tempfile::tempdir().unwrap();
    let e = identity(Some(&d.path().join("khong-co"))).unwrap_err();
    assert!(e.contains("chưa đăng nhập") && e.contains("chưa có"), "{e}");

    so(d.path(), r#"{"hasCompletedOnboarding":true}"#);
    let e = identity(Some(d.path())).unwrap_err();
    assert!(e.contains("không có oauthAccount"), "{e}");

    so(d.path(), "{ hỏng");
    let e = identity(Some(d.path())).unwrap_err();
    assert!(e.contains("hỏng JSON"), "{e}");
}

// ── Cú pháp ─────────────────────────────────────────────────────────────────

#[test]
fn cu_phap_ten_tran_va_detail_cu() {
    use huba::pipeline::AccountsOrder::*;
    // Dạng mới (Hà 10/10): tên trần = xem chi tiết — kể cả tên lạ, để route trả
    // DANH SÁCH tài khoản thay vì câu hướng dẫn.
    assert_eq!(accounts_order("acc3"), Detail("acc3".into()));
    assert_eq!(accounts_order("acc9"), Detail("acc9".into()));
    // Dạng cũ vẫn nhận.
    assert_eq!(accounts_order("detail acc3"), Detail("acc3".into()));
    assert_eq!(accounts_order("chitiet acc3"), Detail("acc3".into()));
    // Vế ngược: thiếu tên / thừa chữ ⟹ hướng dẫn.
    assert_eq!(accounts_order("detail"), Usage);
    assert_eq!(accounts_order("detail acc3 thêm"), Usage);

    let (k, _, arg) = huba::verbs::parse_command("/accounts acc3").unwrap();
    assert_eq!(k, huba::adapters::CommandKind::Accounts);
    assert_eq!(arg, "acc3");
}

// ── Câu trả lời ─────────────────────────────────────────────────────────────

fn phien(acc: &str, ten: &str, host: &str) -> LiveSession {
    LiveSession {
        account: acc.into(),
        name: ten.into(),
        host: host.into(),
        ..Default::default()
    }
}

#[test]
fn chi_tiet_in_email_khoa_va_phien_dang_chay() {
    let snap = SessionsSnapshot {
        sessions: vec![
            phien("acc3", "projects-1a", ""),
            phien("acc3", "projects-da-tat", "dead"),
            phien("acc1", "projects-khac", ""),
        ],
        ..Default::default()
    };
    let dead = std::collections::BTreeMap::new();
    let id = identity_mau();
    let t = account_detail_text(
        &cfg_voi(None, true),
        &snap,
        "acc3",
        None,
        None,
        Ok(id),
        &dead,
        BAY_GIO,
    );
    assert!(t.contains("email: trogiup.gdk@gmail.com"), "{t}");
    assert!(t.contains("tổ chức trogiup's Organization"), "{t}");
    assert!(t.contains("🔒 ĐANG KHOÁ"), "{t}");
    assert!(t.contains("/accounts acc3 -a"), "{t}");
    assert!(t.contains("~/.claude-acc3") && t.contains("claude3"), "{t}");
    assert!(t.contains("phiên đang chạy (1): projects-1a"), "{t}");
    assert!(
        !t.contains("projects-da-tat"),
        "phiên đã tắt không phải đang chạy: {t}"
    );
    assert!(
        !t.contains("projects-khac"),
        "phiên của tài khoản khác: {t}"
    );

    // Vế ngược: mở khoá ⟹ không dấu khoá, chỉ đường KHOÁ.
    let mo = account_detail_text(
        &cfg_voi(None, false),
        &SessionsSnapshot::default(),
        "acc3",
        None,
        None,
        Ok(identity_mau()),
        &dead,
        BAY_GIO,
    );
    assert!(!mo.contains("ĐANG KHOÁ"), "{mo}");
    assert!(mo.contains("/accounts acc3 -b"), "{mo}");
    assert!(mo.contains("phiên đang chạy: không có"), "{mo}");
}

fn identity_mau() -> Identity {
    Identity {
        email: Some("trogiup.gdk@gmail.com".into()),
        org: Some("trogiup's Organization".into()),
        ..Default::default()
    }
}

#[test]
fn chi_tiet_noi_ro_khi_khong_doc_duoc_email_va_ten_la() {
    let dead = std::collections::BTreeMap::new();
    let snap = SessionsSnapshot::default();
    let t = account_detail_text(
        &cfg_voi(None, false),
        &snap,
        "acc3",
        None,
        None,
        Err("chưa đăng nhập (sổ không có oauthAccount)".into()),
        &dead,
        BAY_GIO,
    );
    assert!(
        t.contains("email: chưa đọc được — chưa đăng nhập"),
        "phải nói VÌ SAO không có email: {t}"
    );
    let la = account_detail_text(
        &cfg_voi(None, false),
        &snap,
        "acc9",
        None,
        None,
        Ok(identity_mau()),
        &dead,
        BAY_GIO,
    );
    assert!(la.contains("Không có tài khoản 'acc9'"), "{la}");
    assert!(la.contains("acc1 · acc3"), "{la}");
}

/// `claude` GIẢ: mỗi lần bị gọi ghi một dòng vào `<config_dir>/goi.log`, rồi in câu
/// `/usage` với tuần 42 % — cùng khuôn với `tests/usage_do_khi_chon.rs`.
///
/// Chỉ Unix (kịch bản bash + quyền 0o755): thiếu cổng này thì clippy đích Windows
/// của `gate.sh` đỏ — đỏ từ 11/10 tới 20/10 mà không lượt cổng trọn nào thấy.
#[cfg(unix)]
fn claude_gia(dir: &Path) -> std::path::PathBuf {
    let p = dir.join("claude-gia.sh");
    std::fs::write(
        &p,
        "#!/bin/bash\n\
         echo x >> \"$CLAUDE_CONFIG_DIR/goi.log\"\n\
         printf '{\"result\":\"Current session: 3%% used\\\\nCurrent week (all models): 42%% used\"}\\n'\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[cfg(unix)]
fn so_lan_goi(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("goi.log"))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

/// Trọn đường: thư mục cấu hình thật (tạm) → sổ `.claude.json` → đo lại `/usage` → câu.
///
/// 🔴 Hà 2026-10-10: *"Lệnh `/accounts detail acc` vẫn phải đo lại được kể cả đang
/// khóa"*. Tài khoản ở đây ĐANG KHOÁ, và lượt dò phải chạy đúng MỘT lần: lần hỏi
/// thứ hai trong 5′ dùng lại số vừa đo (luật 5′ của `/accounts`, Hà 24/09).
#[test]
#[cfg(unix)]
fn chi_tiet_do_lai_ca_tai_khoan_dang_khoa() {
    let d = tempfile::tempdir().unwrap();
    let acc3 = d.path().join("acc3");
    so(&acc3, SO_DAY_DU);
    let db = huba::db::Db::open(&d.path().join("huba.sqlite")).unwrap();
    let mut c = cfg_voi(Some(&acc3), true);
    c.claude_cli = claude_gia(d.path()).display().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let dead = std::collections::BTreeMap::new();

    let t = huba::runtime::account_detail_say(
        &c,
        &db,
        &SessionsSnapshot::default(),
        "acc3",
        &dead,
        now,
    );
    assert_eq!(so_lan_goi(&acc3), 1, "tài khoản khoá phải được đo lại: {t}");
    assert!(t.contains("🔒 ĐANG KHOÁ"), "{t}");
    assert!(t.contains("email: trogiup.gdk@gmail.com"), "{t}");
    assert!(
        t.contains("đo lại /usage (vừa đo): tuần 42% · 5 tiếng 3%"),
        "số vừa đo phải tới màn: {t}"
    );

    let t2 = huba::runtime::account_detail_say(
        &c,
        &db,
        &SessionsSnapshot::default(),
        "acc3",
        &dead,
        now,
    );
    assert_eq!(so_lan_goi(&acc3), 1, "số còn mới (<5′) mà vẫn đo lại: {t2}");
    assert!(t2.contains("tuần 42%"), "{t2}");

    // Vế ngược: đường TỰ chọn tài khoản vẫn không dò tài khoản khoá.
    let mut c_cu = c.clone();
    c_cu.claude_accounts.retain(|a| a.name == "acc3");
    let _ = huba::runtime::usage_lam_moi(
        &c_cu,
        &huba::db::Db::open(&d.path().join("khac.sqlite")).unwrap(),
        0,
    );
    assert_eq!(
        so_lan_goi(&acc3),
        1,
        "usage_lam_moi (tự chọn) đã dò tài khoản khoá"
    );
}

#[test]
fn dong_do_lai_chi_noi_khi_moi_hon_so_cli_va_noi_ra_khi_hong() {
    use huba::runtime::do_lai_line;
    let now = 1_790_000_000_000;
    let row = serde_json::json!({ "week_pct": 42, "session_pct": 3,
        "week_models": [{ "name": "Fable", "pct": 7 }], "at_ms": now - 3 * 60_000 });
    let l = do_lai_line(Some(&row), Some(now - 3_600_000), now).unwrap();
    assert!(
        l.contains("(3 phút trước): tuần 42% · 5 tiếng 3% · Fable 7%"),
        "{l}"
    );
    // Sổ CLI mới hơn lượt dò ⟹ dòng `hạn mức:` đã là số mới nhất ⟹ im.
    assert_eq!(do_lai_line(Some(&row), Some(now - 60_000), now), None);
    assert_eq!(do_lai_line(None, None, now), None);
    // Lượt dò gần nhất hỏng ⟹ nói ra, kể cả khi còn số cũ.
    let hong =
        serde_json::json!({ "err": "hết giờ 60 s", "week_pct": 40, "at_ms": now - 7_200_000 });
    let l = do_lai_line(Some(&hong), None, now).unwrap();
    assert!(l.contains("lượt gần nhất HỎNG — hết giờ 60 s"), "{l}");
    assert!(l.contains("(2 tiếng trước): tuần 40%"), "{l}");

    // Đo 10/10: CLI chạy xong mà KHÔNG có dòng % nào (acc1 · acc2 · acc3 khoá) ⟹
    // nói thẳng là không có số, KHÔNG đưa câu thô của CLI ra như thể đó là số đo.
    let tho = serde_json::json!({ "raw": "You are currently using your subscription to power your Claude Code usage ·  · What's contributing to your limits usage?", "at_ms": now });
    let l = do_lai_line(Some(&tho), None, now).unwrap();
    assert!(
        l.contains("(vừa đo): CLI chạy xong nhưng KHÔNG trả số % nào"),
        "{l}"
    );
    assert!(l.contains("số mới nhất vẫn là dòng hạn mức ở trên"), "{l}");
    assert!(
        !l.contains("What's contributing"),
        "câu thô lọt ra màn: {l}"
    );

    // Đo 11/10: tài khoản CHƯA đăng nhập ⟹ `/usage` trả bảng `/cost`. Nói ra là chưa
    // đăng nhập, chỉ đường -l, và KHÔNG một dấu `$` nào lên màn (luật 9).
    let cost = serde_json::json!({ "raw": "Total cost:            $0.0000 · Total duration (API):  0s · Total duration (wall): 0s", "at_ms": now });
    let l = do_lai_line(Some(&cost), None, now).unwrap();
    assert!(
        l.contains("KHÔNG thấy tài khoản này đăng nhập") && l.contains("cờ -l"),
        "{l}"
    );
    assert!(!l.contains('$'), "tiền lên màn: {l}");
    // Câu thô lạ có `$` (không phải bảng /cost) cũng không được trích ra.
    let la = serde_json::json!({ "raw": "Spent $3 this week", "at_ms": now });
    let l = do_lai_line(Some(&la), None, now).unwrap();
    assert!(!l.contains('$'), "tiền lên màn: {l}");
}

// ── `/accounts <tên> -l` — đăng nhập lại (Hà 2026-10-11) ─────────────────────

/// Màn THẬT của `claude auth login` (CLI 2.1.280, cửa sổ Terminal, 11/10) — đường
/// dẫn rút gọn phần `scope`/`code_challenge`/`state`, giữ đúng hình dạng.
const MAN_DANG_NHAP: &str = "Last login: Sun Oct 11 07:09:51 on ttys003\n\
hanguyen@Has-MacBook-Pro ~ % CLAUDE_CONFIG_DIR='/Users/hanguyen/.claude-acc3' 'claude' auth login\n\
Opening browser to sign in…\n\
If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback&scope=user%3Aprofile&code_challenge=XXXX&code_challenge_method=S256&state=YYYY\n\
Paste code here if prompted > \n\n";

/// Cùng cửa sổ sau khi gõ một mã SAI — nguyên văn 11/10.
const MAN_MA_SAI: &str =
    "hanguyen@Has-MacBook-Pro ~ % CLAUDE_CONFIG_DIR='/tmp/x' 'claude' auth login\n\
Opening browser to sign in…\n\
If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?code=true&state=YYYY\n\
Paste code here if prompted > Login failed: Request failed with status code 400\n\
hanguyen@Has-MacBook-Pro scratchpad % \n";

#[test]
fn dang_nhap_doc_dung_duong_dan_trang_hien_ma_va_o_cho_ma() {
    use huba::account_login::{after_prompt, login_url, waiting_for_code};
    let url = login_url(MAN_DANG_NHAP).expect("màn thật phải có đường dẫn");
    assert!(
        url.starts_with("https://claude.com/cai/oauth/authorize?"),
        "{url}"
    );
    assert!(
        url.contains("platform.claude.com%2Foauth%2Fcode%2Fcallback")
            && url.ends_with("state=YYYY"),
        "phải là đường TRANG HIỆN MÃ, lấy trọn tới hết dòng: {url}"
    );
    assert!(waiting_for_code(MAN_DANG_NHAP));
    // Vế ngược: CLI đã thoát sau mã sai ⟹ KHÔNG còn chờ, và lời của nó đọc ra được,
    // không lẫn dấu nhắc shell.
    assert!(!waiting_for_code(MAN_MA_SAI));
    assert_eq!(
        after_prompt(MAN_MA_SAI),
        "Login failed: Request failed with status code 400"
    );
    assert_eq!(after_prompt(MAN_DANG_NHAP), "");
    assert_eq!(login_url("không có gì"), None);
    // Chốt cửa sổ: màn của huba thì nhận; màn của một phiên khác (id cửa sổ dùng lại) thì KHÔNG.
    assert!(huba::account_login::la_man_dang_nhap(MAN_DANG_NHAP));
    assert!(huba::account_login::la_man_dang_nhap(MAN_MA_SAI));
    assert!(!huba::account_login::la_man_dang_nhap(
        "❯ \n⏵⏵ auto mode on · visit: https://x"
    ));
    assert_eq!(
        login_url("visit: http://localhost:61422/callback"),
        None,
        "chỉ nhận https"
    );
}

#[test]
fn dang_nhap_dung_lenh_dung_tai_khoan() {
    use huba::account_login::login_command;
    let d = tempfile::tempdir().unwrap();
    let c = cfg_voi(Some(d.path()), true);
    let acc3 = c.claude_accounts[1].clone();
    let l = login_command(&c, &acc3, Some("a'b@x.com"));
    assert!(
        l.starts_with(&format!("CLAUDE_CONFIG_DIR='{}' ", d.path().display())),
        "{l}"
    );
    assert!(
        l.contains(" auth login --email 'a'\\''b@x.com'"),
        "email phải được bọc: {l}"
    );
    // Tài khoản mặc định = biến VẮNG MẶT, kể cả khi hồ sơ shell có đặt sẵn.
    let acc1 = c.claude_accounts[0].clone();
    let l1 = login_command(&c, &acc1, None);
    assert!(l1.starts_with("env -u CLAUDE_CONFIG_DIR "), "{l1}");
    assert!(l1.ends_with(" auth login"), "{l1}");
}

#[test]
fn dang_nhap_cu_phap_va_hinh_dang_ma() {
    use huba::account_login::{code_shape_ok, guide};
    use huba::pipeline::AccountsOrder::*;
    assert_eq!(accounts_order("acc3 -l"), Login("acc3".into(), None));
    assert_eq!(accounts_order("-l acc3"), Login("acc3".into(), None));
    assert_eq!(
        accounts_order("acc3 -l Ab12Cd34#Ef56"),
        Login("acc3".into(), Some("Ab12Cd34#Ef56".into()))
    );
    assert_eq!(accounts_order("-l"), Usage, "thiếu tên");
    // Nhiều chữ sau -l vẫn là MỘT lượt gửi mã — `code_shape_why` nói đúng chỗ sai.
    assert_eq!(
        accounts_order("acc3 -l ma thua"),
        Login("acc3".into(), Some("ma thua".into()))
    );

    assert!(code_shape_ok("Ab12Cd34#Ef56"));
    assert!(!code_shape_ok("ngan"));
    assert!(
        !code_shape_ok("hai tu ghep"),
        "câu chữ lẫn vào không phải mã"
    );
    // Đo 11/10: thiếu `#` ⟹ CLI từ chối «Invalid code … full code» — chặn TRƯỚC khi gõ.
    assert!(
        !code_shape_ok("abchdhxyz123"),
        "mã thiếu # không được đem gõ"
    );
    let why = huba::account_login::code_shape_why(
        "acc3",
        "https://claude.com/cai/oauth/authorize?code=true",
    )
    .unwrap();
    assert!(why.contains("ĐƯỜNG DẪN, không phải mã"), "{why}");
    let why = huba::account_login::code_shape_why("acc3", "abchdhxyz123").unwrap();
    assert!(
        why.contains("CÓ dấu #") && why.contains("/accounts acc3 -l <mã>"),
        "{why}"
    );
    assert_eq!(
        huba::account_login::code_shape_why("acc3", "Ab12Cd34#Ef56"),
        None
    );

    let g = guide(
        "acc3",
        Some("x@y.com"),
        "https://claude.com/cai/oauth/authorize?a=1",
        42,
        false,
    );
    assert!(
        g.contains("https://claude.com/cai/oauth/authorize?a=1"),
        "{g}"
    );
    assert!(
        g.contains("ĐÚNG x@y.com") && g.contains("/accounts acc3 -l <mã>"),
        "{g}"
    );
    let g = guide("acc3", None, "https://x", 42, true);
    assert!(
        g.contains("ĐÚNG tài khoản acc3") && g.contains("nhắc lại"),
        "{g}"
    );
}

/// Màn THẬT của lượt Hà 11/10 01:05Z sau khi huba (bản đầu) gõ thêm `exit`: CLI không
/// in lại dấu nhắc, mỗi mã hỏng thêm một câu `Invalid code` — và vẫn CHẠY.
const MAN_INVALID_HAI_LAN: &str = "hanguyen@Has-MacBook-Pro ~ % CLAUDE_CONFIG_DIR='/Users/hanguyen/.claude-acc3' 'claude' auth login --email 'x@y.com'\n\
Opening browser to sign in…\n\
If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?code=true&state=YYYY\n\
Paste code here if prompted > Invalid code. Please make sure the full code was copied.\n\
Invalid code. Please make sure the full code was copied.\n";

#[test]
fn dang_nhap_cham_ket_cuc_bang_tien_trinh_khong_bang_dong_cuoi() {
    use huba::account_login::{ket_cuc, login_pid, new_after, waiting_for_code, Ket};
    // Dòng cuối KHÔNG phải dấu nhắc mà CLI vẫn chờ — đúng chỗ bản đầu đọc sai.
    assert!(!waiting_for_code(MAN_INVALID_HAI_LAN));
    // Chữ MỚI sau lượt gõ: chỉ câu thứ hai, không lặp câu cũ.
    let truoc = MAN_INVALID_HAI_LAN.replacen(
        "\nInvalid code. Please make sure the full code was copied.\n",
        "\n",
        1,
    );
    assert_ne!(truoc, MAN_INVALID_HAI_LAN, "mẫu phải đổi được");
    assert_eq!(
        new_after(&truoc, MAN_INVALID_HAI_LAN),
        "Invalid code. Please make sure the full code was copied."
    );
    assert_eq!(new_after(MAN_DANG_NHAP, MAN_DANG_NHAP), "");

    let invalid = "Invalid code. Please make sure the full code was copied.";
    assert_eq!(
        ket_cuc(invalid, true),
        Ket::ChuaNhan(invalid.into()),
        "còn chạy ⟹ CHƯA nhận, không đóng"
    );
    let failed = "Login failed: Request failed with status code 400";
    assert_eq!(ket_cuc(failed, false), Ket::Hong(failed.into()));
    assert_eq!(ket_cuc(invalid, false), Ket::Hong(invalid.into()));
    assert_eq!(ket_cuc("", true), Ket::ChuaNoi);
    assert_eq!(
        ket_cuc("Login successful.", false),
        Ket::Xong("Login successful.".into())
    );

    // `ps -t ttys014 -o pid=,command=` đo thật 11/10.
    let ps = "58110 login -pf hanguyen\n58148 -zsh\n58259 claude auth login --email x@y.com\n";
    assert_eq!(login_pid(ps), Some(58259));
    assert_eq!(
        login_pid("58110 login -pf hanguyen\n58148 -zsh\n"),
        None,
        "chỉ còn shell ⟹ CLI đã thoát"
    );
}
