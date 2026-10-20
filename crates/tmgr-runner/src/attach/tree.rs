use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;

pub const MAX_LEAVES: usize = 16;
pub const RESIZE_STEP: u16 = 50;
const MIN_SIDE: u16 = 3;
const NARROW_ROW: u16 = 50;
const RATIO_MIN: u16 = 100;
const RATIO_MAX: u16 = 900;
const RATIO_HALF: u16 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDir {
    Right,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Left,
    Right,
    Up,
    Down,
}

impl Nav {
    fn axis(self) -> SplitDir {
        match self {
            Nav::Left | Nav::Right => SplitDir::Right,
            Nav::Up | Nav::Down => SplitDir::Down,
        }
    }

    fn forward(self) -> bool {
        matches!(self, Nav::Right | Nav::Down)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Node {
    Leaf(String),
    Split {
        dir: SplitDir,
        ratio: u16,
        a: Box<Node>,
        b: Box<Node>,
    },
}

enum Resized {
    Missing,
    NoAxis,
    Done(bool),
}

impl Node {
    pub fn leaf(id: &str) -> Node {
        Node::Leaf(id.to_string())
    }

    pub fn leaves(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Node::Leaf(id) => out.push(id),
            Node::Split { a, b, .. } => {
                a.collect(out);
                b.collect(out);
            }
        }
    }

    pub fn contains(&self, id: &str) -> bool {
        match self {
            Node::Leaf(x) => x == id,
            Node::Split { a, b, .. } => a.contains(id) || b.contains(id),
        }
    }

    pub fn is_valid(&self) -> bool {
        let leaves = self.leaves();
        leaves.len() <= MAX_LEAVES
            && leaves
                .iter()
                .enumerate()
                .all(|(i, id)| !leaves[..i].contains(id))
    }

    pub fn clamp_ratios(&mut self) {
        if let Node::Split { ratio, a, b, .. } = self {
            *ratio = (*ratio).clamp(RATIO_MIN, RATIO_MAX);
            a.clamp_ratios();
            b.clamp_ratios();
        }
    }

    pub fn split_leaf(&mut self, id: &str, dir: SplitDir, new_id: &str) -> bool {
        if self.contains(new_id) {
            return false;
        }
        self.split_at(id, dir, new_id)
    }

    fn split_at(&mut self, id: &str, dir: SplitDir, new_id: &str) -> bool {
        if matches!(self, Node::Leaf(x) if x == id) {
            let old = std::mem::replace(self, Node::Leaf(String::new()));
            *self = Node::Split {
                dir,
                ratio: RATIO_HALF,
                a: Box::new(old),
                b: Box::new(Node::leaf(new_id)),
            };
            return true;
        }
        match self {
            Node::Leaf(_) => false,
            Node::Split { a, b, .. } => a.split_at(id, dir, new_id) || b.split_at(id, dir, new_id),
        }
    }

    pub fn remove(tree: &mut Option<Node>, id: &str) -> bool {
        let Some(node) = tree.take() else {
            return false;
        };
        let (rest, removed) = node.without(id);
        *tree = rest;
        removed
    }

    fn without(self, id: &str) -> (Option<Node>, bool) {
        match self {
            Node::Leaf(x) if x == id => (None, true),
            leaf @ Node::Leaf(_) => (Some(leaf), false),
            Node::Split { dir, ratio, a, b } => {
                let (a, ra) = (*a).without(id);
                let (b, rb) = (*b).without(id);
                let node = match (a, b) {
                    (Some(a), Some(b)) => Some(Node::Split {
                        dir,
                        ratio,
                        a: Box::new(a),
                        b: Box::new(b),
                    }),
                    (one, other) => one.or(other),
                };
                (node, ra || rb)
            }
        }
    }

    pub fn sibling_leaf(&self, id: &str) -> Option<String> {
        let Node::Split { a, b, .. } = self else {
            return None;
        };
        if matches!(&**a, Node::Leaf(x) if x == id) {
            return b.leaves().first().map(|s| s.to_string());
        }
        if matches!(&**b, Node::Leaf(x) if x == id) {
            return a.leaves().last().map(|s| s.to_string());
        }
        a.sibling_leaf(id).or_else(|| b.sibling_leaf(id))
    }

    pub fn rename_leaf(&mut self, old: &str, new: &str) -> bool {
        if old == new {
            return self.contains(old);
        }
        if self.contains(new) {
            return false;
        }
        self.rename_at(old, new)
    }

    fn rename_at(&mut self, old: &str, new: &str) -> bool {
        match self {
            Node::Leaf(x) if x == old => {
                *x = new.to_string();
                true
            }
            Node::Leaf(_) => false,
            Node::Split { a, b, .. } => a.rename_at(old, new) || b.rename_at(old, new),
        }
    }

    pub fn resize(&mut self, rect: Rect, id: &str, nav: Nav, step_permille: u16) -> bool {
        matches!(
            self.resize_in(rect, id, nav, step_permille),
            Resized::Done(true)
        )
    }

    fn resize_in(&mut self, rect: Rect, id: &str, nav: Nav, step: u16) -> Resized {
        let Node::Split { dir, ratio, a, b } = self else {
            return if self.contains(id) {
                Resized::NoAxis
            } else {
                Resized::Missing
            };
        };
        let dir = effective_dir(*dir, rect);
        let in_a = a.contains(id);
        if !in_a && !b.contains(id) {
            return Resized::Missing;
        }
        let Some((ra, rb)) = split_rects(dir, *ratio, a, b, rect) else {
            return Resized::Done(false);
        };
        let inner = if in_a {
            a.resize_in(ra, id, nav, step)
        } else {
            b.resize_in(rb, id, nav, step)
        };
        match inner {
            Resized::NoAxis if dir == nav.axis() => {
                let next = if nav.forward() {
                    ratio.saturating_add(step)
                } else {
                    ratio.saturating_sub(step)
                }
                .clamp(RATIO_MIN, RATIO_MAX);
                let changed = next != *ratio;
                *ratio = next;
                Resized::Done(changed)
            }
            other => other,
        }
    }

    fn min_size(&self) -> (u16, u16) {
        match self {
            Node::Leaf(_) => (MIN_SIDE, MIN_SIDE),
            Node::Split { dir, a, b, .. } => {
                let (aw, ah) = a.min_size();
                let (bw, bh) = b.min_size();
                match dir {
                    SplitDir::Right => (aw.saturating_add(bw), ah.max(bh)),
                    SplitDir::Down => (aw.max(bw), ah.saturating_add(bh)),
                }
            }
        }
    }

    pub fn layout(&self, rect: Rect) -> Option<Vec<(String, Rect)>> {
        let mut out = Vec::new();
        self.place(rect, &mut out).then_some(out)
    }

    fn place(&self, r: Rect, out: &mut Vec<(String, Rect)>) -> bool {
        let Node::Split { dir, ratio, a, b } = self else {
            if let Node::Leaf(id) = self {
                out.push((id.clone(), r));
            }
            return true;
        };
        let dir = effective_dir(*dir, r);
        let Some((ra, rb)) = split_rects(dir, *ratio, a, b, r) else {
            return false;
        };
        a.place(ra, out) && b.place(rb, out)
    }
}

fn effective_dir(dir: SplitDir, r: Rect) -> SplitDir {
    if dir == SplitDir::Right && r.width < NARROW_ROW {
        SplitDir::Down
    } else {
        dir
    }
}

fn split_rects(dir: SplitDir, ratio: u16, a: &Node, b: &Node, r: Rect) -> Option<(Rect, Rect)> {
    let (amin, bmin) = (a.min_size(), b.min_size());
    Some(match dir {
        SplitDir::Right => {
            let aw = divide(r.width, ratio, amin.0, bmin.0)?;
            (
                Rect::new(r.x, r.y, aw, r.height),
                Rect::new(r.x + aw, r.y, r.width - aw, r.height),
            )
        }
        SplitDir::Down => {
            let ah = divide(r.height, ratio, amin.1, bmin.1)?;
            (
                Rect::new(r.x, r.y, r.width, ah),
                Rect::new(r.x, r.y + ah, r.width, r.height - ah),
            )
        }
    })
}

fn divide(total: u16, ratio: u16, amin: u16, bmin: u16) -> Option<u16> {
    let room = total.checked_sub(bmin).filter(|room| *room >= amin)?;
    let first = (u32::from(total) * u32::from(ratio) / 1000) as u16;
    Some(first.clamp(amin, room))
}

fn overlap(a0: u16, a1: u16, b0: u16, b1: u16) -> u16 {
    a1.min(b1).saturating_sub(a0.max(b0))
}

pub fn neighbour(rects: &[(String, Rect)], focused: &str, nav: Nav) -> Option<String> {
    let f = rects.iter().find(|(id, _)| id == focused)?.1;
    rects
        .iter()
        .enumerate()
        .filter(|(_, (id, _))| id != focused)
        .filter_map(|(i, (id, c))| {
            let (gap, span) = match nav {
                Nav::Right => (
                    c.x.checked_sub(f.right())?,
                    overlap(f.y, f.bottom(), c.y, c.bottom()),
                ),
                Nav::Left => (
                    f.x.checked_sub(c.right())?,
                    overlap(f.y, f.bottom(), c.y, c.bottom()),
                ),
                Nav::Down => (
                    c.y.checked_sub(f.bottom())?,
                    overlap(f.x, f.right(), c.x, c.right()),
                ),
                Nav::Up => (
                    f.y.checked_sub(c.bottom())?,
                    overlap(f.x, f.right(), c.x, c.right()),
                ),
            };
            (span > 0).then_some(((gap, Reverse(span), i), id))
        })
        .min_by_key(|(key, _)| *key)
        .map(|(_, id)| id.clone())
}
