//! File-plus-position activation through one exclusive rename. macOS first.
//!
//! Provisional internal adapter, not yet called by the interactive App. Explicit
//! publication creates `.hair-layout/tx-<random>/record.toml`; read-only lookup
//! does not create storage. V2 records activate only for a unique regular file
//! matching a 128-bit xattr token plus inode and birth-time identity in the held
//! directory; same-folder renames follow that file, not its former raw name.
//! Missing files leave dormant records. Different-inode copied attributes cannot
//! activate them; hard-linked ambiguity falls back with a warning. V1 records
//! lack the additional identity and fall back with diagnostics, never auto-rewrite.
//! No witness hardlink is kept. Birth-time changes can lose association, and
//! this is an accident guard, not authentication or hostile same-UID protection.
//!
//! Storage is bounded to 256 staging/history slots and 4096 bytes per read record, with a
//! 32 MiB aggregate logical staging-payload budget checked under the lock before
//! a new attempt. Failed preparation attempts to truncate its held staging file;
//! SIGKILL, a failed truncation, or an ambiguous rename can retain payload bytes.
//! Only clean, empty never-committed slots may be reused under the writer lock;
//! committed history and nonempty/uncertain leftovers are not reclaimed. A reused
//! payload keeps its creation time. Provenance must match a fresh inode sampled
//! on this device in this process; no attributes or ACLs are stripped. Without
//! a verified origin, reuse may require a fresh slot first. Cold decoration
//! reads sample in private same-device temporary storage outside the visited
//! directory; unavailable scratch refuses without writing there. This process
//! supports only its first sampled device: other devices refuse
//! explicitly, not as foreign metadata. Restart HAIR for the intended device
//! with trusted private same-device TMPDIR; restart cannot fix foreign metadata
//! or unavailable scratch and never authorizes cross-device probe creation.
//! At either cap, publication refuses visibly. Bounded syntax-or-schema-invalid owned unpublished records
//! are reported separately by `read_layout` without repair; other malformed or
//! unreadable metadata still errors. Recovery/storage UX must be resolved before
//! App exposure. The lock file/root also persist after
//! the first write attempt. No persistent inventory or power-loss guarantee is
//! implemented. Cooperative writers share a nonblocking, explicitly scoped flock.
//! Close-on-exec alone does not end a lock while a spawned child is pre-exec. The eventual
//! App must validate geometry/routes under that same transaction lock before
//! exposure; this adapter alone deliberately does not authorize room geometry.
//! Confirmed rename is the logical commit. A later sync warning stays committed;
//! uncertain rename errors retain inventory and must not be retried blindly.
//!
//! Decoration relocation keeps the dc-<placement_id> slot and strict v1 schema.
//! A fresh exclusive-created pending.toml replaces only checked decoration.toml
//! atomically, avoiding file-token identity, revisions, migration and another
//! shared slot. Before activation the old record is untouched. A retained pending
//! beside an active record is one incomplete attempt, never a second decoration;
//! it blocks further checked writes. No automatic deletion/recovery is provided.
//!
//! Explicit v2 pinned decorations additionally reference ac-<digest> closure slots,
//! each limited to closure.toml plus one self-contained safely decoded asset.glb.
//! Descriptor preparation uses bounded closure.pending.toml beside asset.glb,
//! exclusively renamed only after complete bytes are synced. Interrupted pending
//! bytes count as incomplete, never decode; malformed final descriptors still error.
//! All slots share the 256 history cap and closure bytes share staging accounting.
//! At most three closures (including orphan/incomplete evidence) are admitted;
//! complete unmatched closures remain recovery evidence. See pinned.rs and the
//! slice6A representation receipt for format/budget and downgrade semantics.
//!
//! `inspect_placements` acquires a bounded read-only inventory with private exact
//! storage evidence and explicit inactive/published/ambiguous/unsupported states.
//! Its shared flock ends on return, even if evidence or pre-exec children live.
//! `validate_inspection` only detects stale evidence read-only; neither API grants
//! activation, cleanup, retry, restoration or deletion authority. The separate
//! prepared-decoration selection/Theme resume action reacquires exclusive lock
//! and fresh held targets; unsupported file/partial/orphan evidence stays inert.
//! Slice8A1 snapshots retain one directory anchor, not all child inodes. Exact
//! acquired bytes supply both layout and labels; bounded scoped reacquisition
//! only compares stale evidence. This deliberately separates non-authorizing
//! sampled identity detection from the stronger lifetime-inode-pin guarantee.
//! Actual acquired payload/closure bytes share 32 MiB; metadata has a separate
//! <=2*256*4096-byte allowance. Encoded GLB Arcs are shared with decoded layout.
//! File-only/empty roots add no scratch-probe trigger; ordinary decoration/closure
//! qualification may still perform bounded system scratch I/O, never owner writes.
use crate::{Result, local_explorer::Explorer};
use std::ffi::OsStr;
mod decoration_record;
pub use decoration_record::Decoration;
#[derive(Clone, Copy)]
enum DecorationUpdate { Relocate, Resize, Animation }
impl DecorationUpdate {
    fn validate(self, expected: &Decoration, proposed: &Decoration) -> Result<()> {
        match self { Self::Relocate => expected.validate_relocation(proposed), Self::Resize => expected.validate_resize(proposed), Self::Animation => expected.validate_animation(proposed) }
    }
}
/// Immutable explicit mode-change intent, not ordinary relocation permission.
pub struct DecorationAnimation { expected: Decoration, proposed: Decoration }
impl DecorationAnimation {
    pub fn new(expected: &Decoration, proposed: &Decoration) -> Result<Self> {
        expected.validate_animation(proposed)?;
        Ok(Self { expected: expected.clone(), proposed: proposed.clone() })
    }
}
pub fn animate_decoration_checked(explorer: &Explorer, intent: &DecorationAnimation,
    mut validate: impl FnMut(&LayoutRead) -> Result<()>) -> Outcome {
    #[cfg(target_os = "macos")]
    { mac::update_decoration_kind(explorer, &intent.expected, &intent.proposed, DecorationUpdate::Animation, &mut validate, |_|Ok(())) }
    #[cfg(not(target_os = "macos"))]
    { let _=(explorer,intent,&mut validate); Outcome::NotCommitted("Decoration transactions require macOS".into()) }
}
/// Explicit checked resize intent; private snapshots are immutable after validation.
pub struct DecorationResize { expected: Decoration, proposed: Decoration }
impl DecorationResize {
    pub fn new(expected: &Decoration, proposed: &Decoration) -> Result<Self> {
        expected.validate_resize(proposed)?;
        Ok(Self { expected: expected.clone(), proposed: proposed.clone() })
    }
}
/// Storage primitive, not scene permission. Caller must validate complete fresh
/// appearance and all-page geometry at BOTH callbacks. Relocation stays strict.
pub fn resize_decoration_checked(explorer: &Explorer, intent: &DecorationResize,
    mut validate: impl FnMut(&LayoutRead) -> Result<()>) -> Outcome {
    #[cfg(target_os = "macos")]
    { mac::update_decoration(explorer, &intent.expected, &intent.proposed, true, &mut validate, |_| Ok(())) }
    #[cfg(not(target_os = "macos"))]
    { let _ = (explorer, intent, &mut validate); Outcome::NotCommitted("Decoration transactions require macOS".into()) }
}
mod inspection;
pub use inspection::{AttemptKind, AttemptState, InspectedAttempt, PlacementInspection, inspect_placements, validate_inspection};
pub(crate) mod resume;
pub use resume::{PreparedDecorationResume, ResumeStage};
pub(crate) mod pinned;
pub use pinned::CheckedPin;
#[cfg(target_os = "macos")]
pub(crate) use mac::{HeldFlock, entries_bounded as storage_entries_bounded};

/// Select actual safely loaded catalog bytes; no filesystem writes in the pack.
pub fn select_pin_asset(theme: &crate::theme::Theme, entry_id: &str) -> Result<CheckedPin> {
    #[cfg(target_os = "macos")]
    { mac::select_pin_asset(theme, entry_id) }
    #[cfg(not(target_os = "macos"))]
    { let _ = (theme, entry_id); Err("Pin acquisition requires macOS".into()) }
}

/// Explicit source plus independent logical placement; validator sees the fresh
/// candidate including the pin, under the same writer lock as activation.
/// Generic storage primitive: accepting here does NOT imply appearance admission.
/// Use Theme::publish_pin_checked for pack+pin admission before writes and in
/// every locked callback, plus the caller's fresh geometry/route validator.
pub fn publish_pin_checked(
    explorer: &Explorer,
    source: &CheckedPin,
    decoration: &Decoration,
    mut validate: impl FnMut(&LayoutRead) -> Result<()>,
) -> Outcome {
    #[cfg(target_os = "macos")]
    { mac::publish_pin(explorer, source, decoration, &mut validate, |_| Ok(())) }
    #[cfg(not(target_os = "macos"))]
    { let _ = (explorer, source, decoration, &mut validate); Outcome::NotCommitted("Pin publication requires macOS".into()) }
}

/// Explicit owned pin publication foundation; not exposed as an App choice.
pub fn pin_decoration_checked(
    explorer: &Explorer,
    theme: &crate::theme::Theme,
    entry_id: &str,
    decoration: &Decoration,
    validate: impl FnMut(&LayoutRead) -> Result<()>,
) -> Outcome {
    match select_pin_asset(theme, entry_id) {
        Ok(source) => publish_pin_checked(explorer, &source, decoration, validate),
        Err(e) => Outcome::NotCommitted(e),
    }
}

/// Checked saved-decoration relocation foundation; no filesystem-file identity.
/// The validator receives the complete candidate layout, with exactly the expected
/// decoration replaced. It must not recursively publish under the held lock.
/// Use placement_scene::validate_decoration_candidate for fresh all-page geometry,
/// not the new-decoration validator (which would append the proposal again).
/// Identity/tag/size changes, stale expected records and no-ops refuse. A prepared
/// refusal retains pending metadata and requires separate recovery before retry.
pub fn relocate_decoration_checked(
    explorer: &Explorer,
    expected: &Decoration,
    proposed: &Decoration,
    mut validate: impl FnMut(&LayoutRead) -> Result<()>,
) -> Outcome {
    #[cfg(target_os = "macos")]
    {
        mac::relocate_decoration(explorer, expected, proposed, &mut validate, |_| Ok(()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (explorer, expected, proposed, &mut validate);
        Outcome::NotCommitted("Decoration transactions require macOS".into())
    }
}

/// Storage foundation only: callers must validate fresh geometry under this lock.
pub fn place_decoration_checked(
    explorer: &Explorer,
    decoration: &Decoration,
    mut validate: impl FnMut(&LayoutRead) -> Result<()>,
) -> Outcome {
    #[cfg(target_os = "macos")]
    {
        mac::place_decoration(explorer, decoration, &mut validate, |_| Ok(()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (explorer, decoration, &mut validate);
        Outcome::NotCommitted("Decoration transactions require macOS".into())
    }
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub at: [f32; 2],
    pub yaw: f32,
    pub size: [f32; 3],
    #[serde(default)]
    pub allow_overlap: bool,
    /// None preserves the legacy horizontal floor transform. Normal plus yaw
    /// defines the full surface orientation without parenting to another file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<crate::placement::surface::Contact>,
    #[serde(default, skip_serializing_if = "crate::placement::assembly::Properties::is_default")]
    pub properties: crate::placement::assembly::Properties,
}
impl Position {
    pub fn validate(&self) -> Result<()> {
        self.properties.validate()?;
        if let Some(s) = self.properties.scaling { s.validate_size(self.size)?; }
        if !self.at.iter().all(|x| x.is_finite() && x.abs() <= 10000.)
            || !self.yaw.is_finite()
            || self.yaw.abs() > std::f32::consts::TAU
            || !self
                .size
                .iter()
                .all(|x| x.is_finite() && *x > 0. && *x <= 16.)
        {
            return Err("Invalid saved placement".into());
        }
        if let Some(s) = &self.surface {
            s.validate()?;
            if s.at[0] != self.at[0] || s.at[2] != self.at[1] {
                return Err("Conflicting saved placement coordinates".into());
            }
        }
        if self.properties.base && self.surface.is_some_and(|s| !(0.0..=0.0501).contains(&s.at[1]) || s.normal != [0., 1., 0.]) {
            return Err("Document bases require floor contact; switch base off for other surfaces".into());
        }
        Ok(())
    }
    pub fn transform(&self, model: &[crate::assets::Vertex]) -> Result<Vec<crate::assets::Vertex>> {
        self.validate()?;
        if self.properties.base {
            let contact = crate::placement::surface::Contact {
                at: [self.at[0], self.surface.map_or(0., |s| s.at[1]) + crate::placement::assembly::FILE_LIFT * self.properties.scale_factor(), self.at[1]],
                normal: [0., 1., 0.],
            };
            return crate::placement::surface::attached(model, &contact, self.yaw, self.size);
        }
        if let Some(s) = &self.surface {
            crate::placement::surface::attached(model, s, self.yaw, self.size)
        } else {
            crate::placement::grounded(model, self.at, self.yaw, self.size)
        }
    }
    /// Conservative host-owned logical envelope, independent of theme/preview.
    pub fn bounds(&self) -> Result<crate::host_scene::Geometry> {
        let mut b = self.file_bounds()?;
        if let Some(base) = self.base_bounds()? {
            b.at[1] = base.at[1];
            b.size[0] = b.size[0].max(base.size[0]);
            b.size[1] += crate::placement::assembly::FILE_LIFT * self.properties.scale_factor();
            b.size[2] = b.size[2].max(base.size[2]);
        }
        Ok(b)
    }
    pub fn file_bounds(&self) -> Result<crate::host_scene::Geometry> {
        self.validate()?;
        use glam::{Quat, Vec3};
        let mut contact = self.surface.unwrap_or(crate::placement::surface::Contact {
            at: [self.at[0], 0.0, self.at[1]],
            normal: [0.0, 1.0, 0.0],
        });
        if self.properties.base { contact.at[1] += crate::placement::assembly::FILE_LIFT * self.properties.scale_factor(); }
        let turn = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(contact.normal).normalize())
            * Quat::from_rotation_y(self.yaw);
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for x in [-0.5, 0.5] {
            for y in [0.0, 1.0] {
                for z in [-0.5, 0.5] {
                    let p = turn * (Vec3::new(x, y, z) * Vec3::from_array(self.size))
                        + Vec3::from_array(contact.at);
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
        }
        if !lo.is_finite() || !hi.is_finite() {
            return Err("Saved placement envelope overflow".into());
        }
        Ok(crate::host_scene::Geometry {
            at: [(lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5],
            size: (hi - lo).to_array(),
        })
    }
}
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Committed { synced: bool },
    NotCommitted(String),
    Ambiguous(String),
}
pub fn publish(
    explorer: &Explorer,
    name: &OsStr,
    bytes: &[u8],
    position: &Position,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Outcome {
    #[cfg(target_os = "macos")]
    {
        mac::publish(explorer, name, bytes, position, cancelled, |_| Ok(()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (explorer, name, bytes, position, cancelled);
        Outcome::NotCommitted("Placement transactions require macOS".into())
    }
}
/// Reconcile and validate under the same cooperative exclusive lock as commit.
/// The callback receives a read-only live layout; it must not publish recursively.
pub fn publish_checked(
    explorer: &Explorer,
    name: &OsStr,
    bytes: &[u8],
    position: &Position,
    cancelled: &std::sync::atomic::AtomicBool,
    mut validate: impl FnMut(&LayoutRead) -> Result<()>,
) -> Outcome {
    #[cfg(target_os = "macos")]
    {
        mac::publish_checked(explorer, name, bytes, position, cancelled, &mut validate)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (explorer, name, bytes, position, cancelled, &mut validate);
        Outcome::NotCommitted("Placement transactions require macOS".into())
    }
}
/// Read-only layout result. The eventual App must surface incomplete-record
/// diagnostics; this is not inventory recovery or permission to remove storage.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutRead {
    /// Independent owned placements; never filesystem names or file tokens.
    pub decorations: Vec<Decoration>,
    pub positions: Vec<(std::ffi::OsString, Position)>,
    /// Repositioned generated files keep their rank occupied so other bases do not shift.
    pub retained_slots: Vec<std::ffi::OsString>,
    /// Syntax-invalid bounded owned file records proven unpublished by their
    /// retained payload/token, plus inactive decoration slots or pending relocation
    /// attempts. Not a count of all file attempts or retained bytes.
    pub incomplete_records: usize,
    /// Bounded reconciliation diagnostics; records and owner files remain intact.
    pub warnings: Vec<String>,
}
impl LayoutRead {
    #[cfg(test)]
    fn relocating(&self, expected: &Decoration, proposed: &Decoration) -> Result<Self> {
        self.updating_decoration(expected, proposed, false)
    }
    #[cfg(test)]
    fn updating_decoration(&self, expected: &Decoration, proposed: &Decoration, resize: bool) -> Result<Self> {
        self.updating_decoration_kind(expected, proposed, if resize { DecorationUpdate::Resize } else { DecorationUpdate::Relocate })
    }
    fn updating_decoration_kind(&self, expected: &Decoration, proposed: &Decoration, kind: DecorationUpdate) -> Result<Self> {
        kind.validate(expected, proposed)?;
        if self.incomplete_records != 0 || !self.warnings.is_empty() {
            return Err("Layout needs recovery; no decoration relocated".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for d in &self.decorations {
            d.validate()?;
            if !ids.insert(&d.placement_id) { return Err("Duplicate decoration placement identity".into()); }
        }
        let index = self.decorations.iter().position(|d| d.placement_id == expected.placement_id)
            .ok_or("Expected decoration is missing")?;
        if self.decorations[index] != *expected { return Err("Stale expected decoration record".into()); }
        let mut candidate = self.clone();
        candidate.decorations[index] = proposed.clone();
        Ok(candidate)
    }
}
/// Held no-follow file authority, not a display pathname or acquired file bytes.
pub struct Existing {
    name: std::ffi::OsString,
    directory: [u64; 2],
    file: std::fs::File,
    identity: [u64; 3],
    token: Option<Vec<u8>>,
}
impl Existing {
    pub fn name(&self) -> &OsStr {
        &self.name
    }
}
pub fn select_existing(explorer: &Explorer, name: &OsStr) -> Result<Existing> {
    #[cfg(target_os = "macos")]
    {
        mac::select_existing(explorer, name)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (explorer, name);
        Err("Existing-file layout editing requires macOS".into())
    }
}
pub fn validate_existing(explorer: &Explorer, target: &Existing) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        mac::validate_existing(explorer, target)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (explorer, target);
        Err("Existing-file layout editing requires macOS".into())
    }
}
/// Fresh v2 metadata plus one activation-attribute switch on the held file.
/// Never copies/truncates/renames file bytes or rewrites existing records. A new
/// defaulted record flag retains generated-slot rank; old records default false.
pub fn reposition_checked(
    explorer: &Explorer,
    target: &Existing,
    position: &Position,
    mut validate: impl FnMut(&LayoutRead) -> Result<()>,
) -> Outcome {
    #[cfg(target_os = "macos")]
    {
        mac::reposition(explorer, target, position, &mut validate, |_| Ok(()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (explorer, target, position, &mut validate);
        Outcome::NotCommitted("Existing-file layout editing requires macOS".into())
    }
}
/// Position-only compatibility view; interactive callers must use `read_layout`
/// so an unfinished-record warning is not lost.
pub fn positions(explorer: &Explorer) -> Result<Vec<(std::ffi::OsString, Position)>> {
    Ok(read_layout(explorer)?.positions)
}
pub fn read_layout(explorer: &Explorer) -> Result<LayoutRead> {
    #[cfg(target_os = "macos")]
    {
        mac::read_layout(explorer)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = explorer;
        Ok(LayoutRead::default())
    }
}
#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::{
        ffi::{CStr, CString, OsString},
        fs::File,
        io::{Read, Seek, Write},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{
                ffi::{OsStrExt, OsStringExt},
                fs::MetadataExt,
            },
        },
        sync::atomic::{AtomicBool, Ordering},
    };
    const ROOT: &str = ".hair-layout";
    const MAX_RECORDS: usize = 256;
    const RECORD_BYTES: u64 = 4096;
    const DIR: i32 = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    const READ: i32 = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
    const CREATE: i32 =
        libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    const ATTR: &CStr = c"org.hair.placement-token-v1";
    const REWRITE: i32 = libc::O_RDWR | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
    const PROVENANCE: &CStr = c"com.apple.provenance";
    struct FreshOrigin {
        dev: u64,
        value: Option<Vec<u8>>,
    }
    static FRESH_ORIGIN: std::sync::OnceLock<FreshOrigin> = std::sync::OnceLock::new();
    fn provenance(file: &File) -> Result<Option<Vec<u8>>> {
        let mut value = [0u8; 64];
        let n = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                PROVENANCE.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        if n >= 0 {
            return Ok(Some(value[..n as usize].to_vec()));
        }
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::ENOATTR) {
            Ok(None)
        } else {
            Err(format!("Inspect fresh-file provenance: {e}"))
        }
    }
    fn remember_fresh_origin(file: &File) -> Result<()> {
        let dev = file.metadata().map_err(|e| e.to_string())?.dev();
        if !supported_origin_device(dev, FRESH_ORIGIN.get())? {
            let origin = FreshOrigin {
                dev,
                value: provenance(file)?,
            };
            let _ = FRESH_ORIGIN.set(origin);
        }
        supported_origin_device(dev, FRESH_ORIGIN.get())?;
        Ok(())
    }
    include!("placement_store/scratch_origin.rs");
    unsafe extern "C" {
        // SDK sys/acl.h: ACL_TYPE_EXTENDED = 0x100. Opaque storage is
        // inspected only for presence and always released, never modified.
        fn acl_get_fd_np(fd: libc::c_int, kind: libc::c_int) -> *mut libc::c_void;
        fn acl_free(acl: *mut libc::c_void) -> libc::c_int;
    }
    fn no_extended_acl(file: &File) -> Result<bool> {
        let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), 0x100) };
        if acl.is_null() {
            // On this macOS API a valid held fd with no extended ACL returns
            // ENOENT; qualified with named absent/present owned ACL controls.
            let e = std::io::Error::last_os_error();
            return if e.raw_os_error() == Some(libc::ENOENT) {
                Ok(true)
            } else {
                Err(format!("Inspect staging ACL: {e}"))
            };
        }
        if unsafe { acl_free(acl) } != 0 {
            return Err(error("Release staging ACL"));
        }
        Ok(false)
    }
    fn stat_name(parent: &File, name: &OsStr) -> Result<Option<libc::stat>> {
        let n = c(name)?;
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                n.as_ptr(),
                st.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            let e = std::io::Error::last_os_error();
            return if e.raw_os_error() == Some(libc::ENOENT) {
                Ok(None)
            } else {
                Err(format!("Inspect reusable layout entry: {e}"))
            };
        }
        Ok(Some(unsafe { st.assume_init() }))
    }
    fn owned_shape(st: &libc::stat, kind: libc::mode_t, mode: libc::mode_t) -> bool {
        st.st_mode & libc::S_IFMT == kind
            && st.st_mode & 0o7777 == mode
            && st.st_uid == unsafe { libc::geteuid() }
            && st.st_flags == 0
            && (kind == libc::S_IFDIR || st.st_nlink == 1)
    }
    fn clean_attributes(file: &File, payload: bool) -> Result<bool> {
        supported_origin_device(file.metadata().map_err(|e| e.to_string())?.dev(), FRESH_ORIGIN.get())?;
        let mut attrs = [0u8; 256];
        let n = unsafe {
            libc::flistxattr(file.as_raw_fd(), attrs.as_mut_ptr().cast(), attrs.len(), 0)
        };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::ERANGE) {
                return Ok(false);
            }
            return Err(format!("Inspect staging attributes: {e}"));
        }
        let listed = &attrs[..n as usize];
        if (!listed.is_empty() && listed.last() != Some(&0))
            || listed
                .split(|b| *b == 0)
                .filter(|a| !a.is_empty())
                .any(|a| a != PROVENANCE.to_bytes() && !(payload && a == ATTR.to_bytes()))
        {
            return Ok(false);
        }
        // Never strip or blindly accept provenance. Compare against an actual
        // O_EXCL-created inode sampled by this process on the same device.
        if let Some(origin) = FRESH_ORIGIN.get() {
            if provenance(file)? != origin.value {
                return Ok(false);
            }
        } else if listed
            .split(|b| *b == 0)
            .any(|a| a == PROVENANCE.to_bytes())
        {
            return Ok(false);
        }
        no_extended_acl(file)
    }
    fn strict_token(file: &File) -> Result<Option<Vec<u8>>> {
        let mut value = [0u8; 16];
        let n = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                ATTR.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        if n == 16 {
            return Ok(Some(value.to_vec()));
        }
        if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOATTR) {
            return Ok(None);
        }
        Err("Cannot establish reusable staging token".into())
    }
    struct Reusable {
        name: OsString,
        tx: File,
        payload: Option<File>,
        record: Option<File>,
        replace_token: bool,
    }
    fn reusable_slot(root: &File, dir: &File, names: &[OsString]) -> Result<Option<Reusable>> {
        for name in names {
            let raw = name.as_bytes();
            if raw.len() != 35
                || !raw.starts_with(b"tx-")
                || !raw[3..].iter().all(u8::is_ascii_hexdigit)
            {
                continue;
            }
            let Some(st) = stat_name(root, name)? else {
                continue;
            };
            if !owned_shape(&st, libc::S_IFDIR, 0o700) {
                continue;
            }
            let tx = open(root, name, DIR)?;
            verify_named_identity(root, name, &tx)?;
            if !no_extended_acl(&tx)? {
                continue;
            }
            let children = entries(&tx)?;
            if children
                .iter()
                .any(|n| n != "payload" && n != "record.toml")
            {
                continue;
            }
            let ps = stat_name(&tx, OsStr::new("payload"))?;
            let rs = stat_name(&tx, OsStr::new("record.toml"))?;
            // Record without payload is committed history, not a free slot.
            if ps.is_none() && rs.is_some() {
                continue;
            }
            let payload = if let Some(st) = ps {
                if !owned_shape(&st, libc::S_IFREG, 0o600) || st.st_size != 0 {
                    continue;
                }
                let file = regular(&tx, OsStr::new("payload"), REWRITE)?;
                if file.metadata().map_err(|e| e.to_string())?.len() != 0
                    || !clean_attributes(&file, true)?
                {
                    continue;
                }
                Some(file)
            } else {
                None
            };
            let record = if let Some(st) = rs {
                if !owned_shape(&st, libc::S_IFREG, 0o600)
                    || st.st_size < 0
                    || st.st_size as u64 > RECORD_BYTES
                {
                    continue;
                }
                let mut file = regular(&tx, OsStr::new("record.toml"), REWRITE)?;
                if !clean_attributes(&file, false)? {
                    continue;
                }
                let mut text = String::new();
                Read::by_ref(&mut file)
                    .take(RECORD_BYTES + 1)
                    .read_to_string(&mut text)
                    .map_err(|e| e.to_string())?;
                if text.len() as u64 > RECORD_BYTES {
                    continue;
                }
                let Ok(r) = toml::from_str::<Record>(&text) else {
                    continue;
                };
                let final_name = OsString::from_vec(r.name);
                if !matches!(r.version, 1 | 2)
                    || r.token.len() != 16
                    || !valid_name(&final_name)
                    || !valid_position(&r.position)
                {
                    continue;
                }
                if let Some(st) = stat_name(dir, &final_name)? {
                    if st.st_mode & libc::S_IFMT != libc::S_IFREG {
                        continue;
                    }
                    let final_file = regular(dir, &final_name, READ)?;
                    if strict_token(&final_file)? == Some(r.token) {
                        continue;
                    }
                }
                file.rewind().map_err(|e| e.to_string())?;
                Some(file)
            } else {
                None
            };
            let replace_token = match &payload {
                Some(file) => strict_token(file)?.is_some(),
                None => false,
            };
            return Ok(Some(Reusable {
                name: name.clone(),
                tx,
                payload,
                record,
                replace_token,
            }));
        }
        Ok(None)
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Record {
        version: u32,
        name: Vec<u8>,
        token: Vec<u8>,
        #[serde(default)]
        identity: Option<[u64; 3]>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        retained_slot: bool,
        position: Position,
    }
    fn error(context: &str) -> String {
        format!("{context}: {}", std::io::Error::last_os_error())
    }
    fn c(s: &OsStr) -> Result<CString> {
        CString::new(s.as_bytes()).map_err(|_| "NUL in placement component".into())
    }
    fn valid_name(name: &OsStr) -> bool {
        let b = name.as_bytes();
        !b.is_empty()
            && b.len() <= 255
            && !b.contains(&0)
            && !b.contains(&b'/')
            && b != b"."
            && b != b".."
            && b != ROOT.as_bytes()
    }
    fn valid_position(p: &Position) -> bool {
        p.at.iter().all(|x| x.is_finite() && x.abs() <= 10000.)
            && p.properties.scaling.is_none_or(|s| s.validate_size(p.size).is_ok())
            && p.yaw.is_finite()
            && p.yaw.abs() <= std::f32::consts::TAU
            && p.size.iter().all(|x| x.is_finite() && *x > 0. && *x <= 16.)
            && p.surface
                .as_ref()
                .is_none_or(|s| s.validate().is_ok() && s.at[0] == p.at[0] && s.at[2] == p.at[1])
    }
    fn open(parent: &File, name: &OsStr, flags: i32) -> Result<File> {
        let n = c(name)?;
        let fd = unsafe { libc::openat(parent.as_raw_fd(), n.as_ptr(), flags, 0o600) };
        if fd < 0 {
            Err(error("open placement entry"))
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn regular(parent: &File, name: &OsStr, flags: i32) -> Result<File> {
        let n = c(name)?;
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                n.as_ptr(),
                st.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(error("Inspect layout file"));
        }
        let st = unsafe { st.assume_init() };
        if st.st_mode & libc::S_IFMT != libc::S_IFREG {
            return Err("Layout entry is not a regular file".into());
        }
        let file = open(parent, name, flags)?;
        let m = file.metadata().map_err(|e| e.to_string())?;
        if !m.is_file() || m.dev() != st.st_dev as u64 || m.ino() != st.st_ino {
            return Err("Layout entry changed during open".into());
        }
        Ok(file)
    }
    // A held descriptor is authority to an inode, not proof that readers can
    // still discover it by this name. Recheck before the exclusive commit.
    // This detects completed rebinding, not an atomic hostile-namespace CAS.
    fn verify_named_identity(parent: &File, name: &OsStr, held: &File) -> Result<()> {
        let n = c(name)?;
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                n.as_ptr(),
                st.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(error("Revalidate placement storage"));
        }
        let st = unsafe { st.assume_init() };
        let m = held.metadata().map_err(|e| e.to_string())?;
        if m.dev() != st.st_dev as u64 || m.ino() != st.st_ino {
            return Err("Placement storage name changed before commit".into());
        }
        Ok(())
    }
    fn missing(parent: &File, name: &OsStr) -> bool {
        let Ok(n) = c(name) else { return false };
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        (unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                n.as_ptr(),
                st.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        }) < 0
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT)
    }
    fn mkdir(parent: &File, name: &OsStr) -> Result<()> {
        let n = c(name)?;
        if unsafe { libc::mkdirat(parent.as_raw_fd(), n.as_ptr(), 0o700) } < 0 {
            Err(error("create placement directory"))
        } else {
            Ok(())
        }
    }
    fn sync(file: &File) -> bool {
        file.sync_all().is_ok()
    }
    fn entries(dir: &File) -> Result<Vec<OsString>> {
        entries_bounded(dir, MAX_RECORDS + 1)
    }
    pub(crate) fn entries_bounded(dir: &File, bound: usize) -> Result<Vec<OsString>> {
        let file = open(dir, OsStr::new("."), DIR)?;
        use std::os::fd::IntoRawFd;
        let fd = file.into_raw_fd();
        let dp = unsafe { libc::fdopendir(fd) };
        if dp.is_null() {
            unsafe { libc::close(fd) };
            return Err(error("enumerate layout"));
        }
        struct Guard(*mut libc::DIR);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe { libc::closedir(self.0) };
            }
        }
        let _guard = Guard(dp);
        let mut names = vec![];
        loop {
            unsafe { *libc::__error() = 0 };
            let p = unsafe { libc::readdir(dp) };
            if p.is_null() {
                if std::io::Error::last_os_error().raw_os_error() != Some(0) {
                    return Err(error("read layout entries"));
                }
                break;
            }
            let raw = unsafe { CStr::from_ptr((*p).d_name.as_ptr()) }.to_bytes();
            if raw == b"." || raw == b".." {
                continue;
            }
            if names.len() >= bound {
                return Err("Directory reconciliation/storage enumeration limit reached".into());
            }
            names.push(OsString::from_vec(raw.to_vec()));
        }
        Ok(names)
    }
    /// Own one unshared lock descriptor for exactly the transaction scope.
    /// A concurrent posix_spawn can retain its open description before exec,
    /// even with CLOEXEC, so close alone is not a deterministic lock release.
    pub(crate) struct HeldFlock {
        file: File,
        pid: libc::pid_t,
    }
    impl HeldFlock {
        pub(crate) fn acquire(file: File, operation: i32) -> Result<Self> {
            if unsafe { libc::flock(file.as_raw_fd(), operation | libc::LOCK_NB) } != 0 {
                return Err(error("Layout is busy"));
            }
            Ok(Self {
                file,
                pid: unsafe { libc::getpid() },
            })
        }
    }
    impl std::ops::Deref for HeldFlock {
        type Target = File;
        fn deref(&self) -> &File {
            &self.file
        }
    }
    impl Drop for HeldFlock {
        fn drop(&mut self) {
            // Never let a fork child's inherited guard unlock its parent's
            // still-live transaction. No retry or early release on contention.
            if self.pid == unsafe { libc::getpid() } {
                // Failure retains the close fallback (and may remain busy in
                // an inherited child); it cannot authorize another writer.
                unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
            }
        }
    }
    fn layout_lock(root: &File) -> Result<File> {
        let fd = unsafe { libc::openat(root.as_raw_fd(), c"lock".as_ptr(), CREATE, 0o600) };
        let file = if fd >= 0 {
            unsafe { File::from_raw_fd(fd) }
        } else {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() != Some(libc::EEXIST) {
                return Err(format!("Create layout lock: {e}"));
            }
            regular(
                root,
                OsStr::new("lock"),
                libc::O_RDWR | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            )?
        };
        Ok(file)
    }
    fn pending_bytes(root: &File, names: &[OsString]) -> Result<u64> {
        let mut total = 0u64;
        for name in names {
            if name == "lock" {
                continue;
            }
            if decoration_slot(name).is_some() {
                continue; // Metadata only; shares MAX_RECORDS, never file payload.
            }
            if closure_slot(name).is_some() {
                let tx = open(root, name, DIR)?;
                if !missing(&tx, OsStr::new("asset.glb")) {
                    let asset = regular(&tx, OsStr::new("asset.glb"), READ)?;
                    let len = asset.metadata().map_err(|e| e.to_string())?.len();
                    if len > GLB_BYTES { return Err("Pinned asset exceeds encoded bound".into()); }
                    total = total.checked_add(len).ok_or("Layout payload overflow")?;
                }
                continue;
            }
            if !name.as_bytes().starts_with(b"tx-") {
                return Err("Unrecognized layout storage entry".into());
            }
            let tx = open(root, name, DIR)?;
            if missing(&tx, OsStr::new("payload")) {
                continue;
            }
            let payload = regular(&tx, OsStr::new("payload"), READ)?;
            let len = payload.metadata().map_err(|e| e.to_string())?.len();
            if len > crate::local_explorer::MAX_BODY_BYTES {
                return Err("Inactive placement payload exceeds per-file bound".into());
            }
            total = total
                .checked_add(len)
                .ok_or("Inactive placement byte overflow")?;
        }
        Ok(total)
    }
    fn token(file: &File) -> Option<Vec<u8>> {
        let mut b = [0u8; 16];
        let n = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                ATTR.as_ptr(),
                b.as_mut_ptr().cast(),
                b.len(),
                0,
                0,
            )
        };
        (n == 16).then(|| b.to_vec())
    }
    fn read_record_text(tx: &File) -> Result<String> {
        let file = regular(tx, OsStr::new("record.toml"), READ)?;
        let m = file.metadata().map_err(|e| e.to_string())?;
        if !m.is_file() || m.len() > RECORD_BYTES {
            return Err("Invalid layout record bounds".into());
        }
        let mut s = String::new();
        file.take(RECORD_BYTES + 1)
            .read_to_string(&mut s)
            .map_err(|e| e.to_string())?;
        if s.len() as u64 > RECORD_BYTES {
            return Err("Layout record changed beyond bound".into());
        }
        Ok(s)
    }
    // A syntax error may be an interrupted inactive record write. Establish
    // the cooperative pre-rename state without trusting the malformed text.
    // Never use this to swallow an acquisition, permission or bounds error.
    fn unfinished_record(root: &File, name: &OsStr, tx: &File) -> Result<bool> {
        let raw = name.as_bytes();
        if raw.len() != 35
            || !raw.starts_with(b"tx-")
            || !raw[3..].iter().all(u8::is_ascii_hexdigit)
        {
            return Ok(false);
        }
        let Some(dir_stat) = stat_name(root, name)? else {
            return Ok(false);
        };
        if !owned_shape(&dir_stat, libc::S_IFDIR, 0o700) {
            return Ok(false);
        }
        verify_named_identity(root, name, tx)?;
        let Some(record) = stat_name(tx, OsStr::new("record.toml"))? else {
            return Ok(false);
        };
        if !owned_shape(&record, libc::S_IFREG, 0o600)
            || record.st_size < 0
            || record.st_size as u64 > RECORD_BYTES
        {
            return Ok(false);
        }
        let Some(payload) = stat_name(tx, OsStr::new("payload"))? else {
            return Ok(false);
        };
        if !owned_shape(&payload, libc::S_IFREG, 0o600)
            || payload.st_size < 0
            || payload.st_size as u64 > crate::local_explorer::MAX_BODY_BYTES
        {
            return Ok(false);
        }
        let held = regular(tx, OsStr::new("payload"), READ)?;
        Ok(strict_token(&held)?.is_some())
    }
    fn persistent_identity(file: &File) -> Result<[u64; 3]> {
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        let born = metadata
            .created()
            .map_err(|e| format!("Placement birth identity unavailable: {e}"))?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?;
        Ok([
            metadata.ino(),
            born.as_secs(),
            u64::from(born.subsec_nanos()),
        ])
    }
    struct Candidate {
        name: OsString,
        identity: [u64; 3],
        links: u64,
        token: Vec<u8>,
    }
    fn live_candidates(dir: &File) -> Result<Vec<Candidate>> {
        let dev = dir.metadata().map_err(|e| e.to_string())?.dev();
        let mut out = vec![];
        for name in entries_bounded(dir, crate::local_explorer::MAX_ENTRIES)? {
            let st =
                stat_name(dir, &name)?.ok_or("Directory changed during reconciliation; refresh")?;
            if st.st_mode & libc::S_IFMT != libc::S_IFREG {
                continue;
            }
            let file = regular(dir, &name, READ)?;
            let Some(token) = strict_token(&file)? else {
                continue;
            };
            let metadata = file.metadata().map_err(|e| e.to_string())?;
            if metadata.dev() != dev {
                return Err("Reconciliation file crossed device".into());
            }
            out.push(Candidate {
                name,
                identity: persistent_identity(&file)?,
                links: metadata.nlink(),
                token,
            });
        }
        Ok(out)
    }
    pub(super) fn read_layout(explorer: &Explorer) -> Result<LayoutRead> {
        let dir = explorer.placement_directory()?;
        // A missing layout is the only silently ignored directory-open error.
        let root = match open(&dir, OsStr::new(ROOT), DIR) {
            Ok(d) => d,
            Err(e) => {
                let name = c(OsStr::new(ROOT))?;
                let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
                let r = unsafe {
                    libc::fstatat(
                        dir.as_raw_fd(),
                        name.as_ptr(),
                        st.as_mut_ptr(),
                        libc::AT_SYMLINK_NOFOLLOW,
                    )
                };
                if r < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
                    return Ok(LayoutRead::default());
                }
                return Err(e);
            }
        };
        // Never create a lock or layout during a read-only visit. Cooperating
        // writers hold LOCK_EX for the entire preparation/commit; refuse while
        // busy rather than parsing a partially written inactive record.
        let lock = regular(&root, OsStr::new("lock"), READ)?;
        let lock_meta = lock.metadata().map_err(|e| e.to_string())?;
        if lock_meta.nlink() != 1 || lock_meta.uid() != unsafe { libc::geteuid() } {
            return Err("Invalid layout read lock".into());
        }
        let _lock = HeldFlock::acquire(lock, libc::LOCK_SH)?;
        read_locked(&dir, &root)
    }
    fn read_locked(dir: &File, root: &File) -> Result<LayoutRead> {
        let mut out = vec![];
        let mut decorations = vec![];
        let mut incomplete_records = 0;
        let mut warnings = vec![];
        let mut retained_slots = vec![];
        let candidates = live_candidates(dir)?;
        let names = entries(root)?;
        let closures = read_closures(dir, root, &names)?;
        for name in names {
            if name == "lock" {
                continue;
            }
            if closure_slot(&name).is_some() { continue; }
            if decoration_slot(&name).is_some() {
                owned_metadata(dir, OsStr::new(ROOT), root, libc::S_IFDIR, 0o700)?;
                let held_lock = regular(root, OsStr::new("lock"), READ)?;
                owned_metadata(root, OsStr::new("lock"), &held_lock, libc::S_IFREG, 0o600)?;
                let (record, pending) = read_decoration_slot(dir, root, &name)?;
                if let Some(record) = record { decorations.push(record); }
                incomplete_records += usize::from(pending);
                continue;
            }
            if !name.as_bytes().starts_with(b"tx-") {
                return Err("Unrecognized layout storage entry".into());
            }
            let tx = open(root, &name, DIR)?;
            let text = match read_record_text(&tx) {
                Ok(text) => text,
                Err(_) if missing(&tx, OsStr::new("record.toml")) => continue,
                Err(e) => return Err(format!("Invalid placement metadata: {e}")),
            };
            let record = match toml::from_str::<Record>(&text) {
                Ok(record) => record,
                Err(_) if unfinished_record(root, &name, &tx)? => {
                    incomplete_records += 1;
                    continue;
                }
                Err(e) => return Err(format!("Invalid placement metadata: {e}")),
            };
            let original_name = OsString::from_vec(record.name);
            if !matches!(record.version, 1 | 2)
                || record.token.len() != 16
                || !valid_name(&original_name)
                || !valid_position(&record.position)
            {
                warnings.push("Invalid placement schema/geometry; record preserved".into());
                continue;
            }
            let Some(identity) = record
                .identity
                .filter(|id| record.version == 2 && id[0] != 0 && id[2] < 1_000_000_000)
            else {
                warnings.push(
                    "Legacy or invalid identity record; default placement used; metadata preserved"
                        .into(),
                );
                continue;
            };
            let matches: Vec<_> = candidates
                .iter()
                .filter(|c| c.token == record.token && c.identity == identity)
                .collect();
            if matches.len() > 1 || matches.iter().any(|c| c.links != 1) {
                warnings.push(
                    "Ambiguous hard-linked placement identity; default placement used".into(),
                );
                continue;
            }
            // A copied activation attribute never lends another inode this position.
            if candidates
                .iter()
                .any(|c| c.token == record.token && c.identity != identity)
            {
                warnings
                    .push("Copied placement attribute ignored on a different file identity".into());
            }
            let Some(candidate) = matches.first() else {
                continue;
            };
            let file = regular(dir, &candidate.name, READ)?;
            if persistent_identity(&file)? != identity
                || strict_token(&file)? != Some(record.token)
                || file.metadata().map_err(|e| e.to_string())?.nlink() != 1
            {
                return Err("Placement identity changed during reconciliation; refresh".into());
            }
            verify_named_identity(dir, &candidate.name, &file)?;
            if out.iter().any(|(n, _)| *n == candidate.name) {
                return Err("Conflicting active placement records".into());
            }
            out.push((candidate.name.clone(), record.position));
            if record.retained_slot {
                retained_slots.push(candidate.name.clone());
            }
        }
        incomplete_records += bind_closures(&mut decorations, &closures)?;
        decorations.sort_by(|a, b| a.placement_id.cmp(&b.placement_id));
        Ok(LayoutRead {
            decorations,
            positions: out,
            retained_slots,
            incomplete_records,
            warnings,
        })
    }
    pub(super) fn rename_failure(file: &File, e: std::io::Error) -> Outcome {
        // macOS rename(2): these failures leave both argument files unaffected.
        // EIO, transport/stale/interruption errors and unknown codes remain
        // conservative Ambiguous cases; never truncate evidence for those.
        if matches!(
            e.raw_os_error(),
            Some(
                libc::EACCES
                    | libc::EPERM
                    | libc::EDQUOT
                    | libc::ENOSPC
                    | libc::EROFS
                    | libc::EXDEV
                    | libc::ENOENT
                    | libc::EINVAL
                    | libc::ENOTSUP
                    | libc::EBADF
                    | libc::EISDIR
                    | libc::ENOTDIR
                    | libc::ENOTEMPTY
                    | libc::ELOOP
                    | libc::EDEADLK
                    | libc::EFAULT
                    | libc::EEXIST
                    | libc::EILSEQ
                    | libc::ENAMETOOLONG
            )
        ) {
            if let Err(cleanup) = file.set_len(0) {
                return Outcome::NotCommitted(format!(
                    "Rename refused: {e}; cannot clear owned staging: {cleanup}"
                ));
            }
            Outcome::NotCommitted(format!("Placement rename refused: {e}"))
        } else {
            Outcome::Ambiguous(format!("Placement rename result needs inspection: {e}"))
        }
    }
    pub(super) fn publish(
        explorer: &Explorer,
        name: &OsStr,
        bytes: &[u8],
        position: &Position,
        cancelled: &AtomicBool,
        barrier: impl FnMut(u8) -> Result<()>,
    ) -> Outcome {
        publish_inner(explorer, name, bytes, position, cancelled, barrier, None)
    }
    pub(super) fn publish_checked(
        explorer: &Explorer,
        name: &OsStr,
        bytes: &[u8],
        position: &Position,
        cancelled: &AtomicBool,
        validate: &mut dyn FnMut(&LayoutRead) -> Result<()>,
    ) -> Outcome {
        publish_inner(
            explorer,
            name,
            bytes,
            position,
            cancelled,
            |_| Ok(()),
            Some(validate),
        )
    }
    include!("placement_reposition.rs");
    include!("placement_decoration.rs");
    include!("placement_store/decoration_relocation.rs");
    include!("placement_store/pinned_mac.rs");
    include!("placement_store/inspection_mac.rs");
    include!("placement_store/resume_mac.rs");
    type Validator<'a> = Option<&'a mut dyn FnMut(&LayoutRead) -> Result<()>>;
    fn publish_inner(
        explorer: &Explorer,
        name: &OsStr,
        bytes: &[u8],
        position: &Position,
        cancelled: &AtomicBool,
        mut barrier: impl FnMut(u8) -> Result<()>,
        mut validate: Validator<'_>,
    ) -> Outcome {
        let result = (|| -> Result<Outcome> {
            if !valid_name(name)
                || !valid_position(position)
                || bytes.len() as u64 > crate::local_explorer::MAX_BODY_BYTES
            {
                return Err("Invalid placement name, geometry or byte bound".into());
            }
            let check = |stage: u8, barrier: &mut dyn FnMut(u8) -> Result<()>| -> Result<()> {
                barrier(stage)?;
                if cancelled.load(Ordering::Acquire) {
                    Err("Placement cancelled".into())
                } else {
                    Ok(())
                }
            };
            check(0, &mut barrier)?;
            let dir = explorer.placement_directory()?;
            let root = match open(&dir, OsStr::new(ROOT), DIR) {
                Ok(d) => d,
                Err(e) => {
                    let n = c(OsStr::new(ROOT))?;
                    let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
                    if unsafe {
                        libc::fstatat(
                            dir.as_raw_fd(),
                            n.as_ptr(),
                            st.as_mut_ptr(),
                            libc::AT_SYMLINK_NOFOLLOW,
                        )
                    } == 0
                        || std::io::Error::last_os_error().raw_os_error() != Some(libc::ENOENT)
                    {
                        return Err(e);
                    }
                    mkdir(&dir, OsStr::new(ROOT))?;
                    open(&dir, OsStr::new(ROOT), DIR)?
                }
            };
            let lock = layout_lock(&root)?;
            let m = lock.metadata().map_err(|e| e.to_string())?;
            if !m.is_file() || m.nlink() != 1 || m.uid() != unsafe { libc::geteuid() } {
                return Err("Invalid layout lock".into());
            }
            let lock = HeldFlock::acquire(lock, libc::LOCK_EX)?;
            let layout = read_locked(&dir, &root)?;
            if !layout.decorations.is_empty() && validate.is_none() {
                return Err("Saved decorations require checked geometry validation; unchecked file publication refused".into());
            }
            if let Some(validate) = validate.as_mut() {
                validate(&layout)?;
            }
            let names = entries(&root)?;
            let pending = pending_bytes(&root, &names)?;
            if pending
                .checked_add(bytes.len() as u64)
                .is_none_or(|n| n > crate::inventory::MAX_TOTAL as u64)
            {
                return Err(
                    "Inactive placement staging budget reached (32 MiB); recovery required".into(),
                );
            }
            let mut id = [0u8; 16];
            if unsafe { libc::getentropy(id.as_mut_ptr().cast(), id.len()) } != 0 {
                return Err(error("Generate placement token"));
            }
            let Reusable {
                name: txname,
                tx,
                payload,
                record: mut reusable_record,
                replace_token,
            } = match reusable_slot(&root, &dir, &names)? {
                Some(slot) => slot,
                None => {
                    if names.iter().filter(|n| *n != "lock").count() >= MAX_RECORDS {
                        return Err("Layout storage limit reached; no reusable empty slot".into());
                    }
                    let name = OsString::from(format!(
                        "tx-{}",
                        id.iter().map(|b| format!("{b:02x}")).collect::<String>()
                    ));
                    mkdir(&root, &name)?;
                    let tx = open(&root, &name, DIR)?;
                    Reusable {
                        name,
                        tx,
                        payload: None,
                        record: None,
                        replace_token: false,
                    }
                }
            };
            let mut file = match payload {
                Some(file) => file,
                None => {
                    let file = open(&tx, OsStr::new("payload"), CREATE)?;
                    remember_fresh_origin(&file)?;
                    file
                }
            };
            // Only truncate the still-owned staging descriptor on precommit errors; never unlink a destination or a name that could have been rebound.
            let prepare = (|| -> Result<()> {
                check(1, &mut barrier)?;
                file.write_all(bytes).map_err(|e| e.to_string())?;
                if unsafe {
                    libc::fsetxattr(
                        file.as_raw_fd(),
                        ATTR.as_ptr(),
                        id.as_ptr().cast(),
                        id.len(),
                        0,
                        if replace_token {
                            libc::XATTR_REPLACE
                        } else {
                            libc::XATTR_CREATE
                        },
                    )
                } != 0
                {
                    return Err(error("Store placement activation token (xattrs required)"));
                }
                if token(&file) != Some(id.to_vec()) {
                    return Err("Placement token readback mismatch".into());
                }
                file.sync_all().map_err(|e| e.to_string())?;
                check(2, &mut barrier)?;
                let r = Record {
                    version: 2,
                    name: name.as_bytes().to_vec(),
                    token: id.to_vec(),
                    identity: Some(persistent_identity(&file)?),
                    retained_slot: false,
                    position: position.clone(),
                };
                let text = toml::to_string(&r).map_err(|e| e.to_string())?;
                if text.len() as u64 > RECORD_BYTES {
                    return Err("Placement record exceeds bound".into());
                }
                let mut record = match reusable_record.take() {
                    Some(file) => {
                        file.set_len(0).map_err(|e| e.to_string())?;
                        file
                    }
                    None => open(&tx, OsStr::new("record.toml"), CREATE)?,
                };
                record
                    .write_all(text.as_bytes())
                    .map_err(|e| e.to_string())?;
                record.sync_all().map_err(|e| e.to_string())?;
                if !sync(&tx) || !sync(&root) || !sync(&dir) {
                    return Err("Could not sync inactive placement record".into());
                }
                check(3, &mut barrier)?;
                if let Some(validate) = validate.as_mut() {
                    let live = read_locked(&dir, &root)?;
                    validate(&live)?;
                }
                // Check the prepared inode's exact bounded bytes, not merely
                // valid TOML: an in-place edit must not change this action.
                record.rewind().map_err(|e| e.to_string())?;
                let mut actual = String::new();
                Read::by_ref(&mut record)
                    .take(RECORD_BYTES + 1)
                    .read_to_string(&mut actual)
                    .map_err(|e| e.to_string())?;
                if actual != text {
                    return Err("Prepared placement metadata changed before commit".into());
                }
                verify_named_identity(&dir, OsStr::new(ROOT), &root)?;
                verify_named_identity(&root, OsStr::new("lock"), &lock)?;
                verify_named_identity(&root, OsStr::new(&txname), &tx)?;
                verify_named_identity(&tx, OsStr::new("record.toml"), &record)?;
                Ok(())
            })();
            let prepare = prepare.and_then(|()| {
                let staged = regular(&tx, OsStr::new("payload"), READ)?;
                let a = file.metadata().map_err(|e| e.to_string())?;
                let b = staged.metadata().map_err(|e| e.to_string())?;
                if a.dev() != b.dev() || a.ino() != b.ino() || token(&staged) != Some(id.to_vec()) {
                    return Err("Staged placement changed before commit".into());
                }
                Ok(())
            });
            if let Err(e) = prepare {
                if let Err(cleanup) = file.set_len(0) {
                    return Err(format!("{e}; cannot clear staged bytes: {cleanup}"));
                }
                return Err(e);
            }
            let target = c(name)?;
            let r = unsafe {
                libc::renameatx_np(
                    tx.as_raw_fd(),
                    c"payload".as_ptr(),
                    dir.as_raw_fd(),
                    target.as_ptr(),
                    libc::RENAME_EXCL,
                )
            };
            if r != 0 {
                return Ok(rename_failure(&file, std::io::Error::last_os_error()));
            }
            // No fallible action is allowed to masquerade as ordinary failure after this commit point.
            let synced = barrier(4).is_ok() & sync(&file) & sync(&tx) & sync(&dir);
            Ok(Outcome::Committed { synced })
        })();
        match result {
            Ok(o) => o,
            Err(e) => Outcome::NotCommitted(e),
        }
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::{path::PathBuf, sync::atomic::AtomicBool};
    include!("placement_reposition_tests.rs");
    include!("placement_lock_tests.rs");
    include!("placement_store/slice4c_validator_tests.rs");
    include!("placement_store/slice5a_tests.rs");
    include!("placement_store/r2b_tests.rs");
    include!("placement_store/slice5a_controls.rs");
    include!("placement_store/slice5a_geometry_tests.rs");
    include!("placement_store/slice5a_process_tests.rs");
    include!("placement_store/slice8a_tests.rs");
    include!("placement_store/slice8a_controls.rs");
    include!("placement_store/slice8a_protocol_tests.rs");
    include!("placement_store/slice8a1_tests.rs");
    include!("placement_store/slice8b_tests.rs");
    include!("placement_store/slice8b_controls.rs");
    include!("placement_store/slice8b_geometry_tests.rs");
    include!("placement_store/slice6a_tests.rs");
    include!("placement_store/slice6a_controls.rs");
    include!("placement_store/slice6a1_tests.rs");
    include!("placement_store/slice6a2_tests.rs");
    include!("placement_store/slice6a2_controls.rs");
    include!("placement_store/slice6a3_tests.rs");
    include!("placement_store/slice6a3_controls.rs");
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "hair-placement-store-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn pos() -> Position {
        Position {
            properties: Default::default(),
            at: [2.0, -1.0],
            yaw: 0.5,
            size: [0.8, 0.8, 0.6],
            allow_overlap: false,
            surface: None,
        }
    }
    #[test]
    fn checked_publication_validates_live_layout_under_writer_lock_before_staging() {
        let fixture = Fixture::new();
        let explorer = Explorer::new(&fixture.0).unwrap();
        let cancel = AtomicBool::new(false);
        let mut called = false;
        let outcome = publish_checked(
            &explorer,
            OsStr::new("Blocked"),
            b"must remain inventory",
            &pos(),
            &cancel,
            |layout| {
                called = true;
                assert!(layout.positions.is_empty());
                assert!(read_layout(&explorer).unwrap_err().contains("busy"));
                Err("Live geometry now blocked".into())
            },
        );
        assert_eq!(
            outcome,
            Outcome::NotCommitted("Live geometry now blocked".into())
        );
        assert!(called);
        assert!(!fixture.0.join("Blocked").exists());
        assert!(read_layout(&explorer).unwrap().positions.is_empty());
        assert!(matches!(
            publish(&explorer, OsStr::new("First"), b"one", &pos(), &cancel),
            Outcome::Committed { .. }
        ));
        let outcome = publish_checked(
            &explorer,
            OsStr::new("Second"),
            b"two",
            &pos(),
            &cancel,
            |layout| {
                assert_eq!(layout.positions, vec![("First".into(), pos())]);
                Ok(())
            },
        );
        assert!(matches!(outcome, Outcome::Committed { .. }));
    }
    #[test]
    fn surface_position_persists_contact_normal_height_and_overlap_without_rewriting_legacy() {
        let fixture = Fixture::new();
        let explorer = Explorer::new(&fixture.0).unwrap();
        let input = "at = [2.0, -1.0]\nyaw = 0.5\nsize = [0.8, 0.8, 0.6]\nallow_overlap = true\n[surface]\nat = [2.0, 5.0, -1.0]\nnormal = [1.0, 0.0, 0.0]\n";
        let position: Position = toml::from_str(input).unwrap();
        assert_eq!(
            publish(
                &explorer,
                OsStr::new("Sideways"),
                b"source bytes",
                &position,
                &AtomicBool::new(false)
            ),
            Outcome::Committed { synced: true }
        );
        let read = read_layout(&Explorer::new(&fixture.0).unwrap()).unwrap();
        assert_eq!(read.positions, vec![("Sideways".into(), position.clone())]);
        let actual: toml::Value =
            toml::from_str(&toml::to_string(&read.positions[0].1).unwrap()).unwrap();
        assert_eq!(actual["surface"]["at"][1].as_float(), Some(5.0));
        assert_eq!(actual["surface"]["normal"][0].as_float(), Some(1.0));
        assert_eq!(actual["allow_overlap"].as_bool(), Some(true));
        let legacy: Position =
            toml::from_str("at=[2.0,-1.0]\nyaw=0.5\nsize=[0.8,0.8,0.6]\n").unwrap();
        assert_eq!(legacy, pos());
        for bad in [
            input.replace("normal = [1.0, 0.0, 0.0]", "normal = [2.0, 0.0, 0.0]"),
            input.replace("at = [2.0, 5.0, -1.0]", "at = [3.0, 5.0, -1.0]"),
        ] {
            let position: Position = toml::from_str(&bad).unwrap();
            assert!(matches!(
                publish(
                    &explorer,
                    OsStr::new("Invalid"),
                    b"no",
                    &position,
                    &AtomicBool::new(false)
                ),
                Outcome::NotCommitted(_)
            ));
            assert!(!fixture.0.join("Invalid").exists());
        }
    }
    #[test]
    fn definite_rename_failures_clear_only_owned_staging_bytes() {
        use std::io::Write;
        // Synthetic errno inputs exercise the real file cleanup/classification;
        // they do not simulate or claim an actual permission/disk failure.
        for code in [
            libc::EACCES,
            libc::EPERM,
            libc::EDQUOT,
            libc::ENOSPC,
            libc::EROFS,
            libc::EXDEV,
            libc::ENOENT,
            libc::EINVAL,
            libc::ENOTSUP,
            libc::EBADF,
            libc::EISDIR,
            libc::ENOTDIR,
            libc::ENOTEMPTY,
            libc::ELOOP,
            libc::EDEADLK,
            libc::EFAULT,
            libc::EEXIST,
            libc::EILSEQ,
            libc::ENAMETOOLONG,
        ] {
            let f = Fixture::new();
            let mut staging = std::fs::File::create(f.0.join("staging")).unwrap();
            staging.write_all(b"owned bytes").unwrap();
            std::fs::write(f.0.join("foreign"), b"untouched").unwrap();
            let outcome = mac::rename_failure(&staging, std::io::Error::from_raw_os_error(code));
            assert!(
                matches!(outcome, Outcome::NotCommitted(_)),
                "errno {code}: {outcome:?}"
            );
            assert_eq!(staging.metadata().unwrap().len(), 0, "errno {code}");
            assert_eq!(std::fs::read(f.0.join("foreign")).unwrap(), b"untouched");
        }
    }
    #[test]
    fn uncertain_rename_failures_preserve_staging_evidence() {
        use std::io::Write;
        for code in [
            libc::EIO,
            libc::ETIMEDOUT,
            libc::EINTR,
            libc::ESTALE,
            0,
            9999,
        ] {
            let f = Fixture::new();
            let mut staging = std::fs::File::create(f.0.join("staging")).unwrap();
            staging.write_all(b"owned evidence").unwrap();
            let outcome = mac::rename_failure(&staging, std::io::Error::from_raw_os_error(code));
            assert!(
                matches!(outcome, Outcome::Ambiguous(_)),
                "errno {code}: {outcome:?}"
            );
            assert_eq!(
                std::fs::read(f.0.join("staging")).unwrap(),
                b"owned evidence"
            );
        }
    }
    #[test]
    fn cleanup_failure_is_reported_without_touching_foreign_bytes() {
        let f = Fixture::new();
        let path = f.0.join("staging");
        std::fs::write(&path, b"uncleared owned bytes").unwrap();
        std::fs::write(f.0.join("foreign"), b"preserved").unwrap();
        // A real read-only descriptor makes ftruncate fail, without changing
        // filesystem permissions or manufacturing a syscall result.
        let readonly = std::fs::File::open(&path).unwrap();
        let outcome =
            mac::rename_failure(&readonly, std::io::Error::from_raw_os_error(libc::EEXIST));
        match outcome {
            Outcome::NotCommitted(message) => {
                assert!(message.contains("cannot clear owned staging"), "{message}")
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(std::fs::read(&path).unwrap(), b"uncleared owned bytes");
        assert_eq!(std::fs::read(f.0.join("foreign")).unwrap(), b"preserved");
    }
    #[test]
    fn cancelled_preparations_reuse_only_empty_never_committed_storage() {
        use std::sync::atomic::Ordering;
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        for i in 0..300 {
            let cancelled = AtomicBool::new(false);
            let outcome = mac::publish(
                &explorer,
                OsStr::new("Final"),
                b"owned",
                &pos(),
                &cancelled,
                |stage| {
                    if usize::from(stage) == 1 + i % 3 {
                        if i % 2 == 0 {
                            cancelled.store(true, Ordering::Release);
                        } else {
                            return Err("injected precommit refusal".into());
                        }
                    }
                    Ok(())
                },
            );
            assert!(
                matches!(outcome, Outcome::NotCommitted(_)),
                "{i}: {outcome:?}"
            );
            assert!(!f.0.join("Final").exists());
            assert!(positions(&explorer).unwrap().is_empty());
            assert_eq!(
                std::fs::read_dir(f.0.join(".hair-layout")).unwrap().count(),
                2,
                "iteration {i}: lock plus one never-committed slot"
            );
        }
        assert!(matches!(
            publish(
                &explorer,
                OsStr::new("Final"),
                b"real commit",
                &pos(),
                &AtomicBool::new(false)
            ),
            Outcome::Committed { .. }
        ));
        assert_eq!(std::fs::read(f.0.join("Final")).unwrap(), b"real commit");
        assert_eq!(positions(&explorer).unwrap(), vec![("Final".into(), pos())]);
        assert_eq!(
            std::fs::read_dir(f.0.join(".hair-layout")).unwrap().count(),
            2
        );
    }
    #[test]
    fn a_reusable_slot_still_works_at_the_directory_slot_cap() {
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        assert!(matches!(
            mac::publish(
                &explorer,
                OsStr::new("Never"),
                b"cancelled",
                &pos(),
                &AtomicBool::new(false),
                |stage| if stage == 3 {
                    Err("cancelled fixture".into())
                } else {
                    Ok(())
                }
            ),
            Outcome::NotCommitted(_)
        ));
        let root = f.0.join(".hair-layout");
        // Explicit owned legacy-shaped reservations are not reusable candidates.
        for i in 0..255 {
            std::fs::create_dir(root.join(format!("tx-reserved-{i}"))).unwrap();
        }
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 257);
        assert!(matches!(
            publish(
                &explorer,
                OsStr::new("Final"),
                b"at cap",
                &pos(),
                &AtomicBool::new(false)
            ),
            Outcome::Committed { .. }
        ));
        assert_eq!(std::fs::read_dir(root).unwrap().count(), 257);
        assert_eq!(positions(&explorer).unwrap(), vec![("Final".into(), pos())]);
    }
    #[test]
    fn committed_history_survives_empty_slot_reuse_and_file_restoration() {
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        assert!(matches!(
            publish(
                &explorer,
                OsStr::new("Kept"),
                b"original",
                &pos(),
                &AtomicBool::new(false)
            ),
            Outcome::Committed { .. }
        ));
        let root = f.0.join(".hair-layout");
        let history = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.is_dir())
            .unwrap()
            .join("record.toml");
        let saved = std::fs::read(&history).unwrap();
        // Dormancy requires leaving the immediate directory: a same-folder
        // rename now intentionally follows the file (covered independently).
        let away = f.0.join("away-directory");
        std::fs::create_dir(&away).unwrap();
        std::fs::rename(f.0.join("Kept"), away.join("held")).unwrap();
        for replacement in [false, true] {
            if replacement {
                std::fs::write(f.0.join("Kept"), b"replacement without token").unwrap();
            }
            for _ in 0..32 {
                let outcome = mac::publish(
                    &explorer,
                    OsStr::new("Never"),
                    b"cancelled",
                    &pos(),
                    &AtomicBool::new(false),
                    |stage| {
                        if stage == 3 {
                            Err("cancelled fixture".into())
                        } else {
                            Ok(())
                        }
                    },
                );
                assert!(matches!(outcome, Outcome::NotCommitted(_)));
            }
            assert_eq!(std::fs::read(&history).unwrap(), saved);
            assert!(positions(&explorer).unwrap().is_empty());
        }
        std::fs::rename(f.0.join("Kept"), f.0.join("replacement-away")).unwrap();
        std::fs::rename(away.join("held"), f.0.join("Kept")).unwrap();
        assert_eq!(positions(&explorer).unwrap(), vec![("Kept".into(), pos())]);
        assert_eq!(std::fs::read_dir(root).unwrap().count(), 3);
    }
    #[test]
    fn reuse_refuses_shared_foreign_attributes_acl_and_extra_entries() {
        use std::os::{
            fd::AsRawFd,
            unix::fs::{MetadataExt, PermissionsExt},
        };
        for kind in [
            "payload-link",
            "record-link",
            "extra-xattr",
            "acl",
            "mode",
            "extra-entry",
        ] {
            let f = Fixture::new();
            let explorer = Explorer::new(&f.0).unwrap();
            assert!(matches!(
                mac::publish(
                    &explorer,
                    OsStr::new("Never"),
                    b"cancelled",
                    &pos(),
                    &AtomicBool::new(false),
                    |stage| if stage == 3 {
                        Err("cancelled fixture".into())
                    } else {
                        Ok(())
                    }
                ),
                Outcome::NotCommitted(_)
            ));
            let root = f.0.join(".hair-layout");
            let tx = std::fs::read_dir(&root)
                .unwrap()
                .map(|e| e.unwrap().path())
                .find(|p| p.is_dir())
                .unwrap();
            let payload = tx.join("payload");
            let saved_record = std::fs::read(tx.join("record.toml")).unwrap();
            match kind {
                "payload-link" => std::fs::hard_link(&payload, f.0.join("held-link")).unwrap(),
                "record-link" => {
                    std::fs::hard_link(tx.join("record.toml"), f.0.join("held-link")).unwrap()
                }
                "extra-xattr" => {
                    let file = std::fs::File::open(&payload).unwrap();
                    assert_eq!(
                        unsafe {
                            libc::fsetxattr(
                                file.as_raw_fd(),
                                c"org.hair.fixture".as_ptr(),
                                b"retained".as_ptr().cast(),
                                8,
                                0,
                                libc::XATTR_CREATE,
                            )
                        },
                        0
                    );
                }
                "acl" => {
                    // Add an allow-read ACL only to this owned fixture; no denial,
                    // global security setting or permission-bypass experiment.
                    let id = std::process::Command::new("/usr/bin/id")
                        .arg("-un")
                        .output()
                        .unwrap();
                    assert!(id.status.success());
                    let user = String::from_utf8(id.stdout).unwrap();
                    assert!(
                        std::process::Command::new("/bin/chmod")
                            .args(["+a", &format!("user:{} allow read", user.trim())])
                            .arg(&payload)
                            .status()
                            .unwrap()
                            .success()
                    );
                }
                "mode" => {
                    std::fs::set_permissions(&payload, std::fs::Permissions::from_mode(0o400))
                        .unwrap()
                }
                "extra-entry" => std::fs::write(tx.join("foreign"), b"preserved").unwrap(),
                _ => unreachable!(),
            }
            assert!(
                matches!(
                    publish(
                        &explorer,
                        OsStr::new("New"),
                        b"new body",
                        &pos(),
                        &AtomicBool::new(false)
                    ),
                    Outcome::Committed { .. }
                ),
                "{kind}"
            );
            assert_eq!(
                std::fs::read(tx.join("record.toml")).unwrap(),
                saved_record,
                "{kind}"
            );
            assert_eq!(std::fs::metadata(&payload).unwrap().len(), 0, "{kind}");
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 3, "{kind}");
            if kind == "mode" {
                assert_eq!(std::fs::metadata(&payload).unwrap().mode() & 0o7777, 0o400);
            }
            if kind == "extra-entry" {
                assert_eq!(std::fs::read(tx.join("foreign")).unwrap(), b"preserved");
            }
            if kind == "extra-xattr" {
                let file = std::fs::File::open(&payload).unwrap();
                let mut value = [0u8; 8];
                assert_eq!(
                    unsafe {
                        libc::fgetxattr(
                            file.as_raw_fd(),
                            c"org.hair.fixture".as_ptr(),
                            value.as_mut_ptr().cast(),
                            value.len(),
                            0,
                            0,
                        )
                    },
                    8
                );
                assert_eq!(&value, b"retained");
            }
        }
    }
    #[test]
    fn position_lookup_respects_writer_lock_without_creating_storage() {
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        assert!(positions(&explorer).unwrap().is_empty());
        assert_eq!(std::fs::read_dir(&f.0).unwrap().count(), 0);
        assert!(matches!(
            publish(
                &explorer,
                OsStr::new("Final"),
                b"owned",
                &pos(),
                &AtomicBool::new(false)
            ),
            Outcome::Committed { .. }
        ));
        let root = f.0.join(".hair-layout");
        let lock = std::fs::File::open(root.join("lock")).unwrap();
        let lock = mac::HeldFlock::acquire(lock, libc::LOCK_EX).unwrap();
        let result = positions(&explorer);
        assert!(
            result
                .as_ref()
                .is_err_and(|message| message.contains("busy")),
            "{result:?}"
        );
        drop(lock);
        let shared = std::fs::File::open(root.join("lock")).unwrap();
        let shared = mac::HeldFlock::acquire(shared, libc::LOCK_SH).unwrap();
        assert_eq!(positions(&explorer).unwrap(), vec![("Final".into(), pos())]);
        assert!(matches!(
            publish(
                &explorer,
                OsStr::new("Busy"),
                b"no",
                &pos(),
                &AtomicBool::new(false)
            ),
            Outcome::NotCommitted(_)
        ));
        assert!(!f.0.join("Busy").exists());
        drop(shared);
        assert_eq!(std::fs::read(f.0.join("Final")).unwrap(), b"owned");
    }
    #[test]
    fn pending_payloads_enforce_an_aggregate_staging_budget() {
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        let root = f.0.join(".hair-layout");
        std::fs::create_dir(&root).unwrap();
        // Sparse owned fixtures exercise size accounting without inventing file contents.
        for i in 0..8 {
            let tx = root.join(format!("tx-pending-{i}"));
            std::fs::create_dir(&tx).unwrap();
            std::fs::File::create(tx.join("payload"))
                .unwrap()
                .set_len(crate::local_explorer::MAX_BODY_BYTES)
                .unwrap();
        }
        let outcome = publish(
            &explorer,
            OsStr::new("No"),
            b"one more byte",
            &pos(),
            &AtomicBool::new(false),
        );
        assert!(matches!(outcome, Outcome::NotCommitted(_)), "{outcome:?}");
        assert!(!f.0.join("No").exists());
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            9,
            "budget refusal creates lock only, not another attempt"
        );
        assert!(positions(&explorer).unwrap().is_empty());
    }
    #[test]
    fn maximum_body_boundary_commits_exact_bytes() {
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        let bytes = vec![0x5a; crate::local_explorer::MAX_BODY_BYTES as usize];
        assert!(matches!(
            publish(
                &explorer,
                OsStr::new("Max"),
                &bytes,
                &pos(),
                &AtomicBool::new(false)
            ),
            Outcome::Committed { .. }
        ));
        assert_eq!(std::fs::read(f.0.join("Max")).unwrap(), bytes);
        assert_eq!(positions(&explorer).unwrap().len(), 1);
    }
    #[test]
    fn postcommit_sync_failure_stays_committed_and_cannot_roll_back() {
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        let outcome = mac::publish(
            &explorer,
            OsStr::new("Final"),
            b"committed",
            &pos(),
            &AtomicBool::new(false),
            |stage| {
                if stage == 4 {
                    Err("injected postcommit sync warning".into())
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(outcome, Outcome::Committed { synced: false });
        assert_eq!(std::fs::read(f.0.join("Final")).unwrap(), b"committed");
        assert_eq!(positions(&explorer).unwrap(), vec![("Final".into(), pos())]);
    }
    #[test]
    fn cooperative_lock_refuses_overlapping_publication() {
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        let root = f.0.join(".hair-layout");
        std::fs::create_dir(&root).unwrap();
        let lock = std::fs::File::create(root.join("lock")).unwrap();
        let lock = mac::HeldFlock::acquire(lock, libc::LOCK_EX).unwrap();
        assert!(matches!(
            publish(
                &explorer,
                OsStr::new("No"),
                b"bytes",
                &pos(),
                &AtomicBool::new(false)
            ),
            Outcome::NotCommitted(_)
        ));
        assert!(!f.0.join("No").exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        use std::os::unix::fs::MetadataExt;
        let held = lock.metadata().unwrap();
        let named = std::fs::metadata(root.join("lock")).unwrap();
        assert_eq!((held.dev(), held.ino()), (named.dev(), named.ino()));
        drop(lock);
        let outcome = publish(
            &explorer,
            OsStr::new("Yes"),
            b"bytes",
            &pos(),
            &AtomicBool::new(false),
        );
        assert!(matches!(outcome, Outcome::Committed { .. }),
            "{outcome:?}; fixture={:?}; lock dev={} ino={}", f.0, held.dev(), held.ino());
    }
    #[test]
    #[ignore = "child-only process termination fixture"]
    fn crash_child() {
        let root =
            PathBuf::from(std::env::var_os("HAIR_PLACEMENT_CRASH_ROOT").expect("owned child root"));
        assert!(root.starts_with(std::env::temp_dir()));
        let stage: u8 = std::env::var("HAIR_PLACEMENT_CRASH_STAGE")
            .unwrap()
            .parse()
            .unwrap();
        assert!(stage <= 4);
        let explorer = Explorer::new(&root).unwrap();
        let outcome = mac::publish(
            &explorer,
            OsStr::new("Final"),
            b"crash fixture bytes",
            &pos(),
            &AtomicBool::new(false),
            |s| {
                if s == stage {
                    unsafe { libc::raise(libc::SIGKILL) };
                }
                Ok(())
            },
        );
        panic!("child failed to terminate: {outcome:?}");
    }
    #[test]
    fn actual_rust_adapter_recovers_activation_after_process_termination() {
        use std::os::unix::process::ExitStatusExt;
        for stage in 0..=4 {
            let f = Fixture::new();
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "placement_store::tests::crash_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("HAIR_PLACEMENT_CRASH_ROOT", &f.0)
                .env("HAIR_PLACEMENT_CRASH_STAGE", stage.to_string())
                .output()
                .unwrap();
            assert_eq!(
                child.status.signal(),
                Some(libc::SIGKILL),
                "{}",
                String::from_utf8_lossy(&child.stderr)
            );
            let explorer = Explorer::new(&f.0).unwrap();
            if stage < 4 {
                assert!(!f.0.join("Final").exists());
                assert!(positions(&explorer).unwrap().is_empty());
            } else {
                assert_eq!(
                    std::fs::read(f.0.join("Final")).unwrap(),
                    b"crash fixture bytes"
                );
                assert_eq!(positions(&explorer).unwrap(), vec![("Final".into(), pos())]);
            }
            let mut payloads = vec![];
            if let Ok(txs) = std::fs::read_dir(f.0.join(".hair-layout")) {
                for tx in txs {
                    let tx = tx.unwrap();
                    if tx.file_type().unwrap().is_dir() {
                        let p = tx.path().join("payload");
                        if p.exists() {
                            payloads.push(std::fs::metadata(p).unwrap().len());
                        }
                        assert!(!tx.path().join("witness").exists());
                    }
                }
            }
            let expected = match stage {
                0 | 4 => vec![],
                1 => vec![0],
                2 | 3 => vec![b"crash fixture bytes".len() as u64],
                _ => unreachable!(),
            };
            assert_eq!(
                payloads, expected,
                "retained payload bytes at crash stage {stage}"
            );
            assert!(matches!(
                publish(
                    &explorer,
                    OsStr::new("Recovered"),
                    b"later valid attempt",
                    &pos(),
                    &AtomicBool::new(false)
                ),
                Outcome::Committed { .. }
            ));
            assert_eq!(
                std::fs::read(f.0.join("Recovered")).unwrap(),
                b"later valid attempt"
            );
            assert_eq!(
                positions(&explorer).unwrap().len(),
                if stage == 4 { 2 } else { 1 }
            );
        }
    }
    #[test]
    fn every_precommit_barrier_failure_and_cancel_leaves_no_active_placement() {
        for stage in 0..=3 {
            for cancel in [false, true] {
                let f = Fixture::new();
                let explorer = Explorer::new(&f.0).unwrap();
                let flag = AtomicBool::new(false);
                let output = mac::publish(
                    &explorer,
                    OsStr::new("Target.txt"),
                    b"staged body",
                    &pos(),
                    &flag,
                    |s| {
                        if s == stage {
                            if cancel {
                                flag.store(true, std::sync::atomic::Ordering::Release);
                            } else {
                                return Err(format!("injected precommit failure {s}"));
                            }
                        }
                        Ok(())
                    },
                );
                assert!(
                    matches!(output, Outcome::NotCommitted(_)),
                    "{stage}/{cancel}: {output:?}"
                );
                assert!(!f.0.join("Target.txt").exists());
                assert!(positions(&explorer).unwrap().is_empty());
                if let Ok(txs) = std::fs::read_dir(f.0.join(".hair-layout")) {
                    for tx in txs {
                        let p = tx.unwrap().path().join("payload");
                        if p.exists() {
                            assert_eq!(std::fs::metadata(p).unwrap().len(), 0);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn collision_regular_and_dangling_link_are_preserved() {
        let f = Fixture::new();
        std::fs::write(f.0.join("Existing"), b"foreign bytes").unwrap();
        std::os::unix::fs::symlink(f.0.join("absent"), f.0.join("Link")).unwrap();
        let explorer = Explorer::new(&f.0).unwrap();
        for name in ["Existing", "Link"] {
            assert!(matches!(
                publish(
                    &explorer,
                    OsStr::new(name),
                    b"owned",
                    &pos(),
                    &AtomicBool::new(false)
                ),
                Outcome::NotCommitted(_)
            ));
        }
        assert_eq!(
            std::fs::read(f.0.join("Existing")).unwrap(),
            b"foreign bytes"
        );
        assert_eq!(
            std::fs::read_link(f.0.join("Link")).unwrap(),
            f.0.join("absent")
        );
        let result = positions(&explorer);
        use std::os::unix::fs::MetadataExt;
        let lock = std::fs::metadata(f.0.join(".hair-layout/lock")).unwrap();
        assert!(result.as_ref().is_ok_and(|positions| positions.is_empty()),
            "{result:?}; fixture={:?}; lock dev={} ino={}", f.0, lock.dev(), lock.ino());
    }
    #[test]
    fn held_directory_raw_name_and_replacement_identity() {
        use std::os::unix::ffi::OsStrExt;
        let f = Fixture::new();
        let original = f.0.join("chosen");
        let renamed = f.0.join("moved");
        std::fs::create_dir(&original).unwrap();
        let explorer = Explorer::new(&original).unwrap();
        std::fs::rename(&original, &renamed).unwrap();
        std::fs::create_dir(&original).unwrap();
        let raw = OsStr::from_bytes(b"raw-\n\".txt");
        let output = publish(&explorer, raw, b"first", &pos(), &AtomicBool::new(false));
        assert!(matches!(output, Outcome::Committed { .. }), "{output:?}");
        assert_eq!(
            positions(&explorer).unwrap(),
            vec![(raw.to_os_string(), pos())]
        );
        assert_eq!(std::fs::read_dir(&original).unwrap().count(), 0);
        std::fs::write(renamed.join(raw), b"in-place edit").unwrap();
        assert_eq!(positions(&explorer).unwrap().len(), 1);
        std::fs::write(
            renamed.join("replacement"),
            b"different inode without token",
        )
        .unwrap();
        std::fs::rename(renamed.join("replacement"), renamed.join(raw)).unwrap();
        assert!(positions(&explorer).unwrap().is_empty());
    }
    #[test]
    fn nofollow_metadata_limits_and_invalid_input_fail_closed() {
        let f = Fixture::new();
        let outside = f.0.join("outside");
        let room = f.0.join("room");
        std::fs::create_dir(&outside).unwrap();
        std::fs::create_dir(&room).unwrap();
        let explorer = Explorer::new(&room).unwrap();
        std::os::unix::fs::symlink(&outside, room.join(".hair-layout")).unwrap();
        assert!(positions(&explorer).is_err());
        assert!(matches!(
            publish(
                &explorer,
                OsStr::new("No"),
                b"body",
                &pos(),
                &AtomicBool::new(false)
            ),
            Outcome::NotCommitted(_)
        ));
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        std::fs::remove_file(room.join(".hair-layout")).unwrap();
        for name in ["", ".", "..", "bad/name", ".hair-layout", "bad\0name"] {
            assert!(matches!(
                publish(
                    &explorer,
                    OsStr::new(name),
                    b"body",
                    &pos(),
                    &AtomicBool::new(false)
                ),
                Outcome::NotCommitted(_)
            ));
        }
        let mut bad = pos();
        bad.at[0] = f32::NAN;
        assert!(matches!(
            publish(
                &explorer,
                OsStr::new("No"),
                b"body",
                &bad,
                &AtomicBool::new(false)
            ),
            Outcome::NotCommitted(_)
        ));
        assert_eq!(std::fs::read_dir(&room).unwrap().count(), 0);
        let root = room.join(".hair-layout");
        std::fs::create_dir(&root).unwrap();
        for i in 0..256 {
            std::fs::create_dir(root.join(format!("tx-{i}"))).unwrap();
        }
        assert!(matches!(
            publish(
                &explorer,
                OsStr::new("No"),
                b"body",
                &pos(),
                &AtomicBool::new(false)
            ),
            Outcome::NotCommitted(_)
        ));
        assert!(!room.join("No").exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 257);
    }
    #[test]
    fn torn_unpublished_record_preserves_committed_positions_without_repair() {
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        assert!(matches!(
            publish(
                &explorer,
                OsStr::new("Kept"),
                b"preserved",
                &pos(),
                &AtomicBool::new(false)
            ),
            Outcome::Committed { .. }
        ));
        assert!(matches!(
            mac::publish(
                &explorer,
                OsStr::new("Never"),
                b"unpublished",
                &pos(),
                &AtomicBool::new(false),
                |stage| if stage == 3 {
                    Err("cancelled fixture".into())
                } else {
                    Ok(())
                }
            ),
            Outcome::NotCommitted(_)
        ));
        let root = f.0.join(".hair-layout");
        let pending = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.join("payload").is_file())
            .unwrap();
        let record = pending.join("record.toml");
        std::fs::write(&record, b"version =").unwrap();
        let result = positions(&explorer);
        assert_eq!(result, Ok(vec![("Kept".into(), pos())]));
        let report = read_layout(&explorer).unwrap();
        assert_eq!(report.positions, vec![("Kept".into(), pos())]);
        assert_eq!(report.incomplete_records, 1);
        assert_eq!(std::fs::read(&record).unwrap(), b"version =");
        assert_eq!(std::fs::metadata(pending.join("payload")).unwrap().len(), 0);
        assert_eq!(std::fs::read(f.0.join("Kept")).unwrap(), b"preserved");
        assert!(!f.0.join("Never").exists());
        for bytes in [b"".as_slice(), b"retained unfinished bytes".as_slice()] {
            std::fs::write(pending.join("payload"), bytes).unwrap();
            for text in ["", "version =", "version = 1\nname = ["] {
                std::fs::write(&record, text).unwrap();
                let report = read_layout(&explorer).unwrap();
                assert_eq!(report.positions, vec![("Kept".into(), pos())]);
                assert_eq!(report.incomplete_records, 1);
                assert_eq!(std::fs::read(&record).unwrap(), text.as_bytes());
                assert_eq!(std::fs::read(pending.join("payload")).unwrap(), bytes);
                assert!(!f.0.join("Never").exists());
            }
        }
    }
    #[test]
    fn incomplete_record_classification_refuses_unproven_storage() {
        use std::os::{
            fd::AsRawFd,
            unix::fs::{PermissionsExt, symlink},
        };
        for kind in [
            "committed",
            "missing-payload",
            "payload-symlink",
            "payload-hardlink",
            "payload-mode",
            "record-mode",
            "transaction-mode",
            "record-hardlink",
            "record-symlink",
            "oversized-record",
            "short-token",
            "legacy-name",
        ] {
            let f = Fixture::new();
            let explorer = Explorer::new(&f.0).unwrap();
            let outcome = mac::publish(
                &explorer,
                OsStr::new("Item"),
                b"original",
                &pos(),
                &AtomicBool::new(false),
                |stage| {
                    if kind != "committed" && stage == 3 {
                        Err("fixture refusal".into())
                    } else {
                        Ok(())
                    }
                },
            );
            if kind == "committed" {
                assert!(matches!(outcome, Outcome::Committed { .. }));
            } else {
                assert!(matches!(outcome, Outcome::NotCommitted(_)));
            }
            let root = f.0.join(".hair-layout");
            let mut tx = std::fs::read_dir(&root)
                .unwrap()
                .map(|e| e.unwrap().path())
                .find(|p| p.is_dir())
                .unwrap();
            let record = tx.join("record.toml");
            let payload = tx.join("payload");
            let outside = f.0.join("outside");
            std::fs::write(&outside, b"untouched").unwrap();
            std::fs::write(&record, b"version =").unwrap();
            match kind {
                "missing-payload" => std::fs::remove_file(&payload).unwrap(),
                "payload-symlink" => {
                    std::fs::remove_file(&payload).unwrap();
                    symlink(&outside, &payload).unwrap();
                }
                "payload-hardlink" => std::fs::hard_link(&payload, f.0.join("alias")).unwrap(),
                "payload-mode" => {
                    std::fs::set_permissions(&payload, std::fs::Permissions::from_mode(0o644))
                        .unwrap()
                }
                "record-mode" => {
                    std::fs::set_permissions(&record, std::fs::Permissions::from_mode(0o644))
                        .unwrap()
                }
                "transaction-mode" => {
                    std::fs::set_permissions(&tx, std::fs::Permissions::from_mode(0o755)).unwrap()
                }
                "record-hardlink" => std::fs::hard_link(&record, f.0.join("alias")).unwrap(),
                "record-symlink" => {
                    std::fs::remove_file(&record).unwrap();
                    symlink(&outside, &record).unwrap();
                }
                "oversized-record" => std::fs::write(&record, vec![b'x'; 4097]).unwrap(),
                "short-token" => {
                    let file = std::fs::File::open(&payload).unwrap();
                    assert_eq!(
                        unsafe {
                            libc::fsetxattr(
                                file.as_raw_fd(),
                                c"org.hair.placement-token-v1".as_ptr(),
                                b"x".as_ptr().cast(),
                                1,
                                0,
                                0,
                            )
                        },
                        0
                    );
                }
                "legacy-name" => {
                    let renamed = root.join("tx-legacy");
                    std::fs::rename(&tx, &renamed).unwrap();
                    tx = renamed;
                }
                "committed" => (),
                _ => unreachable!(),
            }
            let stored = std::fs::read(tx.join("record.toml")).unwrap();
            assert!(read_layout(&explorer).is_err(), "{kind}");
            assert_eq!(
                std::fs::read(tx.join("record.toml")).unwrap(),
                stored,
                "{kind}"
            );
            assert_eq!(std::fs::read(&outside).unwrap(), b"untouched", "{kind}");
            if kind == "committed" {
                assert_eq!(std::fs::read(f.0.join("Item")).unwrap(), b"original");
            }
        }
    }
    #[test]
    fn malformed_oversized_or_symlink_record_is_reported() {
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        let tx = f.0.join(".hair-layout/tx-bad");
        std::fs::create_dir_all(&tx).unwrap();
        std::fs::File::create(f.0.join(".hair-layout/lock")).unwrap();
        let record = tx.join("record.toml");
        let check = || {
            assert!(
                positions(&explorer)
                    .is_err_and(|message| message.contains("Invalid placement metadata"))
            )
        };
        std::fs::write(&record, b"bad TOML").unwrap();
        check();
        std::fs::write(&record, vec![b'x'; 4097]).unwrap();
        check();
        std::fs::remove_file(&record).unwrap();
        std::os::unix::fs::symlink(f.0.join("absent"), &record).unwrap();
        check();
    }
    #[test]
    fn invalid_utf8_name_uses_native_filesystem_outcome_without_repair() {
        use std::os::unix::ffi::OsStrExt;
        let f = Fixture::new();
        let native = f.0.join("native");
        let room = f.0.join("room");
        std::fs::create_dir(&native).unwrap();
        std::fs::create_dir(&room).unwrap();
        let raw = OsStr::from_bytes(b"raw-\xff.txt");
        let control = std::fs::write(native.join(raw), b"native control");
        let explorer = Explorer::new(&room).unwrap();
        let outcome = publish(&explorer, raw, b"owned", &pos(), &AtomicBool::new(false));
        match control {
            Ok(()) => {
                assert!(matches!(outcome, Outcome::Committed { .. }));
                assert_eq!(positions(&explorer).unwrap()[0].0, raw);
            }
            Err(e) => {
                assert_eq!(e.raw_os_error(), Some(libc::EILSEQ));
                assert!(matches!(outcome, Outcome::NotCommitted(_)), "{outcome:?}");
                assert!(positions(&explorer).unwrap().is_empty());
            }
        }
    }
    #[test]
    fn prepared_storage_rebinding_refuses_commit_and_preserves_foreign_bytes() {
        let mut observations = vec![];
        for kind in ["root", "lock", "transaction", "record", "record-content"] {
            let f = Fixture::new();
            let explorer = Explorer::new(&f.0).unwrap();
            let mut foreign = None;
            let mut expected_foreign = vec![];
            let mut owned_payload = None;
            let outcome = mac::publish(
                &explorer,
                OsStr::new("Final"),
                b"owned placement bytes",
                &pos(),
                &AtomicBool::new(false),
                |stage| {
                    if stage != 3 {
                        return Ok(());
                    }
                    let root = f.0.join(".hair-layout");
                    let tx = std::fs::read_dir(&root)
                        .unwrap()
                        .map(|e| e.unwrap())
                        .find(|e| e.file_type().unwrap().is_dir())
                        .unwrap()
                        .path();
                    let record = tx.join("record.toml");
                    owned_payload = Some(tx.join("payload"));
                    expected_foreign = b"foreign replacement bytes".to_vec();
                    let target = match kind {
                        "root" => {
                            let away = f.0.join("owned-layout-away");
                            std::fs::rename(&root, &away).unwrap();
                            owned_payload =
                                Some(away.join(tx.file_name().unwrap()).join("payload"));
                            std::fs::create_dir(&root).unwrap();
                            root.join("foreign")
                        }
                        "lock" => {
                            std::fs::rename(root.join("lock"), root.join("owned-lock-away"))
                                .unwrap();
                            root.join("lock")
                        }
                        "transaction" => {
                            let away = root.join("owned-transaction-away");
                            std::fs::rename(&tx, &away).unwrap();
                            owned_payload = Some(away.join("payload"));
                            std::fs::create_dir(&tx).unwrap();
                            tx.join("foreign")
                        }
                        "record" => {
                            std::fs::rename(&record, tx.join("owned-record-away")).unwrap();
                            record
                        }
                        "record-content" => {
                            // Valid TOML with the original name/token but a different
                            // position, written through the same inode.
                            let mut parsed: toml::Value =
                                toml::from_str(&std::fs::read_to_string(&record).unwrap()).unwrap();
                            parsed["position"]["yaw"] = toml::Value::Float(1.5);
                            expected_foreign = toml::to_string(&parsed).unwrap().into_bytes();
                            record
                        }
                        _ => unreachable!(),
                    };
                    std::fs::write(&target, &expected_foreign).unwrap();
                    foreign = Some(target);
                    Ok(())
                },
            );
            assert_eq!(
                std::fs::read(foreign.unwrap()).unwrap(),
                expected_foreign,
                "{kind}"
            );
            let retained_len = std::fs::metadata(owned_payload.unwrap())
                .ok()
                .map(|m| m.len());
            observations.push((kind, outcome, f.0.join("Final").exists(), retained_len));
        }
        assert!(
            observations
                .iter()
                .all(|(_, o, exists, len)| matches!(o, Outcome::NotCommitted(_))
                    && !exists
                    && *len == Some(0)),
            "{observations:?}"
        );
    }
    #[test]
    fn staged_name_rebinding_is_refused_without_deleting_foreign_bytes() {
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        let mut foreign = None;
        let outcome = mac::publish(
            &explorer,
            OsStr::new("Final"),
            b"owned",
            &pos(),
            &AtomicBool::new(false),
            |stage| {
                if stage == 3 {
                    let tx = std::fs::read_dir(f.0.join(".hair-layout"))
                        .unwrap()
                        .map(|e| e.unwrap())
                        .find(|e| e.file_type().unwrap().is_dir())
                        .unwrap()
                        .path();
                    std::fs::rename(tx.join("payload"), tx.join("owned-away")).unwrap();
                    std::fs::write(tx.join("payload"), b"foreign replacement").unwrap();
                    foreign = Some(tx.join("payload"));
                }
                Ok(())
            },
        );
        assert!(matches!(outcome, Outcome::NotCommitted(_)), "{outcome:?}");
        assert_eq!(
            std::fs::read(foreign.unwrap()).unwrap(),
            b"foreign replacement"
        );
        assert!(!f.0.join("Final").exists());
        assert!(positions(&explorer).unwrap().is_empty());
    }
    #[test]
    fn actual_inventory_move_copy_cancel_and_conflict_follow_whole_action() {
        use crate::inventory::Inventory;
        let f = Fixture::new();
        let src = f.0.join("source");
        let dst = f.0.join("dest");
        std::fs::create_dir(&src).unwrap();
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(src.join("Original.txt"), b"unchanged original").unwrap();
        let source = Explorer::new(&src).unwrap();
        let dest = Explorer::new(&dst).unwrap();
        let mut bag = Inventory::default();
        let id = bag.collect(&source, 0).unwrap();
        let yes = AtomicBool::new(true);
        let no = AtomicBool::new(false);
        assert!(matches!(
            bag.place(id, &dest, OsStr::new("Cancelled"), true, &pos(), &yes),
            Outcome::NotCommitted(_)
        ));
        assert_eq!(bag.items().len(), 1);
        assert_eq!(std::fs::read_dir(&dst).unwrap().count(), 0);
        assert!(matches!(
            bag.place(id, &dest, OsStr::new("Copy"), false, &pos(), &no),
            Outcome::Committed { .. }
        ));
        assert_eq!(bag.items().len(), 1);
        assert!(matches!(
            bag.place(id, &dest, OsStr::new("Copy"), true, &pos(), &no),
            Outcome::NotCommitted(_)
        ));
        assert_eq!(bag.items().len(), 1);
        assert_eq!(positions(&dest).unwrap().len(), 1);
        assert!(matches!(
            bag.place(id, &dest, OsStr::new("Moved"), true, &pos(), &no),
            Outcome::Committed { .. }
        ));
        assert!(bag.items().is_empty());
        assert_eq!(positions(&dest).unwrap().len(), 2);
        assert_eq!(
            std::fs::read(src.join("Original.txt")).unwrap(),
            b"unchanged original"
        );
        for n in ["Copy", "Moved"] {
            assert_eq!(std::fs::read(dst.join(n)).unwrap(), b"unchanged original");
        }
    }
    #[test]
    fn same_directory_rename_follows_file_not_its_old_name() {
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        assert!(matches!(
            publish(
                &explorer,
                OsStr::new("Before"),
                b"original identity",
                &pos(),
                &AtomicBool::new(false)
            ),
            Outcome::Committed { .. }
        ));
        let layout_before: Vec<_> = std::fs::read_dir(f.0.join(".hair-layout"))
            .unwrap()
            .map(|n| n.unwrap().path())
            .collect();
        std::fs::rename(f.0.join("Before"), f.0.join("After")).unwrap();
        std::fs::write(f.0.join("Before"), b"different file reusing name").unwrap();
        let report = read_layout(&explorer).unwrap();
        assert_eq!(
            report.positions,
            vec![("After".into(), pos())],
            "same held directory, external rename; old snapshot intentionally stale"
        );
        std::fs::rename(f.0.join("After"), f.0.join("outside")).unwrap();
        let held = f.0.join("outside");
        let away_dir = f.0.join("away-directory");
        std::fs::create_dir(&away_dir).unwrap();
        let away = away_dir.join("held");
        std::fs::rename(&held, &away).unwrap();
        assert!(
            read_layout(&explorer).unwrap().positions.is_empty(),
            "absent file has no ghost"
        );
        std::fs::rename(&away, f.0.join("Restored")).unwrap();
        assert_eq!(
            read_layout(&explorer).unwrap().positions,
            vec![("Restored".into(), pos())],
            "dormant record survives move out and restore"
        );
        let layout_after: Vec<_> = std::fs::read_dir(f.0.join(".hair-layout"))
            .unwrap()
            .map(|n| n.unwrap().path())
            .collect();
        assert_eq!(
            layout_before, layout_after,
            "reconciliation visits create no generations"
        );
        assert_eq!(
            std::fs::read(f.0.join("Before")).unwrap(),
            b"different file reusing name"
        );
    }
    #[test]
    fn copied_activation_attribute_never_grants_a_different_file_placement() {
        use std::os::fd::AsRawFd;
        let f = Fixture::new();
        let e = Explorer::new(&f.0).unwrap();
        assert!(matches!(
            publish(
                &e,
                OsStr::new("Placed"),
                b"owned",
                &pos(),
                &AtomicBool::new(false)
            ),
            Outcome::Committed { .. }
        ));
        let original = std::fs::File::open(f.0.join("Placed")).unwrap();
        let mut token = [0_u8; 16];
        assert_eq!(
            unsafe {
                libc::fgetxattr(
                    original.as_raw_fd(),
                    c"org.hair.placement-token-v1".as_ptr(),
                    token.as_mut_ptr().cast(),
                    16,
                    0,
                    0,
                )
            },
            16
        );
        std::fs::write(f.0.join("Clone"), b"independent owned clone").unwrap();
        let clone = std::fs::File::open(f.0.join("Clone")).unwrap();
        assert_eq!(
            unsafe {
                libc::fsetxattr(
                    clone.as_raw_fd(),
                    c"org.hair.placement-token-v1".as_ptr(),
                    token.as_ptr().cast(),
                    16,
                    0,
                    0,
                )
            },
            0
        );
        let report = read_layout(&e).unwrap();
        assert_eq!(report.positions, vec![("Placed".into(), pos())]);
        assert!(!report.warnings.is_empty());
        std::fs::remove_file(f.0.join("Placed")).unwrap();
        let report = read_layout(&e).unwrap();
        assert!(report.positions.is_empty());
        assert!(
            !report.warnings.is_empty(),
            "copied token without matching birth/inode identity needs an inspectable warning"
        );
        std::fs::rename(f.0.join("Clone"), f.0.join("Placed")).unwrap();
        assert!(
            read_layout(&e).unwrap().positions.is_empty(),
            "even old-name reuse plus copied token is not the original file"
        );
    }
    #[test]
    fn token_commit_reopens_position_without_a_hidden_file_copy() {
        let f = Fixture::new();
        let explorer = Explorer::new(&f.0).unwrap();
        assert!(positions(&explorer).unwrap().is_empty());
        assert_eq!(
            std::fs::read_dir(&f.0).unwrap().count(),
            0,
            "ordinary lookup must not write"
        );
        let output = publish(
            &explorer,
            OsStr::new("Placed.txt"),
            b"real owned bytes",
            &pos(),
            &AtomicBool::new(false),
        );
        assert_eq!(output, Outcome::Committed { synced: true });
        assert_eq!(
            std::fs::read(f.0.join("Placed.txt")).unwrap(),
            b"real owned bytes"
        );
        let reopened = Explorer::new(&f.0).unwrap();
        assert_eq!(
            positions(&reopened).unwrap(),
            vec![("Placed.txt".into(), pos())]
        );
        for tx in std::fs::read_dir(f.0.join(".hair-layout")).unwrap() {
            let tx = tx.unwrap();
            if tx.file_type().unwrap().is_dir() {
                assert!(!tx.path().join("payload").exists());
                assert!(!tx.path().join("witness").exists());
            }
        }
    }
}
