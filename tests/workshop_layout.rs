#![cfg(feature = "workshop")]
use hair::workshop::{Command, DialogKind, Workshop};
fn raw(size: [f32; 2]) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(size[0], size[1]),
        )),
        ..Default::default()
    }
}
fn frame(c: &egui::Context, a: &mut Workshop, size: [f32; 2], events: Vec<egui::Event>) -> String {
    let mut input = raw(size);
    input.events = events;
    let out = c.run_ui(input, |ui| a.ui(ui, None));
    fn text(s: &egui::epaint::Shape, out: &mut String) {
        match s {
            egui::epaint::Shape::Text(t) => {
                out.push_str(&t.galley.job.text);
                out.push('\n');
            }
            egui::epaint::Shape::Vec(v) => {
                for s in v {
                    text(s, out)
                }
            }
            _ => {}
        }
    }
    let mut t = String::new();
    for s in &out.shapes {
        text(&s.shape, &mut t);
    }
    out.drop_without_applying_deltas();
    t
}
fn click(c: &egui::Context, a: &mut Workshop, size: [f32; 2], id: &str) {
    for _ in 0..3 {
        frame(c, a, size, vec![]);
    }
    for _ in 0..80 {
        let r = *a.controls.get(id).unwrap_or_else(|| panic!("missing {id}"));
        if a.controls["authoring-viewport"].contains(r.center()) {
            break;
        }
        let delta = if r.center().y < a.controls["authoring-viewport"].top() {
            100.0
        } else {
            -100.0
        };
        frame(
            c,
            a,
            size,
            vec![
                egui::Event::PointerMoved(egui::pos2(100.0, 200.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    delta: egui::vec2(0.0, delta),
                    modifiers: Default::default(),
                },
            ],
        );
        for _ in 0..3 {
            frame(c, a, size, vec![]);
        }
    }
    let p = a.controls[id].center();
    assert!(
        a.controls["authoring-viewport"].contains(p),
        "unreachable {id}: {p:?}"
    );
    for pressed in [true, false] {
        frame(
            c,
            a,
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
    frame(c, a, size, vec![]);
}
#[test]
fn unified_categories_select_only_contextual_editor_without_dirtying() {
    for size in [[1200.0, 900.0], [640.0, 480.0], [320.0, 480.0]] {
        let c = egui::Context::default();
        let mut a = Workshop::new(vec![]);
        a.dispatch(Command::NewMaterial);
        a.dirty = false;
        for (id, title, import, kind) in [
            ("wall", "Wall", "wall-png", DialogKind::WallTexture),
            ("floor", "Floor", "floor-png", DialogKind::FloorTexture),
            ("door-trim", "Door trim", "import", DialogKind::Import),
            ("decoration", "Decoration", "import", DialogKind::Import),
            ("document", "Document object", "import", DialogKind::Import),
        ] {
            click(&c, &mut a, size, id);
            let text = frame(&c, &mut a, size, vec![]);
            assert!(
                text.contains(&format!("Editing: {title}")),
                "size={size:?} id={id} controls={:?}\n{text}",
                a.controls
            );
            assert!(!a.dirty, "selection must be session-only");
            assert_eq!(a.controls.contains_key("wall-png"), id == "wall");
            assert_eq!(a.controls.contains_key("floor-png"), id == "floor");
            assert_eq!(
                a.controls.contains_key("import"),
                !matches!(id, "wall" | "floor")
            );
            click(&c, &mut a, size, import);
            assert_eq!(a.begin_dialog(), Some(kind));
            let state = (
                a.revision,
                a.dirty,
                a.project.as_ref().unwrap().edit(),
                a.project.as_ref().unwrap().surfaces(),
            );
            a.finish_dialog(Ok(None));
            assert_eq!(
                (
                    a.revision,
                    a.dirty,
                    a.project.as_ref().unwrap().edit(),
                    a.project.as_ref().unwrap().surfaces()
                ),
                state
            );
        }
    }
}
#[test]
fn keyboard_categories_activate_and_paint_selected_highlight() {
    let c = egui::Context::default();
    let mut a = Workshop::new(vec![]);
    a.dispatch(Command::NewMaterial);
    a.dirty = false;
    let size = [640.0, 480.0];
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..100 {
        for pressed in [true, false] {
            frame(
                &c,
                &mut a,
                size,
                vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            );
        }
        if let Some(response) = c.memory(|m| m.focused()).and_then(|id| c.read_response(id)) {
            for (id, title) in [
                ("document", "Document object"),
                ("decoration", "Decoration"),
                ("door-trim", "Door trim"),
                ("wall", "Wall"),
                ("floor", "Floor"),
            ] {
                if response.rect == a.controls[id] {
                    for pressed in [true, false] {
                        frame(
                            &c,
                            &mut a,
                            size,
                            vec![egui::Event::Key {
                                key: egui::Key::Enter,
                                physical_key: None,
                                pressed,
                                repeat: false,
                                modifiers: Default::default(),
                            }],
                        );
                    }
                    assert!(frame(&c, &mut a, size, vec![]).contains(&format!("Editing: {title}")));
                    seen.insert(id);
                    let mut fill = egui::Color32::TRANSPARENT;
                    let out = c.run_ui(raw(size), |ui| {
                        fill = ui.visuals().selection.bg_fill;
                        a.ui(ui, None)
                    });
                    let rect = a.controls[id];
                    fn painted(s: &egui::epaint::Shape, r: egui::Rect, f: egui::Color32) -> bool {
                        match s {
                            egui::epaint::Shape::Rect(s) => {
                                s.rect.contains(r.center()) && s.fill == f
                            }
                            egui::epaint::Shape::Vec(v) => v.iter().any(|s| painted(s, r, f)),
                            _ => false,
                        }
                    }
                    assert!(
                        out.shapes.iter().any(|s| painted(&s.shape, rect, fill)),
                        "selected category must be visibly painted {id}"
                    );
                    out.drop_without_applying_deltas();
                }
            }
        }
        if seen.len() == 5 {
            break;
        }
    }
    assert_eq!(seen.len(), 5, "all selectors must be keyboard reachable");
    assert!(!a.dirty);
}
#[test]
fn every_category_input_panel_focus_and_label_bounds_matrix() {
    for size in [[1200.0, 900.0], [640.0, 480.0], [320.0, 480.0]] {
        let c = egui::Context::default();
        let mut a = Workshop::new(vec![]);
        a.dispatch(Command::NewMaterial);
        for (category, import, kind) in [
            ("document", "import", DialogKind::Import),
            ("decoration", "import", DialogKind::Import),
            ("door-trim", "import", DialogKind::Import),
            ("wall", "wall-png", DialogKind::WallTexture),
            ("floor", "floor-png", DialogKind::FloorTexture),
        ] {
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
            let pose = (a.camera.pose.position, a.camera.pose.yaw, a.camera.height);
            click(&c, &mut a, size, category);
            assert!(!a.camera.captured);
            let ids: &[&str] = if category == "wall" {
                &[
                    "wall-red",
                    "wall-green",
                    "wall-blue",
                    "wall-tile",
                    "wall-clear",
                ]
            } else if category == "floor" {
                &[
                    "floor-red",
                    "floor-green",
                    "floor-blue",
                    "floor-tile",
                    "floor-clear",
                ]
            } else {
                &[
                    "model-scale",
                    "model-yaw",
                    "model-pivot-x",
                    "model-pivot-y",
                    "model-pivot-z",
                    "fit",
                    "reset",
                ]
            };
            for id in ids {
                // Real scrolling/pointer delivery, including disabled Door controls.
                click(&c, &mut a, size, id);
                let r = a.controls[id];
                let clip = a.controls["authoring-viewport"];
                assert!(
                    r.left() >= clip.left() && r.right() <= clip.right() + 1.0,
                    "horizontal clipping {size:?} {id} {r:?} {clip:?}"
                );
                assert_eq!(
                    (a.camera.pose.position, a.camera.pose.yaw, a.camera.height),
                    pose
                );
            }
            for result in [Ok(None), Err("declined panel".into())] {
                click(&c, &mut a, size, import);
                assert_eq!(a.begin_dialog(), Some(kind));
                let state = (
                    a.revision,
                    a.dirty,
                    a.project.as_ref().unwrap().edit(),
                    a.project.as_ref().unwrap().surfaces(),
                );
                a.finish_dialog(result);
                assert_eq!(
                    (
                        a.revision,
                        a.dirty,
                        a.project.as_ref().unwrap().edit(),
                        a.project.as_ref().unwrap().surfaces()
                    ),
                    state
                );
                a.dispatch(Command::AcknowledgeError);
            }
            click(&c, &mut a, size, "advanced-paths");
            click(&c, &mut a, size, "import-path");
            assert!(c.text_edit_focused());
            let text = frame(&c, &mut a, size, vec![egui::Event::Text("wasdqe".into())]);
            let _ = text;
            assert!(a.input_path.contains("wasdqe"));
            assert_eq!(
                (a.camera.pose.position, a.camera.pose.yaw, a.camera.height),
                pose
            );
            a.camera.capture();
            a.camera.step(0.1);
            assert_eq!(
                (a.camera.pose.position, a.camera.pose.yaw, a.camera.height),
                pose,
                "held movement must be cleared"
            );
            a.camera.release();
            click(&c, &mut a, size, "advanced-paths");
        }
    }
}
#[test]
fn contextual_typed_png_import_and_oversize_refusal_keep_last_good() {
    let c = egui::Context::default();
    let mut a = Workshop::new(vec![]);
    click(&c, &mut a, [1200.0, 900.0], "wall");
    a.input_path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/site-a/swatch.png").into();
    a.dispatch(Command::Import);
    assert!(a.error.is_none(), "PNG typed/drop route: {:?}", a.error);
    let s = a.project.as_ref().unwrap().surfaces();
    assert_eq!(s.len(), 1);
    click(&c, &mut a, [1200.0, 900.0], "door-trim");
    a.input_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/door-trim-starter/model.glb"
    )
    .into();
    a.dispatch(Command::Import);
    a.dispatch(Command::ConfirmDiscard);
    let old = (
        a.revision,
        a.dirty,
        a.project.as_ref().unwrap().edit(),
        a.project.as_ref().unwrap().surfaces(),
    );
    let d = std::env::temp_dir().join(format!("hair-layout-oversize-{}", std::process::id()));
    std::fs::create_dir(&d).unwrap();
    std::fs::write(d.join("large.glb"), vec![0; 8_000_001]).unwrap();
    click(&c, &mut a, [1200.0, 900.0], "import");
    a.begin_dialog();
    a.finish_dialog(Ok(Some(d.join("large.glb"))));
    let text = frame(&c, &mut a, [1200.0, 900.0], vec![]);
    assert!(
        !text.contains("Door trim must keep"),
        "size refusal must not be called aperture rejection"
    );
    assert!(a.error.is_some());
    assert_eq!(
        (
            a.revision,
            a.dirty,
            a.project.as_ref().unwrap().edit(),
            a.project.as_ref().unwrap().surfaces()
        ),
        old
    );
    assert_eq!(a.project.as_ref().unwrap().surfaces(), s);
    std::fs::remove_dir_all(d).unwrap();
}
#[test]
fn blank_category_import_cancel_never_starts_dirty_project() {
    for (category, import, kind) in [
        ("wall", "wall-png", DialogKind::WallTexture),
        ("floor", "floor-png", DialogKind::FloorTexture),
        ("decoration", "import", DialogKind::Import),
        ("door-trim", "import", DialogKind::Import),
    ] {
        let c = egui::Context::default();
        let mut a = Workshop::new(vec![]);
        click(&c, &mut a, [1200.0, 900.0], category);
        click(&c, &mut a, [1200.0, 900.0], import);
        assert_eq!(a.begin_dialog(), Some(kind));
        a.finish_dialog(Ok(None));
        assert!(
            a.project.is_none() && !a.dirty && a.revision == 0,
            "cancelled {category} must retain blank start"
        );
    }
}
#[test]
fn rejected_door_import_shows_recovery_not_coordinates() {
    let c = egui::Context::default();
    let mut a = Workshop::new(vec![]);
    a.dispatch(Command::NewMaterial);
    click(&c, &mut a, [1200.0, 900.0], "door-trim");
    a.input_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/door-trim-starter/model.glb"
    )
    .into();
    a.dispatch(Command::Import);
    a.dispatch(Command::ConfirmDiscard);
    let state = (a.revision, a.dirty, a.project.as_ref().unwrap().edit());
    click(&c, &mut a, [1200.0, 900.0], "import");
    a.begin_dialog();
    a.finish_dialog(Ok(Some(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/kenney/desk.glb"),
    )));
    let text = frame(&c, &mut a, [1200.0, 900.0], vec![]);
    assert!(text.contains("Door trim must keep the doorway clear. Use the supplied frame template; your last working model is unchanged."),"{text}");
    assert!(
        !text.contains("X -0.5"),
        "technical details must start collapsed"
    );
    assert_eq!(
        (a.revision, a.dirty, a.project.as_ref().unwrap().edit()),
        state
    );
}
#[test]
fn door_trim_generic_transform_clicks_are_inert() {
    let c = egui::Context::default();
    let mut a = Workshop::new(vec![]);
    a.input_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/door-trim-starter/model.glb"
    )
    .into();
    a.dispatch(Command::NewMaterial);
    a.dispatch(Command::Door);
    a.dispatch(Command::Import);
    a.dispatch(Command::ConfirmDiscard);
    assert!(a.error.is_none(), "{:?}", a.error);
    click(&c, &mut a, [1200.0, 900.0], "door-trim");
    let old = (a.revision, a.dirty, a.project.as_ref().unwrap().edit());
    for id in [
        "fit",
        "reset",
        "rotate-step",
        "model-scale",
        "model-yaw",
        "model-pivot-x",
        "model-pivot-y",
        "model-pivot-z",
    ] {
        click(&c, &mut a, [1200.0, 900.0], id);
        assert_eq!(
            (a.revision, a.dirty, a.project.as_ref().unwrap().edit()),
            old
        );
        assert!(a.error.is_none(), "{:?}", a.error);
    }
}
