use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("hair-package-{label}-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn resources(&self, relative: &str) -> PathBuf {
        let dest = self.0.join(relative);
        let source = Path::new(env!("CARGO_MANIFEST_DIR"));
        // Copy only runtime data, no target/source tree.
        for directory in ["content", "assets/kenney", "packs"] {
            fs::create_dir_all(dest.join(directory)).unwrap();
            for entry in fs::read_dir(source.join(directory)).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_file() {
                    fs::copy(entry.path(), dest.join(directory).join(entry.file_name())).unwrap();
                }
            }
        }
        fs::copy(source.join("scene.toml"), dest.join("scene.toml")).unwrap();
        fs::copy(source.join("host-scene.toml"), dest.join("host-scene.toml")).unwrap();
        dest.canonicalize().unwrap()
    }
    fn executable(&self) -> PathBuf {
        let path = self.0.join("Moved HAIR.app/Contents/MacOS/hair");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(env!("CARGO_BIN_EXE_hair"), &path).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn run(exe: &Path, cwd: &Path, root: Option<&Path>, args: &[&str]) -> Output {
    let mut command = Command::new(exe);
    command
        .current_dir(cwd)
        .env_remove("HAIR_RESOURCE_ROOT")
        .args(args);
    if let Some(root) = root {
        command.env("HAIR_RESOURCE_ROOT", root);
    }
    command.output().unwrap()
}
fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
#[test]
fn missing_bundled_host_refuses_but_external_absence_is_explicit_default() {
    let fixture = Fixture::new("required-host");
    let exe = fixture.executable();
    let root = fixture.resources("Moved HAIR.app/Contents/Resources");
    fs::remove_file(root.join("host-scene.toml")).unwrap();
    for selected in [None, Some(root.as_path())] {
        let output = run(&exe, &fixture.0, selected, &["--resource-report"]);
        assert_eq!(output.status.code(), Some(1), "{}", text(&output));
        assert!(text(&output).contains("host-scene.toml"));
        assert!(text(&output).contains("restore the complete private bundle"));
        assert!(!text(&output).contains("RESOURCE_REPORT_OK"));
        assert!(!text(&output).contains("HOST_SCENE_DEFAULT"));
    }
    let site = fixture.0.join("external");
    fs::create_dir(&site).unwrap();
    fs::write(site.join("index.md"), "# External\n").unwrap();
    let output = run(
        &exe,
        &fixture.0,
        None,
        &["--site", site.to_str().unwrap(), "--resource-report"],
    );
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("HOST_SCENE_DEFAULT deterministic shared default"));
    assert!(text(&output).contains("RESOURCE_REPORT_OK"));
    assert!(!site.join("host-scene.toml").exists());
}

#[cfg(feature = "workshop")]
#[test]
fn workshop_missing_host_refuses_before_proof_inputs_gpu_or_output() {
    let fixture = Fixture::new("workshop-host");
    let root = fixture.resources("data");
    fs::remove_file(root.join("host-scene.toml")).unwrap();
    let output = run(
        Path::new(env!("CARGO_BIN_EXE_hair-workshop")),
        &fixture.0,
        Some(&root),
        &[
            "--offscreen-proof",
            "absent-model",
            "absent-project",
            "absent-pack",
            "absent-output",
        ],
    );
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(
        text(&output).contains("host-scene.toml"),
        "{}",
        text(&output)
    );
    assert!(text(&output).contains("restore the complete private bundle"));
    assert!(!text(&output).contains("ADAPTER"));
    assert!(!text(&output).contains("SMOKE_OUTPUT="));
}

#[test]
fn declared_legacy_models_are_decoded_before_resource_report_success() {
    let fixture = Fixture::new("legacy-health");
    let root = fixture.resources("data");
    let exe = Path::new(env!("CARGO_BIN_EXE_hair"));
    let pack =
        hair::Pack::parse(&fs::read_to_string(root.join("assets/kenney/pack.toml")).unwrap())
            .unwrap();
    for asset in pack.assets {
        let path = root.join("assets/kenney").join(&asset.file);
        let pristine = fs::read(&path).unwrap();
        fs::write(&path, b"REVIEWER CORRUPT RESOURCE\n").unwrap();
        let output = run(exe, &fixture.0, Some(&root), &["--resource-report"]);
        assert_eq!(
            output.status.code(),
            Some(1),
            "{}: {}",
            asset.file,
            text(&output)
        );
        assert!(text(&output).contains(&asset.file));
        assert!(text(&output).contains("restore the complete private bundle"));
        assert!(!text(&output).contains("RESOURCE_REPORT_OK"));
        fs::write(&path, pristine).unwrap();
    }
}

#[test]
fn declared_legacy_models_obey_extent_and_grid_footprint_constraints() {
    let fixture = Fixture::new("legacy-constraints");
    let root = fixture.resources("data");
    let exe = Path::new(env!("CARGO_BIN_EXE_hair"));
    let path = root.join("assets/kenney/pack.toml");
    let pristine = fs::read_to_string(&path).unwrap();
    for (bad, message) in [
        (
            pristine.replacen("max_extent = 2.0", "max_extent = 0.01", 1),
            "geometry exceeds manifest bounds",
        ),
        (
            pristine.replacen("footprint = [3, 2]", "footprint = [1, 1]", 1),
            "exceeds grid footprint",
        ),
    ] {
        fs::write(&path, bad).unwrap();
        let output = run(exe, &fixture.0, Some(&root), &["--resource-report"]);
        assert_eq!(output.status.code(), Some(1), "{}", text(&output));
        assert!(text(&output).contains(message), "{}", text(&output));
        assert!(!text(&output).contains("RESOURCE_REPORT_OK"));
    }
}

#[cfg(feature = "workshop")]
#[test]
fn workshop_required_legacy_glb_refuses_before_proof_inputs() {
    let fixture = Fixture::new("workshop-legacy");
    let root = fixture.resources("data");
    fs::write(
        root.join("assets/kenney/desk.glb"),
        b"REVIEWER CORRUPT RESOURCE\n",
    )
    .unwrap();
    let output = run(
        Path::new(env!("CARGO_BIN_EXE_hair-workshop")),
        &fixture.0,
        Some(&root),
        &[
            "--offscreen-proof",
            "absent-model",
            "absent-project",
            "absent-pack",
            "absent-output",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output).contains("assets/kenney/desk.glb"),
        "{}",
        text(&output)
    );
    assert!(text(&output).contains("restore the complete private bundle"));
    assert!(!text(&output).contains("ADAPTER"));
    assert!(!text(&output).contains("SMOKE_OUTPUT="));
}

#[test]
fn explicit_root_precedes_executable_relative_resources() {
    let fixture = Fixture::new("precedence");
    let exe = fixture.executable();
    let bundled = fixture.resources("Moved HAIR.app/Contents/Resources");
    let selected = fixture.resources("override");
    let output = run(&exe, &fixture.0, Some(&selected), &["--resource-report"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains(&format!("RESOURCE_ROOT={}", selected.display())));
    assert!(!text(&output).contains(&format!("RESOURCE_ROOT={}", bundled.display())));
}
#[test]
fn executable_relative_relocation_without_override_or_cwd_dependency() {
    let fixture = Fixture::new("relocation");
    let exe = fixture.executable();
    let resources = fixture.resources("Moved HAIR.app/Contents/Resources");
    let cwd = fixture.0.join("unrelated");
    fs::create_dir(&cwd).unwrap();
    let output = run(&exe, &cwd, None, &["--resource-report"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains(&format!("RESOURCE_ROOT={}", resources.display())));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let paths: Vec<_> = stdout
        .lines()
        .filter_map(|s| s.strip_prefix("RESOURCE_PATH="))
        .collect();
    assert_eq!(paths.len(), 18);
    assert!(paths.iter().all(|p| Path::new(p).starts_with(&resources)));
}
#[test]
fn invalid_override_never_falls_back_to_valid_bundle_or_source() {
    let fixture = Fixture::new("no-fallback");
    let exe = fixture.executable();
    fixture.resources("Moved HAIR.app/Contents/Resources");
    for root in [
        fixture.0.join("missing"),
        PathBuf::from("relative"),
        fixture.0.clone(),
    ] {
        let output = run(&exe, &fixture.0, Some(&root), &["--resource-report"]);
        assert_eq!(output.status.code(), Some(1), "{}", text(&output));
        assert!(text(&output).contains("HAIR_RESOURCE_ROOT"));
        assert!(text(&output).contains("fallback"));
        assert!(!text(&output).contains("RESOURCE_REPORT_OK"));
    }
}
#[test]
fn missing_and_corrupt_assets_are_actionable() {
    let fixture = Fixture::new("missing-assets");
    let root = fixture.resources("data");
    let exe = Path::new(env!("CARGO_BIN_EXE_hair"));
    fs::remove_file(root.join("packs/desk.glb")).unwrap();
    let missing = run(exe, &fixture.0, Some(&root), &["--resource-report"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(text(&missing).contains("desk.glb"));
    assert!(text(&missing).contains("restore the complete private bundle"));
    fs::write(root.join("packs/desk.glb"), b"corrupt").unwrap();
    let corrupt = run(exe, &fixture.0, Some(&root), &["--resource-report"]);
    assert_eq!(corrupt.status.code(), Some(1));
    assert!(text(&corrupt).contains("restore the complete private bundle"));
    assert!(!text(&missing).contains("RESOURCE_REPORT_OK"));
    assert!(!text(&corrupt).contains("RESOURCE_REPORT_OK"));
    // Restore mandatory assets; an explicitly selected optional user pack can
    // still use transparent role fallback without mislabelling damaged app data.
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("packs/desk.glb"),
        root.join("packs/desk.glb"),
    )
    .unwrap();
    let optional = fixture.0.join("optional");
    fs::create_dir(&optional).unwrap();
    fs::write(optional.join("pack.toml"), "version=2\nname='Optional missing role'\n[[roles]]\nrole='document'\nfile='absent.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0.0,0.0,0.0]\n").unwrap();
    let output = run(
        exe,
        &fixture.0,
        Some(&root),
        &[
            "--pack",
            optional.join("pack.toml").to_str().unwrap(),
            "--resource-report",
        ],
    );
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("ROLE_FALLBACK document"));
    assert!(text(&output).contains("RESOURCE_REPORT_OK"));
}
#[test]
fn smoke_requires_separate_explicit_output_before_gpu_or_writes() {
    let fixture = Fixture::new("smoke-output");
    let exe = fixture.executable();
    let root = fixture.resources("Moved HAIR.app/Contents/Resources");
    let no_output = run(&exe, &fixture.0, None, &["--headless-smoke"]);
    assert_eq!(no_output.status.code(), Some(1), "{}", text(&no_output));
    assert!(
        text(&no_output).contains("--smoke-output"),
        "{}",
        text(&no_output)
    );
    assert!(
        !root.join("evidence").exists(),
        "must not write into Resources"
    );
    for dest in [&root, &fixture.0, Path::new("relative")] {
        let output = run(
            &exe,
            &fixture.0,
            None,
            &["--headless-smoke", "--smoke-output", dest.to_str().unwrap()],
        );
        assert_eq!(output.status.code(), Some(1));
        assert!(text(&output).contains("smoke output"), "{}", text(&output));
        assert!(!text(&output).contains("ADAPTER"));
    }
}
#[test]
fn smoke_output_symlink_and_existing_capture_cannot_overwrite_resources() {
    let fixture = Fixture::new("smoke-symlink");
    let exe = fixture.executable();
    let root = fixture.resources("Moved HAIR.app/Contents/Resources");
    let output = fixture.0.join("captures");
    fs::create_dir(&output).unwrap();
    std::os::unix::fs::symlink(root.join("content/welcome.md"), output.join("office.png")).unwrap();
    let before = fs::read(root.join("content/welcome.md")).unwrap();
    let result = run(
        &exe,
        &fixture.0,
        None,
        &[
            "--headless-smoke",
            "--smoke-output",
            output.to_str().unwrap(),
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(
        text(&result).contains("already exists"),
        "{}",
        text(&result)
    );
    assert_eq!(before, fs::read(root.join("content/welcome.md")).unwrap());
}
#[test]
fn world_and_scene_reject_top_level_symlink_escape() {
    let fixture = Fixture::new("containment");
    let root = fixture.resources("data");
    let outside = fixture.0.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::rename(root.join("content"), outside.join("content")).unwrap();
    std::os::unix::fs::symlink(outside.join("content"), root.join("content")).unwrap();
    assert!(
        hair::World::load(&root).is_err(),
        "World::load must confine graph/documents to selected root"
    );
    fs::rename(root.join("assets"), outside.join("assets")).unwrap();
    std::os::unix::fs::symlink(outside.join("assets"), root.join("assets")).unwrap();
    assert!(
        hair::scene::office(&root).is_err(),
        "scene must confine pack/models to selected root"
    );
}
