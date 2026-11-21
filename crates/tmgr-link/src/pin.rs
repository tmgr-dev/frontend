use crate::store::{lock, Paths};
use crate::{forbidden, link_err};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde::{Deserialize, Serialize};
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::Result;

pub const MAX_FAILURES: u32 = 5;
pub const MIN_LEN: usize = 6;

#[derive(Serialize, Deserialize)]
struct PinFile {
    hash: String,
    #[serde(default)]
    failures: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub set: bool,
    pub failures: u32,
    pub locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Wrong { left: u32 },
    Locked,
    Unset,
}

fn load(paths: &Paths) -> Result<Option<PinFile>> {
    match std::fs::read_to_string(paths.pin_file()) {
        Ok(text) => toml::from_str(&text)
            .map(Some)
            .map_err(|_| link_err("the PIN file is damaged")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(link_err("the PIN file cannot be read")),
    }
}

fn save(paths: &Paths, file: &PinFile) -> Result<()> {
    let text = toml::to_string(file).map_err(|_| link_err("cannot encode the PIN file"))?;
    write_atomic(&paths.pin_file(), text.as_bytes(), 0o600)
}

fn guard(paths: &Paths) -> Result<crate::store::Lock> {
    ensure_private_dir(&paths.config)?;
    ensure_private_dir(&paths.link_dir())?;
    lock(&paths.pin_lock())
}

pub fn set(paths: &Paths, pin: &str) -> Result<()> {
    if pin.chars().count() < MIN_LEN {
        return Err(forbidden(format!(
            "the PIN needs at least {MIN_LEN} characters"
        )));
    }
    let _lock = guard(paths)?;
    let salt = SaltString::generate(&mut rand_core::OsRng);
    let hash = Argon2::default()
        .hash_password(pin.as_bytes(), &salt)
        .map_err(|_| link_err("cannot hash the PIN"))?
        .to_string();
    save(paths, &PinFile { hash, failures: 0 })
}

pub fn reset(paths: &Paths) -> Result<()> {
    let _lock = guard(paths)?;
    if let Some(mut file) = load(paths)? {
        file.failures = 0;
        save(paths, &file)?;
    }
    Ok(())
}

pub fn status(paths: &Paths) -> Result<Status> {
    let _lock = guard(paths)?;
    Ok(match load(paths)? {
        None => Status {
            set: false,
            failures: 0,
            locked: false,
        },
        Some(f) => Status {
            set: true,
            failures: f.failures,
            locked: f.failures >= MAX_FAILURES,
        },
    })
}

pub fn verify(paths: &Paths, pin: &str) -> Result<Verdict> {
    let _lock = guard(paths)?;
    let Some(mut file) = load(paths)? else {
        return Ok(Verdict::Unset);
    };
    if file.failures >= MAX_FAILURES {
        return Ok(Verdict::Locked);
    }
    let parsed = PasswordHash::new(&file.hash).map_err(|_| link_err("the PIN file is damaged"))?;
    match Argon2::default().verify_password(pin.as_bytes(), &parsed) {
        Ok(()) => {
            if file.failures != 0 {
                file.failures = 0;
                save(paths, &file)?;
            }
            Ok(Verdict::Ok)
        }
        Err(argon2::password_hash::Error::Password) => {
            file.failures += 1;
            save(paths, &file)?;
            Ok(if file.failures >= MAX_FAILURES {
                Verdict::Locked
            } else {
                Verdict::Wrong {
                    left: MAX_FAILURES - file.failures,
                }
            })
        }
        Err(_) => Err(link_err("the PIN file is damaged")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn paths() -> (tempfile::TempDir, Paths) {
        let d = crate::testutil::private_tempdir();
        let p = Paths::under(d.path());
        (d, p)
    }

    #[test]
    fn unset_until_a_pin_is_set() {
        let (_d, p) = paths();
        assert_eq!(verify(&p, "123456").unwrap(), Verdict::Unset);
        assert_eq!(
            status(&p).unwrap(),
            Status {
                set: false,
                failures: 0,
                locked: false
            }
        );
    }

    #[test]
    fn a_short_pin_is_refused() {
        let (_d, p) = paths();
        assert!(set(&p, "12345").is_err());
        assert!(!p.pin_file().exists());
    }

    #[test]
    fn set_then_verify_accepts_the_pin_and_wrong_tries_count_down() {
        let (_d, p) = paths();
        set(&p, "correct horse").unwrap();
        assert_eq!(verify(&p, "correct horse").unwrap(), Verdict::Ok);
        assert_eq!(verify(&p, "nope-nope").unwrap(), Verdict::Wrong { left: 4 });
        assert_eq!(verify(&p, "nope-nope").unwrap(), Verdict::Wrong { left: 3 });
        assert_eq!(status(&p).unwrap().failures, 2);
        assert_eq!(verify(&p, "correct horse").unwrap(), Verdict::Ok);
        assert_eq!(status(&p).unwrap().failures, 0);
    }

    #[test]
    fn five_wrong_tries_lock_even_for_the_right_pin_until_a_reset() {
        let (_d, p) = paths();
        set(&p, "correct horse").unwrap();
        for left in [4, 3, 2, 1] {
            assert_eq!(verify(&p, "wrong-one").unwrap(), Verdict::Wrong { left });
        }
        assert_eq!(verify(&p, "wrong-one").unwrap(), Verdict::Locked);
        assert_eq!(verify(&p, "correct horse").unwrap(), Verdict::Locked);
        let st = status(&p).unwrap();
        assert!(st.set && st.locked);
        reset(&p).unwrap();
        assert!(!status(&p).unwrap().locked);
        assert_eq!(verify(&p, "correct horse").unwrap(), Verdict::Ok);
    }

    #[test]
    fn the_file_is_private_and_holds_no_plaintext_pin() {
        let (_d, p) = paths();
        set(&p, "plain-secret-pin").unwrap();
        let mode = std::fs::metadata(p.pin_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let text = std::fs::read_to_string(p.pin_file()).unwrap();
        assert!(!text.contains("plain-secret-pin"));
        assert!(text.contains("$argon2id$"));
    }

    #[test]
    fn two_sets_of_the_same_pin_use_different_salts() {
        let (_d, p) = paths();
        set(&p, "same-pin-1").unwrap();
        let a = std::fs::read_to_string(p.pin_file()).unwrap();
        set(&p, "same-pin-1").unwrap();
        assert_ne!(a, std::fs::read_to_string(p.pin_file()).unwrap());
    }

    #[test]
    fn a_damaged_file_fails_closed() {
        let (_d, p) = paths();
        set(&p, "correct horse").unwrap();
        std::fs::write(p.pin_file(), "hash = \"junk\"\n").unwrap();
        assert!(verify(&p, "correct horse").is_err());
    }
}
