use super::*;
fn app() -> App {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let theme = hair::theme::Theme::load(
        &root.join("packs/office.toml"),
        &mut hair::theme::AssetCache::default(),
    )
    .unwrap();
    App::new(
        root.to_path_buf(),
        vec![theme],
        0,
        World::load(root).unwrap(),
        false,
    )
}
#[test]
fn contextual_reverse_wrap_and_physical_tab_latch() {
    let mut a = two_targets();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_eq!(
        a.world.interaction.as_ref().unwrap().anchor,
        "main-document"
    );
    for repeat in [true, false, true] {
        a.key(KeyCode::Tab, true, repeat).unwrap();
        assert_eq!(
            a.world.interaction.as_ref().unwrap().anchor,
            "main-document",
            "held Tab must not cycle"
        );
    }
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::ShiftLeft, true, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_eq!(a.world.interaction.as_ref().unwrap().anchor, "main-link");
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_eq!(
        a.world.interaction.as_ref().unwrap().anchor,
        "main-document"
    );
    a.key(KeyCode::ShiftLeft, false, false).unwrap();
    a.focus_changed(false);
    assert!(a.world.explicit_target.is_none());
    a.key(KeyCode::Tab, true, false).unwrap();
    assert!(a.world.interaction.is_none());
}
pub(super) fn two_targets() -> App {
    let mut a = app();
    // Two different legitimate roles. No duplicate-role or geometry policy change.
    a.themes[0]
        .host
        .anchors
        .iter_mut()
        .find(|x| x.role == "document")
        .unwrap()
        .at = [-0.65, 0.1, -3.0];
    a.themes[0]
        .host
        .anchors
        .iter_mut()
        .find(|x| x.role == "document")
        .unwrap()
        .size = [0.3, 1.4, 0.3];
    a.themes[0]
        .host
        .anchors
        .iter_mut()
        .find(|x| x.role == "portal")
        .unwrap()
        .at[0] = 0.65;
    let encoded = include_str!("../host-scene.toml")
        .replace("at = [0.0, 0.1, 0.3]", "at = [-0.65, 0.1, -3.0]")
        .replace("size = [1.8, 1.4, 1.2]", "size = [0.3, 1.4, 0.3]")
        .replace("at = [0.0, 0.1, -4.0]", "at = [0.65, 0.1, -4.0]");
    a.themes[0].host = hair::host_scene::HostScene::parse(&encoded).unwrap();
    a.themes[0].document = [-0.65, -3.0];
    a.themes[0].portal = [0.65, -4.0];
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [0.0, -2.6],
        yaw: 0.0,
        pitch: -0.4,
    };
    a
}

#[test]
fn reader_tab_selects_actual_link_hud_and_e_follows_local_with_arrival() {
    let mut a = reader_app();
    let pose = (a.pose.position, a.pose.yaw, a.pose.pitch);
    let before = hair::text::overlay(&a.world, 960, 640, 0);
    a.key(KeyCode::Tab, true, false).unwrap();
    let after = hair::text::overlay(&a.world, 960, 640, 0);
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&before),
        bytemuck::cast_slice::<_, u8>(&after),
        "reader selection needs visible label/focus"
    );
    assert_eq!(
        a.world.selected, 0,
        "reader must not change world destination index"
    );
    assert_eq!(pose, (a.pose.position, a.pose.yaw, a.pose.pitch));
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert_eq!(
        a.world.current, "next.md",
        "reader E follows selected actual local link"
    );
    assert!(!a.world.reading);
    assert_eq!(a.pose.position, a.themes[0].host.spawn);
    assert!(a.world.interaction.is_none());
    a.key(KeyCode::KeyE, true, true).unwrap();
    assert_eq!(a.world.current, "next.md");
}
pub(super) fn reader_app() -> App {
    let mut a = app();
    let path = std::env::temp_dir().join(format!(
        "hair-context-reader-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("index.md"),"# Reader fixture\n[Next page](next.md)\n[External example](https://example.invalid/secret?token=do-not-log)\n[Missing](absent.md)\n").unwrap();
    std::fs::write(path.join("next.md"), "# Arrival\n[Back](index.md)\n").unwrap();
    a.world = World::load_site(&path).unwrap();
    std::fs::remove_dir_all(path).unwrap();
    a.world.reading = true;
    a
}

#[test]
fn reader_reverse_wrap_refusals_close_and_world_isolation() {
    let mut a = reader_app();
    a.scroll = 3;
    a.key(KeyCode::ShiftRight, true, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_eq!(a.world.selected_reader_link().unwrap().ordinal, 2);
    for repeat in [true, false] {
        a.key(KeyCode::Tab, true, repeat).unwrap();
    }
    assert_eq!(a.world.selected_reader_link().unwrap().ordinal, 2);
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert!(a.world.reading && a.world.current == "index.md");
    assert_eq!(a.scroll, 3, "failed activation preserves scroll");
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_eq!(a.world.selected_reader_link().unwrap().ordinal, 1);
    assert!(a.world.reader_link_description().contains("EXTERNAL"));
    assert!(!a.world.reader_link_description().contains("do-not-log"));
    a.key(KeyCode::KeyE, false, false).unwrap();
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert!(a.world.reading && a.world.current == "index.md");
    let selected = a.world.reader_link.clone();
    for key in [KeyCode::KeyW, KeyCode::KeyR, KeyCode::KeyF] {
        a.key(key, true, false).unwrap();
    }
    assert!(!a.input.forward);
    assert_eq!(selected, a.world.reader_link);
    assert!(a.world.explicit_target.is_none());
    a.key(KeyCode::Escape, true, false).unwrap();
    assert!(
        a.world.reader_link.is_none(),
        "Esc clears bounded reader selection"
    );
}

#[test]
fn contextual_invalidation_and_entry_reset_clear_named_selection() {
    let mut a = app();
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [0.0, 1.8],
        yaw: 0.7,
        pitch: -0.5,
    };
    a.key(KeyCode::Tab, true, false).unwrap();
    let identity = a.world.explicit_target.clone();
    a.pose.yaw = 0.0; // aiming elsewhere never wins over eligible explicit identity
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    assert_eq!(identity, a.world.explicit_target);
    a.key(KeyCode::KeyR, true, false).unwrap();
    assert!(
        a.world.explicit_target.is_none(),
        "explicit entry navigation clears selected anchor"
    );
}
#[test]
fn contextual_range_sector_occlusion_empty_and_single_are_bounded() {
    let mut a = app();
    for pose in [
        Pose::default(),
        Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [0.0, 1.8],
            yaw: std::f32::consts::PI,
            pitch: 0.0,
        },
    ] {
        a.pose = pose;
        a.key(KeyCode::Tab, false, false).unwrap();
        a.key(KeyCode::Tab, true, false).unwrap();
        assert!(a.world.interaction.is_none());
    }
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [0.0, 1.8],
        yaw: 0.7,
        pitch: -0.5,
    };
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    let chosen = a.world.explicit_target.clone();
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_eq!(chosen, a.world.explicit_target, "single wraps to itself");
    a.themes[0].host.geometry.push(hair::host_scene::Geometry {
        at: [0.0, 0.0, 1.4],
        size: [2.0, 3.0, 0.1],
    });
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    assert!(a.world.explicit_target.is_none() && a.world.interaction.is_none());
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert!(!a.world.reading);
    a.themes[0].host.geometry.clear();
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    a.pose.position = [0.0, 3.0];
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    assert!(a.world.explicit_target.is_none());
}
#[test]
fn reader_empty_single_stale_failed_local_and_image_are_inert() {
    let mut a = reader_app();
    a.world.current = "next.md".into();
    for _ in 0..4 {
        a.key(KeyCode::Tab, false, false).unwrap();
        a.key(KeyCode::Tab, true, false).unwrap();
        assert_eq!(a.world.selected_reader_link().unwrap().ordinal, 0);
    }
    a.world.site.as_mut().unwrap().pages.remove("index.md");
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert!(
        a.world.reading && a.world.current == "next.md",
        "failed local preserves last good state"
    );
    a.world
        .site
        .as_mut()
        .unwrap()
        .records
        .get_mut("next.md")
        .unwrap()[0]
        .destination = hair::content::Destination::Image("swatch.png".into());
    assert!(
        a.world.selected_reader_link().is_none(),
        "stale identity cannot follow changed record"
    );
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert!(a.world.reader_link_description().contains("IMAGE"));
    a.key(KeyCode::KeyE, false, false).unwrap();
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert!(a.world.reading && a.world.current == "next.md");
    a.world
        .site
        .as_mut()
        .unwrap()
        .records
        .get_mut("next.md")
        .unwrap()
        .clear();
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert!(a.world.reader_link.is_none());
    assert!(
        a.world
            .reader_link_description()
            .contains("NO DOCUMENT LINKS")
    );
}

#[test]
fn reader_self_link_closes_and_uses_same_arrival_contract() {
    let mut a = reader_app();
    let link = &mut a
        .world
        .site
        .as_mut()
        .unwrap()
        .records
        .get_mut("index.md")
        .unwrap()[0];
    link.destination = hair::content::Destination::Page("index.md".into());
    a.pose.position = [2.0, 2.0];
    a.scroll = 4;
    a.key(KeyCode::Tab, true, false).unwrap();
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert!(!a.world.reading);
    assert_eq!(
        a.pose.position, a.themes[0].host.spawn,
        "self-link follows same arrival contract"
    );
    assert_eq!(a.scroll, 0);
    assert_eq!(a.pose.yaw, Pose::default().yaw);
    assert_eq!(a.pose.pitch, Pose::default().pitch);
    assert!(
        a.world.reader_link.is_none()
            && a.world.explicit_target.is_none()
            && a.world.interaction.is_none()
    );
    assert_eq!(a.world.selected, 0);
}

#[test]
#[ignore = "serialized optimized offscreen Metal; HAIR_CONTEXT_CAPTURE_DIR required"]
fn rendered_contextual_selection_reader_tracer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir =
        PathBuf::from(std::env::var_os("HAIR_CONTEXT_CAPTURE_DIR").expect("explicit capture root"));
    let group = |name: &str| {
        let p = dir.join(name);
        std::fs::create_dir(&p).unwrap();
        resources::smoke_output(root, &p).unwrap()
    };
    let world_out = group("world");
    let archive_out = group("archive");
    let reader_out = group("reader");
    let arrival_out = group("follow");
    let mut a = reader_app();
    a.world.reading = false;
    let archive = hair::theme::Theme::load(
        &root.join("packs/archive.toml"),
        &mut hair::theme::AssetCache::default(),
    )
    .unwrap();
    a.themes.push(archive);
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    let mut renderer = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [960, 640],
        &a.themes[0].vertices_for(&a.world),
    ))
    .unwrap();
    renderer.set_materials(&a.themes[0].textures).unwrap();
    a.renderer = Some(renderer);
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [0.0, 1.8],
        yaw: 0.0,
        pitch: -0.5,
    };
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    let capture = |a: &App, out: &resources::SmokeOutput, name: &str| {
        // Native adapter owns scene updates; this deliberately does not repair it here.
        a.renderer
            .as_ref()
            .unwrap()
            .capture(out, name, &a.pose, &a.world)
            .unwrap();
    };
    capture(&a, &world_out, "office.png");
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_eq!(
        a.world.explicit_target.as_ref().unwrap().anchor,
        "main-document"
    );
    capture(&a, &world_out, "reader-workshop.png");
    let plain = png_pixels(&world_out.path().join("office.png"));
    let selected = png_pixels(&world_out.path().join("reader-workshop.png"));
    let changed = (64 * 960..600 * 960)
        .filter(|i| plain[i * 4..i * 4 + 4] != selected[i * 4..i * 4 + 4])
        .count();
    assert!(
        changed > 10,
        "actual world highlight must change off-HUD pixels: {changed}"
    );
    let identity = a.world.explicit_target.clone();
    a.key(KeyCode::Tab, true, true).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_eq!(identity, a.world.explicit_target);
    let pose = (a.pose.position, a.pose.yaw, a.pose.pitch);
    a.key(KeyCode::KeyT, true, false).unwrap();
    assert_eq!(pose, (a.pose.position, a.pose.yaw, a.pose.pitch));
    assert_eq!(identity, a.world.explicit_target);
    capture(&a, &archive_out, "office.png");
    a.key(KeyCode::KeyT, true, false).unwrap();
    a.focus_changed(false);
    assert!(a.world.explicit_target.is_none());
    a.key(KeyCode::Tab, true, false).unwrap();
    assert!(a.world.explicit_target.is_none());
    a.focus_changed(true);
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert!(a.world.reading);
    assert!(a.world.explicit_target.is_none());
    capture(&a, &reader_out, "office.png");
    // The world-held Tab cannot accidentally select a reader link before physical release.
    a.key(KeyCode::Tab, true, false).unwrap();
    assert!(a.world.reader_link.is_none());
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_eq!(a.world.selected_reader_link().unwrap().ordinal, 0);
    capture(&a, &reader_out, "reader-welcome.png");
    let unselected = png_pixels(&reader_out.path().join("office.png"));
    let focused = png_pixels(&reader_out.path().join("reader-welcome.png"));
    assert!(
        (558 * 960..582 * 960).any(|i| unselected[i * 4..i * 4 + 4] != focused[i * 4..i * 4 + 4]),
        "actual selected-link strip pixels must change"
    );
    a.key(KeyCode::ShiftLeft, true, false).unwrap();
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_eq!(a.world.selected_reader_link().unwrap().ordinal, 2);
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_eq!(a.world.selected_reader_link().unwrap().ordinal, 1);
    capture(&a, &reader_out, "reader-workshop.png");
    a.key(KeyCode::KeyE, false, false).unwrap();
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert!(a.world.reading && a.world.current == "index.md");
    a.key(KeyCode::Tab, false, false).unwrap();
    a.key(KeyCode::Tab, true, false).unwrap();
    assert_eq!(a.world.selected_reader_link().unwrap().ordinal, 0);
    a.key(KeyCode::ShiftLeft, false, false).unwrap();
    let link = a.world.reader_link.clone();
    let reader_pose = (a.pose.position, a.pose.yaw, a.pose.pitch);
    a.key(KeyCode::KeyT, true, false).unwrap();
    assert_eq!(link, a.world.reader_link);
    assert!(a.world.reading);
    assert_eq!(reader_pose, (a.pose.position, a.pose.yaw, a.pose.pitch));
    a.focus_changed(false);
    a.focus_changed(true);
    assert_eq!(link, a.world.reader_link);
    a.key(KeyCode::KeyW, true, false).unwrap();
    a.key(KeyCode::KeyR, true, false).unwrap();
    assert!(!a.input.forward && a.world.current == "index.md");
    a.key(KeyCode::KeyT, true, false).unwrap();
    a.key(KeyCode::KeyE, false, false).unwrap();
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert_eq!(a.world.current, "next.md");
    assert!(!a.world.reading);
    assert!(a.world.reader_link.is_none() && a.world.explicit_target.is_none());
    assert_eq!(a.pose.position, a.themes[0].host.spawn);
    assert_eq!(a.scroll, 0);
    capture(&a, &arrival_out, "office.png");
    a.key(KeyCode::KeyE, true, true).unwrap();
    assert_eq!(a.world.current, "next.md");
    a.renderer.as_ref().unwrap().wait().unwrap();
    println!(
        "SLICE2_OFFSCREEN_PASS world highlight changed_pixels={changed}; reader selection/reverse/release/follow/focus/reskin assertions; synthetic adapter input, not native/human"
    );
}
fn png_pixels(path: &Path) -> Vec<u8> {
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    assert_eq!((frame.width, frame.height), (960, 640));
    pixels.truncate(frame.buffer_size());
    assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    pixels
}

#[test]
fn contextual_tab_selects_near_off_aim_document_and_e_uses_highlighted_target() {
    let mut a = app();
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [0.0, 1.8],
        yaw: 0.7,
        pitch: -0.5,
    };
    assert!(hair::interaction::resolve(&a.themes[0].host, &a.pose, &a.world).is_none());
    let before = a.themes[0].vertices_for(&a.world);
    a.key(KeyCode::Tab, true, false).unwrap();
    let target = a
        .world
        .interaction
        .as_ref()
        .expect("Tab selects nearby visible off-crosshair document");
    assert_eq!(target.anchor, "main-document");
    assert_eq!(target.prompt, "E - Read");
    let after = a.themes[0].vertices_for(&a.world);
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&before),
        bytemuck::cast_slice::<_, u8>(&after),
        "selected host marker must be highlighted"
    );
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert!(
        a.world.reading,
        "E must use explicit target, not re-resolve aim only"
    );
}
