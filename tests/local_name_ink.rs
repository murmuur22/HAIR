//! Reserved-sidebar successor of the old six-row overlay oracle. The original
//! executed failures and tests are retained in the q2 frozen qualification clone.
//! Public bitmap geometry only, not framebuffer/native acceptance.
use hair::{
    assets::Vertex,
    local_explorer::{Entry, Kind, Snapshot},
    sidebar::Frame,
    text,
};
use std::collections::BTreeMap;
const FILE: [f32; 3] = [0.65, 0.81, 0.69];
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
fn difference(a: &[Vertex], b: &[Vertex], color: [f32; 3]) -> Vec<Vertex> {
    let mut counts = BTreeMap::new();
    for v in b.iter().filter(|v| v.color == color) {
        *counts.entry(key(v)).or_insert(0usize) += 1;
    }
    a.iter()
        .filter(|v| v.color == color)
        .filter_map(|v| {
            if let Some(n) = counts.get_mut(&key(v))
                && *n > 0
            {
                *n -= 1;
                None
            } else {
                Some(*v)
            }
        })
        .collect()
}
fn normalized(ink: &[Vertex], size: [u32; 2]) -> Vec<[i32; 2]> {
    assert!(!ink.is_empty());
    let points: Vec<_> = ink
        .iter()
        .map(|v| {
            [
                (v.position[0] + 1.0) * 0.5 * size[0] as f32,
                (1.0 - v.position[1]) * 0.5 * size[1] as f32,
            ]
        })
        .collect();
    let x = points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let y = points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    points
        .iter()
        .map(|p| {
            [
                ((p[0] - x) * 100.0).round() as i32,
                ((p[1] - y) * 100.0).round() as i32,
            ]
        })
        .collect()
}
fn snapshot(ancestor: &str) -> Snapshot {
    let folder = std::path::PathBuf::from(format!("/{ancestor}/MiXeD-FolderTail"));
    let entries = [
        "Child024".to_owned(),
        "Child025".to_owned(),
        format!("{}ending024.txt", "Shared production notes ".repeat(5)),
        format!("{}ending025.txt", "Shared production notes ".repeat(5)),
    ]
    .into_iter()
    .map(|name| Entry {
        path: folder.join(&name),
        name: name.into(),
        kind: Kind::File,
    })
    .collect();
    Snapshot { folder, entries }
}
#[test]
fn local_names_keep_literal_distinguishing_tails_and_full_source_in_reserved_sidebar() {
    for size in [[960, 640], [640, 480]] {
        let frame = Frame::new(size, 100, true);
        let a = snapshot("S");
        let mut b = snapshot(&"different-raw-source-ancestor/".repeat(18));
        let saved = a.clone();
        let ui = text::sidebar_overlay(&a, frame, 0, None);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&ui),
            bytemuck::cast_slice::<_, u8>(&text::sidebar_overlay(&b, frame, 0, None)),
            "ancestors cannot steal directory/entry name columns"
        );
        let mut names = vec![];
        for i in 0..a.entries.len() {
            let ink: Vec<_> = ui
                .iter()
                .filter(|v| {
                    v.color == FILE && {
                        let y = (1.0 - v.position[1]) * 0.5 * size[1] as f32;
                        y >= 105.99 + i as f32 * 26.0 && y <= 120.01 + i as f32 * 26.0
                    }
                })
                .copied()
                .collect();
            let full = hair::local_explorer::escape_display(&a.entries[i].name);
            let columns = ((frame.sidebar - 28.0) / 12.0) as usize;
            let label = format!("  {full}");
            let visible = if label.chars().count() > columns {
                format!(
                    "~{}",
                    label
                        .chars()
                        .skip(label.chars().count() - (columns - 1))
                        .collect::<String>()
                )
            } else {
                label
            };
            let reference: Vec<_> = text::pack_overlay(&visible, size[0], size[1])
                .into_iter()
                .filter(|v| v.color == [0.85; 3])
                .collect();
            assert_eq!(
                normalized(&ink, size),
                normalized(&reference, size),
                "complete two-pixel glyphs and distinguishing literal tails"
            );
            names.push(normalized(&ink, size));
        }
        assert_ne!(names[0], names[1]);
        assert_ne!(names[2], names[3]);
        b.entries[0].name = "H".into();
        let actual = text::sidebar_overlay(&b, frame, 0, None);
        b.entries[0].name = " ".into();
        let ink = difference(&actual, &text::sidebar_overlay(&b, frame, 0, None), FILE);
        assert_eq!(ink.len(), 17 * 6, "single-H multiplicity control");
        let reference: Vec<_> = text::pack_overlay("H", size[0], size[1])
            .into_iter()
            .filter(|v| v.color == [0.85; 3])
            .collect();
        assert_eq!(normalized(&ink, size), normalized(&reference, size));
        let (world, _) = hair::folder_scene::build(&a, 0, false).unwrap();
        assert!(
            world
                .document()
                .unwrap()
                .contains(&a.folder.to_string_lossy().to_string()),
            "full source remains available through metadata"
        );
        assert!(text::sidebar_overlay(&a, Frame::new(size, 100, false), 0, None).is_empty());
        assert_eq!(a, saved);
    }
}
