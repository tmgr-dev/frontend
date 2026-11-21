use std::collections::HashMap;
use std::sync::Mutex;

use tmgr_ipc::chat::ChatMessage;

use crate::api::Request;

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
    fn deliver(&self, from_device: &str, msg: ChatMessage) -> Result<(), String>;
}
