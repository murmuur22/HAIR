#![cfg(feature = "workshop")]
use hair::{
    Pose, World, render, resources,
    theme::Theme,
    workshop::{DialogKind, Workshop, WorkshopGpu},
};
use std::{fs, path::Path};
#[test]
#[ignore = "actual finite Metal/synthetic egui; not native input"]
fn two_models_ui_roundtrip_and_greenleaf_metal() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::var_os("HAIR_WHOLE_GPU_DIR")
        .map(std::path::PathBuf::from)
        .expect("exclusive evidence directory");
    fs::create_dir(&base).unwrap();
    let input = base.join("input");
    fs::create_dir(&input).unwrap();
    // Original authored synthetic textured triangle, retained public starter (not private planet).
    let source = root.join("milestone-workshop/package/starter/model.glb");
    let original = fs::read(&source).unwrap();
    fs::write(input.join("document.glb"), &original).unwrap();
    let second = fs::read(root.join("assets/kenney/chairDesk.glb")).unwrap();
    fs::write(input.join("decoration.glb"), &second).unwrap();
    fn raw() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            ..Default::default()
        }
    }
    fn click(
        gpu: &mut WorkshopGpu,
        ctx: &egui::Context,
        a: &mut Workshop,
        v: &wgpu::TextureView,
        id: &str,
    ) {
        for _ in 0..3 {
            gpu.frame(ctx, a, raw(), v, [1200, 900]).unwrap();
        }
        let pos = a.controls[id].center();
        for pressed in [true, false] {
            let mut r = raw();
            r.events = vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ];
            gpu.frame(ctx, a, r, v, [1200, 900]).unwrap();
        }
    }
    fn browse(
        g: &mut WorkshopGpu,
        c: &egui::Context,
        a: &mut Workshop,
        v: &wgpu::TextureView,
        id: &str,
        k: DialogKind,
        p: &Path,
    ) {
        click(g, c, a, v, id);
        assert_eq!(a.begin_dialog(), Some(k));
        a.finish_dialog(Ok(Some(p.into())));
        assert!(a.error.is_none(), "{:?}", a.error);
    }
    pollster::block_on(async {
        let instance = render::instance();
        let adapter = render::adapter(&instance, None).await.unwrap();
        let mut gpu = WorkshopGpu::new(&adapter, wgpu::TextureFormat::Rgba8Unorm, [1200, 900])
            .await
            .unwrap();
        let target = gpu.target([1200, 900]);
        let view = target.create_view(&Default::default());
        let ctx = egui::Context::default();
        let mut app = Workshop::new(vec![root.into(), input.clone()]);
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "import",
            DialogKind::Import,
            &input.join("document.glb"),
        );
        click(&mut gpu, &ctx, &mut app, &view, "rotate-step");
        let doc = app.project.as_ref().unwrap().edit();
        click(&mut gpu, &ctx, &mut app, &view, "decoration");
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "import",
            DialogKind::Import,
            &input.join("decoration.glb"),
        );
        assert!(app.confirmation_pending());
        click(&mut gpu, &ctx, &mut app, &view, "confirm-discard");
        click(&mut gpu, &ctx, &mut app, &view, "fit");
        click(&mut gpu, &ctx, &mut app, &view, "rotate-step");
        let deco = app.project.as_ref().unwrap().edit();
        click(&mut gpu, &ctx, &mut app, &view, "document");
        assert_eq!(app.project.as_ref().unwrap().edit(), doc);
        click(&mut gpu, &ctx, &mut app, &view, "decoration");
        assert_eq!(app.project.as_ref().unwrap().edit(), deco);
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "save-new",
            DialogKind::SaveNew,
            &base.join("project"),
        );
        assert!(!app.dirty);
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "open",
            DialogKind::Open,
            &base.join("project/project.toml"),
        );
        assert_eq!(app.project.as_ref().unwrap().edit(), deco);
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "export",
            DialogKind::Export,
            &base.join("pack"),
        );
        for _ in 0..3 {
            gpu.frame(&ctx, &mut app, raw(), &view, [1200, 900])
                .unwrap();
        }
        let captures = base.join("workshop");
        fs::create_dir(&captures).unwrap();
        let out = resources::smoke_output(root, &captures).unwrap();
        let pixels = gpu.read_pixels(&target, [1200, 900]).unwrap();
        let mut file = out.create_capture("office.png").unwrap();
        let mut enc = png::Encoder::new(&mut file, 1200, 900);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
        writer.finish().unwrap();
        std::io::Write::flush(&mut file).unwrap();
        let mut world =
            World::load_site(&root.join("examples/GreenleafReadingRoom-appearance")).unwrap();
        let spaces = world.spaces.as_ref().unwrap().spaces.clone();
        for (i, s) in spaces.iter().enumerate() {
            world.current_space = s.id.clone();
            world.current = s.default_page.clone();
            let theme = Theme::load_for_scene(
                &base.join("pack/pack.toml"),
                &mut Default::default(),
                &s.scene,
            )
            .unwrap();
            assert_eq!(theme.loaded_roles.len(), 2);
            assert!(
                !theme
                    .diagnostics
                    .iter()
                    .any(|d| d.starts_with("ROLE_FALLBACK document")
                        || d.starts_with("ROLE_FALLBACK decoration"))
            );
            let cap = base.join(format!("visitor-{i}"));
            fs::create_dir(&cap).unwrap();
            let out = resources::smoke_output(root, &cap).unwrap();
            let mut r = render::Renderer::new(
                &adapter,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                [960, 640],
                &theme.vertices_for(&world),
            )
            .await
            .unwrap();
            r.set_materials(&theme.textures).unwrap();
            r.pack_feedback = theme.feedback(&world);
            let t = r.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("whole pack proof"),
                size: wgpu::Extent3d {
                    width: 960,
                    height: 640,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let pose = Pose {
                position: [
                    0.0,
                    if i == 0 {
                        2.8
                    } else if i == 1 {
                        1.7
                    } else {
                        2.2
                    },
                ],
                yaw: if i == 1 { -0.35 } else { 0.45 },
                pitch: -0.12,
            };
            r.draw(&t.create_view(&Default::default()), &pose, &world, 0)
                .unwrap();
            r.capture(&out, "office.png", &pose, &world).unwrap();
            // Actual scene-only negatives: remove just one fitted role's triangles.
            let doc_count = hair::assets::decode(&original, 10.0)
                .unwrap()
                .vertices
                .len();
            let deco_count = hair::assets::decode(&second, 10.0).unwrap().vertices.len();
            let mut no_doc = theme.vertices_for(&world);
            let start = theme.host.vertices().len();
            no_doc.drain(start..start + doc_count);
            r.set_scene(&no_doc);
            r.capture(&out, "reader-welcome.png", &pose, &world)
                .unwrap();
            let mut no_deco = theme.vertices_for(&world);
            let end = theme.vertices.len();
            no_deco.drain(end - deco_count..end);
            r.set_scene(&no_deco);
            r.capture(&out, "reader-workshop.png", &pose, &world)
                .unwrap();
            fn png_pixels(path: &Path) -> Vec<u8> {
                let mut dec =
                    png::Decoder::new(std::io::BufReader::new(fs::File::open(path).unwrap()))
                        .read_info()
                        .unwrap();
                let mut data = vec![0; dec.output_buffer_size().unwrap()];
                dec.next_frame(&mut data).unwrap();
                data
            }
            let full = png_pixels(&cap.join("office.png"));
            for name in ["reader-welcome.png", "reader-workshop.png"] {
                let counter = png_pixels(&cap.join(name));
                let changed = full
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(counter.as_chunks::<4>().0.iter())
                    .enumerate()
                    .filter(|(n, (a, b))| *n / 960 > 160 && *n / 960 < 550 && a != b)
                    .count();
                assert!(
                    changed > 20,
                    "role invisible in {}: {name} changed={changed}",
                    s.id
                );
                println!("ROLE_PIXEL_NEGATIVE {} {name} changed={changed}", s.id);
            }
            r.set_scene(&theme.vertices_for(&world));
            r.resize([640, 480]);
            let small = cap.join("small");
            fs::create_dir(&small).unwrap();
            let small_out = resources::smoke_output(root, &small).unwrap();
            r.capture(&small_out, "office.png", &pose, &world).unwrap();
            println!(
                "WHOLE_PACK_SPACE {} {}",
                s.id,
                r.pack_feedback.replace('\n', " | ")
            );
        }
        gpu.shared.wait().unwrap();
    });
    assert_eq!(fs::read(source).unwrap(), original);
    assert_eq!(fs::read(base.join("pack/model.glb")).unwrap(), original);
    println!(
        "WHOLE_PACK_METAL_PASS {} synthetic-egui=true native=false human=false",
        base.display()
    );
}
