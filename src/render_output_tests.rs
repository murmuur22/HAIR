use super::*;
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str) -> Self {
        let p = std::env::temp_dir().join(format!("hair-capture-{label}-{}", std::process::id()));
        fs::create_dir(&p).unwrap();
        fs::create_dir(p.join("Resources")).unwrap();
        fs::create_dir(p.join("output")).unwrap();
        Self(p)
    }
    fn output(&self) -> crate::resources::SmokeOutput {
        crate::resources::smoke_output(&self.0.join("Resources"), &self.0.join("output")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn late_regular_file_is_not_truncated_at_capture_consumer() {
    let f = Fixture::new("late-file");
    let output = f.output(); // successful preflight BEFORE the competing write
    let target = output.path().join("office.png");
    let sentinel = b"late regular sentinel";
    fs::write(&target, sentinel).unwrap();
    let result = write_capture(&output, "office.png", 1, 1, &[0, 1, 2, 255]);
    assert_eq!(
        fs::read(&target).unwrap(),
        sentinel,
        "late file was truncated"
    );
    assert!(result.is_err(), "late file must reject, not report success");
}
#[test]
fn late_symlink_does_not_follow_into_resources_at_capture_consumer() {
    let f = Fixture::new("late-symlink");
    let output = f.output();
    let victim = f.0.join("Resources/welcome.md");
    let sentinel = b"resource Markdown sentinel";
    fs::write(&victim, sentinel).unwrap();
    let target = output.path().join("office.png");
    std::os::unix::fs::symlink(&victim, &target).unwrap();
    let result = write_capture(&output, "office.png", 1, 1, &[0, 1, 2, 255]);
    assert_eq!(
        fs::read(&victim).unwrap(),
        sentinel,
        "resource target was overwritten"
    );
    assert!(
        result.is_err(),
        "late symlink must reject, not report success"
    );
}
#[test]
fn directory_rebinding_does_not_redirect_capture_into_resources() {
    let f = Fixture::new("rebind");
    let output = f.output();
    fs::rename(output.path(), f.0.join("owned-output")).unwrap();
    std::os::unix::fs::symlink(f.0.join("Resources"), output.path()).unwrap();
    let target = f.0.join("Resources/office.png");
    let sentinel = b"resource sentinel";
    fs::write(&target, sentinel).unwrap();
    let result = write_capture(&output, "office.png", 1, 1, &[0, 1, 2, 255]);
    assert_eq!(
        fs::read(&target).unwrap(),
        sentinel,
        "rebound directory redirected write"
    );
    assert!(result.is_err() || f.0.join("owned-output/office.png").is_file());
}
#[test]
fn fresh_capture_is_complete_and_cannot_be_reopened() {
    let f = Fixture::new("fresh");
    let output = f.output();
    write_capture(&output, "office.png", 1, 1, &[0, 1, 2, 255]).unwrap();
    let bytes = fs::read(output.path().join("office.png")).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(bytes.windows(4).any(|w| w == b"IEND"));
    assert!(write_capture(&output, "office.png", 1, 1, &[4, 5, 6, 255]).is_err());
    assert_eq!(bytes, fs::read(output.path().join("office.png")).unwrap());
}
#[test]
fn unexpected_capture_paths_are_rejected_without_writes() {
    let f = Fixture::new("unexpected");
    let output = f.output();
    for name in [
        "../Resources/welcome.md",
        "/absolute.png",
        "sub/office.png",
        "other.png",
        "office.png\0",
    ] {
        assert!(
            write_capture(&output, name, 1, 1, &[0, 1, 2, 255])
                .unwrap_err()
                .contains("unexpected smoke capture")
        );
    }
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
}
#[test]
fn owned_writer_is_not_reopened_after_leaf_replacement() {
    use std::io::Write;
    let f = Fixture::new("owned");
    let output = f.output();
    let mut writer = output.create_capture("office.png").unwrap();
    let target = output.path().join("office.png");
    fs::rename(&target, output.path().join("owned.png")).unwrap();
    let victim = f.0.join("Resources/welcome.md");
    fs::write(&victim, b"resource sentinel").unwrap();
    std::os::unix::fs::symlink(&victim, &target).unwrap();
    writer.write_all(b"owned bytes").unwrap();
    drop(writer);
    assert_eq!(fs::read(&victim).unwrap(), b"resource sentinel");
    assert_eq!(
        fs::read(output.path().join("owned.png")).unwrap(),
        b"owned bytes"
    );
}
