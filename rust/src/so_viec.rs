//! SỔ VIỆC — một cửa cho mọi việc huba nhận làm hộ; mỗi vai chỉ ĐÁNH DẤU bước.
//!
//! 🔴 Hà 2026-09-24: *"Cơ chế 1 cửa, cửa nhận cứ nhận, cho vào hàng đợi, bên
//! thực thi cứ thực thi xong cũng cho vào hàng đợi kết quả, bên trả kết quả cứ
//! đọc rồi trả, mỗi vai làm gì xong chưa chỉ cần đánh dấu vào thì cho dù có
//! reset cũng không ảnh hưởng công việc, trạng thái xử lý cần có nhiều bước để
//! kiểm soát đọc chưa, là chưa, làm tới đâu, xong chưa, gửi chưa"* — rồi chọn kho:
//! *"Cần gì ghi đĩa tức thì đâu, cần nhanh không nghẽn"* ⟹ Redis Streams.
//!
//! Vì sao phải có: 23/09 18:15:45Z huba nhận CÙNG LÚC hai hòm thư (main dwork
//! `477a2a31`, `8bcdde1b`), rồi 30 giây sau một lượt cài khởi động lại daemon.
//! Hàng việc, lệnh đang chạy và bước dán kết quả đều chỉ sống trong BỘ NHỚ của
//! tiến trình, còn tệp hòm thư đã bị đổi tên `.taken` từ trước (chống chạy lặp)
//! ⟹ việc thứ nhất chạy rồi mà kết quả không về phiên, việc thứ hai CHƯA HỀ
//! chạy — và không một dòng nào nói "mất".
//!
//! Hình dạng trên Redis (mọi khoá mang tiền tố [`So::pre`], mặc định `huba:`):
//!
//! | khoá | kiểu | vai |
//! |---|---|---|
//! | `viec:<id>` | hash | mọi trường của một việc + `buoc` + mốc giờ từng bước |
//! | `hang_viec` | stream, nhóm `thuc_thi` | cửa nhận → bên thực thi |
//! | `ket_qua` | stream, nhóm `tra` | bên thực thi → bên trả |
//! | `khoa:<khoa>` | string, 7 ngày | chống nhận trùng một tệp hòm thư |
//!
//! Mục đã `XREADGROUP` mà chưa `XACK` chính là "đã đọc, chưa xong" — sống qua
//! khởi động lại của huba (nằm trong Redis), và [`khoi_phuc`] quyết định mỗi
//! bước làm gì tiếp. Việc đã bắt đầu chạy thì KHÔNG tự chạy lại.

use std::collections::HashMap;

use anyhow::{anyhow, Result};

use crate::redis_mini::{Redis, Resp};

/// Bước của một việc. Chuỗi lưu trong sổ là [`Buoc::as_str`] — đổi chuỗi là đổi
/// nghĩa của mọi việc đang nằm trong Redis, nên chuỗi là HẰNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Buoc {
    /// Cửa nhận đã ghi; chưa bên nào đọc.
    Nhan,
    /// Bên thực thi đã đọc, CHƯA chạy lệnh.
    Doc,
    /// Lệnh đang chạy (có `pid`, `chay_luc`).
    Chay,
    /// Có kết quả (`ma_thoat` + `ket_qua`), chờ bên trả.
    Xong,
    /// Bên trả đang trả (`lan_gui` đếm số lượt).
    Gui,
    /// Đã trả tới nơi.
    DaGui,
    /// Hỏng ở tầng huba — `ghi_chu` nói vì sao.
    Loi,
    /// Bỏ, có lý do (phiên đích đã tắt, trả mãi không được).
    Bo,
}

impl Buoc {
    pub fn as_str(self) -> &'static str {
        match self {
            Buoc::Nhan => "nhan",
            Buoc::Doc => "doc",
            Buoc::Chay => "chay",
            Buoc::Xong => "xong",
            Buoc::Gui => "gui",
            Buoc::DaGui => "da_gui",
            Buoc::Loi => "loi",
            Buoc::Bo => "bo",
        }
    }

    pub fn parse(s: &str) -> Option<Buoc> {
        Some(match s {
            "nhan" => Buoc::Nhan,
            "doc" => Buoc::Doc,
            "chay" => Buoc::Chay,
            "xong" => Buoc::Xong,
            "gui" => Buoc::Gui,
            "da_gui" => Buoc::DaGui,
            "loi" => Buoc::Loi,
            "bo" => Buoc::Bo,
            _ => return None,
        })
    }

    /// Trường mốc giờ mà bước này ghi.
    fn truong_moc(self) -> &'static str {
        match self {
            Buoc::Nhan => "nhan_luc",
            Buoc::Doc => "doc_luc",
            Buoc::Chay => "chay_luc",
            Buoc::Xong | Buoc::Loi | Buoc::Bo => "xong_luc",
            Buoc::Gui | Buoc::DaGui => "gui_luc",
        }
    }

    /// Bước đã KHÉP — không vai nào còn phải làm gì.
    pub fn da_khep(self) -> bool {
        matches!(self, Buoc::DaGui | Buoc::Loi | Buoc::Bo)
    }
}

/// Một việc, đọc từ hash `viec:<id>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viec {
    pub id: i64,
    pub nguon: String,
    /// Phiên đích (hòm thư: phiên đã nhờ).
    pub phien: String,
    /// Dòng lệnh.
    pub noi_dung: String,
    pub im_lang: bool,
    pub buoc: Buoc,
    pub pid: Option<i64>,
    pub ma_thoat: Option<i64>,
    /// Khối chữ sẽ trả cho phiên.
    pub ket_qua: Option<String>,
    pub ghi_chu: Option<String>,
    pub lan_gui: i64,
    pub nhan_luc: String,
    pub chay_luc: Option<String>,
    /// Lúc có kết quả — mốc để tính hạn bỏ cuộc của bên trả.
    pub xong_luc: Option<String>,
    /// Lượt trả gần nhất — mốc để bên trả không gõ dồn vào một Terminal đang câm.
    pub gui_luc: Option<String>,
    /// Mã mục trong `hang_viec` — để `XACK` đúng mục khi đã có kết quả.
    pub vao: Option<String>,
}

impl Viec {
    fn tu_truong(id: i64, t: &HashMap<String, String>) -> Viec {
        let so = |k: &str| t.get(k).and_then(|v| v.parse::<i64>().ok());
        let chu = |k: &str| t.get(k).filter(|v| !v.is_empty()).cloned();
        Viec {
            id,
            nguon: t.get("nguon").cloned().unwrap_or_default(),
            phien: t.get("phien").cloned().unwrap_or_default(),
            noi_dung: t.get("noi_dung").cloned().unwrap_or_default(),
            im_lang: t.get("im_lang").is_some_and(|v| v == "1"),
            // Một chuỗi bước lạ là sổ hỏng — đọc thành `Loi` để nó HIỆN ra ở chỗ
            // gọi (không bao giờ bị chạy lại), thay vì lặng lẽ biến mất.
            buoc: t
                .get("buoc")
                .and_then(|b| Buoc::parse(b))
                .unwrap_or(Buoc::Loi),
            pid: so("pid"),
            ma_thoat: so("ma_thoat"),
            ket_qua: chu("ket_qua"),
            ghi_chu: chu("ghi_chu"),
            lan_gui: so("lan_gui").unwrap_or(0),
            nhan_luc: t.get("nhan_luc").cloned().unwrap_or_default(),
            chay_luc: chu("chay_luc"),
            xong_luc: chu("xong_luc"),
            gui_luc: chu("gui_luc"),
            vao: chu("vao"),
        }
    }
}

/// Lúc khởi động lại, một việc còn treo ở bước ấy cần gì — THUẦN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KhoiPhuc {
    /// Chưa bắt đầu chạy ⟹ đưa lại cho bên thực thi. Chạy không trùng.
    ChayLai,
    /// Đang chạy khi tiến trình chết ⟹ KHÔNG chạy lại (lệnh có thể đã chạy một
    /// phần); ghi một kết quả nói đúng như vậy để bên trả báo cho người nhờ.
    BaoCoTheDaChay,
    /// Có kết quả rồi — việc của bên trả, bên thực thi chỉ `XACK`.
    DaCoKetQua,
    /// Đã khép.
    KhongLamGi,
}

pub fn khoi_phuc(b: Buoc) -> KhoiPhuc {
    match b {
        Buoc::Nhan | Buoc::Doc => KhoiPhuc::ChayLai,
        Buoc::Chay => KhoiPhuc::BaoCoTheDaChay,
        Buoc::Xong | Buoc::Gui => KhoiPhuc::DaCoKetQua,
        Buoc::DaGui | Buoc::Loi | Buoc::Bo => KhoiPhuc::KhongLamGi,
    }
}

/// Khối chữ báo cho phiên khi huba khởi động lại giữa lúc lệnh của nó đang chạy.
pub fn bao_co_the_da_chay(noi_dung: &str, chay_luc: Option<&str>) -> String {
    format!(
        "[huba chạy hộ]\n$ {noi_dung}\n⚠ huba KHỞI ĐỘNG LẠI khi lệnh này đang chạy (bắt đầu {}). \
         Lệnh CÓ THỂ đã chạy một phần hoặc chạy xong — huba KHÔNG tự chạy lại, vì lệnh như \
         push/kill chạy hai lần là hỏng. Đo lại trạng thái rồi xếp lại hòm thư nếu cần.",
        chay_luc.unwrap_or("không rõ lúc")
    )
}

/// Bên trả gặp một việc nằm trong `ket_qua` — làm gì với nó. THUẦN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraGi {
    /// Việc đã khép (trả rồi, hoặc bỏ có lý do) — chỉ `XACK` mục.
    Khep,
    /// Vừa thử trả chưa lâu — để yên mục, lượt sau hỏi lại.
    ChoLai,
    /// Thử trả lượt này.
    Tra,
    /// Mục nằm trong `ket_qua` mà việc chưa có kết quả — sổ hỏng: khép và NÓI RA,
    /// đừng để nó quay vòng mãi trong hàng.
    SaiBuoc,
}

/// `giay_tu_lan_gui`: số giây từ lượt trả gần nhất (`None` = chưa trả lượt nào).
/// `ngay`: gọi từ chính luồng vừa có kết quả — trả luôn, không chờ nhịp.
pub fn tra_gi(buoc: Buoc, giay_tu_lan_gui: Option<i64>, ngay: bool, nhip_sec: i64) -> TraGi {
    match buoc {
        b if b.da_khep() => TraGi::Khep,
        Buoc::Xong | Buoc::Gui => match giay_tu_lan_gui {
            Some(g) if !ngay && g < nhip_sec => TraGi::ChoLai,
            _ => TraGi::Tra,
        },
        _ => TraGi::SaiBuoc,
    }
}

/// Số giây từ một mốc [`crate::logging::now_iso`] tới `now` (giây epoch). Mốc
/// hỏng ⟹ `None` — chỗ gọi phải tự chọn phía an toàn, không đọc thành 0.
pub fn giay_tu(moc: &str, now: i64) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(moc)
        .ok()
        .map(|t| now - t.timestamp())
}

/// Khoá chống nhận trùng MỘT lần ghi hòm thư: đường dẫn + mốc sửa (ns) + cỡ.
///
/// Đường dẫn một mình KHÔNG phải khoá: phiên ghi lại đúng `huba-run.txt` mỗi lần
/// nhờ, nên khoá chỉ theo đường dẫn là nuốt lệnh thứ hai của cùng phiên như thể
/// nó là lệnh thứ nhất.
pub fn khoa_hom_thu(duong_dan: &str, sua_ns: u128, co: u64) -> String {
    format!("{duong_dan}@{sua_ns}:{co}")
}

/// Trần độ dài hai stream — đủ nhiều ngày việc, không để Redis phình mãi.
const STREAM_MAXLEN: &str = "20000";
/// Việc đã khép giữ hash bao lâu (giây) — đủ để tra lại khi ai đó hỏi.
const GIU_VIEC_KHEP_SEC: &str = "604800";
/// Khoá chống nhận trùng giữ bao lâu (giây).
const GIU_KHOA_SEC: &str = "604800";

/// Tay cầm sổ: tiền tố khoá + tên người đọc trong nhóm.
#[derive(Debug, Clone)]
pub struct So {
    pub pre: String,
    pub nguoi_doc: String,
}

impl So {
    /// Sổ thật của huba.
    pub fn huba() -> So {
        So {
            pre: "huba:".to_string(),
            nguoi_doc: "hubd".to_string(),
        }
    }

    fn k(&self, s: &str) -> String {
        format!("{}{}", self.pre, s)
    }

    /// Mở một kết nối và bảo đảm hai nhóm đọc đã có.
    ///
    /// Gọi [`So::chuan_bi`] MỖI lần, cố ý: Redis máy này không ghi đĩa tức thì
    /// (`appendonly no`), nên một lần nó khởi động lại có thể mất cả nhóm — và
    /// `XREADGROUP` vào nhóm không có trả `NOGROUP` ở MỌI lượt sau, không tự lành.
    /// Hai câu `XGROUP CREATE` trên localhost tốn dưới 1 ms.
    pub fn ket_noi(&self) -> Result<Redis> {
        let mut r = Redis::connect()?;
        self.chuan_bi(&mut r)?;
        Ok(r)
    }

    /// Dựng hai nhóm đọc (bỏ qua `BUSYGROUP` — đã có).
    pub fn chuan_bi(&self, r: &mut Redis) -> Result<()> {
        for (stream, nhom) in [("hang_viec", "thuc_thi"), ("ket_qua", "tra")] {
            match r.cmd_raw(&["XGROUP", "CREATE", &self.k(stream), nhom, "0", "MKSTREAM"])? {
                Resp::Error(e) if e.starts_with("BUSYGROUP") => {}
                Resp::Error(e) => return Err(anyhow!("XGROUP CREATE {stream}: {e}")),
                _ => {}
            }
        }
        Ok(())
    }

    /// CỬA NHẬN: ghi hash TRƯỚC, rồi mới đưa vào `hang_viec`. `Ok(None)` = `khoa`
    /// đã nhận rồi — không phải lỗi.
    pub fn nhan(
        &self,
        r: &mut Redis,
        nguon: &str,
        khoa: Option<&str>,
        phien: &str,
        noi_dung: &str,
        im_lang: bool,
    ) -> Result<Option<i64>> {
        if let Some(kh) = khoa {
            let set = r.cmd(&[
                "SET",
                &self.k(&format!("khoa:{kh}")),
                "1",
                "NX",
                "EX",
                GIU_KHOA_SEC,
            ])?;
            if set.is_nil() {
                return Ok(None);
            }
        }
        let id = r
            .cmd(&["INCR", &self.k("viec:seq")])?
            .as_int()
            .ok_or_else(|| anyhow!("INCR viec:seq không trả số"))?;
        let ids = id.to_string();
        let luc = crate::logging::now_iso();
        r.cmd(&[
            "HSET",
            &self.k(&format!("viec:{id}")),
            "nguon",
            nguon,
            "phien",
            phien,
            "noi_dung",
            noi_dung,
            "im_lang",
            if im_lang { "1" } else { "0" },
            "buoc",
            Buoc::Nhan.as_str(),
            "nhan_luc",
            &luc,
            "lan_gui",
            "0",
        ])?;
        r.cmd(&[
            "XADD",
            &self.k("hang_viec"),
            "MAXLEN",
            "~",
            STREAM_MAXLEN,
            "*",
            "id",
            &ids,
        ])?;
        Ok(Some(id))
    }

    pub fn lay(&self, r: &mut Redis, id: i64) -> Result<Option<Viec>> {
        let v = r.cmd(&["HGETALL", &self.k(&format!("viec:{id}"))])?;
        let items = v.items();
        if items.is_empty() {
            return Ok(None);
        }
        let mut t = HashMap::new();
        for cap in items.chunks(2) {
            if let [k, val] = cap {
                if let (Some(k), Some(val)) = (k.as_str(), val.as_str()) {
                    t.insert(k, val);
                }
            }
        }
        Ok(Some(Viec::tu_truong(id, &t)))
    }

    /// Đánh dấu bước (+ mốc giờ của bước, + ghi chú nếu có). Bước đã khép thì
    /// hash được giữ thêm [`GIU_VIEC_KHEP_SEC`] rồi tự đi.
    pub fn danh_dau(
        &self,
        r: &mut Redis,
        id: i64,
        buoc: Buoc,
        ghi_chu: Option<&str>,
    ) -> Result<()> {
        let khoa = self.k(&format!("viec:{id}"));
        let luc = crate::logging::now_iso();
        let mut args: Vec<&str> = vec![
            "HSET",
            &khoa,
            "buoc",
            buoc.as_str(),
            buoc.truong_moc(),
            &luc,
        ];
        if let Some(g) = ghi_chu {
            args.push("ghi_chu");
            args.push(g);
        }
        r.cmd(&args)?;
        if buoc.da_khep() {
            r.cmd(&["EXPIRE", &khoa, GIU_VIEC_KHEP_SEC])?;
        }
        Ok(())
    }

    /// Bên thực thi nhớ mục `hang_viec` đang cầm — để `XACK` đúng mục sau này.
    pub fn ghi_vao(&self, r: &mut Redis, id: i64, muc: &str) -> Result<()> {
        r.cmd(&["HSET", &self.k(&format!("viec:{id}")), "vao", muc])?;
        Ok(())
    }

    pub fn ghi_pid(&self, r: &mut Redis, id: i64, pid: i64) -> Result<()> {
        r.cmd(&[
            "HSET",
            &self.k(&format!("viec:{id}")),
            "pid",
            &pid.to_string(),
        ])?;
        Ok(())
    }

    /// Bên thực thi ghi KẾT QUẢ: hash sang `xong` → đưa vào `ket_qua` → rồi mới
    /// `XACK` mục bên `hang_viec`. Thứ tự này là cả điểm của sổ: chết giữa chừng
    /// ở bất kỳ đâu thì mục vẫn còn treo ở một trong hai hàng, không rơi khỏi cả hai.
    pub fn ghi_ket_qua(
        &self,
        r: &mut Redis,
        id: i64,
        ma_thoat: Option<i64>,
        ket_qua: &str,
    ) -> Result<()> {
        let khoa = self.k(&format!("viec:{id}"));
        let luc = crate::logging::now_iso();
        let ma = ma_thoat.map(|m| m.to_string()).unwrap_or_default();
        r.cmd(&[
            "HSET",
            &khoa,
            "buoc",
            Buoc::Xong.as_str(),
            "ket_qua",
            ket_qua,
            "ma_thoat",
            &ma,
            "xong_luc",
            &luc,
        ])?;
        r.cmd(&[
            "XADD",
            &self.k("ket_qua"),
            "MAXLEN",
            "~",
            STREAM_MAXLEN,
            "*",
            "id",
            &id.to_string(),
        ])?;
        if let Some(muc) = self.lay(r, id)?.and_then(|v| v.vao) {
            r.cmd(&["XACK", &self.k("hang_viec"), "thuc_thi", &muc])?;
        }
        Ok(())
    }

    /// Bên trả bắt đầu một lượt: `gui`, `lan_gui` + 1. Trả số lượt sau khi tăng.
    pub fn bat_dau_gui(&self, r: &mut Redis, id: i64) -> Result<i64> {
        let khoa = self.k(&format!("viec:{id}"));
        let n = r
            .cmd(&["HINCRBY", &khoa, "lan_gui", "1"])?
            .as_int()
            .unwrap_or(0);
        self.danh_dau(r, id, Buoc::Gui, None)?;
        Ok(n)
    }

    /// Mục MỚI (chưa ai đọc) của một hàng, vào tay người đọc này.
    fn doc(
        &self,
        r: &mut Redis,
        stream: &str,
        nhom: &str,
        tu: &str,
        bao_nhieu: usize,
    ) -> Result<Vec<(String, i64)>> {
        let n = bao_nhieu.to_string();
        let tl = r.cmd(&[
            "XREADGROUP",
            "GROUP",
            nhom,
            &self.nguoi_doc,
            "COUNT",
            &n,
            "STREAMS",
            &self.k(stream),
            tu,
        ])?;
        Ok(muc_tu_xread(&tl))
    }

    /// Bên thực thi: việc MỚI trong `hang_viec`.
    pub fn doc_viec_moi(&self, r: &mut Redis, bao_nhieu: usize) -> Result<Vec<(String, i64)>> {
        self.doc(r, "hang_viec", "thuc_thi", ">", bao_nhieu)
    }

    /// Bên thực thi: việc CỦA MÌNH đã đọc mà chưa `XACK` — dùng lúc khởi động lại.
    pub fn viec_con_treo(&self, r: &mut Redis, bao_nhieu: usize) -> Result<Vec<(String, i64)>> {
        self.doc(r, "hang_viec", "thuc_thi", "0", bao_nhieu)
    }

    /// Bên trả: kết quả MỚI.
    pub fn doc_ket_qua_moi(&self, r: &mut Redis, bao_nhieu: usize) -> Result<Vec<(String, i64)>> {
        self.doc(r, "ket_qua", "tra", ">", bao_nhieu)
    }

    /// Bên trả: kết quả đã đọc mà chưa trả xong, nằm yên ít nhất `im_ms` — lấy về
    /// để trả lại (`XAUTOCLAIM`). Đây là đường thử lại, và cũng là đường khởi động lại.
    pub fn ket_qua_treo(
        &self,
        r: &mut Redis,
        im_ms: u64,
        bao_nhieu: usize,
    ) -> Result<Vec<(String, i64)>> {
        let tl = r.cmd(&[
            "XAUTOCLAIM",
            &self.k("ket_qua"),
            "tra",
            &self.nguoi_doc,
            &im_ms.to_string(),
            "0-0",
            "COUNT",
            &bao_nhieu.to_string(),
        ])?;
        // [con_tro_tiep, [[muc, [truong, gia_tri…]]…], [muc_da_xoa…]]
        let phan = tl.items();
        let muc = phan.get(1).map(|m| m.items()).unwrap_or_default();
        Ok(muc.iter().filter_map(muc_mot).collect())
    }

    /// Bên trả: xong một kết quả (đã trả, hoặc đã khép vì lý do có ghi).
    pub fn da_tra(&self, r: &mut Redis, muc: &str) -> Result<()> {
        r.cmd(&["XACK", &self.k("ket_qua"), "tra", muc])?;
        Ok(())
    }

    /// Bên thực thi: khép một mục `hang_viec` mà không qua `ghi_ket_qua` (vd việc
    /// đã khép từ trước, gặp lại lúc khởi động lại).
    pub fn xong_muc_viec(&self, r: &mut Redis, muc: &str) -> Result<()> {
        r.cmd(&["XACK", &self.k("hang_viec"), "thuc_thi", muc])?;
        Ok(())
    }
}

/// `[muc, [truong, gia_tri, …]]` ⟹ `(muc, id)`.
fn muc_mot(m: &Resp) -> Option<(String, i64)> {
    let cap = m.items();
    let muc = cap.first()?.as_str()?;
    let truong = cap.get(1)?.items();
    let id = truong
        .chunks(2)
        .find(|c| c.first().and_then(Resp::as_str).as_deref() == Some("id"))
        .and_then(|c| c.get(1))
        .and_then(Resp::as_int)?;
    Some((muc, id))
}

/// Câu trả lời `XREADGROUP` ⟹ danh sách `(muc, id)`. `nil` (không có gì) ⟹ rỗng.
pub fn muc_tu_xread(tl: &Resp) -> Vec<(String, i64)> {
    tl.items()
        .iter()
        .flat_map(|luong| luong.items().get(1).map(|v| v.items()).unwrap_or_default())
        .filter_map(|m| muc_mot(&m))
        .collect()
}
