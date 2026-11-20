use hair::local_explorer::{Explorer, Kind, MAX_HISTORY, escape_display};
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("hair-safety-{tag}-{}", std::process::id()));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn repeated_refresh_reenumerates_and_never_reads_descendant_bodies() {
    let f = Fixture::new("refresh");
    fs::create_dir(f.0.join("Child")).unwrap();
    fs::write(f.0.join("Child/Deep.txt"), b"not a direct entry").unwrap();
    let mut e = Explorer::new(&f.0).unwrap();
    assert_eq!(e.current().entries.len(), 1);
    for _ in 0..4 {
        e.refresh().unwrap();
        assert_eq!(e.current().entries.len(), 1);
    }
    fs::write(f.0.join("New.txt"), b"new").unwrap();
    e.refresh().unwrap();
    assert_eq!(e.current().entries.len(), 2);
    fs::remove_file(f.0.join("New.txt")).unwrap();
    e.refresh().unwrap();
    assert_eq!(e.current().entries.len(), 1);
}
#[test]
fn child_entry_uses_held_ancestor_after_original_path_is_rebound() {
    let f = Fixture::new("ancestor");
    let start = f.0.join("Start");
    fs::create_dir(&start).unwrap();
    fs::create_dir(start.join("Child")).unwrap();
    fs::write(start.join("Child/Old.txt"), b"old").unwrap();
    let mut e = Explorer::new(&start).unwrap();
    let i = e
        .current()
        .entries
        .iter()
        .position(|v| v.name == "Child")
        .unwrap();
    fs::rename(&start, f.0.join("Moved")).unwrap();
    fs::create_dir(&start).unwrap();
    fs::create_dir(start.join("Child")).unwrap();
    fs::write(start.join("Child/Wrong.txt"), b"replacement").unwrap();
    e.enter(i).unwrap();
    assert_eq!(
        e.current().folder,
        f.0.join("Moved/Child").canonicalize().unwrap()
    );
    assert!(e.current().entries.iter().any(|v| v.name == "Old.txt"));
    assert!(!e.current().entries.iter().any(|v| v.name == "Wrong.txt"));
}
#[test]
fn history_is_finite_and_recent_directory_handles_remain_usable() {
    let f = Fixture::new("history");
    let mut p = f.0.clone();
    for _ in 0..(MAX_HISTORY + 3) {
        p = p.join("d");
        fs::create_dir(&p).unwrap();
    }
    let mut e = Explorer::new(&f.0).unwrap();
    for _ in 0..(MAX_HISTORY + 3) {
        let i = e
            .current()
            .entries
            .iter()
            .position(|v| v.kind == Kind::Folder)
            .unwrap();
        e.enter(i).unwrap();
    }
    for _ in 0..MAX_HISTORY {
        assert!(e.back().unwrap());
    }
    assert!(!e.back().unwrap());
    assert_eq!(
        e.current().folder,
        f.0.join("d/d/d").canonicalize().unwrap()
    );
}
#[test]
fn controlled_names_remain_os_identities_not_display_or_shell_arguments() {
    let f = Fixture::new("names");
    let names = [
        "--help.txt",
        "control\nname.txt",
        "[not authority](https:example).txt",
        "café.txt",
    ];
    for (i, name) in names.iter().enumerate() {
        fs::write(f.0.join(name), format!("body-{i}")).unwrap();
    }
    let e = Explorer::new(&f.0).unwrap();
    assert_eq!(e.current().entries.len(), names.len());
    for entry in &e.current().entries {
        assert!(!escape_display(&entry.name).contains('\n'));
        let i = e
            .current()
            .entries
            .iter()
            .position(|v| v.name == entry.name)
            .unwrap();
        let body = e.read_entry(i, 1024).unwrap();
        assert!(body.starts_with(b"body-"));
    }
}
#[cfg(unix)]
#[test]
fn non_utf8_display_escapes_without_merging_distinct_raw_names() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let a = OsString::from_vec(b"name-\xff.txt".to_vec());
    let b = OsString::from_vec(b"name-\xfe.txt".to_vec());
    assert_ne!(a, b);
    assert_ne!(escape_display(&a), escape_display(&b));
}
#[cfg(unix)]
#[test]
fn repeated_navigation_and_inspection_release_owned_descriptors() {
    let f = Fixture::new("fds");
    fs::create_dir(f.0.join("Child")).unwrap();
    fs::write(f.0.join("Body.txt"), b"body").unwrap();
    let count = || fs::read_dir("/dev/fd").unwrap().count();
    let before = count();
    for _ in 0..32 {
        let mut e = Explorer::new(&f.0).unwrap();
        let file = e
            .current()
            .entries
            .iter()
            .position(|v| v.name == "Body.txt")
            .unwrap();
        assert_eq!(e.read_entry(file, 1024).unwrap(), b"body");
        let child = e
            .current()
            .entries
            .iter()
            .position(|v| v.name == "Child")
            .unwrap();
        e.enter(child).unwrap();
        assert!(e.back().unwrap());
        e.refresh().unwrap();
    }
    assert!(
        count() <= before + 2,
        "repeated operations leaked descriptors"
    );
}
