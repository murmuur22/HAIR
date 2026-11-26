#![cfg(feature = "workshop")]
use hair::workshop::{DialogKind, Workshop};
use std::{fs, path::Path};

fn frame(a: &mut Workshop, c: &egui::Context, events: Vec<egui::Event>) {
    c.run_ui(egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 1800.))),
        events, ..Default::default()
    }, |ui| a.ui(ui, Some(egui::TextureId::User(0)))).drop_without_applying_deltas();
}
fn click(a: &mut Workshop, c: &egui::Context, id: &str) {
    for _ in 0..3 { frame(a, c, vec![]); }
    let p = a.controls.get(id).unwrap_or_else(|| panic!("actual Workshop control missing: {id}")).center();
    for pressed in [true, false] {
        frame(a, c, vec![egui::Event::PointerMoved(p), egui::Event::PointerButton {
            pos:p, button:egui::PointerButton::Primary, pressed, modifiers:Default::default()
        }]);
    }
}
fn text(a: &mut Workshop, c: &egui::Context, id: &str, value: &str) {
    click(a,c,id);
    frame(a,c,vec![egui::Event::Key { key:egui::Key::A, physical_key:None, pressed:true, repeat:false,
        modifiers:egui::Modifiers { command:true, mac_cmd:true, ..Default::default() } }, egui::Event::Text(value.into())]);
    frame(a,c,vec![egui::Event::Key { key:egui::Key::A, physical_key:None, pressed:false, repeat:false, modifiers:Default::default() }]);
}
fn choose(a: &mut Workshop, c: &egui::Context, id: &str, path: &Path) {
    click(a,c,id);
    assert!(a.begin_dialog().is_some(), "{id} must request native chooser");
    assert_eq!(a.begin_dialog(),None,"native request must issue once");
    a.finish_dialog(Ok(Some(path.to_path_buf())));
    assert!(a.error.is_none(), "{id}: {:?}", a.error);
}
#[test]
fn actual_input_catalog_chair_save_reopen_export_visitor_trace() {
    let root=Path::new(env!("CARGO_MANIFEST_DIR"));
    let base=std::env::temp_dir().join(format!("hair-7b-ui-trace-{}",std::process::id()));
    fs::create_dir(&base).unwrap();
    println!("SLICE7B_FIXTURE={}",base.display());
    let source=base.join("owned-chair.glb");
    fs::copy(root.join("assets/kenney/chairDesk.glb"),&source).unwrap();
    let original=fs::read(&source).unwrap();
    let c=egui::Context::default();
    let mut a=Workshop::new(vec![root.into()]);
    click(&mut a,&c,"new-material");
    click(&mut a,&c,"catalog");
    click(&mut a,&c,"catalog-new");
    for (id,value) in [("catalog-entry-id","reading-chair"),("catalog-tag","hair.seating.chair"),("catalog-name","Reading chair"),("catalog-asset-id","chair-art"),("catalog-file","chair.glb")] {
        text(&mut a,&c,id,value);
    }
    let revision=a.revision;
    choose(&mut a,&c,"catalog-import",&source);
    assert_eq!(a.revision,revision+1);
    assert!(a.dirty);
    assert_eq!(a.preview.as_ref().unwrap().decorations.entries[0].id,"reading-chair");
    text(&mut a,&c,"catalog-name","Green reading chair");
    text(&mut a,&c,"catalog-tag","hair.seating.reading");
    text(&mut a,&c,"catalog-yaw","35");
    text(&mut a,&c,"catalog-scale","0.8");
    click(&mut a,&c,"catalog-collision");
    assert_eq!(a.revision,revision+1,"draft is not an edit transaction");
    click(&mut a,&c,"catalog-apply");
    assert!(a.error.is_none(),"{:?}",a.error);
    assert_eq!(a.revision,revision+2);
    let inspection=a.catalog_inspection().unwrap().unwrap().0;
    assert!(!inspection.is_empty(),"selected real mesh, not just host placeholders");
    let before=toml::to_string(a.project.as_ref().unwrap().catalog().unwrap()).unwrap();
    choose(&mut a,&c,"save-new",&base.join("saved"));
    assert!(!a.dirty);
    choose(&mut a,&c,"open",&base.join("saved/project.toml"));
    assert!(!a.dirty);
    assert_eq!(toml::to_string(a.project.as_ref().unwrap().catalog().unwrap()).unwrap(),before);
    assert_eq!(a.selected_catalog_entry(),None,"open must retire old context");
    let reopened_revision=a.revision;
    click(&mut a,&c,"catalog-row-0");
    assert_eq!(a.selected_catalog_entry(),Some("reading-chair"));
    assert_eq!((a.revision,a.dirty),(reopened_revision,false),"selection alone is non-dirty");
    assert_eq!(bytemuck::cast_slice::<_,u8>(&inspection),bytemuck::cast_slice::<_,u8>(&a.catalog_inspection().unwrap().unwrap().0));
    choose(&mut a,&c,"export",&base.join("export"));
    let visitor=hair::theme::Theme::load(&base.join("export/pack.toml"),&mut Default::default()).unwrap();
    let preview=a.preview.as_ref().unwrap();
    assert_eq!(preview.decorations.entries,visitor.decorations.entries);
    assert_eq!(preview.decorations.assets[0].spec,visitor.decorations.assets[0].spec);
    assert_eq!(bytemuck::cast_slice::<_,u8>(&preview.decorations.assets[0].model.vertices),bytemuck::cast_slice::<_,u8>(&visitor.decorations.assets[0].model.vertices));
    let entry=visitor.decorations.entry("reading-chair").unwrap();
    let transient=hair::placement_store::Decoration::from_entry(entry,[0.,0.],0.,[1.6;3]).unwrap();
    assert_eq!(bytemuck::cast_slice::<_,u8>(&inspection),bytemuck::cast_slice::<_,u8>(&visitor.decoration_preview(&transient).unwrap()));
    assert!(visitor.host.decorations.is_empty());
    assert_eq!(preview.host,visitor.host);
    assert_eq!((&*entry.name,&*entry.tag,entry.defaults.collision),("Green reading chair","hair.seating.reading",true));
    assert_eq!(fs::read(&source).unwrap(),original);
    assert_eq!(fs::read(base.join("export/chair.glb")).unwrap(),original);
    println!("SLICE7B_VISITOR_PACK={}",base.join("export/pack.toml").display());
    // Owned fixture retained for parent inspection.
}

fn start(label:&str) -> (Workshop,egui::Context,std::path::PathBuf) {
    let root=Path::new(env!("CARGO_MANIFEST_DIR"));
    let base=std::env::temp_dir().join(format!("hair-7b-{label}-{}",std::process::id()));
    fs::create_dir(&base).unwrap();
    println!("SLICE7B_FIXTURE={}",base.display());
    fs::copy(root.join("assets/kenney/chairDesk.glb"),base.join("owned-chair.glb")).unwrap();
    fs::copy(root.join("assets/kenney/desk.glb"),base.join("owned-desk.glb")).unwrap();
    let mut a=Workshop::new(vec![root.into()]);
    let c=egui::Context::default();
    click(&mut a,&c,"new-material"); click(&mut a,&c,"catalog");
    new_draft(&mut a,&c,"reading-chair","chair-art","chair.glb");
    choose(&mut a,&c,"catalog-import",&base.join("owned-chair.glb"));
    (a,c,base)
}
fn new_draft(a:&mut Workshop,c:&egui::Context,id:&str,asset:&str,file:&str) {
    click(a,c,"catalog-new");
    for (field,value) in [("catalog-entry-id",id),("catalog-name",id),("catalog-tag",&format!("hair.choice.{id}")),("catalog-asset-id",asset),("catalog-file",file)] {text(a,c,field,value);}
}
fn state(a:&Workshop) -> (u64,bool,String,Option<String>,Vec<u8>) {
    (a.revision,a.dirty,toml::to_string(a.project.as_ref().unwrap().catalog().unwrap()).unwrap(),a.selected_catalog_entry().map(str::to_owned),bytemuck::cast_slice::<_,u8>(&a.catalog_inspection().unwrap().unwrap().0).to_vec())
}

#[test]
fn actual_input_reuse_distinct_asset_lexical_selection_replacement_and_removal() {
    let (mut a,c,base)=start("choices");
    new_draft(&mut a,&c,"a-spare","chair-art","chair.glb");
    click(&mut a,&c,"catalog-reuse");click(&mut a,&c,"catalog-reuse-0");
    click(&mut a,&c,"catalog-apply");assert!(a.error.is_none(),"{:?}",a.error);
    assert_eq!(a.project.as_ref().unwrap().catalog().unwrap().assets.len(),1);
    new_draft(&mut a,&c,"writing-desk","desk-art","desk.glb");
    choose(&mut a,&c,"catalog-import",&base.join("owned-desk.glb"));
    assert_eq!(a.preview.as_ref().unwrap().decorations.entries.iter().map(|e|e.id.as_str()).collect::<Vec<_>>(),["a-spare","reading-chair","writing-desk"]);
    choose(&mut a,&c,"save-new",&base.join("saved"));
    let revision=a.revision;
    click(&mut a,&c,"catalog-row-1");
    assert_eq!(a.selected_catalog_entry(),Some("reading-chair"));
    assert_eq!((a.revision,a.dirty),(revision,false));
    let old=state(&a);
    frame(&mut a,&c,vec![]);
    let obsolete=a.controls.contains_key("catalog-remove-asset");
    if obsolete {
        click(&mut a,&c,"catalog-remove-asset");
        assert!(a.error.as_deref().unwrap().contains("still referenced"));assert_eq!(state(&a),old);
        println!("7B1_RED: impossible unreferenced-asset control is exposed and refuses the validated catalog");
        click(&mut a,&c,"ack-error");
    }
    assert!(!obsolete,"impossible unreferenced-asset control must not be exposed");
    let out=c.run_ui(egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1200.,1800.))),..Default::default()},|ui|a.ui(ui,None));
    fn labels(shape:&egui::epaint::Shape,text:&mut String) {
        match shape {
            egui::epaint::Shape::Text(t)=>text.push_str(&t.galley.job.text),
            egui::epaint::Shape::Vec(v)=>for s in v {labels(s,text);},
            _=>{}
        }
    }
    let mut instruction=String::new();for s in &out.shapes {labels(&s.shape,&mut instruction);}
    out.drop_without_applying_deltas();
    assert!(instruction.contains("Unreferenced assets retire automatically when their last choice is removed or rebound."),"truthful retirement instruction missing: {instruction}");
    choose(&mut a,&c,"catalog-replace",&base.join("owned-desk.glb"));
    assert_eq!(a.selected_catalog_entry(),Some("reading-chair"));
    assert_eq!(a.revision,revision+1);
    assert_ne!(state(&a).4,old.4);
    assert_eq!(a.project.as_ref().unwrap().catalog().unwrap().assets.len(),2);
    click(&mut a,&c,"catalog-remove-entry");
    assert_eq!(a.selected_catalog_entry(),None,"never silently select the new row at old index");
    assert_eq!(a.project.as_ref().unwrap().catalog().unwrap().assets.len(),2,"shared chair asset still referenced");
    click(&mut a,&c,"catalog-row-0");assert_eq!(a.selected_catalog_entry(),Some("a-spare"));
    click(&mut a,&c,"catalog-remove-entry");
    assert_eq!(a.project.as_ref().unwrap().catalog().unwrap().assets.len(),1);
    click(&mut a,&c,"catalog-row-0");assert_eq!(a.selected_catalog_entry(),Some("writing-desk"));
    click(&mut a,&c,"catalog-remove-entry");
    let spec=a.project.as_ref().unwrap().catalog().expect("explicit empty, not catalogNone");
    assert!(spec.entries.is_empty()&&spec.assets.is_empty());
    choose(&mut a,&c,"export",&base.join("empty-pack")); assert!(a.dirty,"export is not save");
    choose(&mut a,&c,"save-new",&base.join("empty-saved"));
    for (file,version) in [("empty-pack/pack.toml",3),("empty-saved/project.toml",4)] {
        let value:toml::Value=toml::from_str(&fs::read_to_string(base.join(file)).unwrap()).unwrap();
        assert_eq!(value["version"].as_integer(),Some(version));assert!(value.get("decorations").is_some());
    }
    choose(&mut a,&c,"open",&base.join("saved/project.toml"));
    assert_eq!(a.selected_catalog_entry(),None);assert!(a.catalog_inspection().unwrap().is_none());
    click(&mut a,&c,"catalog-row-1");assert_eq!(a.selected_catalog_entry(),Some("reading-chair"));
    assert!(!base.join(".hair-layout").exists());
    // Rebinding both chair choices to the retained desk retires the last chair
    // reference automatically; there is no unused asset state to remove manually.
    for row in ["catalog-row-0","catalog-row-1"] {
        click(&mut a,&c,row);click(&mut a,&c,"catalog-reuse");click(&mut a,&c,"catalog-reuse-1");
        click(&mut a,&c,"catalog-apply");assert!(a.error.is_none(),"{:?}",a.error);
    }
    let spec=a.project.as_ref().unwrap().catalog().unwrap();
    assert_eq!(spec.assets.len(),1);assert_eq!(spec.assets[0].id,"desk-art");
    assert!(spec.entries.iter().all(|e|e.asset=="desk-art"));
}

#[test]
fn actual_input_draft_reject_cancel_sources_and_snapshot_are_lastgood() {
    let (mut a,c,base)=start("refusals");
    choose(&mut a,&c,"save-new",&base.join("saved"));
    let old=state(&a);
    for (id,value,diagnostic) in [("catalog-scale","nonsense","Scale"),("catalog-yaw","NaN","Yaw"),("catalog-pivot-y","9","fit"),("catalog-tag","INVALID","entry"),("catalog-entry-id","other-id","stable")] {
        text(&mut a,&c,id,value);click(&mut a,&c,"catalog-apply");
        assert!(a.error.as_deref().unwrap().contains(diagnostic),"{:?}",a.error);
        assert_eq!(state(&a),old);
        click(&mut a,&c,"catalog-row-0");click(&mut a,&c,"ack-error");
    }
    for result in [Ok(None),Err("injected panel failure".into()),Ok(Some(base.join("missing.glb"))),Ok(Some(base.join("saved/project.toml")))] {
        click(&mut a,&c,"catalog-replace");assert_eq!(a.begin_dialog(),Some(DialogKind::CatalogReplace));
        assert_eq!(a.begin_dialog(),None);
        click(&mut a,&c,"catalog-new");text(&mut a,&c,"catalog-name","must not edit pending form");click(&mut a,&c,"catalog-remove-entry");click(&mut a,&c,"floor");
        a.finish_dialog(result);assert_eq!(state(&a),old);
        if a.error.is_some(){click(&mut a,&c,"ack-error");}
    }
    // Public active context revision changes must retire an in-flight snapshot.
    click(&mut a,&c,"catalog-replace");a.begin_dialog();a.revision+=1;
    a.finish_dialog(Ok(Some(base.join("owned-desk.glb"))));
    assert!(a.error.as_deref().unwrap().contains("context changed"));a.revision-=1;
    assert_eq!(state(&a),old);click(&mut a,&c,"ack-error");
    new_draft(&mut a,&c,"reading-chair","chair-art","chair.glb");
    click(&mut a,&c,"catalog-import");assert_eq!(a.begin_dialog(),Some(DialogKind::CatalogImport));
    a.finish_dialog(Ok(Some(base.join("owned-desk.glb"))));
    assert!(a.error.is_some());assert_eq!(state(&a),old,"no implicit replacement on duplicate import");
    assert_eq!(fs::read(base.join("saved/chair.glb")).unwrap(),fs::read(base.join("owned-chair.glb")).unwrap());
    #[cfg(unix)] {
        click(&mut a,&c,"ack-error");click(&mut a,&c,"catalog-row-0");
        std::os::unix::fs::symlink(&base,base.join("alias")).unwrap();
        click(&mut a,&c,"catalog-replace");a.begin_dialog();a.finish_dialog(Ok(Some(base.join("alias/owned-desk.glb"))));
        assert!(a.error.is_some());assert_eq!(state(&a),old,"selected sources retain strict nofollow ancestors");
    }
}

#[test]
fn actual_input_equalbytes_distinct_sources_and_fourth_asset_budget_refusal() {
    let (mut a,c,base)=start("budgets");
    for (id,asset,file) in [("second","second-art","second.glb"),("third","third-art","third.glb")] {
        new_draft(&mut a,&c,id,asset,file);choose(&mut a,&c,"catalog-import",&base.join("owned-chair.glb"));
    }
    let t=a.preview.as_ref().unwrap();
    assert_eq!(t.decorations.assets.len(),3);
    assert!(!std::sync::Arc::ptr_eq(&t.decorations.assets[0].model,&t.decorations.assets[1].model),"separately acquired equalbytes are not the same authority");
    let old=state(&a);
    new_draft(&mut a,&c,"fourth","fourth-art","fourth.glb");
    click(&mut a,&c,"catalog-import");a.begin_dialog();a.finish_dialog(Ok(Some(base.join("owned-chair.glb"))));
    assert!(a.error.as_deref().unwrap().contains("3 assets"));assert_eq!(state(&a),old);
}

// SYNTHETIC INPUT: real decoder accepts the catalog budget, but the shared
// visitor placement limit refuses this repeated-triangle mesh. Not fake output.
fn unplaceable_glb() -> Vec<u8> {
    let count=32769;
    let mut bin=vec![];
    for n in [0_f32,0.,0.,1.,0.,0.,0.,1.,0.,0.,0.,1.,0.,0.,1.,0.,0.,1.] {bin.extend(n.to_le_bytes());}
    for i in 0..count as u32 {bin.extend((i%3).to_le_bytes());}
    let mut json=format!(r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":{}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5125,"count":{count},"type":"SCALAR"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2}}]}}],"nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#,bin.len(),count*4).into_bytes();
    while !json.len().is_multiple_of(4){json.push(b' ');}
    let mut glb=vec![];
    for n in [0x46546c67_u32,2,(28+json.len()+bin.len()) as u32,json.len() as u32,0x4e4f534a] {glb.extend(n.to_le_bytes());}
    glb.extend(json);glb.extend((bin.len() as u32).to_le_bytes());glb.extend(0x004e4942_u32.to_le_bytes());glb.extend(bin);glb
}
#[test]
fn actual_input_shared_placement_refusal_retains_project_source_and_preview() {
    let (mut a,c,base)=start("placement-refusal");
    let source=base.join("synthetic-unplaceable.glb");fs::write(&source,unplaceable_glb()).unwrap();
    choose(&mut a,&c,"save-new",&base.join("saved"));let old=state(&a);
    click(&mut a,&c,"catalog-replace");a.begin_dialog();a.finish_dialog(Ok(Some(source.clone())));
    assert!(a.error.as_deref().unwrap().contains("inspection refused"));assert_eq!(state(&a),old);
    click(&mut a,&c,"ack-error");choose(&mut a,&c,"export",&base.join("lastgood-export"));
    assert_eq!(fs::read(base.join("lastgood-export/chair.glb")).unwrap(),fs::read(base.join("owned-chair.glb")).unwrap());
    new_draft(&mut a,&c,"large","large-art","large.glb");
    click(&mut a,&c,"catalog-import");a.begin_dialog();a.finish_dialog(Ok(Some(source)));
    assert!(a.error.as_deref().unwrap().contains("inspection refused"));assert_eq!(state(&a),old);
}

#[test]
fn actual_input_menu_text_and_chooser_completion_own_camera_gestures() {
    let (mut a,c,_)=start("ownership");
    click(&mut a,&c,"camera-mode");assert!(a.camera.free_fly);
    click(&mut a,&c,"catalog-name");
    let pose=a.camera.pose;
    frame(&mut a,&c,vec![egui::Event::Text("wasd".into()),egui::Event::MouseMoved(egui::vec2(40.,20.))]);
    assert!(!a.camera.captured);assert_eq!(a.camera.pose.position,pose.position);assert_eq!(a.camera.pose.yaw,pose.yaw);
    click(&mut a,&c,"catalog-reuse");assert!(egui::Popup::is_any_open(&c));
    let p=a.controls["preview"].center();
    for pressed in [true,false] {frame(&mut a,&c,vec![egui::Event::PointerMoved(p),egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
    assert!(!a.camera.captured,"menu dismissal must not click through into capture");
    click(&mut a,&c,"catalog-replace");assert_eq!(a.begin_dialog(),Some(DialogKind::CatalogReplace));
    a.finish_dialog(Ok(None));
    // Immediate/held gesture without a neutral post-panel frame must be inert.
    for pressed in [true,true,false] {frame(&mut a,&c,vec![egui::Event::PointerMoved(p),egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
    assert!(!a.camera.captured);assert_eq!(a.begin_dialog(),None);
    frame(&mut a,&c,vec![]);
    for pressed in [true,false] {frame(&mut a,&c,vec![egui::Event::PointerMoved(p),egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
    assert!(a.camera.captured,"a later fresh gesture must still capture normally");
}

fn sized_frame(a:&mut Workshop,c:&egui::Context,size:[f32;2],events:Vec<egui::Event>) {
    c.run_ui(egui::RawInput { screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(size[0],size[1]))), events, ..Default::default() },|ui|a.ui(ui,Some(egui::TextureId::User(0)))).drop_without_applying_deltas();
}
fn scroll_click(a:&mut Workshop,c:&egui::Context,size:[f32;2],id:&str) {
    for _ in 0..3 {sized_frame(a,c,size,vec![]);}
    for _ in 0..80 {
        let p=a.controls[id].center();let v=a.controls["authoring-viewport"];
        if v.contains(p){break;}
        let delta=if p.y<v.top(){100.}else{-100.};
        sized_frame(a,c,size,vec![egui::Event::PointerMoved(egui::pos2(v.right()-25.,v.center().y)),egui::Event::MouseWheel {unit:egui::MouseWheelUnit::Point,phase:egui::TouchPhase::Move,source:egui::MouseWheelSource::Unknown,delta:egui::vec2(0.,delta),modifiers:Default::default()}]);
        for _ in 0..3 {sized_frame(a,c,size,vec![]);}
    }
    let p=a.controls[id].center();assert!(a.controls["authoring-viewport"].contains(p),"unreachable {id} at {size:?}");
    for pressed in [true,false] {sized_frame(a,c,size,vec![egui::Event::PointerMoved(p),egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
}
#[test]
fn actual_input_small_catalog_scroll_keeps_apply_save_and_cancel_reachable() {
    for size in [[900.,700.],[640.,480.],[320.,480.]] {
        let (mut a,c,_)=start(&format!("small-{}",size[0]));
        scroll_click(&mut a,&c,size,"catalog-apply");assert!(a.error.is_none(),"{:?}",a.error);
        scroll_click(&mut a,&c,size,"save-new");assert_eq!(a.begin_dialog(),Some(DialogKind::SaveNew));
        let revision=a.revision; a.finish_dialog(Ok(None));assert_eq!(a.revision,revision);
        scroll_click(&mut a,&c,size,"catalog-replace");assert_eq!(a.begin_dialog(),Some(DialogKind::CatalogReplace));a.finish_dialog(Ok(None));
        sized_frame(&mut a,&c,size,vec![]);
        // Quit is in the reserved title, outside the scrolling form.
        let p=a.controls["quit"].center();
        for pressed in [true,false] {sized_frame(&mut a,&c,size,vec![egui::Event::PointerMoved(p),egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        assert!(a.confirmation_pending());
        sized_frame(&mut a,&c,size,vec![]);
        let p=a.controls["cancel-discard"].center();assert!(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(size[0],size[1])).contains(p));
        for pressed in [true,false] {sized_frame(&mut a,&c,size,vec![egui::Event::PointerMoved(p),egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        assert!(!a.confirmation_pending()&&!a.should_close);
    }
}

#[test]
#[ignore="targeted real Metal selected-mesh witness; synthetic input, not native/human or image inspection"]
fn actual_input_selected_catalog_mesh_metal_witness() {
    let (mut a,c,base)=start("metal");
    new_draft(&mut a,&c,"writing-desk","desk-art","desk.glb");choose(&mut a,&c,"catalog-import",&base.join("owned-desk.glb"));
    fs::remove_file(base.join("owned-chair.glb")).unwrap();fs::remove_file(base.join("owned-desk.glb")).unwrap();
    pollster::block_on(async {
        let instance=hair::render::instance();let adapter=hair::render::adapter(&instance,None).await.unwrap();
        let size=[1200,1800];let mut gpu=hair::workshop::WorkshopGpu::new(&adapter,wgpu::TextureFormat::Rgba8Unorm,size).await.unwrap();
        let target=gpu.target(size);let view=target.create_view(&Default::default());
        let mut captures=vec![];
        for (id,name) in [("catalog-row-0","selected-chair.png"),("catalog-row-1","selected-desk.png")] {
            click(&mut a,&c,id);
            for _ in 0..3 {
                gpu.frame(&c,&mut a,egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(size[0] as f32,size[1] as f32))),..Default::default()},&view,size).unwrap();
            }
            let pixels=gpu.read_pixels(&target,size).unwrap();
            let file=fs::OpenOptions::new().write(true).create_new(true).open(base.join(name)).unwrap();
            let mut encoder=png::Encoder::new(file,size[0],size[1]);encoder.set_color(png::ColorType::Rgba);encoder.set_depth(png::BitDepth::Eight);encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
            let r=a.controls["preview"];
            let mut crop=vec![];
            for y in r.top().ceil() as usize..r.bottom().floor() as usize {for x in r.left().ceil() as usize..r.right().floor() as usize {crop.extend_from_slice(&pixels[(y*size[0] as usize+x)*4..(y*size[0] as usize+x)*4+4]);}}
            assert!(crop.as_chunks::<4>().0.iter().collect::<std::collections::HashSet<_>>().len()>8,"real selected model is not a flat blank image");captures.push(crop);
            println!("SLICE7B_METAL_CAPTURE={}",base.join(name).display());
        }
        assert_ne!(captures[0],captures[1],"actual preview pixels must change with selected asset, independently of UI text");
    });
}
