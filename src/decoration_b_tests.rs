//! Folder-owned catalog vertical: real input, checked acquisition, exited producer.
use super::*;
use hair::placement_store::Decoration;
use crate::slice6c_texture_fixture as input;

#[path = "decoration_b_resources_tests.rs"]
mod extracted_resources;

const CHILD: &str = "directory_map_app_tests::place_mode_app_tests::placement_editor_r2a::decoration_b::decoration_b_process_child";

fn process(root: &Path, phase: &str) {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--ignored", "--nocapture", "--test-threads=1"])
        .env("HAIR_DECORATION_B_ROOT", root).env("HAIR_DECORATION_B_PHASE", phase)
        .output().unwrap();
    std::fs::write(root.join(format!("{phase}.stdout.log")), &out.stdout).unwrap();
    std::fs::write(root.join(format!("{phase}.stderr.log")), &out.stderr).unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    println!("DECORATION_B phase={phase} raw_exit={:?}\n{stdout}{}", out.status.code(), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success() && stdout.contains(&format!("test {CHILD} ..."))
        && stdout.contains("test result: ok. 1 passed; 0 failed; 0 ignored;"));
}

#[test]
fn decoration_b_catalog_two_instances_owned_closure_separate_cold_process() {
    let f = fixture("decoration-b-retained");
    let root = f.0.clone(); std::mem::forget(f);
    println!("DECORATION_B_EVIDENCE={}", root.display());
    for name in ["room", "other-room", "pack"] { std::fs::create_dir(root.join(name)).unwrap(); }
    process(&root, "publish");
    let before = slice4c::records(&root.join("room"));
    let other = slice4c::records(&root.join("other-room"));
    // Only our authored disposable source. Producer has exited, no model cache survives.
    std::fs::remove_dir_all(root.join("pack")).unwrap();
    process(&root, "reopen");
    assert_eq!(slice4c::records(&root.join("room")), before);
    assert_eq!(slice4c::records(&root.join("other-room")), other);
}

fn select(a: &mut App) {
    if a.place_mode.is_none() { tap(a, KeyCode::KeyP); }
    tap(a, KeyCode::KeyB); tap(a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Editing", "{}", a.local_status);
    aim_floor(a, [-2., 0.]);
    assert!(a.decoration_position().unwrap().is_pinned(), "new catalog placement must default to exact folder-owned artwork, not Follow: {}", a.local_status);
    assert!(a.local_status.contains("folder-owned"), "{}", a.local_status);
    let controls = a.decoration_controls();
    assert!(!controls.iter().any(|(c, _, _)| matches!(c, place_mode_native::Control::FollowTheme | place_mode_native::Control::SetAnimation(_))));
    tap(a, KeyCode::KeyF);
    assert!(a.decoration_position().unwrap().is_pinned(), "F must not create new Follow placements");
}

fn place(a: &mut App, at: [f32; 2], percent: &str, collision: bool) -> Decoration {
    select(a);
    size_control(a, percent);
    tap(a, KeyCode::ArrowRight);
    if a.decoration_position().unwrap().collision != collision { tap(a, KeyCode::KeyH); }
    enter_positioning(a); aim_floor(a, at);
    let before = slice4c::records(&a.local.as_ref().unwrap().explorer.current().folder);
    tap(a, KeyCode::Enter); assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
    let d = a.decoration_position().unwrap();
    assert_eq!(slice4c::records(&a.local.as_ref().unwrap().explorer.current().folder), before, "review is write-free");
    tap(a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
    d
}

fn closures(room: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(room.join(".hair-layout")).unwrap()
        .map(|e| e.unwrap().path()).filter(|p| p.file_name().unwrap().to_str().unwrap().starts_with("ac-")).collect();
    paths.sort(); paths
}

fn evidence(a: &App, root: &Path, ds: &[Decoration]) {
    for (i, d) in ds.iter().enumerate() {
        std::fs::write(root.join(format!("expected-{i}.toml")), toml::to_string(d).unwrap()).unwrap();
        let installed = &a.themes[a.theme_index].host.decorations[&d.placement_id];
        let mesh = a.themes[a.theme_index].decoration_preview(installed).unwrap();
        assert!(mesh.len() > 36 && mesh.iter().any(|v| v.texture != 0));
        std::fs::write(root.join(format!("expected-{i}.mesh")), bytes(&mesh)).unwrap();
        let empty = hair::decoration::Catalog::default();
        let (_, asset) = installed.resolve(&empty).unwrap();
        assert_eq!(asset.model.textures[0].rgba, input::rgba());
        let owned = root.join(format!("room/.hair-layout/ac-{}/asset.glb", d.closure_identity().unwrap()));
        assert_eq!(std::fs::read(&owned).unwrap(), std::fs::read(root.join("pack/input.glb")).unwrap());
        std::fs::copy(owned, root.join(format!("expected-{i}.glb"))).unwrap();
    }
}

#[test]
#[ignore = "explicit exited-producer/cold-reopen child, invoked by ordinary vertical"]
fn decoration_b_process_child() {
    let root = PathBuf::from(std::env::var_os("HAIR_DECORATION_B_ROOT").unwrap());
    assert!(root.starts_with(std::env::temp_dir()));
    let mut a = app();
    let room = root.join("room");
    if std::env::var("HAIR_DECORATION_B_PHASE").unwrap() == "publish" {
        a.complete_folder(Ok(Some(room.clone()))).unwrap();
        let pack = root.join("pack");
        input::write_input(&pack.join("input.glb"));
        let manifest = "version=3\nname='B owned fixture'\nroles=[]\n[decorations]\n[[decorations.assets]]\nid='input'\nfile='input.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0.0,0.0,0.0]\n[[decorations.entries]]\nid='input'\ntag='hair.seating.chair'\nasset='input'\nname='B textured asymmetric art'\n";
        std::fs::write(pack.join("pack.toml"), manifest).unwrap();
        a.themes[0] = hair::theme::Theme::load_for_folder_scene(&pack.join("pack.toml"), &mut Default::default(), &a.themes[0].host).unwrap();
        let first = place(&mut a, [-2., 0.], "Size: 75%", true);
        let payload = slice4c::records(&closures(&room)[0]);
        let second = place(&mut a, [0., -1.], "Size: 50%", false);
        assert_ne!(first.placement_id, second.placement_id);
        assert_eq!(first.closure_identity(), second.closure_identity());
        assert_eq!(closures(&room).len(), 1);
        assert_eq!(slice4c::records(&closures(&room)[0]), payload, "second instance adds no asset payload or replacement inode");
        // A cancelled third reference must not delete the shared owned closure.
        let saved = slice4c::records(&room); select(&mut a); tap(&mut a, KeyCode::Escape);
        assert_eq!(slice4c::records(&room), saved);
        // Same tag and exact bytes but different proven fit/provenance is NOT the same asset identity.
        std::fs::write(pack.join("pack.toml"), manifest.replace("scale=1.0", "scale=0.9")).unwrap();
        a.themes[0] = hair::theme::Theme::load_for_folder_scene(&pack.join("pack.toml"), &mut Default::default(), &a.themes[0].host).unwrap();
        let third = place(&mut a, [2., 0.], "Size: 75%", true);
        assert_eq!(first.tag, third.tag); assert_ne!(first.closure_identity(), third.closure_identity());
        assert_eq!(closures(&room).len(), 2);
        evidence(&a, &root, &[first, second, third.clone()]);
        a.complete_folder(Ok(Some(root.join("other-room")))).unwrap();
        let other = place(&mut a, [-2., 0.], "Size: 75%", true);
        assert_eq!(other.closure_identity(), third.closure_identity());
        let local = closures(&root.join("other-room")); assert_eq!(local.len(), 1);
        let source = room.join(format!(".hair-layout/ac-{}/asset.glb", third.closure_identity().unwrap()));
        assert_ne!(std::fs::metadata(&source).unwrap().ino(), std::fs::metadata(local[0].join("asset.glb")).unwrap().ino(), "different folder owns an independent copy");
        a.menu = Some(hair::presentation::Menu::pause()); assert!(a.activate_menu(hair::presentation::Item::Quit));
        return;
    }
    assert!(!root.join("pack").exists());
    a.complete_folder(Ok(Some(room.clone()))).unwrap();
    let archive = a.root.join("packs/archive.toml");
    a.themes.push(hair::theme::Theme::load_for_folder_scene(&archive, &mut Default::default(), &a.themes[0].host).unwrap());
    a.menu = Some(hair::presentation::Menu::settings()); a.activate_menu(hair::presentation::Item::Theme); a.menu = None;
    assert_eq!(a.theme_index, 1, "{}", a.local_status);
    for refresh in [false, true] {
        if refresh { tap(&mut a, KeyCode::F5); }
        let theme = &a.themes[a.theme_index];
        assert_eq!(theme.host.decorations.len(), 3, "{}", a.local_status);
        let scene = theme.vertices_for_at(&a.world, 3.);
        for i in 0..3 {
            let expected: Decoration = toml::from_str(&std::fs::read_to_string(root.join(format!("expected-{i}.toml"))).unwrap()).unwrap();
            let d = &theme.host.decorations[&expected.placement_id]; assert_eq!(d, &expected);
            let preview = theme.decoration_preview(d).unwrap();
            assert_eq!(bytes(&preview), std::fs::read(root.join(format!("expected-{i}.mesh"))).unwrap());
            let empty = hair::decoration::Catalog::default();
            let (_, asset) = d.resolve(&empty).unwrap();
            assert_eq!(std::fs::read(room.join(format!(".hair-layout/ac-{}/asset.glb", d.closure_identity().unwrap()))).unwrap(), std::fs::read(root.join(format!("expected-{i}.glb"))).unwrap());
            assert_eq!(asset.model.textures[0].rgba, input::rgba());
            // The installed renderer scene consumes acquired retained geometry and the correct texture namespace.
            let matched = scene.windows(preview.len()).find(|w| w.iter().zip(&preview).all(|(x,y)| x.position == y.position && x.color == y.color && x.uv == y.uv)).unwrap();
            for v in matched { assert_eq!(theme.textures[v.texture as usize - 1].rgba, input::rgba()); }
            assert_eq!(theme.host.blocked(d.at), d.collision);
        }
        let live = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
        assert_eq!(live.incomplete_records, 0); assert!(live.warnings.is_empty());
        assert_eq!(closures(&room).len(), 2);
    }
}
