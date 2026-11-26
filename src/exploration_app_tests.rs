use super::*;
use hair::exploration::{Connection, MapMode, RoomId};
#[test]
fn route_map_modal_actions_and_history_destination_are_independent_of_recent_visits() {
    let mut a = app();
    let first = a.world.current.clone();
    a.request_web("https://example.org/two", web_native::Mode::Follow)
        .unwrap();
    finish(&mut a);
    assert_eq!(
        a.web.as_ref().unwrap().back_room_id(),
        Some(RoomId::new(&first, 0))
    );
    let second = a.world.current.clone();
    a.request_web(&second, web_native::Mode::Follow).unwrap();
    finish(&mut a);
    assert_eq!(
        a.web.as_ref().unwrap().back_room_id(),
        Some(RoomId::new(&second, 0)),
        "self-follow has a same-room held Back destination, not the previous distinct visit"
    );
    let rooms = a.exploration.rooms.len();
    let history = a.web.as_ref().unwrap().history.len();
    let pose = a.pose;
    tap(&mut a, KeyCode::KeyM);
    for key in [
        KeyCode::KeyR,
        KeyCode::F5,
        KeyCode::KeyE,
        KeyCode::Tab,
        KeyCode::KeyL,
        KeyCode::KeyO,
        KeyCode::KeyQ,
        KeyCode::KeyT,
        KeyCode::F8,
    ] {
        tap(&mut a, key);
    }
    assert_eq!(a.world.current, second);
    assert_eq!(a.exploration.rooms.len(), rooms);
    assert_eq!(a.web.as_ref().unwrap().history.len(), history);
    assert!(a.address.is_none() && a.menu.is_none() && !a.chooser_pending && !a.input.captured);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.pose.yaw, pose.yaw);
    assert_eq!(a.pose.pitch, pose.pitch);
    tap(&mut a, KeyCode::ArrowRight);
    assert_eq!(a.route_pan, [1, 0]);
    a.key(KeyCode::KeyW, true, false).unwrap();
    assert!(!a.input.forward);
    a.key(KeyCode::ShiftLeft, true, false).unwrap();
    tap(&mut a, KeyCode::KeyM);
    a.key(KeyCode::ShiftLeft, false, false).unwrap();
    assert!(!a.minimap_enabled);
    assert_eq!(a.map_mode, MapMode::Expanded);
    tap(&mut a, KeyCode::Escape);
    assert_eq!(a.map_mode, MapMode::Hidden);
    assert!(a.menu.is_none());
    a.key(KeyCode::KeyW, true, true).unwrap();
    assert!(
        !a.input.forward,
        "held map key must stay inert after closing"
    );
    a.key(KeyCode::KeyW, false, false).unwrap();
    a.key(KeyCode::KeyW, true, false).unwrap();
    assert!(a.input.forward);
    a.key(KeyCode::KeyW, false, false).unwrap();
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.route_pan, [0, 0]);
    a.focus_changed(false);
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.map_mode, MapMode::Expanded);
    a.focus_changed(true);
    tap(&mut a, KeyCode::Escape);
    a.web.as_mut().unwrap().fetcher =
        |_, _, _| panic!("Back must restore a held snapshot, never fetch");
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(a.world.current, second);
    assert_eq!(
        a.web.as_ref().unwrap().back_room_id(),
        Some(RoomId::new(&first, 0))
    );
}
#[test]
fn route_map_keys_keep_minimap_preference_independent_and_do_not_toggle_tree() {
    let mut a = app();
    let tree = a.world.tree_collapsed;
    assert_eq!(a.map_mode, MapMode::Compact);
    a.key(KeyCode::ShiftLeft, true, false).unwrap();
    tap(&mut a, KeyCode::KeyM);
    a.key(KeyCode::ShiftLeft, false, false).unwrap();
    assert_eq!(
        a.map_mode,
        MapMode::Hidden,
        "Shift M toggles minimap, not the listing"
    );
    assert_eq!(a.world.tree_collapsed, tree);
    a.input.forward = true;
    a.input.captured = true;
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.map_mode, MapMode::Expanded);
    assert!(!a.input.forward && !a.input.captured);
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(
        a.map_mode,
        MapMode::Hidden,
        "M closes to remembered minimap state"
    );
    a.key(KeyCode::ShiftRight, true, false).unwrap();
    tap(&mut a, KeyCode::KeyM);
    a.key(KeyCode::ShiftRight, false, false).unwrap();
    assert_eq!(a.map_mode, MapMode::Compact);
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.map_mode, MapMode::Expanded);
    tap(&mut a, KeyCode::Escape);
    assert_eq!(a.map_mode, MapMode::Compact);
    assert!(a.menu.is_none());
    assert_eq!(a.world.tree_collapsed, tree);
}
#[test]
fn refresh_discloses_a_surviving_selected_door_moving_out_of_reach() {
    let mut a = app();
    a.web.as_mut().unwrap().fetcher =
        |address, _, _| Page::html(address, b"<p>Old</p><a href='/b'>B</a><a href='/c'>C</a>");
    a.request_web("https://example.org/notice", web_native::Mode::Follow)
        .unwrap();
    finish(&mut a);
    let door = a
        .web
        .as_ref()
        .unwrap()
        .doors
        .iter()
        .find(|d| d.resource.ends_with("/b"))
        .unwrap();
    let anchor = a.themes[0]
        .host
        .anchors
        .iter()
        .find(|x| x.id == door.anchor)
        .unwrap();
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [anchor.at[0], anchor.at[2] + 1.0],
        yaw: 0.0,
        pitch: -0.2,
    };
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    a.world.explicit_target = a.world.interaction.clone();
    assert!(a.world.explicit_target.is_some());
    a.web.as_mut().unwrap().fetcher = |address, _, _| {
        Page::html(
            address,
            b"<p>Changed</p><a href='/a'>A</a><a href='/b'>B</a><a href='/c'>C</a>",
        )
    };
    tap(&mut a, KeyCode::F5);
    finish(&mut a);
    assert!(a.world.explicit_target.is_none());
    assert!(a.web.as_ref().unwrap().status.contains("door moved"));
}
#[test]
fn committed_map_layout_follows_arrival_and_exact_history_without_resizing() {
    let mut a = app();
    let original = a.exploration.rooms[0]
        .layout
        .clone()
        .expect("initial committed room footprint");
    assert_eq!(original.bounds, a.themes[a.theme_index].host.room);
    a.request_web("https://example.org/two", web_native::Mode::Follow)
        .unwrap();
    finish(&mut a);
    let current = a.exploration.current.unwrap();
    assert_eq!(
        a.exploration.rooms[current].layout.as_ref().unwrap().bounds,
        a.themes[a.theme_index].host.room
    );
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(a.exploration.current, Some(0));
    assert_eq!(a.exploration.rooms[0].layout.as_ref(), Some(&original));
    assert_eq!(a.themes[a.theme_index].host.room, original.bounds);
}
#[test]
fn explicit_refresh_rebuilds_shrinking_room_and_relocates_only_unsafe_pose() {
    for unsafe_pose in [false, true] {
        let mut a = app();
        a.web.as_mut().unwrap().fetcher = |address, _, _| {
            let html = (0..64)
                .map(|i| format!("<a href='/large-{i}'>Link</a>"))
                .collect::<String>();
            Page::html(address, html.as_bytes())
        };
        a.request_web("https://example.org/resize", web_native::Mode::Follow)
            .unwrap();
        finish(&mut a);
        let old_bounds = a.themes[a.theme_index].host.room;
        a.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: if unsafe_pose {
                [0.0, old_bounds[3] - 2.0]
            } else {
                [2.0, 1.0]
            },
            yaw: 0.3,
            pitch: -0.1,
        };
        assert!(!a.themes[a.theme_index].host.blocked(a.pose.position));
        let old_pose = a.pose;
        a.web.as_mut().unwrap().fetcher = |address, _, _| {
            Page::html(
                address,
                b"<title>Refreshed small room</title><p>New content</p><a href='/small'>Small</a>",
            )
        };
        tap(&mut a, KeyCode::F5);
        finish(&mut a);
        let host = &a.themes[a.theme_index].host;
        assert_ne!(
            host.room, old_bounds,
            "explicit refresh must commit validated changed geometry"
        );
        assert!(a.world.document().unwrap().contains("New content"));
        assert!(!host.blocked(a.pose.position));
        if unsafe_pose {
            assert_eq!(a.pose.position, host.spawn);
            assert!(
                a.web.as_ref().unwrap().status.contains("entry"),
                "relocation must have visible notice"
            );
        } else {
            assert_eq!(
                (a.pose.position, a.pose.yaw, a.pose.pitch),
                (old_pose.position, old_pose.yaw, old_pose.pitch)
            );
        }
        let room = &a.exploration.rooms[a.exploration.current.unwrap()];
        assert_eq!(room.layout.as_ref().unwrap().bounds, host.room);
        let committed_bounds = host.room;
        let committed_pose = a.pose;
        let committed_layout = room.layout.clone();
        a.web.as_mut().unwrap().fetcher =
            |_, _, _| Err("owned fixture refuses refreshed source".into());
        tap(&mut a, KeyCode::F5);
        finish(&mut a);
        assert_eq!(a.themes[a.theme_index].host.room, committed_bounds);
        assert_eq!(
            (a.pose.position, a.pose.yaw, a.pose.pitch),
            (
                committed_pose.position,
                committed_pose.yaw,
                committed_pose.pitch
            )
        );
        assert_eq!(
            a.exploration.rooms[a.exploration.current.unwrap()].layout,
            committed_layout
        );
        assert!(a.world.document().unwrap().contains("New content"));
        assert!(a.web.as_ref().unwrap().status.contains("Refused:"));
        tap(&mut a, KeyCode::KeyR);
        assert_ne!(a.world.current, "https://example.org/resize");
        assert_eq!(a.exploration.current, Some(0));
    }
}
fn tap(a: &mut App, key: KeyCode) {
    a.key(key, true, false).unwrap();
    a.key(key, false, false).unwrap();
}
#[test]
fn actual_arrivals_follow_back_and_failures_record_only_successful_rooms() {
    let mut a = app();
    let first = a.world.current.clone();
    assert_eq!(
        a.exploration.rooms.len(),
        1,
        "initial committed room must be mapped"
    );
    a.request_web("https://example.org/two", web_native::Mode::Follow)
        .unwrap();
    finish(&mut a);
    assert_eq!(a.exploration.rooms.len(), 2);
    assert_eq!(a.exploration.current, Some(1));
    assert_eq!(a.exploration.edges[0].kind, Connection::Portal);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(a.world.current, first);
    assert_eq!(a.exploration.current, Some(0));
    assert_eq!(
        a.exploration.rooms.len(),
        2,
        "Back preserves exploration outside bounded page cache"
    );
    let edges = a.exploration.edges.len();
    a.request_web("https://example.org/bad", web_native::Mode::Follow)
        .unwrap();
    finish(&mut a);
    assert_eq!(a.exploration.rooms.len(), 2);
    assert_eq!(a.exploration.current, Some(0));
    assert_eq!(a.exploration.edges.len(), edges);
    assert_eq!(a.exploration.rooms[0].id, RoomId::new(&first, 0));
}
#[test]
fn map_modes_are_contextual_and_do_not_steal_tab_or_reader_keys() {
    let mut a = app();
    assert_eq!(a.map_mode, MapMode::Compact);
    let tree = a.world.tree_collapsed;
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.map_mode, MapMode::Expanded);
    assert_eq!(a.world.tree_collapsed, tree);
    a.key(KeyCode::KeyM, true, true).unwrap();
    assert_eq!(a.map_mode, MapMode::Expanded);
    a.key(KeyCode::KeyM, false, false).unwrap();
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.map_mode, MapMode::Compact);
    a.key(KeyCode::ShiftLeft, true, false).unwrap();
    tap(&mut a, KeyCode::KeyM);
    a.key(KeyCode::ShiftLeft, false, false).unwrap();
    assert_eq!(a.world.tree_collapsed, tree);
    assert_eq!(a.map_mode, MapMode::Hidden);
    a.world.reading = true;
    let tree = a.world.tree_collapsed;
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.world.tree_collapsed, tree);
    assert_eq!(a.map_mode, MapMode::Hidden);
    a.world.reading = false;
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.map_mode, MapMode::Hidden);
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::KeyL);
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.map_mode, MapMode::Hidden);
    tap(&mut a, KeyCode::Escape);
}
