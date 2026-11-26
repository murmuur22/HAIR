use super::*;
#[path = "slice4c5_app_tests.rs"]
mod upper_file_occlusion;

fn decoration(a: &App, id: &str, at: [f32; 2], size: [f32; 3]) -> hair::placement_store::Decoration {
    let mut d = hair::placement_store::Decoration::from_entry(&a.themes[0].decorations.entries[0], at, 0., size).unwrap();
    d.placement_id = id.repeat(32);
    d
}
fn aim_decoration(a: &mut App, d: &hair::placement_store::Decoration, distance: f32) {
    a.pose = Pose { position: [d.at[0], d.at[1] + distance], pitch: ((d.size[1] * 0.5 - a.pose.eye_height()) / distance).atan(), ..a.pose };
}
fn target(a: &App) -> Option<String> {
    hair::interaction::decoration_target(&a.themes[a.theme_index].host, &a.pose, &a.world).map(|d| d.placement_id.clone())
}

#[test]
fn slice4c1_target_uses_placement_identity_aim_reach_low_eye_and_all_real_occluders() {
    use hair::host_scene::{Anchor, Geometry};
    let (_f, mut a) = open_fixture("slice4c1-target");
    let d = decoration(&a, "a", [0., 0.], [0.8, 1., 0.6]);
    let host = &mut a.themes[0].host;
    host.anchors.clear(); host.geometry.clear(); host.overhead.clear();
    host.file_placements.clear(); host.open_platform = true;
    host.decorations.insert(d.placement_id.clone(), d.clone());
    for eye in [1.65, 0.65] {
        a.pose.eye_offset = eye;
        aim_decoration(&mut a, &d, 1.5);
        assert_eq!(target(&a), Some(d.placement_id.clone()));
        a.pose.yaw = 1.0;
        assert_eq!(target(&a), None, "off-aim must not inspect nearby decoration");
        a.pose.yaw = 0.;
        aim_decoration(&mut a, &d, 3.5);
        assert_eq!(target(&a), None, "out of reach");
    }
    a.pose.eye_offset = 0.65;
    aim_decoration(&mut a, &d, 1.5);
    let baseline = a.themes[0].host.clone();
    for kind in ["furniture", "solid-base", "geometry", "overhead", "decoration", "wall"] {
        a.themes[0].host = baseline.clone();
        let h = &mut a.themes[0].host;
        let g = Geometry { at: [0., 0., 0.8], size: [0.7, 1.2, 0.2] };
        match kind {
            "furniture" => h.anchors.push(Anchor { id: kind.into(), role: "decoration".into(), at: g.at, size: g.size, binding: "none".into() }),
            "solid-base" => {
                let p = hair::placement_store::Position { at: [0., 0.8], yaw: 0., size: [0.8, 0.85, 0.28], allow_overlap: false, surface: None,
                    properties: hair::placement::assembly::Properties { base: true, base_collision: true, ..Default::default() } };
                h.anchors.push(Anchor { id: kind.into(), role: "document".into(), at: g.at, size: g.size, binding: "explicit-page".into() });
                h.file_anchors.insert(kind.into());
                h.file_placements.insert(kind.into(), hair::host_scene::PlacedItem { bounds: p.bounds().unwrap(), position: p });
                assert_eq!(h.solid_file_bases().len(), 1);
            }
            "geometry" => h.geometry.push(g),
            "overhead" => h.overhead.push(g),
            "decoration" => {
                let mut other = d.clone(); other.placement_id = "b".repeat(32); other.at[1] = 0.8;
                h.decorations.insert(other.placement_id.clone(), other);
            }
            "wall" => { h.open_platform = false; h.room = [-3., -3., 3., 0.6]; }
            _ => unreachable!(),
        }
        assert_ne!(target(&a), Some(d.placement_id.clone()), "{kind} occludes saved decoration");
    }
    a.themes[0].host = baseline;
    a.pose.pitch = f32::NAN;
    assert_eq!(target(&a), None);
}

#[test]
fn slice4c1_occlusion_follows_actual_aim_ray_not_unrelated_centre_line() {
    let (_f, mut a) = open_fixture("slice4c1-ray");
    let d = decoration(&a, "a", [0., 0.], [2., 1., 0.6]);
    let h = &mut a.themes[0].host;
    h.anchors.clear(); h.geometry.clear(); h.overhead.clear(); h.open_platform = true;
    h.decorations.insert(d.placement_id.clone(), d.clone());
    a.pose = Pose { position: [0., 1.5], eye_offset: 0.65, yaw: 0.4, pitch: 0., ..Default::default() };
    assert_eq!(target(&a), Some(d.placement_id.clone()));
    a.themes[0].host.geometry.push(hair::host_scene::Geometry { at: [0.30, 0., 0.8], size: [0.16, 1., 0.1] });
    assert_eq!(target(&a), None, "actual ray is blocked even though decoration centre line is clear");
    a.themes[0].host.geometry[0].at[0] = 0.;
    assert_eq!(target(&a), Some(d.placement_id), "a clear aimed edge must not be hidden by an unrelated centre line");
}

#[test]
fn slice4c1_inspection_clears_stale_file_edit_and_never_leaks_file_x_or_b() {
    let (f, mut a) = open_fixture("slice4c1-inspect");
    start(&mut a, [-2., 0.], true);
    tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    let d = layout(&a).decorations[0].clone();
    aim_file(&mut a, "A.txt"); tap(&mut a, KeyCode::KeyX);
    assert!(a.place_mode.as_ref().unwrap().edit.is_some());
    aim_decoration(&mut a, &d, 1.1);
    let before = records(&f.0);
    tap(&mut a, KeyCode::KeyX);
    assert!(a.local_status.contains(&d.placement_id), "{}", a.local_status);
    // 5B deliberately replaces inspection-only with explicit decoration review.
    assert!(a.local_status.contains("decoration reposition"));
    assert!(a.local_status.contains("Existing Follow theme") && a.local_status.contains("conversion not supported"));
    assert!(!a.toolbox_controls().iter().any(|(c, _, _)| matches!(c,
        place_mode_native::Control::FollowTheme | place_mode_native::Control::PinAsset)));
    assert!(a.place_mode.as_ref().unwrap().edit.is_none(), "inspection must retire the previously selected file, not leave Enter armed for it");
    assert_eq!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().expected(), Some(&d));
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]);
    assert!(a.place_preview().is_ok());
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("frozen decoration reposition review"));
    assert_eq!(records(&f.0), before);
    tap(&mut a, KeyCode::KeyB);
    assert_eq!(records(&f.0), before, "B cannot acquire or save the previously selected file");
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::Enter);
    assert!(a.inventory.items().is_empty());
    assert!(a.placement_action.is_none());
    assert_eq!(records(&f.0), before);
}

#[test]
fn slice4c1_app_first_review_rejects_spawn_doors_bounds_overlap_and_late_visitor_trap() {
    let (f, mut a) = open_fixture("slice4c1-admission");
    let host = a.themes[0].host.clone();
    let door = host.anchors.iter().find(|an| an.role == "portal").unwrap().at;
    let object = host.anchors.iter().find(|an| an.role == "document").unwrap().at;
    for at in [host.spawn, [door[0], door[2]], [object[0], object[2]], [host.room[2], 0.]] {
        start(&mut a, at, true);
        let before = records(&f.0);
        tap(&mut a, KeyCode::Enter);
        assert!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().pending.is_none(), "invalid first review at {at:?}: {}", a.local_status);
        assert_eq!(records(&f.0), before);
        tap(&mut a, KeyCode::Escape);
    }
    start(&mut a, [-2., 0.], true);
    tap(&mut a, KeyCode::Enter);
    let d = a.decoration_position().unwrap();
    a.pose.position = d.at; // Visitor moved after frozen review; final admission must be fresh.
    let before = records(&f.0);
    tap(&mut a, KeyCode::Enter);
    assert!(!a.local_status.contains("decoration saved"), "late visitor trap: {}", a.local_status);
    assert!(layout(&a).decorations.is_empty());
    // Storage preparation may create metadata, but never a live decoration or source changes.
    for (path, bytes) in before { assert_eq!(records(&f.0).get(&path), Some(&bytes)); }
}

#[test]
fn slice4c1_rehydration_is_group_order_independent_and_preserves_unrelated_geometry() {
    let (_f, mut a) = open_fixture("slice4c1-order");
    aim_file(&mut a, "A.txt"); tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
    aim_floor(&mut a, [2., 0.]); tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    for at in [[-2., 0.], [0., -1.]] { start(&mut a, at, true); tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter); }
    let saved = layout(&a);
    let snapshot = a.local.as_ref().unwrap().explorer.current();
    let (w, base) = hair::folder_scene::build(snapshot, 0, true).unwrap();
    let mut h1 = base.clone(); let mut h2 = base.clone();
    let mut reverse = saved.clone(); reverse.positions.reverse(); reverse.decorations.reverse();
    assert!(hair::placement_scene::apply(&saved, &w, &mut h1).unwrap().is_empty());
    assert!(hair::placement_scene::apply(&reverse, &w, &mut h2).unwrap().is_empty());
    assert_eq!(h1, h2);
    assert_eq!(h1, a.themes[0].host);
    let mut invalid = saved.clone(); invalid.decorations[1].at = invalid.decorations[0].at;
    for reversed in [false, true] {
        if reversed { invalid.decorations.reverse(); }
        let mut h = base.clone();
        assert!(hair::placement_scene::apply(&invalid, &w, &mut h).is_err());
        assert_eq!(h, base, "invalid group must be atomic");
    }
}
