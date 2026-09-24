//! Bộ nói giao thức Redis (RESP2) tối giản, ĐỒNG BỘ — đủ cho sổ việc, không hơn.
//!
//! Vì sao không kéo crate `redis`: huba cố ý đồng bộ và cố ý ít phụ thuộc (mọi
//! thứ phải dựng được `--offline`), còn thứ sổ việc cần chỉ là gửi một mảng
//! chuỗi và đọc về một câu trả lời. RESP2 là năm kiểu (`+ - : $ *`), dựng tay ít
//! dòng hơn phần cấu hình một client lạ. Hà 2026-09-24 chốt Redis cho sổ việc:
//! *"Cần gì ghi đĩa tức thì đâu, cần nhanh không nghẽn"* — đo cùng lúc trên máy
//! này: độ trễ trung bình 0,32 ms, cao nhất 3 ms.
//!
//! Mỗi lượt dùng mở MỘT kết nối rồi bỏ (localhost < 1 ms): không có kết nối sống
//! lâu nào để chết im giữa chừng khi Redis khởi động lại.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

/// Địa chỉ Redis — `HUB_REDIS_ADDR` đè được (bài kiểm, máy khác).
pub fn addr() -> String {
    std::env::var("HUB_REDIS_ADDR").unwrap_or_else(|_| "127.0.0.1:6379".to_string())
}

/// Một câu trả lời RESP2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resp {
    Simple(String),
    Error(String),
    Int(i64),
    /// `None` = bulk rỗng (`$-1`), tức "không có".
    Bulk(Option<Vec<u8>>),
    /// `None` = mảng rỗng (`*-1`), vd `XREADGROUP` không có gì mới.
    Array(Option<Vec<Resp>>),
}

impl Resp {
    /// Chuỗi của một `Simple`/`Bulk`; `None` cho mọi thứ khác (kể cả bulk rỗng).
    pub fn as_str(&self) -> Option<String> {
        match self {
            Resp::Simple(s) => Some(s.clone()),
            Resp::Bulk(Some(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Resp::Int(n) => Some(*n),
            Resp::Bulk(Some(b)) => std::str::from_utf8(b).ok()?.trim().parse().ok(),
            Resp::Simple(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    /// Phần tử của một mảng; mảng rỗng/`nil` ⟹ danh sách rỗng.
    pub fn items(&self) -> Vec<Resp> {
        match self {
            Resp::Array(Some(v)) => v.clone(),
            _ => Vec::new(),
        }
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, Resp::Bulk(None) | Resp::Array(None))
    }
}

/// Mã hoá một lệnh thành mảng bulk string.
pub fn encode(args: &[&str]) -> Vec<u8> {
    let mut out = format!("*{}\r\n", args.len()).into_bytes();
    for a in args {
        out.extend_from_slice(format!("${}\r\n", a.len()).as_bytes());
        out.extend_from_slice(a.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn read_line<R: BufRead>(r: &mut R) -> Result<String> {
    let mut line = String::new();
    let n = r.read_line(&mut line).context("đọc dòng RESP")?;
    if n == 0 {
        return Err(anyhow!("Redis đóng kết nối giữa câu trả lời"));
    }
    if !line.ends_with("\r\n") {
        return Err(anyhow!("dòng RESP thiếu CRLF: {line:?}"));
    }
    line.truncate(line.len() - 2);
    Ok(line)
}

/// Đọc MỘT câu trả lời RESP2.
pub fn parse<R: BufRead>(r: &mut R) -> Result<Resp> {
    let line = read_line(r)?;
    let (kind, rest) = line.split_at(line.len().min(1));
    Ok(match kind {
        "+" => Resp::Simple(rest.to_string()),
        "-" => Resp::Error(rest.to_string()),
        ":" => Resp::Int(
            rest.parse()
                .with_context(|| format!("số RESP hỏng: {rest}"))?,
        ),
        "$" => {
            let n: i64 = rest
                .parse()
                .with_context(|| format!("độ dài bulk hỏng: {rest}"))?;
            if n < 0 {
                Resp::Bulk(None)
            } else {
                let mut buf = vec![0u8; n as usize + 2];
                r.read_exact(&mut buf).context("đọc bulk RESP")?;
                if &buf[n as usize..] != b"\r\n" {
                    return Err(anyhow!("bulk RESP thiếu CRLF cuối"));
                }
                buf.truncate(n as usize);
                Resp::Bulk(Some(buf))
            }
        }
        "*" => {
            let n: i64 = rest
                .parse()
                .with_context(|| format!("độ dài mảng hỏng: {rest}"))?;
            if n < 0 {
                Resp::Array(None)
            } else {
                let mut v = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    v.push(parse(r)?);
                }
                Resp::Array(Some(v))
            }
        }
        other => return Err(anyhow!("kiểu RESP lạ {other:?} trong dòng {line:?}")),
    })
}

/// Một kết nối Redis.
pub struct Redis {
    w: TcpStream,
    r: BufReader<TcpStream>,
}

impl Redis {
    /// Kết nối với trần 2 giây cho mọi chiều — Redis trên localhost trả lời trong
    /// dưới 1 ms, nên 2 giây là "Redis đang chết", không phải "Redis đang chậm".
    pub fn connect() -> Result<Redis> {
        let a = addr();
        let sock: std::net::SocketAddr = a
            .parse()
            .with_context(|| format!("địa chỉ Redis hỏng: {a}"))?;
        let s = TcpStream::connect_timeout(&sock, Duration::from_secs(2))
            .with_context(|| format!("không kết nối được Redis {a}"))?;
        s.set_read_timeout(Some(Duration::from_secs(2)))?;
        s.set_write_timeout(Some(Duration::from_secs(2)))?;
        s.set_nodelay(true)?;
        let r = BufReader::new(s.try_clone()?);
        Ok(Redis { w: s, r })
    }

    /// Gửi một lệnh, đọc câu trả lời. Một `-ERR …` của Redis thành `Err` — chỗ
    /// gọi nào CHỜ một lỗi cụ thể (vd `BUSYGROUP`) thì dùng [`Redis::cmd_raw`].
    pub fn cmd(&mut self, args: &[&str]) -> Result<Resp> {
        match self.cmd_raw(args)? {
            Resp::Error(e) => Err(anyhow!(
                "Redis trả lỗi cho {}: {e}",
                args.first().unwrap_or(&"")
            )),
            ok => Ok(ok),
        }
    }

    pub fn cmd_raw(&mut self, args: &[&str]) -> Result<Resp> {
        self.w.write_all(&encode(args)).context("gửi lệnh Redis")?;
        parse(&mut self.r)
    }
}
