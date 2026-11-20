#![cfg(feature = "workshop")]
use hair::workshop::{Command, DialogKind, InspectionCamera, Workshop};
fn frame(a: &mut Workshop, c: &egui::Context, events: Vec<egui::Event>, focused: bool) {
    let mut out = c.run_ui(
        egui::RawInput {
            focused,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 1800.0),
            )),
            events,
            ..Default::default()
        },
        |ui| a.ui(ui, Some(egui::TextureId::User(0))),
    );
    out.textures_delta.clear();
}
fn click(a: &mut Workshop, c: &egui::Context, id: &str) {
    for _ in 0..3 {
        frame(a, c, vec![], true);
    }
    let p = a.controls[id].center();
    for pressed in [true, false] {
        frame(
            a,
            c,
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
            true,
        );
    }
}
fn key(k: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: Some(k),
        pressed,
        repeat: false,
        modifiers: Default::default(),
    }
}
#[test]
fn actual_egui_capture_typing_focus_panel_and_escape_adapter() {
    let mut a = Workshop::new(vec![]);
    let c = egui::Context::default();
    click(&mut a, &c, "camera-mode");
    assert!(a.camera.free_fly);
    click(&mut a, &c, "preview");
    assert!(a.camera.captured);
    frame(
        &mut a,
        &c,
        vec![
            key(egui::Key::W, true),
            egui::Event::MouseMoved(egui::vec2(20.0, 0.0)),
        ],
        true,
    );
    assert!(a.camera.pose.position[1] < 3.5);
    assert!(a.camera.pose.yaw > 0.0);
    frame(&mut a, &c, vec![key(egui::Key::Escape, true)], true);
    assert!(!a.camera.captured);
    assert!(!a.should_close);
    assert!(!a.confirmation_pending());
    frame(&mut a, &c, vec![key(egui::Key::Escape, false)], true);
    click(&mut a, &c, "preview");
    frame(&mut a, &c, vec![key(egui::Key::W, true)], true);
    a.request_dialog(DialogKind::Import);
    assert!(!a.camera.captured);
    assert_eq!(a.begin_dialog(), Some(DialogKind::Import));
    a.finish_dialog(Ok(None));
    let old = a.camera.pose;
    frame(
        &mut a,
        &c,
        vec![egui::Event::MouseMoved(egui::vec2(100.0, 100.0))],
        true,
    );
    assert_eq!(old.position, a.camera.pose.position);
    assert_eq!(old.yaw, a.camera.pose.yaw);
    click(&mut a, &c, "preview");
    frame(&mut a, &c, vec![key(egui::Key::W, true)], false);
    assert!(!a.camera.captured);
    click(&mut a, &c, "advanced-paths");
    click(&mut a, &c, "import-path");
    assert!(
        c.text_edit_focused(),
        "field focus missing: {:?}",
        a.controls["import-path"]
    );
    let old = a.camera.pose;
    frame(
        &mut a,
        &c,
        vec![key(egui::Key::W, true), egui::Event::Text("wasdqe".into())],
        true,
    );
    assert!(a.input_path.contains("wasdqe"), "text={:?}", a.input_path);
    assert_eq!(old.position, a.camera.pose.position);
    frame(&mut a, &c, vec![key(egui::Key::Escape, true)], true);
    assert!(!a.should_close);
    a.dirty = true;
    a.dispatch(Command::RequestQuit);
    assert!(!a.camera.captured);
    assert!(a.confirmation_pending());
    frame(
        &mut a,
        &c,
        vec![
            key(egui::Key::D, true),
            egui::Event::MouseMoved(egui::vec2(10.0, 0.0)),
        ],
        true,
    );
    assert_eq!(old.position, a.camera.pose.position);
}

#[test]
fn pose_motion_is_normalized_bounded_and_vertical() {
    let mut a = InspectionCamera::default();
    a.free_fly = true;
    a.capture();
    a.key(egui::Key::W, true);
    a.step(10.0);
    assert!((a.pose.position[1] - 3.25).abs() < 0.0001);
    let mut b = InspectionCamera::default();
    b.free_fly = true;
    b.capture();
    for k in [egui::Key::W, egui::Key::D, egui::Key::E] {
        b.key(k, true);
    }
    b.step(0.1);
    let d = glam::Vec3::new(
        b.pose.position[0],
        b.height - 1.65,
        b.pose.position[1] - 3.5,
    );
    assert!((d.length() - 0.25).abs() < 0.0001);
    let old = b.pose;
    b.step(f32::NAN);
    assert_eq!(old.position, b.pose.position);
    b.look(20.0, -20.0);
    assert!(b.pose.yaw > 0.0);
    assert!(b.pose.pitch > -0.2);
    b.look(0.0, -1e8);
    assert!(b.pose.pitch <= 1.5);
}
#[test]
fn release_clears_held_and_requires_explicit_recapture() {
    let mut a = InspectionCamera::default();
    a.free_fly = true;
    a.capture();
    a.key(egui::Key::W, true);
    a.release();
    let old = a.pose;
    a.step(0.1);
    a.look(200.0, 200.0);
    assert_eq!(old.position, a.pose.position);
    assert_eq!(old.yaw, a.pose.yaw);
    a.capture();
    a.step(0.1);
    assert_eq!(old.position, a.pose.position);
    a.key(egui::Key::W, true);
    a.key(egui::Key::W, false);
    a.step(0.1);
    assert_eq!(old.position, a.pose.position);
    a.frame();
    assert!(!a.captured);
    assert_eq!(a.height, 1.65);
    assert_eq!(a.pose.position, [0.0, 3.5]);
}
