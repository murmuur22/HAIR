use super::*;

#[test]
fn slice3_motion_label_properties_are_independent_and_persist() {
    let (f, mut a) = open_fixture("slice3-properties");
    let source = std::fs::read(f.0.join("A.txt")).unwrap();
    let ino = std::fs::metadata(f.0.join("A.txt")).unwrap().ino();
    let unrelated = file_mesh(&a, "Z.txt");
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]);
    tap(&mut a, KeyCode::KeyJ);
    tap(&mut a, KeyCode::KeyK);
    tap(&mut a, KeyCode::Enter);
    let pending = a.place_mode.as_ref().unwrap().edit.as_ref().unwrap();
    assert!(pending.confirming());
    // Confirm through the real action, then inspect the bounded public record.
    tap(&mut a, KeyCode::Enter);
    assert!(
        a.local_status.contains("reposition saved"),
        "{}",
        a.local_status
    );
    let layout = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
    let text = toml::to_string(&layout.positions[0].1).unwrap();
    assert!(
        text.contains("animated = true"),
        "J must persist real animation: {text}"
    );
    assert!(
        text.contains("label = false"),
        "K must independently hide ordinary label: {text}"
    );
    let mut reopened = app();
    reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(
        hair::placement_store::read_layout(&reopened.local.as_ref().unwrap().explorer).unwrap(),
        layout
    );
    aim_file(&mut reopened, "A.txt");
    let target =
        hair::interaction::resolve(&reopened.themes[0].host, &reopened.pose, &reopened.world)
            .unwrap();
    assert!(
        target.prompt.contains("A.txt"),
        "hidden floating label never hides inspected source identity"
    );
    assert!(base_mesh(&reopened, "A.txt").is_empty());
    assert_eq!(file_mesh(&reopened, "Z.txt"), unrelated);
    assert_eq!(std::fs::read(f.0.join("A.txt")).unwrap(), source);
    assert_eq!(std::fs::metadata(f.0.join("A.txt")).unwrap().ino(), ino);
    assert_presentation_only(&mut reopened, "A.txt");
    let persisted = records(&f.0);
    tap(&mut reopened, KeyCode::F5);
    assert_eq!(
        hair::placement_store::read_layout(&reopened.local.as_ref().unwrap().explorer).unwrap(),
        layout
    );
    let alternate = hair::theme::Theme::load_for_folder_scene(
        &reopened.root.join("packs/archive.toml"),
        &mut Default::default(),
        &reopened.themes[0].host,
    )
    .unwrap();
    reopened.themes.push(alternate);
    reopened.menu = Some(hair::presentation::Menu::settings());
    reopened.activate_menu(hair::presentation::Item::Theme);
    assert_eq!(reopened.theme_index, 1);
    let id = id(&reopened, "A.txt");
    assert_eq!(
        reopened.themes[1].host.file_placements[&id].position,
        layout.positions[0].1
    );
    assert_ne!(
        bytes(&reopened.themes[1].vertices_for_at(&reopened.world, 0.)),
        bytes(&reopened.themes[1].vertices_for_at(&reopened.world, 3.))
    );
    reopened.activate_menu(hair::presentation::Item::Resume);
    assert_eq!(
        records(&f.0),
        persisted,
        "refresh/theme change is read-only"
    );
    aim_file(&mut reopened, "A.txt");
    tap(&mut reopened, KeyCode::KeyI);
    assert!(reopened.world.reading);
    assert!(
        reopened.world.document().unwrap().contains("A.txt"),
        "I discloses identity even with floating labels off"
    );
}

fn id(a: &App, name: &str) -> String {
    a.world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == std::ffi::OsStr::new(name))
        .unwrap()
        .0
        .clone()
}
fn bytes(mesh: &[hair::assets::Vertex]) -> Vec<u8> {
    bytemuck::cast_slice(mesh).to_vec()
}
fn records(root: &std::path::Path) -> Vec<(PathBuf, Vec<u8>, u64)> {
    let mut out = vec![];
    for entry in std::fs::read_dir(root.join(".hair-layout")).unwrap() {
        let path = entry.unwrap().path().join("record.toml");
        if path.is_file() {
            out.push((
                path.strip_prefix(root).unwrap().to_path_buf(),
                std::fs::read(&path).unwrap(),
                std::fs::metadata(&path).unwrap().ino(),
            ));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
fn edit_properties(a: &App) -> hair::placement::assembly::Properties {
    // The frozen public position exposes the edit properties without relying on
    // private implementation fields from a sibling module.
    a.place_mode
        .as_ref()
        .unwrap()
        .edit
        .as_ref()
        .unwrap()
        .reviewed_position()
        .unwrap()
        .properties
}
fn assert_presentation_only(a: &mut App, name: &str) {
    let folder = a.local.as_ref().unwrap().explorer.current().folder.clone();
    let record_bytes = records(&folder);
    let id = id(a, name);
    aim_file(a, name);
    let host = a.themes[0].host.clone();
    let p = host.file_placements[&id].position.clone();
    let target = hair::interaction::resolve(&host, &a.pose, &a.world).unwrap();
    let static_file = a.themes[0].file_item_shape(&id, std::ffi::OsStr::new(name));
    let base = base_mesh(a, name);
    let target_anchor = host.target_anchor(host.anchors.iter().find(|an| an.id == id).unwrap());
    let solid = hair::player::blocked_at(&host, p.at, 0.);
    let model = a.themes[0].file_item_model(&id, std::ffi::OsStr::new(name));
    let transform = bytes(&p.transform(&model).unwrap());
    let bounds = p.bounds().unwrap();
    let file_bounds = p.file_bounds().unwrap();
    // Identical static support simulation before and after render samples.
    let land = |host: &hair::host_scene::HostScene| {
        let mut player = hair::player::Player::default();
        let mut pose = Pose {
            position: p.at,
            feet_y: 3.,
            ..Default::default()
        };
        let input = hair::Input {
            captured: true,
            ..Default::default()
        };
        for _ in 0..240 {
            player.advance(host, &mut pose, &input, 1. / 120.);
        }
        (pose.feet_y, player.grounded)
    };
    let support = land(&host);
    for seconds in [0., 1.5, 3., 4.5, 6., -3., 1e100, f64::NAN, f64::INFINITY] {
        let mesh = a.themes[0].file_item_shape_at(&id, std::ffi::OsStr::new(name), seconds);
        let offset = p.properties.file_offset(seconds);
        assert!((0. ..=0.024001).contains(&offset));
        for (v, stationary) in mesh.iter().zip(&static_file) {
            assert_eq!(v.position[0], stationary.position[0]);
            assert_eq!(v.position[2], stationary.position[2]);
            assert!((v.position[1] - stationary.position[1] - offset).abs() < 1e-6);
            assert_eq!(v.uv, stationary.uv);
            assert_eq!(v.texture, stationary.texture);
        }
        let _scene = a.themes[0].vertices_for_at(&a.world, seconds);
        assert_eq!(a.themes[0].host, host);
        assert_eq!(
            hair::interaction::resolve(&a.themes[0].host, &a.pose, &a.world),
            Some(target.clone())
        );
        assert_eq!(
            a.themes[0]
                .host
                .target_anchor(host.anchors.iter().find(|an| an.id == id).unwrap()),
            target_anchor
        );
        assert_eq!(hair::player::blocked_at(&a.themes[0].host, p.at, 0.), solid);
        assert_eq!(land(&a.themes[0].host), support);
        assert_eq!(base_mesh(a, name), base);
        assert_eq!(p.bounds().unwrap(), bounds);
        assert_eq!(p.file_bounds().unwrap(), file_bounds);
        assert_eq!(bytes(&p.transform(&model).unwrap()), transform);
    }
    if p.properties.animated {
        assert_ne!(
            bytes(&a.themes[0].file_item_shape_at(&id, std::ffi::OsStr::new(name), 3.)),
            bytes(&static_file)
        );
    }
    if !p.properties.base || !p.properties.base_collision {
        assert!(!solid, "files are non-solid regardless of motion or label");
        assert!(support.0.abs() < 0.001, "file never supports the player");
    }
    // Labels are a separate scene suffix, not source-model or target state.
    let scene = a.themes[0].vertices_for_at(&a.world, 3.);
    a.themes[0]
        .host
        .file_placements
        .get_mut(&id)
        .unwrap()
        .position
        .properties
        .label = !p.properties.label;
    let flipped = a.themes[0].vertices_for_at(&a.world, 3.);
    assert_ne!(
        scene.len(),
        flipped.len(),
        "K must remove real floating label geometry"
    );
    assert_eq!(
        bytes(&a.themes[0].file_item_shape(&id, std::ffi::OsStr::new(name))),
        bytes(&static_file)
    );
    assert_eq!(
        hair::interaction::resolve(&a.themes[0].host, &a.pose, &a.world),
        Some(target.clone())
    );
    assert!(target.prompt.contains(name));
    a.themes[0].host = host;
    assert_eq!(
        records(&folder),
        record_bytes,
        "render sampling never rewrites metadata"
    );
}

#[test]
fn slice3_context_toolbox_switches_are_independent_cancelable_and_review_frozen() {
    use place_mode_native::Control;
    let (f, mut a) = open_fixture("slice3-toolbox-properties");
    tap(&mut a, KeyCode::KeyP);
    assert!(
        !a.toolbox_controls()
            .iter()
            .any(|(c, _, _)| matches!(c, Control::Animation | Control::Label))
    );
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]);
    let contact = a.place_contact().unwrap();
    let stationary = bytes(&a.place_preview().unwrap());
    click(&mut a, Control::Animation);
    assert!(
        a.toolbox_controls()
            .iter()
            .any(|(c, _, s)| *c == Control::Animation && s.ends_with("ON"))
    );
    assert!(
        a.toolbox_controls()
            .iter()
            .any(|(c, _, s)| *c == Control::Label && s.ends_with("ON"))
    );
    // A held/repeated J cannot undo the fresh mouse choice.
    a.key(KeyCode::KeyJ, true, true).unwrap();
    a.key(KeyCode::KeyJ, false, false).unwrap();
    click(&mut a, Control::Label);
    assert_eq!(bytes(&a.place_preview().unwrap()), stationary);
    assert_ne!(bytes(&a.place_preview_at(3.).unwrap()), stationary);
    assert_eq!(a.place_contact().unwrap(), contact);
    tap(&mut a, KeyCode::Enter);
    let frozen = edit_properties(&a);
    assert!(frozen.animated && !frozen.label && !frozen.base && frozen.base_collision);
    assert!(
        !a.toolbox_controls()
            .iter()
            .any(|(c, _, _)| matches!(c, Control::Animation | Control::Label))
    );
    tap(&mut a, KeyCode::KeyJ);
    tap(&mut a, KeyCode::KeyK);
    assert_eq!(edit_properties(&a), frozen);
    assert!(!f.0.join(".hair-layout").exists());
    tap(&mut a, KeyCode::Escape);
    assert!(!f.0.join(".hair-layout").exists());
    assert_eq!(a.editor_phase(), "Editing");
    tap(&mut a, KeyCode::Escape);
    // Every switch combination, through actual keyboard actions and fresh review.
    for (animated, label) in [(false, true), (true, true), (false, false), (true, false)] {
        aim_file(&mut a, "A.txt");
        tap(&mut a, KeyCode::KeyX);
        begin_positioning(&mut a);
        aim_floor(&mut a, [2., 0.]);
        if animated {
            tap(&mut a, KeyCode::KeyJ);
        }
        if !label {
            tap(&mut a, KeyCode::KeyK);
        }
        tap(&mut a, KeyCode::Enter);
        let props = edit_properties(&a);
        assert_eq!((props.animated, props.label), (animated, label));
        tap(&mut a, KeyCode::Escape);
        assert_eq!(a.editor_phase(), "Editing");
        tap(&mut a, KeyCode::Escape);
    }
}

#[test]
fn slice3_animated_base_reopen_collision_independence_and_motion_off() {
    use place_mode_native::Control;
    let (f, mut a) = open_fixture("slice3-animated-base");
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]);
    click(&mut a, Control::Base);
    click(&mut a, Control::Animation);
    click(&mut a, Control::Label);
    let static_preview = a.place_preview().unwrap();
    let animated_preview = a.place_preview_at(3.).unwrap();
    let file_len = a.themes[0]
        .file_item_model(&id(&a, "A.txt"), std::ffi::OsStr::new("A.txt"))
        .len();
    assert!(static_preview.len() > file_len);
    assert_eq!(
        bytes(&static_preview[file_len..]),
        bytes(&animated_preview[file_len..])
    );
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Enter);
    assert!(
        a.local_status.contains("reposition saved"),
        "{}",
        a.local_status
    );
    assert_presentation_only(&mut a, "A.txt");
    let saved = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
    let mut reopened = app();
    reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(
        hair::placement_store::read_layout(&reopened.local.as_ref().unwrap().explorer).unwrap(),
        saved
    );
    assert_presentation_only(&mut reopened, "A.txt");
    aim_file(&mut reopened, "A.txt");
    tap(&mut reopened, KeyCode::KeyP);
    tap(&mut reopened, KeyCode::KeyX);
    begin_positioning(&mut reopened);
    aim_floor(&mut reopened, [2., 0.]);
    click(&mut reopened, Control::Collision);
    click(&mut reopened, Control::Animation);
    // K remains OFF when animation or collision changes.
    tap(&mut reopened, KeyCode::Enter);
    let props = edit_properties(&reopened);
    assert!(props.base && !props.base_collision && !props.animated && !props.label);
    tap(&mut reopened, KeyCode::Enter);
    assert!(
        reopened.local_status.contains("reposition saved"),
        "{}",
        reopened.local_status
    );
    let id = id(&reopened, "A.txt");
    assert_eq!(
        bytes(&reopened.themes[0].file_item_shape_at(&id, "A.txt".as_ref(), 0.)),
        bytes(&reopened.themes[0].file_item_shape_at(&id, "A.txt".as_ref(), 3.))
    );
    assert_presentation_only(&mut reopened, "A.txt");
}

#[test]
fn slice3_inventory_copy_move_presentation_persists_with_and_without_base() {
    for move_item in [false, true] {
        for base in [false, true] {
            let source = fixture("slice3-presentation-source");
            let dest = fixture("slice3-presentation-dest");
            std::fs::write(source.0.join("Placed.txt"), b"immutable original").unwrap();
            let ino = std::fs::metadata(source.0.join("Placed.txt"))
                .unwrap()
                .ino();
            let mut a = app();
            let explorer = hair::local_explorer::Explorer::new(&source.0).unwrap();
            a.inventory.collect(&explorer, 0).unwrap();
            a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
            aim_floor(&mut a, [2., 0.]);
            tap(&mut a, KeyCode::KeyB);
            tap(&mut a, KeyCode::KeyP);
            begin_positioning(&mut a);
            if base {
                tap(&mut a, KeyCode::KeyG);
            }
            tap(&mut a, KeyCode::KeyJ);
            tap(&mut a, KeyCode::KeyK);
            let stationary = bytes(&a.placement_preview().unwrap());
            assert_ne!(stationary, bytes(&a.placement_preview_at(3.).unwrap()));
            assert_eq!(stationary, bytes(&a.placement_preview().unwrap()));
            if move_item {
                tap(&mut a, KeyCode::KeyV);
            }
            tap(&mut a, KeyCode::Enter);
            let frozen = a
                .placement_action
                .as_ref()
                .unwrap()
                .pending
                .clone()
                .unwrap();
            tap(&mut a, KeyCode::KeyJ);
            tap(&mut a, KeyCode::KeyK);
            assert_eq!(
                a.placement_action.as_ref().unwrap().pending.as_ref(),
                Some(&frozen)
            );
            assert!(!dest.0.join("Placed.txt").exists());
            tap(&mut a, KeyCode::Enter);
            assert!(a.local_status.starts_with("Placed "), "{}", a.local_status);
            assert_eq!(a.inventory.items().len(), usize::from(!move_item));
            let saved =
                hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
            assert_eq!(saved.positions[0].1, frozen);
            assert_eq!(
                (
                    frozen.properties.animated,
                    frozen.properties.label,
                    frozen.properties.base
                ),
                (true, false, base)
            );
            assert_presentation_only(&mut a, "Placed.txt");
            let mut reopened = app();
            reopened.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
            assert_eq!(
                hair::placement_store::read_layout(&reopened.local.as_ref().unwrap().explorer)
                    .unwrap(),
                saved
            );
            assert_presentation_only(&mut reopened, "Placed.txt");
            assert_eq!(
                std::fs::read(source.0.join("Placed.txt")).unwrap(),
                b"immutable original"
            );
            assert_eq!(
                std::fs::metadata(source.0.join("Placed.txt"))
                    .unwrap()
                    .ino(),
                ino
            );
            assert_eq!(
                std::fs::read(dest.0.join("Placed.txt")).unwrap(),
                b"immutable original"
            );
        }
    }
}

#[test]
fn slice3_presentation_schema_defaults_remain_strict_and_bounded() {
    use hair::placement_store::Position;
    let old = "at=[2.0,0.0]\nyaw=0.0\nsize=[0.8,0.85,0.28]\n";
    let p: Position = toml::from_str(old).unwrap();
    assert!(!p.properties.animated && p.properties.label);
    assert!(!toml::to_string(&p).unwrap().contains("properties"));
    let base_only: Position = toml::from_str(&format!("{old}[properties]\nbase=true\n")).unwrap();
    assert!(
        !base_only.properties.animated
            && base_only.properties.label
            && base_only.properties.base_collision
    );
    for bad in [
        "animated=1",
        "label='false'",
        "file_collision=true",
        "future=true",
    ] {
        assert!(
            toml::from_str::<Position>(&format!("{old}[properties]\n{bad}\n")).is_err(),
            "{bad}"
        );
    }
    assert!(toml::from_str::<Position>(&format!("{old}future=true\n")).is_err());
    let mut p = p;
    p.properties.animated = true;
    p.properties.label = false;
    let text = toml::to_string(&p).unwrap();
    assert!(text.len() < 4096);
    assert_eq!(toml::from_str::<Position>(&text).unwrap(), p);
    for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(p.properties.file_offset(time), 0.);
    }
}

#[test]
fn slice3_presentation_edits_preserve_busy_rebound_and_malformed_refusals() {
    use std::os::fd::AsRawFd;
    for refusal in ["busy", "rebound", "malformed"] {
        let (f, mut a) = open_fixture("slice3-presentation-refusal");
        let original_ino = std::fs::metadata(f.0.join("A.txt")).unwrap().ino();
        aim_file(&mut a, "A.txt");
        tap(&mut a, KeyCode::KeyP);
        tap(&mut a, KeyCode::KeyX);
        begin_positioning(&mut a);
        aim_floor(&mut a, [2., 0.]);
        tap(&mut a, KeyCode::Enter);
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("reposition saved"));
        aim_file(&mut a, "A.txt");
        tap(&mut a, KeyCode::KeyX);
        begin_positioning(&mut a);
        aim_floor(&mut a, [2., 0.]);
        tap(&mut a, KeyCode::KeyJ);
        tap(&mut a, KeyCode::KeyK);
        tap(&mut a, KeyCode::Enter);
        let mut lock = None;
        match refusal {
            "busy" => {
                let file = std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap();
                assert_eq!(
                    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
                    0
                );
                lock = Some(file);
            }
            "rebound" => {
                std::fs::rename(f.0.join("A.txt"), f.0.join("held-original")).unwrap();
                std::fs::write(f.0.join("A.txt"), b"replacement bytes").unwrap();
            }
            "malformed" => {
                // Deliberately corrupt only our owned fixture after frozen review.
                let path = f.0.join(&records(&f.0)[0].0);
                let mut text = std::fs::read_to_string(&path).unwrap();
                text.push_str("\n[unknown_presentation]\nfuture=true\n");
                std::fs::write(path, text).unwrap();
            }
            _ => unreachable!(),
        }
        let before = records(&f.0);
        tap(&mut a, KeyCode::Enter);
        assert!(
            !a.local_status.contains("reposition saved"),
            "{}: {}",
            refusal,
            a.local_status
        );
        assert_eq!(
            records(&f.0),
            before,
            "refusal preserves every prior record/inode"
        );
        let original = if refusal == "rebound" {
            "held-original"
        } else {
            "A.txt"
        };
        assert_eq!(std::fs::read(f.0.join(original)).unwrap(), b"A.txt");
        assert_eq!(
            std::fs::metadata(f.0.join(original)).unwrap().ino(),
            original_ino
        );
        if refusal == "rebound" {
            assert_eq!(
                std::fs::read(f.0.join("A.txt")).unwrap(),
                b"replacement bytes"
            );
        }
        drop(lock);
    }
}

#[test]
fn slice3_file_animation_and_hidden_label_draw_through_real_metal_renderer() {
    let (f, mut a) = open_fixture("slice3-render-presentation");
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]);
    tap(&mut a, KeyCode::KeyJ);
    tap(&mut a, KeyCode::KeyK);
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::KeyP);
    aim_file(&mut a, "A.txt");
    // Explicit outline follows the presented file, never its base/aim bounds.
    let target = hair::interaction::resolve(&a.themes[0].host, &a.pose, &a.world).unwrap();
    a.world.interaction = Some(target.clone());
    a.world.explicit_target = Some(target);
    let host = a.themes[0].host.clone();
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let mut renderer = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [640, 480],
        &a.themes[0].vertices_for_at(&a.world, 0.),
    ))
    .unwrap();
    renderer.set_theme_appearance(&a.themes[0]).unwrap();
    let mut images = vec![];
    for (index, seconds) in [0., 3., 6.].into_iter().enumerate() {
        renderer.set_scene(&a.themes[0].vertices_for_at(&a.world, seconds));
        let path = f.0.join(format!("frame-{index}"));
        std::fs::create_dir(&path).unwrap();
        let output = hair::resources::smoke_output(&a.root, &path).unwrap();
        renderer
            .capture(&output, "office.png", &a.pose, &a.world)
            .unwrap();
        renderer.wait().unwrap();
        let mut decoder = png::Decoder::new(std::io::BufReader::new(
            std::fs::File::open(path.join("office.png")).unwrap(),
        ))
        .read_info()
        .unwrap();
        let mut pixels = vec![0; decoder.output_buffer_size().unwrap()];
        let info = decoder.next_frame(&mut pixels).unwrap();
        pixels.truncate(info.buffer_size());
        images.push(pixels);
    }
    assert!(
        images[0] != images[1],
        "render-time bob changes actual drawn pixels"
    );
    assert!(
        images[0] == images[2],
        "period returns to deterministic stationary geometry"
    );
    assert_eq!(a.themes[0].host, host);
    // Captures are owned temporary fixtures, not a delivered visual verdict.
}
