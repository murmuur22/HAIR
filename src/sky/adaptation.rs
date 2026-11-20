//! Bounded CPU-only PNG adaptation. No paths, encoders, renderer or schema here.
use super::Panorama;
use crate::{Result, assets::Texture};

/// Runtime fitting only; not a pack field or a Workshop control.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanoramaFit {
    /// Uniform scaling, centered crop (default).
    #[default]
    Fill,
    /// Uniform scaling, centered background padding.
    Fit,
    /// Independent horizontal/vertical scaling; may distort the source.
    Stretch,
}

/// Shared CPU options; values are not serialized into existing panorama packs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanoramaOptions {
    pub fit: PanoramaFit,
    /// Finite linear-light RGB in 0..=1, for transparency and Fit padding.
    pub background_linear: [f64; 3],
}
impl Default for PanoramaOptions {
    fn default() -> Self {
        Self {
            fit: PanoramaFit::Fill,
            background_linear: [0.04, 0.06, 0.05],
        }
    }
}

/// Decode the narrow strict sky PNG profile and return immutable validated pixels.
/// Input: <=4,000,000 encoded bytes, axes1..4096, <=4,194,304 pixels/16MiB RGBA.
/// Existing conforming opaque exact2:1 <=1024x512 pixels pass through unchanged.
/// Conforming transparent inputs keep their dimensions; all other admitted inputs
/// use a fixed1024x512 canvas. Output is opaque sRGB RGBA, <=2MiB.
///
/// The combined52MiB logical admission estimate includes encoded allowance,
/// decoder/frame/RGBA/output/scratch; it is not an RSS or allocator guarantee.
/// This synchronous finite operation has no cancellation/deadline parameter.
/// Borrowed original bytes are never rewritten or retained. A caller retaining
/// other originals/derivatives must budget those separately. Not a360/seam repair.
pub fn adapt_png(bytes: &[u8], options: PanoramaOptions) -> Result<Panorama> {
    if !options
        .background_linear
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    {
        return Err("panorama background must contain finite linear RGB in 0..1".into());
    }
    let source = crate::assets::decode_sky_png(bytes)?;
    let conforming = Panorama::dimensions(source.width, source.height).is_ok();
    if conforming && source.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255) {
        return Panorama::from_image(source);
    }
    let (width, height) = if conforming {
        (source.width, source.height)
    } else {
        (1024, 512)
    };
    let output = sample(&source, width, height, options)?;
    Panorama::from_image(output)
}

fn srgb_to_linear(value: u8) -> f64 {
    let value = f64::from(value) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
fn linear_to_srgb(value: f64) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

fn sample(source: &Texture, width: u32, height: u32, options: PanoramaOptions) -> Result<Texture> {
    Panorama::dimensions(width, height)?;
    let stride = (source.width as usize)
        .checked_mul(4)
        .ok_or("panorama source stride overflow")?;
    let source_size = stride
        .checked_mul(source.height as usize)
        .ok_or("panorama source size overflow")?;
    if source.width == 0 || source.height == 0 || source.rgba.len() != source_size {
        return Err("panorama source shape refused".into());
    }
    let output_size = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= 2_097_152)
        .ok_or("panorama output budget exceeded")?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(output_size)
        .map_err(|_| "panorama output allocation refused")?;
    if rgba.capacity() > output_size {
        return Err("panorama output capacity exceeded".into());
    }
    // No full float raster: one transfer table and four taps on the stack.
    let transfer: [f64; 256] = std::array::from_fn(|i| srgb_to_linear(i as u8));
    let background = options.background_linear;
    let padding = [
        linear_to_srgb(background[0]),
        linear_to_srgb(background[1]),
        linear_to_srgb(background[2]),
        255,
    ];
    let mut sx = f64::from(width) / f64::from(source.width);
    let mut sy = f64::from(height) / f64::from(source.height);
    match options.fit {
        PanoramaFit::Fill => {
            sx = sx.max(sy);
            sy = sx;
        }
        PanoramaFit::Fit => {
            sx = sx.min(sy);
            sy = sx;
        }
        PanoramaFit::Stretch => {}
    }
    let drawn_w = f64::from(source.width) * sx;
    let drawn_h = f64::from(source.height) * sy;
    let offset_x = (f64::from(width) - drawn_w) * 0.5;
    let offset_y = (f64::from(height) - drawn_h) * 0.5;
    for y in 0..height {
        let dy = f64::from(y) + 0.5 - offset_y;
        for x in 0..width {
            let dx = f64::from(x) + 0.5 - offset_x;
            if options.fit == PanoramaFit::Fit
                && (dx < 0.0 || dx >= drawn_w || dy < 0.0 || dy >= drawn_h)
            {
                rgba.extend_from_slice(&padding);
                continue;
            }
            // Center-aligned coordinates, with safe replicated edges even at1px.
            let u = (dx / sx - 0.5).clamp(0.0, f64::from(source.width - 1));
            let v = (dy / sy - 0.5).clamp(0.0, f64::from(source.height - 1));
            let x0 = u.floor() as usize;
            let y0 = v.floor() as usize;
            let x1 = (x0 + 1).min(source.width as usize - 1);
            let y1 = (y0 + 1).min(source.height as usize - 1);
            let fx = u - x0 as f64;
            let fy = v - y0 as f64;
            let taps = [
                (x0, y0, (1.0 - fx) * (1.0 - fy)),
                (x1, y0, fx * (1.0 - fy)),
                (x0, y1, (1.0 - fx) * fy),
                (x1, y1, fx * fy),
            ];
            let mut linear = [0.0; 3];
            for (tx, ty, weight) in taps {
                // Checked full stride*height and exact RGBA length above prove
                // all bounded index products/sums below fit usize and source.
                let i = ty * stride + tx * 4;
                let p = &source.rgba[i..i + 4];
                let alpha = f64::from(p[3]) / 255.0;
                for channel in 0..3 {
                    // Composite EACH straight-alpha tap before interpolation;
                    // transparent poisoned RGB therefore contributes zero light.
                    linear[channel] += weight
                        * (transfer[p[channel] as usize] * alpha
                            + background[channel] * (1.0 - alpha));
                }
            }
            rgba.extend_from_slice(&[
                linear_to_srgb(linear[0]),
                linear_to_srgb(linear[1]),
                linear_to_srgb(linear[2]),
                255,
            ]);
        }
    }
    if rgba.len() != output_size {
        return Err("panorama output count mismatch".into());
    }
    Ok(Texture {
        width,
        height,
        rgba,
    })
}
