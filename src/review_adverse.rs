use super::*;
#[path = "address_repair_tests.rs"]
mod event_order;
fn press(a: &mut App, code: KeyCode) {
    assert!(!a.keyboard_input(Some(code), true, false, None).unwrap());
    assert!(!a.keyboard_input(Some(code), false, false, None).unwrap());
}
fn local(fetcher: web_native::Fetcher) -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut a = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    a.web = Some(web_native::Runtime::new(Vec::new(), fetcher));
    a
}
fn poll(a: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while a.web.as_ref().is_some_and(|w| w.pending.is_some()) {
        a.poll_web().unwrap();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
fn submit(a: &mut App, raw: &str) {
    press(a, KeyCode::KeyL);
    a.keyboard_input(None, true, false, Some(raw)).unwrap();
    press(a, KeyCode::Enter);
}
fn wait_for_cancel(_: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    loop {
        c.check(d)?;
        std::thread::yield_now();
    }
}
#[test]
fn initial_cancel_restores_local_interaction_mode() {
    let mut a = local(wait_for_cancel);
    let before = a.world.current.clone();
    submit(&mut a, "https://example.org/slow");
    press(&mut a, KeyCode::KeyR);
    poll(&mut a);
    assert_eq!(a.world.current, before);
    println!(
        "INDEPENDENT initial cancellation local={} web_mode={} status={:?}",
        a.world.current,
        a.web.is_some(),
        a.web.as_ref().map(|w| &w.status)
    );
    let document = a.themes[0]
        .host
        .anchors
        .iter()
        .find(|x| x.role == "document")
        .unwrap();
    a.pose = Pose {
        position: [document.at[0], document.at[2] + 1.5],
        yaw: 0.0,
        pitch: -0.1,
    };
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    assert_eq!(
        a.world.interaction.as_ref().unwrap().kind,
        hair::interaction::Kind::Read,
        "probe must aim at actual readable local document"
    );
    press(&mut a, KeyCode::KeyE);
    println!(
        "INDEPENDENT local E after cancellation: reading={} status={:?}",
        a.world.reading,
        a.web.as_ref().map(|w| &w.status)
    );
    assert!(
        a.world.reading,
        "cancelled initial acquisition strands bundled local document: aimed E cannot read"
    );
}
#[test]
fn initial_failure_does_not_reopen_entry_after_focus_loss() {
    let mut a = local(|_, _, _| Err("independent offline refusal".into()));
    submit(&mut a, "https://example.org/refused");
    a.focus_changed(false);
    poll(&mut a);
    println!(
        "INDEPENDENT failure after focus loss: focused={} address={:?}",
        a.focused, a.address
    );
    assert!(
        a.address.is_none(),
        "async initial refusal must not reopen transient address focus after focus loss"
    );
}
#[test]
fn initial_failure_releases_movement_before_opening_entry() {
    let mut a = local(|_, _, _| Err("independent offline refusal".into()));
    submit(&mut a, "https://example.org/refused");
    a.key(KeyCode::KeyW, true, false).unwrap();
    assert!(a.input.forward);
    // A user can capture look/movement while the initial request is pending.
    // This sets that prerequisite, not a claim of native mouse-event testing.
    a.input.captured = true;
    poll(&mut a);
    assert!(a.address.is_some());
    let pose = a.pose.position;
    a.themes[a.theme_index].locomote(&mut a.pose, &a.input, 0.1);
    println!(
        "INDEPENDENT async failure address active forward={} before={pose:?} after={:?}",
        a.input.forward, a.pose.position
    );
    assert!(
        !a.input.forward,
        "async error opened address entry without releasing held world movement"
    );
    assert_eq!(
        pose, a.pose.position,
        "typing context must not continue moving"
    );
}
#[test]
fn address_context_consumes_e_release_without_sticking_action() {
    let mut a = app();
    a.world.reading = true;
    press(&mut a, KeyCode::Tab);
    // Hold E after an unsuccessful interaction so there is no asynchronous job.
    a.world.reader_link = None;
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert!(a.interact_held);
    press(&mut a, KeyCode::KeyL);
    a.key(KeyCode::KeyE, false, false).unwrap();
    press(&mut a, KeyCode::Escape);
    println!(
        "INDEPENDENT E released during address: held={}",
        a.interact_held
    );
    assert!(
        !a.interact_held,
        "physical E release swallowed by address focus; next deliberate E is ignored"
    );
}
#[test]
fn address_context_consumes_tab_release_without_sticking_cycle() {
    let mut a = app();
    a.world.reading = true;
    a.key(KeyCode::Tab, true, false).unwrap();
    let old = a.world.selected_reader_link().unwrap().link.clone();
    press(&mut a, KeyCode::KeyL);
    a.key(KeyCode::Tab, false, false).unwrap();
    press(&mut a, KeyCode::Escape);
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_ne!(
        a.world.selected_reader_link().unwrap().link,
        old,
        "physical Tab release swallowed while editing; first new Tab cannot cycle"
    );
}
#[test]
fn address_shift_release_does_not_leave_reverse_selection_stuck() {
    let mut a = app();
    a.key(KeyCode::ShiftLeft, true, false).unwrap();
    press(&mut a, KeyCode::KeyL);
    a.key(KeyCode::ShiftLeft, false, false).unwrap();
    press(&mut a, KeyCode::Escape);
    assert_eq!(
        a.shifts,
        [false, false],
        "shift release in entry must not leave reverse cycle latched"
    );
}
#[test]
fn literal_bounds_and_malformed_addresses_refuse_without_jobs() {
    let mut a = app();
    let before = a.world.current.clone();
    for raw in [
        "https://@example.org/a",
        "HTTPS://example.org/a",
        "https://example.org/a/../b",
        "https://example.org/%xz",
        "https://example.org:443/a",
        "https://example.org/a b",
        "https://example.org/ä",
    ] {
        press(&mut a, KeyCode::KeyL);
        a.keyboard_input(None, true, false, Some(raw)).unwrap();
        press(&mut a, KeyCode::Enter);
        assert!(a.address_error.is_some(), "{raw}");
        assert!(a.web.as_ref().unwrap().pending.is_none());
        assert_eq!(a.world.current, before);
        press(&mut a, KeyCode::Escape);
    }
    press(&mut a, KeyCode::KeyL);
    let raw = format!("https://example.org/{}", "x".repeat(2028));
    assert_eq!(raw.len(), 2048);
    a.keyboard_input(None, true, false, Some(&raw)).unwrap();
    assert_eq!(a.address.as_deref(), Some(raw.as_str()));
    a.keyboard_input(None, true, false, Some("x")).unwrap();
    assert_eq!(a.address.as_deref(), Some(raw.as_str()));
    assert!(a.address_error.is_some());
    press(&mut a, KeyCode::Delete);
    a.keyboard_input(None, true, false, Some("bad\ntext"))
        .unwrap();
    assert_eq!(a.address.as_deref(), Some(""));
}
#[test]
fn cancellation_of_refresh_preserves_transaction() {
    let mut a = app();
    let current = a.world.current.clone();
    let host = a.themes[0].host.clone();
    let pose = a.pose;
    let text = a.world.document().unwrap().to_string();
    a.web.as_mut().unwrap().fetcher = wait_for_cancel;
    press(&mut a, KeyCode::F5);
    press(&mut a, KeyCode::KeyR);
    poll(&mut a);
    assert_eq!(a.world.current, current);
    assert_eq!(a.world.document().unwrap(), text);
    assert_eq!(a.themes[0].host, host);
    assert_eq!(a.pose.position, pose.position);
    assert!(a.web.as_ref().unwrap().history.is_empty());
}
#[test]
fn typed_follow_failure_preserves_reader_pack_pose_and_retry() {
    let mut a = app();
    a.world.reading = true;
    a.scroll = 3;
    let before = a.world.current.clone();
    let pose = a.pose;
    let source = a.themes[0].source.clone();
    submit(&mut a, "https://example.org/bad");
    poll(&mut a);
    assert!(a.world.reading);
    assert_eq!(a.scroll, 3);
    assert_eq!(a.world.current, before);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.themes[0].source, source);
    assert!(a.web.as_ref().unwrap().history.is_empty());
    submit(&mut a, "https://second.example/Path?q=Ab%2Fcd#Case");
    poll(&mut a);
    assert_eq!(
        a.world.current,
        "https://second.example/Path?q=Ab%2Fcd#Case"
    );
    assert_eq!(a.web.as_ref().unwrap().history.len(), 1);
    press(&mut a, KeyCode::KeyR);
    assert_eq!(a.world.current, before);
    assert!(a.world.reading);
    assert_eq!(a.scroll, 3);
}
#[test]
fn current_reader_footer_and_pack_hud_have_disjoint_vertical_geometry() {
    let mut a = app();
    a.world.reading = true;
    for (w, h) in [(960, 640), (640, 480)] {
        let verts = hair::text::overlay(&a.world, w, h, 0);
        let footer: Vec<_> = verts
            .iter()
            .filter(|v| v.color == [0.4, 0.95, 0.6])
            .collect();
        assert!(!footer.is_empty());
        for v in footer {
            let y = (1.0 - v.position[1]) * h as f32 / 2.0;
            assert!(y >= h as f32 - 32.001 && y <= h as f32 - 17.99);
            assert!(y > h as f32 - 42.0, "pack HUD covers footer glyphs");
            assert!(
                (-1.001..=1.001).contains(&v.position[0]),
                "necessary footer help offscreen"
            );
        }
        println!(
            "INDEPENDENT source-derived footer {w}x{h}: required ESC/scroll/R/F5 line in viewport; separate from pack rows; not image viewing or legibility certification"
        );
    }
}
#[test]
fn actual_captured_reader_recovery_help_is_not_painted_over() {
    let mut a = app();
    a.world.reading = true;
    let verts = hair::text::overlay(&a.world, 960, 640, 0);
    let quads: Vec<_> = verts
        .as_chunks::<6>()
        .0
        .iter()
        .filter(|q| q[0].color == [0.4, 0.95, 0.6])
        .collect();
    let srgb = |v: f32| -> u8 {
        ((if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }) * 255.0)
            .round() as u8
    };
    // The inherited test depended on ephemeral captures in another scratch
    // milestone. Generate CURRENT real frames instead; never synthesize old PNGs
    // or claim these replace evidence of those historical browsing journeys.
    let base =
        std::env::temp_dir().join(format!("hair-current-reader-help-{}", std::process::id()));
    std::fs::create_dir(&base).unwrap();
    for (version, feedback) in [
        ("current-normal", "Current pack feedback"),
        (
            "current-long",
            "Long pack/status text that must remain suppressed while the reader owns its footer",
        ),
        (
            "current-error",
            "REFUSED destination | last good place | return/refresh recovery",
        ),
    ] {
        let dir = base.join(version);
        std::fs::create_dir(&dir).unwrap();
        let output = resources::smoke_output(&a.root, &dir).unwrap();
        pollster::block_on(async {
            let instance = render::instance();
            let adapter = render::adapter(&instance, None).await.unwrap();
            let mut renderer = render::Renderer::new(
                &adapter,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                [960, 640],
                &a.themes[a.theme_index].vertices_for(&a.world),
            )
            .await
            .unwrap();
            renderer
                .set_materials(&a.themes[a.theme_index].textures)
                .unwrap();
            renderer.pack_feedback = feedback.into();
            renderer
                .capture(&output, "reader-workshop.png", &a.pose, &a.world)
                .unwrap();
            renderer.error_probe().await.unwrap();
            renderer.wait().unwrap();
        });
        let name = "reader-workshop.png";
        let path = dir.join(name);
        let mut reader =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()))
                .read_info()
                .unwrap();
        assert_eq!((reader.info().width, reader.info().height), (960, 640));
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        let mut missing = 0;
        for q in &quads {
            let x = ((q[0].position[0] + 1.0) * 480.0).round() as usize;
            let y = ((1.0 - q[0].position[1]) * 320.0).round() as usize;
            let pixel = &pixels[(y * 960 + x) * 4..][..4];
            if pixel[3] != 255 || (0..3).any(|i| pixel[i].abs_diff(srgb(q[0].color[i])) > 2) {
                missing += 1;
            }
        }
        println!(
            "CURRENT actual reader recovery footer raster {version}/{name}: expectedGlyphQuads={} missingOrOverpainted={missing}; exact ESC/UP-DOWN/STATIC HTML/R RETURN/F5 REFRESH, not human legibility, historical capture recreation or whole-image review",
            quads.len()
        );
        assert_eq!(
            missing,
            0,
            "necessary reader recovery footer glyphs are missing/overpainted: {}",
            path.display()
        );
    }
    std::fs::remove_dir_all(&base).unwrap();
}
