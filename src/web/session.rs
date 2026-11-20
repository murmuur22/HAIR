//! Shared held-response admission and opaque replay cursors; not a scene cache or RSS sandbox.
use super::Address;
use crate::Result;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

pub const HTML_SOURCE_POOL_BYTES: usize = 4 * 1024 * 1024;
pub const HTML_SOURCE_POOL_SESSIONS: usize = 16;
// Fixed conservative logical allowance for address/URL/Arc/session metadata.
// Allocator headers/capacity and Page/World/theme allocations are NOT measured here.
const METADATA_CHARGE: usize = 8192;
#[derive(Debug, Default)]
struct Budget {
    bytes: usize,
    sessions: usize,
}
impl Budget {
    fn admit(&mut self, charge: usize) -> Result<()> {
        let bytes = self
            .bytes
            .checked_add(charge)
            .ok_or("HTML source accounting overflow")?;
        if bytes > HTML_SOURCE_POOL_BYTES || self.sessions >= HTML_SOURCE_POOL_SESSIONS {
            return Err("shared held HTML source/session budget exceeded".into());
        }
        self.bytes = bytes;
        self.sessions += 1;
        Ok(())
    }
    fn release(&mut self, charge: usize) {
        self.bytes -= charge;
        self.sessions -= 1;
    }
}
static RETAINED: Mutex<Budget> = Mutex::new(Budget {
    bytes: 0,
    sessions: 0,
});
#[derive(Debug)]
struct Reservation(usize);
impl Drop for Reservation {
    fn drop(&mut self) {
        // No admission on a poisoned pool; destruction must still return its own charge.
        RETAINED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .release(self.0);
    }
}

/// Replay always starts at source start; these cursors never represent raw byte offsets.
/// Fields are private: callers may clone a cursor, never manufacture a boundary.
#[derive(Clone, Debug)]
pub struct WindowCursor {
    session: std::sync::Weak<HtmlSession>,
    policy: u32,
    index: usize,
    start: Option<usize>,
}
impl PartialEq for WindowCursor {
    fn eq(&self, other: &Self) -> bool {
        self.session.ptr_eq(&other.session)
            && self.policy == other.policy
            && self.index == other.index
            && self.start == other.start
    }
}
impl Eq for WindowCursor {}
impl WindowCursor {
    /// Zero-based room ordinal, not an address or network destination.
    pub fn index(&self) -> usize {
        self.index
    }
    fn validate(&self, source: &Arc<HtmlSession>) -> Result<()> {
        if !self.session.ptr_eq(&Arc::downgrade(source))
            || self.policy != source.policy_version()
            || self.index >= 8192
            || self.start.is_some_and(|s| s > 1048576)
        {
            return Err("HTML continuation cursor/session/policy mismatch".into());
        }
        Ok(())
    }
}
/// Constant-size metadata for one room; no semantic tail, scenes or cursor chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowPosition {
    cursor: WindowCursor,
    end: usize,
    completeness: super::Completeness,
}
impl WindowPosition {
    pub(super) fn attach(
        page: &mut super::Page,
        source: &Arc<HtmlSession>,
        index: usize,
        start: usize,
        events: usize,
    ) -> Result<()> {
        let end = match page.completeness {
            super::Completeness::Complete => events,
            super::Completeness::FirstWindow { before_event, .. } => before_event,
        };
        if end <= start || end > events || events > 1048576 || index >= 8192 {
            return Err("HTML continuation made no checked progress".into());
        }
        page.window = Some(Self {
            cursor: WindowCursor {
                session: Arc::downgrade(source),
                policy: source.policy_version(),
                index,
                start: Some(start),
            },
            end,
            completeness: page.completeness,
        });
        page.source = Some(source.clone());
        Ok(())
    }
    pub(super) fn validate(
        &self,
        source: &Arc<HtmlSession>,
        address: &Address,
        completeness: super::Completeness,
    ) -> Result<()> {
        self.cursor.validate(source)?;
        if address != source.address()
            || completeness != self.completeness
            || self.cursor.start.is_none_or(|s| s >= self.end)
        {
            return Err("HTML room position/source/completeness mismatch".into());
        }
        Ok(())
    }
    pub fn index(&self) -> usize {
        self.cursor.index
    }
    /// Half-open tokenizer event range under the retained source's policy.
    pub fn event_range(&self) -> std::ops::Range<usize> {
        self.cursor.start.unwrap_or(0)..self.end
    }
    pub(super) fn next(&self) -> Option<WindowCursor> {
        matches!(self.completeness, super::Completeness::FirstWindow { .. }).then(|| WindowCursor {
            session: self.cursor.session.clone(),
            policy: self.cursor.policy,
            index: self.cursor.index + 1,
            start: Some(self.end),
        })
    }
    pub(super) fn previous(&self) -> Option<WindowCursor> {
        self.cursor.index.checked_sub(1).map(|index| WindowCursor {
            session: self.cursor.session.clone(),
            policy: self.cursor.policy,
            index,
            start: None, // Find canonical predecessor by bounded forward replay.
        })
    }
    pub(super) fn disclosure(&self) -> Option<String> {
        if self.cursor.index == 0 && self.completeness == super::Completeness::Complete {
            return None;
        }
        let source = self.cursor.session.upgrade()?;
        let hash = source
            .sha256()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Some(format!(
            "Room {} | source events {}..{} (end exclusive) | snapshot SHA256 {}\n{}",
            self.cursor.index + 1,
            self.cursor.start.unwrap_or(0),
            self.end,
            hash,
            self.completeness
                .disclosure()
                .unwrap_or("EOF: end of readable content in this held response")
        ))
    }
}

/// One immutable validated response, shared by Page, generated World and history.
/// The address is the literal base supplied to Page::html; no redirect identity is invented.
/// Continuation replays the original tokenizer context, never encoded()[offset..].
pub struct HtmlSession {
    encoded: Box<[u8]>,
    address: Address,
    sha256: [u8; 32],
    validated_scalars: usize,
    validated_tokens: usize,
    completeness: super::Completeness,
    reservation: Reservation,
}
impl std::fmt::Debug for HtmlSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HtmlSession")
            .field("address", &self.address)
            .field("sha256", &self.sha256)
            .field("encoded_bytes", &self.encoded.len())
            .field("validated_scalars", &self.validated_scalars)
            .field("validated_tokens", &self.validated_tokens)
            .finish_non_exhaustive()
    }
}
impl HtmlSession {
    /// Prepare exactly one requested room. Next uses its opaque checked boundary;
    /// Previous reconstructs at most 8192 preceding windows, retaining only one
    /// bounded projection at a time. No network, scene creation or source admission.
    pub fn window(
        self: &Arc<Self>,
        cursor: &WindowCursor,
        control: super::WindowControl<'_>,
    ) -> Result<super::Page> {
        cursor.validate(self)?;
        if let Some((cancel, deadline)) = control {
            cancel.check(deadline)?;
        }
        if let Some(start) = cursor.start {
            return super::html::replay(self, cursor.index, start, control);
        }
        let mut start = 0;
        for index in 0..=cursor.index {
            let page = super::html::replay(self, index, start, control)?;
            if index == cursor.index {
                return Ok(page);
            }
            let next = page
                .next_cursor()?
                .ok_or("HTML predecessor is beyond EOF")?;
            let next_start = next.start.ok_or("HTML next boundary unavailable")?;
            if next_start <= start {
                return Err("HTML replay failed to progress".into());
            }
            start = next_start;
        }
        Err("HTML continuation index unavailable".into())
    }
    pub(super) fn retain(
        address: &Address,
        bytes: &[u8],
        scalars: usize,
        tokens: usize,
        completeness: super::Completeness,
    ) -> Result<Arc<Self>> {
        if bytes.len() > 262144
            || address.original.len() > 2048
            || address.url.as_str().len() > 2048
        {
            return Err("held HTML source identity/input budget exceeded".into());
        }
        let charge = bytes
            .len()
            .checked_add(METADATA_CHARGE)
            .ok_or("HTML source accounting overflow")?;
        RETAINED
            .lock()
            .map_err(|_| "held HTML source budget unavailable")?
            .admit(charge)?;
        let reservation = Reservation(charge);
        Ok(Arc::new(Self {
            encoded: bytes.into(),
            address: address.clone(),
            sha256: Sha256::digest(bytes).into(),
            validated_scalars: scalars,
            validated_tokens: tokens,
            completeness,
            reservation,
        }))
    }
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
    pub fn address(&self) -> &Address {
        &self.address
    }
    pub fn sha256(&self) -> [u8; 32] {
        self.sha256
    }
    pub fn policy_version(&self) -> u32 {
        1
    }
    pub fn validated_bytes(&self) -> usize {
        self.encoded.len()
    }
    pub fn validated_scalars(&self) -> usize {
        self.validated_scalars
    }
    pub fn validated_tokens(&self) -> usize {
        self.validated_tokens
    }
    pub fn completeness(&self) -> super::Completeness {
        self.completeness
    }
    pub fn retained_charge(&self) -> usize {
        self.reservation.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_admission_is_finite_and_releases_without_partial_admission() {
        // Isolated ledger: do not exhaust the process pool used by concurrent parser tests.
        let mut b = Budget::default();
        for _ in 0..HTML_SOURCE_POOL_SESSIONS {
            b.admit(METADATA_CHARGE).unwrap();
        }
        assert!(b.admit(1).is_err());
        assert_eq!(b.sessions, HTML_SOURCE_POOL_SESSIONS);
        b.release(METADATA_CHARGE);
        b.admit(METADATA_CHARGE).unwrap();
        let mut b = Budget::default();
        b.admit(HTML_SOURCE_POOL_BYTES).unwrap();
        assert!(b.admit(1).is_err());
        assert!(b.admit(usize::MAX).is_err());
        assert_eq!(b.bytes, HTML_SOURCE_POOL_BYTES);
        assert_eq!(b.sessions, 1);
        b.release(HTML_SOURCE_POOL_BYTES);
        assert_eq!(b.bytes, 0);
        assert_eq!(b.sessions, 0);
    }
}
