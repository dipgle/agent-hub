//! Outbound leak scan — the last gate before text leaves this machine.
//!
//! WHY THIS EXISTS (observed, not theoretical): on 2026-07-26 a real triage run
//! produced a reply draft whose evidence lines quoted the workspace auto-memory
//! ("memory: tfl5 security hardening 2026-07 — CÒN OPEN: PG SPOF, …"). The
//! triage subprocess has no tools, but `claude -p` still loads the workspace's
//! memory and instruction files into its context. Fine for a brief the owner
//! reads; not fine in a reply to an outside sender.
//!
//! A hit does not rewrite the text — it downgrades the item to human review,
//! because silently truncating a reply is its own kind of failure.

use std::sync::OnceLock;

use regex::Regex;

const PATTERNS: [(&str, &str); 10] = [
    (r"(?i)\bvps-[ab]\b", "internal_host"),
    (r"\b(?:\d{1,3}\.){3}\d{1,3}\b", "ip_address"),
    (r"(?i)/Users/[a-z0-9._-]+/", "local_filesystem_path"),
    (
        r"(?i)\b(?:memory|MEMORY\.md|active-context\.md|CLAUDE\.md)\s*:",
        "internal_notes_citation",
    ),
    (r"(?i)\[\[[a-z0-9-]+\]\]", "memory_wikilink"),
    (
        r"(?i)\b(api[_-]?key|bearer|password|secret|access[_-]?token|private[_-]?key)\b",
        "credential_word",
    ),
    // The same words in the language this workspace actually speaks. Without
    // this the gate is decorative: on 2026-08-08 a session transcript said
    // "Mật khẩu là <value>" in plain text and every English pattern passed it
    // through. A leak scan that only reads English on a Vietnamese machine
    // reports zero and means nothing.
    (
        r"(?i)m[aậ]t\s*kh[aẩ]u|m[aã]\s*b[ií]\s*m[aậ]t|kho[áa]\s*(b[ií]\s*m[aậ]t|ri[êe]ng)|th[oô]ng\s*tin\s*[đd][aă]ng\s*nh[aậ]p",
        "credential_word_vi",
    ),
    (
        r"\b(sk-[A-Za-z0-9]{10,}|gh[pousr]_[A-Za-z0-9]{20,}|\d{8,10}:[A-Za-z0-9_-]{30,})\b",
        "credential_literal",
    ),
    (r"-----BEGIN [A-Z ]*PRIVATE KEY-----", "private_key_block"),
    (
        r"(?i)\bSPOF\b|\bchưa fix\b|\bblocker\b",
        "internal_risk_language",
    ),
];

/// Cổng riêng cho MỘT TỆP chủ máy tự tay xin — hẹp hơn, và có lý do.
///
/// 🔴 Hà 2026-08-13, ngay lần bấm nút 📎 đầu tiên: *"chưa gửi được
/// huba.env.example — giữ lại: có dấu hiệu bí mật (credential_word_vi)"*. Tệp
/// ấy là **bản mẫu**, mọi giá trị đều rỗng; thứ khớp chỉ là chữ *"Mật khẩu"*
/// trong nhãn ô nhập.
///
/// Nhìn kỹ thì không phải một luật lỡ tay, mà là **dùng sai cái cân**. Bộ
/// `PATTERNS` sinh ra cho *"a reply to an outside sender"* (dòng đầu tệp này),
/// rồi được mượn sang phần xem trước của phiên. Đem nguyên nó áp cho một tệp
/// thì gần như mọi tài liệu trong repo đều bị chặn: `/Users/…` dính
/// `local_filesystem_path`, `CLAUDE.md:` dính `internal_notes_citation`,
/// `[[abc]]` dính `memory_wikilink`, và bất kỳ trang nào nhắc tới mật khẩu
/// dính `credential_word`. Một cái cổng chặn hết thì không ai dùng cửa ấy nữa.
///
/// Chỗ khác nhau nằm ở NGƯỜI NHẬN và ở AI CHỌN:
/// * Phần xem trước là **mảnh chữ huba tự chọn gửi** — không ai nhìn trước, nên
///   phải ngờ cả những chữ chỉ *gợi ý* có bí mật.
/// * Tệp này là thứ **chủ máy gọi đích danh**, gửi vào **phòng chat của chính
///   anh** (gác bằng `chat_id`). Đường dẫn máy anh, ghi chú của anh, tên host
///   của anh — anh đã có sẵn. Cái duy nhất thật sự nguy là một **GIÁ TRỊ** bí
///   mật lọt vào một tệp anh tưởng là vô hại.
///
/// Nên ở đây chỉ chặn giá trị, ba hình dạng:
/// * `credential_literal` — token có hình dạng nhận ra được.
/// * `private_key_block` — khối khoá riêng.
/// * `secret_assignment` — `MẬT_KHẨU = <có gì đó>`; **rỗng thì không tính**,
///   và đó đúng là chỗ `huba.env.example` khác `huba.env`.
pub fn file_risk(text: &str) -> Vec<String> {
    let mut hits: Vec<String> = leak_scan(text, &[])
        .into_iter()
        .filter(|c| c == "credential_literal" || c == "private_key_block")
        .collect();
    if secret_assignment().is_match(text) {
        hits.push("secret_assignment".to_string());
    }
    hits.sort();
    hits.dedup();
    hits
}

/// `PASSWORD=abc` — khoá nghe như bí mật, và **có giá trị đứng sau**.
///
/// Vế "có giá trị" là cả luật: `HUB_TFL5_PASSWORD=` trong bản mẫu thì không
/// khớp, còn `HUB_TFL5_PASSWORD=abc` trong tệp thật thì khớp. Không có vế ấy
/// thì mọi bản mẫu `.env` trên đời đều bị chặn, mà bản mẫu chính là tệp người
/// mới cần đọc nhất.
fn secret_assignment() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            // 🔴 KHÔNG có `\b`, và đó là cả bài học của lần chạy đầu: bản đầu
            // viết `\bpass(word)?\b` — không khớp `HUB_TFL5_PASSWORD` vì `_`
            // cũng là ký tự từ, nên không có ranh giới nào ở đó. Tức luật bỏ
            // sót đúng hình dạng phổ biến nhất trên đời: **tên biến môi
            // trường**. Ở một cái cổng bảo mật, khớp rộng là chặn nhầm (phiền),
            // khớp hẹp là lọt (mất). Chọn rộng.
            //
            // Hai vế còn lại giữ chặt: giá trị phải nằm CÙNG DÒNG (`[ \t]*`
            // không nuốt được xuống dòng) và phải KHÁC RỖNG — đó đúng là chỗ
            // `huba.env.example` khác `huba.env`.
            //
            // 🔴 Hà 2026-08-13, bấm nút 📎: *"Chưa lấy được file"* — huba trả
            // *"giữ lại: có dấu hiệu bí mật (secret_assignment)"* cho
            // `mailler/scripts/dkim-doctor.sh`. Đo ra đúng một dòng, và nó là
            // một dòng CHÚ THÍCH:
            //
            //     #   The 2026-08-10 DMARC report says spf=pass, dkim=fail for
            //
            // Cầu nối giữa khoá và dấu `=` viết là `[^\n:=]*` — **không có
            // trần**. Nên `pass` (ở đây là một GIÁ TRỊ của `spf=`) bắc qua
            // `, dkim` rồi tóm lấy dấu `=` của một cặp khoá-giá-trị **khác hẳn**
            // ở cuối câu. Cùng một họ với `??` đọc thành cửa sổ: hai thứ chẳng
            // liên quan bị nối bằng một khoảng trống không ai đặt hạn.
            //
            // Vá đúng vế ấy, KHÔNG nới cửa: cầu nối không được chứa `,` hay `;`
            // (chúng kết thúc một mệnh đề — bên kia là chuyện khác), và dài tối
            // đa 24 ký tự, đủ cho `PASSWORD_FOR_PRODUCTION_DB=`. Văn xuôi tiếng
            // Việt vẫn bắt được: *"Mật khẩu đăng nhập: abc"* vẫn khớp, vì đó
            // vẫn là một bí mật bị lộ thật.
            r"(?i)(pass(word)?|secret|token|api[_-]?key|private[_-]?key|credential|m[aậ]t\s*kh[aẩ]u|m[aã]\s*b[ií]\s*m[aậ]t)[^\n:=,;]{0,24}[:=][ \t]*[^\s]",
        )
        .expect("built-in secret-assignment pattern must compile")
    })
}

fn compiled() -> &'static Vec<(Regex, &'static str)> {
    static C: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    C.get_or_init(|| {
        PATTERNS
            .iter()
            .map(|(src, label)| {
                (
                    Regex::new(src).expect("built-in leak pattern must compile"),
                    *label,
                )
            })
            .collect()
    })
}

// `EXTERNAL_CHANNELS` / `is_external_channel` lived here to decide which
// outbound replies had to pass the leak scan. There is no outbound reply path
// any more (2026-08-08), and the one caller left — `sessions::preview_risk` —
// scans unconditionally, because everything it looks at is on its way to a doc
// on a server. "Sometimes" was the part worth deleting.

/// Compile `leak_patterns` from config; a bad pattern is reported, never swallowed.
pub fn compile_extra(list: &[String]) -> Vec<(Regex, String)> {
    let mut out = vec![];
    for src in list {
        match Regex::new(&format!("(?i){src}")) {
            Ok(re) => out.push((re, format!("config:{}", crate::exec::truncate(src, 30)))),
            Err(e) => crate::logging::error(
                "bad_leak_pattern",
                serde_json::json!({ "pattern": src, "err": e.to_string() }),
            ),
        }
    }
    out
}

/// Labels of everything in `text` that must not go out unreviewed.
pub fn leak_scan(text: &str, extra: &[(Regex, String)]) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }
    let mut hits: Vec<String> = vec![];
    for (re, label) in compiled() {
        if re.is_match(text) && !hits.iter().any(|h| h == label) {
            hits.push((*label).to_string());
        }
    }
    for (re, label) in extra {
        if re.is_match(text) && !hits.contains(label) {
            hits.push(label.clone());
        }
    }
    hits
}
