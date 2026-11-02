use std::path::{Path, PathBuf};
use tmgr_core::config::{config_dir, Config};
use tmgr_core::error::{Error, Result};
use tmgr_ipc::api::RunCreateReq;
use tmgr_runner::sanitize::sanitize_text;
use tmgr_runner::sessionwt::{self, Verdict};

fn clean(s: &str) -> String {
    sanitize_text(s).into_owned()
}

pub(super) fn config_on() -> bool {
    config_dir()
        .ok()
        .and_then(|d| Config::load(&d).ok())
        .is_some_and(|c| c.attach_worktree())
}

pub(super) fn wanted(flag: bool, off: bool) -> bool {
    flag || (!off && config_on())
}

pub(super) fn start(
    req: &mut RunCreateReq,
    taken: &[String],
) -> std::result::Result<sessionwt::SessionClone, String> {
    let sc = sessionwt::create(
        Path::new(&req.cwd),
        req.name.as_deref(),
        &req.profile,
        taken,
    )?;
    req.cwd = sc.path.display().to_string();
    req.name = Some(sc.name.clone());
    req.clone_of = Some(sc.repo.display().to_string());
    req.trust_via_repo = Some(true);
    Ok(sc)
}

pub(super) fn offer_removal(cwd: &str, assume_tty: bool) -> Result<Option<String>> {
    let path = PathBuf::from(cwd);
    let Some(repo) = sessionwt::repo_of(&path) else {
        return Ok(None);
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match sessionwt::assess(&repo, &path).map_err(Error::usage)? {
        Verdict::Kept(why) => Ok(Some(format!(
            "worktree {} kept: {}",
            clean(&name),
            clean(&why)
        ))),
        Verdict::Removable { branch } if !assume_tty => Ok(Some(format!(
            "worktree {} is clean and can be deleted (branch {} is kept); run this in a terminal to be asked",
            clean(&name),
            clean(&branch)
        ))),
        Verdict::Removable { branch } => {
            let q = format!(
                "Delete the worktree {} (branch {} is kept)?",
                clean(&path.display().to_string()),
                clean(&branch)
            );
            if crate::interact::confirm(&q, false, false).is_err() {
                return Ok(Some(format!("worktree {} kept", clean(&name))));
            }
            match sessionwt::remove(&repo, &path).map_err(Error::usage)? {
                Verdict::Removable { .. } => Ok(Some(format!("worktree {} deleted", clean(&name)))),
                Verdict::Kept(why) => Ok(Some(format!(
                    "worktree {} kept: {}",
                    clean(&name),
                    clean(&why)
                ))),
            }
        }
    }
}
