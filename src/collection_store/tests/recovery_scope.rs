//! H1/H2: real interrupted mutations; recovery never repairs or trusts LIVE bodies.
use super::*;
use crate::collection_store::recovery::{RecoveryStage, identify_cancelled};

fn interrupt_payload(f: &mut FaultFixture) {
    let cancelled = AtomicBool::new(false);
    refused(f.store.import_inner(OsStr::new("interrupted"), &f.reader, &cancelled, |stage, _| {
        if stage == ImportStage::ChunkWritten { cancelled.store(true, Ordering::Relaxed); }
        Ok(())
    }), "cancelled");
}

fn recover(f: &mut FaultFixture, candidate: &str) {
    let path = f.root.join(candidate);
    let bytes = fs::read(&path).unwrap();
    let before = identity(&path);
    let root = identity(&f.root);
    let manifest = fs::read(f.root.join(MANIFEST)).unwrap();
    let manifest_id = identity(&f.root.join(MANIFEST));
    let lock = identity(&f.root.join("lock"));
    let live: Vec<_> = f.kept.iter().map(|i| {
        let path = f.root.join(payload_name(i.id));
        (selected_state(&path), fs::read(&path).ok())
    }).collect();
    take_body_reads();
    let cap = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    assert_eq!(cap.candidates(), &[OsString::from(candidate)]);
    drop(cap);
    assert_eq!(identity(&f.root), root);
    let mut cap = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    assert!(f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(true)).is_err());
    assert!(f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(false)).is_err());
    drop(cap);
    assert_eq!(identity(&f.root), root);
    assert_eq!(identity(&path), before);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let mut cap = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    assert!(f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(false)).unwrap().is_complete());
    drop(cap);
    let reads = take_body_reads();
    assert!(reads.iter().all(|(d, i, _)| (*d, *i) == (before.0, before.1)), "LIVE body read: {reads:?}");
    if candidate != MANIFEST_PENDING {
        assert_eq!(reads.iter().map(|(_, _, n)| *n as u64).sum::<u64>(), 5 * bytes.len() as u64);
    }
    for (item, (state, bytes)) in f.kept.iter().zip(live) {
        let path = f.root.join(payload_name(item.id));
        assert_eq!(selected_state(&path), state);
        assert_eq!(fs::read(path).ok(), bytes);
    }
    assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), manifest);
    assert_eq!(identity(&f.root.join(MANIFEST)), manifest_id);
    assert_eq!(identity(&f.root.join("lock")), lock);
    assert!(f.store.evidence().unwrap().is_empty());
    originals(f);
}

fn mutate(f: &mut FaultFixture) {
    f.store = Store::open(&f.root, f.store.policy).unwrap();
    let fresh = f.store.import_document(OsStr::new("after recovery"), &f.reader, &AtomicBool::new(false)).unwrap().value;
    assert!(f.store.remove(fresh.id).unwrap().is_complete());
    originals(f);
}

#[test]
fn recovery_scope_damaged_precommit_cancel_and_kernel_ebadf_then_fresh_removal() {
    for kind in ["missing", "overwrite", "truncate"] {
        for fault in ["cancel", "write-empty", "write-prefix"] {
            let mut f = FaultFixture::new();
            damage(&f, kind);
            let mut cap = inspect(&f);
            let root = f.root.clone();
            let cancelled = AtomicBool::new(false);
            let result = f.store.remove_damaged_inner(&mut cap, &cancelled, |stage, _, file| {
                if stage == RemoveStage::ManifestStaging && fault.starts_with("write") {
                    if fault == "write-prefix" { file.write_all(b"HAIRCOLL").unwrap(); }
                    *file = File::open(root.join(MANIFEST_PENDING)).unwrap();
                }
                if stage == RemoveStage::ManifestReady && fault == "cancel" { cancelled.store(true, Ordering::Relaxed); }
                Ok(())
            });
            let Err(Failure::Refused(error)) = result else { panic!("expected precommit refusal"); };
            if fault.starts_with("write") {
                assert!(error.contains(&std::io::Error::from_raw_os_error(libc::EBADF).to_string()), "{error}");
            } else { assert!(error.contains("cancelled")); }
            drop(cap);
            assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), f.manifest);
            assert!(f.store.inspect_damaged_item(&f.kept[0], f.bytes.len() as u64, &AtomicBool::new(false)).is_err());
            recover(&mut f, MANIFEST_PENDING);
            let mut fresh = inspect(&f);
            assert!(f.store.remove_damaged_item(&mut fresh, &AtomicBool::new(false)).unwrap().is_complete());
            drop(fresh);
            assert!(!root.join(payload_name(f.kept[0].id)).exists());
            let mut expected = Manifest::decode(&f.manifest).unwrap();
            expected.generation += 1; expected.items.remove(0);
            assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), expected.encode().unwrap());
            mutate(&mut f);
            println!("RECOVERY_SCOPE_H1 {kind}/{fault}: pending-only discard, no repair, fresh damaged removal and reopen/mutate");
        }
    }
}

#[test]
fn recovery_scope_healthy_removal_pending_with_latent_unrelated_corruption() {
    let mut f = FaultFixture::new();
    damage(&f, "overwrite");
    let cancelled = AtomicBool::new(false);
    assert!(f.store.remove_inner(f.kept[1].id, &cancelled, |stage, _, _| {
        if stage == RemoveStage::ManifestReady { cancelled.store(true, Ordering::Relaxed); }
        Ok(())
    }).is_err());
    recover(&mut f, MANIFEST_PENDING);
    assert!(f.store.remove(f.kept[0].id).is_err(), "recovery granted content trust");
    assert!(f.store.remove(f.kept[1].id).unwrap().is_complete());
    mutate(&mut f);
}

#[test]
fn recovery_scope_damaged_postcommit_orphan_with_unrelated_live_damage() {
    let mut f = FaultFixture::new();
    damage(&f, "overwrite");
    let other = f.root.join(payload_name(f.kept[1].id));
    OpenOptions::new().write(true).open(&other).unwrap().write_all_at(b"?", 0).unwrap();
    let before = identity(&other); let bytes = fs::read(&other).unwrap();
    let mut cap = inspect(&f); let root = f.root.clone(); let restore = RestoreMode(root.clone());
    let outcome = f.store.remove_damaged_inner(&mut cap, &AtomicBool::new(false), |stage, _, _| {
        if stage == RemoveStage::CleanupReady { fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap(); }
        Ok(())
    }).unwrap();
    drop(restore); drop(cap);
    assert!(outcome.cleanup_error.unwrap().contains(&std::io::Error::from_raw_os_error(libc::EACCES).to_string()));
    let candidate = payload_name(f.kept[0].id);
    let mut recovery = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    assert_eq!(recovery.candidates(), &[OsString::from(&candidate)]);
    take_body_reads();
    assert!(f.store.discard_interrupted_import(&mut recovery, &AtomicBool::new(false)).unwrap().is_complete());
    drop(recovery);
    let reads = take_body_reads();
    assert!(!reads.is_empty());
    assert!(reads.iter().all(|(d, i, _)| (*d, *i) != (before.0, before.1)));
    assert_eq!(identity(&other), before); assert_eq!(fs::read(other).unwrap(), bytes);
    mutate(&mut f);
}

#[test]
fn recovery_scope_aggregate_live_over_64mib_hashes_only_candidate() {
    let mut f = FaultFixture::new();
    let length = 33 * 1024 * 1024;
    for item in &f.kept {
        OpenOptions::new().write(true).open(f.root.join(payload_name(item.id))).unwrap().set_len(length).unwrap();
    }
    // Both actual sparse bodies have identical original prefixes and zero tails.
    // Hash one once for real expected metadata, never repeatedly hash the aggregate.
    let file = regular(&f.store.root, &payload_name(f.kept[0].id)).unwrap();
    let (actual, _) = identify_cancelled(&file, &mut { length }, false, true, &AtomicBool::new(false)).unwrap();
    let mut manifest = Manifest::decode(&f.manifest).unwrap();
    for item in &mut manifest.items { item.bytes = length; item.sha256 = actual.digest; }
    fs::write(f.root.join(MANIFEST), manifest.encode().unwrap()).unwrap();
    f.store = Store::open(&f.root, Policy::new(length, 128 * 1024 * 1024).unwrap()).unwrap();
    assert!(f.store.usage() > 64 * 1024 * 1024);
    let live: Vec<_> = f.kept.iter().map(|i| identity(&f.root.join(payload_name(i.id)))).collect();
    interrupt_payload(&mut f);
    let manifest_before = fs::read(f.root.join(MANIFEST)).unwrap();
    let pending = fs::metadata(f.root.join(PAYLOAD_PENDING)).unwrap();
    take_body_reads();
    let mut cap = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    assert!(f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(false)).unwrap().is_complete());
    drop(cap);
    let reads = take_body_reads();
    assert!(!reads.is_empty());
    assert!(reads.iter().all(|(d, i, _)| (*d, *i) == (pending.dev(), pending.ino())), "LIVE reads: {reads:?}");
    assert_eq!(reads.iter().map(|(_, _, n)| *n as u64).sum::<u64>(), 3 * pending.len());
    assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), manifest_before);
    for (item, before) in f.kept.iter().zip(live) { assert_eq!(identity(&f.root.join(payload_name(item.id))), before); }
    mutate(&mut f);
    println!("RECOVERY_SCOPE_H2: actual 66MiB LIVE, candidate-only real reads; reopen/mutate");
}

#[test]
fn recovery_scope_live_observation_stale_and_unsafe_refuse() {
    for kind in ["missing-present", "present-missing", "repaired", "rebind", "mode", "symlink", "fifo", "hardlink", "directory"] {
        let mut f = FaultFixture::new();
        damage(&f, if kind == "missing-present" { "missing" } else { "overwrite" });
        interrupt_payload(&mut f);
        let path = f.root.join(payload_name(f.kept[0].id));
        let mut cap = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
        match kind {
            "missing-present" => { create(&f.store.root, &payload_name(f.kept[0].id)).unwrap().write_all(&f.bytes).unwrap(); },
            "present-missing" => fs::remove_file(&path).unwrap(),
            "repaired" => fs::write(&path, &f.bytes).unwrap(),
            "rebind" => { fs::rename(&path, f._owner.0.join("saved-live")).unwrap(); create(&f.store.root, &payload_name(f.kept[0].id)).unwrap().write_all(&f.bytes).unwrap(); },
            "mode" => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
            "hardlink" => fs::hard_link(&path, f._owner.0.join("external-link")).unwrap(),
            _ => {
                fs::remove_file(&path).unwrap();
                match kind {
                    "symlink" => std::os::unix::fs::symlink(&f.original, &path).unwrap(),
                    "directory" => fs::create_dir(&path).unwrap(),
                    "fifo" => { let p = CString::new(path.as_os_str().as_bytes()).unwrap(); assert_eq!(unsafe { libc::mkfifo(p.as_ptr(), 0o600) }, 0); },
                    _ => unreachable!(),
                }
            },
        }
        let root = identity(&f.root); let state = selected_state(&path);
        let pending = identity(&f.root.join(PAYLOAD_PENDING));
        assert!(f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(false)).is_err(), "{kind}");
        drop(cap);
        assert_eq!(identity(&f.root), root); assert_eq!(selected_state(&path), state);
        assert_eq!(identity(&f.root.join(PAYLOAD_PENDING)), pending);
        if matches!(kind, "mode" | "symlink" | "fifo" | "hardlink" | "directory") {
            assert!(f.store.interrupted_import_metadata(&AtomicBool::new(false)).is_err(), "reported unsafe LIVE {kind}");
            assert!(f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err(), "unsafe LIVE {kind}");
        }
        originals(&f);
    }
}

#[test]
fn recovery_scope_live_change_before_unlink_preserves_candidate_and_active_name() {
    for missing in [false, true] {
        let mut f = FaultFixture::new(); damage(&f, if missing { "missing" } else { "truncate" });
        interrupt_payload(&mut f);
        let mut cap = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
        let path = f.root.join(payload_name(f.kept[0].id));
        let candidate = identity(&f.root.join(PAYLOAD_PENDING));
        let outcome = f.store.discard_recovery_inner(&mut cap, &AtomicBool::new(false), |stage, store| {
            if stage == RecoveryStage::BeforeUnlink(0) {
                if missing { create(&store.root, &payload_name(f.kept[0].id)).unwrap().write_all(b"replacement").unwrap(); }
                else { OpenOptions::new().write(true).open(&path).unwrap().write_all_at(b"?", 0).unwrap(); }
            }
            Ok(())
        }).unwrap();
        assert!(outcome.removed.is_empty()); assert!(outcome.error.is_some());
        assert_eq!(outcome.remaining, Some(vec![OsString::from(PAYLOAD_PENDING)]));
        assert_eq!(identity(&f.root.join(PAYLOAD_PENDING)), candidate);
        assert!(path.exists()); originals(&f);
    }
}
