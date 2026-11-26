// Genuine final fresh-listing/page and dynamic host-policy oracles.
#[test]
fn slice8b_late_listing_adds_other_page_overlap_only_after_successful_review() {
    let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap(); let theme = slice8b_theme();
    let mut d = slice5a_decoration([4.5, 3.]); d.size = [0.8, 0.2, 0.8];
    let mut prepared = false;
    let out = mac::place_decoration(&e, &d, &mut |live| {
        theme.validate_decoration_layout(live, &d)?;
        crate::placement_scene::validate_decoration(&e, live, &d, [0., 2.], 0)
    }, |at| if at == 1 { prepared = true; Err("prepared page witness".into()) } else { Ok(()) });
    assert!(prepared); assert_eq!(out, Outcome::NotCommitted("prepared page witness".into()));
    let i = inspect_placements(&e).unwrap(); let s = i.select_decoration_resume(0).unwrap(); let before = slice8a_snapshot(&f.0);
    let mut stages = vec![];
    let out = theme.resume_decoration_checked(&e, &s, &AtomicBool::new(false), |stage, candidate| {
        stages.push(stage);
        if stage == ResumeStage::Final {
            for n in 0..13 { std::fs::write(f.0.join(format!("file-{n:02}.txt")), b"late actual page entry").unwrap(); }
            let mut fresh = e.try_clone()?; fresh.refresh()?;
            let (world, mut host) = crate::folder_scene::build(fresh.current(), 0, true)?;
            assert!(crate::placement_scene::apply(candidate, &world, &mut host)?.is_empty(), "page zero still admits; only other page supplies the overlap");
        }
        crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)
    });
    assert_eq!(stages, vec![ResumeStage::Review, ResumeStage::Final]);
    assert_eq!(out, Outcome::NotCommitted("Placement overlaps an existing object".into()));
    let after = slice8a_snapshot(&f.0); for entry in before.files { assert!(after.files.contains(&entry)); } for entry in before.attrs { assert!(after.attrs.contains(&entry)); }
    println!("SLICE8B late-other-page stages={stages:?} outcome={out:?} page0=admitted pending=exact");
}
#[test]
fn slice8b_final_host_door_clearance_and_route_policy_are_mandatory_caller_checks() {
    for kind in ["door", "route"] {
        let (f, e, theme, d) = slice8b_prepare("new"); let i = inspect_placements(&e).unwrap(); let s = i.select_decoration_resume(slice8b_index(&i, &d)).unwrap();
        let before = slice8a_snapshot(&f.0); let mut stages = vec![];
        let out = theme.resume_decoration_checked(&e, &s, &AtomicBool::new(false), |stage, candidate| {
            stages.push(stage);
            crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)?;
            if stage == ResumeStage::Final {
                let mut fresh = e.try_clone()?; fresh.refresh()?;
                let (world, mut host) = crate::folder_scene::build(fresh.current(), 0, true)?;
                if kind == "door" {
                    // The caller's newly checked door policy is additional to
                    // generated-folder geometry, never generic storage permission.
                    let portal = host.anchors.iter_mut().find(|a| a.role == "portal").unwrap();
                    portal.at = [d.at[0], 0., d.at[1]];
                    portal.size = [1.45, 2., 0.6];
                } else {
                    // Real route checker: a closed rectangle around the visitor,
                    // disjoint from the proposal's own envelope.
                    for (at, size) in [([0., 0., 1.3], [1.6, 2., 0.2]), ([0., 0., 2.7], [1.6, 2., 0.2]), ([-0.7, 0., 2.], [0.2, 2., 1.2]), ([0.7, 0., 2.], [0.2, 2., 1.2])] {
                        host.geometry.push(crate::host_scene::Geometry { at, size });
                    }
                }
                crate::placement_scene::apply(candidate, &world, &mut host)?;
                host.check_routes(&[[0., 2.]])?;
            }
            Ok(())
        });
        assert_eq!(stages, vec![ResumeStage::Review, ResumeStage::Final]);
        assert!(matches!(out, Outcome::NotCommitted(_)), "{kind}: {out:?}"); assert_eq!(slice8a_snapshot(&f.0), before);
        println!("SLICE8B final-host-policy={kind} stages={stages:?} outcome={out:?}");
    }
}
