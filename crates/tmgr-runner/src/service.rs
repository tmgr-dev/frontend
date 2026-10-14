use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub const LABEL: &str = "dev.tmgr.runner";
pub const UNIT: &str = "tmgr-runner.service";
pub const LAUNCHCTL: &str = "/bin/launchctl";
pub const SYSTEMCTL: &str = "/usr/bin/systemctl";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Launchd,
    Systemd,
}

impl Platform {
    pub fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::Launchd
        } else {
            Platform::Systemd
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Platform::Launchd => "launchd",
            Platform::Systemd => "systemd",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exists {
    No,
    Same,
    Different,
}

#[derive(Clone, Debug)]
pub struct ServicePaths {
    pub platform: Platform,
    pub file: PathBuf,
    pub uid: u32,
}

impl ServicePaths {
    pub fn new(platform: Platform, home: &Path, xdg_config_home: Option<&Path>, uid: u32) -> Self {
        let file = match platform {
            Platform::Launchd => home
                .join("Library/LaunchAgents")
                .join(format!("{LABEL}.plist")),
            Platform::Systemd => xdg_config_home
                .filter(|p| p.is_absolute())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| home.join(".config"))
                .join("systemd/user")
                .join(UNIT),
        };
        ServicePaths {
            platform,
            file,
            uid,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub platform: Platform,
    pub path: PathBuf,
    pub content: String,
    pub existing: Exists,
    pub commands: Vec<Vec<String>>,
    pub note: Option<String>,
}

pub struct RunOut {
    pub ok: bool,
    pub stderr: String,
}

pub trait Sys {
    fn run(&mut self, argv: &[String]) -> io::Result<RunOut>;
}

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, msg.into())
}

fn check_text(what: &str, s: &str) -> io::Result<()> {
    if s.chars().any(char::is_control) {
        return Err(invalid(format!(
            "{what} contains a control character or newline"
        )));
    }
    Ok(())
}

fn xml(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c => o.push(c),
        }
    }
    o
}

pub fn plist(exe: &str, path_env: &str, log: &str) -> io::Result<String> {
    check_text("executable path", exe)?;
    check_text("PATH", path_env)?;
    check_text("log path", log)?;
    let (exe, path_env, log) = (xml(exe), xml(path_env), xml(log));
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>runner</string>
    <string>serve</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>StandardOutPath</key>
  <string>{log}</string>
  <key>StandardErrorPath</key>
  <string>{log}</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>{path_env}</string>
  </dict>
</dict>
</plist>
"#
    ))
}

fn systemd_arg(s: &str) -> String {
    let s = s.replace('%', "%%").replace('$', "$$");
    if s.is_empty() || s.contains(|c: char| c.is_whitespace() || c == '"' || c == '\\') {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s
    }
}

pub fn unit(exe: &str, path_env: &str) -> io::Result<String> {
    check_text("executable path", exe)?;
    check_text("PATH", path_env)?;
    let env = format!("PATH={}", path_env.replace('%', "%%"))
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    Ok(format!(
        "[Unit]\nDescription=tmgr runner ({LABEL})\n\n[Service]\nType=simple\nExecStart={} runner serve\nEnvironment=\"{env}\"\nRestart=no\n\n[Install]\nWantedBy=default.target\n",
        systemd_arg(exe)
    ))
}

pub fn capture_path(raw: &str) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for p in raw.split(':') {
        if p.starts_with('/') && !p.chars().any(char::is_control) && !seen.contains(&p) {
            seen.push(p);
        }
    }
    if seen.is_empty() {
        "/usr/bin:/bin".into()
    } else {
        seen.join(":")
    }
}

pub fn exe_warnings(exe: &Path, uid: u32) -> Vec<String> {
    use std::os::unix::fs::MetadataExt;
    let s = exe.display().to_string();
    let mut w = Vec::new();
    let comps: Vec<_> = exe.components().map(|c| c.as_os_str().to_owned()).collect();
    let has = |n: &str| comps.iter().any(|c| c == n);
    if has("target") && (has("debug") || has("release")) {
        w.push(format!(
            "{s} is inside a cargo target directory and may be rebuilt or removed"
        ));
    }
    if has(".nvm") {
        w.push(format!(
            "{s} is inside .nvm and moves when the node version changes"
        ));
    }
    let tmp = std::env::temp_dir();
    if exe.starts_with(&tmp)
        || exe.starts_with("/tmp")
        || exe.starts_with("/private/tmp")
        || exe.starts_with("/var/folders")
    {
        w.push(format!("{s} is in a temporary directory"));
    }
    if let Ok(m) = fs::metadata(exe) {
        if m.uid() != uid {
            w.push(format!("{s} is not owned by you"));
        }
    }
    w
}

fn read_existing(path: &Path) -> io::Result<Option<String>> {
    match fs::read(path) {
        Ok(b) => Ok(Some(String::from_utf8_lossy(&b).into_owned())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).to_string()).collect()
}

pub fn plan_install(
    paths: &ServicePaths,
    exe: &str,
    path_env: &str,
    log: &str,
) -> io::Result<Plan> {
    let path_env = capture_path(path_env);
    let file = paths.file.display().to_string();
    check_text("service file path", &file)?;
    let (content, commands, note) = match paths.platform {
        Platform::Launchd => {
            let domain = format!("gui/{}", paths.uid);
            (
                plist(exe, &path_env, log)?,
                vec![
                    argv(&[LAUNCHCTL, "bootout", &format!("{domain}/{LABEL}")]),
                    argv(&[LAUNCHCTL, "bootstrap", &domain, &file]),
                ],
                None,
            )
        }
        Platform::Systemd => (
            unit(exe, &path_env)?,
            vec![
                argv(&[SYSTEMCTL, "--user", "daemon-reload"]),
                argv(&[SYSTEMCTL, "--user", "enable", UNIT]),
            ],
            Some(
                "the runner starts when you log in; to start it at boot without a login you would need `loginctl enable-linger` (not run by this command)"
                    .to_string(),
            ),
        ),
    };
    let existing = match read_existing(&paths.file)? {
        None => Exists::No,
        Some(c) if c == content => Exists::Same,
        Some(_) => Exists::Different,
    };
    Ok(Plan {
        platform: paths.platform,
        path: paths.file.clone(),
        content,
        existing,
        commands,
        note,
    })
}

fn write_atomic(path: &Path, content: &str) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("service file has no parent directory"))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".{}.tmp{}", LABEL, std::process::id()));
    let _ = fs::remove_file(&tmp);
    let res = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o644))?;
        fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

fn tolerant(argv: &[String]) -> bool {
    argv.iter().any(|a| a == "bootout")
}

fn exec(sys: &mut dyn Sys, cmd: &[String]) -> io::Result<()> {
    let out = sys.run(cmd)?;
    if out.ok || tolerant(cmd) {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "`{}` failed: {}",
        cmd.join(" "),
        out.stderr.trim()
    )))
}

pub fn install(plan: &Plan, sys: &mut dyn Sys) -> io::Result<()> {
    if plan.existing != Exists::Same {
        write_atomic(&plan.path, &plan.content)?;
    }
    for c in &plan.commands {
        exec(sys, c)?;
    }
    Ok(())
}

pub fn is_ours(platform: Platform, content: &str) -> bool {
    match platform {
        Platform::Launchd => {
            content.contains(&format!("<key>Label</key>\n  <string>{LABEL}</string>"))
                && content.contains("<string>runner</string>\n    <string>serve</string>")
        }
        Platform::Systemd => {
            let lines: Vec<&str> = content.lines().collect();
            lines.contains(&format!("Description=tmgr runner ({LABEL})").as_str())
                && lines
                    .iter()
                    .any(|l| l.starts_with("ExecStart=") && l.ends_with(" runner serve"))
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Removed {
    NotInstalled,
    Foreign,
    Removed,
}

pub fn uninstall(paths: &ServicePaths, sys: &mut dyn Sys) -> io::Result<Removed> {
    let Some(content) = read_existing(&paths.file)? else {
        return Ok(Removed::NotInstalled);
    };
    if !is_ours(paths.platform, &content) {
        return Ok(Removed::Foreign);
    }
    match paths.platform {
        Platform::Launchd => {
            exec(
                sys,
                &argv(&[LAUNCHCTL, "bootout", &format!("gui/{}/{LABEL}", paths.uid)]),
            )?;
            fs::remove_file(&paths.file)?;
        }
        Platform::Systemd => {
            let _ = sys.run(&argv(&[SYSTEMCTL, "--user", "disable", UNIT]));
            fs::remove_file(&paths.file)?;
            exec(sys, &argv(&[SYSTEMCTL, "--user", "daemon-reload"]))?;
        }
    }
    Ok(Removed::Removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Rec {
        calls: Vec<Vec<String>>,
        fail_on: Option<&'static str>,
    }

    impl Sys for Rec {
        fn run(&mut self, argv: &[String]) -> io::Result<RunOut> {
            self.calls.push(argv.to_vec());
            let bad = self.fail_on.is_some_and(|f| argv.iter().any(|a| a == f));
            Ok(RunOut {
                ok: !bad,
                stderr: if bad { "boom".into() } else { String::new() },
            })
        }
    }

    fn paths(p: Platform, home: &Path) -> ServicePaths {
        ServicePaths::new(p, home, None, 501)
    }

    #[test]
    fn plist_snapshot() {
        let got = plist(
            "/usr/local/bin/tmgr",
            "/usr/bin:/bin",
            "/run/dir/runner.log",
        )
        .unwrap();
        let want = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>dev.tmgr.runner</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/tmgr</string>
    <string>runner</string>
    <string>serve</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>StandardOutPath</key>
  <string>/run/dir/runner.log</string>
  <key>StandardErrorPath</key>
  <string>/run/dir/runner.log</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>/usr/bin:/bin</string>
  </dict>
</dict>
</plist>
"#;
        assert_eq!(got, want);
        assert!(!got.contains("KeepAlive"));
    }

    #[test]
    fn unit_snapshot() {
        let got = unit("/usr/local/bin/tmgr", "/usr/bin:/bin").unwrap();
        let want = "[Unit]\nDescription=tmgr runner (dev.tmgr.runner)\n\n[Service]\nType=simple\nExecStart=/usr/local/bin/tmgr runner serve\nEnvironment=\"PATH=/usr/bin:/bin\"\nRestart=no\n\n[Install]\nWantedBy=default.target\n";
        assert_eq!(got, want);
    }

    #[test]
    fn plist_escapes_xml() {
        let got = plist("/a b/&<>\"'/tmgr", "/x&y", "/l<og").unwrap();
        assert!(got.contains("<string>/a b/&amp;&lt;&gt;&quot;&apos;/tmgr</string>"));
        assert!(got.contains("<string>/x&amp;y</string>"));
        assert!(got.contains("<string>/l&lt;og</string>"));
    }

    #[test]
    fn unit_quotes_and_escapes() {
        let got = unit("/my dir/100%/t\"m\\gr$", "/p%q:/r").unwrap();
        assert!(got.contains("ExecStart=\"/my dir/100%%/t\\\"m\\\\gr$$\" runner serve\n"));
        assert!(got.contains("Environment=\"PATH=/p%%q:/r\"\n"));
    }

    #[test]
    fn newline_and_control_paths_are_refused() {
        assert!(plist("/a\n/tmgr", "/bin", "/l").is_err());
        assert!(plist("/a", "/b\0in", "/l").is_err());
        assert!(plist("/a", "/bin", "/l\r").is_err());
        assert!(unit("/a\n/tmgr", "/bin").is_err());
        assert!(unit("/a", "/bi\tn").is_err());
    }

    #[test]
    fn path_capture_keeps_absolute_unique_entries() {
        assert_eq!(
            capture_path("/usr/bin::rel/bin:/bin:/usr/bin:.:/opt/x"),
            "/usr/bin:/bin:/opt/x"
        );
        assert_eq!(capture_path(""), "/usr/bin:/bin");
        assert_eq!(capture_path("a:b"), "/usr/bin:/bin");
    }

    #[test]
    fn service_paths() {
        let h = Path::new("/home/u");
        assert_eq!(
            ServicePaths::new(Platform::Launchd, h, None, 1).file,
            Path::new("/home/u/Library/LaunchAgents/dev.tmgr.runner.plist")
        );
        assert_eq!(
            ServicePaths::new(Platform::Systemd, h, None, 1).file,
            Path::new("/home/u/.config/systemd/user/tmgr-runner.service")
        );
        assert_eq!(
            ServicePaths::new(Platform::Systemd, h, Some(Path::new("/cfg")), 1).file,
            Path::new("/cfg/systemd/user/tmgr-runner.service")
        );
        assert_eq!(
            ServicePaths::new(Platform::Systemd, h, Some(Path::new("rel")), 1).file,
            Path::new("/home/u/.config/systemd/user/tmgr-runner.service")
        );
    }

    #[test]
    fn warnings_flag_unstable_exes() {
        assert!(!exe_warnings(Path::new("/x/proj/target/debug/tmgr"), 0).is_empty());
        assert!(!exe_warnings(Path::new("/h/.nvm/v/bin/tmgr"), 0).is_empty());
        assert!(!exe_warnings(Path::new("/tmp/a/tmgr"), 0).is_empty());
        assert!(exe_warnings(Path::new("/usr/local/lib/tmgr-nonexistent"), 0).is_empty());
    }

    #[test]
    fn launchd_install_writes_file_and_registers() {
        let home = tempfile::tempdir().unwrap();
        let p = paths(Platform::Launchd, home.path());
        let plan = plan_install(&p, "/bin/tmgr", "/usr/bin", "/l/runner.log").unwrap();
        assert_eq!(plan.existing, Exists::No);
        let mut rec = Rec::default();
        install(&plan, &mut rec).unwrap();
        assert_eq!(fs::read_to_string(&p.file).unwrap(), plan.content);
        let mode = fs::metadata(&p.file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
        let dir = fs::read_dir(p.file.parent().unwrap()).unwrap().count();
        assert_eq!(dir, 1);
        let file = p.file.display().to_string();
        assert_eq!(
            rec.calls,
            vec![
                argv(&[LAUNCHCTL, "bootout", "gui/501/dev.tmgr.runner"]),
                argv(&[LAUNCHCTL, "bootstrap", "gui/501", &file]),
            ]
        );
    }

    #[test]
    fn bootout_failure_is_ignored_but_bootstrap_failure_is_not() {
        let home = tempfile::tempdir().unwrap();
        let p = paths(Platform::Launchd, home.path());
        let plan = plan_install(&p, "/bin/tmgr", "/usr/bin", "/l").unwrap();
        let mut rec = Rec {
            fail_on: Some("bootout"),
            ..Rec::default()
        };
        install(&plan, &mut rec).unwrap();
        let mut rec = Rec {
            fail_on: Some("bootstrap"),
            ..Rec::default()
        };
        let e = install(&plan, &mut rec).unwrap_err();
        assert!(e.to_string().contains("boom"));
    }

    #[test]
    fn systemd_install_registers_and_notes_linger() {
        let home = tempfile::tempdir().unwrap();
        let p = paths(Platform::Systemd, home.path());
        let plan = plan_install(&p, "/bin/tmgr", "/usr/bin", "/l").unwrap();
        assert!(plan.note.as_deref().unwrap().contains("enable-linger"));
        let mut rec = Rec::default();
        install(&plan, &mut rec).unwrap();
        assert!(p.file.starts_with(home.path().join(".config/systemd/user")));
        assert_eq!(
            rec.calls,
            vec![
                argv(&[SYSTEMCTL, "--user", "daemon-reload"]),
                argv(&[SYSTEMCTL, "--user", "enable", "tmgr-runner.service"]),
            ]
        );
        assert!(!rec
            .calls
            .iter()
            .any(|c| c.iter().any(|a| a.contains("linger"))));
    }

    #[test]
    fn existing_file_states() {
        let home = tempfile::tempdir().unwrap();
        let p = paths(Platform::Systemd, home.path());
        let plan = plan_install(&p, "/bin/tmgr", "/usr/bin", "/l").unwrap();
        install(&plan, &mut Rec::default()).unwrap();
        let again = plan_install(&p, "/bin/tmgr", "/usr/bin", "/l").unwrap();
        assert_eq!(again.existing, Exists::Same);
        let mut rec = Rec::default();
        install(&again, &mut rec).unwrap();
        assert_eq!(rec.calls.len(), 2);
        let other = plan_install(&p, "/bin/other", "/usr/bin", "/l").unwrap();
        assert_eq!(other.existing, Exists::Different);
    }

    #[test]
    fn uninstall_removes_only_our_file() {
        let home = tempfile::tempdir().unwrap();
        for platform in [Platform::Launchd, Platform::Systemd] {
            let p = paths(platform, home.path());
            let mut rec = Rec::default();
            assert_eq!(uninstall(&p, &mut rec).unwrap(), Removed::NotInstalled);
            assert!(rec.calls.is_empty());

            fs::create_dir_all(p.file.parent().unwrap()).unwrap();
            for foreign in [
                "something else".to_string(),
                format!("# mentions {LABEL} in a comment\nExecStart=/bin/other\n"),
                format!("<!-- {LABEL} --><plist><string>other</string></plist>"),
            ] {
                fs::write(&p.file, foreign).unwrap();
                assert_eq!(uninstall(&p, &mut rec).unwrap(), Removed::Foreign);
                assert!(p.file.exists());
            }
            assert!(rec.calls.is_empty());

            let plan = plan_install(&p, "/bin/tmgr", "/usr/bin", "/l").unwrap();
            install(&plan, &mut Rec::default()).unwrap();
            assert_eq!(uninstall(&p, &mut rec).unwrap(), Removed::Removed);
            assert!(!p.file.exists());
            let want = match platform {
                Platform::Launchd => {
                    vec![argv(&[LAUNCHCTL, "bootout", "gui/501/dev.tmgr.runner"])]
                }
                Platform::Systemd => vec![
                    argv(&[SYSTEMCTL, "--user", "disable", "tmgr-runner.service"]),
                    argv(&[SYSTEMCTL, "--user", "daemon-reload"]),
                ],
            };
            assert_eq!(rec.calls, want);
        }
    }
}
