//! Narrow Sky-only palette validation. Tables borrow the encoded source: no copy,
//! expanded decoder raster, or allocation outside the existing shared charge.
use crate::Result;

#[derive(Default)]
pub(super) struct Palette<'a> {
    rgb: Option<&'a [u8]>,
    alpha: Option<&'a [u8]>,
    idat: bool,
}
impl<'a> Palette<'a> {
    // Called by the existing bounded structural walk, including post-IDAT chunks.
    // CRC validation remains the pinned decoder's responsibility on accepted input.
    pub(super) fn observe(&mut self, kind: &[u8], payload: &'a [u8]) -> Result<()> {
        match kind {
            b"PLTE" => {
                if self.rgb.is_some() || self.idat {
                    return Err("sky PNG palette PLTE duplicate/late".into());
                }
                if payload.is_empty() || !payload.len().is_multiple_of(3) || payload.len() > 768 {
                    return Err("sky PNG palette PLTE requires 1..256 RGB entries".into());
                }
                self.rgb = Some(payload);
            }
            b"tRNS" => {
                let entries = self.rgb.ok_or("sky PNG palette tRNS before PLTE")?.len() / 3;
                if self.alpha.is_some()
                    || self.idat
                    || payload.is_empty()
                    || payload.len() > entries
                {
                    return Err("sky PNG palette tRNS duplicate/late/empty/overlong".into());
                }
                self.alpha = Some(payload);
            }
            b"IDAT" => {
                if self.rgb.is_none() {
                    return Err("sky PNG palette missing pre-IDAT PLTE".into());
                }
                self.idat = true;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn finish(&self) -> Result<()> {
        if self.rgb.is_none() || !self.idat {
            return Err("sky PNG palette requires pre-IDAT PLTE and image data".into());
        }
        Ok(())
    }

    // Input is the one real unfiltered 8-bit index frame from png, NOT EXPAND's
    // black-filled output. Validate ALL indices before allocating final RGBA.
    pub(super) fn rgba(&self, indices: &[u8], rgba_size: usize, cap: usize) -> Result<Vec<u8>> {
        let rgb = self.rgb.ok_or("sky PNG palette missing PLTE")?;
        let entries = rgb.len() / 3;
        if indices.iter().any(|&i| usize::from(i) >= entries) {
            return Err("sky PNG palette index outside PLTE".into());
        }
        if indices.len().checked_mul(4) != Some(rgba_size) {
            return Err("sky PNG palette raw index count mismatch".into());
        }
        let mut rgba = super::sky_png_reserve(rgba_size, cap)?;
        for &index in indices {
            let index = usize::from(index);
            let offset = index * 3; // bounded above by the checked <=256-entry table
            rgba.extend_from_slice(&[
                rgb[offset],
                rgb[offset + 1],
                rgb[offset + 2],
                self.alpha
                    .and_then(|a| a.get(index))
                    .copied()
                    .unwrap_or(255),
            ]);
        }
        if rgba.len() != rgba_size || rgba.capacity() > rgba_size {
            return Err("sky PNG palette RGBA count/capacity mismatch".into());
        }
        Ok(rgba)
    }
}
