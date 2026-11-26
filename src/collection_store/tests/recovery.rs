//! Actual owned interrupted imports; no real cache or fabricated I/O results.
use super::*;
#[path = "recovery_safety.rs"]
mod safety;
#[path = "evidence.rs"]
mod evidence;

fn interrupt(f: &mut FaultFixture, late: bool) -> Vec<(String, Vec<u8>)> {
    let cancelled = AtomicBool::new(false);
    let result = f.store.import_inner(OsStr::new("interrupted"), &f.reader, &cancelled, |stage, _| {
        if stage == if late { ImportStage::ManifestReady } else { ImportStage::ChunkWritten } {
            cancelled.store(true, Ordering::Relaxed);
        }
        Ok(())
    });
    refused(result, "cancelled");
    let names = if late { vec![f.orphan(), MANIFEST_PENDING.into()] } else { vec![PAYLOAD_PENDING.into()] };
    names.into_iter().map(|n| { let bytes = fs::read(f.root.join(&n)).unwrap(); (n, bytes) }).collect()
}

#[test]
fn recovery_cancel_inspect_fresh_confirm_reopen_import_remove() {
    for late in [false, true] {
        let mut f = FaultFixture::new();
        let evidence = interrupt(&mut f, late);
        f.check(evidence.clone());
        let identities: Vec<_> = evidence.iter().map(|(n, _)| identity(&f.root.join(n))).collect();
        let root_before = identity(&f.root);
        let lock_before = identity(&f.root.join("lock"));
        let mut inspection = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
        assert_eq!(inspection.candidates(), f.store.evidence().unwrap());
        assert_eq!(identity(&f.root), root_before, "inspection wrote namespace");
        let error = f.store.discard_interrupted_import(&mut inspection, &AtomicBool::new(true)).unwrap_err();
        assert!(error.contains("cancelled"), "{error}");
        drop(inspection);
        for ((n, _), before) in evidence.iter().zip(&identities) { assert_eq!(&identity(&f.root.join(n)), before); }
        assert_eq!(identity(&f.root), root_before, "cancelled recovery wrote namespace");
        f.check(evidence);
        let mut inspection = f.store.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).unwrap();
        let outcome = f.store.discard_interrupted_import(&mut inspection, &AtomicBool::new(false)).unwrap();
        assert!(outcome.error.is_none() && outcome.sync_warning.is_none(), "{outcome:?}");
        assert_eq!(outcome.removed.len(), identities.len());
        assert_eq!(outcome.remaining, Some(Vec::new()));
        assert!(f.store.discard_interrupted_import(&mut inspection, &AtomicBool::new(false)).is_err(), "retry reused capability");
        drop(inspection);
        assert_eq!(identity(&f.root.join("lock")), lock_before);
        f.check(Vec::new());
        f.store = Store::open(&f.root, policy()).unwrap();
        let before = Manifest::decode(&f.manifest).unwrap();
        let imported = f.store.import_document(OsStr::new("after-recovery"), &f.reader, &AtomicBool::new(false)).unwrap();
        assert_eq!(imported.value.id, before.next);
        let committed = Manifest::decode(&fs::read(f.root.join(MANIFEST)).unwrap()).unwrap();
        assert_eq!(committed.generation, before.generation + 1);
        assert!(f.store.remove(imported.value.id).unwrap().is_complete());
        let committed = Manifest::decode(&fs::read(f.root.join(MANIFEST)).unwrap()).unwrap();
        assert_eq!(committed.generation, before.generation + 2);
        assert_eq!(committed.items, before.items);
        f.manifest = fs::read(f.root.join(MANIFEST)).unwrap();
        f.manifest_identity = identity(&f.root.join(MANIFEST));
        f.check(Vec::new());
        println!("RECOVERY_FLOW late={late}: exact evidence only discarded; committed generation unchanged by recovery; reopen/import/remove and original/live/room identities preserved");
    }
}
