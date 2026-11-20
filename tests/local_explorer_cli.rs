use std::{fs, process::Command};
#[test]
fn chosen_folder_cli_lists_immediate_contents_without_mutating_them() {
    let root = std::env::temp_dir().join(format!("hair-folder-cli-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("Child folder")).unwrap();
    fs::write(root.join("Read me.md"), "Owner's original contents\n").unwrap();
    let before = fs::read(root.join("Read me.md")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_hair"))
        .args(["--folder", root.to_str().unwrap(), "--resource-report"])
        .env("HAIR_RESOURCE_ROOT", env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let error = String::from_utf8_lossy(&output.stderr).into_owned();
    let unchanged = fs::read(root.join("Read me.md")).unwrap() == before;
    fs::remove_dir_all(&root).unwrap();
    assert!(
        output.status.success(),
        "Folder exploration should be available; exit={:?}, stdout={text}, stderr={error}",
        output.status.code()
    );
    assert!(
        text.contains("LOCAL_FOLDER="),
        "report must identify actual local folder: {text}"
    );
    assert!(
        text.contains("Child folder") && text.contains("Read me.md"),
        "report must expose immediate folder/file entries: {text}"
    );
    assert!(unchanged, "read-only exploration mutated source bytes");
}
