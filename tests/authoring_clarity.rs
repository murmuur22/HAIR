//! Existing writer safety exercised through both operations, independent of egui.
use hair::authoring::Project;
use std::{fs, path::Path};
#[test]
fn save_and_export_siblings_refuse_ancestors_aliases_and_existing_sentinels() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-core-clarity-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    let downloads = base.join("Downloads");
    fs::create_dir(&downloads).unwrap();
    let model = downloads.join("source.glb");
    fs::copy(root.join("assets/kenney/desk.glb"), &model).unwrap();
    let project = Project::import(&model).unwrap();
    let protected = base.join("App Resources");
    fs::create_dir(&protected).unwrap();
    let sentinel = protected.join("KEEP");
    fs::write(&sentinel, b"PROTECTED").unwrap();
    let alias = base.join("Resources Alias");
    std::os::unix::fs::symlink(&protected, &alias).unwrap();
    // Shell-looking filenames are literal filesystem data; never interpreted as commands.
    let existing = base.join("$(touch SHOULD_NOT_EXIST); literal output");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("KEEP"), b"EXISTING").unwrap();
    let linked = base.join("Existing Symlink");
    std::os::unix::fs::symlink(&existing, &linked).unwrap();
    for save in [true, false] {
        for target in [
            downloads.clone(),
            base.clone(),
            protected.join("NEW"),
            alias.join("NEW"),
            existing.clone(),
            linked.clone(),
        ] {
            let result = if save {
                project.save_new(&target, std::slice::from_ref(&protected))
            } else {
                project.export(&target, std::slice::from_ref(&protected))
            };
            let error = result.unwrap_err();
            if target == downloads {
                assert!(error.contains(&model.display().to_string()), "{error}");
            }
            if target == protected.join("NEW") || target == alias.join("NEW") {
                assert!(error.contains(&protected.display().to_string()), "{error}");
            }
        }
    }
    let saved = downloads.join("HAIR Project");
    let exported = downloads.join("HAIR Appearance Pack");
    project
        .save_new(&saved, std::slice::from_ref(&protected))
        .unwrap();
    let reopened = Project::open(&saved.join("project.toml")).unwrap();
    reopened
        .export(&exported, &[protected.clone(), saved.clone()])
        .unwrap();
    assert!(saved.join("project.toml").is_file() && exported.join("pack.toml").is_file());
    assert_eq!(
        fs::read(&model).unwrap(),
        fs::read(exported.join("model.glb")).unwrap()
    );
    assert_eq!(fs::read(sentinel).unwrap(), b"PROTECTED");
    assert_eq!(fs::read(existing.join("KEEP")).unwrap(), b"EXISTING");
    assert!(!base.join("SHOULD_NOT_EXIST").exists());
    assert!(!protected.join("NEW").exists());
    fs::remove_dir_all(base).unwrap();
}
