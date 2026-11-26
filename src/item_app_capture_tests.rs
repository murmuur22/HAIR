//! Bounded App-to-Metal integration probe; synthetic owned input and elapsed time.
use super::*;
fn tap(a: &mut App, k: KeyCode) {
    for down in [true, false] {
        assert!(!a.keyboard_input(Some(k), down, false, None).unwrap());
    }
}
#[test]
#[ignore = "serialized actual Metal with owned files; explicit absent HAIR_ITEM_APP_CAPTURE_ROOT"]
fn acquired_file_item_world_collection_and_inventory_metal_captures() {
    let output = PathBuf::from(
        std::env::var_os("HAIR_ITEM_APP_CAPTURE_ROOT").expect("explicit absent owned output"),
    );
    assert!(output.starts_with(std::env::temp_dir()));
    std::fs::create_dir(&output).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let image = include_bytes!("../examples/site-a/swatch.png");
    for (label, name, bytes) in [
        (
            "paper",
            "Notes.pdf",
            b"Synthetic paper class fixture, not a valid PDF".as_slice(),
        ),
        (
            "cassette",
            "Track.wav",
            b"Synthetic audio class fixture, not a valid WAV".as_slice(),
        ),
        ("image", "Swatch.png", image.as_slice()),
    ] {
        let directory = output.join(label);
        std::fs::create_dir(&directory).unwrap();
        let room = directory.join("Owned Room");
        std::fs::create_dir(&room).unwrap();
        std::fs::write(room.join(name), bytes).unwrap();
        let theme =
            hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default())
                .unwrap();
        let mut a = App::new(
            root.clone(),
            vec![theme],
            0,
            World::load(&root).unwrap(),
            false,
        );
        a.focused = true;
        a.complete_folder(Ok(Some(room.clone()))).unwrap();
        let anchor = a.themes[0]
            .host
            .anchors
            .iter()
            .find(|a| a.id == "entry-0")
            .unwrap()
            .clone();
        let inward = a.themes[0].host.portal_inward(&anchor.id);
        a.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [
                anchor.at[0] + 1.05 * inward[0],
                anchor.at[2] + 1.05 * inward[1],
            ],
            yaw: (-inward[0]).atan2(inward[1]),
            pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
        };
        let mut renderer = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [960, 640],
            &a.themes[0].vertices_for(&a.world),
        ))
        .unwrap();
        renderer.set_theme_appearance(&a.themes[0]).unwrap();
        a.renderer = Some(renderer);
        a.input.captured = true;
        hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
        assert_eq!(a.world.interaction.as_ref().unwrap().anchor, "entry-0");
        if label == "image" {
            let deadline = Instant::now() + Duration::from_secs(5);
            while a.object_previews.cache.is_empty() {
                a.poll_local_image().unwrap();
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(2));
            }
            a.renderer
                .as_mut()
                .unwrap()
                .set_scene(&a.themes[0].vertices_for(&a.world));
        }
        fn capture(a: &mut App, dir: &Path, label: &str) {
            let dest = dir.join(label);
            std::fs::create_dir(&dest).unwrap();
            a.update_hud();
            let target = hair::resources::smoke_output(&a.root, &dest).unwrap();
            a.renderer
                .as_ref()
                .unwrap()
                .capture(&target, "office.png", &a.pose, &a.world)
                .unwrap();
            a.renderer.as_ref().unwrap().wait().unwrap();
            if label.starts_with("placement-") {
                let color: [f32; 3] =
                    if label == "placement-clear" || label.starts_with("placement-overlap") {
                        [0.1, 0.9, 0.5]
                    } else {
                        [1.0, 0.2, 0.15]
                    };
                let expected =
                    color.map(|v| ((1.055 * v.powf(1.0 / 2.4) - 0.055) * 255.0).round() as u8);
                let mut reader = png::Decoder::new(std::io::BufReader::new(
                    std::fs::File::open(dest.join("office.png")).unwrap(),
                ))
                .read_info()
                .unwrap();
                let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
                let info = reader.next_frame(&mut pixels).unwrap();
                let count = pixels[..info.buffer_size()]
                    .chunks_exact(info.color_type.samples())
                    .filter(|p| p[..3].iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2))
                    .count();
                assert!(
                    count > 100,
                    "actual GPU output must contain visible valid/blocked hologram pixels, not just HUD"
                );
            }
        }
        capture(&mut a, &directory, "world");
        tap(&mut a, KeyCode::KeyX);
        assert_eq!(a.inventory.items().len(), 1);
        let (start, effect) = a
            .collection_feedback
            .as_mut()
            .expect("actual successful collection effect");
        *start = Instant::now() - Duration::from_millis(150);
        let preview = a.inventory.preview(a.inventory.items()[0].id).unwrap();
        assert!(
            effect
                .textures
                .iter()
                .zip(&preview.model.textures)
                .all(|(a, b)| Arc::ptr_eq(a, b))
        );
        capture(&mut a, &directory, "collection-150ms");
        tap(&mut a, KeyCode::KeyB);
        assert!(a.inventory_open && !a.input.captured);
        capture(&mut a, &directory, "inventory");
        tap(&mut a, KeyCode::ArrowRight);
        capture(&mut a, &directory, "inventory-rotated");
        tap(&mut a, KeyCode::KeyP);
        assert!(a.placement_action.is_some() && !a.inventory_open);
        a.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [0.0, 2.0],
            yaw: 0.0,
            pitch: -0.8,
        };
        let clear = a.placement_preview().unwrap();
        assert!(!clear.is_empty());
        capture(&mut a, &directory, "placement-clear");
        assert!(a.local_status.contains("Surface clear"));
        assert!(
            a.renderer.as_ref().unwrap().directory_map.is_none(),
            "placement owns the screen; inactive map controls must not be advertised"
        );
        a.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [anchor.at[0], anchor.at[2] + 1.65],
            yaw: 0.0,
            pitch: -std::f32::consts::FRAC_PI_4,
        };
        capture(&mut a, &directory, "placement-blocked");
        assert!(a.local_status.contains("overlaps"));
        let before = a.placement_preview().unwrap();
        tap(&mut a, KeyCode::KeyC);
        capture(&mut a, &directory, "placement-overlap");
        assert!(a.local_status.contains("Overlaps ON") && a.local_status.contains("Surface clear"));
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&before),
            bytemuck::cast_slice::<_, u8>(&a.placement_preview().unwrap()),
            "floor transform unaffected by overlap toggle"
        );
        a.renderer.as_mut().unwrap().resize([640, 480]);
        a.ui_scale = 200;
        capture(&mut a, &directory, "placement-overlap-small");
        a.pose.pitch = 0.2;
        assert!(a.placement_preview().is_err());
        capture(&mut a, &directory, "no-floor-owned-controls");
        assert!(a.local_status.contains("Look down"));
        assert!(!a.renderer.as_ref().unwrap().placement_overlay.is_empty());
        tap(&mut a, KeyCode::Escape);
        assert!(a.placement_action.is_none());
        assert_eq!(a.inventory.items().len(), 1);
        assert_eq!(std::fs::read(room.join(name)).unwrap(), bytes);
        assert_eq!(
            std::fs::read_dir(&room).unwrap().count(),
            1,
            "ordinary visit and collection cannot create metadata"
        );
        drop(a);
    }
}
