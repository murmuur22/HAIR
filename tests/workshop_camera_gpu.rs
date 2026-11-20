#![cfg(feature = "workshop")]
use hair::{
    render, resources,
    workshop::{DialogKind, Workshop, WorkshopGpu},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
const SIZE: [u32; 2] = [1200, 1800];
struct Harness {
    gpu: WorkshopGpu,
    ctx: egui::Context,
    app: Workshop,
    target: wgpu::Texture,
    time: f64,
}
impl Harness {
    fn frame(&mut self, events: Vec<egui::Event>, focused: bool) {
        self.time += 1.0 / 30.0;
        let raw = egui::RawInput {
            time: Some(self.time),
            focused,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(SIZE[0] as f32, SIZE[1] as f32),
            )),
            events,
            ..Default::default()
        };
        self.gpu
            .frame(
                &self.ctx,
                &mut self.app,
                raw,
                &self.target.create_view(&Default::default()),
                SIZE,
            )
            .unwrap();
    }
    fn click(&mut self, id: &str) {
        for _ in 0..3 {
            self.frame(vec![], true);
        }
        let pos = self.app.controls[id].center();
        for pressed in [true, false] {
            self.frame(
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                ],
                true,
            );
        }
    }
    fn browse(&mut self, id: &str, k: DialogKind, p: &Path) {
        self.click(id);
        assert!(!self.app.camera.captured);
        assert_eq!(self.app.begin_dialog(), Some(k));
        self.app.finish_dialog(Ok(Some(p.into())));
        assert!(self.app.error.is_none(), "{:?}", self.app.error);
    }
    fn pixels(&self) -> Vec<u8> {
        self.gpu.read_pixels(&self.target, SIZE).unwrap()
    }
    fn scene(&self, p: &[u8]) -> Vec<u8> {
        let r = self.app.controls["preview"];
        let mut out = vec![];
        for y in r.min.y.ceil() as usize..r.max.y.floor() as usize {
            for x in r.min.x.ceil() as usize..r.max.x.floor() as usize {
                let i = (y * SIZE[0] as usize + x) * 4;
                out.extend_from_slice(&p[i..i + 4]);
            }
        }
        out
    }
}
fn key(key: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers: Default::default(),
    }
}
fn files(p: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = vec![];
    for e in fs::read_dir(p).unwrap() {
        let e = e.unwrap();
        if e.path().is_file() {
            out.push((PathBuf::from(e.file_name()), fs::read(e.path()).unwrap()));
        }
    }
    out.sort();
    out
}
#[test]
#[ignore = "finite real Metal and synthetic executable egui input, not OS/human"]
fn camera_changes_shared_multi_role_pixels_not_project_or_exports() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = PathBuf::from(std::env::var_os("HAIR_CAMERA_GPU_DIR").expect("exclusive output"));
    fs::create_dir(&base).unwrap();
    let source = root.join("milestone-workshop/package/starter/model.glb");
    let deco = root.join("assets/kenney/chairDesk.glb");
    let originals = (fs::read(&source).unwrap(), fs::read(&deco).unwrap());
    pollster::block_on(async {
        let instance = render::instance();
        let adapter = render::adapter(&instance, None).await.unwrap();
        println!("ADAPTER={:?}", adapter.get_info());
        let gpu = WorkshopGpu::new(&adapter, wgpu::TextureFormat::Rgba8Unorm, SIZE)
            .await
            .unwrap();
        let target = gpu.target(SIZE);
        let mut h = Harness {
            gpu,
            target,
            ctx: Default::default(),
            app: Workshop::new(vec![root.into()]),
            time: 0.0,
        };
        h.browse("import", DialogKind::Import, &source);
        h.click("decoration");
        h.browse("import", DialogKind::Import, &deco);
        h.click("confirm-discard");
        h.click("fit");
        assert_eq!(h.app.preview.as_ref().unwrap().loaded_roles.len(), 2);
        h.browse("save-new", DialogKind::SaveNew, &base.join("saved-before"));
        h.browse("export", DialogKind::Export, &base.join("pack-before"));
        let edit = h.app.project.as_ref().unwrap().edit();
        let revision = h.app.revision;
        let vertices =
            bytemuck::cast_slice::<_, u8>(&h.app.preview.as_ref().unwrap().vertices).to_vec();
        h.click("camera-mode");
        h.frame(vec![], true);
        let before = h.pixels();
        let before_scene = h.scene(&before);
        h.click("preview");
        assert!(h.app.camera.captured);
        h.frame(
            vec![
                key(egui::Key::W, true),
                key(egui::Key::D, true),
                key(egui::Key::E, true),
                egui::Event::MouseMoved(egui::vec2(80.0, -20.0)),
            ],
            true,
        );
        for _ in 0..7 {
            h.frame(vec![], true);
        }
        h.frame(
            vec![
                key(egui::Key::W, false),
                key(egui::Key::D, false),
                key(egui::Key::E, false),
            ],
            true,
        );
        assert!(h.app.camera.height > 1.65);
        let moved = h.app.camera.pose;
        h.frame(vec![], true);
        assert_eq!(moved.position, h.app.camera.pose.position);
        let after = h.pixels();
        let after_scene = h.scene(&after);
        let changed = before_scene
            .as_chunks::<4>()
            .0
            .iter()
            .zip(after_scene.as_chunks::<4>().0.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed > 100, "scene camera difference {changed}");
        println!("CAMERA_SCENE_CHANGED_PIXELS={changed}");
        assert_eq!(h.app.project.as_ref().unwrap().edit(), edit);
        assert!(!h.app.dirty);
        assert_eq!(h.app.revision, revision);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&h.app.preview.as_ref().unwrap().vertices),
            vertices
        );
        h.frame(vec![key(egui::Key::Escape, true)], true);
        assert!(!h.app.camera.captured);
        assert!(!h.app.should_close);
        assert!(!h.app.confirmation_pending());
        h.frame(vec![key(egui::Key::Escape, false)], true);
        h.click("frame-camera");
        h.frame(vec![], true);
        let reset = h.pixels();
        assert_eq!(before_scene, h.scene(&reset));
        h.click("preview");
        h.frame(vec![key(egui::Key::W, true)], true);
        h.click("import");
        assert_eq!(h.app.begin_dialog(), Some(DialogKind::Import));
        h.app.finish_dialog(Ok(None));
        let pose = h.app.camera.pose;
        h.frame(
            vec![egui::Event::MouseMoved(egui::vec2(500.0, 500.0))],
            true,
        );
        assert!(!h.app.camera.captured);
        assert_eq!(pose.yaw, h.app.camera.pose.yaw);
        assert_eq!(pose.position, h.app.camera.pose.position);
        h.click("preview");
        h.frame(vec![key(egui::Key::W, true)], false);
        assert!(!h.app.camera.captured);
        h.frame(vec![], true);
        assert!(!h.app.camera.captured);
        h.click("advanced-paths");
        h.click("import-path");
        assert!(h.ctx.text_edit_focused());
        let pose = h.app.camera.pose;
        h.frame(
            vec![key(egui::Key::W, true), egui::Event::Text("wasdqe".into())],
            true,
        );
        assert!(h.app.input_path.contains("wasdqe"));
        assert_eq!(pose.position, h.app.camera.pose.position);
        h.frame(vec![key(egui::Key::Escape, true)], true);
        assert!(!h.app.should_close);
        h.frame(vec![key(egui::Key::Escape, false)], true);
        h.browse("save-new", DialogKind::SaveNew, &base.join("saved-after"));
        h.browse("export", DialogKind::Export, &base.join("pack-after"));
        assert_eq!(
            files(&base.join("saved-before")),
            files(&base.join("saved-after"))
        );
        assert_eq!(
            files(&base.join("pack-before")),
            files(&base.join("pack-after"))
        );
        h.browse(
            "open",
            DialogKind::Open,
            &base.join("saved-after/project.toml"),
        );
        assert_eq!(h.app.camera.pose.position, [0.0, 3.5]);
        assert!(!h.app.camera.captured);
        h.click("preview");
        h.app.dirty = true;
        h.click("quit");
        assert!(h.app.confirmation_pending());
        assert!(!h.app.camera.captured);
        let pose = h.app.camera.pose;
        h.frame(
            vec![
                key(egui::Key::W, true),
                egui::Event::MouseMoved(egui::vec2(50.0, 0.0)),
            ],
            true,
        );
        assert_eq!(pose.position, h.app.camera.pose.position);
        assert_eq!(pose.yaw, h.app.camera.pose.yaw);
        h.click("cancel-discard");
        let outdir = base.join("captures");
        fs::create_dir(&outdir).unwrap();
        let out = resources::smoke_output(root, &outdir).unwrap();
        for (name, pixels) in [
            ("office.png", before),
            ("reader-welcome.png", after),
            ("reader-workshop.png", reset),
        ] {
            let colors = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| [p[0], p[1], p[2]])
                .collect::<std::collections::HashSet<_>>()
                .len();
            assert!(colors >= 20);
            assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
            let mut f = out.create_capture(name).unwrap();
            let mut e = png::Encoder::new(&mut f, SIZE[0], SIZE[1]);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            let mut w = e.write_header().unwrap();
            w.write_image_data(&pixels).unwrap();
            w.finish().unwrap();
            std::io::Write::flush(&mut f).unwrap();
            println!("CAPTURE {name} colors={colors}");
        }
        println!(
            "CAMERA_GPU_PASS native=false human=false project_vertices_revision_dirty_export_bytes_unchanged=true frame_exact_scene_restore=true"
        );
    });
    assert_eq!(
        originals,
        (fs::read(source).unwrap(), fs::read(deco).unwrap())
    );
}
