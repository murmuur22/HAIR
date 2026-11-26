use super::*;
#[path = "player_render_tests.rs"]
mod gpu;
#[path = "player_r7_app_tests.rs"]
mod r7_tests;
#[path = "stanchions_r8_app_tests.rs"]
mod r8_tests;
#[path = "player_r9_app_tests.rs"]
mod r9_tests;

/// Shared test-only diagnostic readback; never a production smoke certificate.
pub(super) fn diagnostic_capture(a: &mut App, root: &Path, name: &str) {
    r8_tests::capture(a, root, name);
}

fn app() -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    )
}
fn key(a: &mut App, down: bool, repeat: bool) {
    assert!(
        !a.keyboard_input(Some(KeyCode::Space), down, repeat, None)
            .unwrap()
    );
}
fn tap(a: &mut App, code: KeyCode) {
    a.keyboard_input(Some(code), true, false, None).unwrap();
    a.keyboard_input(Some(code), false, false, None).unwrap();
}
fn frames(a: &mut App, count: usize) {
    for _ in 0..count {
        a.advance_motion(1.0 / 120.0);
    }
}
fn empty(a: &mut App) {
    let h = &mut a.themes[0].host;
    h.anchors.clear();
    h.geometry.clear();
    h.overhead.clear();
    h.file_placements.clear();
    h.open_platform = true;
    a.pose = Pose {
        position: [0.0, 0.0],
        pitch: 0.0,
        ..Default::default()
    };
    a.input.captured = true;
}

#[test]
fn space_changes_actual_app_vertical_frame_then_lands() {
    let mut a = app();
    a.input.captured = true;
    let start = format!("{:?}", a.pose);
    let position = a.pose.position;
    let host = a.themes[0].host.clone();
    key(&mut a, true, false);
    a.advance_motion(0.1);
    assert_ne!(
        format!("{:?}", a.pose),
        start,
        "actual App Space/update must raise the camera frame, not only a detached Player unit"
    );
    assert!(a.pose.feet_y > 0.5 && a.player.velocity > 0.0);
    assert_eq!(a.pose.position, position);
    frames(&mut a, 240);
    assert_eq!(
        format!("{:?}", a.pose),
        start,
        "held Space cannot auto-bounce"
    );
    for repeat in [true, false] {
        key(&mut a, true, repeat);
        a.advance_motion(0.1);
        assert_eq!(a.pose.feet_y, 0.0);
    }
    key(&mut a, false, false);
    key(&mut a, true, false);
    a.advance_motion(0.1);
    assert!(a.pose.feet_y > 0.5, "physical release rearms Space");
    assert_eq!(
        a.themes[0].host, host,
        "player motion cannot mutate placements or room"
    );
}

#[test]
fn actual_app_jumps_over_side_clearance_stands_and_walks_off_furniture() {
    let mut a = app();
    let mut desk = a.themes[0].host.anchor("document").clone();
    desk.at = [0.0, 0.0, 0.0];
    desk.size = [1.6, 0.8, 1.6];
    empty(&mut a);
    a.themes[0].host.anchors.push(desk);
    a.pose.position = [0.0, 1.1];
    a.keyboard_input(Some(KeyCode::KeyW), true, false, None)
        .unwrap();
    a.advance_motion(0.25);
    assert!(
        a.pose.position[1] > 1.02,
        "grounded player cannot walk through stationary desk"
    );
    key(&mut a, true, false);
    frames(&mut a, 45);
    a.keyboard_input(Some(KeyCode::KeyW), false, false, None)
        .unwrap();
    assert!(
        a.pose.position[1] < 0.8 && a.pose.feet_y > 0.8,
        "height-aware side collision permits clearing the desktop"
    );
    frames(&mut a, 180);
    assert!((a.pose.feet_y - 0.8).abs() < 0.001 && a.player.grounded);
    assert!((a.pose.eye_height() - 2.45).abs() < 0.001);
    a.keyboard_input(Some(KeyCode::KeyD), true, false, None)
        .unwrap();
    frames(&mut a, 60);
    assert!(
        !a.player.grounded && a.pose.feet_y < 0.8,
        "walking off a support must fall"
    );
    a.keyboard_input(Some(KeyCode::KeyD), false, false, None)
        .unwrap();
    frames(&mut a, 180);
    assert_eq!(a.pose.feet_y, 0.0);
    assert!(a.player.grounded);
}

#[test]
fn actual_app_ceiling_overhead_and_airborne_platform_edges_block() {
    for overhead in [false, true] {
        let mut a = app();
        empty(&mut a);
        if overhead {
            a.themes[0].host.overhead.push(hair::host_scene::Geometry {
                at: [0.0, 2.2, 0.0],
                size: [2.0, 0.3, 2.0],
            });
        } else {
            a.themes[0].host.open_platform = false;
            a.themes[0].host.height = 2.2;
        }
        key(&mut a, true, false);
        let mut peak = 0.0_f32;
        for _ in 0..240 {
            frames(&mut a, 1);
            peak = peak.max(a.pose.feet_y);
        }
        assert!(
            peak > 0.2 && peak <= 0.4001,
            "body hits ceiling underside: {peak}"
        );
        assert!(a.player.grounded && a.pose.feet_y == 0.0);
    }
    for (code, axis, bound) in [
        (KeyCode::KeyD, 0, 2),
        (KeyCode::KeyA, 0, 0),
        (KeyCode::KeyS, 1, 3),
        (KeyCode::KeyW, 1, 1),
    ] {
        let mut a = app();
        empty(&mut a);
        let edge = a.themes[0].host.room[bound];
        a.pose.position[axis] = edge + if bound < 2 { 0.01 } else { -0.01 };
        key(&mut a, true, false);
        a.keyboard_input(Some(code), true, false, None).unwrap();
        a.advance_motion(0.25);
        assert!(a.pose.feet_y > 0.5, "edge check must really be airborne");
        let h = &a.themes[0].host;
        assert!(a.pose.position[axis] >= h.room[axis] && a.pose.position[axis] <= h.room[axis + 2]);
    }
}

#[test]
fn actual_app_recovers_same_room_once_with_occupied_spawn_and_clears_velocity() {
    for bad in [f32::NAN, -8.0, 99.0] {
        let mut a = app();
        let mut desk = a.themes[0].host.anchor("document").clone();
        empty(&mut a);
        desk.at = [0.0, 0.0, 0.0];
        desk.size = [1.6, 0.8, 1.6];
        a.themes[0].host.spawn = [0.0, 0.0];
        a.themes[0].host.anchors.push(desk);
        let host = a.themes[0].host.clone();
        let page = a.world.current.clone();
        a.pose.position = [999.0, 999.0];
        a.pose.feet_y = bad;
        a.player.velocity = -100.0;
        a.advance_motion(0.1);
        assert_eq!(a.pose.feet_y, 0.0);
        assert_eq!(a.player.velocity, 0.0);
        assert!(!hair::player::blocked_at(
            &host,
            a.pose.position,
            a.pose.feet_y
        ));
        assert_ne!(
            a.pose.position, host.spawn,
            "cannot recover inside stationary furniture"
        );
        let recovered = format!("{:?}", a.pose);
        frames(&mut a, 120);
        assert_eq!(format!("{:?}", a.pose), recovered, "recovery must not loop");
        assert_eq!(a.world.current, page);
        assert_eq!(a.themes[0].host, host);
        key(&mut a, true, false);
        frames(&mut a, 12);
        assert!(
            a.pose.feet_y > 0.1,
            "recovery must leave a playable, rearmed player"
        );
    }
}

#[test]
fn actual_app_files_are_neither_side_colliders_nor_support() {
    for allow_overlap in [false, true] {
        let mut a = app();
        empty(&mut a);
        a.themes[0].host.file_placements.insert(
            "owned".into(),
            hair::host_scene::PlacedItem {
                position: hair::placement_store::Position {
                    properties: Default::default(),
                    at: [0.0, 0.0],
                    yaw: 0.0,
                    size: [1.0; 3],
                    allow_overlap,
                    surface: None,
                },
                bounds: hair::host_scene::Geometry {
                    at: [0.0; 3],
                    size: [1.0; 3],
                },
            },
        );
        let before = a.themes[0].host.clone();
        a.pose.position = [0.0, 1.0];
        a.keyboard_input(Some(KeyCode::KeyW), true, false, None)
            .unwrap();
        frames(&mut a, 48);
        assert!(
            a.pose.position[1].abs() < 0.01,
            "player can walk through a saved file"
        );
        a.keyboard_input(Some(KeyCode::KeyW), false, false, None)
            .unwrap();
        key(&mut a, true, false);
        frames(&mut a, 240);
        assert_eq!(a.pose.feet_y, 0.0, "file top is not a support surface");
        assert_eq!(a.themes[0].host, before);
    }
}

#[test]
fn space_modal_ownership_focus_capture_and_reset_need_fresh_down() {
    for context in 0..7 {
        let mut a = app();
        match context {
            0 => tap(&mut a, KeyCode::KeyL),
            1 => tap(&mut a, KeyCode::KeyB),
            2 => tap(&mut a, KeyCode::Escape),
            3 => {
                a.world.reading = true;
                a.release_navigation();
            }
            4 => {
                a.map_mode = hair::exploration::MapMode::Expanded;
                a.release_navigation();
            }
            5 => {
                a.chooser_pending = true;
                a.release_navigation();
            }
            _ => a.focus_changed(false),
        }
        // Even stale pointer capture cannot transfer modal Space to the world.
        a.input.captured = true;
        key(&mut a, true, false);
        a.advance_motion(0.1);
        assert_eq!(a.pose.feet_y, 0.0, "context {context} owns Space");
        assert_eq!(a.player.velocity, 0.0);
        a.address = None;
        a.inventory_open = false;
        a.menu = None;
        a.world.reading = false;
        a.map_mode = hair::exploration::MapMode::Compact;
        a.chooser_pending = false;
        a.focus_changed(true);
        a.input.captured = true;
        key(&mut a, true, false);
        a.advance_motion(0.1);
        assert_eq!(
            a.pose.feet_y, 0.0,
            "context-held Space cannot become fresh on exit"
        );
        key(&mut a, false, false);
        key(&mut a, true, false);
        a.advance_motion(0.1);
        assert!(a.pose.feet_y > 0.5, "release rearms context {context}");
        a.focus_changed(false);
        assert_eq!(
            a.player.velocity, 0.0,
            "focus loss clears vertical velocity"
        );
        let height = a.pose.feet_y;
        a.advance_motion(0.25);
        assert_eq!(a.pose.feet_y, height);
    }
    let mut a = app();
    key(&mut a, true, false);
    a.advance_motion(0.1);
    assert_eq!(a.pose.feet_y, 0.0, "uncaptured Space is not a jump");
    a.input.captured = true;
    key(&mut a, true, true);
    a.advance_motion(0.1);
    assert_eq!(a.pose.feet_y, 0.0, "capture must not reinterpret a repeat");
    key(&mut a, false, false);
    key(&mut a, true, false);
    a.advance_motion(0.1);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(a.pose.feet_y, 0.0);
    assert_eq!(a.player.velocity, 0.0);
}

#[test]
fn elevated_camera_aim_floor_surface_and_collection_share_one_frame() {
    use hair::{
        assets::Vertex,
        placement::{floor_target, surface},
    };
    let mut a = app();
    empty(&mut a);
    key(&mut a, true, false);
    frames(&mut a, 24);
    assert!(a.pose.feet_y > 0.9);
    a.pose.pitch = -std::f32::consts::FRAC_PI_4;
    let expected_z = a.pose.position[1] - a.pose.eye_height();
    assert!((floor_target(&a.pose).unwrap()[1] - expected_z).abs() < 0.0001);
    let contact = surface::target(&a.pose, &[]).unwrap().unwrap();
    assert!((contact.at[2] - expected_z).abs() < 0.0001);
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
    let mut baseline = a.pose;
    baseline.feet_y = 0.0;
    let translated = vertices.map(|mut v| {
        v.position[1] -= a.pose.feet_y;
        v
    });
    let reference = hair::collection::Effect::new(&translated, &baseline, frame)
        .unwrap()
        .paint(0.0, false);
    assert_eq!(effect.len(), reference.len());
    for (a, b) in effect.iter().zip(&reference) {
        for axis in 0..3 {
            assert!((a.position[axis] - b.position[axis]).abs() < 0.00001);
        }
    }
    let hit = surface::target(&a.pose, &[&vertices]).unwrap().unwrap();
    assert!((hit.at[1] - a.pose.eye_height()).abs() < 0.0001);
    // A low lintel blocks ground aim, but is below the raised eye's real ray.
    let mut target = app().themes[0].host.anchor("document").clone();
    target.at = [0.0, a.pose.eye_height() - 0.2, -1.0];
    target.size = [0.6, 0.4, 0.4];
    a.themes[0].host.anchors.push(target);
    a.themes[0].host.geometry.push(hair::host_scene::Geometry {
        at: [0.0, 1.3, -0.5],
        size: [0.8, 0.5, 0.1],
    });
    assert!(
        hair::interaction::resolve(&a.themes[0].host, &a.pose, &a.world).is_some(),
        "raised eye ray clears low lintel"
    );
    a.pose.feet_y = 5.0;
    a.pose.pitch = -1.3;
    assert!(
        hair::interaction::resolve(&a.themes[0].host, &a.pose, &a.world).is_none(),
        "planar proximity alone is not airborne reach"
    );
}
