use super::Painter;
use crate::{assets::Vertex, directory_map::Map, exploration::MapMode};
const CURRENT: [f32; 3] = [0.50, 0.85, 0.53];
const BG: [f32; 3] = [0.025, 0.045, 0.038];
const INK: [f32; 3] = [0.79, 0.87, 0.81];
const MUTED: [f32; 3] = [0.43, 0.56, 0.48];
const BACK: [f32; 3] = [0.94, 0.77, 0.47];
const LINK: [f32; 3] = [0.28, 0.48, 0.36];
pub(super) fn short(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!(
            "{}..",
            s.chars().take(n.saturating_sub(2)).collect::<String>()
        )
    } else {
        s.into()
    }
}
fn stroke(p: &mut Painter, a: [f32; 2], b: [f32; 2], color: [f32; 3], thickness: f32) {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = d[0].hypot(d[1]);
    if len < 0.1 {
        return;
    }
    let n = [-d[1] * thickness / len * 0.5, d[0] * thickness / len * 0.5];
    for q in [
        [a[0] + n[0], a[1] + n[1]],
        [b[0] + n[0], b[1] + n[1]],
        [b[0] - n[0], b[1] - n[1]],
        [a[0] + n[0], a[1] + n[1]],
        [b[0] - n[0], b[1] - n[1]],
        [a[0] - n[0], a[1] - n[1]],
    ] {
        p.vertices.push(Vertex {
            position: [2.0 * q[0] / p.width - 1.0, 1.0 - 2.0 * q[1] / p.height, 0.0],
            color,
            uv: [0.0; 2],
            texture: 0,
        });
    }
}
// Clip the segment before tessellation. Even deep off-view branches do bounded work.
fn segment(a: [f32; 2], b: [f32; 2], r: [f32; 4]) -> Option<([f32; 2], [f32; 2])> {
    let d = [b[0] - a[0], b[1] - a[1]];
    let mut lo: f32 = 0.0;
    let mut hi: f32 = 1.0;
    for (p, q) in [
        (-d[0], a[0] - r[0]),
        (d[0], r[2] - a[0]),
        (-d[1], a[1] - r[1]),
        (d[1], r[3] - a[1]),
    ] {
        if p.abs() < 0.0001 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                lo = lo.max(t);
            } else {
                hi = hi.min(t);
            }
            if lo > hi {
                return None;
            }
        }
    }
    Some((
        [a[0] + lo * d[0], a[1] + lo * d[1]],
        [a[0] + hi * d[0], a[1] + hi * d[1]],
    ))
}
pub fn overlay(map: &Map, mode: MapMode, width: u32, height: u32, pan: [i32; 2]) -> Vec<Vertex> {
    overlay_with_controls(map, mode, width, height, pan, Controls::default())
}
#[derive(Default)]
pub struct Controls<'a> {
    pub selection: Option<&'a crate::directory_map::Confirmation>,
    pub history_offset: usize,
    pub confirm_travel: bool,
    pub hover: Option<crate::directory_map_view::Action>,
}
pub fn overlay_with_controls(
    map: &Map,
    mode: MapMode,
    width: u32,
    height: u32,
    pan: [i32; 2],
    controls: Controls<'_>,
) -> Vec<Vertex> {
    if mode == MapMode::Hidden || width < 640 || height < 480 || map.nodes.is_empty() {
        return vec![];
    }
    let full = mode == MapMode::Expanded;
    let w = width as f32;
    let h = height as f32;
    let mut p = Painter {
        vertices: vec![],
        width: w,
        height: h,
    };
    let panel = if full {
        [0.0, 0.0, w, h]
    } else {
        [w - 256.0, 76.0, w - 16.0, 346.0]
    };
    p.rect(
        panel[0],
        panel[1],
        panel[2] - panel[0],
        panel[3] - panel[1],
        BG,
    );
    let view = crate::directory_map_view::View::new(
        map,
        width,
        height,
        full,
        pan,
        controls.history_offset,
    );
    let area = view.area;
    let scale = view.scale;
    let points = &view.points;
    let current = map.current.unwrap_or(0);
    let visible = |i: usize| view.visible(i);
    let mut graph = Painter {
        vertices: vec![],
        width: w,
        height: h,
    };
    for (parent, child, path) in &view.edges {
        let mut ends = vec![];
        for q in path.windows(2) {
            if let Some((a, b)) = segment(q[0], q[1], area) {
                stroke(&mut graph, a, b, LINK, if full { 2.0 } else { 1.5 });
                ends.push((a, b));
            }
        }
        if full && !ends.is_empty() {
            for (id, q) in [(*parent, ends[0].0), (*child, ends.last().unwrap().1)] {
                if !visible(id) {
                    graph.text(
                        q[0].clamp(area[0] + 4.0, area[2] - 42.0),
                        q[1].clamp(area[1] + 4.0, area[3] - 14.0),
                        &format!("#{}", id + 1),
                        1.2,
                        MUTED,
                    );
                }
            }
        }
    }
    for (i, node) in map.nodes.iter().enumerate() {
        let q = points[i];
        let nw = view.cards[i][2] - view.cards[i][0];
        let nh = view.cards[i][3] - view.cards[i][1];
        if !visible(i) {
            continue;
        }
        let hovered = full
            && controls.hover
                == Some(crate::directory_map_view::Action::Destination(
                    crate::directory_map::Target::Node(i),
                ));
        let color = if hovered {
            [0.75, 1.0, 0.79]
        } else if i == current {
            CURRENT
        } else if map.back == Some(i) {
            BACK
        } else {
            MUTED
        };
        graph.rect(
            q[0] - nw * 0.5 - 2.0,
            q[1] - nh * 0.5 - 2.0,
            nw + 4.0,
            nh + 4.0,
            color,
        );
        graph.rect(
            q[0] - nw * 0.5,
            q[1] - nh * 0.5,
            nw,
            nh,
            if i == current {
                [0.10, 0.22, 0.14]
            } else {
                [0.055, 0.095, 0.073]
            },
        );
        if full {
            let fs = 1.25 * scale;
            let x = q[0] - nw * 0.5 + 7.0 * scale;
            graph.text(
                x,
                q[1] - nh * 0.5 + 7.0 * scale,
                &format!("#{}", i + 1),
                scale,
                color,
            );
            if i == current {
                graph.text(
                    q[0] + nw * 0.5 - 31.0 * scale,
                    q[1] - nh * 0.5 + 7.0 * scale,
                    "YOU",
                    scale,
                    CURRENT,
                );
            } else if map.back == Some(i) {
                graph.text(
                    q[0] + nw * 0.5 - 38.0 * scale,
                    q[1] - nh * 0.5 + 7.0 * scale,
                    "BACK",
                    scale,
                    BACK,
                );
            }
            let label = short(&node.name, 30);
            let first: String = label.chars().take(15).collect();
            let second: String = label.chars().skip(15).collect();
            graph.text(x, q[1] - 12.0 * scale, &first, fs, INK);
            graph.text(x, q[1] + 2.0 * scale, &second, fs, INK);
            graph.text(
                x,
                q[1] + nh * 0.5 - 17.0 * scale,
                &format!("{}D  {}F", node.folders, node.files),
                scale,
                MUTED,
            );
        } else {
            graph.text(q[0] - 7.0, q[1] - 4.0, &format!("{}", i + 1), 1.0, color);
        }
    }
    // All graph quads are clipped to their own pane, never over header/sidebar/footer.
    for v in &mut graph.vertices {
        v.position[0] = v.position[0].clamp(2.0 * area[0] / w - 1.0, 2.0 * area[2] / w - 1.0);
        v.position[1] = v.position[1].clamp(1.0 - 2.0 * area[3] / h, 1.0 - 2.0 * area[1] / h);
    }
    p.vertices.extend(graph.vertices);
    let n = &map.nodes[current];
    if full {
        p.text(26.0, 24.0, "DIRECTORY MAP", 2.4, INK);
        p.text(
            26.0,
            58.0,
            "FOLDERS ARE ROOMS / FILES ARE CONTENTS",
            1.3,
            MUTED,
        );
        p.text(
            26.0,
            86.0,
            &format!(
                "YOU: #{} {}",
                current + 1,
                short(&n.name, ((w - view.side - 160.0) / 9.0) as usize)
            ),
            1.5,
            CURRENT,
        );
        p.text(
            26.0,
            112.0,
            &format!(
                "FOLDER PAGE {}/{} | ONE ROOM ON THIS MAP",
                n.page + 1,
                n.pages
            ),
            1.2,
            MUTED,
        );
        {
            let target = |id: Option<usize>| {
                id.map(|i| format!("#{} {}", i + 1, short(&map.nodes[i].name, 18)))
                    .unwrap_or_else(|| "NOT MAPPED".into())
            };
            let back = if map.back.is_none() && map.has_back {
                "RETAINED OUTSIDE MAP".into()
            } else {
                target(map.back)
            };
            p.text(
                26.0,
                132.0,
                &format!("BACK(R): {} / PARENT(U): {}", back, target(n.parent)),
                1.1,
                BACK,
            );
        }
        p.rect(24.0, h - 68.0, w - 48.0, 1.0, [0.15, 0.25, 0.19]);
        p.text(
            26.0,
            h - 53.0,
            &format!(
                "{} {}/{} VISITED FOLDERS IN VIEW",
                if map.restarted {
                    "MAP RESTARTED (128 LIMIT) |"
                } else {
                    "D=FOLDERS F=FILES |"
                },
                (0..map.nodes.len()).filter(|&i| visible(i)).count(),
                map.nodes.len()
            ),
            1.15,
            MUTED,
        );
        p.text(
            26.0,
            h - 29.0,
            "ARROWS PAN | M/ESC CLOSE | SHIFT+M MINIMAP",
            1.25,
            INK,
        );
    } else {
        p.text(panel[0] + 12.0, panel[1] + 12.0, "DIRECTORY MAP", 1.4, INK);
        p.text(
            panel[0] + 12.0,
            panel[1] + 35.0,
            &short(&format!("YOU: {}", n.name), 29),
            1.15,
            CURRENT,
        );
        p.text(
            panel[0] + 12.0,
            panel[3] - 23.0,
            "M EXPAND / SHIFT+M HIDE",
            1.1,
            MUTED,
        );
    }
    if full {
        super::directory_travel::draw(&mut p, map, &view, &controls);
    }
    p.vertices
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_map_restart_keeps_controls_and_discloses_retained_back() {
        let mut m = Map::default();
        m.observe(
            &crate::local_explorer::Snapshot {
                folder: "/example".into(),
                entries: vec![],
            },
            [1, 1],
            Some([2, 2]),
            0,
            [-10.5, -6.0, 10.5, 6.0],
            crate::directory_map::Relation::Selection,
        );
        for (w, h) in [(640, 480), (1600, 1100)] {
            let normal = overlay(&m, MapMode::Expanded, w, h, [0, 0]);
            m.restarted = true;
            let restarted = overlay(&m, MapMode::Expanded, w, h, [0, 0]);
            let footer = |v: &Vec<Vertex>| {
                v.iter()
                    .filter(|v| {
                        v.color == INK && v.position[1] < 1.0 - 2.0 * (h as f32 - 32.0) / h as f32
                    })
                    .map(|v| v.position)
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                footer(&normal),
                footer(&restarted),
                "restart disclosure must not replace controls"
            );
            let mut expected = Painter {
                vertices: vec![],
                width: w as f32,
                height: h as f32,
            };
            // Back/Parent now live in the fixed header at all widths; the new
            // sidebar is chronological route history, not duplicate metadata.
            expected.text(
                26.0,
                132.0,
                "BACK(R): RETAINED OUTSIDE MAP / PARENT(U): NOT MAPPED",
                1.1,
                BACK,
            );
            let actual: Vec<_> = restarted.iter().map(|v| (v.position, v.color)).collect();
            let wanted: Vec<_> = expected
                .vertices
                .iter()
                .map(|v| (v.position, v.color))
                .collect();
            assert!(
                actual.windows(wanted.len()).any(|v| v == wanted),
                "retained Back outside map must be explicit"
            );
            m.restarted = false;
        }
    }
    #[test]
    fn small_directory_map_keeps_back_disclosure_when_panned() {
        let mut m = Map::default();
        let mut s = crate::local_explorer::Snapshot {
            folder: "/parent".into(),
            entries: vec![],
        };
        m.observe(
            &s,
            [1, 1],
            None,
            0,
            [-10.5, -6.0, 10.5, 6.0],
            crate::directory_map::Relation::Selection,
        );
        s.folder = "/parent/child".into();
        m.observe(
            &s,
            [1, 2],
            Some([1, 1]),
            0,
            [-10.5, -6.0, 10.5, 6.0],
            crate::directory_map::Relation::ChildOf([1, 1]),
        );
        let v = overlay(&m, MapMode::Expanded, 640, 480, [24, 24]);
        assert!(
            v.iter().any(|v| v.color == BACK),
            "held Back must remain disclosed even when its room is off-view"
        );
    }
    #[test]
    fn directory_map_overlay_covers_fullscreen_and_keeps_current_visible() {
        let mut m = Map::default();
        let s = crate::local_explorer::Snapshot {
            folder: "/Example".into(),
            entries: vec![],
        };
        m.observe(
            &s,
            [1, 1],
            None,
            0,
            [-10.5, -6.0, 10.5, 6.0],
            crate::directory_map::Relation::Selection,
        );
        for (w, h) in [
            (640, 480),
            (1280, 480),
            (1600, 480),
            (1280, 960),
            (1600, 1100),
        ] {
            for percent in crate::presentation::STEPS {
                let logical = crate::presentation::ui_viewport([w, h], percent);
                assert!(
                    !overlay(&m, MapMode::Expanded, logical[0], logical[1], [0, 0]).is_empty(),
                    "scaled map must not become an invisible modal: {w}x{h} at {percent}%"
                );
            }
            let v = overlay(&m, MapMode::Expanded, w, h, [0, 0]);
            assert!(!v.is_empty(), "directory overlay must render after local M");
            assert!(v.iter().any(|v| v.color == CURRENT));
            assert!(v.iter().any(|v| v.position[..2] == [-1.0, 1.0]));
            assert!(v.iter().any(|v| v.position[..2] == [1.0, -1.0]));
            for q in v {
                assert!(q.position.iter().all(|n| n.is_finite()));
                assert!(
                    (-1.0..=1.0).contains(&q.position[0]) && (-1.0..=1.0).contains(&q.position[1])
                );
            }
        }
        assert!(overlay(&m, MapMode::Hidden, 1280, 960, [0, 0]).is_empty());
    }
}
