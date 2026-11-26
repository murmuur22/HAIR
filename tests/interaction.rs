use hair::{
    Pose, World,
    host_scene::{Geometry, HostScene},
    interaction::{Kind, resolve},
};
use std::path::Path;
fn world() -> World {
    World::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn read_pose() -> Pose {
    Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [0.0, 1.8],
        yaw: 0.0,
        pitch: -0.5,
    }
}

#[test]
fn aim_target_prompt_identity_and_bounds() {
    let host = HostScene::shared_default();
    let w = world();
    let pose = read_pose();
    let t = resolve(&host, &pose, &w).unwrap();
    assert_eq!(t.anchor, "main-document");
    assert_eq!(t.kind, Kind::Read);
    assert_eq!(t.prompt, "E - Read");
    assert_eq!(Some(t), resolve(&host, &pose, &w));
    for p in [
        Pose {
            position: [0.0, 3.0],
            ..pose
        },
        Pose {
            yaw: std::f32::consts::PI,
            ..pose
        },
        Pose { pitch: 1.5, ..pose },
        Pose {
            yaw: f32::NAN,
            ..pose
        },
    ] {
        assert!(
            resolve(&host, &p, &w).is_none(),
            "distant, behind, above and nonfinite must refuse {p:?}"
        );
    }
    let mut w = w;
    w.reading = true;
    assert!(resolve(&host, &pose, &w).is_none(), "reader has priority");
}
#[test]
fn solid_geometry_occludes_prompt_and_action_target() {
    let mut host = HostScene::shared_default();
    let pose = read_pose();
    assert!(resolve(&host, &pose, &world()).is_some());
    host.geometry.push(Geometry {
        at: [0.0, 0.0, 1.1],
        size: [2.0, 2.0, 0.15],
    });
    assert!(
        resolve(&host, &pose, &world()).is_none(),
        "must not interact through host solid wall"
    );
}
#[test]
fn rendered_room_shell_occludes_side_portal() {
    let mut host = HostScene::shared_default();
    let portal = host
        .anchors
        .iter_mut()
        .find(|a| a.role == "portal")
        .unwrap();
    portal.at = [4.1, 0.1, 0.0];
    let pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [3.0, 0.0],
        yaw: std::f32::consts::FRAC_PI_2,
        pitch: -0.2,
    };
    assert!(
        resolve(&host, &pose, &world()).is_none(),
        "rendered right wall must occlude side portal"
    );
}
#[test]
fn other_anchor_envelope_occludes_target_independently_of_skin() {
    let mut host = HostScene::shared_default();
    let a = host
        .anchors
        .iter_mut()
        .find(|a| a.role == "decoration")
        .unwrap();
    a.at = [0.0, 0.1, 1.1];
    a.size = [0.5, 2.0, 0.3];
    assert!(
        resolve(&host, &read_pose(), &world()).is_none(),
        "host object envelope must block sight"
    );
}
