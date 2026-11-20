//! Reader wheel/trackpad and held Up/Down scrolling through the same App
//! adapters the native winit KeyboardInput/MouseWheel events call.
use super::*;
use hair::web::{Address, Page};
use winit::dpi::PhysicalPosition;

fn long(a: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    c.check(d)?;
    let body: String = (0..120)
        .map(|i| format!("<p>Row {i} of synthetic reader text</p>"))
        .collect();
    Page::html(
        a,
        format!("<title>Long</title>{body}<a href='/next'>Next</a>").as_bytes(),
    )
}
fn app() -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let p = web_native::prepare(
        Address::parse("https://example.org/long").unwrap(),
        &[root.join("packs/office.toml")],
        long,
        &Default::default(),
    )
    .unwrap();
    let mut app = App::new(root, p.themes, 0, p.generated.world, false);
    app.web = Some(web_native::Runtime::new(p.generated.doors, long));
    app.world.reading = true;
    app
}
fn max(a: &App) -> usize {
    let [w, h] = a.ui_viewport();
    hair::text::local_reader_scroll_max(&a.world, w, h)
}
fn world_state(a: &App) -> (String, [f32; 2], f32, f32, bool, usize) {
    (
        a.world.current.clone(),
        a.pose.position,
        a.pose.yaw,
        a.pose.pitch,
        a.input.forward,
        a.web.as_ref().unwrap().history.len(),
    )
}

#[test]
fn held_up_down_repeat_scrolls_reader_after_fresh_press_and_clamps_to_content() {
    let mut a = app();
    let before = world_state(&a);
    assert!(max(&a) > 40, "fixture must be scrollable");
    a.key(KeyCode::ArrowDown, true, false).unwrap();
    assert_eq!(a.scroll, 1, "initial press scrolls one row");
    for _ in 0..5 {
        a.key(KeyCode::ArrowDown, true, true).unwrap();
    }
    assert_eq!(a.scroll, 6, "native OS repeat continues a held Down");
    a.key(KeyCode::ArrowDown, false, false).unwrap();
    assert_eq!(a.scroll, 6, "release never scrolls");
    a.key(KeyCode::ArrowUp, true, false).unwrap();
    for _ in 0..3 {
        a.key(KeyCode::ArrowUp, true, true).unwrap();
    }
    a.key(KeyCode::ArrowUp, false, false).unwrap();
    assert_eq!(a.scroll, 2, "held Up repeats too");
    a.key(KeyCode::ArrowDown, true, false).unwrap();
    for _ in 0..2000 {
        a.key(KeyCode::ArrowDown, true, true).unwrap();
    }
    assert_eq!(
        a.scroll,
        max(&a),
        "held Down clamps to the last content row"
    );
    // Focus loss drops the held latch: a surviving OS repeat is inert until a
    // fresh physical press, as for every other one-shot action.
    a.focus_changed(false);
    a.focus_changed(true);
    let held = a.scroll;
    a.key(KeyCode::ArrowDown, true, true).unwrap();
    a.key(KeyCode::ArrowUp, true, true).unwrap();
    assert_eq!(
        a.scroll, held,
        "repeat without a fresh press after focus loss"
    );
    a.key(KeyCode::ArrowUp, true, false).unwrap();
    assert_eq!(a.scroll, held - 1);
    a.key(KeyCode::ArrowUp, false, false).unwrap();
    // Closing the reader while Down is held: repeats never reach the world.
    a.key(KeyCode::ArrowDown, true, false).unwrap();
    a.key(KeyCode::Escape, true, false).unwrap();
    a.key(KeyCode::Escape, false, false).unwrap();
    assert!(!a.world.reading);
    let scroll = a.scroll;
    for _ in 0..10 {
        a.key(KeyCode::ArrowDown, true, true).unwrap();
    }
    assert_eq!(a.scroll, scroll);
    assert_eq!(world_state(&a), before, "world/history/camera untouched");
}

fn finish(a: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while a.web.as_ref().unwrap().pending.is_some() {
        a.poll_web().unwrap();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
/// FIX1 C: a fractional wheel/trackpad remainder belongs to one document. A
/// Follow (Tab+E) or R history return starts the next one with none.
#[test]
fn wheel_remainder_never_bleeds_into_a_followed_or_returned_document() {
    let half_row = MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -13.5));
    let mut a = app();
    a.mouse_wheel(half_row);
    assert_eq!(a.scroll, 0, "three quarters of a row does not scroll");
    assert!(a.wheel_rows > 0.5);
    a.key(KeyCode::Tab, true, false).unwrap();
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::KeyE, true, false).unwrap();
    a.key(KeyCode::KeyE, false, false).unwrap();
    finish(&mut a);
    assert_eq!(a.world.current, "https://example.org/next", "followed");
    assert_eq!(a.wheel_rows, 0.0, "remainder reset by Follow");
    a.world.reading = true;
    a.mouse_wheel(half_row);
    assert_eq!(
        a.scroll, 0,
        "new document: a fresh partial row cannot scroll"
    );
    a.key(KeyCode::KeyR, true, false).unwrap();
    a.key(KeyCode::KeyR, false, false).unwrap();
    finish(&mut a);
    assert_eq!(a.world.current, "https://example.org/long", "returned");
    assert_eq!(a.wheel_rows, 0.0, "remainder reset by R return");
}
/// FIX1 C: an arrow held across the modal menu never leaks into the reader
/// after the menu closes, and the folder chooser owns the wheel.
#[test]
fn modal_menu_held_arrow_and_chooser_wheel_never_scroll_reader() {
    let mut a = app();
    a.key(KeyCode::KeyQ, true, false).unwrap();
    a.key(KeyCode::KeyQ, false, false).unwrap();
    assert!(
        a.menu.is_some(),
        "Q opens the quit confirmation over the reader"
    );
    a.key(KeyCode::ArrowDown, true, false).unwrap();
    a.key(KeyCode::Escape, true, false).unwrap();
    a.key(KeyCode::Escape, false, false).unwrap();
    assert!(a.menu.is_none() && a.world.reading);
    for _ in 0..10 {
        a.key(KeyCode::ArrowDown, true, true).unwrap();
    }
    assert_eq!(
        a.scroll, 0,
        "held menu arrow repeats stay inert after close"
    );
    a.key(KeyCode::ArrowDown, false, false).unwrap();
    a.key(KeyCode::ArrowDown, true, false).unwrap();
    a.key(KeyCode::ArrowDown, false, false).unwrap();
    assert_eq!(a.scroll, 1, "fresh press after release scrolls");
    for chooser in [[true, false], [false, true]] {
        [a.chooser_active, a.chooser_pending] = chooser;
        a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, -3.0));
        a.mouse_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
            0.0, -13.5,
        )));
        assert_eq!((a.scroll, a.wheel_rows), (1, 0.0), "chooser owns the wheel");
    }
}
/// FIX1 A: the 2x address panel scrolls through the real key adapter; held
/// arrows are bounded to the wrapped rows, and any edit returns to the cursor.
#[test]
fn address_entry_up_down_scrolls_bounded_rows_and_edits_return_to_cursor() {
    let mut a = app();
    a.world.reading = false;
    let before = world_state(&a);
    a.key(KeyCode::KeyL, true, false).unwrap();
    a.key(KeyCode::KeyL, false, false).unwrap();
    a.address_text(&format!("https://example.org/{}", "aB".repeat(1000)));
    let [w, h] = a.ui_viewport();
    let max = hair::text::address_scroll_max(a.address.as_deref().unwrap(), None, w, h);
    assert!(max > 0, "2048-byte input overflows the 960x640 panel");
    a.key(KeyCode::ArrowUp, true, false).unwrap();
    for _ in 0..500 {
        a.key(KeyCode::ArrowUp, true, true).unwrap();
    }
    assert_eq!(a.address_scroll, max, "held Up clamps to the first row");
    a.key(KeyCode::ArrowDown, true, false).unwrap();
    assert_eq!(a.address_scroll, max - 1, "Down responds at once");
    a.key(KeyCode::Backspace, true, false).unwrap();
    assert_eq!(a.address_scroll, 0, "edit returns to the cursor");
    a.key(KeyCode::ArrowUp, true, false).unwrap();
    a.address_text("x");
    assert_eq!(a.address_scroll, 0, "typing returns to the cursor");
    assert_eq!(a.scroll, 0, "reader scroll untouched");
    a.key(KeyCode::Escape, true, false).unwrap();
    a.key(KeyCode::Escape, false, false).unwrap();
    assert!(a.address.is_none());
    assert_eq!(world_state(&a), before, "world/history/camera untouched");
}
/// FIX2: Up/Down held in the address modal over a reader stays inert after
/// Escape until physical release or a fresh press, through the native adapter.
#[test]
fn address_held_arrows_never_leak_into_reader_after_escape() {
    let mut a = app();
    let k = |a: &mut App, code, down, repeat| {
        a.keyboard_input(Some(code), down, repeat, None).unwrap();
    };
    k(&mut a, KeyCode::ArrowDown, true, false);
    k(&mut a, KeyCode::ArrowDown, false, false);
    assert_eq!(a.scroll, 1, "reader scrolls before address entry");
    k(&mut a, KeyCode::KeyL, true, false);
    k(&mut a, KeyCode::KeyL, false, false);
    assert!(a.address.is_some() && a.world.reading);
    a.address_text(&format!("https://example.org/{}", "aB".repeat(1000)));
    for code in [KeyCode::ArrowUp, KeyCode::ArrowDown] {
        k(&mut a, code, true, false);
        k(&mut a, code, true, true);
    }
    assert_eq!(a.address_scroll, 0, "address Up/Down still scroll its rows");
    k(&mut a, KeyCode::ArrowUp, true, true);
    assert_eq!(a.address_scroll, 1, "address accepts held repeats");
    k(&mut a, KeyCode::Escape, true, false);
    k(&mut a, KeyCode::Escape, false, false);
    assert!(a.address.is_none() && a.world.reading);
    for code in [KeyCode::ArrowDown, KeyCode::ArrowUp] {
        for _ in 0..10 {
            k(&mut a, code, true, true);
        }
        assert_eq!(
            a.scroll, 1,
            "held address {code:?} stays inert in the reader"
        );
    }
    k(&mut a, KeyCode::ArrowDown, false, false);
    k(&mut a, KeyCode::ArrowDown, true, true);
    assert_eq!(a.scroll, 2, "after release, Down works again");
    k(&mut a, KeyCode::ArrowUp, true, false);
    assert_eq!(a.scroll, 1, "fresh Up press works without a release");
    k(&mut a, KeyCode::ArrowUp, true, true);
    assert_eq!(a.scroll, 0, "and then repeats normally");
}

#[test]
fn wheel_line_and_pixel_deltas_scroll_reader_bounded_and_isolated() {
    let mut a = app();
    let before = world_state(&a);
    a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, -1.0));
    assert_eq!(a.scroll, 3, "one wheel notch scrolls three rows");
    a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, 1.0));
    assert_eq!(a.scroll, 0);
    a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, 5.0));
    assert_eq!(a.scroll, 0, "clamped at the top");
    a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, -1.0));
    assert_eq!(a.scroll, 3, "no backlog accumulates beyond the top");
    a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, 1.0));
    // Trackpad pixels accumulate smoothly: the default 960x640 physical
    // viewport at 100% draws 2x reader rows, 18 physical pixels each.
    for _ in 0..3 {
        a.mouse_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
            0.0, -4.5,
        )));
    }
    assert_eq!(a.scroll, 0, "partial row does not jump");
    a.mouse_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
        0.0, -4.5,
    )));
    assert_eq!(a.scroll, 1, "accumulated pixels complete one row");
    for bad in [
        MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, f64::NAN)),
        MouseScrollDelta::PixelDelta(PhysicalPosition::new(f64::INFINITY, -1.0)),
        MouseScrollDelta::LineDelta(0.0, f32::NEG_INFINITY),
    ] {
        a.mouse_wheel(bad);
        assert_eq!(a.scroll, 1, "non-finite deltas are refused");
    }
    a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, -1.0));
    assert_eq!(a.scroll, 4, "refused deltas leave the accumulator usable");
    a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, -1e30));
    assert_eq!(a.scroll, max(&a), "huge delta clamps to content");
    a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, 1.0));
    assert_eq!(a.scroll, max(&a) - 3, "no backlog beyond the bottom");
    // A half-row remainder is discarded by focus loss.
    a.mouse_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
        0.0, -9.0,
    )));
    a.focus_changed(false);
    let parked = a.scroll;
    a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, -1.0));
    assert_eq!(a.scroll, parked, "unfocused wheel is inert");
    a.focus_changed(true);
    a.mouse_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
        0.0, 9.0,
    )));
    assert_eq!(a.scroll, parked, "stale remainder discarded on focus loss");
    a.menu = Some(hair::presentation::Menu::pause());
    a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, 3.0));
    assert_eq!(a.scroll, parked, "menu owns the wheel");
    assert_eq!(a.menu.unwrap().focused(), hair::presentation::Item::Resume);
    a.menu = None;
    a.address = Some(String::new());
    a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, 3.0));
    assert_eq!(a.scroll, parked, "address entry owns the wheel");
    assert_eq!(a.address.as_deref(), Some(""));
    a.address = None;
    a.world.reading = false;
    a.mouse_wheel(MouseScrollDelta::LineDelta(0.0, -3.0));
    a.mouse_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
        40.0, -400.0,
    )));
    assert_eq!(a.scroll, parked, "closed reader: wheel is not world input");
    assert_eq!(world_state(&a), before);
}
