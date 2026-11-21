use tmgr_core::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum Outcome {
    Approved,
    Denied,
    Unavailable,
}

pub fn authenticate(reason: &str) -> Result<Outcome> {
    imp::authenticate(reason)
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Outcome;
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};
    use std::sync::mpsc;
    use tmgr_core::error::Result;

    pub fn authenticate(reason: &str) -> Result<Outcome> {
        let policy = LAPolicy::DeviceOwnerAuthentication;
        let (tx, rx) = mpsc::channel();
        let reply = RcBlock::new(move |ok: Bool, _err: *mut NSError| {
            let _ = tx.send(ok.as_bool());
        });
        let reason = NSString::from_str(reason);
        // SAFETY: the context outlives the call and the reply block only moves a Sender.
        let started = unsafe {
            let ctx = LAContext::new();
            if ctx.canEvaluatePolicy_error(policy).is_err() {
                return Ok(Outcome::Unavailable);
            }
            ctx.evaluatePolicy_localizedReason_reply(policy, &reason, &reply);
            ctx
        };
        let outcome = match rx.recv() {
            Ok(true) => Outcome::Approved,
            _ => Outcome::Denied,
        };
        drop(started);
        Ok(outcome)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::Outcome;
    use tmgr_core::error::Result;

    pub fn authenticate(_reason: &str) -> Result<Outcome> {
        Ok(Outcome::Unavailable)
    }
}
