//! Healthy removal status is exercised against actual fixture namespace effects.
use super::*;

struct RestoreMode(PathBuf);
impl Drop for RestoreMode {
    fn drop(&mut self) { fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700)).unwrap(); }
}
fn preserved(f: &FaultFixture) {
    assert_eq!(fs::read(&f.original).unwrap(), f.bytes);
    assert_eq!(identity(&f.original), f.original_identity);
    assert_eq!(fs::read(&f.room).unwrap(), f.bytes);
    assert_eq!(identity(&f.room), f.room_identity);
    let unselected = f.root.join(payload_name(f.kept[1].id));
    assert_eq!(fs::read(&unselected).unwrap(), f.bytes);
    assert_eq!(identity(&unselected), f.kept_identities[1]);
}
fn committed(f: &FaultFixture, outcome: &RemovalOutcome) {
    let old = Manifest::decode(&f.manifest).unwrap();
    let mut expected = old;
    expected.generation += 1;
    expected.items.remove(0);
    assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), expected.encode().unwrap());
    assert_eq!(outcome.removed_id, f.kept[0].id);
    assert_eq!(outcome.committed_generation, expected.generation);
    assert_eq!(f.store.items(), &f.kept[1..]);
    preserved(f);
}

#[test]
fn removal_outcome_positive_reports_actual_removed_and_known_complete() {
    let mut f = FaultFixture::new();
    create(&f.store.root, FINDER).unwrap().write_all(b"Finder bytes untouched").unwrap();
    let finder = identity(&f.root.join(FINDER));
    let outcome = f.store.remove(f.kept[0].id).unwrap();
    assert!(!f.root.join(payload_name(f.kept[0].id)).exists());
    committed(&f, &outcome);
    assert!(outcome.payload_removed, "actual unlink lacks structured status: {outcome:?}");
    assert_eq!(outcome.remaining, Some(Vec::new()));
    assert!(outcome.is_complete(), "{outcome:?}");
    assert_eq!(f.store.evidence(), Some(&[][..]));
    assert_eq!(fs::read(f.root.join(FINDER)).unwrap(), b"Finder bytes untouched");
    assert_eq!(identity(&f.root.join(FINDER)), finder);
}

#[test]
fn removal_outcome_cleanup_kernel_eacces_is_error_not_sync_warning() {
    let mut f = FaultFixture::new();
    let path = f.root.join(payload_name(f.kept[0].id));
    let before = identity(&path);
    let root = f.root.clone(); let restore = RestoreMode(root.clone());
    let outcome = f.store.remove_inner(f.kept[0].id, &AtomicBool::new(false), |stage, _, _| {
        if stage == RemoveStage::CleanupReady { fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap(); }
        Ok(())
    }).unwrap();
    drop(restore);
    assert_eq!(identity(&path), before); assert_eq!(fs::read(&path).unwrap(), f.bytes);
    committed(&f, &outcome);
    assert!(!outcome.is_complete()); assert!(!outcome.payload_removed);
    let error = outcome.cleanup_error.as_ref().expect("actual EACCES lacks cleanup_error");
    assert!(error.contains(&std::io::Error::from_raw_os_error(libc::EACCES).to_string()), "{error}");
    assert!(outcome.sync_warning.is_none(), "cleanup failure is not a sync failure");
    assert_eq!(outcome.remaining, Some(vec![OsString::from(payload_name(f.kept[0].id))]));
    assert_eq!(f.store.evidence(), outcome.remaining.as_deref());
    println!("REMOVAL_OUTCOME_ACTUAL_CLEANUP: {error}");
    assert!(f.store.import_document(OsStr::new("blocked by orphan"), &f.reader, &AtomicBool::new(false)).is_err());
    let mut cap = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    assert!(f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(false)).unwrap().is_complete());
    drop(cap);
    assert!(!path.exists());
    preserved(&f);
}

#[test]
fn removal_outcome_new_unknown_is_incomplete_and_preserved() {
    let mut f = FaultFixture::new(); let root = f.root.clone();
    let path = root.join(payload_name(f.kept[0].id)); let before = identity(&path);
    let outcome = f.store.remove_inner(f.kept[0].id, &AtomicBool::new(false), |stage, _, _| {
        if stage == RemoveStage::CleanupReady { fs::write(root.join("unknown"), b"actual unknown").unwrap(); }
        Ok(())
    }).unwrap();
    committed(&f, &outcome);
    assert_eq!(fs::read(root.join("unknown")).unwrap(), b"actual unknown");
    assert!(!outcome.is_complete());
    assert!(outcome.cleanup_error.is_some(), "new unknown lacks structured cleanup failure: {outcome:?}");
    assert!(!outcome.payload_removed);
    assert_eq!(identity(&path), before);
    assert_eq!(outcome.remaining, Some(vec![OsString::from(payload_name(f.kept[0].id)), OsString::from("unknown")]));
    assert_eq!(f.store.evidence(), outcome.remaining.as_deref());
    assert!(f.store.import_document(OsStr::new("blocked"), &f.reader, &AtomicBool::new(false)).is_err());
    assert!(f.store.remove(f.kept[1].id).is_err());
    assert!(f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err());
    preserved(&f);
}

#[test]
fn removal_outcome_rebound_or_changed_selected_is_not_unlinked_after_commit() {
    for kind in ["rebind", "same-inode", "manifest", "finder"] {
        let mut f = FaultFixture::new(); let root = f.root.clone();
        let path = root.join(payload_name(f.kept[0].id));
        let bytes = f.bytes.clone();
        create(&f.store.root, FINDER).unwrap().write_all(b"Finder before").unwrap();
        let mut selected_after = None;
        let outcome = f.store.remove_inner(f.kept[0].id, &AtomicBool::new(false), |stage, _, _| {
            if stage == RemoveStage::CleanupReady {
                match kind {
                    "rebind" => {
                        fs::remove_file(&path).unwrap();
                        OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).unwrap().write_all(&bytes).unwrap();
                    },
                    "same-inode" => {
                        let mut changed = bytes.clone(); changed[0] ^= 1;
                        fs::write(&path, changed).unwrap();
                    },
                    "manifest" => {
                        let b = fs::read(root.join(MANIFEST)).unwrap();
                        fs::remove_file(root.join(MANIFEST)).unwrap();
                        OpenOptions::new().write(true).create_new(true).mode(0o600).open(root.join(MANIFEST)).unwrap().write_all(&b).unwrap();
                    },
                    "finder" => fs::write(root.join(FINDER), b"Finder after").unwrap(),
                    _ => unreachable!(),
                }
                selected_after = Some((identity(&path), fs::read(&path).unwrap()));
            }
            Ok(())
        }).unwrap();
        committed(&f, &outcome);
        let (id, actual) = selected_after.unwrap();
        assert_eq!(identity(&path), id); assert_eq!(fs::read(&path).unwrap(), actual);
        assert!(!outcome.payload_removed); assert!(!outcome.is_complete());
        assert!(outcome.cleanup_error.is_some()); assert!(outcome.sync_warning.is_none());
        assert_eq!(outcome.remaining, Some(vec![OsString::from(payload_name(f.kept[0].id))]));
    }
}

#[test]
fn removal_outcome_cancel_before_commit_refuses_but_after_commit_reports_completion() {
    for at in [RemoveStage::ManifestReady, RemoveStage::CleanupReady, RemoveStage::CleanupSync] {
        let mut f = FaultFixture::new(); let cancelled = AtomicBool::new(false);
        let result = f.store.remove_inner(f.kept[0].id, &cancelled, |stage, _, _| {
            if stage == at { cancelled.store(true, Ordering::Relaxed); }
            Ok(())
        });
        assert!(cancelled.load(Ordering::Relaxed));
        if at == RemoveStage::ManifestReady {
            assert!(matches!(result, Err(Failure::Refused(e)) if e.contains("cancelled")));
            let mut expected = Manifest::decode(&f.manifest).unwrap(); expected.generation += 1; expected.items.remove(0);
            f.check(vec![(MANIFEST_PENDING.into(), expected.encode().unwrap())]);
        } else {
            let outcome = result.unwrap();
            committed(&f, &outcome);
            assert!(outcome.is_complete()); assert!(outcome.payload_removed);
            assert!(!f.root.join(payload_name(f.kept[0].id)).exists());
        }
    }
}

#[test]
fn removal_outcome_kernel_sync_and_namespace_failures_stay_unknown_until_fresh_safe_reload() {
    for next in ["import", "remove"] {
        let mut f = FaultFixture::new();
        let saved = f.store.root.try_clone().unwrap();
        take_body_reads();
        let outcome = f.store.remove_inner(f.kept[0].id, &AtomicBool::new(false), |stage, store, _| {
            if stage == RemoveStage::CleanupSync {
                // Same no-op seam as M-C: actual kernel fsync and directory-read
                // errors on a pipe descriptor, never synthesized I/O results.
                let mut fds = [-1; 2]; assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
                store.root = unsafe { File::from_raw_fd(fds[0]) };
                drop(unsafe { File::from_raw_fd(fds[1]) });
            }
            Ok(())
        }).unwrap();
        let reads = take_body_reads();
        let selected = &f.kept_identities[0];
        assert!(reads.iter().all(|(dev, ino, _)| (*dev, *ino) == (selected.0, selected.1)));
        assert_eq!(reads.iter().map(|(_, _, n)| *n as u64).sum::<u64>(), f.kept[0].bytes);
        assert!(outcome.payload_removed); assert!(!outcome.is_complete());
        assert_eq!(outcome.remaining, None); assert_eq!(f.store.evidence(), None);
        assert!(outcome.cleanup_error.as_ref().unwrap().contains("remaining inspection failed"));
        let warning = outcome.sync_warning.as_ref().unwrap();
        assert!(warning.contains(&std::io::Error::from_raw_os_error(libc::EBADF).to_string()), "{warning}");
        println!("REMOVAL_OUTCOME_ACTUAL_SYNC: {warning}; remaining={:?}", outcome.remaining);
        committed(&f, &outcome);
        assert!(!f.root.join(payload_name(f.kept[0].id)).exists());
        assert!(f.store.refresh().is_err()); assert_eq!(f.store.evidence(), None);
        assert!(f.store.import_document(OsStr::new("invalid FD"), &f.reader, &AtomicBool::new(false)).is_err());
        assert!(f.store.remove(f.kept[1].id).is_err()); assert_eq!(f.store.evidence(), None);
        f.store.root = saved;
        // A valid restored FD is not sufficient: fresh actual unknown evidence
        // MUST still block ordinary writes, even from an unknown prior snapshot.
        fs::write(f.root.join("unknown"), b"do not delete").unwrap();
        assert!(f.store.import_document(OsStr::new("unknown"), &f.reader, &AtomicBool::new(false)).is_err());
        assert!(f.store.remove(f.kept[1].id).is_err());
        assert_eq!(f.store.evidence().unwrap(), &[OsString::from("unknown")]);
        preserved(&f);
        // Explicit fixture-only removal; production never cleans this name.
        fs::remove_file(f.root.join("unknown")).unwrap();
        f.store.evidence = None;
        if next == "import" {
            let imported = f.store.import_document(OsStr::new("fresh actual reload"), &f.reader, &AtomicBool::new(false)).unwrap();
            assert!(imported.sync_warning.is_none());
            assert!(f.store.remove(imported.value.id).unwrap().is_complete());
            preserved(&f);
        } else {
            assert!(f.store.remove(f.kept[1].id).unwrap().is_complete());
        }
        assert_eq!(f.store.evidence(), Some(&[][..]));
        assert_eq!(Store::open(&f.root, policy()).unwrap().items(), f.store.items());
        assert_eq!(identity(&f.original), f.original_identity); assert_eq!(identity(&f.room), f.room_identity);
    }
}

#[test]
fn removal_outcome_unknown_remaining_without_sync_failure_is_not_empty() {
    let mut f = FaultFixture::new(); let root = f.root.clone();
    let outcome = f.store.remove_inner(f.kept[0].id, &AtomicBool::new(false), |stage, _, _| {
        if stage == RemoveStage::CleanupSync {
            // Actual namespace count overrun; directory fsync itself succeeds.
            for n in 0..(MAX_ITEMS + 16) { fs::write(root.join(format!("unknown-{n}")), b"keep").unwrap(); }
        }
        Ok(())
    }).unwrap();
    committed(&f, &outcome);
    assert!(outcome.payload_removed); assert!(outcome.sync_warning.is_none());
    assert!(outcome.cleanup_error.is_some()); assert!(!outcome.is_complete());
    assert_eq!(outcome.remaining, None); assert_eq!(f.store.evidence(), None);
    assert!(f.store.import_document(OsStr::new("overfull namespace"), &f.reader, &AtomicBool::new(false)).is_err());
    assert!(f.store.remove(f.kept[1].id).is_err());
    assert_eq!(fs::read(root.join("unknown-0")).unwrap(), b"keep");
    preserved(&f);
}

#[test]
fn removal_outcome_new_unknown_after_unlink_reports_actual_payload_progress() {
    let mut f = FaultFixture::new(); let root = f.root.clone();
    let outcome = f.store.remove_inner(f.kept[0].id, &AtomicBool::new(false), |stage, _, _| {
        if stage == RemoveStage::CleanupSync { fs::write(root.join("unknown"), b"keep").unwrap(); }
        Ok(())
    }).unwrap();
    committed(&f, &outcome);
    assert!(outcome.payload_removed); assert!(!outcome.is_complete());
    assert_eq!(outcome.remaining, Some(vec![OsString::from("unknown")]));
    assert!(outcome.cleanup_error.is_some()); assert!(outcome.sync_warning.is_none());
    assert_eq!(fs::read(root.join("unknown")).unwrap(), b"keep");
}

#[test]
fn removal_outcome_completion_predicate_requires_known_empty_and_no_error_or_warning() {
    // Pure status predicate coverage, not synthetic filesystem failure evidence.
    let mut outcome = RemovalOutcome { removed_id: 1, committed_generation: 3,
        payload_removed: true, remaining: Some(Vec::new()), cleanup_error: None, sync_warning: None };
    assert!(outcome.is_complete());
    outcome.sync_warning = Some("predicate-only warning".into());
    assert!(!outcome.is_complete());
    outcome.sync_warning = None; outcome.cleanup_error = Some("predicate-only error".into());
    assert!(!outcome.is_complete());
    outcome.cleanup_error = None; outcome.remaining = None;
    assert!(!outcome.is_complete());
    outcome.remaining = Some(vec![OsString::from("actual-name-required-by-caller")]);
    assert!(!outcome.is_complete());
}
