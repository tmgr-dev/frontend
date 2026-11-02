use super::ts;
use crate::audit::Audit;
use crate::paths::Paths;
use crate::state::{self, TeamRequest};
use chrono::{DateTime, Utc};
use serde_json::json;
use tmgr_core::error::{Error, Result};
use tmgr_ipc::api::{TaskRef, TeamMode, MAX_TEAM_SIZE};

pub fn request_team(
    paths: &Paths,
    task: TaskRef,
    size: u8,
    mode: TeamMode,
    now: DateTime<Utc>,
) -> Result<()> {
    if !(1..=MAX_TEAM_SIZE).contains(&size) {
        return Err(Error::usage(format!(
            "a team has 1 to {MAX_TEAM_SIZE} workers"
        )));
    }
    let _lock = paths.lock()?;
    let mut st = state::load(paths)?;
    st.team_requests.retain(|r| r.task != task);
    st.team_requests.push(TeamRequest {
        task,
        size,
        mode,
        at: ts(now),
    });
    Audit::new(paths).append(
        "team_requested",
        json!({"task": task.task_id, "size": size, "mode": mode.as_str()}),
    )?;
    state::save(paths, &mut st)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_ipc::api::TeamMode;

    fn task(id: i64) -> TaskRef {
        TaskRef {
            workspace_id: -7,
            task_id: id,
        }
    }

    #[test]
    fn a_new_request_replaces_the_earlier_one_for_that_task_only() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::at(tmp.path());
        let now = Utc::now();
        request_team(&paths, task(1), 2, TeamMode::Eco, now).unwrap();
        request_team(&paths, task(2), 3, TeamMode::Eco, now).unwrap();
        request_team(&paths, task(1), 4, TeamMode::Heavy, now).unwrap();
        let st = state::load(&paths).unwrap();
        assert_eq!(st.team_requests.len(), 2);
        let one = st.team_requests.iter().find(|r| r.task == task(1)).unwrap();
        assert_eq!((one.size, one.mode), (4, TeamMode::Heavy));
    }

    #[test]
    fn a_size_outside_one_to_five_is_refused_and_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::at(tmp.path());
        for size in [0, 6] {
            assert!(request_team(&paths, task(1), size, TeamMode::Eco, Utc::now()).is_err());
        }
        assert!(state::load(&paths).unwrap().team_requests.is_empty());
    }

    #[test]
    fn a_state_without_requests_or_teams_serializes_as_before() {
        let text = serde_json::to_string(&state::State::default()).unwrap();
        assert!(!text.contains("team"));
    }
}
