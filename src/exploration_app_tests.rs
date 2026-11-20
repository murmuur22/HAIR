use super::*;
use hair::exploration::{Connection, MapMode, RoomId};
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
    assert_eq!(a.map_mode, MapMode::Hidden);
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.map_mode, MapMode::Compact);
    a.key(KeyCode::ShiftLeft, true, false).unwrap();
    tap(&mut a, KeyCode::KeyM);
    a.key(KeyCode::ShiftLeft, false, false).unwrap();
    assert_ne!(a.world.tree_collapsed, tree);
    assert_eq!(a.map_mode, MapMode::Compact);
    a.world.reading = true;
    let tree = a.world.tree_collapsed;
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.world.tree_collapsed, !tree);
    assert_eq!(a.map_mode, MapMode::Compact);
    a.world.reading = false;
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.map_mode, MapMode::Compact);
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::KeyL);
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(a.map_mode, MapMode::Compact);
    tap(&mut a, KeyCode::Escape);
}
