use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::model::{Account, ParsedRecord, Tool};

pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogFile {
    pub path: PathBuf,
    pub rel: String,
    pub account: String,
    pub tool: Tool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParserState(pub serde_json::Value);

impl ParserState {
    pub fn get<T: DeserializeOwned>(&self) -> Option<T> {
        serde_json::from_value(self.0.clone()).ok()
    }

    pub fn set<T: Serialize>(&mut self, value: &T) {
        self.0 = serde_json::to_value(value).unwrap_or_default();
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseOutcome {
    pub next_offset: u64,
    pub oversized: u64,
    pub bad_lines: u64,
}

pub trait Collector {
    fn tool(&self) -> Tool;
    fn parser_version(&self) -> u32;
    fn discover(&self, account: &Account) -> Vec<LogFile>;
    fn parse(
        &self,
        file: &LogFile,
        from: u64,
        state: &mut ParserState,
        sink: &mut dyn FnMut(ParsedRecord),
    ) -> Result<ParseOutcome>;
}

pub struct LineScan {
    pub next_offset: u64,
    pub oversized: u64,
}

pub fn for_each_line(
    path: &Path,
    from: u64,
    max_line: usize,
    f: &mut dyn FnMut(&[u8]),
) -> Result<LineScan> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(from))?;
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut offset = from;
    let mut line_start = from;
    let mut line: Vec<u8> = Vec::new();
    let mut too_long = false;
    let mut oversized = 0;
    loop {
        let (consumed, found) = {
            let buf = reader.fill_buf()?;
            if buf.is_empty() {
                break;
            }
            let (chunk, found) = match memchr::memchr(b'\n', buf) {
                Some(i) => (&buf[..i], true),
                None => (buf, false),
            };
            if !too_long {
                line.extend_from_slice(chunk);
                if line.len() > max_line {
                    too_long = true;
                    line.clear();
                }
            }
            (chunk.len() + usize::from(found), found)
        };
        reader.consume(consumed);
        offset += consumed as u64;
        if found {
            if too_long {
                oversized += 1;
            } else {
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if !line.is_empty() {
                    f(&line);
                }
            }
            line.clear();
            too_long = false;
            line_start = offset;
        }
    }
    Ok(LineScan {
        next_offset: line_start,
        oversized,
    })
}

pub fn walk_files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() && path.extension().is_some_and(|e| e == ext) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

pub fn rel_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
