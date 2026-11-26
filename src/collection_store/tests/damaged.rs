//! Real on-disk damaged LIVE selected items, not synthetic integrity results.
use super::*;
use std::os::unix::fs::FileExt;
#[path = "recovery_scope.rs"]
mod recovery_scope;
#[path = "large_damage.rs"]
mod large_damage;
fn damage(f: &FaultFixture, kind: &str) {
    let path = f.root.join(payload_name(f.kept[0].id));
    match kind {
        "overwrite" => OpenOptions::new().write(true).open(path).unwrap().write_all_at(b"!", 0).unwrap(),
        "truncate" => OpenOptions::new().write(true).open(path).unwrap().set_len(1).unwrap(),
        "empty" => OpenOptions::new().write(true).open(path).unwrap().set_len(0).unwrap(),
        "append" => OpenOptions::new().append(true).open(path).unwrap().write_all(b"extra").unwrap(),
        "missing" => fs::remove_file(path).unwrap(),
        _ => unreachable!(),
    }
}
fn inspect_supported(kind: &str) {
    let f = FaultFixture::new();
    damage(&f, kind);
    let path = f.root.join(payload_name(f.kept[0].id));
    if kind == "missing" { assert!(!path.exists()); }
    else {
        let actual = fs::read(&path).unwrap();
        assert_ne!(actual, f.bytes);
        if kind == "overwrite" { assert_eq!(actual.len(), f.bytes.len()); }
        else { assert_eq!(actual.len(), 1); }
    }
    let result = f.store.inspect_damaged_item(&f.kept[0], 1024 * 1024, &AtomicBool::new(false));
    assert!(result.is_ok(), "missing explicit damaged-item inspection capability: {}", result.err().unwrap());
    assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), f.manifest);
}
#[test]
fn damaged_real_same_length_overwrite_inspection() { inspect_supported("overwrite"); }
#[test]
fn damaged_real_truncate_inspection() { inspect_supported("truncate"); }
#[test]
fn damaged_real_missing_inspection() { inspect_supported("missing"); }

fn selected_state(path: &Path) -> Option<(Identity, u64, u32)> {
    match fs::symlink_metadata(path) {
        Ok(m) => Some((Identity(m.dev(), m.ino(), m.len(), m.mode(), m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec()), m.nlink(), m.uid())),
        Err(e) => { assert_eq!(e.kind(), std::io::ErrorKind::NotFound); None },
    }
}
fn inspect(f: &FaultFixture) -> DamagedInspection {
    f.store.inspect_damaged_item(&f.kept[0], 1024 * 1024, &AtomicBool::new(false)).unwrap()
}
fn originals(f: &FaultFixture) {
    assert_eq!(identity(&f.original), f.original_identity);
    assert_eq!(identity(&f.room), f.room_identity);
    assert_eq!(fs::read(&f.original).unwrap(), f.bytes);
    assert_eq!(fs::read(&f.room).unwrap(), f.bytes);
}
fn completed_vertical(kind: &str, unrelated_damage: bool) {
    let mut f = FaultFixture::new();
    create(&f.store.root, FINDER).unwrap().write_all(b"inert Finder").unwrap();
    let finder = identity(&f.root.join(FINDER));
    damage(&f, kind);
    if unrelated_damage {
        OpenOptions::new().write(true).open(f.root.join(payload_name(f.kept[1].id))).unwrap().write_all_at(b"?", 0).unwrap();
    }
    let kept_path = f.root.join(payload_name(f.kept[1].id));
    let kept_identity = identity(&kept_path);
    let kept_bytes = fs::read(&kept_path).unwrap();
    let path = f.root.join(payload_name(f.kept[0].id));
    let before = selected_state(&path);
    let actual = before.as_ref().map(|_| fs::read(&path).unwrap());
    assert!(f.store.remove(f.kept[0].id).is_err(), "normal healthy removal bypassed selected integrity");
    let root_before = identity(&f.root);
    let mut cap = inspect(&f);
    assert_eq!(cap.expected(), &f.kept[0]);
    assert_eq!(cap.generated_binding(), payload_name(f.kept[0].id));
    assert_eq!(cap.committed_generation(), Manifest::decode(&f.manifest).unwrap().generation);
    match (cap.observed(), &actual) {
        (ObservedDamage::Missing, None) => {},
        (ObservedDamage::Regular { bytes, sha256, device, inode, modified, changed }, Some(actual)) => {
            let m = fs::metadata(&path).unwrap();
            assert_eq!(*bytes, actual.len() as u64);
            assert_eq!(*sha256, <[u8; 32]>::from(Sha256::digest(actual)));
            assert_eq!((*device, *inode), (m.dev(), m.ino()));
            assert_eq!(*modified, (m.mtime(), m.mtime_nsec()));
            assert_eq!(*changed, (m.ctime(), m.ctime_nsec()));
        },
        other => panic!("wrong actual observation: {other:?}"),
    }
    assert_eq!(identity(&f.root), root_before);
    assert!(f.store.refresh().is_err(), "capability did not retain exclusive lock");
    drop(cap);
    assert_eq!(identity(&f.root), root_before, "drop wrote");
    cap = inspect(&f);
    assert!(matches!(f.store.remove_damaged_item(&mut cap, &AtomicBool::new(true)), Err(Failure::Refused(e)) if e.contains("cancelled")));
    assert!(f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).is_err(), "cancelled capability reused");
    drop(cap);
    assert_eq!(identity(&f.root), root_before, "cancel wrote");
    assert_eq!(selected_state(&path), before);
    assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), f.manifest);
    assert_eq!(identity(&f.root.join(MANIFEST)), f.manifest_identity);
    let mut cap = inspect(&f);
    take_body_reads();
    let outcome = f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).unwrap();
    let reads = take_body_reads();
    assert!(outcome.is_complete(), "{outcome:?}");
    assert_eq!(outcome.payload_removed, kind != "missing");
    assert_eq!(outcome.removed_id, f.kept[0].id);
    assert_eq!(outcome.remaining, Some(Vec::new()));
    if let Some((Identity(dev, ino, ..), _, _)) = before {
        assert!(reads.iter().all(|(d, i, _)| (*d, *i) == (dev, ino)), "unrelated body read: {reads:?}");
        assert_eq!(reads.iter().map(|(_, _, n)| *n).sum::<usize>(), actual.as_ref().unwrap().len());
    } else { assert!(reads.is_empty()); }
    assert!(f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).is_err(), "committed capability reused");
    drop(cap);
    assert!(selected_state(&path).is_none());
    let mut expected = Manifest::decode(&f.manifest).unwrap();
    expected.generation += 1;
    expected.items.remove(0);
    assert_eq!(outcome.committed_generation, expected.generation);
    assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), expected.encode().unwrap());
    assert_eq!(identity(&kept_path), kept_identity);
    assert_eq!(fs::read(&kept_path).unwrap(), kept_bytes);
    assert_eq!(identity(&f.root.join(FINDER)), finder);
    assert_eq!(fs::read(f.root.join(FINDER)).unwrap(), b"inert Finder");
    originals(&f);
    let mut reopened = Store::open(&f.root, policy()).unwrap();
    assert_eq!(reopened.items(), expected.items);
    assert!(reopened.evidence().unwrap().is_empty());
    let fresh = reopened.import_document(OsStr::new("subsequent healthy"), &f.reader, &AtomicBool::new(false)).unwrap().value;
    assert_eq!(fresh.id, expected.next);
    assert!(reopened.remove(fresh.id).unwrap().is_complete());
    expected.next += 1; expected.generation += 2;
    assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), expected.encode().unwrap());
    assert_eq!(identity(&kept_path), kept_identity);
    assert_eq!(fs::read(kept_path).unwrap(), kept_bytes);
    if unrelated_damage {
        assert_eq!(reopened.inspect().unwrap()[0].status, PayloadStatus::Unchecked);
        assert!(reopened.remove(f.kept[1].id).is_err());
    }
    originals(&f);
}
#[test]
fn damaged_overwrite_complete_vertical() { completed_vertical("overwrite", false); }
#[test]
fn damaged_truncate_complete_vertical() { completed_vertical("truncate", false); }
#[test]
fn damaged_missing_complete_vertical() { completed_vertical("missing", false); }
#[test]
fn damaged_empty_regular_complete_vertical() { completed_vertical("empty", false); }
#[test]
fn damaged_appended_regular_complete_vertical() { completed_vertical("append", false); }
#[test]
fn damaged_unrelated_corruption_preserves_exact_expected_records_without_reads_or_authority() {
    completed_vertical("overwrite", true);
}

#[test]
fn damaged_healthy_and_mismatched_expected_and_stale_view_refuse_writefree() {
    let mut f = FaultFixture::new();
    assert!(f.store.inspect_damaged_item(&f.kept[0], f.bytes.len() as u64, &AtomicBool::new(false)).is_err());
    damage(&f, "overwrite");
    for field in ["id", "name", "bytes", "digest", "category"] {
        let mut selected = f.kept[0].clone();
        match field {
            "id" => selected.id += 10,
            "name" => selected.name = "wrong".into(),
            "bytes" => selected.bytes += 1,
            "digest" => selected.sha256[0] ^= 1,
            "category" => selected.category = Category::Decoration,
            _ => unreachable!(),
        }
        assert!(f.store.inspect_damaged_item(&selected, f.bytes.len() as u64, &AtomicBool::new(false)).is_err(), "{field}");
    }
    let root = identity(&f.root);
    let mut changed = Manifest::decode(&f.manifest).unwrap(); changed.generation += 1;
    fs::write(f.root.join(MANIFEST), changed.encode().unwrap()).unwrap();
    assert!(f.store.inspect_damaged_item(&f.kept[0], f.bytes.len() as u64, &AtomicBool::new(false)).is_err());
    assert_eq!(identity(&f.root), root);
    f.store.refresh().unwrap();
    drop(inspect(&f));
    originals(&f);
}

#[test]
fn damaged_unsafe_selected_types_and_modes_refuse_without_following_or_deleting() {
    for kind in ["symlink", "directory", "fifo", "hardlink", "mode"] {
        let f = FaultFixture::new();
        let path = f.root.join(payload_name(f.kept[0].id));
        match kind {
            "hardlink" => fs::hard_link(&path, f._owner.0.join("external-link")).unwrap(),
            "mode" => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
            _ => {
                fs::remove_file(&path).unwrap();
                match kind {
                    "symlink" => std::os::unix::fs::symlink(&f.original, &path).unwrap(),
                    "directory" => fs::create_dir(&path).unwrap(),
                    "fifo" => {
                        let name = CString::new(path.as_os_str().as_bytes()).unwrap();
                        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
                    },
                    _ => unreachable!(),
                }
            },
        }
        let before = selected_state(&path);
        let root = identity(&f.root);
        take_body_reads();
        assert!(f.store.inspect_damaged_item(&f.kept[0], f.bytes.len() as u64, &AtomicBool::new(false)).is_err(), "{kind}");
        assert!(take_body_reads().is_empty());
        assert_eq!(selected_state(&path), before);
        assert_eq!(identity(&f.root), root);
        assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), f.manifest);
        originals(&f);
    }
}

#[test]
fn damaged_stale_repaired_rebound_metadata_manifest_root_lock_finder_namespace_refuse() {
    for kind in ["content", "repaired", "rebound", "metadata", "manifest", "manifest-rebound", "generation",
        "root-rebound", "root-metadata", "lock", "lock-rebound", "finder", "unknown", "transient", "missing-present"] {
        let mut f = FaultFixture::new();
        create(&f.store.root, FINDER).unwrap().write_all(b"before").unwrap();
        damage(&f, if kind == "missing-present" { "missing" } else { "overwrite" });
        let mut cap = inspect(&f);
        let path = f.root.join(payload_name(f.kept[0].id));
        match kind {
            "content" => OpenOptions::new().write(true).open(&path).unwrap().write_all_at(b"?", 1).unwrap(),
            "repaired" => fs::write(&path, &f.bytes).unwrap(),
            "rebound" => {
                let bytes = fs::read(&path).unwrap();
                fs::rename(&path, f._owner.0.join("saved-payload")).unwrap();
                OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).unwrap().write_all(&bytes).unwrap();
            },
            "metadata" => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(),
            "manifest" => fs::write(f.root.join(MANIFEST), &f.manifest).unwrap(),
            "manifest-rebound" | "lock-rebound" => {
                let name = if kind == "manifest-rebound" { MANIFEST } else { "lock" };
                let bytes = fs::read(f.root.join(name)).unwrap();
                fs::rename(f.root.join(name), f._owner.0.join("saved-metadata")).unwrap();
                OpenOptions::new().write(true).create_new(true).mode(0o600).open(f.root.join(name)).unwrap().write_all(&bytes).unwrap();
            },
            "generation" => { let mut m = Manifest::decode(&f.manifest).unwrap(); m.generation += 1; fs::write(f.root.join(MANIFEST), m.encode().unwrap()).unwrap(); },
            "root-rebound" => {
                fs::rename(&f.root, f._owner.0.join("saved-root")).unwrap();
                fs::create_dir(&f.root).unwrap(); fs::set_permissions(&f.root, fs::Permissions::from_mode(0o700)).unwrap();
            },
            "root-metadata" => { fs::set_permissions(&f.root, fs::Permissions::from_mode(0o500)).unwrap(); fs::set_permissions(&f.root, fs::Permissions::from_mode(0o700)).unwrap(); },
            "lock" => fs::write(f.root.join("lock"), b"changed").unwrap(),
            "finder" => fs::write(f.root.join(FINDER), b"changed").unwrap(),
            "unknown" => fs::write(f.root.join("new-unknown"), b"preserve").unwrap(),
            "transient" => { fs::write(f.root.join("transient"), b"x").unwrap(); fs::remove_file(f.root.join("transient")).unwrap(); },
            "missing-present" => { create(&f.store.root, &payload_name(f.kept[0].id)).unwrap().write_all(&f.bytes).unwrap(); },
            _ => unreachable!(),
        }
        let root = identity(&f.root);
        let before = selected_state(&path);
        let manifest = fs::read(f.root.join(MANIFEST)).ok();
        assert!(f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).is_err(), "{kind} stale authority accepted");
        assert!(f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).is_err());
        drop(cap);
        assert_eq!(identity(&f.root), root, "{kind}: refusal wrote");
        assert_eq!(selected_state(&path), before, "{kind}: replacement removed");
        assert_eq!(fs::read(f.root.join(MANIFEST)).ok(), manifest);
        assert!(!f.root.join(MANIFEST_PENDING).exists());
        if kind == "repaired" { assert!(f.store.inspect_damaged_item(&f.kept[0], f.bytes.len() as u64, &AtomicBool::new(false)).is_err()); }
        originals(&f);
    }
}

#[test]
fn damaged_crossroot_capability_consumed_without_writes() {
    let mut f = FaultFixture::new(); damage(&f, "overwrite");
    let mut other = FaultFixture::new(); damage(&other, "overwrite");
    let mut cap = inspect(&f);
    assert!(other.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).is_err());
    assert!(f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).is_err());
    drop(cap);
    for f in [&f, &other] {
        assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), f.manifest);
        assert!(!f.root.join(MANIFEST_PENDING).exists()); originals(f);
    }
}

#[test]
fn damaged_pending_or_unknown_evidence_and_external_activation_never_grant_authority() {
    for kind in ["pending", "unknown", "activated"] {
        let mut f = FaultFixture::new(); damage(&f, "overwrite");
        let mut cap = inspect(&f);
        let mut successor = Manifest::decode(&f.manifest).unwrap();
        successor.generation += 1; successor.items.remove(0);
        let name = if kind == "unknown" { "unknown" } else { MANIFEST_PENDING };
        create(&f.store.root, name).unwrap().write_all(&successor.encode().unwrap()).unwrap();
        if kind == "activated" { fs::rename(f.root.join(MANIFEST_PENDING), f.root.join(MANIFEST)).unwrap(); }
        let manifest = fs::read(f.root.join(MANIFEST)).unwrap();
        let path = f.root.join(payload_name(f.kept[0].id)); let before = identity(&path);
        assert!(f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).is_err());
        drop(cap);
        assert_eq!(identity(&path), before);
        assert_eq!(fs::read(f.root.join(MANIFEST)).unwrap(), manifest);
        assert!(f.store.inspect_damaged_item(&f.kept[0], f.bytes.len() as u64, &AtomicBool::new(false)).is_err());
        originals(&f);
    }
}

#[test]
fn damaged_bounded_selected_actual_reads_and_inspect_confirm_cancellation() {
    use std::sync::Arc;
    let f = Fixture::new(); let root = f.root("cache");
    let (original, reader) = small_source(&f, &[0xa5; 131073]);
    let mut store = Store::open(&root, policy()).unwrap();
    let selected = store.import_document(OsStr::new("selected"), &reader, &AtomicBool::new(false)).unwrap().value;
    let kept = store.import_document(OsStr::new("kept"), &reader, &AtomicBool::new(false)).unwrap().value;
    let path = root.join(payload_name(selected.id));
    OpenOptions::new().write(true).open(&path).unwrap().write_all_at(b"!", 0).unwrap();
    let m = fs::metadata(&path).unwrap();
    let manifest = fs::read(root.join(MANIFEST)).unwrap(); let root_before = identity(&root);
    take_body_reads();
    assert!(store.inspect_damaged_item(&selected, 131072, &AtomicBool::new(false)).is_err());
    assert!(take_body_reads().is_empty(), "over-budget inspection read body");
    assert!(store.inspect_damaged_item(&selected, 131073, &AtomicBool::new(true)).is_err());
    assert!(take_body_reads().is_empty());
    let cancelled = Arc::new(AtomicBool::new(false));
    cancel_after_metadata_check(cancelled.clone());
    assert!(store.inspect_damaged_item(&selected, 131073, &cancelled).is_err());
    assert!(take_body_reads().is_empty());
    for confirm in [false, true] {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut cap = if confirm {
            Some(store.inspect_damaged_item(&selected, 131073, &cancelled).unwrap())
        } else { None };
        take_body_reads(); cancel_after_body_read(cancelled.clone());
        if let Some(cap) = &mut cap {
            assert!(matches!(store.remove_damaged_item(cap, &cancelled), Err(Failure::Refused(e)) if e.contains("cancelled")));
        } else { assert!(store.inspect_damaged_item(&selected, 131073, &cancelled).is_err()); }
        let reads = take_body_reads(); assert_eq!(reads, vec![(m.dev(), m.ino(), 65536)]);
        drop(cap);
        assert_eq!(identity(&root), root_before);
        assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), manifest);
    }
    take_body_reads();
    let mut cap = store.inspect_damaged_item(&selected, 131073, &AtomicBool::new(false)).unwrap();
    let reads = take_body_reads();
    assert!(reads.iter().all(|(d, i, n)| (*d, *i) == (m.dev(), m.ino()) && *n <= 65536));
    assert_eq!(reads.iter().map(|(_, _, n)| *n).sum::<usize>(), 131073);
    let outcome = store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).unwrap(); assert!(outcome.is_complete());
    drop(cap);
    let reads = take_body_reads(); assert_eq!(reads.iter().map(|(_, _, n)| *n).sum::<usize>(), 131073);
    assert!(reads.iter().all(|(d, i, n)| (*d, *i) == (m.dev(), m.ino()) && *n <= 65536));
    assert_eq!(store.items(), &[kept]); assert_eq!(fs::metadata(original).unwrap().len(), 131073);
}

#[test]
fn damaged_actual_length_caller_budget_and_lowered_policy_do_not_grant_authority() {
    let mut f = FaultFixture::new(); damage(&f, "truncate");
    f.store.set_policy(Policy::new(1, 1).unwrap());
    let mut cap = inspect(&f);
    assert!(f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).unwrap().is_complete()); drop(cap);
    let f = FaultFixture::new();
    let path = f.root.join(payload_name(f.kept[0].id));
    OpenOptions::new().write(true).open(path).unwrap().set_len(64 * 1024 * 1024 + 1).unwrap();
    take_body_reads(); let before = identity(&f.root);
    let error = f.store.inspect_damaged_item(&f.kept[0], 64 * 1024 * 1024, &AtomicBool::new(false)).err().unwrap();
    assert!(error.contains("budget"), "{error}");
    assert!(error.contains("actual bytes 67108865"), "missing observed length: {error}");
    assert!(error.contains("available bytes 67108864"), "missing chosen budget: {error}");
    assert!(take_body_reads().is_empty()); assert_eq!(identity(&f.root), before);
}

#[test]
fn damaged_kernel_manifest_write_failure_and_precommit_cancel_preserve_actual_evidence() {
    for kind in ["write-empty", "write-prefix", "cancel", "repaired", "rebound", "finder"] {
        let mut f = FaultFixture::new(); damage(&f, "overwrite");
        let path = f.root.join(payload_name(f.kept[0].id));
        let mut cap = inspect(&f); let cancelled = AtomicBool::new(false);
        let actual_before = fs::read(&path).unwrap(); let identity_before = identity(&path);
        let root = f.root.clone();
        let result = f.store.remove_damaged_inner(&mut cap, &cancelled, |stage, _, file| {
            if stage == RemoveStage::ManifestStaging && kind.starts_with("write") {
                if kind == "write-prefix" { file.write_all(&b"HAIRCOLL"[..]).unwrap(); }
                *file = File::open(root.join(MANIFEST_PENDING)).unwrap();
            }
            if stage == RemoveStage::ManifestReady {
                match kind {
                    "cancel" => cancelled.store(true, Ordering::Relaxed),
                    "repaired" => fs::write(&path, &f.bytes).unwrap(),
                    "rebound" => {
                        let bytes = fs::read(&path).unwrap(); fs::remove_file(&path).unwrap();
                        create(&acquire_root(&root).unwrap().2, &payload_name(f.kept[0].id)).unwrap().write_all(&bytes).unwrap();
                    },
                    "finder" => { create(&acquire_root(&root).unwrap().2, FINDER).unwrap().write_all(b"new").unwrap(); },
                    _ => {},
                }
            }
            Ok(())
        });
        match result {
            Err(Failure::Refused(error)) => {
                if kind.starts_with("write") {
                    assert!(error.contains("Manifest write failed"));
                    assert!(error.contains(&std::io::Error::from_raw_os_error(libc::EBADF).to_string()));
                    println!("DAMAGED_ACTUAL_MANIFEST_WRITE {kind}: {error}");
                }
            },
            other => panic!("{kind}: unexpected {other:?}"),
        }
        assert!(f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).is_err()); drop(cap);
        assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), f.manifest);
        assert_eq!(identity(&root.join(MANIFEST)), f.manifest_identity);
        assert!(path.exists()); assert!(root.join(MANIFEST_PENDING).exists());
        let pending = fs::read(root.join(MANIFEST_PENDING)).unwrap();
        if kind == "write-empty" { assert!(pending.is_empty()); }
        else if kind == "write-prefix" { assert_eq!(pending, b"HAIRCOLL"); }
        else {
            let mut expected = Manifest::decode(&f.manifest).unwrap(); expected.generation += 1; expected.items.remove(0);
            assert_eq!(pending, expected.encode().unwrap());
        }
        if kind.starts_with("write") || kind == "cancel" || kind == "finder" {
            assert_eq!(identity(&path), identity_before); assert_eq!(fs::read(&path).unwrap(), actual_before);
        }
        originals(&f);
        // Explicit recovery discards only recognized pending evidence. Actual
        // damaged LIVE bytes stay untouched and acquire no integrity trust.
        let recovery_before = selected_state(&path);
        let recovery_bytes = fs::read(&path).unwrap();
        let mut recovery = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
        assert!(f.store.discard_interrupted_import(&mut recovery, &AtomicBool::new(false)).unwrap().is_complete());
        drop(recovery);
        assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), f.manifest);
        assert_eq!(selected_state(&path), recovery_before);
        assert_eq!(fs::read(&path).unwrap(), recovery_bytes);
    }
}

#[test]
fn damaged_committed_cleanup_kernel_eacces_is_truthful_and_fresh_orphan_recovery_works() {
    let mut f = FaultFixture::new(); damage(&f, "overwrite");
    let path = f.root.join(payload_name(f.kept[0].id)); let bytes = fs::read(&path).unwrap(); let before = identity(&path);
    let mut cap = inspect(&f); let root = f.root.clone(); let restore = RestoreMode(root.clone());
    let outcome = f.store.remove_damaged_inner(&mut cap, &AtomicBool::new(false), |stage, _, _| {
        if stage == RemoveStage::CleanupReady { fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap(); }
        Ok(())
    }).unwrap();
    drop(restore); drop(cap);
    assert!(!outcome.is_complete()); assert!(!outcome.payload_removed);
    let error = outcome.cleanup_error.unwrap();
    assert!(error.contains(&std::io::Error::from_raw_os_error(libc::EACCES).to_string()), "{error}");
    println!("DAMAGED_ACTUAL_CLEANUP: {error}");
    assert_eq!(identity(&path), before); assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(outcome.remaining, Some(vec![OsString::from(payload_name(f.kept[0].id))]));
    let now = Manifest::decode(&fs::read(root.join(MANIFEST)).unwrap()).unwrap();
    assert_eq!(now.items, f.kept[1..]); assert_eq!(now.next, 3); assert_eq!(now.generation, 3);
    let mut recovery = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    assert!(f.store.discard_interrupted_import(&mut recovery, &AtomicBool::new(false)).unwrap().is_complete()); drop(recovery);
    assert!(!path.exists()); originals(&f);
    let mut reopened = Store::open(&root, policy()).unwrap();
    let fresh = reopened.import_document(OsStr::new("after cleanup"), &f.reader, &AtomicBool::new(false)).unwrap().value;
    assert!(reopened.remove(fresh.id).unwrap().is_complete());
}

#[test]
fn damaged_postcommit_cancel_reports_committed_and_cleanup_replacement_is_preserved() {
    for kind in ["cancel", "replacement", "missing-present", "unknown", "manifest"] {
        let mut f = FaultFixture::new(); damage(&f, if kind == "missing-present" { "missing" } else { "overwrite" });
        let mut cap = inspect(&f); let root = f.root.clone();
        let path = root.join(payload_name(f.kept[0].id)); let cancelled = AtomicBool::new(false);
        let outcome = f.store.remove_damaged_inner(&mut cap, &cancelled, |stage, _, _| {
            if stage == RemoveStage::CleanupReady {
                match kind {
                    "cancel" => cancelled.store(true, Ordering::Relaxed),
                    "replacement" | "missing-present" => {
                        if path.exists() { fs::remove_file(&path).unwrap(); }
                        OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).unwrap().write_all(b"replacement").unwrap();
                    },
                    "unknown" => fs::write(root.join("unknown"), b"keep").unwrap(),
                    "manifest" => {
                        let b = fs::read(root.join(MANIFEST)).unwrap(); fs::write(root.join(MANIFEST), b).unwrap();
                    },
                    _ => unreachable!(),
                }
            }
            Ok(())
        }).unwrap(); drop(cap);
        assert_eq!(outcome.committed_generation, 3);
        if kind == "cancel" { assert!(outcome.is_complete()); assert!(outcome.payload_removed); assert!(!path.exists()); }
        else {
            assert!(!outcome.is_complete()); assert!(!outcome.payload_removed); assert!(outcome.cleanup_error.is_some());
            if kind == "replacement" || kind == "missing-present" { assert_eq!(fs::read(&path).unwrap(), b"replacement"); }
        }
        originals(&f);
    }
}

#[test]
fn damaged_real_cleanup_sync_fd_failure_reports_committed_removed_and_unknown_remainder() {
    let mut f = FaultFixture::new(); damage(&f, "overwrite"); let mut cap = inspect(&f);
    let saved = f.store.root.try_clone().unwrap();
    let outcome = f.store.remove_damaged_inner(&mut cap, &AtomicBool::new(false), |stage, store, _| {
        if stage == RemoveStage::CleanupSync {
            let mut fds = [-1; 2]; assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
            store.root = unsafe { File::from_raw_fd(fds[0]) }; drop(unsafe { File::from_raw_fd(fds[1]) });
        }
        Ok(())
    }).unwrap(); f.store.root = saved; drop(cap);
    assert!(outcome.payload_removed); assert_eq!(outcome.committed_generation, 3);
    assert!(!outcome.is_complete()); assert!(outcome.remaining.is_none()); assert!(outcome.cleanup_error.is_some());
    assert_eq!(f.store.evidence(), None);
    let warning = outcome.sync_warning.unwrap(); assert!(warning.contains(&std::io::Error::from_raw_os_error(libc::EBADF).to_string()), "{warning}");
    println!("DAMAGED_ACTUAL_SYNC: {warning}");
    assert!(!f.root.join(payload_name(f.kept[0].id)).exists());
    assert_eq!(Store::open(&f.root, policy()).unwrap().items(), &f.kept[1..]); originals(&f);
}
