// Actual bundled art and safe acquisition/publication; all writes are fixture-owned.
fn slice6a3_exact_mesh(a: &crate::decoration::LoadedAsset, b: &crate::decoration::LoadedAsset) {
    assert_eq!(a.spec, b.spec);
    assert_eq!(a.encoded, b.encoded);
    assert_eq!(bytemuck::cast_slice::<_, u8>(&a.model.vertices), bytemuck::cast_slice::<_, u8>(&b.model.vertices));
    assert_eq!(a.model.textures.len(), b.model.textures.len());
    for (a, b) in a.model.textures.iter().zip(&b.model.textures) {
        assert_eq!((a.width, a.height, &a.rgba), (b.width, b.height, &b.rgba));
    }
}

#[test]
fn slice6a3_authentic_bundled_pin_tracer_all_combinations() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let originals = slice5a_snapshot(&root.join("packs"));
    for (source_pack, choices) in [("office", ["chair", "desk"]), ("archive", ["closed-bookcase", "open-bookcase"])] {
        let selected_theme = crate::theme::Theme::load(&root.join(format!("packs/{source_pack}.toml")), &mut Default::default()).unwrap();
        for choice in choices {
            let source = select_pin_asset(&selected_theme, choice).unwrap();
            for active_pack in ["office", "archive"] {
                let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
                let path = root.join(format!("packs/{active_pack}.toml"));
                let (world, host) = crate::folder_scene::build(e.current(), 0, true).unwrap();
                let mut cache = Default::default();
                let active = crate::theme::Theme::load_for_scene(&path, &mut cache, &host).unwrap();
                let d = Decoration::from_entry(source.entry(), [-2., 0.], 0., [0.8; 3]).unwrap();
                active.validate_decoration_layout(&LayoutRead::default(), &d).unwrap();
                let before = slice5a_snapshot(&f.0);
                let mut callbacks = 0;
                let out = active.publish_pin_checked(&e, &source, &d, |candidate| {
                    callbacks += 1;
                    crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)
                });
                if matches!(out, Outcome::NotCommitted(_)) {
                    assert_eq!(slice5a_snapshot(&f.0), before);
                    assert!(!f.0.join(".hair-layout").exists());
                }
                println!("SLICE6A3 tracer active={active_pack} source={source_pack} entry={choice} publish={out:?} callbacks={callbacks}");
                assert_eq!(out, Outcome::Committed { synced: true }, "actual bundled pin must be admitted by the partitioned model policy");
                assert_eq!(callbacks, 2);
                let layout = read_layout(&Explorer::new(&f.0).unwrap()).unwrap();
                assert_eq!(layout.decorations.len(), 1);
                let empty = crate::decoration::Catalog::default();
                let retained = layout.decorations[0].resolve(&empty).unwrap().1;
                slice6a3_exact_mesh(&source.loaded, retained);
                let mut refreshed_host = host.clone();
                assert!(crate::placement_scene::apply(&layout, &world, &mut refreshed_host).unwrap().is_empty());
                for cold in [true, false] {
                    let mut cold_cache = Default::default();
                    let refreshed = crate::theme::Theme::load_for_scene(&path, if cold { &mut cold_cache } else { &mut cache }, &refreshed_host).unwrap();
                    refreshed.validate_decoration_candidate_layout(&layout).unwrap();
                    assert_eq!(bytemuck::cast_slice::<_, u8>(&refreshed.decoration_preview(&layout.decorations[0]).unwrap()), bytemuck::cast_slice::<_, u8>(&retained.placed(&layout.decorations[0]).unwrap()));
                }
                println!("SLICE6A3 authentic active={active_pack} source={source_pack} entry={choice} retained=EXACT cold_warm_refresh=PASS other_theme_missing_tag={}", active_pack != source_pack);
            }
        }
    }
    assert_eq!(slice5a_snapshot(&root.join("packs")), originals, "bundled pack bytes/inodes unchanged");
}
