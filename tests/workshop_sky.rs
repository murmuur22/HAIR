#![cfg(feature = "workshop")]
use hair::{sky::SkyAppearance, theme::Theme, workshop::{DialogKind, Workshop}};
use std::{fs, path::{Path, PathBuf}};
#[path = "workshop_sky/cases.rs"]
mod cases;

fn fixture(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hair-7d-{label}-{}", std::process::id()));
    fs::create_dir(&p).unwrap();
    println!("SLICE7D_FIXTURE={}", p.display());
    p
}
fn frame(c: &egui::Context, a: &mut Workshop, size: [f32;2], events: Vec<egui::Event>) -> String {
    let out = c.run_ui(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0],size[1]))), events, ..Default::default() }, |ui| a.ui(ui,None));
    fn text(s:&egui::epaint::Shape, t:&mut String) {
        match s { egui::epaint::Shape::Text(s)=>{t.push_str(&s.galley.job.text);t.push('\n');}, egui::epaint::Shape::Vec(v)=>for s in v {text(s,t)}, _=>{} }
    }
    let mut t=String::new(); for s in &out.shapes {text(&s.shape,&mut t);} out.drop_without_applying_deltas(); t
}
fn click(c:&egui::Context,a:&mut Workshop,size:[f32;2],id:&str) {
    for _ in 0..3 {frame(c,a,size,vec![]);}
    for _ in 0..100 {
        let r=*a.controls.get(id).unwrap_or_else(||panic!("SKY_UI_BEHAVIORAL_RED: missing actual control {id}"));
        if matches!(id,"ack-error"|"cancel-discard"|"confirm-discard"|"quit"|"camera-mode") {break;}
        let clip=a.controls["authoring-viewport"];
        if clip.contains(r.center()) {break;}
        let delta=if r.center().y<clip.top(){100.}else{-100.};
        frame(c,a,size,vec![egui::Event::PointerMoved(clip.center()),egui::Event::MouseWheel {unit:egui::MouseWheelUnit::Point,phase:egui::TouchPhase::Move,source:egui::MouseWheelSource::Unknown,delta:egui::vec2(0.,delta),modifiers:Default::default()}]);
        for _ in 0..3 {frame(c,a,size,vec![]);}
    }
    let r=a.controls[id]; let clip=a.controls["authoring-viewport"];
    // Global status/modal controls need not live in the authoring scroll area.
    if !matches!(id,"ack-error"|"cancel-discard"|"confirm-discard"|"quit"|"camera-mode") {
        assert!(clip.contains(r.center()),"unreachable {id} at {size:?}: {r:?} {clip:?}");
        assert!(r.left()>=clip.left() && r.right()<=clip.right()+1.,"horizontal clipping {id}");
    }
    for pressed in [true,false] {frame(c,a,size,vec![egui::Event::PointerMoved(r.center()),egui::Event::PointerButton{pos:r.center(),button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
    frame(c,a,size,vec![]);
}
fn type_value(c:&egui::Context,a:&mut Workshop,size:[f32;2],id:&str,value:&str) {
    click(c,a,size,id);
    let modifiers=egui::Modifiers{command:true,mac_cmd:true,..Default::default()};
    frame(c,a,size,vec![egui::Event::Key{key:egui::Key::A,physical_key:None,pressed:true,repeat:false,modifiers},egui::Event::Key{key:egui::Key::A,physical_key:None,pressed:false,repeat:false,modifiers},egui::Event::Text(value.into())]);
}
fn choose(c:&egui::Context,a:&mut Workshop,size:[f32;2],id:&str,kind:DialogKind,path:Option<PathBuf>) {
    click(c,a,size,id); assert_eq!(a.begin_dialog(),Some(kind)); a.finish_dialog(Ok(path)); frame(c,a,size,vec![]);
    assert!(a.error.is_none(),"{:?}",a.error);
}
fn same(a:&Theme,b:&Theme) {
    assert_eq!(a.sky,b.sky); assert_eq!(a.host,b.host); assert_eq!(a.loaded_roles,b.loaded_roles);
    assert_eq!(bytemuck::cast_slice::<_,u8>(&a.vertices),bytemuck::cast_slice::<_,u8>(&b.vertices));
    assert_eq!(a.textures.len(),b.textures.len()); for (a,b) in a.textures.iter().zip(&b.textures) {assert_eq!((a.width,a.height,&a.rgba),(b.width,b.height,&b.rgba));}
    assert_eq!(a.decorations.entries,b.decorations.entries);
}
fn visitor(p:&Path)->Theme {Theme::load(p,&mut Default::default()).unwrap()}
#[test]
fn actual_input_colour_new_save_cold_reopen_export_visitor() {
    let base=fixture("colour"); let c=egui::Context::default(); let mut a=Workshop::new(vec![]); let size=[900.,700.];
    click(&c,&mut a,size,"new-material"); click(&c,&mut a,size,"sky");
    assert!(a.project.as_ref().unwrap().sky().is_none());
    let revision=a.revision;
    for (id,value) in [("sky-red","0.125"),("sky-green","0.75"),("sky-blue","1")] {type_value(&c,&mut a,size,id,value);}
    assert_eq!(a.revision,revision); assert!(a.project.as_ref().unwrap().sky().is_none());
    click(&c,&mut a,size,"sky-apply"); assert_eq!(a.revision,revision+1); assert!(a.dirty); assert!(a.error.is_none(),"{:?}",a.error);
    assert_eq!(a.preview.as_ref().unwrap().sky,SkyAppearance::Colour([0.125,0.75,1.]));
    choose(&c,&mut a,size,"save-new",DialogKind::SaveNew,Some(base.join("saved"))); assert!(!a.dirty);
    let saved=a.saved_project.clone().unwrap(); let preview=a.preview.take().unwrap(); drop(a);
    let mut a=Workshop::new(vec![]);
    choose(&c,&mut a,size,"open",DialogKind::Open,Some(saved.clone())); same(&preview,a.preview.as_ref().unwrap());
    choose(&c,&mut a,size,"export",DialogKind::Export,Some(base.join("visitor")));
    same(&preview,&visitor(a.exported_pack.as_ref().unwrap())); assert_eq!(a.saved_project,Some(saved)); assert!(!a.dirty);
}
