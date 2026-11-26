//! Explicit actual-input retained-pin Metal witness; no native/human acceptance.
use super::*;
use hair::placement_store::Decoration;
use crate::slice6c_texture_fixture as input;

// Disposable evidence only; do not follow a deliberately replaced source link.
fn metadata(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<i128>> {
    fn walk(p: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<i128>>) {
        let m = std::fs::symlink_metadata(p).unwrap();
        out.insert(p.to_path_buf(), vec![m.dev() as i128, m.ino() as i128, m.mode() as i128,
            m.nlink() as i128, m.uid() as i128, m.gid() as i128, m.len() as i128,
            m.mtime() as i128, m.mtime_nsec() as i128, m.ctime() as i128, m.ctime_nsec() as i128,
            std::os::macos::fs::MetadataExt::st_flags(&m) as i128]);
        if m.is_dir() { for e in std::fs::read_dir(p).unwrap() { walk(&e.unwrap().path(), out); } }
    }
    let mut out = Default::default(); walk(root, &mut out); out
}

fn frame(a: &mut App) {
    a.poll_web().unwrap(); a.poll_local_image().unwrap(); a.advance_motion(0.);
    let old = a.world.interaction.clone();
    hair::interaction::refresh_with_windows(&a.themes[a.theme_index].host, &a.pose,
        &mut a.world, a.web.as_ref().map_or(&[], |w| w.window_doors.as_slice()));
    if old != a.world.interaction || (a.focused && a.place_context()
        && a.themes[a.theme_index].has_file_animation(&a.world)) { a.place_scene_refresh(); }
    a.update_hud();
}
fn settle(a: &mut App) {
    let deadline = Instant::now() + std::time::Duration::from_secs(15);
    loop {
        frame(a);
        let (acquisitions, idle) = a.object_previews.test_model_fallback_progress();
        if idle {
            assert!(acquisitions > 0, "idle scheduler is not evidence of real acquisition");
            for name in ["A.txt", "Z.txt"] {
                let id = file_anchor(a, name);
                let acquired = a.object_previews.cache.get(id).expect("real held-text worker result");
                let expected = hair::text::literal_excerpt_texture(name, false);
                assert_eq!((acquired.width, acquired.height), (expected.width, expected.height));
                assert_eq!(acquired.rgba, expected.rgba, "owned text bytes must reach worker pixels");
                for theme in &a.themes {
                    assert!(theme.folder_preview_admitted(id, Some(acquired), None),
                        "every theme must admit the actual worker Arc before snapshots");
                    published_file_identity(a, theme, name).unwrap();
                }
            }
            break;
        }
        assert!(Instant::now() < deadline, "real preview worker completion deadline");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
fn capture(a: &mut App, out: &Path, name: &str, backend: &str) -> Vec<u8> {
    settle(a); // Drain real workers and all theme publications before snapshots.
    let scene = a.place_scene_at(0.); // Receipt only: positive never calls set_scene.
    let image = shot(a, out, name, backend, &scene);
    let data = &a.local.as_ref().unwrap().explorer.current().folder;
    let mut text = format!("status={}\nproposal={:#?}\ncontrols={:#?}\n", a.local_status,
        a.decoration_position().ok(), capture_controls(a));
    for (p,(ino,b)) in records(data) { text += &format!("{} inode={ino} bytes={} sha256={}\n", p.display(), b.len(), hash(&b)); }
    text += &format!("test_sha256={}\ninput_author_sha256={}\n", hash(&std::fs::read(a.root.join("src/slice6c_app_tests.rs")).unwrap()), hash(&std::fs::read(a.root.join("src/slice6c_texture_fixture.rs")).unwrap()));
    std::fs::write(out.join(format!("{name}-state.txt")), text).unwrap();
    image
}
fn copied_pack(a: &App, dir: &Path, kind: &str, textured: bool, adapted: bool) {
    std::fs::create_dir(dir).unwrap();
    for e in std::fs::read_dir(a.root.join("packs")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "glb") {
            std::fs::copy(&p, dir.join(p.file_name().unwrap())).unwrap();
        }
    }
    let mut text = std::fs::read_to_string(a.root.join(format!("packs/{kind}.toml"))).unwrap();
    if textured {
        input::write_input(&dir.join("input.glb"));
        // Deliberately replaces BOTH the decoration asset and host desk role.
        text = text.replace("desk.glb", "input.glb");
    }
    if adapted { text = text.replace("hair.storage.closed-bookcase", "hair.seating.chair"); }
    // Distinct non-pin material at one-based slot 1. Wrong local pin index must
    // sample purple instead of accidentally sampling the same retained pixels.
    let decoy = [180, 20, 200, 255].repeat(32*32);
    std::fs::write(dir.join("decoy.png"), input::png_bytes(&decoy)).unwrap();
    text += "\n[[surfaces]]\nsurface='floor'\nfile='decoy.png'\ntint=[0.35,0.35,0.35]\nmeters_per_tile=1.0\n";
    std::fs::write(dir.join("pack.toml"), text).unwrap();
}
fn add_theme(a: &mut App, path: &Path) {
    let host = a.themes[a.theme_index].host.clone();
    a.themes.push(hair::theme::Theme::load_for_folder_scene(path, &mut Default::default(), &host).unwrap());
}
fn catalog(a: &mut App, choice: usize) {
    if a.place_mode.is_none() { tap(a, KeyCode::KeyP); }
    tap(a, KeyCode::KeyB);
    for _ in 0..3 { tap(a, KeyCode::ArrowUp); }
    for _ in 0..choice { tap(a, KeyCode::ArrowDown); }
    tap(a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Editing", "{}", a.local_status);
    aim_floor(a, [-2., 0.]);
    retained(a);
}
fn retained(a: &mut App) {
    let d = a.decoration_position().unwrap();
    assert!(d.is_pinned(), "{}", a.local_status);
    assert!(!a.decoration_controls().iter().any(|(c, _, _)| *c == place_mode_native::Control::FollowTheme));
    let mesh = a.place_preview().unwrap();
    tap(a, KeyCode::KeyF);
    assert_eq!(a.decoration_position().unwrap(), d, "F cannot substitute theme art");
    tap(a, KeyCode::KeyV); // Already selected: not an acquisition boundary.
    assert_eq!(a.decoration_position().unwrap(), d);
    assert_eq!(mesh_bytes(&a.place_preview().unwrap()), mesh_bytes(&mesh));
}
fn positioning(a: &mut App, at: [f32; 2]) {
    begin_positioning(a);
    if a.editor_compact() { click(a, place_mode_native::Control::EditorDetails); }
    aim_floor(a, at);
}
fn begin(a: &mut App, at: [f32; 2]) {
    assert!(a.place_decoration_selection().is_some(), "retain selection from a real completed catalog save");
    assert!(!a.editor_held());
    tap(a, KeyCode::KeyN); // Actual N dispatch with no active decoration draft.
    assert_eq!(a.editor_phase(), "Editing", "{}", a.local_status);
    retained(a);
    positioning(a, at);
}
// Edit-first diagnostics are beside the artwork, not in the old right header.
fn header_label(a: &App, mesh: &[Vertex], row: usize, label: &str) {
    let size=a.inventory_frame().full;
    let x=(size[0] as f32-632.).max(0.)+8.; let y=80.+row as f32*10.;
    let template=hair::placement::toolbox::paint(size,&[],&[([x-6.,y-7.,x+304.,y+13.],label)],None);
    let expected=&template[12..]; // Omit only panel/control rectangles, not glyphs.
    assert!(!expected.is_empty());
    assert!(expected.iter().all(|v|v.position[0].abs()<=1.&&v.position[1].abs()<=1.));
    assert!(mesh.windows(expected.len()).any(|w|mesh_bytes(w)==mesh_bytes(expected)),"complete editor diagnostic missing: {label}");
}
fn disclosure(a: &mut App, frozen: bool) {
    let d = a.decoration_position().unwrap();
    let (entry,asset) = d.resolve(&a.themes[a.theme_index].decorations).unwrap();
    let entry_line = format!("Entry {} | asset {}", entry.id, asset.spec.id).chars().take(49).collect::<String>();
    let closure = format!("Closure: {}", &d.closure_identity().unwrap()[..16]);
    let ui = a.place_ui();
    header_label(a, &ui, 2, "New pin | folder-owned artwork");
    header_label(a, &ui, 8, &entry_line); header_label(a, &ui, 9, &closure);
    header_label(a, &ui, 6, if frozen { FROZEN_LABEL } else { PENDING_LABEL });
    if frozen {
        assert_eq!(a.toolbox_controls().iter().map(|(_,_,s)| *s).collect::<Vec<_>>(),
            ["Enter: save pin", "Esc: back / cancel", "P: exit Place Mode"]);
    } else {
        assert!(!a.decoration_controls().iter().any(|(c,_,_)| *c == place_mode_native::Control::FollowTheme));
        reveal_control(a, place_mode_native::Control::PinAsset);
        assert!(a.toolbox_controls().iter().any(|(_,_,s)| *s == "V: Pin selected (this folder)"));
    }
}
fn view(a: &mut App, d: &Decoration) {
    a.pose = Pose { position: [d.at[0],d.at[1]+1.65],
        pitch: ((d.size[1]*0.5-1.65)/1.65).atan(), ..Default::default() };
}
fn actual_mesh(a: &App, d: &Decoration) -> Vec<Vertex> {
    let local = a.themes[a.theme_index].decoration_preview(d).unwrap();
    let scene = a.place_scene_at(0.);
    let hits = scene.windows(local.len()).filter(|w| w.iter().zip(&local).all(|(a,b)|
        a.position == b.position && a.color == b.color && a.uv == b.uv)).collect::<Vec<_>>();
    assert_eq!(hits.len(), 1, "actual scene must contain exact retained authored geometry once");
    hits[0].to_vec()
}
fn region_changes(a: &[u8], b: &[u8], x0: usize, x1: usize) -> usize {
    (330..650).flat_map(|y| (x0..x1).map(move |x| (y*1280+x)*4))
        .filter(|&i| a[i..i+3] != b[i..i+3]).count()
}
fn gpu_controls(a: &mut App, out: &Path, phase: &str, backend: &str,
    d: &Decoration, positive: &[u8], textured: bool) {
    let scene = a.place_scene_at(0.);
    let mesh = actual_mesh(a,d);
    let mut cases = vec![("missing-model", vec![])];
    let mut wrong = hair::assets::load(&a.root.join("packs/desk.glb"),3.).unwrap().vertices;
    wrong = hair::placement::grounded(&wrong,d.at,d.yaw,d.size).unwrap();
    cases.push(("wrong-model",wrong));
    if textured {
        let key = mesh[0].texture;
        assert!(key > 1, "retained texture must have a real nonzero base offset");
        let t = &a.themes[a.theme_index].textures[key as usize-1];
        assert_eq!((t.width,t.height), (32,32)); assert_eq!(t.rgba,input::rgba());
        assert_ne!(a.themes[a.theme_index].textures[0].rgba,input::rgba());
        assert!(mesh.iter().all(|v| v.texture==key));
        for (name,key) in [("missing-texture",0),("wrong-material-offset",1)] {
            cases.push((name,mesh.iter().map(|v| Vertex { texture:key,..*v }).collect()));
        }
        let mut counts=[0usize;4];
        // Independent fixed world rectangle, excludes HUD/toolbox/sidebar.
        for y in 330..650 { for x in 480..800 {
            let i=(y*1280+x)*4; let p=&positive[i..i+3];
            for (n, matches) in [p[0]>150&&p[1]<70&&p[2]<70,
                p[1]>150&&p[0]<70&&p[2]<70,p[2]>150&&p[0]<70&&p[1]<70,
                p[0]>150&&p[1]>150&&p[2]<70].into_iter().enumerate() { if matches { counts[n]+=1; } }
        }}
        std::fs::write(out.join(format!("{phase}-landmarks.txt")),format!("fixed ROI x480..800 y330..650; source RG/BY landmarks GPU counts={counts:?}; material_key={}\n",mesh[0].texture)).unwrap();
        assert!(counts.iter().all(|&n| n>100), "all four source-color landmarks must materially reach GPU world ROI: {counts:?}");
    }
    for (kind,replacement) in cases {
        let altered=substitute(&scene,&mesh,&replacement);
        a.renderer.as_mut().unwrap().set_scene(&altered); // Negative controls only.
        let negative=shot(a,out,&format!("{phase}-{kind}-negative"),backend,&altered);
        let total=changed_world_pixels(positive,&negative);
        let left=region_changes(positive,&negative,480,640);
        let right=region_changes(positive,&negative,640,800);
        std::fs::write(out.join(format!("{phase}-{kind}-roi.txt")),format!("total={total}\nleft_fixed_world_roi={left}\nright_fixed_world_roi={right}\n")).unwrap();
        println!("SLICE6C_CONTROL {phase} {kind}: total={total} left={left} right={right}");
        assert!(total>100);
        if textured { assert!(left>100&&right>100,"independently chosen left/right world regions must both materially change"); }
    }
    a.place_scene_refresh(); a.update_hud();
}
fn exit_place(a: &mut App) { if a.place_mode.is_some() { tap(a,KeyCode::KeyP); } }
fn file_anchor<'a>(a: &'a App, name: &str) -> &'a str {
    a.world.folder_view.as_ref().unwrap().bindings.iter()
        .find(|(_,b)| b.entry.name==std::ffi::OsStr::new(name)).unwrap().0
}
fn published_file_identity(a: &App, theme: &hair::theme::Theme, name: &str) -> hair::Result<Vec<u8>> {
    // Scene material keys are allocation addresses, not appearance identity.
    // Preserve all vertex bits, shader high flag, dimensions and exact resolved
    // pixels while allowing the production pin upload to shift slot numbers.
    let vertices=theme.file_item_shape(file_anchor(a,name),std::ffi::OsStr::new(name));
    let expected=hair::text::literal_excerpt_texture(name,false);
    let mut result=vec![];
    let mut textured=0;
    for v in &vertices {
        let key=v.texture & 0x7fff_ffff;
        let normalized=Vertex { texture: if key==0 {0} else {(v.texture & 0x8000_0000)|1},..*v };
        result.extend_from_slice(bytemuck::bytes_of(&normalized));
        if key!=0 {
            textured+=1;
            let t=theme.textures.get(key as usize-1).ok_or("file texture key outside actual theme materials")?;
            if v.texture & 0x8000_0000 == 0 || (t.width,t.height)!=(expected.width,expected.height)
                || t.rgba!=expected.rgba {
                return Err(format!("{name}: unpublished/representative material, not exact held-text pixels/flags"));
            }
            result.extend(t.width.to_le_bytes()); result.extend(t.height.to_le_bytes());
            result.extend(Sha256::digest(&t.rgba));
        }
    }
    if textured==0 { return Err(format!("{name}: no actual nonzero file material key")); }
    Ok(result)
}
fn file_identity(a: &App, name: &str) -> Vec<u8> {
    published_file_identity(a,&a.themes[a.theme_index],name).unwrap()
}
fn unpublished_control(a: &mut App, out: &Path) {
    let attempts=a.object_previews.test_model_fallback_progress().0;
    let identities=[file_identity(a,"A.txt"),file_identity(a,"Z.txt")];
    let mut receipt=format!("real scheduler acquisition attempts={attempts}; drained=true\n");
    for theme in &mut a.themes { theme.set_folder_previews(&Default::default()); }
    // Withdraw actual Theme publication, leaving completed workers/cache enabled
    // and intact. Representative art may itself have nonzero material keys.
    for (index,theme) in a.themes.iter().enumerate() {
        for name in ["A.txt","Z.txt"] {
            let error=published_file_identity(a,theme,name).expect_err("unpublished representation must fail the same identity oracle");
            receipt+=&format!("theme={index} {name} expected rejection: {error}\n");
        }
    }
    for theme in &mut a.themes { theme.set_folder_previews(&a.object_previews.cache); }
    a.poll_local_image().unwrap(); a.place_scene_refresh(); settle(a);
    assert_eq!(a.object_previews.test_model_fallback_progress().0,attempts);
    unchanged_files(a,&identities);
    for (index,theme) in a.themes.iter().enumerate() {
        for name in ["A.txt","Z.txt"] {
            let vertices=theme.file_item_shape(file_anchor(a,name),std::ffi::OsStr::new(name));
            let keys=vertices.iter().map(|v|v.texture).filter(|k|k&0x7fff_ffff!=0).collect::<std::collections::BTreeSet<_>>();
            let t=&a.object_previews.cache[file_anchor(a,name)];
            receipt+=&format!("restored theme={index} {name} actual_keys={keys:?} dimensions={}x{} exact_rgba_sha256={} geometry_uv_flags_and_resolved_pixels_sha256={}\n",
                t.width,t.height,hash(&t.rgba),hash(&published_file_identity(a,theme,name).unwrap()));
        }
    }
    println!("SLICE6C_PUBLICATION {receipt}");
    std::fs::write(out.join("real-acquisition-publication-and-unpublished-control.txt"),receipt).unwrap();
}
fn unchanged_files(a: &App, files: &[Vec<u8>;2]) {
    assert_eq!(hash(&file_identity(a,"A.txt")),hash(&files[0]));
    assert_eq!(hash(&file_identity(a,"Z.txt")),hash(&files[1]));
}

fn pin_screen_bounds(a: &App, d: &Decoration) -> [f32;4] {
    use glam::{Mat4,Vec3};
    let frame=hair::sidebar::Frame::new(VIEW,a.ui_scale,
        a.renderer.as_ref().unwrap().sidebar_enabled && a.world.folder_view.is_some() && !a.world.tree_collapsed);
    let width=(VIEW[0]-frame.left) as f32;
    let pose=&a.pose;
    let direction=Vec3::new(pose.yaw.sin()*pose.pitch.cos(),pose.pitch.sin(),-pose.yaw.cos()*pose.pitch.cos());
    let matrix=Mat4::perspective_rh(65.0_f32.to_radians(),width/VIEW[1] as f32,0.05,50.)
        * Mat4::look_to_rh(Vec3::new(pose.position[0],pose.eye_height(),pose.position[1]),direction,Vec3::Y);
    let mut bounds=[f32::INFINITY,f32::INFINITY,f32::NEG_INFINITY,f32::NEG_INFINITY];
    for v in actual_mesh(a,d) {
        let clip=matrix*Vec3::from_array(v.position).extend(1.);
        assert!(clip.w>0.,"switch witness must keep whole pin in front of camera");
        let x=frame.left as f32+(clip.x/clip.w+1.)*width*0.5;
        let y=(1.-clip.y/clip.w)*VIEW[1] as f32*0.5;
        bounds[0]=bounds[0].min(x-8.); bounds[1]=bounds[1].min(y-8.);
        bounds[2]=bounds[2].max(x+8.); bounds[3]=bounds[3].max(y+8.);
    }
    bounds
}
fn switch_world_oracle(cold: &[u8], candidate: &[u8], pin: [f32;4]) -> hair::Result<usize> {
    assert_eq!(cold.len(),1280*960*4); assert_eq!(candidate.len(),cold.len());
    // Fixed interior world ROI independently excludes sidebar, toolbox and
    // top/bottom HUD; exclude entire projected retained pin plus an 8px margin.
    let mut changes=0;
    for y in 240..760 { for x in 320..960 {
        if (x as f32)>=pin[0] && (x as f32)<=pin[2]
            && (y as f32)>=pin[1] && (y as f32)<=pin[3] { continue; }
        let i=(y*1280+x)*4;
        if cold[i..i+3]!=candidate[i..i+3] { changes+=1; }
    }}
    if changes<=100 { return Err(format!("stale world: only {changes} changed pixels outside HUD and retained pin")); }
    Ok(changes)
}
fn switch_controls(a: &mut App, out: &Path, backend: &str, cold_scene: &[Vertex],
    cold: &[u8], switched: &[u8], pin: [f32;4]) {
    let changes=switch_world_oracle(cold,switched,pin).expect("actual menu switch must change non-pin world geometry/appearance");
    let mut receipt=format!("fixed world ROI x320..960 y240..760; excluded pin bounds={pin:?}\nactual menu switched changed pixels={changes}\n");
    // Actual pre-switch geometry, not a missing-model proxy. Leave destination
    // appearance installed: this specifically models omitting only set_scene.
    a.renderer.as_mut().unwrap().set_scene(cold_scene);
    let missing=shot(a,out,"switch-missing-set-scene-negative",backend,cold_scene);
    let error=switch_world_oracle(cold,&missing,pin).expect_err("missing set_scene must fail world-switch oracle despite new theme index/appearance/HUD");
    receipt+=&format!("missing set_scene expected rejection: {error}\n");
    a.renderer.as_mut().unwrap().set_theme_appearance(&a.themes[0]).unwrap();
    let stale=shot(a,out,"switch-stale-scene-and-appearance-negative",backend,cold_scene);
    let error=switch_world_oracle(cold,&stale,pin).expect_err("actual cold scene/appearance must fail world-switch oracle");
    receipt+=&format!("stale scene and appearance expected rejection: {error}\n");
    // Restore through production appearance/scene consumers, not an expected mesh.
    a.renderer.as_mut().unwrap().set_theme_appearance(&a.themes[a.theme_index]).unwrap();
    a.place_scene_refresh(); settle(a);
    let restored=capture(a,out,"switch-restored-through-production-consumers",backend);
    let restored_changes=switch_world_oracle(cold,&restored,pin).unwrap();
    assert_eq!(restored,switched,"production consumers must restore exact switched frame");
    receipt+=&format!("production restoration changed pixels={restored_changes}; exact positive frame restored\n");
    println!("SLICE6C_SWITCH {receipt}");
    std::fs::write(out.join("world-switch-and-stale-scene-controls.txt"),receipt).unwrap();
}

#[test]
fn slice6c_capture_controls_follow_real_cancel_exit_and_render_context() {
    let (f, mut a) = open_fixture("slice6c-capture-controls");
    let before = records(&f.0);
    assert!(a.place_mode.is_none());
    assert!(a.place_ui().is_empty());
    assert!(capture_controls(&a).is_empty());
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    assert!(a.place_mode.is_some());
    assert!(!a.place_ui().is_empty());
    assert!(capture_controls(&a).iter().any(|(c, _, _)| *c == place_mode_native::Control::Exit));
    tap(&mut a, KeyCode::KeyX);
    assert_eq!(a.editor_phase(), "Editing");
    assert!(capture_controls(&a).iter().any(|(_, _, s)| *s == "Enter: begin positioning"));
    begin_positioning(&mut a); aim_floor(&mut a, [2., 0.]);
    assert!(a.editor_compact());
    assert!(capture_controls(&a).iter().any(|(c, _, _)| *c == place_mode_native::Control::EditorDetails));
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
    assert!(capture_controls(&a).iter().any(|(_, _, s)| *s == "Enter: save file position"));
    tap(&mut a, KeyCode::Escape);
    assert_eq!(a.editor_phase(), "Editing");
    exit_place(&mut a); // Same real P dispatch as cancel-restored-write-free.
    assert!(a.place_mode.is_none());
    assert!(a.place_ui().is_empty(), "ordinary HUD has no Place Mode overlay after exit");
    a.update_hud();
    assert!(capture_controls(&a).is_empty(), "receipt must not query unavailable toolbox controls");
    tap(&mut a, KeyCode::KeyP);
    assert!(!capture_controls(&a).is_empty(), "fresh real P reopens available controls");
    a.focus_changed(false);
    assert!(!a.place_context());
    assert!(a.place_ui().is_empty());
    assert!(capture_controls(&a).is_empty());
    assert_eq!(records(&f.0), before);
}

#[test]
fn slice6c_retained_default_actual_n_write_free() {
    let (f, mut a) = open_fixture("slice6c-retained-default");
    catalog(&mut a, 0); positioning(&mut a, [0., -1.]);
    tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
    let before = records(&f.0);
    begin(&mut a, [-2., 0.]);
    assert!(a.decoration_position().unwrap().is_pinned());
    assert_eq!(records(&f.0), before);
}

#[test]
fn slice6c_actual_n_acquisition_refuses_tampered_source_without_edit_or_publication() {
    use std::os::unix::fs::PermissionsExt;
    for kind in ["asset-bytes", "manifest", "symlink", "permissions", "entry", "fit", "source"] {
        let (f, mut a) = open_fixture("slice6c-n-refusal");
        let pack = fixture("slice6c-n-owned-pack");
        let source = pack.0.join("source"); copied_pack(&a, &source, "office", false, false);
        a.themes[0] = hair::theme::Theme::load_for_folder_scene(&source.join("pack.toml"), &mut Default::default(), &a.themes[0].host).unwrap();
        // A genuine completed catalog save retains the selected entry for N.
        // No legacy helper/private selection fields stand in for production input.
        catalog(&mut a, 0); positioning(&mut a, [0., -1.]);
        tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
        assert!(!a.editor_held());
        let before = records(&f.0); let stamps = metadata(&f.0);
        let saved = layout(&a); let host = a.themes[0].host.clone();
        match kind {
            "asset-bytes" => std::fs::write(source.join("chairDesk.glb"), b"changed owned fixture").unwrap(),
            "manifest" => { let p=source.join("pack.toml"); let t=std::fs::read_to_string(&p).unwrap(); std::fs::write(p, format!("{t}\n# changed owned fixture\n")).unwrap(); }
            "symlink" => { let p=source.join("chairDesk.glb"); std::fs::rename(&p,source.join("held.glb")).unwrap(); std::os::unix::fs::symlink("held.glb",p).unwrap(); }
            "permissions" => std::fs::set_permissions(source.join("chairDesk.glb"),std::fs::Permissions::from_mode(0o666)).unwrap(),
            "entry" => a.themes[0].decorations.entries[0].asset="office-desk".into(),
            "fit" => a.themes[0].decorations.assets[0].spec.scale=1.1,
            "source" => a.themes[0].source=a.root.join("packs/office.toml"),
            _ => unreachable!(),
        }
        let source_bytes = records(&source); let source_stamps = metadata(&source);
        tap(&mut a, KeyCode::KeyN); // Actual retained selection, no private draft seam.
        let reason = match kind {
            "asset-bytes" => "Catalog bytes changed; reload and reselect",
            "manifest" => "manifest bytes changed",
            "symlink" => "Layout entry is not a regular file",
            "permissions" => "Foreign/shared pin source metadata",
            "entry" | "fit" => "Selected catalog identity/fit changed; reload and reselect",
            "source" => "Select a catalog decoration with B first",
            _ => unreachable!(),
        };
        println!("SLICE6C_N_REFUSAL kind={kind} status={}", a.local_status);
        assert!(a.local_status.contains(reason), "{kind}: {}", a.local_status);
        assert!(!a.editor_held()); assert!(a.place_preview().is_err());
        assert!(a.place_mode.as_ref().unwrap().decoration_edit.is_none());
        // Neither V nor stray confirmations can resurrect either intent.
        for key in [KeyCode::KeyV, KeyCode::Enter, KeyCode::Enter] { tap(&mut a,key); }
        assert!(!a.editor_held());
        assert_eq!(records(&f.0), before); assert_eq!(metadata(&f.0), stamps);
        assert_eq!(layout(&a), saved); assert_eq!(a.themes[0].host, host);
        assert_eq!(records(&source), source_bytes); assert_eq!(metadata(&source), source_stamps);
        assert!(a.inventory.items().is_empty() && a.placement_action.is_none());
    }
}

#[test]
fn slice6c_stale_source_reasons_preserve_earlier_guards_and_records() {
    for kind in ["asset-stamp", "manifest-bytes", "metadata-first"] {
        let (f, mut a) = open_fixture("slice6c-stale-reason");
        let pack = fixture("slice6c-stale-owned-pack");
        let source=pack.0.join("source"); copied_pack(&a,&source,"office",false,false);
        a.themes[0]=hair::theme::Theme::load_for_folder_scene(&source.join("pack.toml"),&mut Default::default(),&a.themes[0].host).unwrap();
        catalog(&mut a,0); positioning(&mut a,[-2.,0.]);
        tap(&mut a,KeyCode::Enter); assert_eq!(a.editor_phase(),"Review");
        match kind {
            "asset-stamp" => { let p=source.join("chairDesk.glb"); let b=std::fs::read(&p).unwrap(); std::fs::write(p,b).unwrap(); }
            "manifest-bytes" => { let p=source.join("pack.toml"); let t=std::fs::read_to_string(&p).unwrap(); std::fs::write(p,format!("{t}\n# stale manifest\n")).unwrap(); }
            "metadata-first" => { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(source.join("chairDesk.glb"),std::fs::Permissions::from_mode(0o666)).unwrap(); }
            _ => unreachable!(),
        }
        let before=records(&f.0); let stamps=metadata(&f.0);
        let source_bytes=records(&source); let source_stamps=metadata(&source);
        tap(&mut a,KeyCode::Enter);
        let reason=match kind { "asset-stamp"=>"asset stamp changed", "manifest-bytes"=>"manifest bytes changed", "metadata-first"=>"Foreign/shared pin source metadata", _=>unreachable!() };
        println!("SLICE6C_STALE_REASON kind={kind} status={}",a.local_status);
        assert!(a.local_status.contains(reason),"{kind}: {}",a.local_status);
        assert_eq!(records(&f.0),before); assert_eq!(metadata(&f.0),stamps);
        assert_eq!(records(&source),source_bytes); assert_eq!(metadata(&source),source_stamps);
        assert!(layout(&a).decorations.is_empty());
    }
}

#[test]
#[ignore = "explicit serialized Metal; fresh HAIR_SLICE6C_OUTPUT and HAIR_SLICE6C_FIXTURES required"]
fn slice6c_actual_pinned_chair_and_embedded_png_retention_metal() {
    if let Some(owned)=std::env::var_os("HAIR_SLICE6C_COLD_CHILD") {
        let owned=PathBuf::from(owned); assert!(owned.starts_with(std::env::temp_dir()));
        assert!(!owned.join("disposable-source").exists());
        let out=PathBuf::from(std::env::var_os("HAIR_SLICE6C_COLD_EXPECTED").unwrap());
        let mut a=app();
        a.themes[0]=hair::theme::Theme::load(&owned.join("cold-office/pack.toml"),&mut Default::default()).unwrap();
        a.complete_folder(Ok(Some(owned.join("folder")))).unwrap(); settle(&mut a);
        for refresh in [false,true] {
            if refresh { tap(&mut a,KeyCode::F5); settle(&mut a); }
            let saved=layout(&a);
            assert_eq!(format!("{saved:#?}"),std::fs::read_to_string(out.join("cold-expected-layout.txt")).unwrap());
            assert_eq!(a.themes[0].host.decorations.len(),saved.decorations.len());
            for d in &saved.decorations {
                assert_eq!(a.themes[0].host.decorations[&d.placement_id],*d);
                let mesh=a.themes[0].decoration_preview(d).unwrap();
                assert_eq!(mesh_bytes(&mesh),std::fs::read(out.join(format!("cold-expected-{}.mesh",d.placement_id))).unwrap());
                if d.is_pinned() { actual_mesh(&a,d); }
            }
        }
        println!("SLICE6C_COLD_PROCESS source_removed=true installed_exact=true refresh_read_only=true");
        return;
    }
    let output=PathBuf::from(std::env::var_os("HAIR_SLICE6C_OUTPUT").expect("exclusive capture root"));
    let fixtures=PathBuf::from(std::env::var_os("HAIR_SLICE6C_FIXTURES").expect("owned fixture root"));
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(output.parent(),root.parent());
    assert_eq!(fixtures.parent().unwrap().canonicalize().unwrap(),std::env::temp_dir().canonicalize().unwrap());
    std::fs::create_dir(&output).unwrap(); std::fs::create_dir(&fixtures).unwrap();
    std::fs::write(output.join("PROVENANCE.txt"),"Actual App key/pointer input, real preview workers, production offscreen Renderer/frame/scene/overlay consumers, protected capture writer. No native App::redraw/window/surface, physical input or human visual/comfort acceptance. Authentic copied chair and archive meshes. Synthetic semantic adaptation: archive closed-bookcase -> chair tag. Separate synthetic INPUT: 108-vertex asymmetric stepped three-box GLB with real embedded 32x32 PNG; top-left red, top-right green, bottom-left blue, bottom-right yellow; white base material factor. Actual pack loader/check selection/publication/cold storage/GPU composition. Purple floor texture occupies a distinct earlier material. Negative GPU-only model/material corruption is labelled, never an accepted positive. Generic capture guard unchanged; any sparse fallback remains diagnostic only. Small footer/layout Lows remain open. package_ready=false.\n").unwrap();
    for textured in [false,true] {
        let case=if textured { "texture" } else { "chair" };
        let out=output.join(case); std::fs::create_dir(&out).unwrap();
        let owned=fixtures.join(case); std::fs::create_dir(&owned).unwrap();
        let data=owned.join("folder"); std::fs::create_dir(&data).unwrap();
        for name in ["A.txt","Z.txt"] { std::fs::write(data.join(name),name.as_bytes()).unwrap(); }
        let source=owned.join("disposable-source"); let destination=owned.join("destination");
        let cold_pack=owned.join("cold-office");
        let mut a=app();
        copied_pack(&a,&source,"office",textured,false);
        copied_pack(&a,&destination,"archive",false,!textured);
        copied_pack(&a,&cold_pack,"office",false,false);
        let source_receipt=records(&source);
        std::fs::write(out.join("source-records.txt"),format!("{source_receipt:#?}")).unwrap();
        a.themes[0]=hair::theme::Theme::load(&source.join("pack.toml"),&mut Default::default()).unwrap();
        // Install fixture packs before folder open so the real App scheduler
        // publishes completed text previews into every theme, as at startup.
        add_theme(&mut a,&destination.join("pack.toml"));
        a.complete_folder(Ok(Some(data.clone()))).unwrap();
        let original=records(&data);
        let backend=attach(&mut a);
        aim_file(&mut a,"A.txt"); tap(&mut a,KeyCode::KeyP); tap(&mut a,KeyCode::KeyX);
        begin_positioning(&mut a); aim_floor(&mut a,[2.,0.]); tap(&mut a,KeyCode::Enter); tap(&mut a,KeyCode::Enter);
        assert!(a.local_status.contains("reposition saved"),"{}",a.local_status);
        settle(&mut a); unpublished_control(&mut a,&out);
        let files=[file_identity(&a,"A.txt"),file_identity(&a,"Z.txt")];
        let positions=layout(&a).positions;
        let baseline=records(&data);
        let choice=if textured {1} else {0};
        catalog(&mut a,choice); positioning(&mut a,[-2.,0.]);
        capture(&mut a,&out,"retained-default-catalog-choice",&backend);
        if !textured {
            let manifest=source.join("pack.toml"); let held=source.join("manifest-held.toml");
            // Cancel the acquired draft. A failed real catalog selection retains
            // only its selected entry, allowing the actual N acquisition route.
            tap(&mut a,KeyCode::Escape); tap(&mut a,KeyCode::Escape);
            std::fs::rename(&manifest,&held).unwrap();
            tap(&mut a,KeyCode::KeyB); tap(&mut a,KeyCode::Enter);
            assert!(!a.editor_held());
            tap(&mut a,KeyCode::KeyN);
            assert!(!a.editor_held());
            assert!(!a.local_status.contains("New pin"),"{}",a.local_status);
            capture(&mut a,&out,"actual-n-write-free-acquisition-refusal-exact-wording",&backend);
            assert_eq!(records(&data),baseline);
            std::fs::rename(&held,&manifest).unwrap();
            begin(&mut a,[-2.,0.]);
        }
        tap(&mut a,KeyCode::ArrowRight); tap(&mut a,KeyCode::KeyH);
        disclosure(&mut a,false);
        let choice_pixels=capture(&mut a,&out,"pin-choice-preview",&backend);
        let scene=a.place_scene_at(0.);
        preview_controls(&mut a,&out,"6c-choice",&backend,&scene,&choice_pixels,PENDING);
        assert_eq!(records(&data),baseline);
        if !textured {
            a.renderer.as_mut().unwrap().resize(SMALL_VIEW);
            disclosure(&mut a,false);
            capture(&mut a,&out,"choice-640x480-open-layout-low",&backend);
            a.renderer.as_mut().unwrap().resize(VIEW);
        }
        tap(&mut a,KeyCode::Escape); exit_place(&mut a);
        capture(&mut a,&out,"cancel-restored-write-free",&backend);
        assert_eq!(records(&data),baseline); unchanged_files(&a,&files);
        catalog(&mut a,choice); positioning(&mut a,[-2.,0.]);
        tap(&mut a,KeyCode::ArrowRight); tap(&mut a,KeyCode::KeyH);
        let proposed=a.decoration_position().unwrap();
        let preview=a.place_preview().unwrap();
        assert!(preview.len()>36);
        assert_eq!(mesh_bytes(&preview),mesh_bytes(&a.themes[0].decoration_preview(&proposed).unwrap()));
        a.key(KeyCode::Enter,true,false).unwrap();
        assert!(a.local_status.contains("frozen"),"{}",a.local_status);
        for repeat in [false,true] { a.key(KeyCode::Enter,true,repeat).unwrap(); }
        for key in [KeyCode::KeyF,KeyCode::KeyV,KeyCode::ArrowLeft,KeyCode::KeyH] { tap(&mut a,key); }
        assert_eq!(a.decoration_position().unwrap(),proposed);
        disclosure(&mut a,true);
        let frozen_pixels=capture(&mut a,&out,"frozen-first-enter-write-free",&backend);
        let scene=a.place_scene_at(0.);
        preview_controls(&mut a,&out,"6c-frozen",&backend,&scene,&frozen_pixels,PENDING);
        assert_eq!(records(&data),baseline);
        if !textured {
            a.renderer.as_mut().unwrap().resize(SMALL_VIEW); disclosure(&mut a,true);
            capture(&mut a,&out,"frozen-640x480-open-footer-low",&backend);
            a.renderer.as_mut().unwrap().resize(VIEW);
        }
        a.key(KeyCode::Enter,false,false).unwrap(); tap(&mut a,KeyCode::Enter);
        assert!(a.local_status.contains("decoration pinned"),"{}",a.local_status);
        settle(&mut a); exit_place(&mut a);
        assert_eq!(layout(&a).positions,positions);
        if !textured {
            let before=records(&data);
            catalog(&mut a,choice); positioning(&mut a,proposed.at);
            frame(&mut a); tap(&mut a,KeyCode::Enter);
            assert!(a.local_status.contains("overlap"),"{}",a.local_status);
            capture(&mut a,&out,"first-review-refusal-pin-header-exact-clipping",&backend);
            let reason=a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().preview_reason.clone().unwrap();
            let full=format!("Snapshot refused: {reason}");
            let shown=full.chars().take(49).collect::<String>();
            let omitted=full.chars().skip(147).collect::<String>();
            let ui=a.place_ui(); header_label(&a,&ui,10,&shown);
            for (i,chunk) in full.chars().collect::<Vec<_>>().chunks(49).take(3).enumerate() {
                let mut line=chunk.iter().collect::<String>();
                if i==2 && full.chars().count()>147 {line.pop();line.push('~');}
                header_label(&a,&ui,10+i,&line);
            }
            std::fs::write(out.join("observed-refusal-wrapping.txt"),format!("full={full}\nrow10={shown}\nomitted_after_three_rows={omitted}\nstatus={}\n",a.local_status)).unwrap();
            assert_eq!(records(&data),before);
            tap(&mut a,KeyCode::Escape); exit_place(&mut a);
        }
        let mut follow=None;
        if !textured {
            // HISTORICAL Follow comparison: author saved compatibility data
            // through the existing checked legacy writer, NEVER the new N route.
            let d=Decoration::from_entry(&a.themes[0].decorations.entries[0], [0.,-1.], 0., [0.8,0.85,0.6]).unwrap();
            let explorer=&a.local.as_ref().unwrap().explorer;
            assert!(matches!(hair::placement_store::place_decoration_checked(explorer,&d,|live| {
                a.themes[0].validate_decoration_layout(live,&d)?;
                hair::placement_scene::validate_decoration(explorer,live,&d,a.pose.position,0)
            }),hair::placement_store::Outcome::Committed {..}));
            assert!(!d.is_pinned());
            follow=Some(d); tap(&mut a,KeyCode::F5); settle(&mut a); exit_place(&mut a);
        }
        let saved=layout(&a); let pinned=records(&data); let host=a.themes[0].host.clone();
        let pinned_metadata=metadata(&data);
        let retained=proposed.resolve(&hair::decoration::Catalog::default()).unwrap().1.model.clone();
        assert_eq!(retained.textures.len(),if textured {1} else {0});
        if textured { assert_eq!(retained.textures[0].rgba,input::rgba()); }
        tap(&mut a,KeyCode::F5); settle(&mut a); view(&mut a,&proposed);
        invariant(&a,&saved,&host,&data,&pinned); unchanged_files(&a,&files);
        capture(&mut a,&out,"committed-refreshed-source-present",&backend);
        assert_eq!(records(&source),source_receipt);
        std::fs::write(out.join("cold-expected-layout.txt"),format!("{saved:#?}")).unwrap();
        for d in &saved.decorations {
            std::fs::write(out.join(format!("cold-expected-{}.mesh",d.placement_id)),mesh_bytes(&a.themes[0].decoration_preview(d).unwrap())).unwrap();
        }
        a.renderer.take().unwrap().wait().unwrap(); drop(a);
        // Only owned source copy; original bundled assets and destination retained.
        std::fs::remove_dir_all(&source).unwrap(); assert!(!source.exists());
        const THIS: &str="directory_map_app_tests::place_mode_app_tests::slice4c::metal_witness::slice6c_witness::slice6c_actual_pinned_chair_and_embedded_png_retention_metal";
        let child=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact",THIS,"--ignored","--nocapture","--test-threads=1"])
            .env("HAIR_SLICE6C_COLD_CHILD",&owned).env("HAIR_SLICE6C_COLD_EXPECTED",&out).output().unwrap();
        std::fs::write(out.join("cold-child.stdout.log"),&child.stdout).unwrap();
        std::fs::write(out.join("cold-child.stderr.log"),&child.stderr).unwrap();
        let stdout=String::from_utf8_lossy(&child.stdout);
        println!("SLICE6C_COLD_CHILD case={case} raw_exit={:?}\n{stdout}{}",child.status.code(),String::from_utf8_lossy(&child.stderr));
        assert!(child.status.success() && stdout.contains(&format!("test {THIS} ...")) && stdout.contains("test result: ok. 1 passed; 0 failed; 0 ignored;"));
        assert_eq!(records(&data),pinned); assert_eq!(metadata(&data),pinned_metadata);
        let mut a=app();
        a.themes[0]=hair::theme::Theme::load(&cold_pack.join("pack.toml"),&mut Default::default()).unwrap();
        add_theme(&mut a,&destination.join("pack.toml"));
        if !textured { add_theme(&mut a,&root.join("packs/archive.toml")); }
        a.complete_folder(Ok(Some(data.clone()))).unwrap(); settle(&mut a);
        assert_eq!(layout(&a),saved); assert_eq!(records(&data),pinned); unchanged_files(&a,&files);
        assert_eq!(metadata(&data),pinned_metadata);
        let backend=attach(&mut a); view(&mut a,&proposed);
        let positive=capture(&mut a,&out,"cold-reopened-source-removed",&backend);
        gpu_controls(&mut a,&out,"cold-reopened",&backend,&proposed,&positive,textured);
        let old_local=a.themes[0].decoration_preview(&proposed).unwrap();
        let old_follow=follow.as_ref().map(|d|a.themes[0].decoration_preview(d).unwrap());
        overview(&mut a);
        let cold_world=capture(&mut a,&out,"pre-switch-world-outside-pin",&backend);
        let cold_scene=a.place_scene_at(0.);
        let pin_bounds=pin_screen_bounds(&a,&proposed);
        let pose=format!("{:?}",a.pose);
        switch_by_input(&mut a); assert_eq!(a.theme_index,1,"{}",a.local_status);
        assert_eq!(format!("{:?}",a.pose),pose);
        assert_eq!(mesh_bytes(&a.themes[1].decoration_preview(&proposed).unwrap()),mesh_bytes(&old_local));
        assert_eq!(a.themes[1].host,host); assert_eq!(layout(&a),saved); assert_eq!(records(&data),pinned);
        unchanged_files(&a,&files);
        let switched_world=capture(&mut a,&out,"actual-menu-switched-world-outside-pin",&backend);
        switch_controls(&mut a,&out,&backend,&cold_scene,&cold_world,&switched_world,pin_bounds);
        unchanged_files(&a,&files);
        view(&mut a,&proposed);
        let switched=capture(&mut a,&out,"renderer-menu-switched-source-removed",&backend);
        gpu_controls(&mut a,&out,"menu-switched",&backend,&proposed,&switched,textured);
        if let Some(d)=&follow {
            let new=a.themes[1].decoration_preview(d).unwrap();
            assert_ne!(mesh_bytes(old_follow.as_ref().unwrap()),mesh_bytes(&new));
            view(&mut a,d); let pixels=capture(&mut a,&out,"follow-theme-changed-authentic-bookcase",&backend);
            gpu_controls(&mut a,&out,"follow-comparison",&backend,d,&pixels,false);
            switch_by_input(&mut a); assert_eq!(a.theme_index,2);
            assert!(d.artwork(&a.themes[2].decorations).unwrap().is_none());
            let gray = a.themes[2].decoration_preview(d).unwrap();
            assert!(gray[..36].iter().all(|v| v.color == [0.5; 3] && v.texture == 0));
            assert_eq!(mesh_bytes(&a.themes[2].decoration_preview(&proposed).unwrap()),mesh_bytes(&old_local), "retained pin never becomes a placeholder");
            invariant(&a,&saved,&host,&data,&pinned);
            capture(&mut a,&out,"follow-missing-tag-graybox-pin-still-authentic",&backend);
            switch_by_input(&mut a); assert_eq!(a.theme_index,0);
            switch_by_input(&mut a); assert_eq!(a.theme_index,1);
            assert_eq!(mesh_bytes(&a.themes[1].decoration_preview(d).unwrap()),mesh_bytes(&new));
            capture(&mut a,&out,"follow-return-authentic-art-same-pose",&backend);
        }
        assert_eq!(metadata(&data),pinned_metadata,"theme roundtrip and historical fallback never migrate records");
        // Actual retained-pin X selection/relocation after cold reopen and source deletion.
        if a.place_mode.is_none() { tap(&mut a,KeyCode::KeyP); }
        a.pose=Pose { position:[proposed.at[0],proposed.at[1]+1.1],
            pitch:((proposed.size[1]*0.5-1.65)/1.1).atan(),..Default::default() };
        frame(&mut a);
        assert_eq!(hair::interaction::decoration_target(&a.themes[1].host,&a.pose,&a.world),Some(&proposed));
        tap(&mut a,KeyCode::KeyX);
        assert!(a.local_status.contains("Existing pinned"),"{}",a.local_status);
        positioning(&mut a,[-1.85,0.]); tap(&mut a,KeyCode::ArrowRight); tap(&mut a,KeyCode::KeyH);
        let moved=a.decoration_position().unwrap(); proposed.validate_relocation(&moved).unwrap();
        assert_ne!(moved,proposed,"actual X relocation must change the selected placement");
        capture(&mut a,&out,"source-free-x-relocation-preview",&backend);
        tap(&mut a,KeyCode::Enter); capture(&mut a,&out,"source-free-x-frozen",&backend);
        assert_eq!(records(&data),pinned);
        tap(&mut a,KeyCode::Enter);
        assert!(a.local_status.contains("decoration reposition saved"),"{}",a.local_status);
        settle(&mut a); exit_place(&mut a); view(&mut a,&moved);
        capture(&mut a,&out,"source-free-x-relocation-saved",&backend);
        let selected=data.join(format!(".hair-layout/dc-{}/decoration.toml",proposed.placement_id));
        assert_ne!(records(&data).get(&selected),pinned.get(&selected),
            "selected relocation record must change, not merely preserve other files");
        let mut expected=pinned.clone(); expected.remove(&selected);
        let mut actual=records(&data); actual.remove(&selected); assert_eq!(actual,expected);
        let mut expected_metadata=pinned_metadata.clone(); expected_metadata.remove(&selected); expected_metadata.remove(selected.parent().unwrap());
        let mut actual_metadata=metadata(&data); actual_metadata.remove(&selected); actual_metadata.remove(selected.parent().unwrap());
        assert_eq!(actual_metadata,expected_metadata,"only the exact selected record and its publication directory may change metadata");
        assert_eq!(layout(&a).positions,positions); unchanged_files(&a,&files);
        for (p,v) in &original { assert_eq!(records(&data).get(p),Some(v)); }
        assert_eq!(a.themes[1].host.blocked(moved.at),moved.collision);
        assert_eq!(hair::player::blocked_at(&a.themes[1].host,moved.at,0.),moved.collision);
        a.renderer.take().unwrap().wait().unwrap();
    }
    std::fs::write(output.join("WITNESS-PASS.txt"),"Actual ignored slice6c witness assertions passed on offscreen Metal. Parent full suite, frozen review and actual PNG visual inspection pending. No native or human acceptance. package_ready=false.\n").unwrap();
}
