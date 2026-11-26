//! Player-only vertical motion. File placements never gain rigid-body physics.
//! The App uses advance for swept planar/vertical motion in one feet-relative frame.
//! Legacy step remains vertical-only for callers which own their planar adapter.
use crate::{Input, Pose, host_scene::HostScene};

pub const BODY_HEIGHT: f32 = 1.8;
/// Crouch to a closed low room, without creating artificial jumping headroom.
/// Furniture never changes this height.
pub fn body_height(host: &HostScene) -> f32 {
    if host.open_platform {
        BODY_HEIGHT
    } else {
        BODY_HEIGHT.min(host.height)
    }
}

/// Floor-relative eye shared by runtime camera fitting and host route sightlines.
pub fn eye_offset(host: &HostScene) -> f32 {
    let body = body_height(host);
    // Preserve the exact ordinary-room float, including framebuffer witnesses.
    if body == BODY_HEIGHT {
        1.65
    } else {
        body - 0.15
    }
}

pub fn fit_camera(host: &HostScene, pose: &mut Pose) {
    pose.eye_offset = eye_offset(host);
}
#[derive(Clone, Copy, Debug)]
pub struct Player {
    pub height: f32,
    pub velocity: f32,
    pub grounded: bool,
}
impl Default for Player {
    fn default() -> Self {
        Self {
            height: 0.0,
            velocity: 0.0,
            grounded: true,
        }
    }
}
impl Player {
    pub fn jump(&mut self) {
        if self.grounded {
            self.velocity = 6.5;
            self.grounded = false;
        }
    }
    /// Actual visitor adapter: small shared timesteps prevent planar/vertical tunnelling.
    pub fn advance(&mut self, host: &HostScene, pose: &mut Pose, input: &Input, dt: f32) {
        fit_camera(host, pose);
        if !input.captured || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.height = pose.feet_y;
        if !self.height.is_finite()
            || !self.velocity.is_finite()
            || self.height < -2.0
            || self.height > host.height + 8.0
            || outside(host, pose.position)
            || blocked_at(host, pose.position, self.height)
        {
            self.recover(host, pose);
            return;
        }
        let dt = dt.min(0.25);
        let count = (dt * 120.0).ceil().max(1.0) as usize;
        let h = dt / count as f32;
        let local = glam::Vec2::new(
            (input.right as i32 - input.left as i32) as f32,
            (input.back as i32 - input.forward as i32) as f32,
        )
        .normalize_or_zero();
        let (s, c) = pose.yaw.sin_cos();
        let delta = glam::Vec2::new(c * local.x - s * local.y, s * local.x + c * local.y)
            * h
            * 2.5
            * if input.running { 2.0 } else { 1.0 };
        for _ in 0..count {
            self.step(host, pose, input, h);
            for axis in 0..2 {
                let mut p = pose.position;
                p[axis] += delta[axis];
                if !blocked_at(host, p, self.height) {
                    pose.position = p;
                }
            }
        }
        // Horizontal motion may have just left a support edge. Do not permit
        // a coyote-free midair jump before the following simulation timestep.
        if self.grounded && self.height > 0.001 {
            self.grounded = solids(host).iter().any(|(at, size)| {
                footprint(pose.position, *at, *size, 0.22)
                    && (self.height - at[1] - size[1]).abs() < 0.001
            });
        }
        pose.feet_y = self.height;
    }
    /// Bounded same-room floor search. Never trusts a stale saved-file envelope
    /// or repeatedly respawns at a furniture-occupied authored spawn.
    fn recover(&mut self, host: &HostScene, pose: &mut Pose) {
        let safe = std::iter::once(host.spawn)
            .chain((0..=64).flat_map(|z| {
                (0..=64).map(move |x| {
                    [
                        host.room[0] + (host.room[2] - host.room[0]) * x as f32 / 64.0,
                        host.room[1] + (host.room[3] - host.room[1]) * z as f32 / 64.0,
                    ]
                })
            }))
            .find(|p| !blocked_at(host, *p, 0.0));
        self.velocity = 0.0;
        if let Some(p) = safe {
            pose.position = p;
            pose.feet_y = 0.0;
            self.height = 0.0;
            self.grounded = true;
        }
    }
    pub fn step(&mut self, host: &HostScene, pose: &mut Pose, input: &Input, dt: f32) {
        fit_camera(host, pose);
        if !input.captured || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let solids = solids(host);
        let body = body_height(host);
        let dt = dt.min(0.25);
        let count = (dt * 120.0).ceil().max(1.0) as usize;
        let h = dt / count as f32;
        for _ in 0..count {
            let before = self.height;
            self.velocity -= 14.0 * h;
            let mut next = before + self.velocity * h;
            self.grounded = false;
            for (at, size) in &solids {
                if !footprint(pose.position, *at, *size, 0.22) {
                    continue;
                }
                let top = at[1] + size[1];
                if self.velocity <= 0.0 && before >= top - 0.001 && next <= top {
                    next = top;
                    self.velocity = 0.0;
                    self.grounded = true;
                } else if self.velocity > 0.0
                    && before + body <= at[1] + 0.001
                    && next + body >= at[1]
                {
                    next = at[1] - body;
                    self.velocity = 0.0;
                }
            }
            if !host.open_platform && next + body > host.height {
                next = (host.height - body).max(0.0);
                self.velocity = self.velocity.min(0.0);
            }
            self.height = next;
            if self.height <= 0.0 {
                self.height = 0.0;
                self.velocity = 0.0;
                self.grounded = true;
            }
        }
    }
}
fn outside(host: &HostScene, p: [f32; 2]) -> bool {
    p.iter().any(|v| !v.is_finite())
        || p[0] < host.room[0]
        || p[0] > host.room[2]
        || p[1] < host.room[1]
        || p[1] > host.room[3]
}
/// Stationary envelopes only. File placements are visual/interaction targets,
/// never player colliders, independent of their overlap preference.
pub fn blocked_at(host: &HostScene, p: [f32; 2], feet: f32) -> bool {
    outside(host, p)
        || !feet.is_finite()
        || (!host.open_platform && feet + body_height(host) > host.height + 0.001)
        || solids(host).iter().any(|(at, size)| {
            feet < at[1] + size[1] - 0.001
                && feet + body_height(host) > at[1] + 0.001
                && footprint(p, *at, *size, 0.22)
        })
}
fn footprint(p: [f32; 2], at: [f32; 3], size: [f32; 3], radius: f32) -> bool {
    (p[0] - at[0]).abs() < size[0] * 0.5 + radius && (p[1] - at[2]).abs() < size[2] * 0.5 + radius
}
fn solids(host: &HostScene) -> Vec<([f32; 3], [f32; 3])> {
    host.anchors
        .iter()
        .filter(|a| {
            matches!(a.role.as_str(), "document" | "decoration")
                && !host.file_anchors.contains(&a.id)
        })
        .map(|a| (a.at, a.size))
        .chain(
            host.geometry
                .iter()
                .chain(host.overhead.iter())
                .map(|g| (g.at, g.size)),
        )
        .chain(host.solid_file_bases().into_iter().map(|g| (g.at, g.size)))
        .chain(host.solid_decorations().map(|g| (g.at, g.size)))
        .collect()
}
#[cfg(test)]
#[path = "player_r7_tests.rs"]
mod r7_tests;
#[cfg(test)]
#[path = "player_tests.rs"]
mod regression_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn falling_player_lands_on_the_stationary_object_top() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut host =
            crate::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default())
                .unwrap()
                .host;
        let mut base = host.anchor("document").clone();
        base.at = [0.0, 0.0, 0.0];
        base.size = [1.6, 0.8, 1.6];
        host.anchors = vec![base];
        host.geometry.clear();
        let mut pose = Pose {
            position: [0.0, 0.0],
            ..Default::default()
        };
        let mut p = Player {
            height: 2.2,
            velocity: -1.0,
            grounded: false,
        };
        let input = Input {
            captured: true,
            ..Default::default()
        };
        for _ in 0..180 {
            p.step(&host, &mut pose, &input, 1.0 / 120.0);
        }
        assert!(p.grounded);
        assert!(
            (p.height - 0.8).abs() < 0.001,
            "feet must settle on the object, got {}",
            p.height
        );
    }
    #[test]
    fn jump_rises_then_lands_without_moving_the_room() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let host =
            crate::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default())
                .unwrap()
                .host;
        let before = host.clone();
        let mut pose = Pose {
            position: host.spawn,
            ..Default::default()
        };
        let input = Input {
            captured: true,
            ..Default::default()
        };
        let mut p = Player::default();
        p.jump();
        p.step(&host, &mut pose, &input, 0.1);
        assert!(p.height > 0.2, "Space must actually leave the floor");
        for _ in 0..240 {
            p.step(&host, &mut pose, &input, 1.0 / 120.0);
        }
        assert!(p.grounded && p.height.abs() < 0.001);
        assert_eq!(host, before);
    }
}
