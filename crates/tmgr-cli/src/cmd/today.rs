use crate::cli::TodayArgs;
use crate::ctx::Ctx;
use crate::out::Output;
use tmgr_core::error::{Error, Result};
pub fn run(_ctx: &Ctx, _args: &TodayArgs) -> Result<Output> {
    Err(Error::internal("todo"))
}
