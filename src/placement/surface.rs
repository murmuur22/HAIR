//! Bounded geometric support contact; no filesystem or inventory authority.
use crate::{Result, assets::Vertex};
use glam::{Quat, Vec3};
/// Conservative 3D actual-mesh bounds. Touching a support plane is not
/// penetration. World/arrival/door reservations stay independent of item overlap.
pub fn clearance(
    host: &crate::host_scene::HostScene,
    mesh: &[Vertex],
    overlap: super::Overlap,
) -> Result<()> {
    if mesh.is_empty() || mesh.len() > 32766 || !mesh.len().is_multiple_of(3) {
        return Err("Invalid surface placement mesh".into());
    }
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for v in mesh {
        let p = Vec3::from_array(v.position);
        if !p.is_finite() || p.abs().max_element() > 64.0 {
            return Err("Invalid surface placement bounds".into());
        }
        lo = lo.min(p);
        hi = hi.max(p);
    }
    if lo.y < -0.0001 || (!host.open_platform && hi.y > host.height + 0.0001) {
        return Err("Surface placement leaves floor/ceiling bounds".into());
    }
    let size = (hi - lo).max(Vec3::splat(0.0001));
    let centre = (lo + hi) * 0.5;
    let mut reserved = host.clone();
    reserved.anchors.retain(|a| a.role == "portal");
    reserved.geometry.clear();
    reserved.overhead.clear();
    reserved.decorations.clear();
    super::clearance_with_overlap(
        &reserved,
        [centre.x, centre.z],
        0.0,
        [size.x, size.y.min(host.height), size.z],
        super::Overlap::Allow,
    )?;
    let penetrates = |at: [f32; 3], size: [f32; 3]| -> Result<bool> {
        if at.into_iter().chain(size).any(|v| !v.is_finite()) || size.into_iter().any(|v| v <= 0.0)
        {
            return Err("Invalid support envelope".into());
        }
        let b = Vec3::from_array(at);
        let s = Vec3::from_array(size);
        let bottom = b - Vec3::new(s.x * 0.5, 0.0, s.z * 0.5);
        let top = bottom + s;
        Ok((0..3).all(|i| {
            if hi[i] - lo[i] <= 0.0001 {
                // A flat image inside a volume penetrates it; boundary contact does not.
                lo[i] > bottom[i] + 0.0001 && hi[i] < top[i] - 0.0001
            } else {
                hi[i].min(top[i]) - lo[i].max(bottom[i]) > 0.0001
            }
        }))
    };
    for g in host.geometry.iter().chain(&host.overhead) {
        if penetrates(g.at, g.size)? {
            return Err("Placement intersects structural geometry".into());
        }
    }
    // Collision OFF changes walking physics, not placement overlap admission.
    for g in host.decoration_bounds() {
        if penetrates(g.at, g.size)? {
            return Err("Placement overlaps a saved decoration".into());
        }
    }
    if overlap == super::Overlap::Prevent {
        for item in host.file_placements.values() {
            if penetrates(item.bounds.at, item.bounds.size)? {
                return Err("Placement overlaps a saved file item".into());
            }
        }
        for a in &host.anchors {
            if matches!(a.role.as_str(), "document" | "decoration")
                && !host.file_placements.contains_key(&a.id)
                && penetrates(a.at, a.size)?
            {
                return Err("Placement overlaps an existing object".into());
            }
        }
    }
    Ok(())
}
pub const MAX_VERTICES: usize = 262_143;
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contact {
    pub at: [f32; 3],
    pub normal: [f32; 3],
}
impl Contact {
    pub fn validate(&self) -> Result<()> {
        let normal = Vec3::from_array(self.normal);
        if self
            .at
            .into_iter()
            .any(|v| !v.is_finite() || v.abs() > 64.0)
            || !normal.is_finite()
            || (normal.length_squared() - 1.0).abs() > 0.0001
        {
            return Err("Invalid surface contact".into());
        }
        Ok(())
    }
}
/// Ground in model-local space first, spin around local up, then align that up
/// with the support normal. Only the item's transform changes, never its support.
pub fn attached(
    model: &[Vertex],
    contact: &Contact,
    spin: f32,
    envelope: [f32; 3],
) -> Result<Vec<Vertex>> {
    contact.validate()?;
    if contact.normal == [0.0, 1.0, 0.0] {
        let mut out = super::grounded(model, [contact.at[0], contact.at[2]], spin, envelope)?;
        for v in &mut out {
            v.position[1] += contact.at[1];
        }
        return Ok(out);
    }
    let align = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(contact.normal).normalize());
    let origin = Vec3::from_array(contact.at);
    let mut out = super::grounded(model, [0.0, 0.0], spin, envelope)?;
    for v in &mut out {
        let p = align * Vec3::from_array(v.position) + origin;
        if !p.is_finite() {
            return Err("Surface transform overflow".into());
        }
        v.position = p.to_array();
    }
    Ok(out)
}
/// Cast the real bounded camera ray against actual support triangles. Choose the
/// nearest face, even when another surface's bounding box appears nearer. Normals
/// face the ray's incident side, allowing both sides of planar file items.
pub fn target(pose: &crate::Pose, surfaces: &[&[Vertex]]) -> Result<Option<Contact>> {
    if !pose
        .position
        .into_iter()
        .chain([pose.yaw, pose.pitch, pose.feet_y])
        .all(f32::is_finite)
        || pose.pitch.abs() > std::f32::consts::FRAC_PI_2
    {
        return Err("Invalid surface ray".into());
    }
    let mut count = 0usize;
    for mesh in surfaces {
        count = count
            .checked_add(mesh.len())
            .ok_or("Surface work overflow")?;
        if count > MAX_VERTICES
            || !mesh.len().is_multiple_of(3)
            || mesh.iter().any(|v| {
                v.position
                    .into_iter()
                    .any(|p| !p.is_finite() || p.abs() > 64.0)
            })
        {
            return Err("Invalid or oversized support geometry".into());
        }
    }
    let eye = Vec3::new(pose.position[0], pose.eye_height(), pose.position[1]);
    let direction = Vec3::new(
        pose.yaw.sin() * pose.pitch.cos(),
        pose.pitch.sin(),
        -pose.yaw.cos() * pose.pitch.cos(),
    );
    let mut nearest = 6.0f32;
    let mut hit = None;
    if let Some(at) = super::floor_target(pose) {
        let point = Vec3::new(at[0], 0.0, at[1]);
        nearest = point.distance(eye);
        hit = Some(Contact {
            at: point.to_array(),
            normal: Vec3::Y.to_array(),
        });
    }
    for mesh in surfaces {
        for tri in mesh.as_chunks::<3>().0 {
            let a = Vec3::from_array(tri[0].position);
            let b = Vec3::from_array(tri[1].position);
            let c = Vec3::from_array(tri[2].position);
            let e1 = b - a;
            let e2 = c - a;
            let cross = direction.cross(e2);
            let det = e1.dot(cross);
            if det.abs() < 1.0e-7 {
                continue;
            }
            let rel = eye - a;
            let u = rel.dot(cross) / det;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = rel.cross(e1);
            let v = direction.dot(q) / det;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let t = e2.dot(q) / det;
            if !t.is_finite() || t < 0.001 || t >= nearest {
                continue;
            }
            let mut normal = e1.cross(e2).normalize_or_zero();
            if normal == Vec3::ZERO {
                continue;
            }
            if normal.dot(direction) > 0.0 {
                normal = -normal;
            }
            let candidate = Contact {
                at: (eye + direction * t).to_array(),
                normal: normal.to_array(),
            };
            candidate.validate()?;
            hit = Some(candidate);
            nearest = t;
        }
    }
    Ok(hit)
}
