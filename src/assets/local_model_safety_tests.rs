//! Bounded qualification of the actual local-profile decoder; no product repairs.
use super::{Model, decode, decode_local_model};
#[allow(dead_code)]
#[path = "../model_safety_fixture.rs"]
mod fixture;
use fixture::*;

fn valid(doc: &str, bin: &[u8]) -> Vec<u8> {
    let bytes = glb(doc, bin);
    gltf::Gltf::from_slice(&bytes)
        .expect("structurally valid adverse fixture, not a parser-error RED");
    bytes
}
fn refused(bytes: &[u8], reason: &str) {
    match decode_local_model(bytes) {
        Ok(_) => panic!("LOCAL_GLB_REFUSAL_MISSING: {reason}"),
        Err(error) => assert!(error.contains(reason), "expected {reason:?}, got {error:?}"),
    }
}
fn accepted(bytes: &[u8]) -> Model {
    decode_local_model(bytes).unwrap_or_else(|e| panic!("LOCAL_GLB_VALID_CONTROL: {e}"))
}

#[test]
fn asymmetric_control_preserves_actual_decoder_geometry_and_material() {
    let bytes = control();
    let expected = decode(&bytes, 10.0).unwrap();
    let actual = accepted(&bytes);
    assert_eq!(actual.min, [1.0, 2.0, 3.0]);
    assert_eq!(actual.max, [5.0, 4.0, 9.0]);
    assert_eq!(actual.vertices.len(), 12);
    assert!(actual.textures.is_empty());
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&actual.vertices),
        bytemuck::cast_slice::<_, u8>(&expected.vertices)
    );
    assert!(actual.vertices.iter().all(|v| v.texture == 0
        && v.uv == [0.0; 2]
        && v.color[0] > v.color[1] * 4.0
        && v.color[2] > v.color[1] * 3.0));
    assert!(
        actual.vertices.capacity() * std::mem::size_of::<super::Vertex>()
            + std::mem::size_of::<Model>()
            <= 4 * 1024 * 1024
    );
}

#[test]
fn actual_embedded_png_uv_model_is_pack_valid_but_local_refused() {
    accepted(&control());
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[19, 37, 83, 255]).unwrap();
        writer.finish().unwrap();
    }
    let mut bin = binary();
    for uv in [[0.0_f32, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
        for n in uv {
            bin.extend_from_slice(&n.to_le_bytes());
        }
    }
    bin.extend_from_slice(&png);
    let mut doc = replace(
        &document(),
        r#""byteLength":120"#,
        &format!(r#""byteLength":{}"#, bin.len()),
    );
    doc = replace(
        &doc,
        r#""byteLength":24,"target":34963}"#,
        &format!(
            r#""byteLength":24,"target":34963}},{{"buffer":0,"byteOffset":120,"byteLength":32}},{{"buffer":0,"byteOffset":152,"byteLength":{}}}"#,
            png.len()
        ),
    );
    doc = replace(
        &doc,
        INDEX_ACCESSOR,
        &format!(
            r#"{INDEX_ACCESSOR},{{"bufferView":3,"componentType":5126,"count":4,"type":"VEC2"}}"#
        ),
    );
    doc = replace(&doc, r#""NORMAL":1"#, r#""NORMAL":1,"TEXCOORD_0":3"#);
    doc = replace(
        &doc,
        r#""metallicFactor":0"#,
        r#""baseColorTexture":{"index":0},"metallicFactor":0"#,
    );
    doc = root_field(
        &doc,
        r#""images":[{"bufferView":4,"mimeType":"image/png"}],"textures":[{"source":0}]"#,
    );
    let bytes = valid(&doc, &bin);
    let pack = decode(&bytes, 10.0).expect("real pack-valid textured positive control");
    assert_eq!(pack.textures.len(), 1);
    assert_eq!(pack.textures[0].rgba, [19, 37, 83, 255]);
    assert!(pack.vertices.iter().all(|v| v.texture == 1));
    refused(&bytes, "untextured static embedded triangles only");
}

#[test]
fn external_buffer_and_image_uris_refuse_without_resource_resolution() {
    accepted(&control());
    // Inert identifiers only: no path or URL loader is called by this test.
    let external = replace(
        &document(),
        r#""buffers":[{"byteLength":120}]"#,
        r#""buffers":[{"byteLength":120,"uri":"never-read.bin"}]"#,
    );
    refused(&valid(&external, &binary()), "external resources refused");
    for uri in ["never-read.png", "https://example.invalid/never-fetch.png"] {
        let doc = root_field(&document(), &format!(r#""images":[{{"uri":"{uri}"}}]"#));
        refused(&valid(&doc, &binary()), "external resources refused");
    }
}

#[test]
fn animation_and_declared_skin_refuse_with_static_control() {
    accepted(&control());
    let mut bin = binary();
    for t in [0.0_f32, 1.0, 2.0, 3.0] {
        bin.extend_from_slice(&t.to_le_bytes());
    }
    let mut doc = replace(&document(), r#""byteLength":120"#, r#""byteLength":136"#);
    doc = replace(
        &doc,
        r#""byteLength":24,"target":34963}"#,
        r#""byteLength":24,"target":34963},{"buffer":0,"byteOffset":120,"byteLength":16}"#,
    );
    doc = replace(
        &doc,
        INDEX_ACCESSOR,
        &format!(
            r#"{INDEX_ACCESSOR},{{"bufferView":3,"componentType":5126,"count":4,"type":"SCALAR","min":[0],"max":[3]}}"#
        ),
    );
    doc = root_field(
        &doc,
        r#""animations":[{"samplers":[{"input":3,"output":0,"interpolation":"LINEAR"}],"channels":[{"sampler":0,"target":{"node":0,"path":"translation"}}]}]"#,
    );
    refused(
        &valid(&doc, &bin),
        "untextured static embedded triangles only",
    );
    let skin = root_field(&document(), r#""skins":[{"joints":[0]}]"#);
    refused(
        &valid(&skin, &binary()),
        "untextured static embedded triangles only",
    );
}

#[test]
fn sparse_accessor_is_explicitly_refused_even_when_shared_decoder_can_read_it() {
    accepted(&control());
    let needle = r#""min":[1,2,3],"max":[5,4,9]}"#;
    let doc = replace(
        &document(),
        needle,
        r#""min":[1,2,3],"max":[5,4,9],"sparse":{"count":1,"indices":{"bufferView":2,"componentType":5123},"values":{"bufferView":0}}}"#,
    );
    let bytes = valid(&doc, &binary());
    assert_eq!(decode(&bytes, 10.0).unwrap().vertices.len(), 12);
    refused(&bytes, "accessor expansion budget refused");
}

fn unused_accessors(counts: &[usize]) -> Vec<u8> {
    let mut bin = binary();
    let extra = counts.iter().copied().max().unwrap_or(1) * 2;
    bin.resize(120 + extra, 0);
    let mut doc = replace(
        &document(),
        r#""byteLength":120"#,
        &format!(r#""byteLength":{}"#, bin.len()),
    );
    doc = replace(
        &doc,
        r#""byteLength":24,"target":34963}"#,
        &format!(
            r#""byteLength":24,"target":34963}},{{"buffer":0,"byteOffset":120,"byteLength":{extra}}}"#
        ),
    );
    let mut accessors = INDEX_ACCESSOR.to_owned();
    for count in counts {
        accessors.push_str(&format!(
            r#",{{"bufferView":3,"componentType":5123,"count":{count},"type":"SCALAR"}}"#
        ));
    }
    doc = replace(&doc, INDEX_ACCESSOR, &accessors);
    valid(&doc, &bin)
}

#[test]
fn unused_accessor_single_aggregate_and_descriptor_limits_are_inclusive() {
    accepted(&unused_accessors(&[32768]));
    refused(
        &unused_accessors(&[32769]),
        "accessor expansion budget refused",
    );
    // Existing POSITION/NORMAL/index descriptors contribute 4+4+12 = 20.
    accepted(&unused_accessors(&[32768, 32768, 32768, 32748]));
    refused(
        &unused_accessors(&[32768, 32768, 32768, 32749]),
        "accessor expansion budget refused",
    );
    accepted(&unused_accessors(&[1; 125])); // 3 + 125 = 128 accessors.
    refused(
        &unused_accessors(&[1; 126]),
        "untextured static embedded triangles only",
    );
}

#[test]
fn graph_node_and_primitive_limits_include_unused_nodes_and_meshes() {
    for (count, ok) in [(256, true), (257, false)] {
        let nodes = std::iter::once(r#"{"mesh":0}"#)
            .chain(std::iter::repeat_n("{}", count - 1))
            .collect::<Vec<_>>()
            .join(",");
        let doc = replace(
            &document(),
            r#""nodes":[{"mesh":0}]"#,
            &format!(r#""nodes":[{nodes}]"#),
        );
        let bytes = valid(&doc, &binary());
        if ok {
            accepted(&bytes);
        } else {
            refused(&bytes, "untextured static embedded triangles only");
        }
    }
    for (count, ok) in [(64, true), (65, false)] {
        // Only the original mesh is selected; the extra mesh still consumes preflight budget.
        let extra = std::iter::repeat_n(PRIMITIVE, count - 1)
            .collect::<Vec<_>>()
            .join(",");
        let doc = replace(
            &document(),
            &format!(r#""meshes":[{{"primitives":[{PRIMITIVE}]}}]"#),
            &format!(r#""meshes":[{{"primitives":[{PRIMITIVE}]}},{{"primitives":[{extra}]}}]"#),
        );
        let bytes = valid(&doc, &binary());
        if ok {
            accepted(&bytes);
        } else {
            refused(&bytes, "untextured static embedded triangles only");
        }
    }
}

#[test]
fn depth_32_is_inclusive_and_repeated_selected_node_is_refused() {
    for (depth, ok) in [(32, true), (33, false)] {
        let mut nodes: Vec<_> = (0..depth)
            .map(|i| format!(r#"{{"children":[{}]}}"#, i + 1))
            .collect();
        nodes.push(r#"{"mesh":0}"#.into());
        let doc = replace(
            &document(),
            r#""nodes":[{"mesh":0}]"#,
            &format!(r#""nodes":[{}]"#, nodes.join(",")),
        );
        let bytes = valid(&doc, &binary());
        if ok {
            accepted(&bytes);
        } else {
            refused(&bytes, "node hierarchy too deep/cyclic");
        }
    }
    let doc = replace(
        &document(),
        r#""scenes":[{"nodes":[0]}]"#,
        r#""scenes":[{"nodes":[0,0]}]"#,
    );
    refused(&valid(&doc, &binary()), "repeated node");
}

#[test]
fn all_node_instances_count_and_triangle_boundary_is_32766_not_32768() {
    for (count, ok) in [(32766_usize, true), (32768, false), (32769, false)] {
        let mut bin = binary();
        bin.truncate(96);
        for i in 0..count {
            bin.extend_from_slice(&((i % 4) as u16).to_le_bytes());
        }
        let mut doc = replace(
            &document(),
            r#""byteLength":120"#,
            &format!(r#""byteLength":{}"#, bin.len()),
        );
        doc = replace(
            &doc,
            r#""byteLength":24"#,
            &format!(r#""byteLength":{}"#, count * 2),
        );
        doc = replace(&doc, r#""count":12"#, &format!(r#""count":{count}"#));
        let bytes = valid(&doc, &bin);
        if ok {
            assert_eq!(accepted(&bytes).vertices.len(), count);
        } else if count == 32768 {
            refused(&bytes, "invalid/oversized triangles");
        } else {
            refused(&bytes, "accessor expansion budget refused");
        }
        if ok {
            // Unselected second instance must also count (65532), despite selected output 32766.
            let doc = replace(
                &doc,
                r#""nodes":[{"mesh":0}]"#,
                r#""nodes":[{"mesh":0},{"mesh":0}]"#,
            );
            refused(&valid(&doc, &bin), "exceeds 32768 emitted vertices");
        }
    }
}

#[test]
fn view_and_material_descriptor_caps_are_inclusive_even_when_unused() {
    let view = r#"{"buffer":0,"byteOffset":96,"byteLength":24,"target":34963}"#;
    let material = r#"{"pbrMetallicRoughness":{"baseColorFactor":[0.91,0.13,0.67,1],"metallicFactor":0,"roughnessFactor":1},"doubleSided":true}"#;
    for (count, ok) in [(128, true), (129, false)] {
        let views = std::iter::repeat_n(view, count - 2)
            .collect::<Vec<_>>()
            .join(",");
        let doc = replace(&document(), view, &views);
        let bytes = valid(&doc, &binary());
        if ok {
            accepted(&bytes);
        } else {
            refused(&bytes, "untextured static embedded triangles only");
        }
    }
    for (count, ok) in [(16, true), (17, false)] {
        let materials = std::iter::repeat_n(material, count)
            .collect::<Vec<_>>()
            .join(",");
        let doc = replace(&document(), material, &materials);
        let bytes = valid(&doc, &binary());
        if ok {
            accepted(&bytes);
        } else {
            refused(&bytes, "untextured static embedded triangles only");
        }
    }
}

#[test]
fn local_profile_does_not_silently_accept_default_shiny_alpha_or_emissive_materials() {
    accepted(&control());
    let implicit = replace(&document(), ",\"material\":0", "");
    refused(&valid(&implicit, &binary()), "explicit matte material");
    for (from, to) in [
        (r#""metallicFactor":0"#, r#""metallicFactor":1"#),
        (r#""roughnessFactor":1"#, r#""roughnessFactor":0.5"#),
        (
            r#""doubleSided":true"#,
            r#""doubleSided":true,"alphaMode":"BLEND""#,
        ),
        (
            r#""baseColorFactor":[0.91,0.13,0.67,1]"#,
            r#""baseColorFactor":[0.91,0.13,0.67,0.5]"#,
        ),
        (
            r#""doubleSided":true"#,
            r#""doubleSided":true,"emissiveFactor":[1,0,0]"#,
        ),
    ] {
        refused(
            &valid(&replace(&document(), from, to), &binary()),
            "opaque matte base color only",
        );
    }
}

#[test]
fn encoded_and_json_byte_caps_are_inclusive_without_broadening_container_profile() {
    let mut doc = document();
    doc.push_str(&" ".repeat(65536 - doc.len()));
    accepted(&glb(&doc, &binary()));
    doc.push_str("    ");
    refused(&glb(&doc, &binary()), "<=64 KiB JSON");
    let json_len = 2048;
    let bin_len = 4 * 1024 * 1024 - 28 - json_len;
    let mut bin = binary();
    bin.resize(bin_len, 0);
    let mut doc = replace(
        &document(),
        r#""byteLength":120"#,
        &format!(r#""byteLength":{bin_len}"#),
    );
    doc.push_str(&" ".repeat(json_len - doc.len()));
    let bytes = valid(&doc, &bin);
    assert_eq!(bytes.len(), 4 * 1024 * 1024);
    accepted(&bytes);
    bin.extend_from_slice(&[0; 4]);
    let over = replace(
        &doc,
        &format!(r#""byteLength":{bin_len}"#),
        &format!(r#""byteLength":{}"#, bin.len()),
    );
    refused(&valid(&over, &bin), "<=4 MiB");
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod held_acquisition {
    use super::*;
    use crate::local_explorer::{Explorer, MAX_BODY_BYTES, READER_CHUNK_BYTES};
    use fixture::owned::Fixture;
    use std::{
        fs,
        os::unix::fs::{OpenOptionsExt, symlink},
        sync::atomic::AtomicBool,
    };

    #[test]
    fn model_cancellation_and_changed_clone_baselines_leave_raw_recovery_independent() {
        let f = Fixture::new();
        let good = control();
        f.file("source\né.GLB", &good);
        let explorer = Explorer::new(&f.0).unwrap();
        let mut reader = explorer.open_reader(0).unwrap();
        let clone = reader.try_clone().unwrap();
        assert_eq!(
            reader.read_model_bytes(&AtomicBool::new(false)).unwrap(),
            good
        );
        assert!(
            reader
                .read_model_bytes(&AtomicBool::new(true))
                .unwrap_err()
                .contains("cancelled")
        );
        assert_eq!(reader.read_chunk(0).unwrap().body(), good);
        let mut changed = good.clone();
        changed.extend_from_slice(b"literal invalid GLB tail\0\xff");
        fs::write(f.0.join("source\né.GLB"), &changed).unwrap();
        assert!(
            reader
                .read_model_bytes(&AtomicBool::new(false))
                .unwrap_err()
                .contains("source changed")
        );
        assert!(clone.read_model_bytes(&AtomicBool::new(false)).is_err());
        assert!(reader.read_chunk(0).is_err());
        assert_eq!(reader.retry_from_zero().unwrap().body(), changed);
        assert!(clone.read_model_bytes(&AtomicBool::new(false)).is_err());
        let bytes = reader.read_model_bytes(&AtomicBool::new(false)).unwrap();
        assert_eq!(
            bytes, changed,
            "acquisition is byte-preserving, not a GLB parser"
        );
        assert!(decode_local_model(&bytes).is_err());
        assert_eq!(explorer.read_entry(0, MAX_BODY_BYTES).unwrap(), changed);
    }

    #[test]
    fn renamed_held_directory_never_reopens_display_impostor() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("original")).unwrap();
        let good = control();
        f.file("original/source.glb", &good);
        let explorer = Explorer::new(&f.0.join("original")).unwrap();
        fs::rename(f.0.join("original"), f.0.join("held")).unwrap();
        fs::create_dir(f.0.join("original")).unwrap();
        f.file("original/source.glb", b"PATH IMPOSTOR\0\xff");
        let held = explorer.open_reader(0).unwrap();
        assert_eq!(
            held.read_model_bytes(&AtomicBool::new(false)).unwrap(),
            good
        );
        assert_eq!(held.read_chunk(0).unwrap().body(), good);
        assert_eq!(
            fs::read(f.0.join("original/source.glb")).unwrap(),
            b"PATH IMPOSTOR\0\xff"
        );
    }

    #[test]
    fn exact_4mib_acquisition_and_one_byte_over_refusal_do_not_disable_raw_chunks() {
        let f = Fixture::new();
        f.file("source.glb", b"glTF\0\xff");
        let writer = fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(f.0.join("source.glb"))
            .unwrap();
        writer.set_len(MAX_BODY_BYTES).unwrap();
        let explorer = Explorer::new(&f.0).unwrap();
        let held = explorer.open_reader(0).unwrap();
        let acquired = held.read_model_bytes(&AtomicBool::new(false)).unwrap();
        assert_eq!(acquired.len(), MAX_BODY_BYTES as usize);
        assert_eq!(&acquired[..6], b"glTF\0\xff");
        assert!(acquired[6..].iter().all(|b| *b == 0));
        drop(acquired);
        writer.set_len(MAX_BODY_BYTES + 1).unwrap();
        let over = explorer.open_reader(0).unwrap();
        assert!(
            over.read_model_bytes(&AtomicBool::new(false))
                .unwrap_err()
                .contains("4 MiB")
        );
        let raw = over.read_chunk(0).unwrap();
        assert_eq!(raw.used, READER_CHUNK_BYTES);
        assert_eq!(&raw.body()[..6], b"glTF\0\xff");
        assert_eq!(over.read_chunk(MAX_BODY_BYTES).unwrap().body(), &[0]);
        assert!(explorer.read_entry(0, MAX_BODY_BYTES).is_err());
    }

    #[test]
    fn snapshot_regular_file_swapped_for_owned_symlink_is_refused_before_model_read() {
        let f = Fixture::new();
        f.file("source.glb", &control());
        let explorer = Explorer::new(&f.0).unwrap();
        fs::rename(f.0.join("source.glb"), f.0.join("held.bin")).unwrap();
        symlink("held.bin", f.0.join("source.glb")).unwrap();
        assert!(explorer.open_reader(0).is_err());
        assert!(explorer.read_entry(0, MAX_BODY_BYTES).is_err());
        assert_eq!(fs::read(f.0.join("held.bin")).unwrap(), control());
    }
}
