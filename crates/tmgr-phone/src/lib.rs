mod error;
mod proto;
mod seal;
mod sign;
mod util;

pub use error::Error;
pub use proto::*;
pub use seal::{
    ctx, open, open_ctx, seal, seal_ctx_with, seal_with, Dir, Entropy, Kind, Opened, Sealed,
};
pub use sign::{public_key, sign, verify, verify_der};
pub use util::{b64, b64_decode, h, hex};

pub type Result<T> = std::result::Result<T, Error>;
