use crate::cli::RoutineCommand;
use crate::ctx::Ctx;
use crate::out::Output;
use tmgr_core::error::{Error, Result};
pub fn run(_ctx: &Ctx, _c: &RoutineCommand) -> Result<Output> {
    Err(Error::internal("todo"))
}
