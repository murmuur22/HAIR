#![cfg(feature = "workshop")]
//! Finite offscreen Metal witness. Synthetic egui events and chooser callbacks,
//! not OS panels, a physical window, human inspection or package acceptance.
use hair::workshop::{DialogKind, Workshop, WorkshopGpu};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::{Path, PathBuf}};

fn root() -> &'static Path { Path::new(env!("CARGO_MANIFEST_DIR")) }
fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn raw(size: [u32; 2], time: f64, events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0] as f32, size[1] as f32))),
        time: Some(time), events, ..Default::default()
    }
}
fn crop(pixels: &[u8], size: [u32; 2], r: egui::Rect) -> Vec<u8> {
    let mut out=vec![];
    for y in r.top().ceil().max(0.) as usize..r.bottom().floor().min(size[1] as f32) as usize {
        for x in r.left().ceil().max(0.) as usize..r.right().floor().min(size[0] as f32) as usize {
            out.extend_from_slice(&pixels[(y*size[0] as usize+x)*4..(y*size[0] as usize+x)*4+4]);
        }
    }
    out
}
fn changed(a: &[u8], b: &[u8]) -> usize {
    assert_eq!(a.len(),b.len(),"counterfactual regions must have identical extents");
    a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()).filter(|(a,b)|a!=b).count()
}
fn bright(pixels: &[u8]) -> usize {
    // Actual egui title glyphs peak at RGB(140,140,140) on RGB(27,27,27).
    // The retained >140 oracle rejected valid glyphs. Count high-contrast grey
    // ink above 100, and retain the independently rendered missing-atlas zero
    // control below; flat panel/background pixels cannot satisfy this threshold.
    pixels.as_chunks::<4>().0.iter().filter(|p|p[0]>100 && p[1]>100 && p[2]>100).count()
}
fn write_capture(base: &Path, name: &str, size: [u32; 2], pixels: &[u8], state: &str) {
    let dir=base.join(name);fs::create_dir(&dir).unwrap();
    // Reuse the real protected/descriptor-held, O_EXCL + O_NOFOLLOW capture path.
    // Each descriptive directory owns one allowlisted office.png; no old frame
    // or source directory is used as a capture target.
    let output=hair::resources::smoke_output(root(),&dir).unwrap();
    let file=output.create_capture("office.png").unwrap();
    let mut encoder=png::Encoder::new(file,size[0],size[1]);
    encoder.set_color(png::ColorType::Rgba);encoder.set_depth(png::BitDepth::Eight);
    let mut writer=encoder.write_header().unwrap();writer.write_image_data(pixels).unwrap();writer.finish().unwrap();
    let path=dir.join("office.png");
    let receipt=format!("capture={}\ndimensions={size:?}\nrgba_sha256={}\npng_sha256={}\n{state}\n",path.display(),hash(pixels),hash(&fs::read(&path).unwrap()));
    let mut file=fs::OpenOptions::new().write(true).create_new(true).open(dir.join("state.txt")).unwrap();
    file.write_all(receipt.as_bytes()).unwrap();file.sync_all().unwrap();
    println!("7B1_CAPTURE {receipt}");
}
struct Harness {
    gpu: WorkshopGpu,
    ctx: egui::Context,
    app: Workshop,
    target: wgpu::Texture,
    size: [u32; 2],
    time: f64,
    base: PathBuf,
    adapter: String,
}
impl Harness {
    async fn new(adapter: &wgpu::Adapter, base: PathBuf) -> Self {
        let size=[1280,960];
        let gpu=WorkshopGpu::new(adapter,wgpu::TextureFormat::Rgba8Unorm,size).await.unwrap();
        let target=gpu.target(size);
        Self {gpu,ctx:egui::Context::default(),app:Workshop::new(vec![root().into()]),target,size,time:0.,base,adapter:format!("{:?}",adapter.get_info())}
    }
    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time+=1./30.;
        let view=self.target.create_view(&Default::default());
        // No CPU run_ui and no discarded texture delta on this context, ever.
        self.gpu.frame(&self.ctx,&mut self.app,raw(self.size,self.time,events),&view,self.size).unwrap();
    }
    fn settle(&mut self) {for _ in 0..4 {self.frame(vec![]);}}
    fn resize(&mut self,size:[u32;2]) {self.size=size;self.target=self.gpu.target(size);self.settle();}
    fn reveal(&mut self,id:&str) {
        self.settle();
        if matches!(id,"preview"|"camera-mode"|"frame-camera"|"ack-error"|"quit"|"cancel-discard") {return;}
        for _ in 0..100 {
            let p=self.app.controls.get(id).unwrap_or_else(||panic!("missing real control {id}")).center();
            let v=self.app.controls["authoring-viewport"];
            if v.shrink(3.).contains(p) {return;}
            let dy=if p.y<v.top(){120.}else{-120.};
            self.frame(vec![egui::Event::PointerMoved(egui::pos2(v.right()-25.,v.center().y)),egui::Event::MouseWheel {
                unit:egui::MouseWheelUnit::Point,phase:egui::TouchPhase::Move,source:egui::MouseWheelSource::Unknown,
                delta:egui::vec2(0.,dy),modifiers:Default::default()
            }]);self.settle();
        }
        panic!("unreachable real control {id} at {:?}",self.size);
    }
    fn click(&mut self,id:&str) {
        self.reveal(id);let p=self.app.controls[id].center();
        for pressed in [true,false] {self.frame(vec![egui::Event::PointerMoved(p),egui::Event::PointerButton {
            pos:p,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()
        }]);}
        self.settle();
    }
    fn text(&mut self,id:&str,value:&str) {
        self.click(id);
        self.frame(vec![egui::Event::Key {key:egui::Key::A,physical_key:None,pressed:true,repeat:false,
            modifiers:egui::Modifiers {command:true,mac_cmd:true,..Default::default()}},egui::Event::Text(value.into())]);
        self.frame(vec![egui::Event::Key {key:egui::Key::A,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
        self.settle();
    }
    fn choose(&mut self,id:&str,kind:DialogKind,path:&Path) {
        self.click(id);assert_eq!(self.app.begin_dialog(),Some(kind));assert_eq!(self.app.begin_dialog(),None);
        self.app.finish_dialog(Ok(Some(path.into())));self.settle();
        assert!(self.app.error.is_none(),"{id}: {:?}",self.app.error);
    }
    fn draft(&mut self,id:&str,name:&str,asset:&str,file:&str) {
        self.click("catalog-new");
        for (control,value) in [("catalog-entry-id",id),("catalog-tag",&format!("hair.choice.{id}")),("catalog-name",name),("catalog-asset-id",asset),("catalog-file",file)] {self.text(control,value);}
    }
    fn state(&self) -> String {
        let spec=self.app.project.as_ref().and_then(|p|p.catalog()).map(|s|toml::to_string(s).unwrap()).unwrap_or_default();
        let preview=self.app.preview.as_ref().map(|t|toml::to_string(&hair::decoration::Spec {
            assets:t.decorations.assets.iter().map(|a|a.spec.clone()).collect(),entries:t.decorations.entries.clone()
        }).unwrap()).unwrap_or_default();
        let mesh=self.app.catalog_inspection().unwrap().map(|m|hash(bytemuck::cast_slice::<_,u8>(&m.0)));
        format!("adapter={}\nrevision={} dirty={} selected={:?}\nproject_catalog_sha256={}\npreview_catalog_sha256={}\nselected_mesh_sha256={mesh:?}\nsaved={:?}\nexported={:?}\nerror={:?}\nnotice={}\ncontrols={:?}\nproject_catalog={spec}",self.adapter,self.app.revision,self.app.dirty,self.app.selected_catalog_entry(),hash(spec.as_bytes()),hash(preview.as_bytes()),self.app.saved_project,self.app.exported_pack,self.app.error,self.app.notice,self.app.controls)
    }
    fn lastgood(&self) -> (u64,bool,String,Option<String>,String) {
        (self.app.revision,self.app.dirty,toml::to_string(self.app.project.as_ref().unwrap().catalog().unwrap()).unwrap(),self.app.selected_catalog_entry().map(str::to_owned),hash(bytemuck::cast_slice::<_,u8>(&self.app.catalog_inspection().unwrap().unwrap().0)))
    }
    fn capture(&mut self,name:&str) -> Vec<u8> {
        self.settle();
        let pixels=self.gpu.read_pixels(&self.target,self.size).unwrap();
        let quit=self.app.controls["quit"];
        let title_rect=egui::Rect::from_min_max(egui::pos2(5.,quit.top()-3.),egui::pos2(350.,quit.bottom()+3.));
        let title=crop(&pixels,self.size,title_rect);
        let pane=crop(&pixels,self.size,self.app.controls["authoring-viewport"]);
        println!("7B1_UI_PIXELS {name}: title_glyph_pixels={} title_rect={title_rect:?} pane_bright_pixels={}",bright(&title),bright(&pane));
        // Preserve the actual frame even if a following pixel assertion fails.
        write_capture(&self.base,name,self.size,&pixels,&self.state());
        assert!(bright(&title)>200,"font atlas / title glyphs absent, not a complete UI frame");
        assert!(bright(&pane)>150,"authoring text and controls absent, not a complete UI frame");
        assert!(!self.app.controls.contains_key("catalog-remove-asset"));
        pixels
    }
}

#[test]
#[ignore="explicit real Metal full compositor, actual synthetic egui input/chooser callbacks; no native/human/package acceptance"]
fn actual_input_full_catalog_composited_metal_witness() {
    let base=std::env::temp_dir().join(format!("hair-7b1-composited-{}",std::process::id()));fs::create_dir(&base).unwrap();
    println!("7B1_OWNED_FIXTURE={}",base.display());
    let chair=base.join("owned-chair.glb");let desk=base.join("owned-desk.glb");
    fs::copy(root().join("assets/kenney/chairDesk.glb"),&chair).unwrap();fs::copy(root().join("assets/kenney/desk.glb"),&desk).unwrap();
    let chair_bytes=fs::read(&chair).unwrap();let desk_bytes=fs::read(&desk).unwrap();
    println!("7B1_SOURCE {} sha256={}\n7B1_SOURCE {} sha256={}",chair.display(),hash(&chair_bytes),desk.display(),hash(&desk_bytes));
    pollster::block_on(async {
        let instance=hair::render::instance();let adapter=hair::render::adapter(&instance,None).await.unwrap();
        assert_eq!(adapter.get_info().backend,wgpu::Backend::Metal,"explicit Metal witness must not pass on a different backend");
        // Reproduce the old harness ownership error on a separate disposable
        // context/painter. The initial font/white-pixel atlas delta was discarded.
        let stale_ctx=egui::Context::default();let mut stale_app=Workshop::new(vec![root().into()]);
        let out=stale_ctx.run_ui(raw([1280,960],0.,vec![]),|ui|stale_app.ui(ui,None));
        assert!(!out.textures_delta.set.is_empty(),"diagnosis requires the real initial atlas upload");
        out.drop_without_applying_deltas();
        let mut stale_gpu=WorkshopGpu::new(&adapter,wgpu::TextureFormat::Rgba8Unorm,[1280,960]).await.unwrap();
        let stale_target=stale_gpu.target([1280,960]);let view=stale_target.create_view(&Default::default());
        for i in 1..4 {stale_gpu.frame(&stale_ctx,&mut stale_app,raw([1280,960],i as f64/30.,vec![]),&view,[1280,960]).unwrap();}
        let lost=stale_gpu.read_pixels(&stale_target,[1280,960]).unwrap();
        let quit=stale_app.controls["quit"];
        let lost_title=bright(&crop(&lost,[1280,960],egui::Rect::from_min_max(egui::pos2(5.,quit.top()-3.),egui::pos2(350.,quit.bottom()+3.))));
        write_capture(&base,"diagnostic-discarded-atlas-not-acceptance",[1280,960],&lost,"diagnostic only: CPU consumed initial atlas delta before GPU attach; no preview/project");
        assert_eq!(lost_title,0,"discarded-atlas reproduction unexpectedly rendered title glyphs");
        println!("7B1_DIAGNOSIS discarded_initial_atlas_title_pixels={lost_title}; fresh-context GPU consumer follows native ownership; production compositor unchanged");
        drop(stale_gpu);

        let mut h=Harness::new(&adapter,base.clone()).await;
        h.click("new-material");h.click("catalog");
        h.draft("reading-chair","Reading chair","chair-art","chair.glb");
        h.choose("catalog-import",DialogKind::CatalogImport,&chair);
        let revision=h.app.revision;h.text("catalog-yaw","35");assert_eq!(h.app.revision,revision);
        h.click("catalog-apply");assert!(h.app.error.is_none());assert_eq!(h.app.revision,revision+1);
        h.draft("writing-desk","Writing desk","desk-art","desk.glb");h.choose("catalog-import",DialogKind::CatalogImport,&desk);
        let before_selection=(h.app.revision,h.app.dirty);
        h.click("catalog-row-0");assert_eq!(h.app.selected_catalog_entry(),Some("reading-chair"));
        let chair_frame=h.capture("normal1280-selected-chair");let preview_rect=h.app.controls["preview"];
        let chair_world=crop(&chair_frame,h.size,preview_rect);
        h.click("catalog-row-1");assert_eq!(h.app.selected_catalog_entry(),Some("writing-desk"));
        let desk_frame=h.capture("normal1280-selected-desk");
        assert_eq!(h.app.controls["preview"],preview_rect);
        let world_delta=changed(&chair_world,&crop(&desk_frame,h.size,preview_rect));
        println!("7B1_COUNTERFACTUAL world_changed_pixels={world_delta} rect={preview_rect:?}");
        assert!(world_delta>300,"different real selected meshes must alter a meaningful world region, not just HUD/text");
        assert_eq!((h.app.revision,h.app.dirty),before_selection);

        // Independent UI-region counterfactual: the actual category button hides
        // the catalog form. This is not counted as a world counterfactual.
        let ui_rect=h.app.controls["authoring-viewport"];
        h.click("floor");let hidden=h.capture("normal1280-counterfactual-catalog-hidden");
        let ui_delta=changed(&crop(&desk_frame,h.size,ui_rect),&crop(&hidden,h.size,ui_rect));
        println!("7B1_COUNTERFACTUAL catalog_UI_changed_pixels={ui_delta} rect={ui_rect:?}");
        assert!(ui_delta>3000,"whole catalog pane counterfactual must change controls/text, not one pixel");
        assert!(!h.app.controls.contains_key("catalog-entry-id"));h.click("catalog");
        h.click("catalog-row-0");h.reveal("catalog-name");h.capture("normal1280-edit-form");

        h.draft("reading-spare","Shared chair choice","chair-art","chair.glb");
        h.click("catalog-reuse");assert!(egui::Popup::is_any_open(&h.ctx));h.capture("normal1280-reuse-menu");
        h.click("catalog-reuse-0");h.click("catalog-apply");assert!(h.app.error.is_none(),"{:?}",h.app.error);
        assert_eq!(h.app.project.as_ref().unwrap().catalog().unwrap().assets.len(),2);
        assert_eq!(h.app.project.as_ref().unwrap().catalog().unwrap().entries.len(),3);
        h.click("catalog-row-0");let lastgood=h.lastgood();
        h.text("catalog-scale","nonsense");h.click("catalog-apply");
        assert!(h.app.error.as_deref().unwrap().contains("Scale"));assert_eq!(h.lastgood(),lastgood);
        h.reveal("catalog-scale");h.capture("normal1280-invalid-draft-lastgood");
        h.click("ack-error");h.click("catalog-row-0");

        h.click("catalog-replace");assert_eq!(h.app.begin_dialog(),Some(DialogKind::CatalogReplace));
        h.click("catalog-remove-entry");assert_eq!(h.lastgood(),lastgood,"pending chooser must own input");
        h.app.finish_dialog(Ok(None));h.settle();assert_eq!(h.lastgood(),lastgood);
        h.reveal("catalog-replace");h.capture("normal1280-cancelled-source-lastgood");
        h.click("catalog-replace");assert_eq!(h.app.begin_dialog(),Some(DialogKind::CatalogReplace));
        // Explicit adverse-input injection, not a user action or successful edit.
        h.app.revision+=1;h.app.finish_dialog(Ok(Some(desk.clone())));h.app.revision-=1;
        assert!(h.app.error.as_deref().unwrap().contains("context changed"));assert_eq!(h.lastgood(),lastgood);
        h.capture("normal1280-stale-callback-refusal");h.click("ack-error");

        h.choose("save-new",DialogKind::SaveNew,&base.join("editable-copy"));assert!(!h.app.dirty);
        h.capture("normal1280-saved-separate-paths");
        // Delete only our two disposable inputs, only after successful Save NEW.
        assert_eq!(fs::read(&chair).unwrap(),chair_bytes);assert_eq!(fs::read(&desk).unwrap(),desk_bytes);
        fs::remove_file(&chair).unwrap();fs::remove_file(&desk).unwrap();
        h.choose("open",DialogKind::Open,&base.join("editable-copy/project.toml"));
        assert_eq!(h.app.selected_catalog_entry(),None);assert!(!h.app.dirty);
        h.capture("normal1280-reopened-retained-assets");
        h.click("catalog-row-0");h.choose("export",DialogKind::Export,&base.join("appearance-export"));
        assert!(!h.app.dirty);assert_ne!(h.app.saved_project.as_ref().unwrap().parent(),h.app.exported_pack.as_ref().unwrap().parent());
        h.capture("normal1280-exported-separate-paths");
        let visitor=hair::theme::Theme::load(&base.join("appearance-export/pack.toml"),&mut Default::default()).unwrap();
        let theme=h.app.preview.as_ref().unwrap();
        assert_eq!(theme.decorations.entries,visitor.decorations.entries);
        assert_eq!(theme.host,visitor.host);assert!(visitor.host.decorations.is_empty());
        for (a,b) in theme.decorations.assets.iter().zip(&visitor.decorations.assets) {
            assert_eq!(a.spec,b.spec);
            assert_eq!(bytemuck::cast_slice::<_,u8>(&a.model.vertices),bytemuck::cast_slice::<_,u8>(&b.model.vertices));
            assert_eq!(a.model.textures.len(),b.model.textures.len());
            for (a,b) in a.model.textures.iter().zip(&b.model.textures) {assert_eq!((a.width,a.height,&a.rgba),(b.width,b.height,&b.rgba));}
        }
        assert_eq!(theme.decorations.assets.len(),visitor.decorations.assets.len());
        for (i,e) in visitor.decorations.entries.iter().enumerate() {
            h.click(&format!("catalog-row-{i}"));assert_eq!(h.app.selected_catalog_entry(),Some(e.id.as_str()));
            let d=hair::placement_store::Decoration::from_entry(e,[0.,0.],0.,[1.6;3]).unwrap();
            assert_eq!(bytemuck::cast_slice::<_,u8>(&h.app.catalog_inspection().unwrap().unwrap().0),bytemuck::cast_slice::<_,u8>(&visitor.decoration_preview(&d).unwrap()));
        }
        for (file,bytes) in [("chair.glb",&chair_bytes),("desk.glb",&desk_bytes)] {
            assert_eq!(fs::read(base.join("editable-copy").join(file)).unwrap(),*bytes);
            assert_eq!(fs::read(base.join("appearance-export").join(file)).unwrap(),*bytes);
        }
        assert!(!chair.exists()&&!desk.exists()&&!base.join(".hair-layout").exists());
        println!("7B1_RETAINED_AGREEMENT saved_reopened_exported_visitor=true full_mesh_material_texture_identity=true; originals bundled and owner layouts untouched");
        h.click("catalog-row-0");
        for (size,label) in [([900,700],"native-min900"),([640,480],"offscreen-stress640-not-native-min")] {
            h.resize(size);h.reveal("catalog-row-0");h.capture(&format!("{label}-choices"));
            h.reveal("catalog-name");h.capture(&format!("{label}-edit-form-scrolled"));
            h.reveal("catalog-apply");h.capture(&format!("{label}-actions-scrolled"));
            let old=h.lastgood();h.click("save-new");assert_eq!(h.app.begin_dialog(),Some(DialogKind::SaveNew));
            h.app.finish_dialog(Ok(None));h.settle();assert_eq!(h.lastgood(),old);
            h.capture(&format!("{label}-save-cancel-separate-paths"));
        }
        println!("7B1_SCOPE normal=1280x960 native_min=900x700 offscreen_stress=640x480; synthetic_pointer_text_and_chooser_callbacks=true OS_panels=false physical_window=false human_visual=false package_ready=false");
    });
}
