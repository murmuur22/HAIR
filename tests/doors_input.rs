#![cfg(feature = "workshop")]
use hair::workshop::{Command, DialogKind, Workshop};
use std::path::Path;
fn frame(a: &mut Workshop, c: &egui::Context, events: Vec<egui::Event>, focused: bool) {
    c.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 1800.0),
            )),
            events,
            focused,
            ..Default::default()
        },
        |u| a.ui(u, Some(egui::TextureId::User(0))),
    )
    .drop_without_applying_deltas();
}
fn pointer(p: egui::Pos2, b: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(p),
        egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: b,
            modifiers: Default::default(),
        },
    ]
}
fn click(a: &mut Workshop, c: &egui::Context, p: egui::Pos2) {
    for b in [true, false] {
        frame(a, c, pointer(p, b), true);
    }
}
fn control(a: &mut Workshop, c: &egui::Context, id: &str) -> egui::Rect {
    for _ in 0..3 {
        frame(a, c, vec![], true);
    }
    a.controls[id]
}
fn key(k: egui::Key, b: bool) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: b,
        repeat: false,
        modifiers: Default::default(),
    }
}
#[test]
fn supported_models_numeric_slider_typing_held_focus_panel_and_dirty_matrix() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut a = Workshop::new(vec![root.into()]);
    let c = egui::Context::default();
    a.dispatch(Command::NewMaterial);
    a.input_path = root.join("assets/kenney/desk.glb").display().to_string();
    a.dispatch(Command::Import);
    a.dispatch(Command::ConfirmDiscard);
    assert!(a.error.is_none(), "{:?}", a.error);
    for role in ["document", "decoration"] {
        let p = control(&mut a, &c, role).center();
        click(&mut a, &c, p);
        a.dispatch(Command::Import);
        a.dispatch(Command::ConfirmDiscard);
        for (id, value) in [
            ("model-yaw", 0.1),
            ("model-scale", 1.0),
            ("model-pivot-x", 0.001),
        ] {
            let r = control(&mut a, &c, id);
            a.camera.free_fly = true;
            a.camera.capture();
            for k in [
                egui::Key::W,
                egui::Key::A,
                egui::Key::S,
                egui::Key::D,
                egui::Key::Q,
                egui::Key::E,
            ] {
                a.camera.key(k, true);
            }
            let old = (a.camera.pose.position, a.camera.pose.yaw, a.camera.height);
            let p = egui::pos2(r.left() + 120.0, r.center().y);
            click(&mut a, &c, p);
            click(&mut a, &c, p);
            assert!(c.text_edit_focused(), "numeric focus {id}");
            let select = egui::Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    command: true,
                    mac_cmd: true,
                    ..Default::default()
                },
            };
            frame(
                &mut a,
                &c,
                vec![
                    select,
                    egui::Event::Text(value.to_string()),
                    key(egui::Key::W, true),
                ],
                true,
            );
            frame(&mut a, &c, vec![key(egui::Key::Enter, true)], true);
            assert_eq!(
                old,
                (a.camera.pose.position, a.camera.pose.yaw, a.camera.height)
            );
            assert!(!a.camera.captured);
            let edit = a.project.as_ref().unwrap().edit();
            let actual = if id == "model-yaw" {
                edit.yaw_degrees
            } else if id == "model-scale" {
                edit.scale
            } else {
                edit.at[0]
            };
            assert!((actual - value).abs() < 0.0001, "{id} actual {actual}");
            let p = egui::pos2(r.left() + 20.0, r.center().y);
            for events in [
                pointer(p, true),
                vec![
                    egui::Event::PointerMoved(p + egui::vec2(30.0, 0.0)),
                    key(egui::Key::W, true),
                ],
                pointer(p, false),
            ] {
                a.dispatch(Command::AcknowledgeError);
                let before = (a.revision, a.dirty, a.project.as_ref().unwrap().edit());
                frame(&mut a, &c, events, true);
                if a.error.is_some() {
                    assert_eq!(
                        before,
                        (a.revision, a.dirty, a.project.as_ref().unwrap().edit())
                    );
                } else {
                    assert!(a.project.as_ref().unwrap().preview().is_ok());
                }
                assert_eq!(
                    old,
                    (a.camera.pose.position, a.camera.pose.yaw, a.camera.height)
                );
            }
            a.dispatch(Command::AcknowledgeError);
            frame(&mut a, &c, vec![key(egui::Key::W, true)], false);
            assert!(!a.camera.captured);
        }
    }
    for result in [Ok(None), Err("injected panel error".into())] {
        let before = (a.revision, a.dirty, a.project.as_ref().unwrap().edit());
        a.camera.capture();
        a.request_dialog(DialogKind::Import);
        assert_eq!(a.begin_dialog(), Some(DialogKind::Import));
        a.finish_dialog(result);
        assert_eq!(
            before,
            (a.revision, a.dirty, a.project.as_ref().unwrap().edit())
        );
        assert!(!a.camera.captured);
        a.dispatch(Command::AcknowledgeError);
    }
    let edit = a.project.as_ref().unwrap().edit();
    a.dispatch(Command::RequestQuit);
    assert!(a.confirmation_pending());
    frame(&mut a, &c, vec![key(egui::Key::W, true)], false);
    a.dispatch(Command::Cancel);
    assert_eq!(edit, a.project.as_ref().unwrap().edit());
    assert!(!a.should_close);
}
