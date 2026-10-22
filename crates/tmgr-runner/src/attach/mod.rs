mod actions;
mod attention;
mod broadcast;
mod chip;
mod keys;
mod modes;
mod mouse;
mod newsession;
mod palette;
mod place;
mod prompts;
mod qmenu;
mod render;
mod signal;
mod status;
mod theme;
pub(crate) mod title;
mod tree;
mod usage;
mod view;

#[cfg(test)]
mod tests;

pub use actions::{ActionDef, ACTIONS};
pub use attention::AttentionTracker;
pub use broadcast::{peer_bytes, Broadcast};
pub use chip::{chip_text, open_urls, TaskChip};
pub use keys::{Action, Cmd, InputSplitter, ScrollKey};
pub use modes::{enter_mouse, focus_on, mode_changes, reset_modes};
pub use mouse::{encode, wants, Kind, MouseEvent};
pub use newsession::{
    KindOpt, NewEdit, NewField, NewRequest, NewSession, Place, SpawnKind, TaskOpt,
};
pub use palette::{Palette, PaletteEdit, PaletteEntry, PaletteMode, RowView};
pub use place::{fit, probe, tail2, Loc};
pub use prompts::{defaults as default_prompts, paste_bytes, Prompt, PromptForm};
pub use prompts::{parse as parse_prompts, render as render_prompts};
pub use qmenu::{find_question, goto_target, GotoMark, QButton, QEdit, QMenu};
pub use render::{cursor, render, ScreenSource};
pub use signal::{deliver, detect, Need, NeedKind, Notifier, SignalSettings, SignalTracker};
pub use status::{
    activity_label, peek_lines, Peek, PeekEdit, SessionStatus, StatusLine, StatusScan, SubAgent,
};
pub use theme::{kind_for_background, parse_osc11, ThemeKind, OSC11_QUERY};
pub use title::{age_label, clip, locate, AgentTitle, Format, Source, TitleScan};
pub use tree::{neighbour, Nav, Node, SplitDir, MAX_LEAVES};
pub use usage::{Cost, Part, Reading, SessionMeta, Tone, UsageBar};
pub use view::{
    header_button, layout, sidebar_button, slot_control, Confirm, Control, Hit, Layout,
    LayoutPrefs, MouseOut, PaneInfo, PaneRect, Rename, RenameEdit, ScrollState, SessionRow,
    SidebarEdit, SidebarItem, SidebarLayout, SidebarLine, TabItem, TabPrefs, ViewState,
};
