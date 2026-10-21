use super::keys::Cmd;
use super::tree::Nav;

#[derive(Clone, Copy, Debug)]
pub struct ActionDef {
    pub cmd: Cmd,
    pub keys: &'static [u8],
    pub hotkey: &'static str,
    pub label: &'static str,
    pub palette: bool,
}

const fn def(
    cmd: Cmd,
    keys: &'static [u8],
    hotkey: &'static str,
    label: &'static str,
    palette: bool,
) -> ActionDef {
    ActionDef {
        cmd,
        keys,
        hotkey,
        label,
        palette,
    }
}

pub const ACTIONS: &[ActionDef] = &[
    def(
        Cmd::Detach,
        b"d",
        "d",
        "Detach (sessions keep running)",
        true,
    ),
    def(Cmd::NextSession, b"n", "n", "Next session", true),
    def(Cmd::PrevSession, b"p", "p", "Previous session", true),
    def(Cmd::Split, b"s", "s", "Two panes, or collapse to one", true),
    def(
        Cmd::SplitRight,
        b"|v",
        "| v",
        "New shell to the right, same folder",
        true,
    ),
    def(
        Cmd::SplitDown,
        b"-h",
        "- h",
        "New shell below, same folder",
        true,
    ),
    def(
        Cmd::NewSession,
        b"c",
        "c",
        "New session (claude, codex, shell, ...)",
        true,
    ),
    def(Cmd::FocusOther, b"\t", "Tab", "Focus next pane", true),
    def(
        Cmd::FocusPrev,
        b"",
        "Shift-Tab",
        "Focus previous pane",
        true,
    ),
    def(Cmd::Zoom, b"z", "z", "Zoom the pane", true),
    def(Cmd::Sidebar, b"b", "b", "Show or hide the sidebar", true),
    def(Cmd::Rename, b"r", "r", "Rename the focused session", true),
    def(
        Cmd::Resume,
        b"\rR",
        "Enter / R",
        "Resume the focused session",
        true,
    ),
    def(
        Cmd::Stop,
        b"x",
        "x",
        "Stop (close) the focused session",
        true,
    ),
    def(
        Cmd::Remove,
        b"X",
        "X",
        "Remove (close) the focused exited session",
        true,
    ),
    def(
        Cmd::ScrollMode,
        b"[",
        "[",
        "Scroll back through history",
        true,
    ),
    def(Cmd::Help, b"?", "?", "Show the key help", true),
    def(Cmd::Picker, b"w", "w", "Session picker", false),
    def(Cmd::Palette, b":", ":", "Command palette", false),
    def(Cmd::Focus(Nav::Left), b"", "arrows", "Move focus", false),
];

pub fn find(cmd: Cmd) -> Option<&'static ActionDef> {
    ACTIONS.iter().find(|a| a.cmd == cmd)
}
