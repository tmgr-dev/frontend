use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Expired,
    ClockSkew,
    BadTtl,
    UnknownKey,
    NotTrusted,
    Mismatch,
    BadToken,
    BadSig,
    UnsupportedVersion,
    UnsupportedAlg,
}

impl Refusal {
    pub fn counts_as_attempt(self) -> bool {
        self == Refusal::BadSig
    }

    pub fn code(self) -> &'static str {
        match self {
            Refusal::Expired => "expired",
            Refusal::ClockSkew => "clock_skew",
            Refusal::BadTtl => "bad_ttl",
            Refusal::UnknownKey => "unknown_key",
            Refusal::NotTrusted => "not_trusted",
            Refusal::Mismatch => "mismatch",
            Refusal::BadToken => "bad_token",
            Refusal::BadSig => "bad_sig",
            Refusal::UnsupportedVersion => "unsupported_version",
            Refusal::UnsupportedAlg => "unsupported_alg",
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Refusal::Expired => "spawn approval expired",
            Refusal::ClockSkew => "spawn approval timestamp is outside the allowed clock skew",
            Refusal::BadTtl => "spawn challenge lifetime is too long",
            Refusal::UnknownKey => "no usable pinned spawn key",
            Refusal::NotTrusted => "approving device is not trusted",
            Refusal::Mismatch => "spawn approval does not match the pending challenge",
            Refusal::BadToken => "spawn approval field has an invalid format",
            Refusal::BadSig => "spawn approval signature is invalid",
            Refusal::UnsupportedVersion => "unsupported spawn approval version",
            Refusal::UnsupportedAlg => "unsupported spawn approval algorithm",
        };
        f.write_str(text)
    }
}

impl std::error::Error for Refusal {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bad_sig_counts_as_attempt() {
        for r in [
            Refusal::Expired,
            Refusal::ClockSkew,
            Refusal::BadTtl,
            Refusal::UnknownKey,
            Refusal::NotTrusted,
            Refusal::Mismatch,
            Refusal::BadToken,
            Refusal::UnsupportedVersion,
            Refusal::UnsupportedAlg,
        ] {
            assert!(!r.counts_as_attempt(), "{r:?}");
        }
        assert!(Refusal::BadSig.counts_as_attempt());
    }
}
