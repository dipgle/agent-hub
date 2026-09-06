//! Nhãn dự án của một phiên phải là thứ phiên TỰ KHAI, không phải thứ đếm được
//! trong đuôi nhật ký của nó.
//!
//! 🔴 Hà 2026-08-18: *"đang làm phiên onghut giờ mất luôn thành 2 phiên tfl5"*.
//! Phiên `08b1a8e8` là onghut; nó vừa làm một quãng đụng vào `AI/tfl5` (chỗ ghim
//! Playwright), và `folder_from_tail` đếm đường dẫn trong cửa sổ 256 KB rồi lấy
//! cái nhiều nhất. Đếm lại đúng cửa sổ ấy tại đúng thời điểm ấy
//! (byte 8508947..8771091 của nhật ký thật, cắt tại `2026-08-18T08:47:47Z`):
//!
//! | nhãn | số lần nhắc |
//! |---|---|
//! | `AI/tfl5` | 43 |
//! | `onghut` | 24 |
//!
//! ⟹ nhãn lật sang `[tfl5]`, danh sách hiện HAI hàng `[tfl5]` (hàng thật
//! `f58e9e12` + hàng onghut bị đổi tên), và chữ "onghut" biến mất khỏi màn — dù
//! phiên vẫn sống và lời cuối của nó vẫn mở đầu bằng `[onghut]`.
//!
//! Fixture dưới đây cắt ra từ CHÍNH cửa sổ ấy (17 bản ghi thật, giữ nguyên tỉ lệ
//! tfl5 > onghut; riêng lượt nói cuối bị cắt ngắn phần chữ cho dễ đọc).

use std::path::Path;

fn fixture() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/onghut-mislabel-2026-08-18.jsonl");
    std::fs::read_to_string(p).expect("fixture nằm cạnh bài kiểm")
}

/// Workspace thật: phép đối chiếu nhãn phải trỏ vào thư mục CÓ THẬT, nên bài
/// kiểm này chỉ chạy được trên máy có `~/projects/onghut` và `~/projects/AI/tfl5`.
fn workspace() -> String {
    let home = std::env::var("HOME").expect("HOME");
    format!("{home}/projects")
}

/// Ca gốc: phép đếm CHỌN SAI, và nó chọn sai một cách hoàn toàn hợp lý.
///
/// Giữ bài kiểm này lại để lần sau ai đọc mã còn biết vì sao phải có tầng khai
/// báo: bản thân phép đếm không hỏng, nó chỉ trả lời một câu hỏi khác — *"phiên
/// này đụng vào thư mục nào nhiều nhất"* — mà câu ấy không phải *"phiên này là
/// dự án gì"*.
#[test]
fn the_path_census_picks_the_wrong_project_here() {
    let got = huba::sessions::folder_from_tail(&fixture(), &workspace());
    assert_eq!(
        got.as_deref(),
        Some("AI/tfl5"),
        "fixture phải giữ nguyên thế mất cân bằng đã gây lỗi"
    );
}

/// Bản vá: lời phiên TỰ KHAI thắng phép đếm.
#[test]
fn the_session_own_prefix_wins_over_the_census() {
    let got = huba::sessions::folder_declared(&fixture(), &workspace());
    assert_eq!(
        got.as_deref(),
        Some("onghut"),
        "phiên tự mở đầu bằng `[onghut]` mà nhãn vẫn đi theo phép đếm"
    );
}

/// Và đường ghép cuối cùng — thứ dòng danh sách thật sự đọc.
#[test]
fn the_label_the_list_shows_is_the_declared_one() {
    let ws = workspace();
    let got = huba::sessions::folder_for_session("test-onghut-08b1a8e8", &fixture(), &ws, || None);
    assert_eq!(got.as_deref(), Some("onghut"));
    assert_eq!(
        huba::sessions::display_name("hanguyen-9c", "onghut"),
        "[onghut]"
    );
}

/// Không được vá quá tay: phiên KHÔNG khai gì thì phép đếm vẫn là câu trả lời.
///
/// Nhật ký của một phiên có thể toàn lượt gọi công cụ hàng giờ liền (`💬 [dùng
/// Bash]`), lúc ấy không có lời nào để đọc — bỏ phép đếm đi là đổi một lỗi lấy
/// một lỗi khác.
#[test]
fn with_no_declaration_the_census_still_answers() {
    let ws = workspace();
    let tail_without_prose: String = fixture()
        .lines()
        .filter(|l| !l.contains("[onghut] Đã viết lại"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        huba::sessions::folder_declared(&tail_without_prose, &ws),
        None,
        "không có lời nào thì đừng bịa ra một lời khai"
    );
    let got =
        huba::sessions::folder_for_session("test-no-prose", &tail_without_prose, &ws, || None);
    assert_eq!(
        got.as_deref(),
        Some("AI/tfl5"),
        "mất phép đếm là mất luôn nhãn"
    );
}

/// Một lượt nói chỉ có tên trong ngoặc thì chưa đủ — phải là thư mục CÓ THẬT.
///
/// Câu văn bàn về một thứ trong ngoặc vuông (`[BUG-02]`, `[hôm qua]`) không phải
/// một lời khai dự án, và đối chiếu với đĩa là cổng rẻ nhất chặn được cả lớp ấy.
#[test]
fn a_bracket_that_is_not_a_project_is_not_a_declaration() {
    let ws = workspace();
    for text in [
        "[BUG-02] đã vá",
        "[dùng Bash]",
        "[không-có-thư-mục-nào-tên-này] xong",
        "[Request interrupted by user]",
    ] {
        assert_eq!(
            huba::sessions::declared_label(text, &ws),
            None,
            "nhận nhầm một lời khai: {text}"
        );
    }
    // Còn tên thật thì nhận, kể cả khi được bọc trong dấu nháy ngược như phiên
    // huba vẫn viết (`[huba]`), và kể cả khi nằm trong ngăn kéo `AI/`.
    assert_eq!(
        huba::sessions::declared_label("`[huba]` Ba việc, gọn từng cái.", &ws).as_deref(),
        Some("huba")
    );
    assert_eq!(
        huba::sessions::declared_label("[tfl5] Con số không sai", &ws).as_deref(),
        Some("AI/tfl5")
    );
}

/// Phép đếm KHÔNG được lật cái nhãn phiên đã tự khai.
///
/// Đây là nửa còn lại của lỗi: lời khai có thể trôi ra khỏi cửa sổ 256 KB sau
/// một quãng dài chỉ gọi công cụ. Lúc ấy phép đếm quay lại một mình — và nếu nó
/// được phép ghi đè cái đã khai thì nhãn lật y như cũ, chỉ chậm hơn.
#[test]
fn the_census_may_not_overwrite_a_declared_label() {
    let ws = workspace();
    let sid = "test-memo-onghut";
    // Lượt đo 1: có lời khai ⟹ nhớ "onghut" kèm hạng "tự khai".
    assert_eq!(
        huba::sessions::folder_for_session(sid, &fixture(), &ws, || None).as_deref(),
        Some("onghut")
    );
    // Lượt đo 2: lời khai đã trôi mất, chỉ còn đường dẫn tfl5 ⟹ GIỮ NGUYÊN.
    let tail_without_prose: String = fixture()
        .lines()
        .filter(|l| !l.contains("[onghut] Đã viết lại"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        huba::sessions::folder_for_session(sid, &tail_without_prose, &ws, || None).as_deref(),
        Some("onghut"),
        "phép đếm đã lật cái nhãn phiên tự khai — đúng lỗi 18/08"
    );
}

/// 🔴 `[dwork/A-DSIGN]` — dự án THẬT kèm một LÀN không phải thư mục.
///
/// Hà 2026-08-22, ảnh chụp một tin có cả hai cái tên trong đúng một khung: huba
/// viết `[dwork]·Quét GitHub làm design`, phiên tự xưng `[dwork/A-DSIGN]` —
/// *"Sao cái tên phiên ở trên không làm giống ở dưới vừa gọn vừa dễ hiểu"*.
///
/// Cửa cũ đòi CẢ chuỗi phải là thư mục có thật, nên lời khai bị loại và
/// `label_sessions` phải tự dựng lại phần phân biệt bằng tên việc cắt ở 34 ký
/// tự. Cửa mới đứng ở phần ĐẦU; phần sau là chuyện nội bộ của dự án ấy.
#[test]
fn a_declared_lane_keeps_the_project_and_carries_the_lane() {
    let ws = workspace();
    let p = std::path::Path::new(&ws);
    // Điều kiện của bài kiểm, KHAI RA thay vì giả định — hai đường này quyết
    // định câu trả lời, nên sai một cái là bài kiểm đo nhầm thứ khác.
    assert!(p.join("dwork").is_dir(), "cần {ws}/dwork là thư mục thật");
    assert!(
        !p.join("dwork/A-DSIGN").is_dir(),
        "ca này chỉ có nghĩa khi làn KHÔNG phải thư mục"
    );

    assert_eq!(
        huba::sessions::declared_parts("[dwork/A-DSIGN] đã đóng", &ws),
        Some(("dwork".to_string(), "A-DSIGN".to_string()))
    );
    // Nhãn DỰ ÁN không đổi một chữ: ô màu băm từ nó, `clean_inbox` ghép đường
    // dẫn từ nó, và cả hai phải tiếp tục trỏ vào thư mục thật.
    assert_eq!(
        huba::sessions::declared_label("[dwork/A-DSIGN] đã đóng", &ws).as_deref(),
        Some("dwork")
    );
    // Không khai làn ⟹ làn rỗng, đường cũ nguyên vẹn.
    assert_eq!(
        huba::sessions::declared_parts("[huba] ok", &ws),
        Some(("huba".to_string(), String::new()))
    );
    // Cửa vẫn đứng: đầu không phải dự án thì từ chối CẢ cụm, không nhận bừa
    // phần đuôi.
    assert_eq!(
        huba::sessions::declared_parts("[khong-co-du-an-nao-ten-nay/LANE] ok", &ws),
        None
    );
    // Gạch chéo cụt: `Path::join` nuốt nó im lặng, nên phải chặn tay — nếu
    // không thì nhãn in ra `[dwork/]`.
    assert_eq!(
        huba::sessions::declared_parts("[dwork/] ok", &ws),
        Some(("dwork".to_string(), String::new()))
    );
}

/// Ba phiên cùng dự án, ba làn tự khai ⟹ ba nhãn khác nhau — **và mỗi cái vẫn
/// nói việc nó đang làm**.
///
/// 🔄 ĐẢO CHIỀU 2026-09-05, không xoá. Bản trước khoá điều ngược lại
/// (*"làn đã phân biệt được rồi thì đừng mượn tên việc nữa"*, Hà 22/08), và luật
/// ấy đúng với câu hỏi của nó: *lấy gì làm TÊN*. Hôm nay Hà trả lời một câu
/// khác — *có tên rồi thì có được nói thêm việc không* — và chốt **có**, sau khi
/// phiên `[dwork/a-ddoc]` chỉ ra cái bẫy: đuôi `·<việc>` cũ chỉ mọc ở nhánh
/// "nhãn TRÙNG nhau", nên nó sống được là nhờ 9 làn dwork khai sai mà đọc ra
/// `[dwork]` giống hệt nhau. Khai đúng làn ⟹ hết trùng ⟹ mất dòng việc.
///
/// Vế PHẢI GIỮ của bài kiểm cũ không mất và nằm ngay dưới đây: cái làn tự khai
/// vẫn phải có mặt nguyên vẹn trong nhãn, không bị tên việc đẩy ra.
#[test]
fn declared_lanes_keep_the_lane_and_still_carry_the_task() {
    let ws = workspace();
    let root = std::path::Path::new(&ws);
    let mut rows: Vec<huba::sessions::LiveSession> = ["A-DSIGN", "A-DDOC", ""]
        .iter()
        .enumerate()
        .map(|(i, lane)| {
            let mut s = huba::sessions::LiveSession {
                session_id: format!("{i}{i}{i}{i}{i}{i}{i}{i}-0000-0000-0000-000000000000"),
                ..Default::default()
            };
            s.folder = "dwork".into();
            s.lane = (*lane).to_string();
            // Tên việc DÀI và khác nhau — ba làn khác nhau nên không cái nào
            // phải mượn của cái nào; đây đúng ca Hà chốt hôm 05/09.
            s.doing = format!("Quét GitHub làm design lượt {i}");
            s
        })
        .collect();
    huba::sessions::label_sessions(&mut rows, root);
    let nhan: Vec<String> = rows.iter().map(|s| s.label.clone()).collect();
    assert_eq!(
        nhan,
        vec![
            "[dwork/A-DSIGN]·Quét GitHub làm design lượt 0",
            "[dwork/A-DDOC]·Quét GitHub làm design lượt 1",
            "[dwork]·Quét GitHub làm design lượt 2",
        ]
    );
    // Vế giữ lại từ bản 22/08: LÀN không được biến mất khỏi nhãn. Đây là thứ
    // hỏng thật lần trước — `[dwork]·Quét GitHub làm design` cho một phiên tự
    // xưng `[dwork/A-DSIGN]`.
    assert!(
        nhan[0].starts_with("[dwork/A-DSIGN]") && nhan[1].starts_with("[dwork/A-DDOC]"),
        "làn tự khai phải còn nguyên trong ngoặc: {nhan:?}"
    );
    // Và KHÔNG hàng nào được rơi về mã id: id chỉ dành cho ca không còn gì để
    // phân biệt (xem `two_sessions_doing_the_same_thing_fall_back_to_the_id`).
    assert!(
        !nhan
            .iter()
            .any(|l| l.contains("00000000") || l.contains("11111111")),
        "còn làn và còn việc thì đừng đeo id: {nhan:?}"
    );
}

/// ĐỐI CHỨNG NGƯỢC cho lượt đảo chiều: đuôi việc KHÔNG được nuốt mất luật id.
///
/// Ba ca đứng cạnh nhau vì chúng là ba nhánh của cùng một quyết định, và một
/// bản vá "luôn gắn đuôi" viết ẩu sẽ làm hỏng đúng hai ca sau mà ca đầu vẫn
/// xanh.
#[test]
fn the_always_on_task_tail_does_not_erase_the_id_fallback() {
    let ws = workspace();
    let root = std::path::Path::new(&ws);
    let row = |id: &str, lane: &str, doing: &str| huba::sessions::LiveSession {
        session_id: id.to_string(),
        folder: "dwork".into(),
        lane: lane.to_string(),
        doing: doing.to_string(),
        ..Default::default()
    };

    // ① Nhãn TRÙNG + việc GIỐNG NHAU ⟹ tên việc không tách được ai với ai, phải
    //    về id — nếu không thì hai hàng in ra hai chuỗi y hệt.
    let mut rows = vec![
        row(
            "aaaaaaaa-0000-0000-0000-000000000000",
            "",
            "Chạy cổng chất lượng",
        ),
        row(
            "bbbbbbbb-0000-0000-0000-000000000000",
            "",
            "Chạy cổng chất lượng",
        ),
    ];
    huba::sessions::label_sessions(&mut rows, root);
    assert_eq!(rows[0].label, "[dwork]·aaaaaaaa");
    assert_eq!(rows[1].label, "[dwork]·bbbbbbbb");

    // ② Nhãn TRÙNG + không đọc được việc ⟹ vẫn là id, y như trước lượt đảo.
    let mut rows = vec![
        row("cccccccc-0000-0000-0000-000000000000", "", ""),
        row("dddddddd-0000-0000-0000-000000000000", "", ""),
    ];
    huba::sessions::label_sessions(&mut rows, root);
    assert_eq!(rows[0].label, "[dwork]·cccccccc");
    assert_eq!(rows[1].label, "[dwork]·dddddddd");

    // ③ Một mình + không đọc được việc ⟹ nhãn TRẦN. Không dấu `·` cụt đuôi,
    //    không id: ca hiếm không được làm phiền ca thường.
    let mut rows = vec![row("eeeeeeee-0000-0000-0000-000000000000", "a-ddoc", "")];
    huba::sessions::label_sessions(&mut rows, root);
    assert_eq!(rows[0].label, "[dwork/a-ddoc]");
}
