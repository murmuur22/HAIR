//! Prepared only; no observed RED/GREEN. Real worker/feedback, no live requests.
use super::*;
use hair::render::web_feedback::{MAX_VERTICES, Mode as VisualMode};
use web_native::{FeedbackPhase, Mode};
#[cfg(target_os = "macos")]
#[path = "web_vortex_capture_tests.rs"]
mod metal;
fn tap(a: &mut App, key: KeyCode) {
    a.keyboard_input(Some(key), true, false, None).unwrap();
    a.keyboard_input(Some(key), false, false, None).unwrap();
}
fn completed(a: &App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !a
        .web
        .as_ref()
        .unwrap()
        .pending
        .as_ref()
        .unwrap()
        .is_finished()
    {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
#[test]
fn vortex_real_pending_cancel_and_ready_have_no_visual_arrival_gate() {
    let mut a = app();
    let original = a.world.current.clone();
    let pose = a.pose;
    let pack = a.themes[0].source.clone();
    a.world.reading = true;
    a.scroll = 3;
    let destination = "https://example.org/Case?q=A%2Fb#Tail";
    a.request_web(destination, Mode::Follow).unwrap();
    completed(&a);
    let view = a.web_visual_at(Instant::now());
    assert_eq!(view.mode, VisualMode::Pending);
    assert_eq!(view.destination, destination);
    assert_eq!(view.status(), "Loading…");
    assert!(view.controls().contains("R cancel"));
    assert_eq!(a.world.current, original);
    assert!(a.world.reading);
    assert_eq!(a.scroll, 3);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.themes[0].source, pack);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(a.web_visual_at(Instant::now()).mode, VisualMode::Cancelled);
    finish(&mut a);
    assert_eq!(a.world.current, original);
    assert!(a.world.reading);
    a.request_web(destination, Mode::Follow).unwrap();
    completed(&a);
    a.poll_web().unwrap(); // one ordinary poll, no animation duration check
    assert_eq!(a.world.current, destination);
    assert_eq!(a.web_feedback(Instant::now()).phase, FeedbackPhase::Success);
    assert_eq!(a.web_visual_at(Instant::now()).mode, VisualMode::Hidden);
}
#[test]
fn vortex_failure_red_expires_without_clearing_reason_pose_reader_or_source() {
    let mut a = app();
    let initial = a.world.current.clone();
    a.request_web(&initial, Mode::Initial).unwrap();
    finish(&mut a);
    let source = a
        .web
        .as_ref()
        .unwrap()
        .page
        .as_ref()
        .unwrap()
        .source
        .as_ref()
        .unwrap()
        .clone();
    let current = a.world.current.clone();
    let pose = a.pose;
    let document = a.world.document().unwrap().to_string();
    a.world.reading = true;
    a.scroll = 2;
    a.request_web("https://example.org/bad", Mode::Follow)
        .unwrap();
    finish(&mut a);
    let now = Instant::now();
    let red = a.web_visual_at(now);
    assert_eq!(red.mode, VisualMode::FailureRed);
    let settled = a.web_visual_at(now + web_native::FAILURE_RED_DURATION);
    assert_eq!(settled.mode, VisualMode::FailureSettled);
    assert_eq!(settled.reason, red.reason);
    assert!(settled.reason.contains("fixture failed refresh"));
    assert_eq!(a.world.current, current);
    assert!(Arc::ptr_eq(
        &source,
        a.web
            .as_ref()
            .unwrap()
            .page
            .as_ref()
            .unwrap()
            .source
            .as_ref()
            .unwrap()
    ));
    assert_eq!(a.world.document().unwrap(), document);
    assert!(a.world.reading);
    assert_eq!(a.scroll, 2);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.pose.yaw, pose.yaw);
    assert_eq!(a.pose.pitch, pose.pitch);
}
#[test]
fn vortex_initial_failure_survives_runtime_removal_and_context_overlay_is_finite() {
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
    a.web = Some(web_native::Runtime::new(Vec::new(), fixture));
    a.request_web("https://example.org/bad", Mode::Initial)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while a.web.as_ref().is_some_and(|w| w.pending.is_some()) {
        a.poll_web().unwrap();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(a.web.is_none());
    assert!(a.address.is_some()); // existing focus-aware recovery is unchanged
    let view = a.web_visual_at(Instant::now());
    assert_eq!(view.mode, VisualMode::FailureRed);
    for size in [[960, 640], [640, 480]] {
        let mut vertices = Vec::new();
        view.paint(&mut vertices, size, true);
        assert!(vertices.len() <= MAX_VERTICES);
        assert!(vertices.iter().all(|v| {
            v.position
                .iter()
                .all(|x| x.is_finite() && (-1.0..=1.0).contains(x))
        }));
    }
}
#[test]
fn vortex_disclosure_scroll_does_not_rebind_destination_or_consume_movement_and_m() {
    let mut a = app();
    let destination = format!("https://example.org/{}?q=A%2Fb#ExactTail", "aB".repeat(980));
    a.request_web(&destination, Mode::Follow).unwrap();
    completed(&a);
    tap(&mut a, KeyCode::F8);
    assert!(a.web_details);
    let mut reached_tail = false;
    for _ in 0..120 {
        tap(&mut a, KeyCode::ArrowDown);
        reached_tail |= a
            .web_visual_at(Instant::now())
            .visible_rows([640, 480])
            .join("")
            .contains("#ExactTail");
    }
    assert!(
        reached_tail,
        "exact URL tail must be reachable through the actual arrow adapter"
    );
    assert_eq!(a.web_feedback(Instant::now()).destination, destination);
    a.keyboard_input(Some(KeyCode::KeyW), true, false, None)
        .unwrap();
    assert!(a.input.forward);
    let collapsed = a.world.tree_collapsed;
    let mode = a.map_mode;
    tap(&mut a, KeyCode::KeyM);
    assert_ne!(
        a.map_mode, mode,
        "attempt details must not consume world-map M"
    );
    assert_eq!(
        a.world.tree_collapsed, collapsed,
        "M no longer toggles optional links"
    );
    a.key(KeyCode::ShiftLeft, true, false).unwrap();
    tap(&mut a, KeyCode::KeyM);
    a.key(KeyCode::ShiftLeft, false, false).unwrap();
    assert_eq!(a.world.tree_collapsed, collapsed);
    assert!(!a.minimap_enabled);
    tap(&mut a, KeyCode::Escape); // modal map owns R until explicitly closed
    tap(&mut a, KeyCode::KeyR);
    finish(&mut a);
    // Address and reader contexts never steal their arrows for disclosure.
    a.world.reading = true;
    a.scroll = 0;
    tap(&mut a, KeyCode::ArrowDown);
    assert_eq!(a.scroll, 1);
}
#[test]
fn vortex_is_global_attempt_not_a_rebound_door_and_cache_ready_is_immediate() {
    let mut a = app();
    let destination = "https://example.org/CacheReady";
    a.request_web(destination, Mode::Follow).unwrap();
    finish(&mut a);
    a.web.as_mut().unwrap().fetcher = |_, _, _| panic!("cached follow fetched");
    a.request_web(destination, Mode::Follow).unwrap();
    completed(&a);
    a.world.interaction = None; // feedback cannot follow a new slot/aim binding
    let view = a.web_visual_at(Instant::now());
    assert!(view.cached);
    assert_eq!(view.destination, destination);
    a.poll_web().unwrap();
    assert_eq!(a.world.current, destination);
    assert_eq!(a.web_visual_at(Instant::now()).mode, VisualMode::Hidden);
}

#[test]
fn vortex_delayed_work_keeps_input_and_bounded_frame_preparation_available() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static WORKING: AtomicBool = AtomicBool::new(false);
    fn delayed(
        _address: Address,
        cancel: &hair::acquisition::Cancellation,
        deadline: Instant,
    ) -> Result<Page> {
        WORKING.store(true, Ordering::SeqCst);
        loop {
            cancel.check(deadline)?;
            std::thread::yield_now();
            // This owned fixture deliberately waits for cancellation; no socket,
            // forced success, detached thread or fake completion is involved.
        }
    }
    WORKING.store(false, Ordering::SeqCst);
    let mut a = app();
    let current = a.world.current.clone();
    a.web.as_mut().unwrap().fetcher = delayed;
    a.request_web("https://example.org/Delayed", Mode::Follow)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !WORKING.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    a.keyboard_input(Some(KeyCode::KeyW), true, false, None)
        .unwrap();
    assert!(a.input.forward);
    let mut vertices = Vec::with_capacity(MAX_VERTICES);
    for _ in 0..8 {
        let view = a.web_visual_at(Instant::now());
        assert_eq!(view.mode, VisualMode::Pending);
        view.paint(&mut vertices, [640, 480], false);
        assert!(vertices.len() <= MAX_VERTICES);
        a.poll_web().unwrap();
        assert_eq!(a.world.current, current);
    }
    tap(&mut a, KeyCode::KeyR);
    finish(&mut a);
    assert_eq!(a.web_visual_at(Instant::now()).mode, VisualMode::Cancelled);
    assert_eq!(a.world.current, current);
}

#[test]
fn vortex_disclosure_latch_focus_and_address_reader_contexts_preserve_controls() {
    let mut a = app();
    a.request_web("https://example.org/bad", Mode::Follow)
        .unwrap();
    finish(&mut a);
    a.key(KeyCode::F8, true, false).unwrap();
    assert!(a.web_details);
    a.key(KeyCode::F8, true, true).unwrap();
    a.key(KeyCode::F8, true, false).unwrap();
    assert!(a.web_details);
    a.focus_changed(false);
    a.focus_changed(true);
    tap(&mut a, KeyCode::F8);
    assert!(!a.web_details);
    tap(&mut a, KeyCode::KeyL);
    tap(&mut a, KeyCode::F8);
    assert!(!a.web_details);
    tap(&mut a, KeyCode::KeyR);
    assert!(a.address.is_some());
    tap(&mut a, KeyCode::Escape);
    a.world.reading = true;
    tap(&mut a, KeyCode::F8);
    assert!(!a.web_details);
    tap(&mut a, KeyCode::ArrowDown);
    assert_eq!(a.scroll, 1);
}
