//! Barriers inside real backend chunks and completed-but-unpolled admission.
use super::*;
pub(super) fn finish_unpolled(a: &App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !a.documents_worker_finished() {
        assert!(Instant::now() < deadline, "worker did not finish without polling");
        std::thread::yield_now();
    }
}
fn body_gate(a: &mut App) -> (Receiver<()>, Sender<()>) {
    let pair = gate(a, false);
    a.storage.worker_gate.as_mut().unwrap().body = true;
    pair
}
#[test]
fn cancellation_at_real_import_chunk_preserves_original_and_pending_evidence() {
    for focus in [false, true] {
        let (_f, mut a, cache, source) = fixture(if focus { "async-body-focus" } else { "async-body-esc" });
        let manifest = std::fs::read(cache.join("manifest")).unwrap();
        let (arrived, release) = body_gate(&mut a);
        tap(&mut a, KeyCode::KeyX);
        entered(&arrived);
        let evidence = std::fs::read(cache.join("payload.pending")).unwrap();
        assert_eq!(evidence, b"held original unchanged");
        a.poll_documents();
        if focus { a.focus_changed(false); } else { tap(&mut a, KeyCode::Escape); }
        release.send(()).unwrap();
        pump_until(&mut a, |a| !a.inventory.is_busy());
        assert!(a.local_status.contains("Not committed"), "{}", a.local_status);
        assert!(a.inventory.items().is_empty());
        assert_eq!(std::fs::read(cache.join("manifest")).unwrap(), manifest);
        assert_eq!(std::fs::read(cache.join("payload.pending")).unwrap(), evidence);
        assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"held original unchanged");
        a.focus_changed(true); a.menu = None;
        aim(&mut a); tap(&mut a, KeyCode::KeyX);
        pump_until(&mut a, |a| !a.inventory.is_busy());
        assert!(a.local_status.contains("evidence"), "retry must reconcile/refuse without cleanup: {}", a.local_status);
        assert_eq!(std::fs::read(cache.join("payload.pending")).unwrap(), evidence);
        assert_eq!(std::fs::read(cache.join("manifest")).unwrap(), manifest);
    }
}
#[test]
fn selected_body_read_focus_cancel_and_unready_placement_are_responsive() {
    let (_f, mut a, _cache, _source) = fixture("async-preview-body");
    tap(&mut a, KeyCode::KeyX); drain(&mut a);
    let id = a.storage.selected.unwrap();
    let (arrived, release) = body_gate(&mut a);
    tap(&mut a, KeyCode::KeyB); entered(&arrived);
    tap(&mut a, KeyCode::KeyP);
    assert!(a.placement_action.is_none());
    assert!(a.inventory_overlay_for([640, 480]).is_some());
    a.poll_documents();
    a.focus_changed(false);
    assert!(a.local_status.contains("Cancelling"));
    release.send(()).unwrap();
    pump_until(&mut a, |a| !a.inventory.is_busy());
    assert!(a.inventory.preview(id).is_none());
    assert!(a.placement_action.is_none());
    assert_eq!(a.inventory.items()[0].id, id);
}
#[test]
fn superseded_preview_installs_only_latest_id_with_one_worker() {
    let (_f, mut a, _cache, _source) = fixture("async-latest");
    tap(&mut a, KeyCode::KeyX); drain(&mut a);
    tap(&mut a, KeyCode::KeyX); drain(&mut a);
    let first = a.inventory.items()[0].id;
    let second = a.inventory.items()[1].id;
    let (arrived, release) = gate(&mut a, true);
    tap(&mut a, KeyCode::KeyB); entered(&arrived);
    tap(&mut a, KeyCode::ArrowUp);
    assert_eq!(a.storage.selected, Some(first));
    assert_eq!(a.documents_worker_count(), 1);
    release.send(()).unwrap(); drain(&mut a);
    assert_eq!(a.storage.selected, Some(first));
    assert!(a.inventory.preview(first).is_some());
    assert!(a.inventory.preview(second).is_none());
    assert_eq!(a.documents_worker_count(), 0);
}
#[test]
fn completed_unpolled_preview_peer_removal_is_not_admitted_or_rebound() {
    let (_f, mut a, cache, source) = fixture("async-preview-removed");
    tap(&mut a, KeyCode::KeyX); drain(&mut a);
    let id = a.storage.selected.unwrap();
    let (arrived, release) = gate(&mut a, true);
    tap(&mut a, KeyCode::KeyB); entered(&arrived);
    release.send(()).unwrap(); finish_unpolled(&a);
    let mut peer = hair::collection_store::Store::open(&cache, a.storage.preferences.policy().unwrap()).unwrap();
    assert!(peer.remove(id).unwrap().is_complete());
    a.poll_documents();
    assert!(a.inventory.items().is_empty());
    assert!(a.inventory.preview(id).is_none());
    assert_eq!(a.storage.selected, None);
    assert!(a.inventory.items().get(a.inventory_selection).is_none());
    tap(&mut a, KeyCode::KeyP);
    assert!(a.placement_action.is_none());
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"held original unchanged");
}
#[test]
fn completed_collect_navigation_keeps_old_commit_without_selecting_new_source() {
    let (f, mut a, cache, source) = fixture("async-navigation");
    let other = f.directory("other");
    std::fs::write(other.join("Other.txt"), b"never imported").unwrap();
    let (arrived, release) = gate(&mut a, true);
    tap(&mut a, KeyCode::KeyX); entered(&arrived);
    release.send(()).unwrap(); finish_unpolled(&a);
    a.complete_folder(Ok(Some(other))).unwrap();
    a.poll_documents();
    assert_eq!(a.storage.selected, None);
    assert_eq!(a.inventory.items()[0].name, "Note.txt");
    assert!(a.local_status.contains("COMMITTED"), "{}", a.local_status);
    assert!(a.storage_lines().iter().any(|line| line.contains("COMMITTED") && line.contains(cache.to_str().unwrap())));
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"held original unchanged");
}
