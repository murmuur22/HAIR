//! Pure bounded placement geometry; no filesystem or inventory authority.
use crate::{Result, assets::Vertex};
pub mod surface;
pub mod toolbox;
pub mod assembly;
pub mod scaling;
pub mod animation;
/// Fit uniformly inside a host-owned logical envelope, then ground the real
/// mesh base at y=0 and rotate about its own horizontal center.
pub fn grounded(
    model: &[Vertex],
    at: [f32; 2],
    yaw: f32,
    envelope: [f32; 3],
) -> Result<Vec<Vertex>> {
    if model.is_empty()
        || model.len() > 32766
        || !model.len().is_multiple_of(3)
        || !at.into_iter().chain([yaw]).all(f32::is_finite)
        || envelope
            .into_iter()
            .any(|v| !v.is_finite() || v <= 0.0 || v > 16.0)
        || model.iter().any(|v| {
            !v.position
                .into_iter()
                .chain(v.color)
                .chain(v.uv)
                .all(f32::is_finite)
        })
    {
        return Err("Invalid or oversized placement geometry".into());
    }
    let mut lo = glam::Vec3::splat(f32::INFINITY);
    let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
    for v in model {
        let p = glam::Vec3::from_array(v.position);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let extent = hi - lo;
    if !extent.is_finite() || extent.max_element() <= 0.000001 {
        return Err("Placement model has no usable extent".into());
    }
    let scale = extent
        .to_array()
        .into_iter()
        .zip(envelope)
        .filter(|(d, _)| *d > 0.0)
        .map(|(d, e)| e / d)
        .fold(f32::INFINITY, f32::min);
    if !scale.is_finite() || scale <= 0.0 {
        return Err("Placement fit refused".into());
    }
    let middle = lo + extent * 0.5;
    let origin = glam::Vec3::new(middle.x, lo.y, middle.z);
    let turn = glam::Mat3::from_rotation_y(yaw);
    let mut result = Vec::new();
    result
        .try_reserve_exact(model.len())
        .map_err(|_| "Placement allocation refused")?;
    for v in model {
        let p = turn * ((glam::Vec3::from_array(v.position) - origin) * scale)
            + glam::Vec3::new(at[0], 0.0, at[1]);
        if !p.is_finite() {
            return Err("Placement transform overflow".into());
        }
        result.push(Vertex {
            position: p.to_array(),
            ..*v
        });
    }
    if !result.as_chunks::<3>().0.iter().any(|t| {
        let [a, b, c] = t.map(|v| glam::Vec3::from_array(v.position));
        let area = (b - a).cross(c - a).length_squared();
        area.is_finite() && area > 1.0e-12
    }) {
        return Err("Placement model has no visible triangles".into());
    }
    Ok(result)
}
/// Bounded eye-ray intersection with the floor; looking level/up never invents a hit.
pub fn floor_target(pose: &crate::Pose) -> Option<[f32; 2]> {
    if !pose
        .position
        .into_iter()
        .chain([pose.yaw, pose.pitch, pose.feet_y])
        .all(f32::is_finite)
        || !(-std::f32::consts::FRAC_PI_2..0.0).contains(&pose.pitch)
    {
        return None;
    }
    let vertical = pose.pitch.sin();
    let distance = -pose.eye_height() / vertical;
    if !distance.is_finite() || !(0.0..=6.0).contains(&distance) {
        return None;
    }
    let horizontal = distance * pose.pitch.cos();
    let at = [
        pose.position[0] + pose.yaw.sin() * horizontal,
        pose.position[1] - pose.yaw.cos() * horizontal,
    ];
    at.into_iter().all(f32::is_finite).then_some(at)
}
/// Manual overlap preference, independent of floor targeting and grounding.
/// Generated rooms and every caller without explicit consent remain Prevent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overlap {
    #[default]
    Prevent,
    Allow,
}
/// Conservative, theme-independent rotated envelope for preview clearance.
/// Returns a new obstacle; never mutates the host scene.
pub fn clearance(
    host: &crate::host_scene::HostScene,
    at: [f32; 2],
    yaw: f32,
    size: [f32; 3],
) -> Result<crate::host_scene::Geometry> {
    clearance_with_overlap(host, at, yaw, size, Overlap::Prevent)
}
/// Explicit manual-placement policy. Never use Allow for automatic generation.
pub fn clearance_with_overlap(
    host: &crate::host_scene::HostScene,
    at: [f32; 2],
    yaw: f32,
    size: [f32; 3],
    overlap_policy: Overlap,
) -> Result<crate::host_scene::Geometry> {
    use crate::host_scene::Geometry;
    if !at.into_iter().chain([yaw]).all(f32::is_finite)
        || size
            .into_iter()
            .any(|n| !n.is_finite() || n <= 0.0 || n > 16.0)
        || size[1] > host.height
        || !host
            .room
            .into_iter()
            .chain(host.spawn)
            .chain([host.height])
            .all(f32::is_finite)
        || host.anchors.len() > 256
        || host.geometry.len() > 256
        || host.overhead.len() > 256
    {
        return Err("Invalid placement envelope or host".into());
    }
    let (s, c) = yaw.sin_cos();
    let extent = [
        c.abs() * size[0] + s.abs() * size[2],
        size[1],
        s.abs() * size[0] + c.abs() * size[2],
    ];
    let bounds = [
        at[0] - extent[0] * 0.5,
        at[1] - extent[2] * 0.5,
        at[0] + extent[0] * 0.5,
        at[1] + extent[2] * 0.5,
    ];
    if bounds[0] < host.room[0] + 0.25
        || bounds[1] < host.room[1] + 0.25
        || bounds[2] > host.room[2] - 0.25
        || bounds[3] > host.room[3] - 0.25
    {
        return Err("Placement leaves platform clearance".into());
    }
    let overlap = |p: [f32; 3], z: [f32; 3], pad: f32| {
        bounds[0] <= p[0] + z[0] * 0.5 + pad
            && bounds[2] >= p[0] - z[0] * 0.5 - pad
            && bounds[1] <= p[2] + z[2] * 0.5 + pad
            && bounds[3] >= p[2] - z[2] * 0.5 - pad
    };
    if overlap([host.spawn[0], 0.0, host.spawn[1]], [0.0; 3], 0.5) {
        return Err("Placement blocks arrival".into());
    }
    for a in &host.anchors {
        if a.role == "portal" {
            let inward = host.portal_inward(&a.id);
            let reserve = if inward[0] != 0.0 {
                [3.0, host.height, a.size[2].max(1.65) + 0.5]
            } else {
                [a.size[0].max(1.65) + 0.5, host.height, 3.0]
            };
            if overlap(
                [a.at[0] + inward[0], 0.0, a.at[2] + inward[1]],
                reserve,
                0.22,
            ) {
                return Err("Placement blocks doorway approach".into());
            }
        } else if overlap_policy == Overlap::Prevent
            && matches!(a.role.as_str(), "document" | "decoration")
            && overlap(a.at, a.size, 0.22)
        {
            return Err("Placement overlaps room object".into());
        }
    }
    for g in host.geometry.iter().chain(&host.overhead) {
        if g.at[1] <= size[1] + 0.1 && g.at[1] + g.size[1] >= 0.0 && overlap(g.at, g.size, 0.22) {
            return Err("Placement overlaps host geometry".into());
        }
    }
    Ok(Geometry {
        at: [at[0], 0.0, at[1]],
        size: extent,
    })
}
/// Confirmation-only route check on a disposable host clone. Preview must use
/// `clearance` instead of running the flood fill on every frame.
pub fn confirm_clearance(
    host: &crate::host_scene::HostScene,
    at: [f32; 2],
    yaw: f32,
    size: [f32; 3],
    visitor: [f32; 2],
) -> Result<()> {
    confirm_with_overlap(host, at, yaw, size, visitor, Overlap::Prevent)
}
/// Allow-overlap items are non-solid; their visual pile cannot sever walking
/// routes. The caller must preserve this policy during saved-scene rehydration.
pub fn confirm_with_overlap(
    host: &crate::host_scene::HostScene,
    at: [f32; 2],
    yaw: f32,
    size: [f32; 3],
    visitor: [f32; 2],
    overlap_policy: Overlap,
) -> Result<()> {
    let obstacle = clearance_with_overlap(host, at, yaw, size, overlap_policy)?;
    if visitor.into_iter().any(|v| !v.is_finite()) {
        return Err("Invalid placement visitor".into());
    }
    if overlap_policy == Overlap::Allow {
        // No physical obstacle is introduced by this explicitly non-solid item.
        return host.check_routes(&[visitor]);
    }
    if visitor.into_iter().any(|v| !v.is_finite())
        || (visitor[0] - at[0]).abs() <= obstacle.size[0] * 0.5 + 0.22
            && (visitor[1] - at[1]).abs() <= obstacle.size[2] * 0.5 + 0.22
    {
        return Err("Placement blocks visitor".into());
    }
    let mut staged = host.clone();
    staged.geometry.push(obstacle);
    staged.check_routes(&[visitor])
}
/// Owning placement chrome stays visible even when no floor point is in reach.
/// Neutral UI colors are deliberately distinct from valid/blocked mesh witnesses.
pub fn hud(size: [u32; 2], title: &str, reason: &str, confirming: bool) -> Vec<Vertex> {
    let [w, h] = size.map(|n| n.max(1) as f32);
    let mut p = crate::text::Painter {
        vertices: vec![],
        width: w,
        height: h,
    };
    p.rect(0.0, 0.0, w, 52.0_f32.min(h), [0.025, 0.045, 0.035]);
    let lines: Vec<_> = title
        .lines()
        .chain([
            reason,
            if confirming {
                "ENTER CONFIRM | ESC CANCEL"
            } else {
                "ARROWS ROTATE | V COPY/MOVE | C OVERLAPS"
            },
            if confirming {
                "FROZEN POSITION | NO WRITES UNTIL CONFIRMED"
            } else {
                "ENTER REVIEW | ESC CANCEL | SURFACE SNAP"
            },
        ])
        .collect();
    // Assembly state gets its own rows instead of disappearing off a long title.
    let top = (h - (12.0 + lines.len() as f32 * 22.0).max(132.0)).max(0.0);
    p.rect(0.0, top, w, h - top, [0.025, 0.045, 0.035]);
    let scale = if w >= 600.0 && h >= 360.0 { 2.0 } else { 1.0 };
    let cols = ((w - 24.0).max(0.0) / (6.0 * scale)) as usize;
    if h >= 52.0 {
        p.literal_text(
            12.0,
            16.0,
            &"TEMPORARY PLACEMENT | CLICK TO LOOK"
                .chars()
                .take(cols)
                .collect::<String>(),
            scale,
            [0.8, 0.9, 0.82],
        );
    }
    for (i, line) in lines.into_iter().enumerate() {
        let y = top + 12.0 + i as f32 * 22.0;
        if y + 7.0 * scale <= h {
            p.literal_text(
                12.0,
                y,
                &line.chars().take(cols).collect::<String>(),
                scale,
                [0.8, 0.9, 0.82],
            );
        }
    }
    p.vertices
}
#[cfg(test)]
mod tests {
    #[test]
    fn explicit_overlap_policy_allows_items_but_not_world_or_doorway_escape() {
        use crate::host_scene::{Anchor, Geometry, HostScene};
        let mut host = HostScene {
            file_anchors: Default::default(),
            version: 2,
            name: "manual overlap fixture".into(),
            decorations: Default::default(),
            room: [-5.0, -5.0, 5.0, 5.0],
            spawn: [0.0, 4.0],
            height: 4.0,
            anchors: vec![
                Anchor {
                    id: "file".into(),
                    role: "document".into(),
                    at: [2.0, 0.0, 0.0],
                    size: [0.8; 3],
                    binding: "file".into(),
                },
                Anchor {
                    id: "door".into(),
                    role: "portal".into(),
                    at: [0.0, 0.0, -5.5],
                    size: [1.45, 2.0, 0.6],
                    binding: "explicit-door".into(),
                },
            ],
            geometry: vec![],
            overhead: vec![],
            wall_apertures: vec![],
            open_platform: true,
            file_placements: Default::default(),
        };
        let before = host.clone();
        assert!(
            super::clearance(&host, [2.0, 0.0], 0.0, [0.8; 3]).is_err(),
            "generated/default placement remains separated"
        );
        assert!(
            super::clearance_with_overlap(&host, [2.0, 0.0], 0.0, [0.8; 3], super::Overlap::Allow)
                .is_ok()
        );
        assert!(
            host.blocked([2.0, 0.0]),
            "independent witness: the pre-existing object is solid"
        );
        assert!(!host.blocked([0.0, 0.0]));
        assert!(
            super::confirm_with_overlap(
                &host,
                [2.0, 0.0],
                0.0,
                [0.8; 3],
                [0.0, 0.0],
                super::Overlap::Allow
            )
            .is_ok()
        );
        assert!(
            super::confirm_with_overlap(
                &host,
                [0.0, 0.0],
                0.0,
                [0.8; 3],
                [0.0, 0.0],
                super::Overlap::Allow
            )
            .is_ok(),
            "new explicitly overlapping item is non-solid; it does not trap the visitor"
        );
        assert!(
            super::confirm_with_overlap(
                &host,
                [2.0, 0.0],
                0.0,
                [0.8; 3],
                [2.0, 0.0],
                super::Overlap::Allow
            )
            .is_err(),
            "do not bypass existing host solids for the visitor"
        );
        assert_eq!(host, before);
        host.anchors[0].role = "decoration".into();
        assert!(
            super::clearance_with_overlap(&host, [2.0, 0.0], 0.0, [0.8; 3], super::Overlap::Allow)
                .is_ok()
        );
        for at in [[5.0, 0.0], [0.0, 4.0], [0.0, -4.0], [f32::NAN, 0.0]] {
            assert!(
                super::clearance_with_overlap(&host, at, 0.0, [0.8; 3], super::Overlap::Allow)
                    .is_err(),
                "overlap is not permission to leave floor or cover a door/arrival"
            );
        }
        host.geometry.push(Geometry {
            at: [2.0, 0.0, 0.0],
            size: [0.8; 3],
        });
        assert!(
            super::clearance_with_overlap(&host, [2.0, 0.0], 0.0, [0.8; 3], super::Overlap::Allow)
                .is_err(),
            "structural host geometry remains distinct from file/decor objects"
        );
        let pose = crate::Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [2.0, 3.0],
            yaw: 0.0,
            pitch: -0.8,
        };
        assert!(
            super::floor_target(&pose).is_some(),
            "floor snapping is independent of overlap policy"
        );
        let grounded = super::grounded(&mesh(), [2.0, 0.0], 0.0, [0.8; 3]).unwrap();
        assert_eq!(
            grounded
                .iter()
                .map(|v| v.position[1])
                .fold(f32::INFINITY, f32::min),
            0.0
        );
    }
    #[test]
    fn confirmation_rejects_route_cut_and_preserves_host() {
        use crate::host_scene::{Anchor, HostScene};
        let host = HostScene {
            file_anchors: Default::default(),
            version: 2,
            name: "route cut control".into(),
            decorations: Default::default(),
            room: [-2.9, -3.0, 2.9, 3.0],
            spawn: [0.0, 2.5],
            height: 4.0,
            anchors: vec![Anchor {
                id: "goal".into(),
                role: "portal".into(),
                at: [0.0, 0.0, -3.5],
                size: [1.45, 2.0, 0.6],
                binding: "explicit-door".into(),
            }],
            geometry: vec![],
            overhead: vec![],
            wall_apertures: vec![],
            open_platform: true,
            file_placements: Default::default(),
        };
        host.check_routes(&[]).unwrap();
        let original = host.clone();
        assert!(super::confirm_clearance(&host, [1.5, 0.0], 0.0, [0.8; 3], host.spawn).is_ok());
        let mut barrier = host.clone();
        barrier.geometry.push(crate::host_scene::Geometry {
            at: [0.0; 3],
            size: [5.25, 1.0, 1.0],
        });
        for step in -12..=12 {
            assert!(
                barrier.blocked([step as f32 * 0.25, 0.0]),
                "independent route frontier must actually be closed"
            );
        }
        assert!(
            super::clearance(&host, [0.0; 2], 0.0, [5.25, 1.0, 1.0]).is_ok(),
            "cheap geometric test permits barrier"
        );
        assert!(
            super::confirm_clearance(&host, [0.0; 2], 0.0, [5.25, 1.0, 1.0], host.spawn).is_err(),
            "confirm must reject route cut"
        );
        assert!(
            super::confirm_clearance(&host, [1.5, 0.0], 0.0, [0.8; 3], [1.5, 0.0]).is_err(),
            "cannot put object on visitor"
        );
        assert_eq!(host, original);
    }
    #[test]
    fn clearance_reserves_room_spawn_solids_and_door_approaches_without_mutating_host() {
        use crate::host_scene::{Anchor, Geometry, HostScene, Wall, WallAperture};
        let mut host = HostScene {
            file_anchors: Default::default(),
            version: 2,
            name: "owned synthetic clearance host".into(),
            decorations: Default::default(),
            room: [-5.0, -5.0, 5.0, 5.0],
            spawn: [0.0, 4.0],
            height: 4.0,
            anchors: vec![Anchor {
                id: "door".into(),
                role: "portal".into(),
                at: [0.0, 0.0, -5.5],
                size: [1.45, 2.0, 0.6],
                binding: "explicit-door".into(),
            }],
            geometry: vec![Geometry {
                at: [-2.0, 0.0, 0.0],
                size: [1.0; 3],
            }],
            overhead: vec![],
            wall_apertures: vec![],
            open_platform: true,
            file_placements: Default::default(),
        };
        let before = host.clone();
        let box_ = super::clearance(&host, [2.0, 0.0], 0.0, [0.8, 0.8, 0.6]).unwrap();
        assert_eq!(box_.at, [2.0, 0.0, 0.0]);
        assert_eq!(box_.size, [0.8, 0.8, 0.6]);
        assert_eq!(host, before);
        for at in [[5.0, 0.0], [0.0, 4.0], [-2.0, 0.0], [0.0, -4.0]] {
            assert!(
                super::clearance(&host, at, 0.0, [0.8; 3]).is_err(),
                "must reserve {at:?}"
            );
        }
        // Side door metadata must rotate the approach reserve as well.
        host.anchors[0].at = [5.5, 0.0, 0.0];
        host.wall_apertures.push(WallAperture {
            anchor: "door".into(),
            wall: Wall::Right,
            width: 1.65,
            height: 2.2,
        });
        assert!(super::clearance(&host, [4.0, 0.0], 0.0, [0.8; 3]).is_err());
        assert!(super::clearance(&host, [0.0, 0.0], 0.0, [0.8; 3]).is_ok());
    }
    #[test]
    fn clearance_uses_rotated_logical_footprint_not_theme_mesh() {
        let host = crate::host_scene::HostScene {
            file_anchors: Default::default(),
            version: 2,
            name: "rotation control".into(),
            decorations: Default::default(),
            room: [-5.0, -5.0, 5.0, 5.0],
            spawn: [0.0, 4.0],
            height: 4.0,
            anchors: vec![],
            geometry: vec![],
            overhead: vec![],
            wall_apertures: vec![],
            open_platform: true,
            file_placements: Default::default(),
        };
        assert!(super::clearance(&host, [4.0, 0.0], 0.0, [0.6, 0.8, 3.0]).is_ok());
        assert!(
            super::clearance(
                &host,
                [4.0, 0.0],
                std::f32::consts::FRAC_PI_2,
                [0.6, 0.8, 3.0]
            )
            .is_err()
        );
        for size in [
            [0.0, 1.0, 1.0],
            [1.0, f32::NAN, 1.0],
            [1.0, 5.0, 1.0],
            [17.0, 1.0, 1.0],
        ] {
            assert!(super::clearance(&host, [0.0; 2], 0.0, size).is_err());
        }
        assert!(super::clearance(&host, [f32::NAN, 0.0], 0.0, [1.0; 3]).is_err());
        assert!(super::clearance(&host, [0.0; 2], f32::NAN, [1.0; 3]).is_err());
    }
    #[test]
    fn floor_target_follows_real_look_and_refuses_out_of_reach() {
        let mut pose = crate::Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [2.0, 3.0],
            yaw: 0.0,
            pitch: -std::f32::consts::FRAC_PI_4,
        };
        let target = super::floor_target(&pose).unwrap();
        assert!((target[0] - 2.0).abs() < 0.00001 && (target[1] - 1.35).abs() < 0.00001);
        pose.yaw = std::f32::consts::FRAC_PI_2;
        let target = super::floor_target(&pose).unwrap();
        assert!((target[0] - 3.65).abs() < 0.00001 && (target[1] - 3.0).abs() < 0.00001);
        for pitch in [0.0, 0.1, -0.01, f32::NAN, f32::INFINITY] {
            pose.pitch = pitch;
            assert!(super::floor_target(&pose).is_none());
        }
        pose.pitch = -0.8;
        pose.position[0] = f32::NAN;
        assert!(super::floor_target(&pose).is_none());
    }
    use super::*;
    fn mesh() -> Vec<Vertex> {
        [[10.0, 5.0, -3.0], [14.0, 5.0, -3.0], [10.0, 7.0, 1.0]]
            .map(|position| Vertex {
                position,
                color: [0.2, 0.4, 0.6],
                uv: [0.1, 0.2],
                texture: 3,
            })
            .to_vec()
    }
    #[test]
    fn refuses_wholly_collinear_mesh_but_accepts_planar_geometry() {
        let line =
            [[10.0, 5.0, -3.0], [12.0, 6.0, -1.0], [14.0, 7.0, 1.0]].map(|position| Vertex {
                position,
                color: [1.0; 3],
                uv: [0.0; 2],
                texture: 0,
            });
        assert!(grounded(&line, [0.0; 2], 0.0, [1.0; 3]).is_err());
        let plane = [[0.0, 2.0, 0.0], [1.0, 2.0, 0.0], [0.0, 2.0, 1.0]].map(|position| Vertex {
            position,
            ..line[0]
        });
        let result = grounded(&plane, [0.0; 2], 0.0, [1.0; 3]).unwrap();
        assert!(result.iter().all(|v| v.position[1] == 0.0));
    }
    #[test]
    fn bounds_and_nonfinite_inputs_are_refused() {
        let source = mesh();
        for at in [[f32::NAN, 0.0], [0.0, f32::INFINITY]] {
            assert!(grounded(&source, at, 0.0, [1.0; 3]).is_err());
        }
        for yaw in [f32::NAN, f32::INFINITY] {
            assert!(grounded(&source, [0.0; 2], yaw, [1.0; 3]).is_err());
        }
        for bound in [0.0, -1.0, 17.0, f32::NAN, f32::INFINITY] {
            assert!(grounded(&source, [0.0; 2], 0.0, [bound; 3]).is_err());
        }
        for kind in 0..3 {
            let mut bad = source.clone();
            match kind {
                0 => bad[0].position[0] = f32::NAN,
                1 => bad[0].color[1] = f32::INFINITY,
                _ => bad[0].uv[1] = f32::NAN,
            };
            assert!(grounded(&bad, [0.0; 2], 0.0, [1.0; 3]).is_err());
        }
        for len in [0, 1, 2, 32769] {
            let bad = source.iter().copied().cycle().take(len).collect::<Vec<_>>();
            assert!(grounded(&bad, [0.0; 2], 0.0, [1.0; 3]).is_err());
        }
        let max = source
            .iter()
            .copied()
            .cycle()
            .take(32766)
            .collect::<Vec<_>>();
        assert_eq!(
            grounded(&max, [0.0; 2], 0.0, [1.0; 3]).unwrap().len(),
            max.len()
        );
    }
    #[test]
    fn grounds_real_base_with_uniform_fit_and_preserves_source() {
        let source = mesh();
        let before = bytemuck::cast_slice::<_, u8>(&source).to_vec();
        let placed = grounded(&source, [3.0, -2.0], 0.0, [2.0, 4.0, 6.0]).unwrap();
        for (i, want) in [[2.0, 0.0, -3.0], [4.0, 0.0, -3.0], [2.0, 1.0, -1.0]]
            .into_iter()
            .enumerate()
        {
            assert_eq!(placed[i].position, want);
            assert_eq!(placed[i].color, source[i].color);
            assert_eq!(placed[i].texture, source[i].texture);
            assert_eq!(placed[i].uv, source[i].uv);
        }
        assert_eq!(bytemuck::cast_slice::<_, u8>(&source), before);
        let turned = grounded(
            &source,
            [3.0, -2.0],
            std::f32::consts::FRAC_PI_2,
            [2.0, 4.0, 6.0],
        )
        .unwrap();
        for (a, b) in turned[0].position.into_iter().zip([2.0, 0.0, -1.0]) {
            assert!((a - b).abs() < 0.000001);
        }
    }
}
