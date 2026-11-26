#![cfg(target_os = "macos")]
//! Public-API regression for the replacement file-and-position transaction.
use hair::{
    local_explorer::{Explorer, MAX_BODY_BYTES},
    placement_store::{self, Outcome, Position},
};
use std::{ffi::OsStr, path::PathBuf, sync::atomic::AtomicBool};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hair-inv-write-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn inventory_write_is_no_replace_and_held_directory_relative() {
    let f = Fixture::new();
    let old = f.0.join("original");
    let renamed = f.0.join("renamed");
    std::fs::create_dir(&old).unwrap();
    let explorer = Explorer::new(&old).unwrap();
    std::fs::rename(&old, &renamed).unwrap();
    std::fs::create_dir(&old).unwrap();
    let position = Position {
        properties: Default::default(),
        at: [0., 0.],
        yaw: 0.,
        size: [1., 1., 1.],
        allow_overlap: false,
        surface: None,
    };
    let cancelled = AtomicBool::new(false);
    let publish = |name: &OsStr, bytes: &[u8]| {
        placement_store::publish(&explorer, name, bytes, &position, &cancelled)
    };
    assert_eq!(
        publish(OsStr::new("New.txt"), b"owned inventory bytes"),
        Outcome::Committed { synced: true }
    );
    assert_eq!(
        std::fs::read(renamed.join("New.txt")).unwrap(),
        b"owned inventory bytes"
    );
    assert!(!old.join("New.txt").exists());
    assert!(matches!(
        publish(OsStr::new("New.txt"), b"overwrite"),
        Outcome::NotCommitted(_)
    ));
    assert_eq!(
        std::fs::read(renamed.join("New.txt")).unwrap(),
        b"owned inventory bytes"
    );
    std::os::unix::fs::symlink(f.0.join("absent"), renamed.join("Link.txt")).unwrap();
    assert!(matches!(
        publish(OsStr::new("Link.txt"), b"no"),
        Outcome::NotCommitted(_)
    ));
    assert!(!f.0.join("absent").exists());
    for name in ["", ".", "..", "escape/file", "bad\0name"] {
        assert!(matches!(
            publish(OsStr::new(name), b"no"),
            Outcome::NotCommitted(_)
        ));
    }
    assert!(matches!(
        publish(OsStr::new("Huge"), &vec![0; MAX_BODY_BYTES as usize + 1]),
        Outcome::NotCommitted(_)
    ));
    let positions = placement_store::positions(&Explorer::new(&renamed).unwrap()).unwrap();
    assert_eq!(positions, vec![("New.txt".into(), position)]);
    let names: Vec<_> = std::fs::read_dir(&renamed)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        names.len(),
        3,
        "only final file, existing link and explicit hidden layout storage; no loose transfer files"
    );
}
