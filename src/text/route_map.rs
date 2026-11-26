//! Room coordinates stay stable; connectors bend through reserved gutters.
use super::*;
#[path = "route_paths.rs"]
mod paths;
use crate::exploration::{Room, RoomId};
const ARRIVAL: [f32; 3] = [0.94, 0.77, 0.47];
fn caption(title: &str) -> Vec<String> {
    let chars: Vec<_> = title
        .trim_start_matches(['#', ' '])
        .chars()
        .take(96)
        .collect();
    if chars.len() <= 9 {
        return vec![chars.into_iter().collect()];
    }
    vec![
        chars[..9].iter().collect(),
        bounded(chars[9..].iter().collect::<String>().trim(), 9),
    ]
}
#[cfg(test)]
#[test]
fn a_small_graph_uses_the_map_pane_instead_of_hugging_its_right_edge() {
    let mut map = Map::default();
    map.arrive(
        RoomId::new("https://example.org/home", 0),
        "Home",
        [],
        Connection::Portal,
        false,
    );
    let vertices = overlay(&map, MapMode::Expanded, 960, 640, [0, 0], None);
    let room: Vec<_> = vertices
        .iter()
        .filter(|v| v.color == CURRENT)
        .take(6)
        .collect();
    assert_eq!(room.len(), 6);
    let min = room
        .iter()
        .map(|v| v.position[0])
        .fold(f32::INFINITY, f32::min);
    let max = room
        .iter()
        .map(|v| v.position[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let x = (1.0 + (min + max) * 0.5) * 480.0;
    assert!((x - 380.0).abs() < 0.1, "small graph centre {x}");
}
#[cfg(test)]
#[test]
fn route_captions_clear_footer_at_minimum_size() {
    let mut map = Map::default();
    for i in 0..9 {
        map.arrive(
            RoomId::new(&format!("https://example.org/{i}"), 0),
            "Long room label",
            [],
            Connection::Portal,
            false,
        );
        map.rooms[i].cell = [i as i32 % 3, i as i32 / 3];
    }
    map.current = Some(4);
    map.recent.clear();
    let actual = overlay(&map, MapMode::Expanded, 640, 480, [0, 0], None);
    for r in &mut map.rooms {
        r.title.clear();
    }
    let control: HashSet<Vec<u8>> = overlay(&map, MapMode::Expanded, 640, 480, [0, 0], None)
        .iter()
        .map(|v| bytemuck::bytes_of(v).to_vec())
        .collect();
    let changed: Vec<_> = actual
        .iter()
        .filter(|v| !control.contains(bytemuck::bytes_of(*v)))
        .collect();
    assert!(!changed.is_empty());
    for v in changed {
        let y = (1.0 - v.position[1]) * 240.0;
        assert!(y < 416.0, "room caption reaches footer: {y}");
    }
}
#[cfg(test)]
#[test]
fn room_caption_keeps_distinguishing_path_suffix() {
    assert_eq!(caption("/branch-3 physical room")[0], "/branch-3");
}
#[cfg(test)]
#[test]
fn nonadjacent_visited_rooms_have_a_continuous_drawn_connection() {
    let mut map = Map::default();
    map.arrive(
        RoomId::new("https://example.org/a", 0),
        "Alpha",
        [],
        Connection::Portal,
        false,
    );
    map.arrive(
        RoomId::new("https://example.org/b", 0),
        "Beta",
        [],
        Connection::Portal,
        false,
    );
    map.rooms[0].cell = [0, 0];
    map.rooms[1].cell = [3, 2];
    map.recent.clear();
    assert!(!map.edges.is_empty());
    let vertices = overlay(&map, MapMode::Expanded, 1280, 960, [0, 0], None);
    let bounds = |quad: &[Vertex]| {
        let points: Vec<_> = quad
            .iter()
            .map(|v| [(1.0 + v.position[0]) * 640.0, (1.0 - v.position[1]) * 480.0])
            .collect();
        [
            points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min),
            points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min),
            points
                .iter()
                .map(|p| p[0])
                .fold(f32::NEG_INFINITY, f32::max),
            points
                .iter()
                .map(|p| p[1])
                .fold(f32::NEG_INFINITY, f32::max),
        ]
    };
    let a = bounds(
        vertices
            .as_chunks::<6>()
            .0
            .iter()
            .find(|q| q[0].color == VISITED)
            .unwrap(),
    );
    let b = bounds(
        vertices
            .as_chunks::<6>()
            .0
            .iter()
            .find(|q| q[0].color == CURRENT)
            .unwrap(),
    );
    let mut ink = HashSet::new();
    for q in vertices
        .as_chunks::<6>()
        .0
        .iter()
        .filter(|q| q[0].color == LINK)
    {
        let r = bounds(q);
        for y in r[1].round() as i32..r[3].round() as i32 {
            for x in r[0].round() as i32..r[2].round() as i32 {
                ink.insert([x, y]);
            }
        }
    }
    let touches = |p: [i32; 2], r: [f32; 4]| {
        p[0] as f32 >= r[0] - 3.0
            && p[0] as f32 <= r[2] + 3.0
            && p[1] as f32 >= r[1] - 3.0
            && p[1] as f32 <= r[3] + 3.0
    };
    let mut pending: std::collections::VecDeque<_> =
        ink.iter().copied().filter(|p| touches(*p, a)).collect();
    let mut seen = HashSet::new();
    let mut joined = false;
    while let Some(p) = pending.pop_front() {
        if !seen.insert(p) {
            continue;
        }
        if touches(p, b) {
            joined = true;
            break;
        }
        for d in [[-1, 0], [1, 0], [0, -1], [0, 1]] {
            let n = [p[0] + d[0], p[1] + d[1]];
            if ink.contains(&n) {
                pending.push_back(n);
            }
        }
    }
    assert!(
        joined,
        "nonadjacent visited rooms need a continuous routed line, not a +N badge"
    );
}

#[cfg(test)]
#[test]
fn off_view_connections_keep_a_drawn_boundary_stub() {
    let mut map = Map::default();
    map.arrive(
        RoomId::new("https://example.org/a", 0),
        "Alpha",
        [],
        Connection::Portal,
        false,
    );
    map.arrive(
        RoomId::new("https://example.org/b", 0),
        "Beta",
        [],
        Connection::Portal,
        false,
    );
    map.rooms[0].cell = [0, 0];
    map.rooms[1].cell = [8, 0];
    map.current = Some(0);
    map.recent.clear();
    let vertices = overlay(&map, MapMode::Expanded, 640, 480, [0, 0], None);
    assert!(
        vertices
            .as_chunks::<6>()
            .0
            .iter()
            .filter(|q| q[0].color == LINK)
            .any(|q| {
                let lo = q
                    .iter()
                    .map(|v| (1.0 + v.position[0]) * 320.0)
                    .fold(f32::INFINITY, f32::min);
                let hi = q
                    .iter()
                    .map(|v| (1.0 + v.position[0]) * 320.0)
                    .fold(f32::NEG_INFINITY, f32::max);
                hi - lo > 20.0 && hi < 424.0
            }),
        "an off-view connection needs a real line to the viewport boundary, not just a count"
    );
}

fn shape(room: &Room) -> [f32; 2] {
    let ratio = room.layout.as_ref().map_or(1.0, |l| {
        (l.bounds[2] - l.bounds[0]) / (l.bounds[3] - l.bounds[1])
    });
    if ratio > 2.5 {
        [100.0, 38.0]
    } else if ratio < 0.4 {
        [44.0, 70.0]
    } else {
        [84.0, 54.0]
    }
}
fn rule(p: &mut Painter, a: [f32; 2], b: [f32; 2], color: [f32; 3]) {
    if (a[0] - b[0]).abs() < 0.01 {
        p.rect(
            a[0] - 1.5,
            a[1].min(b[1]),
            3.0,
            (a[1] - b[1]).abs().max(3.0),
            color,
        );
    } else {
        p.rect(
            a[0].min(b[0]),
            a[1] - 1.5,
            (a[0] - b[0]).abs().max(3.0),
            3.0,
            color,
        );
    }
}
pub(super) fn overlay(
    map: &Map,
    mode: MapMode,
    width: u32,
    height: u32,
    pan: [i32; 2],
    back: Option<RoomId>,
) -> Vec<Vertex> {
    if mode == MapMode::Hidden || width < 640 || height < 480 {
        return vec![];
    }
    let full = mode == MapMode::Expanded;
    let mut p = Painter {
        vertices: vec![],
        width: width as f32,
        height: height as f32,
    };
    let panel = if full {
        [0.0, 0.0, width as f32, height as f32]
    } else {
        [width as f32 - 252.0, 76.0, 240.0, 270.0]
    };
    let [x, y, w, h] = panel;
    p.rect(x, y, w, h, PANEL);
    let Some(current) = map.current.filter(|i| *i < map.rooms.len()) else {
        p.text(
            x + 16.0,
            y + 16.0,
            if map.limited {
                "MAP LIMIT"
            } else {
                "NO ENTERED ROOMS"
            },
            2.0,
            LABEL,
        );
        return p.vertices;
    };
    let side = if full { 200.0 } else { 0.0 };
    let map_width = w - side - 32.0;
    let map_height = h - if full { 170.0 } else { 82.0 };
    let cell = if full { [152.0, 148.0] } else { [66.0, 66.0] };
    let capacity = [
        (map_width / cell[0]).floor().clamp(1.0, 24.0) as i32,
        (map_height / cell[1]).floor().clamp(1.0, 24.0) as i32,
    ];
    let bounds = [0, 1].map(|axis| {
        let lo = map
            .rooms
            .iter()
            .take(128)
            .map(|r| r.cell[axis])
            .min()
            .unwrap();
        let hi = map
            .rooms
            .iter()
            .take(128)
            .map(|r| r.cell[axis])
            .max()
            .unwrap();
        (lo, (hi as i64 - lo as i64 + 1).min(i32::MAX as i64) as i32)
    });
    let [cols, rows] = [0, 1].map(|axis| capacity[axis].min(bounds[axis].1));
    let origin = [0, 1].map(|axis| {
        let start = if bounds[axis].1 <= capacity[axis] {
            bounds[axis].0
        } else {
            map.rooms[current].cell[axis].saturating_sub(capacity[axis] / 2)
        };
        start.saturating_add(pan[axis])
    });
    let grid = [
        x + 16.0 + (map_width - cols as f32 * cell[0]) * 0.5,
        y + if full { 88.0 } else { 46.0 } + (map_height - rows as f32 * cell[1]) * 0.5,
    ];
    let centres: Vec<_> = map
        .rooms
        .iter()
        .enumerate()
        .take(128)
        .filter_map(|(i, r)| {
            let dx = r.cell[0] as i64 - origin[0] as i64;
            let dy = r.cell[1] as i64 - origin[1] as i64;
            (dx >= 0 && dx < (cols as i64) && dy >= 0 && dy < (rows as i64)).then(|| {
                (
                    i,
                    [
                        grid[0] + (dx as f32 + 0.5) * cell[0],
                        grid[1] + (dy as f32 + 0.5) * cell[1],
                    ],
                )
            })
        })
        .collect();
    let previous = map
        .recent
        .iter()
        .rev()
        .find(|i| **i != current && **i < map.rooms.len())
        .copied();
    let extent = [cols * 2, rows * 2];
    let lattice = |i: usize| {
        [0, 1].map(|axis| {
            ((map.rooms[i].cell[axis] as i64 - origin[axis] as i64) * 2 + 1)
                .clamp(-2, extent[axis] as i64 + 2) as i32
        })
    };
    let occupied: HashSet<_> = centres.iter().map(|(i, _)| lattice(*i)).collect();
    let mut used = std::collections::HashMap::new();
    let mut off_view = Vec::new();
    let mut seen = HashSet::new();
    for edge in map.edges.iter().take(512) {
        if edge.from >= map.rooms.len()
            || edge.to >= map.rooms.len()
            || edge.from == edge.to
            || !seen.insert((edge.from.min(edge.to), edge.from.max(edge.to)))
        {
            continue;
        }
        let a = centres.iter().find(|(i, _)| *i == edge.from);
        let b = centres.iter().find(|(i, _)| *i == edge.to);
        let (from, to, stub) = match (a, b) {
            (Some(_), Some(_)) => (edge.from, edge.to, false),
            (Some(_), None) => (edge.from, edge.to, true),
            (None, Some(_)) => (edge.to, edge.from, true),
            (None, None) => continue,
        };
        let start = lattice(from);
        let mut end = lattice(to);
        if stub {
            end = [0, 1].map(|axis| end[axis].clamp(0, extent[axis]) / 2 * 2);
        }
        let Some(path) = paths::route(start, end, extent, &occupied, &used, full) else {
            // Reserved even gutters keep every valid visible room reachable.
            debug_assert!(false, "schematic gutter routing failed");
            continue;
        };
        for pair in path.windows(2) {
            *used.entry(paths::segment(pair[0], pair[1])).or_insert(0u16) += 1;
        }
        let scale = if full { 1.0 } else { 0.48 };
        let mut points: Vec<_> = path
            .iter()
            .map(|p| {
                [
                    grid[0] + p[0] as f32 * cell[0] * 0.5,
                    grid[1] + p[1] as f32 * cell[1] * 0.5,
                ]
            })
            .collect();
        let sa = shape(&map.rooms[from]);
        let sb = shape(&map.rooms[to]);
        let last = points.len() - 1;
        for axis in 0..2 {
            points[0][axis] += (path[1][axis] - path[0][axis]) as f32 * sa[axis] * 0.5 * scale;
            if !stub {
                points[last][axis] +=
                    (path[last - 1][axis] - path[last][axis]) as f32 * sb[axis] * 0.5 * scale;
            }
        }
        if stub {
            off_view.push((points[last], to + 1));
        }
        let arrival = previous.is_some_and(|v| {
            (edge.from == current && edge.to == v) || (edge.to == current && edge.from == v)
        });
        for pair in points.windows(2) {
            rule(
                &mut p,
                pair[0],
                pair[1],
                if arrival { ARRIVAL } else { LINK },
            );
        }
    }
    for &(i, c) in &centres {
        let r = &map.rooms[i];
        let size = shape(r);
        let scale = if full { 1.0 } else { 0.48 };
        let size = [size[0] * scale, size[1] * scale];
        let color = if i == current {
            CURRENT
        } else if Some(i) == previous {
            ARRIVAL
        } else if r.stale {
            STALE
        } else {
            VISITED
        };
        p.rect(
            c[0] - size[0] * 0.5,
            c[1] - size[1] * 0.5,
            size[0],
            size[1],
            color,
        );
        if i != current {
            p.rect(
                c[0] - size[0] * 0.5 + 2.0,
                c[1] - size[1] * 0.5 + 2.0,
                size[0] - 4.0,
                size[1] - 4.0,
                PANEL,
            );
        }
        let label = format!("{}", i + 1);
        p.text(
            c[0] - label.len() as f32 * 3.0,
            c[1] - 4.0,
            &label,
            1.0,
            if i == current { PANEL } else { LABEL },
        );
        if full {
            for (row, label) in caption(&r.title).iter().enumerate() {
                p.literal_text(
                    c[0] - label.chars().count() as f32 * 6.0,
                    c[1] + 37.0 + row as f32 * 17.0,
                    label,
                    2.0,
                    LABEL,
                );
            }
        }
    }
    for (at, number) in off_view {
        p.rect(at[0] - 3.0, at[1] - 3.0, 6.0, 6.0, LINK);
        if full {
            let label = format!("#{number}");
            let tx = (at[0] + 5.0).clamp(
                grid[0] + 4.0,
                grid[0] + cols as f32 * cell[0] - label.len() as f32 * 6.0 - 4.0,
            );
            let ty = (at[1] - 12.0).clamp(grid[1] + 4.0, grid[1] + rows as f32 * cell[1] - 12.0);
            p.text(tx, ty, &label, 1.0, LABEL);
        }
    }
    let omitted = map.rooms.len().saturating_sub(centres.len());
    if full {
        p.text(
            20.0,
            20.0,
            if map.limited {
                "ROOM MAP | CAP"
            } else {
                "ROOM MAP"
            },
            3.0,
            CURRENT,
        );
        p.text(
            20.0,
            54.0,
            "SCHEMATIC LINKS | #N AT EDGE = OFF VIEW",
            1.0,
            LABEL,
        );
        let sx = w - side;
        rule(&mut p, [sx - 12.0, 88.0], [sx - 12.0, h - 72.0], LINK);
        p.text(sx, 90.0, "RECENT ROUTE", 2.0, ARRIVAL);
        let count = ((h - 285.0) / 42.0).floor().clamp(1.0, 10.0) as usize;
        let recent: Vec<_> = map
            .recent
            .iter()
            .rev()
            .filter(|i| **i < map.rooms.len())
            .take(count)
            .copied()
            .collect();
        for (row, i) in recent.iter().rev().enumerate() {
            let color = if *i == current { CURRENT } else { LABEL };
            let title = bounded(map.rooms[*i].title.trim_start_matches(['#', ' ']), 12);
            p.literal_text(
                sx,
                124.0 + row as f32 * 42.0,
                &format!("{} {}", i + 1, title),
                2.0,
                color,
            );
        }
        let visited = map.rooms[current]
            .exits
            .iter()
            .filter(|id| map.rooms.iter().any(|r| !r.stale && r.id == **id))
            .count();
        p.text(
            sx,
            h - 146.0,
            &format!("{visited}/{} VISITED", map.rooms[current].exits.len()),
            2.0,
            LABEL,
        );
        p.text(sx, h - 116.0, "BACK DESTINATION", 1.0, ARRIVAL);
        let name = back
            .map(|id| {
                map.rooms
                    .iter()
                    .find(|r| r.id == id)
                    .map_or("UNRECORDED", |r| r.title.as_str())
            })
            .unwrap_or("NO HELD HISTORY");
        p.literal_text(
            sx,
            h - 98.0,
            &bounded(name.trim_start_matches(['#', ' ']), 14),
            2.0,
            ARRIVAL,
        );
        p.text(20.0, h - 53.0, "M / ESC CLOSE | ARROWS PAN", 2.0, LABEL);
        p.text(
            20.0,
            h - 26.0,
            &format!("{omitted} OFF VIEW | GREEN HERE | AMBER ARRIVAL"),
            1.0,
            ARRIVAL,
        );
    } else {
        p.text(x + 10.0, y + 10.0, "MINIMAP", 2.0, CURRENT);
        p.text(
            x + 10.0,
            y + h - 30.0,
            &format!("#{}, {omitted} OFF VIEW", current + 1),
            1.0,
            LABEL,
        );
        p.text(x + 10.0, y + h - 16.0, "M MAP | SHIFT M HIDE", 1.0, LINK);
    }
    p.vertices
}
