use hair::{authoring::Project, sky::SkyAppearance, theme::Theme};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture(label: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("hair-7c-{label}-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    println!("SLICE7C_FIXTURE={}", base.display());
    base
}
fn same_appearance(a: &Theme, b: &Theme) {
    assert_eq!(a.sky, b.sky);
    assert_eq!(a.host, b.host);
    assert_eq!(a.loaded_roles, b.loaded_roles);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&a.vertices),
        bytemuck::cast_slice::<_, u8>(&b.vertices)
    );
    assert_eq!(a.textures.len(), b.textures.len());
    for (a, b) in a.textures.iter().zip(&b.textures) {
        assert_eq!((a.width, a.height, &a.rgba), (b.width, b.height, &b.rgba));
    }
    assert_eq!(a.decorations.entries, b.decorations.entries);
    assert_eq!(a.decorations.assets.len(), b.decorations.assets.len());
    for (a, b) in a.decorations.assets.iter().zip(&b.decorations.assets) {
        assert_eq!(a.spec, b.spec);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a.model.vertices),
            bytemuck::cast_slice::<_, u8>(&b.model.vertices)
        );
    }
}
fn visitor(path: &Path) -> Theme {
    Theme::load(path, &mut Default::default()).unwrap()
}
// SYNTHETIC PNG INPUT only; encoder, shared decoder/adapter and output are real.
fn png_input(path: &Path, width: u32, height: u32) -> Vec<u8> {
    let pixels: Vec<u8> = (0..width * height)
        .flat_map(|i| [i as u8, 90, 160, if i % 3 == 0 { 80 } else { 255 }])
        .collect();
    let mut encoder = png::Encoder::new(fs::File::create(path).unwrap(), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
    fs::read(path).unwrap()
}
#[test]
fn existing_project_consumer_retains_panorama_closure() {
    let base = fixture("panorama-consumer");
    let original = png_input(&base.join("sky.png"), 3, 5);
    let path = base.join("project.toml");
    fs::write(
        &path,
        "version=4\nactive_document=true\nassets=[]\n[sky]\nkind='panorama'\nfile='sky.png'\n",
    )
    .unwrap();
    let p = Project::open(&path).expect(
        "SKY_PROJECT_PANORAMA_RED: retain project-root panorama bytes for real Theme preview",
    );
    let preview = p.preview().unwrap();
    let expected = SkyAppearance::Panorama(std::sync::Arc::new(
        hair::sky::adapt_png(&original, Default::default()).unwrap(),
    ));
    assert_eq!(preview.sky, expected);
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    fs::remove_file(base.join("sky.png")).unwrap();
    let pack = Project::open(&saved)
        .unwrap()
        .export(&base.join("export"), &[])
        .unwrap();
    same_appearance(&preview, &visitor(&pack));
    assert_eq!(fs::read(base.join("export/sky.png")).unwrap(), original);
}
#[test]
fn existing_project_consumer_accepts_explicit_shared_colour_and_roundtrips() {
    let base = fixture("colour-consumer");
    let input = base.join("project.toml");
    let text =
        "version=4\nactive_document=true\nassets=[]\n[sky]\nkind='colour'\nrgb=[0.1,0.6,0.2]\n";
    fs::write(&input, text).unwrap();
    let p = Project::open(&input)
        .expect("SKY_PROJECT_COLOUR_RED: editable v4 must accept shared SkySpec");
    let preview = p.preview().unwrap();
    assert_eq!(preview.sky, SkyAppearance::Colour([0.1, 0.6, 0.2]));
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    let reopened = Project::open(&saved).unwrap();
    let pack = reopened.export(&base.join("visitor"), &[]).unwrap();
    same_appearance(&preview, &visitor(&pack));
    assert_eq!(fs::read_to_string(input).unwrap(), text);
}

#[test]
fn public_colour_edit_save_cold_reopen_export_and_clear() {
    let base = fixture("public-colour");
    let mut p = Project::blank();
    assert!(p.sky().is_none());
    let preview = p.set_sky_colour([0.125, 0.75, 1.0]).unwrap();
    assert_eq!(preview.sky, SkyAppearance::Colour([0.125, 0.75, 1.0]));
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 1.01] {
        for axis in 0..3 {
            let mut rgb = [0.5; 3];
            rgb[axis] = value;
            assert!(p.set_sky_colour(rgb).is_err());
            same_appearance(&preview, &p.preview().unwrap());
        }
    }
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    drop(p);
    let mut reopened = Project::open(&saved).unwrap();
    let pack = reopened.export(&base.join("visitor"), &[]).unwrap();
    let manifest: toml::Value = toml::from_str(&fs::read_to_string(&pack).unwrap()).unwrap();
    assert_eq!(manifest["version"].as_integer(), Some(3));
    same_appearance(&preview, &visitor(&pack));
    let cleared = reopened.clear_sky().unwrap();
    assert!(reopened.sky().is_none());
    assert_eq!(cleared.sky, SkyAppearance::Legacy);
    let saved = reopened.save_new(&base.join("cleared"), &[]).unwrap();
    assert!(Project::open(&saved).unwrap().sky().is_none());
}

#[test]
fn public_panorama_import_holds_exact_input_after_delete_cold_reopen_export() {
    let base = fixture("public-panorama");
    let selected = base.join("synthetic-selected.png");
    let bytes = png_input(&selected, 7, 3); // deliberately NOT 2:1, includes alpha
    let expected = SkyAppearance::Panorama(std::sync::Arc::new(
        hair::sky::adapt_png(&bytes, Default::default()).unwrap(),
    ));
    let mut p = Project::blank();
    let preview = p.import_sky_panorama(&selected).unwrap();
    assert_eq!(preview.sky, expected);
    assert_eq!(fs::read(&selected).unwrap(), bytes);
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    assert_eq!(fs::read(base.join("saved/sky.png")).unwrap(), bytes);
    fs::remove_file(&selected).unwrap(); // ONLY this test-owned input
    same_appearance(&preview, &p.preview().unwrap());
    p.export(&base.join("memory-export"), &[]).unwrap();
    drop(p);
    let p = Project::open(&saved).unwrap();
    let pack = p.export(&base.join("visitor"), &[]).unwrap();
    same_appearance(&preview, &visitor(&pack));
    assert_eq!(fs::read(base.join("visitor/sky.png")).unwrap(), bytes);
    println!(
        "SLICE7C_PANORAMA_SAVED={} PACK={}",
        saved.display(),
        pack.display()
    );
}

fn full_project(base: &Path) -> Project {
    use hair::decoration::{Asset, Defaults, Entry};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut p = Project::import(&root.join("assets/kenney/desk.glb")).unwrap();
    p.update(hair::authoring::Edit {
        at: [0.1, 0.0, -0.1],
        yaw_degrees: 15.0,
        ..p.edit()
    })
    .unwrap();
    p.update(hair::authoring::Edit {
        document_role: false,
        yaw_degrees: -20.0,
        ..p.edit()
    })
    .unwrap();
    p.select_door().unwrap();
    let mut door = Project::import(&root.join("examples/door-trim-starter/model.glb")).unwrap();
    door.assign_import(&p);
    p = door;
    let texture = base.join("surface-input.png");
    png_input(&texture, 2, 1);
    p.import_surface("wall", &texture).unwrap();
    p.set_surface(hair::materials::Surface {
        surface: "floor".into(),
        file: None,
        tint: [0.2, 0.7, 0.4],
        meters_per_tile: 2.5,
    })
    .unwrap();
    p.import_catalog_asset(
        Asset {
            id: "chair-art".into(),
            file: "chair.glb".into(),
            max_extent: 10.0,
            scale: 0.8,
            yaw_degrees: 25.0,
            pivot: [0.1, 0.0, -0.2],
        },
        Entry {
            id: "chair".into(),
            tag: "hair.seating.chair".into(),
            asset: "chair-art".into(),
            name: "Reading chair".into(),
            defaults: Defaults { collision: true },
        },
        &root.join("assets/kenney/chairDesk.glb"),
    )
    .unwrap();
    p
}
fn same_except_sky(a: &Theme, b: &Theme) {
    assert_eq!(a.host, b.host);
    assert_eq!(a.loaded_roles, b.loaded_roles);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&a.vertices),
        bytemuck::cast_slice::<_, u8>(&b.vertices)
    );
    assert_eq!(a.decorations.entries, b.decorations.entries);
    assert_eq!(a.decorations.assets.len(), b.decorations.assets.len());
    for (a, b) in a.decorations.assets.iter().zip(&b.decorations.assets) {
        assert_eq!(a.spec, b.spec);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a.model.vertices),
            bytemuck::cast_slice::<_, u8>(&b.model.vertices)
        );
    }
    assert_eq!(a.textures.len(), b.textures.len());
    for (a, b) in a.textures.iter().zip(&b.textures) {
        assert_eq!((a.width, a.height, &a.rgba), (b.width, b.height, &b.rgba));
    }
}
#[test]
fn whole_pack_sky_replacements_invalid_edits_clear_and_role_import_preserve_every_other_resource() {
    let base = fixture("coexistence");
    let mut p = full_project(&base);
    let legacy = p.preview().unwrap();
    assert_eq!(legacy.loaded_roles, ["decoration", "document", "door-trim"]);
    let edit = p.edit();
    let surfaces = p.surfaces();
    let catalog = toml::to_string(p.catalog().unwrap()).unwrap();
    let path = base.join("synthetic-panorama.png");
    png_input(&path, 4, 2);
    let panorama = p.import_sky_panorama(&path).unwrap();
    same_except_sky(&legacy, &panorama);
    assert!(p.set_sky_colour([f64::NAN, 0.0, 0.0]).is_err());
    let bad = base.join("bad.png");
    fs::write(&bad, b"SYNTHETIC INVALID INPUT").unwrap();
    assert!(p.import_sky_panorama(&bad).is_err());
    same_appearance(&panorama, &p.preview().unwrap());
    // Fail the complete Theme candidate after a valid sky acquisition, then repair
    // only this deliberate public-edit corruption and confirm source rollback.
    let replacement = base.join("replacement.png");
    png_input(&replacement, 2, 1);
    p.scale = f32::NAN;
    assert!(p.set_sky_colour([0.0; 3]).is_err());
    assert!(p.clear_sky().is_err());
    assert!(p.import_sky_panorama(&replacement).is_err());
    p.scale = edit.scale;
    same_appearance(&panorama, &p.preview().unwrap());
    let colour = p.set_sky_colour([0.0, 1.0, 0.5]).unwrap();
    same_except_sky(&legacy, &colour);
    p.import_sky_panorama(&path).unwrap();
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    let q = Project::open(&saved).unwrap();
    assert_eq!(q.edit(), edit);
    assert_eq!(q.surfaces(), surfaces);
    assert_eq!(toml::to_string(q.catalog().unwrap()).unwrap(), catalog);
    let pack = q.export(&base.join("visitor"), &[]).unwrap();
    let mut cache = Default::default();
    let loaded = Theme::load(&pack, &mut cache).unwrap();
    assert_eq!(cache.loads, 3, "sky is not a fourth model or material slot");
    same_appearance(&panorama, &loaded);
    Theme::load(&pack, &mut cache).unwrap();
    assert_eq!(cache.loads, 3);
    let mut imported = Project::import(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/door-trim-starter/model.glb"),
    )
    .unwrap();
    imported.assign_import(&p);
    same_appearance(&panorama, &imported.preview().unwrap());
    let cleared = p.clear_sky().unwrap();
    same_appearance(&legacy, &cleared);
    assert_eq!(p.edit(), edit);
    assert_eq!(p.surfaces(), surfaces);
    assert_eq!(toml::to_string(p.catalog().unwrap()).unwrap(), catalog);
    let cleared = p.export(&base.join("cleared"), &[]).unwrap();
    assert!(!base.join("cleared/sky.png").exists());
    same_appearance(&legacy, &visitor(&cleared));
}

#[test]
fn unsafe_selected_sources_profiles_and_sizes_refuse_with_last_good_closure() {
    use std::os::unix::fs::symlink;
    let base = fixture("source-guards").canonicalize().unwrap();
    let selected = base.join("selected.png");
    let original = png_input(&selected, 2, 1);
    let mut p = Project::blank();
    let good = p.import_sky_panorama(&selected).unwrap();
    symlink(&selected, base.join("linked.png")).unwrap();
    symlink(&base, base.join("alias")).unwrap();
    fs::create_dir(base.join("directory.png")).unwrap();
    fs::File::create(base.join("large.png"))
        .unwrap()
        .set_len(4_000_001)
        .unwrap();
    fs::write(base.join("wrong.PNG"), &original).unwrap();
    let fifo = base.join("fifo.png");
    let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    let mut unsupported = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut unsupported, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Sixteen);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[0, 1, 0, 2, 0, 3])
            .unwrap();
    }
    fs::write(base.join("sixteen.png"), unsupported).unwrap();
    png_input(&base.join("wide.png"), 4097, 1);
    png_input(&base.join("pixels.png"), 2049, 2048);
    fs::write(base.join("malformed.png"), b"SYNTHETIC invalid PNG").unwrap();
    for file in [
        "linked.png",
        "alias/selected.png",
        "missing/../selected.png",
        "../selected.png",
        "directory.png",
        "fifo.png",
        "large.png",
        "wrong.PNG",
        "sixteen.png",
        "wide.png",
        "pixels.png",
        "malformed.png",
        "missing.png",
        "a:b.png",
        "a\\b.png",
    ] {
        let error = p.import_sky_panorama(&base.join(file)).err().expect(file);
        assert!(
            !error.contains("partial NEW output retained"),
            "{file}: {error}"
        );
        same_appearance(&good, &p.preview().unwrap());
    }
    // Changing the original after acquisition never changes preview or output.
    fs::write(&selected, b"SYNTHETIC replaced original").unwrap();
    same_appearance(&good, &p.preview().unwrap());
    p.save_new(&base.join("saved"), &[]).unwrap();
    assert_eq!(fs::read(base.join("saved/sky.png")).unwrap(), original);
}

#[test]
fn saved_sky_project_root_alias_reopens_but_unsafe_leaf_refuses() {
    use std::os::unix::fs::symlink;
    let base = fixture("saved-guards").canonicalize().unwrap();
    let owned = base.join("owned");
    fs::create_dir(&owned).unwrap();
    let alias = base.join("alias");
    symlink(&owned, &alias).unwrap();
    let input = owned.join("selected.png");
    let original = png_input(&input, 2, 1);
    let mut p = Project::blank();
    let good = p.import_sky_panorama(&input).unwrap();
    assert!(p.import_sky_panorama(&alias.join("selected.png")).is_err());
    let saved = p.save_new(&alias.join("saved"), &[]).unwrap();
    let manifest = fs::read(&saved).unwrap();
    let aliased = alias.join("saved/project.toml");
    same_appearance(&good, &Project::open(&aliased).unwrap().preview().unwrap());
    let leaf = owned.join("saved/sky.png");
    let held = owned.join("retained.png");
    fs::rename(&leaf, &held).unwrap();
    symlink(&held, &leaf).unwrap();
    assert!(Project::open(&aliased).is_err());
    fs::remove_file(&leaf).unwrap();
    fs::create_dir(&leaf).unwrap();
    assert!(Project::open(&aliased).is_err());
    fs::remove_dir(&leaf).unwrap();
    fs::File::create(&leaf).unwrap().set_len(4_000_001).unwrap();
    assert!(Project::open(&aliased).is_err());
    fs::remove_file(&leaf).unwrap();
    let c = std::ffi::CString::new(leaf.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    assert!(Project::open(&aliased).is_err());
    fs::remove_file(&leaf).unwrap();
    fs::rename(&held, &leaf).unwrap();
    same_appearance(&good, &Project::open(&aliased).unwrap().preview().unwrap());
    assert_eq!(fs::read(saved).unwrap(), manifest);
    assert_eq!(fs::read(leaf).unwrap(), original);
}

#[test]
fn strict_sky_fields_versions_names_and_protected_existing_outputs() {
    use std::os::unix::fs::symlink;
    let base = fixture("schema-output");
    let input = base.join("selected.png");
    let original = png_input(&input, 2, 1);
    let mut p = Project::blank();
    p.import_sky_panorama(&input).unwrap();
    let path = p.save_new(&base.join("saved"), &[]).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    let value: toml::Value = toml::from_str(&text).unwrap();
    let mut refusals = Vec::new();
    for version in [1, 2, 3, 5] {
        let mut v = value.clone();
        v["version"] = version.into();
        refusals.push(v);
    }
    for field in ["fit", "background", "unknown"] {
        let mut v = value.clone();
        v["sky"]
            .as_table_mut()
            .unwrap()
            .insert(field.into(), "unsupported".into());
        refusals.push(v);
    }
    for file in [
        "floor.png",
        "wall.png",
        "model.glb",
        "chair.glb",
        "project.toml",
        "pack.toml",
        "PROVENANCE.toml",
        "SKY.png",
        "../sky.png",
        "/sky.png",
        "a\\sky.png",
        "a:sky.png",
        "a/sky.png",
    ] {
        let mut v = value.clone();
        v["sky"]["file"] = file.into();
        refusals.push(v);
    }
    for kind in ["legacy", "skybox", "Colour", "unknown"] {
        let mut v = value.clone();
        v["sky"]["kind"] = kind.into();
        refusals.push(v);
    }
    for v in refusals {
        let bad = toml::to_string(&v).unwrap();
        fs::write(&path, &bad).unwrap();
        assert!(Project::open(&path).is_err(), "{bad}");
        assert_eq!(fs::read_to_string(&path).unwrap(), bad);
    }
    fs::write(&path, &text).unwrap();
    let existing = base.join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("sentinel"), b"untouched").unwrap();
    symlink(&existing, base.join("output-link")).unwrap();
    let protected = base.join("protected");
    fs::create_dir(&protected).unwrap();
    for target in [
        existing.clone(),
        input.clone(),
        base.join("output-link"),
        protected.join("new"),
    ] {
        assert!(
            p.save_new(&target, std::slice::from_ref(&protected))
                .is_err()
        );
        assert!(p.export(&target, std::slice::from_ref(&protected)).is_err());
    }
    assert!(!protected.join("new").exists());
    assert_eq!(fs::read(existing.join("sentinel")).unwrap(), b"untouched");
    assert_eq!(fs::read(&input).unwrap(), original);
}

#[test]
fn legacy_absence_and_explicit_clear_restore_exact_saved_and_exported_bytes() {
    let base = fixture("legacy-bytes");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/kenney/desk.glb");
    let mut p = Project::import(&source).unwrap();
    let legacy = p.preview().unwrap();
    let before = p.save_new(&base.join("before"), &[]).unwrap();
    let old_pack = p.export(&base.join("before-pack"), &[]).unwrap();
    p.set_sky_colour([0.04, 0.06, 0.05]).unwrap();
    assert!(p.sky().is_some(), "explicit historical RGB is NOT Legacy");
    let explicit = p.save_new(&base.join("explicit"), &[]).unwrap();
    let v: toml::Value = toml::from_str(&fs::read_to_string(explicit).unwrap()).unwrap();
    assert_eq!(v["version"].as_integer(), Some(4));
    same_appearance(&legacy, &p.clear_sky().unwrap());
    let after = p.save_new(&base.join("after"), &[]).unwrap();
    let new_pack = p.export(&base.join("after-pack"), &[]).unwrap();
    assert_eq!(fs::read(before).unwrap(), fs::read(after).unwrap());
    assert_eq!(fs::read(old_pack).unwrap(), fs::read(new_pack).unwrap());
    assert_eq!(
        fs::read(base.join("before-pack/PROVENANCE.toml")).unwrap(),
        fs::read(base.join("after-pack/PROVENANCE.toml")).unwrap()
    );
}

#[test]
fn sky_preserves_catalog_three_asset_thirty_two_entry_limits_and_does_not_borrow_pin_partition() {
    use hair::decoration::{Asset, Defaults, Entry};
    let base = fixture("catalog-limits");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/kenney/chairDesk.glb");
    let mut p = Project::blank();
    for n in 0..3 {
        p.import_catalog_asset(
            Asset {
                id: format!("art-{n}"),
                file: format!("art-{n}.glb"),
                max_extent: 10.0,
                scale: 0.8,
                yaw_degrees: 0.0,
                pivot: [0.0; 3],
            },
            Entry {
                id: format!("choice-{n}"),
                tag: format!("hair.choice.{n}"),
                asset: format!("art-{n}"),
                name: format!("Choice {n}"),
                defaults: Defaults::default(),
            },
            &source,
        )
        .unwrap();
    }
    let mut spec = p.catalog().unwrap().clone();
    for n in 3..32 {
        let mut entry = spec.entries[0].clone();
        entry.id = format!("choice-{n}");
        entry.tag = format!("hair.choice.{n}");
        spec.entries.push(entry);
    }
    p.set_catalog(spec.clone()).unwrap();
    let input = base.join("selected.png");
    png_input(&input, 2, 1);
    let preview = p.import_sky_panorama(&input).unwrap();
    let mut bad = spec.clone();
    let mut extra = bad.entries[0].clone();
    extra.id = "overflow".into();
    extra.tag = "hair.overflow".into();
    bad.entries.push(extra);
    assert!(p.set_catalog(bad).is_err());
    let mut imported = Project::import(&source).unwrap();
    imported.assign_import(&p);
    assert!(
        imported
            .preview()
            .err()
            .unwrap()
            .contains("aggregate whole-pack budget")
    );
    same_appearance(&preview, &p.preview().unwrap());
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    let q = Project::open(&saved).unwrap();
    assert_eq!(q.catalog().unwrap().entries.len(), 32);
    same_appearance(&preview, &q.preview().unwrap());
}

#[test]
fn serialized_colour_refuses_nonfinite_range_shape_and_unknown_fields() {
    let base = fixture("colour-schema");
    let path = base.join("project.toml");
    for sky in [
        "kind='colour'\nrgb=[nan,0.0,0.0]",
        "kind='colour'\nrgb=[inf,0.0,0.0]",
        "kind='colour'\nrgb=[-0.1,0.0,0.0]",
        "kind='colour'\nrgb=[1.1,0.0,0.0]",
        "kind='colour'\nrgb=[0.0,0.0]",
        "kind='colour'\nrgb=[0.0,0.0,0.0,0.0]",
        "kind='colour'\nrgb=[0.0,0.0,0.0]\nfile='sky.png'",
        "rgb=[0.0,0.0,0.0]",
    ] {
        let text = format!("version=4\nactive_document=true\nassets=[]\n[sky]\n{sky}\n");
        fs::write(&path, &text).unwrap();
        assert!(Project::open(&path).is_err(), "{sky}");
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }
}

#[test]
fn successful_panorama_replacement_and_clear_retire_old_closure_without_rewriting_sources() {
    let base = fixture("replacement");
    let first = base.join("first.png");
    let second = base.join("second.png");
    let a = png_input(&first, 2, 1);
    let b = png_input(&second, 4, 2);
    let mut p = Project::blank();
    let old = p.import_sky_panorama(&first).unwrap();
    let new = p.import_sky_panorama(&second).unwrap();
    assert_ne!(old.sky, new.sky);
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    assert_eq!(fs::read(base.join("saved/sky.png")).unwrap(), b);
    same_appearance(&new, &Project::open(&saved).unwrap().preview().unwrap());
    assert_eq!(fs::read(&first).unwrap(), a);
    assert_eq!(fs::read(&second).unwrap(), b);
    p.set_sky_colour([1.0, 0.0, 0.0]).unwrap();
    let colour = p.save_new(&base.join("colour"), &[]).unwrap();
    assert!(!base.join("colour/sky.png").exists());
    assert_eq!(
        Project::open(&colour).unwrap().preview().unwrap().sky,
        SkyAppearance::Colour([1.0, 0.0, 0.0])
    );
    p.clear_sky().unwrap();
    let cleared = p.save_new(&base.join("cleared"), &[]).unwrap();
    assert!(!base.join("cleared/sky.png").exists());
    assert!(Project::open(&cleared).unwrap().sky().is_none());
}

#[cfg(feature = "workshop")]
#[test]
fn existing_workshop_open_cancel_import_save_export_preserve_sky_without_new_ui() {
    use hair::workshop::{Command, DialogKind, Workshop};
    let base = fixture("workshop-consumer");
    let mut p = Project::blank();
    let preview = p.set_sky_colour([0.15, 0.6, 0.3]).unwrap();
    let saved = p.save_new(&base.join("saved"), &[]).unwrap();
    let mut app = Workshop::new(vec![]);
    app.project_path = saved.display().to_string();
    app.dispatch(Command::Open);
    assert!(app.error.is_none(), "{:?}", app.error);
    same_appearance(&preview, app.preview.as_ref().unwrap());
    let revision = app.revision;
    for kind in [
        DialogKind::Import,
        DialogKind::Open,
        DialogKind::SaveNew,
        DialogKind::Export,
    ] {
        app.request_dialog(kind);
        app.begin_dialog();
        app.finish_dialog(Ok(None));
        assert_eq!(app.revision, revision);
        assert!(!app.dirty);
        same_appearance(&preview, app.preview.as_ref().unwrap());
    }
    app.input_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/kenney/desk.glb")
        .display()
        .to_string();
    app.dispatch(Command::Import);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.preview.as_ref().unwrap().sky, preview.sky);
    app.project_path = base.join("resaved").display().to_string();
    app.dispatch(Command::SaveNew);
    assert!(app.error.is_none(), "{:?}", app.error);
    app.export_path = base.join("export").display().to_string();
    app.dispatch(Command::Export);
    assert!(app.error.is_none(), "{:?}", app.error);
    same_appearance(
        app.preview.as_ref().unwrap(),
        &visitor(app.exported_pack.as_ref().unwrap()),
    );
}
