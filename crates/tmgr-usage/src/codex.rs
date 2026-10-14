use crate::collector::{Collector, LogFile, ParseOutcome, ParserState};
use crate::error::{Result, UsageError};
use crate::model::{Account, ParsedRecord, Tool};

pub const PARSER_VERSION: u32 = 1;

pub struct CodexCollector;

impl Collector for CodexCollector {
    fn tool(&self) -> Tool {
        Tool::Codex
    }

    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }

    fn discover(&self, _account: &Account) -> Vec<LogFile> {
        Vec::new()
    }

    fn parse(
        &self,
        _file: &LogFile,
        _from: u64,
        _state: &mut ParserState,
        _sink: &mut dyn FnMut(ParsedRecord),
    ) -> Result<ParseOutcome> {
        Err(UsageError::Unsupported)
    }
}
