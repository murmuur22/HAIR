use super::*;
use hair::web::{Address, Page};
#[path = "web_app_continuation_tests.rs"]
mod app_continuation;
#[path = "web_cache_loading_tests.rs"]
mod cache_loading;
#[path = "web_context_app_tests.rs"]
mod context_app;
#[path = "exploration_app_tests.rs"]
mod exploration_app;
#[path = "exploration_capture_tests.rs"]
mod exploration_capture;
#[path = "exploration_composition_tests.rs"]
mod exploration_composition;
#[path = "web_grouped_tests.rs"]
mod grouped;
#[path = "web_landmark_tests.rs"]
mod landmarks;
#[path = "web_flow_tests.rs"]
mod live_flow;
#[path = "pocket_native_tests.rs"]
mod pocket;
#[path = "pocket_failure_tests.rs"]
mod pocket_failure;
#[path = "pocket_recovery_tests.rs"]
mod pocket_recovery;
#[path = "reader_context_capture_tests.rs"]
mod reader_context_capture;
#[path = "web_return_door_tests.rs"]
mod return_door;
#[path = "web_room_input_tests.rs"]
mod room_input;
#[path = "web_vortex_tests.rs"]
mod vortex;
fn fixture(a: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    c.check(d)?;
    if a.url.path() == "/bad" {
        return Err("fixture failed refresh".into());
    }
    Page::html(a, b"<title>Static</title><h1>Read me</h1><a href='/two?q=AB%2Fcd#part'>Next</a><a href='https://second.example/landing'>Second origin</a>")
}
fn app() -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let p = web_native::prepare(
        Address::parse("https://example.org/one").unwrap(),
        &[root.join("packs/office.toml")],
        fixture,
        &Default::default(),
    )
    .unwrap();
    let mut app = App::new(root, p.themes, 0, p.generated.world, false);
    app.web = Some(web_native::Runtime::new(p.generated.doors, fixture));
    app
}
fn finish(a: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while a.web.as_ref().unwrap().pending.is_some() {
        a.poll_web().unwrap();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
#[test]
fn selected_long_destination_tail_is_reachable_through_reader_keys() {
    let make = |tail: &str| {
        let mut a = app();
        let target = format!("https://example.org/{}?q={tail}", "aB".repeat(990));
        a.world = Page::html(
            Address::parse("https://example.org/Read").unwrap(),
            format!("<a href='{target}'>Next</a>").as_bytes(),
        )
        .unwrap()
        .generate()
        .unwrap()
        .world;
        a.world.reading = true;
        a.keyboard_input(Some(KeyCode::Tab), true, false, None)
            .unwrap();
        a.keyboard_input(Some(KeyCode::Tab), false, false, None)
            .unwrap();
        (a, target)
    };
    for (w, h) in [(960, 640), (640, 480)] {
        let (mut a, target) = make("aB%2Fcd#Case");
        let (mut b, _) = make("AB%2FCD#CASE");
        let body = |a: &App| {
            hair::text::overlay(&a.world, w, h, a.scroll)
                .into_iter()
                .filter(|v| v.color == [0.9, 0.96, 0.91])
                .collect::<Vec<_>>()
        };
        if w == 960 {
            assert!(
                bytemuck::cast_slice::<_, u8>(&body(&a))
                    == bytemuck::cast_slice::<_, u8>(&body(&b)),
                "2x reader tail should initially be below viewport"
            );
        }
        let mut reached = false;
        for _ in 0..40 {
            for a in [&mut a, &mut b] {
                a.keyboard_input(Some(KeyCode::ArrowDown), true, false, None)
                    .unwrap();
                a.keyboard_input(Some(KeyCode::ArrowDown), false, false, None)
                    .unwrap();
            }
            let vertices = body(&a);
            assert!(vertices.iter().all(|v| {
                v.position
                    .iter()
                    .all(|x| x.is_finite() && (-1.0..=1.0).contains(x))
            }));
            reached |= bytemuck::cast_slice::<_, u8>(&vertices)
                != bytemuck::cast_slice::<_, u8>(&body(&b));
        }
        assert!(
            reached,
            "selected destination tail unreachable through ArrowDown"
        );
        assert_eq!(
            a.world.selected_reader_link().unwrap().link.destination,
            hair::content::Destination::External(target.clone())
        );
        a.keyboard_input(Some(KeyCode::KeyE), true, false, None)
            .unwrap();
        a.keyboard_input(Some(KeyCode::KeyE), false, false, None)
            .unwrap();
        finish(&mut a);
        assert_eq!(
            a.world.current, target,
            "display/scroll must not change exact activation target"
        );
    }
}

#[test]
fn address_focus_consumes_world_keys_and_escape_does_not_quit() {
    let mut a = app();
    let current = a.world.current.clone();
    a.key(KeyCode::KeyL, true, false).unwrap();
    assert!(
        !a.key(KeyCode::KeyQ, true, false).unwrap(),
        "typing Q in address must not quit"
    );
    a.key(KeyCode::KeyW, true, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert!(!a.input.forward);
    assert!(a.world.reader_link.is_none());
    assert!(a.web.as_ref().unwrap().pending.is_none());
    assert_eq!(a.world.current, current);
    assert!(!a.key(KeyCode::Escape, true, false).unwrap());
}
#[test]
fn address_submit_uses_literal_text_and_retains_last_good_on_refusal() {
    let mut a = app();
    let source = a.themes[0].source.clone();
    a.world.reading = true;
    a.key(KeyCode::KeyL, true, false).unwrap();
    a.address_text("https://second.example/Path?q=AB%2Fcd#part");
    a.key(KeyCode::Enter, true, false).unwrap();
    finish(&mut a);
    assert_eq!(
        a.world.current,
        "https://second.example/Path?q=AB%2Fcd#part"
    );
    assert_eq!(a.themes[0].source, source);
    assert_eq!(a.web.as_ref().unwrap().history.len(), 1);
    a.key(KeyCode::KeyL, false, false).unwrap();
    a.key(KeyCode::KeyL, true, false).unwrap();
    a.address_text("https://EXAMPLE.org/a/../b");
    a.key(KeyCode::Enter, true, false).unwrap();
    assert!(a.address.is_some());
    assert!(a.web.as_ref().unwrap().pending.is_none());
    assert!(a.web.as_ref().unwrap().status.contains("Refused"));
    assert_eq!(
        a.world.current,
        "https://second.example/Path?q=AB%2Fcd#part"
    );
}
#[test]
fn address_native_adapter_repeats_focus_bounds_and_last_key_releases() {
    let mut a = app();
    a.keyboard_input(Some(KeyCode::KeyL), true, false, Some("l"))
        .unwrap();
    assert_eq!(a.address.as_deref(), Some(""));
    a.keyboard_input(Some(KeyCode::KeyW), true, false, Some("w"))
        .unwrap();
    a.keyboard_input(Some(KeyCode::KeyW), true, true, Some("w"))
        .unwrap();
    a.keyboard_input(Some(KeyCode::KeyW), false, false, None)
        .unwrap();
    assert_eq!(a.address.as_deref(), Some("ww"));
    a.key(KeyCode::Backspace, true, true).unwrap();
    assert_eq!(a.address.as_deref(), Some("w"));
    a.address_text(&"x".repeat(2048));
    assert!(a.address_error.is_some());
    a.key(KeyCode::Enter, true, false).unwrap();
    assert!(a.web.as_ref().unwrap().pending.is_none());
    a.focus_changed(false);
    assert!(a.address.is_none(), "focus loss dismisses transient entry");
    assert!(!a.input.forward);
    a.keyboard_input(Some(KeyCode::KeyQ), true, false, Some("q"))
        .unwrap();
    assert!(a.address.is_none());
    a.focus_changed(true);
    a.key(KeyCode::KeyL, true, false).unwrap();
    assert!(a.address.is_some());
    a.key(KeyCode::Escape, true, false).unwrap();
    assert!(!a.key(KeyCode::Escape, true, false).unwrap());
    a.key(KeyCode::Escape, false, false).unwrap();
}
#[test]
fn address_failed_initial_request_restores_usable_local_entry() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut a = App::new(
        root.clone(),
        vec![
            hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default())
                .unwrap(),
        ],
        0,
        World::load(&root).unwrap(),
        false,
    );
    let current = a.world.current.clone();
    a.web = Some(web_native::Runtime::new(Vec::new(), fixture));
    a.key(KeyCode::KeyL, true, false).unwrap();
    a.address_text("https://example.org/bad");
    a.key(KeyCode::Enter, true, false).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while a.web.as_ref().is_some_and(|w| w.pending.is_some()) {
        a.poll_web().unwrap();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(
        a.web.is_none(),
        "failed first web load must not strand local interaction behind web mode"
    );
    assert_eq!(a.world.current, current);
    assert!(a.address_error.as_deref().unwrap().contains("Refused"));
}
#[test]
fn url_argument_is_literal_and_separate_from_local_site() {
    let mut args = vec![
        "--url".into(),
        "https://example.org/one?q=AB%2Fcd#part".into(),
        "--pack".into(),
        "local.toml".into(),
    ];
    assert_eq!(
        take_url(&mut args).unwrap().unwrap().original,
        "https://example.org/one?q=AB%2Fcd#part"
    );
    assert_eq!(args, ["--pack", "local.toml"]);
    for values in [
        vec!["--url"],
        vec!["--url", "http://example.org/"],
        vec!["--url", "https://example.org/", "--site", "folder"],
        vec![
            "--url",
            "https://example.org/",
            "--url",
            "https://second.example/",
        ],
    ] {
        let mut args = values.iter().map(|v| v.to_string()).collect();
        assert!(take_url(&mut args).is_err());
    }
}
#[test]
fn web_input_follow_return_and_failed_refresh_preserve_last_good_pack() {
    let mut a = app();
    let source = a.themes[0].source.clone();
    a.world.reading = true;
    a.key(KeyCode::Tab, true, false).unwrap();
    a.key(KeyCode::Tab, false, false).unwrap();
    assert!(
        a.world
            .reader_link_description()
            .contains("https://example.org/two?q=AB%2Fcd#part")
    );
    assert!(!a.world.reader_link_description().contains("NOT SUPPORTED"));
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert_eq!(
        a.world.current, "https://example.org/one",
        "unfinished destination must not activate"
    );
    a.key(KeyCode::KeyE, true, false).unwrap(); // held cannot queue twice
    finish(&mut a);
    assert_eq!(a.world.current, "https://example.org/two?q=AB%2Fcd#part");
    assert_eq!(a.themes[0].source, source);
    assert_eq!(a.web.as_ref().unwrap().history.len(), 1);
    a.key(KeyCode::KeyR, true, false).unwrap();
    a.key(KeyCode::KeyR, false, false).unwrap();
    assert_eq!(a.world.current, "https://example.org/one");
    assert!(a.world.reading);
    // Refresh failure may not replace readable content, pose, pack or history.
    a.web.as_mut().unwrap().fetcher = |_, _, _| Err("fixture failed refresh".into());
    let pose = a.pose;
    let text = a.world.document().unwrap().to_string();
    a.key(KeyCode::F5, true, false).unwrap();
    finish(&mut a);
    assert_eq!(a.world.document().unwrap(), text);
    assert_eq!(
        (a.pose.position, a.pose.yaw, a.pose.pitch),
        (pose.position, pose.yaw, pose.pitch)
    );
    assert_eq!(a.themes[0].source, source);
    assert!(a.web.as_ref().unwrap().status.contains("last good"));
    // Presentation menu migration: Q opens a Quit confirmation; Enter exits.
    assert!(!a.key(KeyCode::KeyQ, true, false).unwrap());
    assert!(a.key(KeyCode::Enter, true, false).unwrap());
}
#[test]
fn web_portals_expose_upstream_identity_and_history_not_folder_parent() {
    let a = app();
    for door in &a.web.as_ref().unwrap().doors {
        let anchor = a.themes[0]
            .host
            .anchors
            .iter()
            .find(|x| x.id == door.anchor)
            .unwrap();
        let pose = Pose {
            position: [anchor.at[0], -2.8],
            yaw: 0.0,
            pitch: -0.2,
        };
        let t =
            hair::interaction::resolve(&a.themes[0].host, &pose, &a.world).expect("aimed portal");
        assert_eq!(t.anchor, door.anchor);
        assert_eq!(
            t.destination.as_deref(),
            Some(door.destinations[0].as_str())
        );
        assert!(t.prompt.contains(&door.destinations[0]));
    }
}

fn landmark_fixture(a: Address, _: &hair::acquisition::Cancellation, _: Instant) -> Result<Page> {
    Page::html(
        a,
        b"<p>Changed content</p><a href='/a'>A</a><a href='/b'>B</a><a href='/c'>C</a>",
    )
}
fn landmark_app() -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let g = Page::html(
        Address::parse("https://example.org/one?q=AB%2Fcd#top").unwrap(),
        b"<p>Old content</p><a href='/b'>B</a><a href='/c'>C</a>",
    )
    .unwrap()
    .generate()
    .unwrap();
    let mut cache = hair::theme::AssetCache::default();
    let themes = ["packs/office.toml", "packs/archive.toml"]
        .iter()
        .map(|p| hair::theme::Theme::load_for_scene(&root.join(p), &mut cache, &g.host).unwrap())
        .collect();
    let mut a = App::new(root, themes, 1, g.world, false);
    a.web = Some(web_native::Runtime::new(g.doors, landmark_fixture));
    a
}
#[test]
fn web_refresh_preserves_safe_camera_reader_selection_and_surviving_landmarks() {
    let mut a = landmark_app();
    a.pose = Pose {
        position: [2.0, 1.0],
        yaw: 0.3,
        pitch: -0.2,
    };
    let pose = a.pose;
    a.world.reading = true;
    a.world.cycle_reader_link(false);
    let selected = a.world.selected_reader_link().unwrap().link.clone();
    a.scroll = 3;
    let host = a.themes[1].host.clone();
    let source = a.themes[1].source.clone();
    a.key(KeyCode::F5, true, false).unwrap();
    a.key(KeyCode::F5, false, false).unwrap();
    finish(&mut a);
    assert_eq!(
        (a.pose.position, a.pose.yaw, a.pose.pitch),
        (pose.position, pose.yaw, pose.pitch),
        "refresh must not respawn"
    );
    assert!(a.world.reading, "refresh must not close reader");
    assert_eq!(a.world.selected_reader_link().unwrap().link, selected);
    assert_eq!(a.scroll, 3);
    assert!(a.world.document().unwrap().contains("Changed content"));
    assert_eq!(a.world.current, "https://example.org/one?q=AB%2Fcd#top");
    assert_eq!(a.theme_index, 1);
    assert_eq!(a.themes[1].source, source);
    for anchor in &host.anchors {
        let after = a.themes[1]
            .host
            .anchors
            .iter()
            .find(|x| x.id == anchor.id)
            .unwrap();
        assert_eq!(after.binding, anchor.binding);
        assert_eq!(after.role, anchor.role);
        if anchor.role != "portal" {
            assert_eq!(after, anchor);
        }
    }
    assert_eq!(a.web.as_ref().unwrap().history.len(), 0);
}
#[test]
fn web_refresh_deleted_selection_clears_without_moving_camera_or_closing_reader() {
    let mut a = landmark_app();
    a.world.reading = true;
    a.world.cycle_reader_link(false);
    a.scroll = 2;
    a.web.as_mut().unwrap().fetcher =
        |a, _, _| Page::html(a, b"<p>Still readable; links removed</p>");
    let pose = a.pose;
    a.key(KeyCode::F5, true, false).unwrap();
    finish(&mut a);
    assert!(a.world.reading);
    assert!(a.world.reader_link.is_none());
    assert_eq!(a.scroll, 2);
    assert_eq!(
        (a.pose.position, a.pose.yaw, a.pose.pitch),
        (pose.position, pose.yaw, pose.pitch)
    );
    assert!(
        a.themes[1]
            .host
            .anchors
            .iter()
            .any(|a| a.id == "return-history")
    );
}

#[path = "review_adverse.rs"]
mod reviewer_adverse;
