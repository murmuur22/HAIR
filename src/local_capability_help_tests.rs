use super::*;

#[test]
fn generic_file_label_advertises_actual_raw_content_access() {
    let entry = hair::local_explorer::Entry {
        name: "bytes.bin".into(),
        path: "/Fixture/bytes.bin".into(),
        kind: hair::local_explorer::Kind::File,
    };
    let metadata = hair::folder_scene::metadata(&entry);
    assert!(metadata.contains("stored bytes"));
    assert!(!metadata.contains("metadata only"));
}

#[test]
fn directory_information_exposes_shift_running_control() {
    let path = std::env::temp_dir().join(format!("hair-run-help-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    app.complete_folder(Ok(Some(path.canonicalize().unwrap())))
        .unwrap();
    app.keyboard_input(Some(KeyCode::KeyI), true, false, None)
        .unwrap();
    app.keyboard_input(Some(KeyCode::KeyI), false, false, None)
        .unwrap();
    assert!(app.world.reading);
    let document = app.world.document().unwrap().to_string();
    drop(app);
    std::fs::remove_dir(path).unwrap();
    assert!(
        document.contains("Shift run"),
        "directory information should teach the running control: {document}"
    );
}
