#[test]
fn slice5a_candidate_rejects_new_blocked_route_without_old_bounds_or_lost_files() {
    let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
    let p = Position { at: [0., 0.], yaw: 0., size: [0.4, 0.8, 0.4], allow_overlap: false,
        surface: None, properties: Default::default() };
    assert_eq!(publish_checked(&e, OsStr::new("target.txt"), b"owned route target", &p, &AtomicBool::new(false), |live|
        crate::placement_scene::validate_publication(&e, live, OsStr::new("target.txt"), &p, [0., 3.], 0).map(|_| ())), Outcome::Committed { synced: true });
    // Three walls leave an accessible west opening. Relocating the fourth from
    // its remote position closes that opening without any geometric overlap.
    let mut walls = vec![];
    for (at, size) in [([0., 0.7], [1.6, 2., 0.2]), ([0., -0.7], [1.6, 2., 0.2]),
        ([0.7, 0.], [0.2, 2., 1.2]), ([-3., 0.], [0.2, 2., 1.2])] {
        let mut d = slice5a_decoration(at); d.size = size;
        assert_eq!(place_decoration_checked(&e, &d, |live|
            crate::placement_scene::validate_decoration(&e, live, &d, [0., 3.], 0)), Outcome::Committed { synced: true });
        walls.push(d);
    }
    let old = walls.last().unwrap(); let mut proposed = old.clone(); proposed.at = [-0.7, 0.];
    let before = slice5a_snapshot(&f.0);
    let out = relocate_decoration_checked(&e, old, &proposed, |candidate| {
        assert_eq!(candidate.positions, vec![("target.txt".into(), p.clone())]);
        assert_eq!(candidate.decorations.len(), 4);
        crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 3.], 0)
    });
    assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("unreachable")), "new route blockage, not overlap: {out:?}");
    assert_eq!(slice5a_snapshot(&f.0), before);
    assert_eq!(std::fs::read(f.0.join("target.txt")).unwrap(), b"owned route target");
}
#[test]
fn slice5a_final_candidate_uses_fresh_listing_and_preserves_unrelated_records() {
    let f = Fixture::new();
    std::fs::write(f.0.join("a.txt"), b"untouched original").unwrap();
    let e = Explorer::new(&f.0).unwrap();
    let mut d = slice5a_decoration([2., 0.]); d.size = [0.8, 0.2, 0.8];
    assert_eq!(place_decoration_checked(&e, &d, |live|
        crate::placement_scene::validate_decoration(&e, live, &d, [0., 2.], 0)), Outcome::Committed { synced: true });
    let mut proposed = d.clone(); proposed.at = [-1.5, -3.];
    let before = slice4c1_record_snapshot(&f.0); let mut calls = 0;
    let out = relocate_decoration_checked(&e, &d, &proposed, |candidate| {
        calls += 1;
        if calls == 2 {
            // a.txt occupies slot zero, zlate.txt adds the slot-one envelope
            // under the proposed decoration. Storage itself is not tampered.
            std::fs::write(f.0.join("zlate.txt"), b"actual late listing entry").unwrap();
        }
        let result = crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0);
        if calls == 1 { assert!(result.is_ok(), "first pass must admit: {result:?}"); }
        result
    });
    assert_eq!(calls, 2);
    assert_eq!(out, Outcome::NotCommitted("Placement overlaps an existing object".into()));
    assert_eq!(slice4c1_record_snapshot(&f.0), before);
    let reopened = read_layout(&e).unwrap(); assert_eq!(reopened.decorations, vec![d]); assert_eq!(reopened.incomplete_records, 1);
    assert_eq!(std::fs::read(f.0.join("a.txt")).unwrap(), b"untouched original");
    assert_eq!(std::fs::read(f.0.join("zlate.txt")).unwrap(), b"actual late listing entry");
}
#[test]
fn slice5a_candidate_retains_saved_file_and_other_page_generated_envelopes() {
    for saved in [false, true] {
        let f = Fixture::new();
        for i in 0..13 { std::fs::write(f.0.join(format!("file-{i:02}.txt")), b"owned page entry").unwrap(); }
        let e = Explorer::new(&f.0).unwrap();
        let mut d = slice5a_decoration([0., 1.]); d.size = [0.8, 0.2, 0.8];
        assert_eq!(place_decoration_checked(&e, &d, |live|
            crate::placement_scene::validate_decoration(&e, live, &d, [0., 2.], 0)), Outcome::Committed { synced: true });
        let mut proposed = d.clone();
        if saved {
            let target = select_existing(&e, OsStr::new("file-12.txt")).unwrap();
            let mut p = pos(); p.at = [0., -1.]; p.yaw = 0.; p.size = [0.8; 3];
            assert_eq!(reposition_checked(&e, &target, &p, |live|
                crate::placement_scene::validate_reposition(&e, live, target.name(), &p, [0., 2.], 0).map(|_| ())), Outcome::Committed { synced: true });
            proposed.at = p.at;
        } else {
            // .hair-layout moves file-11 to page one, retaining global slot
            // eleven at [4.5, 3.]. Page zero has no envelope at that point.
            proposed.at = [4.5, 3.];
            let candidate = read_layout(&e).unwrap().relocating(&d, &proposed).unwrap();
            let mut fresh = e.try_clone().unwrap(); fresh.refresh().unwrap();
            let (world, mut host) = crate::folder_scene::build(fresh.current(), 0, true).unwrap();
            assert!(crate::placement_scene::apply(&candidate, &world, &mut host).unwrap().is_empty(), "page zero must admit the candidate");
            let (world, mut host) = crate::folder_scene::build(fresh.current(), 1, true).unwrap();
            assert_eq!(crate::placement_scene::apply(&candidate, &world, &mut host).unwrap_err(), "Placement overlaps an existing object", "only the other page supplies the obstructing generated envelope");
        }
        let before = slice5a_snapshot(&f.0);
        let out = relocate_decoration_checked(&e, &d, &proposed, |candidate|
            crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0));
        assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("overlap") || s.contains("recovery")), "saved={saved}: {out:?}");
        assert_eq!(slice5a_snapshot(&f.0), before);
    }
}
