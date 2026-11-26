use super::*;
use crate::{
    local_explorer::{Entry, Kind, Snapshot},
    placement_store::{LayoutRead, Position},
};

fn folder() -> (crate::World, HostScene, String) {
    let folder = std::path::PathBuf::from("/r9-read-only-snapshot");
    let (world, host) = crate::folder_scene::build(
        &Snapshot {
            folder: folder.clone(),
            entries: vec![Entry {
                name: "File.txt".into(),
                path: folder.join("File.txt"),
                kind: Kind::File,
            }],
        },
        0,
        false,
    )
    .unwrap();
    let id = world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .keys()
        .next()
        .unwrap()
        .clone();
    (world, host, id)
}

#[test]
fn r9_navigation_routes_ignore_generated_and_saved_files_but_not_furniture() {
    for allow_overlap in [false, true] {
        let (world, mut host, id) = folder();
        let original = host.anchors.iter().find(|a| a.id == id).unwrap().clone();
        let notes = crate::placement_scene::apply(
            &LayoutRead {
                retained_slots: vec![],
                decorations: vec![],
                positions: vec![(
                    "File.txt".into(),
                    Position {
                        at: [-1.5, 0.0],
                        yaw: 0.0,
                        size: [0.8; 3],
                        allow_overlap,
                        surface: None,
                        properties: Default::default(),
                    },
                )],
                incomplete_records: 0,
                warnings: vec![],
            },
            &world,
            &mut host,
        )
        .unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let saved = &host.file_placements[&id];
        let arrivals = [
            [original.at[0], original.at[2]],
            [saved.bounds.at[0], saved.bounds.at[2]],
        ];
        let before = host.clone();
        for at in arrivals {
            assert!(!crate::player::blocked_at(&host, at, 0.0));
            assert!(
                !host.blocked(at),
                "navigation disagrees at file {at:?}, overlap={allow_overlap}"
            );
        }
        host.check_routes(&arrivals).unwrap();
        for role in ["document", "decoration"] {
            let mut furniture_host = host.clone();
            let furniture = furniture_host
                .anchors
                .iter_mut()
                .find(|a| a.id == "vocabulary-document")
                .unwrap();
            furniture.role = role.into();
            let at = [furniture.at[0], furniture.at[2]];
            assert!(furniture_host.blocked(at));
            assert!(crate::player::blocked_at(&furniture_host, at, 0.0));
            assert!(furniture_host.check_routes(&[at]).is_err());
        }
        assert_eq!(host, before);
    }
}

#[test]
fn r9_saved_install_final_routes_do_not_discard_files_when_spawn_is_in_a_file_slot() {
    let (world, mut host, id) = folder();
    let original = host.anchors.iter().find(|a| a.id == id).unwrap();
    host.spawn = [original.at[0], original.at[2]];
    assert!(!crate::player::blocked_at(&host, host.spawn, 0.0));
    let notes = crate::placement_scene::apply(
        &LayoutRead {
            retained_slots: vec![],
            decorations: vec![],
            positions: vec![(
                "File.txt".into(),
                Position {
                    at: [-1.5, 0.0],
                    yaw: 0.0,
                    size: [0.8; 3],
                    allow_overlap: false,
                    surface: None,
                    properties: Default::default(),
                },
            )],
            incomplete_records: 0,
            warnings: vec![],
        },
        &world,
        &mut host,
    )
    .unwrap();
    assert!(
        notes.is_empty(),
        "valid non-solid saved placement discarded: {notes:?}"
    );
    assert!(host.file_placements.contains_key(&id));
    host.check_routes(&[[-1.5, 0.0]]).unwrap();
}

#[test]
fn r9_only_explicit_bases_own_collision_and_support_not_file_envelopes() {
    for base in [false, true] {
        for base_collision in [false, true] {
            for allow_overlap in [false, true] {
                let (world, mut host, id) = folder();
                let original = host.anchors.iter().find(|a| a.id == id).unwrap().clone();
                let original_at = [original.at[0], original.at[2]];
                assert!(host.file_bases().is_empty());
                assert!(host.solid_file_bases().is_empty());
                let position = Position {
                    at: [-1.5, 0.0],
                    yaw: 0.6,
                    size: [2.4, 1.2, 2.4],
                    allow_overlap,
                    surface: None,
                    properties: crate::placement::assembly::Properties {
                        base,
                        base_collision,
                        ..Default::default()
                    },
                };
                let notes = crate::placement_scene::apply(
                    &LayoutRead {
                        retained_slots: vec![],
                        decorations: vec![],
                        positions: vec![("File.txt".into(), position.clone())],
                        incomplete_records: 0,
                        warnings: vec![],
                    },
                    &world,
                    &mut host,
                ).unwrap();
                assert!(notes.is_empty(), "{notes:?}");
                assert!(host.file_placements.contains_key(&id));
                let expected: Vec<_> = position.base_bounds().unwrap().into_iter().collect();
                assert_eq!(host.file_bases(), expected.iter().cloned().map(|g| (id.clone(), g)).collect::<Vec<_>>());
                assert_eq!(host.solid_file_bases(), if base_collision { expected } else { vec![] });
                let solid = base && base_collision;
                assert_eq!(host.blocked(position.at), solid);
                assert_eq!(crate::player::blocked_at(&host, position.at, 0.0), solid);
                assert!(!host.blocked(original_at), "dormant slot cannot resurrect a base");
                assert!(!crate::player::blocked_at(&host, original_at, 0.0));
                // Inside the larger file envelope, outside the fixed base and
                // player radius. File size/yaw never enlarge a physics base.
                let file_only = [position.at[0] + 1.2, position.at[1]];
                assert!(!host.blocked(file_only));
                assert!(!crate::player::blocked_at(&host, file_only, 0.0));
                assert!(!crate::player::blocked_at(&host, position.at, 0.8), "above the base, the lifted file is non-solid");
                let mut pose = crate::Pose {
                    position: position.at,
                    feet_y: 3.0,
                    ..Default::default()
                };
                let mut player = crate::player::Player::default();
                for _ in 0..240 {
                    player.advance(&host, &mut pose, &crate::Input {
                        captured: true,
                        ..Default::default()
                    }, 1.0 / 120.0);
                }
                assert_eq!(pose.position, position.at);
                assert!((pose.feet_y - if solid { 0.8 } else { 0.0 }).abs() < 0.001, "only an enabled collidable base supports the player");
                host.check_routes(&[original_at, file_only]).unwrap();
            }
        }
    }
}

#[test]
fn r9_low_closed_room_route_rejects_target_occluded_at_the_runtime_eye() {
    let mut host = HostScene::shared_default();
    host.version = 2;
    host.room = [-3.0, -3.0, 3.0, 3.0];
    host.spawn = [0.0, 2.0];
    host.height = 1.0;
    host.anchors = vec![
        Anchor {
            id: "target".into(),
            role: "document".into(),
            at: [0.0; 3],
            size: [0.4, 0.8, 0.4],
            binding: "explicit-page".into(),
        },
        Anchor {
            id: "door".into(),
            role: "portal".into(),
            at: [0.0, 0.0, -3.5],
            size: [1.0, 1.0, 0.4],
            binding: "explicit-door".into(),
        },
        Anchor {
            id: "image".into(),
            role: "image".into(),
            at: [2.0, 0.0, -3.5],
            size: [0.4; 3],
            binding: "current-image".into(),
        },
    ];
    // Full-height four-sided enclosure: the player cannot enter, and the low
    // runtime eye cannot see in. The old 1.65m route eye sees over its near edge.
    host.geometry = vec![
        Geometry {
            at: [0.0, 0.0, 0.55],
            size: [1.2, 1.0, 0.1],
        },
        Geometry {
            at: [0.0, 0.0, -0.55],
            size: [1.2, 1.0, 0.1],
        },
        Geometry {
            at: [0.55, 0.0, 0.0],
            size: [0.1, 1.0, 1.0],
        },
        Geometry {
            at: [-0.55, 0.0, 0.0],
            size: [0.1, 1.0, 1.0],
        },
    ];
    let mut pose = crate::Pose {
        position: [0.0, 1.0],
        ..Default::default()
    };
    crate::player::fit_camera(&host, &mut pose);
    assert_eq!(pose.eye_height(), 0.85);
    assert!(
        !crate::player::blocked_at(&host, pose.position, 0.0),
        "witness is a reachable standing position"
    );
    let centre = glam::Vec3::new(0.0, 0.4, 0.0);
    let occluded = |eye| {
        host.geometry
            .iter()
            .any(|g| crate::interaction::segment_hits_box(eye, centre, g.at, g.size))
    };
    assert!(occluded(glam::Vec3::new(0.0, pose.eye_height(), 1.0)));
    assert!(
        !occluded(glam::Vec3::new(0.0, 1.65, 1.0)),
        "old eye genuinely sees over the enclosure"
    );
    let encoded = toml::to_string(&host).unwrap();
    let error = HostScene::parse(&encoded)
        .expect_err("route validation must not qualify a genuinely occluded low-room target");
    assert!(
        error.contains("unreachable"),
        "not an unrelated validity failure: {error}"
    );
    host.geometry.clear();
    let valid = HostScene::parse(&toml::to_string(&host).unwrap()).unwrap();
    valid.check_routes(&[]).unwrap();
    host.height = 3.0;
    crate::player::fit_camera(&host, &mut pose);
    assert_eq!(
        pose.eye_offset.to_bits(),
        1.65_f32.to_bits(),
        "ordinary eye remains exact"
    );
}
