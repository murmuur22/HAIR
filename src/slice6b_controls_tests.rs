use super::*;
use place_mode_native::Control;

fn frozen(a: &App) -> bool {
    a.place_mode.as_ref().and_then(|m| m.decoration_edit.as_ref()).is_some_and(|e| e.pending.is_some())
}
pub(super) fn copied_pack(a: &App, kind: &str, adapted: bool) -> Fixture {
    let f = fixture("slice6b-owned-pack");
    for e in std::fs::read_dir(a.root.join("packs")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|s| s == "glb") { std::fs::copy(&p, f.0.join(p.file_name().unwrap())).unwrap(); }
    }
    let text = std::fs::read_to_string(a.root.join(format!("packs/{kind}.toml"))).unwrap();
    let text = if adapted { text.replace("hair.storage.closed-bookcase", "hair.seating.chair") } else { text };
    std::fs::write(f.0.join("pack.toml"), text).unwrap();
    f
}
pub(super) fn use_pack(a: &mut App, pack: &Fixture) {
    a.themes[0] = hair::theme::Theme::load_for_folder_scene(&pack.0.join("pack.toml"), &mut Default::default(), &a.themes[0].host).unwrap();
}
fn pointer_at(a: &mut App, control: Control) {
    let (_, r, _) = a.toolbox_controls().into_iter().find(|(c, _, _)| *c == control).unwrap();
    a.pointer_moved(Some([f64::from(r[0] + 10.), f64::from(r[1] + 10.)]));
}
pub(super) fn pin_start(a: &mut App, at: [f32; 2]) {
    start(a, at, true); choose_pin(a);
}

#[test]
fn slice6b_choice_cancel_focus_navigation_write_free_and_no_file_edit_leak() {
    for reviewed in [false, true] {
        for exit in [Some(KeyCode::Escape), Some(KeyCode::KeyP), Some(KeyCode::F5), Some(KeyCode::PageDown), Some(KeyCode::KeyI), Some(KeyCode::KeyM), None] {
            let (f, mut a) = open_fixture("slice6b-exit");
            let before = records(&f.0);
            aim_file(&mut a, "A.txt"); tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
            assert!(a.place_mode.as_ref().unwrap().edit.is_some());
            start(&mut a, [-2., 0.], true);
            assert!(a.place_mode.as_ref().unwrap().edit.is_none());
            let follow = a.decoration_position().unwrap();
            // Rotation is eligible and state-changing for both Follow and Pin;
            // an already-selected source control could make this a vacuous check.
            reveal_control(&mut a, Control::Right);
            pointer_at(&mut a, Control::Right); a.place_pointer(true);
            assert_eq!(a.decoration_position().unwrap(), follow, "press alone must not rotate");
            a.pointer_moved(None); a.place_pointer(false);
            assert_eq!(a.decoration_position().unwrap(), follow, "drag-off release must not rotate");
            click(&mut a, Control::Right);
            let rotated = a.decoration_position().unwrap();
            assert_ne!(rotated.yaw, follow.yaw, "the same eligible control must really rotate on click");
            click(&mut a, Control::Left);
            assert_eq!(a.decoration_position().unwrap(), follow);
            assert_eq!(records(&f.0), before, "press/drag and positive control are write-free");
            choose_pin(&mut a);
            let retained = a.decoration_position().unwrap();
            tap(&mut a, KeyCode::KeyF);
            assert_eq!(a.decoration_position().unwrap(), retained, "F cannot downgrade a retained draft");
            tap(&mut a, KeyCode::KeyV); // Same choice through actual keyboard input.
            assert!(a.decoration_position().unwrap().is_pinned());
            if reviewed { tap(&mut a, KeyCode::Enter); assert!(frozen(&a), "{}", a.local_status); }
            if let Some(k) = exit {
                tap(&mut a, k);
                if k == KeyCode::Escape {
                    assert_eq!(a.editor_phase(), "Editing");
                    tap(&mut a, KeyCode::Escape);
                    assert_eq!(a.editor_phase(), "Toolbox");
                }
            } else { a.focus_changed(false); a.focus_changed(true); }
            assert_eq!(records(&f.0), before, "reviewed={reviewed}, exit={exit:?}");
            assert!(a.place_mode.as_ref().is_none_or(|m| m.decoration_edit.is_none()));
            assert!(a.inventory.items().is_empty() && a.placement_action.is_none());
        }
    }
}

#[test]
fn slice6b_frozen_intent_source_pose_held_enter_and_pointer_ownership() {
    let (f, mut a) = open_fixture("slice6b-frozen");
    pin_start(&mut a, [-2., 0.]); tap(&mut a, KeyCode::ArrowRight);
    let proposal = a.decoration_position().unwrap(); let before = records(&f.0);
    a.key(KeyCode::Enter, true, false).unwrap(); assert!(frozen(&a));
    for repeat in [false, true, true] { a.key(KeyCode::Enter, true, repeat).unwrap(); }
    for k in [KeyCode::KeyF, KeyCode::KeyV, KeyCode::KeyB, KeyCode::KeyN, KeyCode::KeyX, KeyCode::KeyH, KeyCode::ArrowLeft] { tap(&mut a, k); }
    aim_floor(&mut a, [2., 0.]);
    assert_eq!(a.decoration_position().unwrap(), proposal);
    assert!(a.toolbox_controls().iter().all(|(c, _, _)| matches!(c, Control::Enter | Control::Cancel | Control::Exit)));
    pointer_at(&mut a, Control::Enter); a.place_pointer(true);
    a.pointer_moved(None); a.place_pointer(false);
    assert_eq!(records(&f.0), before);
    a.key(KeyCode::Enter, false, false).unwrap();
    click(&mut a, Control::Enter);
    assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
    assert_eq!(layout(&a).decorations, vec![proposal]);
    let committed = records(&f.0); tap(&mut a, KeyCode::Enter);
    assert_eq!(records(&f.0), committed, "no duplicate retry after retirement");
}

#[test]
// HISTORICAL Follow -> explicit V conversion fixture, not current catalog/N UI.
// Actual N acquisition/refusal is independently covered by slice6c.
fn slice6b_legacy_follow_v_acquisition_refusal_keeps_exact_candidate_and_original_sources() {
    for kind in ["asset-bytes", "manifest", "symlink", "permissions", "entry", "asset-fit", "source"] {
        let (f, mut a) = open_fixture("slice6b-acquire-refuse");
        let pack = copied_pack(&a, "office", false); use_pack(&mut a, &pack);
        start(&mut a, [-2., 0.], true);
        let follow = a.decoration_position().unwrap(); let preview = a.place_preview().unwrap();
        match kind {
            "asset-bytes" => std::fs::write(pack.0.join("chairDesk.glb"), b"changed fixture").unwrap(),
            "manifest" => { let p = pack.0.join("pack.toml"); let t = std::fs::read_to_string(&p).unwrap(); std::fs::write(p, format!("{t}\n# changed fixture\n")).unwrap(); }
            "symlink" => { let p = pack.0.join("chairDesk.glb"); std::fs::rename(&p, pack.0.join("held.glb")).unwrap(); std::os::unix::fs::symlink("held.glb", p).unwrap(); }
            "permissions" => { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(pack.0.join("chairDesk.glb"), std::fs::Permissions::from_mode(0o666)).unwrap(); }
            "entry" => a.themes[0].decorations.entries[0].asset = "office-desk".into(),
            "asset-fit" => a.themes[0].decorations.assets[0].spec.scale = 1.1,
            "source" => a.themes[0].source = a.root.join("packs/office.toml"),
            _ => unreachable!(),
        }
        let before = records(&f.0);
        tap(&mut a, KeyCode::KeyV);
        assert_eq!(a.decoration_position().unwrap(), follow, "{kind}: {}", a.local_status);
        assert_eq!(bytemuck::cast_slice::<_, u8>(&a.place_preview().unwrap()), bytemuck::cast_slice::<_, u8>(&preview));
        assert!(!a.local_status.contains("New pin"), "{kind}: {}", a.local_status);
        assert_eq!(records(&f.0), before);
        tap(&mut a, KeyCode::Escape); tap(&mut a, KeyCode::Enter);
        assert_eq!(records(&f.0), before);
        assert!(a.inventory.items().is_empty() && a.placement_action.is_none());
    }
}

#[test]
fn slice6b_held_source_not_reacquired_on_redraw_or_frozen_catalog_edits() {
    for kind in ["asset", "manifest", "renamed-manifest", "renamed-directory", "catalog"] {
        let (f, mut a) = open_fixture("slice6b-stale-source");
        let pack = copied_pack(&a, "office", false); use_pack(&mut a, &pack);
        let moved = fixture("slice6b-owned-rename-target");
        pin_start(&mut a, [-2., 0.]); tap(&mut a, KeyCode::Enter); assert!(frozen(&a));
        let proposed = a.decoration_position().unwrap(); let preview = a.place_preview().unwrap();
        match kind {
            "asset" => std::fs::write(pack.0.join("chairDesk.glb"), b"stale fixture").unwrap(),
            "manifest" => { let p = pack.0.join("pack.toml"); let t = std::fs::read_to_string(&p).unwrap(); std::fs::write(p, format!("{t}\n# stale fixture\n")).unwrap(); }
            "renamed-manifest" => std::fs::rename(pack.0.join("pack.toml"), pack.0.join("removed.toml")).unwrap(),
            "renamed-directory" => { std::fs::rename(&pack.0, moved.0.join("pack")).unwrap(); std::fs::create_dir(&pack.0).unwrap(); }
            "catalog" => { a.themes[0].decorations.entries.clear(); a.themes[0].decorations.assets.clear(); }
            _ => unreachable!(),
        }
        let before = records(&f.0);
        for _ in 0..3 { a.place_ui(); a.update_hud(); }
        assert_eq!(a.decoration_position().unwrap(), proposed);
        assert_eq!(bytemuck::cast_slice::<_, u8>(&a.place_preview().unwrap()), bytemuck::cast_slice::<_, u8>(&preview));
        assert_eq!(records(&f.0), before, "redraw does not reacquire or write");
        tap(&mut a, KeyCode::Enter);
        if kind == "catalog" {
            assert!(a.local_status.contains("decoration pinned"), "immutable checked source is authoritative, not public catalog: {}", a.local_status);
            assert_eq!(layout(&a).decorations, vec![proposed]);
        } else {
            assert!(!a.local_status.contains("decoration pinned"), "{kind}: {}", a.local_status);
            assert_eq!(records(&f.0), before);
            assert!(layout(&a).decorations.is_empty());
        }
    }
}

#[test]
fn slice6b_late_geometry_visitor_listing_and_busy_storage_refuse() {
    use std::os::fd::AsRawFd;
    for kind in ["geometry", "visitor", "listing", "busy"] {
        let (f, mut a) = open_fixture("slice6b-late");
        // An actual saved comparison supplies the real storage lock.
        start(&mut a, [0., -1.], true); tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
        pin_start(&mut a, [-2., 0.]); tap(&mut a, KeyCode::Enter); assert!(frozen(&a));
        let proposed = a.decoration_position().unwrap();
        let mut held = None;
        match kind {
            "geometry" => {
                let mut rival = hair::placement_store::Decoration::from_entry(&a.themes[0].decorations.entries[0], proposed.at, 0., proposed.size).unwrap();
                rival.collision = false;
                assert!(matches!(hair::placement_store::place_decoration_checked(&a.local.as_ref().unwrap().explorer, &rival, |_| Ok(())), hair::placement_store::Outcome::Committed { .. }));
            }
            "visitor" => a.pose.position = proposed.at,
            "listing" => for i in 0..20 { std::fs::write(f.0.join(format!("M{i:02}.txt")), b"owned late listing").unwrap(); },
            "busy" => { let file = std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap(); assert_eq!(unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }, 0); held = Some(file); }
            _ => unreachable!(),
        }
        let before = records(&f.0); tap(&mut a, KeyCode::Enter);
        let reason = match kind { "geometry" | "listing" => "overlap", "visitor" => "unreachable", "busy" => "busy", _ => unreachable!() };
        assert!(a.local_status.contains(reason), "{kind}: {}", a.local_status);
        assert!(!frozen(&a)); assert_eq!(records(&f.0), before);
        drop(held);
        assert!(!layout(&a).decorations.iter().any(|d| d.placement_id == proposed.placement_id));
    }
}

#[test]
fn slice6b_failed_new_catalog_action_cannot_leave_file_enter_armed() {
    let (f, mut a) = open_fixture("slice6b-failed-new");
    aim_file(&mut a, "A.txt"); tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
    assert!(a.place_mode.as_ref().unwrap().edit.is_some());
    let before = records(&f.0);
    // N without a catalog selection refuses, retiring the previous file intent.
    tap(&mut a, KeyCode::KeyN);
    assert!(a.local_status.contains("Select a catalog decoration"));
    assert!(a.place_mode.as_ref().unwrap().edit.is_none());
    tap(&mut a, KeyCode::KeyV); tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert_eq!(records(&f.0), before);
    assert!(a.inventory.items().is_empty() && a.placement_action.is_none());
}

#[test]
fn slice6b_complete_appearance_refusal_at_review_and_safe_theme_prepublication() {
    for late in [false, true] {
        let (f, mut a) = open_fixture("slice6b-scene-resource");
        pin_start(&mut a, [-2., 0.]);
        let proposed = a.decoration_position().unwrap();
        if late { tap(&mut a, KeyCode::Enter); assert!(frozen(&a)); }
        // Labelled synthetic scene-resource pressure. Exact loaded pin, normal
        // complete Theme admission; no budget-policy edits or fabricated GLB.
        let vertex = a.themes[0].vertices[0];
        a.themes[0].vertices.resize(300_001, vertex);
        let before = records(&f.0); tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("Decoration scene exceeds 300000 vertices"), "late={late}: {}", a.local_status);
        assert_eq!(records(&f.0), before, "safe Theme preflight refuses before preparing storage");
        assert!(!f.0.join(".hair-layout").exists());
        assert_eq!(a.decoration_position().unwrap(), proposed, "intent, id and retained closure survive refusal");
    }
}
