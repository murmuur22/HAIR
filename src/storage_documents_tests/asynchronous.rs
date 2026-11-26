//! Real keyboard/focus entrypoints and production polling; no synchronous test fork.
mod seams;
mod lifecycle;
mod lifecycle_edges;
use super::*;
use std::sync::mpsc::{self, Receiver, Sender};
fn tap(a: &mut App, key: KeyCode) {
    assert!(!a.keyboard_input(Some(key), true, false, None).unwrap());
    assert!(!a.keyboard_input(Some(key), false, false, None).unwrap());
}
fn gate(a: &mut App, after: bool) -> (Receiver<()>, Sender<()>) {
    let (entered, arrived) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    a.storage.worker_gate = Some(storage_native::WorkerGate { entered, release: wait, after, body: false, panic_on_return: false });
    (arrived, release)
}
fn entered(arrived: &Receiver<()>) {
    arrived.recv_timeout(Duration::from_secs(5)).expect("App input must schedule a worker at the backend operation seam");
}
fn fixture(label: &str) -> (Fixture, App, PathBuf, PathBuf) {
    let f = Fixture::new(label);
    let cache = f.directory("cache");
    let source = f.directory("source");
    std::fs::write(source.join("Note.txt"), b"held original unchanged").unwrap();
    let mut a = app();
    a.restore_documents_at(&cache).unwrap();
    a.complete_folder(Ok(Some(source.clone()))).unwrap();
    aim(&mut a);
    (f, a, cache, source)
}
fn pump_until(a: &mut App, mut predicate: impl FnMut(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate(a) {
        assert!(Instant::now() < deadline, "worker did not retire: {}", a.local_status);
        a.poll_documents();
        std::thread::yield_now();
    }
}
#[test]
fn collect_returns_gated_actual_escape_and_focus_cancel_precommit() {
    for focus in [false, true] {
        let (_f, mut a, cache, source) = fixture(if focus { "async-focus" } else { "async-escape" });
        let manifest = std::fs::read(cache.join("manifest")).unwrap();
        let (arrived, release) = gate(&mut a, false);
        tap(&mut a, KeyCode::KeyX);
        entered(&arrived);
        assert!(a.inventory.is_persistent(), "busy must never be session-only");
        assert!(a.inventory.items().is_empty());
        assert!(a.inventory_overlay_for([640, 480]).is_none());
        a.poll_documents(); // Must not join the gated worker.
        a.input.captured = true;
        tap(&mut a, KeyCode::KeyW);
        a.advance_motion(0.01);
        if focus { a.focus_changed(false); } else { tap(&mut a, KeyCode::Escape); }
        assert!(a.local_status.contains("Cancelling"), "{}", a.local_status);
        assert_eq!(std::fs::read(cache.join("manifest")).unwrap(), manifest);
        release.send(()).unwrap();
        pump_until(&mut a, |a| a.local_status.contains("Not committed"));
        assert!(a.inventory.items().is_empty());
        assert_eq!(std::fs::read(cache.join("manifest")).unwrap(), manifest);
        assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"held original unchanged");
    }
}
#[test]
fn committed_before_cancel_unpolled_is_pinned_and_old_cache_cannot_adopt() {
    let (f, mut a, cache, source) = fixture("async-committed");
    let next = f.directory("next");
    let (arrived, release) = gate(&mut a, true);
    tap(&mut a, KeyCode::KeyX);
    entered(&arrived); // Backend has committed, App has not polled.
    release.send(()).unwrap();
    seams::finish_unpolled(&a);
    tap(&mut a, KeyCode::Escape);
    assert!(a.restore_documents_at(&next).is_err(), "busy cache adoption must fail closed");
    assert!(std::fs::read_dir(&next).unwrap().next().is_none());
    pump_until(&mut a, |a| a.local_status.contains("COMMITTED"));
    assert_eq!(a.inventory.items().len(), 1, "late cancellation cannot discard commit");
    let id = a.inventory.items()[0].id;
    assert_eq!(a.storage.selected, None, "cancelled UI intent must not select the new result");
    assert!(a.inventory.preview(id).is_none());
    assert!(a.local_status.contains("original unchanged"));
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), b"held original unchanged");
    drop(a);
    let reopened = hair::inventory::Inventory::persistent(&cache, hair::collection_store::Policy::new(1024, 4096).unwrap()).unwrap();
    assert_eq!(reopened.items()[0].id, id);
}
#[test]
fn selected_preview_supersession_and_begin_placement_do_not_block_or_rebind() {
    let (f, mut a, _cache, _source) = fixture("async-preview");
    tap(&mut a, KeyCode::KeyX);
    pump_until(&mut a, |a| !a.inventory.items().is_empty());
    // Settle the selected preview without replacing the production worker path.
    pump_until(&mut a, |a| a.inventory.preview(a.inventory.items()[0].id).is_some());
    tap(&mut a, KeyCode::KeyX);
    pump_until(&mut a, |a| a.inventory.items().len() == 2);
    let second = a.inventory.items()[1].id;
    pump_until(&mut a, |a| a.inventory.preview(second).is_some());
    let room = f.directory("room");
    a.complete_folder(Ok(Some(room))).unwrap();
    let (arrived, release) = gate(&mut a, false);
    tap(&mut a, KeyCode::KeyB); // Refresh invalidates old previews, schedules selected ID.
    entered(&arrived);
    tap(&mut a, KeyCode::KeyP); // Unready preview must not synchronously read/decode.
    assert!(a.placement_action.is_none());
    for _ in 0..20 {
        tap(&mut a, KeyCode::ArrowUp); tap(&mut a, KeyCode::ArrowDown);
        assert_eq!(a.documents_worker_count(), 1, "only one worker may retire per authority");
    }
    let selected = a.storage.selected.unwrap();
    a.poll_documents();
    assert!(a.inventory.preview(selected).is_none());
    tap(&mut a, KeyCode::Escape);
    release.send(()).unwrap();
    pump_until(&mut a, |a| a.local_status.contains("Preview cancelled"));
    assert!(a.inventory.preview(selected).is_none(), "disowned preview cannot install");
    assert!(a.placement_action.is_none());
}
