use crate::cli::{SelfUpdateArgs, UpdateChannel, VERSION};
use crate::ctx::Ctx;
use crate::interact;
use crate::out::Output;
use tmgr_core::active::state_dir;
use tmgr_core::error::{Error, Result};
use tmgr_core::update::{self, Channel, Options};

const API_ENV: &str = "TMGR_UPDATE_API_URL";

pub fn run(ctx: &Ctx, args: &SelfUpdateArgs) -> Result<Output> {
    let key = update::embedded_key()?;
    if args.allow_downgrade {
        interact::confirm("Allow installing an older version?", false, ctx.yes)?;
    }
    let state = state_dir().ok_or_else(|| Error::internal("cannot locate the state directory"))?;
    let exe =
        std::env::current_exe().map_err(|_| Error::internal("cannot locate the running binary"))?;
    let current = update::Version::parse(VERSION)
        .map_err(|_| Error::internal("the running version is not valid semver"))?;
    let opts = Options {
        api_base: std::env::var(API_ENV).unwrap_or_else(|_| update::DEFAULT_API.to_string()),
        channel: match args.channel {
            UpdateChannel::Stable => Channel::Stable,
            UpdateChannel::Beta => Channel::Beta,
        },
        check_only: args.check,
        allow_downgrade: args.allow_downgrade,
        current,
        target: env!("TMGR_TARGET").to_string(),
        exe,
        state_dir: state,
        now: chrono::Utc::now(),
        user_agent: Ctx::user_agent(),
    };
    let report = update::run(&opts, &key)?;
    let human = if report.installed {
        format!(
            "Updated tmgr {} -> {}. The previous binary is kept as {}.",
            report.current,
            report.latest.as_deref().unwrap_or("?"),
            report.backup.as_deref().unwrap_or("tmgr.old")
        )
    } else if report.update_available {
        format!(
            "tmgr {} is available (you have {}). Run `tmgr self-update`.",
            report.latest.as_deref().unwrap_or("?"),
            report.current
        )
    } else {
        format!("tmgr {} is up to date.", report.current)
    };
    Ok(Output::new(report, human))
}
