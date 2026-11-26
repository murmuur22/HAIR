//! No legacy auto-draining tap: every input below runs against a gated real job.
use super::*;
use std::sync::atomic::Ordering;
pub(super) fn staged(a: &mut App) -> (Receiver<()>, Sender<()>) {
    let pair = gate(a, false);
    a.storage.worker_gate.as_mut().unwrap().body = true;
    tap(a, KeyCode::KeyX);
    entered(&pair.0);
    pair
}
pub(super) fn committed_and_usable(a: &mut App, cache: &Path, source: &Path) {
    drain(a);
    assert_eq!(a.inventory.items().len(), 1, "{}", a.local_status);
    assert!(a.storage_lines().iter().any(|line| line.contains("COMMITTED")));
    assert!(!cache.join("payload.pending").exists(), "ordinary viewing must not orphan staging");
    assert_eq!(a.inventory.store().unwrap().evidence().unwrap(), &[] as &[std::ffi::OsString]);
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"held original unchanged");
    // A fresh actual mutation works without deleting evidence or reopening the Store.
    a.inventory_open = false;
    a.menu = None;
    a.map_mode = hair::exploration::MapMode::Compact;
    if a.world.reading { tap(a, KeyCode::Escape); }
    aim(a);
    tap(a, KeyCode::KeyX);
    drain(a);
    assert_eq!(a.inventory.items().len(), 2, "next mutation refused: {}", a.local_status);
}
#[test]
fn r1_documents_open_and_duplicate_import_do_not_cancel_staged_collect() {
    let (_f, mut a, cache, source) = fixture("lifecycle-documents");
    let (_arrived, release) = staged(&mut a);
    let token = a.documents_cancel_token();
    assert!(cache.join("payload.pending").exists());
    tap(&mut a, KeyCode::KeyX);
    let duplicate_status = a.local_status.clone();
    let duplicate_cancelled = token.load(Ordering::Relaxed);
    tap(&mut a, KeyCode::KeyB);
    let busy_status = a.local_status.clone();
    let busy_activity = a.document_activity();
    let overlay = a.inventory_overlay_for([640, 480]).unwrap();
    let expected_loading = hair::inventory_ui::overlay_documents(&[], a.inventory_yaw,
        hair::sidebar::Frame::new([640, 480], a.ui_scale, false).full, &[],
        hair::inventory_ui::Presentation { state: hair::inventory_ui::DocumentsState::PersistentLoading,
            selection: None, representative: false });
    let loading_visible = bytemuck::cast_slice::<_, u8>(&overlay) == bytemuck::cast_slice::<_, u8>(&expected_loading);
    tap(&mut a, KeyCode::ArrowDown); // Empty/last-known selection is not cancel.
    tap(&mut a, KeyCode::F8); // Non-authorizing recovery/details inspection.
    let cancelled = token.load(Ordering::Relaxed);
    release.send(()).unwrap();
    drain(&mut a);
    assert!(!duplicate_cancelled, "second X cancelled a staged import");
    assert!(duplicate_status.contains("busy") && duplicate_status.contains("duplicate"), "{duplicate_status}");
    assert!(!cancelled, "Documents/details opening changed the cancel flag: {busy_status}");
    assert!(!busy_activity.contains("Cancelling"), "{busy_activity}");
    assert!(busy_status.contains("busy") && loading_visible, "B must paint DISK BUSY / LOADING, not cancelling: {busy_status}");
    committed_and_usable(&mut a, &cache, &source);
}
#[test]
fn r1_actual_map_reader_and_folder_refresh_preserve_staged_collect() {
    for action in [KeyCode::KeyM, KeyCode::KeyE, KeyCode::F5, KeyCode::KeyL] {
        let (_f, mut a, cache, source) = fixture(&format!("lifecycle-action-{action:?}"));
        let (_arrived, release) = staged(&mut a);
        let token = a.documents_cancel_token();
        tap(&mut a, action);
        let transitioned = match action {
            KeyCode::KeyM => a.map_mode == hair::exploration::MapMode::Expanded,
            KeyCode::KeyE => a.world.reading,
            KeyCode::F5 => a.local.is_some(),
            KeyCode::KeyL => a.local_status.contains("inactive"),
            _ => unreachable!(),
        };
        let cancelled = token.load(Ordering::Relaxed);
        release.send(()).unwrap();
        drain(&mut a);
        assert!(transitioned, "actual input did not reach {action:?}");
        assert!(!cancelled, "{action:?} changed the collect cancel flag");
        committed_and_usable(&mut a, &cache, &source);
    }
}
#[test]
fn r1_actual_folder_paging_preserves_captured_source_and_refuses_cache_adoption() {
    let (f, mut a, cache, source) = fixture("lifecycle-pages");
    for n in 0..hair::folder_scene::PAGE_SIZE {
        std::fs::write(source.join(format!("z-{n}.txt")), b"not imported").unwrap();
    }
    tap(&mut a, KeyCode::F5);
    aim(&mut a);
    let (_arrived, release) = staged(&mut a);
    let token = a.documents_cancel_token();
    let other = f.directory("other-cache");
    assert!(a.restore_documents_at(&other).is_err());
    let adoption_cancelled = token.load(Ordering::Relaxed);
    tap(&mut a, KeyCode::PageDown);
    let page = a.local.as_ref().unwrap().page;
    let cancelled = token.load(Ordering::Relaxed);
    release.send(()).unwrap();
    drain(&mut a);
    assert!(!adoption_cancelled, "refused adoption must not cancel");
    assert!(!cancelled, "page change must not cancel held-source work");
    assert_eq!(page, 1);
    assert_eq!(a.inventory.items()[0].name, "Note.txt");
    assert_eq!(a.storage.selected, None, "new page must not inherit old UI intent");
    assert!(std::fs::read_dir(other).unwrap().next().is_none());
    tap(&mut a, KeyCode::PageUp);
    committed_and_usable(&mut a, &cache, &source);
}
#[test]
fn r2_shutdown_waits_for_pre_activation_cancellation_and_returns_held_store() {
    let (_f, mut a, cache, source) = fixture("lifecycle-shutdown-before");
    let (arrived, release) = gate(&mut a, false);
    tap(&mut a, KeyCode::KeyX); entered(&arrived);
    let token = a.documents_cancel_token();
    let releaser = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !token.load(Ordering::Relaxed) {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        release.send(()).unwrap();
    });
    let settled = a.shutdown_documents(Duration::from_secs(5));
    releaser.join().unwrap();
    let retained = a.inventory.store().is_some();
    let status = a.local_status.clone();
    drain(&mut a);
    assert!(settled && retained, "post-loop helper failed to settle: {status}");
    assert!(status.contains("Not committed"), "{status}");
    assert!(a.inventory.items().is_empty());
    assert!(!cache.join("payload.pending").exists());
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"held original unchanged");
}
#[test]
fn r2_shutdown_reconciles_completed_unpolled_commit_instead_of_detaching() {
    let (_f, mut a, cache, source) = fixture("lifecycle-shutdown-commit");
    let (arrived, release) = gate(&mut a, true);
    tap(&mut a, KeyCode::KeyX); entered(&arrived);
    release.send(()).unwrap(); seams::finish_unpolled(&a);
    let settled = a.shutdown_documents(Duration::ZERO);
    let retained = a.inventory.store().is_some();
    let status = a.local_status.clone();
    drain(&mut a);
    assert!(settled && retained, "completed worker/Store was not adopted: {status}");
    assert!(status.contains("COMMITTED"), "{status}");
    assert_eq!(a.documents_worker_count(), 0);
    assert_eq!(a.inventory.items().len(), 1);
    assert!(!cache.join("payload.pending").exists());
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"held original unchanged");
}
#[test]
fn r2_shutdown_stuck_chunk_is_bounded_unknown_then_late_result_reconciles() {
    let (_f, mut a, cache, source) = fixture("lifecycle-shutdown-timeout");
    let (_arrived, release) = staged(&mut a);
    let evidence = std::fs::read(cache.join("payload.pending")).unwrap();
    let start = Instant::now();
    assert!(!a.shutdown_documents(Duration::from_millis(10)));
    assert!(start.elapsed() < Duration::from_secs(1), "shutdown blocked on a gate");
    let status = a.local_status.clone();
    let retained = a.documents_worker_count();
    assert_eq!(std::fs::read(cache.join("payload.pending")).unwrap(), evidence);
    release.send(()).unwrap(); seams::finish_unpolled(&a);
    let settled = a.shutdown_documents(Duration::ZERO);
    drain(&mut a);
    assert!(status.contains("UNSETTLED") && status.contains("UNKNOWN") && status.contains("evidence"), "{status}");
    assert_eq!(retained, 1, "timeout must retain the worker owner until actual process exit");
    assert!(settled, "late outcome was not reconciled");
    assert!(a.local_status.contains("Not committed"), "{}", a.local_status);
    assert!(a.inventory.store().is_some());
    assert!(a.inventory.items().is_empty());
    assert_eq!(std::fs::read(cache.join("payload.pending")).unwrap(), evidence);
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"held original unchanged");
}
