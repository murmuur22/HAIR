#[test]
fn slice6a3_authentic_three_closures_six_models_sharing_and_fourth_refusal() {
    use super::pinned::Budget;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let office = crate::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let archive = crate::theme::Theme::load(&root.join("packs/archive.toml"), &mut Default::default()).unwrap();
    let sources = [select_pin_asset(&office, "chair").unwrap(), select_pin_asset(&office, "desk").unwrap(),
        select_pin_asset(&archive, "closed-bookcase").unwrap(), select_pin_asset(&archive, "open-bookcase").unwrap()];
    for active_pack in ["office", "archive"] {
        let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
        let path = root.join(format!("packs/{active_pack}.toml"));
        let active = crate::theme::Theme::load(&path, &mut Default::default()).unwrap();
        for (i, source) in sources.iter().take(3).enumerate() {
            let d = Decoration::from_entry(source.entry(), [-2., i as f32 - 1.], 0., [0.8; 3]).unwrap();
            assert_eq!(active.publish_pin_checked(&e, source, &d, |_| Ok(())), Outcome::Committed { synced: true });
            let layout = read_layout(&e).unwrap();
            let b = Budget { theme_models: 3, ..Default::default() }.with_pins(&layout.decorations).unwrap();
            assert_eq!((b.theme_models, b.pin_models), (3, i + 1));
        }
        // A fourth placement of exactly the first closure stays at six models.
        let shared = Decoration::from_entry(sources[0].entry(), [2., 0.], 0., [0.8; 3]).unwrap();
        assert_eq!(active.publish_pin_checked(&e, &sources[0], &shared, |_| Ok(())), Outcome::Committed { synced: true });
        let layout = read_layout(&e).unwrap(); let empty = crate::decoration::Catalog::default();
        let repeated: Vec<_> = layout.decorations.iter().filter(|d| d.closure_identity() == Some(sources[0].closure_identity().unwrap().as_str())).collect();
        assert_eq!(repeated.len(), 2);
        assert!(std::sync::Arc::ptr_eq(&repeated[0].resolve(&empty).unwrap().1.model, &repeated[1].resolve(&empty).unwrap().1.model));
        let b = Budget { theme_models: 3, ..Default::default() }.with_pins(&layout.decorations).unwrap();
        assert_eq!((b.theme_models, b.pin_models), (3, 3));
        assert_eq!(b.encoded, sources.iter().take(3).map(|s| s.loaded.encoded.len()).sum::<usize>());
        let mut host = active.host.clone(); host.decorations = layout.decorations.iter().map(|d| (d.placement_id.clone(), d.clone())).collect();
        let mut cache = Default::default();
        for _ in 0..2 { crate::theme::Theme::load_for_scene(&path, &mut cache, &host).unwrap().validate_decoration_candidate_layout(&layout).unwrap(); }
        assert_eq!(cache.loads, 3);
        let excess = Decoration::from_entry(sources[3].entry(), [2., 2.], 0., [0.8; 3]).unwrap();
        let before = slice5a_snapshot(&f.0);
        let out = active.publish_pin_checked(&e, &sources[3], &excess, |_| panic!("fourth closure reached routes"));
        assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("<=3 distinct pin closures")), "{out:?}");
        assert_eq!(slice5a_snapshot(&f.0), before);
        let mut projected = host.clone(); projected.decorations.insert(excess.placement_id.clone(), sources[3].decoration(&excess).unwrap());
        for cold in [true, false] {
            let mut cold_cache = Default::default();
            assert!(crate::theme::Theme::load_for_scene(&path, if cold { &mut cold_cache } else { &mut cache }, &projected).err().unwrap().contains("<=3 distinct pin closures"));
        }
        // Pin-only storage admission must independently refuse even if a generic
        // caller supplies no appearance validator. No new closure/record writes.
        assert!(matches!(publish_pin_checked(&e, &sources[3], &excess, |_| Ok(())), Outcome::NotCommitted(ref s) if s.contains("budget")));
        assert_eq!(slice5a_snapshot(&f.0), before);
        println!("SLICE6A3 authentic active={active_pack} theme_sources=3 pin_closures=3 placements=4 combined_models=6 fourth_closure=REFUSED cold_warm=PASS exact_sharing=PASS");
    }
}

#[test]
fn slice6a3_theme_only_four_distinct_paths_refuse_even_with_identical_bytes() {
    let f = Fixture::new(); let (pack, _) = slice6a_pack(&f);
    let mut manifest = std::fs::read_to_string(pack.join("pack.toml")).unwrap().replace("roles=[]\n", "");
    let bytes = std::fs::read(pack.join("chair.glb")).unwrap();
    let mut cache = Default::default();
    for i in 0..3 {
        std::fs::write(pack.join(format!("model-{i}.glb")), &bytes).unwrap();
        manifest.push_str(&format!("\n[[roles]]\nrole='future-{i}'\nfile='model-{i}.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0,0,0]\n"));
        std::fs::write(pack.join("pack.toml"), &manifest).unwrap();
        for _ in 0..2 {
            let result = crate::theme::Theme::load(&pack.join("pack.toml"), &mut cache);
            if i < 2 { result.unwrap(); }
            else { assert!(result.err().unwrap().contains("<=3 theme model sources")); }
        }
    }
    assert_eq!(cache.loads, 4, "four distinct paths, no content-only sharing, no warm redecode");
}

#[test]
fn slice6a3_pin_only_fourth_namespace_refuses_orphan_and_incomplete_shapes() {
    use std::os::unix::fs::PermissionsExt;
    for shape in ["orphan", "empty", "asset", "pending"] {
        let f = Fixture::new(); let (pack, _) = slice6a_pack(&f); let e = Explorer::new(&f.0).unwrap();
        let original = std::fs::read_to_string(pack.join("pack.toml")).unwrap();
        let mut fourth = None;
        let mut placements = vec![];
        for i in 0..4 {
            std::fs::write(pack.join("pack.toml"), original.replace("chair-mesh", &format!("chair-mesh-{i}"))).unwrap();
            let theme = crate::theme::Theme::load(&pack.join("pack.toml"), &mut Default::default()).unwrap();
            let source = select_pin_asset(&theme, "chair").unwrap();
            if i == 3 { fourth = Some(source); break; }
            let d = slice6a_new(&theme);
            assert_eq!(publish_pin_checked(&e, &source, &d, |_| Ok(())), Outcome::Committed { synced: true });
            placements.push(d.placement_id);
        }
        // Only after all publications: an orphan correctly blocks further writes.
        // All three retained closures may be unreferenced; they still count.
        for id in placements {
            std::fs::remove_dir_all(f.0.join(".hair-layout").join(format!("dc-{id}"))).unwrap();
        }
        let three = read_layout(&e).unwrap();
        assert!(three.decorations.is_empty()); assert_eq!(three.incomplete_records, 3);
        let source = fourth.unwrap();
        let slot = f.0.join(".hair-layout").join(format!("ac-{}", source.closure_identity().unwrap()));
        std::fs::create_dir(&slot).unwrap(); std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o700)).unwrap();
        if shape != "empty" {
            let asset = slot.join("asset.glb"); std::fs::write(&asset, &source.loaded.encoded).unwrap();
            std::fs::set_permissions(asset, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        if shape == "orphan" || shape == "pending" {
            let path = slot.join(if shape == "orphan" { "closure.toml" } else { "closure.pending.toml" });
            std::fs::write(&path, source.descriptor.text().unwrap()).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let before = slice5a_snapshot(&f.0);
        assert!(read_layout(&Explorer::new(&f.0).unwrap()).unwrap_err().contains("model/file limit reached (3, including orphan/incomplete slots)"));
        assert_eq!(slice5a_snapshot(&f.0), before);
        println!("SLICE6A3 pin-only read fourth={shape} REFUSED three_orphans=charged evidence=exact");
    }
}

#[test]
fn slice6a3_authentic_copied_source_removal_retains_all_choices_in_both_themes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let originals = slice5a_snapshot(&root.join("packs"));
    for (pack_name, choices) in [("office", ["chair", "desk"]), ("archive", ["closed-bookcase", "open-bookcase"])] {
        for choice in choices {
            for active_pack in ["office", "archive"] {
                let f = Fixture::new(); let copied = f.0.join("copied-pack"); std::fs::create_dir(&copied).unwrap();
                let filename = format!("{pack_name}.toml");
                let manifest = std::fs::read_to_string(root.join("packs").join(&filename)).unwrap();
                let value: toml::Value = toml::from_str(&manifest).unwrap();
                let mut files = std::collections::BTreeSet::new();
                for role in value["roles"].as_array().unwrap() { files.insert(role["file"].as_str().unwrap()); }
                for asset in value["decorations"]["assets"].as_array().unwrap() { files.insert(asset["file"].as_str().unwrap()); }
                for file in files { std::fs::copy(root.join("packs").join(file), copied.join(file)).unwrap(); }
                std::fs::write(copied.join(&filename), manifest).unwrap();
                let selected = crate::theme::Theme::load(&copied.join(&filename), &mut Default::default()).unwrap();
                let source = select_pin_asset(&selected, choice).unwrap();
                let e = Explorer::new(&f.0).unwrap();
                let active_path = root.join(format!("packs/{active_pack}.toml"));
                let active = crate::theme::Theme::load(&active_path, &mut Default::default()).unwrap();
                let d = Decoration::from_entry(source.entry(), [-2., 0.], 0., [0.8; 3]).unwrap();
                assert_eq!(active.publish_pin_checked(&e, &source, &d, |_| Ok(())), Outcome::Committed { synced: true });
                std::fs::remove_dir_all(&copied).unwrap(); // ONLY disposable fixture copy.
                drop(selected);
                let layout = read_layout(&Explorer::new(&f.0).unwrap()).unwrap(); let empty = crate::decoration::Catalog::default();
                slice6a3_exact_mesh(&source.loaded, layout.decorations[0].resolve(&empty).unwrap().1);
                let mut host = active.host.clone(); host.decorations = layout.decorations.iter().map(|d| (d.placement_id.clone(), d.clone())).collect();
                let mut cache = Default::default();
                for _ in 0..2 { crate::theme::Theme::load_for_scene(&active_path, &mut cache, &host).unwrap().validate_decoration_candidate_layout(&layout).unwrap(); }
                println!("SLICE6A3 removed-owned-copy source={pack_name}:{choice} active={active_pack} retained=EXACT cold_warm=PASS");
            }
        }
    }
    assert_eq!(slice5a_snapshot(&root.join("packs")), originals);
}

// Six safely acquired model sources. Encoded fixtures pad real authored GLBs;
// vertex fixtures use indexed triangles, with pins below the placement mesh cap.
fn slice6a3_six_pack(f: &Fixture, category: &str, excess: bool) -> (PathBuf, crate::theme::Theme, Vec<CheckedPin>) {
    let theme_dir = f.0.join("active"); let pin_dir = f.0.join("selected");
    std::fs::create_dir(&theme_dir).unwrap(); std::fs::create_dir(&pin_dir).unwrap();
    let mut theme = "version=3\nname='Three theme sources'\n".to_string();
    let mut pins = "version=3\nname='Three pin sources'\nroles=[]\n".to_string();
    for i in 0..3 {
        let vertices = if category == "vertices" { 90_000 } else { 3 };
        let bytes = slice6a2_glb(vertices, &[], (category == "encoded").then_some(4_000_000));
        std::fs::write(theme_dir.join(format!("theme-{i}.glb")), bytes).unwrap();
        theme.push_str(&format!("\n[[roles]]\nrole='future-{i}'\nfile='theme-{i}.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0,0,0]\n"));
        let vertices = if category == "vertices" { if i == 2 { 10_002 + usize::from(excess) * 3 } else { 9_999 } } else { 3 };
        let encoded = (category == "encoded").then_some(4_000_000 + usize::from(excess && i == 2) * 4);
        std::fs::write(pin_dir.join(format!("pin-{i}.glb")), slice6a2_glb(vertices, &[], encoded)).unwrap();
        pins.push_str(&format!("\n[[decorations.assets]]\nid='asset-{i}'\nfile='pin-{i}.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0,0,0]\n[[decorations.entries]]\nid='entry-{i}'\ntag='hair.pin-{i}'\nasset='asset-{i}'\nname='Pin {i}'\n"));
    }
    let path = theme_dir.join("pack.toml"); std::fs::write(&path, theme).unwrap();
    std::fs::write(pin_dir.join("pack.toml"), pins).unwrap();
    let active = crate::theme::Theme::load(&path, &mut Default::default()).unwrap();
    let selected = crate::theme::Theme::load(&pin_dir.join("pack.toml"), &mut Default::default()).unwrap();
    let sources = (0..3).map(|i| select_pin_asset(&selected, &format!("entry-{i}")).unwrap()).collect();
    (path, active, sources)
}

#[test]
fn slice6a3_combined_six_unchanged_encoded_and_vertex_boundaries() {
    use super::pinned::Budget;
    for category in ["encoded", "vertices"] {
        for excess in [false, true] {
            let f = Fixture::new(); let (path, active, sources) = slice6a3_six_pack(&f, category, excess);
            let e = Explorer::new(&f.0).unwrap();
            let mut pack_budget = Budget { theme_models: 3, ..Default::default() };
            for i in 0..3 {
                let bytes = crate::content::read_local(path.parent().unwrap(), &format!("theme-{i}.glb"), 8_000_000).unwrap();
                let model = crate::assets::decode(&bytes, 3.).unwrap();
                assert!(model.textures.is_empty());
                pack_budget.encoded += bytes.len(); pack_budget.vertices += model.vertices.len();
            }
            pack_budget.validate().unwrap();
            let encoded = pack_budget.encoded + sources.iter().map(|s| s.loaded.encoded.len()).sum::<usize>();
            let vertices = pack_budget.vertices + sources.iter().map(|s| s.loaded.model.vertices.len()).sum::<usize>();
            if category == "encoded" { assert_eq!(encoded, 24_000_000 + usize::from(excess) * 4); assert!(vertices < 300_000); }
            else { assert_eq!(vertices, 300_000 + usize::from(excess) * 3); assert!(encoded < 24_000_000); }
            println!("SLICE6A3 measured six-model category={category} excess={excess} encoded={encoded} vertices={vertices}");
            for (i, source) in sources.iter().enumerate() {
                let d = Decoration::from_entry(source.entry(), [-2., i as f32 - 1.], 0., [0.8; 3]).unwrap();
                let before = slice5a_snapshot(&f.0);
                let out = active.publish_pin_checked(&e, source, &d, |_| Ok(()));
                if i == 2 && excess {
                    assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("aggregate whole-pack budget")), "{category}: {out:?}");
                    assert_eq!(slice5a_snapshot(&f.0), before);
                } else { assert_eq!(out, Outcome::Committed { synced: true }); }
                let mut layout = read_layout(&e).unwrap();
                if i == 2 && excess { layout.decorations.push(source.decoration(&d).unwrap()); }
                let mut host = active.host.clone(); host.decorations = layout.decorations.iter().map(|d| (d.placement_id.clone(), d.clone())).collect();
                let mut cache = Default::default();
                for _ in 0..2 {
                    let refresh = crate::theme::Theme::load_for_scene(&path, &mut cache, &host);
                    if i == 2 && excess { assert!(refresh.err().unwrap().contains("aggregate whole-pack budget")); }
                    else { refresh.unwrap().validate_decoration_candidate_layout(&layout).unwrap(); }
                }
                if i == 2 {
                    let b = pack_budget.with_pins(&layout.decorations);
                    assert_eq!(b.is_err(), excess);
                    if !excess {
                        let b = b.unwrap(); assert_eq!((b.theme_models, b.pin_models), (3, 3));
                        if category == "encoded" { assert_eq!(b.encoded, 24_000_000); } else { assert_eq!(b.vertices, 300_000); }
                    }
                    println!("SLICE6A3 six-model boundary category={category} excess={excess} commit_refresh_admitted={} shared_cap=unchanged", !excess);
                }
            }
        }
    }
}

#[test]
fn slice6a3_partition_and_shared_resource_exact_at_over_controls() {
    use super::pinned::Budget;
    let at = Budget { theme_models: 3, pin_models: 3, encoded: 24_000_000, vertices: 300_000, textures: 64,
        texture_bytes: 64_000_000 - 24_000_000 - 300_000 * std::mem::size_of::<crate::assets::Vertex>(), extra_retained: 0 };
    at.validate().unwrap();
    for over in [Budget { theme_models: 4, pin_models: 0, ..Default::default() },
        Budget { theme_models: 0, pin_models: 4, ..Default::default() },
        Budget { theme_models: 4, pin_models: 2, ..Default::default() },
        Budget { theme_models: 2, pin_models: 4, ..Default::default() },
        Budget { pin_models: 4, ..at }, Budget { theme_models: 4, ..at },
        Budget { encoded: 24_000_001, ..at }, Budget { vertices: 300_001, ..at },
        Budget { textures: 65, ..at }, Budget { extra_retained: 1, ..at },
        Budget { texture_bytes: 32_000_001, ..Default::default() }] { assert!(over.validate().is_err()); }
    Budget { theme_models: 3, pin_models: 3, texture_bytes: 32_000_000, textures: 64, ..Default::default() }.validate().unwrap();
}
