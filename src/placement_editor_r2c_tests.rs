//! Document animation controls and explicitly authored LEGACY decoration state.
use super::*;
use hair::placement::animation::Mode as Animation;
use crate::slice6c_texture_fixture as input;
#[path = "placement_editor_r2c_metal_tests.rs"]
mod metal;
#[test]
fn decoration_animation_guard_catalog_draft_refuses_every_mode_without_mutation() {
    let (f,mut a)=open_fixture("animation-guard-catalog");
    tap(&mut a,KeyCode::KeyP);tap(&mut a,KeyCode::KeyB);tap(&mut a,KeyCode::Enter);
    let before=slice4c::records(&f.0);
    let draft=a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().record.clone();
    assert!(draft.is_pinned());assert_eq!(draft.animation,None);
    for positioning in [false,true] {
        if positioning {enter_positioning(&mut a);aim_floor(&mut a,[-2.,0.]);}
        for mode in [Animation::None,Animation::Breathe,Animation::BreatheRotate] {
            assert!(a.editor_animation(mode).is_err(),"new decoration accepted {mode:?}");
            assert_eq!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().record,draft);
            assert_eq!(slice4c::records(&f.0),before);
        }
    }
}
#[test]
fn decoration_animation_guard_existing_draft_refuses_even_clearing_legacy_mode() {
    for saved in [None,Some(Animation::None),Some(Animation::Breathe),Some(Animation::BreatheRotate)] {
        let (f,mut a)=open_fixture("animation-guard-existing");
        tap(&mut a,KeyCode::KeyP);tap(&mut a,KeyCode::KeyB);tap(&mut a,KeyCode::Enter);
        enter_positioning(&mut a);aim_floor(&mut a,[-2.,0.]);tap(&mut a,KeyCode::Enter);tap(&mut a,KeyCode::Enter);
        let mut old=hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone();
        // Intentionally authored LEGACY saved record, not current UI/API creation.
        old.animation=saved;
        let record=f.0.join(format!(".hair-layout/dc-{}/decoration.toml",old.placement_id));
        std::fs::write(record,toml::to_string(&old).unwrap()).unwrap();tap(&mut a,KeyCode::F5);tap(&mut a,KeyCode::KeyP);
        let before=slice4c::records(&f.0);edit_decoration(&mut a,&old);
        for mode in [Animation::None,Animation::Breathe,Animation::BreatheRotate] {
            assert!(a.editor_animation(mode).is_err(),"existing decoration {saved:?} accepted {mode:?}");
            let edit=a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap();
            assert_eq!(edit.record,old);assert_eq!(edit.expected(),Some(&old));assert!(edit.pending.is_none());
            assert_eq!(slice4c::records(&f.0),before);
        }
    }
}
#[test]
fn r2c_fixture_exact_production_local_preview_decoder() {
    let f=fixture("r2c-local-decoder");let path=f.0.join("Art.glb");
    input::write_input(&path);
    let historical=std::fs::read(&path).unwrap();
    let file=gltf::Gltf::from_slice(&historical).unwrap();
    assert_eq!(file.materials().next().unwrap().pbr_metallic_roughness().metallic_factor(),1.0);
    let error=hair::assets::decode_local_preview_model(&historical).err().expect("historical metallic default must remain refused");
    assert_eq!(error,"local preview requires opaque white matte textured material");
    println!("R2C_LOCAL_DECODER historical_default_metallic=1 refused={error}");
    write_local_input(&path);
}
fn write_local_input(path: &Path) {
    input::write_input_with_matte(path,true);
    let source=std::fs::read(path).unwrap();
    let file=gltf::Gltf::from_slice(&source).unwrap();
    assert_eq!(file.materials().next().unwrap().pbr_metallic_roughness().metallic_factor(),0.0);
    let model=hair::assets::decode_local_preview_model(&source)
        .unwrap_or_else(|e|panic!("R2C exact production local preview decoder refused authored input: {e}"));
    assert_eq!(model.vertices.len(),108);
    assert_eq!(model.min,[-0.5,0.,-0.35]);assert_eq!(model.max,[0.5,1.,0.2]);
    assert_eq!(model.textures.len(),1);let t=&model.textures[0];
    assert_eq!((t.width,t.height),(32,32));assert_eq!(t.rgba,input::rgba());
    assert!(model.vertices.iter().all(|v|v.texture==1));
    println!("R2C_LOCAL_DECODER explicit_metallic=0 admitted vertices={} textures={} image={}x{}",model.vertices.len(),model.textures.len(),t.width,t.height);
}
fn select_mode(a: &mut App, mode: Animation) {
    mode_control(a,match mode { Animation::None=>"Animation: None",Animation::Breathe=>"Animation: Breathe",Animation::BreatheRotate=>"Animation: Breathe + rotate" });
    assert_eq!(a.editor_animation_mode(),mode);
}
fn assert_motion(a: &App, mode: Animation) {
    let x=a.editor_geometry(0.).unwrap(); let y=a.editor_geometry(3.).unwrap();
    assert_eq!(x.2,y.2);
    assert_eq!(bytes(&x.0),bytes(&a.editor_geometry(12.).unwrap().0));
    assert_eq!(bytes(&x.0),bytes(&a.editor_geometry(f64::NAN).unwrap().0));
    assert_eq!(bytes(&x.0),bytes(&a.editor_geometry(f64::INFINITY).unwrap().0));
    assert_eq!(bytes(&x.0)==bytes(&y.0),mode==Animation::None);
    if mode==Animation::BreatheRotate {
        assert!(x.0.iter().zip(&y.0).any(|(a,b)|(a.position[0]-b.position[0]).abs()>0.001||(a.position[2]-b.position[2]).abs()>0.001));
    }
}
fn theme_roundtrip(a: &mut App) {
    let host=a.themes[0].host.clone();
    a.themes.push(hair::theme::Theme::load_for_folder_scene(&a.root.join("packs/archive.toml"),&mut Default::default(),&host).unwrap());
    for index in [1,0] {
        a.menu=Some(hair::presentation::Menu::settings()); a.activate_menu(hair::presentation::Item::Theme);
        assert_eq!(a.theme_index,index,"{}",a.local_status); a.menu=None;
    }
}
#[test]
fn r2c_actual_mode_labels_all_widths_scales_and_released_positioning_pages() {
    for size in [[640,480],[960,640],[1280,800]] {
        let (_f,mut a)=open_fixture("r2c-control-layout"); layout_renderer(&mut a,size);
        aim_file(&mut a,"A.txt");tap(&mut a,KeyCode::KeyP);tap(&mut a,KeyCode::KeyX);
        for scale in hair::presentation::STEPS {
            a.ui_scale=scale;
            for positioning in [false,true] {
                if positioning {enter_positioning(&mut a);if a.editor_compact(){click(&mut a,place_mode_native::Control::EditorDetails);}}
                for mode in [Animation::None,Animation::Breathe,Animation::BreatheRotate] {
                    select_mode(&mut a,mode);let controls=a.toolbox_controls();let full=a.inventory_frame().full;
                    let ui=a.place_ui();
                    for (_,r,label) in controls {
                        assert!(r[3]-r[1]>=19.&&r[3]<=full[1]as f32-104.);
                        assert!(label.chars().count()as f32*6.<=r[2]-r[0]-12.);
                        let p=hair::placement::toolbox::paint(full,&[],&[([r[0],r[1]-4.,r[2],r[3]],label)],None);
                        assert!(ui.windows(p[12..].len()).any(|v|bytes(v)==bytes(&p[12..])),"full control not painted: {label}");
                    }
                    assert!(!a.input.captured);
                }
                if positioning {tap(&mut a,KeyCode::Escape);}
            }
        }
    }
}
fn cold_child(root: &Path, kind: &str, expected: &str) {
    let identity="directory_map_app_tests::place_mode_app_tests::placement_editor_r2a::animation::r2c_cold_child";
    let out=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact",identity,"--ignored","--nocapture","--test-threads=1"])
        .env("HAIR_R2C_ROOT",root).env("HAIR_R2C_KIND",kind).env("HAIR_R2C_EXPECT",expected).output().unwrap();
    let stdout=String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(),"{stdout}{}",String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains(&format!("test {identity} ..."))&&stdout.contains("running 1 test")
        &&stdout.contains("test result: ok. 1 passed; 0 failed; 0 ignored;"),"wrong/empty child execution: {stdout}");
    println!("R2C_CHILD kind={kind} {stdout}");
}
#[test]
#[ignore = "explicit child-only owned R2C cold-process fixture"]
fn r2c_cold_child() {
    let root=PathBuf::from(std::env::var_os("HAIR_R2C_ROOT").unwrap()); assert!(root.starts_with(std::env::temp_dir()));
    let before=slice4c::records(&root); let expected=std::env::var("HAIR_R2C_EXPECT").unwrap();
    let mut a=app(); a.complete_folder(Ok(Some(root.clone()))).unwrap();
    theme_roundtrip(&mut a); tap(&mut a,KeyCode::F5); tap(&mut a,KeyCode::KeyP);
    if std::env::var("HAIR_R2C_KIND").unwrap()=="file" {
        let p:hair::placement_store::Position=toml::from_str(&expected).unwrap();
        assert_eq!(hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().positions[0].1,p);
        aim_saved(&mut a,&p); tap(&mut a,KeyCode::KeyX);
        assert_eq!(a.editor_animation_mode(),p.properties.mode()); assert_motion(&a,p.properties.mode());
    } else {
        let d:hair::placement_store::Decoration=toml::from_str(&expected).unwrap();
        let live=hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone();
        assert_eq!(live,d); edit_decoration(&mut a,&live);
        assert_eq!(a.editor_animation_mode(),d.animation_mode()); assert_motion(&a,d.animation_mode());
        if d.is_pinned() { assert_eq!(a.editor_geometry(0.).unwrap().1[0].rgba,input::rgba()); }
    }
    assert_eq!(slice4c::records(&root),before);
}
#[test]
fn r2c_all_file_modes_base_review_frozen_focus_cancel_and_separate_child_menu_refresh() {
    for mode in [Animation::None,Animation::Breathe,Animation::BreatheRotate] {
        let (f,mut a)=open_fixture("r2c-file-matrix"); let originals=slice4c::records(&f.0);
        aim_file(&mut a,"A.txt"); tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyX);
        select_mode(&mut a,mode); assert_motion(&a,mode); tap(&mut a,KeyCode::KeyG);
        size_control(&mut a,"Size: 150%");
        enter_positioning(&mut a); aim_floor(&mut a,[2.,0.]);
        if a.editor_compact(){click(&mut a,place_mode_native::Control::EditorDetails);}
        select_mode(&mut a,mode); assert!(!a.input.captured);
        a.key(KeyCode::Enter,true,false).unwrap(); assert_eq!(a.editor_phase(),"Review","{}",a.local_status);
        let frozen=a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().pending.clone().unwrap();
        for repeat in [true,false] {a.key(KeyCode::Enter,true,repeat).unwrap(); a.key(KeyCode::KeyJ,true,repeat).unwrap();}
        assert!(a.editor_animation(Animation::None).is_err());
        assert_eq!(a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().pending,Some(frozen.clone()));
        assert_eq!(slice4c::records(&f.0),originals);
        a.key(KeyCode::Enter,false,false).unwrap(); a.key(KeyCode::KeyJ,false,false).unwrap(); tap(&mut a,KeyCode::Enter);
        assert!(a.local_status.contains("reposition saved"),"{}",a.local_status);
        let saved=slice4c::records(&f.0); let host=a.themes[0].host.clone();
        let id=a.world.folder_view.as_ref().unwrap().bindings.iter().find(|(_,b)|b.entry.name=="A.txt").unwrap().0.clone();
        aim_saved(&mut a,&frozen);
        let target=hair::interaction::resolve(&host,&a.pose,&a.world);
        assert!(target.is_some());
        let base=bytes(&a.themes[0].collection_shape(&id));
        for time in [0.,1.5,3.,6.,12.,f64::NAN] {
            let _=a.themes[0].vertices_for_at(&a.world,time); assert_eq!(a.themes[0].host,host);
            assert_eq!(hair::interaction::resolve(&host,&a.pose,&a.world),target);
            assert_eq!(bytes(&a.themes[0].collection_shape(&id)),base);
            let b=frozen.bounds().unwrap();let factor=frozen.properties.scale_factor();
            let at=[b.at[0],b.at[1]+b.size[1]+0.12*factor+frozen.properties.file_offset(time),b.at[2]+b.size[2]*0.5+0.02*factor];
            let mut label=hair::text::object_label("A.txt",[0.;3],25);
            for v in &mut label{v.position=std::array::from_fn(|i|at[i]+v.position[i]*factor);}
            let scene=a.themes[0].vertices_for_at(&a.world,time);
            let contains=|label:&[hair::assets::Vertex]|scene.windows(label.len()).any(|v|v.iter().zip(label).all(|(x,y)|x.color==y.color&&x.texture==y.texture&&x.uv==y.uv&&x.position.iter().zip(y.position).all(|(x,y)|(x-y).abs()<0.000001)));
            assert!(contains(&label));for v in &mut label{v.position[0]+=0.05;}assert!(!contains(&label),"detached caption negative");
        }
        let base_bounds=frozen.base_bounds().unwrap().unwrap();let top=base_bounds.at[1]+base_bounds.size[1];
        assert!(hair::player::blocked_at(&host,frozen.at,0.));assert!(!hair::player::blocked_at(&host,frozen.at,top));
        a.pose=Pose{position:frozen.at,feet_y:top+0.25,..Default::default()};a.input.captured=true;
        for _ in 0..240{a.advance_motion(1./120.);}
        assert!(a.player.grounded&&(a.pose.feet_y-top).abs()<0.001,"stationary base support for {mode:?}");
        cold_child(&f.0,"file",&toml::to_string(&frozen).unwrap());
        tap(&mut a,KeyCode::F5); aim_saved(&mut a,&frozen); tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyX);
        assert_eq!(a.editor_animation_mode(),mode); select_mode(&mut a,Animation::None);
        a.focus_changed(false); a.focus_changed(true); assert!(!a.editor_held());
        assert_eq!(slice4c::records(&f.0),saved);
        for (path,s) in originals {assert_eq!(saved[&path],s);}
    }
}
#[test]
fn r2c_carried_both_routes_all_modes_cancel_copy_move_and_reopen() {
    for toolbox in [false,true] { for mode in [Animation::None,Animation::Breathe,Animation::BreatheRotate] {
        let (source,mut a)=open_fixture("r2c-carried"); carried(&mut a); let original=slice4c::records(&source.0);
        let dest=fixture("r2c-carried-target"); a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
        if toolbox {tap(&mut a,KeyCode::KeyP);click(&mut a,place_mode_native::Control::FileInventory);}else{tap(&mut a,KeyCode::KeyB);}
        tap(&mut a,KeyCode::KeyP); select_mode(&mut a,mode); assert_motion(&a,mode);
        tap(&mut a,KeyCode::Escape); assert!(a.inventory_open&&a.placement_action.is_none());
        assert_eq!(a.inventory.items().len(),1); assert!(!dest.0.join("A.txt").exists());
        tap(&mut a,KeyCode::KeyP); select_mode(&mut a,mode); if toolbox{tap(&mut a,KeyCode::KeyV);}
        enter_positioning(&mut a); aim_floor(&mut a,[2.,0.]); tap(&mut a,KeyCode::Enter);
        let frozen=a.placement_action.as_ref().unwrap().pending.clone().unwrap();
        assert!(a.editor_animation(Animation::None).is_err()); tap(&mut a,KeyCode::KeyJ);
        assert_eq!(a.placement_action.as_ref().unwrap().pending,Some(frozen.clone()));
        assert!(!dest.0.join("A.txt").exists()); tap(&mut a,KeyCode::Enter);
        assert!(a.local_status.contains("Placed"),"{}",a.local_status); assert_eq!(a.inventory.items().is_empty(),toolbox);
        let mut cold=app();cold.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
        assert_eq!(hair::placement_store::read_layout(&cold.local.as_ref().unwrap().explorer).unwrap().positions[0].1,frozen);
        assert_eq!(slice4c::records(&source.0),original);
    }}
}
#[test]
fn r2c_legacy_interrupted_animation_inspector_truthfully_preserves_pending() {
    use std::{io::Write,os::unix::fs::OpenOptionsExt};
    let (f,mut a)=open_fixture("r2c-interrupted");
    let old=legacy_saved_decoration(&mut a,true,Animation::Breathe);
    let before=slice4c::records(&f.0);
    let mut proposed=old.clone();proposed.animation=Some(Animation::BreatheRotate);
    assert!(hair::placement_store::DecorationAnimation::new(&old,&proposed).is_err());
    // Intentionally authored LEGACY interrupted transaction, not current UI/API.
    let pending=f.0.join(format!(".hair-layout/dc-{}/pending.toml",old.placement_id));
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600)
        .custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC).open(pending).unwrap();
    file.write_all(toml::to_string(&proposed).unwrap().as_bytes()).unwrap();file.sync_all().unwrap();
    let live=hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();assert_eq!(live.decorations,vec![old.clone()]);assert_eq!(live.incomplete_records,1);
    for(path,s)in before{assert_eq!(slice4c::records(&f.0)[&path],s);}
    let retained=slice4c::records(&f.0);
    click(&mut a,place_mode_native::Control::InspectRecovery);let status=a.recovery_status();
    assert!(status.contains("Pending animation retained")&&status.contains("not resumable")&&status.contains("recovery required"),"{status}");
    let mut cold=app();cold.complete_folder(Ok(Some(f.0.clone()))).unwrap();tap(&mut cold,KeyCode::KeyP);
    click(&mut cold,place_mode_native::Control::InspectRecovery);assert!(cold.recovery_status().contains("Pending animation retained"));
    assert_eq!(slice4c::records(&f.0),retained);
}
fn textured_theme(a: &mut App, pack: &Path) {
    input::write_input(&pack.join("input.glb"));
    std::fs::write(pack.join("pack.toml"),"version=3\nname='R2C owned input'\nroles=[]\n[decorations]\n[[decorations.assets]]\nid='input'\nfile='input.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0.0,0.0,0.0]\n[[decorations.entries]]\nid='input'\ntag='hair.seating.chair'\nasset='input'\nname='Owned textured asymmetric art'\n").unwrap();
    let host=a.themes[0].host.clone(); a.themes[0]=hair::theme::Theme::load_for_folder_scene(&pack.join("pack.toml"),&mut Default::default(),&host).unwrap();
}
#[test]
fn r2c_legacy_follow_pin_all_modes_relocation_resize_gray_restore_and_cold_child() {
    for pin in [false,true] { for mode in [Animation::None,Animation::Breathe,Animation::BreatheRotate] {
        let (f,mut a)=open_fixture("r2c-deco-matrix"); let originals=slice4c::records(&f.0);
        let pack=fixture("r2c-art"); textured_theme(&mut a,&pack.0); let source=slice4c::records(&pack.0);
        let old=legacy_saved_decoration(&mut a,pin,mode);
        let saved_legacy=slice4c::records(&f.0);
        let path=f.0.join(format!(".hair-layout/dc-{}/decoration.toml",old.placement_id));
        let metadata=record_metadata(&path);
        edit_decoration(&mut a,&old);
        assert_eq!(a.editor_geometry(0.).unwrap().1[0].rgba,input::rgba()); assert_motion(&a,mode);
        for requested in [Animation::None,Animation::Breathe,Animation::BreatheRotate] {assert!(a.editor_animation(requested).is_err());}
        tap(&mut a,KeyCode::Escape);
        cold_child(&f.0,"decoration",&toml::to_string(&old).unwrap());
        assert_eq!(record_metadata(&path),metadata);assert_eq!(slice4c::records(&f.0),saved_legacy);
        edit_decoration(&mut a,&old);tap(&mut a,KeyCode::ArrowRight);
        enter_positioning(&mut a);aim_floor(&mut a,[-2.,0.]);tap(&mut a,KeyCode::Enter);tap(&mut a,KeyCode::Enter);
        assert!(a.local_status.contains("reposition saved"),"{}",a.local_status);
        let relocated=hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone();
        assert_eq!(relocated.animation,old.animation);old.validate_relocation(&relocated).unwrap();
        edit_decoration(&mut a,&relocated); assert_eq!(a.editor_animation_mode(),mode);
        size_control(&mut a,"Size: 75%");
        enter_positioning(&mut a);aim_floor(&mut a,[-2.,0.]);tap(&mut a,KeyCode::Enter);
        let proposed=a.decoration_position().unwrap(); assert!(a.editor_animation(Animation::None).is_err());
        tap(&mut a,KeyCode::KeyJ);assert_eq!(a.decoration_position().unwrap(),proposed);
        tap(&mut a,KeyCode::Enter);assert!(a.local_status.contains("reposition saved"),"{}",a.local_status);
        assert_eq!(proposed.animation,old.animation);relocated.validate_resize(&proposed).unwrap();
        assert_eq!(source,slice4c::records(&pack.0));
        if pin {std::fs::remove_file(pack.0.join("input.glb")).unwrap();std::fs::remove_file(pack.0.join("pack.toml")).unwrap();}
        let saved=slice4c::records(&f.0); let host=a.themes[0].host.clone();
        for t in [0.,3.,12.,f64::NAN] {let _=a.themes[0].vertices_for_at(&a.world,t);assert_eq!(a.themes[0].host,host);}
        cold_child(&f.0,"decoration",&toml::to_string(&proposed).unwrap());
        assert_eq!(slice4c::records(&f.0),saved); for(path,s)in originals{assert_eq!(saved[&path],s);}
    }}
}
fn mode_control(a: &mut App, label: &str) {
    assert!(a.place_mode.as_ref().is_none_or(|m|m.decoration_edit.is_none()),"actual animation controls are document-only");
    size_control(a,label);
}
fn record_metadata(path: &Path) -> [i128; 7] {
    let m=std::fs::metadata(path).unwrap();
    [m.ino() as i128,m.mode() as i128,m.len() as i128,m.mtime() as i128,m.mtime_nsec() as i128,m.ctime() as i128,m.ctime_nsec() as i128]
}
// LEGACY saved fixture: publish a static source/closure first, then intentionally
// author historical record bytes in the disposable room. Not actual animation UI.
fn legacy_saved_decoration(a: &mut App, pin: bool, mode: Animation) -> hair::placement_store::Decoration {
    if a.place_mode.is_none(){tap(a,KeyCode::KeyP);}
    tap(a,KeyCode::KeyB);tap(a,KeyCode::Enter);
    if !pin {a.legacy_follow_fixture();}
    enter_positioning(a);aim_floor(a,[-2.,0.]);tap(a,KeyCode::Enter);tap(a,KeyCode::Enter);
    assert!(a.local_status.contains(if pin{"decoration pinned"}else{"decoration saved"}),"{}",a.local_status);
    let local=a.local.as_ref().unwrap();
    let mut d=hair::placement_store::read_layout(&local.explorer).unwrap().decorations[0].clone();
    d.animation=Some(mode);d.validate().unwrap();
    let root=local.explorer.current().folder.clone();
    std::fs::write(root.join(format!(".hair-layout/dc-{}/decoration.toml",d.placement_id)),toml::to_string(&d).unwrap()).unwrap();
    tap(a,KeyCode::F5);tap(a,KeyCode::KeyP);
    assert_eq!(hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0],d);
    d
}
fn aim_saved(a: &mut App, p: &hair::placement_store::Position) {
    let b=p.file_bounds().unwrap();
    a.pose=Pose{position:[p.at[0],p.at[1]+1.1],yaw:0.,pitch:((b.at[1]+b.size[1]*0.5-1.65)/1.1).atan(),..Default::default()};
}
fn edit_decoration(a: &mut App, d: &hair::placement_store::Decoration) {
    a.pose=Pose{position:[d.at[0],d.at[1]+1.1],yaw:0.,pitch:((d.size[1]*0.5-a.pose.eye_height())/1.1).atan(),..a.pose};
    tap(a,KeyCode::KeyX); assert_eq!(a.editor_phase(),"Editing","{}",a.local_status);
}
#[test]
fn r2c_actual_catalog_and_existing_decoration_refuse_modes_and_save_static() {
    let (f,mut a)=open_fixture("r2c-decoration"); let originals=slice4c::records(&f.0);
    tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyB); tap(&mut a,KeyCode::Enter);
    for mode in [Animation::None,Animation::Breathe,Animation::BreatheRotate] {assert!(a.editor_animation(mode).is_err());}
    assert_motion(&a,Animation::None);
    assert_eq!(slice4c::records(&f.0),originals);
    enter_positioning(&mut a); aim_floor(&mut a,[-2.,0.]); tap(&mut a,KeyCode::Enter); tap(&mut a,KeyCode::Enter);
    assert!(a.local_status.contains("decoration pinned"),"{}",a.local_status);
    let old=hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone();
    assert_eq!(old.animation,None);assert!(!toml::to_string(&old).unwrap().contains("animation"));
    edit_decoration(&mut a,&old);
    let saved=slice4c::records(&f.0);
    for mode in [Animation::None,Animation::Breathe,Animation::BreatheRotate] {assert!(a.editor_animation(mode).is_err());}
    assert_eq!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().record,old);assert_eq!(slice4c::records(&f.0),saved);
    tap(&mut a,KeyCode::ArrowRight);
    enter_positioning(&mut a); aim_floor(&mut a,[-2.,0.]); tap(&mut a,KeyCode::Enter); tap(&mut a,KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"),"{}",a.local_status);
    let new=hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone();
    old.validate_relocation(&new).unwrap();assert_eq!(new.animation,None);
    assert_eq!(old.placement_id,new.placement_id); assert_eq!(old.size,new.size);
    let mut cold=app(); cold.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(cold.themes[0].host.decorations[&old.placement_id],new);
    for (path,state) in originals {assert_eq!(slice4c::records(&f.0)[&path],state);}
}
#[test]
fn r2c_actual_file_mode_controls_stationary_breathe_rotate_commit_reopen() {
    let (f,mut a)=open_fixture("r2c-file-mode");
    let originals=slice4c::records(&f.0);
    aim_file(&mut a,"A.txt"); tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyX);
    mode_control(&mut a,"Animation: None");
    assert_eq!(bytes(&a.editor_geometry(0.).unwrap().0),bytes(&a.editor_geometry(3.).unwrap().0));
    mode_control(&mut a,"Animation: Breathe");
    let start=a.editor_geometry(0.).unwrap(); let peak=a.editor_geometry(3.).unwrap();
    assert_eq!(start.2,peak.2); assert_ne!(bytes(&start.0),bytes(&peak.0));
    assert_eq!(bytes(&start.0),bytes(&a.editor_geometry(6.).unwrap().0));
    mode_control(&mut a,"Animation: Breathe + rotate");
    let first=a.editor_geometry(0.).unwrap().0; let turned=a.editor_geometry(3.).unwrap().0;
    assert!(first.iter().zip(&turned).any(|(x,y)|x.position[0]!=y.position[0]||x.position[2]!=y.position[2]));
    assert_eq!(slice4c::records(&f.0),originals);
    enter_positioning(&mut a); aim_floor(&mut a,[2.,0.]);
    tap(&mut a,KeyCode::Enter); assert_eq!(a.editor_phase(),"Review","{}",a.local_status);
    tap(&mut a,KeyCode::Enter); assert!(a.local_status.contains("reposition saved"),"{}",a.local_status);
    let p=hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().positions[0].1.clone();
    assert!(toml::to_string(&p).unwrap().contains("breathe-rotate"));
    let mut cold=app(); cold.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(hair::placement_store::read_layout(&cold.local.as_ref().unwrap().explorer).unwrap().positions[0].1,p);
    aim_file(&mut cold,"A.txt"); tap(&mut cold,KeyCode::KeyP); tap(&mut cold,KeyCode::KeyX);
    assert_eq!(bytes(&cold.editor_geometry(3.).unwrap().0),bytes(&turned));
    for (path,state) in originals {assert_eq!(slice4c::records(&f.0)[&path],state);}
}
