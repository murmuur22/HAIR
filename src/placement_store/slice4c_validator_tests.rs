// Included in placement_store::tests; every mutation stays in owned scratch.
include!("slice4c1_geometry_tests.rs");
include!("slice4c3_occlusion_tests.rs");
#[test]
fn slice4c_checked_file_publication_and_reposition_admit_clear_and_reject_decoration_overlap() {
    let f = Fixture::new();
    std::fs::write(f.0.join("existing.txt"), b"original").unwrap();
    let e = Explorer::new(&f.0).unwrap();
    let entry = crate::decoration::Entry { id: "chair".into(), tag: "hair.seating.chair".into(), asset: "mesh".into(), name: "Chair".into(), defaults: crate::decoration::Defaults { collision: true } };
    let d = Decoration::from_entry(&entry, [-2., 0.], 0.2, [0.8; 3]).unwrap();
    assert!(matches!(place_decoration_checked(&e, &d, |live| crate::placement_scene::validate_decoration(&e, live, &d, [0., 2.], 0)), Outcome::Committed { .. }));
    let mut p = pos(); p.at = [2., 0.];
    let name = OsStr::new("added.txt");
    let outcome = publish_checked(&e, name, b"new file", &p, &AtomicBool::new(false), |live| crate::placement_scene::validate_publication(&e, live, name, &p, [0., 2.], 0).map(|_| ()));
    assert!(matches!(outcome, Outcome::Committed { .. }), "geometry-aware file publication must admit a clear point with saved decorations: {outcome:?}");
    let target = select_existing(&e, OsStr::new("existing.txt")).unwrap();
    p.at = [0., -1.];
    let outcome = reposition_checked(&e, &target, &p, |live| crate::placement_scene::validate_reposition(&e, live, target.name(), &p, [0., 2.], 0).map(|_| ()));
    assert!(matches!(outcome, Outcome::Committed { .. }), "clear reposition: {outcome:?}");
    let target = select_existing(&e, OsStr::new("existing.txt")).unwrap();
    p.at = d.at; p.allow_overlap = true;
    let outcome = reposition_checked(&e, &target, &p, |live| crate::placement_scene::validate_reposition(&e, live, target.name(), &p, [0., 2.], 0).map(|_| ()));
    assert!(matches!(outcome, Outcome::NotCommitted(_)), "file Allow must not bypass decoration envelopes");
    assert_eq!(std::fs::read(f.0.join("existing.txt")).unwrap(), b"original");
    assert_eq!(read_layout(&e).unwrap().decorations, vec![d]);
}

#[test]
fn slice4c_file_writers_repeat_fresh_validator_after_preparation_under_same_lock() {
    for reposition in [false, true] {
        let f = Fixture::new();
        std::fs::write(f.0.join("existing.txt"), b"original").unwrap();
        let e = Explorer::new(&f.0).unwrap();
        let target = select_existing(&e, OsStr::new("existing.txt")).unwrap();
        let mut calls = 0;
        let mut validate = |_: &LayoutRead| {
            calls += 1;
            assert!(read_layout(&e).unwrap_err().contains("busy"));
            if calls == 2 { Err("fresh final geometry refusal".into()) } else { Ok(()) }
        };
        let out = if reposition { mac::reposition(&e, &target, &pos(), &mut validate, |_| Ok(())) }
            else { mac::publish_checked(&e, OsStr::new("added.txt"), b"new", &pos(), &AtomicBool::new(false), &mut validate) };
        assert_eq!(out, Outcome::NotCommitted("fresh final geometry refusal".into()), "both writers must invoke final locked validator");
        assert_eq!(calls, 2);
        assert!(read_layout(&e).unwrap().positions.is_empty());
        assert!(!f.0.join("added.txt").exists());
    }
}
