//! Bản bàn giao từ nhật ký phải in ĐÚNG cây làm việc của phiên — không bao giờ
//! một cây nằm NGOÀI dự án.
//!
//! 🔴 Đo 2026-10-01 trên các phiên kế nhiệm do hạn mức: 29 bản bàn giao có mục
//! "Cây làm việc THẬT", **16 sai** — cả 16 đều là phiên dwork, đều in cây của
//! repo workspace `~/projects` (`M ../.claude/settings.json`). `dwork` không
//! phải một cây git riêng; mỗi làn bên trong nó mới là một cây, nên
//! `git status` chạy ở `~/projects/dwork` leo ngược lên tận gốc workspace.

use std::path::{Path, PathBuf};
use std::process::Command;

fn git_init(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let ok = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git init hỏng ở {}", dir.display());
}

fn fresh_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "huba-cay-lan-{tag}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// Dựng đúng hình dạng đã đo: workspace là một repo, dự án KHÔNG phải repo
/// riêng, làn bên trong mới là repo.
fn workspace_with_lanes(tag: &str) -> (PathBuf, PathBuf) {
    let ws = fresh_root(tag);
    git_init(&ws);
    std::fs::write(ws.join("cua-workspace.txt"), "x").unwrap();
    let proj = ws.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("CLAUDE.md"), "x").unwrap();
    git_init(&proj.join("lane-a"));
    std::fs::write(proj.join("lane-a/cua-lan-a.rs"), "x").unwrap();
    git_init(&proj.join("lane-b"));
    std::fs::write(proj.join("lane-b/cua-lan-b.rs"), "x").unwrap();
    (ws, proj)
}

#[test]
fn a_project_that_is_not_its_own_tree_never_reports_the_workspace_tree() {
    let (ws, proj) = workspace_with_lanes("khong-goi-y");
    let out = huba::sessions::working_tree_summary_at(&proj, "");
    assert!(
        !out.as_deref().unwrap_or("").contains("cua-workspace.txt"),
        "in cây của workspace thay cho cây của dự án: {out:?}"
    );
    let _ = std::fs::remove_dir_all(ws);
}

#[test]
fn the_lane_the_session_worked_in_is_the_tree_reported() {
    let (ws, proj) = workspace_with_lanes("goi-y");
    let p = proj.display().to_string();
    let hint = format!(
        r#"{{"file_path":"{p}/lane-a/src/x.rs"}} {{"command":"cd {p}/lane-a && cargo test"}}
{{"file_path":"{p}/lane-a/README.md"}} {{"command":"git -C {p}/lane-b log -1"}}"#
    );
    let out = huba::sessions::working_tree_summary_at(&proj, &hint)
        .expect("phiên làm ở lane-a, cây ấy có tệp chưa commit — phải có mục cây");
    assert!(
        out.contains("cua-lan-a.rs"),
        "không in cây của làn phiên làm việc: {out}"
    );
    assert!(
        !out.contains("cua-lan-b.rs"),
        "in nhầm làn ít được chạm tới hơn: {out}"
    );
    assert!(
        !out.contains("cua-workspace.txt"),
        "in cây workspace: {out}"
    );
    assert!(out.contains("lane-a"), "không nói cây nào được đọc: {out}");
    let _ = std::fs::remove_dir_all(ws);
}

/// Dự án LÀ một cây (huba, tfl5…) thì giữ nguyên hành vi cũ.
#[test]
fn a_project_that_is_its_own_tree_is_read_as_before() {
    let ws = fresh_root("cay-rieng");
    git_init(&ws);
    std::fs::write(ws.join("cua-workspace.txt"), "x").unwrap();
    let proj = ws.join("proj");
    git_init(&proj);
    std::fs::write(proj.join("cua-du-an.rs"), "x").unwrap();
    let out = huba::sessions::working_tree_summary_at(&proj, "").expect("có tệp chưa commit");
    assert!(out.contains("cua-du-an.rs"), "{out}");
    assert!(!out.contains("cua-workspace.txt"), "{out}");
    let _ = std::fs::remove_dir_all(ws);
}
