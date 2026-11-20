//! Synthetic actual-input regressions, prepared file-only; parent executes.
use super::*;
use std::sync::Arc;
const BOARD: &str = "https://example.org/Board?Case=A%2Fb";
fn input(a: &mut App, key: KeyCode) {
    a.keyboard_input(Some(key), true, false, None).unwrap();
    a.keyboard_input(Some(key), false, false, None).unwrap();
}
fn board(address: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    c.check(d)?;
    if address.url.path() != "/Board" {
        return Page::html(address, b"<title>Destination</title><p>Exact arrival</p>");
    }
    let mut html = String::from("<title>Rooms</title><p>Start</p>");
    for i in 0..130 {
        html.push_str(&format!(
            "<a href='/Exact/{i}?Case=AB%2Fcd#Tail'>Link{i}</a>"
        ));
    }
    html.push_str("<p>Final tail</p>");
    Page::html(address, html.as_bytes())
}
fn no_fetch(_: Address, _: &hair::acquisition::Cancellation, _: Instant) -> Result<Page> {
    panic!("same-response continuation invoked Fetcher")
}
fn enter() -> App {
    let mut a = app();
    a.web.as_mut().unwrap().fetcher = board;
    a.request_web(BOARD, web_native::Mode::Initial).unwrap();
    finish(&mut a);
    a
}
fn aim(a: &mut App, id: &str) {
    a.world.reading = false;
    a.world.explicit_target = None;
    let host = &a.themes[a.theme_index].host;
    let door = host.anchors.iter().find(|d| d.id == id).unwrap();
    let inward = host.portal_inward(id);
    a.pose = Pose {
        position: [door.at[0] + inward[0], door.at[2] + inward[1]],
        yaw: (-inward[0]).atan2(inward[1]),
        pitch: -0.1,
    };
}
fn go(a: &mut App, id: &str) {
    aim(a, id);
    input(a, KeyCode::KeyE);
    assert!(
        a.web.as_ref().unwrap().pending.is_some(),
        "typed physical action must schedule one worker"
    );
    finish(a);
}
fn source(a: &App) -> Arc<hair::web::HtmlSession> {
    a.world
        .pocket
        .as_ref()
        .unwrap()
        .source
        .as_ref()
        .unwrap()
        .clone()
}
#[test]
fn dense_middle_window_records_both_continuation_exits_without_false_cap() {
    let mut a = enter();
    go(&mut a, "web-next-window");
    let room = &a.exploration.rooms[a.exploration.current.unwrap()];
    assert_eq!(
        room.exits.len(),
        66,
        "64 links plus Previous and Next are real exits"
    );
    assert!(
        !a.exploration.limited,
        "maximum legitimate window is not a recording overflow"
    );
    assert!(
        room.exits
            .contains(&hair::exploration::RoomId::new(BOARD, 0))
    );
    assert!(
        room.exits
            .contains(&hair::exploration::RoomId::new(BOARD, 2))
    );
    for p in &room.layout.as_ref().unwrap().ports {
        if let Some(target) = p.target {
            assert!(room.exits.contains(&target));
        }
    }
}
#[test]
fn actual_next_previous_no_fetch_and_reciprocal_arrival() {
    let mut a = enter();
    let held = source(&a);
    a.web.as_mut().unwrap().fetcher = no_fetch;
    for (door, first, reciprocal) in [
        ("web-next-window", "Link64", "web-previous-window"),
        ("web-next-window", "Link128", "web-previous-window"),
        ("web-previous-window", "Link64", "web-next-window"),
        ("web-previous-window", "Link0", "web-next-window"),
    ] {
        go(&mut a, door);
        let window = a.world.pocket.as_ref().unwrap().room.unwrap().0;
        let current = a.exploration.current.unwrap();
        assert_eq!(
            a.exploration.rooms[current].id,
            hair::exploration::RoomId::new(BOARD, window)
        );
        assert!(
            a.exploration
                .edges
                .iter()
                .any(|e| e.kind == hair::exploration::Connection::Passage)
        );
        assert_eq!(a.world.reader_links()[0].label, first);
        assert_eq!(a.world.current, BOARD);
        assert!(Arc::ptr_eq(&held, &source(&a)));
        assert!(a.web.as_ref().unwrap().history.is_empty());
        let host = &a.themes[a.theme_index].host;
        let anchor = host.anchors.iter().find(|d| d.id == reciprocal).unwrap();
        let inward = host.portal_inward(reciprocal);
        assert_eq!(
            a.pose.position,
            [
                anchor.at[0] + inward[0] * 1.05,
                anchor.at[2] + inward[1] * 1.05
            ]
        );
        assert_eq!(a.pose.yaw, inward[0].atan2(-inward[1]));
        assert!(!host.blocked(a.pose.position));
        host.check_routes(&[a.pose.position]).unwrap();
    }
}
#[test]
fn physical_link_from_nonfirst_room_and_r_restores_exact_window_pose_and_actions() {
    let mut a = enter();
    go(&mut a, "web-next-window");
    let held = source(&a);
    let door = a.web.as_ref().unwrap().doors[0].anchor.clone();
    aim(&mut a, &door);
    let target = hair::web::door_destination(&a.world, &door).unwrap();
    assert!(
        a.world
            .reader_links()
            .iter()
            .any(|l| l.destination == hair::content::Destination::External(target.clone()))
    );
    let departure = a.pose;
    let links = a.world.reader_links();
    input(&mut a, KeyCode::KeyE);
    finish(&mut a);
    assert_eq!(a.world.current, target);
    assert_eq!(a.web.as_ref().unwrap().history.len(), 1);
    input(&mut a, KeyCode::KeyR);
    assert_eq!(a.world.current, BOARD);
    assert_eq!(a.world.reader_links(), links);
    assert_eq!(a.pose.position, departure.position);
    assert_eq!(a.pose.yaw, departure.yaw);
    assert_eq!(a.pose.pitch, departure.pitch);
    assert!(Arc::ptr_eq(&held, &source(&a)));
    a.web.as_mut().unwrap().fetcher = no_fetch;
    go(&mut a, "web-previous-window");
    assert_eq!(a.world.reader_links()[0].label, "Link0");
}
#[test]
fn pending_r_discards_even_completed_continuation_without_popping_page_history() {
    let mut a = enter();
    a.request_web(BOARD, web_native::Mode::Follow).unwrap();
    finish(&mut a);
    let held = source(&a);
    let links = a.world.reader_links();
    let pack = a.themes[a.theme_index].source.clone();
    aim(&mut a, "web-next-window");
    let pose = a.pose;
    let map = a.exploration.clone();
    input(&mut a, KeyCode::KeyE);
    assert!(a.web.as_ref().unwrap().pending.is_some());
    input(&mut a, KeyCode::KeyR);
    assert!(
        Arc::ptr_eq(&map, &a.exploration),
        "cancel must not publish a map observation"
    );
    assert_eq!(a.web.as_ref().unwrap().history.len(), 1);
    finish(&mut a);
    assert!(a.web.as_ref().unwrap().status.contains("Cancelled"));
    assert_eq!(a.web.as_ref().unwrap().history.len(), 1);
    assert_eq!(a.world.reader_links(), links);
    assert!(Arc::ptr_eq(&held, &source(&a)));
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.pose.yaw, pose.yaw);
    assert_eq!(a.themes[a.theme_index].source, pack);
    a.web.as_mut().unwrap().fetcher = no_fetch;
    go(&mut a, "web-next-window");
    assert_eq!(a.world.reader_links()[0].label, "Link64");
}
fn oversized(address: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    c.check(d)?;
    let html = format!(
        "<p>Start</p>{}<a href='/big'>{}</a>",
        "<a href='/a'>a</a>".repeat(64),
        "x".repeat(16385)
    );
    Page::html(address, html.as_bytes())
}
#[test]
fn refused_continuation_keeps_last_good_room_pose_pack_source_and_history() {
    let mut a = enter();
    a.web.as_mut().unwrap().fetcher = oversized;
    a.request_web(BOARD, web_native::Mode::Refresh).unwrap();
    finish(&mut a);
    let held = source(&a);
    let links = a.world.reader_links();
    let host = a.themes[a.theme_index].host.clone();
    let pack = a.themes[a.theme_index].source.clone();
    a.web.as_mut().unwrap().fetcher = no_fetch;
    aim(&mut a, "web-next-window");
    let pose = a.pose;
    input(&mut a, KeyCode::KeyE);
    finish(&mut a);
    assert!(a.web.as_ref().unwrap().status.contains("Refused:"));
    assert_eq!(a.world.reader_links(), links);
    assert_eq!(a.themes[a.theme_index].host, host);
    assert_eq!(a.themes[a.theme_index].source, pack);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.pose.yaw, pose.yaw);
    assert!(Arc::ptr_eq(&held, &source(&a)));
    assert!(a.web.as_ref().unwrap().history.is_empty());
}
#[test]
fn refresh_nonfirst_room_fetches_exact_url_and_explicitly_resets_source() {
    let mut a = enter();
    go(&mut a, "web-next-window");
    let held = source(&a);
    input(&mut a, KeyCode::F5);
    finish(&mut a);
    assert_eq!(a.world.current, BOARD);
    assert_eq!(a.world.reader_links()[0].label, "Link0");
    assert!(!Arc::ptr_eq(&held, &source(&a)));
    assert!(a.web.as_ref().unwrap().status.contains("reset"));
    let page = hair::exploration::RoomId::new(BOARD, 0);
    assert_eq!(a.exploration.rooms[a.exploration.current.unwrap()].id, page);
    assert!(
        a.exploration
            .rooms
            .iter()
            .any(|r| r.id.page == page.page && r.id.window == 1 && r.stale)
    );
    assert!(a.web.as_ref().unwrap().history.is_empty());
}
