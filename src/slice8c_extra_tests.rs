use super::*;
use hair::placement_store::{self as store, ResumeStage};

#[test]
fn slice8c_complete_appearance_budget_refuses_before_review_and_at_checked_action() {
    for late in [false, true] {
        let (f, mut a, d, _) = prepare("pin");
        controls::inspect_select(&mut a, &d);
        if late { tap(&mut a, KeyCode::Enter); }
        // Explicit synthetic loaded-theme budget stress, not claimed external
        // geometry. Both paths use Theme's real complete-candidate consumer.
        let v = a.themes[0].vertices[0];
        a.themes[0].vertices.resize(300_001, v);
        let before = records(&f.0);
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("300000"), "{}", a.local_status);
        assert!(a.local_status.contains("New inspection required"));
        assert_eq!(records(&f.0), before);
        println!("SLICE8C synthetic budget late={late} actual-action-outcome={:?} status={}", a.recovery_test_outcome(), a.local_status);
    }
}
#[test]
fn slice8c_real_commit_then_actual_refresh_failure_is_not_reported_as_rollback() {
    let (f, mut a, d, _) = prepare("relocation");
    let pack = fixture("slice8c-owned-refresh-source");
    for entry in std::fs::read_dir(a.root.join("packs")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "glb") {
            std::fs::copy(&path, pack.0.join(path.file_name().unwrap())).unwrap();
        }
    }
    let manifest = pack.0.join("office.toml");
    std::fs::copy(a.root.join("packs/office.toml"), &manifest).unwrap();
    a.themes[0] = hair::theme::Theme::load_for_folder_scene(&manifest, &mut Default::default(), &a.themes[0].host).unwrap();
    controls::inspect_select(&mut a, &d); tap(&mut a, KeyCode::Enter);
    a.recovery_test_probe(move |stage| {
        if stage == ResumeStage::Final { std::fs::remove_file(&manifest).unwrap(); }
    });
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.recovery_test_outcome(), Some("Committed { synced: true }"));
    assert!(a.local_status.contains("committed") && a.local_status.contains("refresh failed") && a.local_status.contains("NOT rolled back"), "{}", a.local_status);
    assert_eq!(layout(&a).decorations, vec![d.clone()]);
    let status = a.local_status.clone(); tap(&mut a, KeyCode::Enter); assert_eq!(a.local_status, status);
    tap(&mut a, KeyCode::KeyI); assert!(a.world.document().unwrap().contains("refresh failed"));
    let mut reopened = app(); reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(reopened.themes[0].host.decorations.get(&d.placement_id), Some(&d));
    println!("SLICE8C genuine source deletion reached=Final actual-outcome=Committed(synced) actual-refresh=failed cold-source-restoration=not-required reopen-with-original-theme=PASS");
}
#[test]
#[ignore = "child-only interrupted actual public file writer; no recovery intent inferred"]
fn slice8c_file_writer_child() {
    let root = PathBuf::from(std::env::var_os("HAIR_SLICE8C_FILE_ROOT").unwrap());
    assert!(root.starts_with(std::env::temp_dir()));
    let e = hair::local_explorer::Explorer::new(&root).unwrap();
    let mut calls = 0;
    let p = store::Position { at: [2., 0.], yaw: 0., size: [0.4; 3], allow_overlap: false, surface: None, properties: Default::default() };
    let out = store::publish_checked(&e, std::ffi::OsStr::new("Unproven.txt"), b"sole staged bytes; Copy or Move intent unproven", &p, &std::sync::atomic::AtomicBool::new(false), |_| {
        calls += 1;
        if calls == 2 { std::process::exit(73); }
        Ok(())
    });
    panic!("file writer did not reach prepared boundary: {calls} {out:?}");
}
#[test]
fn slice8c_actual_interrupted_file_payload_is_preserved_without_disposition() {
    let (f, mut a, d, _) = prepare("follow");
    controls::inspect_select(&mut a, &d); tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    click_label(&mut a, "Close inspection");
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "directory_map_app_tests::place_mode_app_tests::slice4c::recovery::extra::slice8c_file_writer_child", "--ignored", "--nocapture"])
        .env("HAIR_SLICE8C_FILE_ROOT", &f.0).output().unwrap();
    assert_eq!(child.status.code(), Some(73), "{}{}", String::from_utf8_lossy(&child.stdout), String::from_utf8_lossy(&child.stderr));
    let before = records(&f.0); let xattrs = controls::attrs(&f.0);
    assert!(before.values().any(|(_, b)| b == b"sole staged bytes; Copy or Move intent unproven"));
    click_label(&mut a, "Inspect unfinished placements");
    let detail = a.recovery_status();
    assert!(detail.contains("FileTransaction") && detail.contains("unknown / unproven"), "{detail}");
    assert!(!a.toolbox_controls().iter().any(|(_,_,s)| s.contains("Select attempt") || s.contains("Confirm resume")));
    for _ in 0..3 { tap(&mut a, KeyCode::Enter); }
    click_label(&mut a, "Close inspection");
    assert_eq!(records(&f.0), before); assert_eq!(controls::attrs(&f.0), xattrs);
    println!("SLICE8C actual-file-writer exit=73 retained-payload=exact intent=unproven controls=none");
}
