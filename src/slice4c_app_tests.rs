use super::*;
#[path = "slice4c1_app_tests.rs"]
mod qualification;
#[path = "slice4c4_app_tests.rs"]
mod metal_witness;
#[path = "slice4c6_app_tests.rs"]
mod presentation;
#[path = "slice5b_app_tests.rs"]
mod saved_reposition;
#[path = "slice6b_app_tests.rs"]
mod pin_choice;
#[path = "slice8c_app_tests.rs"]
mod recovery;
#[path = "combined_b1_app_tests.rs"]
mod combined_b1;

#[test]
fn slice4c_catalog_selection_exposes_explicit_place_action_without_saving() {
    let (f, mut a) = open_fixture("slice4c-explicit");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::Enter);
    assert!(!f.0.join(".hair-layout").exists());
    assert_eq!(a.editor_phase(), "Editing");
    assert!(a.toolbox_controls().iter().any(|(_, _, label)| label.contains("begin positioning")),
        "catalog selection must expose explicit positioning, not publish");
    assert!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().pending.is_none());
}

fn start(a: &mut App, at: [f32; 2], collision: bool) {
    if a.place_mode.is_none() { tap(a, KeyCode::KeyP); }
    // Legacy positioning fixtures now explicitly traverse the edit-first phase.
    // Dedicated R2A tests assert each phase and held-key boundary separately.
    for _ in 0..2 { if a.editor_held() { tap(a, KeyCode::Escape); } }
    tap(a, KeyCode::KeyB);
    tap(a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Editing");
    // Explicit legacy transaction fixture, not the current catalog default.
    a.legacy_follow_fixture();
    tap(a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Positioning");
    tap(a, KeyCode::Tab);
    // Positioning now protects the central scene at the default logical width.
    // These property-focused fixtures explicitly open the real disclosure;
    // independent layout tests qualify compact world visibility and recapture.
    if a.editor_compact() {
        click(a, place_mode_native::Control::EditorDetails);
        assert!(!a.editor_compact());
    }
    aim_floor(a, at);
    if a.decoration_position().unwrap().collision != collision { tap(a, KeyCode::KeyH); }
}
fn layout(a: &App) -> hair::placement_store::LayoutRead {
    hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap()
}
pub(super) fn records(root: &Path) -> std::collections::BTreeMap<PathBuf, (u64, Vec<u8>)> {
    fn walk(root: &Path, out: &mut std::collections::BTreeMap<PathBuf, (u64, Vec<u8>)>) {
        for e in std::fs::read_dir(root).unwrap() {
            let p = e.unwrap().path();
            let m = std::fs::symlink_metadata(&p).unwrap();
            if m.is_dir() { walk(&p, out); }
            else { out.insert(p.clone(), (m.ino(), std::fs::read(p).unwrap())); }
        }
    }
    let mut out = Default::default(); walk(root, &mut out); out
}
#[test]
fn slice4c_actual_app_two_same_tag_commit_refresh_reopen_and_theme_menu_preserve_identity() {
    let (f, mut a) = open_fixture("slice4c-two");
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]);
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
    let original = records(&f.0);
    let file = file_mesh(&a, "A.txt");
    let other = file_mesh(&a, "Z.txt");
    let positions = layout(&a).positions;
    for (at, collision) in [([-2., 0.], false), ([0., -1.], true)] {
        start(&mut a, at, collision);
        tap(&mut a, KeyCode::ArrowRight);
        let preview = a.place_preview().unwrap();
        assert!(preview.len() > 36, "actual catalog mesh, not a cuboid stub");
        assert!(!a.place_ui().is_empty());
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("frozen decoration review"), "{}", a.local_status);
        let frozen = a.decoration_position().unwrap();
        for key in [KeyCode::ArrowLeft, KeyCode::KeyH, KeyCode::KeyB, KeyCode::KeyX, KeyCode::KeyN, KeyCode::KeyG, KeyCode::KeyC] { tap(&mut a, key); }
        aim_floor(&mut a, [3., 1.]);
        assert_eq!(a.decoration_position().unwrap(), frozen);
        a.key(KeyCode::Enter, true, true).unwrap();
        a.key(KeyCode::Enter, false, false).unwrap();
        assert!(!layout(&a).decorations.iter().any(|d| d.placement_id == frozen.placement_id));
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("decoration saved"), "{}", a.local_status);
        let host = &a.themes[0].host;
        assert_eq!(host.decorations[&frozen.placement_id], frozen);
        assert_eq!(hair::player::blocked_at(host, frozen.at, 0.), collision);
        assert_eq!(host.blocked(frozen.at), collision);
        assert!(!host.anchors.iter().any(|an| an.id == frozen.placement_id));
        assert_eq!(file_mesh(&a, "A.txt"), file);
        assert_eq!(file_mesh(&a, "Z.txt"), other);
        for (path, value) in &original { assert_eq!(records(&f.0).get(path), Some(value)); }
    }
    let saved = layout(&a);
    assert_eq!(saved.positions, positions);
    assert_eq!(saved.decorations.len(), 2);
    assert_eq!(saved.decorations[0].tag, saved.decorations[1].tag);
    assert_ne!(saved.decorations[0].placement_id, saved.decorations[1].placement_id);
    tap(&mut a, KeyCode::F5);
    assert_eq!(layout(&a), saved);
    let mut reopened = app();
    reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(reopened.themes[0].host.decorations, a.themes[0].host.decorations);
    assert_eq!(file_mesh(&reopened, "A.txt"), file);
    let pack = fixture("slice4c-theme");
    for name in ["bookcaseOpenLow.glb", "bookcaseOpen.glb", "bookcaseClosedDoors.glb"] {
        std::fs::copy(a.root.join("packs").join(name), pack.0.join(name)).unwrap();
    }
    let text = std::fs::read_to_string(a.root.join("packs/archive.toml")).unwrap()
        .replace("hair.storage.closed-bookcase", &saved.decorations[0].tag);
    std::fs::write(pack.0.join("pack.toml"), text).unwrap();
    let theme = hair::theme::Theme::load_for_folder_scene(&pack.0.join("pack.toml"), &mut Default::default(), &reopened.themes[0].host).unwrap();
    let before_mesh = reopened.themes[0].decoration_preview(&saved.decorations[0]).unwrap();
    reopened.themes.push(theme);
    reopened.menu = Some(hair::presentation::Menu::settings());
    reopened.activate_menu(hair::presentation::Item::Theme);
    assert_eq!(reopened.theme_index, 1, "{}", reopened.local_status);
    let new_mesh = reopened.themes[1].decoration_preview(&saved.decorations[0]).unwrap();
    assert_ne!(bytemuck::cast_slice::<_, u8>(&before_mesh), bytemuck::cast_slice::<_, u8>(&new_mesh));
    assert_eq!(reopened.themes[0].host, reopened.themes[1].host);
    assert_eq!(layout(&reopened), saved);
    let bytes = records(&f.0);
    reopened.themes[0].decorations.entries.clear();
    reopened.activate_menu(hair::presentation::Item::Theme);
    assert_eq!(reopened.theme_index, 0);
    assert!(reopened.themes[0].decoration_preview(&saved.decorations[0]).unwrap()[..36].iter().all(|v| v.color == [0.5; 3]));
    assert_eq!(records(&f.0), bytes);
}

#[test]
fn slice4c_actual_cancel_focus_navigation_late_overlap_and_busy_lock_preserve_sources() {
    use std::os::fd::AsRawFd;
    let (f, mut a) = open_fixture("slice4c-cancel");
    let original = records(&f.0);
    for exit in [KeyCode::Escape, KeyCode::KeyP, KeyCode::F5] {
        start(&mut a, [-2., 0.], true);
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("frozen decoration review"), "{}", a.local_status);
        tap(&mut a, exit);
        assert_eq!(records(&f.0), original);
        if a.place_mode.is_some() { tap(&mut a, KeyCode::KeyP); }
    }
    start(&mut a, [-2., 0.], true);
    a.focus_changed(false);
    assert!(a.place_mode.is_none());
    a.focus_changed(true);
    assert_eq!(records(&f.0), original);
    start(&mut a, [-2., 0.], true);
    tap(&mut a, KeyCode::Enter);
    let mut rival = a.decoration_position().unwrap();
    rival.placement_id = "ffffffffffffffffffffffffffffffff".into();
    assert!(matches!(hair::placement_store::place_decoration_checked(&a.local.as_ref().unwrap().explorer, &rival, |_| Ok(())), hair::placement_store::Outcome::Committed { .. }));
    let before = records(&f.0);
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("overlap"), "{}", a.local_status);
    assert_eq!(records(&f.0), before);
    tap(&mut a, KeyCode::Escape);
    start(&mut a, [2., 0.], true);
    tap(&mut a, KeyCode::Enter);
    let lock = std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap();
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }, 0);
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("busy"), "{}", a.local_status);
    assert_eq!(records(&f.0), before);
    assert!(a.inventory.items().is_empty());
}
