//! Explicit prepared-decoration action. Display rows are never capabilities.
use super::*;

/// Immutable selection borrowed from exactly one producing inspection. No path,
/// display row, caller Decoration or second inspection can replace its intent.
///
/// ```compile_fail
/// use hair::placement_store::{InspectedAttempt, PreparedDecorationResume};
/// fn forged(row: &InspectedAttempt) -> PreparedDecorationResume<'_> {
///     row.clone() // Cloneable display data is not a selection capability.
/// }
/// ```
/// ```compile_fail
/// use hair::placement_store::{Decoration, PlacementInspection, PreparedDecorationResume};
/// fn forged(i: &PlacementInspection, proposal: Decoration) -> PreparedDecorationResume<'_> {
///     PreparedDecorationResume { inspection: i, index: 0, proposal, predecessor: None }
/// }
/// ```
pub struct PreparedDecorationResume<'a> {
    pub(super) inspection: &'a PlacementInspection,
    pub(super) index: usize,
    pub(super) proposal: Decoration,
    pub(super) predecessor: Option<Decoration>,
}
impl PreparedDecorationResume<'_> {
    pub fn proposal(&self) -> &Decoration { &self.proposal }
    pub fn predecessor(&self) -> Option<&Decoration> { self.predecessor.as_ref() }
    /// Sampled, non-authorizing candidate for write-free App preflight. Uses the
    /// same narrow count/replacement rules as the locked action, not display rows.
    /// Caller must separately validate_inspection and fresh scene state.
    pub fn candidate_layout(&self) -> Result<LayoutRead> {
        #[cfg(target_os = "macos")]
        { super::mac::resume_candidate(self.inspection.layout.clone(), &self.proposal, self.predecessor.as_ref()) }
        #[cfg(not(target_os = "macos"))]
        { Err("Prepared decoration resume requires macOS".into()) }
    }
}
/// Both boundaries run under the same exclusive transaction lock. The mandatory
/// validator must freshly check full appearance, all pages/routes and visitor
/// state, not just accept a previously rendered preview. Never publish recursively.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResumeStage { Review, Final }
impl PlacementInspection {
    /// Read-only selection of a complete pending intent from this inspection.
    /// Unsupported file history, orphan closures and partial records never select.
    /// Unrelated incomplete/ambiguous evidence conservatively blocks this action;
    /// nothing is discarded or silently subtracted to force scene admission.
    pub fn select_decoration_resume(&self, index: usize) -> Result<PreparedDecorationResume<'_>> {
        #[cfg(target_os = "macos")]
        { super::mac::select_decoration_resume(self, index) }
        #[cfg(not(target_os = "macos"))]
        { let _ = index; Err("Prepared decoration resume requires macOS".into()) }
    }
}

/// Internal storage primitive, not geometry permission and not an App entrypoint.
/// The Theme wrapper supplies full appearance admission on every callback.
pub(crate) fn resume_decoration_checked(
    explorer: &Explorer, selection: &PreparedDecorationResume<'_>,
    cancelled: &std::sync::atomic::AtomicBool,
    validate: &mut dyn FnMut(ResumeStage, &LayoutRead) -> Result<()>,
) -> Outcome {
    #[cfg(target_os = "macos")]
    { super::mac::resume_decoration(explorer, selection, cancelled, validate, |_| Ok(()), None) }
    #[cfg(not(target_os = "macos"))]
    { let _ = (explorer, selection, cancelled, validate); Outcome::NotCommitted("Prepared decoration resume requires macOS".into()) }
}
