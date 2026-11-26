#![cfg(feature = "workshop")]
//! Real compositor/visitor appearance, synthetic input only. Never native-panel acceptance.
use hair::workshop::{DialogKind, Workshop, WorkshopGpu};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::{Path,PathBuf}};
fn root()->&'static Path {Path::new(env!("CARGO_MANIFEST_DIR"))}
fn hash(b:&[u8])->String {format!("{:x}",Sha256::digest(b))}
struct Harness {gpu:WorkshopGpu,ctx:egui::Context,app:Workshop,target:wgpu::Texture,size:[u32;2],time:f64,base:PathBuf,adapter:String}
impl Harness {
    async fn new(adapter:&wgpu::Adapter,base:PathBuf)->Self {
        let size=[1280,960];let gpu=WorkshopGpu::new(adapter,wgpu::TextureFormat::Rgba8Unorm,size).await.unwrap();let target=gpu.target(size);
        Self{gpu,ctx:Default::default(),app:Workshop::new(vec![root().into()]),target,size,time:0.,base,adapter:format!("{:?}",adapter.get_info())}
    }
    fn frame(&mut self,events:Vec<egui::Event>) {
        self.time+=1./30.;let view=self.target.create_view(&Default::default());
        self.gpu.frame(&self.ctx,&mut self.app,egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(self.size[0] as f32,self.size[1] as f32))),time:Some(self.time),events,..Default::default()},&view,self.size).unwrap();
    }
    fn settle(&mut self){for _ in 0..4 {self.frame(vec![]);}}
    fn reveal(&mut self,id:&str) {
        self.settle();if matches!(id,"ack-error"){return;}
        for _ in 0..100 {let p=self.app.controls[id].center();let v=self.app.controls["authoring-viewport"];if v.shrink(3.).contains(p){return;}
            let dy=if p.y<v.top(){120.}else{-120.};self.frame(vec![egui::Event::PointerMoved(egui::pos2(v.right()-25.,v.center().y)),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,phase:egui::TouchPhase::Move,source:egui::MouseWheelSource::Unknown,delta:egui::vec2(0.,dy),modifiers:Default::default()}]);self.settle();}
        panic!("unreachable {id} {:?}",self.size);
    }
    fn click(&mut self,id:&str){self.reveal(id);let pos=self.app.controls[id].center();for pressed in [true,false]{self.frame(vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}self.settle();}
    fn text(&mut self,id:&str,value:&str){self.click(id);let modifiers=egui::Modifiers{command:true,mac_cmd:true,..Default::default()};self.frame(vec![egui::Event::Key{key:egui::Key::A,physical_key:None,pressed:true,repeat:false,modifiers},egui::Event::Text(value.into())]);self.frame(vec![egui::Event::Key{key:egui::Key::A,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);self.settle();}
    fn choose(&mut self,id:&str,kind:DialogKind,path:&Path){self.click(id);assert_eq!(self.app.begin_dialog(),Some(kind));assert_eq!(self.app.begin_dialog(),None);self.app.finish_dialog(Ok(Some(path.into())));self.settle();assert!(self.app.error.is_none(),"{:?}",self.app.error);}
    fn resize(&mut self,size:[u32;2]){self.size=size;self.target=self.gpu.target(size);self.settle();}
    fn sky_view(&mut self) {
        // Test-only session camera fixture, NOT a native physical camera claim.
        // Every world vertex is below the eye. Pitch1.4 rad minus the full
        // 32.5° half vertical FOV remains positive. The central inset crop
        // therefore contains only upward sky rays, never world geometry/labels.
        let max_y=self.app.preview.as_ref().unwrap().vertices.iter().map(|v|v.position[1]).fold(f32::NEG_INFINITY,f32::max);
        self.app.camera.free_fly=true;self.app.camera.height=max_y+10.;self.app.camera.pose.pitch=1.4;self.app.camera.pose.yaw=0.;self.settle();
        assert!(self.app.camera.height>max_y);assert!(self.app.camera.pose.pitch>65_f32.to_radians()/2.);
    }
    fn capture(&mut self,name:&str)->Vec<u8>{
        self.settle();let pixels=self.gpu.read_pixels(&self.target,self.size).unwrap();let dir=self.base.join(name);fs::create_dir(&dir).unwrap();
        let output=hair::resources::smoke_output(root(),&dir).unwrap();let file=output.create_capture("office.png").unwrap();let mut encoder=png::Encoder::new(file,self.size[0],self.size[1]);encoder.set_color(png::ColorType::Rgba);encoder.set_depth(png::BitDepth::Eight);let mut writer=encoder.write_header().unwrap();writer.write_image_data(&pixels).unwrap();writer.finish().unwrap();
        let path=dir.join("office.png");let state=format!("capture={}\nsize={:?}\nadapter={}\npng_sha256={}\nrgba_sha256={}\nrevision={} dirty={}\nsky={:?}\nsaved={:?}\nexported={:?}\nerror={:?}\ncontrols={:?}\n",path.display(),self.size,self.adapter,hash(&fs::read(&path).unwrap()),hash(&pixels),self.app.revision,self.app.dirty,self.app.project.as_ref().and_then(|p|p.sky()),self.app.saved_project,self.app.exported_pack,self.app.error,self.app.controls);
        let mut receipt=fs::OpenOptions::new().write(true).create_new(true).open(dir.join("state.txt")).unwrap();receipt.write_all(state.as_bytes()).unwrap();receipt.sync_all().unwrap();println!("7D_CAPTURE {state}");
        // Preserve failures before assertions. The independent missing-atlas
        // negative and RGB100 title oracle remain in the unchanged 7B1 witness.
        let q=self.app.controls["quit"];let title=egui::Rect::from_min_max(egui::pos2(5.,q.top()-3.),egui::pos2(350.,q.bottom()+3.));
        assert!(crop(&pixels,self.size,title).as_chunks::<4>().0.iter().filter(|p|p[0]>100&&p[1]>100&&p[2]>100).count()>200,"complete font atlas/title required");
        pixels
    }
    fn sky_crop(&self,pixels:&[u8])->Vec<u8>{let r=self.app.controls["preview"];crop(pixels,self.size,egui::Rect::from_center_size(r.center(),r.size()*0.5))}
}
fn crop(pixels:&[u8],size:[u32;2],r:egui::Rect)->Vec<u8>{let mut v=vec![];for y in r.top().ceil() as usize..r.bottom().floor() as usize{for x in r.left().ceil() as usize..r.right().floor() as usize{v.extend_from_slice(&pixels[(y*size[0] as usize+x)*4..(y*size[0] as usize+x)*4+4]);}}v}
fn changed(a:&[u8],b:&[u8])->usize {assert_eq!(a.len(),b.len());a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()).filter(|(a,b)|a!=b).count()}
#[test]
#[ignore="explicit full composited Metal sky-only counterfactual; synthetic input/callbacks, no OS panel/physical/Retina/human/package acceptance"]
fn actual_input_full_sky_composited_metal_witness(){
    let base=std::env::temp_dir().join(format!("hair-7d-metal-{}",std::process::id()));fs::create_dir(&base).unwrap();println!("7D_OWNED_FIXTURE={}",base.display());
    let selected=base.join("owned-selected.png");
    // Synthetic non2:1 alpha PNG INPUT, real shared adapter and GPU OUTPUT.
    let pixels:Vec<u8>=(0..21).flat_map(|i|[20,(i*11) as u8,210,if i%3==0{80}else{255}]).collect();
    let mut encoder=png::Encoder::new(fs::OpenOptions::new().write(true).create_new(true).open(&selected).unwrap(),7,3);encoder.set_color(png::ColorType::Rgba);encoder.set_depth(png::BitDepth::Eight);encoder.write_header().unwrap().write_image_data(&pixels).unwrap();let original=fs::read(&selected).unwrap();println!("7D_SELECTED sha256={}",hash(&original));
    pollster::block_on(async{
        let instance=hair::render::instance();let adapter=hair::render::adapter(&instance,None).await.unwrap();assert_eq!(adapter.get_info().backend,wgpu::Backend::Metal);
        // This fresh context is GPU-owned from its very first frame.
        let mut h=Harness::new(&adapter,base.clone()).await;h.click("new-material");h.click("sky");h.sky_view();
        let legacy=h.capture("normal1280-legacy-sky-only");let legacy_crop=h.sky_crop(&legacy);
        h.text("sky-red","0.1");h.text("sky-green","0.7");h.text("sky-blue","0.2");h.click("sky-apply");h.sky_view();
        let colour=h.capture("normal1280-colour-form-sky-only");let colour_crop=h.sky_crop(&colour);
        let delta=changed(&legacy_crop,&colour_crop);println!("7D_SKY_ONLY legacy_to_colour_changed_pixels={delta} compared_pixels={} excludes_labels_world_geometry=true",colour_crop.len()/4);assert!(delta>10000);
        h.choose("sky-png",DialogKind::SkyPanorama,&selected);h.sky_view();let panorama=h.capture("normal1280-panorama-form-sky-only");let panorama_crop=h.sky_crop(&panorama);let delta=changed(&colour_crop,&panorama_crop);println!("7D_SKY_ONLY colour_to_panorama_changed_pixels={delta}");assert!(delta>10000);
        h.choose("save-new",DialogKind::SaveNew,&base.join("saved"));fs::remove_file(&selected).unwrap();
        h.choose("open",DialogKind::Open,&base.join("saved/project.toml"));h.sky_view();let reopened=h.capture("normal1280-reopened-source-deleted-sky-only");assert_eq!(h.sky_crop(&reopened),panorama_crop);
        h.choose("export",DialogKind::Export,&base.join("visitor"));let visitor=hair::theme::Theme::load(h.app.exported_pack.as_ref().unwrap(),&mut Default::default()).unwrap();assert_eq!(visitor.sky,h.app.preview.as_ref().unwrap().sky);assert_eq!(fs::read(base.join("visitor/sky.png")).unwrap(),original);
        h.sky_view();let exported=h.capture("normal1280-export-agreement-sky-only");assert_eq!(h.sky_crop(&exported),panorama_crop);
        // Positive pixels above are the production visitor renderer, not injected
        // expected pixels. Return to normal stage for readable form/error captures.
        h.app.frame_camera();h.capture("normal1280-stage-form");h.reveal("sky-clear");h.capture("normal1280-actions");
        h.text("sky-green","NaN");let revision=h.app.revision;h.click("sky-apply");h.capture("normal1280-error-lastgood");assert!(h.app.error.is_some());assert_eq!(h.app.revision,revision);h.click("ack-error");
        h.resize([900,700]);h.reveal("sky-green");h.capture("native-min900-form");h.reveal("sky-clear");h.capture("native-min900-actions");
        h.click("sky-apply");h.capture("native-min900-error-lastgood");assert!(h.app.error.is_some());h.click("ack-error");
        h.click("sky-png");assert_eq!(h.app.begin_dialog(),Some(DialogKind::SkyPanorama));h.app.finish_dialog(Ok(None));h.settle();assert_eq!(h.app.revision,revision);
        h.click("sky-clear");assert_eq!(h.app.preview.as_ref().unwrap().sky,hair::sky::SkyAppearance::Legacy);h.capture("native-min900-cleared-legacy");
        h.resize([640,480]);h.reveal("sky-apply");h.reveal("sky-png");h.reveal("sky-clear");h.capture("offscreen-stress640-actions-not-native-min");
        assert!(!base.join(".hair-layout").exists());println!("7D_SCOPE backend=Metal normal1280x960 native_min900x700 stress640x480 synthetic_input=true native_OS_panel=false physical=false Retina=false human=false package_ready=false");
    });
}
