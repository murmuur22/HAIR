// Production layout/targeting witnesses; all files belong to disposable Fixture.
fn slice4c3_entry() -> crate::decoration::Entry {
    crate::decoration::Entry {
        id: "shelf".into(), tag: "hair.storage.shelf".into(), asset: "mesh".into(),
        name: "Shelf".into(), defaults: Default::default(),
    }
}
fn slice4c3_pose(host: &crate::host_scene::HostScene, id: &str, at: [f32; 2], feet_y: f32) -> crate::Pose {
    let a = host.target_anchor(host.anchors.iter().find(|a| a.id == id).unwrap());
    let delta = glam::Vec3::from_array(a.at) + glam::Vec3::Y * a.size[1] * 0.5
        - glam::Vec3::new(at[0], feet_y + crate::player::eye_offset(host), at[1]);
    crate::Pose {
        position: at, feet_y, eye_offset: crate::player::eye_offset(host),
        yaw: delta.x.atan2(-delta.z),
        pitch: delta.y.atan2(delta.x.hypot(delta.z)).clamp(-1.5, 1.5),
    }
}
fn slice4c3_file_id(world: &crate::World) -> String {
    world.folder_view.as_ref().unwrap().bindings.iter()
        .find(|(_, b)| b.entry.kind == crate::local_explorer::Kind::File).unwrap().0.clone()
}
fn slice4c3_resolves(host: &crate::host_scene::HostScene, world: &crate::World, id: &str, at: [f32; 2], feet_y: f32) -> bool {
    crate::interaction::resolve(host, &slice4c3_pose(host, id, at, feet_y), world)
        .is_some_and(|t| t.anchor == id)
}

#[test]
fn slice4c3_segment_start_inside_and_endpoint_only_controls() {
    use glam::Vec3;
    let hits = |from, to| crate::interaction::segment_hits_box(from, to, [0.; 3], [2.; 3]);
    assert!(hits(Vec3::new(0., 1., 0.), Vec3::new(3., 1., 0.)), "starting inside blocks until exit");
    assert!(hits(Vec3::new(0., 1., 0.), Vec3::new(0.5, 1., 0.)), "entire segment inside blocks");
    assert!(!hits(Vec3::new(3., 1., 0.), Vec3::new(1., 1., 0.)), "touching only at target endpoint does not block");
    assert!(!hits(Vec3::new(1., 1., 0.), Vec3::new(3., 1., 0.)), "starting at face and leaving does not block");
    assert!(hits(Vec3::new(1., 1., 0.), Vec3::new(0., 1., 0.)), "starting at face and entering blocks");
}

#[test]
fn slice4c3_walkthrough_ring_recovers_target_but_inside_box_still_occludes() {
    let f = Fixture::new();
    std::fs::write(f.0.join("target.txt"), b"owned ring control").unwrap();
    let e = Explorer::new(&f.0).unwrap();
    let (world, base) = crate::folder_scene::build(e.current(), 0, true).unwrap();
    let id = slice4c3_file_id(&world);
    let p = Position { at: [0., 0.], yaw: 0., size: [0.4, 0.8, 0.4], allow_overlap: false,
        surface: None, properties: Default::default() };
    let mut layout = LayoutRead { positions: vec![("target.txt".into(), p)], ..Default::default() };
    // Four touching, non-overlapping boxes, taller than the floor eye.
    for (at, size) in [([0., 0.7], [1.6, 2., 0.2]), ([0., -0.7], [1.6, 2., 0.2]),
        ([0.7, 0.], [0.2, 2., 1.2]), ([-0.7, 0.], [0.2, 2., 1.2])] {
        let d = Decoration::from_entry(&slice4c3_entry(), at, 0., size).unwrap();
        assert!(!d.collision);
        layout.decorations.push(d);
    }
    let mut host = base.clone();
    assert!(crate::placement_scene::apply(&layout, &world, &mut host).unwrap().is_empty());
    host.check_routes(&[[0., 0.]]).unwrap();
    assert!(!slice4c3_resolves(&host, &world, &id, [0., 1.2], 0.), "outside ring is occluded");
    assert!(!slice4c3_resolves(&host, &world, &id, [0., 0.7], 0.), "eye starting inside the box is still occluded");
    assert!(slice4c3_resolves(&host, &world, &id, [0., 0.], 0.), "file is non-solid: look down from the accessible central cavity");
    let mut pose = slice4c3_pose(&host, &id, [0., 1.5], 0.);
    pose.pitch = 0.; pose.yaw = 0.;
    let mut player = crate::player::Player::default();
    let input = crate::Input { captured: true, forward: true, ..Default::default() };
    for _ in 0..72 { player.advance(&host, &mut pose, &input, 1. / 120.); }
    assert!(pose.position[1].abs() < 0.03, "actual player traverses the ring: {:?}", pose.position);
    assert!(slice4c3_resolves(&host, &world, &id, pose.position, pose.feet_y));
    let mut solid = layout.clone();
    for d in &mut solid.decorations { d.collision = true; }
    let mut rejected = base.clone();
    assert!(crate::placement_scene::apply(&solid, &world, &mut rejected).unwrap_err().contains("unreachable"));
    assert_eq!(rejected, base, "solid ring must reject atomically");
    println!("SLICE4C3 ring: outside blocked; start-inside blocked; actual walk-through to central cavity recovers file; solid ring rejected");
}

#[test]
fn slice4c3_tall_walkthrough_support_cannot_qualify_an_unseeable_saved_file() {
    let f = Fixture::new();
    std::fs::write(f.0.join("target.txt"), b"owned elevated target").unwrap();
    let e = Explorer::new(&f.0).unwrap();
    let (world, base) = crate::folder_scene::build(e.current(), 0, true).unwrap();
    let id = slice4c3_file_id(&world);
    let d = Decoration::from_entry(&slice4c3_entry(), [0., 0.], 0., [4., 3.4, 4.]).unwrap();
    let p = Position { at: [0., 0.], yaw: 0., size: [0.4; 3], allow_overlap: false,
        surface: Some(crate::placement::surface::Contact { at: [0., d.bounds().unwrap().at[1] + d.size[1], 0.], normal: [0., 1., 0.] }),
        properties: Default::default() };
    let clear_layout = LayoutRead { positions: vec![("target.txt".into(), p.clone())], ..Default::default() };
    let mut clear = base.clone();
    assert!(crate::placement_scene::apply(&clear_layout, &world, &mut clear).unwrap().is_empty());
    assert!(slice4c3_resolves(&clear, &world, &id, [0., 0.], 0.), "same height/reach/pitch is targetable without decor");
    let decor_only = LayoutRead { decorations: vec![d.clone()], ..Default::default() };
    let mut support = base.clone();
    assert!(crate::placement_scene::apply(&decor_only, &world, &mut support).unwrap().is_empty());
    assert!(!support.blocked([0., 0.]));
    assert!(!crate::player::blocked_at(&support, [0., 0.], 0.));
    // Assemble the runtime witness without asking the admission oracle to bless it.
    // The file only touches the decor top, so production overlap clearance passes.
    let mut trapped = clear.clone();
    trapped.decorations = support.decorations.clone();
    crate::placement::surface::clearance(&support, &p.envelope_mesh().unwrap(), crate::placement::Overlap::Prevent).unwrap();
    let mut pose = slice4c3_pose(&trapped, &id, [0., 0.], 0.);
    let mut player = crate::player::Player::default();
    let input = crate::Input { captured: true, ..Default::default() };
    player.jump();
    let mut max_eye = pose.eye_height();
    for _ in 0..240 {
        player.advance(&trapped, &mut pose, &input, 1. / 120.);
        max_eye = max_eye.max(pose.eye_height());
        assert!(!slice4c3_resolves(&trapped, &world, &id, pose.position, pose.feet_y));
    }
    assert!(max_eye < 3.4, "jump cannot emerge above decor: {max_eye}");
    // Every possible planar activation point (reach <1.7) is strictly inside the
    // decor's +/-2 footprint. Until eye >3.4 every ray starts in its volume.
    for x in -17..=17 {
        for z in -17..=17 {
            let at = [x as f32 * 0.1, z as f32 * 0.1];
            for feet in [0., max_eye - crate::player::eye_offset(&trapped)] {
                assert!(!slice4c3_resolves(&trapped, &world, &id, at, feet));
            }
        }
    }
    println!("SLICE4C3 elevated: clear target resolves; non-solid decor walks through; max jump eye={max_eye}; all in-reach eyes remain inside +/-2m decor; no file target");
    let layout = LayoutRead { decorations: vec![d], positions: clear_layout.positions, ..Default::default() };
    let mut admitted = base.clone();
    let outcome = crate::placement_scene::apply(&layout, &world, &mut admitted);
    println!("SLICE4C3 production saved-group admission: {outcome:?}");
    let error = outcome.expect_err("F1: production must reject a group whose only floor-reach target rays start inside walk-through decor");
    assert!(error.contains("unreachable"), "not an unrelated overlap failure: {error}");
    assert_eq!(admitted, base, "rejected saved group must be atomic");
    assert!(trapped.check_routes(&[]).unwrap_err().contains("unreachable"));
}

#[test]
fn slice4c3_checked_file_writers_reject_unseeable_support_top_without_mutation() {
    for reposition in [false, true] {
        let f = Fixture::new();
        std::fs::write(f.0.join("existing.txt"), b"owned unchanged source").unwrap();
        let e = Explorer::new(&f.0).unwrap();
        let d = Decoration::from_entry(&slice4c3_entry(), [0., 0.], 0., [4., 3.4, 4.]).unwrap();
        let visitor = [0., 4.];
        let outcome = place_decoration_checked(&e, &d, |live|
            crate::placement_scene::validate_decoration(&e, live, &d, visitor, 0));
        assert!(matches!(outcome, Outcome::Committed { .. }), "decor alone admits: {outcome:?}");
        let p = Position { at: [0., 0.], yaw: 0., size: [0.4; 3], allow_overlap: false,
            surface: Some(crate::placement::surface::Contact { at: [0., d.bounds().unwrap().at[1] + d.size[1], 0.], normal: [0., 1., 0.] }),
            properties: Default::default() };
        let before = read_layout(&e).unwrap();
        let records = slice4c1_record_snapshot(&f.0);
        let outcome = if reposition {
            let target = select_existing(&e, OsStr::new("existing.txt")).unwrap();
            reposition_checked(&e, &target, &p, |live|
                crate::placement_scene::validate_reposition(&e, live, target.name(), &p, visitor, 0).map(|_| ()))
        } else {
            publish_checked(&e, OsStr::new("new.txt"), b"must not publish", &p, &AtomicBool::new(false), |live|
                crate::placement_scene::validate_publication(&e, live, OsStr::new("new.txt"), &p, visitor, 0).map(|_| ()))
        };
        println!("SLICE4C3 checked writer reposition={reposition}: {outcome:?}");
        assert!(matches!(outcome, Outcome::NotCommitted(ref e) if e.contains("unreachable")), "target visibility must reject, not publish: {outcome:?}");
        assert_eq!(read_layout(&e).unwrap(), before);
        assert_eq!(slice4c1_record_snapshot(&f.0), records);
        assert_eq!(std::fs::read(f.0.join("existing.txt")).unwrap(), b"owned unchanged source");
        assert!(!f.0.join("new.txt").exists());
    }
}
