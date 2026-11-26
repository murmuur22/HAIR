//! Selection, inspection and worker-loss edges through production adapters.
use super::*;
use super::lifecycle::{staged, committed_and_usable};
use std::sync::atomic::Ordering;
#[test]
fn r1_actual_source_chooser_navigation_preserves_gated_held_source_job() {
    let (f, mut a, cache, source) = fixture("lifecycle-source-navigation");
    let other = f.directory("other-source");
    std::fs::write(other.join("Other.txt"), b"never imported").unwrap();
    let (_arrived, release) = staged(&mut a);
    let token = a.documents_cancel_token();
    tap(&mut a, KeyCode::KeyO);
    a.frames = 1; // Fixture claims the production chooser without native launch.
    assert!(a.claim_folder_request());
    a.complete_folder(Ok(Some(other.clone()))).unwrap();
    let cancelled = token.load(Ordering::Relaxed);
    release.send(()).unwrap(); drain(&mut a);
    assert!(!cancelled, "source navigation cancelled held-source work");
    assert_eq!(a.inventory.items()[0].name, "Note.txt");
    assert_eq!(a.storage.selected, None);
    assert!(a.storage_lines().iter().any(|line| line.contains("COMMITTED")));
    assert!(!cache.join("payload.pending").exists());
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"held original unchanged");
    assert_eq!(std::fs::read(other.join("Other.txt")).unwrap(), b"never imported");
    aim(&mut a); tap(&mut a, KeyCode::KeyX); drain(&mut a);
    assert_eq!(a.inventory.items().len(), 2);
    assert_eq!(a.inventory.items()[1].name, "Other.txt");
}
#[test]
fn r1_preview_selection_during_collect_never_cancels_and_latest_id_wins() {
    let (_f, mut a, cache, source) = fixture("lifecycle-queued-preview");
    tap(&mut a, KeyCode::KeyX); drain(&mut a);
    tap(&mut a, KeyCode::KeyX); drain(&mut a);
    let first = a.inventory.items()[0].id;
    let second = a.inventory.items()[1].id;
    let (_arrived, release) = staged(&mut a);
    let token = a.documents_cancel_token();
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::ArrowUp);
    tap(&mut a, KeyCode::ArrowDown);
    tap(&mut a, KeyCode::ArrowUp);
    let selected = a.storage.selected;
    let cancelled = token.load(Ordering::Relaxed);
    release.send(()).unwrap(); drain(&mut a);
    assert!(!cancelled);
    assert_eq!(selected, Some(first));
    assert_eq!(a.storage.selected, Some(first));
    assert!(a.inventory.preview(first).is_some());
    assert!(a.inventory.preview(second).is_none());
    assert_eq!(a.inventory.items().len(), 3);
    assert!(!cache.join("payload.pending").exists());
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"held original unchanged");
}
#[test]
fn r1_recovery_pointer_inspection_does_not_cancel_staged_collect() {
    let (_f, mut a, cache, source) = fixture("lifecycle-recovery-inspect");
    let (_arrived, release) = staged(&mut a);
    let token = a.documents_cancel_token();
    tap(&mut a, KeyCode::KeyP);
    let frame = a.inventory_frame();
    let (_, rect, _) = a.toolbox_controls().into_iter()
        .find(|(control, _, _)| *control == place_mode_native::Control::InspectRecovery).unwrap();
    a.pointer_moved(Some([
        f64::from((rect[0] + rect[2]) * 0.5) * f64::from(frame.physical[0]) / f64::from(frame.full[0]),
        f64::from((rect[1] + rect[3]) * 0.5) * f64::from(frame.physical[1]) / f64::from(frame.full[1]),
    ]));
    a.place_pointer(true); a.place_pointer(false);
    let inspected = a.recovery_open();
    let cancelled = token.load(Ordering::Relaxed);
    release.send(()).unwrap(); drain(&mut a);
    assert!(inspected, "actual pointer input failed to open recovery inspection");
    assert!(!cancelled);
    tap(&mut a, KeyCode::KeyP);
    committed_and_usable(&mut a, &cache, &source);
}
#[test]
fn r2_shutdown_timeout_after_backend_commit_later_adopts_exact_store() {
    let (_f, mut a, cache, _source) = fixture("lifecycle-shutdown-late-commit");
    let (arrived, release) = gate(&mut a, true);
    tap(&mut a, KeyCode::KeyX); entered(&arrived);
    assert!(!a.shutdown_documents(Duration::ZERO));
    assert!(a.local_status.contains("UNSETTLED") && a.local_status.contains("UNKNOWN"));
    assert_eq!(a.documents_worker_count(), 1);
    assert!(!cache.join("payload.pending").exists());
    release.send(()).unwrap(); seams::finish_unpolled(&a);
    assert!(a.shutdown_documents(Duration::ZERO));
    assert!(a.local_status.contains("COMMITTED"));
    assert!(a.inventory.store().is_some());
    assert_eq!(a.inventory.items().len(), 1);
    assert_eq!(a.documents_worker_count(), 0);
}
#[test]
fn r2_shutdown_join_panic_blocks_unknown_without_forever_busy_or_path_reopen() {
    let (f, mut a, cache, source) = fixture("lifecycle-shutdown-join-panic");
    let (arrived, release) = gate(&mut a, true);
    a.storage.worker_gate.as_mut().unwrap().panic_on_return = true;
    tap(&mut a, KeyCode::KeyX); entered(&arrived);
    release.send(()).unwrap(); seams::finish_unpolled(&a);
    assert!(!a.shutdown_documents(Duration::ZERO));
    assert!(a.local_status.contains("UNKNOWN") && a.local_status.contains("restart"), "{}", a.local_status);
    assert!(!a.inventory.is_busy());
    assert_eq!(a.documents_worker_count(), 0);
    assert!(a.inventory.is_persistent());
    assert!(a.inventory.has_lost_authority());
    assert!(a.inventory.refresh().is_err());
    let next = f.directory("next-cache");
    assert!(a.restore_documents_at(&next).unwrap_err().contains("restart"));
    assert!(std::fs::read_dir(next).unwrap().next().is_none());
    assert!(cache.join("manifest").exists());
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"held original unchanged");
}
