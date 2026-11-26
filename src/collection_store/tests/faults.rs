//! Real held-file/streaming faults. No synthetic ReaderChunk or returned I/O data.
use super::*;
use crate::local_explorer::READER_CHUNK_BYTES;
use std::io::Seek;

#[path = "integrity.rs"]
mod integrity;
#[path = "recovery.rs"]
mod recovery;
#[path = "mutation.rs"]
mod mutation;
#[path = "damaged.rs"]
mod damaged;
#[path = "removal_outcome.rs"]
mod removal_outcome;

#[derive(Debug, PartialEq, Eq)]
struct Identity(u64, u64, u64, u32, i64, i64, i64, i64);
fn identity(path: &Path) -> Identity {
    let m = fs::metadata(path).unwrap();
    Identity(m.dev(), m.ino(), m.len(), m.mode(), m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec())
}
struct FaultFixture {
    _owner: Fixture,
    root: PathBuf,
    original: PathBuf,
    room: PathBuf,
    bytes: Vec<u8>,
    reader: ReaderFile,
    store: Store,
    kept: Vec<Item>,
    manifest: Vec<u8>,
    manifest_identity: Identity,
    original_identity: Identity,
    room_identity: Identity,
    kept_identities: Vec<Identity>,
}
impl FaultFixture {
    fn new() -> Self {
        let f = Fixture::new();
        let root = f.root("cache");
        let bytes: Vec<_> = (0..(3 * READER_CHUNK_BYTES + 17)).map(|i| (i % 251) as u8).collect();
        let (original, reader) = small_source(&f, &bytes);
        let room = f.root("independent-room").join("retained-copy");
        fs::copy(&original, &room).unwrap();
        let mut store = Store::open(&root, policy()).unwrap();
        let kept: Vec<_> = ["unselected-one", "unselected-two"].into_iter().map(|name| {
            store.import_document(OsStr::new(name), &reader, &AtomicBool::new(false)).unwrap().value
        }).collect();
        let manifest = fs::read(root.join(MANIFEST)).unwrap();
        let manifest_identity = identity(&root.join(MANIFEST));
        let original_identity = identity(&original);
        let room_identity = identity(&room);
        let kept_identities = kept.iter().map(|i| identity(&root.join(payload_name(i.id)))).collect();
        Self { _owner: f, root, original, room, bytes, reader, store, kept, manifest,
            manifest_identity, original_identity, room_identity, kept_identities }
    }
    fn orphan(&self) -> String { payload_name(self.kept.last().unwrap().id + 1) }
    fn check(&mut self, mut evidence: Vec<(String, Vec<u8>)>) {
        assert_eq!(fs::read(self.root.join(MANIFEST)).unwrap(), self.manifest);
        assert_eq!(identity(&self.root.join(MANIFEST)), self.manifest_identity);
        assert_eq!(fs::read(&self.original).unwrap(), self.bytes);
        assert_eq!(identity(&self.original), self.original_identity);
        assert_eq!(fs::read(&self.room).unwrap(), self.bytes);
        assert_eq!(identity(&self.room), self.room_identity);
        for (item, before) in self.kept.iter().zip(&self.kept_identities) {
            let path = self.root.join(payload_name(item.id));
            assert_eq!(fs::read(&path).unwrap(), self.bytes);
            assert_eq!(&identity(&path), before);
        }
        assert_eq!(self.store.items(), self.kept);
        // evidence() is a checked snapshot, not a continuously refreshed listing.
        self.store.refresh().unwrap();
        evidence.sort_by(|a, b| a.0.cmp(&b.0));
        let names: Vec<_> = evidence.iter().map(|(n, _)| OsString::from(n)).collect();
        assert_eq!(self.store.evidence().unwrap(), names);
        let reopened = Store::open(&self.root, policy()).unwrap();
        assert_eq!(reopened.items(), self.kept);
        assert_eq!(reopened.evidence().unwrap(), names);
        if !evidence.is_empty() {
            let refused = self.store.import_document(OsStr::new("must-not-retry"), &self.reader, &AtomicBool::new(false));
            assert!(matches!(refused, Err(Failure::Refused(_))));
            assert!(matches!(self.store.remove(self.kept[0].id), Err(Failure::Refused(_))));
        }
        for (name, bytes) in evidence {
            assert_eq!(fs::read(self.root.join(name)).unwrap(), bytes, "evidence silently changed/deleted");
        }
        assert_eq!(fs::read(self.root.join(MANIFEST)).unwrap(), self.manifest);
    }
}
fn refused(result: std::result::Result<Commit<Item>, Failure>, reason: &str) {
    match result {
        Err(Failure::Refused(s)) => assert!(s.contains(reason), "wrong refusal: {s}"),
        other => panic!("expected {reason} refusal, got {other:?}"),
    }
}

#[test]
fn pre_cancel_never_stages_or_commits() {
    let mut f = FaultFixture::new();
    refused(f.store.import_document(OsStr::new("cancel"), &f.reader, &AtomicBool::new(true)), "cancelled");
    f.check(Vec::new());
}

#[test]
fn mid_stream_cancel_retains_exact_prefix_without_commit() {
    let mut f = FaultFixture::new();
    let cancelled = AtomicBool::new(false);
    let mut chunks = 0;
    let result = f.store.import_inner(OsStr::new("cancel"), &f.reader, &cancelled, |stage, _| {
        if stage == ImportStage::ChunkWritten {
            chunks += 1;
            cancelled.store(true, Ordering::Relaxed);
        }
        Ok(())
    });
    refused(result, "cancelled");
    assert_eq!(chunks, 1);
    f.check(vec![(PAYLOAD_PENDING.into(), f.bytes[..READER_CHUNK_BYTES].to_vec())]);
}

#[test]
fn post_stream_pre_activation_cancel_retains_manifest_and_orphan_without_commit() {
    let mut f = FaultFixture::new();
    let cancelled = AtomicBool::new(false);
    let mut staged = None;
    let result = f.store.import_inner(OsStr::new("cancel"), &f.reader, &cancelled, |stage, file| {
        if stage == ImportStage::ManifestReady {
            let mut bytes = Vec::new();
            file.rewind().unwrap();
            file.read_to_end(&mut bytes).unwrap();
            let candidate = Manifest::decode(&bytes).unwrap();
            assert_eq!(candidate.items.len(), f.kept.len() + 1);
            staged = Some(bytes);
            cancelled.store(true, Ordering::Relaxed);
        }
        Ok(())
    });
    refused(result, "cancelled");
    f.check(vec![(f.orphan(), f.bytes.clone()), (MANIFEST_PENDING.into(), staged.unwrap())]);
}

#[test]
fn held_reader_short_syscalls_exhaust_budget_after_prefix_without_source_mutation() {
    let mut f = FaultFixture::new();
    f.reader.limit_chunk_reads_from(READER_CHUNK_BYTES as u64, 1);
    let mut chunks = 0;
    let result = f.store.import_inner(OsStr::new("partial"), &f.reader, &AtomicBool::new(false), |stage, _| {
        if stage == ImportStage::ChunkWritten { chunks += 1; }
        Ok(())
    });
    refused(result, "incomplete reader chunk");
    assert_eq!(chunks, 1);
    // Reset only the test syscall-size constraint, not the held baseline.
    f.reader.limit_chunk_reads_from(0, READER_CHUNK_BYTES);
    f.check(vec![(PAYLOAD_PENDING.into(), f.bytes[..READER_CHUNK_BYTES].to_vec())]);
}

#[test]
fn held_reader_short_syscalls_complete_when_within_attempt_budget() {
    let mut f = FaultFixture::new();
    f.reader.limit_chunk_reads_from(0, READER_CHUNK_BYTES / 2);
    let item = f.store.import_document(OsStr::new("short-but-complete"), &f.reader, &AtomicBool::new(false)).unwrap().value;
    let mut exported = Vec::new();
    f.store.copy_to(item.id, &mut exported, &AtomicBool::new(false)).unwrap();
    assert_eq!(exported, f.bytes);
    assert_eq!(fs::read(&f.original).unwrap(), f.bytes);
    assert_eq!(identity(&f.original), f.original_identity);
    assert_eq!(fs::read(&f.room).unwrap(), f.bytes);
    assert!(f.store.evidence().unwrap().is_empty());
}

#[test]
fn payload_and_manifest_real_descriptor_write_failures_never_commit() {
    for stage_to_fail in [ImportStage::PayloadStaging, ImportStage::ChunkWritten, ImportStage::ManifestStaging] {
        let mut f = FaultFixture::new();
        let mut injected = false;
        let root = f.root.clone();
        let result = f.store.import_inner(OsStr::new("write-fail"), &f.reader, &AtomicBool::new(false), |stage, file| {
            if stage == stage_to_fail && !injected {
                let name = if stage == ImportStage::ManifestStaging { MANIFEST_PENDING } else { PAYLOAD_PENDING };
                // Same real inode, read-only FD: write_all must reach the kernel and
                // fail EBADF. This is access-mode failure, not claimed as EACCES.
                *file = File::open(root.join(name)).unwrap();
                injected = true;
            }
            Ok(())
        });
        let reason = if stage_to_fail == ImportStage::ManifestStaging { "Manifest write failed" } else { "payload write failed" };
        match result {
            Err(Failure::Refused(s)) => {
                assert!(s.contains(reason), "{s}");
                assert!(s.contains(&std::io::Error::from_raw_os_error(libc::EBADF).to_string()), "{s}");
            }
            other => panic!("kernel write failure reported as {other:?}"),
        }
        assert!(injected);
        let evidence = match stage_to_fail {
            ImportStage::ManifestStaging => vec![(f.orphan(), f.bytes.clone()), (MANIFEST_PENDING.into(), Vec::new())],
            ImportStage::ChunkWritten => vec![(PAYLOAD_PENDING.into(), f.bytes[..READER_CHUNK_BYTES].to_vec())],
            _ => vec![(PAYLOAD_PENDING.into(), Vec::new())],
        };
        f.check(evidence);
    }
}

struct RestoreMode(PathBuf);
impl Drop for RestoreMode {
    fn drop(&mut self) { fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700)).unwrap(); }
}
#[test]
fn activation_kernel_permission_refusal_is_ambiguous_preserves_evidence_and_never_retries() {
    let mut f = FaultFixture::new();
    let root = f.root.clone();
    let restore = RestoreMode(root.clone());
    let mut staged = None;
    let result = f.store.import_inner(OsStr::new("activation-fail"), &f.reader, &AtomicBool::new(false), |stage, file| {
        if stage == ImportStage::ManifestReady {
            let mut bytes = Vec::new();
            file.rewind().unwrap();
            file.read_to_end(&mut bytes).unwrap();
            Manifest::decode(&bytes).unwrap();
            staged = Some(bytes);
            // Only this disposable fixture directory loses write permission.
            // Unlike corrupting a fixture, the actual rename must fail EACCES.
            fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();
        }
        Ok(())
    });
    drop(restore);
    match result {
        Err(Failure::Ambiguous(s)) => {
            assert!(s.contains("Manifest activation uncertain"), "{s}");
            assert!(s.contains(&std::io::Error::from_raw_os_error(libc::EACCES).to_string()), "not actual EACCES: {s}");
        }
        other => panic!("expected actual kernel EACCES as ambiguous activation, got {other:?}"),
    }
    f.check(vec![(f.orphan(), f.bytes.clone()), (MANIFEST_PENDING.into(), staged.unwrap())]);
}

#[test]
fn export_kernel_write_failure_and_cancel_never_report_complete_data() {
    let mut f = FaultFixture::new();
    let id = f.kept[0].id;
    let path = f._owner.0.join("failed-export");
    fs::write(&path, b"caller staging sentinel").unwrap();
    let mut readonly = File::open(&path).unwrap();
    let error = f.store.copy_to(id, &mut readonly, &AtomicBool::new(false)).unwrap_err();
    assert!(error.contains("Collection output failed"));
    assert!(error.contains(&std::io::Error::from_raw_os_error(libc::EBADF).to_string()));
    assert_eq!(fs::read(&path).unwrap(), b"caller staging sentinel");

    struct PrefixThenReadOnly { file: File, path: PathBuf, left: usize }
    impl Write for PrefixThenReadOnly {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.left == 0 {
                self.file = File::open(&self.path)?;
                return self.file.write(bytes); // real kernel EBADF after real prefix
            }
            let n = self.file.write(&bytes[..bytes.len().min(self.left)])?;
            self.left -= n;
            Ok(n)
        }
        fn flush(&mut self) -> std::io::Result<()> { self.file.flush() }
    }
    let mut output = PrefixThenReadOnly { file: File::create(&path).unwrap(), path: path.clone(), left: 17 };
    let error = f.store.copy_to(id, &mut output, &AtomicBool::new(false)).unwrap_err();
    assert!(error.contains("Collection output failed"));
    assert_eq!(fs::read(&path).unwrap(), f.bytes[..17]);
    assert!(fs::metadata(&path).unwrap().len() < f.kept[0].bytes);

    let mut untouched = Vec::new();
    assert!(f.store.copy_to(id, &mut untouched, &AtomicBool::new(true)).unwrap_err().contains("cancelled"));
    assert!(untouched.is_empty());
    struct CancelAfterWrite<'a> { bytes: Vec<u8>, cancelled: &'a AtomicBool }
    impl Write for CancelAfterWrite<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            self.cancelled.store(true, Ordering::Relaxed);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }
    let cancelled = AtomicBool::new(false);
    let mut output = CancelAfterWrite { bytes: Vec::new(), cancelled: &cancelled };
    assert!(f.store.copy_to(id, &mut output, &cancelled).unwrap_err().contains("cancelled"));
    // Even if all bytes reached the caller, late cancellation is not success.
    assert_eq!(output.bytes, f.bytes);
    f.check(Vec::new());
}
