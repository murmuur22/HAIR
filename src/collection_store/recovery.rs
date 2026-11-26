//! Conservative explicit discard, not resume or automatic cleanup. Cooperative
//! same-user storage contract, not authentication against a hostile same UID.
use super::*;
use std::os::unix::fs::FileExt;

const REFUSAL: &str = "Interrupted-mutation recovery refused; preserve evidence and inspect the explicit root with a compatible build";

/// Actual stat lengths, not expected manifest sizes. NOT discard authority.
#[derive(Debug, PartialEq, Eq)]
pub struct RecoveryCandidate {
    pub name: OsString,
    pub actual_bytes: u64,
}
/// Ephemeral non-authorizing report. Holds no lock and is NEVER accepted by
/// discard. Inspection independently reacquires and validates actual evidence.
#[derive(Debug, PartialEq, Eq)]
pub struct RecoveryMetadata {
    pub candidates: Vec<RecoveryCandidate>,
    /// Aggregate candidate PAYLOAD bytes required per verification pass. Pending
    /// manifest length is reported above but has its own fixed metadata budget.
    pub required_candidate_bytes: u64,
}
enum CandidateWork { Metadata, Verify(u64) }

/// Opaque, single-use confirmation capability. Holds the exclusive root lock
/// until dropped, including after cancellation/failure. Drop is write-free.
/// Callers must drop it before reopen/refresh/other ordinary store operations.
pub struct RecoveryInspection {
    root: File,
    lock: HeldFlock,
    state: Snapshot,
    max_bytes: u64,
    used: bool,
}
impl RecoveryInspection {
    pub fn candidates(&self) -> &[OsString] { &self.state.candidates }
    pub fn committed_generation(&self) -> u64 { self.state.generation }
    pub fn max_bytes(&self) -> u64 { self.max_bytes }
}
/// Ok means a progress report, NOT completion. Only is_complete() means every
/// candidate was discarded, remaining evidence is known empty and sync succeeded.
/// Partial/unknown remainder or sync uncertainty requires caller attention and a
/// NEW inspection, never retry of this consumed capability. No rollback is promised.
#[must_use = "Recovery may be partial; check is_complete(), remaining, error and sync_warning"]
#[derive(Debug)]
pub struct RecoveryOutcome {
    pub removed: Vec<OsString>,
    pub remaining: Option<Vec<OsString>>,
    pub error: Option<String>,
    pub sync_warning: Option<String>,
}
impl RecoveryOutcome {
    pub fn is_complete(&self) -> bool {
        self.error.is_none() && self.sync_warning.is_none() && self.remaining.as_ref().is_some_and(|names| names.is_empty())
    }
}
#[derive(PartialEq, Eq)]
pub(super) struct FileIdentity { pub stamp: Stamp, pub digest: [u8; 32] }
#[derive(PartialEq, Eq)]
pub(super) struct RootIdentity(u64, u64, u32, u32, u64, i64, i64, i64, i64);
pub(super) fn root_identity(root: &File) -> Result<RootIdentity> {
    let m = owned(root, true)?;
    Ok(RootIdentity(m.dev(), m.ino(), m.mode(), m.uid(), m.nlink(), m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec()))
}
#[derive(PartialEq, Eq)]
struct Snapshot {
    root: RootIdentity,
    finder: Option<Stamp>,
    lock: FileIdentity,
    generation: u64,
    live: Vec<(String, Option<Stamp>)>,
    entries: Vec<(String, FileIdentity)>,
    candidates: Vec<OsString>,
}

fn recovery_metadata(entries: &[(String, FileIdentity)], names: &[OsString]) -> Result<RecoveryMetadata> {
    let mut candidates = Vec::new();
    let mut required_candidate_bytes = 0_u64;
    for name in names {
        let (_, identity) = entries.iter().find(|(n, _)| OsStr::new(n) == name)
            .ok_or("Missing recovery candidate metadata")?;
        let actual_bytes = identity.stamp.len;
        if actual_bytes > MAX_STORAGE_BYTES { return Err(format!("{REFUSAL}: actual length exceeds supported OS offset ceiling")); }
        if name != MANIFEST_PENDING {
            required_candidate_bytes = required_candidate_bytes.checked_add(actual_bytes)
                .filter(|n| *n <= MAX_STORAGE_BYTES).ok_or("Recovery candidate aggregate exceeds supported byte ceiling")?;
        }
        candidates.push(RecoveryCandidate { name: name.clone(), actual_bytes });
    }
    Ok(RecoveryMetadata { candidates, required_candidate_bytes })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoveryStage { BeforeUnlink(usize), UnlinkReady(usize), BeforeSync }

// Finite 8-attempt positional chunks; no allocation for payload content.
pub(super) fn identify_cancelled(file: &File, budget: &mut u64, capture: bool, body: bool,
    cancelled: &AtomicBool) -> Result<(FileIdentity, Vec<u8>)> {
    check_cancel(cancelled)?;
    let before = stamp(file)?;
    // Keep positional additions inside the SAME supported storage/OS ceiling,
    // even when a caller deliberately chooses a very large finite read budget.
    // Only bounded manifest metadata may be captured; payloads stay streaming.
    if before.len > MAX_STORAGE_BYTES { return Err(format!("{REFUSAL}: actual length exceeds supported OS offset ceiling")); }
    if capture && before.len > MANIFEST_BYTES { return Err(format!("{REFUSAL}: capture exceeds bounded manifest metadata limit")); }
    *budget = budget.checked_sub(before.len).ok_or_else(|| format!("{REFUSAL}: bounded recovery read budget exceeded; actual bytes {}; available bytes {}", before.len, *budget))?;
    let mut bytes = Vec::new();
    let mut hash = Sha256::new();
    let mut offset = 0;
    let mut buf = [0; 65536];
    while offset < before.len {
        check_cancel(cancelled)?;
        let wanted = (before.len - offset).min(buf.len() as u64) as usize;
        let mut used = 0;
        for _ in 0..8 {
            check_cancel(cancelled)?;
            if used == wanted { break; }
            match file.read_at(&mut buf[used..wanted], offset + used as u64) {
                Ok(0) => break,
                Ok(n) => {
                    #[cfg(test)]
                    if body { record_body_read(file, n); }
                    #[cfg(not(test))]
                    let _ = body;
                    used += n;
                },
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {},
                Err(e) => return Err(format!("{REFUSAL}: read failed: {e}")),
            }
        }
        if used != wanted { return Err(format!("{REFUSAL}: incomplete read")); }
        hash.update(&buf[..used]);
        if capture { bytes.extend_from_slice(&buf[..used]); }
        offset += used as u64;
    }
    check_cancel(cancelled)?;
    if stamp(file)? != before { return Err(format!("{REFUSAL}: file changed during inspection")); }
    Ok((FileIdentity { stamp: before, digest: hash.finalize().into() }, bytes))
}
fn read_manifest(root: &File, name: &str, cancelled: &AtomicBool) -> Result<(Manifest, FileIdentity)> {
    let file = regular(root, name)?;
    let mut budget = MANIFEST_BYTES;
    let (identity, bytes) = identify_cancelled(&file, &mut budget, true, false, cancelled)?;
    verify_named(root, OsStr::new(name), &file)?;
    Ok((Manifest::decode(&bytes)?, identity))
}
// Absence is an actual no-follow kernel ENOENT observation, never an open error
// interpreted as absence. Present entries must still be safe private regular
// files bound to this exact active generated name; expected length/hash confer
// no trust. These sampled stamps detect cooperative changes, not hostile same-UID
// writes that can evade sampling. No LIVE body reads, even for wrong lengths.
fn observe_live(root: &File, name: &str) -> Result<Option<Stamp>> {
    let component = CString::new(name).map_err(|e| e.to_string())?;
    let mut raw = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: validated generated component and live directory descriptor.
    if unsafe { libc::fstatat(root.as_raw_fd(), component.as_ptr(), raw.as_mut_ptr(), libc::AT_SYMLINK_NOFOLLOW) } != 0 {
        let error = std::io::Error::last_os_error();
        return if error.raw_os_error() == Some(libc::ENOENT) { Ok(None) }
            else { Err(format!("{REFUSAL}: cannot establish LIVE metadata: {error}")) };
    }
    let file = regular(root, name)?;
    let observed = stamp(&file)?;
    verify_named(root, OsStr::new(name), &file)?;
    Ok(Some(observed))
}
fn import_successor(live: &Manifest, pending: &Manifest) -> bool {
    live.generation.checked_add(1) == Some(pending.generation)
        && live.next.checked_add(1) == Some(pending.next)
        && pending.items.len() == live.items.len() + 1
        && pending.items[..live.items.len()] == live.items
        && pending.items.last().is_some_and(|i| i.id == live.next && i.category == Category::Document)
}
fn removal_successor(live: &Manifest, pending: &Manifest) -> bool {
    if live.generation.checked_add(1) != Some(pending.generation) || live.next != pending.next
        || pending.items.len().checked_add(1) != Some(live.items.len()) { return false; }
    let index = live.items.iter().zip(&pending.items).position(|(a, b)| a != b).unwrap_or(pending.items.len());
    live.items[..index] == pending.items[..index] && live.items[index + 1..] == pending.items[index..]
}
// Failed sequential writes can leave a prefix, not arbitrary undecodable data.
// For import, accept ONLY the immutable successor header/live records/appended
// allocator ID prefix, stopping BEFORE new item metadata. For pending-only also
// accept proper prefixes of an exact removal successor. Empty is a write-EBADF
// shape in this fixed reserved namespace. Full invalid/foreign manifests, partial
// new import metadata and checksum-corrupt complete files remain unsupported.
fn interrupted_prefix(live: &Manifest, bytes: &[u8], with_generated: bool) -> Result<bool> {
    let Some(generation) = live.generation.checked_add(1) else { return Ok(false); };
    if let Some(next) = live.next.checked_add(1).filter(|_| live.items.len() < MAX_ITEMS) {
        let mut candidate = live.clone();
        candidate.generation = generation;
        candidate.next = next;
        candidate.items.push(Item { id: live.next, name: "_".into(), bytes: 0, category: Category::Document, sha256: [0; 32] });
        if let Ok(mut fixed) = candidate.encode() {
            fixed.truncate(live.encode()?.len() - 32 + 8);
            if fixed.starts_with(bytes) { return Ok(true); }
        }
    }
    if !with_generated {
        for index in 0..live.items.len() {
            let mut candidate = live.clone();
            candidate.generation = generation;
            candidate.items.remove(index);
            let encoded = candidate.encode()?;
            if bytes.len() < encoded.len() && encoded.starts_with(bytes) { return Ok(true); }
        }
    }
    Ok(false)
}
fn unreferenced_generated(live: &Manifest, name: &OsStr) -> bool {
    let Some(text) = name.to_str() else { return false; };
    let Some(hex) = text.strip_prefix("item-").filter(|s| s.len() == 16) else { return false; };
    let Ok(id) = u64::from_str_radix(hex, 16) else { return false; };
    let import_possible = live.next.checked_add(1).is_some() && live.generation.checked_add(1).is_some() && live.items.len() < MAX_ITEMS;
    id != 0 && (id < live.next || (id == live.next && import_possible))
        && payload_name(id) == text && !live.items.iter().any(|i| i.id == id)
}
impl Store {
    /// Bounded metadata-only precheck: recognizes the SAME conservative evidence
    /// shapes as inspection and reports exact actual candidate stat lengths BEFORE
    /// hashing any payload. No LIVE/candidate body reads, writes or capability.
    /// Lock/active/pending manifest bytes remain separately bounded. The temporary
    /// lock is released on return; report freshness and authority are NOT retained.
    pub fn interrupted_import_metadata(&self, cancelled: &AtomicBool) -> Result<RecoveryMetadata> {
        check_cancel(cancelled)?;
        let lock = self.lock().map_err(|e| format!("{REFUSAL}: {e}"))?;
        let state = self.recovery_snapshot(&lock, CandidateWork::Metadata, cancelled)?;
        recovery_metadata(&state.entries, &state.candidates)
    }
    /// Read-only, bounded inspection of interrupted import/removal evidence.
    /// Reserved payload.pending alone; lone exact generated 0 < id <= live.next
    /// ABSENT from live records; or manifest.pending alone/with item-{live.next}.
    /// A valid pending manifest must be the exact import successor, or (alone)
    /// exact generation+1/same-next/live-minus-one removal successor. Undecodable
    /// bytes must fit the conservative interrupted_prefix boundary above.
    /// No arbitrary unknown-file cleanup, live deletion, source/display-name
    /// authority or no-active-manifest initialization recovery is provided.
    /// All exact active names are protected, including actually missing ones.
    /// Safe LIVE regular metadata is observed without checking expected bytes;
    /// this grants neither body trust nor LIVE deletion authority. Only candidate
    /// payloads are hashed within explicit max_bytes, 1..=MAX_STORAGE_BYTES, per
    /// pass. No policy-derived allowance, sentinel or fixed 64 MiB clamp. Actual
    /// aggregate lengths are checked BEFORE body reads. To choose a finite budget
    /// use interrupted_import_metadata; that report never authorizes deletion.
    /// Inspection, fresh confirmation, and EACH unlink-boundary rehash receive
    /// the SAME budget. Successful recovery reads candidate payload bytes THREE
    /// times: inspect + confirm + first-unlink boundary. An optional second unlink
    /// rechecks only manifests (payload was removed first); up to four manifest
    /// passes, with lock bytes separately bounded as well.
    /// Reinspection/drop/cancel attempts add their own work; not a total operation
    /// budget. Constant 64 KiB/eight-attempt cancellable positional reads;
    /// cancellation is cooperative, not an interruptible syscall/fsync deadline.
    pub fn inspect_interrupted_import(&self, max_bytes: u64, cancelled: &AtomicBool) -> Result<RecoveryInspection> {
        check_cancel(cancelled)?;
        if max_bytes == 0 || max_bytes > MAX_STORAGE_BYTES {
            return Err("Invalid recovery verification budget; choose 1..=MAX_STORAGE_BYTES actual candidate bytes per pass".into());
        }
        let lock = self.lock().map_err(|e| format!("{REFUSAL}: {e}"))?;
        let state = self.recovery_snapshot(&lock, CandidateWork::Verify(max_bytes), cancelled).map_err(|e| format!("{REFUSAL}: {e}; inspection changed no files"))?;
        Ok(RecoveryInspection { root: self.root.try_clone().map_err(|e| e.to_string())?, lock, state, max_bytes, used: false })
    }
    fn recovery_snapshot(&self, lock: &File, work: CandidateWork, cancelled: &AtomicBool) -> Result<Snapshot> {
        check_cancel(cancelled)?;
        self.verify_anchor(lock)?;
        let root = root_identity(&self.root)?;
        let names = self.names()?;
        let finder = finder_identity(&self.root, &names)?;
        // Lock bytes are not application data; even an oversized lock refuses.
        let mut lock_budget = MANIFEST_BYTES;
        let (lock_identity, _) = identify_cancelled(lock, &mut lock_budget, false, false, cancelled)?;
        let (live, manifest_identity) = read_manifest(&self.root, MANIFEST, cancelled)?;
        let mut entries = vec![(MANIFEST.into(), manifest_identity)];
        let mut observed_live = Vec::new();
        let mut expected = vec![OsString::from("lock"), OsString::from(MANIFEST)];
        if finder.is_some() { expected.push(OsString::from(FINDER)); }
        for item in &live.items {
            check_cancel(cancelled)?;
            let name = payload_name(item.id);
            let observed = observe_live(&self.root, &name)?;
            if observed.is_some() != names.contains(&OsString::from(&name)) {
                return Err(format!("{REFUSAL}: LIVE presence changed during inspection"));
            }
            expected.push(OsString::from(&name));
            observed_live.push((name, observed));
        }
        let evidence: Vec<_> = names.iter().filter(|n| !expected.contains(n)).cloned().collect();
        if evidence == [OsString::from(PAYLOAD_PENDING)] {
            // Reserved staging name, private owned regular single-link file.
            // No generated ID or display/user filename supplies path authority.
            let file = regular(&self.root, PAYLOAD_PENDING)?;
            let identity = FileIdentity { stamp: stamp(&file)?, digest: [0; 32] };
            verify_named(&self.root, OsStr::new(PAYLOAD_PENDING), &file)?;
            entries.push((PAYLOAD_PENDING.into(), identity));
        } else if evidence.len() == 1 && unreferenced_generated(&live, &evidence[0]) {
            let name = evidence[0].to_str().ok_or("Invalid generated name")?;
            let file = regular(&self.root, name)?;
            let identity = FileIdentity { stamp: stamp(&file)?, digest: [0; 32] };
            verify_named(&self.root, &evidence[0], &file)?;
            entries.push((name.into(), identity));
        } else {
            let generated = payload_name(live.next);
            if evidence != [OsString::from(&generated), OsString::from(MANIFEST_PENDING)]
                && evidence != [OsString::from(MANIFEST_PENDING)] {
                return Err(format!("{REFUSAL}: unsupported/unknown evidence {evidence:?}; no automatic deletion"));
            }
            let file = regular(&self.root, MANIFEST_PENDING)?;
            let mut pending_budget = MANIFEST_BYTES;
            let (pending_identity, bytes) = identify_cancelled(&file, &mut pending_budget, true, false, cancelled)?;
            verify_named(&self.root, OsStr::new(MANIFEST_PENDING), &file)?;
            let with_generated = evidence.len() == 2;
            let recognized = match Manifest::decode(&bytes) {
                Ok(pending) => import_successor(&live, &pending) || (!with_generated && removal_successor(&live, &pending)),
                Err(_) => interrupted_prefix(&live, &bytes, with_generated)?,
            };
            if !recognized {
                return Err(format!("{REFUSAL}: pending manifest is not an exact successor or supported interrupted-write prefix"));
            }
            if evidence.len() == 2 {
                // Discard authority comes from the exact uncommitted allocator
                // ID/predecessor, not trusting candidate payload integrity. A
                // failed M4 staged-output check may leave corrupt bytes here.
                let file = regular(&self.root, &generated)?;
                let identity = FileIdentity { stamp: stamp(&file)?, digest: [0; 32] };
                verify_named(&self.root, OsStr::new(&generated), &file)?;
                entries.push((generated, identity));
            }
            entries.push((MANIFEST_PENDING.into(), pending_identity));
        }
        let report = recovery_metadata(&entries, &evidence)?;
        if let CandidateWork::Verify(max_bytes) = work {
            if report.required_candidate_bytes > max_bytes {
                return Err(format!("{REFUSAL}: candidate verification budget {max_bytes} exceeded; actual lengths {report:?}"));
            }
            let mut budget = max_bytes;
            for (name, identity) in &mut entries {
                if name == MANIFEST || name == MANIFEST_PENDING { continue; }
                let file = regular(&self.root, name)?;
                if stamp(&file)? != identity.stamp { return Err(format!("{REFUSAL}: candidate changed after metadata precheck")); }
                let (now, _) = identify_cancelled(&file, &mut budget, false, true, cancelled)?;
                verify_named(&self.root, OsStr::new(name), &file)?;
                *identity = now;
            }
        }
        // Bracket the whole inspection, not merely each individual read.
        for (name, identity) in &entries {
            let file = regular(&self.root, name)?;
            if stamp(&file)? != identity.stamp { return Err(format!("{REFUSAL}: namespace/content changed")); }
        }
        for (name, observed) in &observed_live {
            if observe_live(&self.root, name)? != *observed { return Err(format!("{REFUSAL}: LIVE metadata changed")); }
        }
        self.verify_anchor(lock)?;
        if self.names()? != names || finder_identity(&self.root, &names)? != finder || root_identity(&self.root)? != root || stamp(lock)? != lock_identity.stamp {
            return Err(format!("{REFUSAL}: root/lock/evidence changed during inspection"));
        }
        check_cancel(cancelled)?;
        Ok(Snapshot { root, finder, lock: lock_identity, generation: live.generation, live: observed_live, entries, candidates: evidence })
    }
    /// Fresh confirmation under the inspection's STILL HELD exclusive lock.
    /// Revalidates bounded manifest/lock bytes, observed LIVE metadata/absence,
    /// candidate bytes/identities and exact evidence first. No LIVE body hash.
    /// A cancelled/refused/partially successful capability cannot be retried.
    pub fn discard_interrupted_import(&mut self, inspection: &mut RecoveryInspection, cancelled: &AtomicBool) -> Result<RecoveryOutcome> {
        self.discard_recovery_inner(inspection, cancelled, |_, _| Ok(()))
    }
    pub(super) fn discard_recovery_inner(&mut self, inspection: &mut RecoveryInspection, cancelled: &AtomicBool,
        mut seam: impl FnMut(RecoveryStage, &mut Self) -> Result<()>) -> Result<RecoveryOutcome> {
        if inspection.used { return Err(format!("{REFUSAL}: consumed snapshot; drop and reinspect")); }
        inspection.used = true;
        check_cancel(cancelled)?;
        verify_named(&self.parent, &self.root_name, &inspection.root)?;
        self.verify_anchor(&inspection.lock)?;
        if self.recovery_snapshot(&inspection.lock, CandidateWork::Verify(inspection.max_bytes), cancelled)? != inspection.state {
            return Err(format!("{REFUSAL}: stale snapshot; drop and reinspect"));
        }
        check_cancel(cancelled)?;
        let mut outcome = RecoveryOutcome { removed: Vec::new(), remaining: None, error: None, sync_warning: None };
        // Pending manifest LAST: partial cleanup retains recognized successor or
        // bounded prefix evidence for a NEW inspection, never snapshot reuse.
        let mut names = inspection.state.candidates.clone();
        names.sort_by_key(|n| if n == MANIFEST_PENDING { 1 } else { 0 });
        let mut expected_root = root_identity(&self.root)?;
        for (index, name) in names.iter().enumerate() {
            let result = (|| -> Result<()> {
                seam(RecoveryStage::BeforeUnlink(index), self)?;
                check_cancel(cancelled)?;
                self.verify_anchor(&inspection.lock)?;
                if root_identity(&self.root)? != expected_root { return Err(format!("{REFUSAL}: root changed before unlink")); }
                // Recheck all remaining evidence plus committed state before
                // each unlink; our own earlier unlink is the only allowed delta.
                let expected: Vec<_> = inspection.state.entries.iter().filter(|(n, _)| !outcome.removed.contains(&OsString::from(n)))
                    .map(|(n, _)| OsString::from(n))
                    .chain(inspection.state.live.iter().filter(|(_, s)| s.is_some()).map(|(n, _)| OsString::from(n)))
                    .chain(std::iter::once(OsString::from("lock"))).collect();
                let mut actual_expected = expected;
                if inspection.state.finder.is_some() { actual_expected.push(OsString::from(FINDER)); }
                actual_expected.sort();
                if self.names()? != actual_expected { return Err(format!("{REFUSAL}: new/changed namespace during discard")); }
                if finder_identity(&self.root, &actual_expected)? != inspection.state.finder {
                    return Err(format!("{REFUSAL}: Finder metadata changed before unlink"));
                }
                for (entry, observed) in &inspection.state.live {
                    if observe_live(&self.root, entry)? != *observed { return Err(format!("{REFUSAL}: LIVE metadata changed before unlink: {entry}")); }
                }
                // Metadata preflight of ALL remaining bindings before body work.
                // No grown/rebound candidate spends any of the old allowance.
                for (entry, old) in &inspection.state.entries {
                    if outcome.removed.contains(&OsString::from(entry)) { continue; }
                    let file = regular(&self.root, entry)?;
                    verify_named(&self.root, OsStr::new(entry), &file)?;
                    if stamp(&file)? != old.stamp { return Err(format!("{REFUSAL}: changed file before rehash: {entry}")); }
                }
                let mut budget = inspection.max_bytes;
                for (entry, old) in &inspection.state.entries {
                    if outcome.removed.contains(&OsString::from(entry)) { continue; }
                    let file = regular(&self.root, entry)?;
                    let metadata = entry == MANIFEST || entry == MANIFEST_PENDING;
                    let mut metadata_budget = MANIFEST_BYTES;
                    let (now, _) = identify_cancelled(&file, if metadata { &mut metadata_budget } else { &mut budget }, false, !metadata, cancelled)?;
                    verify_named(&self.root, OsStr::new(entry), &file)?;
                    if now != *old { return Err(format!("{REFUSAL}: changed file before unlink: {entry}")); }
                }
                if stamp(&inspection.lock)? != inspection.state.lock.stamp { return Err(format!("{REFUSAL}: lock changed")); }
                // Bracket candidate hashing with sampled LIVE bindings too.
                for (entry, observed) in &inspection.state.live {
                    if observe_live(&self.root, entry)? != *observed { return Err(format!("{REFUSAL}: LIVE metadata changed at unlink boundary: {entry}")); }
                }
                let name = name.to_str().ok_or("Recovery candidate is not a generated storage name")?;
                let file = regular(&self.root, name)?;
                let old = inspection.state.entries.iter().find(|(n, _)| n == name).ok_or("Missing recovery identity")?;
                if stamp(&file)? != old.1.stamp || root_identity(&self.root)? != expected_root {
                    return Err(format!("{REFUSAL}: candidate/root changed at unlink boundary"));
                }
                self.verify_anchor(&inspection.lock)?;
                verify_named(&self.root, OsStr::new(name), &file)?;
                seam(RecoveryStage::UnlinkReady(index), self)?;
                check_cancel(cancelled)?;
                unlink(&self.root, name)
            })();
            if let Err(error) = result { outcome.error = Some(error); break; }
            outcome.removed.push(name.clone());
            match root_identity(&self.root) {
                Ok(now) => expected_root = now,
                Err(e) => { outcome.error = Some(e); break; }
            }
        }
        // Even a partial unlink is irreversible; sync/report it, never pretend
        // rollback or hide remaining evidence. No sync needed for zero writes.
        if !outcome.removed.is_empty() {
            if let Err(error) = seam(RecoveryStage::BeforeSync, self) { outcome.error = Some(error); }
            outcome.sync_warning = self.root.sync_all().err().map(|e| format!("Recovery unlink directory sync uncertain: {e}"));
        }
        match self.names() {
            Ok(names) => {
                let live_names: Vec<_> = inspection.state.entries.iter().filter(|(n, _)| n != MANIFEST_PENDING && !inspection.state.candidates.contains(&OsString::from(n)))
                    .map(|(n, _)| OsString::from(n))
                    .chain(inspection.state.live.iter().map(|(n, _)| OsString::from(n)))
                    .chain(std::iter::once(OsString::from("lock"))).collect();
                let safe_finder = match finder_identity(&self.root, &names) {
                    Ok(_) => true,
                    Err(e) => { outcome.error = Some(format!("Unsafe remaining Finder metadata: {e}; prior error: {:?}", outcome.error)); false },
                };
                let remaining: Vec<_> = names.into_iter().filter(|n| (!safe_finder || n != FINDER) && !live_names.contains(n)).collect();
                if !remaining.is_empty() && outcome.error.is_none() {
                    outcome.error = Some("Recovery incomplete: evidence remains; preserve and freshly inspect".into());
                }
                self.evidence = Some(remaining.clone());
                outcome.remaining = Some(remaining);
            }
            Err(e) => {
                self.evidence = None;
                let prior = outcome.error.take().map(|s| format!("{s}; ")).unwrap_or_default();
                outcome.error = Some(format!("{prior}remaining evidence inspection failed: {e}"));
            }
        }
        Ok(outcome)
    }
}
