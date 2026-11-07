use super::team::{
    marker_present, restart_if_future, retry_due, secs_since, EXIT_RETRY_SECS, EXIT_TEXT,
    MAX_EXIT_ATTEMPTS,
};
use super::{ts, Deps, Pass};
use crate::state::{self, Status};
use serde_json::json;
use std::path::Path;
use tmgr_core::error::Result;

pub(super) fn solo_exit_phase(pass: &mut Pass, deps: &Deps) -> Result<()> {
    for i in 0..pass.state.pickups.len() {
        let p = &pass.state.pickups[i];
        if !p.finish_marker || p.status != Status::Started || p.run_exited || p.team.is_some() {
            continue;
        }
        let Some(run) = p.run_id.clone() else {
            continue;
        };
        if !marker_present(Path::new(&p.bundle.launch.cwd)) {
            continue;
        }
        let id = p.id.clone();
        let was = (p.exit_attempts, p.exit_sent_at.clone(), p.exit_noted);
        let (mut attempts, mut sent_at, mut noted) = was.clone();
        restart_if_future(&mut sent_at, deps.now);
        if retry_due(attempts, sent_at.as_deref(), deps.now) {
            if deps.runs.idle(&run) && deps.runs.send(&run, EXIT_TEXT).is_ok() {
                attempts += 1;
                sent_at = Some(ts(deps.now));
                pass.audit
                    .append("run_exit_sent", json!({"pickup": id, "run": run}))?;
            }
        } else if attempts >= MAX_EXIT_ATTEMPTS
            && !noted
            && secs_since(sent_at.as_deref(), deps.now) >= EXIT_RETRY_SECS
        {
            noted = true;
            pass.audit
                .append("run_exit_unanswered", json!({"pickup": id, "run": run}))?;
        }
        if (attempts, sent_at.clone(), noted) == was {
            continue;
        }
        let p = &mut pass.state.pickups[i];
        p.exit_attempts = attempts;
        p.exit_sent_at = sent_at;
        p.exit_noted = noted;
        state::save(pass.paths, &mut pass.state)?;
    }
    Ok(())
}
