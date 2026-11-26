//! CPU application/menu/child-process evidence, not new GPU acceptance.
use super::*;
use std::path::Path;
use crate::slice6c_texture_fixture as input;

fn switch(a: &mut App, expected: usize) {
    a.menu = Some(hair::presentation::Menu::settings());
    a.activate_menu(hair::presentation::Item::Theme);
    assert_eq!(a.theme_index, expected, "{}", a.local_status);
    a.menu = None;
}
fn missing_theme(a: &mut App) {
    let host = a.themes[0].host.clone();
    a.themes.push(hair::theme::Theme::load_for_folder_scene(&a.root.join("packs/archive.toml"), &mut Default::default(), &host).unwrap());
    assert!(a.themes[1].decorations.entries.iter().all(|e| e.tag != "hair.seating.chair"));
}
fn assert_assembly(a: &mut App, p: &store::Position) {
    let t = &a.themes[a.theme_index];
    let id = a.world.folder_view.as_ref().unwrap().bindings.iter().find(|(_,b)|b.entry.name=="A.txt").unwrap().0;
    let item = &t.host.file_placements[id];
    assert_eq!(&item.position, p); assert_eq!(item.bounds, p.bounds().unwrap());
    let base = p.base_bounds().unwrap().unwrap();
    let factor=p.properties.scale_factor(); let b=p.bounds().unwrap();
    let at=[b.at[0],b.at[1]+b.size[1]+0.12*factor,b.at[2]+b.size[2]*0.5+0.02*factor];
    let mut label=hair::text::object_label("A.txt",[0.;3],25);
    for v in &mut label {v.position=std::array::from_fn(|i|at[i]+v.position[i]*factor);}
    let scene=t.vertices_for_at(&a.world,0.);
    // The renderer's optimized transform may fuse multiply-add; compare every
    // declared glyph vertex within rounding tolerance, not candidate bounds.
    let contains_label = |expected: &[hair::assets::Vertex]| scene.windows(expected.len()).any(|actual|
        actual.iter().zip(expected).all(|(x,y)| x.color==y.color && x.uv==y.uv && x.texture==y.texture
            && x.position.iter().zip(y.position).all(|(a,b)|(a-b).abs()<=0.000001)));
    assert!(contains_label(&label),"scaled filename label must be in actual world geometry");
    let mut detached=label.clone();
    for v in &mut detached {v.position[0]+=0.05;}
    assert!(!contains_label(&detached),"label association must reject a detached caption");
    assert_eq!(base.size, [1.2, 1.2, 0.90000004]);
    let top = base.at[1] + base.size[1];
    assert!(hair::player::blocked_at(&t.host, p.at, 0.));
    assert!(!hair::player::blocked_at(&t.host, p.at, top), "scaled file body must remain non-solid at base top");
    // Actual App physics falls onto the installed base; include real slab contact.
    a.pose = Pose { position: p.at, feet_y: top + 0.25, ..Default::default() };
    a.input.captured = true;
    for _ in 0..240 { a.advance_motion(1./120.); }
    assert!(a.player.grounded && (a.pose.feet_y-top).abs()<0.001, "feet={} grounded={}",a.pose.feet_y,a.player.grounded);
}
fn assert_decoration(a: &App, d: &store::Decoration, gray: bool) {
    let t = &a.themes[a.theme_index];
    assert_eq!(&t.host.decorations[&d.placement_id], d);
    let mesh = t.decoration_preview(d).unwrap();
    if gray { assert!(mesh[..36].iter().all(|v|v.color==[0.5;3] && v.texture==0)); }
    else { assert!(mesh.len()>36); }
    assert_eq!(d.size, d.reference_size().map(|n|n*(f32::from(d.scale_percent())/100.)));
}
fn child(root: &Path, kind: &str, expected: &str) {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","directory_map_app_tests::place_mode_app_tests::placement_editor_r2a::qualification::persistence::r2b_qual_cold_child","--ignored","--nocapture","--test-threads=1"])
        .env("HAIR_R2BQ_ROOT",root).env("HAIR_R2BQ_KIND",kind).env("HAIR_R2BQ_EXPECT",expected).output().unwrap();
    let stdout=String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(),"{}{}",stdout,String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("running 1 test") && stdout.contains("test result: ok. 1 passed; 0 failed; 0 ignored;")
        && stdout.contains("::r2b_qual_cold_child ... ok"),"child filter must run exactly the expected test: {stdout}");
    println!("R2BQ child kind={kind} {stdout}");
}
#[test]
fn r2b_qual_scaled_file_base_real_menu_cold_process_and_player_support() {
    let (f,mut a)=open_fixture("r2b-qual-file"); let originals=slice4c::records(&f.0);
    aim_file(&mut a,"A.txt"); tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyX);
    size_control(&mut a,"Size: 150%"); tap(&mut a,KeyCode::KeyG);
    enter_positioning(&mut a); aim_floor(&mut a,[2.,0.]);
    tap(&mut a,KeyCode::Enter); tap(&mut a,KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"),"{}",a.local_status);
    let p=store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().positions[0].1.clone();
    assert_eq!(p.properties.scaling.unwrap().reference,[0.8,0.85,0.28]);
    let saved=slice4c::records(&f.0);
    tap(&mut a,KeyCode::KeyP); missing_theme(&mut a);
    assert_assembly(&mut a,&p); switch(&mut a,1); assert_assembly(&mut a,&p);
    switch(&mut a,0); assert_assembly(&mut a,&p);
    child(&f.0,"file",&toml::to_string(&p).unwrap());
    assert_eq!(slice4c::records(&f.0),saved);
    for (path,s) in originals {assert_eq!(saved[&path],s);}
}
#[test]
fn r2b_qual_new_existing_follow_real_gray_art_menu_return_and_cold_process() {
    let (f,mut a)=open_fixture("r2b-qual-follow"); let originals=slice4c::records(&f.0);
    tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyB); tap(&mut a,KeyCode::Enter);
    a.legacy_follow_fixture();
    size_control(&mut a,"Size: 50%");
    enter_positioning(&mut a); aim_floor(&mut a,[-2.,0.]); tap(&mut a,KeyCode::Enter); tap(&mut a,KeyCode::Enter);
    let old=store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone();
    assert_eq!(old.scale_percent(),50);
    edit_decoration(&mut a,&old); size_control(&mut a,"Size: 150%");
    enter_positioning(&mut a); aim_floor(&mut a,[-2.,0.]); tap(&mut a,KeyCode::Enter); tap(&mut a,KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"),"{}",a.local_status);
    let d=store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone();
    assert_eq!(d.reference_size(),old.reference_size()); assert_eq!(d.scale_percent(),150);
    let saved=slice4c::records(&f.0); tap(&mut a,KeyCode::KeyP); missing_theme(&mut a);
    let art=bytes(&a.themes[0].decoration_preview(&d).unwrap());
    switch(&mut a,1); assert_decoration(&a,&d,true); switch(&mut a,0);
    assert_decoration(&a,&d,false); assert_eq!(bytes(&a.themes[0].decoration_preview(&d).unwrap()),art);
    child(&f.0,"follow",&toml::to_string(&d).unwrap());
    assert_eq!(slice4c::records(&f.0),saved);
    for (path,s) in originals {assert_eq!(saved[&path],s);}
}
#[test]
fn r2b_qual_textured_scaled_pin_removed_disposable_source_real_menu_and_cold_process() {
    let (f,mut a)=open_fixture("r2b-qual-pin"); let originals=slice4c::records(&f.0);
    let pack=fixture("r2b-qual-pin-pack"); input::write_input(&pack.0.join("input.glb"));
    std::fs::write(pack.0.join("pack.toml"),"version=3\nname='R2BQ owned input'\nroles=[]\n[decorations]\n[[decorations.assets]]\nid='input'\nfile='input.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0.0,0.0,0.0]\n[[decorations.entries]]\nid='input'\ntag='hair.seating.chair'\nasset='input'\nname='Owned textured shape'\n").unwrap();
    let source=slice4c::records(&pack.0);
    let host=a.themes[0].host.clone(); a.themes[0]=hair::theme::Theme::load_for_folder_scene(&pack.0.join("pack.toml"),&mut Default::default(),&host).unwrap();
    tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyB); tap(&mut a,KeyCode::Enter);
    size_control(&mut a,"Size: 75%"); tap(&mut a,KeyCode::KeyV);
    enter_positioning(&mut a); aim_floor(&mut a,[-2.,0.]); tap(&mut a,KeyCode::Enter); tap(&mut a,KeyCode::Enter);
    assert!(a.local_status.contains("decoration pinned"),"{}",a.local_status);
    assert_eq!(slice4c::records(&pack.0),source);
    let old=store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone();
    edit_decoration(&mut a,&old); size_control(&mut a,"Size: 125%");
    enter_positioning(&mut a); aim_floor(&mut a,[-2.,0.]); tap(&mut a,KeyCode::Enter); tap(&mut a,KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"),"{}",a.local_status);
    let d=store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone();
    assert_eq!(d.closure_identity(),old.closure_identity()); assert_eq!(d.reference_size(),old.reference_size());
    assert_eq!(slice4c::records(&pack.0),source);
    // Deliberately remove ONLY disposable source files after byte/inode proof.
    std::fs::remove_file(pack.0.join("input.glb")).unwrap(); std::fs::remove_file(pack.0.join("pack.toml")).unwrap();
    let saved=slice4c::records(&f.0); tap(&mut a,KeyCode::KeyP); missing_theme(&mut a);
    let (_,asset)=d.resolve(&a.themes[0].decorations).unwrap();
    assert_eq!(asset.model.textures[0].rgba,input::rgba()); let art=bytes(&a.themes[0].decoration_preview(&d).unwrap());
    switch(&mut a,1); assert_decoration(&a,&d,false); assert_eq!(bytes(&a.themes[1].decoration_preview(&d).unwrap()),art);
    switch(&mut a,0); child(&f.0,"pin",&toml::to_string(&d).unwrap());
    assert_eq!(slice4c::records(&f.0),saved);
    for (path,s) in originals {assert_eq!(saved[&path],s);}
}
#[test]
#[ignore = "child-only owned R2BQ cold process, called explicitly by parent tests"]
fn r2b_qual_cold_child() {
    let root=PathBuf::from(std::env::var_os("HAIR_R2BQ_ROOT").unwrap()); assert!(root.starts_with(std::env::temp_dir()));
    let before=slice4c::records(&root); let kind=std::env::var("HAIR_R2BQ_KIND").unwrap();
    let expected=std::env::var("HAIR_R2BQ_EXPECT").unwrap();
    let mut a=app(); a.complete_folder(Ok(Some(root.clone()))).unwrap();
    if kind=="file" {
        let p:store::Position=toml::from_str(&expected).unwrap(); assert_assembly(&mut a,&p);
        let b=p.file_bounds().unwrap();
        a.pose=Pose{position:[p.at[0],p.at[1]+1.1],pitch:((b.at[1]+b.size[1]*0.5-1.65)/1.1).atan(),..Default::default()};
        tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyX);
        assert_eq!(a.editor_phase(),"Editing","{}",a.local_status);
        let reference=a.editor_geometry(0.).unwrap().2; let mesh=bytes(&a.editor_geometry(0.).unwrap().0);
        size_control(&mut a,"Size: 150%"); assert_eq!(a.editor_geometry(0.).unwrap().2,reference);
        assert_eq!(bytes(&a.editor_geometry(0.).unwrap().0),mesh); assert_eq!(a.editor_scale_percent(),150);
    } else {
        let d:store::Decoration=toml::from_str(&expected).unwrap();
        let live=store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap().decorations[0].clone(); assert_eq!(live,d);
        assert_decoration(&a,&live,false); missing_theme(&mut a); switch(&mut a,1); assert_decoration(&a,&live,kind=="follow");
        if kind=="pin" {let (_,asset)=live.resolve(&a.themes[1].decorations).unwrap(); assert_eq!(asset.model.textures[0].rgba,input::rgba());}
        switch(&mut a,0); tap(&mut a,KeyCode::KeyP); edit_decoration(&mut a,&live);
        let mesh=bytes(&a.editor_geometry(0.).unwrap().0); let reference=a.editor_geometry(0.).unwrap().2;
        size_control(&mut a,if kind=="pin" {"Size: 125%"}else{"Size: 150%"});
        assert_eq!(bytes(&a.editor_geometry(0.).unwrap().0),mesh); assert_eq!(a.editor_geometry(0.).unwrap().2,reference);
    }
    assert_eq!(slice4c::records(&root),before);
}
