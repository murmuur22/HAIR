//! Actual input coverage of the existing vertical; fixture roots only.
mod edge_cases;
mod identity;
mod settings_lifecycle;
use super::*;
use hair::presentation::{Item, View};

fn activate(a: &mut App, item: Item) {
    assert!(a.menu.unwrap().items().contains(&item));
    for _ in 0..a.menu.unwrap().items().len() {
        if a.menu.unwrap().focused() == item { break; }
        tap(a, KeyCode::ArrowDown);
    }
    assert_eq!(a.menu.unwrap().focused(), item);
    tap(a, KeyCode::Enter);
}
fn settings(a: &mut App) {
    tap(a, KeyCode::Escape);
    activate(a, Item::Settings);
    activate(a, Item::PlayerSettings);
    activate(a, Item::Storage);
    assert_eq!(a.menu.unwrap().view, View::Storage);
}
fn text(a: &mut App, value: &str) {
    assert!(!a.keyboard_input(None, true, false, Some(value)).unwrap());
    assert!(!a.keyboard_input(None, false, false, None).unwrap());
}
fn collect(a: &mut App, source: &Path) {
    a.complete_folder(Ok(Some(source.into()))).unwrap();
    aim(a);
    tap(a, KeyCode::KeyX);
    assert!(!a.inventory.items().is_empty(), "{}", a.local_status);
}
fn choose(a: &mut App, result: Result<Option<PathBuf>>) {
    activate(a, Item::StorageChoose);
    a.frames = 1;
    assert!(a.claim_storage_request());
    assert!(!a.claim_storage_request(), "one native panel per request");
    a.complete_storage_selection(result);
    assert!(!a.chooser_active && !a.chooser_pending && !a.storage.chooser);
}

#[test]
fn storage_documents_settings_actual_input_positive_invalid_cancel_reopen_limits() {
    let f = Fixture::new("limits");
    let prefs = f.directory("prefs");
    let cache = f.directory("cache");
    let source = f.directory("source");
    let body = b"retained over lowered limits";
    std::fs::write(source.join("Note.txt"), body).unwrap();
    let mut a = app();
    a.restore_preferences_at(&prefs).unwrap();
    a.restore_documents_at(&cache).unwrap();
    collect(&mut a, &source);
    let manifest = std::fs::read(cache.join("manifest")).unwrap();
    settings(&mut a);
    let pose = a.pose;
    activate(&mut a, Item::StorageItemLimit);
    text(&mut a, "0");
    tap(&mut a, KeyCode::Enter);
    assert!(a.storage.edit.is_some());
    assert_eq!(a.storage.preferences.item_bytes, storage_native::DEFAULT_ITEM_BYTES);
    tap(&mut a, KeyCode::Backspace);
    text(&mut a, "-5");
    assert!(a.storage.edit.as_ref().unwrap().1.is_empty());
    text(&mut a, "8");
    tap(&mut a, KeyCode::Enter);
    assert!(a.storage.edit.is_none());
    activate(&mut a, Item::StorageDiskLimit);
    text(&mut a, "9223372036854775807");
    tap(&mut a, KeyCode::Enter);
    assert!(a.storage.edit.is_some(), "OS ceiling must still apply");
    tap(&mut a, KeyCode::Escape);
    activate(&mut a, Item::StorageDiskLimit);
    text(&mut a, "12");
    tap(&mut a, KeyCode::Enter);
    let saved = std::fs::read(prefs.join("HAIR/player-settings.toml")).unwrap();
    activate(&mut a, Item::StorageItemLimit);
    text(&mut a, "99");
    a.focus_changed(false);
    text(&mut a, "3");
    assert_eq!(a.storage.edit.as_ref().unwrap().1, "99");
    a.focus_changed(true);
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::KeyW);
    tap(&mut a, KeyCode::KeyX);
    assert_eq!((a.pose.position, a.pose.yaw, a.pose.pitch), (pose.position, pose.yaw, pose.pitch));
    assert_eq!(std::fs::read(prefs.join("HAIR/player-settings.toml")).unwrap(), saved);
    assert_eq!(std::fs::read(cache.join("manifest")).unwrap(), manifest);
    assert_eq!(a.inventory.items().len(), 1, "lowering cannot delete");
    drop(a);
    let mut b = app();
    b.restore_preferences_at(&prefs).unwrap();
    assert_eq!((b.storage.preferences.item_bytes, b.storage.preferences.disk_bytes), (8, 12));
    b.restore_documents_at(&b.storage.preferences.path().unwrap()).unwrap();
    assert_eq!(b.inventory.items().len(), 1);
    b.complete_folder(Ok(Some(source.clone()))).unwrap();
    aim(&mut b);
    tap(&mut b, KeyCode::KeyX);
    assert_eq!(b.inventory.items().len(), 1, "new import obeys lowered policy");
    assert!(b.local_status.contains("Not committed") && b.local_status.contains("Per-item"), "{}", b.local_status);
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), body);
}

#[test]
fn storage_documents_backward_v1_and_unsupported_preferences_preserved() {
    let f = Fixture::new("old-prefs");
    let prefs = f.directory("prefs");
    let store = hair::acquisition::PreferenceFile::open(&prefs).unwrap();
    let old = "version = 1\nui_scale = 150\ntree_visible = true\n[fog]\nenabled = false\ndistance = 20.0\ntint = [0.1, 0.2, 0.3]\n";
    store.write(old.as_bytes()).unwrap();
    let mut a = app();
    a.restore_preferences_at(&prefs).unwrap();
    assert_eq!(a.ui_scale, 150);
    assert!(!a.world.tree_collapsed && !a.fog.enabled());
    assert_eq!(a.storage.preferences.item_bytes, storage_native::DEFAULT_ITEM_BYTES);
    assert_eq!(a.storage.preferences.disk_bytes, storage_native::DEFAULT_DISK_BYTES);
    assert!(a.storage.preferences.path().is_none());
    settings(&mut a);
    activate(&mut a, Item::StorageItemLimit);
    text(&mut a, "17");
    tap(&mut a, KeyCode::Enter);
    let mut b = app();
    b.restore_preferences_at(&prefs).unwrap();
    assert_eq!((b.ui_scale, b.fog, b.world.tree_collapsed), (a.ui_scale, a.fog, a.world.tree_collapsed));
    assert_eq!(b.storage.preferences.item_bytes, 17);
    let unsupported = old.replacen("version = 1", "version = 2", 1);
    store.write(unsupported.as_bytes()).unwrap();
    let mut c = app();
    assert!(c.restore_preferences_at(&prefs).unwrap_err().contains("Unsupported"));
    settings(&mut c);
    activate(&mut c, Item::StorageDiskLimit);
    text(&mut c, "23");
    tap(&mut c, KeyCode::Enter);
    assert!(c.storage.edit.is_some());
    assert_eq!(c.storage.preferences.disk_bytes, storage_native::DEFAULT_DISK_BYTES);
    assert_eq!(store.read().unwrap().unwrap(), unsupported.as_bytes());
}

#[test]
fn storage_documents_actual_chooser_cancel_failure_unsafe_incompatible_and_adopt() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new("chooser");
    let prefs = f.directory("prefs");
    let old_cache = f.directory("old");
    let next_cache = f.directory("next");
    let source = f.directory("source");
    let body = b"original survives adoption";
    std::fs::write(source.join("Note.txt"), body).unwrap();
    let mut next = app();
    next.restore_documents_at(&next_cache).unwrap();
    collect(&mut next, &source);
    let next_id = next.inventory.items()[0].id;
    drop(next);
    let next_manifest = std::fs::read(next_cache.join("manifest")).unwrap();
    let mut a = app();
    a.restore_preferences_at(&prefs).unwrap();
    settings(&mut a);
    choose(&mut a, Ok(Some(old_cache.clone())));
    for _ in 0..4 { tap(&mut a, KeyCode::Escape); }
    collect(&mut a, &source);
    settings(&mut a);
    let saved = std::fs::read(prefs.join("HAIR/player-settings.toml")).unwrap();
    let old_manifest = std::fs::read(old_cache.join("manifest")).unwrap();
    let incompatible = f.directory("incompatible");
    drop(hair::collection_store::Store::open(&incompatible, a.storage.preferences.policy().unwrap()).unwrap());
    let mut bad_manifest = std::fs::read(incompatible.join("manifest")).unwrap();
    bad_manifest[8..12].copy_from_slice(&2_u32.to_le_bytes());
    std::fs::write(incompatible.join("manifest"), &bad_manifest).unwrap();
    let unsafe_root = f.directory("unsafe");
    std::fs::set_permissions(&unsafe_root, std::fs::Permissions::from_mode(0o777)).unwrap();
    let alias = f.0.join("alias");
    std::os::unix::fs::symlink(&next_cache, &alias).unwrap();
    for result in [Ok(None), Err("injected native chooser failure".into()), Ok(Some(incompatible.clone())), Ok(Some(unsafe_root)), Ok(Some(alias))] {
        choose(&mut a, result);
        assert_eq!(a.storage.preferences.path().unwrap(), old_cache);
        assert_eq!(a.inventory.items().len(), 1);
        assert_eq!(std::fs::read(prefs.join("HAIR/player-settings.toml")).unwrap(), saved);
        assert_eq!(std::fs::read(old_cache.join("manifest")).unwrap(), old_manifest);
        assert_eq!(std::fs::read(incompatible.join("manifest")).unwrap(), bad_manifest);
        assert_eq!(std::fs::read(next_cache.join("manifest")).unwrap(), next_manifest);
    }
    choose(&mut a, Ok(Some(next_cache.clone())));
    assert_eq!(a.storage.preferences.path().unwrap(), next_cache);
    assert_eq!(a.inventory.items()[0].id, next_id);
    assert!(a.local_status.contains("Previous cache untouched"));
    assert_eq!(std::fs::read(old_cache.join("manifest")).unwrap(), old_manifest);
    assert_eq!(std::fs::read(next_cache.join("manifest")).unwrap(), next_manifest);
    let mut b = app();
    b.restore_preferences_at(&prefs).unwrap();
    b.restore_documents_at(&b.storage.preferences.path().unwrap()).unwrap();
    assert_eq!(b.inventory.items()[0].name, "Note.txt");
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), body);
}

#[test]
fn storage_documents_actual_confirmation_cancel_shared_lock_and_stale_identity() {
    use std::os::fd::AsRawFd;
    let f = Fixture::new("stale");
    let cache = f.directory("cache");
    let source = f.directory("source");
    let room = f.directory("room");
    let body = b"original and room stay intact";
    std::fs::write(source.join("Note.txt"), body).unwrap();
    std::fs::write(room.join("Note.txt"), body).unwrap();
    let mut a = app();
    a.restore_documents_at(&cache).unwrap();
    collect(&mut a, &source);
    let first = a.inventory.items()[0].id;
    tap(&mut a, KeyCode::KeyX);
    let second = a.inventory.items()[1].id;
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::Delete);
    assert_eq!(a.storage.removal.as_ref().unwrap().id, second);
    assert!(a.storage_lines().iter().any(|line| line.contains(&format!("item-{second:016x}"))));
    let manifest = std::fs::read(cache.join("manifest")).unwrap();
    // Planning releases the lock while the user thinks.
    let lock = std::fs::File::open(cache.join("lock")).unwrap();
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }, 0);
    tap(&mut a, KeyCode::Escape);
    assert!(a.storage.removal.is_none());
    assert_eq!(std::fs::read(cache.join("manifest")).unwrap(), manifest);
    drop(lock);
    tap(&mut a, KeyCode::Delete);
    let lock = std::fs::File::open(cache.join("lock")).unwrap();
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }, 0);
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("busy"), "{}", a.local_status);
    assert_eq!(std::fs::read(cache.join("manifest")).unwrap(), manifest);
    drop(lock);
    for _ in 0..4 { tap(&mut a, KeyCode::Escape); }
    assert!(a.menu.is_none() && a.inventory_open);
    tap(&mut a, KeyCode::ArrowUp);
    tap(&mut a, KeyCode::Delete);
    assert_eq!(a.storage.removal.as_ref().unwrap().id, first);
    let mut peer = hair::collection_store::Store::open(&cache, a.storage.preferences.policy().unwrap()).unwrap();
    assert!(peer.remove(first).unwrap().is_complete());
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("Stale"), "{}", a.local_status);
    let mut reopened = app();
    reopened.restore_documents_at(&cache).unwrap();
    assert_eq!(reopened.inventory.items().len(), 1);
    assert_eq!(reopened.inventory.items()[0].id, second);
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), body);
    assert_eq!(std::fs::read(room.join("Note.txt")).unwrap(), body);
}

#[test]
fn storage_documents_unavailable_cache_blocks_actual_collect_session_fallback() {
    let f = Fixture::new("unavailable");
    let source = f.directory("source");
    let prefs = f.directory("prefs");
    std::fs::write(source.join("Note.txt"), b"no fallback copy").unwrap();
    let mut a = app();
    a.restore_preferences_at(&prefs).unwrap();
    a.storage.required = true; // Native startup's explicit persistent-required state.
    assert!(a.restore_documents_at(&f.0.join("missing-cache")).is_err());
    a.complete_folder(Ok(Some(source.clone()))).unwrap();
    aim(&mut a);
    tap(&mut a, KeyCode::KeyX);
    assert!(a.inventory.items().is_empty());
    assert!(a.local_status.contains("No silent session fallback"));
    settings(&mut a);
    assert!(a.storage_lines().iter().any(|line| line.contains("No active disk cache")));
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"no fallback copy");
}

#[test]
fn storage_documents_actual_selection_only_preview_budget_and_exact_metadata_list() {
    let f = Fixture::new("preview");
    let cache = f.directory("cache");
    let source = f.directory("source");
    let path = source.join("Large.txt");
    let file = std::fs::File::create(&path).unwrap();
    let length = hair::local_explorer::MAX_BODY_BYTES + 1;
    file.set_len(length).unwrap();
    drop(file);
    let mut a = app();
    a.restore_documents_at(&cache).unwrap();
    collect(&mut a, &source);
    let large = a.inventory.items()[0].id;
    let small_body = b"small text for selected preview";
    std::fs::write(&path, small_body).unwrap();
    // Fresh source metadata is required after replacing its bytes.
    a.complete_folder(Ok(Some(source))).unwrap();
    aim(&mut a);
    tap(&mut a, KeyCode::KeyX);
    let small = a.inventory.items()[1].id;
    drop(a);
    let mut b = app();
    b.restore_documents_at(&cache).unwrap();
    tap(&mut b, KeyCode::KeyB);
    assert_eq!(b.inventory.items().iter().map(|i| (i.id, i.stored_bytes, i.bytes.len())).collect::<Vec<_>>(), vec![(large, Some(length), 0), (small, Some(small_body.len() as u64), 0)]);
    assert!(!b.inventory.preview(large).unwrap().content_derived);
    assert!(b.inventory.preview(large).unwrap().warning.as_ref().unwrap().contains("budget"));
    assert!(b.inventory.preview(small).is_none());
    assert!(!b.inventory_overlay_for([640, 480]).unwrap().is_empty());
    tap(&mut b, KeyCode::ArrowDown);
    assert_eq!(b.storage.selected, Some(small));
    assert!(b.inventory.preview(large).is_none(), "no retained whole-cache previews");
    assert!(b.inventory.preview(small).is_some());
    tap(&mut b, KeyCode::F8);
    assert!(b.storage_lines().iter().any(|line| line.contains(&format!("{} bytes / 2 documents", length + small_body.len() as u64)) && line.contains("UNCHECKED")));
}

#[test]
fn storage_documents_actual_panel_640_scaled_path_help_errors_reachable() {
    let f = Fixture::new("layout");
    let prefs = f.directory("prefs");
    let cache = f.directory(&"long-cache-location-".repeat(10));
    let mut a = app();
    a.restore_preferences_at(&prefs).unwrap();
    a.restore_documents_at(&cache).unwrap();
    settings(&mut a);
    a.local_status = "VISIBLE ERROR: ".to_owned() + &"long error evidence preserved ".repeat(30);
    for scale in hair::presentation::STEPS {
        a.ui_scale = scale;
        a.storage.scroll = 0;
        let physical = [640, 480];
        let (viewport, columns, rows) = hair::storage_ui::text_frame(physical, scale);
        let lines = a.storage_lines();
        let wrapped = lines.iter().map(|line| line.chars().count().div_ceil(columns)).sum::<usize>();
        assert!(columns * 6 + 32 <= viewport[0] as usize);
        let layout = hair::presentation::layout(&a.menu.unwrap(), scale, physical);
        assert!(46. + rows as f32 * 12. <= viewport[1] as f32 - 148.);
        for (item, [x, y, w, h]) in &layout {
            assert!(*y >= viewport[1] as f32 - 132. && y + h <= viewport[1] as f32);
            let point = [(x + w / 2.) as f64, (y + h / 2.) as f64];
            assert_eq!(hair::presentation::hit(&a.menu.unwrap(), physical, scale, point), Some(*item));
        }
        let mut visited = vec![false; wrapped];
        while a.storage.scroll < wrapped + 8 {
            let offset = a.storage.scroll.min(wrapped.saturating_sub(rows));
            visited.iter_mut().skip(offset).take(rows).for_each(|seen| *seen = true);
            let vertices = a.storage_panel(physical);
            assert!(!vertices.is_empty());
            assert!(vertices.iter().all(|v| v.position.iter().all(|p| p.is_finite() && (-1.0..=1.0).contains(p))));
            tap(&mut a, KeyCode::PageDown);
        }
        assert!(visited.into_iter().all(|seen| seen), "every path/help/error character row reachable");
        tap(&mut a, KeyCode::PageUp);
    }
    // Actual click follows the same frame, not direct activation.
    let (_, [x, y, w, h]) = hair::presentation::layout(&a.menu.unwrap(), a.ui_scale, [960, 640])[0];
    let viewport = hair::presentation::ui_viewport([960, 640], a.ui_scale);
    assert!(!a.menu_click([(x + w / 2.) as f64 * 960. / viewport[0] as f64, (y + h / 2.) as f64 * 640. / viewport[1] as f64]));
    assert!(a.storage.edit.is_some());
    let focused = a.menu.unwrap().focus;
    tap(&mut a, KeyCode::ArrowDown);
    assert_eq!(a.menu.unwrap().focus, focused, "numeric edit owns navigation");
}
