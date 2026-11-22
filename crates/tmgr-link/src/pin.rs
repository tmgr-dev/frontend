use crate::store::{lock, Paths};
use crate::{forbidden, link_err};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::Result;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

pub const MAX_FAILURES: u32 = 5;
pub const MIN_LEN: usize = 12;
const CURRENT_VERSION: u32 = 2;
const NOT_PRIVATE: &str = "pin.toml is not private (readable by others, a link, or not yours); set a new PIN with `tmgr link pin set`";

#[derive(Serialize, Deserialize)]
struct PinFile {
    hash: String,
    #[serde(default)]
    failures: u32,
    #[serde(default = "legacy_version")]
    v: u32,
}

fn legacy_version() -> u32 {
    1
}

fn hasher() -> Result<Argon2<'static>> {
    let params = Params::new(65536, 3, 1, None).map_err(|_| link_err("cannot hash the PIN"))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub set: bool,
    pub failures: u32,
    pub locked: bool,
    pub legacy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Wrong { left: u32 },
    Locked,
    Unset,
    TooShort,
}

#[cfg(unix)]
fn read_private(paths: &Paths) -> std::io::Result<Option<String>> {
    use std::io::Read;
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(paths.pin_file())
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) if e.raw_os_error() == Some(libc::ELOOP) => {
            return Err(std::io::Error::other(NOT_PRIVATE))
        }
        Err(e) => return Err(e),
    };
    let meta = file.metadata()?;
    // SAFETY: geteuid has no preconditions and cannot fail.
    let me = unsafe { libc::geteuid() };
    if !meta.is_file() || meta.uid() != me || meta.mode() & 0o077 != 0 {
        return Err(std::io::Error::other(NOT_PRIVATE));
    }
    let mut text = String::new();
    (&file).read_to_string(&mut text)?;
    Ok(Some(text))
}

#[cfg(not(unix))]
fn read_private(paths: &Paths) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(paths.pin_file()) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn load(paths: &Paths) -> Result<Option<PinFile>> {
    match read_private(paths) {
        Ok(None) => Ok(None),
        Ok(Some(text)) => toml::from_str(&text)
            .map(Some)
            .map_err(|_| link_err("the PIN file is damaged")),
        Err(e) if e.to_string() == NOT_PRIVATE => Err(link_err(NOT_PRIVATE)),
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
    let hash = hasher()?
        .hash_password(pin.as_bytes(), &salt)
        .map_err(|_| link_err("cannot hash the PIN"))?
        .to_string();
    save(
        paths,
        &PinFile {
            hash,
            failures: 0,
            v: CURRENT_VERSION,
        },
    )
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
    Ok(match load(paths)? {
        None => Status {
            set: false,
            failures: 0,
            locked: false,
            legacy: false,
        },
        Some(f) => Status {
            set: true,
            failures: f.failures,
            locked: f.failures >= MAX_FAILURES,
            legacy: f.v < CURRENT_VERSION,
        },
    })
}

pub fn verify_strong(paths: &Paths, pin: &str) -> Result<Verdict> {
    if pin.chars().count() < MIN_LEN {
        return Ok(Verdict::TooShort);
    }
    verify(paths, pin)
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
        assert_eq!(verify(&p, "123456789012").unwrap(), Verdict::Unset);
        assert_eq!(
            status(&p).unwrap(),
            Status {
                set: false,
                failures: 0,
                locked: false,
                legacy: false
            }
        );
    }

    #[test]
    fn a_short_pin_is_refused() {
        let (_d, p) = paths();
        assert!(set(&p, "12345678901").is_err());
        assert!(!p.pin_file().exists());
    }

    #[test]
    fn set_then_verify_accepts_the_pin_and_wrong_tries_count_down() {
        let (_d, p) = paths();
        set(&p, "correct horse battery").unwrap();
        assert_eq!(verify(&p, "correct horse battery").unwrap(), Verdict::Ok);
        assert_eq!(
            verify(&p, "nope-nope-nope-0").unwrap(),
            Verdict::Wrong { left: 4 }
        );
        assert_eq!(
            verify(&p, "nope-nope-nope-0").unwrap(),
            Verdict::Wrong { left: 3 }
        );
        assert_eq!(status(&p).unwrap().failures, 2);
        assert_eq!(verify(&p, "correct horse battery").unwrap(), Verdict::Ok);
        assert_eq!(status(&p).unwrap().failures, 0);
    }

    #[test]
    fn five_wrong_tries_lock_even_for_the_right_pin_until_a_reset() {
        let (_d, p) = paths();
        set(&p, "correct horse battery").unwrap();
        for left in [4, 3, 2, 1] {
            assert_eq!(
                verify(&p, "wrong-one-wrong-1").unwrap(),
                Verdict::Wrong { left }
            );
        }
        assert_eq!(verify(&p, "wrong-one-wrong-1").unwrap(), Verdict::Locked);
        assert_eq!(
            verify(&p, "correct horse battery").unwrap(),
            Verdict::Locked
        );
        let st = status(&p).unwrap();
        assert!(st.set && st.locked);
        reset(&p).unwrap();
        assert!(!status(&p).unwrap().locked);
        assert_eq!(verify(&p, "correct horse battery").unwrap(), Verdict::Ok);
    }

    #[test]
    fn the_file_is_private_and_holds_no_plaintext_pin() {
        let (_d, p) = paths();
        set(&p, "plain-secret-pin-0").unwrap();
        let mode = std::fs::metadata(p.pin_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let text = std::fs::read_to_string(p.pin_file()).unwrap();
        assert!(!text.contains("plain-secret-pin-0"));
        assert!(text.contains("$argon2id$"));
    }

    #[test]
    fn two_sets_of_the_same_pin_use_different_salts() {
        let (_d, p) = paths();
        set(&p, "same-pin-0000001").unwrap();
        let a = std::fs::read_to_string(p.pin_file()).unwrap();
        set(&p, "same-pin-0000001").unwrap();
        assert_ne!(a, std::fs::read_to_string(p.pin_file()).unwrap());
    }

    #[test]
    fn a_damaged_file_fails_closed() {
        let (_d, p) = paths();
        set(&p, "correct horse battery").unwrap();
        std::fs::write(p.pin_file(), "hash = \"junk\"\n").unwrap();
        assert!(verify(&p, "correct horse battery").is_err());
    }

    fn write_file(p: &Paths, mode: u32) {
        std::fs::write(p.pin_file(), "hash = \"x\"\n").unwrap();
        std::fs::set_permissions(p.pin_file(), std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn new_files_use_the_strong_params_and_version_two() {
        let (_d, p) = paths();
        set(&p, "correct horse battery").unwrap();
        let text = std::fs::read_to_string(p.pin_file()).unwrap();
        assert!(text.contains("m=65536,t=3,p=1"), "{text}");
        assert!(text.contains("v = 2"));
        assert!(!status(&p).unwrap().legacy);
    }

    #[test]
    fn an_old_short_pin_file_still_verifies_and_reads_as_legacy() {
        let (_d, p) = paths();
        let salt = SaltString::generate(&mut rand_core::OsRng);
        let hash = Argon2::default()
            .hash_password(b"short1", &salt)
            .unwrap()
            .to_string();
        let guard = guard(&p).unwrap();
        save(
            &p,
            &PinFile {
                hash,
                failures: 0,
                v: 1,
            },
        )
        .unwrap();
        drop(guard);
        let raw = std::fs::read_to_string(p.pin_file())
            .unwrap()
            .replace("v = 1\n", "");
        std::fs::write(p.pin_file(), raw).unwrap();
        assert!(status(&p).unwrap().legacy);
        assert_eq!(verify(&p, "short1").unwrap(), Verdict::Ok);
    }

    #[test]
    fn verify_strong_refuses_a_short_pin_without_counting_a_failure() {
        let (_d, p) = paths();
        set(&p, "correct horse battery").unwrap();
        assert_eq!(verify_strong(&p, "short").unwrap(), Verdict::TooShort);
        assert_eq!(status(&p).unwrap().failures, 0);
        assert_eq!(
            verify_strong(&p, "correct horse battery").unwrap(),
            Verdict::Ok
        );
    }

    #[test]
    fn a_group_readable_file_is_refused_and_set_repairs_it() {
        let (_d, p) = paths();
        std::fs::create_dir_all(p.link_dir()).unwrap();
        write_file(&p, 0o644);
        assert!(status(&p).is_err());
        assert!(verify(&p, "correct horse battery").is_err());
        set(&p, "correct horse battery").unwrap();
        let mode = std::fs::metadata(p.pin_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(verify(&p, "correct horse battery").unwrap(), Verdict::Ok);
    }

    #[test]
    fn a_symlinked_file_is_refused_and_set_replaces_the_link() {
        let (d, p) = paths();
        set(&p, "correct horse battery").unwrap();
        let real = d.path().join("real.toml");
        std::fs::rename(p.pin_file(), &real).unwrap();
        std::os::unix::fs::symlink(&real, p.pin_file()).unwrap();
        assert!(status(&p).is_err());
        set(&p, "another long pin 1").unwrap();
        assert!(!std::fs::symlink_metadata(p.pin_file())
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(verify(&p, "another long pin 1").unwrap(), Verdict::Ok);
    }
}
