use super::*;

fn physical_fixture(
    address: Address,
    c: &hair::acquisition::Cancellation,
    d: Instant,
) -> Result<Page> {
    c.check(d)?;
    let count = if address.url.path() == "/hall" { 64 } else { 4 };
    let mut html = format!(
        "<title>{} physical room</title><p>Owned room map fixture</p>",
        address.url.path()
    );
    for i in 0..count {
        html.push_str(&format!("<a href='/branch-{i}'>Branch {i}</a>"));
    }
    html.push_str("<a href='/hall'>Hall</a>");
    Page::html(address, html.as_bytes())
}
#[test]
#[ignore = "serialized actual Metal shared physical room graph; explicit capture output"]
fn physical_focus_graph_captures_branch_loop_and_hallway() {
    let mut a = app();
    a.web.as_mut().unwrap().fetcher = physical_fixture;
    a.request_web("https://example.org/hall", web_native::Mode::Follow)
        .unwrap();
    finish(&mut a);
    for i in 0..7 {
        a.request_web(
            &format!("https://example.org/branch-{i}"),
            web_native::Mode::Follow,
        )
        .unwrap();
        finish(&mut a);
        a.return_web().unwrap();
    }
    let current = a.exploration.current.unwrap();
    let layout = a.exploration.rooms[current].layout.clone().unwrap();
    assert_eq!(layout.bounds, a.themes[0].host.room);
    assert_eq!(layout.ports.len(), 66);
    let count = a.exploration.rooms.len();
    a.request_web("https://example.org/hall", web_native::Mode::Follow)
        .unwrap();
    finish(&mut a);
    assert_eq!(count, a.exploration.rooms.len());
    assert_eq!(a.exploration.current, Some(current));
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let mut r = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [960, 640],
        &a.themes[0].vertices_for(&a.world),
    ))
    .unwrap();
    r.set_theme_appearance(&a.themes[0]).unwrap();
    a.renderer = Some(r);
    a.pose = Pose {
        position: [0.6, -1.5],
        yaw: std::f32::consts::PI,
        pitch: -0.2,
    };
    a.world.explicit_target = None;
    a.world.interaction = None;
    a.world.tree_collapsed = true;
    let root = PathBuf::from(std::env::var_os("HAIR_EXPLORATION_CAPTURE_DIR").unwrap());
    for (label, size, scale) in [
        ("hall-min", [640, 480], 100),
        ("hall-normal", [960, 640], 100),
        ("hall-large", [1920, 1280], 200),
    ] {
        let dir = root.join(label);
        std::fs::create_dir(&dir).unwrap();
        let out = resources::smoke_output(&a.root, &dir).unwrap();
        a.map_mode = hair::exploration::MapMode::Expanded;
        a.ui_scale = scale;
        a.update_hud();
        let r = a.renderer.as_mut().unwrap();
        r.resize(size);
        r.capture(&out, "office.png", &a.pose, &a.world).unwrap();
    }
    a.renderer.as_ref().unwrap().wait().unwrap();
    assert_eq!(a.exploration.rooms[current].layout.as_ref(), Some(&layout));
}
#[test]
#[ignore = "serialized actual Metal map integration; HAIR_EXPLORATION_CAPTURE_DIR"]
fn actual_map_renderer_receives_navigation_and_captures_modes() {
    let mut a = app();
    for path in ["/guides", "/install", "/first-steps", "/examples"] {
        a.request_web(
            &format!("https://example.org{path}"),
            web_native::Mode::Follow,
        )
        .unwrap();
        finish(&mut a);
    }
    a.return_web().unwrap();
    a.pose = Pose {
        position: [0.0, 3.0],
        yaw: 0.0,
        pitch: 0.0,
    };
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    let mut r = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [960, 640],
        &a.themes[0].vertices_for(&a.world),
    ))
    .unwrap();
    r.set_theme_appearance(&a.themes[0]).unwrap();
    a.renderer = Some(r);
    a.update_hud();
    assert!(
        a.renderer.as_ref().unwrap().exploration.is_some(),
        "actual App HUD must publish the committed exploration snapshot"
    );
    let root = PathBuf::from(
        std::env::var_os("HAIR_EXPLORATION_CAPTURE_DIR").expect("explicit owned output root"),
    );
    for (label, mode, size, scale) in [
        (
            "compact-960",
            hair::exploration::MapMode::Compact,
            [960, 640],
            100,
        ),
        (
            "expanded-960",
            hair::exploration::MapMode::Expanded,
            [960, 640],
            100,
        ),
        (
            "hidden-960",
            hair::exploration::MapMode::Hidden,
            [960, 640],
            100,
        ),
        (
            "compact-640",
            hair::exploration::MapMode::Compact,
            [640, 480],
            100,
        ),
        (
            "expanded-640",
            hair::exploration::MapMode::Expanded,
            [640, 480],
            100,
        ),
        (
            "compact-capped",
            hair::exploration::MapMode::Compact,
            [960, 640],
            200,
        ),
        (
            "expanded-200",
            hair::exploration::MapMode::Expanded,
            [1920, 1280],
            200,
        ),
    ] {
        let dir = root.join(label);
        std::fs::create_dir_all(&dir).unwrap();
        let output = resources::smoke_output(&a.root, &dir).unwrap();
        a.map_mode = mode;
        a.ui_scale = scale;
        a.update_hud();
        let r = a.renderer.as_mut().unwrap();
        r.resize(size);
        r.capture(&output, "office.png", &a.pose, &a.world).unwrap();
    }
    a.world.reading = true;
    a.update_hud();
    assert!(
        a.renderer.as_ref().unwrap().exploration.is_none(),
        "reader owns its space"
    );
    a.world.reading = false;
    a.address = Some("https://example.org/".into());
    a.update_hud();
    assert!(
        a.renderer.as_ref().unwrap().exploration.is_none(),
        "address owns its space"
    );
    a.renderer.as_ref().unwrap().wait().unwrap();
}
