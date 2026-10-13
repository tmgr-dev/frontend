use tmgr_ipc::api::TaskRef;

pub trait TaskResolver {
    fn resolve(&self, reference: &str) -> Result<TaskRef, String>;
    fn content_hash(&self, task: &TaskRef) -> Option<String>;
}

pub struct LocalRefs;

impl TaskResolver for LocalRefs {
    fn resolve(&self, reference: &str) -> Result<TaskRef, String> {
        parse_local(reference).ok_or_else(|| {
            "the task must be a task of this machine's local workspace, written w<workspace>:<task>"
                .to_string()
        })
    }

    fn content_hash(&self, _task: &TaskRef) -> Option<String> {
        None
    }
}

pub fn parse_local(reference: &str) -> Option<TaskRef> {
    let rest = reference.strip_prefix('w')?;
    let (ws, id) = rest.split_once(':')?;
    let number = |s: &str| {
        (1..=9).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()) && !s.starts_with('0')
    };
    if !number(ws) || !number(id) {
        return None;
    }
    Some(TaskRef {
        workspace_id: -ws.parse::<i64>().ok()?,
        task_id: id.parse().ok()?,
    })
}

pub fn format_ref(task: &TaskRef) -> String {
    if task.workspace_id < 0 {
        format!("w{}:{}", -task.workspace_id, task.task_id)
    } else {
        format!("c{}:{}", task.workspace_id, task.task_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_refs_round_trip_and_are_always_negative_workspaces() {
        let t = parse_local("w3:412").unwrap();
        assert_eq!((t.workspace_id, t.task_id), (-3, 412));
        assert_eq!(format_ref(&t), "w3:412");
    }

    #[test]
    fn cloud_looking_and_malformed_refs_are_refused() {
        for bad in [
            "TM-512",
            "#8974",
            "412",
            "w0:1",
            "w1:0",
            "w-3:4",
            "w3:",
            "w:4",
            "w3:4:5",
            "W3:4",
            "w03:4",
            "w3:-4",
            "w1234567890:1",
            "w3:1234567890",
            "",
            "w3 :4",
        ] {
            assert!(parse_local(bad).is_none(), "{bad}");
            assert!(LocalRefs.resolve(bad).is_err());
        }
    }

    #[test]
    fn the_stub_cannot_vouch_for_content() {
        let t = parse_local("w1:1").unwrap();
        assert!(LocalRefs.content_hash(&t).is_none());
    }
}
