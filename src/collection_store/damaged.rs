//! Selected damage authority is an actual observation, never an integrity bypass.
//! Cooperative same-user bracketed checks, not hostile-same-UID authentication.
use super::*;
use super::{inspection::MetadataGuard, recovery::{FileIdentity, RootIdentity, identify_cancelled, root_identity}};

/// Actual selected-payload observation, distinct from expected manifest metadata.
/// Regular snapshots are acquired owned/private/0600/single-link/no-follow only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObservedDamage {
    Missing,
    Regular { bytes: u64, sha256: [u8; 32], device: u64, inode: u64,
        modified: (i64, i64), changed: (i64, i64) },
}
/// Opaque single-use capability holding the exclusive lock, root, manifest and
/// (for regular damage) payload descriptor. Drop is write-free. Drop before other
/// ordinary operations. Neither Item nor ObservedDamage can forge this authority.
pub struct DamagedInspection {
    root: File, lock: HeldFlock, guard: MetadataGuard, root_identity: RootIdentity,
    names: Vec<OsString>, live: Manifest, expected: Item, binding: String,
    payload: Option<File>, identity: Option<FileIdentity>, observed: ObservedDamage,
    lock_identity: FileIdentity, max_bytes: u64, used: bool,
}
impl DamagedInspection {
    pub fn expected(&self) -> &Item { &self.expected }
    pub fn observed(&self) -> &ObservedDamage { &self.observed }
    pub fn generated_binding(&self) -> &str { &self.binding }
    pub fn committed_generation(&self) -> u64 { self.live.generation }
}
/// Ok always means the manifest committed; Err never claims a definite rollback
/// after an ambiguous activation. Inspect cleanup_error, remaining and sync_warning.
/// None remaining means namespace inspection FAILED, not an empty namespace.
#[must_use = "Removal committed; inspect payload_removed, cleanup_error, remaining and sync_warning"]
#[derive(Debug)]
pub struct DamagedRemovalOutcome {
    pub removed_id: u64,
    pub committed_generation: u64,
    pub payload_removed: bool,
    pub remaining: Option<Vec<OsString>>,
    pub cleanup_error: Option<String>,
    pub sync_warning: Option<String>,
}
impl DamagedRemovalOutcome {
    pub fn is_complete(&self) -> bool {
        self.cleanup_error.is_none() && self.sync_warning.is_none()
            && self.remaining.as_ref().is_some_and(|n| n.is_empty())
    }
}
// Only kernel ENOENT means missing. Unknown acquisition errors never authorize
// record-only removal; fstatat does not follow a selected symlink or block on FIFO.
fn require_missing(root: &File, name: &str) -> Result<()> {
    let name = CString::new(name).map_err(|e| e.to_string())?;
    let mut raw = std::mem::MaybeUninit::<libc::stat>::uninit();
    let result = unsafe { libc::fstatat(root.as_raw_fd(), name.as_ptr(), raw.as_mut_ptr(), libc::AT_SYMLINK_NOFOLLOW) };
    if result == 0 { return Err("Selected missing payload became present; inspect again".into()); }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() != Some(libc::ENOENT) { return Err(format!("Cannot establish selected payload absence: {error}")); }
    Ok(())
}
fn same_manifest(a: &Manifest, b: &Manifest) -> bool {
    a.generation == b.generation && a.next == b.next && a.items == b.items
}
fn identify_lock(lock: &File, cancelled: &AtomicBool) -> Result<FileIdentity> {
    let mut budget = MANIFEST_BYTES;
    Ok(identify_cancelled(lock, &mut budget, false, false, cancelled)?.0)
}
impl Store {
    /// Read-only inspection of EXACT selected expected record in this fresh view.
    /// Supports actual missing or safely held regular wrong-length/digest bodies
    /// ONLY. Healthy, unsafe, foreign, linked or nonregular selections refuse.
    /// Hash ONLY the FULL selected actual body, within explicit max_bytes per
    /// pass (inspection and fresh confirmation each get that same budget).
    /// Budget must be positive and <= MAX_STORAGE_BYTES; no implicit unlimited
    /// sentinel, policy-derived allowance or 64 MiB clamp. Appended damage needs
    /// enough budget for the ACTUAL appended length, not just expected bytes.
    /// Over-budget/unsupported actual lengths refuse before any body read.
    /// Constant 64 KiB/eight-attempt cancellable positional chunks; storage and
    /// verification budgets neither authorize nor change preview/render limits.
    /// Pending/unknown evidence refuses; unrelated damaged records remain untrusted
    /// and unchanged without body reads. Inspect/cancel/drop never writes.
    pub fn inspect_damaged_item(&self, selected: &Item, max_bytes: u64, cancelled: &AtomicBool) -> Result<DamagedInspection> {
        check_cancel(cancelled)?;
        if max_bytes == 0 || max_bytes > MAX_STORAGE_BYTES {
            return Err("Invalid damaged-item verification budget; choose 1..=MAX_STORAGE_BYTES actual bytes per pass".into());
        }
        let lock = self.lock()?;
        let root = root_identity(&self.root)?;
        let guard = self.metadata_guard(&lock)?;
        let lock_identity = identify_lock(&lock, cancelled)?;
        let names = self.names()?;
        let (live, evidence) = self.load_cancelled(cancelled)?;
        self.writable(&evidence)?;
        if !same_manifest(&live, &self.view) || !live.items.iter().any(|i| i == selected) {
            return Err("Stale or mismatched selected expected record; refresh and inspect again".into());
        }
        let binding = payload_name(selected.id);
        let (payload, identity, observed) = if names.iter().any(|n| n == OsStr::new(&binding)) {
            let file = regular(&self.root, &binding)?;
            let (actual, _) = identify_cancelled(&file, &mut { max_bytes }, false, true, cancelled)?;
            verify_named(&self.root, OsStr::new(&binding), &file)?;
            if actual.stamp.len == selected.bytes && actual.digest == selected.sha256 {
                return Err("Selected payload is healthy; damaged-removal authority not granted".into());
            }
            let s = &actual.stamp;
            let observed = ObservedDamage::Regular { bytes: s.len, sha256: actual.digest, device: s.dev,
                inode: s.ino, modified: s.mtime, changed: s.ctime };
            (Some(file), Some(actual), observed)
        } else {
            require_missing(&self.root, &binding)?;
            (None, None, ObservedDamage::Missing)
        };
        let inspection = DamagedInspection { root: self.root.try_clone().map_err(|e| e.to_string())?, lock, guard,
            root_identity: root, names, live, expected: selected.clone(), binding, payload, identity,
            observed, lock_identity, max_bytes, used: false };
        self.validate_damaged(&inspection, cancelled, None, &inspection.root_identity, false)?;
        Ok(inspection)
    }
    fn check_damage_binding(&self, inspection: &DamagedInspection, cancelled: &AtomicBool, hash: bool) -> Result<()> {
        match (&inspection.payload, &inspection.identity) {
            (None, None) => require_missing(&self.root, &inspection.binding),
            (Some(file), Some(old)) => {
                verify_named(&self.root, OsStr::new(&inspection.binding), file)?;
                if stamp(file)? != old.stamp { return Err("Selected observed damaged payload changed; inspect again".into()); }
                if hash {
                    let (now, _) = identify_cancelled(file, &mut { inspection.max_bytes }, false, true, cancelled)?;
                    if now != *old { return Err("Selected actual damage snapshot changed; inspect again".into()); }
                }
                Ok(())
            },
            _ => Err("Invalid private damaged-item capability".into()),
        }
    }
    fn validate_damaged(&self, inspection: &DamagedInspection, cancelled: &AtomicBool,
        staged: Option<&File>, root: &RootIdentity, hash: bool) -> Result<()> {
        check_cancel(cancelled)?;
        verify_named(&self.parent, &self.root_name, &inspection.root)?;
        self.verify_metadata_guard(&inspection.guard, &inspection.lock)?;
        if root_identity(&self.root)? != *root || identify_lock(&inspection.lock, cancelled)? != inspection.lock_identity {
            return Err("Damaged inspection root/lock changed; inspect again".into());
        }
        let mut names = inspection.names.clone();
        if let Some(file) = staged {
            names.push(MANIFEST_PENDING.into()); names.sort();
            verify_named(&self.root, OsStr::new(MANIFEST_PENDING), file)?;
        }
        let (live, evidence) = self.load_cancelled(cancelled)?;
        let expected_evidence: Vec<OsString> = if staged.is_some() { vec![MANIFEST_PENDING.into()] } else { Vec::new() };
        if self.names()? != names || evidence != expected_evidence || !same_manifest(&live, &inspection.live)
            || !same_manifest(&self.view, &inspection.live) {
            return Err("Damaged inspection manifest/namespace/view changed; inspect again".into());
        }
        self.check_damage_binding(inspection, cancelled, hash)?;
        self.verify_metadata_guard(&inspection.guard, &inspection.lock)?;
        if self.names()? != names || root_identity(&self.root)? != *root {
            return Err("Damaged inspection changed during validation".into());
        }
        check_cancel(cancelled)
    }
    /// Separate explicit fresh confirmation. Consumes capability on ANY attempt.
    /// Rehashes the held actual damage, never calls healthy remove with a bypass.
    /// Manifest removes exactly this ID at generation+1, allocator unchanged.
    /// Pre-activation cancellation refuses (staging evidence may remain). Once
    /// committed cancellation cannot undo it: report committed cleanup/sync state.
    pub fn remove_damaged_item(&mut self, inspection: &mut DamagedInspection, cancelled: &AtomicBool)
        -> std::result::Result<DamagedRemovalOutcome, Failure> {
        self.remove_damaged_inner(inspection, cancelled, |_, _, _| Ok(()))
    }
    pub(super) fn remove_damaged_inner(&mut self, inspection: &mut DamagedInspection, cancelled: &AtomicBool,
        mut seam: impl FnMut(RemoveStage, &mut Self, &mut File) -> Result<()>)
        -> std::result::Result<DamagedRemovalOutcome, Failure> {
        if inspection.used { return Err(Failure::Refused("Consumed damaged-item capability; drop and inspect again".into())); }
        inspection.used = true;
        self.validate_damaged(inspection, cancelled, None, &inspection.root_identity, true)?;
        let mut live = inspection.live.clone();
        live.items.retain(|i| i.id != inspection.expected.id);
        live.generation = live.generation.checked_add(1).ok_or("Collection generation exhausted".to_string())?;
        check_cancel(cancelled)?;
        let mut manifest = create(&self.root, MANIFEST_PENDING)?;
        let staged_root = root_identity(&self.root)?;
        seam(RemoveStage::ManifestStaging, self, &mut manifest)?;
        self.write_manifest(&manifest, &live)?;
        let staged_stamp = stamp(&manifest)?;
        seam(RemoveStage::ManifestReady, self, &mut manifest)?;
        self.validate_damaged(inspection, cancelled, Some(&manifest), &staged_root, false)?;
        if stamp(&manifest)? != staged_stamp { return Err(Failure::Refused("Staged removal manifest changed".into())); }
        check_cancel(cancelled)?;
        let warning = self.activate(&manifest)?;
        self.view = live;
        // From here no Err: the manifest has irreversibly committed. Bound cleanup
        // to the SAME held selected descriptor, never a newly acquired replacement.
        let mut outcome = DamagedRemovalOutcome { removed_id: inspection.expected.id,
            committed_generation: self.view.generation, payload_removed: false,
            remaining: None, cleanup_error: None, sync_warning: warning };
        let cleanup = (|| -> Result<()> {
            let committed_guard = self.metadata_guard(&inspection.lock)?;
            verify_named(&self.root, OsStr::new(MANIFEST), &manifest)?;
            self.verify_metadata_guard(&committed_guard, &inspection.lock)?;
            if identify_lock(&inspection.lock, &AtomicBool::new(false))? != inspection.lock_identity {
                return Err("Committed removal; lock changed before cleanup".into());
            }
            if finder_identity(&self.root, &self.names()?)? != inspection.guard.finder {
                return Err("Committed removal; Finder changed before cleanup".into());
            }
            if self.names()? != inspection.names { return Err("Committed removal; namespace changed before cleanup".into()); }
            self.check_damage_binding(inspection, &AtomicBool::new(false), false)?;
            seam(RemoveStage::CleanupReady, self, &mut manifest)?;
            self.verify_metadata_guard(&committed_guard, &inspection.lock)?;
            if self.names()? != inspection.names { return Err("Committed removal; namespace changed at cleanup".into()); }
            self.check_damage_binding(inspection, &AtomicBool::new(false), false)?;
            if inspection.payload.is_some() {
                unlink(&self.root, &inspection.binding)?;
                outcome.payload_removed = true;
                seam(RemoveStage::CleanupSync, self, &mut manifest)?;
                if let Err(error) = self.root.sync_all() {
                    outcome.sync_warning = Some(format!("Committed damaged removal; cleanup namespace sync uncertain: {error}; {:?}", outcome.sync_warning));
                }
            }
            Ok(())
        })();
        if let Err(error) = cleanup { outcome.cleanup_error = Some(format!("Committed removal; payload cleanup incomplete: {error}")); }
        outcome.remaining = self.removal_remaining(&mut outcome.cleanup_error);
        Ok(outcome)
    }
}
