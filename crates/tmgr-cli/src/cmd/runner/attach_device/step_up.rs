use super::{scope_error, SCOPE};
use tmgr_core::api::DEFAULT_SCOPES;
use tmgr_core::error::Result;

pub const PROMPT: &str = "This needs the chat:devices permission. Sign in again to add it? [Y/n]";

pub struct Seams<'a> {
    pub interactive: bool,
    pub ask: &'a dyn Fn(&str) -> bool,
    pub login: &'a dyn Fn(&[String]) -> Result<()>,
}

pub fn wanted_scopes(have: &[String]) -> Vec<String> {
    let mut scopes: Vec<String> = if have.is_empty() {
        DEFAULT_SCOPES.iter().map(|s| (*s).to_owned()).collect()
    } else {
        have.to_vec()
    };
    scopes.push(SCOPE.to_owned());
    scopes
}

pub fn has_scope(have: &[String]) -> bool {
    have.iter().any(|s| s == SCOPE || s == "*")
}

pub fn pair_after_step_up<T>(
    have: Option<&[String]>,
    seams: &Seams,
    attempt: impl FnOnce() -> Result<T>,
) -> Result<T> {
    if let Some(have) = have.filter(|h| !has_scope(h)) {
        if !seams.interactive || !(seams.ask)(PROMPT) {
            return Err(scope_error());
        }
        (seams.login)(&wanted_scopes(have))?;
    }
    attempt()
}
