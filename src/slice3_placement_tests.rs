use super::*;

#[test]
fn slice3_stale_listing_new_layout_does_not_duplicate_generated_slots() {
    let snapshot = hair::local_explorer::Snapshot {
        folder: "/slice3-stale".into(),
        // Page one avoids the page-zero saturating duplicate, whose legitimate
        // route diagnostic rolls the whole group back before positions can be
        // observed (original failure retained in slice3-corrections-red.log).
        entries: (0..14)
            .map(|i| hair::local_explorer::Entry {
                name: format!("B-{i:02}.txt").into(),
                path: PathBuf::from("/slice3-stale").join(format!("B-{i:02}.txt")),
                kind: hair::local_explorer::Kind::File,
            })
            .collect(),
    };
    let (world, mut host) = hair::folder_scene::build(&snapshot, 1, true).unwrap();
    let before = host.anchors.clone();
    let layout = hair::placement_store::LayoutRead {
        positions: vec![(
            "A-new.txt".into(),
            Position {
                properties: Default::default(),
                at: [2., 0.],
                yaw: 0.,
                size: [0.8, 0.85, 0.28],
                allow_overlap: false,
                surface: None,
            },
        )],
        ..Default::default()
    };
    let notes = hair::placement_scene::apply(&layout, &world, &mut host).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    for id in ["entry-0", "entry-1"] {
        assert_eq!(
            host.anchors.iter().find(|a| a.id == id).unwrap().at,
            before.iter().find(|a| a.id == id).unwrap().at,
            "absent saved name must not consume a snapshot slot"
        );
    }
}

#[test]
fn slice3_ignored_saved_reposition_preserves_generated_rank() {
    let f = fixture("slice3-ignored-saved");
    for n in ["A.txt", "Z.txt"] {
        std::fs::write(f.0.join(n), n).unwrap();
    }
    let mut explorer = hair::local_explorer::Explorer::new(&f.0).unwrap();
    let invalid = Position {
        properties: Default::default(),
        at: [60., 0.],
        yaw: 0.,
        size: [0.8, 0.85, 0.28],
        allow_overlap: false,
        surface: None,
    };
    // Schema-valid but outside room: a legacy file-only record is ignored at admission.
    assert!(matches!(
        hair::placement_store::publish(
            &explorer,
            OsStr::new("B.txt"),
            b"bytes",
            &invalid,
            &std::sync::atomic::AtomicBool::new(false)
        ),
        hair::placement_store::Outcome::Committed { .. }
    ));
    explorer.refresh().unwrap();
    let (world, mut before) = hair::folder_scene::build(explorer.current(), 0, true).unwrap();
    let layout = hair::placement_store::read_layout(&explorer).unwrap();
    assert!(
        !hair::placement_scene::apply(&layout, &world, &mut before)
            .unwrap()
            .is_empty()
    );
    let z = world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == "Z.txt")
        .unwrap()
        .0;
    let at = before.anchors.iter().find(|a| &a.id == z).unwrap().at;
    let target = hair::placement_store::select_existing(&explorer, OsStr::new("B.txt")).unwrap();
    let valid = Position {
        at: [2., 0.],
        ..invalid
    };
    let outcome = hair::placement_store::reposition_checked(&explorer, &target, &valid, |layout| {
        hair::placement_scene::validate_reposition(
            &explorer,
            layout,
            OsStr::new("B.txt"),
            &valid,
            [0., 5.],
            0,
        )
        .map(|_| ())
    });
    assert!(
        matches!(outcome, hair::placement_store::Outcome::Committed { .. }),
        "{outcome:?}"
    );
    let layout = hair::placement_store::read_layout(&explorer).unwrap();
    let (world, mut after) = hair::folder_scene::build(explorer.current(), 0, true).unwrap();
    assert!(
        hair::placement_scene::apply(&layout, &world, &mut after)
            .unwrap()
            .is_empty()
    );
    assert_eq!(after.anchors.iter().find(|a| &a.id == z).unwrap().at, at);
    assert!(layout.retained_slots.contains(&"B.txt".into()));
}
