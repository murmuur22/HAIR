#![cfg(feature = "workshop")]
use hair::workshop::{Command, DialogKind, Workshop};
fn frame(a: &mut Workshop, c: &egui::Context, size: [f32; 2], events: Vec<egui::Event>) {
    let mut o = c.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0], size[1]),
            )),
            events,
            ..Default::default()
        },
        |ui| a.ui(ui, Some(egui::TextureId::User(0))),
    );
    o.textures_delta.clear();
}
fn visible(a: &mut Workshop, c: &egui::Context, size: [f32; 2], id: &str) -> egui::Pos2 {
    for _ in 0..3 {
        frame(a, c, size, vec![]);
    }
    for _ in 0..30 {
        if let Some(r) = a.controls.get(id)
            && r.center().y > 40.0
            && r.center().y < size[1] - 120.0
        {
            return r.center();
        }
        let delta = if a.controls.get(id).is_some_and(|r| r.center().y < 40.0) {
            80.0
        } else {
            -80.0
        };
        frame(
            a,
            c,
            size,
            vec![
                egui::Event::PointerMoved(egui::pos2(200.0, 200.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    delta: egui::vec2(0.0, delta),
                    modifiers: Default::default(),
                },
            ],
        );
    }
    panic!("control not visible {id}: {:?}", a.controls.get(id));
}
fn click(a: &mut Workshop, c: &egui::Context, size: [f32; 2], id: &str) {
    let p = visible(a, c, size, id);
    for pressed in [true, false] {
        frame(
            a,
            c,
            size,
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
}
#[test]
fn material_only_start_clear_and_pointer_widgets_retain_at_normal_and_small_viewports() {
    for size in [[1200.0, 1800.0], [640.0, 480.0]] {
        let mut a = Workshop::new(vec![]);
        let c = egui::Context::default();
        click(&mut a, &c, size, "new-material");
        assert!(a.project.as_ref().unwrap().model.vertices.is_empty());
        assert!(a.dirty);
        for id in ["wall-red", "wall-tile", "floor-red", "floor-tile"] {
            click(
                &mut a,
                &c,
                size,
                if id.starts_with("wall") {
                    "wall"
                } else {
                    "floor"
                },
            );
            let p = visible(&mut a, &c, size, id);
            let r = a.controls[id];
            let pos = egui::pos2(r.left() + r.width() * 0.25, p.y);
            for pressed in [true, false] {
                frame(
                    &mut a,
                    &c,
                    size,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                );
            }
        }
        let s = a.project.as_ref().unwrap().surfaces();
        assert_eq!(s.len(), 2);
        assert!(
            s.iter()
                .all(|s| s.tint[0] < 0.99 && s.meters_per_tile > 1.1)
        );
        a.camera.free_fly = true;
        a.camera.capture();
        let pose = a.camera.pose;
        click(&mut a, &c, size, "wall");
        click(&mut a, &c, size, "wall-clear");
        assert!(!a.camera.captured);
        assert_eq!(a.camera.pose.position, pose.position);
        assert_eq!(a.project.as_ref().unwrap().surfaces(), s);
        let input =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/site-a/swatch.png");
        click(&mut a, &c, size, "wall-png");
        assert_eq!(a.begin_dialog(), Some(DialogKind::WallTexture));
        a.finish_dialog(Ok(Some(input)));
        assert!(a.error.is_none(), "{:?}", a.error);
        assert!(
            a.project
                .as_ref()
                .unwrap()
                .surfaces()
                .iter()
                .find(|s| s.surface == "wall")
                .unwrap()
                .file
                .is_some()
        );
        click(&mut a, &c, size, "wall");
        click(&mut a, &c, size, "wall-clear");
        assert_eq!(a.project.as_ref().unwrap().surfaces(), s);
        // A material panel cancels without losing edits or moving the camera.
        let pose = a.camera.pose;
        let revision = a.revision;
        a.request_dialog(DialogKind::WallTexture);
        assert_eq!(a.begin_dialog(), Some(DialogKind::WallTexture));
        a.finish_dialog(Ok(None));
        assert_eq!(a.revision, revision);
        assert_eq!(a.camera.pose.position, pose.position);
        assert!(!a.camera.captured);
        a.dispatch(Command::RequestQuit);
        assert!(a.confirmation_pending());
        a.dispatch(Command::Cancel);
        assert_eq!(a.project.as_ref().unwrap().surfaces(), s);
    }
}
