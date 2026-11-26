//! Bounded owned R2B qualification; never owner folders or renderer substitutes.
use super::*;
use hair::placement_store as store;
#[path = "r2b_qualification_persistence_tests.rs"]
mod persistence;

fn saved_follow(a: &mut App) -> store::Decoration {
    tap(a, KeyCode::KeyP); tap(a, KeyCode::KeyB); tap(a, KeyCode::Enter);
    a.legacy_follow_fixture();
    enter_positioning(a); aim_floor(a, [-2., 0.]);
    tap(a, KeyCode::Enter); tap(a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration saved"), "{}", a.local_status);
    store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone()
}
fn edit_decoration(a: &mut App, d: &store::Decoration) {
    a.pose = Pose { position: [d.at[0], d.at[1]+1.1], yaw: 0.,
        pitch: ((d.size[1]*0.5-a.pose.eye_height())/1.1).atan(), ..a.pose };
    tap(a, KeyCode::KeyX); assert_eq!(a.editor_phase(), "Editing", "{}", a.local_status);
}
fn painted(mesh: &[hair::assets::Vertex], size: [u32;2], at: [f32;2], text: &str) {
    let p = hair::placement::toolbox::paint(size,&[],&[([at[0]-6.,at[1]-7.,at[0]+600.,at[1]+20.],text)],None);
    let glyphs=&p[12..];
    assert!(mesh.windows(glyphs.len()).any(|v|bytes(v)==bytes(glyphs)),"not visibly painted: {text}");
}
#[test]
fn r2b_qual_interrupted_resize_actual_app_and_inspector_disclose_retained_unsupported() {
    for listing in [false, true] {
        let (f, mut a) = open_fixture("r2b-qual-interrupted");
        let originals = slice4c::records(&f.0);
        let old = saved_follow(&mut a);
        let active = slice4c::records(&f.0);
        edit_decoration(&mut a, &old); size_control(&mut a, "Size: 150%");
        enter_positioning(&mut a); aim_floor(&mut a, [-2., 0.]);
        tap(&mut a, KeyCode::Enter); assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
        let proposed = a.decoration_position().unwrap();
        let root = f.0.clone();
        a.place_mode.as_mut().unwrap().decoration_edit.as_mut().unwrap().validation_probe = Some(Box::new(move |stage| {
            if stage == 2 {
                if listing { std::fs::write(root.join("late-listing.txt"), b"owned final-listing refusal").unwrap(); }
                else { return Err("owned post-preparation refusal".into()); }
            }
            Ok(())
        }));
        tap(&mut a, KeyCode::Enter);
        let pending = f.0.join(format!(".hair-layout/dc-{}/pending.toml", old.placement_id));
        assert_eq!(std::fs::read_to_string(&pending).unwrap(), toml::to_string(&proposed).unwrap());
        assert!(a.local_status.contains(if listing { "External listing changed" } else { "owned post-preparation refusal" }), "{}", a.local_status);
        let live = store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
        assert_eq!(live.decorations, vec![old.clone()]); assert_eq!(live.incomplete_records, 1);
        for (p, s) in &active { assert_eq!(&slice4c::records(&f.0)[p], s); }
        assert!(a.local_status.contains("Pending resize retained") && a.local_status.contains("not resumable") && a.local_status.contains("recovery required"), "{}", a.local_status);
        let ui = a.place_ui();
        if a.editor_compact() {
            painted(&ui,a.inventory_frame().full,[8.,78.],"Pending resize retained; not resumable; recovery required");
            click(&mut a,place_mode_native::Control::EditorDetails);
            let expanded=a.place_ui(); let x=(a.inventory_frame().full[0] as f32-632.).max(0.)+8.;
            painted(&expanded,a.inventory_frame().full,[x,100.],"Pending resize retained; not resumable");
            painted(&expanded,a.inventory_frame().full,[x,110.],"Recovery required");
        } else {
            let x = (a.inventory_frame().full[0] as f32-632.).max(0.)+8.;
            painted(&ui,a.inventory_frame().full,[x,100.],"Pending resize retained; not resumable");
            painted(&ui,a.inventory_frame().full,[x,110.],"Recovery required");
        }
        let retained = slice4c::records(&f.0);
        tap(&mut a, KeyCode::Escape); tap(&mut a, KeyCode::Escape);
        click(&mut a, place_mode_native::Control::InspectRecovery);
        let status = a.recovery_status();
        assert!(status.contains("Pending resize retained") && status.contains("not resumable") && status.contains("recovery required"), "{status}");
        let ui=a.recovery_ui(); let x=(a.inventory_frame().full[0] as f32-316.).max(0.)+8.;
        painted(&ui,a.inventory_frame().full,[x,160.],"Pending resize retained; not resumable");
        painted(&ui,a.inventory_frame().full,[x,170.],"Recovery required");
        let i = store::inspect_placements(&a.local.as_ref().unwrap().explorer).unwrap();
        let row = i.attempts().iter().position(|r| r.identity() == format!("dc-{}", old.placement_id)).unwrap();
        assert_eq!(format!("{:?}", i.attempts()[row].kind()), "DecorationResize");
        assert!(i.select_decoration_resume(row).err().unwrap().contains("not resumable"));
        let intent = store::DecorationResize::new(&old, &proposed).unwrap();
        assert!(matches!(store::resize_decoration_checked(&a.local.as_ref().unwrap().explorer, &intent, |_| panic!("blocked write reached callback")), store::Outcome::NotCommitted(_)));
        assert_eq!(slice4c::records(&f.0), retained);
        assert_eq!(a.themes[0].host.decorations[&old.placement_id], old);
        for (p, s) in originals { assert_eq!(retained[&p], s); }
    }
}
