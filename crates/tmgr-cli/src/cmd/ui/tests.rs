use super::app::{App, Screen};
use super::cmdline::{Value, Values};
use super::model::Kind;
use super::*;
use clap::CommandFactory;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use std::fmt::Write as _;

const DENIED: [&str; 4] = ["yes", "json", "server", "verbose"];

fn clap_root() -> clap::Command {
    let mut cmd = crate::cli::Cli::command();
    cmd.build();
    cmd
}

fn tree() -> model::Node {
    model::build(&crate::cli::Cli::command())
}

fn fresh() -> App {
    App::new(tree(), Vec::new())
}

fn home_app() -> App {
    let mut app = fresh();
    app.header = "tmgr test".into();
    app
}

fn p(path: &[&str]) -> Vec<String> {
    path.iter().map(|s| s.to_string()).collect()
}

fn vals(pairs: &[(&str, Value)]) -> Values {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

fn node_at<'a>(root: &'a model::Node, path: &[&str]) -> &'a model::Node {
    root.find(&p(path))
        .unwrap_or_else(|| panic!("no node for {path:?}"))
}

fn walk<'a>(
    cmd: &clap::Command,
    node: &'a model::Node,
    f: &mut dyn FnMut(&clap::Command, &'a model::Node),
) {
    f(cmd, node);
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" {
            continue;
        }
        let child = node
            .children
            .iter()
            .find(|c| c.path.last().map(String::as_str) == Some(name))
            .unwrap_or_else(|| panic!("no node for subcommand {name}"));
        walk(sub, child, f);
    }
}

fn all_nodes(node: &model::Node) -> Vec<&model::Node> {
    let mut out = vec![node];
    for c in &node.children {
        out.extend(all_nodes(c));
    }
    out
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ch(c: char) -> KeyEvent {
    key(KeyCode::Char(c))
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        app.on_key(ch(c));
    }
}

fn form(app: &App) -> &app::Form {
    match &app.screen {
        Screen::Form(f) => f,
        _ => panic!("not on a form"),
    }
}

fn focus_field(app: &mut App, id: &str) {
    let Screen::Form(f) = &mut app.screen else {
        panic!("not on a form");
    };
    let idx = f
        .node
        .fields
        .iter()
        .position(|x| x.id == id)
        .unwrap_or_else(|| panic!("no field {id}"));
    f.cursor = idx;
}

fn synth_field(id: &str, positional: bool, kind: Kind, required: bool) -> model::Field {
    model::Field {
        id: id.into(),
        long: (!positional).then(|| id.to_string()),
        positional,
        kind,
        required,
        default: None,
        help: String::new(),
        sensitive: false,
    }
}

fn synth_node(path: &[&str], fields: Vec<model::Field>) -> model::Node {
    model::Node {
        path: p(path),
        about: String::new(),
        children: Vec::new(),
        fields,
        runnable: true,
    }
}

fn kind_name(k: &Kind) -> String {
    match k {
        Kind::Bool => "bool".into(),
        Kind::Count => "count".into(),
        Kind::Text => "text".into(),
        Kind::Path => "path".into(),
        Kind::Select(v) => format!("select({})", v.join("|")),
        Kind::Multi => "multi".into(),
    }
}

fn check_snapshot(actual: &str, name: &str, expected: &str) {
    if std::env::var("UPDATE_SNAPSHOT").is_ok_and(|v| v == "1") {
        let path = format!("{}/src/cmd/ui/snapshots/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::write(&path, actual).unwrap();
        return;
    }
    assert_eq!(
        actual, expected,
        "snapshot {name} changed; rerun with UPDATE_SNAPSHOT=1 to regenerate"
    );
}

fn commands_text(root: &model::Node) -> String {
    let mut out = String::new();
    let labels: Vec<(Vec<String>, String)> = groups::groups(root)
        .into_iter()
        .flat_map(|g| {
            let label = g.label.clone();
            g.entries.into_iter().map(move |e| (e.path, label.clone()))
        })
        .collect();
    let ids = |n: &model::Node| {
        if n.fields.is_empty() {
            "-".to_string()
        } else {
            n.fields
                .iter()
                .map(|f| f.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    writeln!(out, "<root> | - | {}", ids(root)).unwrap();
    for leaf in root.leaves() {
        if groups::is_internal(&leaf.path) {
            continue;
        }
        let group = labels
            .iter()
            .find(|(path, _)| *path == leaf.path)
            .map(|(_, g)| g.as_str())
            .unwrap_or("?");
        writeln!(out, "{} | {} | {}", leaf.path.join(" "), group, ids(leaf)).unwrap();
    }
    out
}

fn forms_text(root: &model::Node, paths: &[&[&str]]) -> String {
    let mut out = String::new();
    for path in paths {
        let node = node_at(root, path);
        writeln!(out, "tmgr {}", node.path.join(" ")).unwrap();
        for f in &node.fields {
            writeln!(
                out,
                "  {} kind={} required={} default={} sensitive={} positional={}",
                f.id,
                kind_name(&f.kind),
                f.required,
                f.default.as_deref().unwrap_or("-"),
                f.sensitive,
                f.positional
            )
            .unwrap();
        }
    }
    out
}

const PINNED: &[&[&str]] = &[
    &["add"],
    &["start"],
    &["done"],
    &["ws", "use"],
    &["task", "list"],
    &["board"],
    &["page", "pull"],
    &["usage"],
];

#[cfg(not(feature = "runner"))]
#[test]
fn command_list_snapshot() {
    check_snapshot(
        &commands_text(&tree()),
        "commands.txt",
        include_str!("snapshots/commands.txt"),
    );
}

#[cfg(feature = "runner")]
#[test]
fn command_list_snapshot() {
    check_snapshot(
        &commands_text(&tree()),
        "commands_runner.txt",
        include_str!("snapshots/commands_runner.txt"),
    );
}

#[cfg(not(feature = "runner"))]
#[test]
fn pinned_forms_snapshot() {
    check_snapshot(
        &forms_text(&tree(), PINNED),
        "forms.txt",
        include_str!("snapshots/forms.txt"),
    );
}

#[cfg(feature = "runner")]
#[test]
fn pinned_forms_snapshot() {
    let mut paths: Vec<&[&str]> = PINNED.to_vec();
    paths.push(&["run"]);
    check_snapshot(
        &forms_text(&tree(), &paths),
        "forms_runner.txt",
        include_str!("snapshots/forms_runner.txt"),
    );
}

#[test]
fn every_leaf_resolves_to_its_node() {
    let root = tree();
    let leaves = root.leaves();
    assert!(!leaves.is_empty());
    for leaf in leaves {
        let found = root.find(&leaf.path).expect("leaf maps to a node");
        assert_eq!(found.path, leaf.path);
    }
}

#[test]
fn no_global_or_denied_field_is_exposed() {
    let root = tree();
    for node in all_nodes(&root) {
        for f in &node.fields {
            assert!(
                !DENIED.contains(&f.id.as_str()),
                "{:?} exposes {}",
                node.path,
                f.id
            );
            assert!(
                !f.long.as_deref().is_some_and(|l| DENIED.contains(&l)),
                "{:?} exposes --{:?}",
                node.path,
                f.long
            );
        }
    }
}

#[test]
fn help_and_version_fields_never_appear() {
    let root = tree();
    for node in all_nodes(&root) {
        for f in &node.fields {
            assert!(f.id != "help" && f.id != "version", "{:?}", node.path);
        }
        assert!(!node.path.iter().any(|s| s == "help"), "{:?}", node.path);
    }
}

#[test]
fn only_root_carries_ws() {
    let root = tree();
    let ids: Vec<&str> = root.fields.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["ws"]);
    for node in all_nodes(&root).into_iter().filter(|n| !n.path.is_empty()) {
        assert!(node.fields.iter().all(|f| f.id != "ws"), "{:?}", node.path);
    }
}

#[test]
fn hidden_args_never_appear() {
    let root = tree();
    let cmd = clap_root();
    walk(&cmd, &root, &mut |c, node| {
        for arg in c.get_arguments() {
            if arg.is_hide_set() {
                let id = arg.get_id().as_str();
                assert!(
                    node.fields.iter().all(|f| f.id != id),
                    "hidden arg {id} shown under {:?}",
                    node.path
                );
            }
        }
    });
}

#[test]
fn visible_local_args_have_fields_with_matching_shape() {
    let root = tree();
    let cmd = clap_root();
    walk(&cmd, &root, &mut |c, node| {
        for arg in c.get_arguments() {
            let id = arg.get_id().as_str();
            if arg.is_global_set()
                || arg.is_hide_set()
                || id == "help"
                || id == "version"
                || DENIED.contains(&id)
            {
                continue;
            }
            let field = node
                .fields
                .iter()
                .find(|f| f.id == id)
                .unwrap_or_else(|| panic!("{:?} lacks field {id}", node.path));
            assert_eq!(
                field.required,
                arg.is_required_set(),
                "{:?} {id} required",
                node.path
            );
            match arg.get_action() {
                clap::ArgAction::SetTrue | clap::ArgAction::SetFalse => {
                    assert_eq!(field.kind, Kind::Bool, "{:?} {id}", node.path)
                }
                clap::ArgAction::Count => {
                    assert_eq!(field.kind, Kind::Count, "{:?} {id}", node.path)
                }
                clap::ArgAction::Append => {
                    assert_eq!(field.kind, Kind::Multi, "{:?} {id}", node.path)
                }
                _ => {
                    let options: Vec<String> = arg
                        .get_possible_values()
                        .iter()
                        .filter(|v| !v.is_hide_set())
                        .map(|v| v.get_name().to_string())
                        .collect();
                    if options.is_empty() {
                        assert!(
                            !matches!(
                                field.kind,
                                Kind::Select(_) | Kind::Bool | Kind::Count | Kind::Multi
                            ),
                            "{:?} {id} is {:?}",
                            node.path,
                            field.kind
                        );
                    } else {
                        assert_eq!(field.kind, Kind::Select(options), "{:?} {id}", node.path);
                    }
                }
            }
        }
    });
}

#[test]
fn path_arguments_resolve_to_path_kind() {
    let root = tree();
    for (path, id) in [
        (&["add"][..], "desc_file"),
        (&["project", "map"][..], "path"),
        (&["git", "hook", "commit-msg"][..], "file"),
        (&["runner", "trust"][..], "dir"),
        (&["page", "pull"][..], "dir"),
        (&["usage", "accounts", "add"][..], "root"),
    ] {
        if root.find(&p(path)).is_none() {
            continue;
        }
        let field = node_at(&root, path)
            .fields
            .iter()
            .find(|f| f.id == id)
            .unwrap_or_else(|| panic!("{path:?} lacks {id}"));
        assert_eq!(field.kind, Kind::Path, "{path:?} {id}");
    }
}

#[test]
fn vec_arguments_resolve_to_multi_kind() {
    let root = tree();
    let files = node_at(&root, &["page", "push"])
        .fields
        .iter()
        .find(|f| f.id == "files")
        .expect("page push files");
    assert_eq!(files.kind, Kind::Multi);
    let repos = node_at(&root, &["link", "scope"])
        .fields
        .iter()
        .find(|f| f.id == "repos")
        .expect("link scope repos");
    assert_eq!(repos.kind, Kind::Multi);
}

#[test]
fn value_enum_arguments_resolve_to_select_with_variants() {
    let root = tree();
    let init = node_at(&root, &["init"]);
    let target = init.fields.iter().find(|f| f.id == "target").unwrap();
    assert_eq!(
        target.kind,
        Kind::Select(
            ["zsh", "bash", "fish", "starship", "tmux", "claude"]
                .map(String::from)
                .to_vec()
        )
    );
    let mode = node_at(&root, &["agent", "finish"])
        .fields
        .iter()
        .find(|f| f.id == "status")
        .unwrap();
    assert_eq!(
        mode.kind,
        Kind::Select(
            ["succeeded", "failed", "cancelled"]
                .map(String::from)
                .to_vec()
        )
    );
}

#[test]
fn bool_flags_resolve_to_bool_kind() {
    let root = tree();
    let start = node_at(&root, &["start"]);
    let no_branch = start.fields.iter().find(|f| f.id == "no_branch").unwrap();
    assert_eq!(no_branch.kind, Kind::Bool);
    assert_eq!(no_branch.default.as_deref(), Some("false"));
}

#[test]
fn groups_place_every_visible_leaf_exactly_once() {
    let root = tree();
    let mut expected: Vec<Vec<String>> = root
        .leaves()
        .into_iter()
        .filter(|n| !groups::is_internal(&n.path))
        .map(|n| n.path.clone())
        .collect();
    let mut grouped: Vec<Vec<String>> = groups::groups(&root)
        .into_iter()
        .flat_map(|g| g.entries.into_iter().map(|e| e.path))
        .collect();
    expected.sort();
    grouped.sort();
    assert_eq!(grouped, expected);
    assert!(grouped.windows(2).all(|w| w[0] != w[1]));
}

#[test]
fn internal_paths_never_reach_groups() {
    let root = tree();
    let grouped: Vec<Vec<String>> = groups::groups(&root)
        .into_iter()
        .flat_map(|g| g.entries.into_iter().map(|e| e.path))
        .collect();
    let mut internal: Vec<&[&str]> = vec![
        &["worker"],
        &["link"],
        &["mcp"],
        &["prompt"],
        &["statusline"],
        &["ui"],
        &["hook", "claude"],
        &["hook", "codex"],
        &["git", "hook"],
    ];
    if cfg!(feature = "runner") {
        internal.push(&["runner", "serve"]);
        internal.push(&["queue", "autopilot", "serve"]);
        internal.push(&["autopilot", "serve"]);
    }
    internal.push(&["usage", "statusline-hook"]);
    for path in internal {
        assert!(root.find(&p(path)).is_some(), "{path:?} exists in the tree");
        assert!(groups::is_internal(&p(path)), "{path:?} is internal");
        assert!(
            grouped.iter().all(|g| !g.starts_with(&p(path))),
            "{path:?} leaked into a group"
        );
    }
}

#[test]
fn describe_lists_root_and_leaf_fields() {
    let text = model::describe(&tree());
    assert!(text.starts_with("  --ws text\n"));
    assert!(text.contains("\nstart\n  <reference text required\n"));
    assert!(text.contains("\nws [runnable]\n"));
}

#[test]
fn is_internal_matches_whole_path_prefixes_only() {
    assert!(groups::is_internal(&p(&["hook", "claude", "stop"])));
    assert!(groups::is_internal(&p(&["git", "hook", "commit-msg"])));
    assert!(!groups::is_internal(&p(&["hook", "install"])));
    assert!(!groups::is_internal(&p(&["git", "install-hooks"])));
    assert!(!groups::is_internal(&p(&["usage"])));
}

#[cfg(feature = "runner")]
#[test]
fn runner_commands_exist_only_with_the_feature() {
    let root = tree();
    for path in [
        &["run"][..],
        &["runner", "allow"],
        &["queue", "enable"],
        &["usage", "sync", "on"],
    ] {
        assert!(root.find(&p(path)).is_some(), "{path:?}");
    }
}

#[cfg(not(feature = "runner"))]
#[test]
fn runner_commands_are_absent_without_the_feature() {
    let root = tree();
    for path in [&["run"][..], &["runner"], &["queue"], &["agents"]] {
        assert!(root.find(&p(path)).is_none(), "{path:?}");
    }
}

#[test]
fn quote_leaves_plain_arguments_alone() {
    assert_eq!(cmdline::quote("plain-arg_1.0"), "plain-arg_1.0");
    assert_eq!(
        cmdline::quote("--project=tmgr/x@y:z,%+"),
        "--project=tmgr/x@y:z,%+"
    );
}

#[test]
fn quote_wraps_spaces_quotes_empty_and_unicode() {
    assert_eq!(cmdline::quote("two words"), "'two words'");
    assert_eq!(cmdline::quote("it's"), r"'it'\''s'");
    assert_eq!(cmdline::quote(""), "''");
    assert_eq!(cmdline::quote("$HOME"), "'$HOME'");
    assert_eq!(cmdline::quote("héllo"), "'héllo'");
}

#[test]
fn render_starts_with_tmgr_and_quotes_each_argument() {
    assert_eq!(cmdline::render(&[]), "tmgr");
    assert_eq!(
        cmdline::render(&["add".into(), "x y".into()]),
        "tmgr add 'x y'"
    );
}

#[test]
fn long_options_use_the_equals_form() {
    let root = tree();
    let node = node_at(&root, &["task", "list"]);
    let v = vals(&[("project", text("tmgr")), ("limit", text("10"))]);
    assert_eq!(
        cmdline::build_argv(node, &v, false).unwrap(),
        ["task", "list", "--project=tmgr", "--limit=10"]
    );
}

#[test]
fn option_values_that_look_like_flags_stay_attached() {
    let root = tree();
    let node = node_at(&root, &["task", "list"]);
    for raw in ["--yes", "-x"] {
        let v = vals(&[("project", text(raw))]);
        assert_eq!(
            cmdline::build_argv(node, &v, false).unwrap(),
            [
                "task".to_string(),
                "list".into(),
                format!("--project={raw}")
            ]
        );
    }
}

#[test]
fn positional_starting_with_dash_gets_separator_after_options() {
    let root = tree();
    let node = node_at(&root, &["add"]);
    let with_option = vals(&[("title", text("-draft")), ("project", text("p"))]);
    assert_eq!(
        cmdline::build_argv(node, &with_option, false).unwrap(),
        ["add", "--project=p", "--", "-draft"]
    );
    let bare = vals(&[("title", text("-draft"))]);
    assert_eq!(
        cmdline::build_argv(node, &bare, false).unwrap(),
        ["add", "--", "-draft"]
    );
}

#[test]
fn multi_values_repeat_the_flag_and_drop_empty_items() {
    let root = tree();
    let node = node_at(&root, &["link", "scope"]);
    let v = vals(&[
        ("name", text("work")),
        (
            "repos",
            Value::Multi(vec!["a".into(), "".into(), "b".into()]),
        ),
    ]);
    assert_eq!(
        cmdline::build_argv(node, &v, false).unwrap(),
        ["link", "scope", "--repos=a", "--repos=b", "work"]
    );
}

#[test]
fn bool_flags_render_only_when_true() {
    let root = tree();
    let node = node_at(&root, &["start"]);
    let on = vals(&[
        ("reference", text("TM-1")),
        ("no_branch", Value::Bool(true)),
    ]);
    assert_eq!(
        cmdline::build_argv(node, &on, false).unwrap(),
        ["start", "--no-branch", "TM-1"]
    );
    let off = vals(&[
        ("reference", text("TM-1")),
        ("no_branch", Value::Bool(false)),
    ]);
    assert_eq!(
        cmdline::build_argv(node, &off, false).unwrap(),
        ["start", "TM-1"]
    );
}

#[test]
fn count_flags_repeat_and_zero_is_omitted() {
    let node = synth_node(
        &["x"],
        vec![synth_field("quiet", false, Kind::Count, false)],
    );
    let twice = vals(&[("quiet", Value::Count(2))]);
    assert_eq!(
        cmdline::build_argv(&node, &twice, false).unwrap(),
        ["x", "--quiet", "--quiet"]
    );
    let none = vals(&[("quiet", Value::Count(0))]);
    assert_eq!(cmdline::build_argv(&node, &none, false).unwrap(), ["x"]);
}

#[test]
fn missing_required_positional_is_an_error() {
    let root = tree();
    let node = node_at(&root, &["add"]);
    assert_eq!(
        cmdline::build_argv(node, &Values::new(), false).unwrap_err(),
        "title is required"
    );
    let init = node_at(&root, &["init"]);
    assert_eq!(
        cmdline::build_argv(init, &Values::new(), false).unwrap_err(),
        "target is required"
    );
}

#[test]
fn gap_before_a_filled_positional_is_an_error() {
    let node = synth_node(
        &["x"],
        vec![
            synth_field("alpha", true, Kind::Text, false),
            synth_field("beta", true, Kind::Text, false),
        ],
    );
    let v = vals(&[("beta", text("b"))]);
    assert_eq!(
        cmdline::build_argv(&node, &v, false).unwrap_err(),
        "fill alpha first"
    );
    let filled = vals(&[("alpha", text("a")), ("beta", text("b"))]);
    assert_eq!(
        cmdline::build_argv(&node, &filled, false).unwrap(),
        ["x", "a", "b"]
    );
}

#[test]
fn line_breaks_and_nul_in_values_are_rejected() {
    let root = tree();
    let add = node_at(&root, &["add"]);
    for bad in ["a\nb", "a\rb", "a\0b"] {
        let v = vals(&[("title", text(bad))]);
        assert_eq!(
            cmdline::build_argv(add, &v, false).unwrap_err(),
            "values cannot contain line breaks or control characters",
            "{bad:?}"
        );
    }
    let list = node_at(&root, &["task", "list"]);
    let v = vals(&[("project", text("x\ny"))]);
    assert!(cmdline::build_argv(list, &v, false).is_err());
}

#[test]
fn tilde_expands_only_for_path_fields() {
    let home = std::env::var("HOME").expect("HOME is set");
    assert_eq!(cmdline::expand_tilde("~"), home);
    assert_eq!(cmdline::expand_tilde("~/x/y"), format!("{home}/x/y"));
    assert_eq!(cmdline::expand_tilde("~user/x"), "~user/x");
    assert_eq!(cmdline::expand_tilde("a~/x"), "a~/x");

    let root = tree();
    let map = node_at(&root, &["project", "map"]);
    let v = vals(&[("code", text("~/raw")), ("path", text("~/proj"))]);
    assert_eq!(
        cmdline::build_argv(map, &v, false).unwrap(),
        vec![
            "project".to_string(),
            "map".into(),
            "~/raw".into(),
            format!("{home}/proj")
        ]
    );

    let acc = node_at(&root, &["usage", "accounts", "add"]);
    let v = vals(&[
        ("tool", text("claude")),
        ("root", text("~")),
        ("label", text("~/label")),
    ]);
    assert_eq!(
        cmdline::build_argv(acc, &v, false).unwrap(),
        [
            "usage".to_string(),
            "accounts".into(),
            "add".into(),
            "--tool=claude".into(),
            format!("--root={home}"),
            "--label=~/label".into(),
        ]
    );
}

#[test]
fn sensitive_fields_are_masked_only_when_asked() {
    let root = tree();
    let node = node_at(&root, &["config", "set"]);
    let v = vals(&[("key", text("api.token")), ("value", text("v"))]);
    let masked = cmdline::build_argv(node, &v, true).unwrap();
    assert_eq!(masked, ["config", "set", "api.token", "<hidden>"]);
    let raw = cmdline::build_argv(node, &v, false).unwrap();
    assert_eq!(raw, ["config", "set", "api.token", "v"]);
    assert_eq!(
        cmdline::render(&masked),
        "tmgr config set api.token '<hidden>'"
    );
}

#[test]
fn ws_is_passed_as_a_global_option_of_the_form() {
    let root = tree();
    let mut node = node_at(&root, &["whoami"]).clone();
    node.fields.extend(root.fields.iter().cloned());
    let v = vals(&[("ws", text("tmgr-dev"))]);
    assert_eq!(
        cmdline::build_argv(&node, &v, false).unwrap(),
        ["whoami", "--ws=tmgr-dev"]
    );
}

#[test]
fn scrub_drops_sensitive_fields() {
    let root = tree();
    let node = node_at(&root, &["config", "set"]);
    let v = vals(&[("key", text("api.token")), ("value", text("v"))]);
    let scrubbed = history::scrub(node, &v);
    assert_eq!(scrubbed, vals(&[("key", text("api.token"))]));

    let login = node_at(&root, &["auth", "login"]);
    let v = vals(&[("with_token", Value::Bool(true)), ("label", text("ci"))]);
    assert_eq!(history::scrub(login, &v), vals(&[("label", text("ci"))]));
}

#[test]
fn credential_commands_are_never_recorded() {
    let root = tree();
    for path in [
        &["login"][..],
        &["auth", "login"],
        &["auth", "logout"],
        &["config", "set"],
        &["config", "get"],
        &["notify"],
        &["alarm"],
        &["link", "add"],
        &["agents", "login"],
        &["agents", "logout"],
        &["agents", "sync"],
        &["usage", "accounts", "list"],
    ] {
        if root.find(&p(path)).is_none() {
            continue;
        }
        assert!(!history::recordable(node_at(&root, path)), "{path:?}");
    }
    let mut seen = 0;
    for path in [
        &["add"][..],
        &["start"],
        &["task", "list"],
        &["usage"],
        &["usage", "pricing", "show"],
    ] {
        assert!(history::recordable(node_at(&root, path)), "{path:?}");
        seen += 1;
    }
    assert_eq!(seen, 5);
}

#[test]
fn record_skips_never_recorded_commands() {
    let root = tree();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ui-history.json");
    history::record(
        &file,
        node_at(&root, &["config", "set"]),
        &vals(&[("key", text("k")), ("value", text("v"))]),
    );
    assert!(!file.exists());
}

#[test]
fn history_file_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let root = tree();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ui-history.json");
    history::record(
        &file,
        node_at(&root, &["add"]),
        &vals(&[("title", text("a"))]),
    );
    let mode = std::fs::metadata(&file).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn history_keeps_at_most_forty_entries_newest_first() {
    let root = tree();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ui-history.json");
    for i in 0..45 {
        history::record(
            &file,
            node_at(&root, &["add"]),
            &vals(&[("title", text(&format!("t{i}")))]),
        );
    }
    let all = history::load(&file);
    assert_eq!(all.len(), 40);
    assert_eq!(all[0].values, vals(&[("title", text("t44"))]));
    assert_eq!(all[39].values, vals(&[("title", text("t5"))]));
}

#[test]
fn repeating_an_entry_moves_it_to_the_front() {
    let root = tree();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ui-history.json");
    let add = node_at(&root, &["add"]);
    history::record(&file, add, &vals(&[("title", text("A"))]));
    history::record(&file, add, &vals(&[("title", text("B"))]));
    history::record(&file, add, &vals(&[("title", text("A"))]));
    let all = history::load(&file);
    let titles: Vec<&str> = all
        .iter()
        .map(|r| match r.values.get("title") {
            Some(Value::Text(t)) => t.as_str(),
            _ => "",
        })
        .collect();
    assert_eq!(titles, ["A", "B"]);
}

#[test]
fn corrupt_or_missing_history_loads_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ui-history.json");
    assert!(history::load(&file).is_empty());
    std::fs::write(&file, "{not json").unwrap();
    assert!(history::load(&file).is_empty());
}

#[test]
fn every_non_runner_guard_rule_resolves_and_runner_rules_do_not_exist_without_feature() {
    let root = tree();
    for path in [
        &["task", "confirm"][..],
        &["init"],
        &["git", "install-hooks"],
        &["hook", "install"],
        &["usage", "sync", "on"],
    ] {
        let rule = guard::GUARDED
            .iter()
            .find(|r| r.path == path)
            .unwrap_or_else(|| panic!("rule {path:?}"));
        assert!(guard::rule_resolves(&root, rule), "{path:?}");
    }
    #[cfg(not(feature = "runner"))]
    for rule in guard::GUARDED
        .iter()
        .filter(|r| matches!(r.path[0], "run" | "queue" | "runner"))
    {
        assert!(!guard::rule_resolves(&root, rule), "{:?}", rule.path);
    }
}

#[cfg(feature = "runner")]
#[test]
fn every_runner_guard_rule_resolves() {
    let root = tree();
    for rule in guard::GUARDED.iter().filter(|r| r.path != ["self-update"]) {
        assert!(
            guard::rule_resolves(&root, rule),
            "{:?} {:?}",
            rule.path,
            rule.arg
        );
    }
}

#[test]
fn self_update_guard_follows_the_allow_downgrade_field() {
    let root = tree();
    let node = node_at(&root, &["self-update"]);
    assert!(guard::is_guarded(
        node,
        &vals(&[("allow_downgrade", Value::Bool(true))])
    ));
}

#[cfg(feature = "runner")]
#[test]
fn run_is_guarded_only_for_bypass_trust_and_publish_flags() {
    let root = tree();
    let run = node_at(&root, &["run"]);
    assert!(guard::is_guarded(run, &vals(&[("mode", text("bypass"))])));
    assert!(guard::is_guarded(
        run,
        &vals(&[("trust", Value::Bool(true))])
    ));
    assert!(guard::is_guarded(
        run,
        &vals(&[("no_sandbox", Value::Bool(true))])
    ));
    assert!(!guard::is_guarded(run, &vals(&[("mode", text("ask"))])));
    assert!(!guard::is_guarded(run, &Values::new()));
}

#[test]
fn guard_for_plain_and_flag_guarded_commands() {
    let root = tree();
    assert!(guard::is_guarded(
        node_at(&root, &["task", "confirm"]),
        &Values::new()
    ));
    assert!(guard::is_guarded(
        node_at(&root, &["hook", "install"]),
        &Values::new()
    ));
    assert!(!guard::is_guarded(
        node_at(&root, &["status"]),
        &Values::new()
    ));
    let init = node_at(&root, &["init"]);
    assert!(!guard::is_guarded(
        init,
        &vals(&[("install", Value::Bool(false))])
    ));
    assert!(guard::is_guarded(
        init,
        &vals(&[("install", Value::Bool(true))])
    ));
}

#[test]
fn fuzzy_empty_query_matches_everything_in_order() {
    assert_eq!(fuzzy::score("", "anything"), Some(0));
    let items = vec!["b".to_string(), "a".to_string()];
    assert_eq!(fuzzy::rank("", items, |s| s.clone()), ["b", "a"]);
}

#[test]
fn fuzzy_non_match_is_none_and_order_matters() {
    assert_eq!(fuzzy::score("z", "abc"), None);
    assert_eq!(fuzzy::score("ba", "ab"), None);
    assert!(fuzzy::matched("zz", "abc").is_empty());
}

#[test]
fn fuzzy_is_case_insensitive() {
    assert!(fuzzy::score("AB", "xaby").is_some());
    assert_eq!(fuzzy::matched("AB", "xaxb"), [1, 3]);
}

#[test]
fn fuzzy_prefers_prefix_and_consecutive_matches() {
    let prefix = fuzzy::score("ab", "ab").unwrap();
    let middle = fuzzy::score("ab", "xab").unwrap();
    let scattered = fuzzy::score("ab", "a_x_b").unwrap();
    assert!(prefix > middle);
    assert!(middle > scattered);
}

#[test]
fn fuzzy_rank_orders_by_score_then_original_position() {
    let items = vec![
        "last stop".to_string(),
        "start".to_string(),
        "status".to_string(),
    ];
    assert_eq!(
        fuzzy::rank("st", items, |s| s.clone()),
        ["start", "status", "last stop"]
    );
}

#[test]
fn fuzzy_matched_reports_positions_in_order() {
    assert_eq!(fuzzy::matched("ab", "xaxb"), [1, 3]);
    assert_eq!(fuzzy::matched("", "x"), Vec::<usize>::new());
}

#[test]
fn down_then_enter_opens_the_selected_command() {
    let mut app = fresh();
    app.on_key(key(KeyCode::Enter));
    app.on_key(key(KeyCode::Down));
    let expected = app.groups[0].entries[1].path.clone();
    assert!(app.on_key(key(KeyCode::Enter)).is_none());
    assert_eq!(form(&app).node.path, expected);
}

#[test]
fn esc_returns_from_a_form_to_home() {
    let mut app = fresh();
    app.open(&p(&["add"]), Values::new());
    app.on_key(key(KeyCode::Esc));
    assert!(matches!(app.screen, Screen::Home));
}

#[test]
fn typing_in_a_text_field_appends_and_backspace_removes() {
    let mut app = fresh();
    app.open(&p(&["add"]), Values::new());
    type_str(&mut app, "ab");
    assert_eq!(form(&app).values.get("title"), Some(&text("ab")));
    app.on_key(key(KeyCode::Backspace));
    assert_eq!(form(&app).values.get("title"), Some(&text("a")));
}

#[test]
fn space_toggles_a_bool_field() {
    let mut app = fresh();
    app.open(&p(&["start"]), Values::new());
    focus_field(&mut app, "no_branch");
    app.on_key(ch(' '));
    assert_eq!(form(&app).values.get("no_branch"), Some(&Value::Bool(true)));
    app.on_key(ch(' '));
    assert_eq!(
        form(&app).values.get("no_branch"),
        Some(&Value::Bool(false))
    );
}

#[test]
fn right_and_left_cycle_a_select_field_through_default() {
    let mut app = fresh();
    app.open(&p(&["init"]), Values::new());
    focus_field(&mut app, "target");
    app.on_key(key(KeyCode::Right));
    assert_eq!(form(&app).values.get("target"), Some(&text("zsh")));
    app.on_key(key(KeyCode::Right));
    assert_eq!(form(&app).values.get("target"), Some(&text("bash")));
    app.on_key(key(KeyCode::Left));
    assert_eq!(form(&app).values.get("target"), Some(&text("zsh")));
}

#[test]
fn enter_on_a_complete_unguarded_form_runs_the_command() {
    let mut app = fresh();
    app.open(&p(&["add"]), Values::new());
    type_str(&mut app, "Hello");
    match app.on_key(key(KeyCode::Enter)) {
        Some(Action::Run { argv, .. }) => assert_eq!(argv, ["add", "Hello"]),
        _ => panic!("expected a run action"),
    }
}

#[test]
fn enter_with_a_missing_required_field_sets_an_error() {
    let mut app = fresh();
    app.open(&p(&["add"]), Values::new());
    assert!(app.on_key(key(KeyCode::Enter)).is_none());
    assert_eq!(form(&app).error.as_deref(), Some("title is required"));
}

#[test]
fn guarded_form_asks_for_review_then_runs_on_second_enter() {
    let mut app = fresh();
    app.open(&p(&["task", "confirm"]), Values::new());
    type_str(&mut app, "TM-1");
    assert!(app.on_key(key(KeyCode::Enter)).is_none());
    assert!(form(&app).review);
    match app.on_key(key(KeyCode::Enter)) {
        Some(Action::Run { argv, .. }) => assert_eq!(argv, ["task", "confirm", "TM-1"]),
        _ => panic!("expected a run action"),
    }
}

#[test]
fn any_other_key_cancels_the_guard_review() {
    let mut app = fresh();
    app.open(&p(&["task", "confirm"]), Values::new());
    type_str(&mut app, "TM-1");
    app.on_key(key(KeyCode::Enter));
    assert!(app.on_key(ch('x')).is_none());
    assert!(!form(&app).review);
    assert_eq!(form(&app).values.get("reference"), Some(&text("TM-1")));
}

#[test]
fn ctrl_y_copies_the_command_with_sensitive_values_masked() {
    let mut app = fresh();
    app.open(&p(&["config", "set"]), Values::new());
    type_str(&mut app, "api.token");
    app.on_key(key(KeyCode::Tab));
    type_str(&mut app, "secretvalue");
    match app.on_key(ctrl('y')) {
        Some(Action::Copy(text)) => {
            assert_eq!(text, "tmgr config set api.token '<hidden>'");
            assert!(!text.contains("secret"));
        }
        _ => panic!("expected a copy action"),
    }
}

#[test]
fn slash_opens_palette_filters_and_enter_opens_the_hit() {
    let mut app = fresh();
    app.on_key(ch('/'));
    assert!(app.palette.is_some());
    type_str(&mut app, "add");
    let palette = app.palette.as_ref().unwrap();
    assert_eq!(palette.query, "add");
    assert!(matches!(
        palette.hits.first(),
        Some(app::Hit::Command(path)) if *path == p(&["add"])
    ));
    assert!(app.on_key(key(KeyCode::Enter)).is_none());
    assert!(app.palette.is_none());
    assert_eq!(form(&app).node.path, p(&["add"]));
}

#[test]
fn q_on_home_quits() {
    let mut app = fresh();
    assert!(matches!(app.on_key(ch('q')), Some(Action::Quit)));
}

#[test]
fn q_inside_a_text_field_types_the_letter() {
    let mut app = fresh();
    app.open(&p(&["add"]), Values::new());
    assert!(app.on_key(ch('q')).is_none());
    assert_eq!(form(&app).values.get("title"), Some(&text("q")));
}

#[test]
fn ctrl_c_quits_from_every_screen() {
    let mut home = fresh();
    assert!(matches!(home.on_key(ctrl('c')), Some(Action::Quit)));

    let mut form_app = fresh();
    form_app.open(&p(&["add"]), Values::new());
    assert!(matches!(form_app.on_key(ctrl('c')), Some(Action::Quit)));

    let mut palette_app = fresh();
    palette_app.on_key(ch('/'));
    assert!(matches!(palette_app.on_key(ctrl('c')), Some(Action::Quit)));

    let mut done_app = fresh();
    done_app.finish(app::Done {
        node: node_at(&done_app.root, &["add"]).clone(),
        values: Values::new(),
        command: "tmgr add x".into(),
        code: Some(0),
    });
    assert!(matches!(done_app.on_key(ctrl('c')), Some(Action::Quit)));
}

#[test]
fn done_screen_edit_reopens_the_same_values() {
    let mut app = fresh();
    let node = node_at(&app.root, &["add"]).clone();
    app.finish(app::Done {
        node,
        values: vals(&[("title", text("Again"))]),
        command: "tmgr add Again".into(),
        code: Some(1),
    });
    app.on_key(ch('e'));
    assert_eq!(form(&app).values.get("title"), Some(&text("Again")));
}

#[test]
fn done_screen_enter_returns_home() {
    let mut app = fresh();
    let node = node_at(&app.root, &["add"]).clone();
    app.finish(app::Done {
        node,
        values: Values::new(),
        command: "tmgr add".into(),
        code: Some(0),
    });
    app.on_key(key(KeyCode::Enter));
    assert!(matches!(app.screen, Screen::Home));
}

fn plain_look() -> view::Look {
    view::Look {
        level: crate::cmd::board::theme::ColorLevel::None,
    }
}

fn screen_text(app: &App) -> String {
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| view::draw(f, app, plain_look())).unwrap();
    let buf = term.backend().buffer();
    let width = buf.area.width as usize;
    let mut out = String::new();
    for row in buf.content.chunks(width) {
        let line: String = row.iter().map(|c| c.symbol()).collect();
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

#[cfg(not(feature = "runner"))]
#[test]
fn render_home_snapshot() {
    check_snapshot(
        &screen_text(&home_app()),
        "render_home.txt",
        include_str!("snapshots/render_home.txt"),
    );
}

#[cfg(feature = "runner")]
#[test]
fn render_home_snapshot() {
    check_snapshot(
        &screen_text(&home_app()),
        "render_home_runner.txt",
        include_str!("snapshots/render_home_runner.txt"),
    );
}

#[test]
fn render_add_form_snapshot() {
    let mut app = home_app();
    app.open(
        &p(&["add"]),
        vals(&[("title", text("Ship the menu")), ("project", text("tmgr"))]),
    );
    check_snapshot(
        &screen_text(&app),
        "render_add_form.txt",
        include_str!("snapshots/render_add_form.txt"),
    );
}

fn palette_app() -> App {
    let mut app = home_app();
    app.on_key(ch('/'));
    type_str(&mut app, "done");
    app
}

#[cfg(not(feature = "runner"))]
#[test]
fn render_palette_snapshot() {
    check_snapshot(
        &screen_text(&palette_app()),
        "render_palette_done.txt",
        include_str!("snapshots/render_palette_done.txt"),
    );
}

#[cfg(feature = "runner")]
#[test]
fn render_palette_snapshot() {
    check_snapshot(
        &screen_text(&palette_app()),
        "render_palette_done_runner.txt",
        include_str!("snapshots/render_palette_done_runner.txt"),
    );
}

#[test]
fn render_guarded_review_snapshot() {
    let mut app = home_app();
    app.open(&p(&["task", "confirm"]), Values::new());
    type_str(&mut app, "TM-1");
    app.on_key(key(KeyCode::Enter));
    check_snapshot(
        &screen_text(&app),
        "render_guarded_review.txt",
        include_str!("snapshots/render_guarded_review.txt"),
    );
}

#[test]
fn render_done_snapshot() {
    let mut app = home_app();
    let node = node_at(&app.root, &["add"]).clone();
    app.finish(app::Done {
        node,
        values: vals(&[("title", text("Ship the menu"))]),
        command: "tmgr add 'Ship the menu'".into(),
        code: Some(0),
    });
    check_snapshot(
        &screen_text(&app),
        "render_done.txt",
        include_str!("snapshots/render_done.txt"),
    );
}

#[test]
fn a_paste_with_line_breaks_never_submits_and_lands_in_the_focused_field() {
    let mut app = fresh();
    app.open(&p(&["add"]), Values::new());
    app.on_paste("first line\nsecond\r\n\x1b[31mred");
    assert_eq!(
        form(&app).values.get("title"),
        Some(&text("first linesecond[31mred"))
    );
    assert!(matches!(app.screen, Screen::Form(_)));
}

#[test]
fn a_paste_goes_into_the_palette_query_when_it_is_open() {
    let mut app = fresh();
    app.on_key(ch('/'));
    app.on_paste("rest\nart");
    assert_eq!(app.palette.as_ref().unwrap().query, "restart");
}

#[test]
fn a_paste_is_ignored_during_the_guard_review() {
    let mut app = fresh();
    app.open(&p(&["task", "confirm"]), Values::new());
    type_str(&mut app, "TM-1");
    app.on_key(key(KeyCode::Enter));
    assert!(form(&app).review);
    app.on_paste("yes");
    assert_eq!(form(&app).values.get("reference"), Some(&text("TM-1")));
}

#[test]
fn history_leaves_no_temporary_file_behind() {
    let root = tree();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ui-history.json");
    history::record(
        &file,
        node_at(&root, &["start"]),
        &vals(&[("reference", text("TM-1"))]),
    );
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["ui-history.json"]);
}

#[test]
fn history_round_trips_a_recorded_command() {
    let root = tree();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ui-history.json");
    history::record(
        &file,
        node_at(&root, &["start"]),
        &vals(&[("reference", text("TM-1"))]),
    );
    let loaded = history::load(&file);
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].path, p(&["start"]));
}

fn picker_items(n: &[(&str, &str)]) -> Vec<picker::Item> {
    n.iter()
        .map(|(v, t)| picker::Item {
            value: v.to_string(),
            cols: vec![v.to_string(), t.to_string()],
            preview: vec![t.to_string()],
        })
        .collect()
}

#[test]
fn pickers_are_bound_to_task_session_workspace_and_profile_fields() {
    let root = tree();
    let field = |path: &[&str], id: &str| {
        node_at(&root, path)
            .fields
            .iter()
            .find(|f| f.id == id)
            .unwrap_or_else(|| panic!("{path:?} {id}"))
            .clone()
    };
    let src = |path: &[&str], id: &str| picker::source_for(&p(path), &field(path, id));
    assert_eq!(src(&["start"], "reference"), Some(picker::Source::Tasks));
    assert_eq!(src(&["done"], "reference"), Some(picker::Source::Tasks));
    assert_eq!(src(&["log"], "task"), Some(picker::Source::Tasks));
    assert_eq!(
        src(&["ws", "use"], "code"),
        Some(picker::Source::Workspaces)
    );
    assert_eq!(src(&["add"], "title"), None);
    #[cfg(feature = "runner")]
    {
        assert_eq!(src(&["peek"], "session"), Some(picker::Source::Sessions));
        assert_eq!(src(&["run"], "agent"), Some(picker::Source::Profiles));
        assert_eq!(src(&["run"], "task"), Some(picker::Source::Tasks));
    }
}

#[test]
fn task_answer_becomes_items_with_a_preview() {
    let json = serde_json::json!({"ok": true, "data": {"items": [
        {"ref": "TM-7", "title": "Open in Desktop", "status": {"name": "In progress"}, "project": "TM", "updated_at": "2026-10-20"},
        {"ref": "#9", "title": "No status", "status": null, "project": null}
    ]}});
    let items = picker::tasks(&json).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].value, "TM-7");
    assert_eq!(
        items[0].cols,
        ["TM-7", "Open in Desktop", "In progress", "TM"]
    );
    assert!(items[0].preview.iter().any(|l| l.contains("In progress")));
    assert_eq!(items[1].value, "#9");
    assert!(picker::tasks(&serde_json::json!({"data": {}})).is_err());
}

#[test]
fn workspace_answer_prefixes_local_codes() {
    let json = serde_json::json!({"ok": true, "data": {"workspaces": [
        {"code": "tmgrdev", "name": "TMGR", "current": true, "local": false},
        {"code": "notes", "name": "Notes", "current": false, "local": true}
    ]}});
    let items = picker::workspaces(&json).unwrap();
    assert_eq!(items[0].value, "tmgrdev");
    assert_eq!(items[1].value, "local:notes");
}

#[test]
fn session_answer_fills_the_session_id() {
    let json = serde_json::json!({"ok": true, "data": {"sessions": [
        {"id": "s1", "name": "cc", "cwd": "/w", "state": {"kind": "working"}, "profile": "claude-code"},
        {"id": "s2", "name": "", "cwd": "", "state": {"kind": "exited"}}
    ]}});
    let items = picker::sessions(&json).unwrap();
    assert_eq!(items[0].value, "s1");
    assert_eq!(items[1].value, "s2");
}

#[test]
fn picker_filter_ranks_matches_and_drops_the_rest() {
    let items = picker_items(&[
        ("TM-1", "Fix login"),
        ("TM-2", "Desktop bar"),
        ("TM-3", "Docs"),
    ]);
    assert_eq!(picker::filter(&items, ""), [0, 1, 2]);
    assert_eq!(picker::filter(&items, "desk"), [1]);
    assert!(picker::filter(&items, "zzz").is_empty());
}

#[test]
fn ctrl_o_on_a_bound_field_loads_and_enter_fills_the_field() {
    let mut app = fresh();
    app.open(&p(&["start"]), Values::new());
    let action = app.on_key(ctrl('o'));
    assert!(matches!(
        action,
        Some(app::Action::Load {
            source: picker::Source::Tasks,
            ws: None,
            ..
        })
    ));
    assert!(app.picker.is_some());
    app.picker_loaded(
        app.picker_generation,
        Ok(picker_items(&[
            ("TM-1", "Fix login"),
            ("TM-2", "Desktop bar"),
        ])),
    );
    type_str(&mut app, "desk");
    assert_eq!(app.picker.as_ref().unwrap().shown, [1]);
    assert!(app.on_key(key(KeyCode::Enter)).is_none());
    assert!(app.picker.is_none());
    assert_eq!(form(&app).values.get("reference"), Some(&text("TM-2")));
}

#[test]
fn esc_closes_the_picker_without_touching_the_field() {
    let mut app = fresh();
    app.open(&p(&["start"]), Values::new());
    app.on_key(ctrl('o'));
    app.picker_loaded(app.picker_generation, Ok(picker_items(&[("TM-1", "a")])));
    app.on_key(key(KeyCode::Esc));
    assert!(app.picker.is_none());
    assert!(!form(&app).values.contains_key("reference"));
}

#[test]
fn ctrl_o_on_an_unbound_field_does_nothing() {
    let mut app = fresh();
    app.open(&p(&["add"]), Values::new());
    assert!(app.on_key(ctrl('o')).is_none());
    assert!(app.picker.is_none());
}

#[test]
fn picker_load_uses_the_workspace_field_of_the_form() {
    let mut app = fresh();
    app.open(&p(&["start"]), vals(&[("ws", text("local:notes"))]));
    match app.on_key(ctrl('o')) {
        Some(app::Action::Load { ws, .. }) => assert_eq!(ws.as_deref(), Some("local:notes")),
        _ => panic!("expected a load action"),
    }
}

#[test]
fn a_failed_load_shows_the_error_and_a_late_answer_for_another_source_is_ignored() {
    let mut app = fresh();
    app.open(&p(&["start"]), Values::new());
    app.on_key(ctrl('o'));
    app.picker_loaded(app.picker_generation - 1, Ok(picker_items(&[("x", "y")])));
    assert!(app.picker.as_ref().unwrap().items.is_none());
    app.picker_loaded(app.picker_generation, Err("not logged in".into()));
    assert_eq!(
        app.picker.as_ref().unwrap().error.as_deref(),
        Some("not logged in")
    );
}

#[test]
fn control_characters_in_values_are_rejected() {
    let root = tree();
    let node = node_at(&root, &["add"]);
    for bad in ["a\tb", "a\x1b[31mb", "a\x07b", "a\x7fb"] {
        let v = vals(&[("title", text(bad))]);
        assert!(cmdline::build_argv(node, &v, false).is_err(), "{bad:?}");
    }
}

#[test]
fn capture_returns_output_and_status_of_a_quick_child() {
    let (ok, text) = capture::capture(
        std::path::Path::new("/bin/echo"),
        &["hello".to_string()],
        std::time::Duration::from_secs(5),
    )
    .unwrap();
    assert!(ok);
    assert_eq!(text.trim(), "hello");
}

#[test]
fn capture_kills_a_child_that_outlives_the_limit() {
    let started = std::time::Instant::now();
    let err = capture::capture(
        std::path::Path::new("/bin/sleep"),
        &["30".to_string()],
        std::time::Duration::from_millis(300),
    )
    .unwrap_err();
    assert_eq!(err, "timed out");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn only_the_task_list_follows_the_workspace_field() {
    assert_eq!(
        picker::ws_args(picker::Source::Tasks, Some("local:notes")),
        ["--ws=local:notes"]
    );
    assert!(picker::ws_args(picker::Source::Workspaces, Some("half typed")).is_empty());
    assert!(picker::ws_args(picker::Source::Profiles, Some("x")).is_empty());
    assert!(picker::ws_args(picker::Source::Tasks, Some("")).is_empty());
}

#[test]
fn profile_pickers_skip_commands_whose_agent_is_not_a_profile_id() {
    let root = tree();
    let agent = |path: &[&str]| {
        node_at(&root, path)
            .fields
            .iter()
            .find(|f| f.id == "agent")
            .cloned()
            .map(|f| picker::source_for(&p(path), &f))
    };
    assert_eq!(agent(&["log"]), Some(None));
    #[cfg(feature = "runner")]
    assert_eq!(agent(&["run"]), Some(Some(picker::Source::Profiles)));
    #[cfg(not(feature = "runner"))]
    assert_eq!(agent(&["agent", "start"]), Some(None));
}

#[test]
fn server_text_in_picker_items_has_no_control_characters() {
    let json = serde_json::json!({"ok": true, "data": {"items": [
        {"ref": "TM-1", "title": "a\u{1b}[31mred\u{7}", "status": {"name": "x\ny"}, "project": "P\tQ"}
    ]}});
    let items = picker::tasks(&json).unwrap();
    let all = items[0].cols.iter().chain(items[0].preview.iter());
    for text in all {
        assert!(!text.chars().any(|c| c.is_control()), "{text:?}");
    }
    assert!(!items[0].value.chars().any(|c| c.is_control()));
}

#[test]
fn a_paste_goes_into_the_picker_filter() {
    let mut app = fresh();
    app.open(&p(&["start"]), Values::new());
    app.on_key(ctrl('o'));
    app.picker_loaded(
        app.picker_generation,
        Ok(picker_items(&[
            ("TM-1", "Fix login"),
            ("TM-2", "Desktop bar"),
        ])),
    );
    app.on_paste("desk\n");
    assert_eq!(app.picker.as_ref().unwrap().query, "desk");
    assert_eq!(app.picker.as_ref().unwrap().shown, [1]);
}

#[test]
fn picking_into_a_multi_field_appends_without_commas() {
    let mut app = fresh();
    app.open(&p(&["page", "push"]), Values::new());
    match &mut app.screen {
        Screen::Form(f) => {
            f.pick(0, "a,b".into());
            f.pick(0, "c".into());
            assert_eq!(
                f.values.get("files"),
                Some(&Value::Multi(vec!["a b".into(), "c".into()]))
            );
        }
        _ => panic!("expected a form"),
    }
}

#[test]
fn render_task_picker_snapshot() {
    let mut app = home_app();
    app.open(&p(&["start"]), Values::new());
    app.on_key(ctrl('o'));
    let json = serde_json::json!({"ok": true, "data": {"items": [
        {"ref": "TM-644", "title": "Open in TMGR Desktop bar", "status": {"name": "In progress"}, "project": "frontend", "updated_at": "2026-10-20"},
        {"ref": "TM-639", "title": "Desktop: inventory of features", "status": {"name": "In progress"}, "project": "docs", "updated_at": "2026-10-19"},
        {"ref": "TM-438", "title": "Desktop without account, phase 2", "status": {"name": "Backlog"}, "project": "frontend", "updated_at": "2026-10-01"},
        {"ref": "TM-301", "title": "Fix login redirect", "status": {"name": "Review"}, "project": "web", "updated_at": "2026-09-30"}
    ]}});
    app.picker_loaded(app.picker_generation, picker::tasks(&json));
    type_str(&mut app, "desk");
    check_snapshot(
        &screen_text(&app),
        "render_task_picker.txt",
        include_str!("snapshots/render_task_picker.txt"),
    );
}
