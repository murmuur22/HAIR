#![cfg(feature = "workshop")]
use hair::workshop::{Command, DialogKind, Workshop};
use std::{fs, path::Path};
fn frame(a: &mut Workshop, c: &egui::Context, events: Vec<egui::Event>) {
    let mut out = c.run_ui(
        egui::RawInput {
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
        frame(a, c, vec![]);
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
        );
    }
}
#[test]
fn native_texture_adapter_ui_retains_cancel_error_dirty_and_whole_pack_roundtrip() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let d = std::env::temp_dir().join(format!("hair-material-ui-{}", std::process::id()));
    fs::create_dir(&d).unwrap();
    let png = d.join("input.png");
    {
        let mut e = png::Encoder::new(fs::File::create(&png).unwrap(), 2, 2);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .unwrap()
            .write_image_data(&[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
            ])
            .unwrap();
    }
    let mut a = Workshop::new(vec![root.into()]);
    let c = egui::Context::default();
    a.input_path = root.join("assets/kenney/desk.glb").display().to_string();
    a.dispatch(Command::Import);
    click(&mut a, &c, "decoration");
    click(&mut a, &c, "import");
    a.begin_dialog();
    a.finish_dialog(Ok(Some(root.join("assets/kenney/desk.glb"))));
    a.dispatch(Command::ConfirmDiscard);
    click(&mut a, &c, "document");
    for (id, kind) in [
        ("wall-png", DialogKind::WallTexture),
        ("floor-png", DialogKind::FloorTexture),
    ] {
        click(
            &mut a,
            &c,
            if kind == DialogKind::WallTexture {
                "wall"
            } else {
                "floor"
            },
        );
        click(&mut a, &c, id);
        assert_eq!(a.begin_dialog(), Some(kind));
        a.finish_dialog(Ok(Some(png.clone())));
        assert!(a.error.is_none(), "{:?}", a.error);
    }
    assert_eq!(a.project.as_ref().unwrap().surfaces().len(), 2);
    let old = a.project.as_ref().unwrap().surfaces();
    let revision = a.revision;
    let dirty = a.dirty;
    for result in [
        Ok(None),
        Err("panel refused".into()),
        Ok(Some(d.join("absent.png"))),
    ] {
        click(&mut a, &c, "wall");
        click(&mut a, &c, "wall-png");
        assert_eq!(a.begin_dialog(), Some(DialogKind::WallTexture));
        a.finish_dialog(result);
        assert_eq!(a.project.as_ref().unwrap().surfaces(), old);
        assert_eq!((a.revision, a.dirty), (revision, dirty));
        a.dispatch(Command::AcknowledgeError);
    }
    for (id, kind, path) in [
        ("save-new", DialogKind::SaveNew, d.join("saved")),
        ("open", DialogKind::Open, d.join("saved/project.toml")),
        ("export", DialogKind::Export, d.join("export")),
    ] {
        click(&mut a, &c, id);
        assert_eq!(a.begin_dialog(), Some(kind));
        a.finish_dialog(Ok(Some(path)));
        assert!(a.error.is_none(), "{:?}", a.error);
    }
    let t = hair::theme::Theme::load(&d.join("export/pack.toml"), &mut Default::default()).unwrap();
    assert_eq!(t.loaded_roles.len(), 2);
    assert_eq!(t.textures.len(), 2);
    assert!(!a.dirty);
    fs::remove_dir_all(d).unwrap();
}
