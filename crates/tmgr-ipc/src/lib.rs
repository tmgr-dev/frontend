#![cfg(unix)]

pub mod auth;
pub mod dirs;
pub mod endpoint;
pub mod frame;
pub mod peer;
pub mod proto;
pub mod secret;
pub mod server;

pub(crate) fn euid() -> u32 {
    unsafe { libc::geteuid() }
}
