use super::*;

#[test]
fn storage_documents_numeric_edit_keeps_paging_errors_and_help_reachable() {
    let f = Fixture::new("editing-scroll");
    let prefs = f.directory("prefs");
    let mut a = app();
    a.restore_preferences_at(&prefs).unwrap();
    settings(&mut a);
    a.ui_scale = 200;
    activate(&mut a, Item::StorageItemLimit);
    text(&mut a, "0");
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("Limits unchanged"));
    a.storage.report = "long inspection/error evidence ".repeat(100);
    let initial = a.storage_panel([640, 480]);
    let (viewport, columns, rows) = hair::storage_ui::text_frame([640, 480], a.ui_scale);
    let wrapped = a.storage_lines().iter().map(|line| line.chars().count().div_ceil(columns)).sum::<usize>();
    let focused = a.menu.unwrap().focus;
    let mut visited = vec![false; wrapped];
    for _ in 0..wrapped.div_ceil(8) + 2 {
        let offset = a.storage.scroll.min(wrapped.saturating_sub(rows));
        visited.iter_mut().skip(offset).take(rows).for_each(|seen| *seen = true);
        let vertices = a.storage_panel([640, 480]);
        assert!(vertices.iter().all(|v| v.position.iter().all(|p| p.is_finite() && (-1.0..=1.0).contains(p))));
        tap(&mut a, KeyCode::PageDown);
    }
    assert!(a.storage.scroll > 0, "numeric editor must not swallow advertised paging keys");
    assert!(visited.into_iter().all(|seen| seen));
    assert_ne!(bytemuck::cast_slice::<_, u8>(&initial), bytemuck::cast_slice::<_, u8>(&a.storage_panel([640, 480])));
    assert_eq!(a.menu.unwrap().focus, focused);
    assert_eq!(a.storage.edit.as_ref().unwrap().1, "0");
    assert_eq!(viewport, [640, 480]);
    tap(&mut a, KeyCode::PageUp);
    tap(&mut a, KeyCode::Escape);
}

#[test]
fn storage_documents_chooser_preference_lock_and_session_copies_preserved() {
    use std::os::fd::AsRawFd;
    let f = Fixture::new("adopt-refused");
    let prefs = f.directory("prefs");
    let cache = f.directory("cache");
    let next = f.directory("next");
    let source = f.directory("source");
    std::fs::write(source.join("Note.txt"), b"no implicit session migration").unwrap();
    let mut a = app();
    a.restore_preferences_at(&prefs).unwrap();
    settings(&mut a);
    choose(&mut a, Ok(Some(cache.clone())));
    let saved = std::fs::read(prefs.join("HAIR/player-settings.toml")).unwrap();
    let lock = std::fs::File::open(prefs.join("HAIR")).unwrap();
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }, 0);
    choose(&mut a, Ok(Some(next.clone())));
    assert!(a.local_status.contains("busy"));
    assert_eq!(a.storage.preferences.path().unwrap(), cache);
    assert_eq!(std::fs::read(prefs.join("HAIR/player-settings.toml")).unwrap(), saved);
    drop(lock);
    let mut session = app();
    session.restore_preferences_at(&f.directory("session-prefs")).unwrap();
    collect(&mut session, &source);
    assert!(!session.inventory.is_persistent());
    let body = session.inventory.items()[0].bytes.clone();
    settings(&mut session);
    choose(&mut session, Ok(Some(next)));
    assert!(!session.inventory.is_persistent());
    assert_eq!(session.inventory.items()[0].bytes, body);
    assert!(session.local_status.contains("no implicit migration"));
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), body);
}

#[test]
fn storage_documents_actual_requested_move_is_copy_then_remove_keeps_published_room() {
    let f = Fixture::new("move-copy");
    let cache = f.directory("cache");
    let prefs = f.directory("prefs");
    let source = f.directory("source");
    let room = f.directory("room");
    let body = b"actual checked placement survives cache removal";
    std::fs::write(source.join("Note.txt"), body).unwrap();
    let mut a = app();
    a.restore_preferences_at(&prefs).unwrap();
    a.restore_documents_at(&cache).unwrap();
    collect(&mut a, &source);
    a.complete_folder(Ok(Some(room.clone()))).unwrap();
    a.pose = Pose { position: [2., 1.65], pitch: -std::f32::consts::FRAC_PI_4, ..Default::default() };
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    assert_eq!(a.editor_phase(), "Editing");
    tap(&mut a, KeyCode::KeyV);
    assert!(a.placement_action.as_ref().unwrap().move_item);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Positioning");
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
    tap(&mut a, KeyCode::Enter);
    assert!(a.placement_action.is_none(), "{}", a.local_status);
    assert!(a.local_status.contains("COPY") && a.local_status.contains("source retained"), "{}", a.local_status);
    assert_eq!(a.inventory.items().len(), 1);
    assert_eq!(std::fs::read(room.join("Note.txt")).unwrap(), body);
    // The editor may remain active after publication; exit it before list actions.
    if a.place_mode.is_some() { tap(&mut a, KeyCode::KeyP); }
    if !a.inventory_open { tap(&mut a, KeyCode::KeyB); }
    assert!(a.inventory_open);
    tap(&mut a, KeyCode::Delete);
    assert!(a.storage.removal.is_some(), "{}", a.local_status);
    tap(&mut a, KeyCode::Enter);
    assert!(a.inventory.items().is_empty(), "{}", a.local_status);
    let mut reopened = app();
    reopened.restore_documents_at(&cache).unwrap();
    reopened.complete_folder(Ok(Some(room.clone()))).unwrap();
    assert!(reopened.inventory.items().is_empty());
    assert_eq!(hair::placement_store::read_layout(&reopened.local.as_ref().unwrap().explorer).unwrap().positions.len(), 1);
    assert_eq!(std::fs::read(room.join("Note.txt")).unwrap(), body);
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), body);
}

#[test]
fn storage_documents_cleanup_status_distinguishes_committed_error_sync_and_unknown() {
    // Display formatting only: actual filesystem cleanup fault execution is backend-qualified.
    let mut outcome = hair::collection_store::RemovalOutcome {
        removed_id: 7, committed_generation: 2, payload_removed: false,
        remaining: Some(vec!["item-0000000000000007".into()]),
        cleanup_error: Some("permission denied".into()), sync_warning: None,
    };
    let text = storage_native::removal_status(&outcome);
    assert!(text.contains("COMMITTED") && text.contains("payload removed=false") && text.contains("permission denied") && text.contains("item-0000000000000007"));
    outcome.payload_removed = true;
    outcome.cleanup_error = None;
    outcome.sync_warning = Some("directory sync uncertain".into());
    outcome.remaining = None;
    let text = storage_native::removal_status(&outcome);
    assert!(text.contains("payload removed=true") && text.contains("sync uncertain") && text.contains("UNKNOWN") && text.contains("No automatic recovery or Retry"));
}
