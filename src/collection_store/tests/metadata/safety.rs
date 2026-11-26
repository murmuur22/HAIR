use super::*;

#[test]
fn full_verify_detects_same_length_corruption_without_cached_trust() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (_, reader) = small_source(&f, &[0xa5; 131073]);
    let mut store = Store::open(&root, policy()).unwrap();
    let good = store.import_document(OsStr::new("good"), &reader, &AtomicBool::new(false)).unwrap().value;
    let bad = store.import_document(OsStr::new("bad"), &reader, &AtomicBool::new(false)).unwrap().value;
    let path = root.join(payload_name(bad.id));
    fs::write(&path, [0x5a; 131073]).unwrap();
    let damaged = identity(&path);
    let manifest = fs::read(root.join(MANIFEST)).unwrap();
    take_body_reads();
    assert!(store.inspect().unwrap().iter().all(|i| i.status == PayloadStatus::Unchecked));
    assert!(take_body_reads().is_empty());
    let report = store.verify_all(store.usage(), &AtomicBool::new(false)).unwrap();
    assert_eq!(report[0].item, good);
    assert_eq!(report[0].status, PayloadStatus::Verified);
    assert!(matches!(report[1].status, PayloadStatus::Invalid(_)));
    assert_eq!(take_body_reads().iter().map(|(_, _, n)| *n as u64).sum::<u64>(), store.usage());
    // No durable trust flags: metadata-only inspection remains Unchecked.
    assert!(store.inspect().unwrap().iter().all(|i| i.status == PayloadStatus::Unchecked));
    let mut output = Vec::new();
    assert!(store.copy_to(bad.id, &mut output, &AtomicBool::new(false)).unwrap_err().contains("Untrusted partial output"));
    assert_eq!(output, [0x5a; 131073]);
    assert!(store.remove(bad.id).is_err());
    assert_eq!(identity(&path), damaged);
    assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), manifest);
}

#[test]
fn full_verify_budget_and_real_chunk_cancellation_are_bounded_write_free() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (_, reader) = small_source(&f, &[0xa5; 131073]);
    let mut store = Store::open(&root, policy()).unwrap();
    let item = store.import_document(OsStr::new("large"), &reader, &AtomicBool::new(false)).unwrap().value;
    let before = identity(&root.join(payload_name(item.id)));
    take_body_reads();
    assert!(store.verify_all(item.bytes - 1, &AtomicBool::new(false)).unwrap_err().contains("budget"));
    assert!(store.verify_all(item.bytes, &AtomicBool::new(true)).unwrap_err().contains("cancelled"));
    assert!(take_body_reads().is_empty());
    let cancelled = std::sync::Arc::new(AtomicBool::new(false));
    cancel_after_body_read(cancelled.clone());
    assert!(store.verify_all(item.bytes, &cancelled).unwrap_err().contains("cancelled"));
    let reads = take_body_reads();
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].2, 65536);
    assert_eq!(identity(&root.join(payload_name(item.id))), before);
    assert!(store.evidence().unwrap().is_empty());
}

#[test]
fn finder_unsafe_types_modes_links_and_size_refuse_without_initialization() {
    for case in ["symlink", "directory", "fifo", "hardlink", "writable", "special", "oversized"] {
        let f = Fixture::new();
        let root = f.root("cache");
        let p = root.join(FINDER);
        let external = f.0.join("external");
        fs::write(&external, b"preserved").unwrap();
        match case {
            "symlink" => std::os::unix::fs::symlink(&external, &p).unwrap(),
            "directory" => fs::create_dir(&p).unwrap(),
            "fifo" => { let c = CString::new(p.as_os_str().as_bytes()).unwrap(); assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0); },
            "hardlink" => fs::hard_link(&external, &p).unwrap(),
            _ => {
                fs::write(&p, b"Finder").unwrap();
                let mode = match case { "writable" => 0o666, "special" => 0o1644, _ => 0o644 };
                fs::set_permissions(&p, fs::Permissions::from_mode(mode)).unwrap();
                if case == "oversized" { fs::OpenOptions::new().write(true).open(&p).unwrap().set_len(1024 * 1024 + 1).unwrap(); }
            },
        }
        let m = fs::symlink_metadata(&p).unwrap();
        assert!(Store::open(&root, policy()).is_err(), "accepted {case}");
        let now = fs::symlink_metadata(&p).unwrap();
        assert_eq!((m.dev(), m.ino(), m.mode(), m.len()), (now.dev(), now.ino(), now.mode(), now.len()));
        assert!(!root.join("lock").exists());
        assert!(!root.join(MANIFEST).exists());
        assert_eq!(fs::read(&external).unwrap(), b"preserved");
    }
}

#[test]
fn finder_recovery_preserves_identity_and_changed_finder_refuses_stale_capability() {
    for changed in [false, true] {
        let f = Fixture::new();
        let root = f.root("cache");
        let (_, reader) = small_source(&f, b"live original");
        let mut store = Store::open(&root, policy()).unwrap();
        let item = store.import_document(OsStr::new("live"), &reader, &AtomicBool::new(false)).unwrap().value;
        let finder = root.join(FINDER);
        fs::write(&finder, b"opaque Finder").unwrap();
        fs::set_permissions(&finder, fs::Permissions::from_mode(0o644)).unwrap();
        let before = identity(&finder);
        let cancelled = AtomicBool::new(false);
        assert!(store.import_inner(OsStr::new("cancel"), &reader, &cancelled, |stage, _| {
            if stage == ImportStage::ManifestReady { cancelled.store(true, Ordering::Relaxed); }
            Ok(())
        }).is_err());
        let manifest = fs::read(root.join(MANIFEST)).unwrap();
        let mut inspection = store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
        assert!(!inspection.candidates().iter().any(|n| n == FINDER));
        if changed { fs::write(&finder, b"changedFinder").unwrap(); }
        let result = store.discard_interrupted_import(&mut inspection, &AtomicBool::new(false));
        if changed {
            assert!(result.is_err());
            assert!(root.join(MANIFEST_PENDING).exists());
            assert!(root.join(payload_name(2)).exists());
            assert_eq!(fs::read(&finder).unwrap(), b"changedFinder");
        } else {
            let outcome = result.unwrap();
            assert_eq!(outcome.removed.len(), 2);
            assert_eq!(outcome.remaining, Some(Vec::new()));
            assert!(outcome.error.is_none() && outcome.sync_warning.is_none());
            assert_eq!(identity(&finder), before);
            assert_eq!(fs::read(&finder).unwrap(), b"opaque Finder");
        }
        drop(inspection);
        assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), manifest);
        let mut bytes = Vec::new();
        store.copy_to(item.id, &mut bytes, &AtomicBool::new(false)).unwrap();
        assert_eq!(bytes, b"live original");
    }
}

#[test]
fn other_dotfiles_remain_preserved_write_blocking_evidence() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (_, reader) = small_source(&f, b"bytes");
    let mut store = Store::open(&root, policy()).unwrap();
    for name in [".ds_store", ".DS_Store.extra", ".owner-evidence"] { fs::write(root.join(name), name.as_bytes()).unwrap(); }
    store.refresh().unwrap();
    assert_eq!(store.evidence().unwrap().len(), 3);
    assert!(store.import_document(OsStr::new("no"), &reader, &AtomicBool::new(false)).is_err());
    assert!(store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err());
    for name in [".ds_store", ".DS_Store.extra", ".owner-evidence"] { assert_eq!(fs::read(root.join(name)).unwrap(), name.as_bytes()); }
}

struct MutateOutput<'a> { bytes: Vec<u8>, mutate: Option<Box<dyn FnOnce() + 'a>> }
impl Write for MutateOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        if let Some(mutate) = self.mutate.take() { mutate(); }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

#[test]
fn selected_export_rejects_real_body_and_authority_changes_with_untrusted_output() {
    for case in ["growth", "truncate", "same-length", "payload-rebind", "manifest-rebind", "manifest-tamper", "lock-rebind", "lock-tamper", "root-rebind"] {
        let f = Fixture::new();
        let root = f.root("cache");
        let (_, reader) = small_source(&f, &[0xa5; 131073]);
        let mut store = Store::open(&root, policy()).unwrap();
        let item = store.import_document(OsStr::new("selected"), &reader, &AtomicBool::new(false)).unwrap().value;
        let mut output = MutateOutput { bytes: Vec::new(), mutate: Some(Box::new(|| {
            let payload = root.join(payload_name(item.id));
            match case {
                "growth" => fs::OpenOptions::new().append(true).open(&payload).unwrap().write_all(b"growth").unwrap(),
                "truncate" => fs::OpenOptions::new().write(true).open(&payload).unwrap().set_len(65536).unwrap(),
                "same-length" => { use std::os::unix::fs::FileExt; fs::OpenOptions::new().write(true).open(&payload).unwrap().write_at(b"changed", 70000).unwrap(); },
                "root-rebind" => { fs::rename(&root, f.0.join("held-cache")).unwrap(); fs::create_dir(&root).unwrap(); },
                "manifest-tamper" | "lock-tamper" => {
                    let p = root.join(if case == "manifest-tamper" { MANIFEST } else { "lock" });
                    fs::OpenOptions::new().append(true).open(p).unwrap().write_all(b"changed").unwrap();
                },
                _ => {
                    let p = match case { "payload-rebind" => payload, "manifest-rebind" => root.join(MANIFEST), _ => root.join("lock") };
                    let bytes = fs::read(&p).unwrap();
                    fs::rename(&p, f.0.join("held-authority")).unwrap();
                    fs::write(&p, bytes).unwrap();
                    fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
                },
            }
        })) };
        let error = store.copy_to(item.id, &mut output, &AtomicBool::new(false)).unwrap_err();
        assert!(error.contains("Untrusted partial output; discard output"), "{case}: {error}");
        assert!(!output.bytes.is_empty());
        assert!(output.mutate.is_none());
    }
}

#[test]
fn import_rejects_late_manifest_tamper_or_rebind_preserving_evidence() {
    for rebind in [false, true] {
        let f = Fixture::new();
        let root = f.root("cache");
        let (_, reader) = small_source(&f, b"original");
        let mut store = Store::open(&root, policy()).unwrap();
        let before = fs::read(root.join(MANIFEST)).unwrap();
        let result = store.import_inner(OsStr::new("no"), &reader, &AtomicBool::new(false), |stage, _| {
            if stage == ImportStage::ManifestReady {
                let path = root.join(MANIFEST);
                if rebind {
                    fs::rename(&path, f.0.join("held-manifest")).unwrap();
                    fs::write(&path, &before).unwrap();
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
                } else { fs::OpenOptions::new().append(true).open(path).unwrap().write_all(b"tamper").unwrap(); }
            }
            Ok(())
        });
        assert!(matches!(result, Err(Failure::Refused(_))));
        assert!(store.items().is_empty());
        assert!(root.join(payload_name(1)).exists());
        assert!(root.join(MANIFEST_PENDING).exists());
        if rebind { assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), before); }
        else { assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), [before, b"tamper".to_vec()].concat()); }
    }
}
