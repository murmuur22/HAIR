use super::*;

// Test-only diagnostic readback. No production capture threshold/writer change.
fn sparse_diagnostic(a: &mut App, base: &Path) {
    let saved = a.pose;
    a.pose = Pose {
        position: [0.0, -4.9],
        yaw: std::f32::consts::PI,
        pitch: -0.15,
    };
    let r = a.renderer.as_mut().unwrap();
    r.pack_feedback = a.themes[0].feedback(&a.world);
    r.set_scene(&a.themes[0].vertices_for(&a.world));
    let [w, h] = r.size;
    let t = r.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("UNCERTIFIED original sparse hallway pose"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    r.draw(&t.create_view(&Default::default()), &a.pose, &a.world, 0)
        .unwrap();
    let pitch = (w * 4).div_ceil(256) * 256;
    let b = r.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("UNCERTIFIED pixels"),
        size: (pitch * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut e = r.device.create_command_encoder(&Default::default());
    e.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &t,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &b,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    r.queue.submit([e.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    b.slice(..).map_async(wgpu::MapMode::Read, move |v| {
        let _ = tx.send(v);
    });
    r.wait().unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    let map = b.slice(..).get_mapped_range().unwrap();
    let pixels: Vec<u8> = map
        .chunks(pitch as usize)
        .flat_map(|row| row[..w as usize * 4].iter().copied())
        .collect();
    drop(map);
    b.unmap();
    let colors = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert_eq!(colors, 13, "exact original sparse pose replay");
    assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    let dir = base.join("UNCERTIFIED-original-sparse");
    std::fs::create_dir(&dir).unwrap();
    let out = resources::smoke_output(&a.root, &dir).unwrap();
    assert!(
        r.capture(&out, "reader-workshop.png", &a.pose, &a.world)
            .unwrap_err()
            .contains("13 colors")
    );
    // Explicit uncertified evidence only; not a generic smoke pass or a semantic oracle.
    let mut f = out.create_capture("office.png").unwrap();
    let mut png = png::Encoder::new(&mut f, w, h);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    let mut png = png.write_header().unwrap();
    png.write_image_data(&pixels).unwrap();
    png.finish().unwrap();
    std::io::Write::flush(&mut f).unwrap();
    println!(
        "UNCERTIFIED_SPARSE original={:?} colors={colors} production20guard still refuses",
        a.pose
    );
    a.pose = saved;
}

fn distributable_packs(base: &Path, host: &hair::host_scene::HostScene) -> Vec<hair::theme::Theme> {
    use hair::authoring::Project;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let inputs = base.join("synthetic-inputs");
    std::fs::create_dir(&inputs).unwrap();
    let mut themes = Vec::new();
    for variant in 0..2 {
        // Rights-cleared many-color surface fixtures, not the unavailable owner pack.
        let path = inputs.join(format!("surface-{variant}.png"));
        let mut rgba = Vec::new();
        for y in 0..16 {
            for x in 0..16 {
                rgba.extend(if variant == 0 {
                    [40 + x * 8, 80 + y * 8, 40 + (x + y) * 4, 255]
                } else {
                    [40 + y * 6, 40 + x * 6, 100 + (x + y) * 4, 255]
                });
            }
        }
        let f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let mut p = png::Encoder::new(f, 16, 16);
        p.set_color(png::ColorType::Rgba);
        p.set_depth(png::BitDepth::Eight);
        let mut p = p.write_header().unwrap();
        p.write_image_data(&rgba).unwrap();
        p.finish().unwrap();
        let model = root.join("milestone-workshop/package/starter/model.glb");
        let mut project = Project::import(&model).unwrap();
        project.select_model(false).unwrap();
        let mut next = Project::import(&model).unwrap();
        next.assign_import(&project);
        project = next;
        project.select_door().unwrap();
        let mut next = Project::import(&root.join("examples/door-trim-starter/model.glb")).unwrap();
        next.assign_import(&project);
        project = next;
        project.import_surface("floor", &path).unwrap();
        project.import_surface("wall", &path).unwrap();
        let manifest = project
            .export(
                &base.join(format!("exported-pack-{variant}")),
                std::slice::from_ref(&root.to_path_buf()),
            )
            .unwrap();
        let theme =
            hair::theme::Theme::load_for_scene(&manifest, &mut Default::default(), host).unwrap();
        assert_eq!(theme.loaded_roles.len(), 3);
        assert!(
            !theme
                .diagnostics
                .iter()
                .any(|v| v.starts_with("ROLE_FALLBACK"))
        );
        themes.push(theme);
    }
    themes
}
fn suite() -> App {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let world = World::load_site(&root.join("examples/GreenleafReadingRoom-hallway")).unwrap();
    let host = &world.spaces.as_ref().unwrap().spaces[0].scene;
    let mut cache = hair::theme::AssetCache::default();
    let themes = ["office", "archive"]
        .iter()
        .map(|name| {
            hair::theme::Theme::load_for_scene(
                &root.join(format!("packs/{name}.toml")),
                &mut cache,
                host,
            )
            .unwrap()
        })
        .collect();
    App::new(root.into(), themes, 0, world, false)
}
fn press(a: &mut App, k: KeyCode) {
    assert!(!a.key(k, false, false).unwrap());
    assert!(!a.key(k, true, false).unwrap());
}
fn walk(a: &mut App, to: [f32; 2]) {
    assert!(!a.world.reading);
    a.input.captured = true; // Synthetic capture state, not an OS mouse claim.
    a.key(KeyCode::KeyW, true, false).unwrap();
    let mut arrived = false;
    for _ in 0..1000 {
        let dx = to[0] - a.pose.position[0];
        let dz = to[1] - a.pose.position[1];
        let d = dx.hypot(dz);
        if d < 0.015 {
            arrived = true;
            break;
        }
        a.pose.yaw = dx.atan2(-dz);
        let before = a.pose.position;
        // Same movement call and reader gate as App::redraw, bounded deterministic dt.
        a.themes[a.theme_index].locomote(&mut a.pose, &a.input, (d / 2.5).min(0.02));
        assert_ne!(a.pose.position, before, "blocked on walk to {to:?}");
        assert!((a.pose.position[0] - before[0]).hypot(a.pose.position[1] - before[1]) <= 0.051);
        assert!(!a.themes[a.theme_index].blocked(a.pose.position));
    }
    a.key(KeyCode::KeyW, false, false).unwrap();
    assert!(arrived, "walk failed to finish");
}
fn aim(a: &mut App, id: &str) {
    let anchor = a.themes[a.theme_index]
        .host
        .anchors
        .iter()
        .find(|v| v.id == id)
        .unwrap();
    let dx = anchor.at[0] - a.pose.position[0];
    let dz = anchor.at[2] - a.pose.position[1];
    a.pose.yaw = dx.atan2(-dz);
    a.pose.pitch = (anchor.at[1] + anchor.size[1] * 0.5 - 1.65).atan2(dx.hypot(dz));
    hair::interaction::refresh(&a.themes[a.theme_index].host, &a.pose, &mut a.world);
    assert_eq!(a.world.interaction.as_ref().unwrap().anchor, id);
}
#[test]
fn continuous_suite_actual_keys_read_tree_links_and_physical_return() {
    let mut a = suite();
    walk(&mut a, [0.0, -4.9]);
    aim(&mut a, "entrance-note");
    press(&mut a, KeyCode::Tab);
    press(&mut a, KeyCode::KeyE);
    assert!(a.world.reading);
    assert_eq!(a.world.current, "index.md");
    let pose = a.pose.position;
    press(&mut a, KeyCode::KeyW);
    assert!(!a.input.forward, "reader isolates movement key");
    assert_eq!(a.pose.position, pose);
    press(&mut a, KeyCode::Escape);
    for to in [[0.0, -2.0], [0.0, 2.0], [2.4, 2.0]] {
        walk(&mut a, to);
    }
    aim(&mut a, "hallway-note");
    press(&mut a, KeyCode::KeyE);
    assert!(a.world.reading);
    assert_eq!(a.world.current, "hallway.md");
    assert_eq!(a.world.navigation.current_index(&a.world.current), Some(1));
    press(&mut a, KeyCode::Escape);
    for to in [[2.5, 2.0], [2.5, 4.5], [-0.9, 4.5]] {
        walk(&mut a, to);
    }
    aim(&mut a, "reading-note");
    press(&mut a, KeyCode::KeyE);
    assert_eq!(a.world.current, "reading.md");
    assert_eq!(a.world.current_space, "greenleaf-suite");
    assert_eq!(a.world.navigation.current_index(&a.world.current), Some(2));
    press(&mut a, KeyCode::Tab);
    assert_eq!(
        a.world.selected_reader_link().unwrap().link.destination,
        hair::content::Destination::Page("index.md".into())
    );
    press(&mut a, KeyCode::KeyE);
    assert!(!a.world.reading);
    assert_eq!(a.world.current, "index.md");
    assert_eq!(
        a.pose.position,
        [0.0, -5.9],
        "reader link is explicitly an arrival reset, not walking"
    );
    for to in [
        [0.0, -2.0],
        [0.0, 2.0],
        [2.5, 2.0],
        [2.5, 4.5],
        [2.5, 2.0],
        [0.0, 2.0],
        [0.0, -2.0],
        [0.0, -5.9],
    ] {
        walk(&mut a, to);
    }
    assert_eq!(a.world.current, "index.md");
    press(&mut a, KeyCode::KeyR);
    assert_eq!(a.pose.position, [0.0, -5.9]);
}

#[test]
#[ignore = "finite serialized Metal with actual key adapter; HAIR_HALLWAY_CAPTURE_DIR required"]
fn rendered_continuous_hallway_reskins_keep_all_state() {
    let base = PathBuf::from(
        std::env::var_os("HAIR_HALLWAY_CAPTURE_DIR").expect("explicit separate capture root"),
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut a = suite();
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    let mut r = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [960, 640],
        &a.themes[0].vertices_for(&a.world),
    ))
    .unwrap();
    r.set_materials(&a.themes[0].textures).unwrap();
    a.renderer = Some(r);
    sparse_diagnostic(&mut a, &base);
    let host = a.themes[0].host.clone();
    // The sparse office/default view remains unqualified. These separate rendered
    // pack proofs include genuine exported surface/model roles, not a lower guard.
    a.themes = distributable_packs(&base, &host);
    a.renderer
        .as_mut()
        .unwrap()
        .set_materials(&a.themes[0].textures)
        .unwrap();
    for (area, route, anchor, page) in [
        ("entrance", vec![[0.0, -5.9]], "entrance-note", "index.md"),
        (
            "bend",
            vec![[0.0, -2.0], [0.0, 2.0], [2.4, 2.0]],
            "hallway-note",
            "hallway.md",
        ),
        (
            "reading",
            vec![[2.5, 2.0], [2.5, 4.5]],
            "reading-note",
            "reading.md",
        ),
    ] {
        for to in route {
            walk(&mut a, to);
        }
        // Views include the local marker/shape instead of an empty wall close-up.
        // These are diagnostic views reached by walking; no claimed replay of failed pose.
        let look = if area == "entrance" {
            [-0.4, -3.0]
        } else if area == "bend" {
            [3.0, 2.5]
        } else {
            [-2.0, 4.5]
        };
        a.pose.yaw = (look[0] - a.pose.position[0]).atan2(-(look[1] - a.pose.position[1]));
        a.pose.pitch = -0.15;
        for theme in 0..2 {
            let dir = base.join(format!("{area}-{theme}"));
            std::fs::create_dir(&dir).unwrap();
            let out = resources::smoke_output(root, &dir).unwrap();
            hair::interaction::refresh(&host, &a.pose, &mut a.world);
            let r = a.renderer.as_mut().unwrap();
            r.pack_feedback = a.themes[a.theme_index].feedback(&a.world);
            r.set_scene(&a.themes[a.theme_index].vertices_for(&a.world));
            r.capture(&out, "office.png", &a.pose, &a.world).unwrap();
            let before = (
                a.pose.position,
                a.pose.yaw,
                a.pose.pitch,
                a.world.current.clone(),
                a.world.current_space.clone(),
                a.world.reading,
                a.world.explicit_target.clone(),
                a.world.reader_link.clone(),
                a.world.tree_collapsed,
                a.scroll,
            );
            let old = a.theme_index;
            press(&mut a, KeyCode::KeyT);
            assert_ne!(a.theme_index, old);
            assert_eq!(host, a.themes[a.theme_index].host);
            assert_eq!(
                before,
                (
                    a.pose.position,
                    a.pose.yaw,
                    a.pose.pitch,
                    a.world.current.clone(),
                    a.world.current_space.clone(),
                    a.world.reading,
                    a.world.explicit_target.clone(),
                    a.world.reader_link.clone(),
                    a.world.tree_collapsed,
                    a.scroll
                )
            );
        }
        if area == "entrance" {
            walk(&mut a, [0.0, -4.9]);
        }
        if area == "reading" {
            walk(&mut a, [-0.9, 4.5]);
        }
        aim(&mut a, anchor);
        press(&mut a, KeyCode::Tab);
        let target = a.world.explicit_target.clone();
        assert!(target.is_some());
        press(&mut a, KeyCode::KeyT);
        assert_eq!(target, a.world.explicit_target);
        press(&mut a, KeyCode::KeyE);
        assert!(a.world.reading);
        assert_eq!(a.world.current, page);
        press(&mut a, KeyCode::Tab);
        assert!(a.world.reader_link.is_some());
        press(&mut a, KeyCode::KeyM);
        press(&mut a, KeyCode::ArrowDown);
        let before = (
            a.pose.position,
            a.pose.yaw,
            a.pose.pitch,
            a.world.current.clone(),
            a.world.current_space.clone(),
            a.world.reading,
            a.world.explicit_target.clone(),
            a.world.reader_link.clone(),
            a.world.tree_collapsed,
            a.scroll,
        );
        let source_before = a.world.document().unwrap().to_string();
        let routes_before = format!("{:?}", a.world.spaces);
        press(&mut a, KeyCode::KeyT);
        assert_eq!(source_before, a.world.document().unwrap());
        assert_eq!(routes_before, format!("{:?}", a.world.spaces));
        assert_eq!(
            before,
            (
                a.pose.position,
                a.pose.yaw,
                a.pose.pitch,
                a.world.current.clone(),
                a.world.current_space.clone(),
                a.world.reading,
                a.world.explicit_target.clone(),
                a.world.reader_link.clone(),
                a.world.tree_collapsed,
                a.scroll
            )
        );
        assert_eq!(host, a.themes[a.theme_index].host);
        assert_eq!(
            a.world.navigation.current_index(page),
            Some(if area == "entrance" {
                0
            } else if area == "bend" {
                1
            } else {
                2
            })
        );
        let dir = base.join(format!("{area}-reader"));
        std::fs::create_dir(&dir).unwrap();
        let out = resources::smoke_output(root, &dir).unwrap();
        let r = a.renderer.as_mut().unwrap();
        r.pack_feedback = a.themes[a.theme_index].feedback(&a.world);
        r.capture(&out, "reader-welcome.png", &a.pose, &a.world)
            .unwrap();
        // Return destination is unchanged by reskin in each shape.
        let g = a.world.spaces.as_ref().unwrap();
        assert_eq!(
            g.destination(
                &g.entry_space,
                &g.entry_page,
                &g.entry_arrival,
                a.world.site.as_ref().unwrap()
            )
            .unwrap()
            .1
            .position,
            [0.0, -5.9]
        );
        press(&mut a, KeyCode::Escape);
        if a.world.tree_collapsed {
            press(&mut a, KeyCode::KeyM);
        }
    }
    for to in [[2.5, 4.5], [2.5, 2.0], [0.0, 2.0], [0.0, -2.0], [0.0, -5.9]] {
        walk(&mut a, to);
    }
    assert_eq!(
        a.world.current, "reading.md",
        "physical return preserves current document"
    );
    press(&mut a, KeyCode::KeyR);
    assert_eq!(a.world.current, "index.md");
    assert_eq!(a.pose.position, [0.0, -5.9]);
    assert_eq!(host, a.themes[a.theme_index].host);
    println!(
        "CONTINUOUS_HALLWAY_GPU_PASS 9 captures, bounded WASD locomotion through bend and back; actual E/Tab/M/T/R adapter; 2 packs, one shared scene; no native/human input claim"
    );
}
