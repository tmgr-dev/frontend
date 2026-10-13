pub mod api;
pub mod proto;

#[cfg(unix)]
pub mod auth;
#[cfg(unix)]
pub mod dirs;
#[cfg(unix)]
pub mod endpoint;
#[cfg(unix)]
pub mod frame;
#[cfg(unix)]
pub mod peer;
#[cfg(unix)]
pub mod secret;
#[cfg(unix)]
pub mod server;

#[cfg(unix)]
pub(crate) fn euid() -> u32 {
    unsafe { libc::geteuid() }
}
