use hair::{
    Input, Pose, World,
    theme::{AssetCache, Theme},
};
use std::path::{Path, PathBuf};
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/GreenleafReadingRoom-hallway")
}
#[test]
fn bounded_overhead_lintel_renders_but_does_not_close_walkway() {
    let text = include_str!("../host-scene.toml");
    let lintel = "\n[[overhead]]\nat=[0,2.5,2]\nsize=[2,0.5,0.2]\n";
    let scene = hair::host_scene::HostScene::parse(&(text.to_owned() + lintel))
        .expect("explicit host overhead geometry with at least 2.2m clearance");
    assert!(!scene.blocked([0.0, 2.0]));
    let old = hair::host_scene::HostScene::parse(text).unwrap();
    assert_eq!(scene.vertices().len(), old.vertices().len() + 36);
    for bad in [
        lintel.replace("2.5", "2.19"),
        lintel.replace("0.5", "1.5"),
        lintel.replace("2,0.5,0.2", "0,0.5,0.2"),
    ] {
        assert!(hair::host_scene::HostScene::parse(&(text.to_owned() + &bad)).is_err());
    }
}
#[test]
fn authored_hallway_is_continuous_not_a_chain_of_portal_arrivals() {
    let w = World::load_site(&fixture()).expect("authored continuous Greenleaf fixture");
    let graph = w.spaces.as_ref().unwrap();
    assert_eq!(
        graph.spaces.len(),
        1,
        "one continuous scene, not three teleports"
    );
    let scene = &graph.space("greenleaf-suite").unwrap().scene;
    assert_eq!(w.current_space, "greenleaf-suite");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let theme = Theme::load_for_scene(
        &root.join("packs/office.toml"),
        &mut AssetCache::default(),
        scene,
    )
    .unwrap();
    let mut pose = Pose {
        position: scene.spawn,
        yaw: 0.0,
        pitch: 0.0,
    };
    let input = Input {
        captured: true,
        forward: true,
        ..Default::default()
    };
    // 4x4 entrance -> north doorway -> narrow hall -> east bend -> wide reading room.
    // Each segment exercises production sub-stepped locomotion. No destination call.
    for to in [[0.0, -2.0], [0.0, 2.0], [2.5, 2.0], [2.5, 4.5], [0.0, 4.5]] {
        walk(&theme, &mut pose, &input, to);
    }
    assert_eq!(
        w.current, "index.md",
        "walking does not fake document navigation"
    );
    assert_eq!(w.current_space, "greenleaf-suite");
    for to in [[2.5, 4.5], [2.5, 2.0], [0.0, 2.0], [0.0, -2.0], scene.spawn] {
        walk(&theme, &mut pose, &input, to);
    }
    for p in [[-2.0, 0.0], [2.0, 0.0], [0.0, 3.0], [-3.0, -5.0]] {
        assert!(
            scene.blocked(p),
            "authored concave corner/partition/entrance boundary {p:?}"
        );
    }
    for p in [[0.0, -3.0], [0.0, 0.0], [0.0, 2.0], [2.5, 3.0], [-3.0, 4.5]] {
        assert!(!scene.blocked(p), "door/hall/reading safe floor {p:?}");
    }
    assert_eq!(w.navigation.rows.len(), 3);
    assert_eq!(w.navigation.current_index("reading.md"), Some(2));
}
#[test]
fn overhead_occludes_real_target_rays_without_changing_legacy_boxes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut w = World::load(root).unwrap();
    let mut host = hair::host_scene::HostScene::shared_default();
    assert!(host.overhead.is_empty());
    assert!(!toml::to_string(&host).unwrap().contains("overhead"));
    let anchor = host
        .anchors
        .iter_mut()
        .find(|a| a.role == "document")
        .unwrap();
    anchor.at = [0.0, 2.5, 1.0];
    anchor.size = [0.2, 0.5, 0.2];
    let pose = Pose {
        position: [0.0, 2.5],
        yaw: 0.0,
        pitch: (2.75_f32 - 1.65).atan2(1.5),
    };
    assert!(hair::interaction::resolve(&host, &pose, &w).is_some());
    host.overhead.push(hair::host_scene::Geometry {
        at: [0.0, 2.2, 1.75],
        size: [2.0, 0.8, 0.2],
    });
    assert!(
        !host.blocked([0.0, 1.75]),
        "overhead clearance permits walking"
    );
    assert!(
        hair::interaction::resolve(&host, &pose, &w).is_none(),
        "real overhead remains an occluder"
    );
    host.geometry.push(host.overhead.pop().unwrap());
    assert!(
        host.blocked([0.0, 1.75]),
        "legacy geometry remains conservative even if high"
    );
    w.reading = true;
    assert!(hair::interaction::resolve(&host, &pose, &w).is_none());
}
#[test]
fn hallway_refusals_keep_host_limits_and_safe_arrivals() {
    let w = World::load_site(&fixture()).unwrap();
    let good = w.spaces.as_ref().unwrap();
    let site = w.site.as_ref().unwrap();
    for case in 0..9 {
        let mut g = good.clone();
        let scene = &mut g.spaces[0].scene;
        match case {
            0 => scene.spawn = [-2.0, 0.0],
            1 => scene.spawn = [5.0, 0.0],
            2 => scene.geometry.push(hair::host_scene::Geometry {
                at: [0.0, 0.0, 0.0],
                size: [2.0, 3.0, 0.2],
            }),
            3 => scene.overhead[0].at[1] = 2.19,
            4 => scene.overhead = vec![scene.overhead[0].clone(); 65],
            5 => scene.overhead[0].size[0] = f32::NAN,
            6 => {
                scene.anchors[0].at[1] = 2.2;
                scene.anchors[0].size[1] = 0.7;
                scene.overhead[0].at = [-1.3, 2.2, -4.9];
            }
            7 => g.spaces[0].arrivals[0].at = [2.0, 0.0],
            _ => g.spaces[0].arrivals[0].yaw_degrees = 0.0,
        }
        assert!(g.validate(site).is_err(), "invalid hallway case {case}");
    }
    let text = std::fs::read_to_string(fixture().join("host-scene.toml")).unwrap();
    for text in [
        text.replace("height=3.0", "height=3.0\nregions=[]"),
        text.replace(
            "size=[1.3, 0.5, 0.2]",
            "size=[1.3, 0.5, 0.2]\ncollision=false",
        ),
    ] {
        assert!(
            hair::spaces::Spaces::parse(&text, site).is_err(),
            "no unselected region/collision bypass schema"
        );
    }
}
#[test]
fn doorway_and_concave_corner_block_tunnelling_across_reskins() {
    let w = World::load_site(&fixture()).unwrap();
    let host = &w.spaces.as_ref().unwrap().spaces[0].scene;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut cache = AssetCache::default();
    let a = Theme::load_for_scene(&root.join("packs/office.toml"), &mut cache, host).unwrap();
    let b = Theme::load_for_scene(&root.join("packs/archive.toml"), &mut cache, host).unwrap();
    let input = Input {
        captured: true,
        forward: true,
        ..Default::default()
    };
    for (start, yaw, axis, max) in [
        ([0.8, -3.5], std::f32::consts::PI, 1, -3.31),
        ([0.0, 0.0], std::f32::consts::FRAC_PI_2, 0, 0.79),
        ([0.0, 2.0], std::f32::consts::PI, 1, 2.69),
    ] {
        let mut end = None;
        for theme in [&a, &b] {
            let mut pose = Pose {
                position: start,
                yaw,
                pitch: 0.0,
            };
            for _ in 0..20 {
                theme.locomote(&mut pose, &input, 0.25);
            }
            assert!(
                pose.position[axis] <= max,
                "crossed jamb/corner/partition {:?}",
                pose.position
            );
            assert!(!theme.blocked(pose.position));
            if let Some(old) = end {
                assert_eq!(old, pose.position);
            }
            end = Some(pose.position);
        }
    }
    for x in -32..=32 {
        for z in -56..=48 {
            let p = [x as f32 * 0.125, z as f32 * 0.125];
            assert_eq!(a.blocked(p), b.blocked(p));
        }
    }
}
fn walk(theme: &Theme, pose: &mut Pose, input: &Input, to: [f32; 2]) {
    for _ in 0..1000 {
        let dx = to[0] - pose.position[0];
        let dz = to[1] - pose.position[1];
        let distance = dx.hypot(dz);
        if distance < 0.015 {
            return;
        }
        pose.yaw = dx.atan2(-dz);
        let before = pose.position;
        theme.locomote(pose, input, (distance / 2.5).min(0.02));
        assert!(!theme.blocked(pose.position));
        assert_ne!(
            pose.position, before,
            "stuck on route to {to:?} at {before:?}"
        );
        assert!(
            (pose.position[0] - before[0]).hypot(pose.position[1] - before[1]) <= 0.051,
            "no teleport"
        );
    }
    panic!("route did not finish at {to:?}");
}
