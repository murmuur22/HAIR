use hair::local_explorer::Explorer;
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("hair-nav-{tag}-{}", std::process::id()));
        fs::create_dir(&p).unwrap();
        fs::create_dir(p.join("Start")).unwrap();
        fs::create_dir(p.join("Start/Child")).unwrap();
        fs::write(p.join("Start/Original.txt"), b"unchanged\n").unwrap();
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn enter_back_and_up_beyond_start_are_explicit_read_only_transactions() {
    let f = Fixture::new("normal");
    let start = f.0.join("Start").canonicalize().unwrap();
    let mut e = Explorer::new(&start).unwrap();
    let i = e
        .current()
        .entries
        .iter()
        .position(|v| v.name == "Child")
        .unwrap();
    e.enter(i).expect("enter a real child directory");
    assert_eq!(e.current().folder, start.join("Child"));
    assert!(e.back().unwrap());
    assert_eq!(e.current().folder, start);
    assert!(e.up().expect("Parent is allowed beyond initial folder"));
    assert_eq!(e.current().folder, f.0.canonicalize().unwrap());
    assert!(e.back().unwrap());
    assert_eq!(e.current().folder, start);
    assert_eq!(
        fs::read(start.join("Original.txt")).unwrap(),
        b"unchanged\n"
    );
}
#[test]
fn root_up_is_a_bounded_noop_and_deleted_entry_retains_place() {
    let mut root = Explorer::new(std::path::Path::new("/")).unwrap();
    assert!(!root.up().expect("root edge should not be an error"));
    assert_eq!(root.current().folder, std::path::Path::new("/"));
    let f = Fixture::new("deleted");
    let mut e = Explorer::new(&f.0.join("Start")).unwrap();
    let before = e.current().clone();
    let i = before
        .entries
        .iter()
        .position(|v| v.name == "Child")
        .unwrap();
    fs::remove_dir(f.0.join("Start/Child")).unwrap();
    assert!(e.enter(i).is_err());
    assert_eq!(e.current(), &before);
    e.refresh().expect("explicit refresh");
    assert!(!e.current().entries.iter().any(|v| v.name == "Child"));
}
#[cfg(unix)]
#[test]
fn a_selected_directory_replaced_by_a_link_is_not_followed() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new("link-swap");
    let mut e = Explorer::new(&f.0.join("Start")).unwrap();
    let before = e.current().clone();
    let i = before
        .entries
        .iter()
        .position(|v| v.name == "Child")
        .unwrap();
    fs::remove_dir(f.0.join("Start/Child")).unwrap();
    symlink(&f.0, f.0.join("Start/Child")).unwrap();
    assert!(
        e.enter(i).is_err(),
        "stale folder selection must not follow a replacement link"
    );
    assert_eq!(e.current(), &before);
}
