// Storage/API witnesses only; not App input or owner-folder qualification.
fn slice5a_decoration(at: [f32; 2]) -> Decoration {
    Decoration::from_entry(&crate::decoration::Entry {
        id: "chair".into(), tag: "hair.seating.chair".into(), asset: "mesh".into(),
        name: "Chair".into(), defaults: crate::decoration::Defaults { collision: true },
    }, at, 0., [0.8; 3]).unwrap()
}
fn slice5a_record(f: &Fixture, d: &Decoration) -> PathBuf {
    f.0.join(".hair-layout").join(format!("dc-{}", d.placement_id)).join("decoration.toml")
}
#[test]
fn slice5a_real_candidate_scene_replaces_old_bounds_and_rejects_other_overlap() {
    let f = Fixture::new();
    let e = Explorer::new(&f.0).unwrap();
    let d = slice5a_decoration([-2., 0.]);
    let other = slice5a_decoration([2., 0.]);
    for record in [&d, &other] {
        assert_eq!(place_decoration_checked(&e, record, |live|
            crate::placement_scene::validate_decoration(&e, live, record, [0., 2.], 0)), Outcome::Committed { synced: true });
    }
    let mut proposed = d.clone(); proposed.at = [-1.9, 0.];
    let mut calls = 0;
    let out = relocate_decoration_checked(&e, &d, &proposed, |candidate| {
        calls += 1;
        crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)
    });
    assert_eq!(out, Outcome::Committed { synced: true }, "old envelope must be replaced, not retained alongside new");
    assert_eq!(calls, 2);
    let before = slice4c1_record_snapshot(&f.0);
    let mut blocked = proposed.clone(); blocked.at = other.at; blocked.collision = false;
    let out = relocate_decoration_checked(&e, &proposed, &blocked, |candidate|
        crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0));
    assert_eq!(out, Outcome::NotCommitted("Placement overlaps a saved decoration".into()));
    assert_eq!(slice4c1_record_snapshot(&f.0), before);
    assert!(read_layout(&e).unwrap().decorations.contains(&proposed));
}
#[test]
fn slice5a_checked_relocation_reopens_same_identity_and_preserves_other_records() {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new();
    std::fs::write(f.0.join("original.txt"), b"untouched original").unwrap();
    let e = Explorer::new(&f.0).unwrap();
    assert_eq!(publish(&e, OsStr::new("saved.txt"), b"saved source", &pos(), &AtomicBool::new(false)), Outcome::Committed { synced: true });
    let d = slice5a_decoration([-2., 0.]);
    let other = slice5a_decoration([4., 0.]);
    for record in [&d, &other] {
        assert_eq!(place_decoration_checked(&e, record, |_| Ok(())), Outcome::Committed { synced: true });
    }
    let before = slice4c1_record_snapshot(&f.0);
    let originals: Vec<_> = ["original.txt", "saved.txt"].into_iter().map(|name| {
        let path = f.0.join(name);
        (path.clone(), std::fs::metadata(&path).unwrap().ino(), std::fs::read(path).unwrap())
    }).collect();
    let mut proposed = d.clone();
    proposed.at = [-1.9, 0.]; // Overlaps old bounds: replacement, not old + new.
    proposed.yaw = 0.2;
    proposed.collision = false;
    let mut calls = 0;
    let outcome = relocate_decoration_checked(&e, &d, &proposed, |candidate| {
        calls += 1;
        assert!(read_layout(&e).unwrap_err().contains("busy"));
        assert_eq!(candidate.decorations.len(), 2);
        assert!(candidate.decorations.contains(&proposed));
        assert!(candidate.decorations.contains(&other));
        assert!(!candidate.decorations.contains(&d));
        assert_eq!(candidate.positions, vec![("saved.txt".into(), pos())]);
        Ok(())
    });
    assert_eq!(outcome, Outcome::Committed { synced: true });
    assert_eq!(calls, 2);
    let reopened = read_layout(&Explorer::new(&f.0).unwrap()).unwrap();
    assert_eq!(reopened.decorations.len(), 2);
    assert_eq!(reopened.decorations.iter().filter(|x| x.placement_id == d.placement_id).collect::<Vec<_>>(), vec![&proposed]);
    assert!(reopened.decorations.contains(&other));
    assert_eq!(reopened.positions, vec![("saved.txt".into(), pos())]);
    assert_eq!(reopened.incomplete_records, 0);
    assert!(reopened.warnings.is_empty());
    let mut untouched = before;
    untouched.retain(|(path, _, _)| *path != slice5a_record(&f, &d));
    let after = slice4c1_record_snapshot(&f.0);
    for record in untouched { assert!(after.contains(&record), "{record:?}"); }
    for (path, ino, bytes) in originals {
        assert_eq!(std::fs::metadata(&path).unwrap().ino(), ino);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}
