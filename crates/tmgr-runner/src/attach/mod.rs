mod actions;
mod attention;
mod broadcast;
mod chip;
mod export;
mod filter;
mod find;
mod git;
mod keys;
mod layouts;
mod modes;
mod mouse;
mod newsession;
mod notes;
mod palette;
mod place;
mod prompts;
mod push;
mod qmenu;
mod render;
mod signal;
mod status;
mod summary;
mod templates;
mod theme;
pub(crate) mod title;
mod tree;
mod usage;
mod view;
mod watch;

#[cfg(test)]
mod tests;

pub use actions::{ActionDef, ACTIONS};
pub use attention::AttentionTracker;
pub use broadcast::{peer_bytes, Broadcast};
pub use chip::{chip_text, open_urls, TaskChip};
pub use export::{
    escape_markdown, fmt_size, markdown_lines, scan as scan_export,
    transcript as export_transcript, Decision, ExportAction, ExportState, Finding,
};
pub use filter::{Filter, FilterEdit};
pub use find::{CopyState, ScrollText, SearchEdit, SearchState};
pub use git::{
    parse_numstat, parse_pr, parse_status, valid_pr_url, GitAction, GitInfo, Pr, PrLookup,
    StatusInfo,
};
pub use keys::{Action, Cmd, InputSplitter, ScrollKey};
pub use layouts::{
    clean_name as clean_layout_name, parse as parse_layouts, render as render_layouts,
    without_ids as prefs_without_ids, LayoutOffer, LayoutRestore, Member, SavedLayout, Shape,
    MAX_LAYOUTS,
};
pub use modes::{enter_mouse, focus_on, mode_changes, reset_modes};
pub use mouse::{encode, wants, Kind, MouseEvent};
pub use newsession::{
    KindOpt, NewEdit, NewField, NewRequest, NewSession, Place, SpawnKind, TaskOpt,
};
pub use notes::{NotesAction, NotesState, NotesTask};
pub use palette::{Palette, PaletteEdit, PaletteEntry, PaletteMode, RowView};
pub use place::{fit, probe, tail2, Loc};
pub use prompts::{defaults as default_prompts, paste_bytes, Prompt, PromptForm};
pub use prompts::{parse as parse_prompts, render as render_prompts};
pub use push::{PushEvent, PushKind, PushSettings, PushTracker};
pub use qmenu::{find_question, goto_target, GotoMark, QButton, QEdit, QMenu};
pub use render::{cursor, render, ScreenSource};
pub use signal::{deliver, detect, Need, NeedKind, Notifier, SignalSettings, SignalTracker};
pub use status::{
    activity_label, mask_secrets, peek_lines, Peek, PeekEdit, SessionStatus, StatusLine,
    StatusScan, SubAgent,
};
pub use summary::{
    clamp as clamp_summary, clean_reply, compose, digest, plain_summary, prompt_input, scrub,
    Digest, PostTarget, SummaryAction, SummaryDraft, PROMPT as SUMMARY_PROMPT,
};
pub use templates::{
    defaults as default_templates, parse as parse_templates, render as render_templates,
    shell_quote, substitute as substitute_template, Field, Template, TemplateForm,
};
pub use theme::{kind_for_background, parse_osc11, ThemeKind, OSC11_QUERY};
pub use title::{age_label, clip, locate, AgentTitle, Format, Source, TitleScan};
pub use tree::{neighbour, Nav, Node, SplitDir, MAX_LEAVES};
pub use usage::{Cost, Part, Reading, SessionMeta, Tone, UsageBar};
pub use view::{
    header_button, layout, sidebar_button, slot_control, template_slot, Confirm, Control, Hit,
    Layout, LayoutPrefs, Machine, MachineState, MouseOut, PaneInfo, PaneRect, Rename, RenameEdit,
    ScrollState, SessionRow, SidebarEdit, SidebarItem, SidebarLayout, SidebarLine, TabItem,
    TabPrefs, ViewState, LOCAL_SECTION,
};
pub use watch::{duration_label, WatchState, WatchStatus};
