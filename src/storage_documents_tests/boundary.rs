use super::*;

fn collect(a: &mut App, source: &Path) {
    a.complete_folder(Ok(Some(source.into()))).unwrap();
    aim(a); tap(a, KeyCode::KeyX);
    assert_eq!(a.inventory.items().len(), 1, "{}", a.local_status);
}
fn positioning(a: &mut App, room: &Path) {
    a.complete_folder(Ok(Some(room.into()))).unwrap();
    a.pose = Pose { position: [2., 1.65], pitch: -std::f32::consts::FRAC_PI_4, ..Default::default() };
    tap(a, KeyCode::KeyB); tap(a, KeyCode::KeyP);
    tap(a, KeyCode::Enter); tap(a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
}

fn bootstrap(resources: &Path) -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    App::new(resources.into(), vec![theme], 0, World::load(&root).unwrap(), false)
}
#[test]
fn storage_boundary_resources_move_replacement_retains_original_and_sibling_availability() {
    for replacement in [false, true] {
        let f = Fixture::new(if replacement { "boundary-replace" } else { "boundary-move" });
        let resources = f.directory("Resources"); let cache = f.directory("cache");
        let source = f.directory("source"); let room = f.directory("sibling-room");
        let protected = resources.join("protected-room"); std::fs::create_dir(&protected).unwrap();
        std::fs::write(source.join("Note.txt"), b"original remains").unwrap();
        let mut a = bootstrap(&resources);
        a.restore_documents_at(&cache).unwrap(); collect(&mut a, &source);
        let manifest = std::fs::read(cache.join("manifest")).unwrap();
        positioning(&mut a, &room);
        let held = f.0.join("held-Resources"); std::fs::rename(&resources, &held).unwrap();
        if replacement { std::fs::create_dir(&resources).unwrap(); }
        assert!(a.placement_preview().is_ok(), "unrelated room must not depend on Resources pathname: {}", a.local_status);
        tap(&mut a, KeyCode::Enter);
        assert_eq!(std::fs::read(room.join("Note.txt")).unwrap(), b"original remains");
        assert!(room.join(".hair-layout").exists());
        a.complete_folder(Ok(Some(held.join("protected-room")))).unwrap();
        a.inventory_open = true; tap(&mut a, KeyCode::KeyP);
        assert!(a.placement_action.is_none(), "replacement cannot grant writes into held original Resources: {}", a.local_status);
        assert!(std::fs::read_dir(held.join("protected-room")).unwrap().next().is_none());
        assert_eq!(std::fs::read(cache.join("manifest")).unwrap(), manifest);
        assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"original remains");
        println!("STORAGE-BOUNDARY Resources replacement={replacement} sibling=committed held-original=protected");
    }
}
