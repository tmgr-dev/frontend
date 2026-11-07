mod error;
mod proto;
mod seal;
mod sign;
mod util;

pub use error::Error;
pub use proto::*;
pub use seal::{ctx, open, seal, seal_with, Dir, Entropy, Kind, Opened, Sealed};
pub use sign::{public_key, sign, verify};
pub use util::{b64, b64_decode, h, hex};

pub type Result<T> = std::result::Result<T, Error>;
