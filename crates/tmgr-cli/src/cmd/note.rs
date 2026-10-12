use crate::cli::NoteArgs;
use crate::ctx::Ctx;
use crate::out::Output;
use tmgr_core::error::{Error, Result};
pub fn run(_ctx: &Ctx, _a: &NoteArgs) -> Result<Output> {
    Err(Error::internal("todo"))
}
