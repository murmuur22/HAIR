#[test]
#[ignore = "private original and finite GPU; local only, not distribution"]
fn actual_private_export_loads_in_paired_client_with_document_interaction() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("private-inputs/planet.glb");
    let before = fs::read(&source).unwrap();
    let base = std::env::temp_dir().join(format!("hair-private-export-e2e-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    let project = Project::import(&source).unwrap();
    let pack = project
        .export(&base.join("pack"), &[root.to_path_buf()])
        .unwrap();
    let loaded = hair::theme::Theme::load(&pack, &mut Default::default()).unwrap();
    assert_eq!(loaded.textures[0].rgba, project.model.textures[0].rgba);
    assert_eq!(fs::read(base.join("pack/model.glb")).unwrap(), before);
    let captures = base.join("captures");
    fs::create_dir(&captures).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hair"))
        .env("HAIR_RESOURCE_ROOT", root)
        .arg("--pack")
        .arg(&pack)
        .arg("--site")
        .arg(root.join("examples/site-a"))
        .arg("--headless-smoke")
        .arg("--smoke-output")
        .arg(&captures)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("{stdout}\n{stderr}");
    assert!(
        output.status.success(),
        "paired client did not load exported pack"
    );
    assert!(!stderr.contains("PACK_FALLBACK"));
    assert!(stdout.contains("THEME=Workshop object"));
    assert!(stdout.contains("THEME_SWITCH_PRESERVED"));
    assert!(stdout.contains("reading=true"));
    assert_eq!(fs::read(&source).unwrap(), before);
    println!("PRIVATE_EXPORT_E2E={}", base.display());
}

#[test]
fn invalid_export_refuses_before_target_mutation_and_preserves_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut p = Project::import(&root.join("assets/kenney/desk.glb")).unwrap();
    let base = std::env::temp_dir().join(format!("hair-invalid-export-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    let out = base.join("absent");
    p.scale = f32::NAN;
    assert!(p.export(&out, &[root.to_path_buf()]).is_err());
    assert!(!out.exists());
    assert!(p.scale.is_nan());
    p.reset();
    assert!(
        p.export(
            &root.join("private-inputs/refuse-output"),
            &[root.to_path_buf()]
        )
        .is_err()
    );
    assert!(!root.join("private-inputs/refuse-output").exists());
    let target = base.join("symlink");
    std::os::unix::fs::symlink(&base, &target).unwrap();
    assert!(p.export(&target, &[root.to_path_buf()]).is_err());
    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::remove_dir_all(base).unwrap();
}

use hair::authoring::Project;
use std::{fs, path::Path};

#[test]
fn import_fit_export_is_a_client_pack_without_mutating_input() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("assets/kenney/desk.glb");
    let before = fs::read(&source).unwrap();
    let out = std::env::temp_dir().join(format!("hair-authoring-{}", std::process::id()));
    let project = Project::import(&source).unwrap();
    assert!(!project.solid);
    assert!(project.scale > 0.0);
    project.export(&out, &[root.to_path_buf()]).unwrap();
    let theme = hair::theme::Theme::load(&out.join("pack.toml"), &mut Default::default()).unwrap();
    let mut world = hair::World::load(root).unwrap();
    theme.act(&mut world, hair::Action::Inspect, theme.document);
    assert!(world.reading);
    assert_eq!(fs::read(&source).unwrap(), before);
    assert_eq!(fs::read(out.join("model.glb")).unwrap(), before);
    assert!(project.export(&out, &[root.to_path_buf()]).is_err());
    fs::remove_dir_all(out).unwrap();
}
