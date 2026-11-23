pub const MIN_RATE: u32 = 8000;
pub const MAX_RATE: u32 = 96000;
const NO_AUDIO: &str = "the voice returned no audio";

#[derive(Clone, Copy, PartialEq)]
enum Kind {
  Int,
  Float,
}

struct Format {
  kind: Kind,
  channels: usize,
  rate: u32,
  bits: u16,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
  u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
  u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn parse_fmt(body: &[u8]) -> Result<Format, String> {
  if body.len() < 16 {
    return Err("the WAV format chunk is truncated".to_owned());
  }
  let mut tag = u16_at(body, 0);
  if tag == 0xFFFE {
    if body.len() < 26 {
      return Err("the WAV format chunk is truncated".to_owned());
    }
    tag = u16_at(body, 24);
  }
  let channels = u16_at(body, 2);
  let rate = u32_at(body, 4);
  let bits = u16_at(body, 14);
  let kind = match (tag, bits) {
    (1, 16) | (1, 32) => Kind::Int,
    (3, 32) => Kind::Float,
    (1 | 3, other) => return Err(format!("unsupported WAV sample size: {other}-bit (need 16-bit or 32-bit)")),
    (other, _) => return Err(format!("unsupported WAV encoding: {other}")),
  };
  if !(1..=2).contains(&channels) {
    return Err(format!("unsupported WAV channel count: {channels}"));
  }
  if !(MIN_RATE..=MAX_RATE).contains(&rate) {
    return Err(format!("unsupported WAV sample rate: {rate} Hz"));
  }
  Ok(Format { kind, channels: channels as usize, rate, bits })
}

fn decode(format: &Format, data: &[u8]) -> Vec<f32> {
  let width = usize::from(format.bits / 8);
  let frame = width * format.channels;
  data
    .chunks_exact(frame)
    .map(|f| {
      let sum: f32 = f
        .chunks_exact(width)
        .map(|s| match (format.kind, width) {
          (Kind::Int, 2) => f32::from(i16::from_le_bytes([s[0], s[1]])) / 32768.0,
          (Kind::Int, _) => (f64::from(i32::from_le_bytes([s[0], s[1], s[2], s[3]])) / 2147483648.0) as f32,
          (Kind::Float, _) => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
        })
        .sum();
      sum / format.channels as f32
    })
    .collect()
}

pub fn parse(buf: &[u8]) -> Result<(u32, Vec<f32>), String> {
  if buf.len() < 12 || &buf[0..4] != b"RIFF" || &buf[8..12] != b"WAVE" {
    return Err("the voice did not return a WAV file".to_owned());
  }
  let mut pos: u64 = 12;
  let mut format: Option<Format> = None;
  let total = buf.len() as u64;
  while pos + 8 <= total {
    let at = pos as usize;
    let id = &buf[at..at + 4];
    let size = u64::from(u32_at(buf, at + 4));
    let start = pos + 8;
    let remaining = total - start;
    if id == b"data" {
      let Some(format) = format.as_ref() else {
        return Err("the WAV data comes before its format".to_owned());
      };
      let len = if size == 0 || size == 0xFFFF_FFFF || size > remaining { remaining } else { size };
      let samples = decode(format, &buf[start as usize..(start + len) as usize]);
      if samples.is_empty() {
        return Err(NO_AUDIO.to_owned());
      }
      return Ok((format.rate, samples));
    }
    if id == b"fmt " {
      let len = size.min(remaining);
      format = Some(parse_fmt(&buf[start as usize..(start + len) as usize])?);
    }
    pos = start + size + (size & 1);
  }
  if format.is_none() {
    return Err("the WAV file is truncated".to_owned());
  }
  Err(NO_AUDIO.to_owned())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
      out.push(0);
    }
    out
  }

  fn fmt_body(tag: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
    let block = channels * bits / 8;
    let mut b = Vec::new();
    b.extend_from_slice(&tag.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * block as u32).to_le_bytes());
    b.extend_from_slice(&block.to_le_bytes());
    b.extend_from_slice(&bits.to_le_bytes());
    b
  }

  fn riff(chunks: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = chunks.concat();
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32 + 4).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(&body);
    out
  }

  fn pcm16(samples: &[i16]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
  }

  fn simple(tag: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
    riff(&[chunk(b"fmt ", &fmt_body(tag, channels, rate, bits)), chunk(b"data", data)])
  }

  #[test]
  fn pcm16_mono() {
    let (rate, s) = parse(&simple(1, 1, 22050, 16, &pcm16(&[0, 16384, -32768]))).unwrap();
    assert_eq!(rate, 22050);
    assert_eq!(s, vec![0.0, 0.5, -1.0]);
  }

  #[test]
  fn pcm32_int() {
    let data: Vec<u8> = [0i32, 1 << 30, i32::MIN].iter().flat_map(|s| s.to_le_bytes()).collect();
    let (_, s) = parse(&simple(1, 1, 24000, 32, &data)).unwrap();
    assert_eq!(s, vec![0.0, 0.5, -1.0]);
  }

  #[test]
  fn float32() {
    let data: Vec<u8> = [0.25f32, -0.75].iter().flat_map(|s| s.to_le_bytes()).collect();
    let (_, s) = parse(&simple(3, 1, 24000, 32, &data)).unwrap();
    assert_eq!(s, vec![0.25, -0.75]);
  }

  fn extensible(sub: u16, bits: u16, data: &[u8]) -> Vec<u8> {
    let mut body = fmt_body(0xFFFE, 1, 24000, bits);
    body.extend_from_slice(&22u16.to_le_bytes());
    body.extend_from_slice(&bits.to_le_bytes());
    body.extend_from_slice(&4u32.to_le_bytes());
    body.extend_from_slice(&sub.to_le_bytes());
    body.extend_from_slice(&[0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);
    riff(&[chunk(b"fmt ", &body), chunk(b"data", data)])
  }

  #[test]
  fn extensible_pcm_and_float() {
    let (_, s) = parse(&extensible(1, 16, &pcm16(&[16384]))).unwrap();
    assert_eq!(s, vec![0.5]);
    let (_, s) = parse(&extensible(3, 32, &0.5f32.to_le_bytes())).unwrap();
    assert_eq!(s, vec![0.5]);
    assert!(parse(&extensible(7, 16, &pcm16(&[1]))).is_err());
  }

  #[test]
  fn stereo_is_averaged() {
    let (_, s) = parse(&simple(1, 2, 24000, 16, &pcm16(&[16384, 0, -16384, -16384]))).unwrap();
    assert_eq!(s, vec![0.25, -0.5]);
  }

  #[test]
  fn skips_list_and_unknown_chunks_with_odd_padding() {
    let wav = riff(&[chunk(b"LIST", b"INFOabc"), chunk(b"fmt ", &fmt_body(1, 1, 16000, 16)), chunk(b"junk", b"x"), chunk(b"data", &pcm16(&[8192]))]);
    assert_eq!(parse(&wav).unwrap(), (16000, vec![0.25]));
  }

  #[test]
  fn streaming_sizes_mean_rest_of_buffer() {
    for size in [0u32, 0xFFFF_FFFF] {
      let mut wav = simple(1, 1, 24000, 16, &pcm16(&[8192, 16384]));
      wav[4..8].copy_from_slice(&size.to_le_bytes());
      let at = wav.len() - 4 - 4;
      wav[at..at + 4].copy_from_slice(&size.to_le_bytes());
      assert_eq!(parse(&wav).unwrap().1, vec![0.25, 0.5], "{size}");
    }
  }

  #[test]
  fn data_size_beyond_buffer_is_clamped_and_partial_frame_dropped() {
    let mut wav = simple(1, 1, 24000, 16, &pcm16(&[8192, 16384]));
    wav.push(7);
    let at = wav.len() - 5 - 4;
    wav[at..at + 4].copy_from_slice(&0x7000_0000u32.to_le_bytes());
    assert_eq!(parse(&wav).unwrap().1, vec![0.25, 0.5]);
  }

  #[test]
  fn truncated_inputs_are_errors() {
    assert!(parse(b"").is_err());
    assert!(parse(b"RIFF\0\0\0\0WAVE").is_err());
    assert!(parse(b"RIFF\0\0\0\0WAVEfmt ").is_err());
    assert!(parse(b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0\x01\0").is_err());
    assert!(parse(b"not a wave file at all").is_err());
  }

  #[test]
  fn huge_non_data_chunk_is_an_error() {
    let mut wav = riff(&[chunk(b"fmt ", &fmt_body(1, 1, 24000, 16))]);
    wav.extend_from_slice(b"LIST");
    wav.extend_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
    wav.extend_from_slice(b"abcd");
    assert!(parse(&wav).is_err());
  }

  #[test]
  fn rejects_unsupported_formats() {
    let e8 = parse(&simple(1, 1, 24000, 8, &[1, 2])).unwrap_err();
    assert!(e8.contains("8-bit"), "{e8}");
    let e24 = parse(&simple(1, 1, 24000, 24, &[1, 2, 3])).unwrap_err();
    assert!(e24.contains("24-bit"), "{e24}");
    assert!(parse(&simple(1, 0, 24000, 16, &[0, 0])).is_err());
    assert!(parse(&simple(1, 3, 24000, 16, &[0; 6])).is_err());
    assert!(parse(&simple(3, 1, 24000, 16, &[0; 2])).is_err());
    assert!(parse(&simple(2, 1, 24000, 16, &[0; 2])).is_err());
  }

  #[test]
  fn rate_must_be_in_range() {
    assert!(parse(&simple(1, 1, 7999, 16, &[0, 0])).is_err());
    assert!(parse(&simple(1, 1, 96001, 16, &[0, 0])).is_err());
    assert!(parse(&simple(1, 1, 8000, 16, &[0, 0])).is_ok());
    assert!(parse(&simple(1, 1, 96000, 16, &[0, 0])).is_ok());
  }

  #[test]
  fn empty_data_is_an_error() {
    assert_eq!(parse(&simple(1, 1, 24000, 16, &[])).unwrap_err(), NO_AUDIO);
    assert_eq!(parse(&simple(1, 2, 24000, 16, &[1, 0])).unwrap_err(), NO_AUDIO);
  }

  #[test]
  fn data_before_fmt_is_an_error() {
    assert!(parse(&riff(&[chunk(b"data", &[0, 0]), chunk(b"fmt ", &fmt_body(1, 1, 24000, 16))])).is_err());
  }
}
