//! Read-only acquired inventory. None of these values authorize disposition.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptKind { FileTransaction, NewDecoration, DecorationRelocation, DecorationResize, DecorationAnimation, PinClosure, Decoration }
/// DefinitelyInactive describes evidence, not permission to discard it. In
/// particular file Copy/Move intent is not persisted and cannot be reconstructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptState { DefinitelyInactive, Published, Ambiguous, Unsupported }

/// Cloneable display data only, NOT an action handle. Checked selection must be
/// bound to its producing private PlacementInspection and freshly validated;
/// accepting a cloned identity/kind/state as authority would be a separate bug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectedAttempt {
    pub(super) identity: String,
    pub(super) kind: AttemptKind,
    pub(super) state: AttemptState,
    pub(super) active_predecessor: Option<bool>,
    pub(super) status: &'static str,
}
impl InspectedAttempt {
    pub fn identity(&self) -> &str { &self.identity }
    pub fn kind(&self) -> AttemptKind { self.kind }
    pub fn state(&self) -> AttemptState { self.state }
    /// Some(true): checked active predecessor remains; Some(false): none in
    /// this attempt's storage protocol. None: file history/intent is unproven.
    pub fn active_predecessor(&self) -> Option<bool> { self.active_predecessor }
    pub fn status(&self) -> &str { self.status }
}

/// Acquired evidence is private, non-serializable and not construction authority.
/// Inspection releases its cooperative shared lock before returning.
/// It retains ONE visited-directory anchor, not every child inode. Child bytes,
/// inode/birth/ctime metadata, ancestry and tokens are sampled and compared with
/// scoped no-follow acquisitions. Completed replacement/mutation is detected in
/// the supported cooperative/accidental-change model, not authenticated against
/// a hostile same-UID actor able to restore all sampled metadata. In particular
/// this is NOT the former stronger all-child-inodes-pinned-for-lifetime guarantee:
/// inode reuse is no longer prevented by holding each inode open. No fabricated
/// filesystem generation token substitutes for that stronger guarantee.
///
/// Application-data read-only does not mean zero system scratch I/O: the same
/// bounded provenance qualification as read_layout applies to closures/nonempty
/// decoration slots. File-only/empty roots introduce no new scratch trigger;
/// unqualified provenance-bearing storage fails closed rather than being sampled
/// opportunistically or treated as supported data.
pub struct PlacementInspection {
    pub(super) layout: LayoutRead,
    pub(super) attempts: Vec<InspectedAttempt>,
    #[cfg(target_os = "macos")]
    pub(super) evidence: super::mac::InspectionEvidence,
}
impl PlacementInspection {
    pub fn layout(&self) -> &LayoutRead { &self.layout }
    /// Lexically ordered storage slots, including explicit Published controls.
    /// One pending relocation beside an active decoration is one attempt here.
    /// These display values cannot independently select/authorize actions.
    pub fn attempts(&self) -> &[InspectedAttempt] { &self.attempts }
}

/// Inventory only. A separate disposition must acquire exclusive lock and freshly
/// revalidate all evidence; this operation never activates, resumes or deletes.
pub fn inspect_placements(explorer: &Explorer) -> Result<PlacementInspection> {
    #[cfg(target_os = "macos")]
    { super::mac::inspect_placements(explorer, || Ok(())) }
    #[cfg(not(target_os = "macos"))]
    { let _ = explorer; Err("Placement inspection requires macOS".into()) }
}

/// Read-only stale-evidence detection, NOT authorization for a later write.
/// A writer must reacquire exclusive lock and validate under that lock.
/// It must acquire fresh held objects; snapshot stamps alone are never enough.
pub fn validate_inspection(explorer: &Explorer, inspection: &PlacementInspection) -> Result<()> {
    #[cfg(target_os = "macos")]
    { super::mac::validate_inspection(explorer, inspection) }
    #[cfg(not(target_os = "macos"))]
    { let _ = (explorer, inspection); Err("Placement inspection requires macOS".into()) }
}
