//! Separate matte and strict textured room-preview admission; pack policies stay unchanged.
use super::{Model, Vertex};
use crate::Result;

const MIB: usize = 1024 * 1024;
const VERTICES: usize = 32_768;

pub fn decode_local_model(bytes: &[u8]) -> Result<Model> {
    // Bound JSON before serde/glTF construction. BIN is never interpreted as JSON.
    if bytes.len() > 4 * MIB
        || bytes.get(..8) != Some(b"glTF\x02\0\0\0")
        || bytes.len() < 20
        || u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize != bytes.len()
        || bytes[16..20] != *b"JSON"
        || u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize > 64 * 1024
    {
        return Err("local model requires GLB 2, <=4 MiB, <=64 KiB JSON; raw retained".into());
    }
    {
        let file = gltf::Gltf::from_slice(bytes).map_err(|e| e.to_string())?;
        if file.images().next().is_some()
            || file.textures().next().is_some()
            || file.extensions_used().next().is_some()
            || file.extensions_required().next().is_some()
            || file.buffers().count() != 1
            || file
                .buffers()
                .any(|b| !matches!(b.source(), gltf::buffer::Source::Bin))
            || file.animations().next().is_some()
            || file.skins().next().is_some()
            || file.nodes().count() > 256
            || file.meshes().map(|m| m.primitives().count()).sum::<usize>() > 64
            || file.accessors().count() > 128
            || file.views().count() > 128
            || file.materials().count() > 16
        {
            return Err("local model: untextured static embedded triangles only; images/textures/extensions/external resources refused; raw retained".into());
        }
        // Check counts BEFORE assets::decode collects accessors. All accessors,
        // including unused ones, count. Sparse/oversized expansion is refused.
        let mut samples = 0_usize;
        for accessor in file.accessors() {
            samples = samples
                .checked_add(accessor.count())
                .ok_or("model count overflow")?;
            if accessor.count() > VERTICES || accessor.sparse().is_some() || samples > 4 * VERTICES
            {
                return Err("local model accessor expansion budget refused".into());
            }
        }
        let mut emitted = 0_usize;
        // Count every node instance, even outside the selected scene. The shared
        // decoder additionally rejects repeated nodes/cycles and depth >32.
        for node in file.nodes() {
            if let Some(mesh) = node.mesh() {
                for p in mesh.primitives() {
                    if p.mode() != gltf::mesh::Mode::Triangles || p.morph_targets().len() != 0 {
                        return Err("local model requires static indexed triangles".into());
                    }
                    if p.material().index().is_none()
                        || p.attributes().any(|(semantic, _)| {
                            !matches!(
                                semantic,
                                gltf::Semantic::Positions | gltf::Semantic::Normals
                            )
                        })
                    {
                        return Err("local model needs explicit matte material and POSITION/NORMAL only; UV/color/skin attributes refused".into());
                    }
                    let count = p.indices().ok_or("local model needs indices")?.count();
                    emitted = emitted.checked_add(count).ok_or("model vertex overflow")?;
                    if emitted > VERTICES {
                        return Err("local model exceeds 32768 emitted vertices".into());
                    }
                }
            }
        }
        for material in file.materials() {
            let pbr = material.pbr_metallic_roughness();
            if material.alpha_mode() != gltf::material::AlphaMode::Opaque
                || pbr.base_color_factor()[3] != 1.0
                || pbr.metallic_factor() != 0.0
                || pbr.roughness_factor() != 1.0
                || pbr.base_color_texture().is_some()
                || pbr.metallic_roughness_texture().is_some()
                || material.normal_texture().is_some()
                || material.occlusion_texture().is_some()
                || material.emissive_texture().is_some()
                || material.emissive_factor() != [0.0; 3]
            {
                return Err(
                    "local model supports opaque matte base color only; raw retained".into(),
                );
            }
        }
    } // Release preflight document/BIN before the existing decoder reparses.
    // Shared 32 MiB work ledger, conservative LOGICAL payload estimate:
    // encoded 4 MiB + decoder BIN copy 4 MiB + JSON representation allowance
    // 8 MiB + output Vec growth/reallocation 3*32768*sizeof(Vertex) + twice
    // the position/normal/UV/index temporaries 32768*(32+4). Textures: ZERO
    // (refused before image decoding). Not an allocator/RSS bound or
    // interruptible decode; parser representation allowance is not a heap cap.
    let work = 4 * MIB
        + 4 * MIB
        + 8 * MIB
        + 3 * VERTICES * std::mem::size_of::<Vertex>()
        + 2 * VERTICES * (32 + 4);
    if work > 32 * MIB {
        return Err("local model shared work reservation exceeded".into());
    }
    let model = super::decode(bytes, 10.0)?;
    // Refuse an unfittable point before successful cache/Theme publication.
    // A planar model remains valid: only all-zero extents are collapsed.
    let extents = [0, 1, 2].map(|axis| model.max[axis] - model.min[axis]);
    if extents.iter().any(|extent| !extent.is_finite())
        || extents.iter().all(|extent| *extent == 0.0)
    {
        return Err("local model has invalid or all-zero bounds extents; raw retained".into());
    }
    // Charge retained capacity, not encoded length or only initialized vertices.
    let retained =
        model.vertices.capacity() * std::mem::size_of::<Vertex>() + std::mem::size_of::<Model>();
    if !model.textures.is_empty()
        || model.vertices.iter().any(|v| v.texture != 0)
        || model.vertices.len() > VERTICES
        || retained > 4 * MIB
    {
        return Err("local model decoded cache budget/material refused".into());
    }
    Ok(model)
}

/// Conservative logical retained payload, including capacity and every Arc-held
/// image even when shared. Overflow is refusal, never an understated charge.
pub fn retained_model_bytes(model: &Model) -> Option<usize> {
    let mut bytes = std::mem::size_of::<Model>()
        .checked_add(
            model
                .vertices
                .capacity()
                .checked_mul(std::mem::size_of::<Vertex>())?,
        )?
        .checked_add(
            model
                .textures
                .capacity()
                .checked_mul(std::mem::size_of::<std::sync::Arc<super::Texture>>())?,
        )?;
    for texture in &model.textures {
        bytes = bytes
            .checked_add(std::mem::size_of::<super::Texture>())?
            .checked_add(texture.rgba.capacity())?;
    }
    Some(bytes)
}

/// Local-reader dispatcher only. No path reopening and no permissive fallback
/// after textured intent. Activation and scene-material remapping are separate.
pub fn decode_local_preview_model(bytes: &[u8]) -> Result<Model> {
    let (json, bin) = preview_chunks(bytes)?;
    let textured = {
        let raw: gltf::json::Value =
            gltf::json::deserialize::from_slice(json).map_err(|e| e.to_string())?;
        refuse_raw_features(&raw)?;
        let textured = textured_intent(&raw);
        if textured {
            validate_raw_profile(&raw)?;
        }
        textured
    }; // Drop the raw JSON tree before constructing the typed document/BIN copy.
    if !textured {
        return decode_local_model(bytes);
    }
    if textured_work_bytes().filter(|n| *n <= 32 * MIB).is_none() {
        return Err("local textured model shared work reservation exceeded".into());
    }
    {
        let file = gltf::Gltf::from_slice(bytes).map_err(|e| e.to_string())?;
        validate_textured_document(&file, bin)?;
    } // Release typed preflight document and copied BIN before shared reparse.
    let model = super::decode_with_image_decoder(bytes, 10.0, decode_preview_png)?;
    let extents = [0, 1, 2].map(|axis| model.max[axis] - model.min[axis]);
    if extents.iter().any(|n| !n.is_finite())
        || extents.iter().all(|n| *n == 0.0)
        || model.vertices.len() > VERTICES
        || model.vertices.capacity() > VERTICES
        || model.textures.len() != 1
        || model.vertices.iter().any(|v| v.texture != 1)
        || retained_model_bytes(&model)
            .filter(|n| *n <= 4 * MIB)
            .is_none()
    {
        return Err("local textured model retained budget/material/bounds refused".into());
    }
    Ok(model)
}

// Logical work admission, NOT an RSS/allocator/GPU bound or decode deadline.
// Keep all terms checked, including the actual platform Vertex size.
fn textured_work_bytes() -> Option<usize> {
    let output = 3_usize
        .checked_mul(VERTICES)?
        .checked_mul(std::mem::size_of::<Vertex>())?;
    let scratch = 2_usize.checked_mul(VERTICES)?.checked_mul(32 + 4)?;
    [
        4 * MIB,
        4 * MIB,
        8 * MIB,
        output,
        scratch,
        8 * MIB,
        2 * MIB,
        64 * 1024,
    ]
    .into_iter()
    .try_fold(0_usize, |sum, term| sum.checked_add(term))
}

fn word(bytes: &[u8], at: usize) -> Result<usize> {
    let end = at.checked_add(4).ok_or("GLB offset overflow")?;
    let value = bytes.get(at..end).ok_or("truncated GLB word")?;
    Ok(u32::from_le_bytes(value.try_into().unwrap()) as usize)
}

// Exactly JSON then BIN, both four-byte aligned; no unknown/trailing chunks.
fn preview_chunks(bytes: &[u8]) -> Result<(&[u8], &[u8])> {
    if bytes.len() > 4 * MIB
        || bytes.len() < 28
        || bytes.get(..8) != Some(b"glTF\x02\0\0\0")
        || word(bytes, 8)? != bytes.len()
        || bytes.get(16..20) != Some(b"JSON")
    {
        return Err("local preview requires bounded exact GLB 2 framing".into());
    }
    let json_len = word(bytes, 12)?;
    if json_len == 0 || json_len > 64 * 1024 || !json_len.is_multiple_of(4) {
        return Err("local preview JSON length/alignment refused".into());
    }
    let json_end = 20_usize.checked_add(json_len).ok_or("GLB JSON overflow")?;
    let bin_start = json_end.checked_add(8).ok_or("GLB BIN header overflow")?;
    let kind_start = json_end.checked_add(4).ok_or("GLB BIN type overflow")?;
    if bytes.get(kind_start..bin_start) != Some(b"BIN\0") {
        return Err("local preview requires one BIN chunk".into());
    }
    let bin_len = word(bytes, json_end)?;
    if !bin_len.is_multiple_of(4) || bin_start.checked_add(bin_len) != Some(bytes.len()) {
        return Err("local preview BIN length/trailing bytes refused".into());
    }
    Ok((
        bytes.get(20..json_end).ok_or("truncated GLB JSON")?,
        bytes.get(bin_start..).ok_or("truncated GLB BIN")?,
    ))
}

fn textured_intent(value: &gltf::json::Value) -> bool {
    match value {
        gltf::json::Value::Object(object) => object.iter().any(|(key, value)| {
            ((key == "images" || key == "textures")
                && value.as_array().is_none_or(|items| !items.is_empty()))
                || key.ends_with("Texture")
                || textured_intent(value)
        }),
        gltf::json::Value::Array(items) => items.iter().any(textured_intent),
        _ => false,
    }
}

// Walk raw JSON, not typed extensions_used(): unknown/undeclared extensions and
// unused URI/unsupported records must not disappear through deserialization.
fn refuse_raw_features(value: &gltf::json::Value) -> Result<()> {
    match value {
        gltf::json::Value::Object(object) => {
            for (key, value) in object {
                if matches!(
                    key.as_str(),
                    "extensions"
                        | "extensionsUsed"
                        | "extensionsRequired"
                        | "uri"
                        | "sampler"
                        | "samplers"
                        | "animations"
                        | "skins"
                        | "skin"
                        | "targets"
                        | "weights"
                        | "camera"
                        | "cameras"
                        | "normalTexture"
                        | "occlusionTexture"
                        | "emissiveTexture"
                        | "metallicRoughnessTexture"
                ) {
                    return Err(
                        "local textured model raw extension/resource/feature refused".into(),
                    );
                }
                refuse_raw_features(value)?;
            }
        }
        gltf::json::Value::Array(items) => {
            for value in items {
                refuse_raw_features(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn fields(value: &gltf::json::Value, allowed: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .ok_or("local preview expected JSON object")?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("local textured model unsupported record/field type".into());
    }
    // Optional typed glTF fields may turn JSON null into absence. Admit their
    // raw types explicitly instead, including fields on unused records.
    for (key, value) in object {
        if key == "extras" {
            continue;
        }
        if value.is_null() {
            return Err("local textured model null field refused".into());
        }
        match key.as_str() {
            "scene" | "mesh" | "buffer" | "bufferView" | "source" | "index" | "componentType"
            | "target" | "mode" | "texCoord" => {
                value
                    .as_u64()
                    .filter(|n| *n <= u32::MAX as u64)
                    .ok_or("local preview requires unsigned integer field")?;
            }
            "byteOffset" | "byteLength" => {
                value
                    .as_u64()
                    .filter(|n| *n <= (4 * MIB) as u64)
                    .ok_or("local preview raw byte range refused")?;
            }
            "count" => {
                value
                    .as_u64()
                    .filter(|n| *n > 0 && *n <= VERTICES as u64)
                    .ok_or("local preview raw accessor count refused")?;
            }
            "byteStride" => {
                value
                    .as_u64()
                    .filter(|n| (4..=252).contains(n) && n.is_multiple_of(4))
                    .ok_or("local preview raw byte stride refused")?;
            }
            "normalized" | "doubleSided" => {
                if !value.is_boolean() {
                    return Err("local preview requires boolean field".into());
                }
            }
            "name" | "version" | "minVersion" | "generator" | "copyright" | "mimeType" | "type"
            | "alphaMode" => {
                if !value.is_string() {
                    return Err("local preview requires string field".into());
                }
            }
            "metallicFactor" | "roughnessFactor" => {
                finite_number(value)?;
            }
            "matrix" | "translation" | "rotation" | "scale" | "baseColorFactor"
            | "emissiveFactor" | "min" | "max" => {
                let numbers = value
                    .as_array()
                    .ok_or("local preview requires number array")?;
                let expected = match key.as_str() {
                    "matrix" => 16,
                    "rotation" | "baseColorFactor" => 4,
                    "translation" | "scale" | "emissiveFactor" => 3,
                    _ => numbers.len(),
                };
                if numbers.len() != expected || numbers.is_empty() || numbers.len() > 16 {
                    return Err("local preview numeric array shape refused".into());
                }
                for number in numbers {
                    finite_number(number)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn finite_number(value: &gltf::json::Value) -> Result<()> {
    let number = value
        .as_f64()
        .ok_or("local preview requires numeric value")?;
    if !number.is_finite() || !(number as f32).is_finite() {
        return Err("local preview nonfinite numeric field".into());
    }
    Ok(())
}

fn records<'a>(
    value: &'a gltf::json::Value,
    key: &str,
    cap: usize,
) -> Result<&'a [gltf::json::Value]> {
    let Some(value) = value.get(key) else {
        return Ok(&[]);
    };
    let items = value
        .as_array()
        .ok_or("local preview expected record array")?;
    if items.len() > cap {
        return Err("local preview raw record count exceeded".into());
    }
    Ok(items)
}

fn validate_raw_profile(raw: &gltf::json::Value) -> Result<()> {
    fields(
        raw,
        &[
            "asset",
            "scene",
            "scenes",
            "nodes",
            "meshes",
            "buffers",
            "bufferViews",
            "accessors",
            "images",
            "textures",
            "materials",
            "extras",
        ],
    )?;
    let asset = raw.get("asset").ok_or("missing asset")?;
    fields(
        asset,
        &["version", "minVersion", "generator", "copyright", "extras"],
    )?;
    if asset.get("version").and_then(|v| v.as_str()) != Some("2.0")
        || asset
            .get("minVersion")
            .is_some_and(|v| v.as_str() != Some("2.0"))
    {
        return Err("local preview requires glTF asset version 2.0".into());
    }
    let mut samples = 0_usize;
    for (key, cap, allowed) in [
        ("buffers", 1, &["byteLength", "name", "extras"][..]),
        (
            "bufferViews",
            128,
            &[
                "buffer",
                "byteOffset",
                "byteLength",
                "byteStride",
                "target",
                "name",
                "extras",
            ][..],
        ),
        (
            "accessors",
            128,
            &[
                "bufferView",
                "byteOffset",
                "componentType",
                "normalized",
                "count",
                "type",
                "min",
                "max",
                "name",
                "extras",
            ][..],
        ),
        (
            "images",
            1,
            &["bufferView", "mimeType", "name", "extras"][..],
        ),
        ("textures", 1, &["source", "name", "extras"][..]),
        ("scenes", 256, &["nodes", "name", "extras"][..]),
        (
            "nodes",
            256,
            &[
                "mesh",
                "children",
                "matrix",
                "translation",
                "rotation",
                "scale",
                "name",
                "extras",
            ][..],
        ),
    ] {
        for record in records(raw, key, cap)? {
            fields(record, allowed)?;
            if key == "textures" && record.get("source").and_then(|v| v.as_u64()) != Some(0) {
                return Err("local preview texture requires explicit image source zero".into());
            }
            if key == "images"
                && (record.get("bufferView").and_then(|v| v.as_u64()).is_none()
                    || record.get("mimeType").and_then(|v| v.as_str()) != Some("image/png"))
            {
                return Err("local preview requires explicit embedded PNG image record".into());
            }
            let children = match key {
                "scenes" => record.get("nodes"),
                "nodes" => record.get("children"),
                _ => None,
            };
            if let Some(children) = children {
                for child in children
                    .as_array()
                    .ok_or("local preview expected node index array")?
                {
                    child
                        .as_u64()
                        .filter(|n| *n <= u32::MAX as u64)
                        .ok_or("local preview invalid node index")?;
                }
            }
            if key == "nodes"
                && record.get("matrix").is_some()
                && ["translation", "rotation", "scale"]
                    .iter()
                    .any(|k| record.get(*k).is_some())
            {
                return Err("local preview matrix plus TRS refused".into());
            }
            if key == "accessors" {
                let count = record
                    .get("count")
                    .and_then(|v| v.as_u64())
                    .ok_or("missing accessor count")? as usize;
                samples = samples
                    .checked_add(count)
                    .ok_or("raw accessor sample overflow")?;
                if samples > 4 * VERTICES {
                    return Err("local preview raw accessor sample budget exceeded".into());
                }
                let width = match record.get("type").and_then(|v| v.as_str()) {
                    Some("SCALAR") => 1,
                    Some("VEC2") => 2,
                    Some("VEC3") => 3,
                    _ => return Err("local preview unsupported raw accessor dimensions".into()),
                };
                for key in ["min", "max"] {
                    if record
                        .get(key)
                        .is_some_and(|v| v.as_array().is_none_or(|v| v.len() != width))
                    {
                        return Err("local preview raw accessor bound dimensions refused".into());
                    }
                }
            }
        }
    }
    for key in ["buffers", "images", "textures", "materials"] {
        if records(raw, key, 1)?.len() != 1 {
            return Err(
                "local textured model needs exactly one buffer/image/texture/material".into(),
            );
        }
    }
    for material in records(raw, "materials", 1)? {
        fields(
            material,
            &[
                "pbrMetallicRoughness",
                "alphaMode",
                "doubleSided",
                "emissiveFactor",
                "name",
                "extras",
            ],
        )?;
        let pbr = material
            .get("pbrMetallicRoughness")
            .ok_or("missing textured PBR material")?;
        fields(
            pbr,
            &[
                "baseColorFactor",
                "baseColorTexture",
                "metallicFactor",
                "roughnessFactor",
                "extras",
            ],
        )?;
        fields(
            pbr.get("baseColorTexture")
                .ok_or("missing base color texture")?,
            &["index", "texCoord", "extras"],
        )?;
    }
    let mut primitives = 0_usize;
    for mesh in records(raw, "meshes", 64)? {
        fields(mesh, &["primitives", "name", "extras"])?;
        let items = records(mesh, "primitives", 64)?;
        if items.is_empty() {
            return Err("empty local preview mesh".into());
        }
        primitives = primitives
            .checked_add(items.len())
            .ok_or("primitive count overflow")?;
        if primitives > 64 {
            return Err("local preview primitive count exceeded".into());
        }
        for primitive in items {
            fields(
                primitive,
                &["attributes", "indices", "material", "mode", "extras"],
            )?;
            let attributes = primitive.get("attributes").ok_or("missing attributes")?;
            fields(attributes, &["POSITION", "NORMAL", "TEXCOORD_0"])?;
            if attributes.as_object().ok_or("invalid attributes")?.len() != 3 {
                return Err("local textured preview needs POSITION/NORMAL/TEXCOORD_0 only".into());
            }
        }
    }
    Ok(())
}

fn validate_textured_document(file: &gltf::Gltf, bin: &[u8]) -> Result<()> {
    use gltf::accessor::{DataType, Dimensions};
    if file.buffers().count() != 1
        || file.images().count() != 1
        || file.textures().count() != 1
        || file.materials().count() != 1
        || file.nodes().count() > 256
        || file.accessors().count() > 128
        || file.views().count() > 128
        || file.samplers().next().is_some()
        || file.extensions_used().next().is_some()
        || file.extensions_required().next().is_some()
        || file.animations().next().is_some()
        || file.skins().next().is_some()
    {
        return Err("local textured document profile refused".into());
    }
    let buffer = file.buffers().next().ok_or("missing BIN buffer")?;
    let declared = buffer.length();
    if !matches!(buffer.source(), gltf::buffer::Source::Bin)
        || declared > bin.len()
        || bin.len() - declared > 3
        || bin[declared..].iter().any(|b| *b != 0)
        || file.blob.as_deref() != Some(bin)
    {
        return Err("local preview declared/actual BIN boundary refused".into());
    }
    for view in file.views() {
        let end = view
            .offset()
            .checked_add(view.length())
            .ok_or("view range overflow")?;
        if view.buffer().index() != 0
            || view.length() == 0
            || end > declared
            || end > bin.len()
            || !view.offset().is_multiple_of(4)
            || view
                .stride()
                .is_some_and(|s| !(4..=252).contains(&s) || !s.is_multiple_of(4))
        {
            return Err("local preview view range/stride/alignment refused".into());
        }
    }
    let mut samples = 0_usize;
    for accessor in file.accessors() {
        validate_accessor(accessor, bin, &mut samples)?;
    }
    for texture in file.textures() {
        if texture.source().index() != 0 || texture.sampler().index().is_some() {
            return Err("local preview texture source/sampler refused".into());
        }
    }
    for image in file.images() {
        match image.source() {
            gltf::image::Source::View {
                view,
                mime_type: "image/png",
            } if view.stride().is_none() && view.target().is_none() && view.length() <= MIB => {}
            _ => return Err("local preview requires one embedded PNG image view".into()),
        }
    }
    for material in file.materials() {
        let pbr = material.pbr_metallic_roughness();
        let info = pbr
            .base_color_texture()
            .ok_or("missing local base color image")?;
        if info.texture().index() != 0
            || info.tex_coord() != 0
            || pbr.base_color_factor() != [1.0; 4]
            || material.alpha_mode() != gltf::material::AlphaMode::Opaque
            || pbr.metallic_factor() != 0.0
            || pbr.roughness_factor() != 1.0
            || pbr.metallic_roughness_texture().is_some()
            || material.normal_texture().is_some()
            || material.occlusion_texture().is_some()
            || material.emissive_texture().is_some()
            || material.emissive_factor() != [0.0; 3]
        {
            return Err("local preview requires opaque white matte textured material".into());
        }
    }
    let mut primitives = 0_usize;
    for mesh in file.meshes() {
        for p in mesh.primitives() {
            primitives = primitives
                .checked_add(1)
                .ok_or("primitive count overflow")?;
            if primitives > 64
                || p.mode() != gltf::mesh::Mode::Triangles
                || p.morph_targets().len() != 0
                || p.material().index() != Some(0)
                || p.attributes().count() != 3
            {
                return Err("local preview static indexed textured triangles only".into());
            }
            let position = p
                .get(&gltf::Semantic::Positions)
                .ok_or("missing POSITION")?;
            let normal = p.get(&gltf::Semantic::Normals).ok_or("missing NORMAL")?;
            let uv = p
                .get(&gltf::Semantic::TexCoords(0))
                .ok_or("missing TEXCOORD_0")?;
            for a in [&position, &normal] {
                if a.data_type() != DataType::F32 || a.dimensions() != Dimensions::Vec3 {
                    return Err("local preview requires float VEC3 POSITION/NORMAL".into());
                }
            }
            if uv.data_type() != DataType::F32
                || uv.dimensions() != Dimensions::Vec2
                || position.count() != normal.count()
                || position.count() != uv.count()
            {
                return Err("local preview requires matched float VEC2 UV0".into());
            }
            let indices = p.indices().ok_or("missing local triangle indices")?;
            if indices.dimensions() != Dimensions::Scalar
                || !matches!(
                    indices.data_type(),
                    DataType::U8 | DataType::U16 | DataType::U32
                )
                || !indices.count().is_multiple_of(3)
                || indices
                    .view()
                    .ok_or("missing index view")?
                    .stride()
                    .is_some()
            {
                return Err("local preview index type/count/stride refused".into());
            }
            let reader = p.reader(|_| Some(bin));
            if reader
                .read_indices()
                .ok_or("unreadable local indices")?
                .into_u32()
                .any(|i| i as usize >= position.count())
            {
                return Err("local preview index out of vertex range".into());
            }
        }
    }
    let mut emitted = 0_usize;
    for node in file.nodes() {
        let transform = glam::Mat4::from_cols_array_2d(&node.transform().matrix());
        if !transform.is_finite()
            || !transform.determinant().is_finite()
            || transform.determinant().abs() < 0.000001
        {
            return Err("local preview invalid node transform".into());
        }
        if let Some(mesh) = node.mesh() {
            for p in mesh.primitives() {
                emitted = emitted
                    .checked_add(p.indices().ok_or("missing indices")?.count())
                    .ok_or("local emitted count overflow")?;
                if emitted > VERTICES {
                    return Err("local preview emitted vertex budget exceeded".into());
                }
            }
        }
    }
    fn visit(node: gltf::Node<'_>, depth: usize, visited: &mut [bool]) -> Result<()> {
        if depth > 32 || visited[node.index()] {
            return Err("local preview deep/repeated selected node".into());
        }
        visited[node.index()] = true;
        for child in node.children() {
            visit(child, depth + 1, visited)?;
        }
        Ok(())
    }
    let scene = file
        .default_scene()
        .or_else(|| file.scenes().next())
        .ok_or("missing scene")?;
    let mut visited = vec![false; file.nodes().count()];
    for node in scene.nodes() {
        visit(node, 0, &mut visited)?;
    }
    Ok(())
}

// Validate EVERY accessor, including unused ones, without expanded collections.
// VEC2 is exclusively UV, VEC3 is geometry, unsigned SCALAR is triangle indices.
fn validate_accessor(a: gltf::Accessor<'_>, bin: &[u8], samples: &mut usize) -> Result<()> {
    use gltf::accessor::{DataType, Dimensions};
    *samples = samples
        .checked_add(a.count())
        .ok_or("accessor sample overflow")?;
    if a.count() == 0
        || a.count() > VERTICES
        || *samples > 4 * VERTICES
        || a.sparse().is_some()
        || a.normalized()
    {
        return Err("local preview accessor expansion/profile refused".into());
    }
    let (component, width) = match (a.data_type(), a.dimensions()) {
        (DataType::F32, Dimensions::Vec2) => (4_usize, 2_usize),
        (DataType::F32, Dimensions::Vec3) => (4, 3),
        (DataType::U8, Dimensions::Scalar) => (1, 1),
        (DataType::U16, Dimensions::Scalar) => (2, 1),
        (DataType::U32, Dimensions::Scalar) => (4, 1),
        _ => return Err("local preview unsupported accessor component/dimensions".into()),
    };
    let view = a.view().ok_or("local preview accessor requires view")?;
    let size = component
        .checked_mul(width)
        .ok_or("accessor element overflow")?;
    let stride = view.stride().unwrap_or(size);
    let start = view
        .offset()
        .checked_add(a.offset())
        .ok_or("accessor start overflow")?;
    let relative_end = (a.count() - 1)
        .checked_mul(stride)
        .and_then(|n| n.checked_add(size))
        .and_then(|n| n.checked_add(a.offset()))
        .ok_or("accessor range overflow")?;
    let end = view
        .offset()
        .checked_add(relative_end)
        .ok_or("accessor end overflow")?;
    if stride < size
        || !stride.is_multiple_of(component)
        || !a.offset().is_multiple_of(component)
        || !start.is_multiple_of(component)
        || relative_end > view.length()
        || end > bin.len()
        || view.target().is_some_and(|target| match target {
            gltf::buffer::Target::ArrayBuffer => width == 1,
            gltf::buffer::Target::ElementArrayBuffer => width != 1,
        })
        || (width == 1 && (view.stride().is_some() || !a.count().is_multiple_of(3)))
    {
        return Err("local preview accessor range/stride/alignment refused".into());
    }
    for i in 0..a.count() {
        let at = i
            .checked_mul(stride)
            .and_then(|n| start.checked_add(n))
            .ok_or("accessor sample offset overflow")?;
        let stop = at.checked_add(size).ok_or("accessor sample end overflow")?;
        let sample = bin.get(at..stop).ok_or("truncated accessor sample")?;
        if width > 1 {
            for bytes in sample.as_chunks::<4>().0 {
                let n = f32::from_le_bytes(*bytes);
                if !n.is_finite() || (width == 2 && !(0.0..=1.0).contains(&n)) {
                    return Err("local preview nonfinite geometry or UV outside [0,1]".into());
                }
            }
        } else {
            let index = match component {
                1 => sample[0] as usize,
                2 => u16::from_le_bytes(sample.try_into().unwrap()) as usize,
                4 => u32::from_le_bytes(sample.try_into().unwrap()) as usize,
                _ => return Err("local preview invalid index component width".into()),
            };
            if index >= VERTICES {
                return Err("local preview unused/used index exceeds vertex ceiling".into());
            }
        }
    }
    Ok(())
}

// Only this local GLB callback tightens PNG admission. Decode once using the
// unchanged strict local PNG implementation, including its CRC/entropy finish.
fn decode_preview_png(bytes: &[u8]) -> Result<super::Texture> {
    if bytes.len() > MIB
        || bytes.len() < 33
        || bytes.get(..8) != Some(b"\x89PNG\r\n\x1a\n")
        || bytes.get(8..16) != Some(b"\0\0\0\rIHDR")
    {
        return Err("local model PNG signature/first IHDR/encoded budget refused".into());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let rgba = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= MIB)
        .ok_or("local model PNG RGBA budget overflow")?;
    if !(1..=512).contains(&width) || !(1..=512).contains(&height) {
        return Err("local model PNG axes must be 1..512".into());
    }
    // Tighter aggregate metadata matches this component's 64 KiB work term.
    // The strict decoder below still owns all structure/CRC/static validation.
    let mut at = 8_usize;
    let mut metadata = 0_usize;
    while at < bytes.len() {
        let head_end = at.checked_add(8).ok_or("PNG header overflow")?;
        let header = bytes
            .get(at..head_end)
            .ok_or("truncated model PNG header")?;
        let len = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let end = at
            .checked_add(12)
            .and_then(|n| n.checked_add(len))
            .filter(|n| *n <= bytes.len())
            .ok_or("model PNG chunk range refused")?;
        if &header[4..8] != b"IDAT" {
            metadata = metadata
                .checked_add(len)
                .ok_or("model PNG metadata overflow")?;
            if metadata > 64 * 1024 {
                return Err("model PNG metadata exceeds 64 KiB".into());
            }
        }
        at = end;
    }
    let texture = super::decode_local_png(bytes)?;
    if texture.width as usize != width
        || texture.height as usize != height
        || texture.rgba.len() != rgba
        || texture
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] != 255)
    {
        return Err("local model requires opaque finished PNG pixels".into());
    }
    Ok(texture)
}
