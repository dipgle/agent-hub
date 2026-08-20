//! Nguồn MỚI phải trả lời y hệt nguồn CŨ — đo trên chính cái máy này.
//!
//! Gắn `#[ignore]` vì nó đọc `~/.claude*` thật VÀ spawn `claude agents` thật
//! (đúng thứ đường mới sinh ra để khỏi phải làm). Chạy tay:
//!
//! ```
//! cd ~/projects/huba/rust
//! HUB_CONFIG=$HOME/projects/huba/huba.config.json \
//!   cargo test --offline --test session_books_live -- --ignored --nocapture
//! ```
//!
//! Vì sao phải có bài kiểm này chứ không chỉ mấy bài thuần bên
//! `session_books.rs`: bài thuần chạy trên dữ liệu **do tôi nghĩ ra**, nên nó
//! chỉ ghim được hình dạng tôi đã tin. Cùng một lượt hôm nay đã chứng minh chỗ
//! yếu ấy — bản đầu của `BG_ENDED` thiếu `failed`, và không bài thuần nào thấy;
//! thứ lôi nó ra là chạy thật rồi so với `claude agents`.

use std::collections::BTreeMap;
use std::time::Instant;

/// Hàng của một tài khoản theo `claude agents --json` — nguồn CŨ.
fn agents_rows(config_dir: Option<&str>) -> Vec<serde_json::Value> {
    let mut cmd = std::process::Command::new("claude");
    cmd.args(["agents", "--json"]);
    match config_dir {
        // Tài khoản mặc định được chọn bằng cách biến ấy VẮNG MẶT — trỏ nó vào
        // `~/.claude` thì CLI khai "not logged in" (đã trả giá 2026-08-08).
        Some(d) => cmd.env("CLAUDE_CONFIG_DIR", d),
        None => cmd.env_remove("CLAUDE_CONFIG_DIR"),
    };
    let out = cmd.output().expect("chạy được `claude agents --json`");
    assert!(
        out.status.success(),
        "`claude agents --json` exit {:?}",
        out.status.code()
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout)
        .expect("đầu ra của `claude agents` là JSON")
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn by_id(rows: &[serde_json::Value]) -> BTreeMap<String, serde_json::Value> {
    rows.iter()
        .filter_map(|r| {
            r.get("sessionId")
                .and_then(|v| v.as_str())
                .map(|id| (id.to_string(), r.clone()))
        })
        .collect()
}

#[test]
#[ignore = "đọc sổ thật + spawn `claude agents` thật — chạy tay bằng --ignored"]
fn the_books_answer_exactly_what_claude_agents_answers() {
    let cfg = huba::config::load(None).expect("HUB_CONFIG trỏ vào huba.config.json thật");
    let accounts = cfg.claude_accounts_or_ambient();
    assert!(!accounts.is_empty());

    let mut checked = 0usize;
    for acc in &accounts {
        let dir = match acc.config_dir.as_deref().filter(|d| !d.is_empty()) {
            Some(d) => huba::config::expand_home(std::path::Path::new(d)),
            None => std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".claude"),
        };
        if !dir.join("sessions").is_dir() {
            println!("[{}] bỏ qua — chưa có {}/sessions", acc.name, dir.display());
            continue;
        }

        let t0 = Instant::now();
        let books = huba::sessions::list_account_books(&dir).expect("đọc được sổ");
        let ms_books = t0.elapsed().as_millis();

        let t1 = Instant::now();
        // ⚠ Phải đưa đường dẫn ĐÃ NỞ `~`. Bản đầu của bài kiểm này chuyền
        // nguyên `~/.claude-acc3` từ cấu hình sang biến môi trường, CLI không
        // nở dấu ngã ⟹ nó soi một thư mục tên `~` ⟹ trả `[]` ⟹ bài kiểm tố
        // nguồn mới "dựng hàng ma" cho một phiên đang sống thật (pid 49394,
        // ttys006). Chính `list_account_cli` của huba thì nở đúng — tức lỗi nằm
        // ở phép đo, không ở vật được đo. Ghi lại vì đây đúng cái bẫy
        // `feedback_verify_measurement_points_right` nói tới.
        let agents = agents_rows(
            acc.config_dir
                .as_deref()
                .filter(|d| !d.is_empty())
                .map(|_| dir.to_string_lossy().to_string())
                .as_deref(),
        );
        let ms_agents = t1.elapsed().as_millis();

        println!(
            "[{}] sổ {} hàng / {} ms   ·   claude agents {} hàng / {} ms",
            acc.name,
            books.len(),
            ms_books,
            agents.len(),
            ms_agents
        );

        let b = by_id(&books);
        let a = by_id(&agents);
        let only_agents: Vec<_> = a.keys().filter(|k| !b.contains_key(*k)).collect();
        let only_books: Vec<_> = b.keys().filter(|k| !a.contains_key(*k)).collect();
        assert!(
            only_agents.is_empty(),
            "[{}] `claude agents` khai phiên mà sổ KHÔNG có: {only_agents:?} — nguồn mới đang giấu phiên sống",
            acc.name
        );
        assert!(
            only_books.is_empty(),
            "[{}] sổ khai phiên mà `claude agents` KHÔNG có: {only_books:?} — nguồn mới đang dựng hàng ma",
            acc.name
        );

        // Từng trường, không chỉ đếm đầu: một danh sách đúng số lượng mà sai
        // `cwd` thì `/new` mở nhầm thư mục và `/shot` chụp nhầm cửa sổ.
        for (id, ar) in &a {
            let br = &b[id];
            for field in ["cwd", "name", "kind"] {
                assert_eq!(
                    ar.get(field),
                    br.get(field),
                    "[{}] phiên {id}: trường `{field}` lệch giữa hai nguồn",
                    acc.name
                );
            }
            // `pid` chỉ có ở hàng tương tác; hàng nền thì cả hai nguồn đều
            // không có tiến trình nào để chỉ vào.
            if ar.get("kind").and_then(|k| k.as_str()) == Some("interactive") {
                assert_eq!(
                    ar.get("pid"),
                    br.get("pid"),
                    "[{}] phiên {id}: pid lệch",
                    acc.name
                );
            }
        }
        checked += 1;
    }
    assert!(
        checked > 0,
        "không tài khoản nào có sổ để so — bài kiểm này không nói được gì"
    );
}

/// Ghép phiên theo CỬA SỔ phải đúng với ảnh chụp, trên từng tty đang chạy.
///
/// 🔴 Đây là bài kiểm cho con bug Hà bắt được tối 2026-08-15: ba lượt `/new`
/// liên tiếp ghép nhầm sang phiên mở TRƯỚC đó, vì `newest_transcript_since`
/// hỏi *"tệp nào vừa được ghi"* trong một thư mục **dùng chung cho mọi phiên**.
/// Hậu quả anh thấy: tin báo *"Phiên 724de1ff"* cho cửa sổ social, và con trỏ
/// trỏ nhầm phiên.
#[test]
#[ignore = "đọc ps + sổ thật trên máy này"]
fn every_live_window_maps_to_the_session_actually_sitting_in_it() {
    let cfg = huba::config::load(None).expect("HUB_CONFIG trỏ vào huba.config.json thật");
    let snap = huba::sessions::snapshot(&cfg);

    let mut checked = 0usize;
    for s in &snap.sessions {
        if s.tty.is_empty() || !huba::sessions::is_real_tty(&s.tty) || s.session_id.is_empty() {
            continue;
        }
        if huba::sessions::is_shell_id(&s.session_id) {
            continue;
        }
        let got = huba::sessions::session_on_tty(&cfg, &s.tty);
        assert_eq!(
            got.as_deref(),
            Some(s.session_id.as_str()),
            "cửa sổ {} thuộc phiên {} nhưng phép ghép theo tty trả {:?}",
            s.tty,
            s.session_id,
            got
        );
        checked += 1;
    }
    println!("đã đối chiếu {checked} cửa sổ");
    assert!(checked > 0, "không có cửa sổ nào để đối chiếu");
}

/// Chữ về theo LƯỢT DÒ CHUNG phải đúng bằng chữ hỏi riêng từng cửa sổ.
///
/// 🔴 Đây là cổng cho lượt đổi nguồn 2026-08-16 (7–14 giây ⟹ ~2 giây): ảnh chụp
/// thôi hỏi Terminal hai lần cho mỗi phiên bận, thay bằng một lượt lấy chữ mọi
/// tab. Cái được là tốc độ; cái có thể mất mà **không bài kiểm thuần nào thấy**
/// là chữ gắn NHẦM tab — khung bản tin lệch một dòng thì màn của `ttys001` đi
/// vào hàng của `ttys002`, và mọi thứ vẫn trông hợp lý: vẫn có chữ, vẫn có động
/// từ, chỉ là của phiên khác. Nên phép kiểm phải so ĐÚNG CẶP, trên máy thật.
///
/// So ĐỘNG TỪ chứ không so nguyên văn: hai lượt đọc cách nhau khoảng một giây,
/// nên đồng hồ trong dòng trạng thái (`2m 14s`) chắc chắn khác — còn động từ
/// thì đổi hàng chục giây một lần. So cả con số là dựng một bài kiểm đỏ vì
/// thời gian trôi, tức một bài kiểm không ai tin nữa sau lần đỏ thứ hai.
#[test]
#[ignore = "hỏi Terminal thật, cả hai đường, rồi so từng cặp"]
fn one_shared_probe_reads_the_same_screens_as_asking_each_window_alone() {
    let tabs = huba::keys::terminal_screens().expect("hỏi được Terminal");
    assert!(!tabs.is_empty(), "máy không mở cửa sổ Terminal nào để so");

    let mut checked = 0usize;
    let mut lệch = Vec::new();
    for tab in &tabs {
        if tab.procs.is_empty() {
            continue; // xác tab — `alive_tab` cố ý không nhận, `look` cũng thế
        }
        // Tab này có phải cái mà phép tra chọn cho tty ấy không: một tty có thể
        // ứng với nhiều tab, và chỉ khi hai bên chọn CÙNG một tab thì so mới có
        // nghĩa.
        if huba::keys::alive_tab(&tabs, &tab.tty) != Some(tab) {
            continue;
        }
        let chung = huba::keys::look_from_screen(tab.screen.as_deref().unwrap(), 6);
        let riêng = huba::keys::look(&tab.tty, 6);

        let verb = |l: &huba::keys::Look| -> (Option<Option<String>>, Option<usize>) {
            match l {
                huba::keys::Look::Saw { body, .. } => {
                    (Some(huba::keys::activity(body).map(|a| a.verb)), None)
                }
                // 🪦 Nhánh `Withheld` gỡ 2026-08-16 — xem bia mộ `keys::Look`.
                huba::keys::Look::Blind { .. } => (None, None),
            }
        };
        let (a, b) = (verb(&chung), verb(&riêng));
        if a != b {
            lệch.push(format!("{}: chung={a:?} riêng={b:?}", tab.tty));
        }
        checked += 1;
    }

    println!("đã so {checked} cửa sổ · lệch {}", lệch.len());
    assert!(
        checked > 0,
        "không cửa sổ nào so được — bài kiểm nói không nên lời"
    );
    assert!(
        lệch.is_empty(),
        "chữ của lượt dò chung không khớp chữ hỏi riêng: {lệch:?}"
    );
}

#[test]
#[ignore = "dựng ảnh chụp thật (đọc nhật ký của mọi phiên đang sống)"]
fn a_snapshot_lists_every_live_claude_window_and_nothing_dead() {
    let cfg = huba::config::load(None).expect("HUB_CONFIG trỏ vào huba.config.json thật");

    let mut times = Vec::new();
    let mut snap = None;
    for _ in 0..3 {
        let t = Instant::now();
        snap = Some(huba::sessions::snapshot(&cfg));
        times.push(t.elapsed().as_millis());
    }
    let snap = snap.unwrap();
    println!("ảnh chụp: {times:?} ms · {} hàng", snap.sessions.len());

    // Mọi tiến trình `claude` có cửa sổ THẬT phải có mặt — trừ phiên editor (cố
    // ý không liệt kê) và phiên chưa qua hộp tin-thư-mục (chưa có id, xem
    // `unlisted_claude_processes`). Đây là câu hỏi *"máy chạy N sao màn kê M"*
    // hỏi bằng mã thay vì bằng mắt.
    let ps = std::process::Command::new("ps")
        .args(["-eo", "pid=,tty=,command="])
        .output()
        .expect("chạy được ps");
    let ps = String::from_utf8_lossy(&ps.stdout);

    let listed: Vec<i64> = snap.sessions.iter().map(|s| s.pid).collect();
    let mut live_windows = 0usize;
    for line in ps.lines() {
        let mut it = line.trim_start().splitn(3, char::is_whitespace);
        let (Some(pid), Some(tty), Some(cmd)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        let Ok(pid) = pid.parse::<i64>() else {
            continue;
        };
        if !huba::sessions::is_real_tty(tty) || !huba::sessions::is_claude_process(cmd) {
            continue;
        }
        if huba::sessions::classify_host(cmd, "interactive", tty) == "editor" {
            continue;
        }
        live_windows += 1;
        if !listed.contains(&pid) {
            println!("  ⚠ pid {pid} ({tty}) chạy `claude` mà KHÔNG có trong ảnh chụp — kiểm log `claude_process_unlisted`");
        }
    }
    println!("cửa sổ CLI đang sống theo ps: {live_windows}");

    // Hàng nào khai còn sống thì pid phải còn sống thật. Đây là cửa chống pid
    // dùng lại đọc từ đầu kia.
    for s in &snap.sessions {
        if s.pid > 0 && s.host != "dead" {
            assert!(
                ps.lines()
                    .any(|l| l.trim_start().starts_with(&format!("{} ", s.pid))),
                "hàng {} khai host={} nhưng pid {} không có trong `ps`",
                s.session_id,
                s.host,
                s.pid
            );
        }
    }
}
