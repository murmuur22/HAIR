// Included in placement_store::mac. Private bounded evidence, never a write capability.
// Only the visited-directory anchor survives return. Child identities are
// sampled, not lifetime inode pins; see inspection.rs for the weaker contract.
struct InspectionNode {
    parent: Option<OsString>,
    name: OsString,
    stamp: [i128; 13],
    bytes: Option<Arc<[u8]>>,
    token: Option<Vec<u8>>,
    origin: Option<Vec<u8>>,
    payload: bool,
    cap: u64,
}
pub(super) struct InspectionEvidence {
    anchor: File,
    directory: [u64; 2],
    nodes: Vec<InspectionNode>,
    candidates: Vec<(OsString, [u64; 3], u64, Vec<u8>)>,
}
fn inspection_stamp(file: &File) -> Result<[i128; 13]> {
    let m = file.metadata().map_err(|e| e.to_string())?;
    let born = persistent_identity(file)?;
    Ok([m.dev().into(), m.ino().into(), m.mode().into(), m.uid().into(), m.nlink().into(),
        m.len().into(), m.mtime().into(), m.mtime_nsec().into(), m.ctime().into(), m.ctime_nsec().into(),
        born[1].into(), born[2].into(), std::os::macos::fs::MetadataExt::st_flags(&m).into()])
}
impl InspectionEvidence {
    // Keep held identity, storage shape and byte-budget inputs explicit at acquisition.
    #[allow(clippy::too_many_arguments)]
    fn acquire_node(&mut self, parent: &File, name: &OsStr, held: &File, slot: Option<&OsStr>, directory: bool, payload: bool, cap: u64, budget: &mut InspectionBytes) -> Result<()> {
        owned_metadata(parent, name, held, if directory { libc::S_IFDIR } else { libc::S_IFREG }, if directory { 0o700 } else { 0o600 })?;
        if held.metadata().map_err(|e| e.to_string())?.dev() != self.directory[0] { return Err("Foreign inspection storage device".into()); }
        let stamp = inspection_stamp(held)?;
        let bytes = if directory { None } else { Some(budget.read(held, cap, payload || name == "asset.glb")?) };
        let token = if payload { strict_token(held)? } else { None };
        let origin = provenance(held)?;
        if inspection_stamp(held)? != stamp { return Err("Inspection bytes/metadata changed during acquisition".into()); }
        verify_named_identity(parent, name, held)?;
        self.nodes.push(InspectionNode { parent: slot.map(OsStr::to_os_string), name: name.into(),
            stamp, bytes, token, origin, payload, cap });
        Ok(())
    }
    fn check_acquired(&self, root: &File) -> Result<()> {
        for n in &self.nodes {
            if FRESH_ORIGIN.get().is_none() && n.origin.is_some() {
                return Err("Unqualified provenance-bearing inspection storage; this ordinary-reader shape supplies no scratch qualification trigger; storage preserved".into());
            }
            // Secondary reads compare only; they NEVER derive labels/layout.
            let slot = n.parent.as_ref().map(|s| open(root, s, DIR)).transpose()?;
            let ancestor = slot.as_ref().unwrap_or(root);
            let ancestor_name = n.parent.as_deref().unwrap_or(OsStr::new(ROOT));
            let ancestor_parent = if slot.is_some() { root } else { &self.anchor };
            let expected = self.nodes.iter().find(|a| a.parent.is_none() && a.name == ancestor_name).ok_or("Missing acquired inspection ancestry")?;
            if inspection_stamp(ancestor)? != expected.stamp { return Err("Inspection ancestry changed".into()); }
            verify_named_identity(ancestor_parent, ancestor_name, ancestor)?;
            let parent = slot.as_ref().unwrap_or(if n.name == ROOT { &self.anchor } else { root });
            let held = if n.bytes.is_none() { open(parent, &n.name, DIR)? } else { regular(parent, &n.name, READ)? };
            owned_metadata(parent, &n.name, &held, if n.bytes.is_none() { libc::S_IFDIR } else { libc::S_IFREG }, if n.bytes.is_none() { 0o700 } else { 0o600 })?;
            if inspection_stamp(&held)? != n.stamp || !clean_attributes(&held, n.payload)?
                || provenance(&held)? != n.origin
                || (n.payload && strict_token(&held)? != n.token)
                || n.bytes.as_ref().is_some_and(|b| pin_bytes(&held, n.cap).map(|v| v.as_slice() != b.as_ref()).unwrap_or(true))
                || inspection_stamp(&held)? != n.stamp {
                return Err("Inspection evidence changed; inspect again".into());
            }
            verify_named_identity(parent, &n.name, &held)?;
            if inspection_stamp(ancestor)? != expected.stamp { return Err("Inspection ancestry changed during comparison".into()); }
            verify_named_identity(ancestor_parent, ancestor_name, ancestor)?;
        }
        Ok(())
    }
    fn same(&self, other: &Self) -> bool {
        self.directory == other.directory && self.candidates == other.candidates && self.nodes.len() == other.nodes.len()
            && self.nodes.iter().zip(&other.nodes).all(|(a,b)| a.parent == b.parent && a.name == b.name && a.stamp == b.stamp
                && a.bytes == b.bytes && a.token == b.token && a.origin == b.origin)
    }
}
fn inspection_attempt(name: &OsStr, kind: AttemptKind, state: AttemptState, active_predecessor: Option<bool>, status: &'static str) -> Result<InspectedAttempt> {
    Ok(InspectedAttempt { identity: name.to_str().ok_or("Invalid inspection namespace")?.into(), kind, state, active_predecessor, status })
}
fn inspect_file_attempt(name: &OsStr, nodes: &[InspectionNode], candidates: &[Candidate]) -> Result<InspectedAttempt> {
    use AttemptState::*;
    let kind = AttemptKind::FileTransaction;
    let payload = nodes.iter().find(|n| n.name == "payload");
    let record = if let Some(n) = nodes.iter().find(|n| n.name == "record.toml") {
        let text = inspection_text(n)?;
        match toml::from_str::<Record>(text) {
            Ok(r) => Some(r),
            Err(_) => {
                // Never turn arbitrary malformed TOML into discard authority.
                // A token visible outside staging could indicate publication.
                if let Some(p) = &payload
                    && p.token.as_ref().is_some_and(|t| candidates.iter().any(|c| &c.token == t)) {
                    return Err("Malformed file record with possible active token".into());
                }
                return inspection_attempt(name, kind, Unsupported, None, "Malformed owned record retained; disposition unsupported");
            }
        }
    } else { None };
    let Some(r) = record else {
        return inspection_attempt(name, kind, Unsupported, None, "Unprepared file slot; no complete token/inode record proof; preserved");
    };
    let original = OsString::from_vec(r.name);
    if r.version == 1 && r.token.len() == 16 && valid_name(&original) && valid_position(&r.position) {
        return inspection_attempt(name, kind, Unsupported, None, "Known legacy file record lacks activation identity proof; preserve, no resume/discard authority");
    }
    if r.version != 2 || r.token.len() != 16 || !valid_name(&original) || !valid_position(&r.position)
        || r.identity.is_none_or(|id| id[0] == 0 || id[2] >= 1_000_000_000) {
        if candidates.iter().any(|c| c.token == r.token) {
            return Err("Unsupported file schema with possible active token".into());
        }
        return inspection_attempt(name, kind, Unsupported, None, "Legacy or unsupported file record; preserved without disposition");
    }
    let identity = r.identity.ok_or("Missing inspection identity")?;
    let token_candidates: Vec<_> = candidates.iter().filter(|c| c.token == r.token).collect();
    let published = payload.is_none() && token_candidates.len() == 1
        && token_candidates[0].identity == identity && token_candidates[0].links == 1;
    if published {
        return inspection_attempt(name, kind, Published, None, "Unique live token/inode binding; published, not recovery evidence");
    }
    if let Some(p) = &payload
        && token_candidates.is_empty() && p.token.as_ref() == Some(&r.token)
        && [p.stamp[1] as u64, p.stamp[10] as u64, p.stamp[11] as u64] == identity && p.stamp[4] == 1 {
        return inspection_attempt(name, kind, DefinitelyInactive, None,
            "Exact acquired staging token/inode; Copy/Move history and predecessor unknown; preserve sole-copy bytes, no discard/resume authority");
    }
    inspection_attempt(name, kind, Ambiguous, None,
        "Publication/history/reposition or copied activation cannot be distinguished; preserve, no action")
}
pub(super) fn inspect_placements(explorer: &Explorer, after_acquire: impl FnOnce() -> Result<()>) -> Result<PlacementInspection> {
    inspect_with_hooks(explorer, after_acquire, |_, _| Ok(()))
}
pub(super) fn inspect_with_hooks(explorer: &Explorer, after_acquire: impl FnOnce() -> Result<()>, mut seam: impl FnMut(&str, &OsStr) -> Result<()>) -> Result<PlacementInspection> {
    let dir = explorer.placement_directory()?;
    let dm = dir.metadata().map_err(|e| e.to_string())?;
    let mut evidence = InspectionEvidence { anchor: dir.try_clone().map_err(|e| e.to_string())?, directory: [dm.dev(), dm.ino()], nodes: vec![], candidates: vec![] };
    let mut budget = InspectionBytes::default();
    if missing(&dir, OsStr::new(ROOT)) {
        return Ok(PlacementInspection { layout: LayoutRead::default(), attempts: vec![], evidence });
    }
    let root = open(&dir, OsStr::new(ROOT), DIR)?;
    evidence.acquire_node(&dir, OsStr::new(ROOT), &root, None, true, false, 0, &mut budget)?;
    seam("root-acquired", OsStr::new(ROOT))?;
    // Open the flock independently; never duplicate/retain its description.
    let lock = regular(&root, OsStr::new("lock"), READ)?;
    owned_metadata(&root, OsStr::new("lock"), &lock, libc::S_IFREG, 0o600)?;
    let _lock = HeldFlock::acquire(lock, libc::LOCK_SH)?;
    evidence.acquire_node(&root, OsStr::new("lock"), &_lock, None, false, false, 0, &mut budget)?;
    let mut names = entries(&root)?;
    names.sort();
    if names.iter().filter(|n| *n != "lock").count() > MAX_RECORDS { return Err("Inspection shared slot limit reached".into()); }
    if names.iter().filter(|n| n.as_bytes().starts_with(b"ac-")).count() > super::pinned::MAX_PIN_CLOSURES { return Err("Pinned closure model/file limit reached (3, including orphan/incomplete slots)".into()); }
    if pending_bytes(&root, &names)? > crate::inventory::MAX_TOTAL as u64 { return Err("Inspection staging byte budget reached".into()); }
    seam("budget-checked", OsStr::new(ROOT))?;
    let mut candidates = inspection_candidates(&dir)?;
    candidates.sort_by(|a,b| a.name.cmp(&b.name));
    for candidate in &candidates {
        evidence.candidates.push((candidate.name.clone(), candidate.identity, candidate.links, candidate.token.clone()));
    }
    for name in names.iter().filter(|n| *n != "lock") {
        let raw = name.as_bytes();
        let dc = decoration_slot(name).is_some();
        let ac = closure_slot(name).is_some();
        let tx_name = raw.len() == 35 && raw.starts_with(b"tx-") && raw[3..].iter().all(u8::is_ascii_hexdigit);
        if !dc && !ac && !tx_name { return Err("Foreign inspection namespace".into()); }
        let tx = open(&root, name, DIR)?;
        evidence.acquire_node(&root, name, &tx, None, true, false, 0, &mut budget)?;
        seam("slot-acquired", name)?;
        let mut children = entries_bounded(&tx, 2)?;
        children.sort();
        // Exactly the ordinary reader's triggers, not every existing root.
        if ac || (dc && !children.is_empty()) { ensure_fresh_origin(&dir)?; }
        for child in &children {
            let payload = tx_name && child == "payload";
            let allowed = if dc { child == "pending.toml" || child == "decoration.toml" }
                else if ac { child == "closure.pending.toml" || child == "closure.toml" || child == "asset.glb" }
                else { child == "payload" || child == "record.toml" };
            if !allowed { return Err("Foreign inspection slot child".into()); }
            let cap = if payload { crate::local_explorer::MAX_BODY_BYTES }
                else if ac && child == "asset.glb" { GLB_BYTES } else { RECORD_BYTES };
            let file = regular(&tx, child, READ)?;
            evidence.acquire_node(&tx, child, &file, Some(name), false, payload, cap, &mut budget)?;
            seam("file-acquired", child)?;
            verify_named_identity(&tx, child, &file)?;
        }
        seam("slot-derived", name)?;
        verify_named_identity(&root, name, &tx)?;
    }
    seam("root-derived", OsStr::new(ROOT))?;
    verify_named_identity(&dir, OsStr::new(ROOT), &root)?;
    // Enforce all existing metadata guards BEFORE parsing/decoding. These
    // secondary reads only compare against primary bytes and sampled ancestry.
    evidence.check_acquired(&root)?;
    seam("before-derive", OsStr::new(ROOT))?;
    let mut layout = inspection_layout(&evidence.nodes, &names, &candidates)?;
    layout.positions.sort_by(|a,b| a.0.cmp(&b.0));
    layout.retained_slots.sort();
    layout.warnings.sort();
    let mut attempts = vec![];
    for name in names.iter().filter(|n| *n != "lock") {
        let nodes: Vec<_> = evidence.nodes.iter().filter(|n| n.parent.as_deref() == Some(name.as_os_str())).collect();
        let children: Vec<_> = nodes.iter().map(|n| n.name.clone()).collect();
        let dc = decoration_slot(name).is_some();
        let ac = closure_slot(name).is_some();
        let has = |n: &str| children.iter().any(|c| c == n);
        let attempt = if dc {
            let active = has("decoration.toml");
            let pending = has("pending.toml");
            let checked_change = if active && pending {
                nodes.iter().find(|n| n.name == "pending.toml")
                    .and_then(|n| inspection_text(n).ok())
                    .and_then(|s| toml::from_str::<Decoration>(s).ok())
                    .and_then(|proposed| layout.decorations.iter().find(|d| d.placement_id == decoration_slot(name).unwrap())
                        .and_then(|old| if old.validate_relocation(&proposed).is_ok() { Some(0) }
                            else if old.validate_resize(&proposed).is_ok() { Some(1) }
                            else if old.validate_legacy_animation(&proposed).is_ok() { Some(2) } else { None }))
            } else { None };
            let kind = if active && pending { AttemptKind::DecorationRelocation }
                else if active { AttemptKind::Decoration } else { AttemptKind::NewDecoration };
            inspection_attempt(name, if checked_change == Some(2) { AttemptKind::DecorationAnimation } else if checked_change == Some(1) { AttemptKind::DecorationResize } else { kind },
                if active && !pending { AttemptState::Published }
                else if active && pending && checked_change != Some(0) { AttemptState::Unsupported }
                else { AttemptState::DefinitelyInactive },
                Some(active && pending), if checked_change == Some(2) { "Pending animation retained; not resumable; recovery required. Checked old active record remains; no automatic action" }
                else if checked_change == Some(1) { "Pending resize retained; not resumable; recovery required. Checked old active record remains; no automatic action" }
                else if active && pending && checked_change.is_none() { "Untrusted pending change retained; unsupported recovery evidence; checked active predecessor remains" }
                else if active && pending { "Separate pending relocation; checked active predecessor remains" }
                else if active { "Checked active decoration; not an incomplete attempt" }
                else { "Owned empty or pending decoration slot; no active record; pending bytes are not activation" })?
        } else if ac {
            let used = layout.decorations.iter().any(|d| d.closure_identity() == closure_slot(name));
            inspection_attempt(name, AttemptKind::PinClosure, if used { AttemptState::Published } else { AttemptState::DefinitelyInactive }, Some(false),
                if used { "Strict complete closure referenced by active decoration" }
                else if has("closure.pending.toml") { "Incomplete closure; bounded opaque pending descriptor, never decoded" }
                else if has("closure.toml") && has("asset.glb") { "Strict finalized orphan closure; no active placement reference; preserve" }
                else { "Incomplete closure shape; missing descriptor or asset; preserve" })?
        } else {
            let start = evidence.nodes.iter().position(|n| n.parent.as_deref() == Some(name.as_os_str()));
            let acquired = start.map(|s| &evidence.nodes[s..s + nodes.len()]).unwrap_or(&[]);
            inspect_file_attempt(name, acquired, &candidates)?
        };
        attempts.push(attempt);
    }
    after_acquire()?;
    seam("after-derive", OsStr::new(ROOT))?;
    evidence.check_acquired(&root)?;
    let mut end_names = entries(&root)?; end_names.sort();
    let mut end_candidates = inspection_candidates(&dir)?; end_candidates.sort_by(|a,b| a.name.cmp(&b.name));
    let end_candidates: Vec<_> = end_candidates.into_iter().map(|c| (c.name, c.identity, c.links, c.token)).collect();
    if names != end_names || evidence.candidates != end_candidates { return Err("Inspection namespace/candidates changed".into()); }
    verify_named_identity(&dir, OsStr::new(ROOT), &root)?;
    if inspection_stamp(&root)? != evidence.nodes[0].stamp { return Err("Inspection root changed before return".into()); }
    Ok(PlacementInspection { layout, attempts, evidence })
}
pub(super) fn validate_inspection(explorer: &Explorer, inspection: &PlacementInspection) -> Result<()> {
    // This fresh operation holds SH only; no future writer may treat it as an
    // authorization to skip its own exclusive-lock validation.
    let fresh = inspect_placements(explorer, || Ok(()))?;
    if !inspection.evidence.same(&fresh.evidence) || inspection.attempts != fresh.attempts || inspection.layout != fresh.layout {
        return Err("Stale inspection snapshot; inspect again".into());
    }
    Ok(())
}
include!("inspection_derive.rs");
#[cfg(test)]
pub(super) fn inspection_origin_initialized() -> bool { FRESH_ORIGIN.get().is_some() }
#[cfg(test)]
pub(super) fn inspection_raw_accounting(i: &PlacementInspection) -> (usize, usize, usize) {
    let mut body = 0; let mut metadata = 0; let mut shared = 0;
    for n in &i.evidence.nodes {
        if let Some(bytes) = &n.bytes {
            if n.payload || n.name == "asset.glb" { body += bytes.len(); } else { metadata += bytes.len(); }
            if n.name == "asset.glb" {
                for d in &i.layout.decorations {
                    if let Some(asset) = d.pinned.as_ref().and_then(|p| p.loaded.as_ref())
                        && Arc::ptr_eq(bytes, &asset.encoded) { shared += 1; }
                }
            }
        }
    }
    (body, metadata, shared)
}
