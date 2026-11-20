//! Web navigation panel state through the shared App key adapter: M collapse
//! survives follow/back/menu; trail/visited come only from real arrivals.
use super::*;

fn tap(a: &mut App, code: KeyCode) {
    a.key(code, true, false).unwrap();
    a.key(code, false, false).unwrap();
}

#[test]
fn panel_collapse_trail_and_visited_survive_follow_back_and_menu() {
    let mut a = app();
    let first = a.world.current.clone();
    let collapsed = a.world.tree_collapsed;
    tap(&mut a, KeyCode::KeyM);
    assert_eq!(
        a.world.tree_collapsed, !collapsed,
        "M toggles the web panel"
    );
    let expanded = a.world.tree_collapsed;
    a.world.reading = true;
    a.world.cycle_reader_link(false);
    tap(&mut a, KeyCode::KeyE);
    finish(&mut a);
    assert_ne!(a.world.current, first);
    assert_eq!(a.world.tree_collapsed, expanded, "follow keeps the M state");
    let pocket = a.world.pocket.as_ref().unwrap();
    assert_eq!(
        pocket
            .history
            .iter()
            .map(|p| p.address.as_str())
            .collect::<Vec<_>>(),
        [first.as_str()],
        "trail mirrors the actual history stack"
    );
    assert!(pocket.visited.contains(&first) && pocket.visited.contains(&a.world.current));
    let second = a.world.current.clone();
    tap(&mut a, KeyCode::Escape);
    assert!(a.menu.is_some());
    tap(&mut a, KeyCode::KeyM);
    tap(&mut a, KeyCode::Escape);
    assert!(a.menu.is_none());
    assert_eq!(a.world.tree_collapsed, expanded, "menu isolates M");
    tap(&mut a, KeyCode::KeyM);
    let preference = a.world.tree_collapsed;
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(a.world.current, first, "R is the typed history return");
    assert_eq!(a.world.tree_collapsed, preference, "back keeps the M state");
    let pocket = a.world.pocket.as_ref().unwrap();
    assert!(pocket.history.is_empty() && pocket.previous.is_none());
    assert!(
        pocket.visited.contains(&second),
        "session visited survives the return"
    );
}
