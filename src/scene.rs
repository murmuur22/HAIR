use crate::{Pack, Result, assets::Vertex};
use std::path::Path;
pub fn office(root: &Path) -> Result<Vec<Vertex>> {
    use glam::{Mat4, Vec3};
    let directory = crate::safe_path(root, "assets/kenney")?;
    let pack = Pack::parse(
        &std::fs::read_to_string(crate::safe_path(&directory, "pack.toml")?)
            .map_err(|e| e.to_string())?,
    )?;
    let mut models = std::collections::HashMap::new();
    for asset in pack.assets {
        let model = crate::assets::load(
            &crate::safe_path(&directory, &asset.file)?,
            asset.max_extent,
        )
        .map_err(|e| format!("model {}: {e}", directory.join(&asset.file).display()))?;
        let min = Vec3::from_array(model.min);
        let max = Vec3::from_array(model.max);
        if (max.x - min.x) * 2.0 > asset.footprint[0] as f32 * 0.5 + 0.01
            || (max.z - min.z) * 2.0 > asset.footprint[1] as f32 * 0.5 + 0.01
        {
            return Err(format!("role {} exceeds grid footprint", asset.role));
        }
        println!(
            "ASSET role={} vertices={} min={:?} max={:?}",
            asset.role,
            model.vertices.len(),
            model.min,
            model.max
        );
        models.insert(asset.role, model);
    }
    let mut vertices = Vec::new();
    let mut place = |role: &str, at: [f32; 3], yaw: f32| -> Result<()> {
        let model = models
            .get(role)
            .ok_or_else(|| format!("missing required role {role}"))?;
        let min = Vec3::from_array(model.min);
        let max = Vec3::from_array(model.max);
        // Reversible pivot normalization: center XZ, ground at min Y; attachments may be off-grid.
        let pivot = Vec3::new((min.x + max.x) * 0.5, min.y, (min.z + max.z) * 0.5);
        let transform = Mat4::from_translation(Vec3::from_array(at))
            * Mat4::from_rotation_y(yaw)
            * Mat4::from_scale(Vec3::splat(2.0))
            * Mat4::from_translation(-pivot);
        vertices.extend(model.vertices.iter().map(|v| {
            Vertex {
                position: transform
                    .transform_point3(Vec3::from_array(v.position))
                    .to_array(),
                color: v.color,
                uv: v.uv,
                texture: v.texture,
            }
        }));
        Ok(())
    };
    for x in [-3.0, -1.0, 1.0, 3.0] {
        for z in [-3.0, -1.0, 1.0, 3.0] {
            place("floor", [x, 0.0, z], 0.0)?;
        }
    }
    for x in [-3.0, -1.0, 1.0, 3.0] {
        place("wall", [x, 0.1, 4.0], 0.0)?;
    }
    for x in [-3.0, -2.0, 2.0, 3.0] {
        place("wall", [x, 0.1, -4.0], 0.0)?;
    }
    for z in [-3.0, -1.0, 1.0, 3.0] {
        for x in [-4.0, 4.0] {
            place("wall", [x, 0.1, z], std::f32::consts::FRAC_PI_2)?;
        }
    }
    place("portal", [0.0, 0.1, -4.0], 0.0)?;
    place("door", [2.5, 0.1, -3.85], 0.0)?;
    place("desk", [0.0, 0.1, 0.5], 0.0)?;
    place("chair", [1.5, 0.1, 1.5], 0.0)?;
    place("screen", [0.0, 0.869, 0.25], 0.0)?;
    place("keyboard", [-0.15, 0.869, 0.65], 0.0)?;
    place("mouse", [0.4, 0.869, 0.65], 0.0)?;
    place("lamp", [0.0, 2.2, 0.0], 0.0)?;
    // Small interaction placards supplement, never replace, the actual GLB geometry.
    for (at, size, color) in [
        ([0.0, 0.885, 0.8], [0.45, 0.015, 0.16], [0.1, 0.8, 0.3]),
        ([0.0, 1.25, -4.0], [0.9, 2.3, 0.03], [0.08, 0.38, 0.18]),
    ] {
        cuboid(&mut vertices, at, size, color);
    }
    Ok(vertices)
}
pub(crate) fn cuboid(out: &mut Vec<Vertex>, at: [f32; 3], size: [f32; 3], color: [f32; 3]) {
    let corners: Vec<[f32; 3]> = (0..8)
        .map(|i| {
            std::array::from_fn(|a| at[a] + size[a] * if i & (1 << a) == 0 { -0.5 } else { 0.5 })
        })
        .collect();
    for face in [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ] {
        for i in [0, 1, 2, 0, 2, 3] {
            out.push(Vertex {
                position: corners[face[i]],
                color,
                uv: [0.0; 2],
                texture: 0,
            });
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn scene_uses_every_selected_role_and_real_geometry() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let vertices = super::office(root).unwrap();
        assert!(vertices.len() > 4000);
        assert!(vertices.iter().any(|v| v.position[1] > 2.5));
        assert!(
            vertices
                .iter()
                .all(|v| v.position.iter().all(|n| n.is_finite()))
        );
    }
}
