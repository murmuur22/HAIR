use super::*;

// Saved via the real checked writer and rehydrated by F5. The eye ray goes
// through a triangle centroid of the actual upper-file render mesh, not a
// guessed document centre or a synthetic local-file binding.
fn upper_file_witness(label: &str) -> (Fixture, App, hair::placement_store::Decoration, String) {
    let (f, mut a) = open_fixture(label);
    let id = file_id(&a);
    let base = a.themes[0].host.anchors.iter().find(|an| an.id == id).unwrap();
    let d = decoration(&a, "c", [base.at[0], base.at[2] - 1.2], [0.8, 2.4, 0.6]);
    let local = a.local.as_ref().unwrap();
    let outcome = hair::placement_store::place_decoration_checked(&local.explorer, &d, |live|
        hair::placement_scene::validate_decoration(&local.explorer, live, &d, a.pose.position, local.page));
    assert!(matches!(outcome, hair::placement_store::Outcome::Committed { .. }), "fixture must save: {outcome:?}");
    tap(&mut a, KeyCode::F5);
    assert_eq!(layout(&a).decorations, vec![d.clone()]);
    assert_eq!(a.themes[0].host.decorations.get(&d.placement_id), Some(&d));
    let id = file_id(&a);
    let base = a.themes[0].host.anchors.iter().find(|an| an.id == id).unwrap();
    let mesh = a.themes[0].file_item_shape(&id, std::ffi::OsStr::new("A.txt"));
    let point = mesh.as_chunks::<3>().0.iter().map(|t|
        (glam::Vec3::from_array(t[0].position) + glam::Vec3::from_array(t[1].position)
            + glam::Vec3::from_array(t[2].position)) / 3.)
        .filter(|p| p.y > base.at[1] + base.size[1] + 0.12)
        .min_by(|a, b| (a.y - 1.3).abs().total_cmp(&(b.y - 1.3).abs())).expect("real upper-file triangles");
    assert!(point.y > base.at[1] + base.size[1] + 0.12, "upper-file triangle, above raw pedestal");
    a.pose = Pose { position: [point.x, base.at[2] + 0.95], eye_offset: 1.65,
        pitch: (point.y - 1.65).atan2(base.at[2] + 0.95 - point.z), ..Default::default() };
    let direction = glam::Vec3::new(0., a.pose.pitch.sin(), -a.pose.pitch.cos());
    let eye = glam::Vec3::new(a.pose.position[0], a.pose.eye_height(), a.pose.position[1]);
    assert!((eye + direction * eye.distance(point) - point).length() < 0.0001,
        "the production aim ray crosses an actual rendered triangle");
    assert_eq!(hair::interaction::resolve(&a.themes[0].host, &a.pose, &a.world).unwrap().anchor, id);
    let blocked = a.themes[0].host.blocked([base.at[0], base.at[2]]);
    assert!(!blocked && !hair::player::blocked_at(&a.themes[0].host, [base.at[0], base.at[2]], 0.),
        "generated file remains non-solid");
    println!("SLICE4C5 witness: upper triangle={point:?}, pose={:?}, decoration={:?}", a.pose, d.at);
    (f, a, d, id)
}
fn file_id(a: &App) -> String {
    a.world.folder_view.as_ref().unwrap().bindings.iter()
        .find(|(_, b)| b.entry.name == std::ffi::OsStr::new("A.txt")).unwrap().0.clone()
}

#[test]
fn slice4c5_generated_upper_file_occludes_production_decoration_target() {
    let (_f, mut a, d, id) = upper_file_witness("slice4c5-target");
    let baseline = a.themes[0].host.clone();
    // A displaced file leaves the same aim/reach/decoration valid.
    a.themes[0].host.anchors.iter_mut().find(|an| an.id == id).unwrap().at[0] += 2.;
    assert_eq!(target(&a), Some(d.placement_id.clone()), "clear ray control");
    a.themes[0].host = baseline;
    assert_eq!(target(&a), None, "F4: visible generated upper-file item must occlude saved decoration");
}

#[test]
fn slice4c5_generated_upper_file_occludes_actual_app_inspection() {
    let (f, mut a, d, id) = upper_file_witness("slice4c5-app");
    let before = records(&f.0);
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    assert!(a.place_mode.as_ref().unwrap().decoration_edit.is_none(),
        "F4: actual X must not inspect decoration through the rendered file: {}", a.local_status);
    assert!(a.place_mode.as_ref().unwrap().edit.is_some()
        && a.local_status.contains(&format!("Editing {}", hair::folder_scene::display(std::ffi::OsStr::new("A.txt")))),
        "the visible local file, not the occluded decoration, owns this selection: {}", a.local_status);
    assert!(a.inventory.items().is_empty(), "Place Mode selection is not acquisition");
    assert_eq!(records(&f.0), before);

    // Clear only the occluding file, then select the same saved decoration.
    // 5B arms a decoration-only review, never the retired file save.
    a.themes[0].host.anchors.iter_mut().find(|an| an.id == id).unwrap().at[0] += 2.;
    tap(&mut a, KeyCode::KeyX);
    assert!(a.local_status.contains(&d.placement_id) && a.local_status.contains("decoration reposition"), "{}", a.local_status);
    assert!(a.place_mode.as_ref().unwrap().edit.is_none());
    assert_eq!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().expected(), Some(&d));
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]);
    assert!(a.place_preview().is_ok());
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("frozen decoration reposition review"), "{}", a.local_status);
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::Enter);
    assert!(a.inventory.items().is_empty());
    assert!(a.placement_action.is_none());
    assert_eq!(records(&f.0), before, "no stale file edit, acquisition or source/layout write");
}

#[test]
fn slice4c5_upper_envelope_requires_actual_file_binding_but_raw_furniture_still_occludes() {
    let (_f, mut a, d, id) = upper_file_witness("slice4c5-binding");
    let view = a.world.folder_view.clone();
    let host = a.themes[0].host.clone();
    let pose = a.pose;
    for role in ["document", "decoration", "portal"] {
        a.themes[0].host = host.clone();
        a.themes[0].host.anchors.iter_mut().find(|an| an.id == id).unwrap().role = role.into();
        a.world.folder_view = view.clone();
        a.world.folder_view.as_mut().unwrap().bindings.remove(&id);
        assert_eq!(target(&a), Some(d.placement_id.clone()), "unbound {role} has no invented upper file");
        a.pose = Pose { eye_offset: 0.65, pitch: 0., ..pose };
        if role != "portal" {
            assert_eq!(target(&a), None, "raw generated base/furniture still occludes low eye: {role}");
        }
        a.pose = pose;
    }
    a.themes[0].host = host;
    for kind in [hair::local_explorer::Kind::Folder, hair::local_explorer::Kind::Symlink, hair::local_explorer::Kind::Special] {
        a.world.folder_view = view.clone();
        a.world.folder_view.as_mut().unwrap().bindings.get_mut(&id).unwrap().entry.kind = kind;
        assert_eq!(target(&a), Some(d.placement_id.clone()), "non-file binding {kind:?} has no upper item");
    }
    a.world.folder_view = view;
    assert_eq!(target(&a), None, "only the actual local-file binding supplies the upper item");
}

#[test]
fn slice4c5_saved_file_bounds_override_generated_upper_and_animation_label_are_not_authority() {
    let (f, mut a, d, id) = upper_file_witness("slice4c5-saved");
    let before = records(&f.0);
    let base = a.themes[0].host.anchors.iter().find(|an| an.id == id).unwrap().clone();
    // Runtime controls keep the original generated slot as a decoy. Only the
    // saved file bounds, not that slot or presentation settings, may occlude.
    for height in [1.6, 0.3] {
        let p = hair::placement_store::Position { at: [base.at[0], base.at[2]], yaw: 0.,
            size: [0.8, height, 0.28], allow_overlap: false, surface: None,
            properties: hair::placement::assembly::Properties { animated: true, label: false, ..Default::default() } };
        a.themes[0].host.file_placements.insert(id.clone(), hair::host_scene::PlacedItem { bounds: p.bounds().unwrap(), position: p });
        let expected = if height > 1. { None } else { Some(d.placement_id.clone()) };
        assert_eq!(target(&a), expected, "saved bounds are authoritative, with no generated height added");
        let host = a.themes[0].host.clone();
        let still = a.themes[0].file_item_shape_at(&id, std::ffi::OsStr::new("A.txt"), 0.);
        let floating = a.themes[0].file_item_shape_at(&id, std::ffi::OsStr::new("A.txt"), 3.);
        assert_ne!(bytemuck::cast_slice::<_, u8>(&still), bytemuck::cast_slice::<_, u8>(&floating));
        assert_eq!(a.themes[0].host, host);
        assert_eq!(target(&a), expected, "render-only floating cannot change inspection occlusion");
        assert!(!hair::player::blocked_at(&host, [base.at[0], base.at[2]], 0.));
    }
    aim_file(&mut a, "A.txt");
    let t = hair::interaction::resolve(&a.themes[0].host, &a.pose, &a.world).unwrap();
    assert_eq!(t.anchor, id);
    assert!(t.prompt.contains("A.txt") && t.prompt.contains("I source"));
    tap(&mut a, KeyCode::KeyI);
    assert!(a.world.reading && a.world.document().unwrap().contains("A.txt"), "hidden label retains actual I source inspection");
    assert!(a.inventory.items().is_empty());
    assert_eq!(records(&f.0), before);
}
