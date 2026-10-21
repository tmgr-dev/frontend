use super::attach_shell::{can_split, shell_argv, size_hint};
use super::ops::{login_preflight, resolve_exe, run_create_req, run_create_run};
use super::{bin, connect, rerr};
use ratatui::layout::Rect;
use std::path::{Path, PathBuf};
use tmgr_core::error::Result;
use tmgr_core::redact::redact_secrets;
use tmgr_ipc::api::{CreateReq, RunCreateReq, SessionInfo};
use tmgr_runner::attach::{
    Action, KindOpt, NewRequest, NewSession, Place, SpawnKind, SplitDir, ViewState,
};
use tmgr_runner::daemon::RunnerConfig;
use tmgr_runner::profile::{self, ProfileSet};
use tmgr_runner::sanitize::sanitize_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SpawnErr {
    pub message: String,
    pub hint: Option<String>,
}

#[derive(Debug, PartialEq)]
pub(super) enum Plan {
    Create(CreateReq),
    CreateRun(Box<RunCreateReq>),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Route {
    Dialog,
    Sidebar,
    Other,
}

fn clean(s: &str) -> String {
    sanitize_text(&redact_secrets(s)).into_owned()
}

pub(super) fn route(view: &ViewState, action: &Action) -> Route {
    if view.confirm.is_some()
        || !matches!(
            action,
            Action::Forward(_) | Action::Drop(_) | Action::Paste { .. }
        )
    {
        return Route::Other;
    }
    if view.new_session.is_some() {
        Route::Dialog
    } else if view.sidebar_focus.is_some() {
        Route::Sidebar
    } else {
        Route::Other
    }
}

fn builtin(id: &str, label: &str, spawn: SpawnKind) -> KindOpt {
    KindOpt {
        id: id.into(),
        label: label.into(),
        spawn,
    }
}

pub(super) fn kind_options(profiles: &[(String, String)]) -> Vec<KindOpt> {
    let agent = |profile: &str| SpawnKind::Agent {
        profile: profile.into(),
    };
    let mut kinds = vec![
        builtin("claude", "claude", agent("claude-code")),
        builtin("codex", "codex", agent("codex")),
        builtin("shell", "shell", SpawnKind::Shell),
    ];
    for (id, name) in profiles {
        if id == "claude-code" || id == "codex" {
            continue;
        }
        let label = clean(if name.is_empty() { id } else { name });
        kinds.push(KindOpt {
            id: format!("agent:{id}"),
            label,
            spawn: agent(id),
        });
    }
    kinds.push(builtin(
        "custom",
        "custom",
        SpawnKind::Custom {
            line: String::new(),
        },
    ));
    kinds
}

pub(super) fn installed_kinds() -> Vec<KindOpt> {
    let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
    let profiles: Vec<(String, String)> = set
        .list()
        .iter()
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();
    kind_options(&profiles)
}

fn usable(path: &Path, home: &Path) -> bool {
    path.is_dir() && path != home && path != Path::new("/")
}

pub(super) fn default_folder(
    live: Option<PathBuf>,
    start: Option<&str>,
    cwd: Option<PathBuf>,
    home: &Path,
) -> String {
    let candidates: Vec<PathBuf> = live
        .into_iter()
        .chain(start.filter(|s| !s.is_empty()).map(PathBuf::from))
        .chain(cwd)
        .collect();
    let pick = candidates
        .iter()
        .find(|p| usable(p, home))
        .or(candidates.first());
    pick.map(|p| {
        p.to_string_lossy()
            .chars()
            .filter(|c| !c.is_control())
            .collect()
    })
    .unwrap_or_default()
}

pub(super) fn dialog_for(base: &NewSession, req: &NewRequest) -> NewSession {
    let mut ns = base.clone();
    ns.kind = base
        .kinds
        .iter()
        .position(|k| match (&k.spawn, &req.kind) {
            (SpawnKind::Agent { profile: a }, SpawnKind::Agent { profile: b }) => a == b,
            (SpawnKind::Shell, SpawnKind::Shell)
            | (SpawnKind::Custom { .. }, SpawnKind::Custom { .. }) => true,
            _ => false,
        })
        .unwrap_or(base.kind);
    ns.folder = req.folder.clone();
    ns.name = req.name.clone().unwrap_or_default();
    ns.place = req.place;
    if let SpawnKind::Custom { line } = &req.kind {
        ns.command = line.clone();
    }
    ns.error = None;
    ns.error_hint = None;
    ns
}

pub(super) fn kind_label(kind: &SpawnKind) -> String {
    match kind {
        SpawnKind::Agent { profile } => clean(profile),
        SpawnKind::Shell => "shell".into(),
        SpawnKind::Custom { line } => clean(line.split_whitespace().next().unwrap_or("command")),
    }
}

pub(super) fn spawn_size(content: Rect, place: Place) -> (u16, u16) {
    if content.is_empty() {
        return (80, 24);
    }
    match place {
        Place::Right => size_hint(content, SplitDir::Right),
        Place::Below => size_hint(content, SplitDir::Down),
        Place::Replace => (content.width, content.height),
    }
}

pub(super) fn registers_shell(req: &NewRequest) -> bool {
    req.kind == SpawnKind::Shell && req.name.is_none()
}

pub(super) fn place_session(view: &mut ViewState, target: &str, place: Place, id: &str) -> bool {
    let dir = match place {
        Place::Right => SplitDir::Right,
        Place::Below => SplitDir::Down,
        Place::Replace => {
            view.select(id);
            return false;
        }
    };
    let room = view.panes().iter().any(|p| p == target) && can_split(view.panes().len());
    if room && view.split_at(target, dir, id, true) {
        return true;
    }
    view.select(id);
    false
}

pub(super) fn run_hint(bin: &str, req: &NewRequest) -> Option<String> {
    let q = |s: &str| shell_words::quote(s).into_owned();
    let mut cmd = match &req.kind {
        SpawnKind::Agent { profile } => format!("{bin} run --agent {}", q(profile)),
        SpawnKind::Custom { line } => format!("{bin} run --agent-cmd {}", q(line)),
        SpawnKind::Shell => return None,
    };
    cmd.push_str(&format!(" --cwd {}", q(&req.folder)));
    if let Some(name) = &req.name {
        cmd.push_str(&format!(" --name {}", q(name)));
    }
    Some(redact_secrets(&cmd))
}

pub(super) fn plan(
    req: &NewRequest,
    cols: u16,
    rows: u16,
    resolve: &dyn Fn(&str) -> Result<String>,
) -> std::result::Result<Plan, String> {
    match &req.kind {
        SpawnKind::Agent { profile } => Ok(Plan::CreateRun(Box::new(run_create_run(
            profile.clone(),
            req.folder.clone(),
            req.name.clone(),
            None,
            (cols, rows),
            false,
        )))),
        SpawnKind::Shell => Ok(Plan::Create(run_create_req(
            shell_argv(),
            req.folder.clone(),
            req.name.clone(),
            cols,
            rows,
        ))),
        SpawnKind::Custom { line } => {
            let mut argv =
                shell_words::split(line).map_err(|e| format!("cannot parse the command: {e}"))?;
            if argv.is_empty() {
                return Err("the command is empty".into());
            }
            argv[0] = resolve(&argv[0]).map_err(|e| e.message)?;
            Ok(Plan::Create(run_create_req(
                argv,
                req.folder.clone(),
                req.name.clone(),
                cols,
                rows,
            )))
        }
    }
}

pub(super) fn start_session(
    cfg: &RunnerConfig,
    req: &NewRequest,
    cols: u16,
    rows: u16,
) -> std::result::Result<SessionInfo, SpawnErr> {
    let hint = run_hint(&bin(), req);
    let fail = |message: &str| SpawnErr {
        message: clean(message),
        hint: hint.clone(),
    };
    if let SpawnKind::Agent { profile } = &req.kind {
        login_preflight(profile).map_err(|e| fail(&e.message))?;
    }
    let plan = plan(req, cols, rows, &resolve_exe).map_err(|e| fail(&e))?;
    let mut client = connect(cfg).map_err(|e| fail(&e.message))?;
    match plan {
        Plan::Create(r) => client.create(r),
        Plan::CreateRun(r) => client.create_run(*r),
    }
    .map_err(|e| fail(&rerr(e).message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_ipc::api::{State, StateKind};
    use tmgr_runner::attach::{PaletteEntry, SessionRow};

    fn req(kind: SpawnKind) -> NewRequest {
        NewRequest {
            kind,
            folder: "/work/my project".into(),
            name: None,
            place: Place::Right,
        }
    }

    fn agent(p: &str) -> SpawnKind {
        SpawnKind::Agent { profile: p.into() }
    }

    fn ident(s: &str) -> Result<String> {
        Ok(s.to_string())
    }

    fn row(id: &str) -> SessionRow {
        SessionRow {
            id: id.into(),
            name: id.into(),
            state: State {
                kind: StateKind::Working,
                since_ms: 0,
                detail: None,
                exit_code: None,
            },
            age: String::new(),
            esc: false,
            resumed_from: None,
        }
    }

    #[test]
    fn agent_plan_matches_the_shared_builder_and_never_trusts() {
        let mut r = req(agent("codex"));
        r.name = Some("n".into());
        let Ok(Plan::CreateRun(got)) = plan(&r, 100, 30, &ident) else {
            panic!("expected a run request");
        };
        assert_eq!(
            *got,
            run_create_run(
                "codex".into(),
                r.folder.clone(),
                r.name.clone(),
                None,
                (100, 30),
                false
            )
        );
        assert!(got.trust_cwd.is_none() && got.trust_via_repo.is_none());
        assert!(got.start_key.is_none() && got.origin.is_none() && got.clone_of.is_none());
        assert!(got.cloud.is_none() && got.bundle.is_none() && got.exe_pin.is_none());
        assert!(!got.unattended && !got.agent_work);
    }

    #[test]
    fn custom_plan_splits_and_resolves_the_executable() {
        let r = req(SpawnKind::Custom {
            line: "mytool --flag 'a b'".into(),
        });
        let resolve = |s: &str| -> Result<String> { Ok(format!("/bin/{s}")) };
        let Ok(Plan::Create(got)) = plan(&r, 90, 20, &resolve) else {
            panic!("expected a create request");
        };
        let argv = vec!["/bin/mytool".into(), "--flag".into(), "a b".into()];
        assert_eq!(got, run_create_req(argv, r.folder.clone(), None, 90, 20));
        assert!(got.profile.is_none() && got.env_pass.is_empty() && got.task.is_none());
    }

    #[test]
    fn custom_plan_reports_a_bad_command() {
        let bad = req(SpawnKind::Custom {
            line: "a 'b".into(),
        });
        assert!(plan(&bad, 80, 24, &ident).is_err());
        let empty = req(SpawnKind::Custom { line: "  ".into() });
        assert!(plan(&empty, 80, 24, &ident).is_err());
    }

    #[test]
    fn shell_plan_uses_the_login_shell_and_the_given_name() {
        let mut r = req(SpawnKind::Shell);
        r.name = Some("sh-3".into());
        let Ok(Plan::Create(got)) = plan(&r, 80, 24, &ident) else {
            panic!("expected a create request");
        };
        assert_eq!(got.argv, shell_argv());
        assert_eq!(got.name.as_deref(), Some("sh-3"));
    }

    #[test]
    fn hints_are_exact_quoted_and_never_trust() {
        let mut a = req(agent("claude-code"));
        assert_eq!(
            run_hint("tmgr", &a).as_deref(),
            Some("tmgr run --agent claude-code --cwd '/work/my project'")
        );
        a.name = Some("my agent".into());
        assert_eq!(
            run_hint("tmgr", &a).as_deref(),
            Some("tmgr run --agent claude-code --cwd '/work/my project' --name 'my agent'")
        );
        let c = req(SpawnKind::Custom {
            line: "tool --x 'a b'".into(),
        });
        assert_eq!(
            run_hint("tmgr", &c).as_deref(),
            Some("tmgr run --agent-cmd 'tool --x '\\''a b'\\''' --cwd '/work/my project'")
        );
        assert_eq!(run_hint("tmgr", &req(SpawnKind::Shell)), None);
        assert!(!run_hint("tmgr", &a).unwrap().contains("--trust"));
    }

    #[test]
    fn kinds_put_builtins_first_and_skip_duplicates() {
        let kinds = kind_options(&[
            ("claude-code".into(), "Claude Code".into()),
            ("codex".into(), "Codex".into()),
            ("aider".into(), "Aider\x1b[31m".into()),
        ]);
        let ids: Vec<&str> = kinds.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(ids, ["claude", "codex", "shell", "agent:aider", "custom"]);
        assert_eq!(kinds[0].spawn, agent("claude-code"));
        assert_eq!(kinds[3].spawn, agent("aider"));
        assert!(!kinds[3].label.contains('\x1b'));
    }

    #[test]
    fn default_folder_skips_home_and_root() {
        let dir = std::env::temp_dir();
        let home = dir.join("fake-home-for-test");
        let d = dir.to_string_lossy().into_owned();
        assert_eq!(default_folder(Some(home.clone()), Some(&d), None, &home), d);
        assert_eq!(
            default_folder(Some("/".into()), None, Some(dir.clone()), &home),
            d
        );
        assert_eq!(default_folder(None, None, None, &home), "");
    }

    #[test]
    fn spawn_size_halves_for_a_split_and_fills_for_replace() {
        let content = Rect::new(0, 0, 100, 40);
        assert_eq!(
            spawn_size(content, Place::Right),
            size_hint(content, SplitDir::Right)
        );
        assert_eq!(
            spawn_size(content, Place::Below),
            size_hint(content, SplitDir::Down)
        );
        assert_eq!(spawn_size(content, Place::Replace), (100, 40));
        assert_eq!(spawn_size(Rect::default(), Place::Replace), (80, 24));
    }

    fn open(view: &mut ViewState) {
        view.open_new_session("/".into(), kind_options(&[]), "/h".into());
    }

    #[test]
    fn reopening_with_an_error_round_trips_the_request() {
        let mut view = ViewState::new(vec![row("a")], "t");
        open(&mut view);
        let base = view.new_session.clone().unwrap();
        let mut r = req(SpawnKind::Custom {
            line: "tool x".into(),
        });
        r.name = Some("nm".into());
        r.place = Place::Below;
        let dialog = dialog_for(&base, &r).with_error("boom", Some("tmgr run".into()));
        view.new_session = None;
        view.reopen_new_session(dialog);
        let ns = view.new_session.unwrap();
        assert_eq!(ns.kinds[ns.kind].id, "custom");
        assert_eq!(ns.command, "tool x");
        assert_eq!(ns.folder, r.folder);
        assert_eq!(ns.name, "nm");
        assert_eq!(ns.place, Place::Below);
        assert_eq!(ns.error.as_deref(), Some("boom"));
        assert_eq!(ns.error_hint.as_deref(), Some("tmgr run"));
    }

    #[test]
    fn dialog_input_never_reaches_a_session() {
        let mut view = ViewState::new(vec![row("a")], "t");
        open(&mut view);
        let paste = Action::Paste {
            data: b"\x1b[200~x\r\x1b[201~".to_vec(),
            start: true,
            end: true,
        };
        assert_eq!(route(&view, &paste), Route::Dialog);
        assert_eq!(route(&view, &Action::Drop(b"/tmp".to_vec())), Route::Dialog);
        assert_eq!(route(&view, &Action::Forward(b"a".to_vec())), Route::Dialog);
        view.insert_new_session_text(b"\x1b[200~ab\rcd\x1b[201~");
        assert!(view.new_session.is_some());
        assert_eq!(view.new_session.as_ref().unwrap().folder, "/");
    }

    #[test]
    fn pasted_return_does_not_submit_the_dialog() {
        let mut view = ViewState::new(vec![row("a")], "t");
        open(&mut view);
        view.new_session.as_mut().unwrap().focus = tmgr_runner::attach::NewField::Name;
        view.insert_new_session_text(b"abc\r");
        let ns = view.new_session.as_ref().unwrap();
        assert_eq!(ns.name, "abc");
    }

    #[test]
    fn sidebar_focus_swallows_bytes_and_a_pending_confirm_wins() {
        let mut view = ViewState::new(vec![row("a")], "t");
        view.width = 200;
        assert!(view.enter_sidebar());
        let fwd = Action::Forward(b"x".to_vec());
        assert_eq!(route(&view, &fwd), Route::Sidebar);
        assert_eq!(route(&view, &Action::Drop(b"x".to_vec())), Route::Sidebar);
        assert_eq!(
            route(&view, &Action::Cmd(tmgr_runner::attach::Cmd::Split)),
            Route::Other
        );
        view.confirm = Some(tmgr_runner::attach::Confirm {
            session: "a".into(),
            name: "a".into(),
            remove: false,
        });
        assert_eq!(route(&view, &fwd), Route::Other);
        assert_eq!(route(&view, &Action::Drop(b"x".to_vec())), Route::Other);
        open(&mut view);
        assert!(view.confirm.is_none());
        assert_eq!(route(&view, &fwd), Route::Dialog);
    }

    #[test]
    fn a_split_lands_beside_the_target_and_takes_focus() {
        let mut view = ViewState::new(vec![row("a"), row("n")], "t");
        view.select("a");
        assert!(place_session(&mut view, "a", Place::Below, "n"));
        assert_eq!(view.panes(), ["a", "n"]);
        assert_eq!(view.focused_session(), Some("n"));
    }

    #[test]
    fn a_gone_target_or_no_room_replaces_the_focused_pane() {
        let mut view = ViewState::new(vec![row("a"), row("n")], "t");
        view.select("a");
        assert!(!place_session(&mut view, "gone", Place::Right, "n"));
        assert_eq!(view.panes(), ["n"]);
        let mut view = ViewState::new(vec![row("a"), row("n")], "t");
        view.select("a");
        assert!(!place_session(&mut view, "a", Place::Replace, "n"));
        assert_eq!(view.focused_session(), Some("n"));
    }

    #[test]
    fn only_unnamed_shells_are_registered_for_reaping() {
        let mut shell = req(SpawnKind::Shell);
        assert!(registers_shell(&shell));
        shell.name = Some("mine".into());
        assert!(!registers_shell(&shell));
        assert!(!registers_shell(&req(agent("codex"))));
        assert!(!registers_shell(&req(SpawnKind::Custom {
            line: "x".into()
        })));
    }

    #[test]
    fn plain_input_routes_to_the_session() {
        let view = ViewState::new(vec![row("a")], "t");
        assert_eq!(route(&view, &Action::Forward(b"x".to_vec())), Route::Other);
        let _ = PaletteEntry::Session("a".into());
    }
}
