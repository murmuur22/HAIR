use super::*;
// Synthetic static PNG INPUT. Output uses the actual decoder/adapter/visitor.
pub(super) fn png_input(path:&Path,width:u32,height:u32)->Vec<u8> {
    let pixels:Vec<u8>=(0..width*height).flat_map(|i|[30,(i*23) as u8,180,if i%3==0 {80}else{255}]).collect();
    let mut encoder=png::Encoder::new(fs::OpenOptions::new().write(true).create_new(true).open(path).unwrap(),width,height);
    encoder.set_color(png::ColorType::Rgba);encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();fs::read(path).unwrap()
}
#[derive(Debug,PartialEq)]
struct State { revision:u64, dirty:bool, saved:Option<PathBuf>, exported:Option<PathBuf>, paths:[String;3], sky:String }
fn state(a:&Workshop)->State {
    State {revision:a.revision,dirty:a.dirty,saved:a.saved_project.clone(),exported:a.exported_pack.clone(),paths:[a.input_path.clone(),a.project_path.clone(),a.export_path.clone()],sky:a.project.as_ref().and_then(|p|p.sky()).map(|s|toml::to_string(s).unwrap()).unwrap_or_default()}
}
#[test]
fn actual_chooser_panorama_retained_delete_after_save_cold_reopen_export_clear() {
    let base=fixture("panorama");let selected=base.join("owned-selected.png");let bytes=png_input(&selected,7,3);
    let expected=SkyAppearance::Panorama(std::sync::Arc::new(hair::sky::adapt_png(&bytes,Default::default()).unwrap()));
    let c=egui::Context::default();let mut a=Workshop::new(vec![]);let size=[900.,700.];
    click(&c,&mut a,size,"new-material");click(&c,&mut a,size,"sky");let revision=a.revision;
    choose(&c,&mut a,size,"sky-png",DialogKind::SkyPanorama,Some(selected.clone()));
    assert_eq!(a.revision,revision+1);assert_eq!(a.preview.as_ref().unwrap().sky,expected);
    assert!(frame(&c,&mut a,size,vec![]).contains("retained source copy sky.png"));
    choose(&c,&mut a,size,"save-new",DialogKind::SaveNew,Some(base.join("saved")));
    assert_eq!(fs::read(base.join("saved/sky.png")).unwrap(),bytes);fs::remove_file(&selected).unwrap();
    let old=state(&a);let held=match &a.preview.as_ref().unwrap().sky {SkyAppearance::Panorama(p)=>p.clone(),_=>panic!("panorama")};
    for _ in 0..20 {frame(&c,&mut a,size,vec![]);}assert_eq!(state(&a),old);
    if let SkyAppearance::Panorama(p)=&a.preview.as_ref().unwrap().sky {assert!(std::sync::Arc::ptr_eq(&held,p));}else{panic!("retained preview")}
    choose(&c,&mut a,size,"export",DialogKind::Export,Some(base.join("memory-export")));
    let saved=a.saved_project.clone().unwrap();let preview=a.preview.take().unwrap();drop(a);
    let mut a=Workshop::new(vec![]);choose(&c,&mut a,size,"open",DialogKind::Open,Some(saved));
    same(&preview,a.preview.as_ref().unwrap());
    choose(&c,&mut a,size,"export",DialogKind::Export,Some(base.join("visitor")));
    same(&preview,&visitor(a.exported_pack.as_ref().unwrap()));assert_eq!(fs::read(base.join("visitor/sky.png")).unwrap(),bytes);
    click(&c,&mut a,size,"sky");let old=state(&a);click(&c,&mut a,size,"sky-clear");
    assert_eq!(a.revision,old.revision+1);assert!(a.dirty);assert!(a.project.as_ref().unwrap().sky().is_none());
    assert_eq!(a.preview.as_ref().unwrap().sky,SkyAppearance::Legacy);assert_eq!(a.saved_project,old.saved);assert_eq!(a.exported_pack,old.exported);
    choose(&c,&mut a,size,"save-new",DialogKind::SaveNew,Some(base.join("cleared")));
    choose(&c,&mut a,size,"open",DialogKind::Open,Some(base.join("cleared/project.toml")));
    choose(&c,&mut a,size,"export",DialogKind::Export,Some(base.join("cleared-visitor")));
    assert_eq!(visitor(a.exported_pack.as_ref().unwrap()).sky,SkyAppearance::Legacy);
    assert!(!base.join("cleared/sky.png").exists());assert!(!base.join("cleared-visitor/sky.png").exists());
}
#[test]
fn actual_input_invalid_colour_cancel_stale_unsafe_png_keep_clean_saved_export_state() {
    use std::os::unix::fs::symlink;
    let base=fixture("refusals").canonicalize().unwrap();let selected=base.join("owned.png");png_input(&selected,2,1);
    let c=egui::Context::default();let mut a=Workshop::new(vec![]);let size=[900.,700.];
    click(&c,&mut a,size,"new-material");click(&c,&mut a,size,"sky");click(&c,&mut a,size,"sky-apply");
    assert_eq!(a.preview.as_ref().unwrap().sky,SkyAppearance::Colour([0.04,0.06,0.05]),"explicit historical RGB is not Legacy");
    choose(&c,&mut a,size,"save-new",DialogKind::SaveNew,Some(base.join("saved")));
    choose(&c,&mut a,size,"export",DialogKind::Export,Some(base.join("visitor")));let old=state(&a);
    let manifest=fs::read(base.join("saved/project.toml")).unwrap();
    for value in ["NaN","inf","-inf","-0.01","1.01","nonsense",""] {
        type_value(&c,&mut a,size,"sky-green",value);assert_eq!(state(&a),old);
        click(&c,&mut a,size,"sky-apply");assert!(a.error.as_deref().unwrap().contains("finite linear-light"));assert_eq!(state(&a),old);
        click(&c,&mut a,size,"ack-error");
    }
    fs::write(base.join("bad.png"),b"SYNTHETIC invalid PNG").unwrap();symlink(&base,base.join("alias")).unwrap();symlink(&selected,base.join("linked.png")).unwrap();
    fs::File::create(base.join("large.png")).unwrap().set_len(4_000_001).unwrap();
    for result in [Ok(None),Err("injected panel error".into()),Ok(Some(base.join("bad.png"))),Ok(Some(base.join("missing.png"))),Ok(Some(base.join("alias/owned.png"))),Ok(Some(base.join("linked.png"))),Ok(Some(base.join("large.png")))] {
        click(&c,&mut a,size,"sky-png");assert_eq!(a.begin_dialog(),Some(DialogKind::SkyPanorama));assert_eq!(a.begin_dialog(),None);
        click(&c,&mut a,size,"sky-clear");click(&c,&mut a,size,"wall");click(&c,&mut a,size,"sky-apply");
        assert_eq!(state(&a),old);a.finish_dialog(result);assert_eq!(state(&a),old);
        assert_eq!(a.preview.as_ref().unwrap().sky,SkyAppearance::Colour([0.04,0.06,0.05]));
        if a.error.is_some() {click(&c,&mut a,size,"ack-error");}
    }
    click(&c,&mut a,size,"sky-png");a.begin_dialog();a.revision+=1;let stale=state(&a);
    a.finish_dialog(Ok(Some(selected.clone())));assert!(a.error.as_deref().unwrap().contains("context changed"));assert_eq!(state(&a),stale);
    click(&c,&mut a,size,"ack-error");
    choose(&c,&mut a,size,"open",DialogKind::Open,Some(base.join("saved/project.toml")));let reopened=state(&a);
    a.finish_dialog(Ok(Some(selected)));assert_eq!(state(&a),reopened);
    assert_eq!(fs::read(base.join("saved/project.toml")).unwrap(),manifest);
}
#[test]
fn actual_input_sky_reflow_selection_typing_input_ownership_and_transactions() {
    for size in [[1280.,960.],[900.,700.],[640.,480.]] {
        let c=egui::Context::default();let mut a=Workshop::new(vec![]);
        click(&c,&mut a,size,"sky");assert!(a.project.is_none()&&!a.dirty&&a.revision==0);
        click(&c,&mut a,size,"new-material");let revision=a.revision;
        click(&c,&mut a,size,"camera-mode");assert!(a.camera.free_fly);let pose=a.camera.pose;
        type_value(&c,&mut a,size,"sky-red","wasdqe");
        assert_eq!(a.revision,revision);assert!(!a.camera.captured);assert_eq!(a.camera.pose.position,pose.position);
        click(&c,&mut a,size,"sky-apply");assert!(a.error.is_some());click(&c,&mut a,size,"ack-error");
        type_value(&c,&mut a,size,"sky-red","0.2");click(&c,&mut a,size,"sky-apply");assert_eq!(a.revision,revision+1);
        let old=state(&a);click(&c,&mut a,size,"sky-png");a.begin_dialog();a.finish_dialog(Ok(None));
        let p=a.controls["sky-apply"].center();
        for pressed in [true,true,false] {frame(&c,&mut a,size,vec![egui::Event::PointerMoved(p),egui::Event::PointerButton{pos:p,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        assert_eq!(state(&a),old);frame(&c,&mut a,size,vec![]);
        click(&c,&mut a,size,"new-material");assert!(a.confirmation_pending());
        // The modal disables the underlying scroll area. Deliver attempted
        // pointer gestures to its old controls without pretending they can scroll.
        for id in ["sky-clear","sky-apply"] {
            let p=a.controls[id].center();
            for pressed in [true,false] {frame(&c,&mut a,size,vec![egui::Event::PointerMoved(p),egui::Event::PointerButton{pos:p,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        }
        assert_eq!(state(&a),old);
        click(&c,&mut a,size,"cancel-discard");assert!(!a.confirmation_pending());
        c.run_ui(egui::RawInput{focused:false,screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(size[0],size[1]))),..Default::default()},|ui|a.ui(ui,None)).drop_without_applying_deltas();
        assert!(!a.camera.captured);assert_eq!(state(&a),old);
        click(&c,&mut a,size,"sky-clear");assert_eq!(a.revision,revision+2);assert_eq!(a.preview.as_ref().unwrap().sky,SkyAppearance::Legacy);
        for category in ["wall","floor","catalog","document","sky"] {click(&c,&mut a,size,category);assert_eq!(a.revision,revision+2);}
        assert!(a.project.as_ref().unwrap().sky().is_none());
    }
}
#[test]
fn actual_input_whole_pack_coexistence_sky_and_all_existing_editors() {
    let base=fixture("coexistence");let root=Path::new(env!("CARGO_MANIFEST_DIR"));let c=egui::Context::default();let mut a=Workshop::new(vec![]);let size=[900.,700.];
    click(&c,&mut a,size,"new-material");click(&c,&mut a,size,"sky");type_value(&c,&mut a,size,"sky-green","0.8");click(&c,&mut a,size,"sky-apply");
    let sky=a.preview.as_ref().unwrap().sky.clone();
    for (category,source) in [("document","assets/kenney/desk.glb"),("decoration","assets/kenney/desk.glb"),("door-trim","examples/door-trim-starter/model.glb")] {
        click(&c,&mut a,size,category);click(&c,&mut a,size,"import");assert_eq!(a.begin_dialog(),Some(DialogKind::Import));a.finish_dialog(Ok(Some(root.join(source))));
        assert!(a.confirmation_pending());click(&c,&mut a,size,"confirm-discard");assert!(a.error.is_none(),"{:?}",a.error);assert_eq!(a.preview.as_ref().unwrap().sky,sky);
    }
    for (category,button,kind) in [("wall","wall-png",DialogKind::WallTexture),("floor","floor-png",DialogKind::FloorTexture)] {
        click(&c,&mut a,size,category);choose(&c,&mut a,size,button,kind,Some(root.join("examples/site-a/swatch.png")));assert_eq!(a.preview.as_ref().unwrap().sky,sky);
    }
    click(&c,&mut a,size,"catalog");click(&c,&mut a,size,"catalog-new");
    for (id,value) in [("catalog-entry-id","chair"),("catalog-tag","hair.choice.chair"),("catalog-name","Chair"),("catalog-asset-id","chair-art"),("catalog-file","chair.glb")] {type_value(&c,&mut a,size,id,value);}
    // Document and legacy Decoration share the same source; door owns the
    // second and the catalog explicitly acquires the third permitted source.
    choose(&c,&mut a,size,"catalog-import",DialogKind::CatalogImport,Some(root.join("assets/kenney/chairDesk.glb")));assert_eq!(a.preview.as_ref().unwrap().sky,sky);
    let surfaces=a.project.as_ref().unwrap().surfaces();let catalog=toml::to_string(a.project.as_ref().unwrap().catalog().unwrap()).unwrap();
    let before=a.preview.take().unwrap();let vertices=bytemuck::cast_slice::<_,u8>(&before.vertices).to_vec();
    click(&c,&mut a,size,"sky");let selected=base.join("selected.png");png_input(&selected,5,3);
    choose(&c,&mut a,size,"sky-png",DialogKind::SkyPanorama,Some(selected));
    let after=a.preview.as_ref().unwrap();assert_eq!(before.host,after.host);assert_eq!(before.loaded_roles,after.loaded_roles);assert_eq!(vertices,bytemuck::cast_slice::<_,u8>(&after.vertices));
    assert_eq!(a.project.as_ref().unwrap().surfaces(),surfaces);assert_eq!(toml::to_string(a.project.as_ref().unwrap().catalog().unwrap()).unwrap(),catalog);
    choose(&c,&mut a,size,"save-new",DialogKind::SaveNew,Some(base.join("saved")));
    choose(&c,&mut a,size,"open",DialogKind::Open,Some(base.join("saved/project.toml")));
    choose(&c,&mut a,size,"export",DialogKind::Export,Some(base.join("visitor")));same(a.preview.as_ref().unwrap(),&visitor(a.exported_pack.as_ref().unwrap()));
    assert!(!base.join(".hair-layout").exists());
}

#[test]
fn actual_input_typed_adapter_wrong_context_and_base_vocabulary_are_explicit() {
    let base=fixture("typed");let selected=base.join("owned.png");png_input(&selected,3,5);
    let c=egui::Context::default();let mut a=Workshop::new(vec![]);let size=[900.,700.];
    click(&c,&mut a,size,"new-material");click(&c,&mut a,size,"sky");
    click(&c,&mut a,size,"advanced-paths");type_value(&c,&mut a,size,"import-path",selected.to_str().unwrap());
    let revision=a.revision;click(&c,&mut a,size,"import-path-action");assert!(a.error.is_none(),"{:?}",a.error);assert_eq!(a.revision,revision+1);
    assert!(matches!(a.preview.as_ref().unwrap().sky,SkyAppearance::Panorama(_)));
    click(&c,&mut a,size,"advanced-paths");click(&c,&mut a,size,"document");
    let text=frame(&c,&mut a,size,vec![]);
    for wording in ["Editing: Document object base","shared legacy document role","built-in class art","No independent base-only or file-body authoring","non-solid"] {assert!(text.contains(wording),"{wording}: {text}");}
    // Deliberately request the wrong action/category through the public boundary;
    // it must never turn a document GLB action into a sky replacement.
    let old=state(&a);a.request_dialog(DialogKind::SkyPanorama);assert_eq!(a.begin_dialog(),Some(DialogKind::SkyPanorama));
    a.finish_dialog(Ok(Some(selected)));assert!(a.error.as_deref().unwrap().contains("context changed"));assert_eq!(state(&a),old);
    click(&c,&mut a,size,"ack-error");
    choose(&c,&mut a,size,"import",DialogKind::Import,Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/kenney/desk.glb")));
    click(&c,&mut a,size,"confirm-discard");
    click(&c,&mut a,size,"sky");
    // Explicitly injected invalid public model state forces core Theme rollback,
    // not merely a draft-parser refusal. Repair only the injection afterward.
    a.project.as_mut().unwrap().scale=f32::NAN;let old=state(&a);click(&c,&mut a,size,"sky-clear");
    assert!(a.error.is_some());assert_eq!(state(&a),old);assert!(matches!(a.preview.as_ref().unwrap().sky,SkyAppearance::Panorama(_)));
    a.project.as_mut().unwrap().scale=1.;
}
