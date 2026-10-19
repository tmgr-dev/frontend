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
    Audit::new(paths).append("disable", fields)
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
}
