//! Appearance-only static backgrounds, held separately from world materials.
use crate::Result;
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};

mod adaptation;
pub use adaptation::{PanoramaFit, PanoramaOptions, adapt_png};
#[cfg(test)]
mod adaptation_tests;

/// Unknown kinds and fields refuse; explicit skies require appearance v3.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum SkySpec {
    Colour { rgb: [f64; 3] },
    Panorama { file: String },
}

/// Immutable, validated decoded pixels. No path or encoded source is retained.
/// Only the bounded strict loader/CPU adapter can construct this resource.
#[derive(Debug)]
pub struct Panorama {
    image: crate::assets::Texture,
}
impl PartialEq for Panorama {
    fn eq(&self, other: &Self) -> bool {
        self.image.width == other.image.width
            && self.image.height == other.image.height
            && self.image.rgba == other.image.rgba
    }
}
impl Panorama {
    pub(crate) fn image(&self) -> &crate::assets::Texture {
        &self.image
    }
    fn dimensions(width: u32, height: u32) -> Result<()> {
        if width == 0
            || height == 0
            || width > 1024
            || height > 512
            || Some(width) != height.checked_mul(2)
        {
            return Err("panorama requires exact 2:1 dimensions, at most 1024x512".into());
        }
        Ok(())
    }
    fn from_image(image: crate::assets::Texture) -> Result<Self> {
        Self::dimensions(image.width, image.height)?;
        let expected = (image.width as usize)
            .checked_mul(image.height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or("panorama output overflow")?;
        if image.rgba.len() != expected
            || image.rgba.capacity() > 2_097_152
            || image.rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
        {
            return Err("panorama requires bounded opaque RGBA output".into());
        }
        Ok(Self { image })
    }
    fn load(root: &Path, file: &str) -> Result<Self> {
        let path = Path::new(file);
        if !file.ends_with(".png")
            || file.is_empty()
            || file.contains(['\\', ':'])
            || path.components().count() > 8
            || !path
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err("panorama requires a bounded pack-local relative .png path".into());
        }
        // The existing descriptor-held regular-file reader refuses every symlink
        // component, special files, escapes and growth past the finite read cap.
        let bytes = crate::content::read_local(root, file, 4_000_000)?;
        // Retain the existing descriptor acquisition and decimal encoded cap.
        // Include actual reader vector capacity in the prequalified source allowance.
        if bytes.capacity() > 8 * 1024 * 1024 {
            return Err("panorama encoded source capacity exceeds 8 MiB work allowance".into());
        }
        adapt_png(&bytes, PanoramaOptions::default())
    }
}

/// Absence is exact historical behavior, not an explicit colour.
/// Cloning a panorama shares its held pixels rather than copying the image.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SkyAppearance {
    #[default]
    Legacy,
    Colour([f64; 3]),
    Panorama(Arc<Panorama>),
}
impl SkySpec {
    /// Panorama files automatically use Fill and linear [0.04,0.06,0.05].
    /// There is no independent background/options field in the existing schema.
    pub fn load(self, root: &Path) -> Result<SkyAppearance> {
        let sky = match self {
            Self::Colour { rgb } => SkyAppearance::Colour(rgb),
            Self::Panorama { file } => SkyAppearance::Panorama(Arc::new(
                Panorama::load(root, &file).map_err(|e| format!("SKY_REFUSED: {e}"))?,
            )),
        };
        sky.validate()?;
        Ok(sky)
    }
}
impl SkyAppearance {
    /// Validate colour in f64; Panorama's private resource is immutable and
    /// already validated, so selecting/drawing it never scans or reopens source.
    pub fn validate(&self) -> Result<()> {
        if let Self::Colour(rgb) = self
            && !rgb.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            return Err("SKY_REFUSED: colour rgb must contain exactly three finite linear-light components in 0..1".into());
        }
        Ok(())
    }
}
