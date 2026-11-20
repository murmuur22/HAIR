//! File-only tracer: parent must compile and observe behavioral RED before implementation.
//! Row/data priority is the oracle; overlay calls do not certify rendered suffix ink.
use hair::{
    folder_scene::{self, PAGE_SIZE},
    local_explorer::{Entry, Kind, Snapshot},
    text,
};
use std::path::PathBuf;

#[test]
fn local_outline_prioritizes_current_basename_and_visible_mixed_entry_names() {
    let folder = PathBuf::from(
        "/Users/Robbie/Projects/Archive/Collections/Production/Location/Year/Season Folder CURRENT",
    );
    let mut entries: Vec<_> = (1..=20)
        .map(|i| {
            let name = format!("Child{i:03}");
            Entry {
                path: folder.join(&name),
                name: name.into(),
                kind: Kind::Folder,
            }
        })
        .collect();
    // Both distinguishing pairs are together on page 1 (zero-based), not hidden
    // on different continuation pages. The 25th entry also exercises page 2.
    for (name, kind) in [
        ("Child024", Kind::Folder),
        ("Child025", Kind::Folder),
        (
            "Shared production notes with a long common prefix - ending024.txt",
            Kind::File,
        ),
        (
            "Shared production notes with a long common prefix - ending025.txt",
            Kind::File,
        ),
        ("Last visible page.txt", Kind::File),
    ] {
        entries.push(Entry {
            path: folder.join(name),
            name: name.into(),
            kind,
        });
    }
    let snapshots = [
        Snapshot { folder, entries },
        Snapshot {
            folder: "/".into(),
            entries: Vec::new(),
        },
    ];
    for snapshot in &snapshots {
        let source = folder_scene::display(snapshot.folder.as_os_str());
        let basename = snapshot
            .folder
            .file_name()
            .unwrap_or_else(|| snapshot.folder.as_os_str());
        let expected_label = folder_scene::display(basename);
        for page in 0..snapshot.entries.len().div_ceil(PAGE_SIZE).max(1) {
            for collapsed in [false, true] {
                let (world, _) = folder_scene::build(snapshot, page, collapsed)
                    .expect("valid synthetic local folder page");
                let view = world.folder_view.as_ref().expect("local folder view");
                assert_eq!(
                    world.current, source,
                    "do not shorten the current source ID"
                );
                assert_eq!(world.graph.start, source);
                assert_eq!(view.source, source);
                assert!(world.document().unwrap().contains(&source));
                assert_eq!(world.tree_collapsed, collapsed);
                assert_eq!(view.page, page);
                let current = world
                    .navigation
                    .current_index(&source)
                    .expect("full source ID must still identify the highlighted current row");
                let rows = &world.navigation.rows;
                // Intended first feature RED against the pre-slice3 source.
                assert_eq!(
                    rows[current].label, expected_label,
                    "local current row must use basename, not repeated full ancestry"
                );
                assert!(
                    current <= 1,
                    "only an optional compact Parent may precede current"
                );
                assert_eq!(rows.iter().filter(|row| row.id == source).count(), 1);
                let visible: Vec<_> = snapshot
                    .entries
                    .iter()
                    .skip(page * PAGE_SIZE)
                    .take(PAGE_SIZE)
                    .collect();
                assert_eq!(view.bindings.len(), visible.len());
                assert!(
                    rows.len() > visible.len() && rows.len() <= visible.len() + 2,
                    "outline budget is current + visible contents + at most one Parent"
                );
                for bound in view.bindings.values() {
                    assert_eq!(bound.entry, snapshot.entries[bound.index]);
                    assert!(visible.contains(&&bound.entry));
                }
                let local_ids: Vec<_> = visible
                    .iter()
                    .map(|entry| folder_scene::display(entry.path.as_os_str()))
                    .collect();
                for (entry, id) in visible.iter().zip(&local_ids) {
                    let matching: Vec<_> = rows.iter().filter(|row| &row.id == id).collect();
                    assert_eq!(matching.len(), 1, "one local row for file OR folder: {id}");
                    assert_eq!(matching[0].label, folder_scene::display(&entry.name));
                }
                for row in rows {
                    if row.id != source && !local_ids.contains(&row.id) {
                        let parent = snapshot.folder.parent().expect("root has no Parent row");
                        assert_eq!(row.id, folder_scene::display(parent.as_os_str()));
                        if parent.file_name().is_some() {
                            assert_ne!(row.label, row.id, "Parent must not repeat full ancestry");
                        }
                    }
                }
                // Exercise the real compositor at both supported sizes without
                // pretending nonempty vertices prove which suffix glyphs survive.
                let navigation = world.navigation.clone();
                let saved_view = world.folder_view.clone();
                let document = world.document().unwrap().to_string();
                for (width, height) in [(960, 640), (640, 480)] {
                    assert!(!text::overlay(&world, width, height, 0).is_empty());
                    assert_eq!(world.current, source);
                    assert_eq!(world.graph.start, source);
                    assert_eq!(world.navigation, navigation);
                    assert_eq!(world.folder_view, saved_view);
                    assert_eq!(world.document().unwrap(), document);
                    assert_eq!(world.tree_collapsed, collapsed);
                }
            }
        }
    }
}
