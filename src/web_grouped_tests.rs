use super::*;
fn page(a: Address, _: &hair::acquisition::Cancellation, _: Instant) -> Result<Page> {
    Page::html(a,b"<title>Grouped fixture</title><p>Read me</p><a href='/b?q=AB%2Fcd#one'>First place</a><a href='/b?q=AB%2Fcd#two'>Second place</a>")
}
fn app() -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let p = web_native::prepare(
        Address::parse("https://example.org/start").unwrap(),
        &[
            root.join("packs/office.toml"),
            root.join("packs/archive.toml"),
        ],
        page,
        &Default::default(),
    )
    .unwrap();
    let mut a = App::new(root, p.themes, 1, p.generated.world, false);
    a.web = Some(web_native::Runtime::new(p.generated.doors, page));
    aim(&mut a);
    a
}
fn aim(a: &mut App) {
    let id = &a.web.as_ref().unwrap().doors[0].anchor;
    let anchor = a.themes[a.theme_index]
        .host
        .anchors
        .iter()
        .find(|x| &x.id == id)
        .unwrap();
    a.pose = Pose {
        position: [anchor.at[0], -2.8],
        yaw: 0.0,
        pitch: -0.2,
    };
    hair::interaction::refresh(&a.themes[a.theme_index].host, &a.pose, &mut a.world);
}
fn view(a: &App) -> hair::web::portals::View {
    hair::web::portals::view(&a.world, &a.web.as_ref().unwrap().doors[0].anchor).unwrap()
}
fn tap(a: &mut App, k: KeyCode) {
    a.key(k, true, false).unwrap();
    a.key(k, false, false).unwrap();
}
#[test]
fn grouped_input_adapter_latches_context_focus_escape_and_exact_displayed_follow_back() {
    let mut a = app();
    let initial = view(&a);
    a.key(KeyCode::Tab, true, false).unwrap();
    let selected = view(&a);
    assert_ne!(initial.destination, selected.destination);
    a.key(KeyCode::Tab, true, false).unwrap();
    a.key(KeyCode::Tab, true, true).unwrap();
    assert_eq!(view(&a), selected);
    a.key(KeyCode::Tab, false, false).unwrap();
    tap(&mut a, KeyCode::ShiftLeft); // release means forward
    a.key(KeyCode::ShiftRight, true, false).unwrap();
    tap(&mut a, KeyCode::Tab);
    assert_eq!(view(&a).destination, initial.destination);
    a.key(KeyCode::ShiftRight, false, false).unwrap();
    tap(&mut a, KeyCode::Tab);
    assert_eq!(view(&a), selected);
    a.world.reading = true;
    tap(&mut a, KeyCode::Tab);
    assert!(a.world.selected_reader_link().is_some());
    assert_eq!(view(&a), selected);
    tap(&mut a, KeyCode::Escape);
    assert!(!a.world.reading);
    a.input.captured = true;
    a.input.forward = true;
    tap(&mut a, KeyCode::Escape);
    assert!(!a.input.captured);
    assert!(!a.input.forward);
    // Presentation menu migration: world Escape now pauses (releasing capture as
    // before); a second Escape resumes so the remaining world oracle is unchanged.
    assert!(a.menu.is_some());
    tap(&mut a, KeyCode::Escape);
    assert!(a.menu.is_none() && !a.input.captured);
    a.focus_changed(false);
    tap(&mut a, KeyCode::Tab);
    tap(&mut a, KeyCode::KeyE);
    assert!(a.web.as_ref().unwrap().pending.is_none());
    assert_eq!(view(&a), selected);
    a.focus_changed(true);
    aim(&mut a);
    let prompt = a.world.interaction.as_ref().unwrap();
    assert_eq!(
        prompt.destination.as_deref(),
        Some(selected.destination.as_str())
    );
    assert!(prompt.prompt.contains(&selected.title));
    let source = a.themes[1].source.clone();
    let pose = a.pose;
    let host = a.themes[1].host.clone();
    a.key(KeyCode::KeyE, true, false).unwrap();
    a.key(KeyCode::KeyE, true, false).unwrap();
    a.key(KeyCode::KeyE, true, true).unwrap();
    finish(&mut a);
    assert_eq!(a.world.current, selected.destination);
    assert_eq!(a.theme_index, 1);
    assert_eq!(a.themes[1].source, source);
    assert_eq!(a.web.as_ref().unwrap().history.len(), 1);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(view(&a), selected);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.themes[1].host, host);
}
#[test]
fn grouped_refresh_preserves_surviving_selection_and_explicitly_recovers_removal() {
    let mut a = app();
    tap(&mut a, KeyCode::Tab);
    let selected = view(&a);
    let host = a.themes[1].host.clone();
    let pose = a.pose;
    a.web.as_mut().unwrap().fetcher = |a, _, _| {
        Page::html(a,b"<p>Changed</p><a href='/b?q=AB%2Fcd#zero'>Inserted</a><a href='/b?q=AB%2Fcd#two'>Renamed second</a><a href='/b?q=AB%2Fcd#one'>First place</a>")
    };
    tap(&mut a, KeyCode::F5);
    finish(&mut a);
    assert_eq!(
        view(&a).destination,
        selected.destination,
        "refresh must preserve full target, not default/ordinal"
    );
    assert_eq!(view(&a).title, "Renamed second");
    assert!(!view(&a).recovered);
    assert_eq!(a.themes[1].host, host);
    assert_eq!(a.pose.position, pose.position);
    a.web.as_mut().unwrap().fetcher = |a, _, _| {
        Page::html(
            a,
            b"<p>Removed second</p><a href='/b?q=AB%2Fcd#one'>First place</a>",
        )
    };
    tap(&mut a, KeyCode::F5);
    finish(&mut a);
    assert!(view(&a).recovered);
    assert!(a.web.as_ref().unwrap().status.contains("removed"));
    assert_eq!(a.themes[1].host, host);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.web.as_ref().unwrap().history.len(), 0);
    let recovered = view(&a).destination;
    tap(&mut a, KeyCode::KeyE);
    finish(&mut a);
    assert_eq!(a.world.current, recovered);
}
#[test]
fn grouped_preview_changes_only_appearance_and_has_visible_accurate_label_and_address() {
    let mut a = app();
    let host = a.themes[1].host.clone();
    let pose = a.pose;
    a.world.explicit_target = a.world.interaction.clone();
    let first_vertices = a.themes[1].vertices_for(&a.world);
    let first_overlay = hair::text::overlay(&a.world, 960, 640, 0);
    let first = view(&a);
    tap(&mut a, KeyCode::Tab);
    let second = view(&a);
    assert_ne!(
        hair::web::portals::fallback(&first.destination),
        hair::web::portals::fallback(&second.destination)
    );
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&first_vertices),
        bytemuck::cast_slice::<_, u8>(&a.themes[1].vertices_for(&a.world)),
        "door preview must visibly change"
    );
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&first_overlay),
        bytemuck::cast_slice::<_, u8>(&hair::text::overlay(&a.world, 960, 640, 0))
    );
    assert_eq!(a.themes[1].host, host);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.world.current, "https://example.org/start");
}
fn pixels(path: &Path) -> Vec<u8> {
    let mut r = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
        .read_info()
        .unwrap();
    let mut bytes = vec![0; r.output_buffer_size().unwrap()];
    r.next_frame(&mut bytes).unwrap();
    bytes
}
fn panel_matches(bytes: &[u8], world: &World, w: u32, h: u32) -> bool {
    if bytes.len() != (w * h * 4) as usize {
        return false;
    }
    let quads = hair::text::portal_overlay(world, w, h);
    if quads.is_empty() {
        return false;
    }
    let srgb = |v: f32| -> u8 {
        ((if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }) * 255.0)
            .round() as u8
    };
    let mut matched = 0;
    // Foreground unit-pixel quads assert the actual full case-preserving title,
    // address and placeholder label raster, not just CPU state or image dimensions.
    for q in quads.as_chunks::<6>().0.iter().skip(1) {
        let x = ((q[0].position[0] + 1.0) * w as f32 * 0.5).round() as usize;
        let y = ((1.0 - q[0].position[1]) * h as f32 * 0.5).round() as usize;
        if x >= w as usize || y >= h as usize {
            return false;
        }
        let p = &bytes[(y * w as usize + x) * 4..][..4];
        if p[3] != 255
            || q[0]
                .color
                .iter()
                .enumerate()
                .any(|(i, c)| p[i].abs_diff(srgb(*c)) > 2)
        {
            return false;
        }
        matched += 1;
    }
    matched > 200
}
#[test]
#[ignore = "bounded offscreen Metal grouped-door selected title/address/placeholder, actual Tab/T/E/R/F5 adapter; explicit scratch outputs"]
fn rendered_grouped_doorway_selection_preview_and_pack() {
    let root = PathBuf::from(std::env::var_os("HAIR_GROUP_CAPTURE_DIR").unwrap())
        .canonicalize()
        .unwrap();
    let packs = PathBuf::from(std::env::var_os("HAIR_GROUP_PACK_DIR").unwrap())
        .canonicalize()
        .unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    for boundary in [&source, &packs] {
        assert!(!root.starts_with(boundary) && !boundary.starts_with(&root));
    }
    for (name, w, h) in [("normal", 960, 640), ("small", 640, 480)] {
        let dir = root.join(name);
        std::fs::create_dir(&dir).unwrap();
        let mut a = app();
        // Separate downward/inward view includes real textured floor as context;
        // the original flat close-up failure remains an unchanged rejected gate.
        a.pose.position[1] = -2.4;
        a.pose.pitch = -0.5;
        hair::interaction::refresh(&a.themes[1].host, &a.pose, &mut a.world);
        assert!(a.world.interaction.is_some());
        let host = a.themes[1].host.clone();
        let pose = a.pose;
        let mut cache = hair::theme::AssetCache::default();
        a.themes = ["office.toml", "archive.toml"]
            .iter()
            .map(|n| hair::theme::Theme::load_for_scene(&packs.join(n), &mut cache, &host).unwrap())
            .collect();
        a.world.explicit_target = a.world.interaction.clone();
        let output = resources::smoke_output(&a.root, &dir).unwrap();
        pollster::block_on(async {
            let instance = render::instance();
            let adapter = render::adapter(&instance, None).await.unwrap();
            println!("OBSERVED GPU {:?}", adapter.get_info());
            assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
            let mut r = render::Renderer::new(
                &adapter,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                [w, h],
                &a.themes[1].vertices_for(&a.world),
            )
            .await
            .unwrap();
            r.set_materials(&a.themes[1].textures).unwrap();
            r.pack_feedback = a.themes[1].name.clone();
            a.renderer = Some(r);
            a.renderer
                .as_mut()
                .unwrap()
                .capture(&output, "office.png", &a.pose, &a.world)
                .unwrap();
            let first = pixels(&dir.join("office.png"));
            assert!(panel_matches(&first, &a.world, w, h));
            let initial = view(&a);
            tap(&mut a, KeyCode::Tab);
            let selected = view(&a);
            a.renderer
                .as_mut()
                .unwrap()
                .capture(&output, "reader-welcome.png", &a.pose, &a.world)
                .unwrap();
            let second = pixels(&dir.join("reader-welcome.png"));
            assert!(panel_matches(&second, &a.world, w, h));
            assert!(
                !panel_matches(&first, &a.world, w, h),
                "wrong-selection actual frame must fail"
            );
            assert!(!panel_matches(&vec![0; first.len()], &a.world, w, h));
            assert!(!panel_matches(&second[..second.len() - 4], &a.world, w, h));
            let changed = (70..h / 2)
                .flat_map(|y| (0..w).map(move |x| ((y * w + x) * 4) as usize))
                .filter(|i| first[*i..*i + 4] != second[*i..*i + 4])
                .count();
            assert!(
                changed > 100,
                "portal image region must change independently of bottom text, got {changed}"
            );
            assert_ne!(initial.destination, selected.destination);
            tap(&mut a, KeyCode::KeyT);
            assert_eq!(a.theme_index, 0);
            assert_eq!(view(&a), selected);
            assert_eq!(a.themes[0].host, host);
            assert_eq!(a.pose.position, pose.position);
            a.renderer
                .as_mut()
                .unwrap()
                .capture(&output, "reader-workshop.png", &a.pose, &a.world)
                .unwrap();
            let reskin = pixels(&dir.join("reader-workshop.png"));
            assert!(panel_matches(&reskin, &a.world, w, h));
            assert_ne!(reskin, second);
            tap(&mut a, KeyCode::F5);
            finish(&mut a);
            assert_eq!(view(&a), selected);
            assert_eq!(a.themes[0].host, host);
            tap(&mut a, KeyCode::KeyE);
            finish(&mut a);
            assert_eq!(a.world.current, selected.destination);
            assert_eq!(a.theme_index, 0);
            tap(&mut a, KeyCode::KeyR);
            assert_eq!(view(&a), selected);
            assert_eq!(a.pose.position, pose.position);
            a.renderer.as_mut().unwrap().wait().unwrap();
            println!(
                "OBSERVED GROUPED_METAL {w}x{h} selected={:?} exact_panel_raster=true changed_portal_region_pixels={changed} wrong_state_blank_truncated_rejected=true pack_switch_refresh_follow_back=true",
                selected
            );
        });
    }
}
