use hair::{render, resources};

#[test]
#[ignore = "finite real Metal GPU gate; explicitly run on qualified host"]
fn real_gpu_samples_embedded_uv_texture() {
    let model = assets::decode(&textured_glb(&png()), 3.0).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = std::path::PathBuf::from(std::env::var("TMPDIR").unwrap())
        .join(format!("hair-texture-gpu-{}", std::process::id()));
    std::fs::create_dir(&out).unwrap();
    let output = resources::smoke_output(root, &out).unwrap();
    pollster::block_on(async {
        let instance = render::instance();
        let adapter = render::adapter(&instance, None).await.unwrap();
        let mut renderer = render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [512, 512],
            &model.vertices,
        )
        .await
        .unwrap();
        renderer.set_materials(&model.textures).unwrap();
        let world = hair::World::load(root).unwrap();
        renderer
            .capture(&output, "office.png", &hair::Pose::default(), &world)
            .unwrap();
        renderer.wait().unwrap();
    });
    let mut reader = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(out.join("office.png")).unwrap(),
    ))
    .read_info()
    .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut pixels).unwrap();
    for channel in 0..3 {
        let primary = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[channel] > 100 && p[(channel + 1) % 3] < 30 && p[(channel + 2) % 3] < 30)
            .count();
        assert!(
            primary > 100,
            "texture primary channel {channel} pixels={primary}; capture {}",
            out.display()
        );
    }
    println!("TEXTURE_GPU_CAPTURE={}", out.join("office.png").display());
}

#[test]
fn pack_scene_retains_textures_for_shared_renderer() {
    let dir = std::env::temp_dir().join(format!("hair-textured-pack-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("model.glb"), textured_glb(&png())).unwrap();
    std::fs::write(dir.join("pack.toml"), "version=1\nname='Textured test'\ngrid=0.5\nroom=[-4.0,-4.0,4.0,4.0]\nspawn=[0.0,3.0]\ndocument=[0.0,1.0]\nportal=[0.0,-3.0]\nimage_at=[-1.0,2.0,-3.0]\nimage_size=[1.0,1.0]\n[[models]]\nname='object'\nfile='model.glb'\nmax_extent=3.0\n[[slots]]\nmodel='object'\nat=[0.0,0.0,0.0]\nyaw=0.0\nscale=1.0\nsolid=false\n").unwrap();
    // The old combined scene remains explicitly incompatible, not silently
    // interpreted as host placement. The v2 equivalent exercises the same
    // embedded texture/material preservation through the real theme loader.
    assert!(
        hair::theme::Theme::load(&dir.join("pack.toml"), &mut Default::default())
            .err()
            .unwrap()
            .contains("legacy combined v1")
    );
    std::fs::write(dir.join("pack.toml"), "version=2\nname='Textured test'\n[[roles]]\nrole='document'\nfile='model.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0.0,0.0,0.0]\n").unwrap();
    let theme = hair::theme::Theme::load(&dir.join("pack.toml"), &mut Default::default()).unwrap();
    assert_eq!(theme.textures.len(), 1);
    assert_eq!(theme.textures[0].rgba.len(), 16 * 16 * 4);
    assert!(theme.vertices.iter().any(|v| v.texture == 1));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "private original + finite Metal; not distributable"]
fn original_planet_real_gpu_texture_counterfactual() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut model = assets::load(&root.join("private-inputs/planet.glb"), 10.0).unwrap();
    assert_eq!(
        (model.textures[0].width, model.textures[0].height),
        (512, 512)
    );
    let center =
        glam::Vec3::from_array(std::array::from_fn(|a| (model.min[a] + model.max[a]) * 0.5));
    let extent = (0..3)
        .map(|a| model.max[a] - model.min[a])
        .fold(0.0_f32, f32::max);
    for v in &mut model.vertices {
        v.position = ((glam::Vec3::from_array(v.position) - center) * (1.8 / extent)
            + glam::Vec3::Y)
            .to_array();
    }
    let out = std::env::temp_dir().join(format!("hair-planet-gpu-{}", std::process::id()));
    std::fs::create_dir(&out).unwrap();
    let output = resources::smoke_output(root, &out).unwrap();
    pollster::block_on(async {
        let instance = render::instance();
        let adapter = render::adapter(&instance, None).await.unwrap();
        let mut renderer = render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [960, 640],
            &model.vertices,
        )
        .await
        .unwrap();
        renderer.set_materials(&model.textures).unwrap();
        let world = hair::World::load(root).unwrap();
        renderer
            .capture(&output, "office.png", &hair::Pose::default(), &world)
            .unwrap();
        for v in &mut model.vertices {
            v.texture = 0;
        }
        renderer.set_scene(&model.vertices);
        renderer
            .capture(
                &output,
                "reader-welcome.png",
                &hair::Pose::default(),
                &world,
            )
            .unwrap();
        renderer.wait().unwrap();
    });
    let textured = std::fs::read(out.join("office.png")).unwrap();
    let white = std::fs::read(out.join("reader-welcome.png")).unwrap();
    assert_ne!(
        textured, white,
        "texture counterfactual must affect GPU output"
    );
    println!(
        "PRIVATE_PLANET_GPU={} vertices={} embedded=512x512; no Blender specular/IOR/PBR fidelity",
        out.display(),
        model.vertices.len()
    );
}

#[test]
fn nonfinite_source_normals_are_rejected_before_sanitization() {
    let mut bytes = textured_glb(&png());
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    bytes[28 + json_len + 36..28 + json_len + 40].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(
        assets::decode(&bytes, 3.0).is_err(),
        "invalid source normal must not become a silent zero normal"
    );
}

fn rewrite_json(bytes: &[u8], edit: impl FnOnce(String) -> String) -> Vec<u8> {
    let n = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let mut json = edit(String::from_utf8(bytes[20..20 + n].to_vec()).unwrap()).into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let bin = &bytes[20 + n..];
    let mut out = Vec::new();
    for w in [
        0x46546c67_u32,
        2,
        (20 + json.len() + bin.len()) as u32,
        json.len() as u32,
        0x4e4f534a,
    ] {
        out.extend(w.to_le_bytes());
    }
    out.extend(json);
    out.extend(bin);
    out
}
#[test]
fn malformed_png_uv_material_and_mesh_controls_are_bounded_rejections() {
    let good = textured_glb(&png());
    let mut bad_uv = good.clone();
    let n = u32::from_le_bytes(good[12..16].try_into().unwrap()) as usize;
    bad_uv[28 + n + 72..28 + n + 76].copy_from_slice(&f32::NAN.to_le_bytes());
    let bad_cases = [
        bad_uv,
        textured_glb(b"not png"),
        textured_glb(&png()[..33]),
        rewrite_json(&good, |s| s.replace(",\"TEXCOORD_0\":2", "")),
        rewrite_json(&good, |s| {
            s.replace(
                "\"bufferView\":2,\"componentType\":5126,\"count\":3",
                "\"bufferView\":2,\"componentType\":5126,\"count\":2",
            )
        }),
        rewrite_json(&good, |s| {
            s.replace(
                "\"baseColorTexture\":{\"index\":0}",
                "\"baseColorFactor\":[2,1,1,1],\"baseColorTexture\":{\"index\":0}",
            )
        }),
        rewrite_json(&good, |s| {
            s.replace("\"indices\":3", "\"mode\":1,\"indices\":3")
        }),
        rewrite_json(&good, |s| {
            s.replace(
                "\"bufferView\":4,\"mimeType\":\"image/png\"",
                "\"uri\":\"https://invalid.example/texture.png\"",
            )
        }),
        rewrite_json(&good, |s| {
            s.replace("\"byteOffset\":104", "\"byteOffset\":999999")
        }),
        rewrite_json(&good, |s| {
            s.replace(
                "\"count\":3,\"type\":\"VEC3\"",
                "\"count\":1000000000,\"type\":\"VEC3\"",
            )
        }),
    ];
    for (i, b) in bad_cases.iter().enumerate() {
        let result = std::panic::catch_unwind(|| assets::decode(b, 3.0));
        assert!(result.is_ok(), "case {i} panicked");
        assert!(result.unwrap().is_err(), "case {i} silently accepted");
    }
    let mut huge = Vec::new();
    let mut enc = png::Encoder::new(&mut huge, 1025, 1);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .unwrap()
        .write_image_data(&vec![255; 1025 * 4])
        .unwrap();
    assert!(assets::decode(&textured_glb(&huge), 3.0).is_err());
}

use hair::assets;

// Authored synthetic UV triangle; no user model bytes in public fixtures.
fn textured_glb(png: &[u8]) -> Vec<u8> {
    let mut blob = Vec::new();
    for value in [
        -1.0_f32, -1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0,
        1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
    ] {
        blob.extend_from_slice(&value.to_le_bytes());
    }
    for index in [0_u16, 1, 2, 0] {
        blob.extend_from_slice(&index.to_le_bytes());
    }
    blob.extend(png);
    let raw_len = blob.len();
    while !blob.len().is_multiple_of(4) {
        blob.push(0);
    }
    let mut json = format!(r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{raw_len}}}],
        "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":24}},{{"buffer":0,"byteOffset":96,"byteLength":6}},{{"buffer":0,"byteOffset":104,"byteLength":{}}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[-1,-1,0],"max":[1,1,0]}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}},{{"bufferView":3,"componentType":5123,"count":3,"type":"SCALAR"}}],
        "images":[{{"bufferView":4,"mimeType":"image/png"}}],"textures":[{{"source":0}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}},"indices":3,"material":0}}]}}],"nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#, png.len()).into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut result = Vec::new();
    for word in [
        0x46546c67_u32,
        2,
        (28 + json.len() + blob.len()) as u32,
        json.len() as u32,
        0x4e4f534a,
    ] {
        result.extend(word.to_le_bytes());
    }
    result.extend(json);
    result.extend((blob.len() as u32).to_le_bytes());
    result.extend(0x004e4942_u32.to_le_bytes());
    result.extend(blob);
    result
}
fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut enc = png::Encoder::new(&mut bytes, 16, 16);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut pixels = Vec::new();
    for y in 0..16 {
        for x in 0..16 {
            pixels.extend(match (x < 8, y < 8) {
                (true, true) => [255, 0, 0, 255],
                (false, true) => [0, 255, 0, 255],
                (true, false) => [0, 0, 255, 255],
                (false, false) => [255, 255, 255, 255],
            });
        }
    }
    enc.write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
    bytes
}
#[test]
fn embedded_png_uv_base_color_model_loads() {
    let bytes = textured_glb(&png());
    let model =
        assets::decode(&bytes, 3.0).expect("embedded PNG+UV base color should load, not fallback");
    assert_eq!(model.vertices.len(), 3);
}
#[test]
#[ignore = "private original supplied model; local verification only"]
fn original_private_planet_loads() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("private-inputs/planet.glb");
    let model = assets::load(&path, 10.0).expect("actual supplied planet should load");
    assert!(!model.vertices.is_empty());
}
