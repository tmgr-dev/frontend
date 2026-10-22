use super::tree::Node;
use super::view::{LayoutPrefs, TabPrefs, ViewState};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_LAYOUTS: usize = 50;
pub const MAX_LAYOUT_NAME: usize = 40;
const MAX_HOPS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub dir: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedLayout {
    pub name: String,
    pub prefs: LayoutPrefs,
    #[serde(default)]
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutOffer {
    pub name: String,
    pub resume: Vec<String>,
    pub start: Vec<Member>,
    pub gone: usize,
}

pub type Shape = (Option<Node>, Vec<TabPrefs>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutRestore {
    pub prefs: LayoutPrefs,
    pub waiting: HashSet<String>,
    pub generation: u64,
    pub applied: Option<Shape>,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    layout: Vec<serde_json::Value>,
}

pub fn clean_name(s: &str) -> Option<String> {
    let flat: String = s
        .chars()
        .filter(|c| !c.is_control() && !crate::sanitize::is_unsafe(*c))
        .collect();
    let name: String = flat.trim().chars().take(MAX_LAYOUT_NAME).collect();
    let name = name.trim().to_string();
    (!name.is_empty()).then_some(name)
}

pub fn parse(text: &str) -> Result<Vec<SavedLayout>, String> {
    let file: File = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut out: Vec<SavedLayout> = Vec::new();
    for v in file.layout {
        let Ok(mut l) = serde_json::from_value::<SavedLayout>(v) else {
            continue;
        };
        let Some(name) = clean_name(&l.name) else {
            continue;
        };
        if out.iter().any(|o| o.name.eq_ignore_ascii_case(&name)) || out.len() >= MAX_LAYOUTS {
            continue;
        }
        l.name = name;
        out.push(l);
    }
    Ok(out)
}

pub fn render(layouts: &[SavedLayout]) -> String {
    let file = serde_json::json!({ "layout": layouts });
    serde_json::to_string_pretty(&file).unwrap_or_default()
}

fn ids_of(prefs: &LayoutPrefs) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |id: &str| {
        if !out.iter().any(|o| o == id) {
            out.push(id.to_string());
        }
    };
    if let Some(t) = &prefs.tree {
        t.leaves().into_iter().for_each(&mut add);
    } else {
        prefs.panes.iter().for_each(|p| add(p));
    }
    for tab in &prefs.tabs {
        tab.tree.leaves().into_iter().for_each(&mut add);
    }
    out
}

impl ViewState {
    fn latest_id(&self, id: &str) -> String {
        let mut cur = id.to_string();
        for _ in 0..MAX_HOPS {
            match self
                .sessions
                .iter()
                .find(|r| r.resumed_from.as_deref() == Some(cur.as_str()))
            {
                Some(r) => cur = r.id.clone(),
                None => break,
            }
        }
        cur
    }

    fn rewrite_ids(&self, prefs: &mut LayoutPrefs, map: &[(String, String)]) {
        for (old, new) in map {
            if let Some(t) = prefs.tree.as_mut() {
                t.rename_leaf(old, new);
            }
            for p in prefs.panes.iter_mut().filter(|p| *p == old) {
                *p = new.clone();
            }
            for tab in prefs.tabs.iter_mut() {
                tab.tree.rename_leaf(old, new);
                if tab.focus == *old {
                    tab.focus = new.clone();
                }
            }
        }
    }

    pub fn save_layout(&mut self, name: &str) -> Result<String, String> {
        let name = clean_name(name).ok_or("type a name for the layout")?;
        let mut prefs = self.prefs();
        prefs.sidebar = None;
        prefs.sidebar_width = None;
        prefs.shells = Vec::new();
        prefs.new_kind = None;
        prefs.new_place = None;
        let ids = ids_of(&prefs);
        if prefs.tree.is_none() || ids.is_empty() {
            return Err("nothing to save, no panes are open".into());
        }
        let members = ids
            .iter()
            .filter_map(|id| {
                let row = self.sessions.iter().find(|r| r.id == *id)?;
                Some(Member {
                    id: id.clone(),
                    name: row.name.clone(),
                    kind: self.kinds.get(id).cloned(),
                    dir: self.locs.get(id).map(|l| l.dir.clone()).unwrap_or_default(),
                })
            })
            .collect();
        let saved = SavedLayout {
            name: name.clone(),
            prefs,
            members,
        };
        match self
            .layouts
            .iter()
            .position(|l| l.name.eq_ignore_ascii_case(&name))
        {
            Some(i) => self.layouts[i] = saved,
            None if self.layouts.len() >= MAX_LAYOUTS => {
                return Err(format!("at most {MAX_LAYOUTS} layouts, delete one first"));
            }
            None => self.layouts.push(saved),
        }
        Ok(name)
    }

    pub fn switch_layout(&mut self, idx: usize) -> bool {
        let Some(saved) = self.layouts.get(idx).cloned() else {
            return false;
        };
        let mut prefs = saved.prefs.clone();
        let ids = ids_of(&prefs);
        let map: Vec<(String, String)> = ids
            .iter()
            .map(|id| (id.clone(), self.latest_id(id)))
            .filter(|(a, b)| a != b)
            .collect();
        self.rewrite_ids(&mut prefs, &map);
        let mut offer = LayoutOffer {
            name: saved.name.clone(),
            ..LayoutOffer::default()
        };
        for id in ids_of(&prefs) {
            match self.sessions.iter().find(|r| r.id == id) {
                Some(r) if r.exited() => offer.resume.push(id),
                Some(_) => {}
                None => {
                    let member = saved.members.iter().find(|m| {
                        m.id == id || map.iter().any(|(old, new)| *new == id && m.id == *old)
                    });
                    match member.filter(|m| m.kind.is_some() && !m.dir.is_empty()) {
                        Some(m) => offer.start.push(Member {
                            id: id.clone(),
                            ..m.clone()
                        }),
                        None => offer.gone += 1,
                    }
                }
            }
        }
        self.layout_gen += 1;
        self.layout_restore = None;
        self.layout_offer = None;
        let known = ids_of(&prefs)
            .iter()
            .any(|id| self.sessions.iter().any(|r| r.id == *id));
        let applied = if known {
            self.apply_layout(&prefs);
            Some(self.shape())
        } else {
            None
        };
        if offer.resume.is_empty() && offer.start.is_empty() {
            self.notice = Some(if known && offer.gone == 0 {
                format!("layout {}", saved.name)
            } else if known {
                format!(
                    "layout {}: {} gone session{} left out",
                    saved.name,
                    offer.gone,
                    if offer.gone == 1 { "" } else { "s" }
                )
            } else {
                format!("layout {}: none of its sessions exist any more", saved.name)
            });
        } else {
            self.layout_restore = Some(LayoutRestore {
                prefs,
                waiting: offer.start.iter().map(|m| m.id.clone()).collect(),
                generation: self.layout_gen,
                applied,
            });
            self.layout_offer = Some(offer);
        }
        true
    }

    fn shape(&self) -> Shape {
        let p = self.prefs();
        (p.tree, p.tabs)
    }

    pub fn apply_layout(&mut self, prefs: &LayoutPrefs) {
        let cur = self.prefs();
        let mut p = prefs.clone();
        p.sidebar = cur.sidebar;
        p.sidebar_width = cur.sidebar_width;
        p.shells = cur.shells;
        p.new_kind = cur.new_kind;
        p.new_place = cur.new_place;
        self.apply_prefs(&p);
    }

    pub fn take_layout_offer(&mut self) -> Option<LayoutOffer> {
        self.layout_offer.take()
    }

    pub fn layout_started(&mut self, saved_id: &str, new_id: Option<&str>, generation: u64) {
        let Some(mut restore) = self
            .layout_restore
            .take()
            .filter(|r| r.generation == generation)
        else {
            return;
        };
        restore.waiting.remove(saved_id);
        if let Some(new) = new_id {
            let map = [(saved_id.to_string(), new.to_string())];
            self.rewrite_ids(&mut restore.prefs, &map);
            if restore.applied.as_ref().is_some_and(|a| *a != self.shape()) {
                return;
            }
            let focus = self.focused_session().map(str::to_string);
            let scroll = self.scroll;
            self.apply_layout(&restore.prefs);
            if let Some(i) = focus
                .as_ref()
                .and_then(|f| self.panes().iter().position(|p| p == f))
            {
                self.focus_pane(i);
                self.scroll = scroll;
            }
            restore.applied = Some(self.shape());
        }
        if !restore.waiting.is_empty() {
            self.layout_restore = Some(restore);
        }
    }

    pub(crate) fn restore_renamed(&mut self, old: &str, new: &str) {
        let Some(mut restore) = self.layout_restore.take() else {
            return;
        };
        let map = [(old.to_string(), new.to_string())];
        self.rewrite_ids(&mut restore.prefs, &map);
        if let Some((tree, tabs)) = restore.applied.as_mut() {
            if let Some(t) = tree.as_mut() {
                t.rename_leaf(old, new);
            }
            for tab in tabs.iter_mut() {
                tab.tree.rename_leaf(old, new);
                if tab.focus == old {
                    tab.focus = new.to_string();
                }
            }
        }
        self.layout_restore = Some(restore);
    }
}
