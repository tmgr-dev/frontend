use crate::proto::{self, Request};
use crate::scope::Scope;
use crate::store::Store;
use crate::worker::{Conn, Step};
use serde_json::{json, Value};

pub const METHOD: &str = "chat.enroll";
const MAX_CODE_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub id: String,
    pub device_id: String,
    pub fp: String,
}

pub trait Enroller {
    fn enroll(&self, code: &str, leader_machine: &str) -> Result<Identity, String>;
}

fn audit_down() -> Step {
    Step::Close(proto::error(None, "audit_unavailable", "request refused"))
}

fn params(p: &Value) -> Result<(&str, &str), &'static str> {
    let obj = p.as_object().ok_or("params must be an object")?;
    if obj.keys().any(|k| k != "code" && k != "leader_machine") {
        return Err("unknown parameter");
    }
    let code = obj
        .get("code")
        .and_then(Value::as_str)
        .ok_or("code must be a string")?;
    let machine = obj
        .get("leader_machine")
        .and_then(Value::as_str)
        .ok_or("leader_machine must be a string")?;
    if code.is_empty() || code.len() > MAX_CODE_BYTES || code.chars().any(char::is_whitespace) {
        return Err("code is not valid");
    }
    if !tmgr_ipc::mgr_rpc::valid_machine_id(machine) {
        return Err("leader_machine is not a machine id");
    }
    Ok((code, machine))
}

fn bound_leader(conn: &Conn) -> tmgr_core::error::Result<Option<String>> {
    let store = Store::load(conn.paths)?;
    Ok(store
        .leaders
        .get(conn.name)
        .filter(|l| l.enrollment == conn.enrollment)
        .and_then(|l| l.chat_leader_machine.clone()))
}

fn bind_leader(conn: &Conn, machine: &str) -> tmgr_core::error::Result<()> {
    Store::update(conn.paths, |s| {
        if let Some(l) = s
            .leaders
            .get_mut(conn.name)
            .filter(|l| l.enrollment == conn.enrollment)
        {
            l.chat_leader_machine
                .get_or_insert_with(|| machine.to_string());
        }
        Ok(())
    })
}

pub(crate) fn chat_enroll(conn: &Conn, req: &Request, scope: &Scope) -> Step {
    let id = Some(req.id);
    let deny = |args: Value, code: &str, reason: &str| {
        if conn.record(METHOD, args, "rejected", reason).is_err() {
            return audit_down();
        }
        Step::Reply(proto::error(id, code, reason))
    };
    if !scope.chat_enroll {
        return deny(
            json!({}),
            "forbidden",
            "this link's scope does not allow chat enrolment",
        );
    }
    let (code, machine) = match params(&req.params) {
        Ok(v) => v,
        Err(why) => return deny(json!({}), "bad_params", why),
    };
    let args = json!({"leader_machine": machine});
    let Some(enroller) = conn.rt.enroll else {
        return deny(args, "not_available", "chat enrolment is not available");
    };
    match bound_leader(conn) {
        Ok(Some(b)) if b != machine => {
            return deny(
                args,
                "forbidden",
                "this link is bound to a different leader machine",
            )
        }
        Ok(_) => {}
        Err(_) => return deny(args, "refused", "link state is unavailable"),
    }
    if conn.record(METHOD, args.clone(), "accepted", "").is_err() {
        return audit_down();
    }
    match enroller.enroll(code, machine) {
        Ok(i) => {
            if bind_leader(conn, machine).is_err() {
                return deny(args, "refused", "link state is unavailable");
            }
            let done = json!({"leader_machine": machine, "machine": i.id});
            if conn.note("worker.chat_enrolled", done).is_err() {
                return audit_down();
            }
            Step::Reply(proto::ok(
                id,
                json!({"id": i.id, "device_id": i.device_id, "fp": i.fp}),
            ))
        }
        Err(why) => {
            let reason = tmgr_core::text::truncate_chars(&why, 200).0;
            if conn.record(METHOD, args, "failed", &reason).is_err() {
                return audit_down();
            }
            Step::Reply(proto::error(id, "refused", &reason))
        }
    }
}
