//! Focus is presentation only; all port tangents/proportions come from the committed host.
use super::*;
use crate::exploration::{Port, Room, RoomId};
pub(super) struct Node {
    pub index: usize,
    pub rect: [f32; 4],
    pub progress: [usize; 2],
}
pub(super) struct Marker {
    pub point: [f32; 2],
    inward: [f32; 2],
    known: bool,
    typed: bool,
}
pub(super) struct Path {
    pub points: Vec<[f32; 2]>,
    kind: Connection,
    travelled: bool,
}
pub(super) struct Scene {
    pub nodes: Vec<Node>,
    pub ports: Vec<Marker>,
    pub paths: Vec<Path>,
    pub panel: [f32; 4],
    pub omitted: usize,
}
fn current_position(bounds: [f32; 4], rect: [f32; 4], position: [f32; 2]) -> [f32; 2] {
    [
        rect[0]
            + ((position[0] - bounds[0]) / (bounds[2] - bounds[0])).clamp(0.0, 1.0)
                * (rect[2] - rect[0]),
        rect[1]
            + ((position[1] - bounds[1]) / (bounds[3] - bounds[1])).clamp(0.0, 1.0)
                * (rect[3] - rect[1]),
    ]
}
fn bounds(room: &Room) -> [f32; 4] {
    room.layout
        .as_ref()
        .map_or([-1.0, -1.0, 1.0, 1.0], |l| l.bounds)
}
fn fit(b: [f32; 4], centre: [f32; 2], max: [f32; 2]) -> [f32; 4] {
    let w = b[2] - b[0];
    let h = b[3] - b[1];
    let s = (max[0] / w).min(max[1] / h);
    [
        centre[0] - w * s * 0.5,
        centre[1] - h * s * 0.5,
        centre[0] + w * s * 0.5,
        centre[1] + h * s * 0.5,
    ]
}
fn port_point(room: &Room, r: [f32; 4], p: &Port) -> [f32; 2] {
    let b = bounds(room);
    // Door anchors sit just outside the wall. Project normal to the outline;
    // preserve the host-owned wall-tangent coordinate without stretching axes.
    [
        r[0] + ((p.position[0] - b[0]) / (b[2] - b[0])).clamp(0.0, 1.0) * (r[2] - r[0]),
        r[1] + ((p.position[1] - b[1]) / (b[3] - b[1])).clamp(0.0, 1.0) * (r[3] - r[1]),
    ]
}
fn endpoint(room: &Room, r: [f32; 4], target: RoomId) -> ([f32; 2], [f32; 2]) {
    if let Some(p) = room.layout.as_ref().and_then(|l| {
        l.ports
            .iter()
            .find(|p| p.target == Some(target))
            .or_else(|| l.ports.iter().find(|p| p.anchor == "return-history"))
    }) {
        return (port_point(room, r, p), p.inward);
    }
    ([(r[0] + r[2]) * 0.5, r[1]], [0.0, 1.0])
}
fn route(
    start: [f32; 2],
    ai: [f32; 2],
    end: [f32; 2],
    bi: [f32; 2],
    rect: [f32; 4],
) -> Vec<[f32; 2]> {
    let a = [start[0] - ai[0] * 8.0, start[1] - ai[1] * 8.0];
    let b = [end[0] - bi[0] * 8.0, end[1] - bi[1] * 8.0];
    let obstacle = [rect[0] - 6.0, rect[1] - 6.0, rect[2] + 6.0, rect[3] + 6.0];
    let xs = [a[0], b[0], rect[0] - 12.0, rect[2] + 12.0];
    let ys = [a[1], b[1], rect[1] - 12.0, rect[3] + 12.0];
    let points: Vec<_> = xs
        .iter()
        .flat_map(|x| ys.iter().map(move |y| [*x, *y]))
        .collect();
    let clear = |a: [f32; 2], b: [f32; 2]| {
        if a[0] == b[0] {
            !(a[0] > obstacle[0]
                && a[0] < obstacle[2]
                && a[1].min(b[1]) < obstacle[3]
                && a[1].max(b[1]) > obstacle[1])
        } else if a[1] == b[1] {
            !(a[1] > obstacle[1]
                && a[1] < obstacle[3]
                && a[0].min(b[0]) < obstacle[2]
                && a[0].max(b[0]) > obstacle[0])
        } else {
            false
        }
    };
    let first = points.iter().position(|p| *p == a).unwrap();
    let last = points.iter().position(|p| *p == b).unwrap();
    let mut distances = [f32::INFINITY; 16];
    let mut previous = [None; 16];
    let mut done = [false; 16];
    distances[first] = 0.0;
    for _ in 0..16 {
        let Some(i) = (0..16)
            .filter(|i| !done[*i])
            .min_by(|a, b| distances[*a].total_cmp(&distances[*b]))
        else {
            break;
        };
        if !distances[i].is_finite() {
            break;
        }
        done[i] = true;
        if i == last {
            break;
        }
        for j in 0..16 {
            if !done[j] && clear(points[i], points[j]) {
                let cost = distances[i]
                    + (points[i][0] - points[j][0]).abs()
                    + (points[i][1] - points[j][1]).abs();
                if cost < distances[j] {
                    distances[j] = cost;
                    previous[j] = Some(i);
                }
            }
        }
    }
    if !distances[last].is_finite() {
        return Vec::new();
    }
    let mut middle = vec![b];
    let mut cursor = last;
    while cursor != first {
        let Some(i) = previous[cursor] else {
            return Vec::new();
        };
        cursor = i;
        middle.push(points[i]);
    }
    middle.reverse();
    let mut result = vec![start];
    result.extend(middle);
    result.push(end);
    result
}
pub(super) fn scene(map: &Map, mode: MapMode, width: u32, height: u32) -> Option<Scene> {
    if mode == MapMode::Hidden || width < 640 || height < 480 {
        return None;
    }
    let current = map.current.filter(|i| *i < map.rooms.len())?;
    let physical = bounds(&map.rooms[current]);
    if !physical.iter().all(|v| v.is_finite())
        || physical[2] <= physical[0]
        || physical[3] <= physical[1]
    {
        return None;
    }
    let w = if mode == MapMode::Compact {
        240.0
    } else {
        (width as f32 - 24.0).min(1000.0)
    };
    let h = (height as f32 - TOP - BOTTOM).min(if mode == MapMode::Compact {
        330.0
    } else {
        640.0
    });
    let panel = [
        width as f32 - w - MARGIN,
        TOP,
        width as f32 - MARGIN,
        TOP + h,
    ];
    let body = [
        panel[0] + 12.0,
        panel[1] + 48.0,
        panel[2] - 12.0,
        panel[3] - 80.0,
    ];
    let centre = [(body[0] + body[2]) * 0.5, (body[1] + body[3]) * 0.5];
    let mut indices: Vec<_> = (0..map.rooms.len().min(128))
        .filter(|i| *i != current)
        .collect();
    let origin = map.rooms[current].cell;
    // Adjacent linked identities first, then stable cell distance and ordinal.
    indices.sort_by_key(|i| {
        let linked =
            map.rooms[current].exits.contains(&map.rooms[*i].id)
                || map.edges.iter().take(512).any(|e| {
                    (e.from == current && e.to == *i) || (e.to == current && e.from == *i)
                });
        let c = map.rooms[*i].cell;
        (
            !linked,
            (c[0] as i64 - origin[0] as i64).abs() + (c[1] as i64 - origin[1] as i64).abs(),
            *i,
        )
    });
    let rows = (((body[3] - body[1]) / 42.0).floor() as usize).clamp(1, 6);
    indices.truncate(rows * 2);
    let progress = |room: &Room| {
        let targets: Vec<_> = room.exits.iter().take(66).collect();
        [
            targets
                .iter()
                .filter(|id| map.rooms.iter().any(|r| !r.stale && r.id == ***id))
                .count(),
            targets.len(),
        ]
    };
    let mut nodes = vec![Node {
        index: current,
        rect: fit(
            physical,
            centre,
            [(w - 152.0).max(100.0), (body[3] - body[1] - 12.0).max(60.0)],
        ),
        progress: progress(&map.rooms[current]),
    }];
    for (slot, &index) in indices.iter().enumerate() {
        let side = slot % 2;
        let row = slot / 2;
        let c = [
            if side == 0 {
                body[0] + 24.0
            } else {
                body[2] - 24.0
            },
            body[1] + (row as f32 + 0.5) * (body[3] - body[1]) / rows as f32 - 6.0,
        ];
        let b = bounds(&map.rooms[index]);
        if !b.iter().all(|v| v.is_finite()) || b[2] <= b[0] || b[3] <= b[1] {
            continue;
        }
        nodes.push(Node {
            index,
            rect: fit(b, c, [36.0, 24.0]),
            progress: progress(&map.rooms[index]),
        });
    }
    let mut ports = Vec::new();
    if let Some(l) = &map.rooms[current].layout {
        for p in l.ports.iter().take(67).filter(|p| {
            p.position
                .iter()
                .chain(p.inward.iter())
                .all(|v| v.is_finite())
        }) {
            ports.push(Marker {
                point: port_point(&map.rooms[current], nodes[0].rect, p),
                inward: p.inward,
                typed: p.target.is_none() || p.kind == Connection::Passage,
                known: p
                    .target
                    .is_some_and(|id| map.rooms.iter().any(|r| r.id == id && !r.stale)),
            });
        }
    }
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for e in map.edges.iter().take(512) {
        if e.from == e.to
            || !seen.insert((
                e.from.min(e.to),
                e.from.max(e.to),
                e.kind == Connection::Portal,
            ))
        {
            continue;
        }
        let Some(a) = nodes.iter().find(|n| n.index == e.from) else {
            continue;
        };
        let Some(b) = nodes.iter().find(|n| n.index == e.to) else {
            continue;
        };
        let (start, ai) = endpoint(&map.rooms[a.index], a.rect, map.rooms[b.index].id);
        let (end, bi) = endpoint(&map.rooms[b.index], b.rect, map.rooms[a.index].id);
        paths.push(Path {
            points: route(start, ai, end, bi, nodes[0].rect),
            kind: e.kind,
            travelled: true,
        });
    }
    // Outbound known links can connect already visited identities even before
    // that particular direction has been traversed. Never invent unseen nodes.
    for p in map.rooms[current]
        .layout
        .as_ref()
        .into_iter()
        .flat_map(|l| l.ports.iter())
        .take(67)
    {
        let Some(target) = p.target else {
            continue;
        };
        let Some(n) = nodes
            .iter()
            .find(|n| map.rooms[n.index].id == target && n.index != current)
        else {
            continue;
        };
        let start = port_point(&map.rooms[current], nodes[0].rect, p);
        if paths
            .iter()
            .any(|path| path.points.first() == Some(&start) || path.points.last() == Some(&start))
        {
            continue;
        }
        let (end, bi) = endpoint(&map.rooms[n.index], n.rect, map.rooms[current].id);
        paths.push(Path {
            points: route(start, p.inward, end, bi, nodes[0].rect),
            kind: p.kind,
            travelled: false,
        });
    }
    Some(Scene {
        omitted: map.rooms.len().saturating_sub(nodes.len()),
        nodes,
        ports,
        paths,
        panel,
    })
}
fn line(p: &mut Painter, a: [f32; 2], b: [f32; 2], thickness: f32, color: [f32; 3]) {
    if (a[0] - b[0]).abs() < 0.01 {
        p.rect(
            a[0] - thickness * 0.5,
            a[1].min(b[1]),
            thickness,
            (a[1] - b[1]).abs().max(thickness),
            color,
        );
    } else {
        p.rect(
            a[0].min(b[0]),
            a[1] - thickness * 0.5,
            (a[0] - b[0]).abs().max(thickness),
            thickness,
            color,
        );
    }
}
pub(super) fn overlay(
    map: &Map,
    mode: MapMode,
    position: Option<[f32; 2]>,
    yaw: f32,
    width: u32,
    height: u32,
) -> Vec<Vertex> {
    let Some(s) = scene(map, mode, width, height) else {
        return Vec::new();
    };
    let mut p = Painter {
        vertices: Vec::new(),
        width: width as f32,
        height: height as f32,
    };
    let [x, y, x1, y1] = s.panel;
    p.rect(x, y, x1 - x, y1 - y, PANEL);
    let columns = ((x1 - x - 16.0) / 12.0) as usize;
    p.text(
        x + 8.0,
        y + 8.0,
        &bounded(
            &format!(
                "MAP {}FOCUS #{}",
                if map.limited { "CAP " } else { "" },
                s.nodes[0].index + 1
            ),
            columns,
        ),
        2.0,
        LABEL,
    );
    for path in &s.paths {
        for pair in path.points.windows(2) {
            line(
                &mut p,
                pair[0],
                pair[1],
                if path.travelled && path.kind == Connection::Portal {
                    2.0
                } else {
                    1.0
                },
                if path.travelled { LINK } else { STALE },
            );
        }
    }
    for node in &s.nodes {
        let r = node.rect;
        let current = node.index == s.nodes[0].index;
        let room = &map.rooms[node.index];
        let color = if current {
            CURRENT
        } else if room.stale {
            STALE
        } else {
            VISITED
        };
        p.rect(r[0], r[1], r[2] - r[0], r[3] - r[1], color);
        if r[2] - r[0] > 4.0 && r[3] - r[1] > 4.0 {
            p.rect(
                r[0] + 2.0,
                r[1] + 2.0,
                r[2] - r[0] - 4.0,
                r[3] - r[1] - 4.0,
                PANEL,
            );
        }
        let centre = [(r[0] + r[2]) * 0.5, (r[1] + r[3]) * 0.5];
        if current {
            if let Some(position) = position.filter(|p| p.iter().all(|v| v.is_finite())) {
                let marker_start = p.vertices.len();
                facing(
                    &mut p,
                    current_position(bounds(room), r, position),
                    8.0,
                    yaw,
                );
                // Legacy cells are solid green and need a dark marker. Physical
                // outlines have a dark interior, so invert only this marker.
                for vertex in &mut p.vertices[marker_start..] {
                    vertex.color = CURRENT;
                }
            }
        } else {
            let label = format!(
                "{}#{}",
                if room.layout.is_none() { "~" } else { "" },
                node.index + 1
            );
            p.text(
                centre[0] - label.len() as f32 * 3.0,
                centre[1] - 3.5,
                &label,
                1.0,
                LABEL,
            );
            let counter = format!("{}/{}", node.progress[0], node.progress[1]);
            p.text(
                centre[0] - counter.len() as f32 * 3.0,
                r[3] + 3.0,
                &counter,
                1.0,
                LABEL,
            );
        }
    }
    for m in &s.ports {
        let color = if m.typed {
            LABEL
        } else if m.known {
            CURRENT
        } else {
            STUB
        };
        p.rect(m.point[0] - 1.5, m.point[1] - 1.5, 3.0, 3.0, color);
        line(
            &mut p,
            m.point,
            [
                m.point[0] - m.inward[0] * 6.0,
                m.point[1] - m.inward[1] * 6.0,
            ],
            1.0,
            color,
        );
    }
    let n = &s.nodes[0];
    p.text(
        x + 8.0,
        y1 - 62.0,
        &bounded(
            &format!("#{} {}", n.index + 1, map.rooms[n.index].title),
            columns,
        ),
        2.0,
        CURRENT,
    );
    p.text(
        x + 8.0,
        y1 - 42.0,
        &bounded(
            &if mode == MapMode::Compact {
                format!("{}/{} EXPLORED", n.progress[0], n.progress[1])
            } else {
                format!(
                    "{}/{} EXPLORED | {} OFF MAP",
                    n.progress[0], n.progress[1], s.omitted
                )
            },
            columns,
        ),
        2.0,
        LABEL,
    );
    if mode == MapMode::Compact {
        p.text(
            x + 8.0,
            y1 - 24.0,
            &format!("{} OFF MAP", s.omitted),
            2.0,
            LABEL,
        );
    }
    p.text(
        x + 8.0,
        if mode == MapMode::Compact {
            y1 - 9.0
        } else {
            y1 - 20.0
        },
        &bounded(
            if mode == MapMode::Expanded {
                "PHYSICAL DOORS | PORTAL GRAPH | ~ SCHEMATIC | M MAP"
            } else {
                "PORTAL GRAPH | M MAP"
            },
            columns,
        ),
        1.0,
        LINK,
    );
    p.vertices
}
#[cfg(test)]
mod position_contract {
    use super::*;
    #[test]
    fn physical_position_triangle_contrasts_with_the_hollow_room() {
        let mut map = Map::default();
        let id = RoomId::new("https://test/marker", 0);
        map.arrive(id, "Marker", [], Connection::Portal, false);
        map.rooms[0].layout = Some(crate::exploration::RoomLayout {
            bounds: [-8.0, -32.0, 8.0, 32.0],
            ports: vec![],
        });
        let scene = scene(&map, MapMode::Expanded, 960, 640).unwrap();
        let r = scene.nodes[0].rect;
        let at = current_position([-8.0, -32.0, 8.0, 32.0], r, [0.0, 0.0]);
        let tip = [2.0 * at[0] / 960.0 - 1.0, 1.0 - 2.0 * (at[1] - 8.0) / 640.0];
        let vertices = overlay(&map, MapMode::Expanded, Some([0.0, 0.0]), 0.0, 960, 640);
        let v = vertices
            .iter()
            .find(|v| {
                (v.position[0] - tip[0]).abs() < 1e-6 && (v.position[1] - tip[1]).abs() < 1e-6
            })
            .unwrap();
        assert_eq!(
            v.color, CURRENT,
            "dark legacy marker disappears against the hollow physical-room interior"
        );
    }
    #[test]
    fn hallway_position_moves_from_one_end_to_the_other() {
        let bounds = [-8.0, -32.0, 8.0, 32.0];
        let rect = [100.0, 100.0, 150.0, 300.0];
        let front = current_position(bounds, rect, [0.0, -28.0]);
        let rear = current_position(bounds, rect, [0.0, 28.0]);
        assert!(front[1] < 150.0 && rear[1] > 250.0);
        assert_eq!(current_position(bounds, rect, [0.0, 100.0])[1], 300.0);
    }
}
