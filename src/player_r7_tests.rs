use super::*;
use crate::placement_store::Position;

#[test]
fn r7_real_folder_file_anchors_are_not_walls_or_support_before_or_after_placement() {
    use crate::local_explorer::{Entry, Kind, Snapshot};
    let folder = std::path::PathBuf::from("/r7-read-only-snapshot");
    let snapshot = Snapshot {
        folder: folder.clone(),
        entries: vec![Entry {
            name: "file.txt".into(),
            path: folder.join("file.txt"),
            kind: Kind::File,
        }],
    };
    for moved in [false, true] {
        for allow_overlap in [false, true] {
            let (world, mut host) = crate::folder_scene::build(&snapshot, 0, false).unwrap();
            let (id, _) = world
                .folder_view
                .as_ref()
                .unwrap()
                .bindings
                .iter()
                .next()
                .unwrap();
            let original = host.anchors.iter().find(|a| &a.id == id).unwrap().clone();
            assert_eq!(original.role, "document");
            if moved {
                let notes = crate::placement_scene::apply(
                    &crate::placement_store::LayoutRead {
                        retained_slots: vec![],
                        decorations: vec![],
                        positions: vec![(
                            "file.txt".into(),
                            Position {
                                properties: Default::default(),
                                at: [-1.5, 0.0],
                                yaw: 0.0,
                                size: [0.8; 3],
                                allow_overlap,
                                surface: None,
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
                assert!(host.file_placements.contains_key(id));
            }
            let before = host.clone();
            for at in [[original.at[0], original.at[2]], [-1.5, 0.0]] {
                assert!(
                    !blocked_at(&host, at, 0.0),
                    "file slot is non-solid moved={moved}"
                );
                let mut walking = Pose {
                    position: [at[0], at[1] + 1.0],
                    ..Default::default()
                };
                let mut walker = Player::default();
                for _ in 0..96 {
                    walker.advance(
                        &host,
                        &mut walking,
                        &Input {
                            captured: true,
                            forward: true,
                            ..Default::default()
                        },
                        1.0 / 120.0,
                    );
                }
                assert!(
                    (walking.position[1] - at[1] + 1.0).abs() < 0.001,
                    "walk fully through generated/saved file slot"
                );
                let mut pose = Pose {
                    position: at,
                    feet_y: 1.1,
                    ..Default::default()
                };
                let mut player = Player {
                    height: 1.1,
                    velocity: -1.0,
                    grounded: false,
                };
                let input = Input {
                    captured: true,
                    ..Default::default()
                };
                for _ in 0..240 {
                    player.advance(&host, &mut pose, &input, 1.0 / 120.0);
                }
                assert_eq!(pose.position, at, "file may not force recovery");
                assert_eq!(pose.feet_y, 0.0, "file is not a platform");
            }
            assert_eq!(
                host, before,
                "saved placements and original anchors stay unchanged"
            );
            let furniture = host
                .anchors
                .iter()
                .find(|a| a.id == "vocabulary-document")
                .unwrap();
            assert!(
                blocked_at(&host, [furniture.at[0], furniture.at[2]], 0.0),
                "authored document furniture remains solid"
            );
            let mut pose = Pose {
                position: [furniture.at[0], furniture.at[2]],
                feet_y: 1.1,
                ..Default::default()
            };
            let mut player = Player {
                height: 1.1,
                velocity: -1.0,
                grounded: false,
            };
            for _ in 0..240 {
                player.advance(
                    &host,
                    &mut pose,
                    &Input {
                        captured: true,
                        ..Default::default()
                    },
                    1.0 / 120.0,
                );
            }
            assert_eq!(
                pose.feet_y,
                furniture.at[1] + furniture.size[1],
                "stationary document still supports player"
            );
            let encoded = toml::to_string(&host).unwrap();
            assert!(!encoded.contains("file_anchors"));
            assert!(
                HostScene::parse(&encoded).unwrap().file_anchors.is_empty(),
                "authored scenes cannot inherit runtime file authority"
            );
        }
    }
}
