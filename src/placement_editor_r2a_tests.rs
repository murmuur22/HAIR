//! Actual application-input qualification for the edit-first editor.
use super::*;
#[path = "placement_editor_r2a_metal_tests.rs"]
mod metal;
#[path = "r2b_qualification_app_tests.rs"]
mod qualification;
#[path = "placement_editor_r2c_tests.rs"]
mod animation;
#[path = "persistence_a_tests.rs"]
mod persistence_a;
#[path = "decoration_b_tests.rs"]
mod decoration_b;

fn layout_renderer(a: &mut App, size: [u32; 2]) {
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    a.renderer = Some(pollster::block_on(render::Renderer::new(&adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb, size, &a.themes[0].vertices_for(&a.world))).unwrap());
}
fn point_click(a: &mut App, p: [f32; 2]) {
    let frame = a.inventory_frame();
    a.pointer_moved(Some([f64::from(p[0]) * f64::from(frame.physical[0]) / f64::from(frame.full[0]),
        f64::from(p[1]) * f64::from(frame.physical[1]) / f64::from(frame.full[1])]));
    a.place_pointer(true); a.place_pointer(false);
}
fn has_flat_cover(mesh: &[hair::assets::Vertex], size: [u32; 2], p: [f32; 2]) -> bool {
    let p = [2. * p[0] / size[0] as f32 - 1., 1. - 2. * p[1] / size[1] as f32];
    mesh.as_chunks::<3>().0.iter().any(|t| {
        if t.iter().any(|v| v.color != [0.025, 0.045, 0.035]) { return false; }
        let cross = |a: [f32; 3], b: [f32; 3]| (b[0]-a[0])*(p[1]-a[1])-(b[1]-a[1])*(p[0]-a[0]);
        let c = [cross(t[0].position,t[1].position),cross(t[1].position,t[2].position),cross(t[2].position,t[0].position)];
        c.iter().all(|v| *v >= 0.) || c.iter().all(|v| *v <= 0.)
    })
}
#[test]
fn r2a_layout_m1_positioning_center_is_world_and_recaptures_without_losing_draft() {
    let (f, mut a) = open_fixture("r2a-layout-m1");
    layout_renderer(&mut a, [640,480]);
    aim_file(&mut a,"A.txt"); tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyX);
    tap(&mut a,KeyCode::ArrowRight); tap(&mut a,KeyCode::KeyJ);
    let draft = bytes(&a.editor_geometry(0.).unwrap().0);
    let before = slice4c::records(&f.0);
    tap(&mut a,KeyCode::Enter); aim_floor(&mut a,[2.,0.]);
    for p in [[160.,240.],[320.,240.],[480.,240.]] {
        assert!(!has_flat_cover(&a.place_ui(),[640,480],p),"Positioning world covered at {p:?}");
    }
    tap(&mut a,KeyCode::Tab);
    point_click(&mut a,[320.,240.]);
    assert!(a.input.captured,"central world click must recapture, not hit invisible editor chrome");
    assert_eq!(a.editor_phase(),"Positioning");
    tap(&mut a,KeyCode::Tab);
    click(&mut a,place_mode_native::Control::EditorDetails);
    assert!(!a.editor_compact());
    assert_eq!(bytes(&a.editor_geometry(0.).unwrap().0),draft);
    for p in [[160.,240.],[480.,148.]] {
        point_click(&mut a,p); assert!(!a.input.captured,"diagnostics/artwork must own clicks");
    }
    click(&mut a,place_mode_native::Control::Right);
    let changed = bytes(&a.editor_geometry(0.).unwrap().0);
    assert_ne!(changed,draft);
    click(&mut a,place_mode_native::Control::EditorDetails);
    assert!(a.editor_compact());
    assert_eq!(bytes(&a.editor_geometry(0.).unwrap().0),changed);
    tap(&mut a,KeyCode::Escape);
    assert_eq!(a.editor_phase(),"Editing");
    assert_eq!(bytes(&a.editor_geometry(0.).unwrap().0),changed);
    assert_eq!(slice4c::records(&f.0),before);
}
#[test]
fn r2a_layout_midwidth_positioning_center_remains_visible_and_recaptures() {
    for size in [[960,640], [1024,640], [1260,800]] {
        let (f, mut a) = open_fixture("r2a-midwidth");
        layout_renderer(&mut a, size);
        aim_file(&mut a,"A.txt");
        tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyX);
        let before = slice4c::records(&f.0);
        tap(&mut a,KeyCode::Enter); aim_floor(&mut a,[2.,0.]);
        let full = a.inventory_frame().full;
        let center = [full[0] as f32 / 2., full[1] as f32 / 2.];
        assert!(!has_flat_cover(&a.place_ui(),full,center),"Positioning center obscured at {size:?}");
        tap(&mut a,KeyCode::Tab); point_click(&mut a,center);
        assert!(a.input.captured,"world center failed recapture at {size:?}");
        assert_eq!(a.editor_phase(),"Positioning");
        assert_eq!(slice4c::records(&f.0),before);
    }
}
#[test]
fn r2a_layout_m2_actual_painted_controls_are_legible_and_inside_panel_above_footer() {
    let (_f, mut a) = open_fixture("r2a-layout-m2");
    layout_renderer(&mut a,[640,480]);
    // Exercise a real catalog arrival/cancel, then the file's property pages.
    tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyB); tap(&mut a,KeyCode::Enter);
    tap(&mut a,KeyCode::Escape);
    aim_file(&mut a,"A.txt"); tap(&mut a,KeyCode::KeyX);
    let before = slice4c::records(&_f.0);
    for physical in [[1280,800],[640,480]] {
      a.renderer.as_mut().unwrap().resize(physical);
      for scale in hair::presentation::STEPS {
        a.ui_scale = scale; a.update_hud();
        let size = a.inventory_frame().full;
        while a.toolbox_controls().iter().any(|(c,_,_)|*c==place_mode_native::Control::EditorPrevious) {
            click(&mut a,place_mode_native::Control::EditorPrevious);
        }
        let mut seen = vec![];
        loop {
        let mesh = a.place_ui();
        let controls = a.toolbox_controls();
        for (i,(control,r,label)) in controls.iter().enumerate() {
            assert!(r[3]-r[1]>=19.,"illegible real row: {label} {r:?}");
            assert!(r[3]<=size[1] as f32-104.,"control outside panel / footer: {label} {r:?}");
            assert!(label.chars().count() as f32*6.<=r[2]-r[0]-12.,"label does not fit: {label}");
            assert!(controls[..i].iter().all(|(_,q,_)|r[0]>=q[2]||q[0]>=r[2]||r[1]>=q[3]||q[1]>=r[3]));
            let template = hair::placement::toolbox::paint(size,&[],&[([r[0],r[1]-4.,r[2],r[3]],*label)],None);
            let expected = &template[12..];
            assert!(mesh.windows(expected.len()).any(|v|bytes(v)==bytes(expected)),"actual drawn full label missing: {label}");
            let frame = a.inventory_frame();
            a.pointer_moved(Some([f64::from(r[0]+2.)*f64::from(frame.physical[0])/f64::from(frame.full[0]),
                f64::from(r[1]+2.)*f64::from(frame.physical[1])/f64::from(frame.full[1])]));
            a.place_pointer(true); a.pointer_moved(None); a.place_pointer(false);
            assert!(!a.input.captured,"every drawn target owns its press, even when dragged off");
            seen.push(*control);
        }
        if !controls.iter().any(|(c,_,_)|*c==place_mode_native::Control::EditorNext) { break; }
        click(&mut a,place_mode_native::Control::EditorNext);
        }
        for c in [place_mode_native::Control::Base,place_mode_native::Control::Collision,
            place_mode_native::Control::Animation,place_mode_native::Control::Label,
            place_mode_native::Control::Enter,place_mode_native::Control::Exit] { assert!(seen.contains(&c),"missing actual property {c:?}"); }
        let old = bytes(&a.editor_geometry(0.).unwrap().0);
        click(&mut a,place_mode_native::Control::Right);
        assert_ne!(bytes(&a.editor_geometry(0.).unwrap().0),old);
        assert!(!a.input.captured,"painted button click must not fall through");
        assert_eq!(slice4c::records(&_f.0),before);
      }
    }
}

#[test]
fn r2a_x_enters_editing_before_positioning_or_review() {
    let (f, mut a) = open_fixture("r2a-edit-first");
    aim_file(&mut a, "A.txt");
    let host = a.themes[0].host.clone();
    let inode = std::fs::metadata(f.0.join("A.txt")).unwrap().ino();
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    assert!(a.local_status.contains("Editing"), "{}", a.local_status);
    assert!(!a.input.captured);
    aim_floor(&mut a, [2., 0.]);
    a.key(KeyCode::Enter, true, false).unwrap();
    assert!(a.local_status.contains("Positioning"), "{}", a.local_status);
    assert!(!a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().confirming());
    a.key(KeyCode::Enter, true, true).unwrap();
    a.key(KeyCode::Enter, true, false).unwrap();
    assert!(!a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().confirming());
    a.key(KeyCode::Enter, false, false).unwrap();
    assert_eq!(a.themes[0].host, host);
    assert!(!f.0.join(".hair-layout").exists());
    assert_eq!(std::fs::metadata(f.0.join("A.txt")).unwrap().ino(), inode);
    assert_eq!(std::fs::read(f.0.join("A.txt")).unwrap(), b"A.txt");
    assert!(a.inventory.items().is_empty());
}

fn bytes(mesh: &[hair::assets::Vertex]) -> Vec<u8> { bytemuck::cast_slice(mesh).to_vec() }
fn size_control(a: &mut App, label: &str) {
    while a.toolbox_controls().iter().any(|(c,_,_)| *c == place_mode_native::Control::EditorPrevious) {
        click(a, place_mode_native::Control::EditorPrevious);
    }
    loop {
        if let Some((c,_,_)) = a.toolbox_controls().into_iter().find(|(_,_,s)| *s == label) {
            click(a, c); return;
        }
        assert!(a.toolbox_controls().iter().any(|(c,_,_)| *c == place_mode_native::Control::EditorNext),
            "missing actual scale control: {label}");
        click(a, place_mode_native::Control::EditorNext);
    }
}
#[test]
fn r2b_actual_file_scale_control_commits_and_cold_reopens_without_compounding() {
    let (f, mut a) = open_fixture("r2b-file-scale");
    let original = slice4c::records(&f.0);
    aim_file(&mut a, "A.txt"); tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
    let (before, _, reference) = a.editor_geometry(0.).unwrap();
    size_control(&mut a, "Size: 75%");
    let (after, _, new_reference) = a.editor_geometry(0.).unwrap();
    assert_eq!(reference, new_reference, "scaling must not auto-fit away the size change");
    for (old, new) in before.iter().zip(&after) {
        for axis in 0..3 { assert!((new.position[axis] - old.position[axis] * 0.75).abs() < 0.00001); }
    }
    assert_eq!(slice4c::records(&f.0), original);
    assert!(!f.0.join(".hair-layout").exists());
    enter_positioning(&mut a); aim_floor(&mut a, [2., 0.]);
    tap(&mut a, KeyCode::Enter); assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
    let saved = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
    assert_eq!(saved.positions[0].1.size, [0.8 * 0.75, 0.85 * 0.75, 0.28 * 0.75]);
    let mut reopened = app(); reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    aim_file(&mut reopened, "A.txt"); tap(&mut reopened, KeyCode::KeyP); tap(&mut reopened, KeyCode::KeyX);
    size_control(&mut reopened, "Size: 75%");
    assert_eq!(bytes(&reopened.editor_geometry(0.).unwrap().0), bytes(&after));
    for (p, state) in original { assert_eq!(slice4c::records(&f.0)[&p], state); }
}
#[test]
fn r2b_actual_scale_stops_reset_released_properties_frozen_review_and_focus_cancel() {
    let (f,mut a)=open_fixture("r2b-stops"); let originals=slice4c::records(&f.0);
    aim_file(&mut a,"A.txt"); tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyX);
    let reference=a.editor_geometry(0.).unwrap().2;
    for (label,percent) in [("Size: 50%",50),("Size: 75%",75),("Size: 125%",125),("Size: 150%",150),("Size: 100% (reset)",100)] {
        size_control(&mut a,label); assert_eq!(a.editor_scale_percent(),percent);
        assert_eq!(a.editor_geometry(0.).unwrap().2,reference);
        assert!(a.editor_geometry(0.).unwrap().0.iter().all(|v|v.position.iter().all(|n|n.is_finite())));
    }
    enter_positioning(&mut a); aim_floor(&mut a,[2.,0.]);
    if a.editor_compact(){click(&mut a,place_mode_native::Control::EditorDetails);}
    size_control(&mut a,"Size: 50%"); assert!(!a.input.captured);
    tap(&mut a,KeyCode::Enter); assert_eq!(a.editor_phase(),"Review","{}",a.local_status);
    let frozen=a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().pending.clone();
    assert!(a.editor_scale(150).is_err());
    assert_eq!(a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().pending,frozen);
    a.key(KeyCode::Enter,true,true).unwrap(); a.key(KeyCode::Enter,false,false).unwrap();
    assert_eq!(slice4c::records(&f.0),originals);
    tap(&mut a,KeyCode::Escape); assert_eq!(a.editor_scale_percent(),50);
    a.focus_changed(false); a.focus_changed(true);
    assert!(!a.editor_held()); assert_eq!(slice4c::records(&f.0),originals);
}
fn enter_positioning(a: &mut App) {
    assert_eq!(a.editor_phase(), "Editing");
    tap(a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Positioning");
    tap(a, KeyCode::Tab);
    assert!(!a.input.captured);
}

#[test]
fn r2a_file_preview_rotation_animation_back_pointer_and_metadata_commit() {
    let (f, mut a) = open_fixture("r2a-file-flow");
    aim_file(&mut a, "A.txt");
    let host = a.themes[0].host.clone();
    let original = slice4c::records(&f.0);
    tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
    let mesh = a.editor_geometry(0.).unwrap().0;
    assert!(!mesh.is_empty());
    click(&mut a, place_mode_native::Control::Right);
    assert_ne!(bytes(&mesh), bytes(&a.editor_geometry(0.).unwrap().0));
    tap(&mut a, KeyCode::KeyJ);
    assert_ne!(bytes(&a.editor_geometry(0.).unwrap().0), bytes(&a.editor_geometry(3.).unwrap().0));
    assert_eq!(a.themes[0].host, host);
    assert_eq!(slice4c::records(&f.0), original);
    enter_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]);
    click(&mut a, place_mode_native::Control::Right);
    let geometry = bytes(&a.editor_geometry(0.).unwrap().0);
    tap(&mut a, KeyCode::Escape);
    assert_eq!(a.editor_phase(), "Editing");
    assert_eq!(bytes(&a.editor_geometry(0.).unwrap().0), geometry);
    enter_positioning(&mut a);
    a.key(KeyCode::Enter, true, false).unwrap();
    assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
    for repeat in [true, false] { a.key(KeyCode::Enter, true, repeat).unwrap(); }
    assert_eq!(a.editor_phase(), "Review");
    assert_eq!(slice4c::records(&f.0), original);
    a.key(KeyCode::Enter, false, false).unwrap();
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
    assert!(a.inventory.items().is_empty());
    let saved = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
    assert!(saved.positions[0].1.properties.animated);
    assert_ne!(saved.positions[0].1.yaw, 0.);
    for (p, state) in original { assert_eq!(slice4c::records(&f.0)[&p], state); }
    let mut reopened = app(); reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(hair::placement_store::read_layout(&reopened.local.as_ref().unwrap().explorer).unwrap(), saved);
}

#[test]
fn r2a_catalog_enters_editor_and_actual_mesh_then_fresh_three_phase_commit() {
    let (f, mut a) = open_fixture("r2a-catalog");
    tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyB);
    a.key(KeyCode::Enter, true, false).unwrap();
    assert_eq!(a.editor_phase(), "Editing");
    assert!(a.local_status.contains("Editing"));
    a.key(KeyCode::Enter, true, true).unwrap();
    a.key(KeyCode::Enter, true, false).unwrap();
    assert_eq!(a.editor_phase(), "Editing");
    a.key(KeyCode::Enter, false, false).unwrap();
    let original = bytes(&a.editor_geometry(0.).unwrap().0);
    assert!(original.len() > 36 * std::mem::size_of::<hair::assets::Vertex>());
    tap(&mut a, KeyCode::ArrowRight);
    assert_ne!(bytes(&a.editor_geometry(0.).unwrap().0), original);
    assert!(!f.0.join(".hair-layout").exists());
    enter_positioning(&mut a); aim_floor(&mut a, [-2., 0.]);
    a.key(KeyCode::Enter, true, false).unwrap();
    assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
    for repeat in [true, false] { a.key(KeyCode::Enter, true, repeat).unwrap(); }
    assert_eq!(a.editor_phase(), "Review");
    assert!(!f.0.join(".hair-layout").exists());
    a.key(KeyCode::Enter, false, false).unwrap();
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
}
#[test]
fn r2b_catalog_then_actual_x_checked_resize_keeps_identity_and_original_reference() {
    let (f, mut a) = open_fixture("r2b-decoration");
    let originals = slice4c::records(&f.0);
    tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyB); tap(&mut a, KeyCode::Enter);
    let (before, _, reference) = a.editor_geometry(0.).unwrap();
    size_control(&mut a, "Size: 50%");
    let (small, _, same_reference) = a.editor_geometry(0.).unwrap();
    assert_eq!(reference, same_reference);
    assert!(before.iter().zip(&small).any(|(x,y)| x.position != y.position), "catalog scale must change actual artwork");
    assert_eq!(slice4c::records(&f.0), originals);
    enter_positioning(&mut a); aim_floor(&mut a, [-2.,0.]);
    tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
    let old = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone();
    let before_resize = slice4c::records(&f.0);
    a.pose = Pose { position: [old.at[0], old.at[1]+1.1], yaw: 0.,
        pitch: ((old.size[1]*0.5-a.pose.eye_height())/1.1).atan(), ..a.pose };
    tap(&mut a, KeyCode::KeyX); assert_eq!(a.editor_phase(), "Editing");
    size_control(&mut a, "Size: 150%");
    assert_eq!(slice4c::records(&f.0), before_resize);
    enter_positioning(&mut a); aim_floor(&mut a, [-2.,0.]);
    tap(&mut a, KeyCode::Enter); assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
    let resized = a.decoration_position().unwrap();
    assert_eq!(resized.placement_id, old.placement_id); assert_eq!(resized.tag, old.tag);
    assert_eq!(resized.size, reference.map(|n|n*1.5));
    assert!(old.validate_relocation(&resized).is_err(), "ordinary relocation must still forbid resize");
    assert_eq!(slice4c::records(&f.0), before_resize);
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
    let mut cold = app(); cold.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(cold.themes[0].host.decorations[&old.placement_id], resized);
    for (p,s) in originals { assert_eq!(slice4c::records(&f.0)[&p],s); }
}
#[test]
fn r2b_pin_and_missing_art_resize_preserve_exact_source_through_refresh_and_theme_return() {
    for pin in [false,true] {
        let (f,mut a)=open_fixture("r2b-pin-gray"); let originals=slice4c::records(&f.0);
        tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyB); tap(&mut a,KeyCode::Enter);
        if !pin { a.legacy_follow_fixture(); }
        size_control(&mut a,"Size: 75%"); if pin { tap(&mut a,KeyCode::KeyV); }
        enter_positioning(&mut a); aim_floor(&mut a,[-2.,0.]);
        tap(&mut a,KeyCode::Enter); tap(&mut a,KeyCode::Enter);
        let old=hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone();
        assert_eq!(old.is_pinned(),pin); assert_eq!(old.scale_percent(),75);
        let entries=a.themes[0].decorations.entries.clone();
        if !pin { a.themes[0].decorations.entries.clear(); }
        a.pose=Pose{position:[old.at[0],old.at[1]+1.1],yaw:0.,pitch:((old.size[1]*0.5-a.pose.eye_height())/1.1).atan(),..a.pose};
        tap(&mut a,KeyCode::KeyX); assert_eq!(a.editor_phase(),"Editing");
        if !pin {
            assert!(a.editor_geometry(0.).unwrap().0[..36].iter().all(|v|v.color==[0.5;3]));
            tap(&mut a,KeyCode::KeyV); assert!(!a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().record.is_pinned());
        }
        let before=slice4c::records(&f.0);
        size_control(&mut a,"Size: 125%");
        assert_eq!(slice4c::records(&f.0),before);
        enter_positioning(&mut a); aim_floor(&mut a,[-2.,0.]);
        tap(&mut a,KeyCode::Enter); assert_eq!(a.editor_phase(),"Review","{}",a.local_status);
        let new=a.decoration_position().unwrap();
        assert_eq!(new.closure_identity(),old.closure_identity()); assert_eq!(new.tag,old.tag);
        assert_eq!(new.reference_size(),old.reference_size());
        tap(&mut a,KeyCode::Enter); assert!(a.local_status.contains("reposition saved"),"{}",a.local_status);
        let after=slice4c::records(&f.0);
        let mut cold=app(); cold.complete_folder(Ok(Some(f.0.clone()))).unwrap();
        assert_eq!(cold.themes[0].host.decorations[&old.placement_id],new);
        tap(&mut cold,KeyCode::F5); assert_eq!(cold.themes[0].host.decorations[&old.placement_id],new);
        a.themes[0].decorations.entries=entries;
        assert!(a.themes[0].decoration_preview(&new).unwrap().len()>36);
        assert_eq!(slice4c::records(&f.0),after);
        for (p,s) in originals { assert_eq!(after[&p],s); }
    }
}

fn carried(a: &mut App) {
    aim_file(a, "A.txt"); tap(a, KeyCode::KeyX);
    assert_eq!(a.inventory.items().len(), 1);
}
#[test]
fn r2b_inventory_scale_both_routes_preserve_copy_move_authority_and_base() {
    for toolbox in [false, true] {
        let (source, mut a) = open_fixture("r2b-carried-source"); carried(&mut a);
        let original = slice4c::records(&source.0);
        let dest = fixture("r2b-carried-target"); a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
        if toolbox { tap(&mut a, KeyCode::KeyP); click(&mut a, place_mode_native::Control::FileInventory); }
        else { tap(&mut a, KeyCode::KeyB); }
        tap(&mut a, KeyCode::KeyP);
        let (before, _, reference) = a.editor_geometry(0.).unwrap();
        size_control(&mut a, "Size: 50%");
        let (after, _, scaled_reference) = a.editor_geometry(0.).unwrap();
        assert_eq!(reference, scaled_reference);
        assert_ne!(bytes(&before), bytes(&after), "actual carried-file control must scale geometry");
        tap(&mut a, KeyCode::KeyG);
        if toolbox { tap(&mut a, KeyCode::KeyV); }
        let held_id = a.placement_action.as_ref().unwrap().item;
        enter_positioning(&mut a); aim_floor(&mut a, [2.,0.]);
        tap(&mut a, KeyCode::Escape);
        assert_eq!(a.editor_scale_percent(), 50);
        enter_positioning(&mut a); aim_floor(&mut a, [2.,0.]);
        tap(&mut a, KeyCode::Enter); assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
        let frozen = a.placement_action.as_ref().unwrap().pending.clone().unwrap();
        assert_eq!(frozen.size, [0.4, 0.425, 0.14]);
        assert_eq!(frozen.base_bounds().unwrap().unwrap().size, [0.4, 0.4, 0.3]);
        assert!(!dest.0.join("A.txt").exists());
        assert_eq!(a.inventory.items()[0].id, held_id);
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("Placed"), "{}", a.local_status);
        assert_eq!(a.inventory.items().is_empty(), toolbox);
        assert_eq!(slice4c::records(&source.0), original);
        let mut cold = app(); cold.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
        let p = &hair::placement_store::read_layout(&cold.local.as_ref().unwrap().explorer).unwrap().positions[0].1;
        assert_eq!(p, &frozen);
        assert_eq!(std::fs::read(dest.0.join("A.txt")).unwrap(), b"A.txt");
    }
}
#[test]
fn r2a_both_inventory_routes_cancel_and_publish_copy_move_only_at_final_confirm() {
    for toolbox in [false, true] {
        let (source, mut a) = open_fixture("r2a-carried-source");
        carried(&mut a);
        let original = slice4c::records(&source.0);
        let dest = fixture("r2a-carried-destination");
        a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
        if toolbox {
            tap(&mut a, KeyCode::KeyP);
            click(&mut a, place_mode_native::Control::FileInventory);
        } else { tap(&mut a, KeyCode::KeyB); }
        assert!(a.inventory_open);
        a.key(KeyCode::KeyP, true, false).unwrap();
        assert_eq!(a.editor_phase(), "Editing");
        a.key(KeyCode::KeyP, true, true).unwrap();
        a.key(KeyCode::KeyP, true, false).unwrap();
        assert_eq!(a.editor_phase(), "Editing");
        a.key(KeyCode::KeyP, false, false).unwrap();
        assert!(!a.editor_geometry(0.).unwrap().0.is_empty());
        tap(&mut a, KeyCode::Escape);
        assert!(a.inventory_open && a.placement_action.is_none());
        assert_eq!(a.inventory.items().len(), 1);
        tap(&mut a, KeyCode::KeyP);
        if toolbox { tap(&mut a, KeyCode::KeyV); }
        let held_id = a.placement_action.as_ref().unwrap().item;
        enter_positioning(&mut a); aim_floor(&mut a, [2., 0.]);
        tap(&mut a, KeyCode::Enter);
        assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
        assert!(!dest.0.join("A.txt").exists());
        assert_eq!(slice4c::records(&source.0), original);
        assert_eq!(a.inventory.items()[0].id, held_id);
        a.key(KeyCode::Enter, true, true).unwrap();
        assert!(!dest.0.join("A.txt").exists());
        a.key(KeyCode::Enter, false, false).unwrap();
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("Placed"), "{}", a.local_status);
        assert_eq!(std::fs::read(dest.0.join("A.txt")).unwrap(), b"A.txt");
        assert_eq!(a.inventory.items().is_empty(), toolbox);
        assert_eq!(slice4c::records(&source.0), original);
        let mut reopened = app(); reopened.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
        assert_eq!(reopened.themes[0].host.file_placements.len(), 1);
    }
}

#[test]
fn r2a_focus_context_e_and_stale_inventory_destination_never_publish() {
    for key in [KeyCode::KeyE, KeyCode::F5, KeyCode::KeyI, KeyCode::KeyT, KeyCode::KeyM] {
        let (f, mut a) = open_fixture("r2a-context");
        aim_file(&mut a, "A.txt"); tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
        tap(&mut a, key);
        assert!(a.place_mode.is_none(), "{key:?}");
        assert!(!f.0.join(".hair-layout").exists());
    }
    let (f, mut a) = open_fixture("r2a-focus");
    aim_file(&mut a, "A.txt"); tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
    a.focus_changed(false); a.focus_changed(true);
    assert!(a.place_mode.is_none());
    assert!(!f.0.join(".hair-layout").exists());
    carried(&mut a);
    let dest = fixture("r2a-stale-target"); a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    tap(&mut a, KeyCode::KeyB); tap(&mut a, KeyCode::KeyP);
    enter_positioning(&mut a); aim_floor(&mut a, [2., 0.]); tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Review");
    // Remove the actual held destination, not merely rename its display path:
    // retained directory authority intentionally survives an ordinary rename.
    std::fs::remove_dir(&dest.0).unwrap();
    std::fs::create_dir(&dest.0).unwrap();
    tap(&mut a, KeyCode::Enter);
    assert!(!dest.0.join("A.txt").exists());
    assert_eq!(a.inventory.items().len(), 1);
}

fn png_fixture() -> Vec<u8> {
    let mut data = vec![];
    { let mut encoder = png::Encoder::new(&mut data, 48, 24);
      encoder.set_color(png::ColorType::Rgb); encoder.set_depth(png::BitDepth::Eight);
      let mut w = encoder.write_header().unwrap();
      let pixels: Vec<u8> = (0..24).flat_map(|y| (0..48).flat_map(move |x| if x < 24 { if y < 12 { [241,17,53] } else { [31,67,239] } } else if y < 12 { [23,209,71] } else { [233,181,29] })).collect();
      w.write_image_data(&pixels).unwrap(); }
    data
}
#[test]
fn r2a_png_production_async_arrival_replaces_only_selected_identity() {
    let (f, mut a) = open_fixture("r2a-async");
    std::fs::write(f.0.join("Image.png"), png_fixture()).unwrap();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    aim_file(&mut a, "Image.png"); tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
    let before = bytes(&a.editor_geometry(0.).unwrap().0);
    let selected_anchor = a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().anchor.clone();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !a.object_previews.cache.contains_key(&selected_anchor) {
        a.poll_local_image().unwrap(); assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let (mesh, textures, _) = a.editor_geometry(0.).unwrap();
    assert_ne!(bytes(&mesh), before);
    assert!(mesh.iter().any(|v| v.texture != 0));
    assert!(textures.iter().any(|t| t.width == 48 && t.height == 24));
    while a.object_previews.cache.len() < 3 {
        a.poll_local_image().unwrap(); assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    tap(&mut a, KeyCode::Escape);
    aim_file(&mut a, "Z.txt"); tap(&mut a, KeyCode::KeyX);
    let selected = bytes(&a.editor_geometry(0.).unwrap().0);
    for _ in 0..10 { a.poll_local_image().unwrap(); }
    assert_eq!(bytes(&a.editor_geometry(0.).unwrap().0), selected);
    assert!(!f.0.join(".hair-layout").exists());
}
