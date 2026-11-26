//! Versioned, explicit-root disk collection. No preferences, decoding, Inventory
//! bridge or decoration acquisition. Storage limits do not authorize preview,
//! renderer or room-retained allocations. macOS first, cooperative same-user
//! writers; not authentication against a hostile same-UID namespace mutator.
//!
//! The caller supplies an EXISTING private owned directory, opened component by
//! component without following links. Listing checks metadata, NOT body integrity.
//! Selected exports verify streamed bytes. Mutations reload bounded metadata under
//! the existing nonblocking scoped flock, preserving unrelated expected records
//! WITHOUT verifying or changing their bodies. Import work is linear in the new
//! source/staged bytes; healthy removal hashes only the selected body. Both check
//! cancellation between bounded chunks; neither grants unrelated content authority.
//! Explicit full verification has a caller byte budget; recovery remains separately
//! conservative and bounded. Fixed pending names and
//! unmatched files are visible evidence, never automatically recovered/deleted;
//! evidence blocks further writes. Separate bounded explicit inspection and
//! fresh-confirmed discard handle only recognized interrupted import/removal shapes.
//! Logical payload bytes are the disk policy,
//! not filesystem blocks (metadata is separately bounded). A rename is the commit;
//! sync uncertainty is reported separately, not a power-loss guarantee.
use crate::{Result, local_explorer::ReaderFile, placement_store::{HeldFlock, storage_entries_bounded}};
use sha2::{Digest, Sha256};
use std::{
    ffi::{CString, OsStr, OsString}, fs::{File, Metadata, OpenOptions},
    io::{Read, Write}, os::{fd::{AsRawFd, FromRawFd}, unix::{ffi::{OsStrExt, OsStringExt}, fs::{MetadataExt, OpenOptionsExt}}},
    path::{Component, Path}, sync::atomic::{AtomicBool, Ordering},
};
mod manifest;
mod filesystem;
mod recovery;
mod inspection;
mod damaged;
pub use damaged::{DamagedInspection, DamagedRemovalOutcome, ObservedDamage};
pub use inspection::{ItemInspection, PayloadStatus};
pub use recovery::{RecoveryCandidate, RecoveryInspection, RecoveryMetadata, RecoveryOutcome};
use filesystem::*;
use manifest::Manifest;
const MAX_ITEMS: usize = 1024;
const MANIFEST_BYTES: u64 = 384 * 1024;
const MANIFEST: &str = "manifest";
const MANIFEST_PENDING: &str = "manifest.pending";
const PAYLOAD_PENDING: &str = "payload.pending";

/// Supported logical-byte/positional-I/O ceiling, with one 64 KiB chunk of
/// signed OS offset headroom. Not an implicit verification or preview budget.
pub const MAX_STORAGE_BYTES: u64 = i64::MAX as u64 - 65536;

#[derive(Clone, Copy, Debug)]
pub struct Policy { item_bytes: u64, disk_bytes: u64 }
impl Policy {
    /// Independent positive logical-byte limits, bounded by supported OS offsets.
    /// A per-item limit larger than the aggregate is valid, not an override.
    pub fn new(item_bytes: u64, disk_bytes: u64) -> Result<Self> {
        if item_bytes == 0 || item_bytes > MAX_STORAGE_BYTES { return Err("Invalid per-item collection byte limit".into()); }
        if disk_bytes == 0 || disk_bytes > MAX_STORAGE_BYTES { return Err("Invalid aggregate collection disk budget".into()); }
        Ok(Self { item_bytes, disk_bytes })
    }
    pub fn item_bytes(self) -> u64 { self.item_bytes }
    pub fn disk_bytes(self) -> u64 { self.disk_bytes }
}
/// Decoration is a reserved typed category, not a supported import route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Category { Document, Decoration }
#[derive(Clone, Debug, PartialEq, Eq)]
/// Stored manifest metadata. Length/hash do not assert current body integrity.
pub struct Item { pub id: u64, pub name: OsString, pub bytes: u64, pub category: Category, pub sha256: [u8;32] }
#[derive(Debug)]
pub struct Commit<T> { pub value: T, pub sync_warning: Option<String> }
/// Ok means the manifest committed, not that payload cleanup completed.
/// None remaining means unknown, never a known empty namespace.
#[must_use = "Removal committed; inspect payload_removed, cleanup_error, remaining and sync_warning"]
#[derive(Debug)]
pub struct RemovalOutcome {
    pub removed_id: u64,
    pub committed_generation: u64,
    pub payload_removed: bool,
    pub remaining: Option<Vec<OsString>>,
    pub cleanup_error: Option<String>,
    pub sync_warning: Option<String>,
}
impl RemovalOutcome {
    pub fn is_complete(&self) -> bool {
        self.cleanup_error.is_none() && self.sync_warning.is_none()
            && self.remaining.as_ref().is_some_and(|n| n.is_empty())
    }
}
#[derive(Debug)]
pub enum Failure {
    /// No manifest activation attempted. Partial/published orphan evidence may remain.
    Refused(String),
    /// Activation rename returned an error; inspect/reopen, never retry blindly.
    Ambiguous(String),
}
impl From<String> for Failure { fn from(s: String) -> Self { Self::Refused(s) } }
pub struct Store {
    root: File, parent: File, root_name: OsString, policy: Policy, view: Manifest,
    evidence: Option<Vec<OsString>>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ImportStage { PayloadStaging, ChunkWritten, PayloadPublished, ManifestStaging, ManifestReady }
#[derive(Clone, Copy, PartialEq, Eq)]
enum RemoveStage { ManifestStaging, ManifestReady, CleanupReady, CleanupSync }
impl Store {
    pub fn open(root: &Path, policy: Policy) -> Result<Self> {
        Self::open_checked(root, policy, |_| Ok(()))
    }
    /// Caller isolation check runs on the SAME safely acquired root BEFORE any
    /// initialization. No pathname re-open between validation and adoption.
    pub fn open_checked(root: &Path, policy: Policy, check: impl FnOnce(&File) -> Result<()>) -> Result<Self> {
        let (parent, root_name, root) = acquire_root(root)?;
        check(&root)?;
        verify_named(&parent, &root_name, &root)?;
        let mut store = Self { root, parent, root_name, policy, view: Manifest::empty(), evidence: None };
        let names = store.names()?;
        // Existing/incompatible storage is never "repaired" by creating a lock.
        finder_identity(&store.root, &names)?;
        if names.iter().all(|n| n == FINDER) { create(&store.root, "lock")?.sync_all().map_err(|e| e.to_string())?; }
        let lock = store.lock()?;
        let names = store.names()?;
        finder_identity(&store.root, &names)?;
        if names.iter().filter(|n| *n != FINDER).eq([OsString::from("lock")].iter()) {
            let pending = create(&store.root, MANIFEST_PENDING)?;
            store.write_manifest(&pending, &Manifest::empty())?;
            rename(&store.root, MANIFEST_PENDING, MANIFEST, true)?;
            store.root.sync_all().map_err(|e| format!("Collection initialization sync uncertain; inspect root: {e}"))?;
        }
        let (view, evidence) = store.load()?;
        store.verify_anchor(&lock)?;
        store.view = view;
        store.evidence = Some(evidence);
        Ok(store)
    }
    /// Last checked MANIFEST snapshot, NOT verified payloads or renderer input.
    /// Ordered by durable ID; inspect for per-item availability/integrity status.
    pub fn items(&self) -> &[Item] { &self.view.items }
    /// Read-only active-root/placement boundary; independent of manifest/body
    /// availability. Aliases and reparenting are compared by acquired ancestry.
    pub fn check_placement_destination(&self, destination: &crate::local_explorer::Explorer) -> Result<()> {
        verify_named(&self.parent, &self.root_name, &self.root)?;
        owned(&self.root, true)?;
        if destination.is_within_directory(&self.root)? {
            return Err("Private Documents cache and its descendants cannot receive room files or .hair-layout".into());
        }
        Ok(())
    }
    /// Stored logical-byte accounting, not measured/verified available bytes.
    pub fn usage(&self) -> u64 { self.view.usage() }
    /// Last checked sorted unmatched/pending names, not cleanup authority.
    /// None means unknown after a failed inspection, NEVER a known empty root.
    /// Fresh mutations always reload safe actual metadata before enabling writes.
    pub fn evidence(&self) -> Option<&[OsString]> { self.evidence.as_deref() }
    /// Policy changes never mutate/delete existing entries, even above new limits.
    pub fn set_policy(&mut self, policy: Policy) { self.policy = policy; }
    pub fn refresh(&mut self) -> Result<()> {
        self.evidence = None;
        let lock = self.lock()?;
        let (view, evidence) = self.load()?;
        self.verify_anchor(&lock)?;
        self.view = view;
        self.evidence = Some(evidence);
        Ok(())
    }
    /// Reload bounded manifest/namespace/identity metadata under the mutation lock.
    /// Preserve unrelated entries/hashes, even damaged/unavailable ones, without
    /// reading their bodies or asserting integrity. Unknown evidence still refuses.
    /// Body work is linear only in this source and its final staged verification:
    /// 4 KiB source/write chunks, 64 KiB verification chunks, with cancellation.
    pub fn import_document(&mut self, name: &OsStr, source: &ReaderFile, cancelled: &AtomicBool)
        -> std::result::Result<Commit<Item>, Failure> {
        self.import_document_checkpoint(name, source, cancelled, || {})
    }
    /// Non-authorizing progress checkpoint at real streamed writes. It cannot
    /// access staging files or alter activation; cancellation is checked next.
    pub fn import_document_checkpoint(&mut self, name: &OsStr, source: &ReaderFile, cancelled: &AtomicBool,
        mut chunk_written: impl FnMut()) -> std::result::Result<Commit<Item>, Failure> {
        self.import_inner(name, source, cancelled, |stage, _| {
            if stage == ImportStage::ChunkWritten { chunk_written(); }
            Ok(())
        })
    }
    /// Remove only this collection-owned reference/payload. A stale view refuses
    /// rather than treating a selection made against old shared state as current.
    /// Cancellation-free convenience wrapper over selected-only remove_cancelled.
    /// Prefer that entrypoint for large items; no aggregate/preview deletion cap.
    pub fn remove(&mut self, id: u64) -> std::result::Result<RemovalOutcome, Failure> {
        self.remove_cancelled(id, &AtomicBool::new(false))
    }
    /// Verify only the selected healthy body (64 KiB, eight attempts per chunk),
    /// preserving every unrelated manifest entry without body reads or new trust.
    /// Cancellation before activation refuses with any pending evidence retained;
    /// after activation cleanup is bounded metadata work and commit is reported.
    /// Damaged selected removal needs a separate fresh-confirm contract, not this API.
    /// Ok means COMMITTED: inspect the structured outcome, not just Result::is_ok.
    pub fn remove_cancelled(&mut self, id: u64, cancelled: &AtomicBool) -> std::result::Result<RemovalOutcome, Failure> {
        self.remove_inner(id, cancelled, |_, _, _| Ok(()))
    }
    fn remove_inner(&mut self, id: u64, cancelled: &AtomicBool,
        mut seam: impl FnMut(RemoveStage, &mut Self, &mut File) -> Result<()>) -> std::result::Result<RemovalOutcome, Failure> {
        check_cancel(cancelled)?;
        self.evidence = None;
        let lock = self.lock()?;
        let guard = self.metadata_guard(&lock)?;
        let (mut live, evidence) = self.load_cancelled(cancelled)?;
        self.evidence = Some(evidence.clone());
        self.writable(&evidence)?;
        let names = self.names()?;
        if live.generation != self.view.generation || live.next != self.view.next || live.items != self.view.items {
            return Err(Failure::Refused("Stale collection view; refresh and select again before removal".into()));
        }
        let index = live.items.iter().position(|i| i.id == id).ok_or_else(|| "Collection item no longer present".to_string())?;
        let name = payload_name(id);
        let payload = regular(&self.root, &name)?;
        let selected_stamp = stamp(&payload)?;
        verify_payload_cancelled(&payload, &live.items[index], cancelled)?;
        check_cancel(cancelled)?;
        live.items.remove(index);
        live.generation = live.generation.checked_add(1).ok_or_else(|| "Collection generation exhausted".to_string())?;
        let mut manifest = create(&self.root, MANIFEST_PENDING)?;
        seam(RemoveStage::ManifestStaging, self, &mut manifest)?;
        self.write_manifest(&manifest, &live)?;
        seam(RemoveStage::ManifestReady, self, &mut manifest)?;
        self.verify_metadata_guard(&guard, &lock)?;
        verify_named(&self.root, OsStr::new(&name), &payload)?;
        if stamp(&payload)? != selected_stamp { return Err(Failure::Refused("Collection selected payload changed before removal".into())); }
        check_cancel(cancelled)?;
        let warning = self.activate(&manifest)?;
        self.view = live;
        // Deletion follows manifest activation. A crash/failure leaves explicit
        // orphan evidence, not a live record referring to a removed payload. No
        // Err or cancellation rollback after commit; report actual progress.
        let mut outcome = RemovalOutcome { removed_id: id, committed_generation: self.view.generation,
            payload_removed: false, remaining: None, cleanup_error: None, sync_warning: warning };
        let cleanup = (|| -> Result<()> {
            let committed_guard = self.metadata_guard(&lock)?;
            verify_named(&self.root, OsStr::new(MANIFEST), &manifest)?;
            if committed_guard.finder != guard.finder { return Err("Committed removal; Finder changed before cleanup".into()); }
            seam(RemoveStage::CleanupReady, self, &mut manifest)?;
            self.verify_metadata_guard(&committed_guard, &lock)?;
            if self.names()? != names { return Err("Committed removal; namespace changed at cleanup".into()); }
            verify_named(&self.root, OsStr::new(&name), &payload)?;
            if stamp(&payload)? != selected_stamp { return Err("Committed removal; selected payload changed at cleanup".into()); }
            unlink(&self.root, &name)?;
            outcome.payload_removed = true;
            seam(RemoveStage::CleanupSync, self, &mut manifest)?;
            if let Err(error) = self.root.sync_all() {
                outcome.sync_warning = Some(format!("Committed removal; cleanup namespace sync uncertain: {error}; {:?}", outcome.sync_warning));
            }
            Ok(())
        })();
        if let Err(error) = cleanup {
            outcome.cleanup_error = Some(format!("Committed removal; payload cleanup incomplete: {error}"));
        }
        outcome.remaining = self.removal_remaining(&mut outcome.cleanup_error);
        Ok(outcome)
    }
    // Shared with damaged removal: never retain stale/fictional evidence when
    // fresh bounded metadata inspection cannot establish the committed namespace.
    fn removal_remaining(&mut self, cleanup_error: &mut Option<String>) -> Option<Vec<OsString>> {
        self.evidence = None;
        match self.load() {
            Ok((now, evidence)) if now.generation == self.view.generation
                && now.next == self.view.next && now.items == self.view.items => {
                if !evidence.is_empty() && cleanup_error.is_none() {
                    *cleanup_error = Some("Committed removal; evidence remains, explicitly inspect".into());
                }
                self.evidence = Some(evidence.clone());
                Some(evidence)
            },
            other => {
                let error = match other { Err(e) => e, Ok(_) => "Active manifest changed after commit".into() };
                *cleanup_error = Some(format!("Committed removal; remaining inspection failed: {error}; {cleanup_error:?}"));
                None
            },
        }
    }
    fn import_inner(&mut self, name: &OsStr, source: &ReaderFile, cancelled: &AtomicBool,
        mut seam: impl FnMut(ImportStage, &mut File) -> Result<()>)
        -> std::result::Result<Commit<Item>, Failure> {
        valid_name(name)?;
        check_cancel(cancelled)?;
        // ReaderFile checks its acquisition stamp on EVERY bounded chunk. The
        // initial chunk supplies the held length, never a decoder/body cap.
        let first = source.read_chunk(0)?;
        let len = first.observed_len;
        if len > self.policy.item_bytes { return Err(Failure::Refused("Per-item collection byte limit exceeded before staging".into())); }
        self.evidence = None;
        let lock = self.lock()?;
        let guard = self.metadata_guard(&lock)?;
        let (mut live, evidence) = self.load_cancelled(cancelled)?;
        self.evidence = Some(evidence.clone());
        self.writable(&evidence)?;
        if live.items.len() >= MAX_ITEMS { return Err(Failure::Refused("Collection item count limit reached".into())); }
        if live.usage().checked_add(len).is_none_or(|n| n > self.policy.disk_bytes) {
            return Err(Failure::Refused("Aggregate collection disk budget exceeded before staging".into()));
        }
        check_cancel(cancelled)?;
        let id = live.next;
        live.next = id.checked_add(1).ok_or_else(|| "Collection identity exhausted".to_string())?;
        live.generation = live.generation.checked_add(1).ok_or_else(|| "Collection generation exhausted".to_string())?;
        let mut pending = create(&self.root, PAYLOAD_PENDING)?;
        seam(ImportStage::PayloadStaging, &mut pending)?;
        let mut hash = Sha256::new();
        let mut chunk = first;
        let mut offset = 0;
        loop {
            check_cancel(cancelled)?;
            if chunk.observed_len != len || chunk.start != offset || chunk.end > len || (chunk.used == 0 && offset != len) {
                return Err(Failure::Refused("Collection source growth or incomplete chunk; pending evidence retained".into()));
            }
            pending.write_all(chunk.body()).map_err(|e| format!("Collection payload write failed; pending evidence retained: {e}"))?;
            seam(ImportStage::ChunkWritten, &mut pending)?;
            check_cancel(cancelled)?;
            hash.update(chunk.body());
            offset = chunk.end;
            if offset == len { break; }
            chunk = source.read_chunk(offset)?;
        }
        // Final held check catches growth/truncation after the last body chunk.
        let end = source.read_chunk(len)?;
        if end.used != 0 || end.observed_len != len { return Err(Failure::Refused("Collection source changed at EOF".into())); }
        check_cancel(cancelled)?;
        pending.sync_all().map_err(|e| format!("Collection payload sync failed; pending evidence retained: {e}"))?;
        self.verify_anchor(&lock)?;
        verify_named(&self.root, OsStr::new(PAYLOAD_PENDING), &pending)?;
        let item = Item { id, name: name.to_os_string(), bytes: len, category: Category::Document, sha256: hash.finalize().into() };
        let payload = payload_name(id);
        rename(&self.root, PAYLOAD_PENDING, &payload, true)
            .map_err(|e| Failure::Ambiguous(format!("Payload rename uncertain; pending/orphan evidence retained; inspect before retry: {e}")))?;
        seam(ImportStage::PayloadPublished, &mut pending)?;
        self.root.sync_all().map_err(|e| format!("Payload namespace sync failed; orphan evidence retained: {e}"))?;
        live.items.push(item.clone());
        let mut manifest_file = create(&self.root, MANIFEST_PENDING)?;
        seam(ImportStage::ManifestStaging, &mut manifest_file)?;
        self.write_manifest(&manifest_file, &live)?;
        seam(ImportStage::ManifestReady, &mut manifest_file)?;
        // Source hashing proves the intended bytes, not the staged output. Check
        // the held payload after rename/manifest staging and before activation;
        // refusal leaves the orphan and pending manifest as explicit evidence.
        verify_payload_cancelled(&pending, &item, cancelled)?;
        // Staging/sync can take time. The acquisition stamp must still match at
        // the final activation boundary, not only when body streaming ended.
        let end = source.read_chunk(len)?;
        if end.used != 0 || end.observed_len != len { return Err(Failure::Refused("Collection source changed before activation".into())); }
        check_cancel(cancelled)?;
        self.verify_metadata_guard(&guard, &lock)?;
        verify_named(&self.root, OsStr::new(&payload), &pending)?;
        check_cancel(cancelled)?;
        let warning = self.activate(&manifest_file)?;
        self.view = live;
        self.evidence = Some(Vec::new());
        Ok(Commit { value: item, sync_warning: warning })
    }
    /// Verified, bounded streaming read; no decoder or whole-body allocation.
    /// ANY failure makes all partial output untrusted; discard caller staging.
    pub fn copy_to(&self, id: u64, output: &mut impl Write, cancelled: &AtomicBool) -> Result<()> {
        self.copy_selected(id, output, cancelled).map_err(|e| format!("Untrusted partial output; discard output: {e}"))
    }
    fn copy_selected(&self, id: u64, output: &mut impl Write, cancelled: &AtomicBool) -> Result<()> {
        check_cancel(cancelled)?;
        let lock = self.lock()?;
        let guard = self.metadata_guard(&lock)?;
        let (live, _) = self.load()?;
        let item = live.items.iter().find(|i| i.id == id).ok_or("Collection item no longer present")?;
        if !self.view.items.contains(item) { return Err("Selected collection identity changed; refresh and select again".into()); }
        let mut file = regular(&self.root, &payload_name(id))?;
        let before = stamp(&file)?;
        if before.len != item.bytes { return Err("Collection payload length corrupt/incomplete".into()); }
        let mut hash = Sha256::new();
        let mut remaining = item.bytes;
        let mut buf = [0; 65536];
        while remaining != 0 {
            check_cancel(cancelled)?;
            let n = remaining.min(buf.len() as u64) as usize;
            file.read_exact(&mut buf[..n]).map_err(|e| format!("Collection payload incomplete: {e}"))?;
            #[cfg(test)]
            record_body_read(&file, n);
            hash.update(&buf[..n]);
            output.write_all(&buf[..n]).map_err(|e| format!("Collection output failed: {e}"))?;
            remaining -= n as u64;
        }
        check_cancel(cancelled)?;
        if <[u8;32]>::from(hash.finalize()) != item.sha256 || stamp(&file)? != before {
            return Err("Collection payload changed/corrupt; discard output".into());
        }
        verify_named(&self.root, OsStr::new(&payload_name(id)), &file)?;
        self.verify_metadata_guard(&guard, &lock)
    }
    fn names(&self) -> Result<Vec<OsString>> {
        let mut names = storage_entries_bounded(&self.root, MAX_ITEMS + 16)?;
        names.sort();
        Ok(names)
    }
    fn lock(&self) -> Result<HeldFlock> {
        verify_named(&self.parent, &self.root_name, &self.root)?;
        owned(&self.root, true)?;
        let file = regular(&self.root, "lock")?;
        let lock = HeldFlock::acquire(file, libc::LOCK_EX).map_err(|e| format!("Collection lock: {e}"))?;
        self.verify_anchor(&lock)?;
        Ok(lock)
    }
    fn verify_anchor(&self, lock: &File) -> Result<()> {
        verify_named(&self.parent, &self.root_name, &self.root)?;
        verify_named(&self.root, OsStr::new("lock"), lock)
    }
    fn writable(&self, evidence: &[OsString]) -> Result<()> {
        if !evidence.is_empty() { return Err(format!("Collection has pending/orphan/unknown evidence {evidence:?}; inspect explicitly, no automatic cleanup")); }
        Ok(())
    }
    fn load(&self) -> Result<(Manifest, Vec<OsString>)> {
        self.load_cancelled(&AtomicBool::new(false))
    }
    fn load_cancelled(&self, cancelled: &AtomicBool) -> Result<(Manifest, Vec<OsString>)> {
        check_cancel(cancelled)?;
        let names = self.names()?;
        let finder = finder_identity(&self.root, &names)?;
        let mut file = regular(&self.root, MANIFEST)?;
        let before = stamp(&file)?;
        if before.len > MANIFEST_BYTES { return Err("Collection manifest exceeds bounded metadata limit".into()); }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file).take(MANIFEST_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() as u64 != before.len || stamp(&file)? != before { return Err("Collection manifest changed while reading".into()); }
        let manifest = Manifest::decode(&bytes)?;
        check_cancel(cancelled)?;
        let mut expected = std::collections::BTreeSet::from([OsString::from("lock"), OsString::from(MANIFEST)]);
        if finder.is_some() { expected.insert(OsString::from(FINDER)); }
        for item in &manifest.items {
            check_cancel(cancelled)?;
            let name = payload_name(item.id);
            // A damaged live item is not orphan evidence or global read denial.
            let _ = self.payload_status(item);
            #[cfg(test)]
            record_metadata_check();
            expected.insert(OsString::from(name));
        }
        check_cancel(cancelled)?;
        if self.names()? != names || finder_identity(&self.root, &names)? != finder || stamp(&file)? != before {
            return Err("Collection metadata/namespace changed during inspection".into());
        }
        let evidence = names.into_iter().filter(|n| !expected.contains(n)).collect();
        verify_named(&self.root, OsStr::new(MANIFEST), &file)?;
        check_cancel(cancelled)?;
        Ok((manifest, evidence))
    }
    fn write_manifest(&self, file: &File, manifest: &Manifest) -> Result<()> {
        let bytes = manifest.encode()?;
        let mut out = file;
        out.write_all(&bytes).map_err(|e| format!("Manifest write failed; evidence retained: {e}"))?;
        file.sync_all().map_err(|e| format!("Manifest sync failed; evidence retained: {e}"))
    }
    fn activate(&self, file: &File) -> std::result::Result<Option<String>, Failure> {
        verify_named(&self.root, OsStr::new(MANIFEST_PENDING), file)?;
        rename(&self.root, MANIFEST_PENDING, MANIFEST, false)
            .map_err(|e| Failure::Ambiguous(format!("Manifest activation uncertain; inspect/reopen before retry: {e}")))?;
        Ok(self.root.sync_all().err().map(|e| format!("Committed, but collection directory sync uncertain: {e}")))
    }
}
fn check_cancel(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Relaxed) { Err("Collection operation cancelled; any pending evidence retained".into()) } else { Ok(()) }
}
fn payload_name(id: u64) -> String { format!("item-{id:016x}") }
#[cfg(test)]
mod tests;
