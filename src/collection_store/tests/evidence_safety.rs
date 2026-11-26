use super::*;
use std::os::unix::fs::symlink;

#[derive(Debug, PartialEq, Eq)]
struct Entry(OsString, u64, u64, u64, u32, u32, u64, i64, i64, i64, i64, Option<Vec<u8>>, Option<PathBuf>);
fn snapshot(root: &Path) -> Vec<Entry> {
    let mut paths: Vec<_> = fs::read_dir(root).unwrap().map(|e| e.unwrap().path()).collect();
    paths.sort();
    paths.into_iter().map(|path| {
        let m = fs::symlink_metadata(&path).unwrap();
        Entry(path.file_name().unwrap().into(), m.dev(), m.ino(), m.len(), m.mode(), m.uid(), m.nlink(),
            m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec(),
            if m.is_file() && m.len() <= MANIFEST_BYTES { Some(fs::read(&path).unwrap()) } else { None },
            if m.file_type().is_symlink() { Some(fs::read_link(path).unwrap()) } else { None })
    }).collect()
}
fn refused_unchanged(f: &FaultFixture) {
    let before = snapshot(&f.root);
    let root_before = identity(&f.root);
    take_body_reads();
    if let Ok(report) = f.store.interrupted_import_metadata(&AtomicBool::new(false)) {
        // Only the deliberately caller-underbudgeted safe candidate can report;
        // unsupported/unsafe/unknown evidence must refuse even a planning report.
        assert!(report.required_candidate_bytes > 64 * 1024 * 1024, "unsafe/unknown evidence reported: {report:?}");
    }
    assert!(take_body_reads().is_empty());
    assert!(f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err());
    assert_eq!(snapshot(&f.root), before);
    assert_eq!(identity(&f.root), root_before);
    assert_eq!(fs::read(&f.original).unwrap(), f.bytes);
    assert_eq!(identity(&f.original), f.original_identity);
    assert_eq!(fs::read(&f.room).unwrap(), f.bytes);
    assert_eq!(identity(&f.room), f.room_identity);
    for (item, id) in f.kept.iter().zip(&f.kept_identities) {
        assert_eq!(&identity(&f.root.join(payload_name(item.id))), id);
        assert_eq!(fs::read(f.root.join(payload_name(item.id))).unwrap(), f.bytes);
    }
}

#[test]
fn evidence_pending_only_strict_removal_successors_and_real_write_prefixes() {
    for selected in [0, 1] {
        let mut f = FaultFixture::new();
        let cancelled = AtomicBool::new(false);
        let result = f.store.remove_inner(f.kept[selected].id, &cancelled, |stage, _, _| {
            if stage == RemoveStage::ManifestReady { cancelled.store(true, Ordering::Relaxed); }
            Ok(())
        });
        assert!(matches!(result, Err(Failure::Refused(error)) if error.contains("cancelled")));
        let pending = fs::read(f.root.join(MANIFEST_PENDING)).unwrap();
        complete(&mut f, vec![(MANIFEST_PENDING.into(), pending)]);
    }
    for prefix in [0, 17, 80] {
        let mut f = FaultFixture::new();
        let mut next = Manifest::decode(&f.manifest).unwrap();
        next.generation += 1;
        next.items.remove(0);
        let bytes = next.encode().unwrap();
        let root = f.root.clone();
        let result = f.store.remove_inner(f.kept[0].id, &AtomicBool::new(false), |stage, _, file| {
            if stage == RemoveStage::ManifestStaging {
                file.write_all(&bytes[..prefix]).unwrap();
                *file = File::open(root.join(MANIFEST_PENDING)).unwrap();
            }
            Ok(())
        });
        match result {
            Err(Failure::Refused(error)) => {
                assert!(error.contains("Manifest write failed") && error.contains(&std::io::Error::from_raw_os_error(libc::EBADF).to_string()), "{error}");
                println!("EVIDENCE_ACTUAL_REMOVE_MANIFEST prefix={prefix}: {error}");
            },
            other => panic!("not real EBADF: {other:?}"),
        }
        complete(&mut f, vec![(MANIFEST_PENDING.into(), bytes[..prefix].to_vec())]);
    }
}

#[test]
fn evidence_partial_boundary_refuses_foreign_or_changed_predecessors_and_namespaces() {
    for paired in [false, true] {
        for case in ["magic", "schema", "generation", "allocator", "live-prefix", "checksum", "new-item-metadata", "pending-budget", "extra-unknown", "payload-and-manifest"] {
            let mut f = FaultFixture::new();
            interrupt(&mut f, true);
            let pending = f.root.join(MANIFEST_PENDING);
            let mut bytes = fs::read(&pending).unwrap();
            match case {
                "magic" => { bytes.truncate(8); bytes[0] ^= 1; },
                "schema" => { bytes.truncate(12); bytes[8] = 99; },
                "generation" => { bytes.truncate(20); bytes[12] ^= 1; },
                "allocator" => { bytes.truncate(28); bytes[20] ^= 1; },
                "live-prefix" => { bytes.truncate(80); bytes[50] ^= 1; },
                "checksum" => { let last = bytes.len() - 1; bytes[last] ^= 1; },
                "new-item-metadata" => bytes.truncate(f.manifest.len() - 32 + 9),
                "pending-budget" => bytes.resize(MANIFEST_BYTES as usize + 1, 0),
                "extra-unknown" => { create(&f.store.root, "foreign-note").unwrap().write_all(b"keep").unwrap(); bytes.clear(); },
                "payload-and-manifest" => { create(&f.store.root, PAYLOAD_PENDING).unwrap(); bytes.clear(); },
                _ => unreachable!(),
            }
            fs::write(&pending, bytes).unwrap();
            if !paired { fs::remove_file(f.root.join(f.orphan())).unwrap(); }
            refused_unchanged(&f);
        }
    }
}

#[test]
fn evidence_pending_only_valid_but_not_exact_removal_refuses() {
    for case in ["same", "two-removed", "changed-kept-name", "changed-kept-digest", "next-increment", "generation-skip", "added-record", "import-altered-prefix"] {
        let f = FaultFixture::new();
        let mut pending = Manifest::decode(&f.manifest).unwrap();
        pending.generation += 1;
        pending.items.remove(0);
        match case {
            "same" => pending = Manifest::decode(&f.manifest).unwrap(),
            "two-removed" => pending.items.clear(),
            "changed-kept-name" => pending.items[0].name = "rewritten".into(),
            "changed-kept-digest" => pending.items[0].sha256[0] ^= 1,
            "next-increment" => pending.next += 1,
            "generation-skip" => pending.generation += 1,
            "added-record" => { pending.items = f.kept.clone(); pending.next += 1; let mut added = f.kept[0].clone(); added.id = pending.next - 1; pending.items.push(added); pending.items[0].bytes += 1; },
            "import-altered-prefix" => { pending.items = f.kept.clone(); let mut added = f.kept[0].clone(); added.id = pending.next; pending.next += 1; pending.items.push(added); pending.items[0].name = "changed".into(); },
            _ => unreachable!(),
        }
        create(&f.store.root, MANIFEST_PENDING).unwrap().write_all(&pending.encode().unwrap()).unwrap();
        refused_unchanged(&f);
    }
}

#[test]
fn evidence_generated_names_types_links_metadata_and_bounds_refuse() {
    for case in ["zero", "future", "short", "uppercase", "plus", "semantic-name", "symlink", "directory", "fifo", "hardlink", "mode", "budget", "unknown"] {
        let mut f = FaultFixture::new();
        let name = match case {
            "zero" => "item-0000000000000000".into(), "future" => "item-0000000000000004".into(),
            "short" => "item-3".into(), "uppercase" => "item-000000000000000A".into(),
            "plus" => "item-+000000000000003".into(), "semantic-name" => "unselected-one".into(), _ => f.orphan(),
        };
        let path = f.root.join(&name);
        match case {
            "symlink" => symlink(&f.original, &path).unwrap(),
            "directory" => { fs::create_dir(&path).unwrap(); fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap(); },
            "fifo" => { let path = CString::new(path.as_os_str().as_bytes()).unwrap(); assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0); },
            "hardlink" => {
                fs::hard_link(&f.original, &path).unwrap();
                // Fixture setup itself changes the source inode's ctime/link
                // count. Recovery must preserve that actual post-setup state.
                f.original_identity = identity(&f.original);
            },
            _ => {
                let mut file = create(&f.store.root, &name).unwrap();
                file.write_all(b"bad candidate contents").unwrap();
                if case == "mode" { fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap(); }
                if case == "budget" { file.set_len(64 * 1024 * 1024 + 1).unwrap(); }
                if case == "unknown" { create(&f.store.root, "unknown").unwrap(); }
            },
        }
        refused_unchanged(&f);
    }
}

#[test]
fn evidence_new_shapes_stale_rebind_unknown_and_activated_item_are_never_removed() {
    for shape in ["lone-next", "partial-pair", "removal-pending"] {
        for change in ["content", "rebind", "unknown", "manifest", "activate"] {
            let mut f = FaultFixture::new();
            interrupt(&mut f, true);
            let successor = fs::read(f.root.join(MANIFEST_PENDING)).unwrap();
            let candidate = match shape {
                "lone-next" => { fs::remove_file(f.root.join(MANIFEST_PENDING)).unwrap(); f.orphan() },
                "partial-pair" => { fs::write(f.root.join(MANIFEST_PENDING), &successor[..80]).unwrap(); f.orphan() },
                "removal-pending" => {
                    fs::remove_file(f.root.join(f.orphan())).unwrap();
                    let mut removal = Manifest::decode(&f.manifest).unwrap();
                    removal.generation += 1; removal.items.remove(0);
                    fs::write(f.root.join(MANIFEST_PENDING), removal.encode().unwrap()).unwrap();
                    MANIFEST_PENDING.into()
                },
                _ => unreachable!(),
            };
            let mut inspected = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
            match change {
                "content" => { OpenOptions::new().write(true).open(f.root.join(&candidate)).unwrap().write_all(b"changed").unwrap(); },
                "rebind" => {
                    let bytes = fs::read(f.root.join(&candidate)).unwrap();
                    fs::rename(f.root.join(&candidate), f._owner.0.join("saved-candidate")).unwrap();
                    create(&f.store.root, &candidate).unwrap().write_all(&bytes).unwrap();
                },
                "unknown" => { create(&f.store.root, "new-unknown").unwrap(); },
                "manifest" => {
                    let mut active = Manifest::decode(&f.manifest).unwrap(); active.generation += 1;
                    fs::write(f.root.join(MANIFEST), active.encode().unwrap()).unwrap();
                },
                "activate" => {
                    if shape == "removal-pending" {
                        rename(&f.store.root, MANIFEST_PENDING, MANIFEST, false).unwrap();
                    } else {
                        if shape == "lone-next" { create(&f.store.root, MANIFEST_PENDING).unwrap().write_all(&successor).unwrap(); }
                        else { fs::write(f.root.join(MANIFEST_PENDING), &successor).unwrap(); }
                        rename(&f.store.root, MANIFEST_PENDING, MANIFEST, false).unwrap();
                    }
                },
                _ => unreachable!(),
            }
            let before = snapshot(&f.root);
            let root_before = identity(&f.root);
            assert!(f.store.discard_interrupted_import(&mut inspected, &AtomicBool::new(false)).is_err(), "{shape}/{change}");
            drop(inspected);
            assert_eq!(snapshot(&f.root), before, "{shape}/{change}");
            assert_eq!(identity(&f.root), root_before);
            if change == "activate" && shape != "removal-pending" {
                let active = Manifest::decode(&successor).unwrap();
                let reopened = Store::open(&f.root, policy()).unwrap();
                assert_eq!(reopened.items(), active.items);
                assert!(reopened.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err(), "live item misclassified as candidate");
                assert_eq!(snapshot(&f.root), before);
                assert_eq!(fs::read(f.root.join(f.orphan())).unwrap(), f.bytes);
            }
            assert_eq!(identity(&f.original), f.original_identity);
            assert_eq!(identity(&f.room), f.room_identity);
        }
    }
}

#[test]
fn evidence_unknown_remainder_after_last_unlink_is_not_completion() {
    let mut f = FaultFixture::new();
    let evidence = failed_manifest(&mut f, 17);
    let mut inspected = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    let outcome = f.store.discard_recovery_inner(&mut inspected, &AtomicBool::new(false), |stage, store| {
        if stage == RecoveryStage::BeforeSync { create(&store.root, "new-unknown").unwrap().write_all(b"preserve").unwrap(); }
        Ok(())
    }).unwrap();
    assert_eq!(outcome.removed.len(), evidence.len());
    assert_eq!(outcome.remaining, Some(vec![OsString::from("new-unknown")]));
    assert!(outcome.error.is_some() && !outcome.is_complete());
    drop(inspected);
    refused_unchanged(&f);
    f.check(vec![("new-unknown".into(), b"preserve".to_vec())]);
}

#[test]
fn evidence_real_payload_sync_descriptor_failure_keeps_reserved_staging() {
    let mut f = FaultFixture::new();
    let mut written = 0;
    let len = f.bytes.len();
    let result = f.store.import_inner(OsStr::new("sync-fail"), &f.reader, &AtomicBool::new(false), |stage, file| {
        if stage == ImportStage::ChunkWritten {
            written += if len - written >= READER_CHUNK_BYTES { READER_CHUNK_BYTES } else { len - written };
            if written == len {
                let mut fds = [-1; 2];
                assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
                *file = unsafe { File::from_raw_fd(fds[0]) };
                drop(unsafe { File::from_raw_fd(fds[1]) });
            }
        }
        Ok(())
    });
    match result {
        Err(Failure::Refused(error)) => {
            assert!(error.contains("payload sync failed"), "{error}");
            println!("EVIDENCE_ACTUAL_PAYLOAD_SYNC_DESCRIPTOR: {error}");
        },
        other => panic!("not kernel sync failure: {other:?}"),
    }
    let bytes = f.bytes.clone();
    complete(&mut f, vec![(PAYLOAD_PENDING.into(), bytes)]);
}

#[test]
fn evidence_real_namespace_sync_descriptor_failure_leaves_lone_next() {
    let mut f = FaultFixture::new();
    let saved = f.store.root.try_clone().unwrap();
    let root_fd = f.store.root.as_raw_fd();
    let result = f.store.import_inner(OsStr::new("namespace-sync-fail"), &f.reader, &AtomicBool::new(false), |stage, _| {
        if stage == ImportStage::PayloadPublished {
            let mut fds = [-1; 2];
            assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
            let read = unsafe { File::from_raw_fd(fds[0]) };
            let write = unsafe { File::from_raw_fd(fds[1]) };
            // Replace only this owned fixture Store descriptor after the real
            // rename; the ordinary root.sync_all syscall must fail on the pipe.
            assert_eq!(unsafe { libc::dup2(read.as_raw_fd(), root_fd) }, root_fd);
            drop((read, write));
        }
        Ok(())
    });
    assert_eq!(unsafe { libc::dup2(saved.as_raw_fd(), root_fd) }, root_fd);
    match result {
        Err(Failure::Refused(error)) => {
            assert!(error.contains("Payload namespace sync failed"), "{error}");
            assert!(error.contains(&std::io::Error::from_raw_os_error(libc::EBADF).to_string()), "{error}");
            println!("EVIDENCE_ACTUAL_NAMESPACE_SYNC_DESCRIPTOR: {error}");
        },
        other => panic!("not kernel sync failure: {other:?}"),
    }
    let evidence = vec![(f.orphan(), f.bytes.clone())];
    complete(&mut f, evidence);
}

#[test]
fn evidence_removed_id_becoming_active_invalidates_old_capability() {
    let mut f = FaultFixture::new();
    let selected = f.kept[0].clone();
    let root = f.root.clone();
    let restore = RestoreMode(root.clone());
    let commit = f.store.remove_inner(selected.id, &AtomicBool::new(false), |stage, _, _| {
        if stage == RemoveStage::CleanupReady { fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap(); }
        Ok(())
    }).unwrap();
    drop(restore);
    assert!(!commit.is_complete());
    assert!(commit.cleanup_error.is_some());
    assert!(commit.sync_warning.is_none());
    let mut inspected = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
    assert_eq!(inspected.candidates(), &[OsString::from(payload_name(selected.id))]);
    // Deliberately rewrite a valid active manifest while the capability holds
    // the lock: stale/hostile-same-UID negative, NOT an ordinary ID reuse path.
    let mut active = Manifest::decode(&fs::read(root.join(MANIFEST)).unwrap()).unwrap();
    active.generation += 1;
    active.items.insert(0, selected.clone());
    fs::write(root.join(MANIFEST), active.encode().unwrap()).unwrap();
    let before = snapshot(&root);
    let root_before = identity(&root);
    assert!(f.store.discard_interrupted_import(&mut inspected, &AtomicBool::new(false)).is_err());
    drop(inspected);
    assert_eq!(snapshot(&root), before);
    assert_eq!(identity(&root), root_before);
    let reopened = Store::open(&root, policy()).unwrap();
    assert!(reopened.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err());
    assert_eq!(fs::read(root.join(payload_name(selected.id))).unwrap(), f.bytes);
    assert_eq!(snapshot(&root), before);
    assert_eq!(identity(&f.original), f.original_identity);
    assert_eq!(identity(&f.room), f.room_identity);
}

#[test]
fn evidence_lone_next_requires_current_import_allocator_capacity() {
    for case in ["generation", "allocator", "count"] {
        let f = FaultFixture::new();
        let mut live = Manifest::decode(&f.manifest).unwrap();
        match case {
            "generation" => live.generation = u64::MAX,
            "allocator" => live.next = u64::MAX,
            "count" => {
                while live.items.len() < MAX_ITEMS {
                    let id = live.next;
                    live.next += 1;
                    create(&f.store.root, &payload_name(id)).unwrap();
                    live.items.push(Item { id, name: "metadata-only-fixture".into(), bytes: 0,
                        category: Category::Document, sha256: Sha256::digest([]).into() });
                }
            },
            _ => unreachable!(),
        }
        fs::write(f.root.join(MANIFEST), live.encode().unwrap()).unwrap();
        create(&f.store.root, &payload_name(live.next)).unwrap().write_all(b"impossible import candidate").unwrap();
        refused_unchanged(&f);
    }
}
