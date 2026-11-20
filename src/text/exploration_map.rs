//! Bounded schematic exploration-map overlay. Cells are schematic, never physical walls.
use super::Painter;
use crate::assets::Vertex;
use crate::exploration::{Connection, Map, MapMode};
use std::collections::{HashMap, HashSet};

/// Top HUD band and bottom controls/aim/status band the map must never cover.
const TOP: f32 = 76.0;
const BOTTOM: f32 = 180.0;
const MARGIN: f32 = 12.0;
/// Compact view is a current-centred 5x5 neighbourhood.
const RADIUS: i64 = 2;
/// Renderer-side ceiling independent of model caps, keeping vertex count finite.
const MAX_DRAWN: usize = 512;
const PANEL: [f32; 3] = [0.015, 0.04, 0.025];
const CELL_BG: [f32; 3] = [0.03, 0.08, 0.05];
const LABEL: [f32; 3] = [0.65, 1.0, 0.75];
const CURRENT: [f32; 3] = [0.3, 1.0, 0.45];
const VISITED: [f32; 3] = [0.22, 0.5, 0.3];
const STALE: [f32; 3] = [0.1, 0.22, 0.13];
const TRAIL: [f32; 3] = CURRENT;
const LINK: [f32; 3] = [0.3, 0.42, 0.34];
const DOOR: [f32; 3] = LABEL;
const STUB: [f32; 3] = [0.45, 0.8, 0.55];
/// Gutter fraction each side of a room square; connections live in gutters.
const INSET: f32 = 0.2;
const MAX_EDGES: usize = 1024;
const MAX_TRAIL: usize = 32;
const MAX_EXITS: usize = 64;
/// Up, right, down, left in screen cells.
const DIRS: [[i64; 2]; 4] = [[0, -1], [1, 0], [0, 1], [-1, 0]];

#[path = "exploration_focus.rs"]
mod focus;

#[cfg(test)]
mod focus_contract {
    use super::*;
    use crate::exploration::{Port, RoomId, RoomLayout};
    #[test]
    fn compact_preserves_aim_and_cross_side_connectors_never_cross_focus_interior() {
        let a = RoomId::new("https://test/a", 0);
        let b = RoomId::new("https://test/b", 0);
        let mut m = Map::default();
        m.arrive(b, "B", [], Connection::Portal, false);
        m.arrive(a, "A", [b], Connection::Portal, false);
        m.rooms[1].layout = Some(RoomLayout {
            bounds: [-8.0, -32.0, 8.0, 32.0],
            ports: vec![Port {
                anchor: "side".into(),
                position: [8.5, 0.0],
                inward: [-1.0, 0.0],
                target: Some(b),
                kind: Connection::Portal,
            }],
        });
        for (w, h) in [(640, 480), (800, 533)] {
            let s = focus::scene(&m, MapMode::Compact, w, h).unwrap();
            let c = [w as f32 * 0.5, h as f32 * 0.5];
            assert!(
                s.panel[0] > c[0] + 24.0
                    || s.panel[2] < c[0] - 24.0
                    || s.panel[1] > c[1] + 24.0
                    || s.panel[3] < c[1] - 24.0,
                "compact covers aim"
            );
            let r = s.nodes[0].rect;
            for path in &s.paths {
                for pair in path.points.windows(2) {
                    let a = pair[0];
                    let b = pair[1];
                    let crosses = if (a[0] - b[0]).abs() < 0.01 {
                        a[0] > r[0] + 1.0
                            && a[0] < r[2] - 1.0
                            && a[1].min(b[1]) < r[3] - 1.0
                            && a[1].max(b[1]) > r[1] + 1.0
                    } else {
                        a[1] > r[1] + 1.0
                            && a[1] < r[3] - 1.0
                            && a[0].min(b[0]) < r[2] - 1.0
                            && a[0].max(b[0]) > r[0] + 1.0
                    };
                    assert!(!crosses, "connector hidden under current room: {pair:?}");
                }
            }
        }
    }
    #[test]
    fn physical_focus_is_bounded_at_every_supported_scale_and_records_omissions() {
        let mut m = Map::default();
        for i in 0..128 {
            m.arrive(
                RoomId::new(&format!("https://test/{i}"), 0),
                "Room",
                [],
                Connection::Portal,
                false,
            );
            let n = m.current.unwrap();
            m.rooms[n].layout = Some(RoomLayout {
                bounds: [-8.0, -36.6, 8.0, 36.6],
                ports: Vec::new(),
            });
        }
        for (w, h) in [
            (640, 480),
            (800, 600),
            (960, 640),
            (1280, 960),
            (1920, 1280),
        ] {
            for mode in [MapMode::Compact, MapMode::Expanded] {
                let s = focus::scene(&m, mode, w, h).unwrap();
                assert_eq!(s.nodes.len() + s.omitted, 128);
                assert!(s.nodes.len() <= 13);
                let v = overlay(&m, mode, 0.0, w, h);
                assert!(!v.is_empty() && v.len() < 100_000);
                assert!(
                    v.iter().all(|v| v
                        .position
                        .iter()
                        .all(|v| v.is_finite() && (-1.001..=1.001).contains(v))),
                    "bounded screen vertices {w}x{h}"
                );
            }
        }
        assert!(overlay(&m, MapMode::Hidden, 0.0, 960, 640).is_empty());
    }
    #[test]
    fn current_room_has_physical_proportions_ports_and_separate_progress() {
        let a = RoomId::new("https://test/a", 0);
        let b = RoomId::new("https://test/b", 0);
        let mut m = Map::default();
        m.arrive(b, "B", [], Connection::Portal, false);
        m.arrive(a, "Hall", [b], Connection::Portal, false);
        m.rooms[1].layout = Some(RoomLayout {
            bounds: [-8.0, -32.0, 8.0, 32.0],
            ports: vec![Port {
                anchor: "door".into(),
                position: [-8.5, 8.0],
                inward: [1.0, 0.0],
                target: Some(b),
                kind: Connection::Portal,
            }],
        });
        let before = format!("{m:?}");
        let scene = focus::scene(&m, MapMode::Expanded, 960, 640).expect("focus layout");
        let current = scene.nodes.iter().find(|n| n.index == 1).unwrap();
        let prior = scene.nodes.iter().find(|n| n.index == 0).unwrap();
        let r = current.rect;
        assert!(
            ((r[2] - r[0]) / (r[3] - r[1]) - 0.25).abs() < 0.001,
            "physical proportions, not square nodes"
        );
        assert!(
            (r[2] - r[0]) * (r[3] - r[1])
                > (prior.rect[2] - prior.rect[0]) * (prior.rect[3] - prior.rect[1])
        );
        assert_eq!(current.progress, [1, 1]);
        assert_eq!(
            scene.paths.len(),
            1,
            "a travelled pair must not be redrawn as an untravelled outbound link"
        );
        assert_eq!(scene.ports.len(), 1);
        assert_eq!(scene.ports[0].point[0], r[0]);
        assert!((scene.ports[0].point[1] - (r[1] + (r[3] - r[1]) * 0.625)).abs() < 0.01);
        assert!(
            scene
                .paths
                .iter()
                .any(|p| p.points.first() == Some(&scene.ports[0].point)
                    || p.points.last() == Some(&scene.ports[0].point)),
            "traceable connection starts at the real door"
        );
        assert_eq!(
            before,
            format!("{m:?}"),
            "focus never reshapes physical state"
        );
    }
}

struct Layout {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    glyph: f32,
    line: f32,
    pad: f32,
    columns: usize,
    grid: [f32; 2],
    cell: f32,
    /// Connection clip rectangle [x0, y0, x1, y1] inside the panel.
    clip: [f32; 4],
}

/// Map overlay in logical viewport pixels; scale stays the parent renderer's job.
/// Hidden, empty or unfittable views draw nothing rather than covering controls.
pub fn overlay_at(
    map: &Map,
    mode: MapMode,
    pose: &crate::Pose,
    width: u32,
    height: u32,
) -> Vec<Vertex> {
    if map
        .current
        .and_then(|i| map.rooms.get(i))
        .is_some_and(|r| r.layout.is_some())
    {
        focus::overlay(map, mode, Some(pose.position), pose.yaw, width, height)
    } else {
        overlay(map, mode, pose.yaw, width, height)
    }
}
pub fn overlay(map: &Map, mode: MapMode, yaw: f32, width: u32, height: u32) -> Vec<Vertex> {
    if map
        .current
        .and_then(|i| map.rooms.get(i))
        .is_some_and(|r| r.layout.is_some())
    {
        return focus::overlay(map, mode, None, yaw, width, height);
    }
    if mode == MapMode::Hidden {
        return Vec::new();
    }
    let Some(current) = map.current.filter(|&i| i < map.rooms.len()) else {
        if map.limited && width >= 640 && height >= 480 {
            let mut p = Painter {
                vertices: Vec::new(),
                width: width as f32,
                height: height as f32,
            };
            let x = width as f32 - 252.0;
            p.rect(x, TOP, 240.0, 52.0, PANEL);
            p.text(x + 6.0, TOP + 6.0, "MAP LIMIT", 2.0, LABEL);
            p.text(x + 6.0, TOP + 30.0, "ROOM NOT RECORDED", 2.0, LABEL);
            return p.vertices;
        }
        return Vec::new();
    };
    let here = map.rooms[current].cell;
    let drawn: Vec<usize> = match mode {
        MapMode::Hidden => return Vec::new(),
        MapMode::Compact => (0..map.rooms.len())
            .filter(|&i| {
                let c = map.rooms[i].cell;
                (c[0] as i64 - here[0] as i64).abs() <= RADIUS
                    && (c[1] as i64 - here[1] as i64).abs() <= RADIUS
            })
            .take(MAX_DRAWN)
            .collect(),
        MapMode::Expanded => (0..map.rooms.len()).take(MAX_DRAWN).collect(),
    };
    // Grid origin cell and span in cells.
    let (origin, span) = match mode {
        MapMode::Compact => ([here[0] as i64 - RADIUS, here[1] as i64 - RADIUS], [5, 5]),
        _ => {
            let cells = drawn.iter().chain([&current]).map(|&i| map.rooms[i].cell);
            let (mut lo, mut hi) = ([i64::MAX; 2], [i64::MIN; 2]);
            for [x, y] in cells.map(|c| [c[0] as i64, c[1] as i64]) {
                lo = [lo[0].min(x), lo[1].min(y)];
                hi = [hi[0].max(x), hi[1].max(y)];
            }
            (lo, [hi[0] - lo[0] + 1, hi[1] - lo[1] + 1])
        }
    };
    let Some(l) = layout(mode, span, width, height) else {
        return Vec::new();
    };
    let mut p = Painter {
        vertices: Vec::new(),
        width: width as f32,
        height: height as f32,
    };
    p.rect(l.x, l.y, l.w, l.h, PANEL);
    // Cap disclosure sits right after MAP so truncation cannot hide it.
    let cap = if map.limited { "CAP " } else { "" };
    let header = match mode {
        MapMode::Compact => format!("MAP {cap}NEAR"),
        _ => format!("MAP {cap}ALL {}", map.rooms.len()),
    };
    p.text(
        l.x + l.pad,
        l.y + l.pad,
        &bounded(&header, l.columns),
        l.glyph,
        LABEL,
    );
    if mode == MapMode::Compact {
        for row in 0..5 {
            for col in 0..5 {
                let (cx, cy) = (
                    l.grid[0] + col as f32 * l.cell,
                    l.grid[1] + row as f32 * l.cell,
                );
                p.rect(
                    cx + 1.0,
                    cy + 1.0,
                    (l.cell - 2.0).max(0.0),
                    (l.cell - 2.0).max(0.0),
                    CELL_BG,
                );
            }
        }
    }
    let s = l.cell;
    let half = s * (0.5 - INSET);
    // Line weights grow with cells but stay schematic at large expanded sizes.
    let (thin, thick) = ((s * 0.08).clamp(1.0, 4.0), (s * 0.16).clamp(1.0, 8.0));
    let centre = |cell: [i32; 2]| {
        [
            l.grid[0] + ((cell[0] as i64 - origin[0]) as f32 + 0.5) * s,
            l.grid[1] + ((cell[1] as i64 - origin[1]) as f32 + 0.5) * s,
        ]
    };
    let visible: HashSet<usize> = drawn.iter().copied().collect();
    let occupied: HashSet<[i32; 2]> = map.rooms.iter().map(|r| r.cell).collect();
    let visited: HashSet<([u8; 32], usize)> =
        map.rooms.iter().map(|r| (r.id.page, r.id.window)).collect();
    let start = map.recent.len().saturating_sub(MAX_TRAIL);
    let trail: HashSet<(usize, usize)> = map.recent[start..]
        .windows(2)
        .map(|w| (w[0].min(w[1]), w[0].max(w[1])))
        .collect();
    // Directions already used by connections, so unknown-exit stubs avoid them.
    let mut used: HashMap<usize, Vec<[i64; 2]>> = HashMap::new();
    let mut seen = HashSet::new();
    for e in map.edges.iter().take(MAX_EDGES) {
        let (a, b) = (e.from.min(e.to), e.from.max(e.to));
        let portal = e.kind == Connection::Portal;
        if a == b || b >= map.rooms.len() || !seen.insert((a, b, portal)) {
            continue;
        }
        if !visible.contains(&a) && !visible.contains(&b) {
            continue;
        }
        let color = if trail.contains(&(a, b)) { TRAIL } else { LINK };
        let width = if portal { thick } else { thin };
        let (ca, cb) = (map.rooms[a].cell, map.rooms[b].cell);
        let dir = toward(ca, cb);
        let adjacent =
            (cb[0] as i64 - ca[0] as i64).abs() + (cb[1] as i64 - ca[1] as i64).abs() == 1;
        if adjacent && visible.contains(&a) && visible.contains(&b) {
            let c = centre(ca);
            bar(&mut p, l.clip, c, dir, half, s - half, width, color);
            if portal {
                // Doorway frame across the middle of a page portal; passages stay bare.
                door(&mut p, l.clip, c, dir, s * 0.5, thin, thick * 2.4);
            }
            used.entry(a).or_default().push(dir);
            used.entry(b).or_default().push([-dir[0], -dir[1]]);
            continue;
        }
        // Non-adjacent: paired endpoint stubs, never a long line across unrelated rooms.
        for (end, other) in [(a, b), (b, a)] {
            if !visible.contains(&end) {
                continue;
            }
            let (from, to) = (map.rooms[end].cell, map.rooms[other].cell);
            let dir = toward(from, to);
            let c = centre(from);
            bar(&mut p, l.clip, c, dir, half, s * 0.5, width, color);
            if portal {
                door(
                    &mut p,
                    l.clip,
                    c,
                    dir,
                    s * 0.5 - thin * 0.5,
                    thin,
                    thick * 2.4,
                );
            }
            used.entry(end).or_default().push(dir);
            if mode == MapMode::Compact && !visible.contains(&other) {
                boundary(&mut p, &l, span, c, dir, thick, color);
            }
        }
    }
    let mut unseen_here = 0;
    for &i in &drawn {
        let room = &map.rooms[i];
        let mut unknown = HashSet::new();
        for exit in room.exits.iter().take(MAX_EXITS) {
            if !visited.contains(&(exit.page, exit.window)) {
                unknown.insert((exit.page, exit.window));
            }
        }
        if i == current {
            unseen_here = unknown.len();
        }
        if unknown.is_empty() {
            continue;
        }
        let taken = used.get(&i).map(Vec::as_slice).unwrap_or(&[]);
        let mut dirs: Vec<[i64; 2]> = DIRS
            .into_iter()
            .filter(|d| {
                let n = (
                    room.cell[0].checked_add(d[0] as i32),
                    room.cell[1].checked_add(d[1] as i32),
                );
                let free = match n {
                    (Some(x), Some(y)) => !occupied.contains(&[x, y]),
                    _ => true,
                };
                free && !taken.contains(d)
            })
            .collect();
        if dirs.is_empty() {
            dirs.push(DIRS[0]);
        }
        let color = if room.stale { STALE } else { STUB };
        let c = centre(room.cell);
        let length = s - 2.0 * half;
        for d in dirs.into_iter().take(unknown.len().min(4)) {
            // Dashed: two dashes with a gap, reaching toward the empty neighbour cell.
            bar(&mut p, l.clip, c, d, half, half + length / 3.0, thin, color);
            bar(
                &mut p,
                l.clip,
                c,
                d,
                half + 2.0 * length / 3.0,
                s - half,
                thin,
                color,
            );
        }
    }
    for &i in drawn.iter().filter(|&&i| i != current) {
        let room = &map.rooms[i];
        let [cx, cy] = centre(room.cell);
        if room.stale {
            // Hollow outline: an earlier response's observation, not current truth.
            let edge = (s * 0.1).clamp(1.0, 4.0);
            p.rect(cx - half, cy - half, half * 2.0, half * 2.0, STALE);
            let inner = (half - edge).max(0.0);
            p.rect(cx - inner, cy - inner, inner * 2.0, inner * 2.0, PANEL);
        } else {
            p.rect(cx - half, cy - half, half * 2.0, half * 2.0, VISITED);
        }
        if mode == MapMode::Expanded {
            // Ordinal = visit-order index, stable because rooms are only appended.
            let color = if room.stale { VISITED } else { PANEL };
            ordinal(&mut p, [cx, cy], half, i + 1, color);
        }
    }
    let [cx, cy] = centre(here);
    p.rect(cx - half, cy - half, half * 2.0, half * 2.0, CURRENT);
    facing(&mut p, [cx, cy], half * 0.85, yaw);
    let title_y = l.grid[1] + span[1] as f32 * s + l.pad;
    let below = match mode {
        MapMode::Compact => title_y,
        _ => l.y + l.h - l.pad - 3.0 * l.line,
    };
    let title = bounded(
        &format!("#{} {}", current + 1, map.rooms[current].title),
        l.columns,
    );
    p.text(l.x + l.pad, below, &title, l.glyph, CURRENT);
    let unseen = match unseen_here {
        0 => String::new(),
        n => format!(" {n} UNSEEN"),
    };
    let help = match mode {
        MapMode::Compact => fitting(&[format!("M MAP{unseen}")], l.columns),
        _ => fitting(
            &[
                format!("M NEAR/ALL/OFF  SHIFT+M LINKS{unseen}"),
                format!("M MAP{unseen}"),
            ],
            l.columns,
        ),
    };
    p.text(l.x + l.pad, below + l.line, &help, l.glyph, LABEL);
    if mode != MapMode::Compact {
        // Honest scope: session-only schematic of visited rooms, not site layout.
        let legend = fitting(
            &[
                "SCHEMATIC SESSION ONLY | DOOR=PORTAL THIN=PASSAGE - -=UNSEEN HOLLOW=STALE"
                    .to_string(),
                "SCHEMATIC SESSION ONLY | DOOR=PORTAL - -=UNSEEN".to_string(),
                "SCHEMATIC SESSION ONLY".to_string(),
            ],
            l.columns,
        );
        p.text(l.x + l.pad, below + 2.0 * l.line, &legend, l.glyph, LINK);
    }
    p.vertices
}

/// First candidate that fits whole; otherwise the last one honestly truncated.
fn fitting(candidates: &[String], columns: usize) -> String {
    candidates
        .iter()
        .find(|c| c.chars().count() <= columns)
        .cloned()
        .unwrap_or_else(|| bounded(candidates.last().map_or("", String::as_str), columns))
}

/// Stable visited ordinal centred inside a room square when it fits legibly.
fn ordinal(p: &mut Painter, c: [f32; 2], half: f32, n: usize, color: [f32; 3]) {
    let digits = n.to_string();
    let count = digits.len() as f32;
    let side = 2.0 * half;
    let Some(scale) = [2.0f32, 1.0]
        .into_iter()
        .find(|g| side >= count * 6.0 * g + 2.0 * g && side >= 9.0 * g)
    else {
        return;
    };
    let w = count * 6.0 * scale - scale;
    p.text(c[0] - w * 0.5, c[1] - 3.5 * scale, &digits, scale, color);
}

/// Axis direction from one cell toward another along the dominant axis.
fn toward(from: [i32; 2], to: [i32; 2]) -> [i64; 2] {
    let (dx, dy) = (to[0] as i64 - from[0] as i64, to[1] as i64 - from[1] as i64);
    if dx.abs() >= dy.abs() {
        [dx.signum(), 0]
    } else {
        [0, dy.signum()]
    }
}

/// Rectangle intersected with the map's clip area; empty results draw nothing.
fn clipped(p: &mut Painter, clip: [f32; 4], x: f32, y: f32, w: f32, h: f32, color: [f32; 3]) {
    let (x0, y0) = (x.max(clip[0]), y.max(clip[1]));
    let (x1, y1) = ((x + w).min(clip[2]), (y + h).min(clip[3]));
    if x1 > x0 && y1 > y0 {
        p.rect(x0, y0, x1 - x0, y1 - y0, color);
    }
}

/// Axis-aligned segment from `from` to `to` pixels out of `c` along `dir`.
#[allow(clippy::too_many_arguments)]
fn bar(
    p: &mut Painter,
    clip: [f32; 4],
    c: [f32; 2],
    dir: [i64; 2],
    from: f32,
    to: f32,
    width: f32,
    color: [f32; 3],
) {
    let (dx, dy) = (dir[0] as f32, dir[1] as f32);
    let (a, b) = (
        [c[0] + dx * from, c[1] + dy * from],
        [c[0] + dx * to, c[1] + dy * to],
    );
    if dir[0] != 0 {
        clipped(
            p,
            clip,
            a[0].min(b[0]),
            c[1] - width * 0.5,
            (b[0] - a[0]).abs(),
            width,
            color,
        );
    } else {
        clipped(
            p,
            clip,
            c[0] - width * 0.5,
            a[1].min(b[1]),
            width,
            (b[1] - a[1]).abs(),
            color,
        );
    }
}

/// Doorway frame crossing a connection `at` pixels out of `c`.
fn door(
    p: &mut Painter,
    clip: [f32; 4],
    c: [f32; 2],
    dir: [i64; 2],
    at: f32,
    width: f32,
    span: f32,
) {
    let m = [c[0] + dir[0] as f32 * at, c[1] + dir[1] as f32 * at];
    if dir[0] != 0 {
        clipped(
            p,
            clip,
            m[0] - width * 0.5,
            m[1] - span * 0.5,
            width,
            span,
            DOOR,
        );
    } else {
        clipped(
            p,
            clip,
            m[0] - span * 0.5,
            m[1] - width * 0.5,
            span,
            width,
            DOOR,
        );
    }
}

/// Compact-only marker on the grid border: a connection continues out of view.
fn boundary(
    p: &mut Painter,
    l: &Layout,
    span: [i64; 2],
    c: [f32; 2],
    dir: [i64; 2],
    m: f32,
    color: [f32; 3],
) {
    let s = l.cell;
    let (x0, y0) = (l.grid[0], l.grid[1]);
    let (x1, y1) = (x0 + span[0] as f32 * s, y0 + span[1] as f32 * s);
    match dir {
        [1, _] => clipped(p, l.clip, x1 - m, c[1] - s * 0.3, m, s * 0.6, color),
        [-1, _] => clipped(p, l.clip, x0, c[1] - s * 0.3, m, s * 0.6, color),
        [_, 1] => clipped(p, l.clip, c[0] - s * 0.3, y1 - m, s * 0.6, m, color),
        _ => clipped(p, l.clip, c[0] - s * 0.3, y0, s * 0.6, m, color),
    }
}

/// Panel rectangle inside the logical band between the top HUD and bottom controls.
/// Tries the regular glyph then a smaller one; `None` when nothing legible fits.
fn layout(mode: MapMode, span: [i64; 2], width: u32, height: u32) -> Option<Layout> {
    let (width, height) = (width as f32, height as f32);
    let avail_w = width - 2.0 * MARGIN;
    let avail_h = height - BOTTOM - TOP;
    for glyph in [2.0f32, 1.0] {
        let (line, pad) = (10.0 * glyph, 3.0 * glyph);
        // Header, title and help; expanded adds a disclosure/legend row.
        let text_h = match mode {
            MapMode::Compact => 3.0 * line,
            _ => 4.0 * line,
        };
        let w = match mode {
            MapMode::Compact => avail_w.min(120.0 * glyph),
            _ => avail_w,
        };
        let columns = ((w - 2.0 * pad) / (6.0 * glyph)).floor();
        let grid_w = w - 2.0 * pad;
        let grid_h = avail_h - 2.0 * pad - text_h;
        if columns < 4.0 || grid_w <= 0.0 || grid_h <= 0.0 {
            continue;
        }
        // Few-room maps get large readable cells; available space still wins.
        let max_cell = match mode {
            MapMode::Compact => 14.0 * glyph,
            _ => 36.0 * glyph,
        };
        let (sx, sy) = (span[0].max(1) as f32, span[1].max(1) as f32);
        let cell = (grid_w / sx).min(grid_h / sy).min(max_cell);
        let min_cell = match mode {
            MapMode::Compact => 3.0 * glyph,
            _ => 1.0,
        };
        if !cell.is_finite() || cell < min_cell {
            continue;
        }
        let (x, h) = match mode {
            MapMode::Compact => (width - MARGIN - w, 2.0 * pad + text_h + 5.0 * cell),
            _ => (MARGIN, avail_h),
        };
        let y = TOP;
        let grid_x = x + pad + (grid_w - sx * cell) * 0.5;
        // Compact panel shrinks to its grid; expanded centres within the whole band.
        let grid_y = match mode {
            MapMode::Compact => y + pad + line,
            _ => y + pad + line + (grid_h - sy * cell) * 0.5,
        };
        let clip = match mode {
            MapMode::Compact => [grid_x, grid_y, grid_x + 5.0 * cell, grid_y + 5.0 * cell],
            _ => [
                x + pad,
                y + pad + line,
                x + w - pad,
                y + pad + line + grid_h,
            ],
        };
        return Some(Layout {
            clip,
            x,
            y,
            w,
            h,
            glyph,
            line,
            pad,
            columns: columns as usize,
            grid: [grid_x, grid_y],
            cell,
        });
    }
    None
}

/// Honest truncation: printable characters only, '~' marks anything cut.
fn bounded(text: &str, columns: usize) -> String {
    let clean: Vec<char> = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if clean.len() <= columns {
        return clean.into_iter().collect();
    }
    let mut out: String = clean[..columns.saturating_sub(1)].iter().collect();
    out.push('~');
    out
}

/// Dark facing triangle inside the current green cell; yaw 0 points up the map.
fn facing(p: &mut Painter, centre: [f32; 2], radius: f32, yaw: f32) {
    let yaw = if yaw.is_finite() { yaw } else { 0.0 };
    let (d, n) = ([yaw.sin(), -yaw.cos()], [yaw.cos(), yaw.sin()]);
    let at = |along: f32, across: f32| {
        [
            centre[0] + d[0] * along * radius + n[0] * across * radius,
            centre[1] + d[1] * along * radius + n[1] * across * radius,
        ]
    };
    for [x, y] in [at(1.0, 0.0), at(-0.6, 0.6), at(-0.6, -0.6)] {
        p.vertices.push(Vertex {
            position: [2.0 * x / p.width - 1.0, 1.0 - 2.0 * y / p.height, 0.0],
            color: PANEL,
            uv: [0.0; 2],
            texture: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exploration::{Connection, Edge, Room, RoomId};

    fn visited_pair() -> Map {
        let home = RoomId::new("https://site.test/home", 0);
        let about = RoomId::new("https://site.test/about", 0);
        let unseen = RoomId::new("https://site.test/unseen", 0);
        Map {
            rooms: vec![
                Room {
                    id: home,
                    title: "Home".into(),
                    cell: [0, 0],
                    exits: vec![about],
                    layout: None,
                    stale: false,
                },
                Room {
                    id: about,
                    title: "About".into(),
                    cell: [1, 0],
                    exits: vec![home, unseen],
                    layout: None,
                    stale: false,
                },
            ],
            edges: vec![Edge {
                from: 0,
                to: 1,
                kind: Connection::Portal,
            }],
            current: Some(1),
            recent: vec![0, 1],
            limited: false,
        }
    }

    #[test]
    fn connections_and_unexplored_stubs_have_distinct_visible_pixels() {
        let m = visited_pair();
        let linked = overlay(&m, MapMode::Expanded, 0.0, 960, 640);
        let mut bare = m.clone();
        bare.edges.clear();
        for r in &mut bare.rooms {
            r.exits.clear();
        }
        let plain = overlay(&bare, MapMode::Expanded, 0.0, 960, 640);
        assert!(
            bytemuck::cast_slice::<Vertex, u8>(&linked)
                != bytemuck::cast_slice::<Vertex, u8>(&plain),
            "connections and unexplored exits must change rendered geometry"
        );
    }
    #[test]
    fn few_room_maps_have_readable_markers_and_reserved_bands() {
        assert!(layout(MapMode::Compact, [5, 5], 640, 480).unwrap().cell >= 24.0);
        assert!(layout(MapMode::Expanded, [5, 2], 960, 640).unwrap().cell >= 44.0);
        for (w, h) in [(640, 480), (800, 533), (960, 640)] {
            for mode in [MapMode::Compact, MapMode::Expanded] {
                let l = layout(mode, [5, 5], w, h).unwrap();
                assert!(l.y >= TOP && l.y + l.h <= h as f32 - BOTTOM + 0.01);
            }
        }
    }
    #[test]
    fn recording_limit_without_current_still_has_visible_disclosure() {
        let mut m = visited_pair();
        m.current = None;
        m.limited = true;
        assert!(!overlay(&m, MapMode::Compact, 0.0, 640, 480).is_empty());
        assert!(overlay(&m, MapMode::Hidden, 0.0, 640, 480).is_empty());
    }
    #[test]
    fn connector_and_stub_changes_are_independent_and_finite() {
        let m = visited_pair();
        let bytes = |m: &Map| {
            bytemuck::cast_slice::<Vertex, u8>(&overlay(m, MapMode::Expanded, 0.0, 640, 480))
                .to_vec()
        };
        let mut edges = m.clone();
        edges.edges.clear();
        assert_ne!(bytes(&m), bytes(&edges));
        let mut exits = m.clone();
        for r in &mut exits.rooms {
            r.exits.clear();
        }
        assert_ne!(bytes(&m), bytes(&exits));
        let mut passage = m.clone();
        passage.edges[0].kind = Connection::Passage;
        assert_ne!(bytes(&m), bytes(&passage));
        for (w, h) in [(640, 480), (800, 500), (960, 640), (1920, 1280)] {
            for mode in [MapMode::Compact, MapMode::Expanded] {
                let v = overlay(&m, mode, f32::NAN, w, h);
                assert!(v.len() < 100_000);
                assert!(v.iter().all(|v| {
                    v.position
                        .iter()
                        .all(|x| x.is_finite() && (-1.0..=1.0).contains(x))
                }));
            }
        }
    }
    #[test]
    fn compact_map_of_visited_rooms_draws_vertices() {
        let vertices = overlay(&visited_pair(), MapMode::Compact, 0.0, 640, 480);
        assert!(
            !vertices.is_empty(),
            "compact map with visited rooms must draw a panel"
        );
    }
}
