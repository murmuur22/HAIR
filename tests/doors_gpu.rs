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
fn draw(
    g: &mut WorkshopGpu,
    c: &egui::Context,
    a: &mut Workshop,
    t: &wgpu::Texture,
    size: [u32; 2],
    events: Vec<egui::Event>,
) {
    g.frame(
        c,
        a,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0] as f32, size[1] as f32),
            )),
            events,
            ..Default::default()
        },
        &t.create_view(&Default::default()),
        size,
    )
    .unwrap();
}
fn click(
    g: &mut WorkshopGpu,
    c: &egui::Context,
    a: &mut Workshop,
    t: &wgpu::Texture,
    size: [u32; 2],
    id: &str,
) {
    for _ in 0..3 {
        draw(g, c, a, t, size, vec![]);
    }
    for _ in 0..35 {
        let r = a.controls[id];
        if id == "confirm-discard" || a.controls["authoring-viewport"].contains(r.center()) {
            break;
        }
        let delta = if r.center().y < a.controls["authoring-viewport"].top() {
            100.0
        } else {
            -100.0
        };
        draw(
            g,
            c,
            a,
            t,
            size,
            vec![
                egui::Event::PointerMoved(egui::pos2(200.0, 160.0)),
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
            draw(g, c, a, t, size, vec![]);
        }
    }
    let pos = a.controls[id].center();
    assert!(
        (id == "confirm-discard"
            && egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0] as f32, size[1] as f32)
            )
            .contains(pos))
            || a.controls["authoring-viewport"].contains(pos),
        "unreachable {id} {pos:?}"
    );
    for pressed in [true, false] {
        draw(
            g,
            c,
            a,
            t,
            size,
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
// Test-only native-panel adapter keeps the render target and viewport explicit.
#[allow(clippy::too_many_arguments)]
fn browse(
    g: &mut WorkshopGpu,
    c: &egui::Context,
    a: &mut Workshop,
    t: &wgpu::Texture,
    size: [u32; 2],
    id: &str,
    kind: DialogKind,
    p: &Path,
) {
    click(g, c, a, t, size, id);
    assert_eq!(a.begin_dialog(), Some(kind));
    a.finish_dialog(Ok(Some(p.into())));
    if a.confirmation_pending() {
        click(g, c, a, t, size, "confirm-discard");
    }
    assert!(a.error.is_none(), "{:?}", a.error);
}
fn opening_readback(
    before: &[u8],
    after: &[u8],
    size: [u32; 2],
    pose: &hair::Pose,
    a: &hair::host_scene::Anchor,
) {
    use glam::{Mat4, Vec3};
    let eye = Vec3::new(pose.position[0], 1.65, pose.position[1]);
    let direction = Vec3::new(
        pose.yaw.sin() * pose.pitch.cos(),
        pose.pitch.sin(),
        -pose.yaw.cos() * pose.pitch.cos(),
    );
    let m = Mat4::perspective_rh(
        65.0f32.to_radians(),
        size[0] as f32 / size[1] as f32,
        0.05,
        50.0,
    ) * Mat4::look_to_rh(eye, direction, Vec3::Y);
    let mut count = 0;
    for x in [0.12, 0.18, 0.24, 0.30] {
        for y in [0.3, 0.68, 0.72, 0.76] {
            let p = m.project_point3(Vec3::new(
                a.at[0] + x * a.size[0],
                a.at[1] + y * a.size[1],
                a.at[2] - a.size[2] * 0.5,
            ));
            let px = ((p.x + 1.0) * 0.5 * size[0] as f32) as usize;
            let py = ((1.0 - p.y) * 0.5 * size[1] as f32) as usize;
            if px < 220 || px >= size[0] as usize - 5 || py < 100 || py >= size[1] as usize - 100 {
                continue;
            }
            let i = (py * size[0] as usize + px) * 4;
            assert_eq!(
                &before[i..i + 4],
                &after[i..i + 4],
                "aperture pixel occluded {px},{py}"
            );
            count += 1;
        }
    }
    assert!(count >= 2, "too few visible aperture samples {count}");
    println!("OPENING_READBACK unchanged central aperture samples={count}");
}
fn capture(o: &resources::SmokeOutput, name: &str, size: [u32; 2], p: &[u8]) {
    assert_eq!(p.len(), size[0] as usize * size[1] as usize * 4);
    let colors = p
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert!(colors >= 20);
    assert!(p.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    let mut f = o.create_capture(name).unwrap();
    let mut e = png::Encoder::new(&mut f, size[0], size[1]);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    let mut w = e.write_header().unwrap();
    w.write_image_data(p).unwrap();
    w.finish().unwrap();
    std::io::Write::flush(&mut f).unwrap();
}
#[test]
#[ignore = "finite actual Metal all-category synthetic-egui roundtrip and three-space door counterfactuals"]
fn all_categories_ui_export_and_visible_safe_doors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = PathBuf::from(std::env::var_os("HAIR_DOOR_GPU_DIR").unwrap());
    fs::create_dir(&base).unwrap();
    let surfaces = PathBuf::from(std::env::var_os("HAIR_DOOR_SURFACES").unwrap());
    let trim = PathBuf::from(std::env::var_os("HAIR_DOOR_MODEL").unwrap());
    pollster::block_on(async {
        let instance = render::instance();
        let adapter = render::adapter(&instance, None).await.unwrap();
        let size = [1200, 1800];
        let mut g = WorkshopGpu::new(&adapter, wgpu::TextureFormat::Rgba8Unorm, size)
            .await
            .unwrap();
        let t = g.target(size);
        let c = egui::Context::default();
        let mut a = Workshop::new(vec![root.into(), trim.parent().unwrap().into()]);
        browse(
            &mut g,
            &c,
            &mut a,
            &t,
            size,
            "import",
            DialogKind::Import,
            &root.join("milestone-workshop/package/starter/model.glb"),
        );
        click(&mut g, &c, &mut a, &t, size, "decoration");
        browse(
            &mut g,
            &c,
            &mut a,
            &t,
            size,
            "import",
            DialogKind::Import,
            &root.join("milestone-workshop/package/starter/model.glb"),
        );
        click(&mut g, &c, &mut a, &t, size, "door-trim");
        browse(
            &mut g,
            &c,
            &mut a,
            &t,
            size,
            "import",
            DialogKind::Import,
            &trim,
        );
        click(&mut g, &c, &mut a, &t, size, "document");
        assert!(!a.project.as_ref().unwrap().door_active);
        click(&mut g, &c, &mut a, &t, size, "door-trim");
        assert!(a.project.as_ref().unwrap().door_active);
        for (id, kind, name) in [
            ("wall-png", DialogKind::WallTexture, "wall"),
            ("floor-png", DialogKind::FloorTexture, "floor"),
        ] {
            click(&mut g, &c, &mut a, &t, size, name);
            browse(
                &mut g,
                &c,
                &mut a,
                &t,
                size,
                id,
                kind,
                &surfaces.join(format!("{name}.png")),
            );
        }
        for (id, kind, path) in [
            ("save-new", DialogKind::SaveNew, base.join("saved")),
            ("open", DialogKind::Open, base.join("saved/project.toml")),
            ("export", DialogKind::Export, base.join("export")),
        ] {
            browse(&mut g, &c, &mut a, &t, size, id, kind, &path);
        }
        draw(&mut g, &c, &mut a, &t, size, vec![]);
        let dir = base.join("workshop");
        fs::create_dir(&dir).unwrap();
        let out = resources::smoke_output(root, &dir).unwrap();
        capture(&out, "office.png", size, &g.read_pixels(&t, size).unwrap());
        let small = [640, 480];
        let st = g.target(small);
        let sc = egui::Context::default();
        for _ in 0..3 {
            draw(&mut g, &sc, &mut a, &st, small, vec![]);
        }
        click(&mut g, &sc, &mut a, &st, small, "document");
        click(&mut g, &sc, &mut a, &st, small, "door-trim");
        click(&mut g, &sc, &mut a, &st, small, "import");
        assert_eq!(a.begin_dialog(), Some(DialogKind::Import));
        let state = (a.revision, a.dirty);
        a.finish_dialog(Ok(None));
        assert_eq!(state, (a.revision, a.dirty));
        draw(&mut g, &sc, &mut a, &st, small, vec![]);
        let dir = base.join("workshop-small");
        fs::create_dir(&dir).unwrap();
        capture(
            &resources::smoke_output(root, &dir).unwrap(),
            "office.png",
            small,
            &g.read_pixels(&st, small).unwrap(),
        );
        // Current contextual editors: actual Metal frames, normal and bounded narrow widths.
        for size in [[1200, 900], [640, 480], [320, 480]] {
            let target = g.target(size);
            let context = egui::Context::default();
            let dir = base.join(format!("editors-{}", size[0]));
            fs::create_dir(&dir).unwrap();
            let editor_output = resources::smoke_output(root, &dir).unwrap();
            for (category, name) in [
                ("wall", "office.png"),
                ("floor", "reader-welcome.png"),
                ("door-trim", "reader-workshop.png"),
            ] {
                click(&mut g, &context, &mut a, &target, size, category);
                let import = if category == "wall" {
                    "wall-png"
                } else if category == "floor" {
                    "floor-png"
                } else {
                    "import"
                };
                click(&mut g, &context, &mut a, &target, size, import);
                let old = (a.revision, a.dirty);
                assert!(a.begin_dialog().is_some());
                a.finish_dialog(Ok(None));
                assert_eq!((a.revision, a.dirty), old);
                draw(&mut g, &context, &mut a, &target, size, vec![]);
                capture(
                    &editor_output,
                    name,
                    size,
                    &g.read_pixels(&target, size).unwrap(),
                );
            }
        }
        let pack = base.join("export/pack.toml");
        let mut v: toml::Value = toml::from_str(&fs::read_to_string(&pack).unwrap()).unwrap();
        v["roles"]
            .as_array_mut()
            .unwrap()
            .retain(|r| r["role"].as_str() != Some("door-trim"));
        let plain_path = base.join("export/placeholder.toml");
        fs::write(&plain_path, toml::to_string(&v).unwrap()).unwrap();
        let world =
            hair::World::load_site(&root.join("examples/GreenleafReadingRoom-appearance")).unwrap();
        for (i, s) in world.spaces.as_ref().unwrap().spaces.iter().enumerate() {
            for size in [[960, 640], [640, 480]] {
                let plain = hair::theme::Theme::load_for_scene(
                    &plain_path,
                    &mut Default::default(),
                    &s.scene,
                )
                .unwrap();
                let styled =
                    hair::theme::Theme::load_for_scene(&pack, &mut Default::default(), &s.scene)
                        .unwrap();
                assert_eq!(styled.loaded_roles.len(), 3);
                assert_eq!(styled.host, s.scene);
                assert_eq!(plain.host, styled.host);
                let mut w =
                    hair::World::load_site(&root.join("examples/GreenleafReadingRoom-appearance"))
                        .unwrap();
                w.current = s.default_page.clone();
                w.current_space = s.id.clone();
                let pose = hair::Pose {
                    position: s.scene.spawn,
                    yaw: 0.25,
                    pitch: -0.35,
                };
                hair::interaction::refresh(&styled.host, &pose, &mut w);
                assert!(w.interaction.is_some());
                assert!(!w.reading);
                let state = format!(
                    "{:?}",
                    (
                        &w.current,
                        &w.current_space,
                        w.reading,
                        w.selected,
                        &w.explicit_target,
                        &w.reader_link,
                        w.tree_collapsed,
                        &w.spaces
                    )
                );
                let dir = base.join(format!("greenleaf-{i}-{}", size[0]));
                fs::create_dir(&dir).unwrap();
                let o = resources::smoke_output(root, &dir).unwrap();
                let mut r = render::Renderer::new(
                    &adapter,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                    size,
                    &plain.vertices_for(&w),
                )
                .await
                .unwrap();
                r.set_materials(&plain.textures).unwrap();
                r.capture(&o, "office.png", &pose, &w).unwrap();
                r.set_materials(&styled.textures).unwrap();
                r.set_scene(&styled.vertices_for(&w));
                r.capture(&o, "reader-welcome.png", &pose, &w).unwrap();
                r.wait().unwrap();
                assert_eq!(
                    state,
                    format!(
                        "{:?}",
                        (
                            &w.current,
                            &w.current_space,
                            w.reading,
                            w.selected,
                            &w.explicit_target,
                            &w.reader_link,
                            w.tree_collapsed,
                            &w.spaces
                        )
                    )
                );
                let before = pixels(&dir.join("office.png"));
                let after = pixels(&dir.join("reader-welcome.png"));
                let anchor = styled
                    .host
                    .anchors
                    .iter()
                    .find(|a| a.id == w.interaction.as_ref().unwrap().anchor)
                    .unwrap();
                opening_readback(&before, &after, size, &pose, anchor);
                let changed = before
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(after.as_chunks::<4>().0)
                    .filter(|(a, b)| a != b)
                    .count();
                assert!(changed > 100, "door not visibly changed");
                println!(
                    "DOOR_VISIBLE space={} size={size:?} changed={changed} reader_closed=true target={:?} host_unchanged=true",
                    s.id, w.interaction
                );
            }
        }
    });
}
