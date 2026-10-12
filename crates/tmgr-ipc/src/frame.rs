use crate::proto::valid_session_id;
use serde::Serialize;
use std::io::{self, Read, Write};

pub const MAX_JSON: usize = 1 << 20;
pub const MAX_BIN: usize = 4 << 20;
pub const TAG_OUTPUT: u8 = 1;
pub const TAG_REPLAY: u8 = 2;
pub const TAG_INPUT: u8 = 3;

const KIND_JSON: u8 = 0;
const KIND_BIN: u8 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct BinFrame {
    pub tag: u8,
    pub session: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Json(serde_json::Value),
    Bin(BinFrame),
}

fn bad(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

fn limit(kind: u8) -> io::Result<usize> {
    match kind {
        KIND_JSON => Ok(MAX_JSON),
        KIND_BIN => Ok(MAX_BIN),
        k => Err(bad(format!("unknown frame kind {k}"))),
    }
}

fn check_len(len: usize) -> io::Result<()> {
    if len == 0 {
        Err(bad("empty frame"))
    } else if len - 1 > MAX_BIN {
        Err(bad("frame too large"))
    } else {
        Ok(())
    }
}

fn decode(kind: u8, payload: &[u8]) -> io::Result<Frame> {
    match kind {
        KIND_JSON => serde_json::from_slice(payload)
            .map(Frame::Json)
            .map_err(|e| bad(format!("bad json: {e}"))),
        KIND_BIN => {
            let [tag, sid_len, rest @ ..] = payload else {
                return Err(bad("short binary frame"));
            };
            let n = *sid_len as usize;
            if rest.len() < n {
                return Err(bad("short binary frame"));
            }
            let (sid, data) = rest.split_at(n);
            let sid = std::str::from_utf8(sid).map_err(|_| bad("bad session id"))?;
            if !sid.is_empty() && !valid_session_id(sid) {
                return Err(bad("bad session id"));
            }
            Ok(Frame::Bin(BinFrame {
                tag: *tag,
                session: sid.to_string(),
                data: data.to_vec(),
            }))
        }
        k => Err(bad(format!("unknown frame kind {k}"))),
    }
}

fn encode(kind: u8, payload: &[u8]) -> io::Result<Vec<u8>> {
    if payload.len() > limit(kind)? {
        return Err(bad("frame too large"));
    }
    let mut out = Vec::with_capacity(payload.len() + 5);
    out.extend_from_slice(&((payload.len() + 1) as u32).to_be_bytes());
    out.push(kind);
    out.extend_from_slice(payload);
    Ok(out)
}

fn encode_frame(frame: &Frame) -> io::Result<Vec<u8>> {
    match frame {
        Frame::Json(v) => encode(KIND_JSON, &serde_json::to_vec(v).map_err(io::Error::other)?),
        Frame::Bin(b) => {
            if !b.session.is_empty() && !valid_session_id(&b.session) {
                return Err(bad("bad session id"));
            }
            let mut p = Vec::with_capacity(b.data.len() + b.session.len() + 2);
            p.push(b.tag);
            p.push(b.session.len() as u8);
            p.extend_from_slice(b.session.as_bytes());
            p.extend_from_slice(&b.data);
            encode(KIND_BIN, &p)
        }
    }
}

pub fn write_frame<W: Write>(w: &mut W, frame: &Frame) -> io::Result<()> {
    w.write_all(&encode_frame(frame)?)
}

pub fn write_json<W: Write, T: Serialize>(w: &mut W, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    w.write_all(&encode(KIND_JSON, &bytes)?)
}

pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Option<Frame>> {
    let mut hdr = [0u8; 4];
    let mut got = 0;
    while got < hdr.len() {
        match r.read(&mut hdr[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let len = u32::from_be_bytes(hdr) as usize;
    check_len(len)?;
    let mut kind = [0u8; 1];
    r.read_exact(&mut kind)?;
    if len - 1 > limit(kind[0])? {
        return Err(bad("frame too large"));
    }
    let mut payload = vec![0u8; len - 1];
    r.read_exact(&mut payload)?;
    decode(kind[0], &payload).map(Some)
}

#[derive(Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> io::Result<Option<Frame>> {
        if self.buf.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        check_len(len)?;
        if self.buf.len() < 5 {
            return Ok(None);
        }
        let kind = self.buf[4];
        if len - 1 > limit(kind)? {
            return Err(bad("frame too large"));
        }
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let frame = decode(kind, &self.buf[5..4 + len])?;
        self.buf.drain(..4 + len);
        Ok(Some(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SID: &str = "s_abcdefghij";

    fn bytes(f: &Frame) -> Vec<u8> {
        let mut v = Vec::new();
        write_frame(&mut v, f).unwrap();
        v
    }

    fn bin(sid: &str) -> Frame {
        Frame::Bin(BinFrame {
            tag: TAG_OUTPUT,
            session: sid.into(),
            data: vec![0, 1, 255],
        })
    }

    #[test]
    fn json_round_trip() {
        let f = Frame::Json(json!({"a": [1, 2], "b": "x"}));
        let b = bytes(&f);
        assert_eq!(read_frame(&mut &b[..]).unwrap(), Some(f));
    }

    #[test]
    fn bin_round_trip() {
        for sid in [SID, ""] {
            let f = bin(sid);
            let b = bytes(&f);
            assert_eq!(read_frame(&mut &b[..]).unwrap(), Some(f));
        }
    }

    #[test]
    fn clean_eof_is_none_and_sequence_reads() {
        let mut b = bytes(&bin(SID));
        b.extend(bytes(&Frame::Json(json!(1))));
        let mut r = &b[..];
        assert!(read_frame(&mut r).unwrap().is_some());
        assert!(read_frame(&mut r).unwrap().is_some());
        assert_eq!(read_frame(&mut r).unwrap(), None);
    }

    #[test]
    fn truncated_frames_error() {
        let b = bytes(&bin(SID));
        for cut in [1, 3, 4, 5, b.len() - 1] {
            let e = read_frame(&mut &b[..cut]).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::UnexpectedEof, "cut {cut}");
        }
    }

    #[test]
    fn oversize_header_rejected_without_reading_body() {
        let hdr = 0x8000_0000u32.to_be_bytes();
        let e = read_frame(&mut &hdr[..]).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        let mut d = FrameDecoder::new();
        d.push(&hdr);
        assert_eq!(d.next().unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn json_over_limit_rejected() {
        let mut b = ((MAX_JSON + 2) as u32).to_be_bytes().to_vec();
        b.push(KIND_JSON);
        assert_eq!(
            read_frame(&mut &b[..]).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let big = Frame::Json(json!("x".repeat(MAX_JSON)));
        assert!(write_frame(&mut Vec::new(), &big).is_err());
    }

    #[test]
    fn zero_length_unknown_kind_and_bad_json() {
        assert!(read_frame(&mut &[0u8, 0, 0, 0][..]).is_err());
        assert!(read_frame(&mut &[0u8, 0, 0, 2, 9, 0][..]).is_err());
        assert!(read_frame(&mut &[0u8, 0, 0, 3, 0, b'{', b'x'][..]).is_err());
    }

    #[test]
    fn bad_sid_rejected_both_ways() {
        assert!(write_frame(&mut Vec::new(), &bin("../etc")).is_err());
        let mut p = vec![TAG_OUTPUT, 6];
        p.extend_from_slice(b"../etc");
        let mut b = ((p.len() + 1) as u32).to_be_bytes().to_vec();
        b.push(KIND_BIN);
        b.extend(p);
        assert!(read_frame(&mut &b[..]).is_err());
        assert!(read_frame(&mut &[0u8, 0, 0, 3, 1, 3, 9][..]).is_err());
    }

    #[test]
    fn decoder_byte_by_byte() {
        let frames = [bin(SID), Frame::Json(json!({"k": "v"})), bin("")];
        let all: Vec<u8> = frames.iter().flat_map(bytes).collect();
        let mut d = FrameDecoder::new();
        let mut got = Vec::new();
        for byte in all {
            d.push(&[byte]);
            while let Some(f) = d.next().unwrap() {
                got.push(f);
            }
        }
        assert_eq!(got, frames);
    }

    #[test]
    fn write_json_matches_frame() {
        let mut a = Vec::new();
        write_json(&mut a, &json!({"x": 1})).unwrap();
        assert_eq!(a, bytes(&Frame::Json(json!({"x": 1}))));
    }
}
