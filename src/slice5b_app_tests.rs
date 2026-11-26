//! Saved-decoration App tracer. All writes are confined to owned fixtures.
use super::*;
use hair::placement_store::{Decoration, Outcome};
#[path = "slice5b_controls_tests.rs"]
mod controls;

fn aim_saved(a: &mut App, d: &Decoration) {
    a.pose = Pose { position: [d.at[0], d.at[1] + 1.1], yaw: 0.,
        pitch: ((d.size[1] * 0.5 - a.pose.eye_height()) / 1.1).atan(), ..a.pose };
    assert_eq!(hair::interaction::decoration_target(&a.themes[a.theme_index].host, &a.pose, &a.world)
        .map(|d| &d.placement_id), Some(&d.placement_id), "real saved decoration must own aim");
}
fn select_saved(a: &mut App, d: &Decoration) {
    if a.place_mode.is_none() { tap(a, KeyCode::KeyP); }
    aim_saved(a, d);
    tap(a, KeyCode::KeyX);
    assert!(a.place_mode.as_ref().unwrap().decoration_edit.is_some(),
        "X must arm existing-decoration reposition, not inspection only: {}", a.local_status);
    assert_eq!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().expected(), Some(d));
    assert!(a.place_mode.as_ref().unwrap().edit.is_none());
    assert!(a.inventory.items().is_empty() && a.placement_action.is_none());
    assert!(a.local_status.contains("reposition") && a.local_status.contains(&d.placement_id));
    assert!(a.local_status.contains("Existing Follow theme") && a.local_status.contains("conversion not supported"));
    begin_positioning(a);
}
fn save_new(a: &mut App, at: [f32; 2]) -> Decoration {
    start(a, at, true); // Baseline setup through the existing real new-decoration input.
    assert!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().expected().is_none(),
        "new-decoration intent must remain explicit even when same-tag saved instances exist");
    let d = a.decoration_position().unwrap();
    tap(a, KeyCode::Enter); tap(a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration saved"), "{}", a.local_status);
    d
}

#[test]
fn slice5b_actual_x_reposition_preview_review_commit_refresh_reopen_stable_identity() {
    let (f, mut a) = open_fixture("slice5b-tracer");
    // Preserve both a saved file record and an independent generated file.
    aim_file(&mut a, "A.txt"); tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]); tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
    let old = save_new(&mut a, [-2., 0.]);
    let other = save_new(&mut a, [0., -1.]);
    let before = records(&f.0);
    let saved = layout(&a);
    let file = file_mesh(&a, "A.txt"); let generated = file_mesh(&a, "Z.txt");
    let host = a.themes[0].host.clone();
    select_saved(&mut a, &old);
    assert_eq!(a.editor_phase(), "Positioning");
    assert!(!a.toolbox_controls().iter().any(|(c, _, _)| matches!(c,
        place_mode_native::Control::FollowTheme | place_mode_native::Control::PinAsset)));
    // The replacement overlaps its own old envelope: only that old envelope is excluded.
    aim_floor(&mut a, [-1.85, 0.]);
    click(&mut a, place_mode_native::Control::Right);
    tap(&mut a, KeyCode::KeyH);
    let proposed = a.decoration_position().unwrap();
    assert_eq!(proposed.placement_id, old.placement_id);
    assert_eq!(proposed.tag, old.tag); assert_eq!(proposed.size, old.size);
    assert_ne!(proposed.yaw, old.yaw); assert!(!proposed.collision);
    assert!(a.place_preview().unwrap().len() > 36);
    assert!(!a.place_ui().is_empty());
    assert_eq!(a.themes[0].host, host, "preview must not remove physics/target authority");
    assert_eq!(records(&f.0), before);
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("frozen") && a.local_status.contains("reposition"), "{}", a.local_status);
    assert!(!a.input.captured);
    assert_eq!(a.decoration_position().unwrap(), proposed);
    assert_eq!(records(&f.0), before, "review is write-free");
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
    let after = layout(&a);
    assert_eq!(after.positions, saved.positions);
    assert_eq!(after.decorations.len(), 2);
    assert!(after.decorations.contains(&proposed) && after.decorations.contains(&other));
    assert_eq!(a.themes[0].host.decorations[&old.placement_id], proposed);
    assert!(!hair::player::blocked_at(&a.themes[0].host, proposed.at, 0.));
    assert!(!a.themes[0].host.blocked(proposed.at));
    assert_eq!(file_mesh(&a, "A.txt"), file); assert_eq!(file_mesh(&a, "Z.txt"), generated);
    let record = f.0.join(format!(".hair-layout/dc-{}/decoration.toml", old.placement_id));
    let mut expected = before.clone(); expected.remove(&record);
    let mut actual = records(&f.0); actual.remove(&record);
    assert_eq!(actual, expected, "only the exact relocated metadata may change bytes/inode");
    assert_ne!(records(&f.0)[&record], before[&record]);
    assert!(a.inventory.items().is_empty() && a.placement_action.is_none());
    tap(&mut a, KeyCode::F5);
    assert_eq!(layout(&a), after);
    let mut reopened = app(); reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(layout(&reopened), after);
    assert_eq!(reopened.themes[0].host.decorations, a.themes[0].host.decorations);
    assert_eq!(file_mesh(&reopened, "A.txt"), file);
    assert_eq!(file_mesh(&reopened, "Z.txt"), generated);
}
