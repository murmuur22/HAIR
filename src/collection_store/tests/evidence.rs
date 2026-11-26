//! Ordinary writer failures leave real files; recovery must not activate them.
use super::*;
use crate::collection_store::recovery::RecoveryStage;
#[path = "evidence_safety.rs"]
mod safety;

fn complete(f: &mut FaultFixture, evidence: Vec<(String, Vec<u8>)>) {
    f.check(evidence.clone());
    let before = identity(&f.root);
    let ids: Vec<_> = evidence.iter().map(|(n, _)| identity(&f.root.join(n))).collect();
    let inspection = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    assert_eq!(identity(&f.root), before);
    drop(inspection);
    assert_eq!(identity(&f.root), before, "drop wrote");
    let mut cancelled = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    assert!(f.store.discard_interrupted_import(&mut cancelled, &AtomicBool::new(true)).unwrap_err().contains("cancelled"));
    drop(cancelled);
    assert_eq!(identity(&f.root), before, "cancel wrote");
    for ((name, bytes), id) in evidence.iter().zip(ids) {
        assert_eq!(identity(&f.root.join(name)), id);
        assert_eq!(fs::read(f.root.join(name)).unwrap(), *bytes);
    }
    let mut fresh = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    let outcome = f.store.discard_interrupted_import(&mut fresh, &AtomicBool::new(false)).unwrap();
    assert!(outcome.is_complete(), "{outcome:?}");
    assert!(outcome.error.is_none() && outcome.sync_warning.is_none(), "{outcome:?}");
    assert_eq!(outcome.remaining, Some(Vec::new()));
    assert_eq!(outcome.removed.len(), evidence.len());
    drop(fresh);
    f.check(Vec::new());
    let old = Manifest::decode(&f.manifest).unwrap();
    let item = f.store.import_document(OsStr::new("after-explicit-discard"), &f.reader, &AtomicBool::new(false)).unwrap().value;
    assert_eq!(item.id, old.next);
    assert!(f.store.remove(item.id).unwrap().is_complete());
    let now = Manifest::decode(&fs::read(f.root.join(MANIFEST)).unwrap()).unwrap();
    assert_eq!(now.generation, old.generation + 2);
    assert_eq!(now.next, old.next + 1);
    assert_eq!(now.items, old.items);
    f.manifest = fs::read(f.root.join(MANIFEST)).unwrap();
    f.manifest_identity = identity(&f.root.join(MANIFEST));
    f.check(Vec::new());
}

fn failed_manifest(f: &mut FaultFixture, prefix: usize) -> Vec<(String, Vec<u8>)> {
    let root = f.root.clone();
    let mut successor = Manifest::decode(&f.manifest).unwrap();
    let id = successor.next;
    successor.generation += 1;
    successor.next += 1;
    successor.items.push(Item { id, name: "failed".into(), bytes: f.bytes.len() as u64,
        category: Category::Document, sha256: Sha256::digest(&f.bytes).into() });
    let bytes = successor.encode().unwrap();
    let result = f.store.import_inner(OsStr::new("failed"), &f.reader, &AtomicBool::new(false), |stage, file| {
        if stage == ImportStage::ManifestStaging {
            // Real prefix writes followed by a real same-inode read-only FD.
            file.write_all(&bytes[..prefix]).unwrap();
            *file = File::open(root.join(MANIFEST_PENDING)).unwrap();
        }
        Ok(())
    });
    match result {
        Err(Failure::Refused(error)) => {
            assert!(error.contains("Manifest write failed"), "{error}");
            assert!(error.contains(&std::io::Error::from_raw_os_error(libc::EBADF).to_string()), "{error}");
            println!("EVIDENCE_ACTUAL_MANIFEST_WRITE prefix={prefix}: {error}");
        },
        other => panic!("expected actual EBADF: {other:?}"),
    }
    vec![(f.orphan(), f.bytes.clone()), (MANIFEST_PENDING.into(), bytes[..prefix].to_vec())]
}

#[test]
fn evidence_ordinary_import_manifest_ebadf_empty_and_partial() {
    for prefix in [0, 17, 80] {
        let mut f = FaultFixture::new();
        let evidence = failed_manifest(&mut f, prefix);
        complete(&mut f, evidence);
    }
}

#[test]
fn evidence_ordinary_import_published_manifest_create_eacces_lone_next() {
    let mut f = FaultFixture::new();
    let root = f.root.clone();
    let restore = RestoreMode(root.clone());
    let result = f.store.import_inner(OsStr::new("published"), &f.reader, &AtomicBool::new(false), |stage, _| {
        if stage == ImportStage::PayloadPublished { fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap(); }
        Ok(())
    });
    drop(restore);
    match result {
        Err(Failure::Refused(error)) => {
            assert!(error.contains(&std::io::Error::from_raw_os_error(libc::EACCES).to_string()), "{error}");
            println!("EVIDENCE_ACTUAL_MANIFEST_CREATE: {error}");
        },
        other => panic!("expected actual EACCES: {other:?}"),
    }
    let evidence = vec![(f.orphan(), f.bytes.clone())];
    complete(&mut f, evidence);
}

#[test]
fn evidence_ordinary_remove_staged_finder_and_manifest_guard_failure() {
    for case in ["finder", "manifest"] {
        let mut f = FaultFixture::new();
        if case == "finder" { create(&f.store.root, FINDER).unwrap().write_all(b"before").unwrap(); }
        let root = f.root.clone();
        let result = f.store.remove_inner(f.kept[0].id, &AtomicBool::new(false), |stage, _, _| {
            if stage == RemoveStage::ManifestReady {
                if case == "finder" { fs::write(root.join(FINDER), b"after").unwrap(); }
                else {
                    // Byte-identical active manifest rebind must still refuse.
                    fs::rename(root.join(MANIFEST), root.parent().unwrap().join("saved-manifest")).unwrap();
                    OpenOptions::new().write(true).create_new(true).mode(0o600).open(root.join(MANIFEST)).unwrap()
                        .write_all(&f.manifest).unwrap();
                }
            }
            Ok(())
        });
        match result {
            Err(Failure::Refused(error)) => {
                assert!(error.contains(if case == "finder" { "Finder metadata changed" } else { "identity/name rebound" }), "{error}");
                println!("EVIDENCE_ACTUAL_REMOVE_GUARD {case}: {error}");
            },
            other => panic!("guard failed to refuse: {other:?}"),
        }
        if case == "manifest" { f.manifest_identity = identity(&root.join(MANIFEST)); }
        let finder_before = if case == "finder" { Some(identity(&root.join(FINDER))) } else { None };
        let pending = fs::read(root.join(MANIFEST_PENDING)).unwrap();
        let m = Manifest::decode(&pending).unwrap();
        let live = Manifest::decode(&f.manifest).unwrap();
        assert_eq!(m.generation, live.generation + 1);
        assert_eq!(m.next, live.next);
        assert_eq!(m.items, f.kept[1..]);
        complete(&mut f, vec![(MANIFEST_PENDING.into(), pending)]);
        if let Some(before) = finder_before {
            assert_eq!(identity(&root.join(FINDER)), before);
            assert_eq!(fs::read(root.join(FINDER)).unwrap(), b"after");
        }
    }
}

#[test]
fn evidence_ordinary_remove_cleanup_eacces_leftover_nonlive_id() {
    let mut f = FaultFixture::new();
    let root = f.root.clone();
    let restore = RestoreMode(root.clone());
    let selected = f.kept[0].id;
    let result = f.store.remove_inner(selected, &AtomicBool::new(false), |stage, _, _| {
        if stage == RemoveStage::CleanupReady { fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap(); }
        Ok(())
    }).unwrap();
    drop(restore);
    assert!(!result.is_complete());
    assert!(!result.payload_removed);
    assert!(result.sync_warning.is_none());
    let error = result.cleanup_error.unwrap();
    assert!(error.contains(&std::io::Error::from_raw_os_error(libc::EACCES).to_string()), "{error}");
    println!("EVIDENCE_ACTUAL_REMOVE_CLEANUP: {error}");
    let old = Manifest::decode(&f.manifest).unwrap();
    let now = Manifest::decode(&fs::read(root.join(MANIFEST)).unwrap()).unwrap();
    assert_eq!(now.generation, old.generation + 1);
    assert_eq!(now.next, old.next);
    assert_eq!(now.items, f.kept[1..]);
    f.kept.remove(0);
    f.kept_identities.remove(0);
    f.manifest = fs::read(root.join(MANIFEST)).unwrap();
    f.manifest_identity = identity(&root.join(MANIFEST));
    let evidence = vec![(payload_name(selected), f.bytes.clone())];
    complete(&mut f, evidence);
}

#[test]
fn evidence_partial_import_manifest_cleanup_requires_fresh_pending_only_inspection() {
    for prefix in [0, 17, 80] {
        let mut f = FaultFixture::new();
        let evidence = failed_manifest(&mut f, prefix);
        let pending_before = identity(&f.root.join(MANIFEST_PENDING));
        let mut inspected = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
        let cancelled = AtomicBool::new(false);
        let outcome = f.store.discard_recovery_inner(&mut inspected, &cancelled, |stage, _| {
            if stage == RecoveryStage::BeforeUnlink(1) { cancelled.store(true, Ordering::Relaxed); }
            Ok(())
        }).unwrap();
        assert_eq!(outcome.removed, [OsString::from(f.orphan())]);
        assert!(!outcome.is_complete());
        assert!(outcome.error.as_ref().unwrap().contains("cancelled"));
        assert_eq!(outcome.remaining, Some(vec![OsString::from(MANIFEST_PENDING)]));
        assert_eq!(identity(&f.root.join(MANIFEST_PENDING)), pending_before);
        assert!(f.store.discard_interrupted_import(&mut inspected, &AtomicBool::new(false)).unwrap_err().contains("consumed"));
        drop(inspected);
        complete(&mut f, vec![evidence[1].clone()]);
    }
}
