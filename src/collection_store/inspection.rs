//! Metadata is not body-integrity evidence. Reports are observations, never
//! renderer handles, durable trust flags or deletion/activation capabilities.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PayloadStatus {
    /// Owned/private/single-link, named regular file of the stored length only.
    Unchecked,
    /// Cannot safely acquire the payload (including missing/unsafe file types).
    Unavailable(String),
    /// Acquired metadata or explicitly verified bytes disagree with the manifest.
    Invalid(String),
    /// Full bytes matched during this explicit operation, not a future guarantee.
    Verified,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemInspection { pub item: Item, pub status: PayloadStatus }
pub(super) struct MetadataGuard { file: File, manifest: Stamp, lock: Stamp, pub finder: Option<Stamp> }

impl Store {
    pub(super) fn metadata_guard(&self, lock: &File) -> Result<MetadataGuard> {
        let file = regular(&self.root, MANIFEST)?;
        Ok(MetadataGuard { manifest: stamp(&file)?, file, lock: stamp(lock)?, finder: finder_identity(&self.root, &self.names()?)? })
    }
    pub(super) fn verify_metadata_guard(&self, guard: &MetadataGuard, lock: &File) -> Result<()> {
        self.verify_anchor(lock)?;
        verify_named(&self.root, OsStr::new(MANIFEST), &guard.file)?;
        if stamp(&guard.file)? != guard.manifest || stamp(lock)? != guard.lock {
            return Err("Collection manifest/lock metadata changed during operation".into());
        }
        if finder_identity(&self.root, &self.names()?)? != guard.finder {
            return Err("Collection Finder metadata changed during operation".into());
        }
        Ok(())
    }
    /// Fresh bounded namespace/manifest/identity metadata only. No body reads.
    /// A same-length corrupt body is truthfully Unchecked until explicitly used
    /// or verified. This does not update the last items()/evidence() snapshot.
    pub fn inspect(&self) -> Result<Vec<ItemInspection>> {
        let lock = self.lock()?;
        let (live, _) = self.load()?;
        let items = live.items.iter().map(|item| ItemInspection {
            item: item.clone(), status: self.payload_status(item),
        }).collect();
        self.verify_anchor(&lock)?;
        Ok(items)
    }
    pub(super) fn payload_status(&self, item: &Item) -> PayloadStatus {
        match regular(&self.root, &payload_name(item.id)).and_then(|f| stamp(&f)) {
            Err(e) => PayloadStatus::Unavailable(e),
            Ok(m) if m.len != item.bytes => PayloadStatus::Invalid("Collection payload length corrupt/incomplete".into()),
            Ok(_) => PayloadStatus::Unchecked,
        }
    }
    /// Explicit full verification, with caller-selected aggregate byte ceiling,
    /// 64 KiB chunks, eight read attempts and cancellation checked per attempt.
    /// No writes or cached trust. Per-item damage does not stop healthy checks;
    /// cancellation/budget/changed authority refuses the whole report. Verified
    /// is an observation over bracketed reads, NOT a hostile-same-UID snapshot.
    pub fn verify_all(&self, max_bytes: u64, cancelled: &AtomicBool) -> Result<Vec<ItemInspection>> {
        check_cancel(cancelled)?;
        let lock = self.lock()?;
        let guard = self.metadata_guard(&lock)?;
        let (live, evidence) = self.load()?;
        if live.usage() > max_bytes { return Err("Full collection verification byte budget exceeded".into()); }
        let mut items = Vec::new();
        for item in &live.items {
            check_cancel(cancelled)?;
            let status = match self.payload_status(item) {
                PayloadStatus::Unchecked => match self.verify_item(item, cancelled) {
                    Ok(()) => PayloadStatus::Verified,
                    Err(e) => { check_cancel(cancelled)?; PayloadStatus::Invalid(e) },
                },
                status => status,
            };
            items.push(ItemInspection { item: item.clone(), status });
        }
        check_cancel(cancelled)?;
        let (now, remaining) = self.load()?;
        if now.generation != live.generation || now.next != live.next || now.items != live.items || remaining != evidence {
            return Err("Collection metadata changed during full verification".into());
        }
        self.verify_metadata_guard(&guard, &lock)?;
        Ok(items)
    }
    fn verify_item(&self, item: &Item, cancelled: &AtomicBool) -> Result<()> {
        let name = payload_name(item.id);
        let file = regular(&self.root, &name)?;
        verify_payload_cancelled(&file, item, cancelled)?;
        verify_named(&self.root, OsStr::new(&name), &file)
    }
}
