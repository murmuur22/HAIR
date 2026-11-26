use super::*;

#[test]
fn r7_low_closed_rooms_walk_recover_and_keep_camera_below_ceiling() {
    for height in [1.0, 1.79, 1.8, 3.0] {
        let mut a = app();
        let mut host = hair::host_scene::HostScene::shared_default();
        host.height = height;
        for anchor in &mut host.anchors {
            anchor.at[1] = 0.0;
            anchor.size[1] = anchor.size[1].min(height);
        }
        let host = hair::host_scene::HostScene::parse(&toml::to_string(&host).unwrap()).unwrap();
        a.pose.position = host.spawn;
        a.themes[0].host = host.clone();
        a.input.captured = true;
        let start = a.pose.position;
        a.keyboard_input(Some(KeyCode::KeyA), true, false, None)
            .unwrap();
        a.advance_motion(0.1);
        assert!(
            (a.pose.position[0] - start[0] + 0.25).abs() < 0.001,
            "walk in {height}m room"
        );
        a.keyboard_input(Some(KeyCode::KeyA), false, false, None)
            .unwrap();
        key(&mut a, true, false);
        let mut peak = 0.0_f32;
        for _ in 0..240 {
            frames(&mut a, 1);
            peak = peak.max(a.pose.feet_y);
            assert!(a.pose.eye_height() < height, "camera inside {height}m room");
            assert!(
                a.pose.feet_y <= (height - 1.8).max(0.0) + 0.0001,
                "no ceiling tunneling"
            );
        }
        if height == 3.0 {
            assert!((a.pose.eye_height() - 1.65).abs() < 0.0001);
            assert!(peak > 1.0 && peak <= 1.2001);
        }
        a.pose.feet_y = f32::NAN;
        a.pose.position = [999.0; 2];
        a.player.velocity = -100.0;
        a.advance_motion(0.1);
        assert_eq!(a.pose.position, host.spawn);
        assert_eq!(a.pose.feet_y, 0.0);
        assert_eq!(a.player.velocity, 0.0);
        frames(&mut a, 120);
        assert_eq!(a.pose.position, host.spawn, "recovery must not loop");
        a.keyboard_input(Some(KeyCode::KeyA), true, false, None)
            .unwrap();
        a.advance_motion(0.1);
        assert!(a.pose.position[0] < host.spawn[0] - 0.24);
        assert_eq!(a.themes[0].host, host, "motion is read-only");
    }
}

#[test]
fn r7_low_room_eye_aim_floor_surface_and_collection_use_the_same_crouched_frame() {
    use hair::{
        assets::Vertex,
        placement::{floor_target, surface},
    };
    for height in [1.0, 1.79, 3.0] {
        let mut a = app();
        empty(&mut a);
        let host = &mut a.themes[0].host;
        host.open_platform = false;
        host.height = height;
        a.advance_motion(0.01);
        assert_eq!(
            hair::player::body_height(&a.themes[0].host),
            height.min(1.8)
        );
        assert_eq!(
            a.pose.eye_offset,
            if height < 1.8 { height - 0.15 } else { 1.65 }
        );
        a.pose.pitch = -std::f32::consts::FRAC_PI_4;
        let expected = a.pose.position[1] - a.pose.eye_height();
        assert!((floor_target(&a.pose).unwrap()[1] - expected).abs() < 0.0001);
        let contact = surface::target(&a.pose, &[]).unwrap().unwrap();
        assert!((contact.at[2] - expected).abs() < 0.0001);
        a.pose.pitch = 0.0;
        let vertices = [[-0.3, -0.3], [0.3, -0.3], [0.0, 0.3]].map(|[x, y]| Vertex {
            position: [x, a.pose.eye_height() + y, -1.0],
            color: [1.0; 3],
            uv: [0.0; 2],
            texture: 0,
        });
        let frame = hair::sidebar::Frame::new([960, 640], 100, false);
        let effect = hair::collection::Effect::new(&vertices, &a.pose, frame)
            .unwrap()
            .paint(0.0, false);
        let reference = hair::collection::Effect::new(
            &vertices.map(|mut v| {
                v.position[1] += 1.65 - a.pose.eye_height();
                v
            }),
            &Pose {
                position: a.pose.position,
                pitch: 0.0,
                ..Default::default()
            },
            frame,
        )
        .unwrap()
        .paint(0.0, false);
        assert_eq!(effect.len(), reference.len());
        for (x, y) in effect.iter().zip(reference.iter()) {
            for axis in 0..3 {
                assert!((x.position[axis] - y.position[axis]).abs() < 0.00001);
            }
        }
        let mut target = hair::host_scene::HostScene::shared_default()
            .anchor("document")
            .clone();
        target.at = [0.0, a.pose.eye_height() - 0.1, -1.0];
        target.size = [0.4, 0.2, 0.4];
        a.themes[0].host.anchors.push(target.clone());
        assert!(hair::interaction::resolve(&a.themes[0].host, &a.pose, &a.world).is_some());
    }
}

#[test]
fn r7_actual_app_placement_preview_walks_but_never_jumps_or_moves_in_confirm() {
    use std::os::unix::fs::MetadataExt;
    struct Owned(std::path::PathBuf);
    impl Drop for Owned {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let workspace =
        PathBuf::from(std::env::var_os("HAIR_TEST_WORKSPACE").expect("explicit owned workspace"));
    let temp = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit owned sibling TMPDIR"));
    assert!(workspace.is_absolute() && temp.is_absolute());
    let workspace = workspace.canonicalize().unwrap();
    let temp = temp.canonicalize().unwrap();
    assert_eq!(source_root.parent(), Some(workspace.as_path()));
    assert!(
        !temp.starts_with(&source_root) && !source_root.starts_with(&temp),
        "TMPDIR must not overlap source"
    );
    assert_eq!(temp.parent(), Some(workspace.as_path()));
    let fixture_path = temp.join(format!("r7-preview-{}", std::process::id()));
    std::fs::create_dir(&fixture_path).unwrap();
    let owned = Owned(fixture_path);
    let source = owned.0.join("source");
    let dest = owned.0.join("dest");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(source.join("File.txt"), b"r7 immutable inventory").unwrap();
    let original = std::fs::metadata(source.join("File.txt")).unwrap();
    let mut a = app();
    let id = a.inventory
        .collect(&hair::local_explorer::Explorer::new(&source).unwrap(), 0)
        .unwrap();
    a.complete_folder(Ok(Some(dest.clone()))).unwrap();
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    assert!(a.placement_action.is_some());
    assert_eq!(a.editor_phase(), "Editing");
    a.pose.position = [0.0; 2];
    let editing = a.pose.position;
    a.keyboard_input(Some(KeyCode::KeyW), true, false, None).unwrap();
    key(&mut a, true, false);
    a.advance_motion(0.1);
    assert!(!a.motion_owned(), "Editing isolates movement");
    assert_eq!(a.pose.position, editing);
    assert_eq!(a.pose.feet_y, 0.0);
    assert_eq!(a.player.velocity, 0.0);
    a.keyboard_input(Some(KeyCode::KeyW), false, false, None).unwrap();
    key(&mut a, false, false);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Positioning");
    assert!(a.input.captured, "real Enter acquires positioning look");
    for (code, axis, sign) in [
        (KeyCode::KeyW, 1, -1.0),
        (KeyCode::KeyS, 1, 1.0),
        (KeyCode::KeyA, 0, -1.0),
        (KeyCode::KeyD, 0, 1.0),
    ] {
        let start = a.pose.position;
        a.keyboard_input(Some(code), true, false, None).unwrap();
        key(&mut a, true, false);
        a.advance_motion(0.1);
        assert!(
            (a.pose.position[axis] - start[axis] - sign * 0.25).abs() < 0.001,
            "preview {code:?}"
        );
        assert_eq!(a.pose.feet_y, 0.0, "preview Space is not a jump");
        assert_eq!(a.player.velocity, 0.0);
        a.keyboard_input(Some(code), false, false, None).unwrap();
        key(&mut a, false, false);
    }
    a.pose.position = [2.0, 1.65];
    a.pose.pitch = -std::f32::consts::FRAC_PI_4;
    tap(&mut a, KeyCode::Enter);
    assert!(
        a.placement_action.as_ref().unwrap().pending.is_some(),
        "real Enter enters confirmation: {}",
        a.local_status
    );
    assert_eq!(a.editor_phase(), "Review");
    assert!(!a.input.captured);
    assert!(!a.motion_owned(), "Review freezes even before input is considered");
    let start = a.pose.position;
    a.keyboard_input(Some(KeyCode::KeyW), true, false, None)
        .unwrap();
    a.keyboard_input(Some(KeyCode::KeyD), true, false, None)
        .unwrap();
    key(&mut a, true, false);
    a.advance_motion(0.1);
    assert_eq!(a.pose.position, start);
    assert_eq!(a.pose.feet_y, 0.0);
    assert_eq!(a.player.velocity, 0.0);
    a.keyboard_input(Some(KeyCode::KeyW), false, false, None).unwrap();
    a.keyboard_input(Some(KeyCode::KeyD), false, false, None).unwrap();
    key(&mut a, false, false);
    // Retain the original stale-input defense as a supplemental negative:
    // even corrupt/stale capture and a latched forward bit cannot move Review.
    // The phase itself was reached only through real, released Enter events.
    a.input.captured = true;
    a.input.forward = true;
    a.keyboard_input(Some(KeyCode::KeyD), true, false, None).unwrap();
    key(&mut a, true, false);
    a.advance_motion(0.1);
    assert_eq!(a.pose.position, start);
    assert_eq!(a.pose.feet_y, 0.0);
    assert_eq!(a.player.velocity, 0.0);
    a.keyboard_input(Some(KeyCode::KeyD), false, false, None).unwrap();
    key(&mut a, false, false);
    tap(&mut a, KeyCode::Tab);
    assert!(!a.input.captured && !a.input.forward);
    a.keyboard_input(Some(KeyCode::Enter), true, true, None).unwrap();
    a.keyboard_input(Some(KeyCode::Enter), false, false, None).unwrap();
    assert_eq!(a.editor_phase(), "Review", "repeat cannot commit");
    assert_eq!(a.inventory.items().len(), 1);
    assert!(a.inventory.preview(id).is_some());
    assert!(!dest.join("File.txt").exists());
    assert!(!dest.join(".hair-layout").exists());
    assert_eq!(
        std::fs::read(source.join("File.txt")).unwrap(),
        b"r7 immutable inventory"
    );
    let after = std::fs::metadata(source.join("File.txt")).unwrap();
    assert_eq!((original.dev(), original.ino()), (after.dev(), after.ino()));
}
