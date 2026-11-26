// M2 witnesses: authentic safe loader, actual publisher, disposable owned fixtures.
#[test]
fn slice6a2_real_publish_partitioned_archive_admission_and_shared_byte_refusal() {
    let f = Fixture::new();
    let e = Explorer::new(&f.0).unwrap();
    assert_eq!(publish(&e, OsStr::new("saved.txt"), b"prior file", &pos(), &AtomicBool::new(false)), Outcome::Committed { synced: true });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packs/archive.toml");
    let (world, host) = crate::folder_scene::build(e.current(), 0, true).unwrap();
    let mut cache = Default::default();
    let theme = crate::theme::Theme::load_for_scene(&path, &mut cache, &host).unwrap();
    let source = select_pin_asset(&theme, "closed-bookcase").unwrap();
    let d = Decoration::from_entry(source.entry(), [-2., 0.], 0., [0.8; 3]).unwrap();
    let before = slice5a_snapshot(&f.0);
    let layout_before = read_layout(&e).unwrap();
    let mut calls = 0;
    let out = publish_pin_checked(&e, &source, &d, |candidate| {
        calls += 1;
        theme.validate_decoration_candidate_layout(candidate)
    });
    let layout = read_layout(&e).unwrap();
    let mut refreshed_host = host.clone();
    assert!(crate::placement_scene::apply(&layout, &world, &mut refreshed_host).unwrap().is_empty());
    let reload = crate::theme::Theme::load_for_scene(&path, &mut cache, &refreshed_host);
    println!("SLICE6A2 actual publish={out:?} callbacks={calls} same_snapshot_refresh={:?}", reload.as_ref().err());
    // 6A3 deliberately replaces aggregate-three, not shared resource refusal.
    // Original failing expectation retained in SLICE6A3-old-contract-slice6a2.log.
    assert_eq!(out, Outcome::Committed { synced: true });
    assert_eq!(calls, 2);
    assert_eq!(layout.positions, layout_before.positions);
    assert_eq!(layout.decorations.len(), 1);
    for entry in before { assert!(slice5a_snapshot(&f.0).contains(&entry)); }
    reload.unwrap();
    let f = Fixture::new(); let (_, theme) = slice6a2_budget_pack(&f, "encoded", true);
    let e = Explorer::new(&f.0).unwrap(); let source = select_pin_asset(&theme, "chair").unwrap();
    let before = slice5a_snapshot(&f.0);
    let out = theme.publish_pin_checked(&e, &source, &slice6a_new(&theme), |_| panic!("shared byte overflow reached routes"));
    assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("aggregate whole-pack budget")), "{out:?}");
    assert_eq!(slice5a_snapshot(&f.0), before);
    assert!(!f.0.join(".hair-layout").exists());
}

// Authored static indexed triangle, optional unused embedded PNGs and unused BIN
// padding: all are acquired/charged by the real safe loader, not fake model data.
fn slice6a2_glb(vertices: usize, images: &[(u32, u32)], encoded: Option<usize>) -> Vec<u8> {
    let mut blob = vec![];
    for v in [0.0f32,0.,0., 1.,0.,0., 0.,1.,0., 0.,0.,1., 0.,0.,1., 0.,0.,1.] { blob.extend(v.to_le_bytes()); }
    for i in 0..vertices { blob.extend(((i % 3) as u32).to_le_bytes()); }
    let mut views = String::new();
    let mut image_json = vec![];
    for (i, &(w, h)) in images.iter().enumerate() {
        let mut png = vec![];
        {
            let mut encoder = png::Encoder::new(&mut png, w, h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&[30, 90, 40, 255].repeat((w * h) as usize)).unwrap();
        }
        views.push_str(&format!(",{{\"buffer\":0,\"byteOffset\":{},\"byteLength\":{}}}", blob.len(), png.len()));
        image_json.push(format!("{{\"bufferView\":{},\"mimeType\":\"image/png\"}}", i + 3));
        blob.extend(png);
        while !blob.len().is_multiple_of(4) { blob.push(0); }
    }
    loop {
        let mut json = format!(r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":{}}}{}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5125,"count":{},"type":"SCALAR"}}],"images":[{}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2}}]}}],"nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#, blob.len(), vertices * 4, views, vertices, image_json.join(",")).into_bytes();
        while !json.len().is_multiple_of(4) { json.push(b' '); }
        let total = 28 + json.len() + blob.len();
        if let Some(target) = encoded
            && total != target {
            let next = target.checked_sub(28 + json.len()).unwrap();
            assert!(next >= 72 + vertices * 4 && next.is_multiple_of(4));
            blob.resize(next, 0);
            continue;
        }
        let mut glb = vec![];
        for n in [0x46546c67u32, 2, total as u32, json.len() as u32, 0x4e4f534a] { glb.extend(n.to_le_bytes()); }
        glb.extend(json);
        glb.extend((blob.len() as u32).to_le_bytes()); glb.extend(0x004e4942u32.to_le_bytes()); glb.extend(blob);
        return glb;
    }
}

fn slice6a2_budget_pack(f: &Fixture, category: &str, excess: bool) -> (PathBuf, crate::theme::Theme) {
    let (pack, _) = slice6a_pack(f);
    let mut manifest = std::fs::read_to_string(pack.join("pack.toml")).unwrap().replace("roles=[]", "");
    let textures = [(1024,1024), (1024,1024), (1024,834), (256 + u32::from(excess && category == "texture"),1)];
    let (a, b, surfaces) = match category {
        "encoded" => (slice6a2_glb(3, &[], Some(8_000_000)), Some(slice6a2_glb(3, &[], Some(4_000_000 + usize::from(excess) * 4))), 1),
        "texture" => (slice6a2_glb(3, &textures, None), None, 2),
        "retained" => (slice6a2_glb(30_000, &textures, Some(6_000_000)), Some(slice6a2_glb(240_000, &[], Some(1_200_000 + usize::from(excess) * 4))), 2),
        _ => unreachable!(),
    };
    std::fs::write(pack.join("chair.glb"), a).unwrap();
    if let Some(b) = b {
        std::fs::write(pack.join("unmatched.glb"), b).unwrap();
        manifest.push_str("\n[[roles]]\nrole='future-role'\nfile='unmatched.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0,0,0]\n");
    } else { manifest = manifest.replacen("name='Owned tracer'", "name='Owned tracer'\nroles=[]", 1); }
    let mut png = vec![];
    { let mut encoder = png::Encoder::new(&mut png, 1, 1); encoder.set_color(png::ColorType::Rgba); encoder.write_header().unwrap().write_image_data(&[255; 4]).unwrap(); }
    std::fs::write(pack.join("surface.png"), png).unwrap();
    for surface in ["floor", "wall"].into_iter().take(surfaces) {
        manifest.push_str(&format!("\n[[surfaces]]\nsurface='{surface}'\nfile='surface.png'\ntint=[1,1,1]\nmeters_per_tile=1.0\n"));
    }
    std::fs::write(pack.join("pack.toml"), manifest).unwrap();
    let theme = crate::theme::Theme::load(&pack.join("pack.toml"), &mut Default::default()).unwrap();
    // Prove this fixture reaches the selected category, not an earlier cap.
    let bytes = std::fs::read(pack.join("chair.glb")).unwrap();
    let a = crate::assets::decode(&bytes, 3.).unwrap();
    let mut encoded = bytes.len() * 2 + surfaces * 4_000_000;
    let mut vertices = a.vertices.len() * 2;
    let mut texture_bytes = a.textures.iter().map(|t| t.rgba.len()).sum::<usize>() * 2 + surfaces * 4 * 1024 * 1024;
    if pack.join("unmatched.glb").exists() {
        let b = std::fs::read(pack.join("unmatched.glb")).unwrap();
        encoded += b.len();
        let model = crate::assets::decode(&b, 3.).unwrap();
        vertices += model.vertices.len();
        texture_bytes += model.textures.iter().map(|t| t.rgba.len()).sum::<usize>();
    }
    let retained = encoded + vertices * std::mem::size_of::<crate::assets::Vertex>() + texture_bytes;
    match category {
        "encoded" => { assert_eq!(encoded, 24_000_000 + usize::from(excess) * 4); assert!(texture_bytes < 32_000_000 && retained < 64_000_000); }
        "texture" => { assert_eq!(texture_bytes, 32_000_000 + usize::from(excess) * 8); assert!(encoded < 24_000_000 && retained < 64_000_000); }
        "retained" => { assert_eq!(retained, 64_000_000 + usize::from(excess) * 4); assert!(encoded < 24_000_000 && texture_bytes <= 32_000_000); }
        _ => unreachable!(),
    }
    assert!(vertices <= 300_000 && a.vertices.len() <= 32766);
    println!("SLICE6A2 measured {category} excess={excess} encoded={encoded} vertices={vertices} texture_bytes={texture_bytes} retained={retained}");
    (pack, theme)
}

#[test]
fn slice6a2_actual_encoded_texture_retained_boundaries_cold_warm_commit_refresh() {
    for category in ["encoded", "texture", "retained"] {
        for excess in [false, true] {
            let f = Fixture::new();
            let (pack, _) = slice6a2_budget_pack(&f, category, excess);
            let e = Explorer::new(&f.0).unwrap();
            assert_eq!(publish(&e, OsStr::new("saved.txt"), b"prior file", &pos(), &AtomicBool::new(false)), Outcome::Committed { synced: true });
            let (world, host) = crate::folder_scene::build(e.current(), 0, true).unwrap();
            let path = pack.join("pack.toml");
            let mut cache = crate::theme::AssetCache::default();
            for warm in [false, true] {
                let theme = crate::theme::Theme::load_for_scene(&path, &mut cache, &host).unwrap();
                let source = select_pin_asset(&theme, "chair").unwrap();
                let d = slice6a_new(&theme);
                let review = theme.validate_pin_layout(&LayoutRead::default(), &source, &d);
                assert_eq!(review.is_err(), excess, "{category} warm={warm}: {review:?}");
                let before = slice5a_snapshot(&f.0);
                if excess {
                    let out = publish_pin_checked(&e, &source, &d, |candidate| theme.validate_decoration_candidate_layout(candidate));
                    assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("aggregate whole-pack budget")), "{category}: {out:?}");
                    assert_eq!(slice5a_snapshot(&f.0), before);
                    let mut proposed_host = host.clone();
                    proposed_host.decorations.insert(d.placement_id.clone(), source.decoration(&d).unwrap());
                    assert!(crate::theme::Theme::load_for_scene(&path, &mut cache, &proposed_host).err().unwrap().contains("aggregate whole-pack budget"));
                } else if warm {
                    let mesh = source.loaded.model.clone();
                    let out = theme.publish_pin_checked(&e, &source, &d, |candidate| crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0));
                    assert_eq!(out, Outcome::Committed { synced: true }, "{category}");
                    let reopened = read_layout(&Explorer::new(&f.0).unwrap()).unwrap();
                    let mut refreshed_host = host.clone();
                    assert!(crate::placement_scene::apply(&reopened, &world, &mut refreshed_host).unwrap().is_empty());
                    for cold in [false, true] {
                        let mut cold_cache = Default::default();
                        let refreshed = crate::theme::Theme::load_for_scene(&path, if cold { &mut cold_cache } else { &mut cache }, &refreshed_host).unwrap();
                        refreshed.validate_decoration_candidate_layout(&reopened).unwrap();
                        let retained = reopened.decorations[0].resolve(&crate::decoration::Catalog::default()).unwrap().1.model.clone();
                        assert_eq!(bytemuck::cast_slice::<_, u8>(&mesh.vertices), bytemuck::cast_slice::<_, u8>(&retained.vertices));
                        assert_eq!(mesh.textures.len(), retained.textures.len());
                        for (a,b) in mesh.textures.iter().zip(&retained.textures) { assert_eq!((a.width,a.height,&a.rgba), (b.width,b.height,&b.rgba)); }
                    }
                }
                println!("SLICE6A2 boundary category={category} excess={excess} warm={warm} admitted={}", !excess);
            }
        }
    }
}
