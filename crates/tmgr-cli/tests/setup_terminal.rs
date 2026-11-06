use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct Box {
    tmp: TempDir,
}

impl Box {
    fn new() -> Box {
        let b = Box {
            tmp: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir_all(b.apps()).unwrap();
        b
    }

    fn home(&self) -> &Path {
        self.tmp.path()
    }

    fn apps(&self) -> PathBuf {
        self.tmp.path().join("apps")
    }

    fn iterm_domain(&self) -> PathBuf {
        self.tmp.path().join("iterm").join("com.googlecode.iterm2")
    }

    fn write(&self, rel: &str, body: &str) -> PathBuf {
        let p = self.home().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
        p
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_tmgr"));
        c.env_clear()
            .env("HOME", self.home())
            .env("XDG_CONFIG_HOME", self.home().join("xdg"))
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("TMGR_SETUP_APP_ROOTS", self.apps())
            .env("TMGR_SETUP_ITERM_DOMAIN", self.iterm_domain())
            .env("NO_COLOR", "1")
            .args(["setup", "terminal"])
            .args(args);
        c
    }

    fn json(&self, args: &[&str]) -> Value {
        let mut a = vec!["--json"];
        a.extend_from_slice(args);
        let out = self.cmd(&a).assert().success();
        serde_json::from_slice(&out.get_output().stdout).unwrap()
    }

    fn entry(&self, v: &Value, app: &str) -> Value {
        v["data"]["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["app"] == app)
            .unwrap_or_else(|| panic!("no entry for {app} in {v}"))
            .clone()
    }

    fn backups(&self, dir: &str) -> usize {
        std::fs::read_dir(self.home().join(dir))
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("tmgr-backup")
            })
            .count()
    }
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap()
}

#[test]
fn nothing_installed_reports_nothing_to_do() {
    let b = Box::new();
    let v = b.json(&["--yes"]);
    assert!(v["data"]["apps"].as_array().unwrap().is_empty());
}

#[test]
fn warp_gets_write_only_with_backup_and_second_run_is_already_ok() {
    let b = Box::new();
    let f = b.write(".warp/settings.toml", "[terminal]\ncopy_on_select = true\n");
    let v = b.json(&["--yes"]);
    let e = b.entry(&v, "Warp");
    assert_eq!(e["status"], "changed");
    assert_eq!(e["restart"], true);
    let body = read(&f);
    assert!(body.contains("osc52_clipboard_access = \"write_only\""));
    assert!(body.contains("copy_on_select = true"));
    assert_eq!(b.backups(".warp"), 1);
    let again = b.json(&["--yes"]);
    assert_eq!(b.entry(&again, "Warp")["status"], "already_ok");
    assert_eq!(b.backups(".warp"), 1);
    assert_eq!(read(&f), body);
}

#[test]
fn warposs_creates_the_settings_file_when_only_the_folder_exists() {
    let b = Box::new();
    std::fs::create_dir_all(b.home().join(".warp-oss")).unwrap();
    let v = b.json(&["--yes"]);
    assert_eq!(b.entry(&v, "WarpOss")["status"], "changed");
    let body = read(&b.home().join(".warp-oss/settings.toml"));
    assert!(body.contains("[terminal]"));
    assert!(body.contains("osc52_clipboard_access = \"write_only\""));
}

#[test]
fn warp_read_write_is_left_alone() {
    let b = Box::new();
    let f = b.write(
        ".warp/settings.toml",
        "[terminal]\nosc52_clipboard_access = \"read_write\"\n",
    );
    let v = b.json(&["--yes"]);
    assert_eq!(b.entry(&v, "Warp")["status"], "already_ok");
    assert!(read(&f).contains("read_write"));
    assert_eq!(b.backups(".warp"), 0);
}

#[test]
fn ghostty_deny_becomes_allow_and_the_default_needs_no_change() {
    let b = Box::new();
    let f = b.write(
        "xdg/ghostty/config",
        "font-size = 14\nclipboard-write = deny\ntheme = dark\n",
    );
    let v = b.json(&["--yes"]);
    assert_eq!(b.entry(&v, "Ghostty")["status"], "changed");
    assert_eq!(
        read(&f),
        "font-size = 14\nclipboard-write = allow\ntheme = dark\n"
    );
    let again = b.json(&["--yes"]);
    assert_eq!(b.entry(&again, "Ghostty")["status"], "already_ok");

    let plain = Box::new();
    plain.write("xdg/ghostty/config", "font-size = 14\n");
    let v = plain.json(&["--yes"]);
    assert_eq!(plain.entry(&v, "Ghostty")["status"], "already_ok");
    assert_eq!(
        read(&plain.home().join("xdg/ghostty/config")),
        "font-size = 14\n"
    );
}

#[test]
fn kitty_without_write_clipboard_gets_only_that_token_added() {
    let b = Box::new();
    let f = b.write(
        "xdg/kitty/kitty.conf",
        "font_size 12\nclipboard_control read-clipboard-ask\n",
    );
    let v = b.json(&["--yes"]);
    assert_eq!(b.entry(&v, "kitty")["status"], "changed");
    assert_eq!(
        read(&f),
        "font_size 12\nclipboard_control read-clipboard-ask write-clipboard\n"
    );
    let again = b.json(&["--yes"]);
    assert_eq!(b.entry(&again, "kitty")["status"], "already_ok");
}

#[test]
fn kitty_never_gains_a_read_permission() {
    let b = Box::new();
    let f = b.write("xdg/kitty/kitty.conf", "clipboard_control no-append\n");
    b.json(&["--yes"]);
    let body = read(&f);
    assert!(body.contains("write-clipboard"));
    assert!(!body.contains("read-clipboard"));
    assert!(!body.contains("read-primary"));
}

#[test]
fn alacritty_disabled_osc52_becomes_only_copy() {
    let b = Box::new();
    let f = b.write(
        "xdg/alacritty/alacritty.toml",
        "[terminal]\nosc52 = \"Disabled\"\n",
    );
    let v = b.json(&["--yes"]);
    assert_eq!(b.entry(&v, "Alacritty")["status"], "changed");
    assert!(read(&f).contains("osc52 = \"OnlyCopy\""));
    let again = b.json(&["--yes"]);
    assert_eq!(b.entry(&again, "Alacritty")["status"], "already_ok");
}

#[test]
fn wezterm_is_already_ok_without_touching_anything() {
    let b = Box::new();
    std::fs::create_dir_all(b.apps().join("WezTerm.app")).unwrap();
    let v = b.json(&["--yes"]);
    assert_eq!(b.entry(&v, "WezTerm")["status"], "already_ok");
}

#[test]
fn tmux_gets_set_clipboard_on_appended_once() {
    let b = Box::new();
    let f = b.write(".tmux.conf", "set -g mouse on");
    let v = b.json(&["--yes"]);
    assert_eq!(b.entry(&v, "tmux")["status"], "changed");
    assert_eq!(read(&f), "set -g mouse on\nset -s set-clipboard on\n");
    let again = b.json(&["--yes"]);
    assert_eq!(b.entry(&again, "tmux")["status"], "already_ok");
    assert_eq!(read(&f), "set -g mouse on\nset -s set-clipboard on\n");
}

#[test]
fn without_yes_and_without_a_terminal_nothing_changes() {
    let b = Box::new();
    let f = b.write(".warp/settings.toml", "[terminal]\n");
    let v = b.json(&[]);
    assert_eq!(b.entry(&v, "Warp")["status"], "would_change");
    assert_eq!(read(&f), "[terminal]\n");
    assert_eq!(b.backups(".warp"), 0);
}

#[test]
fn check_never_changes_anything_even_with_yes() {
    let b = Box::new();
    let f = b.write(".warp/settings.toml", "[terminal]\n");
    let v = b.json(&["--check", "--yes"]);
    assert_eq!(b.entry(&v, "Warp")["status"], "would_change");
    assert_eq!(read(&f), "[terminal]\n");
}

#[test]
fn human_output_shows_the_exact_line_and_names_apps_to_restart() {
    let b = Box::new();
    b.write(".warp/settings.toml", "[terminal]\n");
    let out = b.cmd(&["--yes"]).assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(
        text.contains("osc52_clipboard_access = \"write_only\""),
        "{text}"
    );
    assert!(text.contains("Restart Warp"), "{text}");
}

#[cfg(target_os = "macos")]
mod iterm {
    use super::*;

    fn defaults(args: &[&str]) -> String {
        let out = std::process::Command::new("/usr/bin/defaults")
            .args(args)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn iterm2_clipboard_access_is_switched_on_in_the_injected_domain_only() {
        let b = Box::new();
        let domain = b.iterm_domain();
        std::fs::create_dir_all(domain.parent().unwrap()).unwrap();
        let d = domain.to_str().unwrap();
        defaults(&["write", d, "AllowClipboardAccess", "-bool", "false"]);
        assert_eq!(defaults(&["read", d, "AllowClipboardAccess"]), "0");

        let v = b.json(&["--yes"]);
        assert_eq!(b.entry(&v, "iTerm2")["status"], "changed");
        assert_eq!(defaults(&["read", d, "AllowClipboardAccess"]), "1");
        let backups = std::fs::read_dir(domain.parent().unwrap())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("tmgr-backup")
            })
            .count();
        assert_eq!(backups, 1);

        let again = b.json(&["--yes"]);
        assert_eq!(b.entry(&again, "iTerm2")["status"], "already_ok");
    }
}
