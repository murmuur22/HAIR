use super::*;

#[test]
fn held_shift_doubles_world_movement_and_release_restores_walk() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    let start = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [-2.5, 3.0],
        yaw: 0.0,
        pitch: 0.0,
    };
    app.input.captured = true;
    app.keyboard_input(Some(KeyCode::KeyW), true, false, None)
        .unwrap();
    let mut walk = start;
    app.themes[0].locomote(&mut walk, &app.input, 0.1);
    let walk_distance = start.position[1] - walk.position[1];
    assert!(walk_distance > 0.2 && walk_distance < 0.3);
    for shift in [KeyCode::ShiftLeft, KeyCode::ShiftRight] {
        app.keyboard_input(Some(shift), true, false, None).unwrap();
        let mut run = start;
        app.themes[0].locomote(&mut run, &app.input, 0.1);
        assert!(
            (start.position[1] - run.position[1] - walk_distance * 2.0).abs() < 0.0001,
            "holding {shift:?} must run at twice walking speed"
        );
        app.keyboard_input(Some(shift), false, false, None).unwrap();
        let mut released = start;
        app.themes[0].locomote(&mut released, &app.input, 0.1);
        assert!((released.position[1] - walk.position[1]).abs() < 0.0001);
    }
}

// Qualification preparation only. No new movement API or production setters.
fn app() -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    )
}
fn key(a: &mut App, code: KeyCode, down: bool) {
    assert!(!a.keyboard_input(Some(code), down, false, None).unwrap());
}
fn tap(a: &mut App, code: KeyCode) {
    key(a, code, true);
    key(a, code, false);
}
fn start() -> Pose {
    Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [-2.5, 3.0],
        yaw: 0.0,
        pitch: 0.0,
    }
}
fn moved(a: &App, from: Pose, dt: f32) -> Pose {
    let mut p = from;
    a.themes[a.theme_index].locomote(&mut p, &a.input, dt);
    assert!(
        p.position
            .iter()
            .chain([&p.yaw, &p.pitch])
            .all(|v| v.is_finite())
    );
    assert!(!a.themes[a.theme_index].host.blocked(p.position));
    p
}
fn distance(a: Pose, b: Pose) -> f32 {
    (a.position[0] - b.position[0]).hypot(a.position[1] - b.position[1])
}
fn stopped(a: &App) {
    assert!(
        !a.input.running
            && !a.input.forward
            && !a.input.back
            && !a.input.left
            && !a.input.right
            && !a.input.captured
    );
    assert_eq!(moved(a, start(), 0.1).position, start().position);
}
#[test]
fn both_shift_modifiers_preserve_run_until_last_release_in_either_order() {
    for order in [
        [KeyCode::ShiftLeft, KeyCode::ShiftRight],
        [KeyCode::ShiftRight, KeyCode::ShiftLeft],
    ] {
        let mut a = app();
        a.input.captured = true; // pointer prerequisite, not OS mouse input
        key(&mut a, KeyCode::KeyW, true);
        for modifier in order {
            key(&mut a, modifier, true);
        }
        assert!((distance(start(), moved(&a, start(), 0.1)) - 0.5).abs() < 0.0001);
        key(&mut a, order[0], false);
        assert!(a.input.running);
        assert!((distance(start(), moved(&a, start(), 0.1)) - 0.5).abs() < 0.0001);
        key(&mut a, order[1], false);
        assert!(!a.input.running);
        assert!((distance(start(), moved(&a, start(), 0.1)) - 0.25).abs() < 0.0001);
    }
}
#[test]
fn running_diagonals_dt_caps_and_host_collisions_remain_bounded() {
    let mut a = app();
    a.input.captured = true;
    key(&mut a, KeyCode::ShiftLeft, true);
    key(&mut a, KeyCode::KeyW, true);
    let straight = distance(start(), moved(&a, start(), 0.1));
    key(&mut a, KeyCode::KeyD, true);
    assert!(
        (distance(start(), moved(&a, start(), 0.1)) - straight).abs() < 0.0001,
        "diagonal must be normalized"
    );
    for dt in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(moved(&a, start(), dt).position, start().position, "dt={dt}");
    }
    let capped = moved(&a, start(), 0.25);
    assert!((distance(start(), capped) - 1.25).abs() < 0.0001);
    assert_eq!(moved(&a, start(), f32::MAX).position, capped.position);
    key(&mut a, KeyCode::KeyW, false);
    key(&mut a, KeyCode::KeyS, true);
    let h = &a.themes[0].host;
    let corner = Pose {
        feet_y: 0.0,
        position: [h.room[2] - 0.01, h.room[3] - 0.01],
        ..start()
    };
    assert_eq!(
        moved(&a, corner, 1000.0).position,
        corner.position,
        "run cannot cross room bounds"
    );
    key(&mut a, KeyCode::KeyS, false);
    let d = a.themes[0].host.anchor("document");
    let object = Pose {
        feet_y: 0.0,
        position: [d.at[0] - d.size[0] * 0.5 - 0.22 - 0.01, d.at[2]],
        ..start()
    };
    let blocked = moved(&a, object, 1000.0);
    assert!(
        blocked.position[0] < d.at[0] - d.size[0] * 0.5 - 0.22,
        "finite capped run cannot tunnel through host object clearance"
    );
    assert!((blocked.position[0] - object.position[0]).abs() < 0.02);
}
#[test]
fn shift_tab_reverse_targeting_keeps_run_and_tab_repeat_latch() {
    for shift in [KeyCode::ShiftLeft, KeyCode::ShiftRight] {
        let mut a = super::contextual_tests::two_targets();
        a.input.captured = true;
        key(&mut a, KeyCode::KeyW, true);
        tap(&mut a, KeyCode::Tab);
        assert_eq!(
            a.world.interaction.as_ref().unwrap().anchor,
            "main-document"
        );
        key(&mut a, shift, true);
        key(&mut a, KeyCode::Tab, true);
        assert_eq!(a.world.interaction.as_ref().unwrap().anchor, "main-link");
        for repeat in [true, false, true] {
            assert!(
                !a.keyboard_input(Some(KeyCode::Tab), true, repeat, None)
                    .unwrap()
            );
            assert_eq!(a.world.interaction.as_ref().unwrap().anchor, "main-link");
        }
        assert!(a.input.running && a.input.forward);
        assert!((distance(start(), moved(&a, start(), 0.1)) - 0.5).abs() < 0.0001);
        key(&mut a, KeyCode::Tab, false);
        key(&mut a, shift, false);
        tap(&mut a, KeyCode::Tab);
        assert_eq!(
            a.world.interaction.as_ref().unwrap().anchor,
            "main-document"
        );
        assert!(!a.input.running);
    }
}
#[test]
fn focus_loss_and_actual_escape_capture_release_stop_sprinting() {
    for focus in [false, true] {
        let mut a = app();
        a.input.captured = true;
        key(&mut a, KeyCode::KeyW, true);
        key(&mut a, KeyCode::ShiftLeft, true);
        key(&mut a, KeyCode::ShiftRight, true);
        if focus {
            a.focus_changed(false);
        } else {
            tap(&mut a, KeyCode::Escape);
        }
        stopped(&a);
        for shift in [KeyCode::ShiftLeft, KeyCode::ShiftRight] {
            key(&mut a, shift, false);
        }
        if !focus {
            // Captured Escape unlocks without pausing; preserve the walk oracle.
            assert!(a.menu.is_none());
            stopped(&a);
        }
        if focus {
            key(&mut a, KeyCode::KeyW, true);
            key(&mut a, KeyCode::ShiftLeft, true);
            stopped(&a);
            a.focus_changed(true);
        }
        a.input.captured = true;
        key(&mut a, KeyCode::KeyW, true);
        assert!(!a.input.running);
        assert!((distance(start(), moved(&a, start(), 0.1)) - 0.25).abs() < 0.0001);
    }
}
#[test]
fn actual_reader_address_and_folder_dialog_isolate_running_and_release_order() {
    for shift in [KeyCode::ShiftLeft, KeyCode::ShiftRight] {
        // Bundled demo document: actual E opens the reader; no reading setter
        // is used to enter this transition (separate link fixture below).
        let mut a = app();
        let d = a.themes[0].host.anchor("document");
        a.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [d.at[0], d.at[2] + 1.5],
            yaw: 0.0,
            pitch: -0.1,
        };
        hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
        assert_eq!(
            a.world.interaction.as_ref().unwrap().kind,
            hair::interaction::Kind::Read
        );
        a.input.captured = true;
        key(&mut a, KeyCode::KeyW, true);
        key(&mut a, shift, true);
        tap(&mut a, KeyCode::KeyE);
        assert!(a.world.reading);
        stopped(&a);
        key(&mut a, shift, false);
        key(&mut a, KeyCode::ShiftRight, true);
        key(&mut a, KeyCode::KeyW, true);
        stopped(&a);
        key(&mut a, KeyCode::ShiftRight, false);
        tap(&mut a, KeyCode::Escape);
        assert!(!a.world.reading);
        stopped(&a);

        a.input.captured = true;
        key(&mut a, KeyCode::KeyW, true);
        key(&mut a, shift, true);
        tap(&mut a, KeyCode::KeyL);
        assert!(a.address.is_some());
        stopped(&a);
        key(&mut a, shift, false); // release while address consumes actions
        a.keyboard_input(Some(KeyCode::KeyW), true, false, Some("w"))
            .unwrap();
        key(&mut a, KeyCode::KeyW, false);
        assert_eq!(a.address.as_deref(), Some("w"));
        stopped(&a);
        tap(&mut a, KeyCode::Escape);
        assert!(a.address.is_none());
        stopped(&a);

        a.input.captured = true;
        key(&mut a, KeyCode::KeyW, true);
        key(&mut a, shift, true);
        tap(&mut a, KeyCode::KeyO);
        assert!(a.chooser_pending);
        stopped(&a);
        // Claim gate prerequisite, not a claim that a GPU frame was presented.
        a.frames = 1;
        assert!(a.claim_folder_request());
        assert!(!a.claim_folder_request());
        key(&mut a, shift, false);
        key(&mut a, KeyCode::KeyW, true);
        key(&mut a, KeyCode::ShiftRight, true);
        stopped(&a);
        a.complete_folder(Ok(None)).unwrap();
        stopped(&a);
        key(&mut a, KeyCode::ShiftRight, false);
        a.input.captured = true;
        key(&mut a, KeyCode::KeyW, true);
        assert!(
            !a.input.running,
            "cancelled dialog cannot resurrect held run"
        );
    }
}
#[test]
fn capture_release_does_not_resurrect_stale_other_shift_after_recapture() {
    let mut a = app();
    a.input.captured = true;
    key(&mut a, KeyCode::KeyW, true);
    key(&mut a, KeyCode::ShiftLeft, true);
    tap(&mut a, KeyCode::Escape);
    stopped(&a);
    // First captured Escape releases without pausing or leaving a run latch.
    assert!(a.menu.is_none());
    stopped(&a);
    // Fresh right modifier after a context reset must not inherit the old left
    // latch. This is a suspected existing defect, NOT permission to fix runtime.
    a.input.captured = true;
    key(&mut a, KeyCode::KeyW, true);
    key(&mut a, KeyCode::ShiftRight, true);
    key(&mut a, KeyCode::ShiftRight, false);
    assert!(
        !a.input.running,
        "capture reset retained stale ShiftLeft in App.shifts"
    );
    assert!((distance(start(), moved(&a, start(), 0.1)) - 0.25).abs() < 0.0001);
}
#[test]
fn reader_shift_tab_still_selects_links_without_world_running() {
    let mut a = super::contextual_tests::reader_app();
    key(&mut a, KeyCode::ShiftRight, true);
    tap(&mut a, KeyCode::Tab);
    assert_eq!(a.world.selected_reader_link().unwrap().ordinal, 2);
    assert!(!a.input.running && !a.input.forward);
    key(&mut a, KeyCode::ShiftRight, false);
    tap(&mut a, KeyCode::Tab);
    assert_eq!(a.world.selected_reader_link().unwrap().ordinal, 0);
    assert!(!a.input.running);
}
