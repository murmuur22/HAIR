//! Shared directory-map drawing and hit geometry. Coordinates are logical UI pixels.
use crate::directory_map::{Map, Target};
pub type Rect = [f32; 4];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Destination(Target),
    Older,
    Newer,
    Cancel,
    Travel,
}
pub fn action_at(
    view: &View,
    width: u32,
    height: u32,
    confirming: bool,
    point: [f32; 2],
) -> Option<Action> {
    if !point.iter().all(|v| v.is_finite()) {
        return None;
    }
    if confirming {
        return confirmation_rects(width, height)
            .1
            .into_iter()
            .enumerate()
            .find(|(_, r)| contains(*r, point))
            .map(|(i, _)| {
                if i == 0 {
                    Action::Cancel
                } else {
                    Action::Travel
                }
            });
    }
    if let Some((i, _)) = history_buttons(width, height)
        .into_iter()
        .enumerate()
        .find(|(_, r)| contains(*r, point))
    {
        return Some(if i == 0 { Action::Older } else { Action::Newer });
    }
    view.hit(point).map(Action::Destination)
}
pub fn contains(r: Rect, p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3]
}
pub struct View {
    pub area: Rect,
    pub scale: f32,
    pub side: f32,
    pub points: Vec<[f32; 2]>,
    pub cards: Vec<Rect>,
    pub edges: Vec<(usize, usize, Vec<[f32; 2]>)>,
    pub route_rows: Vec<(usize, Rect)>,
}
impl View {
    pub fn new(
        map: &Map,
        width: u32,
        height: u32,
        full: bool,
        pan: [i32; 2],
        history_offset: usize,
    ) -> Self {
        let w = width as f32;
        let h = height as f32;
        let side = if full && width >= 1100 && height >= 800 {
            300.0
        } else {
            0.0
        };
        let area = if full {
            [
                24.0,
                158.0,
                w - side - 24.0,
                h - if side > 0.0 { 90.0 } else { 164.0 },
            ]
        } else {
            [w - 244.0, 137.0, w - 28.0, 309.0]
        };
        let xy = map.layout();
        let maxx = xy.iter().map(|p| p[0]).fold(0.0, f32::max);
        let maxy = xy.iter().map(|p| p[1]).fold(0.0, f32::max);
        let fit = ((area[2] - area[0]) / (maxx * 256.0 + 244.0))
            .min((area[3] - area[1]) / (maxy * 256.0 + 168.0));
        let scale = if full {
            fit.clamp(0.85, 1.2)
        } else {
            fit.clamp(0.06, 0.55)
        };
        let center = if fit >= scale {
            [maxx * 0.5, maxy * 0.5]
        } else {
            xy.get(map.current.unwrap_or(0))
                .copied()
                .unwrap_or([0.0, 0.0])
        };
        let points: Vec<_> = xy
            .iter()
            .map(|p| {
                [
                    (area[0] + area[2]) * 0.5
                        + (p[0] - center[0] - if full { pan[0] as f32 } else { 0.0 })
                            * 256.0
                            * scale,
                    (area[1] + area[3]) * 0.5
                        + (p[1] - center[1] - if full { pan[1] as f32 } else { 0.0 })
                            * 256.0
                            * scale,
                ]
            })
            .collect();
        let cards: Vec<_> = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let size = if map.current == Some(i) {
                    [224.0, 148.0]
                } else {
                    [176.0, 104.0]
                };
                [
                    p[0] - size[0] * scale * 0.5,
                    p[1] - size[1] * scale * 0.5,
                    p[0] + size[0] * scale * 0.5,
                    p[1] + size[1] * scale * 0.5,
                ]
            })
            .collect();
        let mut edges = vec![];
        for (i, n) in map.nodes.iter().enumerate() {
            if let Some(parent) = n.parent {
                let x = points[parent][0];
                let collision = map
                    .nodes
                    .iter()
                    .enumerate()
                    .any(|(j, n)| n.parent == Some(parent) && (points[j][0] - x).abs() < 1.0);
                let siblings = map
                    .nodes
                    .iter()
                    .filter(|n| n.parent == Some(parent))
                    .count();
                let stem = x + if collision && siblings > 1 {
                    28.0 * scale
                } else {
                    0.0
                };
                let bus = (points[parent][1] + points[i][1]) * 0.5;
                edges.push((
                    parent,
                    i,
                    vec![
                        [stem, cards[parent][3]],
                        [stem, bus],
                        [points[i][0], bus],
                        [points[i][0], cards[i][1]],
                    ],
                ));
            }
        }
        let slots = if side > 0.0 {
            ((h - 304.0) / 44.0).max(1.0) as usize
        } else {
            3
        };
        let route_rows = map
            .history
            .iter()
            .enumerate()
            .rev()
            .skip(history_offset)
            .take(slots)
            .enumerate()
            .map(|(row, (index, _))| {
                let r = if side > 0.0 {
                    [
                        w - side + 12.0,
                        206.0 + row as f32 * 44.0,
                        w - 16.0,
                        244.0 + row as f32 * 44.0,
                    ]
                } else {
                    [
                        24.0 + row as f32 * (w - 48.0) / 3.0,
                        h - 124.0,
                        24.0 + (row + 1) as f32 * (w - 48.0) / 3.0 - 8.0,
                        h - 76.0,
                    ]
                };
                (index, r)
            })
            .collect();
        Self {
            area,
            scale,
            side,
            points,
            cards,
            edges,
            route_rows,
        }
    }
    pub fn visible(&self, i: usize) -> bool {
        let r = self.cards[i];
        r[0] - 2.0 >= self.area[0]
            && r[1] - 2.0 >= self.area[1]
            && r[2] + 2.0 <= self.area[2]
            && r[3] + 2.0 <= self.area[3]
    }
    pub fn hit(&self, p: [f32; 2]) -> Option<Target> {
        for &(i, r) in &self.route_rows {
            if contains(r, p) {
                return Some(Target::History(i));
            }
        }
        self.cards
            .iter()
            .enumerate()
            .find(|(i, r)| self.visible(*i) && contains(**r, p))
            .map(|(i, _)| Target::Node(i))
    }
}
pub fn history_buttons(w: u32, h: u32) -> [Rect; 2] {
    let (w, h) = (w as f32, h as f32);
    if w >= 1100.0 && h >= 800.0 {
        [
            [w - 288.0, h - 68.0, w - 160.0, h - 40.0],
            [w - 148.0, h - 68.0, w - 20.0, h - 40.0],
        ]
    } else {
        [
            [w - 196.0, h - 153.0, w - 112.0, h - 130.0],
            [w - 104.0, h - 153.0, w - 20.0, h - 130.0],
        ]
    }
}
pub fn confirmation_rects(w: u32, h: u32) -> (Rect, [Rect; 2]) {
    let (w, h) = (w as f32, h as f32);
    let width = (w - 48.0).min(760.0);
    let height = 352.0;
    let x = (w - width) * 0.5;
    let y = (h - height) * 0.5;
    let r = [x, y, x + width, y + height];
    (
        r,
        [
            [
                x + 24.0,
                y + height - 58.0,
                x + width * 0.46,
                y + height - 24.0,
            ],
            [
                x + width * 0.54,
                y + height - 58.0,
                x + width - 24.0,
                y + height - 24.0,
            ],
        ],
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn map_hover_actions_match_clickable_geometry_and_modal_ownership() {
        let mut map = Map::default();
        map.observe(
            &crate::local_explorer::Snapshot {
                folder: "/root".into(),
                entries: vec![],
            },
            [1, 1],
            None,
            0,
            [-10.5, -6.0, 10.5, 6.0],
            crate::directory_map::Relation::Selection,
        );
        let center = |r: Rect| [(r[0] + r[2]) * 0.5, (r[1] + r[3]) * 0.5];
        for [w, h] in [[640, 480], [960, 640], [1600, 1100]] {
            let view = View::new(&map, w, h, true, [0, 0], 0);
            assert_eq!(
                action_at(&view, w, h, false, center(view.cards[0])),
                Some(Action::Destination(Target::Node(0)))
            );
            assert_eq!(
                action_at(&view, w, h, true, center(view.cards[0])),
                None,
                "modal masks cards"
            );
            for (i, rect) in confirmation_rects(w, h).1.into_iter().enumerate() {
                assert_eq!(
                    action_at(&view, w, h, true, center(rect)),
                    Some(if i == 0 {
                        Action::Cancel
                    } else {
                        Action::Travel
                    })
                );
            }
            for (i, rect) in history_buttons(w, h).into_iter().enumerate() {
                assert_eq!(
                    action_at(&view, w, h, false, center(rect)),
                    Some(if i == 0 { Action::Older } else { Action::Newer })
                );
            }
            assert_eq!(action_at(&view, w, h, false, [-10.0, 20.0]), None);
        }
    }
    #[test]
    fn directory_map_orthogonal_bus_never_has_four_way_crossing() {
        let mut m = Map::default();
        for i in 0..10 {
            m.observe(
                &crate::local_explorer::Snapshot {
                    folder: format!("/n{i}").into(),
                    entries: vec![],
                },
                [1, i],
                None,
                0,
                [-10.5, -6.0, 10.5, 6.0],
                if i == 0 {
                    crate::directory_map::Relation::Selection
                } else if i < 4 {
                    crate::directory_map::Relation::ChildOf([1, 0])
                } else {
                    crate::directory_map::Relation::ChildOf([1, 1 + (i - 4) / 2])
                },
            );
        }
        for [w, h] in [[640, 480], [1600, 1100], [1920, 1280]] {
            let v = View::new(&m, w, h, true, [0, 0], 0);
            let segs: Vec<_> = v
                .edges
                .iter()
                .flat_map(|(_, _, p)| p.windows(2).map(|q| (q[0], q[1])))
                .collect();
            let mut probes = vec![];
            for &(a, b) in &segs {
                assert!(a[0] == b[0] || a[1] == b[1]);
                probes.extend([a, b]);
                for &(c, d) in &segs {
                    if a[0] == b[0]
                        && c[1] == d[1]
                        && a[0] >= c[0].min(d[0])
                        && a[0] <= c[0].max(d[0])
                        && c[1] >= a[1].min(b[1])
                        && c[1] <= a[1].max(b[1])
                    {
                        probes.push([a[0], c[1]]);
                    }
                }
            }
            for p in probes {
                let mut dirs = std::collections::BTreeSet::new();
                for &(a, b) in &segs {
                    if a[0] == b[0]
                        && (p[0] - a[0]).abs() < 0.01
                        && p[1] >= a[1].min(b[1])
                        && p[1] <= a[1].max(b[1])
                    {
                        if a[1].min(b[1]) < p[1] - 0.01 {
                            dirs.insert(0);
                        }
                        if a[1].max(b[1]) > p[1] + 0.01 {
                            dirs.insert(1);
                        }
                    }
                    if a[1] == b[1]
                        && (p[1] - a[1]).abs() < 0.01
                        && p[0] >= a[0].min(b[0])
                        && p[0] <= a[0].max(b[0])
                    {
                        if a[0].min(b[0]) < p[0] - 0.01 {
                            dirs.insert(2);
                        }
                        if a[0].max(b[0]) > p[0] + 0.01 {
                            dirs.insert(3);
                        }
                    }
                }
                assert!(dirs.len() < 4, "four-way crossing at {p:?}");
            }
        }
    }
    #[test]
    fn directory_map_cards_expand_only_current_and_keep_centers() {
        let mut m = Map::default();
        for i in 0..3 {
            m.observe(
                &crate::local_explorer::Snapshot {
                    folder: format!("/n{i}").into(),
                    entries: vec![],
                },
                [1, i],
                None,
                0,
                [-10.5, -6.0, 10.5, 6.0],
                if i == 0 {
                    crate::directory_map::Relation::Selection
                } else {
                    crate::directory_map::Relation::ChildOf([1, 0])
                },
            );
        }
        let a = View::new(&m, 1600, 1100, true, [0, 0], 0);
        let current = m.current.unwrap();
        assert!(
            a.cards[current][2] - a.cards[current][0] > a.cards[0][2] - a.cards[0][0],
            "current card must expand"
        );
        m.current = Some(0);
        let b = View::new(&m, 1600, 1100, true, [0, 0], 0);
        assert_eq!(a.points, b.points);
        for (i, r) in b.cards.iter().enumerate() {
            if b.visible(i) {
                assert_eq!(
                    b.hit([(r[0] + r[2]) * 0.5, (r[1] + r[3]) * 0.5]),
                    Some(Target::Node(i))
                );
            }
        }
        for (_, _, path) in b.edges {
            assert!(
                path.windows(2)
                    .all(|q| q[0][0] == q[1][0] || q[0][1] == q[1][1])
            );
        }
    }
}
