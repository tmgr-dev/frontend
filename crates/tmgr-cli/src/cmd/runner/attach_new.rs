use super::attach_shell::{can_split, shell_argv, size_hint};
use super::ops::{login_preflight, run_create_req, run_create_run};
use super::run_task::{self, Launch, Prepared};
use super::{bin, connect, rerr, PermissionMode};
use ratatui::layout::Rect;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Result};
use tmgr_core::redact::redact_secrets;
use tmgr_core::refs::parse_task_ref;
use tmgr_ipc::api::{CreateReq, RunCreateReq, SessionInfo};
use tmgr_runner::attach::{
    Action, KindOpt, NewRequest, NewSession, Place, SpawnKind, SplitDir, ViewState,
};
use tmgr_runner::daemon::RunnerConfig;
use tmgr_runner::exe;
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
    Git,
    Summary,
    Export,
    Questions,
    Peek,
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
    if view.export.is_some() {
        Route::Export
    } else if view.summary.is_some() {
        Route::Summary
    } else if view.new_session.is_some() {
        Route::Dialog
    } else if view.git_open {
        Route::Git
    } else if view.qmenu.is_some() {
        Route::Questions
    } else if view.peek.is_some() {
        Route::Peek
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
        Place::Replace | Place::Tab => (content.width, content.height),
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
        Place::Tab => {
            view.open(id);
            return true;
        }
    };
    if !view.panes().iter().any(|p| p == target) && view.in_tab(target) {
        view.open(target);
    }
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

pub(super) const SPAWN_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) fn spawn_expired(age: Duration) -> bool {
    age >= SPAWN_TIMEOUT
}

pub(super) struct Spawn {
    id: u64,
    dialog: NewSession,
    name: Option<String>,
    since: Instant,
}

#[derive(Default)]
pub(super) struct NewFlow {
    next: u64,
    spawning: Option<Spawn>,
    failed: Option<NewSession>,
    review: Option<TaskReview>,
}

impl NewFlow {
    pub(super) fn busy(&mut self, now: Instant) -> bool {
        match &self.spawning {
            Some(s) if spawn_expired(now.saturating_duration_since(s.since)) => {
                self.spawning = None;
                false
            }
            Some(_) => true,
            None => false,
        }
    }

    pub(super) fn is_spawning(&self) -> bool {
        self.spawning.is_some()
    }

    pub(super) fn begin(&mut self, dialog: NewSession, name: Option<String>, now: Instant) -> u64 {
        self.failed = None;
        self.next += 1;
        self.spawning = Some(Spawn {
            id: self.next,
            dialog,
            name,
            since: now,
        });
        self.next
    }

    pub(super) fn finish(&mut self, id: u64) -> Option<NewSession> {
        if self.spawning.as_ref().is_some_and(|s| s.id == id) {
            self.spawning.take().map(|s| s.dialog)
        } else {
            None
        }
    }

    pub(super) fn fail(&mut self, dialog: NewSession, e: SpawnErr) {
        self.failed = Some(dialog.with_error(e.message, e.hint));
    }

    pub(super) fn hold_review(&mut self, review: TaskReview) {
        self.review = Some(review);
    }

    pub(super) fn take_review(&mut self) -> Option<TaskReview> {
        self.review.take()
    }

    pub(super) fn review_is_none(&self) -> bool {
        self.review.is_none()
    }

    pub(super) fn drop_review(&mut self) {
        self.review = None;
    }

    pub(super) fn keep_review_if(&mut self, shown: bool) {
        if !shown {
            self.review = None;
        }
    }

    pub(super) fn take_failed(&mut self) -> Option<NewSession> {
        self.failed.take()
    }

    pub(super) fn planned_name(&self) -> Option<&str> {
        self.spawning.as_ref().and_then(|s| s.name.as_deref())
    }
}

fn expand_tilde(s: &str) -> PathBuf {
    match (s.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(s),
    }
}

pub(super) fn resolve_in_folder(name: &str, folder: &str) -> Result<String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let base = expand_tilde(folder);
    let name = expand_tilde(name);
    exe::resolve_exe(&name.to_string_lossy(), &path, Some(&base))
        .map(|p| p.display().to_string())
        .map_err(Error::not_found)
}

pub(super) struct TaskReview {
    pub prepared: Prepared,
    pub req: NewRequest,
    pub target: String,
    pub size: (u16, u16),
}

pub(super) fn in_agent() -> bool {
    tmgr_link::privileged::RUN_ENV
        .iter()
        .any(|k| std::env::var_os(k).is_some())
        || tmgr_core::local::agent_session_detected()
}

fn task_err(e: Error) -> SpawnErr {
    SpawnErr {
        message: clean(&e.message),
        hint: e.hint.as_deref().map(clean),
    }
}

fn guard(in_agent: bool) -> std::result::Result<(), SpawnErr> {
    tmgr_link::privileged::check(in_agent, true).map_err(task_err)
}

fn task_profile(req: &NewRequest) -> std::result::Result<(&str, &str), SpawnErr> {
    let (SpawnKind::Agent { profile }, Some(task)) = (&req.kind, req.task.as_deref()) else {
        return Err(SpawnErr {
            message: "a task needs an agent profile".into(),
            hint: None,
        });
    };
    parse_task_ref(task).map_err(task_err)?;
    Ok((profile, task))
}

pub(super) fn prepare_task(
    req: &NewRequest,
    in_agent: bool,
) -> std::result::Result<Prepared, SpawnErr> {
    guard(in_agent)?;
    let (profile, task) = task_profile(req)?;
    run_task::prepare(None, None, task, profile, PermissionMode::Ask).map_err(task_err)
}

pub(super) fn execute_task(
    review: &TaskReview,
    in_agent: bool,
) -> std::result::Result<SessionInfo, SpawnErr> {
    guard(in_agent)?;
    run_task::execute(
        &review.prepared,
        &Launch {
            model: None,
            name: review.req.name.as_deref(),
            base: None,
            size: review.size,
        },
    )
    .map(|started| started.info)
    .map_err(task_err)
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
    let plan = plan(req, cols, rows, &|name| {
        resolve_in_folder(name, &req.folder)
    })
    .map_err(|e| fail(&e))?;
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
    use tmgr_runner::attach::{NewEdit, SessionRow};

    fn req(kind: SpawnKind) -> NewRequest {
        NewRequest {
            kind,
            folder: "/work/my project".into(),
            name: None,
            place: Place::Right,
            task: None,
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
            you: None,
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
        assert_eq!(spawn_size(content, Place::Tab), (100, 40));
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
        view.new_session.as_mut().unwrap().focus = tmgr_runner::attach::NewField::Folder;
        view.insert_new_session_text(b"\x1b[200~ab\rcd\x1b[201~");
        assert!(view.new_session.is_some());
        assert_eq!(view.new_session.as_ref().unwrap().folder, "/abcd");
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
        assert!(view.enter_sidebar(Rect::new(0, 0, 200, 30)));
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
    fn a_new_tab_shows_the_session_alone_and_keeps_the_split_as_a_tab() {
        let mut view = ViewState::new(vec![row("a"), row("b"), row("n")], "t");
        view.select("a");
        assert!(place_session(&mut view, "a", Place::Right, "b"));
        assert!(place_session(&mut view, "b", Place::Tab, "n"));
        assert_eq!(view.panes(), ["n"]);
        assert_eq!(view.focused_session(), Some("n"));
        view.open("a");
        assert_eq!(view.panes(), ["a", "b"]);
        assert_eq!(view.focused_session(), Some("a"));
    }

    #[test]
    fn a_split_into_a_stashed_tab_switches_to_that_tab() {
        let mut view = ViewState::new(vec![row("a"), row("b"), row("c"), row("n")], "t");
        view.select("a");
        assert!(place_session(&mut view, "a", Place::Right, "b"));
        view.open("c");
        assert!(place_session(&mut view, "a", Place::Below, "n"));
        assert_eq!(view.panes(), ["a", "n", "b"]);
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
    }

    fn base_dialog() -> NewSession {
        let mut view = ViewState::new(vec![row("a")], "t");
        open(&mut view);
        view.new_session.take().unwrap()
    }

    fn err(msg: &str) -> SpawnErr {
        SpawnErr {
            message: msg.into(),
            hint: Some("tmgr run x".into()),
        }
    }

    #[test]
    fn a_failed_dialog_is_kept_until_reopened_with_its_values() {
        let mut flow = NewFlow::default();
        let mut dialog = base_dialog();
        dialog.folder = "/typed".into();
        let id = flow.begin(dialog.clone(), None, Instant::now());
        let d = flow.finish(id).unwrap();
        flow.fail(d, err("boom"));
        assert!(!flow.is_spawning());
        let saved = flow.take_failed().unwrap();
        assert_eq!(saved.folder, "/typed");
        assert_eq!(saved.error.as_deref(), Some("boom"));
        assert_eq!(saved.error_hint.as_deref(), Some("tmgr run x"));
        assert!(flow.take_failed().is_none());
    }

    #[test]
    fn a_fresh_submit_clears_the_failed_slot() {
        let mut flow = NewFlow::default();
        flow.fail(base_dialog(), err("boom"));
        flow.begin(base_dialog(), None, Instant::now());
        assert!(flow.take_failed().is_none());
    }

    #[test]
    fn a_stale_spawn_stops_blocking_and_its_late_result_is_foreign() {
        assert!(!spawn_expired(SPAWN_TIMEOUT - Duration::from_millis(1)));
        assert!(spawn_expired(SPAWN_TIMEOUT));
        let mut flow = NewFlow::default();
        let t0 = Instant::now();
        let old = flow.begin(base_dialog(), None, t0);
        assert!(flow.busy(t0 + Duration::from_secs(29)));
        assert!(!flow.busy(t0 + SPAWN_TIMEOUT));
        assert!(!flow.is_spawning());
        let new = flow.begin(base_dialog(), None, t0 + SPAWN_TIMEOUT);
        assert!(flow.finish(old).is_none());
        assert!(flow.is_spawning());
        assert!(flow.finish(new).is_some());
    }

    #[test]
    fn a_dialog_shell_in_flight_reserves_its_name_for_splits() {
        use super::super::attach_shell::shell_name;
        let mut flow = NewFlow::default();
        let planned = shell_name(&[], &Default::default(), None);
        flow.begin(base_dialog(), Some(planned.clone()), Instant::now());
        let split = shell_name(&[], &Default::default(), flow.planned_name());
        assert_ne!(split, planned);
    }

    #[cfg(unix)]
    #[test]
    fn custom_commands_resolve_against_the_chosen_folder() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("tmgr-resolve-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tool = dir.join("tool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let folder = dir.to_string_lossy().into_owned();
        let got = resolve_in_folder("./tool", &folder).unwrap();
        assert!(got.ends_with("/tool"));
        assert!(Path::new(&got).is_absolute());
        assert!(resolve_in_folder("./missing-tool", &folder).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dialog_paste_lands_in_the_focused_field_without_submitting() {
        let mut view = ViewState::new(vec![row("a")], "t");
        open(&mut view);
        view.new_session.as_mut().unwrap().focus = tmgr_runner::attach::NewField::Folder;
        let paste = Action::Paste {
            data: b"\x1b[200~x\r\x1b[201~".to_vec(),
            start: true,
            end: true,
        };
        assert_eq!(route(&view, &paste), Route::Dialog);
        view.insert_new_session_text(b"\x1b[200~ab\rcd\x1b[201~");
        let ns = view.new_session.as_ref().unwrap();
        assert_eq!(ns.folder, "/abcd");
    }

    fn task_req(kind: SpawnKind, task: &str) -> NewRequest {
        NewRequest {
            task: Some(task.into()),
            ..req(kind)
        }
    }

    #[test]
    fn a_task_start_is_refused_inside_an_agent_session() {
        let e = prepare_task(&task_req(agent("claude-code"), "TM-7"), true)
            .err()
            .unwrap();
        assert!(e.message.contains("agent session"), "{}", e.message);
        assert!(guard(true).is_err());
        assert!(guard(false).is_ok());
    }

    #[test]
    fn a_task_needs_an_agent_profile() {
        for kind in [SpawnKind::Shell, SpawnKind::Custom { line: "vim".into() }] {
            let e = prepare_task(&task_req(kind, "TM-7"), false).err().unwrap();
            assert_eq!(e.message, "a task needs an agent profile");
        }
        assert!(prepare_task(&req(agent("claude-code")), false).is_err());
    }

    #[test]
    fn only_a_plain_task_reference_reaches_the_run() {
        for bad in [
            "TM-7 --cwd /etc",
            "--yes",
            "../x",
            "tm-7",
            "TM-",
            "TM-7\n",
            "$(id)",
            "",
        ] {
            let e = task_profile(&task_req(agent("claude-code"), bad)).unwrap_err();
            assert!(
                e.message.contains("invalid task reference"),
                "{bad}: {}",
                e.message
            );
        }
        for good in ["TM-7", "#42", "ABC123-9999999"] {
            let r = task_req(agent("codex"), good);
            assert_eq!(task_profile(&r).unwrap(), ("codex", good));
        }
    }

    fn held() -> TaskReview {
        TaskReview {
            prepared: run_task::sample(),
            req: task_req(agent("claude-code"), "TM-7"),
            target: "a".into(),
            size: (80, 24),
        }
    }

    #[test]
    fn the_review_is_held_until_enter_and_dropped_when_the_form_goes_back() {
        let mut view = ViewState::new(vec![row("a")], "t");
        open(&mut view);
        let mut flow = NewFlow::default();
        let dialog = view.new_session.take().unwrap();
        view.review_new_session(dialog, vec!["Task:    TM-7 x".into()]);
        flow.hold_review(held());
        flow.keep_review_if(
            view.new_session
                .as_ref()
                .is_some_and(|n| n.review.is_some()),
        );
        assert!(flow.review.is_some());
        assert_eq!(view.edit_new_session(b"\r"), NewEdit::Editing);
        assert_eq!(view.edit_new_session(b"y"), NewEdit::Approve);
        let review = flow.take_review().unwrap();
        assert_eq!(review.req.task.as_deref(), Some("TM-7"));
        assert!(flow.take_review().is_none());

        flow.hold_review(held());
        view.edit_new_session(b"\x1b");
        flow.keep_review_if(
            view.new_session
                .as_ref()
                .is_some_and(|n| n.review.is_some()),
        );
        assert!(flow.take_review().is_none());
        assert!(view.new_session.is_some());
    }

    #[test]
    fn a_fresh_form_forgets_an_old_review() {
        let mut flow = NewFlow::default();
        flow.hold_review(held());
        flow.drop_review();
        assert!(flow.take_review().is_none());
    }

    #[test]
    fn executing_a_held_task_is_refused_inside_an_agent_session() {
        let e = execute_task(&held(), true).unwrap_err();
        assert!(e.message.contains("agent session"), "{}", e.message);
    }
}
