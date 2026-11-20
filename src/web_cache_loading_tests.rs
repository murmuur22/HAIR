//! File-only prepared regressions. Parent owns discovery, RED/GREEN and all execution.
use super::*;
use std::sync::Arc;
use web_native::{FeedbackPhase, Mode};
const EXACT: &str = "https://example.org/Cache?Case=A%2Fb#Tail";
fn tap(a: &mut App, code: KeyCode) {
    a.keyboard_input(Some(code), true, false, None).unwrap();
    a.keyboard_input(Some(code), false, false, None).unwrap();
}
fn version_one(a: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    c.check(d)?;
    Page::html(
        a,
        b"<title>Stable</title><p>Version one</p><a href='/other'>Other</a>",
    )
}
fn version_two(a: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    assert_eq!(
        a.original, EXACT,
        "refresh must fetch the exact original URL"
    );
    c.check(d)?;
    Page::html(
        a,
        b"<title>Stable</title><p>Version two</p><a href='/other'>Other</a>",
    )
}
fn no_fetch(_: Address, _: &hair::acquisition::Cancellation, _: Instant) -> Result<Page> {
    panic!("VISITED_CACHE_NO_FETCH: exact visited URL must use held source")
}
fn enter() -> App {
    let mut a = app();
    a.web.as_mut().unwrap().fetcher = version_one;
    a.request_web(EXACT, Mode::Initial).unwrap();
    finish(&mut a);
    a
}
fn source(a: &App) -> Arc<hair::web::HtmlSession> {
    a.web
        .as_ref()
        .unwrap()
        .page
        .as_ref()
        .unwrap()
        .source
        .as_ref()
        .unwrap()
        .clone()
}
fn wait_completed_unpolled(a: &App) {
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
fn repeated_exact_url_is_one_async_cached_job_and_current_stays_stable() {
    let mut a = enter();
    let held = source(&a);
    let pose = a.pose;
    let pack = a.themes[0].source.clone();
    a.world.reading = true;
    a.scroll = 3;
    a.web.as_mut().unwrap().fetcher = no_fetch;
    a.request_web(EXACT, Mode::Follow).unwrap();
    assert!(a.web.as_ref().unwrap().pending.is_some());
    assert_eq!(a.web_feedback(Instant::now()).phase, FeedbackPhase::Pending);
    assert!(a.web_feedback(Instant::now()).cached);
    assert!(
        a.request_web(EXACT, Mode::Follow).is_err(),
        "one occupied job only"
    );
    assert!(Arc::ptr_eq(&held, &source(&a)));
    assert_eq!(a.pose.position, pose.position);
    assert!(a.world.reading);
    finish(&mut a);
    assert!(Arc::ptr_eq(&held, &source(&a)));
    assert!(
        !a.world.reading,
        "Follow is an arrival, not same-ID short circuit"
    );
    assert_eq!(a.scroll, 0);
    assert_eq!(a.themes[0].source, pack);
    assert!(a.web.as_ref().unwrap().status.contains("Cached"));
    assert_eq!(a.web_feedback(Instant::now()).phase, FeedbackPhase::Success);
    tap(&mut a, KeyCode::KeyR);
    assert!(a.world.reading);
    assert_eq!(a.scroll, 3);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.pose.yaw, pose.yaw);
    assert_eq!(a.pose.pitch, pose.pitch);
}
#[test]
fn cache_exact_query_case_and_fragment_identity_have_independent_sources() {
    let mut a = enter();
    let original = source(&a);
    for distinct in [
        "https://example.org/cache?Case=A%2Fb#Tail",
        "https://example.org/Cache?Case=a%2Fb#Tail",
        "https://example.org/Cache?Case=A%2Fb#tail",
    ] {
        a.request_web(distinct, Mode::Follow).unwrap();
        assert!(!a.web_feedback(Instant::now()).cached);
        finish(&mut a);
        assert_eq!(a.world.current, distinct);
        assert!(!Arc::ptr_eq(&original, &source(&a)));
        tap(&mut a, KeyCode::KeyR);
    }
    a.web.as_mut().unwrap().fetcher = no_fetch;
    a.request_web(EXACT, Mode::Follow).unwrap();
    finish(&mut a);
    assert!(Arc::ptr_eq(&original, &source(&a)));
}
#[test]
fn refresh_fetch_bypasses_cache_replaces_version_and_failure_retains_good_source() {
    let mut a = enter();
    let old = source(&a);
    a.web.as_mut().unwrap().fetcher = version_two;
    tap(&mut a, KeyCode::F5);
    assert!(!a.web_feedback(Instant::now()).cached);
    finish(&mut a);
    let good = source(&a);
    assert!(!Arc::ptr_eq(&old, &good));
    assert!(a.world.document().unwrap().contains("Version two"));
    a.web.as_mut().unwrap().fetcher = |address, _, _| {
        assert_eq!(address.original, EXACT);
        Err("owned refresh refusal".into())
    };
    let pose = a.pose;
    let pack = a.themes[0].source.clone();
    tap(&mut a, KeyCode::F5);
    finish(&mut a);
    assert!(Arc::ptr_eq(&good, &source(&a)));
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.themes[0].source, pack);
    let now = Instant::now();
    let feedback = a.web_feedback(now);
    assert_eq!(feedback.phase, FeedbackPhase::Failure);
    assert_eq!(feedback.destination, EXACT);
    assert!(feedback.red_eligible);
    assert!(feedback.reason.unwrap().contains("owned refresh refusal"));
    let settled = a.web_feedback(now + web_native::FAILURE_RED_DURATION + Duration::from_secs(1));
    assert!(!settled.red_eligible);
    assert!(settled.reason.unwrap().contains("owned refresh refusal"));
    assert!(
        a.web
            .as_ref()
            .unwrap()
            .status
            .contains("owned refresh refusal")
    );
    a.web.as_mut().unwrap().fetcher = no_fetch;
    a.request_web(EXACT, Mode::Follow).unwrap();
    finish(&mut a);
    assert!(Arc::ptr_eq(&good, &source(&a)));
    assert!(a.world.document().unwrap().contains("Version two"));
}
#[test]
fn completed_unpolled_cancellation_does_not_admit_source_or_turn_red() {
    let mut a = enter();
    let held = source(&a);
    let current = a.world.current.clone();
    let pose = a.pose;
    let count = a.web.as_ref().unwrap().cache.accounting().entries;
    a.request_web("https://example.org/Cancelled", Mode::Follow)
        .unwrap();
    wait_completed_unpolled(&a);
    tap(&mut a, KeyCode::KeyR);
    let feedback = a.web_feedback(Instant::now());
    assert_eq!(feedback.phase, FeedbackPhase::Cancelled);
    assert!(feedback.retiring);
    assert!(!feedback.red_eligible);
    assert!(a.request_web(EXACT, Mode::Follow).is_err());
    finish(&mut a);
    assert_eq!(
        a.web_feedback(Instant::now()).phase,
        FeedbackPhase::Cancelled
    );
    assert!(!a.web_feedback(Instant::now()).red_eligible);
    assert_eq!(a.web.as_ref().unwrap().cache.accounting().entries, count);
    assert_eq!(a.world.current, current);
    assert_eq!(a.pose.position, pose.position);
    assert!(Arc::ptr_eq(&held, &source(&a)));
    a.request_web("https://example.org/Cancelled", Mode::Follow)
        .unwrap();
    assert!(
        !a.web_feedback(Instant::now()).cached,
        "cancelled result must not populate cache"
    );
    finish(&mut a);
}
#[test]
fn stale_completed_result_cannot_replace_cache_or_current_page() {
    let mut a = enter();
    let held = source(&a);
    a.web.as_mut().unwrap().fetcher = version_two;
    a.request_web(EXACT, Mode::Refresh).unwrap();
    wait_completed_unpolled(&a);
    a.web.as_mut().unwrap().invalidate_generation_for_test();
    finish(&mut a);
    assert!(Arc::ptr_eq(&held, &source(&a)));
    a.web.as_mut().unwrap().fetcher = no_fetch;
    a.request_web(EXACT, Mode::Follow).unwrap();
    finish(&mut a);
    assert!(Arc::ptr_eq(&held, &source(&a)));
}
#[test]
fn cache_first_room_policy_does_not_change_exact_back_departure_window() {
    let mut a = app();
    a.web.as_mut().unwrap().fetcher = |address, _, _| {
        let html = format!("<p>Start</p>{}", "<a href='/other'>Link</a>".repeat(65));
        Page::html(address, html.as_bytes())
    };
    a.request_web(EXACT, Mode::Initial).unwrap();
    finish(&mut a);
    let held = source(&a);
    let next = a.web.as_ref().unwrap().window_doors[0].action.clone();
    a.request_window(next).unwrap();
    finish(&mut a);
    assert_eq!(
        a.web
            .as_ref()
            .unwrap()
            .page
            .as_ref()
            .unwrap()
            .window
            .as_ref()
            .unwrap()
            .index(),
        1
    );
    let pose = a.pose;
    a.web.as_mut().unwrap().fetcher = no_fetch;
    a.request_web(EXACT, Mode::Follow).unwrap();
    finish(&mut a);
    assert_eq!(
        a.web
            .as_ref()
            .unwrap()
            .page
            .as_ref()
            .unwrap()
            .window
            .as_ref()
            .unwrap()
            .index(),
        0
    );
    assert!(Arc::ptr_eq(&held, &source(&a)));
    assert_eq!(a.web.as_ref().unwrap().cache.accounting().entries, 1);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(
        a.web
            .as_ref()
            .unwrap()
            .page
            .as_ref()
            .unwrap()
            .window
            .as_ref()
            .unwrap()
            .index(),
        1
    );
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.pose.yaw, pose.yaw);
    assert_eq!(a.pose.pitch, pose.pitch);
}
#[test]
fn finite_cache_count_charges_eviction_and_oversize_drop_do_not_refuse_current() {
    use web_native::visited::{MAX_VISITED_BYTES, MAX_VISITED_PAGES, VisitedCache};
    let mut cache = VisitedCache::default();
    for i in 0..MAX_VISITED_PAGES + 2 {
        let page = Page::html(
            Address::parse(&format!("https://example.org/{i}")).unwrap(),
            b"<p>Small</p>",
        )
        .unwrap();
        assert!(cache.admit(&page));
        let stats = cache.accounting();
        assert!(stats.entries <= MAX_VISITED_PAGES);
        assert!(stats.logical_bytes <= MAX_VISITED_BYTES);
        assert!(stats.source_bytes > 0 && stats.page_bytes > 0);
        assert_eq!(stats.logical_bytes, stats.source_bytes + stats.page_bytes);
    }
    assert!(cache.get("https://example.org/0").is_none());
    let key = format!("https://example.org/{}", MAX_VISITED_PAGES + 1);
    let page = cache.get(&key).unwrap();
    let before = cache.accounting();
    assert!(cache.admit(&page));
    assert_eq!(
        cache.accounting(),
        before,
        "repeated same snapshot must not add charge"
    );
    let mut too_big = (*page).clone();
    too_big.text = "x".repeat(MAX_VISITED_BYTES);
    assert!(!cache.admit(&too_big));
    assert!(
        cache.get(&key).is_none(),
        "successful new version too big must drop obsolete same-key cache"
    );
    let mut a = enter();
    // A valid bounded response can exceed this conservative cache-only byte cap.
    a.web.as_mut().unwrap().fetcher = |address, _, _| {
        let comments = "<!--bounded inert padding-->".repeat(MAX_VISITED_BYTES / 20);
        let html = format!("{comments}<p>Valid active page</p>");
        Page::html(address, html.as_bytes())
    };
    a.request_web(EXACT, Mode::Refresh).unwrap();
    finish(&mut a);
    assert!(a.world.document().unwrap().contains("Valid active page"));
    assert_eq!(a.web_feedback(Instant::now()).phase, FeedbackPhase::Success);
    assert!(a.web.as_mut().unwrap().cache.get(EXACT).is_none());
}
#[test]
fn cache_byte_admission_evicts_before_the_count_limit() {
    use web_native::visited::{MAX_VISITED_BYTES, VisitedCache};
    let mut cache = VisitedCache::default();
    let padding = "<!--bounded inert padding-->".repeat(3000);
    for i in 0..2 {
        let page = Page::html(
            Address::parse(&format!("https://example.org/byte-{i}")).unwrap(),
            format!("{padding}<p>Small projection</p>").as_bytes(),
        )
        .unwrap();
        assert!(cache.admit(&page));
        assert_eq!(
            cache.accounting().entries,
            1,
            "byte cap must evict before four-entry count cap"
        );
        assert!(cache.accounting().logical_bytes <= MAX_VISITED_BYTES);
    }
    assert!(cache.get("https://example.org/byte-0").is_none());
    assert!(cache.get("https://example.org/byte-1").is_some());
}
#[test]
fn initial_failure_feedback_survives_restoring_bundled_interaction() {
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
    a.web = Some(web_native::Runtime::new(Vec::new(), |_, _, _| {
        Err("initial owned refusal".into())
    }));
    a.request_web(EXACT, Mode::Initial).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while a.web.as_ref().is_some_and(|w| w.pending.is_some()) {
        a.poll_web().unwrap();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(a.web.is_none());
    let feedback = a.web_feedback(Instant::now());
    assert_eq!(feedback.phase, FeedbackPhase::Failure);
    assert!(feedback.red_eligible);
    assert!(feedback.reason.unwrap().contains("initial owned refusal"));
}
