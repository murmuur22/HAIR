#[test]
fn slice6a2_authentic_office_archive_partitioned_pins_and_shared_byte_control() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (source_pack, choices) in [("office", ["chair", "desk"]), ("archive", ["closed-bookcase", "open-bookcase"])] {
        let source_theme = crate::theme::Theme::load(&root.join(format!("packs/{source_pack}.toml")), &mut Default::default()).unwrap();
        for choice in choices {
            let source = select_pin_asset(&source_theme, choice).unwrap();
            for active_pack in ["office", "archive"] {
                let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
                let active = crate::theme::Theme::load(&root.join(format!("packs/{active_pack}.toml")), &mut Default::default()).unwrap();
                let proposed = Decoration::from_entry(source.entry(), [-2., 0.], 0., [0.8; 3]).unwrap();
                let before = slice5a_snapshot(&f.0);
                // Deliberate 6A3 contract change; original failure log retained.
                let out = active.publish_pin_checked(&e, &source, &proposed, |_| Ok(()));
                assert_eq!(out, Outcome::Committed { synced: true }, "{active_pack}/{source_pack}:{choice}");
                for entry in before { assert!(slice5a_snapshot(&f.0).contains(&entry)); }
                active.validate_decoration_candidate_layout(&read_layout(&e).unwrap()).unwrap();
                let follow = active.validate_decoration_layout(&LayoutRead::default(), &proposed);
                follow.as_ref().unwrap(); // Owner R1: absent exact Follow tag has explicit missing artwork.
                println!("SLICE6A2 updated-by-6A3 authentic active={active_pack} source={source_pack} entry={choice} pin=ADMITTED partition=3_theme+1_pin follow_theme={}", if follow.is_ok() { "ADMITTED" } else { "MISSING_TAG" });
            }
        }
    }
    let f = Fixture::new(); let (_, theme) = slice6a2_budget_pack(&f, "encoded", true);
    let e = Explorer::new(&f.0).unwrap(); let source = select_pin_asset(&theme, "chair").unwrap();
    let before = slice5a_snapshot(&f.0);
    assert!(matches!(theme.publish_pin_checked(&e, &source, &slice6a_new(&theme), |_| panic!("byte overflow")), Outcome::NotCommitted(ref s) if s.contains("aggregate whole-pack budget")));
    assert_eq!(slice5a_snapshot(&f.0), before);
}

#[test]
fn slice6a2_under_and_at_model_budget_distinct_descriptors_and_shared_relocations() {
    let f = Fixture::new(); let (pack, mut theme) = slice6a_pack(&f); let e = Explorer::new(&f.0).unwrap();
    let original = std::fs::read_to_string(pack.join("pack.toml")).unwrap();
    let mut expected = vec![];
    for i in 0..4 {
        std::fs::write(pack.join("pack.toml"), original.replace("chair-mesh", &format!("chair-mesh-{i}"))).unwrap();
        theme = crate::theme::Theme::load(&pack.join("pack.toml"), &mut Default::default()).unwrap();
        let source = select_pin_asset(&theme, "chair").unwrap(); let mut d = slice6a_new(&theme); d.at[0] = i as f32;
        let before = slice5a_snapshot(&f.0);
        let out = theme.publish_pin_checked(&e, &source, &d, |_| Ok(()));
        if i < 3 {
            assert_eq!(out, Outcome::Committed { synced: true });
            expected.push(source.closure_identity().unwrap());
            let layout = read_layout(&e).unwrap();
            let mut host = theme.host.clone(); host.decorations = layout.decorations.iter().map(|d| (d.placement_id.clone(), d.clone())).collect();
            let mut cache = Default::default();
            for _ in 0..2 { crate::theme::Theme::load_for_scene(&pack.join("pack.toml"), &mut cache, &host).unwrap().validate_decoration_candidate_layout(&layout).unwrap(); }
        } else {
            assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("aggregate whole-pack budget")));
            assert_eq!(slice5a_snapshot(&f.0), before);
        }
    }
    assert_ne!(expected[0], expected[1], "same GLB bytes do not merge distinct source descriptors");
    assert_ne!(expected[1], expected[2]);
    assert_eq!(read_layout(&e).unwrap().decorations.len(), 3);
    let layout = read_layout(&e).unwrap();
    let mut moved = layout.decorations[0].clone(); moved.at[0] += 0.1;
    assert_eq!(relocate_decoration_checked(&e, &layout.decorations[0], &moved, |candidate| theme.validate_decoration_candidate_layout(candidate)), Outcome::Committed { synced: true });
    let mut duplicate = moved.clone(); duplicate.placement_id = slice6a_new(&theme).placement_id;
    let mut candidate = read_layout(&e).unwrap(); candidate.decorations.push(duplicate.clone());
    theme.validate_decoration_candidate_layout(&candidate).unwrap(); // same closure charged once
    duplicate.pinned.as_mut().unwrap().loaded = None;
    duplicate.placement_id = slice6a_new(&theme).placement_id;
    candidate.decorations.push(duplicate);
    assert!(theme.validate_decoration_candidate_layout(&candidate).unwrap_err().contains("not acquired"));
}

#[test]
fn slice6a2_locked_final_proposed_restored_other_pins_repeat_full_texture_admission() {
    for target in ["proposed", "restored", "other"] {
        let f = Fixture::new(); let (_, theme) = slice6a2_budget_pack(&f, "texture", true);
        let small_fixture = Fixture::new(); let (_, small_theme) = slice6a_pack(&small_fixture);
        let small = select_pin_asset(&small_theme, "chair").unwrap(); let large = select_pin_asset(&theme, "chair").unwrap();
        let e = Explorer::new(&f.0).unwrap();
        let old = slice6a_new(&theme); let mut other = slice6a_new(&theme); other.at = [2., 0.];
        for d in [&old, &other] { assert_eq!(theme.publish_pin_checked(&e, &small, d, |_| Ok(())), Outcome::Committed { synced: true }); }
        let layout = read_layout(&e).unwrap(); let before = slice4c1_record_snapshot(&f.0);
        let expected = layout.decorations.iter().find(|d| d.placement_id == old.placement_id).unwrap();
        let mut proposed = expected.clone(); proposed.at[0] += 0.1;
        let mut calls = 0;
        let mut validate = |candidate: &LayoutRead| {
            calls += 1;
            if calls == 1 { return theme.validate_decoration_candidate_layout(candidate); }
            let mut changed = candidate.clone();
            let id = if target == "other" { &other.placement_id } else { &old.placement_id };
            let d = changed.decorations.iter_mut().find(|d| d.placement_id == *id).unwrap();
            if target == "restored" { *d = expected.clone(); }
            // Another checked source, not invented resource data. Restore the
            // logical proposal first where requested, then re-evaluate resources.
            d.pinned = Some(super::pinned::PinRef { closure: large.closure_identity().unwrap(), entry: large.entry().clone(), loaded: Some(large.loaded.clone()) });
            theme.validate_decoration_candidate_layout(&changed)
        };
        let out = relocate_decoration_checked(&e, expected, &proposed, &mut validate);
        assert_eq!(calls, 2);
        assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("aggregate whole-pack budget")), "{target}: {out:?}");
        for record in before { assert!(slice4c1_record_snapshot(&f.0).contains(&record)); }
        assert!(read_layout(&e).unwrap().decorations.contains(expected));
        println!("SLICE6A2 locked-final target={target} full_texture_admission=REFUSED old_records=exact");
    }
}

#[test]
fn slice6a2_pre_pending_directory_sync_fault_and_abrupt_exit_preserve_layout() {
    for stage in [10, 11] {
        let (f, e, theme) = slice6a1_saved();
        let before = slice5a_snapshot(&f.0);
        let source = select_pin_asset(&theme, "chair").unwrap();
        let out = mac::publish_pin(&e, &source, &slice6a_new(&theme), &mut |_| Ok(()), |at| if at == stage { Err("injected pre-pending directory sync fault".into()) } else { Ok(()) });
        assert!(matches!(out, Outcome::NotCommitted(_)));
        let closure = slice6a_closure(&f);
        assert!(!closure.join("closure.pending.toml").exists() && !closure.join("closure.toml").exists());
        assert_eq!(std::fs::read(closure.join("asset.glb")).unwrap(), source.loaded.encoded.as_ref());
        for entry in before { assert!(slice5a_snapshot(&f.0).contains(&entry)); }
        assert!(slice6a1_child(&f.0, "read", 1, 0).status.success());
        let (crash_f, _, _) = slice6a1_saved();
        let crash = slice6a1_child(&crash_f.0, &stage.to_string(), 1, 0);
        assert_eq!(crash.status.code(), Some(73));
        assert!(slice6a1_child(&crash_f.0, "read", 1, 0).status.success());
        assert!(!slice6a_closure(&crash_f).join("closure.pending.toml").exists());
        println!("SLICE6A2 pre-pending stage={stage} injected_fault=NotCommitted abrupt_exit=73 cold_read=PASS pending=absent (not power-loss proof)");
    }
}

#[test]
fn slice6a2_directory_sync_order_and_failures_stop_before_pending() {
    use std::cell::RefCell;
    for fail in [None, Some("closure"), Some("root")] {
        let events = RefCell::new(vec![]);
        let result = mac::pin_sync_asset_entries(&mut |at| { events.borrow_mut().push(if at == 10 { "before-closure" } else { "before-root" }); Ok(()) },
            || { events.borrow_mut().push("closure"); fail != Some("closure") },
            || { events.borrow_mut().push("root"); fail != Some("root") });
        if result.is_ok() { events.borrow_mut().push("pending-create-allowed"); }
        assert_eq!(result.is_err(), fail.is_some());
        assert_eq!(*events.borrow(), if fail == Some("closure") { vec!["before-closure", "closure"] }
            else if fail == Some("root") { vec!["before-closure", "closure", "before-root", "root"] }
            else { vec!["before-closure", "closure", "before-root", "root", "pending-create-allowed"] });
    }
}

#[test]
fn slice6a2_actual_publish_locked_final_rechecks_each_resource_category() {
    for category in ["encoded", "texture", "retained"] {
        let f = Fixture::new(); let (_, theme) = slice6a2_budget_pack(&f, category, true);
        let small_fixture = Fixture::new(); let (_, small_theme) = slice6a_pack(&small_fixture);
        let small = select_pin_asset(&small_theme, "chair").unwrap();
        let large = select_pin_asset(&theme, "chair").unwrap();
        let e = Explorer::new(&f.0).unwrap();
        assert_eq!(publish(&e, OsStr::new("saved.txt"), b"prior file", &pos(), &AtomicBool::new(false)), Outcome::Committed { synced: true });
        let before = slice4c1_record_snapshot(&f.0);
        let proposed = slice6a_new(&theme);
        let mut calls = 0;
        let out = publish_pin_checked(&e, &small, &proposed, |candidate| {
            calls += 1;
            if calls == 1 { return theme.validate_decoration_candidate_layout(candidate); }
            let mut changed = candidate.clone();
            // Simulate a late selection in the validator with another genuinely
            // acquired source. The generic core may not waive final admission.
            changed.decorations[0] = large.decoration(&proposed).unwrap();
            theme.validate_decoration_candidate_layout(&changed)
        });
        assert_eq!(calls, 2);
        assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("aggregate whole-pack budget")), "{category}: {out:?}");
        assert!(read_layout(&e).unwrap().decorations.is_empty());
        for record in before { assert!(slice4c1_record_snapshot(&f.0).contains(&record)); }
        println!("SLICE6A2 actual-publish locked-final category={category} callbacks=2 activation=REFUSED prior_records=exact inactive_evidence=retained");
    }
}

#[test]
fn slice6a2_scene_material_expansion_matches_refresh_without_resource_omissions() {
    let f = Fixture::new(); let (pack, _) = slice6a_pack(&f);
    std::fs::write(pack.join("chair.glb"), slice6a2_glb(3, &[(1,1); 4], None)).unwrap();
    let roles = ["document", "portal", "decoration"].map(|role| format!("[[roles]]\nrole='{role}'\nfile='chair.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0,0,0]\n")).join("");
    let manifest = std::fs::read_to_string(pack.join("pack.toml")).unwrap().replace("roles=[]", &roles);
    std::fs::write(pack.join("pack.toml"), manifest).unwrap();
    let mut host = crate::host_scene::HostScene::shared_default();
    let decoration = host.anchors.iter().find(|a| a.role == "decoration").unwrap().clone();
    for i in 0..12 { let mut a = decoration.clone(); a.id = format!("repeated-{i}"); host.anchors.push(a); }
    let path = pack.join("pack.toml"); let mut cache = Default::default();
    let theme = crate::theme::Theme::load_for_scene(&path, &mut cache, &host).unwrap();
    assert_eq!(theme.textures.len(), 60);
    let source = select_pin_asset(&theme, "chair").unwrap(); let d = slice6a_new(&theme);
    let e = Explorer::new(&f.0).unwrap();
    assert_eq!(publish(&e, OsStr::new("saved.txt"), b"prior file", &pos(), &AtomicBool::new(false)), Outcome::Committed { synced: true });
    let before = slice5a_snapshot(&f.0);
    let out = publish_pin_checked(&e, &source, &d, |candidate| theme.validate_decoration_candidate_layout(candidate));
    assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("scene decoded texture budget")), "{out:?}");
    assert_eq!(slice5a_snapshot(&f.0), before);
    let mut projected = host.clone(); projected.decorations.insert(d.placement_id.clone(), source.decoration(&d).unwrap());
    assert!(crate::theme::Theme::load_for_scene(&path, &mut cache, &projected).err().unwrap().contains("scene decoded texture budget"));
    // A smaller same-policy host admits the authentic decoded synthetic source;
    // repeated placements share one pin material set, exactly as Theme load does.
    host.anchors.pop();
    let smaller = crate::theme::Theme::load_for_scene(&path, &mut cache, &host).unwrap();
    assert_eq!(smaller.publish_pin_checked(&e, &source, &d, |_| Ok(())), Outcome::Committed { synced: true });
    let layout = read_layout(&e).unwrap();
    host.decorations = layout.decorations.iter().map(|d| (d.placement_id.clone(), d.clone())).collect();
    let refreshed = crate::theme::Theme::load_for_scene(&path, &mut cache, &host).unwrap();
    assert_eq!(refreshed.textures.len(), 60);
    refreshed.validate_decoration_candidate_layout(&layout).unwrap();
    println!("SLICE6A2 scene-materials 60+4=64 REFUSED; 56+4=60 admitted_commit_refresh; whole-pack=2_models/8_images");
}
