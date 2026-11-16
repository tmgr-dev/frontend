use std::io::{BufRead, Read};

pub const MAX_LINE: usize = 4 << 20;

pub fn for_each_line<R: BufRead>(mut reader: R, mut f: impl FnMut(&str) -> bool) -> usize {
    let mut skipped = 0;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = match (&mut reader)
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut buf)
        {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if buf.last() != Some(&b'\n') && n > MAX_LINE {
            skipped += 1;
            drain_line(&mut reader);
            continue;
        }
        let text = String::from_utf8_lossy(&buf);
        let line = text.trim_end_matches(['\n', '\r']);
        if line.trim().is_empty() {
            continue;
        }
        if !f(line) {
            skipped += 1;
        }
    }
    skipped
}

fn drain_line<R: BufRead>(reader: &mut R) {
    let mut sink = Vec::new();
    loop {
        sink.clear();
        match reader.take(1 << 20).read_until(b'\n', &mut sink) {
            Ok(0) | Err(_) => return,
            Ok(_) if sink.last() == Some(&b'\n') => return,
            Ok(_) => {}
        }
    }
}

pub fn first_line<R: BufRead>(reader: R) -> String {
    let mut buf = Vec::new();
    let _ = reader.take(64 * 1024).read_until(b'\n', &mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}
