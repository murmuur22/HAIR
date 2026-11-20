//! File-only preparation: parent must compile and observe behavioral RED first.
use hair::{
    World,
    assets::Vertex,
    folder_scene,
    local_explorer::Snapshot,
    navigation::{Navigation, Row},
    text,
};
use std::collections::BTreeSet;

const EPS: f32 = 0.01;
const HEADER: [f32; 3] = [0.8; 3];
const ORDINARY: [f32; 3] = [0.85; 3];
const CURRENT: [f32; 3] = [0.6, 1.0, 0.7];
const TREE_PANEL: [f32; 3] = [0.012; 3];

#[derive(Clone, Copy, Debug)]
struct Bounds {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}
impl Bounds {
    fn contains(self, other: Self) -> bool {
        other.left + EPS >= self.left
            && other.top + EPS >= self.top
            && other.right <= self.right + EPS
            && other.bottom <= self.bottom + EPS
    }
}
fn bounds(vertices: &[Vertex], width: u32, height: u32) -> Bounds {
    assert!(!vertices.is_empty(), "missing geometry");
    let mut b = Bounds {
        left: f32::INFINITY,
        top: f32::INFINITY,
        right: f32::NEG_INFINITY,
        bottom: f32::NEG_INFINITY,
    };
    for v in vertices {
        assert!(v.position.iter().all(|p| p.is_finite()));
        let x = (v.position[0] + 1.0) * 0.5 * width as f32;
        let y = (1.0 - v.position[1]) * 0.5 * height as f32;
        b.left = b.left.min(x);
        b.right = b.right.max(x);
        b.top = b.top.min(y);
        b.bottom = b.bottom.max(y);
    }
    assert!(b.left >= -EPS && b.top >= -EPS, "negative bounds: {b:?}");
    assert!(b.right <= width as f32 + EPS && b.bottom <= height as f32 + EPS);
    assert!(
        b.right > b.left && b.bottom > b.top,
        "degenerate bounds: {b:?}"
    );
    b
}
fn key(v: &Vertex) -> [u32; 6] {
    [
        v.position[0].to_bits(),
        v.position[1].to_bits(),
        v.position[2].to_bits(),
        v.color[0].to_bits(),
        v.color[1].to_bits(),
        v.color[2].to_bits(),
    ]
}
fn colored(vertices: &[Vertex], color: [f32; 3]) -> Vec<Vertex> {
    vertices
        .iter()
        .filter(|v| v.color == color)
        .copied()
        .collect()
}
fn difference(original: &[Vertex], control: &[Vertex], color: [f32; 3]) -> Vec<Vertex> {
    let other: BTreeSet<_> = control.iter().map(key).collect();
    original
        .iter()
        .filter(|v| v.color == color && !other.contains(&key(v)))
        .copied()
        .collect()
}
fn tree(world: &World, width: u32, height: u32) -> Vec<Vertex> {
    let ui = text::overlay(world, width, height, 0);
    // Tree is appended by the public compositor. Its unique neutral panel
    // separates it from source/SPACE/footer ink with shared semantic colors.
    let start = ui
        .iter()
        .position(|v| v.color == TREE_PANEL)
        .expect("tree panel");
    ui[start..].to_vec()
}
fn fixture() -> World {
    let snapshot = Snapshot {
        folder: "/Fixture/MiXeD-É\nSourceTail".into(),
        entries: Vec::new(),
    };
    let (mut world, _) = folder_scene::build(&snapshot, 0, false).unwrap();
    let mut rows: Vec<_> = (0..17)
        .map(|i| Row {
            id: format!("row-{i:02}"),
            label: "H".into(),
            depth: i % 3,
        })
        .collect();
    rows.push(Row {
        id: world.current.clone(),
        label: "H".into(),
        depth: 0,
    });
    world.navigation = Navigation {
        title: "H".into(),
        rows,
    };
    world
}
fn check_h(ink: &[Vertex], width: u32, height: u32, role: &str, failures: &mut Vec<String>) {
    // H has 17 lit cells, each a six-vertex quad. A same-length H -> space
    // control isolates ONE glyph, not a whole line/panel or the row pitch.
    assert_eq!(ink.len(), 17 * 6, "single H not isolated: {role}");
    let b = bounds(ink, width, height);
    let actual = b.bottom - b.top;
    println!("TREE_GLYPH {width}x{height} {role} inkHeightPx={actual}");
    if actual + EPS < 14.0 {
        failures.push(format!("{width}x{height} {role}: {actual}px"));
    }
}
fn ink_bands(ink: &[Vertex], width: u32, height: u32) -> Vec<Bounds> {
    assert_eq!(ink.len() % 6, 0);
    let mut cells: Vec<_> = ink
        .as_chunks::<6>()
        .0
        .iter()
        .map(|q| bounds(q.as_slice(), width, height))
        .collect();
    cells.sort_by(|a, b| a.top.total_cmp(&b.top));
    let mut bands: Vec<Bounds> = Vec::new();
    for b in cells {
        if let Some(last) = bands.last_mut()
            && b.top <= last.bottom + EPS
        {
            last.left = last.left.min(b.left);
            last.right = last.right.max(b.right);
            last.bottom = last.bottom.max(b.bottom);
            continue;
        }
        bands.push(b);
    }
    bands
}

#[test]
fn tree_header_and_visible_rows_have_14px_single_glyph_ink_and_keep_current_source() {
    let mut failures = Vec::new();
    for (width, height) in [(960, 640), (640, 480)] {
        for collapsed in [false, true] {
            let mut world = fixture();
            world.tree_collapsed = collapsed;
            let source = world.current.clone();
            let navigation = world.navigation.clone();
            let view = world.folder_view.clone();
            let document = world.document().unwrap().to_string();
            let current = navigation.current_index(&source).unwrap();
            assert_eq!(current, navigation.rows.len() - 1);
            let ui = tree(&world, width, height);
            let panel = bounds(&colored(&ui, TREE_PANEL), width, height);
            let all_ink: Vec<_> = ui
                .iter()
                .filter(|v| v.color != TREE_PANEL)
                .copied()
                .collect();
            assert!(
                panel.contains(bounds(&all_ink, width, height)),
                "tree text outside panel"
            );

            world.navigation.title = " ".into();
            let control = tree(&world, width, height);
            world.navigation.title = navigation.title.clone();
            check_h(
                &difference(&ui, &control, HEADER),
                width,
                height,
                &format!("header collapsed={collapsed}"),
                &mut failures,
            );
            let mut visible = 0;
            let mut current_visible = false;
            for i in 0..navigation.rows.len() {
                world.navigation.rows[i].label = " ".into();
                let control = tree(&world, width, height);
                world.navigation.rows[i].label = navigation.rows[i].label.clone();
                let color = if i == current { CURRENT } else { ORDINARY };
                let ink = difference(&ui, &control, color);
                if ink.is_empty() {
                    continue;
                }
                assert!(!collapsed, "collapsed tree leaked rows");
                visible += 1;
                current_visible |= i == current;
                check_h(&ink, width, height, &format!("row={i}"), &mut failures);
            }
            if collapsed {
                assert_eq!(visible, 0);
                assert!(colored(&ui, CURRENT).is_empty());
                assert!(colored(&ui, ORDINARY).is_empty());
            } else {
                assert!(
                    current_visible,
                    "current row H missing from highlighted ink"
                );
                assert!(
                    visible >= 2 && visible < navigation.rows.len(),
                    "bounded current-centred window"
                );
                assert!(!colored(&ui, ORDINARY).is_empty());
            }
            // Each H spans every bitmap row, so merging touching cell ranges
            // gives one band per line; collisions merge bands and fail the count.
            let bands = ink_bands(&all_ink, width, height);
            assert_eq!(bands.len(), visible + 1, "tree header/rows overlap");
            for pair in bands.windows(2) {
                assert!(
                    pair[1].top - pair[0].bottom > EPS,
                    "no positive row separation"
                );
            }
            assert_eq!(world.current, source);
            assert_eq!(world.graph.start, source);
            assert_eq!(world.navigation, navigation);
            assert_eq!(world.folder_view, view);
            assert_eq!(world.document().unwrap(), document);
            assert_eq!(world.tree_collapsed, collapsed);
        }
    }
    assert!(
        failures.is_empty(),
        "tree text below TwoPixelBitmapGlyph14: {failures:?}"
    );
}

fn normalized_ink(ink: &[Vertex], width: u32, height: u32) -> Vec<(i32, i32)> {
    let b = bounds(ink, width, height);
    ink.iter()
        .map(|v| {
            let x = (v.position[0] + 1.0) * 0.5 * width as f32;
            let y = (1.0 - v.position[1]) * 0.5 * height as f32;
            (
                ((x - b.left) * 100.0).round() as i32,
                ((y - b.top) * 100.0).round() as i32,
            )
        })
        .collect()
}

#[test]
fn bottom_two_long_status_lines_have_positive_separation_bounds_and_explicit_marker() {
    let message = format!(
        "{}First-MiXeD-Tail\n{}Second-Different-Tail",
        "H".repeat(400),
        "H".repeat(400)
    );
    let saved = message.clone();
    for (width, height) in [(960, 640), (640, 480)] {
        let ui = text::pack_overlay(&message, width, height);
        let panels: Vec<_> = ui
            .as_chunks::<6>()
            .0
            .iter()
            .filter(|q| q[0].color == [0.025; 3])
            .map(|q| bounds(q.as_slice(), width, height))
            .collect();
        assert_eq!(panels.len(), 2, "two status panels required");
        assert!(panels[1].top - panels[0].bottom > EPS);
        let ink = colored(&ui, ORDINARY);
        let bands = ink_bands(&ink, width, height);
        assert_eq!(bands.len(), 2, "two separate status ink lines required");
        assert!(bands[1].top - bands[0].bottom > EPS);
        let reference = colored(&text::pack_overlay("~", width, height), ORDINARY);
        let marker_width =
            bounds(&reference, width, height).right - bounds(&reference, width, height).left;
        for (i, band) in bands.iter().enumerate() {
            assert!(panels[i].contains(*band), "status ink outside panel");
            let line: Vec<_> = ink
                .as_chunks::<6>()
                .0
                .iter()
                .filter(|q| {
                    let b = bounds(q.as_slice(), width, height);
                    b.top + EPS >= band.top && b.bottom <= band.bottom + EPS
                })
                .flatten()
                .copied()
                .collect();
            // The existing ~ bitmap spans five columns. Select the final glyph
            // using its measured public reference width, not an old column cap.
            let marker: Vec<_> = line
                .as_chunks::<6>()
                .0
                .iter()
                .filter(|q| {
                    bounds(q.as_slice(), width, height).left + EPS >= band.right - marker_width
                })
                .flatten()
                .copied()
                .collect();
            assert_eq!(
                normalized_ink(&marker, width, height),
                normalized_ink(&reference, width, height),
                "long status line {i} lacks a complete explicit ~ truncation marker"
            );
        }
        println!("STATUS_LAYOUT {width}x{height} panels={panels:?} ink={bands:?}");
    }
    assert_eq!(message, saved);
}
