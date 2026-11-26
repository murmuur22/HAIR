use super::*;
use hair::placement_store::Position;
use std::ffi::OsStr;
use std::os::unix::fs::MetadataExt;

#[path = "slice3_placement_tests.rs"]
mod slice3;

fn binding(a: &App, name: &str) -> String {
    a.world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == OsStr::new(name))
        .unwrap()
        .0
        .clone()
}
fn mesh_bytes(mesh: &[hair::assets::Vertex]) -> Vec<u8> {
    bytemuck::cast_slice(mesh).to_vec()
}
fn item(a: &App, name: &str) -> Vec<u8> {
    mesh_bytes(&a.themes[0].file_item_shape(&binding(a, name), OsStr::new(name)))
}
fn base(a: &App, name: &str) -> Vec<u8> {
    mesh_bytes(&a.themes[0].collection_shape(&binding(a, name)))
}
fn identity(path: &Path) -> (u64, u64) {
    let m = std::fs::metadata(path).unwrap();
    (m.dev(), m.ino())
}
fn collect(a: &mut App, source: &Path, name: &str) {
    let e = hair::local_explorer::Explorer::new(source).unwrap();
    let index = e
        .current()
        .entries
        .iter()
        .position(|e| e.name == OsStr::new(name))
        .unwrap();
    a.inventory.collect(&e, index).unwrap();
    a.inventory_selection = a.inventory.items().len() - 1;
}
fn place(a: &mut App, at: [f32; 2], allow: bool) -> Position {
    a.pose = Pose {
        position: [at[0], at[1] + 1.65],
        pitch: -std::f32::consts::FRAC_PI_4,
        ..Default::default()
    };
    tap(a, KeyCode::KeyB);
    tap(a, KeyCode::KeyP);
    assert_eq!(a.editor_phase(), "Editing");
    if allow {
        tap(a, KeyCode::KeyC);
    }
    tap(a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Positioning");
    assert!(a.placement_action.as_ref().unwrap().pending.is_none());
    // Each tap includes key-up: Editing, Review and publication are distinct
    // fresh input boundaries, not a helper that forces an internal phase.
    tap(a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Review");
    let saved = a
        .placement_action
        .as_ref()
        .and_then(|a| a.pending.clone())
        .unwrap_or_else(|| panic!("review refused: {}", a.local_status));
    tap(a, KeyCode::Enter);
    assert!(
        a.placement_action.is_none(),
        "commit refused: {}",
        a.local_status
    );
    assert!(a.local_status.starts_with("Placed "), "{}", a.local_status);
    saved
}

#[test]
fn slice1_app_publication_pins_unrelated_file_and_furniture_through_fresh_reopen() {
    let source = fixture("slice1-source");
    let dest = fixture("slice1-nonempty");
    std::fs::write(source.0.join("A-new.txt"), b"new immutable source").unwrap();
    std::fs::write(dest.0.join("Z-existing.txt"), b"existing immutable source").unwrap();
    std::fs::create_dir(dest.0.join("Child")).unwrap();
    std::fs::write(dest.0.join("Child/inside.txt"), b"nonempty child").unwrap();
    let original_identity = identity(&dest.0.join("Z-existing.txt"));
    let mut a = app();
    collect(&mut a, &source.0, "A-new.txt");
    a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    let before_item = item(&a, "Z-existing.txt");
    let before_base = base(&a, "Z-existing.txt");
    assert!(!before_base.is_empty());
    let saved = place(&mut a, [2., 0.], false);
    let check = |a: &App| {
        assert_eq!(
            item(a, "Z-existing.txt"),
            before_item,
            "unrelated generated file moved after listing insertion"
        );
        assert_eq!(
            base(a, "Z-existing.txt"),
            before_base,
            "unrelated furniture moved after listing insertion"
        );
        assert_eq!(identity(&dest.0.join("Z-existing.txt")), original_identity);
        let id = binding(a, "A-new.txt");
        assert_eq!(a.themes[0].host.file_placements[&id].position, saved);
        a.themes[0].host.check_routes(&[a.pose.position]).unwrap();
        let host = &a.themes[0].host;
        let existing = binding(a, "Z-existing.txt");
        let anchor = host
            .anchors
            .iter()
            .find(|anchor| anchor.id == existing)
            .unwrap();
        let pose = Pose {
            position: [anchor.at[0], anchor.at[2] + 1.1],
            pitch: ((anchor.at[1] + (anchor.size[1] + 0.97) * 0.5 - 1.65) / 1.1).atan(),
            ..Default::default()
        };
        assert_eq!(
            hair::interaction::resolve(host, &pose, &a.world)
                .unwrap()
                .anchor,
            existing,
            "inactive saved-file placeholder must not occlude the unrelated file"
        );
    };
    check(&a);
    tap(&mut a, KeyCode::F5);
    check(&a);
    let mut reopened = app();
    reopened.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    check(&reopened);
    assert_eq!(
        std::fs::read(dest.0.join("Z-existing.txt")).unwrap(),
        b"existing immutable source"
    );
    assert_eq!(
        std::fs::read(source.0.join("A-new.txt")).unwrap(),
        b"new immutable source"
    );
}

#[test]
fn slice1_app_surface_only_publication_does_not_allocate_a_detached_generated_base() {
    let source = fixture("slice1-base-source");
    let dest = fixture("slice1-base-target");
    std::fs::write(source.0.join("Placed.txt"), b"placed source").unwrap();
    std::fs::write(dest.0.join("Existing.txt"), b"untouched source").unwrap();
    let mut a = app();
    collect(&mut a, &source.0, "Placed.txt");
    a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    place(&mut a, [2., 0.], false);
    let check = |a: &App, name: &str| {
        assert!(!item(a, name).is_empty(), "file route and mesh retained");
        assert!(
            base(a, name).is_empty(),
            "file-only surface record must not manufacture unrelated furniture in a listing slot"
        );
        assert!(
            !base(a, "Existing.txt").is_empty(),
            "unrelated generated base retained"
        );
    };
    check(&a, "Placed.txt");
    let inode = identity(&dest.0.join("Placed.txt"));
    std::fs::rename(dest.0.join("Placed.txt"), dest.0.join("A-renamed.txt")).unwrap();
    tap(&mut a, KeyCode::F5);
    check(&a, "A-renamed.txt");
    assert_eq!(identity(&dest.0.join("A-renamed.txt")), inode);
    let mut reopened = app();
    reopened.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    check(&reopened, "A-renamed.txt");
}

#[test]
fn slice1_app_two_overlapping_publications_preserve_first_saved_identity_and_transform() {
    let source = fixture("slice1-overlap-source");
    let dest = fixture("slice1-overlap-target");
    for name in ["Z-first.txt", "A-second.txt"] {
        std::fs::write(source.0.join(name), name.as_bytes()).unwrap();
    }
    std::fs::write(dest.0.join("Existing.txt"), b"unrelated").unwrap();
    let mut a = app();
    collect(&mut a, &source.0, "Z-first.txt");
    a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    let first = place(&mut a, [2., 0.], false);
    let first_mesh = item(&a, "Z-first.txt");
    let inode = identity(&dest.0.join("Z-first.txt"));
    let unrelated = item(&a, "Existing.txt");
    let furniture = base(&a, "Existing.txt");
    collect(&mut a, &source.0, "A-second.txt");
    // Aim at clear floor just beyond the first item's depth, but intersect its width.
    let second = place(&mut a, [2.2, 0.2], true);
    let b1 = first.bounds().unwrap();
    let b2 = second.bounds().unwrap();
    assert!((b1.at[0] - b2.at[0]).abs() < (b1.size[0] + b2.size[0]) * 0.5);
    assert!((b1.at[2] - b2.at[2]).abs() < (b1.size[2] + b2.size[2]) * 0.5);
    let check = |a: &App, name: &str| {
        let id = binding(a, name);
        assert_eq!(
            a.themes[0].host.file_placements[&id].position, first,
            "existing saved placement must remain admitted"
        );
        assert_eq!(item(a, name), first_mesh);
        assert_eq!(identity(&dest.0.join(name)), inode);
        assert_eq!(item(a, "Existing.txt"), unrelated);
        assert_eq!(base(a, "Existing.txt"), furniture);
        assert!(base(a, name).is_empty());
        assert!(base(a, "A-second.txt").is_empty());
    };
    check(&a, "Z-first.txt");
    tap(&mut a, KeyCode::F5);
    check(&a, "Z-first.txt");
    std::fs::rename(dest.0.join("Z-first.txt"), dest.0.join("0-renamed.txt")).unwrap();
    std::fs::write(
        dest.0.join("00-inserted.txt"),
        b"external listing insertion",
    )
    .unwrap();
    tap(&mut a, KeyCode::F5);
    // Generated unsaved files may reflow on an external ordinary-file insertion;
    // persisted identity/transform must not, including across transaction read order.
    let id = binding(&a, "0-renamed.txt");
    assert_eq!(a.themes[0].host.file_placements[&id].position, first);
    assert_eq!(item(&a, "0-renamed.txt"), first_mesh);
    let mut reopened = app();
    reopened.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    let id = binding(&reopened, "0-renamed.txt");
    assert_eq!(reopened.themes[0].host.file_placements[&id].position, first);
    assert_eq!(item(&reopened, "0-renamed.txt"), first_mesh);
    assert_eq!(identity(&dest.0.join("0-renamed.txt")), inode);
}

#[test]
fn slice1_overlap_reconciliation_is_independent_of_record_enumeration_order() {
    let folder = PathBuf::from("/slice1-read-only");
    let snapshot = hair::local_explorer::Snapshot {
        folder: folder.clone(),
        entries: ["First.txt", "Second.txt"]
            .map(|name| hair::local_explorer::Entry {
                name: name.into(),
                path: folder.join(name),
                kind: hair::local_explorer::Kind::File,
            })
            .to_vec(),
    };
    let first = Position {
        properties: Default::default(),
        at: [2., 0.],
        yaw: 0.,
        size: [0.8, 0.85, 0.28],
        allow_overlap: false,
        surface: None,
    };
    let second = Position {
        at: [2.2, 0.],
        allow_overlap: true,
        ..first.clone()
    };
    for reverse in [false, true] {
        let (world, mut host) = hair::folder_scene::build(&snapshot, 0, true).unwrap();
        let mut positions = vec![
            ("First.txt".into(), first.clone()),
            ("Second.txt".into(), second.clone()),
        ];
        if reverse {
            positions.reverse();
        }
        let notes = hair::placement_scene::apply(
            &hair::placement_store::LayoutRead {
                positions,
                ..Default::default()
            },
            &world,
            &mut host,
        )
        .unwrap();
        assert!(notes.is_empty(), "order={reverse}: {notes:?}");
        assert_eq!(host.file_placements["entry-0"].position, first);
        assert_eq!(host.file_placements["entry-1"].position, second);
    }
}

#[test]
fn slice1_app_publication_preserves_generated_transform_across_page_boundary() {
    let source = fixture("slice1-page-source");
    let dest = fixture("slice1-page-target");
    std::fs::write(source.0.join("A-new.txt"), b"new source").unwrap();
    for index in 0..hair::folder_scene::PAGE_SIZE {
        std::fs::write(dest.0.join(format!("F-{index:02}.txt")), b"existing file").unwrap();
    }
    let name = "F-11.txt";
    let inode = identity(&dest.0.join(name));
    let mut a = app();
    collect(&mut a, &source.0, "A-new.txt");
    a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    let before_item = item(&a, name);
    let before_base = base(&a, name);
    place(&mut a, [7., 0.], false);
    a.placement_refresh(1).unwrap();
    assert_eq!(
        item(&a, name),
        before_item,
        "page migration must not reassign the generated transform"
    );
    assert_eq!(base(&a, name), before_base);
    assert_eq!(identity(&dest.0.join(name)), inode);
    let mut reopened = app();
    reopened.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    reopened.placement_refresh(1).unwrap();
    assert_eq!(item(&reopened, name), before_item);
    assert_eq!(base(&reopened, name), before_base);
}

#[test]
fn slice1_app_existing_allow_cannot_opt_a_new_prevent_action_into_overlap() {
    let source = fixture("slice1-prevent-source");
    let dest = fixture("slice1-prevent-target");
    for name in ["First.txt", "Second.txt"] {
        std::fs::write(source.0.join(name), name.as_bytes()).unwrap();
    }
    let mut a = app();
    collect(&mut a, &source.0, "First.txt");
    a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    let first = place(&mut a, [2., 0.], true);
    collect(&mut a, &source.0, "Second.txt");
    a.pose = Pose {
        position: [2.2, 1.85],
        pitch: -std::f32::consts::FRAC_PI_4,
        ..Default::default()
    };
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Positioning");
    tap(&mut a, KeyCode::Enter);
    assert!(a.placement_action.as_ref().unwrap().pending.is_none());
    assert!(
        a.local_status.contains("overlaps a saved file"),
        "{}",
        a.local_status
    );
    assert!(!dest.0.join("Second.txt").exists());
    assert_eq!(a.inventory.items().len(), 2);
    let layout = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
    assert_eq!(layout.positions, vec![("First.txt".into(), first.clone())]);
    tap(&mut a, KeyCode::Escape);
    assert_eq!(a.editor_phase(), "Editing");
    tap(&mut a, KeyCode::Escape);
    assert!(a.placement_action.is_none() && a.inventory_open);
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    assert!(a.place_mode.is_none());
    place(&mut a, [2.2, 0.2], true);
    assert_eq!(
        a.themes[0].host.file_placements[&binding(&a, "First.txt")].position,
        first
    );
}
