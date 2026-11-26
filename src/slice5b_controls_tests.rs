use super::*;

fn frozen(a: &App) -> bool {
    a.place_mode.as_ref().and_then(|m| m.decoration_edit.as_ref()).is_some_and(|e| e.pending.is_some())
}
fn review(a: &mut App, old: &Decoration, at: [f32; 2]) -> Decoration {
    select_saved(a, old); aim_floor(a, at); tap(a, KeyCode::Enter);
    assert!(frozen(a), "{}", a.local_status);
    a.decoration_position().unwrap()
}
fn external_move(a: &App, old: &Decoration) -> Decoration {
    // Labelled rival-writer setup through the existing checked storage API.
    let mut changed = old.clone(); changed.yaw = 0.4;
    let local = a.local.as_ref().unwrap();
    let out = hair::placement_store::relocate_decoration_checked(&local.explorer, old, &changed, |candidate|
        hair::placement_scene::validate_decoration_candidate(&local.explorer, candidate, a.pose.position, local.page));
    assert!(matches!(out, Outcome::Committed { .. }), "rival setup: {out:?}");
    changed
}
fn point_control(a: &mut App, control: place_mode_native::Control) {
    let (_, r, _) = a.toolbox_controls().into_iter().find(|(c, _, _)| *c == control).unwrap();
    a.pointer_moved(Some([f64::from(r[0] + 10.), f64::from(r[1] + 10.)]));
}

#[test]
fn slice5b_cancel_focus_navigation_and_repeat_are_write_free_in_preview_and_review() {
    for reviewed in [false, true] {
        for exit in [Some(KeyCode::Escape), Some(KeyCode::KeyP), Some(KeyCode::F5),
            Some(KeyCode::PageDown), Some(KeyCode::KeyI), Some(KeyCode::KeyM), None] {
            let (f, mut a) = open_fixture("slice5b-exit");
            let old = save_new(&mut a, [-2., 0.]);
            let before = records(&f.0); let saved = layout(&a);
            select_saved(&mut a, &old); aim_floor(&mut a, [2., 0.]);
            if reviewed { tap(&mut a, KeyCode::Enter); assert!(frozen(&a), "{}", a.local_status); }
            if let Some(key) = exit {
                tap(&mut a, key);
                if key == KeyCode::Escape {
                    assert_eq!(a.editor_phase(), "Editing");
                    tap(&mut a, KeyCode::Escape);
                    assert_eq!(a.editor_phase(), "Toolbox");
                }
            }
            else { a.focus_changed(false); a.focus_changed(true); }
            assert_eq!(records(&f.0), before, "reviewed={reviewed} exit={exit:?}");
            assert_eq!(layout(&a), saved);
            assert!(a.place_mode.as_ref().is_none_or(|m| m.decoration_edit.is_none()));
            assert!(a.inventory.items().is_empty() && a.placement_action.is_none());
        }
    }
}

#[test]
fn slice5b_fresh_enter_frozen_expected_pointer_ownership_and_recapture() {
    use place_mode_native::Control;
    let (f, mut a) = open_fixture("slice5b-owned-input");
    let old = save_new(&mut a, [-2., 0.]);
    select_saved(&mut a, &old); aim_floor(&mut a, [2., 0.]);
    a.pointer_moved(Some([300., 450.])); a.place_pointer(true); a.place_pointer(false);
    assert!(a.input.captured, "world click recaptures only");
    let before = records(&f.0);
    a.key(KeyCode::KeyW, true, false).unwrap();
    a.key(KeyCode::Enter, true, false).unwrap();
    assert!(frozen(&a), "{}", a.local_status);
    let proposal = a.decoration_position().unwrap();
    assert!(!a.input.captured && !a.input.forward);
    for repeat in [false, true, true] { a.key(KeyCode::Enter, true, repeat).unwrap(); }
    assert_eq!(records(&f.0), before, "held/repeated Enter must not confirm");
    a.key(KeyCode::Enter, false, false).unwrap(); a.key(KeyCode::KeyW, false, false).unwrap();
    for key in [KeyCode::ArrowRight, KeyCode::KeyH, KeyCode::KeyB, KeyCode::KeyX,
        KeyCode::KeyN, KeyCode::KeyG, KeyCode::KeyC] { tap(&mut a, key); }
    assert_eq!(a.decoration_position().unwrap(), proposal);
    assert_eq!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().expected(), Some(&old));
    assert!(a.toolbox_controls().iter().all(|(c, _, _)| matches!(c, Control::Enter | Control::Cancel | Control::Exit)));
    point_control(&mut a, Control::Enter); a.place_pointer(true);
    a.pointer_moved(None); a.place_pointer(false);
    assert_eq!(records(&f.0), before, "drag off save is inert");
    point_control(&mut a, Control::Enter); a.place_pointer(true);
    tap(&mut a, KeyCode::Escape); a.place_pointer(false);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(records(&f.0), before, "cancelled pointer press cannot save later");
    assert_eq!(a.editor_phase(), "Positioning");
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::Escape);
    review(&mut a, &old, [2., 0.]);
    click(&mut a, Control::Enter); // A fresh pointer press/release can confirm.
    assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
    assert_eq!(layout(&a).decorations, vec![proposal]);
    assert!(a.place_mode.as_ref().unwrap().edit.is_none());
    assert!(a.inventory.items().is_empty() && a.placement_action.is_none());
}

#[test]
fn slice5b_stale_expected_refuses_at_review_and_again_under_commit_lock() {
    for late in [false, true] {
        let (f, mut a) = open_fixture("slice5b-stale");
        let old = save_new(&mut a, [-2., 0.]);
        select_saved(&mut a, &old); aim_floor(&mut a, [2., 0.]);
        if late { tap(&mut a, KeyCode::Enter); assert!(frozen(&a)); }
        let changed = external_move(&a, &old);
        let before = records(&f.0);
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("Stale expected"), "late={late}: {}", a.local_status);
        assert!(!frozen(&a)); assert_eq!(layout(&a).decorations, vec![changed]);
        assert_eq!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().expected(), Some(&old));
        let status = a.local_status.clone(); a.update_hud(); assert_eq!(a.local_status, status);
        assert_eq!(records(&f.0), before, "stale selection cannot overwrite a newer record");
    }
}

#[test]
fn slice5b_busy_writer_and_retained_pending_refuse_without_repair() {
    use std::os::fd::AsRawFd;
    let (f, mut a) = open_fixture("slice5b-busy");
    let old = save_new(&mut a, [-2., 0.]);
    review(&mut a, &old, [2., 0.]);
    let lock = std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap();
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }, 0);
    let before = records(&f.0); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("busy"), "{}", a.local_status);
    assert_eq!(records(&f.0), before); assert!(!frozen(&a));
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_UN) }, 0);
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::Escape);
    review(&mut a, &old, [2., 0.]);
    // Real rival prepared refusal retains its pending evidence (not a fabricated record).
    let mut rival = old.clone(); rival.yaw = 0.5; let mut calls = 0;
    let out = hair::placement_store::relocate_decoration_checked(&a.local.as_ref().unwrap().explorer,
        &old, &rival, |_| { calls += 1; if calls == 2 { Err("owned fixture late refusal".into()) } else { Ok(()) } });
    assert_eq!(out, Outcome::NotCommitted("owned fixture late refusal".into())); assert_eq!(calls, 2);
    let before = records(&f.0); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("recovery"), "{}", a.local_status);
    assert_eq!(records(&f.0), before); assert_eq!(layout(&a).decorations, vec![old]);
    assert_eq!(layout(&a).incomplete_records, 1);
    tap(&mut a, KeyCode::Escape); tap(&mut a, KeyCode::F5);
    assert_eq!(records(&f.0), before, "refresh does not clear pending evidence");
}

#[test]
fn slice5b_late_geometry_listing_theme_and_visitor_refuse_without_writes() {
    for late in ["decoration", "listing", "missing-tag", "ambiguous-tag", "visitor"] {
        let (f, mut a) = open_fixture("slice5b-late");
        let old = save_new(&mut a, [-2., 0.]);
        let proposal = review(&mut a, &old, [1.5, 0.]);
        match late {
            "decoration" => {
                let mut rival = proposal.clone(); rival.placement_id = "d".repeat(32);
                let out = hair::placement_store::place_decoration_checked(&a.local.as_ref().unwrap().explorer, &rival, |_| Ok(()));
                assert!(matches!(out, Outcome::Committed { .. }), "rival setup: {out:?}");
            }
            "listing" => for i in 0..20 { std::fs::write(f.0.join(format!("M{i:02}.txt")), b"late owned listing").unwrap(); },
            "missing-tag" => a.themes[0].decorations.entries.clear(),
            "ambiguous-tag" => { let duplicate = a.themes[0].decorations.entries[0].clone(); a.themes[0].decorations.entries.push(duplicate); }
            "visitor" => a.pose.position = proposal.at,
            _ => unreachable!(),
        }
        let before = records(&f.0); tap(&mut a, KeyCode::Enter);
        assert!(!frozen(&a), "{late}: {}", a.local_status);
        if late == "missing-tag" {
            assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
            assert!(layout(&a).decorations.contains(&proposal));
            continue;
        }
        assert!(!a.local_status.contains("reposition saved"), "{late}: {}", a.local_status);
        let message = match late { "decoration" => "overlap", "listing" => "unreachable", "missing-tag" => "missing decoration tag",
            "ambiguous-tag" => "Ambiguous theme decoration tag", "visitor" => "unreachable", _ => unreachable!() };
        assert!(a.local_status.contains(message), "{late}: {}", a.local_status);
        assert!(a.local_status.contains("no relocation committed by this attempt"));
        assert_eq!(records(&f.0), before, "{late}: refuse before preparing any pending metadata");
        assert!(layout(&a).decorations.contains(&old));
        assert_eq!(layout(&a).incomplete_records, 0);
    }
}

#[test]
fn slice5b_live_page_guard_before_review_and_before_commit() {
    for late in [false, true] {
        let (f, mut a) = open_fixture("slice5b-page");
        let old = save_new(&mut a, [-2., 0.]);
        select_saved(&mut a, &old); aim_floor(&mut a, [2., 0.]);
        if late { tap(&mut a, KeyCode::Enter); assert!(frozen(&a)); }
        a.local.as_mut().unwrap().page = 100;
        let before = records(&f.0); tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("page is out of range"), "{}", a.local_status);
        assert_eq!(records(&f.0), before);
        assert_eq!(layout(&a).decorations, vec![old]);
    }
}

#[test]
fn slice5b_same_tag_instances_move_independently_and_ghost_omission_is_render_only() {
    let (f, mut a) = open_fixture("slice5b-ghost");
    let first = save_new(&mut a, [-2., 0.]); let second = save_new(&mut a, [0., -1.]);
    assert_eq!(first.tag, second.tag); assert_ne!(first.placement_id, second.placement_id);
    for (old, at) in [(&first, [-1.85, 0.]), (&second, [0., -0.8])] {
        let host = a.themes[0].host.clone(); let before = records(&f.0);
        let full = a.place_scene_at(0.); let mesh = a.themes[0].decoration_preview(old).unwrap();
        // Exact production scene equality after removing only the selected baked mesh.
        let index = full.windows(mesh.len()).position(|part| part.iter().zip(&mesh).all(|(v, m)|
            v.position == m.position && v.color == m.color && v.uv == m.uv)).expect("saved appearance mesh in real scene");
        let mut expected = full.clone(); expected.drain(index..index + mesh.len());
        select_saved(&mut a, old);
        assert_eq!(bytemuck::cast_slice::<_, u8>(&a.place_scene_at(0.)), bytemuck::cast_slice::<_, u8>(&expected));
        assert_eq!(bytemuck::cast_slice::<_, u8>(&a.place_scene_at(3.)), bytemuck::cast_slice::<_, u8>(&expected));
        assert_eq!(a.themes[0].host, host);
        assert!(hair::player::blocked_at(&a.themes[0].host, old.at, 0.), "old collision stays active until commit");
        aim_saved(&mut a, old); // Target stays present despite its render-only omission.
        aim_floor(&mut a, at); a.place_ui();
        assert!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().preview_reason.is_none(), "own old envelope must not reject replacement snapshot");
        assert_eq!(records(&f.0), before);
        tap(&mut a, KeyCode::Escape);
        assert_eq!(a.editor_phase(), "Editing");
        tap(&mut a, KeyCode::Escape);
        assert_eq!(bytemuck::cast_slice::<_, u8>(&a.place_scene_at(0.)), bytemuck::cast_slice::<_, u8>(&full));
        let proposed = review(&mut a, old, at); tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
        assert_eq!(layout(&a).decorations.len(), 2);
        assert_eq!(a.themes[0].host.decorations[&old.placement_id], proposed);
        let other = if old.placement_id == first.placement_id { &second } else { &first };
        assert_eq!(a.themes[0].host.decorations[&other.placement_id], host.decorations[&other.placement_id]);
    }
}

#[test]
fn slice5b_missing_art_selection_retires_stale_file_edit_and_cancel_is_write_free() {
    let (f, mut a) = open_fixture("slice5b-no-file-leak");
    let old = save_new(&mut a, [-2., 0.]);
    aim_file(&mut a, "A.txt"); tap(&mut a, KeyCode::KeyX);
    assert!(a.place_mode.as_ref().unwrap().edit.is_some());
    aim_saved(&mut a, &old); a.themes[0].decorations.entries.clear();
    let before = records(&f.0); tap(&mut a, KeyCode::KeyX);
    assert!(a.local_status.contains("Missing artwork"));
    assert!(a.place_mode.as_ref().unwrap().edit.is_none());
    tap(&mut a, KeyCode::Escape);
    assert_eq!(records(&f.0), before);
    assert!(a.inventory.items().is_empty() && a.placement_action.is_none());
}
