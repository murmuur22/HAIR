//! Reserved-sidebar successor. Original six-row/current-highlight oracle and
//! its executed failures remain frozen in q2. Status-line controls are retained.
use hair::{
    assets::Vertex,
    local_explorer::{Entry, Kind, Snapshot},
    sidebar::{Frame, List},
    text,
};
const EPS: f32 = 0.01;
const ORDINARY: [f32; 3] = [0.85; 3];
#[derive(Clone, Copy, Debug)]
struct Bounds {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}
impl Bounds {
    fn contains(self, b: Self) -> bool {
        b.left + EPS >= self.left
            && b.top + EPS >= self.top
            && b.right <= self.right + EPS
            && b.bottom <= self.bottom + EPS
    }
}
fn bounds(v: &[Vertex], w: u32, h: u32) -> Bounds {
    assert!(!v.is_empty());
    let mut b = Bounds {
        left: f32::INFINITY,
        top: f32::INFINITY,
        right: f32::NEG_INFINITY,
        bottom: f32::NEG_INFINITY,
    };
    for v in v {
        assert!(v.position.iter().all(|v| v.is_finite()));
        let x = (v.position[0] + 1.0) * 0.5 * w as f32;
        let y = (1.0 - v.position[1]) * 0.5 * h as f32;
        b.left = b.left.min(x);
        b.right = b.right.max(x);
        b.top = b.top.min(y);
        b.bottom = b.bottom.max(y);
    }
    assert!(
        b.left >= -EPS && b.top >= -EPS && b.right <= w as f32 + EPS && b.bottom <= h as f32 + EPS
    );
    assert!(b.right > b.left && b.bottom > b.top);
    b
}
fn colored(v: &[Vertex], c: [f32; 3]) -> Vec<Vertex> {
    v.iter().filter(|v| v.color == c).copied().collect()
}
fn ink_bands(ink: &[Vertex], w: u32, h: u32) -> Vec<Bounds> {
    assert_eq!(ink.len() % 6, 0);
    let mut cells: Vec<_> = ink
        .as_chunks::<6>()
        .0
        .iter()
        .map(|q| bounds(q.as_slice(), w, h))
        .collect();
    cells.sort_by(|a, b| a.top.total_cmp(&b.top));
    let mut bands: Vec<Bounds> = vec![];
    for b in cells {
        if let Some(last) = bands.last_mut()
            && b.top <= last.bottom + EPS
        {
            last.left = last.left.min(b.left);
            last.right = last.right.max(b.right);
            last.bottom = last.bottom.max(b.bottom);
        } else {
            bands.push(b);
        }
    }
    bands
}
fn normalized_ink(ink: &[Vertex], w: u32, h: u32) -> Vec<(i32, i32)> {
    let b = bounds(ink, w, h);
    ink.iter()
        .map(|v| {
            (
                (((v.position[0] + 1.0) * 0.5 * w as f32 - b.left) * 100.0).round() as i32,
                (((1.0 - v.position[1]) * 0.5 * h as f32 - b.top) * 100.0).round() as i32,
            )
        })
        .collect()
}
#[test]
fn sidebar_all_scroll_rows_have_14px_ink_inside_the_reserved_strip_and_off_is_empty() {
    let folder = std::path::PathBuf::from("/Fixture/H");
    let snapshot = Snapshot {
        entries: (0..37)
            .map(|i| Entry {
                name: "H".into(),
                path: folder.join(format!("H{i}")),
                kind: Kind::File,
            })
            .collect(),
        folder,
    };
    let saved = snapshot.clone();
    for (w, h) in [(960, 640), (640, 480)] {
        let frame = Frame::new([w, h], 100, true);
        let strip = Bounds {
            left: 0.0,
            top: 0.0,
            right: frame.sidebar,
            bottom: h as f32,
        };
        let mut observed = std::collections::BTreeSet::new();
        for offset in 0..37 {
            let list = List::new(frame, 37, offset);
            let ui = text::sidebar_overlay(&snapshot, frame, offset, None);
            assert!(strip.contains(bounds(&ui, w, h)));
            let bands = ink_bands(&colored(&ui, [0.65, 0.81, 0.69]), w, h);
            assert_eq!(bands.len(), list.rows.len());
            for (band, (i, row)) in bands.iter().zip(&list.rows) {
                assert!(
                    (band.bottom - band.top - 14.0).abs() < EPS,
                    "two-pixel bitmap glyph height"
                );
                assert!(band.top >= row[1] && band.bottom <= row[3]);
                observed.insert(*i);
            }
            let header = bounds(&colored(&ui, [0.84, 0.96, 0.86]), w, h);
            assert!((header.bottom - header.top - 14.0).abs() < EPS);
        }
        assert_eq!(
            observed.len(),
            37,
            "all admitted immediate entries, including off-page files"
        );
        assert!(
            text::sidebar_overlay(&snapshot, Frame::new([w, h], 100, false), 0, None).is_empty()
        );
        let (world, _) = hair::folder_scene::build(&snapshot, 0, false).unwrap();
        assert!(
            !text::overlay(&world, w, h, 0)
                .iter()
                .any(|v| v.color == [0.012; 3]),
            "old local tree removed rather than doubled"
        );
    }
    assert_eq!(snapshot, saved);
}
#[test]
fn bottom_two_long_status_lines_have_positive_separation_bounds_and_explicit_marker() {
    let message = format!(
        "{}First-MiXeD-Tail\n{}Second-Different-Tail",
        "H".repeat(400),
        "H".repeat(400)
    );
    let saved = message.clone();
    for (w, h) in [(960, 640), (640, 480)] {
        let ui = text::pack_overlay(&message, w, h);
        let panels: Vec<_> = ui
            .as_chunks::<6>()
            .0
            .iter()
            .filter(|q| q[0].color == [0.025; 3])
            .map(|q| bounds(q.as_slice(), w, h))
            .collect();
        assert_eq!(panels.len(), 2);
        assert!(panels[1].top - panels[0].bottom > EPS);
        let ink = colored(&ui, ORDINARY);
        let bands = ink_bands(&ink, w, h);
        assert_eq!(bands.len(), 2);
        assert!(bands[1].top - bands[0].bottom > EPS);
        let reference = colored(&text::pack_overlay("~", w, h), ORDINARY);
        let marker_width = bounds(&reference, w, h).right - bounds(&reference, w, h).left;
        for (i, band) in bands.iter().enumerate() {
            assert!(panels[i].contains(*band));
            let line: Vec<_> = ink
                .as_chunks::<6>()
                .0
                .iter()
                .filter(|q| {
                    let b = bounds(q.as_slice(), w, h);
                    b.top + EPS >= band.top && b.bottom <= band.bottom + EPS
                })
                .flatten()
                .copied()
                .collect();
            let marker: Vec<_> = line
                .as_chunks::<6>()
                .0
                .iter()
                .filter(|q| bounds(q.as_slice(), w, h).left + EPS >= band.right - marker_width)
                .flatten()
                .copied()
                .collect();
            assert_eq!(
                normalized_ink(&marker, w, h),
                normalized_ink(&reference, w, h),
                "complete explicit ~ truncation marker"
            );
        }
    }
    assert_eq!(message, saved);
}
