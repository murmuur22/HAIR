use hair::{
    authoring::Project,
    decoration::{Asset, Defaults, Entry, Spec},
    theme::Theme,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture(label: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("hair-7a-{label}-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    println!("SLICE7A_FIXTURE={}", base.display());
    base
}
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
fn chair_spec() -> Spec {
    Spec {
        assets: vec![Asset {
            id: "chair-art".into(),
            file: "chair.glb".into(),
            max_extent: 10.0,
            scale: 0.8,
            yaw_degrees: 25.0,
            pivot: [0.1, 0.0, -0.2],
        }],
        entries: vec![Entry {
            id: "reading-chair".into(),
            tag: "hair.seating.chair".into(),
            asset: "chair-art".into(),
            name: "Reading chair".into(),
            defaults: Defaults { collision: true },
        }],
    }
}
fn write_project(base: &Path, spec: &Spec) -> PathBuf {
    #[derive(serde::Serialize)]
    struct Input<'a> {
        version: u32,
        active_document: bool,
        assets: Vec<u8>,
        decorations: &'a Spec,
    }
    let text = toml::to_string(&Input {
        version: 4,
        active_document: true,
        assets: vec![],
        decorations: spec,
    })
    .unwrap();
    let path = base.join("project.toml");
    fs::write(&path, text).unwrap();
    path
}
fn same_catalog(a: &Theme, b: &Theme) {
    assert_eq!(a.decorations.entries, b.decorations.entries);
    assert_eq!(a.decorations.assets.len(), b.decorations.assets.len());
    for asset in &a.decorations.assets {
        let other = b.decorations.asset(&asset.spec.id).unwrap();
        assert_eq!(asset.spec, other.spec);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&asset.model.vertices),
            bytemuck::cast_slice::<_, u8>(&other.model.vertices)
        );
        assert_eq!(asset.model.min, other.model.min);
        assert_eq!(asset.model.max, other.model.max);
        assert_eq!(asset.model.textures.len(), other.model.textures.len());
        for (a, b) in asset.model.textures.iter().zip(&other.model.textures) {
            assert_eq!((a.width, a.height, &a.rgba), (b.width, b.height, &b.rgba));
        }
    }
    assert_eq!(
        a.host, b.host,
        "catalog collision defaults are metadata only"
    );
}

#[test]
fn selected_glb_explicit_catalog_import_save_delete_reopen_visitor_export() {
    let base = fixture("import-tracer");
    let source = base.join("selected-chair.glb");
    fs::copy(root().join("assets/kenney/chairDesk.glb"), &source).unwrap();
    let original = fs::read(&source).unwrap();
    let spec = chair_spec();
    let mut p = Project::blank();
    assert!(p.catalog().is_none());
    let preview = p
        .import_catalog_asset(spec.assets[0].clone(), spec.entries[0].clone(), &source)
        .unwrap();
    assert_eq!(p.catalog().unwrap().assets, spec.assets);
    assert_eq!(preview.decorations.entries, spec.entries);
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    assert_eq!(fs::read(&source).unwrap(), original);
    fs::remove_file(&source).unwrap();
    // Even the in-memory author can still export after original deletion.
    p.export(&base.join("memory-export"), &[]).unwrap();
    let reopened = Project::open(&saved).unwrap();
    let pack = reopened.export(&base.join("visitor"), &[]).unwrap();
    let visitor = Theme::load(&pack, &mut Default::default()).unwrap();
    same_catalog(&preview, &visitor);
    assert_eq!(fs::read(base.join("visitor/chair.glb")).unwrap(), original);
    println!("SLICE7A_IMPORT_TRACER_PACK={}", pack.display());
}

#[test]
fn shared_catalog_editable_reopen_export_survives_source_deletion() {
    let base = fixture("first-tracer");
    let source = base.join("selected");
    fs::create_dir(&source).unwrap();
    let original = fs::read(root().join("assets/kenney/chairDesk.glb")).unwrap();
    fs::write(source.join("chair.glb"), &original).unwrap();
    let spec = chair_spec();
    let input = write_project(&source, &spec);
    let input_before = fs::read(&input).unwrap();
    let p = Project::open(&input).expect("editable v4 must accept the explicit shared catalog");
    let preview = p.preview().unwrap();
    assert_eq!(preview.decorations.entries, spec.entries);
    assert_eq!(preview.decorations.assets[0].spec, spec.assets[0]);
    assert!(!p.solid);
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    assert_eq!(fs::read(&input).unwrap(), input_before);
    assert_eq!(fs::read(source.join("chair.glb")).unwrap(), original);
    fs::remove_file(source.join("chair.glb")).unwrap(); // disposable selected copy only
    let reopened = Project::open(&saved).unwrap();
    let pack = reopened.export(&base.join("export"), &[]).unwrap();
    let manifest: toml::Value = toml::from_str(&fs::read_to_string(&pack).unwrap()).unwrap();
    assert_eq!(manifest["version"].as_integer(), Some(3));
    let visitor = Theme::load(&pack, &mut Default::default()).unwrap();
    same_catalog(&preview, &visitor);
    assert_eq!(fs::read(base.join("export/chair.glb")).unwrap(), original);
    println!("SLICE7A_TRACER_PACK={}", pack.display());
}

fn imported_chair(base: &Path) -> Project {
    let selected = base.join("selected.glb");
    fs::copy(root().join("assets/kenney/chairDesk.glb"), &selected).unwrap();
    let s = chair_spec();
    let mut p = Project::blank();
    p.import_catalog_asset(s.assets[0].clone(), s.entries[0].clone(), &selected)
        .unwrap();
    p
}
fn catalog_text(p: &Project) -> String {
    toml::to_string(p.catalog().unwrap()).unwrap()
}
fn choice(id: &str, asset: &str) -> Entry {
    Entry {
        id: id.into(),
        tag: format!("hair.choice.{id}"),
        asset: asset.into(),
        name: id.into(),
        defaults: Defaults::default(),
    }
}

#[test]
fn second_choice_reuse_stable_edits_rebind_and_removal_are_complete_candidates() {
    let base = fixture("choices");
    let mut p = imported_chair(&base);
    let desk = base.join("selected-desk.glb");
    fs::copy(root().join("assets/kenney/desk.glb"), &desk).unwrap();
    let mut asset = chair_spec().assets[0].clone();
    asset.id = "desk-art".into();
    asset.file = "desk.glb".into();
    p.import_catalog_asset(asset, choice("writing-desk", "desk-art"), &desk)
        .unwrap();
    let mut spec = p.catalog().unwrap().clone();
    spec.entries.push(choice("spare-chair", "chair-art"));
    p.set_catalog(spec).unwrap();
    let original = p.preview().unwrap();
    let mut edit = p.catalog().unwrap().clone();
    edit.entries.reverse();
    edit.assets.reverse();
    edit.entries
        .iter_mut()
        .find(|e| e.id == "reading-chair")
        .unwrap()
        .name = "Green reading chair".into();
    let art = edit
        .assets
        .iter_mut()
        .find(|a| a.id == "chair-art")
        .unwrap();
    art.scale = 0.5;
    art.yaw_degrees = -75.0;
    art.pivot = [-0.2, 0.1, 0.3];
    let preview = p.set_catalog(edit).unwrap();
    assert_eq!(
        preview
            .decorations
            .entries
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["reading-chair", "spare-chair", "writing-desk"]
    );
    for id in ["reading-chair", "spare-chair", "writing-desk"] {
        let a = original.decorations.entry(id).unwrap();
        let b = preview.decorations.entry(id).unwrap();
        assert_eq!(
            (&a.id, &a.tag, &a.asset, a.defaults),
            (&b.id, &b.tag, &b.asset, b.defaults)
        );
    }
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    let reopened = Project::open(&saved).unwrap();
    let pack = reopened.export(&base.join("export"), &[]).unwrap();
    let visitor = Theme::load(&pack, &mut Default::default()).unwrap();
    same_catalog(&preview, &visitor);
    let d = hair::placement_store::Decoration::from_entry(
        preview.decorations.entry("reading-chair").unwrap(),
        [0.0, 0.0],
        0.3,
        [1.0, 1.5, 1.0],
    )
    .unwrap();
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&preview.decoration_preview(&d).unwrap()),
        bytemuck::cast_slice::<_, u8>(&visitor.decoration_preview(&d).unwrap())
    );
    // A dangling removal is refused, not partially applied.
    let before = catalog_text(&p);
    let mut bad = p.catalog().unwrap().clone();
    bad.assets.retain(|a| a.id != "chair-art");
    assert!(p.set_catalog(bad).is_err());
    assert_eq!(catalog_text(&p), before);
    // Explicit atomic rebind keeps choice IDs/tags while retiring one asset.
    let mut rebind = p.catalog().unwrap().clone();
    for e in &mut rebind.entries {
        if e.asset == "chair-art" {
            e.asset = "desk-art".into();
        }
    }
    rebind.assets.retain(|a| a.id != "chair-art");
    p.set_catalog(rebind).unwrap();
    p.remove_catalog_entry("spare-chair").unwrap();
    assert_eq!(p.catalog().unwrap().assets.len(), 1);
    p.remove_catalog_asset("desk-art").unwrap();
    assert!(p.catalog().unwrap().entries.is_empty());
    // Removing the last valid catalog choice now gives an explicitly missing
    // Follow-theme preview, not a whole-folder error or arbitrary substitute.
    let empty_preview = p.preview().unwrap();
    assert!(empty_preview.decorations.entries.is_empty());
    assert!(empty_preview.decorations.assets.is_empty());
    assert!(d.artwork(&empty_preview.decorations).unwrap().is_none());
    let placeholder = empty_preview.decoration_preview(&d).unwrap();
    assert!(placeholder.len() > 36); // finite gray cuboid plus missing-art labels
    assert!(placeholder[..36].iter().all(|v|
        v.color == [0.5; 3] && v.texture == 0 && v.position.iter().all(|n| n.is_finite())));
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&placeholder),
        bytemuck::cast_slice::<_, u8>(&preview.decoration_preview(&d).unwrap()),
    );
    assert!(p.remove_catalog_entry("absent").is_err());
    assert!(p.remove_catalog_asset("absent").is_err());
    let empty = p.save_new(&base.join("empty-saved"), &[]).unwrap();
    let empty_text = fs::read_to_string(&empty).unwrap();
    let empty_value: toml::Value = toml::from_str(&empty_text).unwrap();
    assert_eq!(empty_value["version"].as_integer(), Some(4));
    let empty_pack = p.export(&base.join("empty-export"), &[]).unwrap();
    let empty_value: toml::Value =
        toml::from_str(&fs::read_to_string(empty_pack).unwrap()).unwrap();
    assert_eq!(empty_value["version"].as_integer(), Some(3));
    assert!(
        Project::open(&empty)
            .unwrap()
            .catalog()
            .unwrap()
            .assets
            .is_empty()
    );
    assert!(!base.join("empty-saved/chair.glb").exists());
    assert!(!base.join("empty-saved/desk.glb").exists());
}

#[test]
fn duplicate_ids_tags_missing_refs_unused_assets_and_invalid_fit_retain_last_good() {
    let base = fixture("candidate-refusals");
    let mut p = imported_chair(&base);
    let good = p.catalog().unwrap().clone();
    let before = catalog_text(&p);
    let preview = p.preview().unwrap();
    let mut bad = Vec::new();
    let mut s = good.clone();
    s.assets.push(s.assets[0].clone());
    bad.push(s);
    let mut s = good.clone();
    s.entries.push(s.entries[0].clone());
    bad.push(s);
    let mut s = good.clone();
    let mut e = choice("another", "chair-art");
    e.tag = s.entries[0].tag.clone();
    s.entries.push(e);
    bad.push(s);
    let mut s = good.clone();
    s.entries[0].asset = "missing".into();
    bad.push(s);
    let mut s = good.clone();
    let mut a = s.assets[0].clone();
    a.id = "unused".into();
    s.assets.push(a);
    bad.push(s);
    let mut s = good.clone();
    s.assets[0].file = "unacquired.glb".into();
    bad.push(s);
    let mut s = good.clone();
    s.entries[0].name.clear();
    bad.push(s);
    let mut s = good.clone();
    s.entries[0].tag = "hair..invalid".into();
    bad.push(s);
    for extent in [0.0, 10.1, f32::NAN, f32::INFINITY, 0.001] {
        let mut s = good.clone();
        s.assets[0].max_extent = extent;
        bad.push(s);
    }
    for scale in [0.0, 4.1, f32::NAN, f32::INFINITY] {
        let mut s = good.clone();
        s.assets[0].scale = scale;
        bad.push(s);
    }
    for yaw in [-180.1, 180.1, f32::NAN] {
        let mut s = good.clone();
        s.assets[0].yaw_degrees = yaw;
        bad.push(s);
    }
    for pivot in [[3.1, 0.0, 0.0], [0.0, f32::NAN, 0.0]] {
        let mut s = good.clone();
        s.assets[0].pivot = pivot;
        bad.push(s);
    }
    for s in bad {
        assert!(p.set_catalog(s).is_err());
        assert_eq!(catalog_text(&p), before);
        same_catalog(&preview, &p.preview().unwrap());
    }
    // Exactly 32 explicit choices fit, the 33rd cannot become last-good.
    let mut s = good.clone();
    for n in 1..32 {
        s.entries.push(choice(&format!("choice-{n}"), "chair-art"));
    }
    p.set_catalog(s.clone()).unwrap();
    let before = catalog_text(&p);
    s.entries.push(choice("overflow", "chair-art"));
    assert!(p.set_catalog(s).is_err());
    assert_eq!(catalog_text(&p), before);
    let saved = p.save_new(&base.join("thirty-two"), &[]).unwrap();
    assert_eq!(
        Project::open(&saved)
            .unwrap()
            .catalog()
            .unwrap()
            .entries
            .len(),
        32
    );
}

#[test]
fn source_replacement_is_explicit_validated_and_independent_of_equal_bytes() {
    let base = fixture("source-replace");
    let mut p = imported_chair(&base);
    let before = catalog_text(&p);
    let old = p.preview().unwrap();
    let bad = base.join("bad.glb");
    fs::write(&bad, b"SYNTHETIC INPUT: not a GLB").unwrap();
    let error = p.replace_catalog_source("chair-art", &bad).err().unwrap();
    assert!(
        error.contains("Preview validation failed; no public output created"),
        "{error}"
    );
    assert!(!error.contains("partial NEW output retained"), "{error}");
    assert_eq!(catalog_text(&p), before);
    same_catalog(&old, &p.preview().unwrap());
    let desk = base.join("desk-input.glb");
    fs::copy(root().join("assets/kenney/desk.glb"), &desk).unwrap();
    let replaced = p.replace_catalog_source("chair-art", &desk).unwrap();
    assert_eq!(
        catalog_text(&p),
        before,
        "source replacement must not regenerate IDs or fits"
    );
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&old.decorations.assets[0].model.vertices),
        bytemuck::cast_slice::<_, u8>(&replaced.decorations.assets[0].model.vertices)
    );
    assert!(p.replace_catalog_source("missing", &desk).is_err());
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    assert_eq!(
        fs::read(base.join("saved/chair.glb")).unwrap(),
        fs::read(&desk).unwrap()
    );
    same_catalog(
        &replaced,
        &Project::open(&saved).unwrap().preview().unwrap(),
    );
}

#[test]
fn catalog_sources_paths_profiles_symlinks_oversize_and_outputs_refuse_without_replacement() {
    use std::os::unix::fs::symlink;
    let base = fixture("paths");
    let mut p = imported_chair(&base);
    let before = catalog_text(&p);
    let bytes = fs::read(base.join("selected.glb")).unwrap();
    let linked = base.join("linked.glb");
    symlink(base.join("selected.glb"), &linked).unwrap();
    symlink(&base, base.join("linked-dir")).unwrap();
    let large = base.join("large.glb");
    fs::File::create(&large)
        .unwrap()
        .set_len(8_000_001)
        .unwrap();
    let bad = base.join("bad.glb");
    fs::write(&bad, b"SYNTHETIC INPUT: malformed GLB").unwrap();
    let wrong = base.join("model.gltf");
    fs::write(&wrong, &bytes).unwrap();
    for source in [
        linked,
        base.join("linked-dir/selected.glb"),
        base.join("missing/../selected.glb"),
        large,
        bad,
        wrong,
        base.join("missing.glb"),
    ] {
        assert!(
            p.replace_catalog_source("chair-art", &source).is_err(),
            "{}",
            source.display()
        );
        assert_eq!(catalog_text(&p), before);
    }
    for file in [
        "../chair.glb",
        "/chair.glb",
        "https://invalid/chair.glb",
        "a\\chair.glb",
        "nested/chair.glb",
        "model.glb",
    ] {
        let mut s = p.catalog().unwrap().clone();
        s.assets[0].file = file.into();
        assert!(p.set_catalog(s).is_err(), "{file}");
    }
    let protected = base.join("protected");
    fs::create_dir(&protected).unwrap();
    let existing = base.join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("sentinel"), b"untouched").unwrap();
    symlink(&existing, base.join("output-link")).unwrap();
    for output in [
        existing.clone(),
        base.join("selected.glb"),
        base.join("output-link"),
        protected.join("new"),
    ] {
        assert!(
            p.save_new(&output, std::slice::from_ref(&protected))
                .is_err()
        );
        assert!(p.export(&output, std::slice::from_ref(&protected)).is_err());
    }
    assert_eq!(fs::read(existing.join("sentinel")).unwrap(), b"untouched");
    assert_eq!(fs::read(base.join("selected.glb")).unwrap(), bytes);
    assert!(!protected.join("new").exists());
    assert_eq!(catalog_text(&p), before);
}

#[test]
fn role_and_catalog_share_three_source_allowance_without_content_dedup_or_pin_borrowing() {
    let base = fixture("model-budget");
    let role = base.join("role.glb");
    let source = base.join("chair-input.glb");
    fs::copy(root().join("assets/kenney/desk.glb"), &role).unwrap();
    fs::copy(root().join("assets/kenney/desk.glb"), &source).unwrap();
    let mut p = Project::import(&role).unwrap();
    for n in 0..2 {
        let mut a = chair_spec().assets[0].clone();
        a.id = format!("art-{n}");
        a.file = format!("chair-{n}.glb");
        p.import_catalog_asset(a.clone(), choice(&format!("choice-{n}"), &a.id), &source)
            .unwrap();
    }
    let before = catalog_text(&p);
    let good = p.preview().unwrap();
    let mut fourth = chair_spec().assets[0].clone();
    fourth.id = "fourth".into();
    fourth.file = "fourth.glb".into();
    let error = p
        .import_catalog_asset(fourth, choice("fourth", "fourth"), &source)
        .err()
        .unwrap();
    assert!(error.contains("aggregate whole-pack budget"), "{error}");
    assert_eq!(catalog_text(&p), before);
    same_catalog(&good, &p.preview().unwrap());
    // Explicit catalog-to-catalog file reference shares only the retained closure.
    let mut spec = p.catalog().unwrap().clone();
    let mut alias = spec.assets[0].clone();
    alias.id = "alias-art".into();
    spec.assets.push(alias);
    spec.entries.push(choice("alias-choice", "alias-art"));
    let theme = p.set_catalog(spec).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &theme.decorations.asset("art-0").unwrap().model,
        &theme.decorations.asset("alias-art").unwrap().model
    ));
    assert!(!std::sync::Arc::ptr_eq(
        &theme.decorations.asset("art-0").unwrap().model,
        &theme.decorations.asset("art-1").unwrap().model
    ));
    let mut too_many = p.catalog().unwrap().clone();
    let mut a = too_many.assets[0].clone();
    a.id = "overflow".into();
    too_many.assets.push(a);
    too_many.entries.push(choice("overflow", "overflow"));
    assert!(p.set_catalog(too_many).is_err());
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    let reopened = Project::open(&saved).unwrap();
    assert_eq!(
        reopened.source().file_name().unwrap(),
        "model.glb",
        "equal catalog bytes must never choose the role's saved filename"
    );
    let pack = reopened.export(&base.join("export"), &[]).unwrap();
    let mut cache = Default::default();
    Theme::load(&pack, &mut cache).unwrap();
    assert_eq!(cache.loads, 3);
    Theme::load(&pack, &mut cache).unwrap();
    assert_eq!(cache.loads, 3);
}

// SYNTHETIC INPUT only: repeated indexed triangles exercise actual decoder and
// shared decoded-resource refusal. No rendered/output/decoder success is faked.
fn synthetic_glb(indices: usize, external: bool) -> Vec<u8> {
    synthetic_padded_glb(indices, external, 0)
}
fn synthetic_padded_glb(indices: usize, external: bool, padding: usize) -> Vec<u8> {
    let mut bin = Vec::new();
    for v in [
        0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
    ] {
        bin.extend(v.to_le_bytes());
    }
    for i in 0..indices as u32 {
        bin.extend((i % 3).to_le_bytes());
    }
    bin.resize(bin.len() + padding, 0);
    let uri = if external {
        ",\"uri\":\"outside.bin\""
    } else {
        ""
    };
    let mut json = format!(r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{}{uri}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":{}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5125,"count":{indices},"type":"SCALAR"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2}}]}}],"nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#, bin.len(), indices * 4).into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut glb = Vec::new();
    for n in [
        0x46546c67u32,
        2,
        (28 + json.len() + bin.len()) as u32,
        json.len() as u32,
        0x4e4f534a,
    ] {
        glb.extend(n.to_le_bytes());
    }
    glb.extend(json);
    glb.extend((bin.len() as u32).to_le_bytes());
    glb.extend(0x004e4942u32.to_le_bytes());
    glb.extend(bin);
    glb
}

#[test]
fn actual_decoder_and_complete_shared_vertex_budget_refuse_catalog_candidate() {
    let base = fixture("decoded-budget-synthetic-input");
    let source = base.join("synthetic-large.glb");
    let bytes = synthetic_glb(180000, false);
    assert_eq!(
        hair::assets::decode(&bytes, 10.0).unwrap().vertices.len(),
        180000
    );
    fs::write(&source, &bytes).unwrap();
    let mut p = Project::import(&source).unwrap();
    let old = p.preview().unwrap();
    let spec = chair_spec();
    let error = p
        .import_catalog_asset(spec.assets[0].clone(), spec.entries[0].clone(), &source)
        .err()
        .unwrap();
    assert!(error.contains("aggregate whole-pack budget"), "{error}");
    assert!(p.catalog().is_none());
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&old.vertices),
        bytemuck::cast_slice::<_, u8>(&p.preview().unwrap().vertices)
    );
    let external = base.join("synthetic-external.glb");
    fs::write(&external, synthetic_glb(3, true)).unwrap();
    let error = p
        .import_catalog_asset(spec.assets[0].clone(), spec.entries[0].clone(), &external)
        .err()
        .unwrap();
    assert!(error.contains("no external resources"), "{error}");
    assert!(p.catalog().is_none());
    assert_eq!(fs::read(&source).unwrap(), bytes);
}

#[test]
fn surface_and_catalog_encoded_charges_share_one_complete_appearance_budget() {
    let base = fixture("surface-budget-synthetic-input");
    let source = base.join("synthetic-padded.glb");
    let bytes = synthetic_padded_glb(3, false, 7_400_000);
    assert_eq!(
        hair::assets::decode(&bytes, 10.0).unwrap().vertices.len(),
        3
    );
    fs::write(&source, &bytes).unwrap();
    let mut p = Project::import(&source).unwrap();
    for n in 0..2 {
        let mut a = chair_spec().assets[0].clone();
        a.id = format!("art-{n}");
        a.file = format!("art-{n}.glb");
        p.import_catalog_asset(a.clone(), choice(&format!("choice-{n}"), &a.id), &source)
            .unwrap();
    }
    let before = catalog_text(&p);
    let old = p.preview().unwrap();
    // SYNTHETIC INPUT: actual PNG encoder, actual surface loader. Even a tiny
    // surface carries the SAME existing conservative four-MB encoded charge.
    let image = base.join("synthetic.png");
    let mut encoder = png::Encoder::new(fs::File::create(&image).unwrap(), 1, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[20, 80, 40, 255])
        .unwrap();
    let error = p.import_surface("wall", &image).err().unwrap();
    assert!(error.contains("aggregate whole-pack budget"), "{error}");
    assert!(p.surfaces().is_empty());
    assert_eq!(catalog_text(&p), before);
    same_catalog(&old, &p.preview().unwrap());
    assert_eq!(fs::read(&source).unwrap(), bytes);
}

#[test]
fn editable_catalog_unknown_fields_versions_and_manifest_bound_refuse_without_rewriting() {
    let base = fixture("strict-schema");
    fs::copy(
        root().join("assets/kenney/chairDesk.glb"),
        base.join("chair.glb"),
    )
    .unwrap();
    let path = write_project(&base, &chair_spec());
    let original = fs::read_to_string(&path).unwrap();
    let value: toml::Value = toml::from_str(&original).unwrap();
    for level in 0..5 {
        let mut bad = value.clone();
        let target = match level {
            0 => &mut bad,
            1 => &mut bad["decorations"],
            2 => &mut bad["decorations"]["assets"][0],
            3 => &mut bad["decorations"]["entries"][0],
            _ => &mut bad["decorations"]["entries"][0]["defaults"],
        };
        target
            .as_table_mut()
            .unwrap()
            .insert("unknown".into(), true.into());
        let text = toml::to_string(&bad).unwrap();
        fs::write(&path, &text).unwrap();
        assert!(Project::open(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }
    for version in [2, 3] {
        let mut bad = value.clone();
        bad["version"] = version.into();
        fs::write(&path, toml::to_string(&bad).unwrap()).unwrap();
        assert!(Project::open(&path).is_err());
    }
    fs::write(&path, original.clone() + &" ".repeat(16385)).unwrap();
    assert!(Project::open(&path).err().unwrap().contains("16384"));
    fs::write(&path, &original).unwrap();
    assert!(Project::open(&path).is_ok());
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn legacy_role_only_v2_v3_v4_remain_catalog_free_and_byte_compatible_in_export() {
    let base = fixture("legacy");
    let model = base.join("model.glb");
    fs::copy(root().join("assets/kenney/desk.glb"), &model).unwrap();
    let old_v2 = "version=2\nasset='model.glb'\noriginal_source='retained'\n[edit]\npivot=[0,0,0]\nyaw_degrees=25.0\nscale=1.0\ndocument_role=false\n";
    let path = base.join("project.toml");
    fs::write(&path, old_v2).unwrap();
    let p = Project::open(&path).unwrap();
    assert!(p.catalog().is_none());
    let a = p.export(&base.join("before"), &[]).unwrap();
    assert_eq!(
        fs::read_to_string(&a).unwrap(),
        "version=2\nname='Workshop object'\n[[roles]]\nrole='decoration'\nfile='model.glb'\nmax_extent=10.0\npivot=[0.0, 0.0, 0.0]\nyaw_degrees=25\nscale=1\n"
    );
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    let v3_text = fs::read_to_string(&saved).unwrap();
    assert_eq!(
        v3_text,
        format!(
            "version = 3\nactive_document = false\n\n[[assets]]\nversion = 2\nasset = \"model.glb\"\noriginal_source = \"{}\"\n\n[assets.edit]\npivot = [0.0, 0.0, 0.0]\nyaw_degrees = 25.0\nscale = 1.0\ndocument_role = false\n",
            model.display()
        )
    );
    let v3: toml::Value = toml::from_str(&v3_text).unwrap();
    assert_eq!(v3["version"].as_integer(), Some(3));
    assert!(v3.get("decorations").is_none());
    let mut reopened = Project::open(&saved).unwrap();
    assert!(reopened.catalog().is_none());
    assert_eq!(reopened.edit(), p.edit());
    assert!(
        !reopened.document_role,
        "legacy decoration role must not become a catalog choice"
    );
    let b = reopened.export(&base.join("after"), &[]).unwrap();
    assert_eq!(fs::read(a).unwrap(), fs::read(b).unwrap());
    assert_eq!(
        fs::read(base.join("before/model.glb")).unwrap(),
        fs::read(base.join("after/model.glb")).unwrap()
    );
    assert_eq!(fs::read_to_string(&saved).unwrap(), v3_text);
    assert_eq!(fs::read_to_string(&path).unwrap(), old_v2);
    // Existing v4 surface path remains v4 with no catalog field or reinterpretation.
    reopened
        .set_surface(hair::materials::Surface {
            surface: "wall".into(),
            file: None,
            tint: [0.2, 0.3, 0.4],
            meters_per_tile: 2.0,
        })
        .unwrap();
    let v4 = reopened.save_new(&base.join("v4"), &[]).unwrap();
    let text = fs::read_to_string(&v4).unwrap();
    assert!(!text.contains("decorations"));
    let q = Project::open(&v4).unwrap();
    assert!(q.catalog().is_none());
    assert_eq!(q.surfaces(), reopened.surfaces());
    let a = reopened.export(&base.join("v4-before"), &[]).unwrap();
    let b = q.export(&base.join("v4-after"), &[]).unwrap();
    assert_eq!(fs::read(a).unwrap(), fs::read(b).unwrap());
    assert_eq!(
        fs::read(&model).unwrap(),
        fs::read(root().join("assets/kenney/desk.glb")).unwrap()
    );
}

#[test]
fn slice7a1_owned_symlink_ancestor_reopens_saved_catalog_like_role_only_project() {
    use std::os::unix::fs::symlink;
    let base = fixture("7a1-reopen").canonicalize().unwrap();
    let owned = base.join("owned");
    fs::create_dir(&owned).unwrap();
    let alias = base.join("owned-alias");
    symlink(&owned, &alias).unwrap();
    let role = owned.join("role.glb");
    let source = owned.join("selected.glb");
    fs::copy(root().join("assets/kenney/desk.glb"), &role).unwrap();
    fs::copy(root().join("assets/kenney/chairDesk.glb"), &source).unwrap();
    let role_bytes = fs::read(&role).unwrap();
    let source_bytes = fs::read(&source).unwrap();
    let mut p = Project::import(&role).unwrap();
    p.save_new(&alias.join("role-only"), &[]).unwrap();
    let role_only = Project::open(&alias.join("role-only/project.toml")).unwrap();
    assert!(role_only.catalog().is_none());
    assert_eq!(role_only.edit(), p.edit());

    let spec = chair_spec();
    let mut blank = Project::blank();
    assert!(
        blank
            .import_catalog_asset(
                spec.assets[0].clone(),
                spec.entries[0].clone(),
                &alias.join("selected.glb"),
            )
            .is_err(),
        "external selected-source symlink ancestors must remain refused"
    );
    assert!(blank.catalog().is_none());
    let preview = p
        .import_catalog_asset(spec.assets[0].clone(), spec.entries[0].clone(), &source)
        .unwrap();
    assert!(
        p.replace_catalog_source("chair-art", &alias.join("selected.glb"))
            .is_err()
    );
    same_catalog(&preview, &p.preview().unwrap());
    let saved = p.save_new(&alias.join("with-catalog"), &[]).unwrap();
    let saved_bytes = fs::read(&saved).unwrap();
    let direct = Project::open(&saved).expect("canonical reopen must already succeed");
    same_catalog(&preview, &direct.preview().unwrap());
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
    assert_eq!(fs::read(&role).unwrap(), role_bytes);
    println!("SLICE7A1_CANONICAL_PROJECT={}", saved.display());
    let aliased = alias.join("with-catalog/project.toml");
    println!("SLICE7A1_ALIASED_PROJECT={}", aliased.display());
    let reopened = Project::open(&aliased).unwrap_or_else(|e| {
        panic!("saved catalog through owned symlink ancestor must reopen: {e}")
    });
    same_catalog(&preview, &reopened.preview().unwrap());
    fs::remove_file(&source).unwrap(); // owned selected copy, not bundled original
    let resaved = reopened.save_new(&base.join("resaved"), &[]).unwrap();
    let pack = Project::open(&resaved)
        .unwrap()
        .export(&base.join("visitor"), &[])
        .unwrap();
    same_catalog(
        &preview,
        &Theme::load(&pack, &mut Default::default()).unwrap(),
    );
    assert_eq!(fs::read(&saved).unwrap(), saved_bytes);
    assert_eq!(
        fs::read(owned.join("with-catalog/chair.glb")).unwrap(),
        source_bytes
    );
    assert_eq!(
        fs::read(base.join("visitor/chair.glb")).unwrap(),
        source_bytes
    );
    assert_eq!(fs::read(&role).unwrap(), role_bytes);
}

#[test]
fn slice7a1_saved_catalog_leaf_symlink_nonregular_and_oversize_stay_refused() {
    use std::os::unix::fs::symlink;
    let base = fixture("7a1-saved-leaf");
    let p = imported_chair(&base);
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    let manifest = fs::read(&saved).unwrap();
    let leaf = base.join("saved/chair.glb");
    let held = base.join("retained.glb");
    fs::rename(&leaf, &held).unwrap();
    let bytes = fs::read(&held).unwrap();
    symlink(&held, &leaf).unwrap();
    let error = Project::open(&saved)
        .err()
        .expect("saved symlink leaf refused");
    assert!(
        error.contains("local path") && error.contains("chair.glb"),
        "{error}"
    );
    fs::remove_file(&leaf).unwrap();
    fs::create_dir(&leaf).unwrap();
    let error = Project::open(&saved)
        .err()
        .expect("saved directory leaf refused");
    assert!(error.contains("must be regular"), "{error}");
    fs::remove_dir(&leaf).unwrap();
    fs::File::create(&leaf).unwrap().set_len(8_000_001).unwrap();
    let error = Project::open(&saved).err().expect("oversize leaf refused");
    assert!(error.contains("8000000"), "{error}");
    fs::remove_file(&leaf).unwrap();
    fs::rename(&held, &leaf).unwrap();
    assert!(Project::open(&saved).is_ok());
    assert_eq!(fs::read(&saved).unwrap(), manifest);
    assert_eq!(fs::read(&leaf).unwrap(), bytes);
    assert_eq!(fs::read(base.join("selected.glb")).unwrap(), bytes);
}

#[test]
fn slice7a1_role_case_aliases_refuse_before_preview_or_source_acquisition() {
    let base = fixture("7a1-role-case");
    let source = base.join("selected.glb");
    fs::copy(root().join("assets/kenney/desk.glb"), &source).unwrap();
    let bytes = fs::read(&source).unwrap();
    for (n, file) in ["MODEL.glb", "Model-1.glb", "MODEL-2.glb"]
        .iter()
        .enumerate()
    {
        let mut spec = chair_spec();
        spec.assets[0].file = (*file).into();
        let mut p = Project::import(&source).unwrap();
        let error = p
            .import_catalog_asset(spec.assets[0].clone(), spec.entries[0].clone(), &source)
            .err()
            .expect("role case alias must be refused");
        assert!(
            error.contains("case-insensitive filename collision"),
            "{file}: {error}"
        );
        assert!(!error.contains("partial NEW output retained"), "{error}");
        assert!(p.catalog().is_none());
        // No closure exists here: schema refusal must precede filesystem reads.
        let input = base.join(format!("input-{n}"));
        fs::create_dir(&input).unwrap();
        let path = write_project(&input, &spec);
        let before = fs::read(&path).unwrap();
        let error = Project::open(&path)
            .err()
            .expect("saved role alias refused");
        assert!(
            error.contains("case-insensitive filename collision"),
            "{error}"
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(!input.join(file).exists());
    }
    assert_eq!(fs::read(&source).unwrap(), bytes);
}

#[test]
fn slice7a1_distinct_catalog_case_aliases_refuse_before_preview_in_either_order() {
    let base = fixture("7a1-catalog-case");
    let source = base.join("selected.glb");
    fs::copy(root().join("assets/kenney/chairDesk.glb"), &source).unwrap();
    let bytes = fs::read(&source).unwrap();
    for (n, files) in [["A.glb", "a.glb"], ["a.glb", "A.glb"]].iter().enumerate() {
        let mut spec = chair_spec();
        spec.assets[0].file = files[0].into();
        let mut p = Project::blank();
        p.import_catalog_asset(spec.assets[0].clone(), spec.entries[0].clone(), &source)
            .unwrap();
        let before = catalog_text(&p);
        let preview = p.preview().unwrap();
        let mut second = spec.assets[0].clone();
        second.id = "second-art".into();
        second.file = files[1].into();
        let entry = choice("second-choice", "second-art");
        let error = p
            .import_catalog_asset(second.clone(), entry.clone(), &source)
            .err()
            .expect("distinct catalog case aliases must be refused");
        assert!(
            error.contains("case-insensitive filename collision"),
            "{error}"
        );
        assert!(!error.contains("partial NEW output retained"), "{error}");
        assert_eq!(catalog_text(&p), before);
        same_catalog(&preview, &p.preview().unwrap());
        spec.assets.push(second);
        spec.entries.push(entry);
        let error = p.set_catalog(spec.clone()).err().unwrap();
        assert!(
            error.contains("case-insensitive filename collision"),
            "{error}"
        );
        let input = base.join(format!("input-{n}"));
        fs::create_dir(&input).unwrap();
        let path = write_project(&input, &spec);
        let before = fs::read(&path).unwrap();
        let error = Project::open(&path).err().unwrap();
        assert!(
            error.contains("case-insensitive filename collision"),
            "{error}"
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(!input.join(files[0]).exists());
    }
    assert_eq!(fs::read(&source).unwrap(), bytes);
}

#[test]
fn slice7a1_exact_catalog_path_reuse_and_filename_grammar_roundtrip() {
    let base = fixture("7a1-exact-reuse");
    let source = base.join("selected.glb");
    fs::copy(root().join("assets/kenney/chairDesk.glb"), &source).unwrap();
    let bytes = fs::read(&source).unwrap();
    let mut spec = chair_spec();
    spec.assets[0].file = "chair.v1_2-art.glb".into();
    let mut p = Project::blank();
    p.import_catalog_asset(spec.assets[0].clone(), spec.entries[0].clone(), &source)
        .unwrap();
    let mut alias = spec.assets[0].clone();
    alias.id = "second-art".into();
    alias.yaw_degrees = -30.0;
    spec.assets.push(alias);
    spec.entries.push(choice("second-choice", "second-art"));
    let preview = p.set_catalog(spec.clone()).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &preview.decorations.asset("chair-art").unwrap().model,
        &preview.decorations.asset("second-art").unwrap().model,
    ));
    for file in [
        "invalid!.glb".to_string(),
        format!("{}.glb", "a".repeat(93)),
    ] {
        let mut bad = spec.clone();
        bad.assets[0].file = file;
        let error = p.set_catalog(bad).err().unwrap();
        assert!(error.contains("digits/-/_/."), "{error}");
        assert_eq!(catalog_text(&p), toml::to_string(&spec).unwrap());
    }
    fs::remove_file(&source).unwrap();
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    let reopened = Project::open(&saved).unwrap();
    let pack = reopened.export(&base.join("visitor"), &[]).unwrap();
    same_catalog(
        &preview,
        &Theme::load(&pack, &mut Default::default()).unwrap(),
    );
    assert_eq!(
        fs::read(base.join("visitor/chair.v1_2-art.glb")).unwrap(),
        bytes
    );
    assert_eq!(reopened.catalog().unwrap().assets, spec.assets);
}

#[cfg(feature = "workshop")]
#[test]
fn workshop_public_open_cancel_failure_and_role_import_preserve_shared_project_state() {
    use hair::workshop::{Command, DialogKind, Workshop};
    let base = fixture("workshop-consumer");
    let source = base.join("catalog-input");
    fs::create_dir(&source).unwrap();
    fs::copy(
        root().join("assets/kenney/chairDesk.glb"),
        source.join("chair.glb"),
    )
    .unwrap();
    let project = write_project(&source, &chair_spec());
    let mut app = Workshop::new(vec![root().into()]);
    app.project_path = project.display().to_string();
    app.dispatch(Command::Open);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(!app.dirty);
    for dirty in [false, true] {
        if dirty {
            app.dispatch(Command::Surface(hair::materials::Surface {
                surface: "wall".into(),
                file: None,
                tint: [0.2, 0.3, 0.4],
                meters_per_tile: 1.0,
            }));
        }
        let revision = app.revision;
        let spec = catalog_text(app.project.as_ref().unwrap());
        let preview = app.project.as_ref().unwrap().preview().unwrap();
        for kind in [
            DialogKind::Import,
            DialogKind::Open,
            DialogKind::SaveNew,
            DialogKind::Export,
        ] {
            app.request_dialog(kind);
            assert_eq!(app.begin_dialog(), Some(kind));
            app.finish_dialog(Ok(None));
            assert_eq!((app.revision, app.dirty), (revision, dirty));
            same_catalog(&preview, app.preview.as_ref().unwrap());
        }
        let bad = base.join("bad");
        if !bad.exists() {
            fs::create_dir(&bad).unwrap();
        }
        let mut invalid = chair_spec();
        invalid.entries[0].asset = "missing".into();
        let bad_path = write_project(&bad, &invalid);
        app.request_dialog(DialogKind::Open);
        app.begin_dialog();
        app.finish_dialog(Ok(Some(bad_path)));
        assert!(app.error.is_some());
        assert_eq!((app.revision, app.dirty), (revision, dirty));
        assert_eq!(catalog_text(app.project.as_ref().unwrap()), spec);
        same_catalog(&preview, app.preview.as_ref().unwrap());
    }
    app.input_path = root().join("assets/kenney/desk.glb").display().to_string();
    app.dispatch(Command::Import);
    assert!(app.confirmation_pending());
    let before = catalog_text(app.project.as_ref().unwrap());
    app.dispatch(Command::Cancel);
    assert_eq!(catalog_text(app.project.as_ref().unwrap()), before);
    app.dispatch(Command::Import);
    app.dispatch(Command::ConfirmDiscard);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(catalog_text(app.project.as_ref().unwrap()), before);
    assert!(app.dirty);
    app.project_path = base.join("saved").display().to_string();
    app.dispatch(Command::SaveNew);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(!app.dirty);
    app.export_path = base.join("export").display().to_string();
    app.dispatch(Command::Export);
    assert!(app.error.is_none(), "{:?}", app.error);
    let loaded = Theme::load(app.exported_pack.as_ref().unwrap(), &mut Default::default()).unwrap();
    same_catalog(app.preview.as_ref().unwrap(), &loaded);
}
