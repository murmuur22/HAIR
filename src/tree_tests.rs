use super::*;
fn app() -> App {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let theme = hair::theme::Theme::load(
        &root.join("packs/office.toml"),
        &mut hair::theme::AssetCache::default(),
    )
    .unwrap();
    App::new(
        root.into(),
        vec![theme],
        0,
        World::load(root).unwrap(),
        false,
    )
}
#[test]
fn tree_m_release_repeat_focus_reader_and_reskin_isolation() {
    let mut a = app();
    assert!(a.world.tree_collapsed, "file tree defaults OFF");
    // Explicitly turn it ON through Settings; M must never change that choice.
    for code in [
        KeyCode::Escape,
        KeyCode::ArrowDown,
        KeyCode::Enter,
        KeyCode::Enter, // Settings > Player Settings
        KeyCode::ArrowDown,
        KeyCode::ArrowDown,
        KeyCode::ArrowDown,
        KeyCode::Enter,
        KeyCode::Escape,
        KeyCode::Escape,
        KeyCode::Escape,
    ] {
        a.key(code, true, false).unwrap();
        a.key(code, false, false).unwrap();
    }
    assert!(!a.world.tree_collapsed && a.menu.is_none());
    a.key(KeyCode::KeyM, true, false).unwrap();
    assert!(
        !a.world.tree_collapsed,
        "M never toggles the Settings-owned tree"
    );
    for repeat in [true, false, true] {
        a.key(KeyCode::KeyM, true, repeat).unwrap();
        assert!(!a.world.tree_collapsed);
    }
    a.world.reading = true;
    a.key(KeyCode::Tab, true, false).unwrap();
    let before = (
        a.world.current.clone(),
        a.world.reader_link.clone(),
        a.pose.position,
        a.world.selected,
    );
    a.key(KeyCode::KeyM, false, false).unwrap();
    a.key(KeyCode::KeyM, true, false).unwrap();
    assert!(!a.world.tree_collapsed);
    assert_eq!(
        before,
        (
            a.world.current.clone(),
            a.world.reader_link.clone(),
            a.pose.position,
            a.world.selected
        )
    );
    assert!(a.world.reading);
    a.key(KeyCode::KeyT, true, false).unwrap();
    assert!(!a.world.tree_collapsed);
    a.focus_changed(false);
    a.key(KeyCode::KeyM, true, false).unwrap();
    assert!(!a.world.tree_collapsed);
    a.focus_changed(true);
    a.key(KeyCode::KeyM, true, false).unwrap();
    assert!(!a.world.tree_collapsed);
    assert!(a.world.reading);
    assert!(!a.input.forward);
    a.key(KeyCode::KeyM, false, false).unwrap();
    a.key(KeyCode::KeyM, true, true).unwrap();
    assert!(!a.world.tree_collapsed);
}
#[test]
#[ignore = "serialized optimized Metal; explicit HAIR_TREE_CAPTURE_DIR and post-build HAIR_TREE_SITE"]
fn rendered_site_tree_tracer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = PathBuf::from(std::env::var_os("HAIR_TREE_CAPTURE_DIR").expect("capture root"));
    let site = PathBuf::from(std::env::var_os("HAIR_TREE_SITE").expect("post-build external site"));
    let host = hair::host_scene::HostScene::load_site(&site).unwrap();
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    for (label, size) in [("normal", [960, 640]), ("small", [640, 480])] {
        let mut cache = hair::theme::AssetCache::default();
        let themes = ["office", "archive"]
            .iter()
            .map(|n| {
                hair::theme::Theme::load_for_scene(
                    &root.join(format!("packs/{n}.toml")),
                    &mut cache,
                    &host,
                )
                .unwrap()
            })
            .collect();
        let mut a = App::new(
            root.into(),
            themes,
            0,
            World::load_site(&site).unwrap(),
            false,
        );
        assert_eq!(a.world.navigation.title, "Host: Greenleaf");
        assert_eq!(
            a.world
                .navigation
                .rows
                .iter()
                .map(|r| r.depth)
                .collect::<Vec<_>>(),
            [0, 1, 1]
        );
        let mut r = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            size,
            &a.themes[0].vertices_for(&a.world),
        ))
        .unwrap();
        r.set_materials(&a.themes[0].textures).unwrap();
        a.renderer = Some(r);
        let group = |name: &str| {
            let p = output.join(format!("{label}-{name}"));
            std::fs::create_dir(&p).unwrap();
            resources::smoke_output(root, &p).unwrap()
        };
        let world = group("world");
        let reader = group("reader");
        let follow = group("follow");
        capture(&a, &world, "office.png", size);
        a.key(KeyCode::KeyM, true, false).unwrap();
        assert!(a.world.tree_collapsed);
        capture(&a, &world, "reader-welcome.png", size);
        let pose = (a.pose.position, a.pose.yaw, a.pose.pitch);
        let nav = a.world.navigation.clone();
        a.key(KeyCode::KeyT, true, false).unwrap();
        assert_eq!(a.theme_index, 1);
        assert!(a.world.tree_collapsed);
        assert_eq!(nav, a.world.navigation);
        assert_eq!(pose, (a.pose.position, a.pose.yaw, a.pose.pitch));
        capture(&a, &world, "reader-workshop.png", size);
        a.key(KeyCode::KeyM, false, false).unwrap();
        a.key(KeyCode::KeyM, true, false).unwrap();
        a.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [-0.55, 1.1],
            yaw: 0.0,
            pitch: -0.5,
        };
        a.key(KeyCode::KeyE, true, false).unwrap();
        assert!(a.world.reading, "actual E opens station");
        a.key(KeyCode::Tab, true, false).unwrap();
        assert_eq!(a.world.selected_reader_link().unwrap().ordinal, 0);
        capture(&a, &reader, "office.png", size);
        let selection = a.world.reader_link.clone();
        a.key(KeyCode::KeyM, false, false).unwrap();
        a.key(KeyCode::KeyM, true, false).unwrap();
        assert!(a.world.tree_collapsed && a.world.reading);
        assert_eq!(selection, a.world.reader_link);
        capture(&a, &reader, "reader-welcome.png", size);
        a.key(KeyCode::KeyE, false, false).unwrap();
        a.key(KeyCode::KeyE, true, false).unwrap();
        assert_eq!(a.world.current, "noticeboard.md");
        assert!(!a.world.reading);
        assert!(a.world.tree_collapsed);
        assert_eq!(a.world.navigation.current_index(&a.world.current), Some(1));
        capture(&a, &follow, "office.png", size);
        a.key(KeyCode::KeyM, false, false).unwrap();
        a.key(KeyCode::KeyM, true, false).unwrap();
        capture(&a, &follow, "reader-welcome.png", size);
        a.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [1.6, -2.1],
            yaw: 0.0,
            pitch: -0.1,
        };
        a.key(KeyCode::KeyE, false, false).unwrap();
        a.key(KeyCode::KeyE, true, false).unwrap();
        assert_eq!(
            a.world.current, "workshop-notes.md",
            "actual door E changes page"
        );
        assert_eq!(a.world.navigation.current_index(&a.world.current), Some(2));
        assert!(!a.world.tree_collapsed);
        capture(&a, &follow, "reader-workshop.png", size);
        a.renderer.as_ref().unwrap().wait().unwrap();
        println!(
            "TREE_INPUT_TRACE {label} size={size:?}: M collapse -> T retained -> M expand -> E reader -> Tab local -> M no follow -> E noticeboard arrival collapsed -> M current highlight -> E door workshop-notes; state+actual HUD raster verified"
        );
    }
}
pub(super) fn capture(a: &App, out: &resources::SmokeOutput, name: &str, size: [u32; 2]) {
    a.renderer
        .as_ref()
        .unwrap()
        .capture(out, name, &a.pose, &a.world)
        .unwrap();
    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(out.path().join(name)).unwrap(),
    ));
    let mut r = decoder.read_info().unwrap();
    let mut pixels = vec![0; r.output_buffer_size().unwrap()];
    let f = r.next_frame(&mut pixels).unwrap();
    assert_eq!([f.width, f.height], size);
    assert!(
        pixels[..f.buffer_size()]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[3] == 255)
    );
    // Independently rasterize actual expected HUD quads over the entire tree rectangle.
    // Exact glyph/background/current color check, not just a screenshot difference.
    // These captures use the three-row site at supported 960x640/640x480.
    // Grow the reference rectangle with the 24px header and 20px row pitch;
    // continue checking EVERY background/glyph/highlight pixel, not the old crop.
    let h = if a.world.tree_collapsed {
        24
    } else {
        24 + a.world.navigation.rows.len().min(6) * 20
    };
    let w = (size[0] as f32 * 0.32).min(260.0) as usize;
    let rgb = |c: f32| {
        let v = if c <= 0.0031308 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (v * 255.0).round() as u8
    };
    let mut expected = vec![[0u8; 3]; w * h];
    for q in hair::text::overlay(&a.world, size[0], size[1], 0)
        .as_chunks::<6>()
        .0
    {
        let xs: Vec<i32> = q
            .iter()
            .map(|v| ((v.position[0] + 1.0) * 0.5 * size[0] as f32).round() as i32)
            .collect();
        let ys: Vec<i32> = q
            .iter()
            .map(|v| ((1.0 - v.position[1]) * 0.5 * size[1] as f32).round() as i32)
            .collect();
        for y in (*ys.iter().min().unwrap()).max(76)..(*ys.iter().max().unwrap()).min(76 + h as i32)
        {
            for x in
                (*xs.iter().min().unwrap()).max(12)..(*xs.iter().max().unwrap()).min(12 + w as i32)
            {
                expected[(y as usize - 76) * w + x as usize - 12] = q[0].color.map(rgb);
            }
        }
    }
    let mut checked = 0;
    for y in 0..h {
        for x in 0..w {
            let i = ((y + 76) * size[0] as usize + x + 12) * 4;
            for c in 0..3 {
                assert!(
                    pixels[i + c].abs_diff(expected[y * w + x][c]) <= 1,
                    "tree HUD raster {name} x={x} y={y} channel={c}"
                );
            }
            checked += 1;
        }
    }
    println!(
        "TREE_CAPTURE {name} size={size:?} page={} collapsed={} reading={} checked_tree_pixels={checked}",
        a.world.current, a.world.tree_collapsed, a.world.reading
    );
}
