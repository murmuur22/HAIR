//! Bounded session-only directory hierarchy, separate from navigation history.
use crate::local_explorer::Snapshot;
pub const MAX_NODES: usize = 128;
pub type Id = [u64; 2];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Node(usize),
    History(usize),
}
#[derive(Clone, Debug, PartialEq)]
pub struct RouteEntry {
    pub id: Id,
    pub name: String,
    pub page: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Confirmation {
    pub target: Target,
    pub id: Id,
    pub name: String,
    pub page: usize,
    pub error: Option<String>,
}
#[derive(Clone, Copy, Debug)]
pub enum Relation {
    Selection,
    ChildOf(Id),
    ParentOf(Id),
    Visit,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: Id,
    pub name: String,
    pub parent: Option<usize>,
    pub folders: usize,
    pub files: usize,
    pub other: usize,
    pub page: usize,
    pub pages: usize,
    pub bounds: [f32; 4],
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map {
    pub nodes: Vec<Node>,
    pub current: Option<usize>,
    pub back: Option<usize>,
    pub has_back: bool,
    pub recent: Vec<usize>,
    pub history: Vec<RouteEntry>,
    pub restarted: bool,
}
impl Map {
    pub fn observe(
        &mut self,
        snapshot: &Snapshot,
        id: Id,
        back: Option<Id>,
        page: usize,
        bounds: [f32; 4],
        relation: Relation,
    ) {
        use crate::folder_scene::{PAGE_SIZE, display as escape_display};
        use crate::local_explorer::Kind;
        if matches!(relation, Relation::Selection) {
            *self = Self::default();
        }
        if self.nodes.len() == MAX_NODES && !self.nodes.iter().any(|n| n.id == id) {
            // Never silently omit the current folder or retain a dangling tree.
            *self = Self {
                restarted: true,
                ..Self::default()
            };
        }
        let folders = snapshot
            .entries
            .iter()
            .filter(|e| e.kind == Kind::Folder)
            .count();
        let files = snapshot
            .entries
            .iter()
            .filter(|e| e.kind == Kind::File)
            .count();
        let quoted = snapshot
            .folder
            .file_name()
            .map(escape_display)
            .unwrap_or_else(|| "\"/\"".into());
        let name = quoted
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(&quoted)
            .chars()
            .take(96)
            .collect();
        let parent = self
            .nodes
            .iter()
            .find(|n| n.id == id)
            .and_then(|n| n.parent);
        let node = Node {
            id,
            name,
            parent,
            folders,
            files,
            other: snapshot.entries.len() - folders - files,
            page,
            pages: snapshot.entries.len().div_ceil(PAGE_SIZE).max(1),
            bounds,
        };
        let current = if let Some(i) = self.nodes.iter().position(|n| n.id == id) {
            self.nodes[i] = node;
            i
        } else {
            self.nodes.push(node);
            self.nodes.len() - 1
        };
        let edge = match relation {
            Relation::ChildOf(parent) => self
                .nodes
                .iter()
                .position(|n| n.id == parent)
                .map(|p| (p, current)),
            Relation::ParentOf(child) => self
                .nodes
                .iter()
                .position(|n| n.id == child)
                .map(|c| (current, c)),
            _ => None,
        };
        if let Some((p, c)) = edge {
            let mut cursor = Some(p);
            let mut cycle = false;
            while let Some(i) = cursor {
                if i == c {
                    cycle = true;
                    break;
                }
                cursor = self.nodes[i].parent;
            }
            if !cycle {
                self.nodes[c].parent = Some(p);
            }
        }
        self.current = Some(current);
        self.has_back = back.is_some();
        self.back = back.and_then(|id| self.nodes.iter().position(|n| n.id == id));
        if self.recent.last() != Some(&current) {
            self.recent.push(current);
            if self.recent.len() > 32 {
                self.recent.remove(0);
            }
        }
    }
    /// Tidy hierarchy, not visit order: disjoint leaf intervals and one row per depth.
    /// New branches may grow the tree; unchanged hierarchy always yields identical cells.
    pub fn layout(&self) -> Vec<[f32; 2]> {
        fn place(map: &Map, i: usize, depth: f32, next: &mut f32, xy: &mut [[f32; 2]]) {
            let children: Vec<_> = map
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.parent == Some(i))
                .map(|(j, _)| j)
                .collect();
            let x = if children.is_empty() {
                let x = *next;
                *next += 1.0;
                x
            } else {
                for &j in &children {
                    place(map, j, depth + 1.0, next, xy);
                }
                (xy[children[0]][0] + xy[*children.last().unwrap()][0]) * 0.5
            };
            xy[i] = [x, depth];
        }
        let mut xy = vec![[0.0, 0.0]; self.nodes.len()];
        let mut next = 0.0;
        for (i, n) in self.nodes.iter().enumerate() {
            if n.parent.is_none() {
                place(self, i, 0.0, &mut next, &mut xy);
                next += 1.0;
            }
        }
        xy
    }
}
#[cfg(test)]
#[path = "directory_map_tests.rs"]
mod tests;
