// Included in placement_store::tests: real storage, fresh directory geometry.
#[test]
fn slice4c1_file_final_validators_refuse_late_generated_overlap_with_active_decor() {
    for reposition in [false, true] {
        slice4c1_late_generated_overlap(reposition, true);
    }
}

#[test]
fn slice4c1_reposition_prepare_refuses_generated_overlap_before_final_callback() {
    slice4c1_late_generated_overlap(true, false);
}

// Keep preparation refusal distinct from the final-callback contract. Inserting
// at callback two changes real directory geometry after preparation, before validation.
fn slice4c1_late_generated_overlap(reposition: bool, insert_at_final: bool) {
    use std::os::unix::fs::MetadataExt;
        let f = Fixture::new();
        std::fs::write(f.0.join("zoriginal.txt"), b"source unchanged").unwrap();
        if !reposition { std::fs::write(f.0.join("existing.txt"), b"source unchanged").unwrap(); }
        let mut e = Explorer::new(&f.0).unwrap();
        if reposition {
            let initial = pos();
            assert!(matches!(publish_checked(&e, OsStr::new("existing.txt"), b"source unchanged", &initial, &AtomicBool::new(false), |live| crate::placement_scene::validate_publication(&e, live, OsStr::new("existing.txt"), &initial, [0., 2.], 0).map(|_| ())), Outcome::Committed { .. }));
        }
        let entry = crate::decoration::Entry { id: "chair".into(), tag: "hair.seating.chair".into(), asset: "mesh".into(), name: "Chair".into(), defaults: crate::decoration::Defaults { collision: false } };
        // Keep this overlap-stage witness below the generated target centre.
        // Taller intersecting decor now correctly fails the earlier sight gate;
        // the same footprint still exercises fresh overlap rejection here.
        let d = Decoration::from_entry(&entry, [if reposition { -1.5 } else { 1.5 }, -3.], 0., [0.8, 0.2, 0.8]).unwrap();
        let saved = place_decoration_checked(&e, &d, |live| crate::placement_scene::validate_decoration(&e, live, &d, [0., 2.], 0));
        assert!(matches!(saved, Outcome::Committed { .. }), "reposition={reposition}: {saved:?}");
        let original = ["existing.txt", "zoriginal.txt"].map(|name| (name, std::fs::metadata(f.0.join(name)).unwrap().ino(), std::fs::read(f.0.join(name)).unwrap()));
        let before = read_layout(&e).unwrap();
        let records = slice4c1_record_snapshot(&f.0);
        assert_eq!(before.decorations, vec![d.clone()]);
        assert_eq!(records.len(), if reposition { 2 } else { 1 });
        // Publication adds a new identity; select from a refreshed listing rather
        // than the pre-publication Explorer snapshot.
        e.refresh().unwrap();
        let target = select_existing(&e, OsStr::new("existing.txt")).unwrap();
        let name = if reposition { target.name() } else { OsStr::new("added.txt") };
        let mut p = pos(); p.at = [2., 0.]; p.allow_overlap = true;
        let mut calls = 0;
        let mut validator = |live: &LayoutRead| {
            calls += 1;
            assert!(read_layout(&e).unwrap_err().contains("busy"));
            assert_eq!(live, &before, "unchanged authoritative saved layout");
            let current_records = slice4c1_record_snapshot(&f.0);
            for record in &records {
                assert!(current_records.contains(record), "original record changed before callback {calls}");
            }
            if insert_at_final && calls == 2 {
                // Prove preparation completed; do not tamper with any record
                // or replace real geometry validation with a synthetic Err.
                assert!(current_records.len() > records.len());
                std::fs::write(f.0.join("zlate.txt"), b"late independent file").unwrap();
            }
            let result = if reposition { crate::placement_scene::validate_reposition(&e, live, name, &p, [0., 2.], 0) }
                else { crate::placement_scene::validate_publication(&e, live, name, &p, [0., 2.], 0) };
            eprintln!("reposition={reposition} insert_at_final={insert_at_final} locked callback={calls} geometry={result:?}");
            if calls == 1 {
                assert!(result.is_ok(), "first geometry must be clear: {result:?}");
                // A real late directory insertion changes generated envelopes.
                // No storage record tampering or synthetic refusal response.
                if !insert_at_final {
                    std::fs::write(f.0.join("zlate.txt"), b"late independent file").unwrap();
                }
            } else {
                assert_eq!(calls, 2, "unexpected extra validator callback");
                assert_eq!(result, Err("Placement overlaps an existing object".into()), "late generated envelope invalidates saved decoration admission");
            }
            result.map(|_| ())
        };
        let outcome = if reposition { reposition_checked(&e, &target, &p, &mut validator) }
            else { publish_checked(&e, name, b"new payload", &p, &AtomicBool::new(false), &mut validator) };
        eprintln!("reposition={reposition} insert_at_final={insert_at_final} completed callbacks={calls} outcome={outcome:?}");
        assert_eq!(calls, if insert_at_final { 2 } else { 1 }, "reposition={reposition}: outcome={outcome:?}");
        assert_eq!(outcome, Outcome::NotCommitted("Placement overlaps an existing object".into()));
        if !insert_at_final {
            // Same production call used during reposition record preparation.
            assert_eq!(crate::placement_scene::retains_generated_slot(&e, &before, name), Err("Placement overlaps an existing object".into()));
        }
        assert_eq!(read_layout(&e).unwrap(), before);
        validate_existing(&e, &target).unwrap();
        let after = slice4c1_record_snapshot(&f.0);
        for record in records {
            assert!(after.contains(&record), "original record bytes/inode changed: {}", record.0.display());
        }
        assert!(!f.0.join("added.txt").exists());
        assert_eq!(std::fs::read(f.0.join("zlate.txt")).unwrap(), b"late independent file");
        for (name, inode, bytes) in original {
            assert_eq!(std::fs::metadata(f.0.join(name)).unwrap().ino(), inode);
            assert_eq!(std::fs::read(f.0.join(name)).unwrap(), bytes);
        }
}

// Refused attempts may retain new inactive records. Old file/decor record
// bytes and inodes must remain exact; inspect only this owned scratch fixture.
fn slice4c1_record_snapshot(folder: &std::path::Path) -> Vec<(PathBuf, u64, Vec<u8>)> {
    use std::os::unix::fs::MetadataExt;
    let mut records = Vec::new();
    for entry in std::fs::read_dir(folder.join(".hair-layout")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            for name in ["record.toml", "decoration.toml"] {
                let path = entry.path().join(name);
                if path.exists() {
                    records.push((path.clone(), std::fs::metadata(&path).unwrap().ino(), std::fs::read(&path).unwrap()));
                }
            }
        }
    }
    records.sort_by(|a, b| a.0.cmp(&b.0));
    records
}

#[test]
fn slice4c1_atomic_group_admission_protects_low_ceiling_and_connected_routes() {
    let f = Fixture::new();
    let e = Explorer::new(&f.0).unwrap();
    let (world, mut host) = crate::folder_scene::build(e.current(), 0, true).unwrap();
    host.room = [-5., -5., 5., 5.]; host.spawn = [0., 3.];
    host.anchors.retain(|a| a.role == "document");
    host.anchors[0].at = [0., 0., -3.];
    host.geometry = vec![
        crate::host_scene::Geometry { at: [-3., 0., 0.], size: [4.4, 2., 0.6] },
        crate::host_scene::Geometry { at: [3., 0., 0.], size: [4.4, 2., 0.6] },
    ];
    host.check_routes(&[[0., -2.]]).unwrap();
    let entry = crate::decoration::Entry { id: "chair".into(), tag: "hair.seating.chair".into(), asset: "mesh".into(), name: "Chair".into(), defaults: crate::decoration::Defaults { collision: true } };
    let d = Decoration::from_entry(&entry, [0., 0.], 0., [1.2, 1., 0.6]).unwrap();
    crate::placement::surface::clearance(&host, &d.envelope_mesh().unwrap(), crate::placement::Overlap::Prevent).unwrap();
    let mut layout = LayoutRead { decorations: vec![d.clone()], ..Default::default() };
    let mut staged = host.clone();
    let error = crate::placement_scene::apply(&layout, &world, &mut staged).unwrap_err();
    assert!(error.contains("unreachable"), "clear local footprint must still reject disconnected route: {error}");
    assert_eq!(staged, host);
    layout.decorations[0].collision = false;
    assert!(crate::placement_scene::apply(&layout, &world, &mut staged).unwrap().is_empty());
    assert!(!staged.blocked([0., 0.]), "saved collision OFF only changes decor solidity");
    host.geometry.clear(); host.open_platform = false; host.height = 0.7;
    let before = host.clone();
    assert!(crate::placement_scene::apply(&layout, &world, &mut host).unwrap_err().contains("ceiling"));
    assert_eq!(host, before);
}
