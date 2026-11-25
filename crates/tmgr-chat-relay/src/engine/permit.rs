use super::{Core, RelayEngine};
use crate::engine::{ChatSink, KeyStore, Transport};
use crate::wire::Opened;
use tmgr_ipc::chat::PollAnswer;
use tmgr_spawn_auth::{PermitSenderKind, SignedPermit};

const REFRESH_FIELD_MAX: usize = 128;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Refresh {
    v: u8,
    refresh: String,
    nonce: String,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum PermitOk {
    Refresh(Refresh),
    Signed(Box<SignedPermit>),
    Poll(PollAnswer),
}

impl<T: Transport, K: KeyStore, S: ChatSink> RelayEngine<T, K, S> {
    pub(super) fn accept_permitok(&self, c: &Core, opened: &Opened) -> Option<()> {
        if self.kind != "runner" || (self.permitok.is_none() && self.poll.is_none()) {
            return None;
        }
        let own = c.identity.as_ref().map(|i| i.device_id.as_str());
        let dev = c.st.trusted.iter().find(|d| d.id == opened.from)?;
        if Some(dev.id.as_str()) == own
            || dev.fp != opened.from_fp
            || c.st.revoked.contains(&dev.fp)
            || c.st.pending_revokes.iter().any(|p| p.target_fp == dev.fp)
        {
            return None;
        }
        let kind = match c.st.kind_of(&dev.id, &dev.fp) {
            Some("desktop") => PermitSenderKind::Desktop,
            Some("mobile") => PermitSenderKind::Mobile,
            _ => return None,
        };
        match serde_json::from_value(opened.payload.clone()).ok()? {
            PermitOk::Refresh(r) => {
                let sink = self.permitok.as_ref()?;
                if r.v != 1
                    || r.refresh.len() > REFRESH_FIELD_MAX
                    || r.nonce.len() > REFRESH_FIELD_MAX
                {
                    return None;
                }
                sink.deliver_permit_refresh(&dev.id, &dev.fp, kind, &r.refresh, &r.nonce);
            }
            PermitOk::Signed(signed) => {
                let sink = self.permitok.as_ref()?;
                if signed.device_fp != dev.fp {
                    return None;
                }
                sink.deliver_permitok(&dev.id, &dev.fp, kind, &dev.sign_pub, &dev.label, &signed);
            }
            PermitOk::Poll(answer) => {
                let sink = self.poll.as_ref()?;
                sink.deliver_poll(&dev.id, &dev.fp, kind, &answer);
            }
        }
        Some(())
    }
}
