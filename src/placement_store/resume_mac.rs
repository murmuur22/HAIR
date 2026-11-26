// Included in mac. Snapshots find/compare intent; freshly held objects authorize
// the ONLY mutation, pending -> active. No new slot, rewriting or reclamation.
#[cfg(test)]
pub(super) fn resume_test_wrong_device(i: &mut PlacementInspection) {
    i.evidence.directory[0] = i.evidence.directory[0].wrapping_add(1);
}
fn resume_node<'a>(i: &'a PlacementInspection, slot: &str, child: &str) -> Result<&'a InspectionNode> {
    i.evidence.nodes.iter().find(|n| n.parent.as_deref() == Some(OsStr::new(slot)) && n.name == child)
        .ok_or_else(|| format!("Unsupported prepared intent: missing {child}; evidence preserved"))
}
fn resume_record(text: &str, id: &str) -> Result<Decoration> {
    let d: Decoration = toml::from_str(text).map_err(|e| format!("Unsupported incomplete pending decoration: {e}; preserved"))?;
    d.validate()?;
    if d.placement_id != id { return Err("Pending decoration identity/namespace mismatch; preserved".into()); }
    Ok(d)
}
pub(super) fn select_decoration_resume(i: &PlacementInspection, index: usize) -> Result<PreparedDecorationResume<'_>> {
    let row = i.attempts.get(index).ok_or("No such entry in this producing inspection")?;
    if row.kind == AttemptKind::DecorationAnimation {
        return Err("Pending animation retained; not resumable; recovery required. No resume/discard authority; old active record remains".into());
    }
    if row.kind == AttemptKind::DecorationResize {
        return Err("Pending resize retained; not resumable; recovery required. No resume/discard authority; old active record remains".into());
    }
    if !matches!(row.kind, AttemptKind::NewDecoration | AttemptKind::DecorationRelocation) || row.state != AttemptState::DefinitelyInactive {
        return Err("Unsupported resume: no complete pending decoration intent; file history/orphan association is not proof; preserved".into());
    }
    let id = decoration_slot(OsStr::new(&row.identity)).ok_or("Invalid pending namespace")?;
    let mut proposal = resume_record(inspection_text(resume_node(i, &row.identity, "pending.toml")?)?, id)?;
    if let Some(pin) = &mut proposal.pinned {
        let slot = format!("ac-{}", pin.closure);
        let node = resume_node(i, &slot, "closure.toml")?;
        let descriptor: Descriptor = toml::from_str(inspection_text(node)?).map_err(|e| e.to_string())?;
        if descriptor.identity()? != pin.closure { return Err("Retained closure identity mismatch".into()); }
        let bytes = resume_node(i, &slot, "asset.glb")?.bytes.as_ref().ok_or("Missing retained closure bytes")?.clone();
        let loaded = descriptor.load(bytes)?;
        if loaded.spec.id != pin.entry.asset { return Err("Pending pin asset identity mismatch".into()); }
        pin.loaded = Some(loaded);
    }
    let predecessor = i.layout.decorations.iter().find(|d| d.placement_id == id).cloned();
    if let Some(old) = &predecessor { old.validate_relocation(&proposal)?; }
    if predecessor.is_some() != (row.kind == AttemptKind::DecorationRelocation) { return Err("Pending predecessor proof mismatch".into()); }
    // Conservative availability policy: other incomplete/ambiguous history may
    // affect scene admission. Classify it explicitly, never silently erase counts.
    for (other_index, other) in i.attempts.iter().enumerate() {
        if other_index == index || other.state == AttemptState::Published { continue; }
        if other.kind == AttemptKind::PinClosure && proposal.closure_identity().is_some_and(|id| other.identity == format!("ac-{id}")) { continue; }
        return Err(format!("Unsupported alongside unrelated {:?}/{:?} evidence {}; preserved", other.kind, other.state, other.identity));
    }
    let selected = PreparedDecorationResume { inspection: i, index, proposal, predecessor };
    resume_candidate(i.layout.clone(), &selected.proposal, selected.predecessor.as_ref())?;
    Ok(selected)
}
pub(super) fn resume_candidate(mut live: LayoutRead, proposal: &Decoration, predecessor: Option<&Decoration>) -> Result<LayoutRead> {
    live.incomplete_records = live.incomplete_records.checked_sub(1).ok_or("Selected pending evidence missing")?;
    if let Some(old) = predecessor {
        old.validate_relocation(proposal)?;
        let entry = live.decorations.iter_mut().find(|d| d.placement_id == old.placement_id).ok_or("Selected predecessor missing")?;
        if entry != old { return Err("Selected predecessor changed".into()); }
        *entry = proposal.clone();
    } else {
        if live.decorations.iter().any(|d| d.placement_id == proposal.placement_id) { return Err("Pending identity is already active".into()); }
        // Exactly this explicitly referenced complete closure ceases to be an
        // orphan diagnostic; no orphan association is inferred from proximity.
        if let Some(id) = proposal.closure_identity()
            && !live.decorations.iter().any(|d| d.closure_identity() == Some(id)) {
            live.incomplete_records = live.incomplete_records.checked_sub(1).ok_or("Selected closure evidence missing")?;
        }
        live.decorations.push(proposal.clone());
    }
    live.decorations.sort_by(|a,b| a.placement_id.cmp(&b.placement_id));
    if live.incomplete_records != 0 || !live.warnings.is_empty() { return Err("Unrelated incomplete/unsafe layout evidence; resume unsupported, preserved".into()); }
    Ok(live)
}
struct ResumeHeld {
    slot_name: OsString,
    slot: File,
    pending: File,
    predecessor: Option<File>,
    // At most ONE referenced closure, all names held until activation completes.
    closure: Option<(OsString, File, File, File)>,
}
impl ResumeHeld {
    fn acquire(root: &File, selected: &PreparedDecorationResume<'_>) -> Result<Self> {
        let slot_name = OsString::from(&selected.inspection.attempts[selected.index].identity);
        let slot = open(root, &slot_name, DIR)?;
        let pending = regular(&slot, OsStr::new("pending.toml"), READ)?;
        let predecessor = selected.predecessor.as_ref().map(|_| regular(&slot, OsStr::new("decoration.toml"), READ)).transpose()?;
        let closure = selected.proposal.closure_identity().map(|id| -> Result<_> {
            let name = OsString::from(format!("ac-{id}"));
            let dir = open(root, &name, DIR)?;
            let descriptor = regular(&dir, OsStr::new("closure.toml"), READ)?;
            let asset = regular(&dir, OsStr::new("asset.glb"), READ)?;
            Ok((name, dir, descriptor, asset))
        }).transpose()?;
        Ok(Self { slot_name, slot, pending, predecessor, closure })
    }
    fn check(&self, root: &File, selected: &PreparedDecorationResume<'_>) -> Result<Decoration> {
        let i = selected.inspection;
        let check = |parent: &File, name: &OsStr, held: &File, slot: Option<&OsStr>, directory: bool| -> Result<()> {
            let node = i.evidence.nodes.iter().find(|n| n.parent.as_deref() == slot && n.name == name).ok_or("Missing selected acquired object")?;
            owned_metadata(parent, name, held, if directory { libc::S_IFDIR } else { libc::S_IFREG }, if directory { 0o700 } else { 0o600 })?;
            if inspection_stamp(held)? != node.stamp || provenance(held)? != node.origin || !clean_attributes(held, false)?
                || node.bytes.as_ref().is_some_and(|b| pin_bytes(held, node.cap).map(|v| v.as_slice() != b.as_ref()).unwrap_or(true))
                || inspection_stamp(held)? != node.stamp { return Err("Selected held object changed; inspect again".into()); }
            verify_named_identity(parent, name, held)
        };
        check(root, &self.slot_name, &self.slot, None, true)?;
        check(&self.slot, OsStr::new("pending.toml"), &self.pending, Some(&self.slot_name), false)?;
        if let Some(old) = &self.predecessor { check(&self.slot, OsStr::new("decoration.toml"), old, Some(&self.slot_name), false)?; }
        let id = decoration_slot(&self.slot_name).ok_or("Invalid held pending namespace")?;
        let mut proposal = resume_record(&decoration_file_text(&self.pending)?, id)?;
        if let Some((name, dir, descriptor, asset)) = &self.closure {
            check(root, name, dir, None, true)?;
            check(dir, OsStr::new("closure.toml"), descriptor, Some(name), false)?;
            check(dir, OsStr::new("asset.glb"), asset, Some(name), false)?;
            let desc: Descriptor = toml::from_str(&decoration_file_text(descriptor)?).map_err(|e| e.to_string())?;
            if Some(desc.identity()?.as_str()) != proposal.closure_identity() { return Err("Fresh retained closure identity mismatch".into()); }
            let pin = proposal.pinned.as_mut().ok_or("Fresh pin missing")?;
            // Exact held bytes, re-decoded safely; no pack source or theme fallback.
            let loaded = desc.load(Arc::from(pin_bytes(asset, GLB_BYTES)?))?;
            if loaded.spec.id != pin.entry.asset { return Err("Fresh retained closure asset mismatch".into()); }
            pin.loaded = Some(loaded);
        }
        if proposal != selected.proposal { return Err("Fresh proposal differs from selected intent".into()); }
        if let Some(old) = &self.predecessor {
            let actual = resume_record(&decoration_file_text(old)?, id)?;
            if Some(&actual) != selected.predecessor.as_ref() { return Err("Fresh predecessor differs from selection".into()); }
            actual.validate_relocation(&proposal)?;
        }
        Ok(proposal)
    }
}
fn resume_compare(dir: &File, root: &File, lock: &File, selected: &PreparedDecorationResume<'_>) -> Result<LayoutRead> {
    let i = selected.inspection;
    let dm = dir.metadata().map_err(|e| e.to_string())?;
    if [dm.dev(), dm.ino()] != i.evidence.directory { return Err("Selection belongs to another inspected directory".into()); }
    validate_decoration_storage(dir, root, lock)?;
    i.evidence.check_acquired(root)?;
    let mut names = entries(root)?; names.sort();
    let mut expected: Vec<_> = i.evidence.nodes.iter().filter(|n| n.parent.is_none() && n.name != ROOT).map(|n| n.name.clone()).collect(); expected.sort();
    if names != expected { return Err("Selected namespace changed; inspect again".into()); }
    if names.iter().filter(|n| *n != "lock").count() > MAX_RECORDS || pending_bytes(root, &names)? > crate::inventory::MAX_TOTAL as u64 {
        return Err("Resume storage count/byte budget reached".into());
    }
    let candidates: Vec<_> = inspection_candidates(dir)?.into_iter().map(|c| (c.name, c.identity, c.links, c.token)).collect();
    if candidates != i.evidence.candidates { return Err("Resume live candidates changed".into()); }
    let mut live = read_locked(dir, root)?;
    live.positions.sort_by(|a,b| a.0.cmp(&b.0)); live.retained_slots.sort(); live.warnings.sort();
    if live != i.layout { return Err("Fresh layout differs from producing inspection".into()); }
    Ok(live)
}
pub(super) fn resume_decoration(
    explorer: &Explorer, selected: &PreparedDecorationResume<'_>, cancelled: &AtomicBool,
    validate: &mut dyn FnMut(ResumeStage, &LayoutRead) -> Result<()>,
    mut barrier: impl FnMut(u8) -> Result<()>,
    #[allow(unused_variables)] rename_error: Option<i32>,
) -> Outcome {
    let result = (|| -> Result<Outcome> {
        let cancel = || -> Result<()> { if cancelled.load(Ordering::Acquire) { Err("Decoration resume cancelled; preserved".into()) } else { Ok(()) } };
        cancel()?;
        let dir = explorer.placement_directory()?;
        let root = open(&dir, OsStr::new(ROOT), DIR)?;
        owned_metadata(&dir, OsStr::new(ROOT), &root, libc::S_IFDIR, 0o700)?;
        let lock = regular(&root, OsStr::new("lock"), REWRITE)?;
        owned_metadata(&root, OsStr::new("lock"), &lock, libc::S_IFREG, 0o600)?;
        let lock = HeldFlock::acquire(lock, libc::LOCK_EX)?;
        // Never call inspect_placements/validate_inspection here: independent SH
        // descriptions would deadlock/refuse and cannot authorize a write.
        let live = resume_compare(&dir, &root, &lock, selected)?;
        let held = ResumeHeld::acquire(&root, selected)?;
        let fresh = held.check(&root, selected)?;
        let candidate = resume_candidate(live, &fresh, selected.predecessor.as_ref())?;
        cancel()?;
        validate(ResumeStage::Review, &candidate)?;
        barrier(0)?;
        cancel()?;
        let live = resume_compare(&dir, &root, &lock, selected)?;
        let fresh = held.check(&root, selected)?;
        let final_candidate = resume_candidate(live, &fresh, selected.predecessor.as_ref())?;
        if final_candidate != candidate { return Err("Resume candidate changed after review".into()); }
        // This is the real late-final oracle: caller refreshes external listing,
        // all-page geometry/visitor state and Theme admission while EX is held.
        let mut listing = explorer.try_clone()?;
        listing.refresh()?;
        let before_final_listing = listing.current().clone();
        validate(ResumeStage::Final, &final_candidate)?;
        barrier(1)?;
        resume_compare(&dir, &root, &lock, selected)?;
        held.check(&root, selected)?;
        listing.refresh()?;
        if listing.current() != &before_final_listing { return Err("External listing changed during final resume validation; preserved".into()); }
        cancel()?;
        #[cfg(test)]
        if let Some(code) = rename_error { return Ok(decoration_rename_failure(std::io::Error::from_raw_os_error(code))); }
        let flags = if selected.predecessor.is_some() { 0 } else { libc::RENAME_EXCL };
        let rc = unsafe { libc::renameatx_np(held.slot.as_raw_fd(), c"pending.toml".as_ptr(), held.slot.as_raw_fd(), c"decoration.toml".as_ptr(), flags) };
        if rc != 0 { return Ok(decoration_rename_failure(std::io::Error::last_os_error())); }
        let synced = barrier(2).is_ok() & sync(&held.pending) & sync(&held.slot) & sync(&root) & sync(&dir);
        Ok(Outcome::Committed { synced })
    })();
    result.unwrap_or_else(Outcome::NotCommitted)
}
