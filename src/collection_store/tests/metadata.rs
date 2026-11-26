use super::*;
mod safety;
fn identity(path: &Path) -> (u64, u64, u64, u32, i64, i64, i64, i64) {
    let m = fs::metadata(path).unwrap();
    (m.dev(), m.ino(), m.len(), m.mode(), m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec())
}

#[test]
fn metadata_hot_paths_read_no_unrelated_bodies() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (_, reader) = small_source(&f, &[0xa5; 131073]);
    let mut store = Store::open(&root, policy()).unwrap();
    let first = store.import_document(OsStr::new("one"), &reader, &AtomicBool::new(false)).unwrap().value;
    let second = store.import_document(OsStr::new("two"), &reader, &AtomicBool::new(false)).unwrap().value;
    take_body_reads();
    store.refresh().unwrap();
    let reopened = Store::open(&root, policy()).unwrap();
    assert_eq!(reopened.items(), &[first.clone(), second.clone()]);
    assert!(take_body_reads().is_empty(), "listing/open/refresh read payload bodies");
    reopened.copy_to(first.id, &mut std::io::sink(), &AtomicBool::new(false)).unwrap();
    let reads = take_body_reads();
    let selected = fs::metadata(root.join(payload_name(first.id))).unwrap();
    assert!(!reads.is_empty());
    assert!(reads.iter().all(|(dev, ino, _)| *dev == selected.dev() && *ino == selected.ino()), "export read an unrelated body");
    assert_eq!(reads.iter().map(|(_, _, n)| *n as u64).sum::<u64>(), first.bytes);
}

#[test]
fn metadata_selected_export_reads_only_selected_body() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (_, reader) = small_source(&f, &[0x42; 131073]);
    let mut store = Store::open(&root, policy()).unwrap();
    let selected = store.import_document(OsStr::new("selected"), &reader, &AtomicBool::new(false)).unwrap().value;
    store.import_document(OsStr::new("unrelated"), &reader, &AtomicBool::new(false)).unwrap();
    let m = fs::metadata(root.join(payload_name(selected.id))).unwrap();
    take_body_reads();
    store.copy_to(selected.id, &mut std::io::sink(), &AtomicBool::new(false)).unwrap();
    let reads = take_body_reads();
    assert!(!reads.is_empty());
    assert!(reads.iter().all(|(dev, ino, _)| (*dev, *ino) == (m.dev(), m.ino())), "selected export read unrelated bodies: {reads:?}");
    assert_eq!(reads.iter().map(|(_, _, n)| *n as u64).sum::<u64>(), selected.bytes);
}

#[test]
fn metadata_unrelated_damage_does_not_block_healthy_access() {
    for damage in ["corrupt", "missing", "directory", "symlink", "fifo"] {
        let f = Fixture::new();
        let root = f.root("cache");
        let (original, reader) = small_source(&f, b"healthy bytes");
        let room = f.root("room").join("retained");
        fs::copy(&original, &room).unwrap();
        let mut store = Store::open(&root, policy()).unwrap();
        let healthy = store.import_document(OsStr::new("healthy"), &reader, &AtomicBool::new(false)).unwrap().value;
        let bad = store.import_document(OsStr::new("bad"), &reader, &AtomicBool::new(false)).unwrap().value;
        let path = root.join(payload_name(bad.id));
        fs::remove_file(&path).unwrap();
        match damage {
            "corrupt" => { fs::write(&path, b"unhappy bytes").unwrap(); fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap(); },
            "directory" => fs::create_dir(&path).unwrap(),
            "symlink" => std::os::unix::fs::symlink(&original, &path).unwrap(),
            "fifo" => { let c = CString::new(path.as_os_str().as_bytes()).unwrap(); assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0); },
            _ => {},
        }
        let manifest = fs::read(root.join(MANIFEST)).unwrap();
        store.refresh().unwrap();
        let reopened = Store::open(&root, policy()).unwrap();
        assert_eq!(reopened.items(), &[healthy.clone(), bad.clone()]);
        let report = reopened.inspect().unwrap();
        assert_eq!(report[0].status, PayloadStatus::Unchecked);
        if damage == "corrupt" { assert_eq!(report[1].status, PayloadStatus::Unchecked); }
        else { assert!(matches!(report[1].status, PayloadStatus::Unavailable(_))); }
        let mut output = Vec::new();
        reopened.copy_to(healthy.id, &mut output, &AtomicBool::new(false)).unwrap();
        assert_eq!(output, b"healthy bytes");
        let mut untrusted = Vec::new();
        let error = reopened.copy_to(bad.id, &mut untrusted, &AtomicBool::new(false)).unwrap_err();
        assert!(error.contains("Untrusted partial output; discard output"));
        if damage == "corrupt" { assert_eq!(untrusted, b"unhappy bytes"); }
        else { assert!(untrusted.is_empty()); }
        // Damaged SELECTED removal still refuses. Independent mutation tests
        // prove healthy import/removal preserve unrelated damaged entries.
        assert!(store.remove(bad.id).is_err());
        assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), manifest);
        assert_eq!(fs::read(&original).unwrap(), b"healthy bytes");
        assert_eq!(fs::read(&room).unwrap(), b"healthy bytes");
    }
}

#[test]
fn metadata_finder_is_inert_not_evidence() {
    for preexisting in [false, true] {
        let f = Fixture::new();
        let root = f.root("cache");
        let (_, reader) = small_source(&f, b"document");
        let finder = root.join(".DS_Store");
        let write_finder = || {
            fs::write(&finder, b"opaque Finder bytes, never parsed").unwrap();
            fs::set_permissions(&finder, fs::Permissions::from_mode(0o644)).unwrap();
        };
        if preexisting { write_finder(); }
        let mut store = Store::open(&root, policy()).unwrap();
        if !preexisting { write_finder(); }
        let before = identity(&finder);
        store.refresh().unwrap();
        assert!(store.evidence().unwrap().is_empty(), "Finder metadata blocks writes as unknown evidence");
        let item = store.import_document(OsStr::new("one"), &reader, &AtomicBool::new(false)).unwrap().value;
        assert!(store.remove(item.id).unwrap().is_complete());
        assert_eq!(identity(&finder), before);
        assert_eq!(fs::read(&finder).unwrap(), b"opaque Finder bytes, never parsed");
    }
}
