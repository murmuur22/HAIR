use super::*;

fn pocket_page(
    a: Address,
    cancel: &hair::acquisition::Cancellation,
    deadline: Instant,
) -> Result<Page> {
    cancel.check(deadline)?;
    Page::html(a, b"<title>Pocket</title><h1>Orchard</h1><p>Apple trees.</p><h2>Harbour</h2><p>Boats arrive.</p><a href='https://second.example/landing'>Next pocket</a>")
}

#[test]
fn native_e_reads_each_section_and_refresh_preserves_binding() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let p = web_native::prepare(
        Address::parse("https://example.org/pocket").unwrap(),
        &[
            root.join("packs/office.toml"),
            root.join("packs/archive.toml"),
        ],
        pocket_page,
        &Default::default(),
    )
    .unwrap();
    let mut a = App::new(root, p.themes, 0, p.generated.world, false);
    a.web = Some(web_native::Runtime::new(p.generated.doors, pocket_page));
    for (wanted, other) in [
        ("Apple trees.", "Boats arrive."),
        ("Boats arrive.", "Apple trees."),
    ] {
        let key = a
            .world
            .pocket
            .as_ref()
            .unwrap()
            .documents
            .iter()
            .find(|(_, d)| d.text.contains(wanted))
            .unwrap()
            .0
            .clone();
        let anchor = a.themes[a.theme_index]
            .host
            .anchors
            .iter()
            .find(|h| h.id == key)
            .unwrap()
            .clone();
        a.pose = Pose {
            position: [anchor.at[0], anchor.at[2] + 1.2],
            yaw: 0.0,
            pitch: -0.5,
        };
        let pose = a.pose;
        a.keyboard_input(Some(KeyCode::KeyE), true, false, None)
            .unwrap();
        a.keyboard_input(Some(KeyCode::KeyE), false, false, None)
            .unwrap();
        assert!(a.world.reading);
        let text = a.world.document().unwrap().to_owned();
        assert!(text.contains(wanted));
        assert!(!text.contains(other));
        a.keyboard_input(Some(KeyCode::F5), true, false, None)
            .unwrap();
        a.keyboard_input(Some(KeyCode::F5), false, false, None)
            .unwrap();
        finish(&mut a);
        assert_eq!(
            a.world.document().unwrap(),
            text,
            "refresh must retain the uniquely identified reading collection"
        );
        a.keyboard_input(Some(KeyCode::Escape), true, false, None)
            .unwrap();
        a.keyboard_input(Some(KeyCode::Escape), false, false, None)
            .unwrap();
        assert!(!a.world.reading);
        // This CPU App intentionally has no renderer; T/reskin is proven below
        // with the real renderer rather than treating a no-op key as a switch.
        assert_eq!(a.pose.position, pose.position);
        assert_eq!(a.pose.yaw, pose.yaw);
        assert_eq!(a.pose.pitch, pose.pitch);
        a.keyboard_input(Some(KeyCode::KeyE), true, false, None)
            .unwrap();
        a.keyboard_input(Some(KeyCode::KeyE), false, false, None)
            .unwrap();
        assert_eq!(a.world.document().unwrap(), text);
        a.keyboard_input(Some(KeyCode::Escape), true, false, None)
            .unwrap();
        a.keyboard_input(Some(KeyCode::Escape), false, false, None)
            .unwrap();
    }
    a.web.as_mut().unwrap().shutdown();
}

#[test]
#[ignore = "explicit owned Metal captures; not a native window or human-input test"]
fn pocket_objects_and_readers_render_at_normal_and_small_sizes() {
    let output_root =
        PathBuf::from(std::env::var_os("HAIR_POCKET_CAPTURES").expect("explicit output directory"));
    assert!(!output_root.exists());
    std::fs::create_dir_all(&output_root).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let packs = root.parent().unwrap().join("proof-packs");
    for (name, width, height) in [("normal", 960, 640), ("small", 640, 480)] {
        let p = web_native::prepare(
            Address::parse("https://example.org/pocket").unwrap(),
            &[packs.join("office.toml"), packs.join("archive.toml")],
            pocket_page,
            &Default::default(),
        )
        .unwrap();
        let mut a = App::new(root.clone(), p.themes, 0, p.generated.world, false);
        a.web = Some(web_native::Runtime::new(p.generated.doors, pocket_page));
        let dir = output_root.join(name);
        std::fs::create_dir(&dir).unwrap();
        let output = resources::smoke_output(&a.root, &dir).unwrap();
        pollster::block_on(async {
            let instance = render::instance();
            let adapter = render::adapter(&instance, None).await.unwrap();
            assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
            let mut renderer = render::Renderer::new(
                &adapter,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                [width, height],
                &a.themes[0].vertices_for(&a.world),
            )
            .await
            .unwrap();
            renderer.set_materials(&a.themes[0].textures).unwrap();
            renderer.pack_feedback = a.themes[0].name.clone();
            a.renderer = Some(renderer);
        });
        a.pose = Pose {
            position: [0.0, 3.0],
            yaw: 0.0,
            pitch: -0.25,
        };
        a.update_hud();
        a.renderer
            .as_mut()
            .unwrap()
            .capture(&output, "office.png", &a.pose, &a.world)
            .unwrap();
        for (wanted, filename) in [
            ("Apple trees.", "reader-welcome.png"),
            ("Boats arrive.", "reader-workshop.png"),
        ] {
            let key = a
                .world
                .pocket
                .as_ref()
                .unwrap()
                .documents
                .iter()
                .find(|(_, d)| d.text.contains(wanted))
                .unwrap()
                .0
                .clone();
            let anchor = a.themes[0]
                .host
                .anchors
                .iter()
                .find(|h| h.id == key)
                .unwrap();
            a.pose = Pose {
                position: [anchor.at[0], anchor.at[2] + 1.2],
                yaw: 0.0,
                pitch: -0.5,
            };
            a.keyboard_input(Some(KeyCode::KeyE), true, false, None)
                .unwrap();
            a.keyboard_input(Some(KeyCode::KeyE), false, false, None)
                .unwrap();
            assert!(a.world.reading && a.world.document().unwrap().contains(wanted));
            let before_pack = a.theme_index;
            let before_pose = a.pose;
            let before_host = a.themes[before_pack].host.clone();
            let before_text = a.world.document().unwrap().to_owned();
            a.keyboard_input(Some(KeyCode::KeyT), true, false, None)
                .unwrap();
            a.keyboard_input(Some(KeyCode::KeyT), false, false, None)
                .unwrap();
            assert_eq!(a.theme_index, (before_pack + 1) % a.themes.len());
            assert_eq!(a.pose.position, before_pose.position);
            assert_eq!(a.themes[a.theme_index].host, before_host);
            assert_eq!(a.world.document().unwrap(), before_text);
            a.update_hud();
            a.renderer
                .as_mut()
                .unwrap()
                .capture(&output, filename, &a.pose, &a.world)
                .unwrap();
            a.keyboard_input(Some(KeyCode::Escape), true, false, None)
                .unwrap();
            a.keyboard_input(Some(KeyCode::Escape), false, false, None)
                .unwrap();
        }
        let failure_dir = output_root.join(format!("{name}-failure"));
        std::fs::create_dir(&failure_dir).unwrap();
        let failure_output = resources::smoke_output(&a.root, &failure_dir).unwrap();
        let door = a.web.as_ref().unwrap().doors[0].anchor.clone();
        let anchor = a.themes[a.theme_index]
            .host
            .anchors
            .iter()
            .find(|h| h.id == door)
            .unwrap();
        a.pose = Pose {
            position: [anchor.at[0] + 0.4, -2.7],
            yaw: 0.0,
            pitch: -0.5,
        };
        let before_pose = a.pose;
        let source = a.world.current.clone();
        a.web.as_mut().unwrap().fetcher = super::pocket_failure::unavailable;
        a.keyboard_input(Some(KeyCode::KeyE), true, false, None)
            .unwrap();
        a.keyboard_input(Some(KeyCode::KeyE), false, false, None)
            .unwrap();
        finish(&mut a);
        hair::interaction::refresh(&a.themes[a.theme_index].host, &a.pose, &mut a.world);
        assert!(
            hair::web::portals::view(&a.world, &door)
                .unwrap()
                .last_failure
                .is_some()
        );
        assert_eq!(a.world.current, source);
        assert_eq!(a.pose.position, before_pose.position);
        a.update_hud();
        a.renderer
            .as_mut()
            .unwrap()
            .capture(&failure_output, "office.png", &a.pose, &a.world)
            .unwrap();
        let before_pack = a.theme_index;
        a.keyboard_input(Some(KeyCode::KeyT), true, false, None)
            .unwrap();
        a.keyboard_input(Some(KeyCode::KeyT), false, false, None)
            .unwrap();
        assert_eq!(a.theme_index, (before_pack + 1) % a.themes.len());
        assert!(
            hair::web::portals::view(&a.world, &door)
                .unwrap()
                .last_failure
                .is_some()
        );
        a.update_hud();
        a.renderer
            .as_mut()
            .unwrap()
            .capture(&failure_output, "reader-welcome.png", &a.pose, &a.world)
            .unwrap();
        a.web.as_mut().unwrap().fetcher = pocket_page;
        a.keyboard_input(Some(KeyCode::KeyE), true, false, None)
            .unwrap();
        a.keyboard_input(Some(KeyCode::KeyE), false, false, None)
            .unwrap();
        finish(&mut a);
        assert_ne!(a.world.current, source);
        a.keyboard_input(Some(KeyCode::KeyR), true, false, None)
            .unwrap();
        a.keyboard_input(Some(KeyCode::KeyR), false, false, None)
            .unwrap();
        assert_eq!(a.world.current, source);
        assert_eq!(a.pose.position, before_pose.position);
        hair::interaction::refresh(&a.themes[a.theme_index].host, &a.pose, &mut a.world);
        assert!(
            hair::web::portals::view(&a.world, &door)
                .unwrap()
                .last_failure
                .is_none()
        );
        a.update_hud();
        a.renderer
            .as_mut()
            .unwrap()
            .capture(&failure_output, "reader-workshop.png", &a.pose, &a.world)
            .unwrap();
        a.web.as_mut().unwrap().shutdown();
    }
}
