use super::*;
use winit::keyboard::KeyCode;
#[path = "stable_placement_tests.rs"]
mod stable_placement_tests;
#[path = "place_mode_app_tests.rs"]
mod place_mode_app_tests;
fn app() -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut a = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    a.focused = true;
    a.complete_folder(Ok(Some(root.join("examples")))).unwrap();
    a
}
fn tap(a: &mut App, k: KeyCode) {
    a.key(k, true, false).unwrap();
    a.key(k, false, false).unwrap();
}
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn fixture(label: &str) -> Fixture {
    let p = std::env::temp_dir().join(format!(
        "dm-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&p).unwrap();
    Fixture(p)
}
#[test]
fn sandbox_floating_image_aim_collects_without_moving_its_base() {
    let f = fixture("floating-image-aim");
    std::fs::write(
        f.0.join("Swatch.png"),
        include_bytes!("../examples/site-a/swatch.png"),
    )
    .unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !a.object_previews.cache.contains_key("entry-0") {
        a.poll_local_image().unwrap();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let host = a.themes[0].host.clone();
    let base = host.anchors.iter().find(|an| an.id == "entry-0").unwrap();
    let t = &a.object_previews.cache["entry-0"];
    let scale = (0.8 / t.width as f32).min(0.85 / t.height as f32);
    let centre = base.at[1] + base.size[1] + 0.12 + t.height as f32 * scale * 0.5;
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [base.at[0], base.at[2] + 1.05],
        yaw: 0.,
        pitch: ((centre - 1.65) / 1.05).atan(),
    };
    hair::interaction::refresh(&host, &a.pose, &mut a.world);
    assert_eq!(
        a.world.interaction.as_ref().map(|t| t.anchor.as_str()),
        Some("entry-0"),
        "looking at actual floating pixels must select their file"
    );
    a.input.captured = true;
    tap(&mut a, KeyCode::KeyX);
    assert_eq!(a.inventory.items().len(), 1);
    assert_eq!(a.themes[0].host, host);
    assert_eq!(
        std::fs::read(f.0.join("Swatch.png")).unwrap(),
        include_bytes!("../examples/site-a/swatch.png")
    );
}
#[test]
fn sandbox_defaults_tree_off_and_escape_unlocks_before_menu() {
    let mut a = app();
    assert!(
        a.world.tree_collapsed,
        "new sessions start with the explorer off"
    );
    a.input.captured = true;
    a.input.forward = true;
    a.key(KeyCode::Escape, true, false).unwrap();
    assert!(!a.input.captured && !a.input.forward);
    assert!(a.menu.is_none(), "unlock must not also pause");
    a.key(KeyCode::Escape, true, true).unwrap();
    a.key(KeyCode::Escape, true, false).unwrap();
    assert!(
        a.menu.is_none(),
        "a held press cannot become a second Escape"
    );
    a.key(KeyCode::Escape, false, false).unwrap();
    tap(&mut a, KeyCode::Escape);
    assert!(a.menu.is_some(), "second fresh Escape pauses");
}

#[test]
fn sandbox_escape_unlock_applies_to_bundled_room_too() {
    let mut a = app();
    a.local = None;
    a.input.captured = true;
    tap(&mut a, KeyCode::Escape);
    assert!(!a.input.captured && a.menu.is_none());
    tap(&mut a, KeyCode::Escape);
    assert!(a.menu.is_some());
}

#[test]
fn sandbox_inventory_consumes_owned_item_mesh_and_closes_without_recapture() {
    let f = fixture("inventory-theme");
    std::fs::write(f.0.join("One.txt"), b"unchanged themed inventory").unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    a.inventory
        .collect(&a.local.as_ref().unwrap().explorer, 0)
        .unwrap();
    a.input.captured = true;
    tap(&mut a, KeyCode::KeyB);
    let actual = a.inventory_overlay_for([640, 480]).unwrap();
    let shape = &a
        .inventory
        .preview(a.inventory.items()[0].id)
        .unwrap()
        .model
        .vertices;
    assert!(!shape.is_empty());
    assert_eq!(
        shape.len(),
        hair::file_items::Catalog::builtin()
            .unwrap()
            .representation(std::ffi::OsStr::new("One.txt"))
            .vertices
            .len()
    );
    assert_ne!(
        shape.len(),
        a.themes[a.theme_index].inventory_document_shape().len()
    );
    // The owned built-in class mesh is representative, not verified document art.
    let presentation = || hair::inventory_ui::Presentation {
        state: hair::inventory_ui::DocumentsState::SessionOnlyFixture,
        selection: Some(a.inventory_selection),
        representative: true,
    };
    let expected = hair::inventory_ui::overlay_documents(
        a.inventory.items(),
        a.inventory_yaw,
        [640, 480],
        shape,
        presentation(),
    );
    assert!(
        bytemuck::cast_slice::<_, u8>(&actual) == bytemuck::cast_slice::<_, u8>(&expected),
        "App inspector must use the owned class-item mesh, never stationary furniture"
    );
    a.ui_scale = 200;
    let viewport = hair::sidebar::Frame::new([1280, 960], a.ui_scale, false).full;
    assert_ne!(viewport, [1280, 960]);
    let scaled = a.inventory_overlay_for([1280, 960]).unwrap();
    let expected_scaled = hair::inventory_ui::overlay_documents(
        a.inventory.items(),
        a.inventory_yaw,
        viewport,
        shape,
        hair::inventory_ui::Presentation {
            state: hair::inventory_ui::DocumentsState::SessionOnlyFixture,
            selection: Some(a.inventory_selection),
            representative: true,
        },
    );
    assert!(
        bytemuck::cast_slice::<_, u8>(&scaled) == bytemuck::cast_slice::<_, u8>(&expected_scaled),
        "inventory must use full-window logical UI scale, not raw physical pixels or reserved sidebar width"
    );
    assert!(!a.input.captured && !a.world.reading);
    tap(&mut a, KeyCode::KeyB);
    assert!(a.inventory_overlay_for([640, 480]).is_none() && !a.input.captured);
    assert_eq!(
        std::fs::read(f.0.join("One.txt")).unwrap(),
        b"unchanged themed inventory"
    );
}
#[test]
fn sandbox_confirmation_freezes_input_and_cancel_or_late_conflict_keeps_inventory() {
    let source = fixture("review-source");
    let dest = fixture("review-dest");
    std::fs::write(source.0.join("File.txt"), b"held review bytes").unwrap();
    let original = sandbox_fixture_records(&source.0);
    let mut a = app();
    let id = a
        .inventory
        .collect(&hair::local_explorer::Explorer::new(&source.0).unwrap(), 0)
        .unwrap();
    a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [2.0, 1.65],
        yaw: 0.0,
        pitch: -std::f32::consts::FRAC_PI_4,
    };
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    a.key(KeyCode::Enter, true, false).unwrap();
    assert_eq!(a.editor_phase(), "Positioning");
    assert!(a.placement_action.as_ref().unwrap().pending.is_none());
    a.key(KeyCode::Enter, true, false).unwrap();
    assert_eq!(a.editor_phase(), "Positioning", "held Enter cannot review");
    a.key(KeyCode::Enter, false, false).unwrap();
    a.key(KeyCode::Enter, true, false).unwrap();
    assert_eq!(a.editor_phase(), "Review");
    assert!(a.placement_action.as_ref().unwrap().pending.is_some());
    a.key(KeyCode::Enter, true, false).unwrap();
    assert!(!dest.0.join("File.txt").exists());
    a.key(KeyCode::Enter, false, false).unwrap();
    let position = a.placement_action.as_ref().unwrap().pending.clone();
    let theme = a.theme_index;
    let shape = a.placement_preview().unwrap();
    for key in [
        KeyCode::ArrowLeft,
        KeyCode::KeyV,
        KeyCode::KeyC,
    ] {
        tap(&mut a, key);
    }
    assert_eq!(a.placement_action.as_ref().unwrap().pending, position);
    assert_eq!(a.theme_index, theme);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&a.placement_preview().unwrap()),
        bytemuck::cast_slice::<_, u8>(&shape)
    );
    assert!(!a.input.captured);
    tap(&mut a, KeyCode::Tab);
    assert_eq!(a.placement_action.as_ref().unwrap().pending, position);
    tap(&mut a, KeyCode::Escape);
    assert_eq!(a.editor_phase(), "Editing");
    assert!(a.placement_action.as_ref().unwrap().pending.is_none());
    tap(&mut a, KeyCode::Escape);
    assert!(a.placement_action.is_none() && a.inventory_open);
    assert!(a.inventory.preview(id).is_some());
    assert!(!dest.0.join("File.txt").exists());
    assert!(
        !dest.0.join(".hair-layout").exists(),
        "review/cancel must be write-free"
    );
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyV);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Positioning");
    tap(&mut a, KeyCode::Enter);
    assert!(a.placement_action.as_ref().unwrap().pending.is_some());
    std::fs::write(dest.0.join("File.txt"), b"external owner replacement").unwrap();
    tap(&mut a, KeyCode::Enter);
    assert!(a.inventory.preview(id).is_some());
    assert_eq!(
        std::fs::read(dest.0.join("File.txt")).unwrap(),
        b"external owner replacement"
    );
    assert!(a.local_status.contains("already exists"));
    a.placement_ui();
    assert!(
        a.local_status.contains("already exists"),
        "refusal stays visible after redraw"
    );
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::Escape);
    assert!(a.placement_action.is_none());
    // The explicit failed final commit may create only the cooperative lock;
    // it must not activate a record or retain staged payload bytes.
    assert!(hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer)
        .unwrap().positions.is_empty());
    for entry in std::fs::read_dir(dest.0.join(".hair-layout")).unwrap() {
        assert_eq!(entry.unwrap().file_name(), "lock");
    }
    assert_eq!(std::fs::read(source.0.join("File.txt")).unwrap(), b"held review bytes");
    for context in [KeyCode::KeyT, KeyCode::F5, KeyCode::KeyM] {
        assert_review_context_cancels_without_writes(&source.0, context);
    }
    assert_eq!(sandbox_fixture_records(&source.0), original);
}
// Context requests cancel rather than preserving the obsolete frozen draft.
// Seed an independent explicit record so cancellation must preserve real layout
// bytes and payload identity, not merely the absence of a new metadata file.
fn sandbox_fixture_records(root: &Path) -> std::collections::BTreeMap<PathBuf, (u64, u64, Option<Vec<u8>>)> {
    use std::os::unix::fs::MetadataExt;
    fn walk(root: &Path, out: &mut std::collections::BTreeMap<PathBuf, (u64, u64, Option<Vec<u8>>)>) {
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            assert!(metadata.is_file() || metadata.is_dir(), "owned fixture contains an unexpected link");
            let bytes = metadata.is_file().then(|| std::fs::read(&path).unwrap());
            out.insert(path.clone(), (metadata.dev(), metadata.ino(), bytes));
            if metadata.is_dir() { walk(&path, out); }
        }
    }
    let mut out = Default::default();
    walk(root, &mut out);
    out
}
fn assert_review_context_cancels_without_writes(source: &Path, context: KeyCode) {
    use std::os::unix::fs::MetadataExt;
    let target = fixture("review-context");
    let explorer = hair::local_explorer::Explorer::new(&target.0).unwrap();
    let existing = hair::placement_store::Position {
        at: [4.0, 0.0], yaw: 0.0, size: [0.8, 0.85, 0.28],
        allow_overlap: false, properties: Default::default(),
        surface: Some(hair::placement::surface::Contact {
            at: [4.0, 0.0, 0.0], normal: [0.0, 1.0, 0.0],
        }),
    };
    assert!(matches!(hair::placement_store::publish(
        &explorer, std::ffi::OsStr::new("Existing.txt"), b"existing context payload",
        &existing, &std::sync::atomic::AtomicBool::new(false)
    ), hair::placement_store::Outcome::Committed { .. }));
    let layout = sandbox_fixture_records(&target.0);
    let payload = std::fs::metadata(target.0.join("Existing.txt")).unwrap();
    let original = std::fs::metadata(source.join("File.txt")).unwrap();
    let mut a = app();
    let id = a.inventory.collect(&hair::local_explorer::Explorer::new(source).unwrap(), 0).unwrap();
    a.complete_folder(Ok(Some(target.0.clone()))).unwrap();
    a.pose = Pose { position: [2.0, 1.65], pitch: -std::f32::consts::FRAC_PI_4, ..Default::default() };
    let host = a.themes[0].host.clone();
    let appearance = a.themes[0].vertices_for(&a.world);
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Review");
    tap(&mut a, context);
    assert!(a.placement_action.is_none() && a.place_mode.is_none(), "{context:?} cancels");
    assert_eq!(a.themes[0].host, host);
    assert_eq!(bytemuck::cast_slice::<_, u8>(&a.themes[0].vertices_for(&a.world)),
        bytemuck::cast_slice::<_, u8>(&appearance), "{context:?} restores appearance");
    if context == KeyCode::KeyM {
        assert_eq!(a.map_mode, hair::exploration::MapMode::Expanded);
        tap(&mut a, KeyCode::Escape);
    }
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Enter);
    assert!(a.placement_action.is_none());
    assert!(a.inventory.preview(id).is_some());
    assert_eq!(a.inventory.items().len(), 1);
    assert!(!target.0.join("File.txt").exists(), "stale confirmation cannot publish");
    assert_eq!(sandbox_fixture_records(&target.0), layout);
    assert_eq!(std::fs::read(target.0.join("Existing.txt")).unwrap(), b"existing context payload");
    let after = std::fs::metadata(target.0.join("Existing.txt")).unwrap();
    assert_eq!((payload.dev(), payload.ino()), (after.dev(), after.ino()));
    assert_eq!(std::fs::read(source.join("File.txt")).unwrap(), b"held review bytes");
    let after = std::fs::metadata(source.join("File.txt")).unwrap();
    assert_eq!((original.dev(), original.ino()), (after.dev(), after.ino()));
    assert_eq!(std::fs::read_dir(&target.0).unwrap().count(), 2);
}
#[test]
fn sandbox_confirmed_copy_commits_file_and_surface_then_move_refuses_overwrite() {
    use std::os::unix::fs::MetadataExt;
    let source = fixture("confirm-owned-source");
    let dest = fixture("confirm-owned-dest");
    std::fs::write(source.0.join("File.txt"), b"immutable owned bytes").unwrap();
    let original = std::fs::metadata(source.0.join("File.txt")).unwrap();
    let mut a = app();
    let id = a
        .inventory
        .collect(&hair::local_explorer::Explorer::new(&source.0).unwrap(), 0)
        .unwrap();
    std::fs::write(source.0.join("Other.txt"), b"unauthorized carried item").unwrap();
    let other = a.inventory
        .collect(&hair::local_explorer::Explorer::new(&source.0).unwrap(), 1)
        .unwrap();
    let originals = sandbox_fixture_records(&source.0);
    a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [2.0, 1.65],
        yaw: 0.0,
        pitch: -std::f32::consts::FRAC_PI_4,
    };
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Positioning");
    assert!(a.placement_action.as_ref().unwrap().pending.is_none());
    tap(&mut a, KeyCode::Enter);
    assert!(
        !dest.0.join("File.txt").exists(),
        "second fresh Enter reviews without publication"
    );
    assert!(
        a.local_status.contains("Confirm"),
        "confirmation must be visible: {}",
        a.local_status
    );
    a.key(KeyCode::Enter, true, true).unwrap();
    assert!(!dest.0.join("File.txt").exists(), "repeat cannot confirm");
    a.key(KeyCode::Enter, false, false).unwrap();
    tap(&mut a, KeyCode::Enter);
    assert_eq!(
        std::fs::read(dest.0.join("File.txt")).unwrap(),
        b"immutable owned bytes"
    );
    assert!(a.inventory.preview(id).is_some(), "Copy retains inventory");
    assert!(a.inventory.preview(other).is_some());
    assert_eq!(a.inventory.items().len(), 2);
    assert!(a.placement_action.is_none());
    let read = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
    assert_eq!(read.positions.len(), 1);
    assert!(read.positions[0].1.surface.is_some());
    let check = |a: &App| {
        let (key, b) = a
            .world
            .folder_view
            .as_ref()
            .unwrap()
            .bindings
            .iter()
            .find(|(_, b)| b.entry.name == std::ffi::OsStr::new("File.txt"))
            .unwrap();
        let mesh = a.themes[0].file_item_shape(key, &b.entry.name);
        let centre = (mesh
            .iter()
            .map(|v| v.position[0])
            .fold(f32::INFINITY, f32::min)
            + mesh
                .iter()
                .map(|v| v.position[0])
                .fold(f32::NEG_INFINITY, f32::max))
            * 0.5;
        assert!((centre - 2.0).abs() < 0.0001);
    };
    check(&a);
    tap(&mut a, KeyCode::F5);
    check(&a);
    a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    check(&a);
    let layout = sandbox_fixture_records(&dest.0);
    let copied = std::fs::metadata(dest.0.join("File.txt")).unwrap();
    // Reopening resets the camera. Aim at known clear floor so the attempt
    // reaches the existing-name check, not a no-target false positive.
    a.pose = Pose { position: [-2.0, 1.65], pitch: -std::f32::consts::FRAC_PI_4, ..Default::default() };
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyV);
    tap(&mut a, KeyCode::KeyC);
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("already exists"), "{}", a.local_status);
    assert!(a.inventory.preview(id).is_some());
    assert_eq!(
        std::fs::read(dest.0.join("File.txt")).unwrap(),
        b"immutable owned bytes"
    );
    assert_eq!(sandbox_fixture_records(&dest.0), layout);
    let after = std::fs::metadata(dest.0.join("File.txt")).unwrap();
    assert_eq!((copied.dev(), copied.ino()), (after.dev(), after.ino()));
    tap(&mut a, KeyCode::Escape);
    let moved = fixture("confirm-move-target");
    a.complete_folder(Ok(Some(moved.0.clone()))).unwrap();
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [2.0, 1.65],
        yaw: 0.0,
        pitch: -std::f32::consts::FRAC_PI_4,
    };
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyV);
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Review");
    assert!(a.inventory.preview(id).is_some());
    assert!(!moved.0.join("File.txt").exists());
    tap(&mut a, KeyCode::Enter);
    assert!(a.inventory.preview(id).is_none());
    assert!(a.inventory.preview(other).is_some(), "Move consumes only the authorized item");
    assert_eq!(a.inventory.items().len(), 1);
    assert_eq!(
        std::fs::read(moved.0.join("File.txt")).unwrap(),
        b"immutable owned bytes"
    );
    assert_eq!(
        std::fs::read(source.0.join("File.txt")).unwrap(),
        b"immutable owned bytes"
    );
    let after = std::fs::metadata(source.0.join("File.txt")).unwrap();
    assert_eq!((original.dev(), original.ino()), (after.dev(), after.ino()));
    assert_eq!(sandbox_fixture_records(&source.0), originals);
}
#[test]
fn sandbox_publication_refuses_to_displace_a_saved_item_off_page_into_invalid_room() {
    let f = fixture("publication-page-boundary");
    let last = hair::folder_scene::PAGE_SIZE - 2;
    let saved = format!("File-{last:03}.txt");
    for i in 0..=hair::folder_scene::PAGE_SIZE {
        if i != last {
            std::fs::write(f.0.join(format!("File-{i:03}.txt")), b"owned page fixture").unwrap();
        }
    }
    let e = hair::local_explorer::Explorer::new(&f.0).unwrap();
    let saved_pos = hair::placement_store::Position {
        properties: Default::default(),
        at: [-9.5, 0.0],
        yaw: 0.,
        size: [0.8, 0.85, 0.28],
        allow_overlap: true,
        surface: None,
    };
    assert!(matches!(
        hair::placement_store::publish(
            &e,
            std::ffi::OsStr::new(&saved),
            b"retained preference",
            &saved_pos,
            &std::sync::atomic::AtomicBool::new(false)
        ),
        hair::placement_store::Outcome::Committed { .. }
    ));
    let e = hair::local_explorer::Explorer::new(&f.0).unwrap();
    let layout = hair::placement_store::read_layout(&e).unwrap();
    let (world, mut host) = hair::folder_scene::build(e.current(), 0, true).unwrap();
    assert!(
        hair::placement_scene::apply(&layout, &world, &mut host)
            .unwrap()
            .is_empty(),
        "baseline saved placement must actually fit before insertion"
    );
    assert_eq!(host.file_placements.len(), 1);
    let pending = hair::placement_store::Position {
        properties: Default::default(),
        at: [2., 0.],
        yaw: 0.,
        size: [0.8, 0.85, 0.28],
        allow_overlap: true,
        surface: None,
    };
    assert!(
        hair::placement_scene::validate_publication(
            &e,
            &layout,
            std::ffi::OsStr::new("000.txt"),
            &pending,
            [0., 1.],
            0
        )
        .is_err(),
        "insertion shifts an existing saved file across a page boundary into the Previous portal approach"
    );
    assert!(!f.0.join("000.txt").exists());
}
#[test]
fn sandbox_saved_surface_rehydrates_file_only_and_follows_external_rename() {
    let f = fixture("saved-surface-scene");
    let explorer = hair::local_explorer::Explorer::new(&f.0).unwrap();
    let position = hair::placement_store::Position {
        at: [4.0, 0.0],
        properties: Default::default(),
        yaw: 0.4,
        size: [0.8, 0.85, 0.28],
        allow_overlap: true,
        surface: Some(hair::placement::surface::Contact {
            at: [4.0, 1.4, 0.0],
            normal: [1.0, 0.0, 0.0],
        }),
    };
    assert!(matches!(
        hair::placement_store::publish(
            &explorer,
            std::ffi::OsStr::new("Placed.txt"),
            b"saved source",
            &position,
            &std::sync::atomic::AtomicBool::new(false)
        ),
        hair::placement_store::Outcome::Committed { .. }
    ));
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let check = |a: &App, name: &str| {
        let view = a.world.folder_view.as_ref().unwrap();
        let (id, bound) = view
            .bindings
            .iter()
            .find(|(_, b)| b.entry.name == std::ffi::OsStr::new(name))
            .unwrap();
        let mesh = a.themes[0].file_item_shape(id, &bound.entry.name);
        let min = mesh
            .iter()
            .map(|v| v.position[0])
            .fold(f32::INFINITY, f32::min);
        assert!(
            (min - 4.0).abs() < 0.00001,
            "saved surface base must be on x=4: {min}"
        );
        let (_, plain) = hair::folder_scene::build(
            a.local.as_ref().unwrap().explorer.current(),
            a.local.as_ref().unwrap().page,
            false,
        )
        .unwrap();
        let mut cache = hair::theme::AssetCache::default();
        let baseline =
            hair::theme::Theme::load_for_folder_scene(&a.themes[0].source, &mut cache, &plain)
                .unwrap();
        assert!(!baseline.collection_shape(id).is_empty());
        assert!(
            a.themes[0].collection_shape(id).is_empty(),
            "a file-only surface record must not allocate a detached listing-slot base"
        );
    };
    check(&a, "Placed.txt");
    tap(&mut a, KeyCode::F5);
    check(&a, "Placed.txt");
    std::fs::rename(f.0.join("Placed.txt"), f.0.join("Renamed.txt")).unwrap();
    tap(&mut a, KeyCode::F5);
    check(&a, "Renamed.txt");
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    check(&a, "Renamed.txt");
    assert_eq!(
        std::fs::read(f.0.join("Renamed.txt")).unwrap(),
        b"saved source"
    );
}
#[test]
fn sandbox_surface_preview_attaches_to_actual_item_face_without_moving_base() {
    let f = fixture("surface-preview");
    std::fs::write(f.0.join("File.txt"), b"surface acquisition").unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    a.inventory
        .collect(&a.local.as_ref().unwrap().explorer, 0)
        .unwrap();
    let base = a.themes[0].collection_shape("entry-0");
    let item = a.themes[0].file_item_shape("entry-0", std::ffi::OsStr::new("File.txt"));
    let centre = item
        .iter()
        .map(|v| glam::Vec3::from_array(v.position))
        .sum::<glam::Vec3>()
        / item.len() as f32;
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [centre.x, centre.z + 1.0],
        yaw: 0.0,
        pitch: (centre.y - 1.65).atan2(1.0),
    };
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    let preview = a.placement_preview().unwrap();
    let min = preview
        .iter()
        .map(|v| v.position[1])
        .fold(f32::INFINITY, f32::min);
    assert!(
        min > 0.1,
        "aiming at a floating item must attach above the floor, not silently hit the floor behind it: {min}"
    );
    tap(&mut a, KeyCode::KeyC);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&preview),
        bytemuck::cast_slice::<_, u8>(&a.placement_preview().unwrap())
    );
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&base),
        bytemuck::cast_slice::<_, u8>(&a.themes[0].collection_shape("entry-0"))
    );
    tap(&mut a, KeyCode::Escape);
    assert_eq!(a.inventory.items().len(), 1);
    assert!(!f.0.join(".hair-layout").exists());
}
#[test]
fn sandbox_placement_owns_visible_controls_even_without_a_floor_target() {
    let f = fixture("placement-no-floor");
    std::fs::write(f.0.join("File.txt"), b"no-floor fixture").unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    a.inventory
        .collect(&a.local.as_ref().unwrap().explorer, 0)
        .unwrap();
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    a.pose.pitch = 0.2;
    assert_eq!(a.editor_phase(), "Editing");
    assert!(a.placement_ui().is_empty(), "Editing owns the editor, not a world ghost");
    let editor = a.place_ui();
    assert!(!editor.is_empty());
    assert!(editor.iter().all(|v| v.position.iter().all(|x| x.is_finite())));
    assert!(a.toolbox_controls().iter().any(|(c, _, _)| *c == place_mode_native::Control::Cancel));
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Positioning");
    assert!(a.placement_preview().is_err());
    let mut ui = a.place_ui();
    ui.extend(a.placement_ui());
    assert!(
        !ui.is_empty(),
        "an unavailable floor target still needs an owning HUD and visible cancel controls"
    );
    assert!(a.placement_action.is_some());
    assert!(a.local_status.contains("Look down"));
    assert!(a.toolbox_controls().iter().any(|(c, _, _)| *c == place_mode_native::Control::Cancel));
    assert!(ui.iter().all(|v| v.position.iter().all(|x| x.is_finite())));
    tap(&mut a, KeyCode::Escape);
    assert_eq!(a.editor_phase(), "Editing");
    assert!(a.placement_action.is_some());
    tap(&mut a, KeyCode::Escape);
    assert!(a.placement_action.is_none());
    assert_eq!(a.inventory.items().len(), 1);
    assert!(!f.0.join(".hair-layout").exists());
}
#[test]
fn sandbox_placement_overlap_toggle_is_explicit_and_support_stays_snapped() {
    let f = fixture("placement-overlap");
    std::fs::write(f.0.join("File.txt"), b"owned overlap input").unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let id = a
        .inventory
        .collect(&a.local.as_ref().unwrap().explorer, 0)
        .unwrap();
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    assert_eq!(a.editor_phase(), "Editing");
    assert!(!a.place_ui().is_empty());
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Positioning");
    let anchor = a.themes[0]
        .host
        .anchors
        .iter()
        .find(|n| n.id == "entry-0")
        .unwrap();
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [anchor.at[0], anchor.at[2] + 1.65],
        yaw: 0.0,
        pitch: -std::f32::consts::FRAC_PI_4,
    };
    let floor = hair::placement::surface::target(
        &Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [0.0, 2.0],
            yaw: 0.0,
            pitch: -std::f32::consts::FRAC_PI_4,
        },
        &[],
    )
    .unwrap()
    .unwrap();
    assert_eq!(floor.normal, [0.0, 1.0, 0.0]);
    assert_eq!(floor.at[1], 0.0);
    let floor_mesh = hair::placement::surface::attached(
        &a.inventory.preview(id).unwrap().model.vertices,
        &floor,
        0.0,
        [0.8, 0.85, 0.28],
    )
    .unwrap();
    assert_eq!(
        floor_mesh
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min),
        0.0
    );
    let grounded = a.placement_preview().unwrap();
    assert!(grounded.iter().all(|v| v.position[1] >= 0.0));
    a.placement_ui();
    assert_eq!(
        a.placement_action.as_ref().unwrap().overlap,
        hair::placement::Overlap::Prevent
    );
    a.key(KeyCode::KeyC, true, false).unwrap();
    a.placement_ui();
    assert!(a.local_status.contains("Overlaps ON"));
    assert!(a.local_status.contains("Surface clear"));
    a.key(KeyCode::KeyC, true, true).unwrap();
    a.placement_ui();
    assert!(
        a.local_status.contains("Overlaps ON"),
        "held/repeat cannot flip twice"
    );
    a.key(KeyCode::KeyC, false, false).unwrap();
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&grounded),
        bytemuck::cast_slice::<_, u8>(&a.placement_preview().unwrap()),
        "collision preference never changes floor grounding or source geometry"
    );
    tap(&mut a, KeyCode::KeyC);
    a.placement_ui();
    assert_eq!(
        a.placement_action.as_ref().unwrap().overlap,
        hair::placement::Overlap::Prevent
    );
    tap(&mut a, KeyCode::KeyC);
    tap(&mut a, KeyCode::Escape);
    assert_eq!(a.editor_phase(), "Editing");
    tap(&mut a, KeyCode::Escape);
    assert!(a.placement_action.is_none() && a.inventory_open);
    assert!(a.inventory.preview(id).is_some());
    assert!(!f.0.join(".hair-layout").exists());
    tap(&mut a, KeyCode::KeyP);
    assert_eq!(a.editor_phase(), "Editing");
    a.place_ui();
    assert_eq!(
        a.placement_action.as_ref().unwrap().overlap,
        hair::placement::Overlap::Prevent,
        "next action defaults to preventing intersections"
    );
    tap(&mut a, KeyCode::Escape);
    assert_eq!(
        std::fs::read(f.0.join("File.txt")).unwrap(),
        b"owned overlap input"
    );
}
/// Compare the actual scene delta, not a stand-alone selection helper.
fn assert_selected_file_outline(a: &mut App) {
    let theme = &a.themes[a.theme_index];
    let target = a.world.explicit_target.as_ref().unwrap();
    let bound = &a.world.folder_view.as_ref().unwrap().bindings[&target.anchor];
    let mesh = theme.file_item_shape(&target.anchor, &bound.entry.name);
    let selected = theme.vertices_for(&a.world);
    let saved = a.world.explicit_target.take();
    let plain = theme.vertices_for(&a.world);
    a.world.explicit_target = saved;
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&selected[..plain.len()]),
        bytemuck::cast_slice::<_, u8>(&plain),
        "selection may only append chrome, never change file, labels or furniture"
    );
    let outline = &selected[plain.len()..];
    assert!(!outline.is_empty());
    assert!(
        outline
            .iter()
            .all(|v| v.color == [0.55, 0.95, 0.65] && v.texture == 0)
    );
    for axis in 0..3 {
        let low = mesh
            .iter()
            .map(|v| v.position[axis])
            .fold(f32::INFINITY, f32::min);
        let high = mesh
            .iter()
            .map(|v| v.position[axis])
            .fold(f32::NEG_INFINITY, f32::max);
        let outline_low = outline
            .iter()
            .map(|v| v.position[axis])
            .fold(f32::INFINITY, f32::min);
        let outline_high = outline
            .iter()
            .map(|v| v.position[axis])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (outline_low - (low - 0.015)).abs() < 1e-5,
            "outline axis {axis} must surround actual item, not stationary furniture: {outline_low} vs {low}"
        );
        assert!((outline_high - (high + 0.015)).abs() < 1e-5);
    }
}
#[test]
fn selected_file_outline_tracks_class_mesh_and_saved_surface_transforms() {
    let f = fixture("outline-surfaces");
    let mut a = app();
    for name in [
        "One.txt", "One.pdf", "One.wav", "One.mp4", "One.png", "One.glb", "One.bin",
    ] {
        let path = f.0.join(name);
        std::fs::write(&path, b"synthetic class fallback fixture").unwrap();
        a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
        let base = a.themes[0]
            .host
            .anchors
            .iter()
            .find(|n| n.id == "entry-0")
            .unwrap()
            .clone();
        a.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [base.at[0], base.at[2] + 1.0],
            yaw: 0.0,
            pitch: (base.at[1] + base.size[1] * 0.5 - 1.65).atan2(1.0),
        };
        tap(&mut a, KeyCode::Tab);
        assert_eq!(a.world.explicit_target.as_ref().unwrap().anchor, "entry-0");
        assert_selected_file_outline(&mut a);
        // Render-consumer witnesses: keep the selected identity while changing
        // only its runtime saved placement, without claiming storage/aim tests.
        for normal in [[0., 1., 0.], [1., 0., 0.], [0., -1., 0.], [0., 0.6, 0.8]] {
            let position = hair::placement_store::Position {
                at: [base.at[0], base.at[2]],
                properties: Default::default(),
                yaw: 0.7,
                size: [0.8, 0.85, 0.28],
                allow_overlap: true,
                surface: Some(hair::placement::surface::Contact {
                    at: [base.at[0], 2.0, base.at[2]],
                    normal,
                }),
            };
            let bounds = position.bounds().unwrap();
            a.themes[0].host.file_placements.insert(
                "entry-0".into(),
                hair::host_scene::PlacedItem { position, bounds },
            );
            let host = a.themes[0].host.clone();
            let furniture = a.themes[0].collection_shape("entry-0");
            let identity = a.world.explicit_target.clone();
            assert_selected_file_outline(&mut a);
            assert_eq!(a.themes[0].host, host);
            assert_eq!(a.world.explicit_target, identity);
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(&furniture),
                bytemuck::cast_slice::<_, u8>(&a.themes[0].collection_shape("entry-0"))
            );
        }
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"synthetic class fallback fixture"
        );
        std::fs::remove_file(path).unwrap();
    }
}

/// Optional retained offscreen witness on the existing real App Tab regression.
fn outline_capture(a: &mut App, label: &str) -> Option<Vec<u8>> {
    let root = std::env::var_os("HAIR_R3_CAPTURE_ROOT")?;
    let root = PathBuf::from(root);
    assert!(root.starts_with(std::env::temp_dir()));
    let dir = root.join(label);
    std::fs::create_dir(&dir).unwrap();
    a.update_hud();
    let target = hair::resources::smoke_output(&a.root, &dir).unwrap();
    a.renderer
        .as_ref()
        .unwrap()
        .capture(&target, "office.png", &a.pose, &a.world)
        .unwrap();
    a.renderer.as_ref().unwrap().wait().unwrap();
    let mut reader = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(dir.join("office.png")).unwrap(),
    ))
    .read_info()
    .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    pixels.truncate(info.buffer_size());
    Some(pixels)
}
#[test]
fn sandbox_explicit_tab_can_select_each_overlapping_local_file() {
    let f = fixture("overlap-tab");
    for name in ["One.txt", "Two.txt"] {
        std::fs::write(f.0.join(name), name.as_bytes()).unwrap();
    }
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let first = a.themes[0]
        .host
        .anchors
        .iter()
        .find(|n| n.id == "entry-0")
        .unwrap()
        .clone();
    let second = a.themes[0]
        .host
        .anchors
        .iter_mut()
        .find(|n| n.id == "entry-1")
        .unwrap();
    second.at = first.at;
    second.size = first.size;
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [first.at[0], first.at[2] + 1.0],
        yaw: 0.0,
        pitch: (first.at[1] + first.size[1] * 0.5 - 1.65).atan2(1.0),
    };
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    assert!(
        a.world.interaction.is_none(),
        "ordinary aim retains occlusion; selecting inside a pile is explicit"
    );
    if std::env::var_os("HAIR_R3_CAPTURE_ROOT").is_some() {
        let instance = render::instance();
        let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
        assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
        let mut renderer = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [960, 640],
            &a.themes[0].vertices_for(&a.world),
        ))
        .unwrap();
        renderer.set_theme_appearance(&a.themes[0]).unwrap();
        a.renderer = Some(renderer);
    }
    let before = outline_capture(&mut a, "ordinary-occluded");
    tap(&mut a, KeyCode::Tab);
    let one = a
        .world
        .explicit_target
        .clone()
        .expect("Tab reaches a file even inside an intersecting pile");
    assert_selected_file_outline(&mut a);
    if let Some(before) = before {
        let after = outline_capture(&mut a, "tab-selected").unwrap();
        let expected = [0.55_f32, 0.95, 0.65]
            .map(|v| ((1.055 * v.powf(1.0 / 2.4) - 0.055) * 255.0).round() as u8);
        let added = before
            .as_chunks::<4>().0.iter()
            .zip(after.as_chunks::<4>().0.iter())
            .enumerate()
            .filter(|(i, (old, new))| {
                let y = i / 960;
                (80..500).contains(&y)
                    && old != new
                    && new[..3]
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| a.abs_diff(b) <= 2)
            })
            .count();
        assert!(
            added > 50,
            "actual scene must add green item outline pixels outside HUD: {added}"
        );
        println!(
            "R3_OFFSCREEN_METAL added_green_world_samples={added}; real App Tab, not native/human input"
        );
    }
    tap(&mut a, KeyCode::Tab);
    let two = a.world.explicit_target.clone().unwrap();
    assert_ne!(one.anchor, two.anchor);
    assert_selected_file_outline(&mut a);
    tap(&mut a, KeyCode::Tab);
    assert_eq!(a.world.explicit_target.as_ref().unwrap().anchor, one.anchor);
    a.key(KeyCode::ShiftLeft, true, false).unwrap();
    tap(&mut a, KeyCode::Tab);
    a.key(KeyCode::ShiftLeft, false, false).unwrap();
    assert_eq!(a.world.explicit_target.as_ref().unwrap().anchor, two.anchor);
    assert_eq!(a.world.folder_view.as_ref().unwrap().bindings.len(), 2);
    assert!(!a.world.reading);
    tap(&mut a, KeyCode::KeyE);
    assert!(
        a.world.reading,
        "explicit selected identity must reach the retained reader"
    );
    tap(&mut a, KeyCode::Escape);
    a.world.explicit_target = None;
    a.world.interaction = None;
    a.themes[0].host.geometry.push(hair::host_scene::Geometry {
        at: [first.at[0], 0.0, first.at[2] + 0.5],
        size: [0.9, 2.0, 0.1],
    });
    tap(&mut a, KeyCode::Tab);
    assert!(
        a.world.explicit_target.is_none(),
        "explicit pile selection cannot see through structural host walls"
    );
    for name in ["One.txt", "Two.txt"] {
        assert_eq!(std::fs::read(f.0.join(name)).unwrap(), name.as_bytes());
    }
}
#[test]
fn sandbox_placement_directory_mismatch_permanently_cancels_action() {
    let f = fixture("placement-authority");
    std::fs::write(f.0.join("File.txt"), b"stable").unwrap();
    let other = f.0.join("Other");
    std::fs::create_dir(&other).unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let index = a
        .local
        .as_ref()
        .unwrap()
        .explorer
        .current()
        .entries
        .iter()
        .position(|e| e.name == "File.txt")
        .unwrap();
    a.inventory
        .collect(&a.local.as_ref().unwrap().explorer, index)
        .unwrap();
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    let original = a.local.as_ref().unwrap().explorer.destination();
    let next = hair::local_explorer::Explorer::new(&other)
        .unwrap()
        .destination();
    a.local.as_mut().unwrap().explorer.jump(&next).unwrap();
    assert!(a.placement_ui().is_empty());
    assert!(
        a.placement_action.is_none(),
        "authority loss must invalidate, not park a resumable action"
    );
    a.local.as_mut().unwrap().explorer.jump(&original).unwrap();
    assert!(a.placement_ui().is_empty() && a.placement_action.is_none());
    assert_eq!(a.inventory.items().len(), 1);
}
#[test]
fn sandbox_temporary_placement_owns_releases_focus_and_sidebar() {
    let f = fixture("placement-ownership");
    std::fs::write(f.0.join("File.txt"), b"stable").unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    a.inventory
        .collect(&a.local.as_ref().unwrap().explorer, 0)
        .unwrap();
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    a.input.captured = true;
    a.key(KeyCode::KeyW, true, false).unwrap();
    assert!(a.input.forward);
    a.key(KeyCode::KeyW, false, false).unwrap();
    assert!(
        !a.input.forward,
        "release belongs to placement, not paused-menu latch suppression"
    );
    a.input.captured = false;
    a.world.tree_collapsed = false;
    assert!(
        a.sidebar_open_entry(0).is_err(),
        "sidebar cannot open a reader through placement"
    );
    assert!(!a.world.reading);
    a.focus_changed(false);
    assert!(a.placement_action.is_none() && !a.input.captured);
    assert_eq!(std::fs::read_dir(&f.0).unwrap().count(), 1);
    assert_eq!(a.inventory.items().len(), 1);
}
#[test]
fn sandbox_inventory_starts_temporary_floor_placement_and_cancel_preserves_copy() {
    use std::os::unix::fs::MetadataExt;
    let f = fixture("placement-preview");
    let source = f.0.join("Original.txt");
    std::fs::write(&source, b"owned preview input").unwrap();
    let original = std::fs::metadata(&source).unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let id = a
        .inventory
        .collect(&a.local.as_ref().unwrap().explorer, 0)
        .unwrap();
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [0.0, 2.0],
        yaw: 0.0,
        pitch: -0.8,
    };
    let host = a.themes[0].host.clone();
    let appearance = a.themes[0].vertices_for(&a.world);
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    assert!(
        !a.inventory_open,
        "P must leave inventory for temporary floor placement"
    );
    let action = a.placement_action.as_ref().expect("temporary action");
    assert_eq!(action.item, id);
    assert!(!action.move_item, "copy is the conservative default");
    assert_eq!(a.editor_phase(), "Editing");
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Positioning");
    let before = (a.pose.position, a.pose.yaw, a.pose.pitch);
    let preview = a.placement_preview().unwrap();
    assert!(!preview.is_empty());
    assert!(
        preview
            .iter()
            .all(|v| v.position.iter().all(|n| n.is_finite()))
    );
    tap(&mut a, KeyCode::ArrowRight);
    tap(&mut a, KeyCode::KeyV);
    assert!(a.placement_action.as_ref().unwrap().move_item);
    assert_ne!(a.placement_action.as_ref().unwrap().yaw, 0.0);
    let yaw = a.placement_action.as_ref().unwrap().yaw;
    tap(&mut a, KeyCode::Tab);
    assert!(!a.input.captured);
    assert_eq!(a.placement_action.as_ref().unwrap().yaw, yaw);
    assert!(a.placement_action.as_ref().unwrap().move_item);
    tap(&mut a, KeyCode::KeyM);
    assert!(a.placement_action.is_none() && a.place_mode.is_none());
    assert_eq!(a.themes[0].host, host);
    assert_eq!(bytemuck::cast_slice::<_, u8>(&a.themes[0].vertices_for(&a.world)),
        bytemuck::cast_slice::<_, u8>(&appearance), "context cancellation restores appearance");
    tap(&mut a, KeyCode::KeyX);
    tap(&mut a, KeyCode::F5);
    assert_eq!(a.inventory.items().len(), 1);
    assert_eq!(a.map_mode, hair::exploration::MapMode::Expanded);
    assert_eq!(before, (a.pose.position, a.pose.yaw, a.pose.pitch));
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Enter);
    assert!(a.placement_action.is_none(), "context change disarms stale Enter");
    assert_eq!(std::fs::read_dir(&f.0).unwrap().count(), 1);
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Escape);
    assert_eq!(a.editor_phase(), "Editing");
    assert!(a.placement_action.is_some());
    tap(&mut a, KeyCode::Escape);
    assert!(a.placement_action.is_none() && a.menu.is_none() && !a.input.captured);
    assert_eq!(a.inventory.items()[0].id, id);
    assert_eq!(std::fs::read(&source).unwrap(), b"owned preview input");
    let after = std::fs::metadata(&source).unwrap();
    assert_eq!((original.dev(), original.ino()), (after.dev(), after.ino()));
    assert_eq!(
        std::fs::read_dir(&f.0).unwrap().count(),
        1,
        "preview/cancel must create no metadata or destination"
    );
}
#[test]
fn sandbox_inventory_owns_input_and_rotates_without_moving_camera() {
    let mut a = app();
    a.input.captured = true;
    a.input.forward = true;
    tap(&mut a, KeyCode::KeyB);
    assert!(a.inventory_open && !a.input.captured && !a.input.forward);
    let pose = a.pose;
    let yaw = a.inventory_yaw;
    tap(&mut a, KeyCode::ArrowRight);
    assert_ne!(yaw, a.inventory_yaw);
    tap(&mut a, KeyCode::KeyW);
    tap(&mut a, KeyCode::KeyM);
    assert!(!a.input.forward && a.map_mode != hair::exploration::MapMode::Expanded);
    assert_eq!(
        (a.pose.position, a.pose.yaw, a.pose.pitch),
        (pose.position, pose.yaw, pose.pitch)
    );
    tap(&mut a, KeyCode::Escape);
    assert!(!a.inventory_open && a.menu.is_none() && !a.input.captured);
}

#[test]
fn sandbox_collection_uses_acquired_image_not_pending_room_art() {
    let f = fixture("acquired-image");
    let bytes = include_bytes!("../examples/site-a/swatch.png");
    let decoded = hair::assets::decode_local_image(bytes).unwrap();
    std::fs::write(f.0.join("Image.png"), bytes).unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert!(a.object_previews.cache.is_empty());
    let anchor = a.themes[a.theme_index]
        .host
        .anchors
        .iter()
        .find(|n| n.id == "entry-0")
        .unwrap()
        .clone();
    a.pose.position = [anchor.at[0], anchor.at[2] + 1.0];
    a.pose.yaw = 0.0;
    a.pose.pitch = (anchor.at[1] + anchor.size[1] * 0.5 - 1.65).atan2(1.0);
    a.input.captured = true;
    tap(&mut a, KeyCode::KeyX);
    let owned = a.inventory.preview(a.inventory.items()[0].id).unwrap();
    assert!(owned.content_derived);
    assert_eq!(owned.model.textures[0].rgba, decoded.rgba);
    let effect = &a.collection_feedback.as_ref().unwrap().1;
    assert_eq!(effect.textures.len(), owned.model.textures.len());
    assert!(
        effect
            .textures
            .iter()
            .zip(&owned.model.textures)
            .all(|(a, b)| Arc::ptr_eq(a, b)),
        "suction must retain the SAME acquired textures as inventory"
    );
    let mut shape =
        hair::placement::grounded(&owned.model.vertices, [0.0, 0.0], 0.0, [0.8, 0.85, 0.28])
            .unwrap();
    for v in &mut shape {
        v.position[0] += anchor.at[0];
        v.position[1] += anchor.at[1] + anchor.size[1] + 0.12;
        v.position[2] += anchor.at[2];
    }
    let expected = hair::collection::Effect::new(&shape, &a.pose, a.sidebar_frame())
        .unwrap()
        .paint(0.0, false);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&effect.paint(0.0, false)),
        bytemuck::cast_slice::<_, u8>(&expected)
    );
    assert_eq!(std::fs::read(f.0.join("Image.png")).unwrap(), bytes);
}
#[test]
fn sandbox_collection_projects_only_the_floating_item_not_its_base() {
    let f = fixture("item-only-collection");
    std::fs::write(f.0.join("One.txt"), b"original item").unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let theme = &a.themes[a.theme_index];
    let anchor = theme
        .host
        .anchors
        .iter()
        .find(|n| {
            a.world
                .folder_view
                .as_ref()
                .unwrap()
                .bindings
                .get(&n.id)
                .is_some_and(|b| b.entry.name == "One.txt")
        })
        .unwrap()
        .clone();
    a.pose.position = [anchor.at[0], anchor.at[2] + 1.0];
    a.pose.yaw = 0.0;
    a.pose.pitch = (anchor.at[1] + anchor.size[1] * 0.5 - 1.65).atan2(1.0);
    a.input.captured = true;
    let shape =
        a.themes[a.theme_index].file_item_shape(&anchor.id, std::ffi::OsStr::new("One.txt"));
    let expected = hair::collection::Effect::new(&shape, &a.pose, a.sidebar_frame())
        .unwrap()
        .paint(0.0, false);
    tap(&mut a, KeyCode::KeyX);
    assert_eq!(a.inventory.items().len(), 1);
    let actual = a.collection_feedback.as_ref().unwrap().1.paint(0.0, false);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&actual),
        bytemuck::cast_slice::<_, u8>(&expected),
        "collection must project item only, never stationary desk"
    );
    assert_eq!(
        std::fs::read(f.0.join("One.txt")).unwrap(),
        b"original item"
    );
}
#[test]
fn sandbox_collection_is_cancelled_and_cannot_run_under_real_modal_transitions() {
    for key in [KeyCode::Escape, KeyCode::KeyB, KeyCode::KeyM] {
        let f = fixture("collect-modal");
        std::fs::write(f.0.join("One.txt"), b"unchanged modal fixture").unwrap();
        let mut a = app();
        a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
        let anchor = a.themes[a.theme_index]
            .host
            .anchors
            .iter()
            .find(|n| {
                a.world
                    .folder_view
                    .as_ref()
                    .unwrap()
                    .bindings
                    .get(&n.id)
                    .is_some_and(|b| b.entry.kind == hair::local_explorer::Kind::File)
            })
            .unwrap();
        a.pose.position = [anchor.at[0], anchor.at[2] + 1.0];
        a.pose.yaw = 0.0;
        a.pose.pitch = (anchor.at[1] + anchor.size[1] * 0.5 - 1.65).atan2(1.0);
        a.input.captured = true;
        tap(&mut a, KeyCode::KeyX);
        assert_eq!(a.inventory.items().len(), 1);
        let feedback = a.collection_feedback.clone();
        assert!(feedback.is_some());
        // Local L is deliberately inert, not the HTTPS address modal.
        tap(&mut a, KeyCode::KeyL);
        assert!(a.address.is_none());
        assert_eq!(
            a.collection_feedback.as_ref().map(|f| f.0),
            feedback.as_ref().map(|f| f.0)
        );
        tap(&mut a, key);
        assert!(
            a.collection_feedback.is_none(),
            "{key:?} must cancel flying geometry"
        );
        if key == KeyCode::Escape {
            assert!(!a.input.captured && a.menu.is_none());
            a.collection_feedback = feedback;
            tap(&mut a, KeyCode::Escape);
            assert!(a.menu.is_some());
        } else if key == KeyCode::KeyB {
            assert!(a.inventory_open);
        } else {
            assert_eq!(a.map_mode, hair::exploration::MapMode::Expanded);
        }
        let before = (
            a.pose.position,
            a.pose.yaw,
            a.pose.pitch,
            a.world.current.clone(),
        );
        tap(&mut a, KeyCode::KeyX);
        assert_eq!(
            a.inventory.items().len(),
            1,
            "X cannot collect through {key:?}"
        );
        assert!(a.collection_ui_at(Instant::now()).is_empty() && a.collection_feedback.is_none());
        assert_eq!(
            before,
            (
                a.pose.position,
                a.pose.yaw,
                a.pose.pitch,
                a.world.current.clone()
            )
        );
        assert_eq!(
            std::fs::read(f.0.join("One.txt")).unwrap(),
            b"unchanged modal fixture"
        );
    }
}
#[test]
fn sandbox_x_collects_aimed_copy_once_without_touching_original() {
    let f = fixture("collect");
    std::fs::write(f.0.join("One.txt"), b"original unchanged").unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let anchor = a.themes[a.theme_index]
        .host
        .anchors
        .iter()
        .find(|n| {
            a.world
                .folder_view
                .as_ref()
                .unwrap()
                .bindings
                .get(&n.id)
                .is_some_and(|b| b.entry.kind == hair::local_explorer::Kind::File)
        })
        .unwrap();
    a.pose.position = [anchor.at[0], anchor.at[2] + 1.0];
    a.pose.yaw = 0.0;
    a.pose.pitch = (anchor.at[1] + anchor.size[1] * 0.5 - 1.65).atan2(1.0);
    assert!(
        hair::interaction::resolve(&a.themes[a.theme_index].host, &a.pose, &a.world).is_some(),
        "fixture is actually aimed at an eligible object"
    );
    a.key(KeyCode::KeyX, true, false).unwrap();
    assert_eq!(a.inventory.items().len(), 1);
    let started = a
        .collection_feedback
        .as_ref()
        .expect("only successful copy starts suction")
        .0;
    let ui = a.collection_ui_at(started + Duration::from_millis(200));
    assert!(!ui.is_empty());
    a.key(KeyCode::KeyX, true, true).unwrap();
    a.key(KeyCode::KeyX, true, false).unwrap();
    assert_eq!(a.inventory.items().len(), 1);
    assert_eq!(
        a.collection_feedback.as_ref().unwrap().0,
        started,
        "held X must not restart suction"
    );
    a.key(KeyCode::KeyX, false, false).unwrap();
    let feedback = a.collection_feedback.clone();
    a.static_web_feedback = true;
    let quiet = a.collection_ui_at(started + Duration::from_millis(200));
    let badge = hair::collection::badge(a.sidebar_frame(), 1, true);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&quiet),
        bytemuck::cast_slice::<_, u8>(&badge),
        "static feedback retains success text without flying geometry"
    );
    a.static_web_feedback = false;
    a.collection_ui_at(started + Duration::from_secs(1));
    assert!(a.collection_feedback.is_none());
    a.collection_feedback = feedback.clone();
    a.world.tree_collapsed = false;
    a.collection_ui_at(started);
    assert!(
        a.collection_feedback.is_none(),
        "viewport changes invalidate the old projection"
    );
    a.world.tree_collapsed = true;
    a.collection_feedback = feedback.clone();
    a.focus_changed(false);
    assert!(a.collection_feedback.is_none());
    a.focus_changed(true);
    a.collection_feedback = feedback;
    assert_eq!(a.inventory.items()[0].bytes, b"original unchanged");
    assert_eq!(
        std::fs::read(f.0.join("One.txt")).unwrap(),
        b"original unchanged"
    );
    a.world.reading = true;
    tap(&mut a, KeyCode::KeyX);
    assert_eq!(a.inventory.items().len(), 1);
    assert!(a.collection_ui_at(started).is_empty() && a.collection_feedback.is_none());
    a.world.reading = false;
    std::fs::remove_file(f.0.join("One.txt")).unwrap();
    tap(&mut a, KeyCode::KeyX);
    assert!(
        a.collection_feedback.is_none(),
        "failed read must not celebrate collection"
    );
    assert!(a.local_status.starts_with("Not collected:"));
    assert_eq!(a.inventory.items().len(), 1);
}

#[test]
fn sidebar_reserves_content_and_release_opens_only_the_pressed_file() {
    let f = fixture("sidebar-input");
    std::fs::write(f.0.join("One.txt"), "SIDEBAR POINTER READER").unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    a.world.tree_collapsed = false;
    let frame = hair::sidebar::Frame::new([960, 640], 100, true);
    assert_eq!(a.ui_viewport(), frame.content);
    let point = [60.0, 112.0];
    assert!(a.sidebar_pointer(true, Some(point)));
    a.key(KeyCode::KeyM, true, false).unwrap();
    assert!(
        a.sidebar_press.is_none(),
        "map entry releases pending sidebar gesture"
    );
    a.key(KeyCode::KeyM, false, false).unwrap();
    a.key(KeyCode::Escape, true, false).unwrap();
    a.key(KeyCode::Escape, false, false).unwrap();
    assert!(a.sidebar_pointer(true, Some(point)));
    a.set_ui_scale(125);
    a.set_ui_scale(100);
    a.sidebar_pointer(false, Some(point));
    assert!(
        !a.world.reading,
        "scale cycle cancels gesture even when original frame returns"
    );
    assert!(a.sidebar_pointer(true, Some(point)));
    a.key(KeyCode::F5, true, false).unwrap();
    a.key(KeyCode::F5, false, false).unwrap();
    a.sidebar_pointer(false, Some(point));
    assert!(
        !a.world.reading,
        "refresh cancels retained snapshot gesture"
    );
    assert!(a.sidebar_pointer(true, Some(point)));
    a.open_menu(
        hair::presentation::Menu {
            view: hair::presentation::View::Pause,
            focus: 0,
        },
        KeyCode::Escape,
    );
    a.menu = None;
    a.sidebar_pointer(false, Some(point));
    assert!(!a.world.reading, "menu cycle cancels pending sidebar press");
    assert!(a.sidebar_pointer(true, Some(point)));
    a.focus_changed(false);
    a.focus_changed(true);
    a.sidebar_pointer(false, Some(point));
    assert!(
        !a.world.reading,
        "focus cycle must cancel pending sidebar press"
    );
    assert!(a.sidebar_pointer(true, Some(point)));
    assert!(!a.input.captured);
    assert!(!a.world.reading);
    assert!(a.sidebar_pointer(false, Some(point)));
    assert!(
        a.world
            .document()
            .unwrap()
            .contains("SIDEBAR POINTER READER")
    );
    a.world.tree_collapsed = true;
    assert_eq!(a.ui_viewport(), [960, 640]);
    assert!(!a.sidebar_pointer(true, Some(point)));
}
#[test]
fn sidebar_reader_opens_off_page_file_without_travel_and_refuses_nonfiles() {
    let f = fixture("sidebar");
    for i in 0..37 {
        std::fs::write(
            f.0.join(format!("File{i:03}.txt")),
            format!("SIDEBAR FILE {i}"),
        )
        .unwrap();
    }
    std::fs::create_dir(f.0.join("Child")).unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    a.world.tree_collapsed = false;
    let index = a
        .local
        .as_ref()
        .unwrap()
        .explorer
        .current()
        .entries
        .iter()
        .position(|e| e.name == "File036.txt")
        .unwrap();
    assert!(index >= hair::folder_scene::PAGE_SIZE);
    let before = a.directory_map.clone();
    let pose = a.pose;
    let snapshot = a.local.as_ref().unwrap().explorer.current().clone();
    let page = a.world.folder_view.as_ref().unwrap().page;
    a.sidebar_open_entry(index).unwrap();
    assert!(a.world.reading);
    assert!(a.world.document().unwrap().contains("SIDEBAR FILE 36"));
    assert_eq!(a.directory_map, before);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.world.folder_view.as_ref().unwrap().page, page);
    assert_eq!(a.local.as_ref().unwrap().explorer.current(), &snapshot);
    assert!(!a.input.captured);
    let folder = snapshot
        .entries
        .iter()
        .position(|e| e.name == "Child")
        .unwrap();
    assert!(a.sidebar_open_entry(folder).is_err());
    assert_eq!(a.directory_map, before);
    a.input.captured = true;
    assert!(a.sidebar_open_entry(index).is_err());
}
#[test]
fn directory_map_hover_is_inert_and_respects_focus_capture_and_modals() {
    use hair::directory_map_view::Action;
    let mut a = app();
    tap(&mut a, KeyCode::KeyM);
    let [w, h] = a.ui_viewport();
    let v = hair::directory_map_view::View::new(&a.directory_map, w, h, true, [0, 0], 0);
    let r = v.cards[0];
    let p = Some([
        f64::from((r[0] + r[2]) * 0.5),
        f64::from((r[1] + r[3]) * 0.5),
    ]);
    let before = a.directory_map.clone();
    let pose = a.pose;
    assert_eq!(
        a.directory_map_action(p),
        Some(Action::Destination(hair::directory_map::Target::Node(0)))
    );
    assert!(a.map_selection.is_none());
    assert_eq!(before, a.directory_map);
    assert_eq!(pose.position, a.pose.position);
    a.input.captured = true;
    assert_eq!(a.directory_map_action(p), None);
    a.input.captured = false;
    a.focused = false;
    assert_eq!(a.directory_map_action(p), None);
    a.focused = true;
    a.menu = Some(hair::presentation::Menu {
        view: hair::presentation::View::Pause,
        focus: 0,
    });
    assert_eq!(a.directory_map_action(p), None);
    a.menu = None;
    a.directory_map_pointer(p, true);
    assert!(a.map_selection.is_some());
    assert_eq!(a.directory_map_action(p), None);
    let r = hair::directory_map_view::confirmation_rects(w, h).1[1];
    let p = Some([
        f64::from((r[0] + r[2]) * 0.5),
        f64::from((r[1] + r[3]) * 0.5),
    ]);
    assert_eq!(a.directory_map_action(p), Some(Action::Travel));
    assert!(!a.map_confirm_travel, "hover cannot change default Cancel");
}
#[test]
fn directory_map_pointer_selects_but_cannot_double_click_through_confirmation() {
    let mut a = app();
    tap(&mut a, KeyCode::KeyM);
    let [w, h] = a.ui_viewport();
    let v = hair::directory_map_view::View::new(&a.directory_map, w, h, true, [0, 0], 0);
    let r = v.cards[0];
    let p = Some([
        f64::from((r[0] + r[2]) * 0.5),
        f64::from((r[1] + r[3]) * 0.5),
    ]);
    let before = a.directory_map.clone();
    a.directory_map_pointer(p, true);
    assert!(
        a.map_selection.is_some(),
        "a visible room click must select, not silently do nothing"
    );
    assert_eq!(a.directory_map, before);
    let (_, buttons) = hair::directory_map_view::confirmation_rects(w, h);
    let r = buttons[1];
    let travel = Some([
        f64::from((r[0] + r[2]) * 0.5),
        f64::from((r[1] + r[3]) * 0.5),
    ]);
    a.directory_map_pointer(travel, true);
    assert_eq!(a.directory_map, before, "held click cannot accept");
    a.directory_map_pointer(travel, false);
    a.directory_map_pointer(travel, true);
    assert_eq!(
        a.directory_map, before,
        "immediate second click cannot accept"
    );
    tap(&mut a, KeyCode::Escape);
    assert!(a.map_selection.is_none());
    assert_eq!(a.map_mode, hair::exploration::MapMode::Expanded);
}
#[test]
fn directory_map_history_confirm_is_transactional_and_focus_cancels_selection() {
    let mut a = app();
    let first = a.directory_map.history[0].clone();
    let door = a
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.kind == hair::local_explorer::Kind::Folder)
        .unwrap()
        .0
        .clone();
    aim(&mut a, &door);
    tap(&mut a, KeyCode::KeyE);
    let before = a.directory_map.clone();
    let pose = a.pose;
    let child = before.nodes[before.current.unwrap()].id;
    tap(&mut a, KeyCode::KeyM);
    a.directory_map_select(hair::directory_map::Target::History(0))
        .unwrap();
    let bad = a.themes[a.theme_index].source.clone();
    a.themes[a.theme_index].source = a.root.join("absent-map-travel-theme.toml");
    assert!(a.directory_map_confirm().is_err());
    assert_eq!(a.directory_map, before);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(
        a.local.as_ref().unwrap().explorer.map_identity().unwrap(),
        child
    );
    assert!(a.map_selection.as_ref().unwrap().error.is_some());
    a.themes[a.theme_index].source = bad;
    a.focus_changed(false);
    assert!(a.map_selection.is_none());
    a.focus_changed(true);
    a.directory_map_select(hair::directory_map::Target::History(0))
        .unwrap();
    tap(&mut a, KeyCode::Enter);
    assert!(a.map_selection.is_none(), "default confirmation is Cancel");
    a.directory_map_select(hair::directory_map::Target::History(0))
        .unwrap();
    tap(&mut a, KeyCode::Tab);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(
        a.local.as_ref().unwrap().explorer.map_identity().unwrap(),
        first.id
    );
    assert_eq!(a.local.as_ref().unwrap().page, first.page);
    assert_eq!(a.directory_map.history.len(), 1);
    assert!(!a.input.captured);
    assert_eq!(a.pose.yaw, 0.0);
}
#[test]
fn directory_map_travel_needs_confirmation_and_arrives_inward() {
    let mut a = app();
    let root = a.local.as_ref().unwrap().explorer.map_identity().unwrap();
    let door = a
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.kind == hair::local_explorer::Kind::Folder)
        .unwrap()
        .0
        .clone();
    aim(&mut a, &door);
    tap(&mut a, KeyCode::KeyE);
    let child = a.local.as_ref().unwrap().explorer.map_identity().unwrap();
    let pose = a.pose;
    tap(&mut a, KeyCode::KeyM);
    a.directory_map_select(hair::directory_map::Target::Node(0))
        .unwrap();
    assert_eq!(
        a.local.as_ref().unwrap().explorer.map_identity().unwrap(),
        child,
        "selection must never travel"
    );
    assert_eq!(a.pose.position, pose.position);
    a.directory_map_confirm().unwrap();
    assert_eq!(
        a.local.as_ref().unwrap().explorer.map_identity().unwrap(),
        root
    );
    assert_ne!(a.map_mode, hair::exploration::MapMode::Expanded);
    assert!(!a.input.captured);
    assert_eq!(a.pose.yaw, 0.0);
    assert_eq!(a.pose.pitch, 0.0);
    assert!(!a.themes[0].host.blocked(a.pose.position));
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(
        a.local.as_ref().unwrap().explorer.map_identity().unwrap(),
        child,
        "map jump appends departure to Back"
    );
    assert_eq!(a.pose.position, pose.position);
}
#[test]
fn directory_map_pages_remain_one_folder_with_paired_door_arrivals() {
    let f = fixture("pages");
    for i in 0..25 {
        std::fs::write(f.0.join(format!("file-{i:02}.bin")), b"actual fixture file").unwrap();
    }
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let id = a.local.as_ref().unwrap().explorer.map_identity().unwrap();
    let cells = a.directory_map.layout();
    let bounds = a.themes[0].host.room;
    for (door, page) in [
        ("folder-next", 1),
        ("folder-next", 2),
        ("folder-previous", 1),
        ("folder-previous", 0),
    ] {
        aim(&mut a, door);
        tap(&mut a, KeyCode::KeyE);
        assert_eq!(a.local.as_ref().unwrap().page, page);
        assert_eq!(a.directory_map.nodes.len(), 1);
        assert_eq!(a.directory_map.nodes[0].id, id);
        assert_eq!(a.directory_map.nodes[0].page, page);
        assert_eq!(a.directory_map.nodes[0].pages, 3);
        assert_eq!(a.directory_map.nodes[0].files, 25);
        assert_eq!(a.directory_map.layout(), cells);
        assert_eq!(a.themes[0].host.room, bounds);
        assert!(!a.themes[0].host.blocked(a.pose.position));
        assert_eq!(a.directory_map.back, None);
    }
}
#[test]
fn directory_map_rename_keeps_handle_identity_and_links_are_not_rooms() {
    let f = fixture("rename");
    let old = f.0.join("original");
    let new = f.0.join("renamed");
    std::fs::create_dir(&old).unwrap();
    std::fs::create_dir(old.join("child")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(old.join("child"), old.join("linked-folder")).unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(old.clone()))).unwrap();
    let id = a.directory_map.nodes[0].id;
    assert_eq!(a.directory_map.nodes[0].folders, 1);
    assert_eq!(a.directory_map.nodes.len(), 1);
    #[cfg(unix)]
    assert_eq!(a.directory_map.nodes[0].other, 1);
    std::fs::rename(old, &new).unwrap();
    tap(&mut a, KeyCode::F5);
    assert_eq!(a.directory_map.nodes.len(), 1);
    assert_eq!(a.directory_map.nodes[0].id, id);
    assert_eq!(a.directory_map.nodes[0].name, "renamed");
}
#[test]
fn directory_map_failed_theme_staging_does_not_record_a_phantom_child() {
    let mut a = app();
    let before = a.directory_map.clone();
    let folder = a.local.as_ref().unwrap().explorer.current().folder.clone();
    let (key, _) = a
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.kind == hair::local_explorer::Kind::Folder)
        .unwrap();
    let key = key.clone();
    aim(&mut a, &key);
    a.themes[0].source = a.root.join("missing-theme-for-directory-map-test.toml");
    tap(&mut a, KeyCode::KeyE);
    assert!(a.local_status.contains("Refused"));
    assert_eq!(a.directory_map, before);
    assert_eq!(a.local.as_ref().unwrap().explorer.current().folder, folder);
}

#[test]
fn directory_map_observes_real_hierarchy_not_travel_history() {
    let mut a = app();
    assert_eq!(
        a.directory_map.nodes.len(),
        1,
        "committed root must appear in directory map"
    );
    let index = a
        .local
        .as_ref()
        .unwrap()
        .explorer
        .current()
        .entries
        .iter()
        .position(|e| e.kind == hair::local_explorer::Kind::Folder)
        .unwrap();
    let binding = a
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.index == index)
        .unwrap()
        .0
        .clone();
    aim(&mut a, &binding);
    tap(&mut a, KeyCode::KeyE);
    assert_eq!(a.directory_map.nodes.len(), 2);
    assert_eq!(a.directory_map.nodes[1].parent, Some(0));
    tap(&mut a, KeyCode::KeyU);
    assert_eq!(a.directory_map.current, Some(0));
    let layout = a.directory_map.layout();
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(a.directory_map.current, Some(1));
    assert_eq!(
        a.directory_map
            .nodes
            .iter()
            .filter(|n| n.parent.is_some())
            .count(),
        1,
        "Back is travel, not ancestry"
    );
    assert_eq!(a.directory_map.layout(), layout);
    tap(&mut a, KeyCode::F5);
    assert_eq!(a.directory_map.layout(), layout);
}
#[test]
#[ignore = "explicit real local directory browsing + offscreen Metal; HAIR_DIRECTORY_CAPTURE_DIR"]
fn real_directory_browsing_captures() {
    let output = PathBuf::from(
        std::env::var_os("HAIR_DIRECTORY_CAPTURE_DIR").expect("explicit capture directory"),
    );
    assert!(output.is_dir() && !output.join("TOUR.txt").exists());
    let mut a = app();
    let root = a.root.clone();
    a.complete_folder(Ok(Some(root.clone()))).unwrap();
    let mut journal=vec![format!("Actual directory input: {}",root.display()),"Synthetic aim + production E/Parent/R/M adapters; offscreen Metal, not native physical input.".into()];
    fn enter(a: &mut App, name: &str, journal: &mut Vec<String>) {
        let index = a
            .local
            .as_ref()
            .unwrap()
            .explorer
            .current()
            .entries
            .iter()
            .position(|e| e.name == name && e.kind == hair::local_explorer::Kind::Folder)
            .expect("actual immediate subdirectory");
        let page = index / hair::folder_scene::PAGE_SIZE;
        while a.local.as_ref().unwrap().page < page {
            let old = a.local.as_ref().unwrap().page;
            aim(a, "folder-next");
            tap(a, KeyCode::KeyE);
            assert_eq!(a.local.as_ref().unwrap().page, old + 1);
        }
        while a.local.as_ref().unwrap().page > page {
            let old = a.local.as_ref().unwrap().page;
            aim(a, "folder-previous");
            tap(a, KeyCode::KeyE);
            assert_eq!(a.local.as_ref().unwrap().page, old - 1);
        }
        let id = a
            .world
            .folder_view
            .as_ref()
            .unwrap()
            .bindings
            .iter()
            .find(|(_, b)| b.index == index)
            .unwrap()
            .0
            .clone();
        aim(a, &id);
        tap(a, KeyCode::KeyE);
        assert_eq!(
            a.local
                .as_ref()
                .unwrap()
                .explorer
                .current()
                .folder
                .file_name()
                .unwrap(),
            name
        );
        journal.push(format!("ENTER {name}"));
    }
    fn parent(a: &mut App, journal: &mut Vec<String>) {
        let old = a.local.as_ref().unwrap().explorer.map_identity().unwrap();
        aim(a, "folder-parent");
        tap(a, KeyCode::KeyE);
        assert_ne!(
            a.local.as_ref().unwrap().explorer.map_identity().unwrap(),
            old
        );
        journal.push("PARENT E (paired doorway)".into());
    }
    enter(&mut a, "examples", &mut journal);
    for name in [
        "GreenleafReadingRoom-appearance",
        "GreenleafReadingRoom-spaces",
    ] {
        enter(&mut a, name, &mut journal);
        parent(&mut a, &mut journal);
    }
    parent(&mut a, &mut journal);
    enter(&mut a, "src", &mut journal);
    for name in ["assets", "text", "render"] {
        enter(&mut a, name, &mut journal);
        parent(&mut a, &mut journal);
    }
    assert_eq!(a.directory_map.nodes.len(), 8);
    assert_eq!(
        a.directory_map
            .nodes
            .iter()
            .filter(|n| n.parent.is_some())
            .count(),
        7
    );
    let layout = a.directory_map.layout();
    let current = a.directory_map.current;
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(
        a.local
            .as_ref()
            .unwrap()
            .explorer
            .current()
            .folder
            .file_name()
            .unwrap(),
        "render"
    );
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(a.directory_map.current, current);
    assert_eq!(a.directory_map.layout(), layout);
    journal.push("R to held render, R to held src; tree unchanged".into());
    fn shot(a: &mut App, output: &Path, label: &str, full: bool, size: [u32; 2]) {
        let instance = render::instance();
        let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
        assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
        let mut renderer = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            size,
            &a.themes[a.theme_index].vertices_for(&a.world),
        ))
        .unwrap();
        renderer
            .set_theme_appearance(&a.themes[a.theme_index])
            .unwrap();
        a.renderer = Some(renderer);
        a.ui_scale = 100;
        let h = &a.themes[a.theme_index].host;
        a.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: h.spawn,
            yaw: 0.0,
            pitch: -0.16,
        };
        a.world.explicit_target = None;
        a.world.interaction = None;
        if full {
            tap(a, KeyCode::KeyM);
        }
        a.update_hud();
        assert!(a.renderer.as_ref().unwrap().directory_map.is_some());
        let dir = output.join(label);
        std::fs::create_dir(&dir).unwrap();
        let target = resources::smoke_output(&a.root, &dir).unwrap();
        a.renderer
            .as_ref()
            .unwrap()
            .capture(&target, "office.png", &a.pose, &a.world)
            .unwrap();
        a.renderer.take().unwrap().wait().unwrap();
        if full {
            tap(a, KeyCode::KeyM);
        }
    }
    shot(
        &mut a,
        &output,
        "populated-directory-map",
        true,
        [1600, 1100],
    );
    shot(
        &mut a,
        &output,
        "directory-world-minimap",
        false,
        [1280, 960],
    );
    shot(&mut a, &output, "directory-map-minimum", true, [640, 480]);
    journal.push(format!(
        "Visited nodes: {} / hierarchy edges: {}",
        a.directory_map.nodes.len(),
        a.directory_map
            .nodes
            .iter()
            .filter(|n| n.parent.is_some())
            .count()
    ));
    for (i, n) in a.directory_map.nodes.iter().enumerate() {
        journal.push(format!(
            "#{} {:?} parent={:?} folders={} files={} other={} page={}/{} bounds={:?}",
            i + 1,
            n.name,
            n.parent.map(|p| p + 1),
            n.folders,
            n.files,
            n.other,
            n.page + 1,
            n.pages,
            n.bounds
        ));
    }
    std::fs::write(output.join("TOUR.txt"), journal.join("\n")).unwrap();
}

fn aim(a: &mut App, id: &str) {
    let h = &a.themes[a.theme_index].host;
    let p = h.anchors.iter().find(|v| v.id == id).unwrap();
    let n = h.portal_inward(id);
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [p.at[0] + 1.05 * n[0], p.at[2] + 1.05 * n[1]],
        yaw: (-n[0]).atan2(n[1]),
        pitch: ((p.at[1] + p.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    a.world.explicit_target = None;
    hair::interaction::refresh(h, &a.pose, &mut a.world);
    assert_eq!(
        a.world.interaction.as_ref().map(|t| t.anchor.as_str()),
        Some(id)
    );
}
#[test]
fn directory_map_pan_can_reach_the_bounded_tree_extent() {
    let mut a = app();
    tap(&mut a, KeyCode::KeyM);
    for _ in 0..130 {
        tap(&mut a, KeyCode::ArrowRight);
    }
    assert_eq!(a.route_pan[0], 128);
    tap(&mut a, KeyCode::KeyM);
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.route_pan, [0, 0]);
}
#[test]
fn directory_map_m_is_modal_and_shift_m_keeps_its_independent_preference() {
    let mut a = app();
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(
        a.map_mode,
        hair::exploration::MapMode::Expanded,
        "M must open the directory map"
    );
    let pose = a.pose;
    let page = a.local.as_ref().unwrap().page;
    for k in [
        KeyCode::KeyW,
        KeyCode::KeyR,
        KeyCode::KeyU,
        KeyCode::PageDown,
        KeyCode::F5,
        KeyCode::KeyE,
        KeyCode::KeyO,
    ] {
        tap(&mut a, k);
    }
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.pose.yaw, pose.yaw);
    assert_eq!(a.pose.pitch, pose.pitch);
    assert_eq!(a.local.as_ref().unwrap().page, page);
    assert!(!a.chooser_pending);
    tap(&mut a, KeyCode::Escape);
    assert_ne!(a.map_mode, hair::exploration::MapMode::Expanded);
    assert!(a.menu.is_none());
    let old = a.minimap_enabled;
    a.key(KeyCode::ShiftLeft, true, false).unwrap();
    tap(&mut a, KeyCode::KeyM);
    a.key(KeyCode::ShiftLeft, false, false).unwrap();
    assert_eq!(a.minimap_enabled, !old);
    tap(&mut a, KeyCode::KeyM);
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.minimap_enabled, !old);
}
