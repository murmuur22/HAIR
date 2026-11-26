//! Presentation only: never supplied to host containment, supports or placement records.
use crate::{
    assets::Vertex,
    host_scene::{HostScene, Wall},
};
use glam::Vec3;

/// Metre-scale vocabulary shared by every theme. Colour comes from an existing
/// appearance role/surface; no new pack schema, file meshes or host anchors.
pub(crate) fn append(out: &mut Vec<Vertex>, host: &HostScene, palette: [f32; 3]) {
    let [left, front, right, rear] = host.room;
    if !host.room.iter().all(|x| x.is_finite() && x.abs() <= 64.0) || right <= left || rear <= front
    {
        return;
    }
    let post = palette.map(|c| (c * 0.65 + 0.25).clamp(0.0, 1.0));
    let rope = palette.map(|c| (c * 0.55).clamp(0.04, 0.7));
    for (wall, axis, edge, low, high) in [
        (None, 2, front, left, right),
        (Some(Wall::Rear), 2, rear, left, right),
        (Some(Wall::Left), 0, left, front, rear),
        (Some(Wall::Right), 0, right, front, rear),
    ] {
        // Existing walls already mark their edges. Retain their authored variation.
        if wall.is_some() && !host.open_platform {
            continue;
        }
        let tangent = if axis == 2 { 0 } else { 2 };
        let mut holes = Vec::new();
        for a in host
            .anchors
            .iter()
            .filter(|a| a.role == "portal" && a.id != crate::folder_scene::INERT_PORTAL)
        {
            let aperture = host.wall_apertures.iter().find(|o| o.anchor == a.id);
            let matches = match aperture {
                Some(o) => Some(o.wall) == wall,
                None => wall.is_none(),
            };
            if matches {
                let width = aperture.map_or(a.size[tangent], |o| o.width);
                let half = width * 0.5 + 0.12;
                holes.push([a.at[tangent] - half, a.at[tangent] + half]);
            }
        }
        holes.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut cursor = low;
        for [start, end] in holes {
            let start = start.clamp(low, high);
            let end = end.clamp(low, high);
            if start > cursor {
                run(out, axis, edge, cursor, start, post, rope);
            }
            cursor = cursor.max(end);
        }
        if cursor < high {
            run(out, axis, edge, cursor, high, post, rope);
        }
    }
}
fn run(
    out: &mut Vec<Vertex>,
    axis: usize,
    edge: f32,
    low: f32,
    high: f32,
    post: [f32; 3],
    rope: [f32; 3],
) {
    // Bases remain inside the retained interval, including beside a doorway.
    let low = low + 0.10;
    let high = high - 0.10;
    if high <= low {
        return;
    }
    let spans = ((high - low) / 1.8).ceil().max(1.0) as usize;
    let point = |t, y| {
        if axis == 2 {
            Vec3::new(t, y, edge)
        } else {
            Vec3::new(edge, y, t)
        }
    };
    for i in 0..=spans {
        let t = low + (high - low) * i as f32 / spans as f32;
        tube(out, point(t, 0.05), point(t, 0.10), 0.10, post);
        tube(out, point(t, 0.10), point(t, 0.94), 0.028, post);
        tube(out, point(t, 0.91), point(t, 1.0), 0.052, post);
        if i == spans {
            continue;
        }
        let next = low + (high - low) * (i + 1) as f32 / spans as f32;
        let curve = |j: usize| {
            let u = j as f32 / 8.0;
            point(t + (next - t) * u, 0.90 - 0.16 * 4.0 * u * (1.0 - u))
        };
        for j in 0..8 {
            tube(out, curve(j), curve(j + 1), 0.022, rope);
        }
    }
}
/// Capped octagonal tube; one consistent untextured mesh convention. A local
/// orthonormal frame avoids axis-dependent rope thickness and nonfinite normals.
fn tube(out: &mut Vec<Vertex>, a: Vec3, b: Vec3, radius: f32, color: [f32; 3]) {
    let direction = (b - a).normalize();
    let reference = if direction.y.abs() > 0.9 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let u = direction.cross(reference).normalize() * radius;
    let v = direction.cross(u).normalize() * radius;
    let ring = |centre: Vec3, i: usize| {
        let angle = std::f32::consts::TAU * (i % 8) as f32 / 8.0;
        centre + u * angle.cos() + v * angle.sin()
    };
    for i in 0..8 {
        let (p, q, r, s) = (ring(a, i), ring(a, i + 1), ring(b, i + 1), ring(b, i));
        for position in [p, q, r, p, r, s, a, q, p, b, s, r] {
            out.push(Vertex {
                position: position.to_array(),
                color,
                uv: [0.0; 2],
                texture: 0,
            });
        }
    }
}
