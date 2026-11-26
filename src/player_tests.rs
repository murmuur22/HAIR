use super::*;
use crate::{
    host_scene::{Geometry, PlacedItem},
    placement_store::Position,
};

fn empty_room() -> HostScene {
    let mut host = HostScene::shared_default();
    host.anchors.clear();
    host.geometry.clear();
    host.overhead.clear();
    host.file_placements.clear();
    host
}
fn advance(player: &mut Player, host: &HostScene, seconds: usize) {
    let mut pose = Pose {
        position: [0.0; 2],
        ..Default::default()
    };
    let input = Input {
        captured: true,
        ..Default::default()
    };
    for _ in 0..seconds * 120 {
        player.step(host, &mut pose, &input, 1.0 / 120.0);
    }
}
#[test]
fn placed_files_are_not_player_support_even_when_overlap_is_prevented() {
    for allow_overlap in [false, true] {
        let mut host = empty_room();
        host.file_placements.insert(
            "owned-file".into(),
            PlacedItem {
                position: Position {
                    properties: Default::default(),
                    at: [0.0; 2],
                    yaw: 0.0,
                    size: [1.0; 3],
                    allow_overlap,
                    surface: None,
                },
                bounds: Geometry {
                    at: [0.0; 3],
                    size: [1.0; 3],
                },
            },
        );
        let before = host.clone();
        let mut player = Player {
            height: 2.0,
            velocity: -1.0,
            grounded: false,
        };
        advance(&mut player, &host, 2);
        assert!(player.grounded);
        assert_eq!(
            player.height, 0.0,
            "saved file transforms are not standable furniture"
        );
        assert_eq!(
            host, before,
            "player simulation must not rewrite file placements"
        );
    }
}
#[test]
fn jump_hits_the_underside_of_overhead_geometry() {
    let mut host = empty_room();
    host.overhead.push(Geometry {
        at: [0.0, 2.2, 0.0],
        size: [2.0, 0.4, 2.0],
    });
    let mut pose = Pose {
        position: [0.0; 2],
        ..Default::default()
    };
    let input = Input {
        captured: true,
        ..Default::default()
    };
    let mut player = Player::default();
    player.jump();
    let mut peak = 0.0_f32;
    for _ in 0..240 {
        player.step(&host, &mut pose, &input, 1.0 / 120.0);
        peak = peak.max(player.height);
    }
    assert!(peak > 0.2, "the player must leave the floor");
    assert!(
        peak <= 0.4001,
        "1.8m player must not pass through 2.2m lintel; peak {peak}"
    );
    assert!(player.grounded && player.height == 0.0);
}
#[test]
fn paused_uncaptured_and_nonpositive_steps_leave_vertical_state_unchanged() {
    let host = empty_room();
    let mut pose = Pose::default();
    for (captured, dt) in [(false, 0.1), (true, 0.0), (true, -0.1), (true, f32::NAN)] {
        let mut player = Player {
            height: 1.0,
            velocity: 2.0,
            grounded: true,
        };
        player.step(
            &host,
            &mut pose,
            &Input {
                captured,
                ..Default::default()
            },
            dt,
        );
        assert_eq!(
            (player.height, player.velocity, player.grounded),
            (1.0, 2.0, true)
        );
    }
}
