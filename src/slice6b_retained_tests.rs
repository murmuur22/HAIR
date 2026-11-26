use super::*;
use super::controls::{copied_pack, use_pack, pin_start};

fn next_theme_input(a: &mut App) {
    if a.place_mode.is_some() { tap(a, KeyCode::KeyP); }
    tap(a, KeyCode::Escape);
    assert_eq!(a.menu.unwrap().view, hair::presentation::View::Pause);
    tap(a, KeyCode::ArrowDown); tap(a, KeyCode::Enter); // Settings
    tap(a, KeyCode::ArrowDown); tap(a, KeyCode::Enter); // Appearance
    assert_eq!(a.menu.unwrap().focused(), hair::presentation::Item::Theme);
    tap(a, KeyCode::Enter); // Actual menu keyboard selection, not theme_index mutation.
    for _ in 0..3 { tap(a, KeyCode::Escape); }
    assert!(a.menu.is_none());
}
fn add_theme(a: &mut App, path: &Path) {
    a.themes.push(hair::theme::Theme::load_for_folder_scene(path, &mut Default::default(), &a.themes[0].host).unwrap());
}
fn select_saved(a: &mut App, d: &hair::placement_store::Decoration) {
    if a.place_mode.is_none() { tap(a, KeyCode::KeyP); }
    a.pose = Pose { position: [d.at[0], d.at[1] + 1.1], yaw: 0.,
        pitch: ((d.size[1] * 0.5 - a.pose.eye_height()) / 1.1).atan(), ..a.pose };
    assert_eq!(hair::interaction::decoration_target(&a.themes[a.theme_index].host, &a.pose, &a.world), Some(d));
    tap(a, KeyCode::KeyX);
    assert_eq!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().expected(), Some(d));
}

#[test]
fn slice6b_deleted_disposable_source_cold_reopen_theme_menu_and_pin_x_relocation() {
    let (f, mut a) = open_fixture("slice6b-deleted-source");
    let originals = records(&a.root.join("packs"));
    let original_files = records(&f.0);
    aim_file(&mut a, "A.txt"); tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]); tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
    let positions = layout(&a).positions;
    let pack = copied_pack(&a, "office", false); use_pack(&mut a, &pack);
    pin_start(&mut a, [-2., 0.]);
    let proposed = a.decoration_position().unwrap(); let before_mesh = mesh(&a, &proposed);
    tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
    tap(&mut a, KeyCode::F5); assert_eq!(layout(&a).decorations, vec![proposed.clone()]);
    assert_eq!(mesh(&a, &proposed), before_mesh);
    let saved = records(&f.0);
    drop(a);
    // Delete ONLY the disposable copied source after checked publication/refresh.
    std::fs::remove_dir_all(&pack.0).unwrap(); std::fs::create_dir(&pack.0).unwrap();
    assert!(!pack.0.join("pack.toml").exists());
    let mut a = app(); a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let archive = a.root.join("packs/archive.toml"); add_theme(&mut a, &archive);
    assert!(a.themes[1].decorations.entries.iter().all(|e| e.tag != proposed.tag));
    next_theme_input(&mut a); assert_eq!(a.theme_index, 1, "{}", a.local_status);
    let old = layout(&a).decorations[0].clone();
    assert_eq!(mesh(&a, &old), before_mesh, "missing semantic tag cannot substitute or hide a pin");
    assert_eq!(records(&f.0), saved);
    tap(&mut a, KeyCode::F5);
    assert_eq!(mesh(&a, &layout(&a).decorations[0]), before_mesh, "actual refresh after source deletion");
    assert_eq!(records(&f.0), saved);
    // Select with X in the other theme (missing its tag), even with a stale
    // file edit. Neither selection nor relocation may access the removed pack.
    assert_eq!(a.theme_index, 1);
    aim_file(&mut a, "A.txt"); tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
    assert!(a.place_mode.as_ref().unwrap().edit.is_some());
    select_saved(&mut a, &old);
    assert!(a.place_mode.as_ref().unwrap().edit.is_none());
    assert!(a.local_status.contains("Existing pinned"));
    assert!(!a.toolbox_controls().iter().any(|(c, _, _)| matches!(c, place_mode_native::Control::FollowTheme | place_mode_native::Control::PinAsset)));
    begin_positioning(&mut a);
    aim_floor(&mut a, [-1.85, 0.]); tap(&mut a, KeyCode::ArrowRight); tap(&mut a, KeyCode::KeyH);
    let relocated = a.decoration_position().unwrap();
    old.validate_relocation(&relocated).unwrap();
    assert_eq!(relocated.closure_identity(), old.closure_identity());
    tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration reposition saved"), "{}", a.local_status);
    assert_eq!(layout(&a).decorations, vec![relocated.clone()]);
    assert_eq!(layout(&a).positions, positions);
    let record = f.0.join(format!(".hair-layout/dc-{}/decoration.toml", old.placement_id));
    let mut expected = saved.clone(); expected.remove(&record);
    let mut actual = records(&f.0); actual.remove(&record);
    assert_eq!(actual, expected, "relocation only replaces selected metadata; closure inode/bytes stay exact");
    tap(&mut a, KeyCode::F5); assert_eq!(layout(&a).decorations, vec![relocated]);
    assert_eq!(records(&a.root.join("packs")), originals);
    for (p, v) in original_files { assert_eq!(records(&f.0).get(&p), Some(&v)); }
}

#[test]
fn slice6b_synthetic_tag_adapted_comparison_and_independent_same_tag_pin_sources() {
    let (f, mut a) = open_fixture("slice6b-same-tag");
    let originals = records(&a.root.join("packs"));
    // Labelled synthetic adaptation: archive closed-bookcase uses office chair tag.
    // Authentic meshes and all fitting/resource rules are unchanged.
    let adapted = copied_pack(&a, "archive", true);
    start(&mut a, [0., -1.], true);
    let follow = a.decoration_position().unwrap(); tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    let follow_office = mesh(&a, &follow);
    pin_start(&mut a, [-2., 0.]); let chair = a.decoration_position().unwrap();
    tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    let chair_mesh = mesh(&a, &chair);
    add_theme(&mut a, &adapted.0.join("pack.toml"));
    next_theme_input(&mut a); assert_eq!(a.theme_index, 1);
    assert_ne!(mesh(&a, &follow), follow_office, "Follow theme actually changes appearance in labelled adapted pack");
    assert_eq!(mesh(&a, &chair), chair_mesh);
    pin_start(&mut a, [2., 0.]); let bookcase = a.decoration_position().unwrap();
    assert_eq!(bookcase.tag, chair.tag); assert_ne!(bookcase.closure_identity(), chair.closure_identity());
    let book_mesh = mesh(&a, &bookcase); assert_ne!(book_mesh, chair_mesh);
    tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
    let saved = layout(&a); assert_eq!(saved.decorations.len(), 3);
    let before = records(&f.0);
    next_theme_input(&mut a); assert_eq!(a.theme_index, 0);
    assert_eq!(mesh(&a, &chair), chair_mesh); assert_eq!(mesh(&a, &bookcase), book_mesh);
    assert_eq!(mesh(&a, &follow), follow_office);
    tap(&mut a, KeyCode::F5); assert_eq!(layout(&a), saved);
    let mut reopened = app(); reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    add_theme(&mut reopened, &adapted.0.join("pack.toml")); next_theme_input(&mut reopened);
    assert_eq!(mesh(&reopened, &chair), chair_mesh); assert_eq!(mesh(&reopened, &bookcase), book_mesh);
    assert_ne!(mesh(&reopened, &follow), follow_office);
    assert_eq!(records(&f.0), before);
    let raw_archive = reopened.root.join("packs/archive.toml"); add_theme(&mut reopened, &raw_archive);
    next_theme_input(&mut reopened);
    assert_eq!(reopened.theme_index, 2, "absent exact Follow tag uses explicit generated missing art");
    assert!(reopened.themes[2].decoration_preview(&follow).unwrap()[..36].iter().all(|v| v.color == [0.5; 3]));
    assert_eq!(mesh(&reopened, &chair), chair_mesh); assert_eq!(mesh(&reopened, &bookcase), book_mesh);
    assert_eq!(records(&f.0), before);
    // Existing Follow-theme records have no conversion controls, including keys.
    select_saved(&mut reopened, &follow);
    assert!(reopened.local_status.contains("Existing Follow theme") && reopened.local_status.contains("conversion not supported"));
    tap(&mut reopened, KeyCode::KeyV);
    assert!(!reopened.decoration_position().unwrap().is_pinned());
    assert_eq!(records(&f.0), before);
    assert_eq!(records(&a.root.join("packs")), originals);
}

#[test]
fn slice6b_three_closures_dedup_fourth_resource_refusal_before_writes() {
    let (f, mut a) = open_fixture("slice6b-counts");
    let archive = a.root.join("packs/archive.toml"); add_theme(&mut a, &archive);
    // Two office sources via actual palette selection.
    pin_start(&mut a, [-2., 0.]); let first = a.decoration_position().unwrap();
    tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    // A second reference shares the first acquired closure, not an extra namespace.
    pin_start(&mut a, [-1., -1.]);
    assert_eq!(a.decoration_position().unwrap().closure_identity(), first.closure_identity());
    tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::KeyB); tap(&mut a, KeyCode::ArrowDown); tap(&mut a, KeyCode::Enter);
    begin_positioning(&mut a); aim_floor(&mut a, [0., -1.]); choose_pin(&mut a);
    tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
    next_theme_input(&mut a); assert_eq!(a.theme_index, 1);
    pin_start(&mut a, [2., 0.]); tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
    assert_eq!(layout(&a).decorations.len(), 4);
    let before = records(&f.0);
    tap(&mut a, KeyCode::KeyB); tap(&mut a, KeyCode::ArrowDown); tap(&mut a, KeyCode::Enter);
    begin_positioning(&mut a); aim_floor(&mut a, [2., -1.]); choose_pin(&mut a);
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("aggregate whole-pack budget"), "{}", a.local_status);
    assert!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().pending.is_none());
    assert_eq!(records(&f.0), before, "complete first review refuses fourth closure before writes");
}
