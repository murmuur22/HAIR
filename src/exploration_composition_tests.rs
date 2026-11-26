use super::*;
fn renderer(a: &mut App) {
    let i = render::instance();
    let ad = pollster::block_on(render::adapter(&i, None)).unwrap();
    let mut r = pollster::block_on(render::Renderer::new(
        &ad,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [960, 640],
        &a.themes[0].vertices_for(&a.world),
    ))
    .unwrap();
    r.set_theme_appearance(&a.themes[0]).unwrap();
    a.renderer = Some(r);
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [0.6, -1.5],
        yaw: std::f32::consts::PI,
        pitch: -0.2,
    };
}
fn shot(a: &mut App, label: &str) -> Vec<u8> {
    let root =
        PathBuf::from(std::env::var_os("HAIR_MAP_COMPOSITION_DIR").expect("explicit owned output"));
    let dir = root.join(label);
    std::fs::create_dir_all(&dir).unwrap();
    let out = resources::smoke_output(&a.root, &dir).unwrap();
    a.update_hud();
    a.renderer
        .as_mut()
        .unwrap()
        .capture(&out, "office.png", &a.pose, &a.world)
        .unwrap();
    std::fs::read(dir.join("office.png")).unwrap()
}
#[test]
#[ignore = "serialized Metal overlay priority; explicit HAIR_MAP_COMPOSITION_DIR"]
fn expanded_map_yields_to_visible_link_listing_pixels() {
    let mut a = app();
    renderer(&mut a);
    a.world.tree_collapsed = true;
    a.map_mode = hair::exploration::MapMode::Expanded;
    a.key(KeyCode::ShiftLeft, true, false).unwrap();
    a.key(KeyCode::KeyM, true, false).unwrap();
    a.key(KeyCode::KeyM, false, false).unwrap();
    a.key(KeyCode::ShiftLeft, false, false).unwrap();
    assert!(!a.world.tree_collapsed);
    let actual = shot(&mut a, "links-expanded");
    a.map_mode = hair::exploration::MapMode::Hidden;
    let control = shot(&mut a, "links-control");
    assert!(
        actual == control,
        "expanded map must yield, preserving the entire opened link panel frame"
    );
}
fn portal_fixture(
    address: Address,
    c: &hair::acquisition::Cancellation,
    d: Instant,
) -> Result<Page> {
    c.check(d)?;
    let tail = if address.url.path() == "/long" {
        "Segment".repeat(200)
    } else {
        "short".into()
    };
    Page::html(
        address,
        format!(
            "<title>Portal priority</title><a href='https://example.org/{tail}'>Source link</a>"
        )
        .as_bytes(),
    )
}
#[test]
#[ignore = "serialized Metal overlay priority; explicit HAIR_MAP_COMPOSITION_DIR"]
fn actionable_short_and_long_portals_keep_identical_disclosure_pixels() {
    for page in ["short", "long"] {
        let mut a = app();
        a.web.as_mut().unwrap().fetcher = portal_fixture;
        a.request_web(
            &format!("https://example.org/{page}"),
            web_native::Mode::Follow,
        )
        .unwrap();
        finish(&mut a);
        renderer(&mut a);
        // The maximal wrapped panel can legitimately occlude nearly the whole
        // 960px scene (12 colours); retain that generic smoke-guard refusal.
        // Use a larger observer viewport, not a weakened image guard.
        if page == "long" {
            a.renderer.as_mut().unwrap().resize([1920, 1280]);
        }
        let id = a.web.as_ref().unwrap().doors[0].anchor.clone();
        let host = &a.themes[0].host;
        let door = host.anchors.iter().find(|d| d.id == id).unwrap();
        let inside = host.portal_inward(&id);
        let near = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [door.at[0] + inside[0], door.at[2] + inside[1]],
            yaw: (-inside[0]).atan2(inside[1]),
            pitch: -0.1,
        };
        hair::interaction::refresh_with_windows(
            host,
            &near,
            &mut a.world,
            &a.web.as_ref().unwrap().window_doors,
        );
        assert_eq!(
            a.world.interaction.as_ref().map(|v| v.anchor.as_str()),
            Some(id.as_str())
        );
        // Explicit observer camera retains resolved selection for readable scene evidence.
        assert!(!hair::text::portal_overlay(&a.world, 960, 640).is_empty());
        a.map_mode = hair::exploration::MapMode::Hidden;
        let control = shot(&mut a, &format!("{page}-control"));
        for (name, mode) in [
            ("compact", hair::exploration::MapMode::Compact),
            ("expanded", hair::exploration::MapMode::Expanded),
        ] {
            a.map_mode = mode;
            let actual = shot(&mut a, &format!("{page}-{name}"));
            assert!(
                actual == control,
                "actionable portal disclosure takes priority over map"
            );
        }
    }
}
