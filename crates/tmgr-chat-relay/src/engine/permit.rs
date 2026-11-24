use super::{Core, RelayEngine};
use crate::engine::{ChatSink, KeyStore, Transport};
use crate::wire::Opened;
use tmgr_spawn_auth::{PermitSenderKind, SignedPermit};

impl<T: Transport, K: KeyStore, S: ChatSink> RelayEngine<T, K, S> {
    pub(super) fn accept_permitok(&self, c: &Core, opened: &Opened) -> Option<()> {
        let sink = self.permitok.as_ref()?;
        if self.kind != "runner" {
            return None;
        }
        let own = c.identity.as_ref().map(|i| i.device_id.as_str());
        let dev = c.st.trusted.iter().find(|d| d.id == opened.from)?;
        if Some(dev.id.as_str()) == own
            || dev.fp != opened.from_fp
            || c.st.revoked.contains(&dev.fp)
        {
            return None;
        }
        let kind = match c.st.kind_of(&dev.id, &dev.fp) {
            Some("desktop") => PermitSenderKind::Desktop,
            Some("mobile") => PermitSenderKind::Mobile,
            _ => return None,
        };
        let signed: SignedPermit = serde_json::from_value(opened.payload.clone()).ok()?;
        if signed.device_fp != dev.fp {
            return None;
        }
        sink.deliver_permitok(&dev.id, &dev.fp, kind, &dev.sign_pub, &dev.label, &signed);
        Some(())
    }
}
