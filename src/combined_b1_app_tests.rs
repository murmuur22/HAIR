//! Owned real GLB input regression: no accounting-field injection.
use super::*;
use sha2::Digest;
use crate::local_model_fallback_tests::fixture as glb_input;

fn pack(label: &str, vertices: usize) -> Fixture {
    let f = fixture(label);
    let mut bin = glb_input::binary();
    bin.truncate(96);
    for i in 0..vertices { bin.extend_from_slice(&([0_u16, 2, 1, 0, 1, 3, 1, 2, 3, 2, 0, 3][i % 12]).to_le_bytes()); }
    let doc = glb_input::document().replace("120", &(96 + vertices * 2).to_string())
        .replace("\"byteLength\":24", &format!("\"byteLength\":{}", vertices * 2))
        .replace("\"count\":12", &format!("\"count\":{vertices}"));
    std::fs::write(f.0.join("asset.glb"), glb_input::glb(&doc, &bin)).unwrap();
    assert_eq!(hair::assets::load(&f.0.join("asset.glb"), 10.).unwrap().vertices.len(), vertices);
    std::fs::write(f.0.join("pack.toml"), r#"version=3
name='B1 owned input'
roles=[]
[decorations]
[[decorations.assets]]
id='shape'
file='asset.glb'
max_extent=10.0
scale=1.0
yaw_degrees=0.0
pivot=[0.0,0.0,0.0]
[[decorations.entries]]
id='shape'
tag='hair.b1'
asset='shape'
name='B1 shape'
"#).unwrap();
    f
}
fn theme_set(a: &mut App, heavy: &Path, light: &Path, index: usize) {
    let host = a.themes[a.theme_index].host.clone();
    a.themes = [heavy, light].iter().map(|p| hair::theme::Theme::load_for_folder_scene(p, &mut Default::default(), &host).unwrap()).collect();
    a.theme_index = index;
}

#[test]
fn combined_b1_real_pin_commit_refresh_and_cold_reopen() {
    let light = pack("b1-light", 12);
    let below = pack("b1-below", 299_988);
    let heavy = pack("b1-heavy", 299_991);
    let (f, mut a) = open_fixture("b1-folder");
    theme_set(&mut a, &heavy.0.join("pack.toml"), &light.0.join("pack.toml"), 1);
    assert_eq!(a.theme_index, 1);
    let originals = records(&f.0);
    start(&mut a, [-2., 0.], false);
    click(&mut a, place_mode_native::Control::PinAsset);
    let proposed = a.decoration_position().unwrap();
    assert!(proposed.is_pinned());
    let mut candidate = layout(&a); candidate.decorations.push(proposed.clone());
    a.themes[1].validate_decoration_candidate_layout(&candidate).unwrap();
    hair::placement_scene::validate_decoration_candidate(&a.local.as_ref().unwrap().explorer, &candidate, a.pose.position, 0).unwrap();
    let mut host = a.themes[1].host.clone();
    host.decorations.insert(proposed.placement_id.clone(), proposed.clone());
    let below_theme = hair::theme::Theme::load_for_folder_scene(&below.0.join("pack.toml"), &mut Default::default(), &host).unwrap();
    below_theme.validate_decorations().unwrap();
    println!("B1 controls: light=12 below=299988 heavy=299991 pin=12; exact shared 300000 below-control PASS; active admission and geometry PASS");
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("frozen decoration review"), "{}", a.local_status);
    assert_eq!(records(&f.0), originals);
    tap(&mut a, KeyCode::Enter);
    let committed = layout(&a);
    assert_eq!(committed.decorations, vec![proposed.clone()], "must reach real commit");
    let published = records(&f.0);
    println!("B1 real App save: {}", a.local_status);
    let refresh = a.placement_refresh(0);
    let mut cold = app();
    theme_set(&mut cold, &heavy.0.join("pack.toml"), &light.0.join("pack.toml"), 1);
    let reopen = cold.complete_folder(Ok(Some(f.0.clone())));
    println!("B1 explicit refresh={refresh:?}; separate cold App reopen={reopen:?}");
    assert_eq!(records(&f.0), published);
    for (p, v) in originals { assert_eq!(published.get(&p), Some(&v)); }
    assert!(refresh.is_ok() && reopen.is_ok(), "B1: inactive combined budget must not strand committed active folder; refresh={refresh:?} reopen={reopen:?}");
    assert_eq!(a.themes[1].host.decorations.get(&proposed.placement_id), Some(&proposed));
    assert_eq!(cold.themes[1].host.decorations, a.themes[1].host.decorations);
    unavailable_switch_retains(&mut a, &f.0);
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "directory_map_app_tests::place_mode_app_tests::slice4c::combined_b1::combined_b1_cold_process", "--ignored", "--nocapture"])
        .env("HAIR_B1_FOLDER", &f.0).env("HAIR_B1_LIGHT", light.0.join("pack.toml"))
        .env("HAIR_B1_HEAVY", heavy.0.join("pack.toml")).output().unwrap();
    println!("B1 cold child: {}{}", String::from_utf8_lossy(&child.stdout), String::from_utf8_lossy(&child.stderr));
    assert!(child.status.success());
}

fn identity(root: &Path) -> Vec<(PathBuf, u64, u32, Vec<u8>)> {
    records(root).into_iter().map(|(p, (ino, bytes))| {
        let mode = std::fs::symlink_metadata(&p).unwrap().mode();
        (p, ino, mode, bytes)
    }).collect()
}
fn unavailable_switch_retains(a: &mut App, root: &Path) {
    assert_eq!(a.themes.len(), 2, "disabled appearance stays listed");
    assert!(a.themes[0].unavailable_reason().unwrap().contains("aggregate whole-pack budget"));
    assert!(a.themes[0].feedback(&a.world).contains("UNAVAILABLE"));
    let before = identity(root);
    let pose = format!("{:?}", a.pose);
    let host = a.themes[1].host.clone();
    let content = format!("{:?}", a.world.document());
    let mesh = bytemuck::cast_slice::<_, u8>(&a.themes[1].vertices_for(&a.world)).to_vec();
    a.menu = Some(hair::presentation::Menu::settings());
    a.activate_menu(hair::presentation::Item::Theme);
    assert_eq!(a.theme_index, 1);
    assert!(a.local_status.contains("Appearance unchanged") && a.local_status.contains("unavailable"));
    assert_eq!(format!("{:?}", a.pose), pose);
    assert_eq!(a.themes[1].host, host);
    assert_eq!(format!("{:?}", a.world.document()), content);
    assert_eq!(bytemuck::cast_slice::<_, u8>(&a.themes[1].vertices_for(&a.world)), mesh);
    assert_eq!(identity(root), before, "refused switch preserves bytes/inodes/modes and pin closure");
    println!("B1 refused-switch payload identity (Debug encoding paths/inodes/modes/bytes) before_sha256={:x} after_sha256={:x}",
        sha2::Sha256::digest(format!("{before:?}")), sha2::Sha256::digest(format!("{:?}", identity(root))));
    a.menu = None;
}

#[test]
#[ignore = "child-only cold process for owned B1 inputs"]
fn combined_b1_cold_process() {
    let folder = PathBuf::from(std::env::var_os("HAIR_B1_FOLDER").unwrap());
    assert!(folder.starts_with(std::env::temp_dir()));
    let light = PathBuf::from(std::env::var_os("HAIR_B1_LIGHT").unwrap());
    let heavy = PathBuf::from(std::env::var_os("HAIR_B1_HEAVY").unwrap());
    let before = identity(&folder);
    let mut a = app();
    theme_set(&mut a, &heavy, &light, 1);
    a.complete_folder(Ok(Some(folder.clone()))).unwrap();
    assert_eq!(layout(&a).decorations.len(), 1);
    a.themes[1].validate_decorations().unwrap();
    unavailable_switch_retains(&mut a, &folder);
    assert_eq!(identity(&folder), before);
    println!("B1 cold process activeIndex=1 valid active pin; disabled other theme; payload identity PASS");
}

#[test]
fn combined_b1_placed_geometry_missing_tag_and_active_refusal_contracts() {
    let heavy = pack("b1-composed", 299_991);
    let light = pack("b1-active-control", 12);
    let (f, mut a) = open_fixture("b1-composed-folder");
    theme_set(&mut a, &heavy.0.join("pack.toml"), &light.0.join("pack.toml"), 1);
    start(&mut a, [-2., 0.], false);
    let follow = a.decoration_position().unwrap();
    tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration saved"), "{}", a.local_status);
    assert_eq!(layout(&a).decorations, vec![follow.clone()]);
    let unavailable = &a.themes[0];
    assert!(unavailable.unavailable_reason().unwrap().contains("Invalid or oversized placement geometry"));
    assert!(unavailable.validate_decorations().is_err());
    assert!(unavailable.validate_layout_decorations(&layout(&a)).is_err());
    assert!(hair::theme::Theme::load_for_scene(&heavy.0.join("pack.toml"), &mut Default::default(), &a.themes[1].host).is_err(), "non-folder public loader remains strict");
    let before = identity(&f.0);
    // Owner R1: exact absent Follow tag is explicit procedural art, not substitution.
    let missing = fixture("b1-missing-tag");
    std::fs::write(missing.0.join("pack.toml"), "version=3\nname='Missing exact tag'\nroles=[]\n").unwrap();
    let missing_theme = hair::theme::Theme::load_for_folder_scene(&missing.0.join("pack.toml"), &mut Default::default(), &a.themes[1].host).unwrap();
    assert!(missing_theme.unavailable_reason().is_none());
    assert!(missing_theme.decorations.entries.is_empty());
    missing_theme.reskin_from(&a.themes[1], &a.pose).unwrap();
    assert!(missing_theme.decoration_preview(&follow).unwrap()[..36].iter().all(|v| v.color == [0.5; 3]));
    let mut cold = app();
    theme_set(&mut cold, &heavy.0.join("pack.toml"), &light.0.join("pack.toml"), 0);
    let old_host = cold.themes[0].host.clone();
    let old_folder = cold.local.as_ref().unwrap().explorer.current().folder.clone();
    let old_pose = format!("{:?}", cold.pose);
    let error = cold.complete_folder(Ok(Some(f.0.clone()))).unwrap_err();
    assert!(error.contains("unavailable") && error.contains("Invalid or oversized placement geometry"), "{error}");
    assert_eq!(cold.themes[0].host, old_host);
    assert_eq!(cold.local.as_ref().unwrap().explorer.current().folder, old_folder);
    assert_eq!(format!("{:?}", cold.pose), old_pose);
    assert_eq!(identity(&f.0), before);
    let mut malformed = a.themes[1].host.clone();
    malformed.decorations.get_mut(&follow.placement_id).unwrap().size[0] = 0.;
    let malformed_error = hair::theme::Theme::load_for_folder_scene(&light.0.join("pack.toml"), &mut Default::default(), &malformed).err().unwrap();
    assert!(malformed_error.contains("Invalid Follow-theme decoration record"));
    // Required pack decode failures are never downgraded to availability state.
    std::fs::write(light.0.join("asset.glb"), b"not a GLB").unwrap();
    assert!(hair::theme::Theme::load_for_folder_scene(&light.0.join("pack.toml"), &mut Default::default(), &a.themes[1].host).is_err());
    println!("B1 real placed-geometry refusal (32766 cap, NOT scene cap), exact missing tag, active-index-zero last-good retention, malformed record and required decode refusal PASS");
}

fn select_attempt(a: &mut App, d: &hair::placement_store::Decoration) {
    super::recovery::click_label(a, "Inspect unfinished placements");
    let id = format!("exact storage-local ID dc-{}", d.placement_id);
    for _ in 0..256 {
        if a.recovery_status().contains(&id) { break; }
        tap(a, KeyCode::PageDown);
    }
    assert!(a.recovery_status().contains(&id));
    super::recovery::click_label(a, "Select attempt");
}

#[test]
fn combined_b1_prepared_pin_recovery_real_late_inactive_resource_change_and_relocation() {
    use hair::placement_store::{Outcome, ResumeStage};
    for change_at in [ResumeStage::Review, ResumeStage::Final] {
        let light = pack("b1-resume-light", 12);
        let heavy = pack("b1-resume-heavy", 299_988);
        let crossing = pack("b1-resume-crossing", 299_991);
        let f = fixture("b1-fresh-no-layout");
        std::fs::write(f.0.join("A.txt"), b"owned original").unwrap();
        let original = identity(&f.0);
        let mut a = app();
        theme_set(&mut a, &heavy.0.join("pack.toml"), &light.0.join("pack.toml"), 1);
        a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
        assert!(!f.0.join(".hair-layout").exists());
        start(&mut a, [-2., 0.], false);
        click(&mut a, place_mode_native::Control::PinAsset);
        let d = a.decoration_position().unwrap();
        let e = &a.local.as_ref().unwrap().explorer;
        let theme = &a.themes[1];
        let source = hair::placement_store::select_pin_asset(theme, "shape").unwrap();
        let follow = hair::placement_store::Decoration::from_entry(&theme.decorations.entries[0], d.at, d.yaw, d.size).unwrap();
        // Synthetic writer refusal ONLY to prepare recoverable evidence; not a
        // claim that a late resource check failed. Both real geometry stages run.
        let mut calls = 0;
        let out = theme.publish_pin_checked(e, &source, &follow, |c| {
            hair::placement_scene::validate_decoration_candidate(e, c, a.pose.position, 0)?;
            calls += 1;
            if calls == 2 { Err("B1 synthetic preparation stop".into()) } else { Ok(()) }
        });
        assert_eq!(calls, 2);
        assert_eq!(out, Outcome::NotCommitted("B1 synthetic preparation stop".into()));
        let prepared = hair::placement_store::inspect_placements(e).unwrap();
        let proposed = prepared.attempts().iter().enumerate().find_map(|(i, _)| prepared.select_decoration_resume(i).ok().map(|s| s.proposal().clone())).unwrap();
        drop(prepared);
        tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::F5); tap(&mut a, KeyCode::KeyP);
        assert!(a.themes[0].unavailable_reason().is_none());
        select_attempt(&mut a, &proposed);
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("frozen recovery review"));
        let before = identity(&f.0);
        let trace = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
        let seen = trace.clone();
        let from = crossing.0.join("asset.glb"); let to = heavy.0.join("asset.glb");
        a.recovery_test_probe(move |stage| {
            seen.borrow_mut().push(stage);
            if stage == change_at { std::fs::copy(&from, &to).unwrap(); }
        });
        tap(&mut a, KeyCode::Enter);
        assert_eq!(*trace.borrow(), vec![ResumeStage::Review, ResumeStage::Final]);
        assert_eq!(a.recovery_test_outcome(), Some("Committed { synced: true }"));
        assert!(a.local_status.contains("refresh succeeded"), "{}", a.local_status);
        assert_eq!(layout(&a).decorations, vec![proposed.clone()]);
        let active = f.0.join(format!(".hair-layout/dc-{}/decoration.toml", proposed.placement_id));
        let pending = active.with_file_name("pending.toml");
        let after = identity(&f.0);
        let pending_before = before.iter().find(|v| v.0 == pending).unwrap();
        let active_after = after.iter().find(|v| v.0 == active).unwrap();
        assert_eq!((&pending_before.1, &pending_before.2, &pending_before.3), (&active_after.1, &active_after.2, &active_after.3));
        for v in &before { if v.0 != pending && v.0 != active { assert!(after.contains(v)); } }
        for v in &original { assert!(after.contains(v)); }
        unavailable_switch_retains(&mut a, &f.0);
        if a.place_mode.is_some() { tap(&mut a, KeyCode::KeyP); }
        tap(&mut a, KeyCode::KeyP);
        a.pose = Pose { position: [proposed.at[0], proposed.at[1] + 1.1], pitch: ((proposed.size[1] * 0.5 - 1.65) / 1.1).atan(), ..Default::default() };
        tap(&mut a, KeyCode::KeyX);
        assert!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().expected().is_some());
        begin_positioning(&mut a);
        aim_floor(&mut a, [-1.85, 0.]);
        tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("decoration reposition saved"), "{}", a.local_status);
        let relocated = layout(&a).decorations[0].clone();
        assert_eq!(relocated.closure_identity(), proposed.closure_identity());
        let relocated_files = identity(&f.0);
        for v in &after { if v.0 != active { assert!(relocated_files.contains(v)); } }
        a.placement_refresh(0).unwrap();
        let mut cold = app(); theme_set(&mut cold, &heavy.0.join("pack.toml"), &light.0.join("pack.toml"), 1);
        cold.complete_folder(Ok(Some(f.0.clone()))).unwrap();
        assert_eq!(layout(&cold).decorations, vec![relocated]);
        // Another folder must rebuild from its own closure set, never keep stale pins.
        let other = fixture("b1-next-folder");
        cold.complete_folder(Ok(Some(other.0.clone()))).unwrap();
        assert!(cold.themes.iter().all(|t| t.host.decorations.is_empty() && t.unavailable_reason().is_none()));
        println!("B1 recovery actual inactive GLB resource crossing at {change_at:?}; locked stages={:?}; commit/refresh/reopen/relocation/closure identity PASS", trace.borrow());
    }
}
