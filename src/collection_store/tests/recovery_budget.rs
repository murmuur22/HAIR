//! Ordinary >64 MiB mutation leftovers must have a complete explicit lifecycle.
use super::*;
use crate::collection_store::recovery::RecoveryStage;

fn interrupted(kind: &str) -> (LargeFixture, String) {
    let length = BOUNDARY + 8192;
    let mut t = LargeFixture::new(length, length);
    create(&t.f.store.root, FINDER).unwrap().write_all(b"preserved Finder").unwrap();
    let root = t.f.root.clone();
    let restore = RestoreMode(root.clone());
    let candidate;
    if kind == "healthy-cleanup" {
        fs::copy(&t.source, &t.path).unwrap();
        candidate = payload_name(t.selected.id);
        let result = t.f.store.remove_inner(t.selected.id, &AtomicBool::new(false), |stage, _, _| {
            if stage == RemoveStage::CleanupReady { fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap(); }
            Ok(())
        }).unwrap();
        assert!(!result.is_complete()); assert!(!result.payload_removed);
        assert!(result.cleanup_error.unwrap().contains(&std::io::Error::from_raw_os_error(libc::EACCES).to_string()));
    } else {
        let reader = Explorer::new(t.source.parent().unwrap()).unwrap().open_reader(0).unwrap();
        t.f.store.set_policy(Policy::new(length, 3 * length).unwrap());
        candidate = if matches!(kind, "midstream" | "midwrite") { PAYLOAD_PENDING.into() } else { payload_name(t.f.store.view.next) };
        let cancelled = AtomicBool::new(false);
        let result = t.f.store.import_inner(OsStr::new("large interrupted"), &reader, &cancelled, |stage, file| {
            if (kind == "midstream" && stage == ImportStage::ChunkWritten && file.metadata().unwrap().len() > BOUNDARY)
                || (kind == "manifest-ready" && stage == ImportStage::ManifestReady) {
                cancelled.store(true, Ordering::Relaxed);
            }
            if kind == "manifest-create" && stage == ImportStage::PayloadPublished {
                fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();
            }
            if kind == "midwrite" && stage == ImportStage::ChunkWritten && file.metadata().unwrap().len() > BOUNDARY {
                *file = File::open(root.join(PAYLOAD_PENDING)).unwrap();
            }
            Ok(())
        });
        let reason = if kind == "manifest-create" { std::io::Error::from_raw_os_error(libc::EACCES).to_string() }
            else if kind == "midwrite" { std::io::Error::from_raw_os_error(libc::EBADF).to_string() }
            else { "cancelled".into() };
        refused(result, &reason);
    }
    drop(restore);
    assert!(fs::metadata(root.join(&candidate)).unwrap().len() > BOUNDARY);
    (t, candidate)
}

fn lifecycle(kind: &str) {
    let (mut t, candidate) = interrupted(kind);
    let manifest = fs::read(t.f.root.join(MANIFEST)).unwrap();
    let manifest_identity = identity(&t.f.root.join(MANIFEST));
    let root = identity(&t.f.root);
    let path = t.f.root.join(&candidate); let before = identity(&path);
    let candidate_digest = digest(&path);
    let live = if kind == "healthy-cleanup" { None } else { Some((identity(&t.path), digest(&t.path))) };
    let finder = identity(&t.f.root.join(FINDER));
    let pending = t.f.root.join(MANIFEST_PENDING);
    let pending_before = if pending.exists() { Some((identity(&pending), fs::read(&pending).unwrap())) } else { None };
    take_body_reads();
    let report = t.f.store.interrupted_import_metadata(&AtomicBool::new(false)).unwrap();
    assert!(take_body_reads().is_empty());
    assert_eq!(report.required_candidate_bytes, before.2);
    assert_eq!(report.candidates.len(), if kind == "manifest-ready" { 2 } else { 1 });
    for c in &report.candidates { assert_eq!(c.actual_bytes, fs::metadata(t.f.root.join(&c.name)).unwrap().len()); }
    // Report is non-authorizing and releases its temporary lock.
    t.f.store.refresh().unwrap(); assert_eq!(identity(&t.f.root), root);
    for budget in [BOUNDARY, report.required_candidate_bytes - 1] {
        let error = t.f.store.inspect_interrupted_import(budget, &AtomicBool::new(false)).err().unwrap();
        assert!(error.contains("budget"), "{error}"); assert!(take_body_reads().is_empty());
    }
    t.f.store.set_policy(Policy::new(1, 1).unwrap());
    let cap = t.f.store.inspect_interrupted_import(report.required_candidate_bytes, &AtomicBool::new(false)).unwrap();
    assert_eq!(cap.max_bytes(), report.required_candidate_bytes); drop(cap);
    reads_for(before.0, before.1, before.2);
    let mut cap = t.f.store.inspect_interrupted_import(report.required_candidate_bytes, &AtomicBool::new(false)).unwrap();
    assert!(cap.candidates().contains(&OsString::from(&candidate)));
    reads_for(before.0, before.1, before.2);
    assert!(t.f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(true)).is_err());
    assert!(t.f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(false)).unwrap_err().contains("consumed")); drop(cap);
    assert!(take_body_reads().is_empty());
    assert_eq!(identity(&t.f.root), root); assert_eq!(identity(&path), before);
    assert_eq!(digest(&path), candidate_digest);
    if let Some((id, bytes)) = pending_before { assert_eq!(identity(&pending), id); assert_eq!(fs::read(&pending).unwrap(), bytes); }
    let mut cap = t.f.store.inspect_interrupted_import(report.required_candidate_bytes, &AtomicBool::new(false)).unwrap();
    assert!(t.f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(false)).unwrap().is_complete()); drop(cap);
    let reads = take_body_reads();
    assert!(reads.iter().all(|(d, i, n)| (*d, *i) == (before.0, before.1) && *n > 0 && *n <= 65536), "LIVE/unbounded reads: {reads:?}");
    assert_eq!(reads.iter().map(|(_, _, n)| *n as u64).sum::<u64>(), 3 * before.2);
    assert_eq!(fs::read(t.f.root.join(MANIFEST)).unwrap(), manifest);
    assert_eq!(identity(&t.f.root.join(MANIFEST)), manifest_identity);
    assert_eq!(identity(&t.f.root.join(FINDER)), finder);
    assert_eq!(fs::read(t.f.root.join(FINDER)).unwrap(), b"preserved Finder");
    if let Some((id, bytes)) = live { assert_eq!(identity(&t.path), id); assert_eq!(digest(&t.path), bytes); }
    let mut reopened = Store::open(&t.f.root, Policy::new(t.selected.bytes, 3 * t.selected.bytes).unwrap()).unwrap();
    let fresh = reopened.import_document(OsStr::new("after large recovery"), &t.f.reader, &AtomicBool::new(false)).unwrap().value;
    assert!(reopened.remove(fresh.id).unwrap().is_complete());
    let mut expected = Manifest::decode(&manifest).unwrap();
    expected.next += 1; expected.generation += 2;
    assert_eq!(fs::read(t.f.root.join(MANIFEST)).unwrap(), expected.encode().unwrap());
    t.preserved();
    println!("RECOVERY_BUDGET_LIFECYCLE {kind}: actual_bytes={} metadata/body0; exact-budget inspect/drop/cancel/fresh-confirm; candidate-only three-pass discard; reopen/import/remove", before.2);
}

#[test]
fn recovery_budget_midstream_large_pending_lifecycle() { lifecycle("midstream"); }
#[test]
fn recovery_budget_midwrite_ebadf_large_pending_lifecycle() { lifecycle("midwrite"); }
#[test]
fn recovery_budget_manifest_ready_large_pair_lifecycle() { lifecycle("manifest-ready"); }
#[test]
fn recovery_budget_manifest_create_eacces_large_lone_next_lifecycle() { lifecycle("manifest-create"); }
#[test]
fn recovery_budget_healthy_cleanup_eacces_large_older_orphan_lifecycle() { lifecycle("healthy-cleanup"); }

#[test]
fn recovery_budget_invalid_and_precancel_before_lock_or_reads() {
    let (mut t, candidate) = interrupted("midstream");
    let root = identity(&t.f.root); let before = identity(&t.f.root.join(&candidate));
    let held = t.f.store.lock().unwrap(); take_body_reads();
    for budget in [0, MAX_STORAGE_BYTES + 1, i64::MAX as u64, u64::MAX] {
        let error = t.f.store.inspect_interrupted_import(budget, &AtomicBool::new(false)).err().unwrap();
        assert!(error.contains("Invalid recovery verification budget"), "lock attempted before invalid budget: {error}");
    }
    let error = t.f.store.inspect_interrupted_import(before.2, &AtomicBool::new(true)).err().unwrap();
    assert!(error.contains("cancelled"), "{error}");
    assert!(t.f.store.interrupted_import_metadata(&AtomicBool::new(true)).unwrap_err().contains("cancelled"));
    assert!(take_body_reads().is_empty()); drop(held);
    let cap = t.f.store.inspect_interrupted_import(MAX_STORAGE_BYTES, &AtomicBool::new(false)).unwrap(); drop(cap);
    reads_for(before.0, before.1, before.2);
    let mut cap = t.f.store.inspect_interrupted_import(before.2, &AtomicBool::new(false)).unwrap(); take_body_reads();
    assert!(t.f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(true)).unwrap_err().contains("cancelled"));
    assert!(t.f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(false)).unwrap_err().contains("consumed")); drop(cap);
    assert!(take_body_reads().is_empty());
    assert_eq!(identity(&t.f.root), root); assert_eq!(identity(&t.f.root.join(candidate)), before); t.preserved();
}

#[test]
fn recovery_budget_midread_inspect_confirm_and_unlink_boundary_stop_one_real_chunk() {
    let (mut t, candidate) = interrupted("manifest-ready");
    let path = t.f.root.join(&candidate); let before = identity(&path); let actual = digest(&path);
    let root = identity(&t.f.root); let manifest = identity(&t.f.root.join(MANIFEST));
    let pending = identity(&t.f.root.join(MANIFEST_PENDING)); let pending_bytes = fs::read(t.f.root.join(MANIFEST_PENDING)).unwrap();
    for stage in ["inspect", "confirm", "boundary"] {
        let mut cap = if stage != "inspect" { Some(t.f.store.inspect_interrupted_import(before.2, &AtomicBool::new(false)).unwrap()) } else { None };
        let cancelled = Arc::new(AtomicBool::new(false)); take_body_reads();
        if stage != "boundary" { cancel_after_body_read(cancelled.clone()); }
        if let Some(cap) = &mut cap {
            if stage == "boundary" {
                let outcome = t.f.store.discard_recovery_inner(cap, &cancelled, |s, _| {
                    if s == RecoveryStage::BeforeUnlink(0) { take_body_reads(); cancel_after_body_read(cancelled.clone()); }
                    Ok(())
                }).unwrap();
                assert!(outcome.removed.is_empty()); assert!(!outcome.is_complete());
                assert!(outcome.error.unwrap().contains("cancelled"));
                assert_eq!(outcome.remaining, Some(vec![candidate.clone().into(), MANIFEST_PENDING.into()]));
            } else {
                assert!(t.f.store.discard_interrupted_import(cap, &cancelled).unwrap_err().contains("cancelled"));
            }
            assert!(t.f.store.discard_interrupted_import(cap, &AtomicBool::new(false)).unwrap_err().contains("consumed"));
        } else {
            let error = t.f.store.inspect_interrupted_import(before.2, &cancelled).err().unwrap();
            assert!(error.contains("cancelled"), "{error}");
        }
        assert_eq!(take_body_reads(), vec![(before.0, before.1, 65536)]); drop(cap);
        assert_eq!(identity(&t.f.root), root); assert_eq!(identity(&path), before); assert_eq!(digest(&path), actual);
        assert_eq!(identity(&t.f.root.join(MANIFEST)), manifest);
        assert_eq!(identity(&t.f.root.join(MANIFEST_PENDING)), pending);
        assert_eq!(fs::read(t.f.root.join(MANIFEST_PENDING)).unwrap(), pending_bytes);
    }
    t.preserved();
}

#[test]
fn recovery_budget_partial_unlink_cancellation_reports_progress_then_fresh_inspection() {
    let (mut t, candidate) = interrupted("manifest-ready");
    let bytes = fs::metadata(t.f.root.join(&candidate)).unwrap().len();
    let manifest = fs::read(t.f.root.join(MANIFEST)).unwrap();
    let pending = identity(&t.f.root.join(MANIFEST_PENDING));
    let mut cap = t.f.store.inspect_interrupted_import(bytes, &AtomicBool::new(false)).unwrap();
    let cancelled = AtomicBool::new(false);
    let outcome = t.f.store.discard_recovery_inner(&mut cap, &cancelled, |stage, _| {
        if stage == RecoveryStage::UnlinkReady(1) { cancelled.store(true, Ordering::Relaxed); }
        Ok(())
    }).unwrap();
    assert_eq!(outcome.removed, vec![OsString::from(&candidate)]);
    assert_eq!(outcome.remaining, Some(vec![MANIFEST_PENDING.into()]));
    assert!(outcome.error.unwrap().contains("cancelled")); assert!(outcome.sync_warning.is_none());
    assert!(!t.f.root.join(candidate).exists()); assert_eq!(identity(&t.f.root.join(MANIFEST_PENDING)), pending);
    assert!(t.f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(false)).unwrap_err().contains("consumed")); drop(cap);
    take_body_reads();
    let report = t.f.store.interrupted_import_metadata(&AtomicBool::new(false)).unwrap();
    assert_eq!(report.required_candidate_bytes, 0);
    assert_eq!(report.candidates, vec![RecoveryCandidate { name: MANIFEST_PENDING.into(), actual_bytes: pending.2 }]);
    let mut cap = t.f.store.inspect_interrupted_import(1, &AtomicBool::new(false)).unwrap();
    assert!(t.f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(false)).unwrap().is_complete()); drop(cap);
    assert!(take_body_reads().is_empty()); assert_eq!(fs::read(t.f.root.join(MANIFEST)).unwrap(), manifest);
    let mut reopened = Store::open(&t.f.root, Policy::new(t.selected.bytes, 3 * t.selected.bytes).unwrap()).unwrap();
    let fresh = reopened.import_document(OsStr::new("after partial discard"), &t.f.reader, &AtomicBool::new(false)).unwrap().value;
    assert!(reopened.remove(fresh.id).unwrap().is_complete()); t.preserved();
}

#[test]
fn recovery_budget_metadata_report_is_not_fresh_authority() {
    let (mut t, candidate) = interrupted("midstream");
    let path = t.f.root.join(candidate);
    let report = t.f.store.interrupted_import_metadata(&AtomicBool::new(false)).unwrap();
    let file = OpenOptions::new().write(true).open(&path).unwrap(); file.set_len(report.required_candidate_bytes + 1).unwrap(); drop(file);
    let root = identity(&t.f.root); let before = identity(&path); take_body_reads();
    let error = t.f.store.inspect_interrupted_import(report.required_candidate_bytes, &AtomicBool::new(false)).err().unwrap();
    assert!(error.contains("budget")); assert!(take_body_reads().is_empty());
    let fresh = t.f.store.interrupted_import_metadata(&AtomicBool::new(false)).unwrap();
    assert_eq!(fresh.required_candidate_bytes, report.required_candidate_bytes + 1);
    assert_eq!(fresh.candidates[0].actual_bytes, before.2);
    assert_eq!(identity(&t.f.root), root); assert_eq!(identity(&path), before);
    let mut cap = t.f.store.inspect_interrupted_import(fresh.required_candidate_bytes, &AtomicBool::new(false)).unwrap();
    assert!(t.f.store.discard_interrupted_import(&mut cap, &AtomicBool::new(false)).unwrap().is_complete()); drop(cap);
    t.preserved();
}
