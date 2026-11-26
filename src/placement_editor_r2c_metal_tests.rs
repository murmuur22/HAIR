//! Explicit Metal document/LEGACY saved-decoration witness; parent serializes GPU.
use super::*;
fn capture_fixed(a: &mut App, root: &Path, label: &str) -> Vec<u8> {
    let dir=root.join(label);std::fs::create_dir(&dir).unwrap();
    let output=hair::resources::smoke_output(&a.root,&dir).unwrap();
    let renderer=a.renderer.as_ref().unwrap();renderer.capture(&output,"office.png",&a.pose,&a.world).unwrap();renderer.wait().unwrap();
    let mut r=png::Decoder::new(std::io::BufReader::new(std::fs::File::open(dir.join("office.png")).unwrap())).read_info().unwrap();
    let mut pixels=vec![0;r.output_buffer_size().unwrap()];let frame=r.next_frame(&mut pixels).unwrap();pixels.truncate(frame.buffer_size());
    assert_eq!([frame.width,frame.height],renderer.size);println!("R2C_CAPTURE {}",dir.display());pixels
}
fn difference(a: &[u8],b: &[u8],size:[u32;2],panel:bool)->usize {
    let [w,h]=size;let (xs,xe,ys,ye)=if panel{(w-308,w-12,88,208)}else{(w/2-180,w/2+180,116,h-104)};
    (ys..ye).flat_map(|y|(xs..xe).map(move|x|((y*w+x)*4)as usize)).filter(|i|a[*i..*i+3]!=b[*i..*i+3]).count()
}
fn remove_exact(scene:&mut Vec<hair::assets::Vertex>,mesh:&[hair::assets::Vertex]) {
    assert!(!mesh.is_empty());let i=scene.windows(mesh.len()).position(|v|bytes(v)==bytes(mesh)).expect("actual admitted mesh must be in scene");
    scene.drain(i..i+mesh.len());
}
#[test]
#[ignore = "explicit serial real Metal R2C animation witness; exclusive absent HAIR_R2C_CAPTURE_ROOT required"]
fn r2c_metal_modes_times_actual_textured_art_preview_placed_base_label_interaction() {
    let output=PathBuf::from(std::env::var_os("HAIR_R2C_CAPTURE_ROOT").expect("explicit absent owned output"));
    assert!(output.is_absolute());std::fs::create_dir(&output).unwrap();
    let instance=render::instance();let adapter=pollster::block_on(render::adapter(&instance,None)).unwrap();assert_eq!(adapter.get_info().backend,wgpu::Backend::Metal);
    for size in [[640,480],[960,640],[1280,800]] {for decoration in [false,true] {for mode in [Animation::None,Animation::Breathe,Animation::BreatheRotate] {
        let (f,mut a)=open_fixture("r2c-metal");let pack=fixture("r2c-metal-input");
        let legacy=if decoration {
            textured_theme(&mut a,&pack.0);
            // LEGACY saved animated pin, intentionally authored by the fixture;
            // never a current catalog animation operation or public API bypass.
            Some(legacy_saved_decoration(&mut a,true,mode))
        }else{
            write_local_input(&f.0.join("Art.glb"));a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
            None
        };
        let mut renderer=pollster::block_on(render::Renderer::new(&adapter,wgpu::TextureFormat::Rgba8UnormSrgb,size,&a.themes[0].vertices_for(&a.world))).unwrap();
        renderer.set_theme_appearance(&a.themes[0]).unwrap();a.renderer=Some(renderer);
        if let Some(d)=&legacy {edit_decoration(&mut a,d);}else{
            aim_file(&mut a,"Art.glb");tap(&mut a,KeyCode::KeyP);tap(&mut a,KeyCode::KeyX);
            let id=a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().anchor.clone();let deadline=Instant::now()+Duration::from_secs(5);
            loop {a.poll_local_image().unwrap();if a.editor_geometry(0.).unwrap().1.iter().any(|t|t.rgba==input::rgba()){break;}
                assert!(Instant::now()<deadline,"actual acquired GLB material missing for {id}; object_status={}; local_status={}; model_ids={:?}",a.object_previews.status,a.local_status,a.object_previews.models.keys().collect::<Vec<_>>());std::thread::sleep(Duration::from_millis(2));}
            println!("R2C_ACQUIRED id={id} object_status={} local_status={}",a.object_previews.status,a.local_status);
            tap(&mut a,KeyCode::KeyG);
        }
        if decoration {assert_eq!(a.editor_animation_mode(),mode);assert!(a.editor_animation(Animation::None).is_err());}
        else {select_mode(&mut a,mode);}
        assert_motion(&a,mode);a.place_mode.as_mut().unwrap().pickup=None;
        let dir=output.join(format!("{}x{}-{}-{mode:?}",size[0],size[1],if decoration{"legacy-decoration"}else{"file"}));std::fs::create_dir(&dir).unwrap();
        let original=slice4c::records(&f.0);let reference=a.editor_geometry(0.).unwrap().2;let mut panels=vec![];
        for time in [0,3,12] {
            a.started=Instant::now()-Duration::from_secs(time);a.update_hud();
            assert_eq!(a.editor_geometry(time as f64).unwrap().2,reference);
            let real=a.renderer.as_ref().unwrap().editor_preview.clone();assert!(!real.is_empty()&&real.iter().any(|v|v.texture!=0));
            assert!(a.editor_geometry(0.).unwrap().1.iter().any(|t|t.rgba==input::rgba()));
            let panel=capture_fixed(&mut a,&dir,&format!("preview-t{time}"));a.renderer.as_mut().unwrap().editor_preview.clear();
            let absent=capture_fixed(&mut a,&dir,&format!("preview-t{time}-art-removed"));
            let changed=difference(&panel,&absent,size,true);println!("R2C_PREVIEW size={size:?} decoration={decoration} mode={mode:?} time={time} art_pixels={changed}");assert!(changed>16,"actual preview art pixels {changed}");
            a.renderer.as_mut().unwrap().editor_preview=real;panels.push(panel);
        }
        let panel_motion=difference(&panels[0],&panels[1],size,true);
        if mode==Animation::None {assert_eq!(panel_motion,0);}else{assert!(panel_motion>1,"actual preview motion pixels {panel_motion}");}
        assert_eq!(slice4c::records(&f.0),original);
        enter_positioning(&mut a);aim_floor(&mut a,if decoration{[-1.8,0.]}else{[2.,0.]});tap(&mut a,KeyCode::Enter);tap(&mut a,KeyCode::Enter);
        assert!(a.local_status.contains("reposition saved"),"{}",a.local_status);
        let layout=hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
        if let Some(old)=&legacy {old.validate_relocation(&layout.decorations[0]).unwrap();assert_eq!(old.animation,layout.decorations[0].animation);}
        tap(&mut a,KeyCode::KeyP);
        let p=layout.positions.first().map(|(_,p)|p.clone());let d=layout.decorations.first().cloned();
        let file_id=if let Some(p)=&p {
            // Frame the saved file AND stationary base, not a close-up of only
            // the raised file. Stay inside the unchanged interaction reach.
            let b=p.bounds().unwrap();a.pose=Pose{position:[p.at[0],p.at[1]+1.65],yaw:0.,pitch:((b.at[1]+b.size[1]*0.5-1.65)/1.65).atan(),..Default::default()};
            println!("R2C_FILE_VIEW bounds={b:?} pose={:?}",a.pose);
            Some(a.world.folder_view.as_ref().unwrap().bindings.iter().find(|(_,b)|b.entry.name=="Art.glb").unwrap().0.clone())}
            else {let d=d.as_ref().unwrap();a.pose=Pose{position:[d.at[0],d.at[1]+1.65],yaw:0.,pitch:((d.size[1]*0.5-1.65)/1.65).atan(),..Default::default()};None};
        a.update_hud();let host=a.themes[0].host.clone();let target=hair::interaction::resolve(&host,&a.pose,&a.world);
        // Decorations use the production typed inspection resolver, not the
        // document/portal resolver used for files. Require the exact saved item.
        let decoration_target=hair::interaction::decoration_target(&host,&a.pose,&a.world).cloned();
        if let Some(d)=&d {assert_eq!(decoration_target.as_ref(),Some(d),"actual saved decoration must be inspectable");}
        else {assert!(target.is_some(),"actual saved file must be actionable");}
        println!("R2C_INTERACTION file={target:?} decoration={:?}",decoration_target.as_ref().map(|d|&d.placement_id));
        let saved=slice4c::records(&f.0);let mut worlds=vec![];
        for time in [0.,3.,12.] {
            let scene=a.themes[0].vertices_for_at(&a.world,time);let mut absent=scene.clone();
            let art=if let Some(id)=&file_id{a.themes[0].file_item_shape_at(id,"Art.glb".as_ref(),time)}else{
                // Scene material namespace is retained; omission supplies only the
                // exact actual decoration range, never a synthesized pixel image.
                let omitted=a.themes[0].vertices_for_at_omitting_decoration(&a.world,time,Some(&d.as_ref().unwrap().placement_id));
                let start=scene.iter().zip(&omitted).position(|(a,b)|bytes(&[*a])!=bytes(&[*b])).expect("decoration range differs");
                scene[start..start+scene.len()-omitted.len()].to_vec()
            };
            remove_exact(&mut absent,&art);a.renderer.as_mut().unwrap().set_scene(&scene);
            let world=capture_fixed(&mut a,&dir,&format!("placed-t{time}"));a.renderer.as_mut().unwrap().set_scene(&absent);
            let missing=capture_fixed(&mut a,&dir,&format!("placed-t{time}-art-removed"));
            let pixels=difference(&world,&missing,size,false);println!("R2C_PLACED size={size:?} decoration={decoration} mode={mode:?} time={time} art_pixels={pixels}");assert!(pixels>16,"actual placed art pixels {pixels}");
            assert_eq!(a.themes[0].host,host);assert_eq!(hair::interaction::resolve(&host,&a.pose,&a.world),target);
            assert_eq!(hair::interaction::decoration_target(&host,&a.pose,&a.world).cloned(),decoration_target);
            if let Some(p)=&p {
                let base=a.themes[0].collection_shape(file_id.as_ref().unwrap());assert!(!base.is_empty());
                let mut no_base=scene.clone();remove_exact(&mut no_base,&base);a.renderer.as_mut().unwrap().set_scene(&no_base);
                let no_base_pixels=capture_fixed(&mut a,&dir,&format!("placed-t{time}-base-removed"));let base_pixels=difference(&world,&no_base_pixels,size,false);
                println!("R2C_BASE size={size:?} mode={mode:?} time={time} pixels={base_pixels}");assert!(base_pixels>16,"actual base pixels {base_pixels}");
                let b=p.bounds().unwrap();let factor=p.properties.scale_factor();let offset=p.properties.file_offset(time);
                let at=[b.at[0],b.at[1]+b.size[1]+0.12*factor+offset,b.at[2]+b.size[2]*0.5+0.02*factor];
                let mut label=hair::text::object_label("Art.glb",[0.;3],25);for v in &mut label{v.position=std::array::from_fn(|i|at[i]+v.position[i]*factor);}
                let start=scene.windows(label.len()).position(|v|v.iter().zip(&label).all(|(x,y)|x.color==y.color&&x.uv==y.uv&&x.texture==y.texture&&x.position.iter().zip(y.position).all(|(x,y)|(x-y).abs()<0.000001))).expect("associated actual label");
                let mut no_label=scene.clone();no_label.drain(start..start+label.len());a.renderer.as_mut().unwrap().set_scene(&no_label);
                capture_fixed(&mut a,&dir,&format!("placed-t{time}-label-removed"));
            }
            worlds.push(world);
        }
        let world_motion=difference(&worlds[0],&worlds[1],size,false);
        if mode==Animation::None{assert_eq!(world_motion,0);}else{assert!(world_motion>1);}
        assert_eq!(slice4c::records(&f.0),saved);
        println!("R2C_METAL size={size:?} decoration={decoration} mode={mode:?} preview_motion_pixels={panel_motion} world_motion_pixels={world_motion}");
    }}}
}
