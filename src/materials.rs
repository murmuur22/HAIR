//! Data-only host surface styling. Never constructs or moves geometry.
use crate::{Result, assets::Vertex};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Surface {
    pub surface: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Linear-light RGB multiplier, not sRGB UI bytes.
    pub tint: [f32; 3],
    pub meters_per_tile: f32,
}
pub fn validate(surfaces: &[Surface]) -> Result<()> {
    let mut names = std::collections::HashSet::new();
    if surfaces.len() > 2 {
        return Err("at most wall and floor surfaces".into());
    }
    for s in surfaces {
        if !["wall", "floor"].contains(&s.surface.as_str())
            || !names.insert(&s.surface)
            || s.tint
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            || !s.meters_per_tile.is_finite()
            || !(0.1..=16.0).contains(&s.meters_per_tile)
        {
            return Err("surface: unique wall/floor, finite linear tint 0..1 and meters_per_tile 0.1..16 required".into());
        }
    }
    Ok(())
}
// Each declaration is charged at its worst decoded size even if bytes/path deduplicate.
pub(crate) fn decoded_budget(surfaces: &[Surface]) -> usize {
    surfaces.iter().filter(|s| s.file.is_some()).count() * 4 * 1024 * 1024
}
fn linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn srgb(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}
pub(crate) fn load(
    root: &std::path::Path,
    surfaces: &[Surface],
) -> Result<Vec<std::sync::Arc<crate::assets::Texture>>> {
    validate(surfaces)?;
    let mut textures = Vec::new();
    for s in surfaces {
        if let Some(file) = &s.file {
            if !file.ends_with(".png") || std::path::Path::new(file).components().count() > 8 {
                return Err("surface texture requires a bounded relative .png path".into());
            }
            let bytes = crate::content::read_local(root, file, 4_000_000)?;
            let mut t = crate::assets::decode_texture(&bytes)?;
            // The host stays opaque: composite alpha over white in linear light,
            // then encode back to sRGB for the existing shared sRGB texture sampler.
            for p in t.rgba.as_chunks_mut::<4>().0 {
                let a = p[3] as f32 / 255.0;
                for c in &mut p[..3] {
                    *c = (srgb(linear(*c as f32 / 255.0) * a + 1.0 - a) * 255.0)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
                p[3] = 255;
            }
            textures.push(std::sync::Arc::new(t));
        }
    }
    Ok(textures)
}
pub(crate) fn apply(vertices: &mut [Vertex], surfaces: &[Surface], wall_count: usize) {
    let mut texture = 0;
    for s in surfaces {
        if s.file.is_some() {
            texture += 1;
        }
        // Host vertices begin with one floor cuboid, then every authoritative
        // wall segment. Static/overhead geometry after those keeps its own look.
        let range = if s.surface == "floor" {
            0..36
        } else {
            36..36 + wall_count * 36
        };
        for face in vertices[range].as_chunks_mut::<6>().0 {
            let a =
                glam::Vec3::from_array(face[1].position) - glam::Vec3::from_array(face[0].position);
            let b =
                glam::Vec3::from_array(face[2].position) - glam::Vec3::from_array(face[0].position);
            let n = a.cross(b).abs();
            for v in face {
                v.color = s.tint;
                v.texture = if s.file.is_some() { texture } else { 0 };
                let p = v.position;
                let uv = if s.surface == "floor" || (n.y > n.x && n.y > n.z) {
                    [p[0], p[2]]
                } else if n.x > n.z {
                    [p[2], p[1]]
                } else {
                    [p[0], p[1]]
                };
                v.uv = uv.map(|v| v / s.meters_per_tile);
            }
        }
    }
}
