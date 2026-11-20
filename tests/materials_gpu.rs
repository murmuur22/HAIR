#![cfg(feature = "workshop")]
use hair::{
    render, resources,
    workshop::{DialogKind, Workshop, WorkshopGpu},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn pixels(p: &Path) -> Vec<u8> {
    let mut r = png::Decoder::new(std::io::BufReader::new(fs::File::open(p).unwrap()))
        .read_info()
        .unwrap();
    let mut b = vec![0; r.output_buffer_size().unwrap()];
    r.next_frame(&mut b).unwrap();
    b
}
const SIZE: [u32; 2] = [1200, 1800];
fn frame(
    g: &mut WorkshopGpu,
    c: &egui::Context,
    a: &mut Workshop,
    t: &wgpu::Texture,
    events: Vec<egui::Event>,
) {
    g.frame(
        c,
        a,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(SIZE[0] as f32, SIZE[1] as f32),
            )),
            events,
            ..Default::default()
        },
        &t.create_view(&Default::default()),
        SIZE,
    )
    .unwrap();
}
fn click(g: &mut WorkshopGpu, c: &egui::Context, a: &mut Workshop, t: &wgpu::Texture, id: &str) {
    for _ in 0..3 {
        frame(g, c, a, t, vec![]);
    }
    let pos = a.controls[id].center();
    for pressed in [true, false] {
        frame(
            g,
            c,
            a,
            t,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: Default::default(),
                },
            ],
        );
    }
}
fn browse(
    g: &mut WorkshopGpu,
    c: &egui::Context,
    a: &mut Workshop,
    t: &wgpu::Texture,
    id: &str,
    kind: DialogKind,
    p: &Path,
) {
    click(g, c, a, t, id);
    assert_eq!(a.begin_dialog(), Some(kind));
    a.finish_dialog(Ok(Some(p.into())));
    assert!(a.error.is_none(), "{:?}", a.error);
}
fn write(out: &resources::SmokeOutput, name: &str, size: [u32; 2], pixels: &[u8]) {
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
    let mut e = png::Encoder::new(&mut f, size[0], size[1]);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    let mut w = e.write_header().unwrap();
    w.write_image_data(pixels).unwrap();
    w.finish().unwrap();
    std::io::Write::flush(&mut f).unwrap();
    println!("CAPTURE {name} colors={colors}");
}
#[test]
#[ignore = "finite real Metal synthetic UI + 3 Greenleaf surfaces; no OS/human"]
fn whole_pack_surfaces_ui_roundtrip_and_greenleaf_pixels_preserve_host_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = PathBuf::from(std::env::var_os("HAIR_MATERIAL_GPU_DIR").unwrap());
    fs::create_dir(&base).unwrap();
    let input = base.join("inputs");
    fs::create_dir(&input).unwrap();
    for (name, colors) in [
        ("wall", [[200, 60, 30, 255], [230, 180, 70, 255]]),
        ("floor", [[20, 120, 170, 255], [50, 210, 130, 255]]),
    ] {
        let mut pixels = vec![];
        for y in 0..16 {
            for x in 0..16 {
                pixels.extend(colors[((x / 4 + y / 4) % 2) as usize]);
            }
        }
        let f = fs::File::create(input.join(format!("{name}.png"))).unwrap();
        let mut e = png::Encoder::new(f, 16, 16);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        w.write_image_data(&pixels).unwrap();
        w.finish().unwrap();
    }
    pollster::block_on(async {
        let instance = render::instance();
        let adapter = render::adapter(&instance, None).await.unwrap();
        let mut g = WorkshopGpu::new(&adapter, wgpu::TextureFormat::Rgba8Unorm, SIZE)
            .await
            .unwrap();
        let t = g.target(SIZE);
        let c = egui::Context::default();
        let mut a = Workshop::new(vec![root.into(), input.clone()]);
        browse(
            &mut g,
            &c,
            &mut a,
            &t,
            "import",
            DialogKind::Import,
            &root.join("milestone-workshop/package/starter/model.glb"),
        );
        click(&mut g, &c, &mut a, &t, "decoration");
        browse(
            &mut g,
            &c,
            &mut a,
            &t,
            "import",
            DialogKind::Import,
            &root.join("milestone-workshop/package/starter/model.glb"),
        );
        click(&mut g, &c, &mut a, &t, "confirm-discard");
        click(&mut g, &c, &mut a, &t, "document");
        frame(&mut g, &c, &mut a, &t, vec![]);
        let before = g.read_pixels(&t, SIZE).unwrap();
        click(&mut g, &c, &mut a, &t, "wall");
        for (id, k, name) in [
            ("wall-png", DialogKind::WallTexture, "wall"),
            ("floor-png", DialogKind::FloorTexture, "floor"),
        ] {
            click(&mut g, &c, &mut a, &t, name);
            browse(
                &mut g,
                &c,
                &mut a,
                &t,
                id,
                k,
                &input.join(format!("{name}.png")),
            );
        }
        // Actual native-adapter save/reopen/export after actual egui browse clicks.
        click(&mut g, &c, &mut a, &t, "wall");
        for (id, k, p) in [
            ("save-new", DialogKind::SaveNew, base.join("saved")),
            ("open", DialogKind::Open, base.join("saved/project.toml")),
            ("export", DialogKind::Export, base.join("export")),
        ] {
            browse(&mut g, &c, &mut a, &t, id, k, &p);
        }
        frame(&mut g, &c, &mut a, &t, vec![]);
        let after = g.read_pixels(&t, SIZE).unwrap();
        let r = a.controls["preview"];
        let mut changed = 0;
        for y in r.min.y.ceil() as usize..r.max.y.floor() as usize {
            for x in r.min.x.ceil() as usize..r.max.x.floor() as usize {
                let i = (y * SIZE[0] as usize + x) * 4;
                if before[i..i + 3] != after[i..i + 3] {
                    changed += 1;
                }
            }
        }
        assert!(changed > 100);
        println!("WORKSHOP_MATERIAL_CHANGED_PIXELS={changed}");
        let caps = base.join("workshop");
        fs::create_dir(&caps).unwrap();
        let out = resources::smoke_output(root, &caps).unwrap();
        write(&out, "office.png", SIZE, &before);
        write(&out, "reader-welcome.png", SIZE, &after);
        click(&mut g, &c, &mut a, &t, "wall");
        frame(&mut g, &c, &mut a, &t, vec![]);
        let controls = g.read_pixels(&t, SIZE).unwrap();
        assert!(a.controls["wall-tile"].max.y < SIZE[1] as f32);
        write(&out, "reader-workshop.png", SIZE, &controls);
        // A separate small egui context/target; retained project, actual clipped scroll UI.
        let small = [640, 480];
        let st = g.target(small);
        let sc = egui::Context::default();
        let raw = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            events,
            ..Default::default()
        };
        for _ in 0..3 {
            g.frame(
                &sc,
                &mut a,
                raw(vec![]),
                &st.create_view(&Default::default()),
                small,
            )
            .unwrap();
        }
        // Expand by real pointer event, then scroll to material controls.
        let pos = a.controls["floor"].center();
        for pressed in [true, false] {
            g.frame(
                &sc,
                &mut a,
                raw(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                ]),
                &st.create_view(&Default::default()),
                small,
            )
            .unwrap();
        }
        for _ in 0..14 {
            g.frame(
                &sc,
                &mut a,
                raw(vec![
                    egui::Event::PointerMoved(egui::pos2(200.0, 200.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Unknown,
                        delta: egui::vec2(0.0, -70.0),
                        modifiers: Default::default(),
                    },
                ]),
                &st.create_view(&Default::default()),
                small,
            )
            .unwrap();
            if a.controls
                .get("floor-tile")
                .is_some_and(|r| r.center().y > 50.0 && r.center().y < 300.0)
            {
                break;
            }
        }
        let small_dir = base.join("workshop-small");
        fs::create_dir(&small_dir).unwrap();
        let so = resources::smoke_output(root, &small_dir).unwrap();
        write(
            &so,
            "office.png",
            small,
            &g.read_pixels(&st, small).unwrap(),
        );
        let pack = base.join("export/pack.toml");
        let loaded = hair::theme::Theme::load(&pack, &mut Default::default()).unwrap();
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&loaded.vertices),
            bytemuck::cast_slice::<_, u8>(&a.preview.as_ref().unwrap().vertices)
        );
        assert_eq!(loaded.loaded_roles.len(), 2);
        let world =
            hair::World::load_site(&root.join("examples/GreenleafReadingRoom-appearance")).unwrap();
        for (i, s) in world.spaces.as_ref().unwrap().spaces.iter().enumerate() {
            let mut plain =
                hair::theme::Theme::load_for_scene(&pack, &mut Default::default(), &s.scene)
                    .unwrap();
            plain.vertices[..144].copy_from_slice(&s.scene.vertices()[..144]);
            let styled =
                hair::theme::Theme::load_for_scene(&pack, &mut Default::default(), &s.scene)
                    .unwrap();
            let pose = hair::Pose {
                position: s.scene.spawn,
                // Old yaw0 faces the nearby outward portal; exact uncertified readback
                // and source diagnosis retained under hair-materials-r2/POSE-DIAGNOSIS.json.
                // Use the host's inward arrival direction for wall/floor/object inspection.
                yaw: std::f32::consts::PI,
                pitch: -0.25,
            };
            println!("MATERIAL_VIEW space={} pose={pose:?}", s.id);
            assert!(!s.scene.blocked(pose.position));
            styled.reskin_from(&plain, &pose).unwrap();
            assert_eq!(styled.host, s.scene);
            assert_eq!(
                styled
                    .host
                    .vertices()
                    .iter()
                    .map(|v| v.position)
                    .collect::<Vec<_>>(),
                s.scene
                    .vertices()
                    .iter()
                    .map(|v| v.position)
                    .collect::<Vec<_>>()
            );
            let mut w =
                hair::World::load_site(&root.join("examples/GreenleafReadingRoom-appearance"))
                    .unwrap();
            w.current = s.default_page.clone();
            w.current_space = s.id.clone();
            let state = (
                w.current.clone(),
                w.reading,
                w.selected,
                pose.position,
                pose.yaw,
                pose.pitch,
            );
            let dir = base.join(format!("greenleaf-{i}"));
            fs::create_dir(&dir).unwrap();
            let o = resources::smoke_output(root, &dir).unwrap();
            let mut renderer = render::Renderer::new(
                &adapter,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                [640, 480],
                &plain.vertices,
            )
            .await
            .unwrap();
            renderer.set_materials(&plain.textures).unwrap();
            renderer.capture(&o, "office.png", &pose, &w).unwrap();
            let a = pixels(&dir.join("office.png"));
            renderer.set_materials(&styled.textures).unwrap();
            renderer.set_scene(&styled.vertices);
            renderer
                .capture(&o, "reader-welcome.png", &pose, &w)
                .unwrap();
            let b = pixels(&dir.join("reader-welcome.png"));
            let changed = a
                .as_chunks::<4>()
                .0
                .iter()
                .zip(b.as_chunks::<4>().0.iter())
                .filter(|(a, b)| a != b)
                .count();
            assert!(changed > 100);
            assert_eq!(
                state,
                (
                    w.current.clone(),
                    w.reading,
                    w.selected,
                    pose.position,
                    pose.yaw,
                    pose.pitch
                )
            );
            println!(
                "GREENLEAF surface={} changed={changed} host_routes_anchors_collision=true",
                s.id
            );
        }
    });
}
