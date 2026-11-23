use std::collections::HashMap;
use std::sync::Mutex;

use tmgr_ipc::chat::ChatMessage;

use crate::api::Request;
use crate::trust::SpawnKey;
use tmgr_spawn_auth::SignedSpawn;

pub trait Transport: Send + Sync {
    fn call(&self, req: &Request, device_token: Option<&str>) -> Result<String, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Identity,
    State,
    Log,
}

impl Slot {
    pub fn name(self) -> &'static str {
        match self {
            Slot::Identity => "identity",
            Slot::State => "state",
            Slot::Log => "log",
        }
    }

    pub const ALL: [Slot; 3] = [Slot::Identity, Slot::State, Slot::Log];
}

pub trait KeyStore: Send + Sync {
    fn load(&self, slot: Slot) -> Result<Option<String>, String>;
    fn save(&self, slot: Slot, value: &str) -> Result<(), String>;
    fn clear(&self) -> Result<(), String>;
}

#[derive(Default)]
pub struct MemKeyStore(Mutex<HashMap<Slot, String>>);

impl KeyStore for MemKeyStore {
    fn load(&self, slot: Slot) -> Result<Option<String>, String> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&slot)
            .cloned())
    }

    fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(slot, value.to_owned());
        Ok(())
    }

    fn clear(&self) -> Result<(), String> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clear();
        Ok(())
    }
}

pub trait ChatSink: Send + Sync {
    fn deliver(
        &self,
        from_device: &str,
        from_kind: Option<&str>,
        msg: ChatMessage,
    ) -> Result<(), String>;
}

/// Runs under the engine lock: implementations must only enqueue (try_send) and never call back into the engine.
pub trait RpcSink: Send + Sync {
    fn deliver_rpc(
        &self,
        from_device: &str,
        from_fp: &str,
        kind: &str,
        msg_id: &str,
        payload: serde_json::Value,
    );
}

/// Runs under the engine lock: implementations must only persist or enqueue and never call back into the engine.
/// The statement is already verified against the sender's pinned keys.
pub trait SpawnKeySink: Send + Sync {
    fn deliver_spawnkey(
        &self,
        from_device: &str,
        from_fp: &str,
        sign_pub_b64: &str,
        kex_pub_b64: &str,
        key: &SpawnKey,
    );
}

/// Runs under the engine lock: implementations must only persist or enqueue and never call back into the engine.
/// The sender is a trusted, joined, non-revoked device other than ourselves and `signed.device_fp == from_fp`;
/// the signature itself is not checked here.
pub trait SpawnOkSink: Send + Sync {
    fn deliver_spawnok(&self, from_device: &str, from_fp: &str, signed: &SignedSpawn);
}

/// Runs under the engine lock: implementations must only persist or enqueue and never call back into the engine.
/// The sender is a trusted, joined, non-revoked device other than ourselves, pinned as a phone.
pub trait ActivitySubSink: Send + Sync {
    fn deliver_activity_on(&self, from_device: &str, on: bool);
}
