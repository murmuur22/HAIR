//! Session-only schematic exploration. No network, filesystem or source-page retention.
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomId {
    pub page: [u8; 32],
    pub window: usize,
}
impl RoomId {
    pub fn new(literal_address: &str, window: usize) -> Self {
        Self {
            page: Sha256::digest(literal_address.as_bytes()).into(),
            window,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapMode {
    #[default]
    Compact,
    Expanded,
    Hidden,
}
impl MapMode {
    pub fn cycle(&mut self) {
        *self = match self {
            Self::Compact => Self::Expanded,
            Self::Expanded => Self::Hidden,
            Self::Hidden => Self::Compact,
        };
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    Portal,
    Passage,
}
#[derive(Clone, Debug)]
pub struct Room {
    pub id: RoomId,
    pub title: String,
    pub cell: [i32; 2],
    pub exits: Vec<RoomId>,
    pub stale: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub kind: Connection,
}
#[derive(Clone, Debug, Default)]
pub struct Map {
    pub rooms: Vec<Room>,
    pub edges: Vec<Edge>,
    pub current: Option<usize>,
    pub recent: Vec<usize>,
    pub limited: bool,
}
impl Map {
    pub fn arrive(
        &mut self,
        id: RoomId,
        title: &str,
        exits: impl IntoIterator<Item = RoomId>,
        kind: Connection,
        refresh: bool,
    ) {
        let previous = if refresh {
            self.recent.clear();
            None
        } else {
            self.current
        };
        if refresh {
            for room in &mut self.rooms {
                if room.id.page == id.page && room.id != id {
                    room.stale = true;
                }
            }
        }
        let known = self.rooms.iter().position(|r| r.id == id);
        if known.is_none() && self.rooms.len() >= 128 {
            self.limited = true;
            self.current = None;
            self.recent.clear();
            return;
        }
        let index = if let Some(i) = known {
            i
        } else {
            let previous_cell = previous.map_or([0, 0], |i| self.rooms[i].cell);
            // Fill a stable, origin-centred square spiral. Prefer a cell near
            // the departure within the next shell, without rearranging visits.
            // 128 retained rooms always fit inside these 625 candidates.
            let mut cell = None;
            for radius in 0_i32..=12 {
                cell = (-radius..=radius)
                    .flat_map(|x| (-radius..=radius).map(move |y| [x, y]))
                    .filter(|c| c[0].abs().max(c[1].abs()) == radius)
                    .filter(|c| !self.rooms.iter().any(|r| r.cell == *c))
                    .min_by_key(|c| {
                        (c[0] - previous_cell[0]).abs() + (c[1] - previous_cell[1]).abs()
                    });
                if cell.is_some() {
                    break;
                }
            }
            let Some(cell) = cell else {
                self.limited = true;
                self.current = None;
                self.recent.clear();
                return;
            };
            self.rooms.push(Room {
                id,
                title: String::new(),
                cell,
                exits: Vec::new(),
                stale: false,
            });
            self.rooms.len() - 1
        };
        let mut chars = title.chars().map(|c| if c.is_control() { ' ' } else { c });
        let mut label: String = chars.by_ref().take(96).collect();
        if chars.next().is_some() {
            label.pop();
            label.push('~');
        }
        self.rooms[index].title = label;
        self.rooms[index].stale = false;
        let mut observed = Vec::new();
        // Inspect at most the producer's bounded semantic link allowance.
        for exit in exits.into_iter().take(8193) {
            if observed.contains(&exit) {
                continue;
            }
            if observed.len() == 64 {
                self.limited = true;
                break;
            }
            observed.push(exit);
        }
        self.rooms[index].exits = observed;
        if let Some(from) = previous.filter(|from| *from != index) {
            let edge = Edge {
                from,
                to: index,
                kind,
            };
            if !self.edges.contains(&edge) {
                if self.edges.len() < 512 {
                    self.edges.push(edge);
                } else {
                    self.limited = true;
                }
            }
        }
        if self.recent.last() != Some(&index) {
            self.recent.push(index);
        }
        if self.recent.len() > 32 {
            self.recent.remove(0);
        }
        self.current = Some(index);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn follows_and_loops_preserve_positions_without_duplicate_rooms() {
        let mut m = Map::default();
        let a = RoomId::new("https://site.test/a", 0);
        let b = RoomId::new("https://site.test/b", 0);
        m.arrive(a, "Home", [b], Connection::Portal, false);
        m.arrive(b, "Guide", [a], Connection::Portal, false);
        let cells: Vec<_> = m.rooms.iter().map(|r| r.cell).collect();
        assert_ne!(
            cells[0], cells[1],
            "visited rooms need distinct stable cells"
        );
        m.arrive(a, "Home", [b], Connection::Portal, false);
        assert_eq!(m.rooms.len(), 2);
        assert_eq!(m.current, Some(0));
        assert_eq!(m.rooms.iter().map(|r| r.cell).collect::<Vec<_>>(), cells);
        assert_eq!(
            m.edges,
            vec![
                Edge {
                    from: 0,
                    to: 1,
                    kind: Connection::Portal
                },
                Edge {
                    from: 1,
                    to: 0,
                    kind: Connection::Portal
                }
            ]
        );
        assert_eq!(m.recent, vec![0, 1, 0]);
    }
    #[test]
    fn refresh_invalidates_unobserved_same_page_windows_without_false_traversal() {
        let mut m = Map::default();
        let a = RoomId::new("https://site.test/a", 0);
        let next = RoomId::new("https://site.test/a", 1);
        m.arrive(a, "Start", [next], Connection::Portal, false);
        m.arrive(next, "Later", [a], Connection::Passage, false);
        m.arrive(a, "Fresh start", [], Connection::Portal, true);
        assert!(
            m.rooms[1].stale,
            "old ordinal belongs to an earlier response until revisited"
        );
        assert_eq!(m.edges.len(), 1, "refresh is not traversing a portal back");
        assert_eq!(m.rooms[0].title, "Fresh start");
        assert!(m.rooms[0].exits.is_empty());
        assert_eq!(m.recent, vec![0]);
        m.arrive(next, "New later", [], Connection::Passage, false);
        assert!(!m.rooms[1].stale);
        assert_eq!(m.rooms[1].title, "New later");
    }
    #[test]
    fn recording_caps_never_move_old_cells_or_claim_unrecorded_current() {
        let mut m = Map::default();
        let huge = "é\n".repeat(1000);
        for i in 0..140 {
            m.arrive(
                RoomId::new(&format!("https://site.test/{i}"), 0),
                &huge,
                (0..80).map(|j| RoomId::new(&format!("https://other.test/{j}"), 0)),
                Connection::Portal,
                false,
            );
        }
        assert_eq!(m.rooms.len(), 128);
        assert!(m.limited);
        assert!(m.current.is_none());
        assert!(m.rooms.iter().all(|r| r.title.chars().count() <= 96
            && !r.title.chars().any(char::is_control)
            && r.exits.len() <= 64));
        assert!(m.recent.len() <= 32 && m.edges.len() <= 512);
        let cell = m.rooms[0].cell;
        m.arrive(
            RoomId::new("https://site.test/0", 0),
            "Home",
            [],
            Connection::Portal,
            false,
        );
        assert_eq!(m.current, Some(0));
        assert_eq!(m.rooms[0].cell, cell);
    }
    #[test]
    fn long_walk_is_spread_over_a_readable_stable_grid() {
        let mut m = Map::default();
        for i in 0..128 {
            m.arrive(
                RoomId::new(&format!("https://site.test/{i}"), 0),
                "Room",
                [],
                Connection::Portal,
                false,
            );
        }
        for axis in 0..2 {
            let lo = m.rooms.iter().map(|r| r.cell[axis]).min().unwrap();
            let hi = m.rooms.iter().map(|r| r.cell[axis]).max().unwrap();
            assert!(
                hi - lo < 24,
                "schematic trail must not become a 128-cell ribbon"
            );
        }
        assert_eq!(
            m.rooms
                .iter()
                .map(|r| r.cell)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            128
        );
    }
    #[test]
    fn first_arrival_records_only_visited_room() {
        let mut m = Map::default();
        let a = RoomId::new("https://site.test/a", 0);
        let b = RoomId::new("https://site.test/b", 0);
        m.arrive(a, "Home", [b], Connection::Portal, false);
        assert_eq!(m.rooms.len(), 1, "unvisited targets must not become rooms");
        assert_eq!(m.current, Some(0));
        assert_eq!(m.rooms[0].exits, vec![b]);
        assert_eq!(m.rooms[0].cell, [0, 0]);
    }
}
