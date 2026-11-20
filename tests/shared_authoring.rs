use hair::{authoring::Project, theme::Theme};
use std::{fs, path::Path};
#[test]
fn workshop_exports_only_appearance_and_reopens_new_project() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-shared-author-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    let mut p = Project::import(&root.join("assets/kenney/desk.glb")).unwrap();
    assert_eq!(
        p.at, [0.0; 3],
        "new imports use asset-local pivot, not world location"
    );
    let mut edit = p.edit();
    edit.at = [0.2, 0.1, -0.2];
    edit.yaw_degrees = 45.0;
    p.update(edit).unwrap();
    let saved = p.save_new(&base.join("project"), &[root.into()]).unwrap();
    let reopened = Project::open(&saved).unwrap();
    assert_eq!(reopened.edit(), p.edit());
    let out = reopened.export(&base.join("pack"), &[root.into()]).unwrap();
    let text = fs::read_to_string(&out).unwrap();
    let value: toml::Value = toml::from_str(&text).unwrap();
    assert_eq!(value["version"].as_integer(), Some(2));
    for k in [
        "room", "spawn", "document", "portal", "slots", "image_at", "grid",
    ] {
        assert!(value.get(k).is_none(), "exported layout key {k}")
    }
    assert_eq!(value["roles"][0]["pivot"].as_array().unwrap().len(), 3);
    let a = Theme::load(&out, &mut Default::default()).unwrap();
    let b = Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    assert_eq!(a.host, b.host);
    assert_eq!(a.document, b.document);
    assert_eq!(a.portal, b.portal);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn legacy_v1_project_is_refused_without_rewriting_original() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let project = root.join("milestone-workshop/package/starter/project.toml");
    let before = fs::read(&project).unwrap();
    let mode = fs::metadata(&project).unwrap().permissions();
    let error = Project::open(&project)
        .err()
        .expect("v1 world placement must not be silently reinterpreted");
    assert!(error.contains("Legacy v1 project preserved"), "{error}");
    assert!(error.contains("NOT asset-local pivot"));
    assert!(error.contains("NEW folders"));
    assert_eq!(fs::read(&project).unwrap(), before);
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(&project).unwrap().permissions().mode(),
        mode.mode()
    );
}
