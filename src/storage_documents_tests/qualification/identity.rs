//! Identity/presentation regression inputs use actual held folders and shared stores.
use super::*;
fn collected(label: &str) -> (Fixture, App, PathBuf, PathBuf) {
    let f = Fixture::new(label);
    let cache = f.directory("cache");
    let source = f.directory("source");
    std::fs::write(source.join("Note.txt"), b"original remains").unwrap();
    let mut a = app();
    a.restore_documents_at(&cache).unwrap();
    collect(&mut a, &source);
    (f, a, cache, source)
}
fn positioning(a: &mut App, room: &Path) {
    a.complete_folder(Ok(Some(room.into()))).unwrap();
    a.pose = Pose { position: [2., 1.65], pitch: -std::f32::consts::FRAC_PI_4, ..Default::default() };
    tap(a, KeyCode::KeyB);
    tap(a, KeyCode::KeyP);
    tap(a, KeyCode::Enter);
    tap(a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
}
fn names(path: &Path) -> Vec<std::ffi::OsString> {
    let mut names = std::fs::read_dir(path).unwrap().map(|e| e.unwrap().file_name()).collect::<Vec<_>>();
    names.sort(); names
}
fn has_text(vertices: &[hair::assets::Vertex], size: [u32; 2], at: [f32; 2], text: &str, color: [f32; 3]) -> bool {
    // The public world-label painter uses the identical glyphs/triangle order.
    // Recover integer pixel coordinates, independently of inventory UI labels.
    let origin = [text.len() as f32 * 6. * 0.012 * 0.5, 0., 0.];
    let expected = hair::text::object_label(text, origin, 200);
    vertices.windows(expected.len()).any(|window| window.iter().zip(&expected).all(|(actual, glyph)| {
        let x = (glyph.position[0] / 0.012).round() + at[0];
        let y = (-glyph.position[1] / 0.012).round() + at[1];
        actual.color == color && (actual.position[0] - (2. * x / size[0] as f32 - 1.)).abs() < 0.00001
            && (actual.position[1] - (1. - 2. * y / size[1] as f32)).abs() < 0.00001
    }))
}
#[test]
fn storage_identity_cache_root_descendant_and_alias_refuse_begin_without_publication() {
    let (f, mut a, cache, source) = collected("identity-boundaries");
    let descendant = cache.join("existing-child");
    std::fs::DirBuilder::new().mode(0o700).create(&descendant).unwrap();
    let alias = f.0.join("cache-alias");
    std::os::unix::fs::symlink(&cache, &alias).unwrap();
    let before = names(&cache);
    for target in [&cache, &descendant, &alias] {
        a.complete_folder(Ok(Some(target.clone()))).unwrap();
        a.inventory_open = true;
        tap(&mut a, KeyCode::KeyP);
        assert!(a.placement_action.is_none(), "cache destination acquired through {target:?} must refuse begin: {}", a.local_status);
        assert!(!target.join(".hair-layout").exists());
        assert!(!target.join("Note.txt").exists());
    }
    assert_eq!(names(&cache), before);
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"original remains");
}
#[test]
fn storage_identity_late_room_reparent_into_cache_refuses_fresh_commit() {
    let (f, mut a, cache, _) = collected("identity-late");
    let room = f.directory("room");
    positioning(&mut a, &room);
    let moved = cache.join("reparented-room");
    std::fs::rename(&room, &moved).unwrap();
    tap(&mut a, KeyCode::Enter);
    assert!(!moved.join("Note.txt").exists(), "fresh confirm must recheck actual ancestors: {}", a.local_status);
    assert!(!moved.join(".hair-layout").exists());
    assert!(a.placement_preview().is_err());
}
#[test]
fn storage_identity_cache_root_rebind_refuses_room_publication() {
    let (f, mut a, cache, _) = collected("identity-rebind");
    let room = f.directory("room");
    positioning(&mut a, &room);
    std::fs::rename(&cache, f.0.join("held-cache")).unwrap();
    std::fs::DirBuilder::new().mode(0o700).create(&cache).unwrap();
    tap(&mut a, KeyCode::Enter);
    assert!(!room.join("Note.txt").exists());
    assert!(!room.join(".hair-layout").exists());
    assert!(a.placement_preview().is_err(), "preview must also refuse rebound active root");
}
#[test]
fn storage_identity_adoption_visited_and_resources_preserve_namespace_and_preferences() {
    let f = Fixture::new("identity-adopt");
    let prefs = f.directory("prefs");
    let cache = f.directory("cache");
    let visited = f.directory("visited");
    let resources = f.directory("Resources");
    let bundled_child = resources.join("empty-bundled-data");
    std::fs::DirBuilder::new().mode(0o700).create(&bundled_child).unwrap();
    let mut a = app_at_resources(&resources); // Explicit fixture bootstrap; no owner data.
    a.restore_preferences_at(&prefs).unwrap();
    a.restore_documents_at(&cache).unwrap();
    a.complete_folder(Ok(Some(visited.clone()))).unwrap();
    settings(&mut a);
    activate(&mut a, Item::StorageItemLimit); text(&mut a, "32"); tap(&mut a, KeyCode::Enter);
    let previous = std::fs::read(prefs.join("HAIR/player-settings.toml")).unwrap();
    for candidate in [&visited, &bundled_child, &resources] {
        let before = names(candidate);
        choose(&mut a, Ok(Some(candidate.clone())));
        assert_eq!(names(candidate), before, "refused adoption must not initialize protected folder {candidate:?}");
        assert_eq!(a.storage.preferences.path().unwrap(), cache);
        assert_eq!(std::fs::read(prefs.join("HAIR/player-settings.toml")).unwrap(), previous);
    }
    // Browsing a high ancestor must not forbid adopting an independent child cache.
    a.menu = None;
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    settings(&mut a);
    let sibling = f.directory("sibling-cache");
    choose(&mut a, Ok(Some(sibling.clone())));
    assert_eq!(a.storage.preferences.path().unwrap(), sibling);
}
#[test]
fn storage_identity_peer_removal_insert_plan_cancel_anchors_row_preview_and_next_actions() {
    let (_f, mut a, cache, _) = collected("identity-selection");
    tap(&mut a, KeyCode::KeyX); tap(&mut a, KeyCode::KeyX);
    let first = a.inventory.items()[0].id;
    let selected = a.inventory.items()[2].id;
    tap(&mut a, KeyCode::KeyB);
    let mut peer = hair::collection_store::Store::open(&cache, a.storage.preferences.policy().unwrap()).unwrap();
    assert!(peer.remove(first).unwrap().is_complete());
    let explorer = &a.local.as_ref().unwrap().explorer;
    let held = explorer.open_reader(0).unwrap();
    peer.import_document(std::ffi::OsStr::new("peer.txt"), &held, &std::sync::atomic::AtomicBool::new(false)).unwrap();
    tap(&mut a, KeyCode::Delete);
    assert_eq!(a.storage.removal.as_ref().unwrap().id, selected);
    assert_eq!(a.inventory.items()[a.inventory_selection].id, selected, "highlight and action must agree after actual peer removal/insert");
    assert!(a.inventory.preview(selected).is_some());
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::KeyP);
    assert_eq!(a.placement_action.as_ref().unwrap().item, selected);
    tap(&mut a, KeyCode::Escape);
    if !a.inventory_open { tap(&mut a, KeyCode::KeyB); }
    tap(&mut a, KeyCode::Delete);
    peer.refresh().unwrap(); assert!(peer.remove(selected).unwrap().is_complete());
    tap(&mut a, KeyCode::Escape);
    assert_eq!(a.storage.selected, None, "cancel refresh explicitly clears disappeared identity");
    assert!(a.inventory.items().get(a.inventory_selection).is_none(), "no highlighted replacement");
    tap(&mut a, KeyCode::KeyP);
    assert!(a.placement_action.is_none());
    tap(&mut a, KeyCode::Delete);
    assert!(a.storage.removal.is_none());
}
#[test]
fn storage_identity_empty_persistent_and_required_unavailable_actual_overlay() {
    let f = Fixture::new("identity-labels");
    let cache = f.directory("cache");
    let mut a = app();
    a.restore_documents_at(&cache).unwrap();
    tap(&mut a, KeyCode::KeyB);
    let ui = a.inventory_overlay_for([640, 480]).unwrap();
    assert!(has_text(&ui, [640, 480], [32., 60.], "DISK METADATA (UNCHECKED) | B / ESC CLOSE", [0.7, 0.8, 0.7]), "empty persistent is not session-only");
    let mut b = app();
    b.storage.required = true;
    b.storage.preferences.root = Some(f.0.join("missing-cache").as_os_str().as_encoded_bytes().to_vec());
    assert!(b.restore_documents_at(&b.storage.preferences.path().unwrap()).is_err());
    tap(&mut b, KeyCode::KeyB);
    let ui = b.inventory_overlay_for([640, 480]).unwrap();
    assert!(has_text(&ui, [640, 480], [32., 60.], "PERSISTENT DOCUMENTS UNAVAILABLE | F8: HELP", [0.7, 0.8, 0.7]));
    b.open_storage();
    assert!(!b.storage_lines().iter().any(|line| line.contains("Session-only")));
}
#[test]
fn storage_identity_last_removal_numpad_confirms_and_retains_disk_label() {
    let (_f, mut a, _, _) = collected("identity-numpad");
    tap(&mut a, KeyCode::KeyB); tap(&mut a, KeyCode::Delete); tap(&mut a, KeyCode::NumpadEnter);
    assert!(a.inventory.items().is_empty(), "numpad Enter must confirm exact selected removal");
    a.menu = None;
    let ui = a.inventory_overlay_for([640, 480]).unwrap();
    assert!(has_text(&ui, [640, 480], [32., 60.], "DISK METADATA (UNCHECKED) | B / ESC CLOSE", [0.7, 0.8, 0.7]));
}
#[test]
fn storage_identity_persistent_move_fallback_label_before_confirmation_and_cycle() {
    let (f, mut a, _, _) = collected("identity-move");
    let room = f.directory("sibling-room");
    a.complete_folder(Ok(Some(room))).unwrap(); tap(&mut a, KeyCode::KeyB); tap(&mut a, KeyCode::KeyP);
    a.editor_inventory_ui();
    assert!(a.local_status.contains("COPY") && a.local_status.contains("source retained"));
    // Reach the actual cycle choice through the existing paged toolbox input.
    loop {
        let controls = a.editor_inventory_controls();
        if controls.iter().any(|(_, _, s)| s.contains("Move unavailable")) { break; }
        let (_, r, _) = controls.iter().find(|(c, _, _)| *c == place_mode_native::Control::EditorNext).expect("fallback-labelled cycle choice must be reachable");
        a.pointer_moved(Some([((r[0] + r[2]) * 0.5) as f64, ((r[1] + r[3]) * 0.5) as f64]));
        a.place_pointer(true); a.place_pointer(false);
    }
    tap(&mut a, KeyCode::KeyV); a.editor_inventory_ui();
    assert!(a.local_status.contains("COPY") && a.local_status.contains("source retained"));
    assert!(!a.local_status.contains("Move |"));
}
#[test]
fn storage_identity_oversized_preview_has_visible_representative_not_verified_marker() {
    let f = Fixture::new("identity-fallback");
    let cache = f.directory("cache"); let source = f.directory("source");
    std::fs::File::create(source.join("Large.txt")).unwrap().set_len(hair::local_explorer::MAX_BODY_BYTES + 1).unwrap();
    let mut a = app(); a.restore_documents_at(&cache).unwrap(); collect(&mut a, &source); tap(&mut a, KeyCode::KeyB);
    let ui = a.inventory_overlay_for([640, 480]).unwrap();
    assert!(has_text(&ui, [640, 480], [280., 350.], "REPRESENTATIVE | NOT VERIFIED", [0.65, 0.8, 0.65]));
}
#[test]
fn storage_identity_storage_chooser_focus_loss_clears_and_requires_fresh_request() {
    let mut a = app(); settings(&mut a); activate(&mut a, Item::StorageChoose); a.frames = 1;
    a.focus_changed(false);
    assert!(!a.storage.chooser && !a.chooser_pending);
    assert!(!a.claim_storage_request());
    a.focus_changed(true);
    assert!(!a.claim_storage_request(), "focus return cannot launch stale panel");
    activate(&mut a, Item::StorageChoose);
    a.focused = false; // Claim independently requires focus, even without event cleanup.
    assert!(!a.claim_storage_request());
}

#[test]
fn storage_identity_resources_placement_and_cache_existing_file_edit_refuse() {
    let f = Fixture::new("identity-other-writes");
    let cache = f.directory("cache"); let source = f.directory("source");
    let resources = f.directory("Resources");
    std::fs::write(source.join("Note.txt"), b"original remains").unwrap();
    let mut a = app_at_resources(&resources);
    a.restore_documents_at(&cache).unwrap(); collect(&mut a, &source);
    a.complete_folder(Ok(Some(resources.clone()))).unwrap();
    tap(&mut a, KeyCode::KeyB); tap(&mut a, KeyCode::KeyP);
    assert!(a.placement_action.is_none(), "bundled data cannot be a room publication destination");
    assert!(names(&resources).is_empty());
    a.inventory_open = false;
    a.complete_folder(Ok(Some(cache.clone()))).unwrap(); aim(&mut a);
    tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
    assert!(a.place_mode.as_ref().is_none_or(|m| m.edit.is_none() && m.decoration_edit.is_none()), "cache files cannot acquire layout-edit authority");
    assert!(!cache.join(".hair-layout").exists());
}

#[test]
fn storage_identity_actual_refresh_failure_has_blocked_persistent_presentation() {
    let (f, mut a, cache, _) = collected("identity-blocked");
    std::fs::rename(&cache, f.0.join("held-cache")).unwrap();
    std::fs::DirBuilder::new().mode(0o700).create(&cache).unwrap();
    tap(&mut a, KeyCode::KeyB);
    let ui = a.inventory_overlay_for([640, 480]).unwrap();
    assert!(has_text(&ui, [640, 480], [32., 60.], "PERSISTENT DOCUMENTS UNAVAILABLE | F8: HELP", [0.7, 0.8, 0.7]));
    a.open_storage();
    assert!(a.storage_lines()[0].contains("No active disk cache; configured path"));
}

#[test]
fn storage_identity_adoption_empty_room_layout_directory_never_initializes() {
    let f = Fixture::new("identity-layout-adopt");
    let cache = f.directory("cache"); let room = f.directory("room"); let prefs = f.directory("prefs");
    let layout = room.join(".hair-layout");
    std::fs::DirBuilder::new().mode(0o700).create(&layout).unwrap();
    let mut a = app(); a.restore_preferences_at(&prefs).unwrap(); a.restore_documents_at(&cache).unwrap();
    a.complete_folder(Ok(Some(room))).unwrap(); settings(&mut a);
    choose(&mut a, Ok(Some(layout.clone())));
    assert!(names(&layout).is_empty(), "room-owned layout namespace must not be initialized as a cache");
    assert_eq!(a.storage.preferences.path().unwrap(), cache);
}
