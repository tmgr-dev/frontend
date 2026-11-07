#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    BadKey,
    BadFormat,
    BadJson,
    UnexpectedKind,
    OpenFailed,
    IdMismatch,
    BadSignature,
    BadMac,
    BadWindow,
    TextTooLong,
    Random,
}

impl Error {
    pub fn code(self) -> &'static str {
        match self {
            Error::BadKey => "bad_key",
            Error::BadFormat => "bad_format",
            Error::BadJson => "bad_json",
            Error::UnexpectedKind => "unexpected_kind",
            Error::OpenFailed => "open_failed",
            Error::IdMismatch => "id_mismatch",
            Error::BadSignature => "bad_signature",
            Error::BadMac => "bad_mac",
            Error::BadWindow => "bad_window",
            Error::TextTooLong => "text_too_long",
            Error::Random => "random",
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for Error {}
