pub mod store;

use tmgr_chat_relay::api::Request;

pub trait RelayTransport: Send + Sync {
    fn call(&self, req: &Request, device_token: Option<&str>) -> Result<String, String>;
}
