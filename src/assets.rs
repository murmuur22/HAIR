use crate::Result;
mod jpeg;
mod local_model;
#[path = "assets/sky_palette.rs"]
mod sky_palette;
pub use local_model::{decode_local_model, decode_local_preview_model, retained_model_bytes};

#[cfg(test)]
#[path = "assets/local_model_safety_tests.rs"]
mod local_model_safety_tests;

/// Signature authority for the opened local reader only, never a suffix claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalImageFormat {
    Png,
    Jpeg,
}
impl LocalImageFormat {
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            Some(Self::Jpeg)
        } else {
            None
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
        }
    }
}

pub fn decode_local_image(bytes: &[u8]) -> Result<Texture> {
    match LocalImageFormat::detect(bytes) {
        Some(LocalImageFormat::Png) => decode_local_png(bytes),
        Some(LocalImageFormat::Jpeg) => jpeg::decode(bytes),
        None => Err("local image signature unsupported; raw source retained".into()),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub uv: [f32; 2],
    // Zero is the implicit opaque white material; images use one-based indices.
    pub texture: u32,
}
#[derive(Debug)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
pub struct Model {
    pub vertices: Vec<Vertex>,
    pub textures: Vec<std::sync::Arc<Texture>>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}
pub fn load(path: &std::path::Path, max_extent: f32) -> Result<Model> {
    if std::fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len()
        > 8_000_000
    {
        return Err("GLB too large".into());
    }
    decode(&std::fs::read(path).map_err(|e| e.to_string())?, max_extent)
        .map_err(|e| format!("{}: {e}", path.display()))
}
pub fn decode(bytes: &[u8], max_extent: f32) -> Result<Model> {
    decode_with_image_decoder(bytes, max_extent, decode_texture)
}

fn decode_with_image_decoder(
    bytes: &[u8],
    max_extent: f32,
    image_decoder: impl Fn(&[u8]) -> Result<Texture>,
) -> Result<Model> {
    use glam::{Mat4, Vec3};
    if bytes.len() > 8_000_000 || !max_extent.is_finite() || max_extent <= 0.0 {
        return Err("invalid GLB size/bounds".into());
    }
    let file = gltf::Gltf::from_slice(bytes).map_err(|e| e.to_string())?;
    if file.buffers().count() != 1
        || file
            .buffers()
            .any(|b| !matches!(b.source(), gltf::buffer::Source::Bin))
        || file.images().count() > 4
        || file.materials().count() > 16
        || file.meshes().map(|m| m.primitives().count()).sum::<usize>() > 64
        || file.extensions_required().next().is_some()
        || file.skins().count() != 0
        || file.animations().count() != 0
        || file.nodes().count() > 256
    {
        return Err("GLB profile: static embedded PNG/UV0 base color only; budget or required feature unsupported; no external resources".into());
    }
    let blob = file.blob.as_deref().ok_or("missing GLB binary")?;
    let mut textures = Vec::new();
    for image in file.images() {
        let gltf::image::Source::View {
            view,
            mime_type: "image/png",
        } = image.source()
        else {
            return Err(
                "texture must be an embedded PNG; re-export GLB with embedded base color".into(),
            );
        };
        let end = view
            .offset()
            .checked_add(view.length())
            .ok_or("image range overflow")?;
        let bytes = blob
            .get(view.offset()..end)
            .ok_or("truncated embedded PNG")?;
        textures.push(std::sync::Arc::new(image_decoder(bytes)?));
    }
    let mut vertices = Vec::new();
    fn visit(
        node: gltf::Node<'_>,
        parent: Mat4,
        blob: &[u8],
        out: &mut Vec<Vertex>,
        depth: u32,
        visited: &mut [bool],
    ) -> Result<()> {
        if depth > 32 {
            return Err("node hierarchy too deep/cyclic".into());
        }
        // Scene-wide, not just the active path: shared subtrees also multiply work.
        // Track nodes, not meshes; distinct nodes may instance the same mesh.
        if visited[node.index()] {
            return Err("repeated node in selected scene hierarchy".into());
        }
        visited[node.index()] = true;
        let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        if !transform.is_finite() || transform.determinant().abs() < 0.000001 {
            return Err("invalid transform".into());
        }
        if let Some(mesh) = node.mesh() {
            for p in mesh.primitives() {
                if p.mode() != gltf::mesh::Mode::Triangles || p.morph_targets().len() != 0 {
                    return Err("unsupported primitive".into());
                }
                let reader = p.reader(|_| Some(blob));
                let positions: Vec<_> = reader
                    .read_positions()
                    .ok_or("missing positions")?
                    .collect();
                let normals: Vec<_> = reader.read_normals().ok_or("missing normals")?.collect();
                if positions.len() != normals.len() || positions.len() > 100_000 {
                    return Err("invalid vertex count".into());
                }
                if positions
                    .iter()
                    .chain(normals.iter())
                    .flatten()
                    .any(|n| !n.is_finite())
                {
                    return Err("nonfinite source geometry".into());
                }
                let indices: Vec<u32> = reader
                    .read_indices()
                    .ok_or("missing indices")?
                    .into_u32()
                    .collect();
                if !indices.len().is_multiple_of(3) || out.len() + indices.len() > 300_000 {
                    return Err("invalid/oversized triangles".into());
                }
                let material = p.material().pbr_metallic_roughness();
                let texture_info = material.base_color_texture();
                let texture = texture_info
                    .as_ref()
                    .map_or(0, |t| t.texture().source().index() as u32 + 1);
                let uv: Vec<[f32; 2]> = if let Some(info) = texture_info {
                    if info.tex_coord() != 0 {
                        return Err("base color needs TEXCOORD_0; re-export UV0".into());
                    }
                    reader
                        .read_tex_coords(0)
                        .ok_or("textured model missing TEXCOORD_0")?
                        .into_f32()
                        .collect()
                } else {
                    vec![[0.0; 2]; positions.len()]
                };
                if uv.len() != positions.len()
                    || uv
                        .iter()
                        .flatten()
                        .any(|v| !v.is_finite() || v.abs() > 1024.0)
                {
                    return Err("invalid UV count/finite coordinate budget".into());
                }
                let color = material.base_color_factor();
                if color.iter().any(|c| !c.is_finite() || *c < 0.0 || *c > 1.0) {
                    return Err("invalid material".into());
                }
                for i in indices {
                    let i = i as usize;
                    let position = transform
                        .transform_point3(Vec3::from_array(*positions.get(i).ok_or("bad index")?));
                    let normal = transform
                        .inverse()
                        .transpose()
                        .transform_vector3(Vec3::from_array(normals[i]))
                        .normalize_or_zero();
                    if !position.is_finite() || !normal.is_finite() {
                        return Err("nonfinite geometry".into());
                    }
                    let light =
                        0.45 + 0.55 * normal.dot(Vec3::new(-0.3, 0.85, 0.4).normalize()).max(0.0);
                    out.push(Vertex {
                        position: position.to_array(),
                        color: [color[0] * light, color[1] * light, color[2] * light],
                        uv: uv[i],
                        texture,
                    });
                }
            }
        }
        for child in node.children() {
            visit(child, transform, blob, out, depth + 1, visited)?;
        }
        Ok(())
    }
    let scene = file
        .default_scene()
        .or_else(|| file.scenes().next())
        .ok_or("missing GLB scene")?;
    let mut visited = vec![false; file.nodes().count()];
    for node in scene.nodes() {
        visit(node, Mat4::IDENTITY, blob, &mut vertices, 0, &mut visited)?;
    }
    if vertices.is_empty() {
        return Err("empty geometry".into());
    }
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for v in &vertices {
        min = min.min(Vec3::from_array(v.position));
        max = max.max(Vec3::from_array(v.position));
    }
    if (max - min).max_element() > max_extent
        || min.abs().max_element() > 10.0
        || max.abs().max_element() > 10.0
    {
        return Err("geometry exceeds manifest bounds".into());
    }
    Ok(Model {
        vertices,
        textures,
        min: min.to_array(),
        max: max.to_array(),
    })
}

/// Bounded local-reader consumer; separate from unchanged Site/pack policies.
/// Static 8-bit gray/RGB/gray-alpha/RGBA only, <=4 MiB encoded, <=1024 axes.
/// Samples are interpreted as sRGB; ICC/text are never expanded. No 16-bit loss.
pub fn decode_local_png(bytes: &[u8]) -> Result<Texture> {
    decode_strict_png(bytes, PngProfile::Local)
}

const PNG_DECODER_ALLOWANCE: usize = 8 * 1024 * 1024;
const SKY_PNG_SOURCE_ALLOWANCE: usize = 8 * 1024 * 1024;
const SKY_PNG_OUTPUT_ALLOWANCE: usize = 2 * 1024 * 1024;
const SKY_PNG_SCRATCH_ALLOWANCE: usize = 1024 * 1024;
const SKY_PNG_WORK_CAP: usize = 52 * 1024 * 1024;

#[derive(Clone, Copy)]
enum PngProfile {
    Local,
    Sky,
}
impl PngProfile {
    fn rgba_cap(self) -> usize {
        match self {
            Self::Local => 4 * 1024 * 1024,
            Self::Sky => 16 * 1024 * 1024,
        }
    }
    fn axis_cap(self) -> u32 {
        match self {
            Self::Local => 1024,
            Self::Sky => 4096,
        }
    }
}
fn checked_png_rgba(width: usize, height: usize, cap: usize) -> Result<usize> {
    width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= cap)
        .ok_or_else(|| "PNG RGBA overflow".into())
}

// Logical admission, NOT an allocator/RSS/sandbox bound. Charge encoded storage
// (including reader growth slack), both live frame/RGBA vectors, library allowance,
// derivative and fixed/row scratch together, even though the decoder is dropped
// before sampling. Indexed Sky uses <=pixels raw indices + <=4*pixels RGBA
// within these same two RGBA charges; borrowed palette metadata adds no raster.
// png::Limits does not account for every internal allocation.
fn sky_png_budget(width: u32, height: u32, encoded: usize, work_cap: usize) -> Result<usize> {
    if encoded > 4_000_000 {
        return Err("sky PNG encoded budget exceeds 4,000,000 bytes".into());
    }
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return Err("sky PNG axes must be 1..4096".into());
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .filter(|n| *n <= 4_194_304)
        .ok_or("sky PNG pixels exceed 4,194,304")?;
    let rgba = checked_png_rgba(pixels, 1, 16_777_216)?;
    SKY_PNG_SOURCE_ALLOWANCE
        .checked_add(rgba)
        .and_then(|n| n.checked_add(rgba))
        .and_then(|n| n.checked_add(PNG_DECODER_ALLOWANCE))
        .and_then(|n| n.checked_add(SKY_PNG_OUTPUT_ALLOWANCE))
        .and_then(|n| n.checked_add(SKY_PNG_SCRATCH_ALLOWANCE))
        .filter(|n| *n <= work_cap)
        .ok_or("sky PNG combined work budget exceeded")?;
    Ok(rgba)
}

pub(crate) fn decode_sky_png(bytes: &[u8]) -> Result<Texture> {
    if bytes.len() < 33
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.get(8..16) != Some(&b"\0\0\0\rIHDR"[..])
    {
        return Err("panorama requires a genuine PNG with first 13-byte IHDR".into());
    }
    // Untrusted header is budget-only preflight BEFORE any library construction.
    // The common strict decoder below still checks header CRC and actual shape.
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    let expected = sky_png_budget(width, height, bytes.len(), SKY_PNG_WORK_CAP)?;
    let image = decode_strict_png(bytes, PngProfile::Sky)?;
    if image.width != width || image.height != height || image.rgba.len() != expected {
        return Err("sky PNG decoded shape differs from preflight".into());
    }
    Ok(image)
}

fn sky_png_reserve(size: usize, cap: usize) -> Result<Vec<u8>> {
    if size > cap {
        return Err("sky PNG allocation budget exceeded".into());
    }
    let mut out = Vec::new();
    out.try_reserve_exact(size)
        .map_err(|_| "sky PNG allocation refused")?;
    if out.capacity() > size {
        return Err("sky PNG allocation capacity exceeded".into());
    }
    Ok(out)
}

fn decode_strict_png(bytes: &[u8], profile: PngProfile) -> Result<Texture> {
    const CAP: usize = 4 * 1024 * 1024;
    if bytes.len() > CAP || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("local PNG signature/4 MiB encoded budget refused".into());
    }
    // Bounded structural preflight before library metadata parsing. Reject APNG
    // wherever located; cap aggregate ancillary bytes/count, including post-IDAT.
    let mut at = 8_usize;
    let mut count = 0_usize;
    let mut metadata = 0_usize;
    let mut ended = false;
    let mut palette = None;
    while at < bytes.len() {
        count = count.checked_add(1).ok_or("PNG chunk count overflow")?;
        if count > 4096 {
            return Err("PNG chunk count exceeds 4096".into());
        }
        let head = bytes
            .get(at..at.checked_add(8).ok_or("PNG offset overflow")?)
            .ok_or("truncated PNG chunk header")?;
        let len = u32::from_be_bytes(head[..4].try_into().unwrap()) as usize;
        let kind = &head[4..8];
        let end = at
            .checked_add(12)
            .and_then(|n| n.checked_add(len))
            .filter(|n| *n <= bytes.len())
            .ok_or("PNG chunk exceeds encoded bytes")?;
        if count == 1 && (kind != b"IHDR" || len != 13) {
            return Err("PNG requires first 13-byte IHDR".into());
        }
        if count == 1 && matches!(profile, PngProfile::Sky) && bytes[at + 17] == 3 {
            // Indexed Sky only. Borrow bounded table payloads; the decoder still
            // checks IHDR/PLTE/tRNS CRCs and all real frame properties below.
            palette = Some(sky_palette::Palette::default());
        }
        if let Some(palette) = &mut palette {
            palette.observe(kind, &bytes[at + 8..end - 4])?;
        }
        if matches!(kind, b"acTL" | b"fcTL" | b"fdAT") {
            return Err("animated PNG refused".into());
        }
        // No silent ICC/gamma/orientation reinterpretation in this first profile.
        // Reject unsupported color/orientation metadata without expanding it.
        if matches!(
            kind,
            b"iCCP" | b"eXIf" | b"cHRM" | b"cICP" | b"mDCV" | b"cLLI"
        ) {
            return Err(
                "PNG custom color profile/orientation metadata unsupported; raw source retained"
                    .into(),
            );
        }
        if kind == b"gAMA" && (len != 4 || bytes[at + 8..at + 12] != 45455_u32.to_be_bytes()) {
            return Err("PNG non-sRGB gamma unsupported; raw source retained".into());
        }
        if kind != b"IDAT" {
            metadata = metadata.checked_add(len).ok_or("PNG metadata overflow")?;
            if metadata > 256 * 1024 {
                return Err("PNG aggregate metadata exceeds 256 KiB".into());
            }
        }
        if kind == b"IEND" {
            if len != 0 || end != bytes.len() {
                return Err("PNG IEND/trailing bytes refused".into());
            }
            ended = true;
            break;
        }
        at = end;
    }
    if !ended {
        return Err("PNG missing IEND".into());
    }
    if let Some(palette) = &palette {
        palette.finish()?;
    }
    let mut options = png::DecodeOptions::default();
    options.set_ignore_text_chunk(true);
    options.set_ignore_iccp_chunk(true);
    options.set_ignore_adler32(false);
    options.set_skip_ancillary_crc_failures(false);
    let mut decoder = png::Decoder::new_with_options(std::io::Cursor::new(bytes), options);
    decoder.set_limits(png::Limits {
        bytes: PNG_DECODER_ALLOWANCE,
    });
    decoder.ignore_checksums(false);
    let header = decoder
        .read_header_info()
        .map_err(|e| format!("local PNG header: {e}"))?;
    let (width, height) = (header.width, header.height);
    if width == 0
        || height == 0
        || width > profile.axis_cap()
        || height > profile.axis_cap()
        || header.bit_depth != png::BitDepth::Eight
        || !(matches!(
            header.color_type,
            png::ColorType::Grayscale
                | png::ColorType::Rgb
                | png::ColorType::GrayscaleAlpha
                | png::ColorType::Rgba
        ) || (palette.is_some() && header.color_type == png::ColorType::Indexed))
    {
        return Err(match profile {
            PngProfile::Local => "local PNG: 1..1024 axes; static 8-bit gray/RGB/gray-alpha/RGBA only (no indexed/16-bit conversion)",
            PngProfile::Sky if palette.is_some() => "sky PNG: 1..4096 axes; static noninterlaced 8-bit indexed only (no lower-depth/16-bit conversion)",
            PngProfile::Sky => "sky PNG: 1..4096 axes; static 8-bit gray/RGB/gray-alpha/RGBA only (no indexed/16-bit conversion)",
        }.into());
    }
    if palette.is_some() && header.interlaced {
        return Err("sky PNG indexed interlace unsupported".into());
    }
    // EXPAND honors existing nonindexed 8-bit tRNS. Indexed Sky MUST retain
    // original indices through the real library unfilter/decode, never EXPAND.
    decoder.set_transformations(if palette.is_some() {
        png::Transformations::IDENTITY
    } else {
        png::Transformations::EXPAND
    });
    let rgba_size = checked_png_rgba(width as usize, height as usize, profile.rgba_cap())?;
    if matches!(profile, PngProfile::Sky) {
        sky_png_budget(width, height, bytes.len(), SKY_PNG_WORK_CAP)?;
    }
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("local PNG metadata: {e}"))?;
    if reader.info().animation_control.is_some() {
        return Err("animated PNG refused".into());
    }
    let size = reader
        .output_buffer_size()
        .filter(|n| *n <= profile.rgba_cap())
        .ok_or(match profile {
            PngProfile::Local => "PNG output exceeds 4 MiB",
            PngProfile::Sky => "sky PNG output exceeds 16 MiB",
        })?;
    if palette.is_some() && size.checked_mul(4) != Some(rgba_size) {
        return Err("sky PNG raw index allocation shape refused".into());
    }
    let mut buffer = match profile {
        PngProfile::Local => vec![0; size],
        PngProfile::Sky => {
            let mut buffer = sky_png_reserve(size, profile.rgba_cap())?;
            buffer.resize(size, 0);
            buffer
        }
    };
    let frame = reader
        .next_frame(&mut buffer)
        .map_err(|e| format!("local PNG frame: {e}"))?;
    reader.finish().map_err(|e| format!("local PNG end: {e}"))?;
    let samples = frame.color_type.samples();
    let row = (width as usize)
        .checked_mul(samples)
        .ok_or("PNG stride overflow")?;
    let used = row
        .checked_mul(height as usize)
        .ok_or("PNG rows overflow")?;
    if frame.width != width
        || frame.height != height
        || frame.bit_depth != png::BitDepth::Eight
        || frame.line_size != row
        || frame.buffer_size() != used
        || used > buffer.len()
        || reader.info().animation_control.is_some()
    {
        return Err("PNG output shape/stride refused".into());
    }
    if let Some(palette) = palette {
        if frame.color_type != png::ColorType::Indexed || used != size {
            return Err("sky PNG raw index frame shape refused".into());
        }
        // Retire decoder storage before allocating final RGBA. At conversion the
        // only rasters are <=pixels raw bytes and <=4*pixels RGBA, contained by
        // the existing two-RGBA logical charge (no second expanded decode).
        drop(reader);
        let rgba = palette.rgba(&buffer[..used], rgba_size, profile.rgba_cap())?;
        return Ok(Texture {
            width,
            height,
            rgba,
        });
    }
    let mut rgba = match profile {
        PngProfile::Local => Vec::with_capacity(rgba_size),
        PngProfile::Sky => sky_png_reserve(rgba_size, profile.rgba_cap())?,
    };
    for p in buffer[..used].chunks_exact(samples) {
        rgba.extend_from_slice(&match frame.color_type {
            png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
            png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
            png::ColorType::Rgb => [p[0], p[1], p[2], 255],
            png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
            _ => return Err("unsupported local PNG sample format".into()),
        });
    }
    if rgba.len() != rgba_size {
        return Err("PNG aggregate RGBA count mismatch".into());
    }
    Ok(Texture {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
#[path = "assets/sky_palette_tests.rs"]
mod sky_palette_tests;
#[cfg(test)]
#[path = "assets/sky_png_tests.rs"]
mod sky_png_tests;

pub(crate) fn decode_texture(bytes: &[u8]) -> Result<Texture> {
    if bytes.len() > 4_000_000 {
        return Err("embedded PNG exceeds 4 MB".into());
    }
    let mut decoder = png::Decoder::new_with_limits(
        std::io::Cursor::new(bytes),
        png::Limits {
            bytes: 16 * 1024 * 1024,
        },
    );
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("embedded PNG: {e}"))?;
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || info.width > 1024
        || info.height > 1024
        || info.animation_control.is_some()
    {
        return Err(
            "embedded PNG must be static, 1..1024 pixels per axis; resize and re-export".into(),
        );
    }
    let size = reader
        .output_buffer_size()
        .filter(|s| *s <= 4 * 1024 * 1024)
        .ok_or("decoded PNG budget exceeded")?;
    let mut buffer = vec![0; size];
    let frame = reader
        .next_frame(&mut buffer)
        .map_err(|e| format!("embedded PNG: {e}"))?;
    let mut rgba = Vec::with_capacity((frame.width * frame.height * 4) as usize);
    for p in buffer[..frame.buffer_size()].chunks(frame.color_type.samples()) {
        rgba.extend(match frame.color_type {
            png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
            png::ColorType::Rgb => [p[0], p[1], p[2], 255],
            png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
            png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
            _ => return Err("unsupported embedded PNG output".into()),
        });
    }
    Ok(Texture {
        width: frame.width,
        height: frame.height,
        rgba,
    })
}
