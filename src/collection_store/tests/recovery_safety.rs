use super::*;
use crate::collection_store::recovery::RecoveryStage;

#[test]
fn recovery_capability_holds_lock_and_drop_is_write_free() {
    let mut f = FaultFixture::new();
    let evidence = interrupt(&mut f, false);
    let before = identity(&f.root);
    let mut other = Store::open(&f.root, policy()).unwrap();
    let inspection = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    assert_eq!(inspection.committed_generation(), Manifest::decode(&f.manifest).unwrap().generation);
    assert!(other.refresh().unwrap_err().contains("busy"));
    assert!(other.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err());
    drop(inspection);
    assert_eq!(identity(&f.root), before);
    f.check(evidence);
}

#[test]
fn recovery_stale_snapshots_refuse_all_changes_without_unlink() {
    for case in ["candidate-content", "candidate-inode", "pending-content", "manifest-inode", "manifest-generation",
        "live-inode", "lock-inode", "lock-content", "root-rebind", "new-unknown", "transient-unknown"] {
        let mut f = FaultFixture::new();
        let evidence = interrupt(&mut f, true);
        let mut inspection = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
        let orphan = f.root.join(f.orphan());
        let pending = f.root.join(MANIFEST_PENDING);
        match case {
            "candidate-content" => { OpenOptions::new().write(true).open(&orphan).unwrap().write_all(b"changed").unwrap(); },
            "candidate-inode" => {
                fs::rename(&orphan, f._owner.0.join("saved-orphan")).unwrap();
                create(&f.store.root, &f.orphan()).unwrap().write_all(&f.bytes).unwrap();
            },
            "pending-content" => {
                let mut m = Manifest::decode(&fs::read(&pending).unwrap()).unwrap();
                m.items.last_mut().unwrap().name = OsString::from("new-label");
                fs::write(&pending, m.encode().unwrap()).unwrap();
            },
            "manifest-inode" | "live-inode" | "lock-inode" => {
                let name = match case { "manifest-inode" => MANIFEST.to_string(), "lock-inode" => "lock".into(), _ => payload_name(f.kept[0].id) };
                let path = f.root.join(&name);
                let bytes = fs::read(&path).unwrap();
                fs::rename(&path, f._owner.0.join("saved-active")).unwrap();
                create(&f.store.root, &name).unwrap().write_all(&bytes).unwrap();
            },
            "manifest-generation" => {
                let mut m = Manifest::decode(&f.manifest).unwrap();
                m.generation += 1;
                fs::write(f.root.join(MANIFEST), m.encode().unwrap()).unwrap();
            },
            "lock-content" => { OpenOptions::new().write(true).open(f.root.join("lock")).unwrap().write_all(b"changed").unwrap(); },
            "root-rebind" => {
                fs::rename(&f.root, f._owner.0.join("saved-root")).unwrap();
                fs::create_dir(&f.root).unwrap();
                fs::set_permissions(&f.root, fs::Permissions::from_mode(0o700)).unwrap();
            },
            "new-unknown" | "transient-unknown" => {
                create(&f.store.root, "foreign-evidence").unwrap().write_all(b"unknown").unwrap();
                if case == "transient-unknown" { fs::remove_file(f.root.join("foreign-evidence")).unwrap(); }
            },
            _ => unreachable!(),
        }
        let held_root = if case == "root-rebind" { f._owner.0.join("saved-root") } else { f.root.clone() };
        let candidate_before: Vec<_> = evidence.iter().map(|(n, _)| (fs::read(held_root.join(n)).unwrap(), identity(&held_root.join(n)))).collect();
        let manifest_before = fs::read(held_root.join(MANIFEST)).unwrap();
        let root_before = identity(&held_root);
        let error = f.store.discard_interrupted_import(&mut inspection, &AtomicBool::new(false)).unwrap_err();
        assert!(!error.is_empty());
        assert!(f.store.discard_interrupted_import(&mut inspection, &AtomicBool::new(false)).unwrap_err().contains("consumed"));
        drop(inspection);
        assert_eq!(identity(&held_root), root_before, "wrote on stale {case}");
        for ((name, _), (bytes, id)) in evidence.iter().zip(candidate_before) {
            assert_eq!(fs::read(held_root.join(name)).unwrap(), bytes, "{case}");
            assert_eq!(identity(&held_root.join(name)), id, "{case}");
        }
        assert_eq!(fs::read(held_root.join(MANIFEST)).unwrap(), manifest_before);
        assert_eq!(identity(&f.original), f.original_identity);
        assert_eq!(identity(&f.room), f.room_identity);
        println!("RECOVERY_STALE {case}: refused, no unlink, consumed capability");
    }
}

#[test]
fn recovery_unknown_foreign_symlink_nonregular_and_namespace_negatives() {
    use std::os::unix::fs::symlink;
    for case in ["unknown", "symlink", "directory", "fifo", "hardlink", "foreign-mode", "lone-future-generated", "wrong-generated-name",
        "same-committed-manifest", "altered-live-prefix", "wrong-generation", "wrong-next", "removal-manifest", "partial-manifest"] {
        let mut f = FaultFixture::new();
        let late = !matches!(case, "unknown" | "symlink" | "directory" | "fifo" | "hardlink" | "foreign-mode");
        interrupt(&mut f, late);
        let pending = f.root.join(if late { MANIFEST_PENDING } else { PAYLOAD_PENDING });
        match case {
            "unknown" => { create(&f.store.root, "unknown-evidence").unwrap().write_all(b"foreign").unwrap(); },
            "symlink" | "directory" | "fifo" | "hardlink" => {
                fs::remove_file(&pending).unwrap();
                match case {
                    "symlink" => symlink(&f.original, &pending).unwrap(),
                    "directory" => { fs::create_dir(&pending).unwrap(); fs::set_permissions(&pending, fs::Permissions::from_mode(0o700)).unwrap(); },
                    "hardlink" => fs::hard_link(&f.original, &pending).unwrap(),
                    "fifo" => {
                        let path = CString::new(pending.as_os_str().as_bytes()).unwrap();
                        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
                    },
                    _ => unreachable!(),
                }
            },
            "foreign-mode" => fs::set_permissions(&pending, fs::Permissions::from_mode(0o644)).unwrap(),
            "lone-future-generated" => {
                fs::remove_file(&pending).unwrap();
                fs::rename(f.root.join(f.orphan()), f.root.join("item-00000000000000ff")).unwrap();
            },
            "wrong-generated-name" => fs::rename(f.root.join(f.orphan()), f.root.join("item-00000000000000ff")).unwrap(),
            "same-committed-manifest" => fs::write(&pending, &f.manifest).unwrap(),
            "partial-manifest" => fs::write(&pending, b"FOREIGN").unwrap(),
            _ => {
                let mut m = Manifest::decode(&fs::read(&pending).unwrap()).unwrap();
                match case {
                    "altered-live-prefix" => m.items[0].name = OsString::from("rewritten-live"),
                    "wrong-generation" => m.generation += 1,
                    "wrong-next" => m.next += 1,
                    "removal-manifest" => { m.items = f.kept[1..].to_vec(); m.next = Manifest::decode(&f.manifest).unwrap().next; },
                    _ => unreachable!(),
                }
                fs::write(&pending, m.encode().unwrap()).unwrap();
            },
        }
        let root_before = identity(&f.root);
        let manifest_before = identity(&f.root.join(MANIFEST));
        let meta_before = fs::symlink_metadata(&pending).ok().map(|m| (m.dev(), m.ino(), m.mode(), m.len(), m.ctime(), m.ctime_nsec()));
        let original_before = identity(&f.original);
        take_body_reads();
        assert!(f.store.interrupted_import_metadata(&AtomicBool::new(false)).is_err(), "reported unsupported {case}");
        assert!(take_body_reads().is_empty());
        assert!(f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err(), "accepted {case}");
        assert_eq!(identity(&f.root), root_before, "inspection wrote {case}");
        assert_eq!(identity(&f.root.join(MANIFEST)), manifest_before);
        assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), f.manifest);
        assert_eq!(fs::symlink_metadata(&pending).ok().map(|m| (m.dev(), m.ino(), m.mode(), m.len(), m.ctime(), m.ctime_nsec())), meta_before);
        assert_eq!(identity(&f.original), original_before);
        assert_eq!(fs::read(&f.original).unwrap(), f.bytes);
        assert_eq!(identity(&f.room), f.room_identity);
        for (item, id) in f.kept.iter().zip(&f.kept_identities) { assert_eq!(&identity(&f.root.join(payload_name(item.id))), id); }
        println!("RECOVERY_NEGATIVE {case}: refused without namespace/active/original/room change");
    }
}

#[test]
fn recovery_activation_ambiguity_never_discards_committed_payload() {
    for pending_again in [false, true] {
        let mut f = FaultFixture::new();
        interrupt(&mut f, true);
        let pending_bytes = fs::read(f.root.join(MANIFEST_PENDING)).unwrap();
        let candidate = Manifest::decode(&pending_bytes).unwrap();
        let new_id = candidate.items.last().unwrap().id;
        // Actual rename models already-activated state, not a fabricated lost
        // syscall response or power-loss test.
        rename(&f.store.root, MANIFEST_PENDING, MANIFEST, false).unwrap();
        if pending_again { create(&f.store.root, MANIFEST_PENDING).unwrap().write_all(&pending_bytes).unwrap(); }
        let payload_before = identity(&f.root.join(payload_name(new_id)));
        let manifest_before = identity(&f.root.join(MANIFEST));
        let root_before = identity(&f.root);
        assert!(f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err());
        assert_eq!(identity(&f.root), root_before);
        assert_eq!(identity(&f.root.join(MANIFEST)), manifest_before);
        assert_eq!(identity(&f.root.join(payload_name(new_id))), payload_before);
        assert_eq!(fs::read(f.root.join(payload_name(new_id))).unwrap(), f.bytes);
        let reopened = Store::open(&f.root, policy()).unwrap();
        assert_eq!(reopened.items(), candidate.items);
        let mut exported = Vec::new();
        reopened.copy_to(new_id, &mut exported, &AtomicBool::new(false)).unwrap();
        assert_eq!(exported, f.bytes);
    }
}

#[test]
fn recovery_partial_cleanup_reports_exact_progress_preserves_remaining_and_requires_reinspect() {
    for case in ["cancel", "root-permission", "new-unknown", "pending-rebind"] {
        let mut f = FaultFixture::new();
        let evidence = interrupt(&mut f, true);
        let pending_bytes = evidence.iter().find(|(n, _)| n == MANIFEST_PENDING).unwrap().1.clone();
        let pending_before = identity(&f.root.join(MANIFEST_PENDING));
        let mut inspection = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
        let cancelled = AtomicBool::new(false);
        let root = f.root.clone();
        let orphan_name = f.orphan();
        let outcome = f.store.discard_recovery_inner(&mut inspection, &cancelled, |stage, store| {
            let inject_at = if case == "root-permission" { RecoveryStage::UnlinkReady(1) } else { RecoveryStage::BeforeUnlink(1) };
            if stage == inject_at {
                assert!(!root.join(&orphan_name).exists(), "first unlink must really occur");
                match case {
                    "cancel" => cancelled.store(true, Ordering::Relaxed),
                    "root-permission" => fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap(),
                    "new-unknown" => { create(&store.root, "unknown-during-discard").unwrap().write_all(b"untouched").unwrap(); },
                    "pending-rebind" => {
                        fs::rename(root.join(MANIFEST_PENDING), root.parent().unwrap().join("saved-pending")).unwrap();
                        create(&store.root, MANIFEST_PENDING).unwrap().write_all(&pending_bytes).unwrap();
                    },
                    _ => unreachable!(),
                }
            }
            Ok(())
        }).unwrap();
        if case == "root-permission" { fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap(); }
        assert_eq!(outcome.removed, [OsString::from(f.orphan())]);
        assert!(outcome.error.is_some(), "partial cleanup called success: {outcome:?}");
        if case == "root-permission" {
            assert!(outcome.error.as_ref().unwrap().contains(&std::io::Error::from_raw_os_error(libc::EACCES).to_string()), "not kernel EACCES: {outcome:?}");
        }
        assert!(outcome.sync_warning.is_none(), "{outcome:?}");
        let mut expected = vec![OsString::from(MANIFEST_PENDING)];
        if case == "new-unknown" { expected.push(OsString::from("unknown-during-discard")); }
        assert_eq!(outcome.remaining, Some(expected));
        assert_eq!(fs::read(root.join(MANIFEST_PENDING)).unwrap(), pending_bytes);
        if case != "pending-rebind" { assert_eq!(identity(&root.join(MANIFEST_PENDING)), pending_before); }
        assert!(f.store.discard_interrupted_import(&mut inspection, &AtomicBool::new(false)).unwrap_err().contains("consumed"));
        drop(inspection);
        if case == "new-unknown" {
            assert!(f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err());
            f.check(vec![(MANIFEST_PENDING.into(), pending_bytes), ("unknown-during-discard".into(), b"untouched".to_vec())]);
        } else {
            let mut fresh = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
            assert_eq!(fresh.candidates(), &[OsString::from(MANIFEST_PENDING)]);
            let final_outcome = f.store.discard_interrupted_import(&mut fresh, &AtomicBool::new(false)).unwrap();
            assert!(final_outcome.error.is_none() && final_outcome.sync_warning.is_none());
            drop(fresh);
            f.check(Vec::new());
        }
        println!("RECOVERY_PARTIAL {case}: {outcome:?}; no rollback, stale retry refused");
    }
}

#[test]
fn recovery_bounded_candidate_and_metadata_inspection_refuse_write_free() {
    for case in ["candidate-budget", "manifest-budget", "lock-budget"] {
        let mut f = FaultFixture::new();
        interrupt(&mut f, false);
        match case {
            "candidate-budget" => OpenOptions::new().write(true).open(f.root.join(PAYLOAD_PENDING)).unwrap().set_len(64 * 1024 * 1024 + 1).unwrap(),
            "manifest-budget" => OpenOptions::new().write(true).open(f.root.join(MANIFEST)).unwrap().set_len(MANIFEST_BYTES + 1).unwrap(),
            "lock-budget" => OpenOptions::new().write(true).open(f.root.join("lock")).unwrap().set_len(MANIFEST_BYTES + 1).unwrap(),
            _ => unreachable!(),
        }
        let root_before = identity(&f.root);
        let manifest_before = identity(&f.root.join(MANIFEST));
        let pending_before = identity(&f.root.join(PAYLOAD_PENDING));
        assert!(f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err(), "accepted {case}");
        assert_eq!(identity(&f.root), root_before);
        assert_eq!(identity(&f.root.join(MANIFEST)), manifest_before);
        assert_eq!(identity(&f.root.join(PAYLOAD_PENDING)), pending_before);
        assert_eq!(identity(&f.original), f.original_identity);
        assert_eq!(identity(&f.room), f.room_identity);
    }
}

#[test]
fn recovery_corrupt_uncommitted_payload_can_be_discarded_without_promoting_it() {
    let mut f = FaultFixture::new();
    interrupt(&mut f, true);
    OpenOptions::new().write(true).open(f.root.join(f.orphan())).unwrap().write_all(b"bad-staged-output").unwrap();
    let mut inspection = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    let outcome = f.store.discard_interrupted_import(&mut inspection, &AtomicBool::new(false)).unwrap();
    assert!(outcome.error.is_none() && outcome.sync_warning.is_none());
    drop(inspection);
    f.check(Vec::new());
}

#[test]
fn recovery_cannot_cross_roots_or_discard_after_activation() {
    let mut f = FaultFixture::new();
    interrupt(&mut f, true);
    let other_root = f._owner.root("other-cache");
    let mut other = Store::open(&other_root, policy()).unwrap();
    let other_manifest = fs::read(other_root.join(MANIFEST)).unwrap();
    let mut inspection = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    assert!(other.discard_interrupted_import(&mut inspection, &AtomicBool::new(false)).is_err());
    drop(inspection);
    assert_eq!(fs::read(other_root.join(MANIFEST)).unwrap(), other_manifest);
    let mut inspection = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    let pending_bytes = fs::read(f.root.join(MANIFEST_PENDING)).unwrap();
    let candidate = Manifest::decode(&pending_bytes).unwrap();
    rename(&f.store.root, MANIFEST_PENDING, MANIFEST, false).unwrap();
    let payload_before = identity(&f.root.join(f.orphan()));
    assert!(f.store.discard_interrupted_import(&mut inspection, &AtomicBool::new(false)).is_err());
    drop(inspection);
    assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), pending_bytes);
    assert_eq!(identity(&f.root.join(f.orphan())), payload_before);
    assert_eq!(Store::open(&f.root, policy()).unwrap().items(), candidate.items);
}

#[test]
fn recovery_real_sync_descriptor_failure_reports_unlinks_not_rollback() {
    let mut f = FaultFixture::new();
    interrupt(&mut f, true);
    let mut inspection = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    let held_root = f.store.root.try_clone().unwrap();
    let mut injected = false;
    let outcome = f.store.discard_recovery_inner(&mut inspection, &AtomicBool::new(false), |stage, store| {
        if stage == RecoveryStage::BeforeSync {
            let mut fds = [-1; 2];
            // SAFETY: two output slots, unique owned real pipe descriptors.
            assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
            let read = unsafe { File::from_raw_fd(fds[0]) };
            let write = unsafe { File::from_raw_fd(fds[1]) };
            drop(write);
            // Real fsync on a pipe must fail, followed by real directory-read
            // failure. Descriptor-access fault, NOT power-loss/disk durability.
            store.root = read;
            injected = true;
        }
        Ok(())
    }).unwrap();
    f.store.root = held_root;
    assert!(injected);
    assert_eq!(outcome.removed, [OsString::from(f.orphan()), OsString::from(MANIFEST_PENDING)]);
    assert!(outcome.sync_warning.is_some(), "{outcome:?}");
    assert!(outcome.error.as_ref().unwrap().contains("remaining evidence inspection failed"));
    assert_eq!(outcome.remaining, None, "cannot claim known remaining evidence after inspection failure");
    assert_eq!(f.store.evidence(), None);
    assert!(f.store.discard_interrupted_import(&mut inspection, &AtomicBool::new(false)).unwrap_err().contains("consumed"));
    drop(inspection);
    f.check(Vec::new());
    println!("RECOVERY_SYNC_DESCRIPTOR_FAULT: {outcome:?}");
}

#[test]
fn recovery_failed_payload_write_then_fresh_import_works() {
    let mut f = FaultFixture::new();
    let root = f.root.clone();
    let failed = f.store.import_inner(OsStr::new("failed-write"), &f.reader, &AtomicBool::new(false), |stage, file| {
        if stage == ImportStage::ChunkWritten { *file = File::open(root.join(PAYLOAD_PENDING)).unwrap(); }
        Ok(())
    });
    refused(failed, "payload write failed");
    f.check(vec![(PAYLOAD_PENDING.into(), f.bytes[..READER_CHUNK_BYTES].to_vec())]);
    let mut inspection = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    let outcome = f.store.discard_interrupted_import(&mut inspection, &AtomicBool::new(false)).unwrap();
    assert!(outcome.error.is_none() && outcome.sync_warning.is_none());
    drop(inspection);
    f.check(Vec::new());
    let item = f.store.import_document(OsStr::new("after-write-failure"), &f.reader, &AtomicBool::new(false)).unwrap().value;
    assert_eq!(fs::read(root.join(payload_name(item.id))).unwrap(), f.bytes);
    assert!(f.store.remove(item.id).unwrap().is_complete());
    f.manifest = fs::read(root.join(MANIFEST)).unwrap();
    f.manifest_identity = identity(&root.join(MANIFEST));
    f.check(Vec::new());
}
