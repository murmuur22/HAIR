//! M1: actual sparse source/import bytes and independent streaming hash oracles.
use super::*;
use std::sync::Arc;
const BOUNDARY: u64 = 64 * 1024 * 1024;
const OFFSET_CEILING: u64 = i64::MAX as u64 - 65536;
#[path = "recovery_budget.rs"]
mod recovery_budget;

fn digest(path: &Path) -> (u64, [u8; 32]) {
    let mut file = File::open(path).unwrap();
    let mut buf = [0; 65536];
    let mut hash = Sha256::new();
    let mut length = 0;
    loop {
        let n = file.read(&mut buf).unwrap();
        if n == 0 { break; }
        hash.update(&buf[..n]); length += n as u64;
    }
    (length, hash.finalize().into())
}
struct LargeFixture {
    f: FaultFixture, selected: Item, path: PathBuf, source: PathBuf, room: PathBuf,
    source_identity: Identity, room_identity: Identity,
}
impl LargeFixture {
    fn new(length: u64, actual: u64) -> Self {
        let mut f = FaultFixture::new();
        let source_dir = f._owner.root("large-source");
        let source = source_dir.join("sparse-original");
        let file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&source).unwrap();
        file.set_len(length).unwrap();
        file.write_all_at(b"actual sparse source", 0).unwrap();
        file.write_all_at(b"Z", length - 1).unwrap(); drop(file);
        let room = f._owner.root("large-room").join("independent-copy");
        fs::copy(&source, &room).unwrap();
        let source_identity = identity(&source); let room_identity = identity(&room);
        let reader = Explorer::new(&source_dir).unwrap().open_reader(0).unwrap();
        let finite_disk = length + f.store.usage() + 1024 * 1024;
        f.store.set_policy(Policy::new(length, finite_disk).unwrap());
        let selected = f.store.import_document(OsStr::new("large selected"), &reader, &AtomicBool::new(false)).unwrap().value;
        assert_eq!(digest(&source), (length, selected.sha256));
        let path = f.root.join(payload_name(selected.id));
        let file = OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(actual).unwrap();
        if actual == length { file.write_all_at(b"!", 0).unwrap(); }
        drop(file);
        Self { f, selected, path, source, room, source_identity, room_identity }
    }
    fn cap(&self, budget: u64) -> DamagedInspection {
        self.f.store.inspect_damaged_item(&self.selected, budget, &AtomicBool::new(false))
            .unwrap_or_else(|e| panic!("explicit finite actual-byte budget {budget} must support inspection: {e}"))
    }
    fn preserved(&self) {
        originals(&self.f);
        for (item, before) in self.f.kept.iter().zip(&self.f.kept_identities) {
            let path = self.f.root.join(payload_name(item.id));
            assert_eq!(&identity(&path), before);
            assert_eq!(fs::read(path).unwrap(), self.f.bytes);
        }
        assert_eq!(identity(&self.source), self.source_identity);
        assert_eq!(identity(&self.room), self.room_identity);
        assert_eq!(digest(&self.source), (self.selected.bytes, self.selected.sha256));
        assert_eq!(digest(&self.room), (self.selected.bytes, self.selected.sha256));
    }
}
fn selected_reads(path: &Path, bytes: u64) {
    let m = fs::metadata(path).unwrap();
    reads_for(m.dev(), m.ino(), bytes);
}
fn reads_for(dev: u64, ino: u64, bytes: u64) {
    let reads = take_body_reads();
    assert!(reads.iter().all(|(d, i, n)| (*d, *i) == (dev, ino) && *n <= 65536 && *n > 0), "unselected/unbounded reads: {reads:?}");
    assert_eq!(reads.iter().map(|(_, _, n)| *n as u64).sum::<u64>(), bytes);
    assert_eq!(reads.len() as u64, bytes.div_ceil(65536));
}
fn vertical(expected: u64, actual: u64) {
    let mut t = LargeFixture::new(expected, actual);
    create(&t.f.store.root, FINDER).unwrap().write_all(b"preserved Finder").unwrap();
    let finder = identity(&t.f.root.join(FINDER));
    let manifest = fs::read(t.f.root.join(MANIFEST)).unwrap();
    let manifest_identity = identity(&t.f.root.join(MANIFEST));
    let root = identity(&t.f.root); let selected = identity(&t.path);
    let observed = digest(&t.path);
    assert_ne!(observed, (expected, t.selected.sha256));
    take_body_reads();
    let cap = t.cap(actual);
    selected_reads(&t.path, actual);
    assert_eq!(cap.expected(), &t.selected);
    assert!(matches!(cap.observed(), ObservedDamage::Regular { bytes, sha256, .. } if (*bytes, *sha256) == observed));
    assert!(t.f.store.refresh().is_err(), "exclusive lock not held");
    drop(cap);
    assert_eq!(identity(&t.f.root), root); assert_eq!(identity(&t.path), selected);
    assert_eq!(identity(&t.f.root.join(MANIFEST)), manifest_identity);
    let mut cap = t.cap(actual); take_body_reads();
    assert!(matches!(t.f.store.remove_damaged_item(&mut cap, &AtomicBool::new(true)), Err(Failure::Refused(e)) if e.contains("cancelled")));
    assert!(t.f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).is_err());
    assert!(take_body_reads().is_empty()); drop(cap);
    assert_eq!(identity(&t.f.root), root); assert_eq!(identity(&t.path), selected);
    assert_eq!(fs::read(t.f.root.join(MANIFEST)).unwrap(), manifest);
    assert!(!t.f.root.join(MANIFEST_PENDING).exists());
    // Storage/preview policy does not become deletion authority or a deletion cap.
    t.f.store.set_policy(Policy::new(1, 1).unwrap());
    let mut cap = t.cap(actual); take_body_reads();
    let outcome = t.f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).unwrap();
    reads_for(selected.0, selected.1, actual);
    assert!(outcome.is_complete(), "{outcome:?}"); assert!(outcome.payload_removed);
    assert_eq!(outcome.removed_id, t.selected.id);
    assert!(t.f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).is_err()); drop(cap);
    assert!(!t.path.exists());
    let mut expected_manifest = Manifest::decode(&manifest).unwrap();
    expected_manifest.generation += 1;
    expected_manifest.items.retain(|i| i.id != t.selected.id);
    assert_eq!(outcome.committed_generation, expected_manifest.generation);
    assert_eq!(fs::read(t.f.root.join(MANIFEST)).unwrap(), expected_manifest.encode().unwrap());
    let mut reopened = Store::open(&t.f.root, policy()).unwrap();
    assert_eq!(reopened.items(), t.f.kept); assert!(reopened.evidence().unwrap().is_empty());
    let fresh = reopened.import_document(OsStr::new("after large removal"), &t.f.reader, &AtomicBool::new(false)).unwrap().value;
    assert_eq!(fresh.id, expected_manifest.next);
    assert!(reopened.remove(fresh.id).unwrap().is_complete());
    expected_manifest.next += 1; expected_manifest.generation += 2;
    assert_eq!(fs::read(t.f.root.join(MANIFEST)).unwrap(), expected_manifest.encode().unwrap());
    assert_eq!(identity(&t.f.root.join(FINDER)), finder);
    assert_eq!(fs::read(t.f.root.join(FINDER)).unwrap(), b"preserved Finder");
    t.preserved();
    println!("LARGE_DAMAGE_VERTICAL expected={expected} actual={actual} finite_budget={actual}: selected-only streamed inspect/drop/cancel/fresh confirm/reopen/import/remove");
}
#[test]
fn large_damage_same_length_wrong_digest_vertical() { vertical(BOUNDARY + 1, BOUNDARY + 1); }
#[test]
fn large_damage_appended_across_boundary_vertical() {
    vertical(BOUNDARY, BOUNDARY + 1);
    vertical(BOUNDARY - 1, BOUNDARY);
}
#[test]
fn large_damage_truncated_around_boundary_vertical() {
    vertical(BOUNDARY + 2, BOUNDARY + 1);
    vertical(BOUNDARY + 1, BOUNDARY - 1);
}
#[test]
fn large_damage_actual_budget_exact_and_plus_one_refusal_before_read() {
    let t = LargeFixture::new(BOUNDARY, BOUNDARY + 1);
    let root = identity(&t.f.root); let before = identity(&t.path);
    let manifest = fs::read(t.f.root.join(MANIFEST)).unwrap();
    for budget in [1, BOUNDARY] {
        take_body_reads();
        let error = t.f.store.inspect_damaged_item(&t.selected, budget, &AtomicBool::new(false)).err().unwrap();
        assert!(error.contains("budget"), "{error}"); assert!(take_body_reads().is_empty());
    }
    take_body_reads(); drop(t.cap(BOUNDARY + 1)); selected_reads(&t.path, BOUNDARY + 1);
    assert_eq!(identity(&t.f.root), root); assert_eq!(identity(&t.path), before);
    assert_eq!(fs::read(t.f.root.join(MANIFEST)).unwrap(), manifest); t.preserved();
}
#[test]
fn large_damage_zero_invalid_and_os_offset_budget_policy() {
    let f = FaultFixture::new(); damage(&f, "overwrite");
    let root = identity(&f.root);
    assert_eq!(MAX_STORAGE_BYTES, OFFSET_CEILING);
    for budget in [0, OFFSET_CEILING + 1, i64::MAX as u64, u64::MAX] {
        take_body_reads();
        let error = f.store.inspect_damaged_item(&f.kept[0], budget, &AtomicBool::new(false)).err().unwrap();
        assert!(error.contains("Invalid damaged-item verification budget"), "{error}");
        assert!(take_body_reads().is_empty());
    }
    for budget in [f.bytes.len() as u64, OFFSET_CEILING] {
        drop(f.store.inspect_damaged_item(&f.kept[0], budget, &AtomicBool::new(false)).unwrap());
    }
    assert!(Policy::new(OFFSET_CEILING, OFFSET_CEILING).is_ok());
    assert!(Policy::new(OFFSET_CEILING + 1, OFFSET_CEILING).is_err());
    assert!(Policy::new(OFFSET_CEILING, OFFSET_CEILING + 1).is_err());
    assert_eq!(identity(&f.root), root); originals(&f);
}
#[test]
fn large_damage_shared_verifier_capture_stays_metadata_bounded() {
    let f = FaultFixture::new();
    let file = create(&f.store.root, PAYLOAD_PENDING).unwrap();
    file.set_len(MANIFEST_BYTES + 1).unwrap();
    let mut budget = OFFSET_CEILING;
    take_body_reads();
    let error = crate::collection_store::recovery::identify_cancelled(&file, &mut budget, true, true, &AtomicBool::new(false)).err().unwrap();
    assert!(error.contains("capture exceeds bounded manifest metadata limit"), "{error}");
    assert_eq!(budget, OFFSET_CEILING); assert!(take_body_reads().is_empty());
    assert_eq!(fs::metadata(f.root.join(PAYLOAD_PENDING)).unwrap().len(), MANIFEST_BYTES + 1);
    originals(&f);
}
#[test]
fn large_damage_precancel_and_midread_inspection_confirmation_are_bounded_writefree() {
    let mut t = LargeFixture::new(BOUNDARY, BOUNDARY + 1);
    let root = identity(&t.f.root); let before = identity(&t.path);
    let manifest = fs::read(t.f.root.join(MANIFEST)).unwrap();
    take_body_reads();
    assert!(t.f.store.inspect_damaged_item(&t.selected, BOUNDARY + 1, &AtomicBool::new(true)).is_err());
    assert!(take_body_reads().is_empty());
    for confirm in [false, true] {
        let mut cap = if confirm { Some(t.cap(BOUNDARY + 1)) } else { None };
        let cancelled = Arc::new(AtomicBool::new(false));
        take_body_reads(); cancel_after_body_read(cancelled.clone());
        if let Some(cap) = &mut cap {
            assert!(matches!(t.f.store.remove_damaged_item(cap, &cancelled), Err(Failure::Refused(e)) if e.contains("cancelled")));
            assert!(t.f.store.remove_damaged_item(cap, &AtomicBool::new(false)).is_err());
        } else {
            let error = t.f.store.inspect_damaged_item(&t.selected, BOUNDARY + 1, &cancelled).err().unwrap();
            assert!(error.contains("cancelled"), "{error}");
        }
        assert_eq!(take_body_reads(), vec![(before.0, before.1, 65536)]); drop(cap);
        assert_eq!(identity(&t.f.root), root); assert_eq!(identity(&t.path), before);
        assert_eq!(fs::read(t.f.root.join(MANIFEST)).unwrap(), manifest);
        assert!(!t.f.root.join(MANIFEST_PENDING).exists());
    }
    t.preserved();
}
#[test]
fn large_damage_late_modified_repaired_rebound_before_activation_refuse() {
    for kind in ["modified", "repaired", "rebound"] {
        let mut t = LargeFixture::new(BOUNDARY, BOUNDARY + 1);
        let manifest = fs::read(t.f.root.join(MANIFEST)).unwrap();
        let mut cap = t.cap(BOUNDARY + 1);
        let result = t.f.store.remove_damaged_inner(&mut cap, &AtomicBool::new(false), |stage, _, _| {
            if stage == RemoveStage::ManifestReady {
                match kind {
                    "modified" => OpenOptions::new().write(true).open(&t.path).unwrap().write_all_at(b"?", BOUNDARY).unwrap(),
                    "repaired" => { fs::copy(&t.source, &t.path).unwrap(); },
                    "rebound" => {
                        let saved = t.f._owner.0.join("held-large"); fs::rename(&t.path, &saved).unwrap();
                        fs::copy(saved, &t.path).unwrap();
                    },
                    _ => unreachable!(),
                }
            }
            Ok(())
        });
        assert!(matches!(result, Err(Failure::Refused(_))), "{kind}: {result:?}"); drop(cap);
        assert_eq!(fs::metadata(&t.path).unwrap().mode() & 0o7777, 0o600, "{kind}: fixture must remain safe regular damage, not an unsafe-mode refusal");
        let before = identity(&t.path); let actual = digest(&t.path);
        assert_eq!(fs::read(t.f.root.join(MANIFEST)).unwrap(), manifest);
        assert!(t.f.root.join(MANIFEST_PENDING).exists());
        // Pending-only recovery observes this large damaged/repaired LIVE metadata,
        // never reads or deletes it. No repair is needed to clear our pending bytes.
        take_body_reads();
        let mut recovery = t.f.store.inspect_interrupted_import(1, &AtomicBool::new(false)).unwrap();
        assert!(t.f.store.discard_interrupted_import(&mut recovery, &AtomicBool::new(false)).unwrap().is_complete()); drop(recovery);
        assert!(take_body_reads().is_empty()); assert_eq!(identity(&t.path), before);
        assert_eq!(digest(&t.path), actual);
        if kind == "repaired" {
            let error = t.f.store.inspect_damaged_item(&t.selected, BOUNDARY + 1, &AtomicBool::new(false)).err().unwrap();
            assert!(error.contains("healthy"), "{error}");
        }
        t.preserved();
    }
}
#[test]
fn large_damage_precommit_cancel_and_kernel_write_evidence_recover_without_live_reads() {
    for fault in ["cancel", "write-empty", "write-prefix"] {
        let mut t = LargeFixture::new(BOUNDARY, BOUNDARY + 1);
        let manifest = fs::read(t.f.root.join(MANIFEST)).unwrap();
        let before = identity(&t.path); let actual = digest(&t.path);
        let mut cap = t.cap(BOUNDARY + 1);
        let cancelled = AtomicBool::new(false); let root = t.f.root.clone();
        let result = t.f.store.remove_damaged_inner(&mut cap, &cancelled, |stage, _, file| {
            if stage == RemoveStage::ManifestStaging && fault.starts_with("write") {
                if fault == "write-prefix" { file.write_all(b"HAIRCOLL").unwrap(); }
                *file = File::open(root.join(MANIFEST_PENDING)).unwrap();
            }
            if stage == RemoveStage::ManifestReady && fault == "cancel" { cancelled.store(true, Ordering::Relaxed); }
            Ok(())
        });
        let Err(Failure::Refused(error)) = result else { panic!("{fault}: expected precommit refusal"); };
        if fault == "cancel" { assert!(error.contains("cancelled")); }
        else { assert!(error.contains(&std::io::Error::from_raw_os_error(libc::EBADF).to_string()), "{error}"); }
        drop(cap);
        assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), manifest);
        let pending = fs::read(root.join(MANIFEST_PENDING)).unwrap();
        if fault == "write-empty" { assert!(pending.is_empty()); }
        if fault == "write-prefix" { assert_eq!(pending, b"HAIRCOLL"); }
        take_body_reads();
        let mut recovery = t.f.store.inspect_interrupted_import(1, &AtomicBool::new(false)).unwrap();
        assert!(t.f.store.discard_interrupted_import(&mut recovery, &AtomicBool::new(false)).unwrap().is_complete()); drop(recovery);
        assert!(take_body_reads().is_empty());
        assert_eq!(identity(&t.path), before); assert_eq!(digest(&t.path), actual);
        assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), manifest);
        let mut cap = t.cap(BOUNDARY + 1);
        assert!(t.f.store.remove_damaged_item(&mut cap, &AtomicBool::new(false)).unwrap().is_complete()); drop(cap);
        let mut reopened = Store::open(&root, policy()).unwrap();
        assert_eq!(reopened.items(), t.f.kept);
        let fresh = reopened.import_document(OsStr::new("after precommit recovery"), &t.f.reader, &AtomicBool::new(false)).unwrap().value;
        assert!(reopened.remove(fresh.id).unwrap().is_complete());
        t.preserved();
        println!("LARGE_DAMAGE_PRECOMMIT {fault}: actual >64MiB LIVE unchanged, pending-only recovery with zero LIVE reads, fresh removal/reopen/import/remove");
    }
}
#[test]
fn large_damage_postcommit_large_orphan_recovery_lifecycle() {
    let mut t = LargeFixture::new(BOUNDARY, BOUNDARY + 1);
    let before = identity(&t.path); let actual = digest(&t.path);
    let mut cap = t.cap(BOUNDARY + 1);
    let root = t.f.root.clone(); let restore = RestoreMode(root.clone());
    let outcome = t.f.store.remove_damaged_inner(&mut cap, &AtomicBool::new(false), |stage, _, _| {
        if stage == RemoveStage::CleanupReady { fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap(); }
        Ok(())
    }).unwrap(); drop(restore); drop(cap);
    assert!(!outcome.is_complete()); assert!(!outcome.payload_removed);
    assert!(outcome.cleanup_error.unwrap().contains(&std::io::Error::from_raw_os_error(libc::EACCES).to_string()));
    assert_eq!(outcome.remaining, Some(vec![OsString::from(payload_name(t.selected.id))]));
    let mut reopened = Store::open(&root, policy()).unwrap();
    assert_eq!(reopened.items(), t.f.kept);
    take_body_reads();
    assert_eq!(identity(&t.path), before); assert_eq!(digest(&t.path), actual);
    let report = reopened.interrupted_import_metadata(&AtomicBool::new(false)).unwrap();
    assert_eq!(report.required_candidate_bytes, actual.0);
    assert_eq!(report.candidates, vec![RecoveryCandidate { name: payload_name(t.selected.id).into(), actual_bytes: actual.0 }]);
    assert!(take_body_reads().is_empty());
    let cap = reopened.inspect_interrupted_import(report.required_candidate_bytes, &AtomicBool::new(false)).unwrap(); drop(cap);
    reads_for(before.0, before.1, actual.0);
    let mut recovery = reopened.inspect_interrupted_import(report.required_candidate_bytes, &AtomicBool::new(false)).unwrap();
    assert!(reopened.discard_interrupted_import(&mut recovery, &AtomicBool::new(true)).is_err()); drop(recovery);
    reads_for(before.0, before.1, actual.0);
    assert_eq!(identity(&t.path), before);
    let committed = fs::read(root.join(MANIFEST)).unwrap(); let committed_id = identity(&root.join(MANIFEST));
    let mut recovery = reopened.inspect_interrupted_import(report.required_candidate_bytes, &AtomicBool::new(false)).unwrap();
    assert!(reopened.discard_interrupted_import(&mut recovery, &AtomicBool::new(false)).unwrap().is_complete()); drop(recovery);
    let reads = take_body_reads();
    assert!(reads.iter().all(|(d, i, n)| (*d, *i) == (before.0, before.1) && *n > 0 && *n <= 65536));
    assert_eq!(reads.iter().map(|(_, _, n)| *n as u64).sum::<u64>(), 3 * actual.0);
    assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), committed); assert_eq!(identity(&root.join(MANIFEST)), committed_id);
    let mut reopened = Store::open(&root, policy()).unwrap();
    let fresh = reopened.import_document(OsStr::new("after orphan recovery"), &t.f.reader, &AtomicBool::new(false)).unwrap().value;
    assert!(reopened.remove(fresh.id).unwrap().is_complete());
    t.preserved();
    println!("LARGE_DAMAGE_RECOVERY: committed cleanup EACCES >64MiB orphan recovered; reopen/import/remove");
}
