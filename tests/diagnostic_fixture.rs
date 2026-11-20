#![cfg(feature = "workshop")]
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
fn setup(name: &str) -> (PathBuf, PathBuf) {
    let b = std::env::temp_dir().join(format!("hair-diag-{}-{name}", std::process::id()));
    fs::create_dir(&b).unwrap();
    let f = b.join("inputs");
    fs::create_dir(&f).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (src, dst) in [
        (
            "milestone-workshop/package/starter/model.glb",
            "document.glb",
        ),
        ("assets/kenney/chairDesk.glb", "decoration.glb"),
        ("examples/door-trim-starter/model.glb", "door.glb"),
        ("examples/site-a/swatch.png", "wall.png"),
        ("examples/site-a/swatch.png", "floor.png"),
    ] {
        fs::copy(root.join(src), f.join(dst)).unwrap();
    }
    fs::create_dir(b.join("captures")).unwrap();
    (b, f)
}
fn refuse(b: &Path, f: &Path, project: &Path, output: &Path, expected: &str) {
    let r = Command::new(env!("CARGO_BIN_EXE_hair-workshop"))
        .env("HAIR_RESOURCE_ROOT", env!("CARGO_MANIFEST_DIR"))
        .args(["--offscreen-proof"])
        .arg(f)
        .arg(project)
        .arg(b.join("pack"))
        .arg(output)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&r.stderr);
    assert!(!r.status.success() && text.contains(expected), "{text}");
    assert!(!b.join("pack").exists());
    assert!(!b.join("project").exists());
    assert!(!b.join("captures/office.png").exists());
}
#[test]
fn fixture_missing_malformed_and_unsafe_are_refused_before_outputs() {
    for name in ["missing", "malformed", "unsafe", "symlink"] {
        let (b, f) = setup(name);
        match name {
            "missing" => fs::remove_file(f.join("floor.png")).unwrap(),
            "malformed" => fs::write(f.join("wall.png"), b"not PNG").unwrap(),
            "unsafe" => {
                fs::copy(f.join("document.glb"), f.join("door.glb")).unwrap();
            }
            _ => {
                fs::remove_file(f.join("wall.png")).unwrap();
                std::os::unix::fs::symlink(f.join("floor.png"), f.join("wall.png")).unwrap();
            }
        }
        refuse(
            &b,
            &f,
            &b.join("project"),
            &b.join("captures"),
            match name {
                "unsafe" => "Unsafe door-trim",
                "missing" => "floor",
                "symlink" => "wall",
                _ => "PNG",
            },
        );
        fs::remove_dir_all(b).unwrap();
    }
}
#[test]
fn fixture_outputs_reject_root_input_alias_and_existing_capture() {
    for name in ["root", "input", "alias", "capture"] {
        let (b, f) = setup(name);
        let output = match name {
            "root" => PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            "input" => f.clone(),
            "alias" => {
                let p = b.join("alias");
                std::os::unix::fs::symlink(&f, &p).unwrap();
                p
            }
            _ => {
                fs::write(b.join("captures/reader-workshop.png"), b"retained").unwrap();
                b.join("captures")
            }
        };
        refuse(
            &b,
            &f,
            &b.join("project"),
            &output,
            if name == "capture" {
                "already exists"
            } else {
                "overlap"
            },
        );
        if name == "capture" {
            assert_eq!(
                fs::read(output.join("reader-workshop.png")).unwrap(),
                b"retained"
            );
        }
        fs::remove_dir_all(b).unwrap();
    }
}
