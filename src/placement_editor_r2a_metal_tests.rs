//! Explicit serialized offscreen Metal witness; not native/human acceptance.
use super::*;
#[test]
#[ignore = "explicit serial real Metal R2B scale witness; absent HAIR_R2B_CAPTURE_ROOT required"]
fn r2b_metal_actual_scale_50_100_150_stable_preview_and_world_geometry() {
    let output=PathBuf::from(std::env::var_os("HAIR_R2B_CAPTURE_ROOT").expect("explicit absent owned output"));
    assert!(output.is_absolute()); std::fs::create_dir(&output).unwrap();
    let instance=render::instance(); let adapter=pollster::block_on(render::adapter(&instance,None)).unwrap();
    assert_eq!(adapter.get_info().backend,wgpu::Backend::Metal);
    for size in [[640,480],[960,640],[1280,800]] {
        let (f,mut a)=open_fixture("r2b-metal");
        let mut renderer=pollster::block_on(render::Renderer::new(&adapter,wgpu::TextureFormat::Rgba8UnormSrgb,size,&a.themes[0].vertices_for(&a.world))).unwrap();
        renderer.set_theme_appearance(&a.themes[0]).unwrap(); a.renderer=Some(renderer);
        let dir=output.join(format!("{}x{}",size[0],size[1])); std::fs::create_dir(&dir).unwrap();
        aim_file(&mut a,"A.txt"); tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyX);
        let originals=slice4c::records(&f.0); let reference=a.editor_geometry(0.).unwrap().2;
        a.place_mode.as_mut().unwrap().pickup=None;
        let mut panels=vec![]; let mut worlds=vec![];
        for percent in [50,100,150] {
            click(&mut a,place_mode_native::Control::Scale(percent));
            assert_eq!(a.editor_geometry(0.).unwrap().2,reference);
            let panel=capture(&mut a,&dir,&format!("editing-{percent}"),true);
            let real=a.renderer.as_ref().unwrap().editor_preview.clone(); assert!(!real.is_empty());
            a.renderer.as_mut().unwrap().editor_preview.clear();
            let absent=capture(&mut a,&dir,&format!("editing-{percent}-geometry-removed"),false);
            let panel_pixels=roi_difference(&panel,&absent,size); assert!(panel_pixels>32);
            a.renderer.as_mut().unwrap().editor_preview=real;
            enter_positioning(&mut a); aim_floor(&mut a,[2.,0.]);
            let world=capture(&mut a,&dir,&format!("positioning-{percent}"),true);
            let overlay=a.renderer.as_ref().unwrap().placement_overlay.clone();
            let ghost=|c:[f32;3]|c==[0.1,0.9,0.5]||c==[1.,0.2,0.15];
            assert!(overlay.iter().any(|v|ghost(v.color)));
            a.renderer.as_mut().unwrap().placement_overlay=overlay.as_chunks::<3>().0.iter()
                .filter(|t|!t.iter().all(|v|ghost(v.color))).flatten().copied().collect();
            let no_ghost=capture(&mut a,&dir,&format!("positioning-{percent}-geometry-removed"),false);
            let scene_pixels=world_roi_difference(&world,&no_ghost,size); assert!(scene_pixels>32);
            println!("R2B_METAL size={size:?} percent={percent} preview_geometry_pixels={panel_pixels} scene_geometry_pixels={scene_pixels}");
            panels.push(panel); worlds.push(world); tap(&mut a,KeyCode::Escape);
            assert_eq!(slice4c::records(&f.0),originals); assert!(!f.0.join(".hair-layout").exists());
        }
        for i in 1..3 {
            let panel=roi_difference(&panels[i-1],&panels[i],size);
            let scene=world_roi_difference(&worlds[i-1],&worlds[i],size);
            assert!(panel>32 && scene>32,"scale must change actual geometry ROIs, not only percentage HUD");
            println!("R2B_METAL_STEP size={size:?} pair={i} panel={panel} scene={scene}");
        }
    }
}
fn capture(a: &mut App, root: &Path, label: &str, update: bool) -> Vec<u8> {
    if update { a.update_hud(); }
    a.place_scene_refresh();
    let dir = root.join(label); std::fs::create_dir(&dir).unwrap();
    let output = hair::resources::smoke_output(&a.root, &dir).unwrap();
    let renderer = a.renderer.as_ref().unwrap();
    renderer.capture(&output, "office.png", &a.pose, &a.world).unwrap();
    renderer.wait().unwrap();
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(dir.join("office.png")).unwrap())).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap(); pixels.truncate(info.buffer_size());
    assert_eq!([info.width, info.height], renderer.size);
    println!("R2A_METAL_CAPTURE {} size={:?}", dir.display(), renderer.size);
    pixels
}
fn roi_difference(a: &[u8], b: &[u8], size: [u32; 2]) -> usize {
    let [w, _] = size;
    (88..208).flat_map(|y| (w-308..w-12).map(move |x| ((y*w+x)*4) as usize))
        .filter(|i| a[*i..*i+3] != b[*i..*i+3]).count()
}
fn world_roi_difference(a: &[u8], b: &[u8], size: [u32;2]) -> usize {
    let [w,h] = size;
    (116..h-104).flat_map(|y|(w/2-180..w/2+180).map(move |x|((y*w+x)*4) as usize))
        .filter(|i|a[*i..*i+3]!=b[*i..*i+3]).count()
}
#[test]
#[ignore = "explicit serial Metal witness; exclusive absent HAIR_R2A_CAPTURE_ROOT"]
fn r2a_metal_live_editor_mesh_texture_rotation_back_cancel_and_both_inventory_routes() {
    let output = PathBuf::from(std::env::var_os("HAIR_R2A_CAPTURE_ROOT").expect("explicit absent owned capture output"));
    assert!(output.is_absolute()); std::fs::create_dir(&output).unwrap();
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    for size in [[1280, 800], [640, 480], [960, 640], [1024, 640]] {
        let (f, mut a) = open_fixture("r2a-metal");
        std::fs::write(f.0.join("Image.png"), png_fixture()).unwrap();
        a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
        let mut renderer = pollster::block_on(render::Renderer::new(&adapter, wgpu::TextureFormat::Rgba8UnormSrgb, size, &a.themes[0].vertices_for(&a.world))).unwrap();
        renderer.set_theme_appearance(&a.themes[0]).unwrap(); a.renderer = Some(renderer);
        let dir = output.join(format!("{}x{}", size[0], size[1])); std::fs::create_dir(&dir).unwrap();
        aim_file(&mut a, "Image.png");
        let original = slice4c::records(&f.0);
        tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
        let id = a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().anchor.clone();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !a.object_previews.cache.contains_key(&id) {
            a.poll_local_image().unwrap(); assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        // End only the cosmetic transition via elapsed time, never supply pixels.
        a.place_mode.as_mut().unwrap().pickup = None;
        let editing = capture(&mut a, &dir, "png-editing", true);
        let real = a.renderer.as_ref().unwrap().editor_preview.clone();
        assert!(!real.is_empty() && real.iter().any(|v| v.texture != 0));
        a.renderer.as_mut().unwrap().editor_preview.clear();
        let missing = capture(&mut a, &dir, "png-counterfactual-missing", false);
        assert!(roi_difference(&editing, &missing, size) > 128, "panel model must produce actual pixels independently of HUD");
        a.renderer.as_mut().unwrap().editor_preview = real;
        tap(&mut a, KeyCode::ArrowRight); tap(&mut a, KeyCode::ArrowRight);
        let rotated = capture(&mut a, &dir, "png-rotated", true);
        assert!(roi_difference(&editing, &rotated, size) > 64);
        tap(&mut a, KeyCode::KeyJ);
        a.started = Instant::now() - Duration::from_secs(3);
        let animated = capture(&mut a, &dir, "png-existing-breathe-animation", true);
        assert!(roi_difference(&rotated, &animated, size) > 4);
        tap(&mut a,KeyCode::KeyJ); // freeze animation for identical scene counterfactuals
        enter_positioning(&mut a); aim_floor(&mut a, [2., 0.]);
        let positioned = capture(&mut a, &dir, "positioning-mouse-free", true);
        let overlay = a.renderer.as_ref().unwrap().placement_overlay.clone();
        // Same HUD, chrome, world and artwork; remove only world-ghost triangles.
        let ghost = |c: [f32;3]|c==[0.1,0.9,0.5] || c==[1.,0.2,0.15];
        assert!(overlay.iter().any(|v|ghost(v.color)));
        a.renderer.as_mut().unwrap().placement_overlay = overlay.as_chunks::<3>().0.iter()
            .filter(|t|!t.iter().all(|v|ghost(v.color))).flatten().copied().collect();
        let no_ghost = capture(&mut a,&dir,"positioning-world-ghost-removed",false);
        let visible = world_roi_difference(&positioned,&no_ghost,size);
        assert!(visible>100,"world ghost must be visible in scene ROI: size={size:?}, pixels={visible}");
        a.renderer.as_mut().unwrap().placement_overlay = overlay.clone();
        if a.editor_compact() {
            a.renderer.as_mut().unwrap().placement_overlay.extend(hair::placement::toolbox::editor(size,&[],&[]));
            let occluded = capture(&mut a,&dir,"positioning-legacy-panels-occlusion-control",false);
            assert!(world_roi_difference(&positioned,&occluded,size)>1000);
            a.renderer.as_mut().unwrap().placement_overlay = overlay;
            let full = a.inventory_frame().full;
            point_click(&mut a,[full[0] as f32*0.5,full[1] as f32*0.5]); assert!(a.input.captured);
            let captured = capture(&mut a,&dir,"positioning-captured-world-visible",true);
            assert_eq!(world_roi_difference(&positioned,&captured,size),0,"capture changes hints, not world visibility");
            tap(&mut a,KeyCode::Tab);
            click(&mut a,place_mode_native::Control::EditorDetails);
            capture(&mut a,&dir,"positioning-expanded-properties",true);
            click(&mut a,place_mode_native::Control::Right);
            click(&mut a,place_mode_native::Control::EditorDetails);
            capture(&mut a,&dir,"positioning-recollapsed-draft-retained",true);
        }
        println!("R2A_LAYOUT_WORLD_ROI size={size:?} visible_ghost_pixels={visible}");
        tap(&mut a, KeyCode::Escape); assert_eq!(a.editor_phase(), "Editing");
        capture(&mut a, &dir, "back-editing", true);
        tap(&mut a, KeyCode::Escape); assert!(!a.editor_held());
        capture(&mut a, &dir, "cancel-world-restored", true);
        assert_eq!(slice4c::records(&f.0), original);
        // Authored catalog mesh is a genuine wrong-preview control for the PNG.
        tap(&mut a, KeyCode::KeyB); tap(&mut a, KeyCode::Enter);
        assert_eq!(a.editor_phase(), "Editing");
        let chair = capture(&mut a, &dir, "authored-chair-editing", true);
        assert!(a.renderer.as_ref().unwrap().editor_preview.len() > 36);
        assert!(roi_difference(&editing, &chair, size) > 128);
        tap(&mut a, KeyCode::ArrowRight);
        let chair_rotated = capture(&mut a, &dir, "authored-chair-rotated", true);
        assert!(roi_difference(&chair, &chair_rotated, size) > 32);
        tap(&mut a, KeyCode::KeyP); // exit before exploration collection
        aim_file(&mut a, "Image.png"); tap(&mut a, KeyCode::KeyX);
        assert_eq!(a.inventory.items().len(), 1);
        for toolbox in [false, true] {
            if toolbox { tap(&mut a, KeyCode::KeyP); click(&mut a, place_mode_native::Control::FileInventory); }
            else { tap(&mut a, KeyCode::KeyB); }
            assert!(a.inventory_open); tap(&mut a, KeyCode::KeyP);
            assert_eq!(a.editor_phase(), "Editing");
            capture(&mut a, &dir, if toolbox { "toolbox-inventory-editing" } else { "explore-inventory-p-editing" }, true);
            tap(&mut a, KeyCode::Escape); assert!(a.inventory_open);
            tap(&mut a, KeyCode::KeyB);
            if a.place_mode.is_some() { tap(&mut a, KeyCode::KeyP); }
        }
        assert_eq!(slice4c::records(&f.0), original);
        assert_eq!(a.inventory.items().len(), 1);
    }
}
