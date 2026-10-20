use crate::audit::Audit;
use crate::paths::Paths;
use crate::rules;
use serde_json::json;
use tmgr_core::error::Result;

/// Turns the queue off through the audited path `tmgr queue disable` uses.
pub fn disable(paths: &Paths, reason: Option<&str>) -> Result<()> {
    let _lock = paths.lock()?;
    let mut file = rules::load(paths)?.file;
    file.enabled = false;
    let sha = rules::save(paths, &file)?;
    let mut fields = json!({ "config_sha256": sha });
    if let Some(reason) = reason {
        fields["reason"] = json!(reason);
    }
    Audit::new(paths).append("disable", fields)?;
    forget_queue_loop(paths);
    Ok(())
}

pub fn forget_queue_loop(paths: &Paths) {
    let _ = std::fs::remove_file(paths.dir.join("autopilot/settings/queue.json"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disable_switches_the_queue_off_and_audits_the_reason() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::at(d.path());
        let mut file = rules::load(&paths).unwrap().file;
        file.enabled = true;
        rules::save(&paths, &file).unwrap();

        disable(&paths, Some("autopilot crashed")).unwrap();

        assert!(!rules::load(&paths).unwrap().file.enabled);
        let events = Audit::new(&paths).events(10).unwrap();
        let last = events.last().unwrap();
        assert_eq!(last["event"], "disable");
        assert_eq!(last["reason"], "autopilot crashed");
        assert!(Audit::new(&paths).verify().unwrap().is_ok());
    }

    #[test]
    fn disable_removes_the_saved_queue_loop_settings_and_only_those() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::at(d.path());
        let dir = d.path().join("autopilot/settings");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("queue.json"), "{}").unwrap();
        std::fs::write(dir.join("ap-1.json"), "{}").unwrap();
        disable(&paths, None).unwrap();
        assert!(!dir.join("queue.json").exists());
        assert!(dir.join("ap-1.json").exists());
    }
}
