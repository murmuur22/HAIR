use super::*;

#[test]
fn release_then_deliberate_e_follows_but_held_e_does_not() {
    let mut a = app();
    a.world.reading = true; // Reader fixture; every action below uses the native keyboard adapter.
    press(&mut a, KeyCode::Tab);
    a.world.reader_link = None; // Unsuccessful E prerequisite, as in the reviewer's probe.
    a.keyboard_input(Some(KeyCode::KeyE), true, false, None)
        .unwrap();
    press(&mut a, KeyCode::KeyL);
    press(&mut a, KeyCode::Escape);
    press(&mut a, KeyCode::Tab);
    let destination = match &a.world.selected_reader_link().unwrap().link.destination {
        hair::content::Destination::External(raw) => raw.clone(),
        _ => panic!("HTTPS fixture required"),
    };
    a.keyboard_input(Some(KeyCode::KeyE), true, false, None)
        .unwrap();
    assert!(
        a.web.as_ref().unwrap().pending.is_none(),
        "entry must not erase the held E latch"
    );
    press(&mut a, KeyCode::KeyL);
    a.keyboard_input(Some(KeyCode::KeyE), false, false, None)
        .unwrap();
    press(&mut a, KeyCode::Escape);
    a.keyboard_input(Some(KeyCode::KeyE), true, false, None)
        .unwrap();
    assert!(
        a.web.as_ref().unwrap().pending.is_some(),
        "release must re-arm deliberate E"
    );
    a.keyboard_input(Some(KeyCode::KeyE), true, true, None)
        .unwrap();
    a.keyboard_input(Some(KeyCode::KeyE), true, false, None)
        .unwrap();
    a.keyboard_input(Some(KeyCode::KeyE), false, false, None)
        .unwrap();
    poll(&mut a);
    assert_eq!(a.world.current, destination);
    assert_eq!(a.web.as_ref().unwrap().history.len(), 1);
    println!(
        "ORDER E down -> L/Escape -> duplicate E suppressed -> L -> E up -> Escape -> E down/repeat/duplicate/up -> exactly one arrival"
    );
}

#[test]
fn tab_hold_survives_entry_and_both_shift_releases_restore_forward_cycle() {
    for shift in [KeyCode::ShiftLeft, KeyCode::ShiftRight] {
        let mut a = app();
        a.world.reading = true;
        let first = a.world.reader_links()[0].clone();
        a.keyboard_input(Some(shift), true, false, None).unwrap();
        press(&mut a, KeyCode::KeyL);
        a.keyboard_input(Some(shift), false, false, None).unwrap();
        // Text events still append literally without world action.
        a.keyboard_input(Some(KeyCode::KeyE), true, false, Some("e"))
            .unwrap();
        a.keyboard_input(Some(KeyCode::KeyE), false, false, None)
            .unwrap();
        assert_eq!(a.address.as_deref(), Some("e"));
        press(&mut a, KeyCode::Escape);
        a.keyboard_input(Some(KeyCode::Tab), true, false, None)
            .unwrap();
        assert_eq!(a.world.selected_reader_link().unwrap().link, first);
        press(&mut a, KeyCode::KeyL);
        press(&mut a, KeyCode::Escape);
        a.keyboard_input(Some(KeyCode::Tab), true, false, None)
            .unwrap();
        assert_eq!(
            a.world.selected_reader_link().unwrap().link,
            first,
            "held Tab must remain suppressed"
        );
        press(&mut a, KeyCode::KeyL);
        a.keyboard_input(Some(KeyCode::Tab), false, false, None)
            .unwrap();
        press(&mut a, KeyCode::Escape);
        press(&mut a, KeyCode::Tab);
        assert_ne!(a.world.selected_reader_link().unwrap().link, first);
        println!(
            "ORDER {shift:?} down -> L -> shift up -> literal e down/up -> Escape -> forward Tab -> L/Escape -> held Tab suppressed -> L -> Tab up -> Escape -> next Tab cycles"
        );
    }
}

#[test]
fn asynchronous_error_release_edit_retry_and_local_cancel_are_ordered() {
    let mut a = local(fixture);
    let current = a.world.current.clone();
    let host = a.themes[0].host.clone();
    let source = a.themes[0].source.clone();
    let pose = a.pose;
    submit(&mut a, "https://example.org/bad");
    a.keyboard_input(Some(KeyCode::KeyW), true, false, None)
        .unwrap();
    a.input.captured = true; // Captured-pointer prerequisite, not a native mouse-grab claim.
    poll(&mut a);
    assert!(a.address.is_some());
    assert!(
        a.address_error
            .as_deref()
            .unwrap()
            .contains("fixture failed refresh")
    );
    assert!(!a.input.captured);
    assert!(!a.input.forward);
    a.keyboard_input(Some(KeyCode::KeyW), false, false, None)
        .unwrap();
    assert_eq!(a.world.current, current);
    assert_eq!(a.themes[0].host, host);
    assert_eq!(a.themes[0].source, source);
    assert_eq!(
        (a.pose.position, a.pose.yaw, a.pose.pitch),
        (pose.position, pose.yaw, pose.pitch)
    );
    press(&mut a, KeyCode::Delete); // Actual recovery clears refusal, not a core setter.
    a.keyboard_input(None, true, false, Some("https://example.org/retry"))
        .unwrap();
    a.web = Some(web_native::Runtime::new(Vec::new(), wait_for_cancel)); // Inject offline transport only.
    press(&mut a, KeyCode::Enter);
    assert!(a.web.as_ref().unwrap().pending.is_some());
    press(&mut a, KeyCode::KeyR);
    poll(&mut a);
    assert!(a.web.is_none());
    assert!(a.address.is_none());
    assert_eq!(a.world.current, current);
    // Aim at the real bundled document, then use E, not World::act.
    let document = a.themes[0]
        .host
        .anchors
        .iter()
        .find(|x| x.role == "document")
        .unwrap();
    a.pose = Pose {
        position: [document.at[0], document.at[2] + 1.5],
        yaw: 0.0,
        pitch: -0.1,
    };
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    press(&mut a, KeyCode::KeyE);
    assert!(a.world.reading);
    press(&mut a, KeyCode::Escape);
    a.web = Some(web_native::Runtime::new(Vec::new(), fixture)); // Offline successful retry transport.
    submit(&mut a, "https://example.org/recovered");
    poll(&mut a);
    assert_eq!(a.world.current, "https://example.org/recovered");
    assert_eq!(a.themes[0].source, source);
    assert!(a.web.as_ref().unwrap().history.is_empty());
    println!(
        "ORDER submit bad -> W down/captured prerequisite -> async error -> capture/movement released before entry -> W up -> Delete/text/Enter retry -> R down/up cancel -> worker retirement -> aimed local E -> Escape -> L/text/Enter successful retry"
    );
}

#[test]
fn failure_after_focus_loss_stays_dismissed_through_focus_regain() {
    let mut a = local(fixture);
    submit(&mut a, "https://example.org/bad");
    a.focus_changed(false);
    poll(&mut a);
    assert!(a.address.is_none());
    assert!(a.address_error.is_none());
    assert!(a.web.is_none());
    a.keyboard_input(Some(KeyCode::KeyE), false, false, None)
        .unwrap();
    a.focus_changed(true);
    assert!(
        a.address.is_none(),
        "focus regain alone does not acquire entry"
    );
    press(&mut a, KeyCode::KeyL);
    a.keyboard_input(Some(KeyCode::KeyW), true, false, Some("w"))
        .unwrap();
    a.keyboard_input(Some(KeyCode::KeyW), false, false, None)
        .unwrap();
    assert_eq!(a.address.as_deref(), Some("w"));
    assert!(!a.input.forward);
    press(&mut a, KeyCode::Escape);
    assert!(a.address.is_none());
    println!(
        "ORDER submit -> focus lost -> async failure -> no entry/error -> E up -> focus regained -> no implicit entry -> explicit L -> literal w/up -> Escape"
    );
}
