//! Explicit normalized three-slot trim representation, not general door rebuilding.
use crate::{
    Result,
    assets::{Model, Vertex},
    host_scene::Anchor,
};
/// Each transformed triangle must be wholly contained in ONE convex trim slot.
/// Vertex-only union tests would admit a diagonal triangle across the aperture.
pub fn qualify(m: &Model, pivot: [f32; 3], yaw: f32, scale: f32) -> Result<Vec<Vertex>> {
    let rot = glam::Mat4::from_rotation_y(yaw.to_radians());
    let vertices: Vec<_> = m
        .vertices
        .iter()
        .map(|v| Vertex {
            position: ((rot.transform_point3(glam::Vec3::from_array(v.position))
                + glam::Vec3::from_array(pivot))
                * scale)
                .to_array(),
            ..*v
        })
        .collect();
    if vertices.is_empty() || !vertices.len().is_multiple_of(3) {
        return Err("door-trim needs nonempty triangles".into());
    }
    for t in vertices.as_chunks::<3>().0 {
        if t.iter().any(|v| {
            let [x, y, z] = v.position;
            !x.is_finite()
                || !y.is_finite()
                || !z.is_finite()
                || !(-0.5..=0.5).contains(&x)
                || !(0.0..=1.0).contains(&y)
                || !(-0.5..=0.5).contains(&z)
        }) || !(t.iter().all(|v| v.position[0] <= -0.4)
            || t.iter().all(|v| v.position[0] >= 0.4)
            || t.iter().all(|v| v.position[1] >= 0.9))
        {
            return Err("Unsafe door-trim triangle: after node/local transforms every triangle must lie wholly in one normalized jamb (X -0.5..-0.4 or 0.4..0.5) or lintel (Y 0.9..1), with Y 0..1 and Z -0.5..0.5. The central aperture is reserved. Last-good appearance retained; use a three-slot trim model, not a door leaf/full opaque model.".into());
        }
    }
    Ok(vertices)
}
pub fn append(out: &mut Vec<Vertex>, trim: &[Vertex], a: &Anchor, offset: u32) {
    for v in trim {
        let p = std::array::from_fn(|i| a.at[i] + v.position[i] * a.size[i]);
        out.push(Vertex {
            position: p,
            texture: if v.texture == 0 {
                0
            } else {
                v.texture + offset
            },
            ..*v
        });
    }
}
pub fn frame(out: &mut Vec<Vertex>, a: &Anchor) {
    for (x, y, w, h) in [
        (-0.45, 0.5, 0.1, 1.0),
        (0.45, 0.5, 0.1, 1.0),
        (0.0, 0.95, 0.8, 0.1),
    ] {
        crate::scene::cuboid(
            out,
            [a.at[0] + x * a.size[0], a.at[1] + y * a.size[1], a.at[2]],
            [w * a.size[0], h * a.size[1], a.size[2] * 0.3],
            [0.4, 0.48, 0.36],
        );
    }
}
