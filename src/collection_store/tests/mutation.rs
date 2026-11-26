//! Real mutation reads and cancellation; hooks observe actual metadata/body I/O.
use super::*;
use std::sync::Arc;

fn remove_refused(result: std::result::Result<RemovalOutcome, Failure>) {
    match result {
        Err(Failure::Refused(e)) => assert!(e.contains("cancelled"), "{e}"),
        other => panic!("expected cancellation refusal, got {other:?}"),
    }
}

#[test]
fn mutation_pre_cancel_remove_is_write_free_and_body_read_free() {
    let mut f = FaultFixture::new();
    take_body_reads();
    remove_refused(f.store.remove_cancelled(f.kept[0].id, &AtomicBool::new(true)));
    assert!(take_body_reads().is_empty());
    f.check(Vec::new());
}

#[test]
fn mutation_mid_metadata_precheck_import_stops_before_staging() {
    let mut f = FaultFixture::new();
    let cancelled = Arc::new(AtomicBool::new(false));
    take_body_reads();
    cancel_after_metadata_check(cancelled.clone());
    refused(f.store.import_document(OsStr::new("cancel"), &f.reader, &cancelled), "cancelled");
    assert!(cancelled.load(Ordering::Relaxed));
    assert!(take_body_reads().is_empty(), "precheck hashed unrelated live bodies");
    f.check(Vec::new());
}

#[test]
fn mutation_mid_metadata_precheck_remove_stops_before_selected_hashing() {
    let mut f = FaultFixture::new();
    let cancelled = Arc::new(AtomicBool::new(false));
    take_body_reads();
    cancel_after_metadata_check(cancelled.clone());
    remove_refused(f.store.remove_cancelled(f.kept[0].id, &cancelled));
    assert!(cancelled.load(Ordering::Relaxed));
    assert!(take_body_reads().is_empty());
    f.check(Vec::new());
}

#[test]
fn mutation_import_reads_only_new_staged_body() {
    let mut f = FaultFixture::new();
    take_body_reads();
    let item = f.store.import_document(OsStr::new("new"), &f.reader, &AtomicBool::new(false)).unwrap().value;
    let reads = take_body_reads();
    let m = fs::metadata(f.root.join(payload_name(item.id))).unwrap();
    assert!(!reads.is_empty());
    assert!(reads.iter().all(|(dev, ino, _)| (*dev, *ino) == (m.dev(), m.ino())), "import read unrelated bodies: {reads:?}");
    assert_eq!(reads.iter().map(|(_, _, n)| *n as u64).sum::<u64>(), item.bytes);
    for (kept, before) in f.kept.iter().zip(&f.kept_identities) {
        let p = f.root.join(payload_name(kept.id));
        assert_eq!(identity(&p), *before);
        assert_eq!(fs::read(p).unwrap(), f.bytes);
    }
    assert_eq!(&f.store.items()[..f.kept.len()], f.kept);
    assert_eq!(identity(&f.original), f.original_identity);
    assert_eq!(identity(&f.room), f.room_identity);
}

#[test]
fn mutation_healthy_remove_reads_selected_body_once_only() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (source, reader) = small_source(&f, &[0xa5; 131073]);
    let mut store = Store::open(&root, policy()).unwrap();
    let selected = store.import_document(OsStr::new("selected"), &reader, &AtomicBool::new(false)).unwrap().value;
    let kept = store.import_document(OsStr::new("kept"), &reader, &AtomicBool::new(false)).unwrap().value;
    let selected_m = fs::metadata(root.join(payload_name(selected.id))).unwrap();
    let kept_before = identity(&root.join(payload_name(kept.id)));
    let source_before = identity(&source);
    // Lowered storage policy is not a new selected-deletion byte ceiling.
    store.set_policy(Policy::new(1, 1).unwrap());
    take_body_reads();
    assert!(store.remove_cancelled(selected.id, &AtomicBool::new(false)).unwrap().is_complete());
    let reads = take_body_reads();
    assert!(!reads.is_empty());
    assert!(reads.iter().all(|(dev, ino, _)| (*dev, *ino) == (selected_m.dev(), selected_m.ino())), "remove read unrelated bodies: {reads:?}");
    assert_eq!(reads.iter().map(|(_, _, n)| *n as u64).sum::<u64>(), selected.bytes);
    assert_eq!(identity(&root.join(payload_name(kept.id))), kept_before);
    assert_eq!(identity(&source), source_before);
    assert_eq!(store.items(), &[kept]);
}

#[test]
fn mutation_mid_selected_hash_cancel_is_write_free() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (_, reader) = small_source(&f, &[0xa5; 131073]);
    let mut store = Store::open(&root, policy()).unwrap();
    let item = store.import_document(OsStr::new("selected"), &reader, &AtomicBool::new(false)).unwrap().value;
    let manifest = fs::read(root.join(MANIFEST)).unwrap();
    let manifest_before = identity(&root.join(MANIFEST));
    let body_before = identity(&root.join(payload_name(item.id)));
    take_body_reads();
    let cancelled = Arc::new(AtomicBool::new(false));
    cancel_after_body_read(cancelled.clone());
    remove_refused(store.remove_cancelled(item.id, &cancelled));
    let reads = take_body_reads();
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].2, 65536);
    assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), manifest);
    assert_eq!(identity(&root.join(MANIFEST)), manifest_before);
    assert_eq!(identity(&root.join(payload_name(item.id))), body_before);
    store.refresh().unwrap();
    assert!(store.evidence().unwrap().is_empty());
    assert_eq!(store.items(), &[item]);
}

fn final_staged_cancel(mid_hash: bool) {
    let f = Fixture::new();
    let root = f.root("cache");
    let (source, reader) = small_source(&f, &[0xa5; 131073]);
    let source_before = identity(&source);
    let mut store = Store::open(&root, policy()).unwrap();
    let manifest = fs::read(root.join(MANIFEST)).unwrap();
    let manifest_before = identity(&root.join(MANIFEST));
    let cancelled = Arc::new(AtomicBool::new(false));
    take_body_reads();
    let result = store.import_inner(OsStr::new("new"), &reader, &cancelled, |stage, _| {
        if stage == ImportStage::ManifestReady {
            if mid_hash { cancel_after_body_read(cancelled.clone()); }
            else { cancelled.store(true, Ordering::Relaxed); }
        }
        Ok(())
    });
    refused(result, "cancelled");
    let reads = take_body_reads();
    if mid_hash {
        assert_eq!(reads.len(), 1, "final staged hashing ignored cancellation: {reads:?}");
        assert_eq!(reads[0].2, 65536);
    } else { assert!(reads.is_empty(), "pre-cancelled final staged hash still read: {reads:?}"); }
    assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), manifest);
    assert_eq!(identity(&root.join(MANIFEST)), manifest_before);
    assert_eq!(identity(&source), source_before);
    assert_eq!(fs::read(root.join(payload_name(1))).unwrap(), [0xa5; 131073]);
    let pending = Manifest::decode(&fs::read(root.join(MANIFEST_PENDING)).unwrap()).unwrap();
    assert_eq!(pending.items[0].bytes, 131073);
    assert_eq!(pending.items[0].sha256, <[u8; 32]>::from(Sha256::digest([0xa5; 131073])));
    store.refresh().unwrap();
    assert!(store.items().is_empty());
    assert_eq!(store.evidence().unwrap(), &[OsString::from(payload_name(1)), OsString::from(MANIFEST_PENDING)]);
}

#[test]
fn mutation_final_staged_hash_mid_chunk_cancel_preserves_evidence() { final_staged_cancel(true); }
#[test]
fn mutation_final_staged_hash_pre_cancel_reads_nothing() { final_staged_cancel(false); }

fn nofollow_identity(path: &Path) -> Option<(Identity, u64, u32)> {
    match fs::symlink_metadata(path) {
        Ok(m) => Some((Identity(m.dev(), m.ino(), m.len(), m.mode(), m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec()), m.nlink(), m.uid())),
        Err(e) => { assert_eq!(e.kind(), std::io::ErrorKind::NotFound); None },
    }
}

fn mutation_beside_damaged_unrelated(import: bool) {
    for damage in ["corrupt", "short", "missing", "directory", "symlink", "fifo", "mode", "hardlink"] {
        let mut f = FaultFixture::new();
        let bad = f.kept[1].clone();
        let bad_path = f.root.join(payload_name(bad.id));
        match damage {
            "corrupt" => { use std::os::unix::fs::FileExt; OpenOptions::new().write(true).open(&bad_path).unwrap().write_all_at(b"!", 0).unwrap(); },
            "short" => OpenOptions::new().write(true).open(&bad_path).unwrap().set_len(1).unwrap(),
            "mode" => fs::set_permissions(&bad_path, fs::Permissions::from_mode(0o644)).unwrap(),
            "hardlink" => fs::hard_link(&bad_path, f._owner.0.join("external-link")).unwrap(),
            _ => {
                fs::remove_file(&bad_path).unwrap();
                match damage {
                    "directory" => fs::create_dir(&bad_path).unwrap(),
                    "symlink" => std::os::unix::fs::symlink(&f.original, &bad_path).unwrap(),
                    "fifo" => {
                        let c = CString::new(bad_path.as_os_str().as_bytes()).unwrap();
                        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
                    },
                    "missing" => {},
                    _ => unreachable!(),
                }
            },
        }
        let bad_before = nofollow_identity(&bad_path);
        let bad_bytes = if matches!(damage, "corrupt" | "short" | "mode" | "hardlink") { Some(fs::read(&bad_path).unwrap()) } else { None };
        let mut expected = Manifest::decode(&f.manifest).unwrap();
        expected.generation += 1;
        take_body_reads();
        let read_identity;
        let read_bytes;
        if import {
            let item = f.store.import_document(OsStr::new("new healthy"), &f.reader, &AtomicBool::new(false)).unwrap().value;
            assert_eq!(item.id, expected.next);
            assert_eq!(item.bytes, f.bytes.len() as u64);
            assert_eq!(item.sha256, <[u8; 32]>::from(Sha256::digest(&f.bytes)));
            expected.next += 1;
            expected.items.push(item.clone());
            read_identity = fs::metadata(f.root.join(payload_name(item.id))).unwrap();
            read_bytes = item.bytes;
        } else {
            let selected = expected.items.remove(0);
            read_identity = fs::metadata(f.root.join(payload_name(selected.id))).unwrap();
            read_bytes = selected.bytes;
            assert!(f.store.remove_cancelled(selected.id, &AtomicBool::new(false)).unwrap().is_complete());
            assert!(!f.root.join(payload_name(selected.id)).exists());
        }
        let reads = take_body_reads();
        assert!(!reads.is_empty());
        assert!(reads.iter().all(|(dev, ino, _)| (*dev, *ino) == (read_identity.dev(), read_identity.ino())), "{damage}: mutation read unrelated bodies: {reads:?}");
        assert_eq!(reads.iter().map(|(_, _, n)| *n as u64).sum::<u64>(), read_bytes);
        assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), expected.encode().unwrap(), "{damage}: changed unrelated manifest metadata");
        assert_eq!(f.store.items(), expected.items);
        assert_eq!(nofollow_identity(&bad_path), bad_before, "{damage}: mutation changed unrelated identity");
        if let Some(bytes) = bad_bytes { assert_eq!(fs::read(&bad_path).unwrap(), bytes); }
        if damage == "symlink" { assert_eq!(fs::read_link(&bad_path).unwrap(), f.original); }
        if import {
            assert_eq!(identity(&f.root.join(payload_name(f.kept[0].id))), f.kept_identities[0]);
            assert_eq!(fs::read(f.root.join(payload_name(f.kept[0].id))).unwrap(), f.bytes);
        }
        assert_eq!(fs::read(&f.original).unwrap(), f.bytes);
        assert_eq!(fs::read(&f.room).unwrap(), f.bytes);
        assert_eq!(identity(&f.original), f.original_identity);
        assert_eq!(identity(&f.room), f.room_identity);
        let reopened = Store::open(&f.root, policy()).unwrap();
        assert_eq!(reopened.items(), expected.items);
        let inspected = reopened.inspect().unwrap();
        let status = &inspected.iter().find(|i| i.item.id == bad.id).unwrap().status;
        match damage {
            "corrupt" => assert_eq!(*status, PayloadStatus::Unchecked),
            "short" => assert!(matches!(status, PayloadStatus::Invalid(_))),
            _ => assert!(matches!(status, PayloadStatus::Unavailable(_))),
        }
        let mut untrusted = Vec::new();
        assert!(reopened.copy_to(bad.id, &mut untrusted, &AtomicBool::new(false)).unwrap_err().contains("Untrusted partial output"));
        if damage == "corrupt" { assert_ne!(untrusted, f.bytes); }
        else { assert!(untrusted.is_empty()); }
        // Preserving expected records is NOT fresh-confirm deletion authority.
        let committed = fs::read(f.root.join(MANIFEST)).unwrap();
        assert!(f.store.remove_cancelled(bad.id, &AtomicBool::new(false)).is_err());
        assert!(f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err());
        assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), committed);
        assert_eq!(nofollow_identity(&bad_path), bad_before);
        assert!(!f.root.join(MANIFEST_PENDING).exists());
    }
}

#[test]
fn mutation_import_beside_real_damaged_unrelated_preserves_records_without_trust() {
    mutation_beside_damaged_unrelated(true);
}

#[test]
fn mutation_remove_healthy_beside_real_damaged_unrelated_preserves_records_without_trust() {
    mutation_beside_damaged_unrelated(false);
}
