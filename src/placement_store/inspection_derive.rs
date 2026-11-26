// Included in mac. Derivation consumes ONLY the exact primary acquired bytes.
// This mirrors read_locked's structural rules; ordinary reader/writers are unchanged.
#[derive(Default)]
struct InspectionBytes {
    payload: u64,
    metadata: u64,
}
impl InspectionBytes {
    fn read(&mut self, file: &File, cap: u64, body: bool) -> Result<Arc<[u8]>> {
        // Records are a separate allowance: <=2 children/slot, <=4096 each.
        // Payloads AND closure GLBs share the existing 32 MiB staging budget.
        let limit = if body { crate::inventory::MAX_TOTAL as u64 } else { 2 * MAX_RECORDS as u64 * RECORD_BYTES };
        let used = if body { self.payload } else { self.metadata };
        let remaining = limit.checked_sub(used).ok_or("Inspection acquired byte budget overflow")?;
        let raw = pin_bytes(file, cap.min(remaining)).map_err(|e| format!("Inspection acquired byte budget/bound: {e}"))?;
        let total = used.checked_add(raw.len() as u64).ok_or("Inspection acquired byte budget overflow")?;
        if body { self.payload = total; } else { self.metadata = total; }
        // One owned Vec-to-Arc transfer may transiently duplicate <=8 MB. The
        // retained Arc is shared with decoded layout, never a second raw copy.
        Ok(Arc::from(raw))
    }
}
fn inspection_text(node: &InspectionNode) -> Result<&str> {
    std::str::from_utf8(node.bytes.as_deref().ok_or("Missing acquired record bytes")?).map_err(|e| format!("Invalid acquired metadata UTF-8: {e}"))
}
fn inspection_candidates(dir: &File) -> Result<Vec<Candidate>> {
    let dev = dir.metadata().map_err(|e| e.to_string())?.dev();
    let mut out = vec![];
    for name in entries_bounded(dir, crate::local_explorer::MAX_ENTRIES)? {
        let st = stat_name(dir, &name)?.ok_or("Directory changed during inspection")?;
        if st.st_mode & libc::S_IFMT != libc::S_IFREG { continue; }
        let file = regular(dir, &name, READ)?;
        let before = inspection_stamp(&file)?;
        let token = strict_token(&file)?;
        let identity = persistent_identity(&file)?;
        let m = file.metadata().map_err(|e| e.to_string())?;
        if m.dev() != dev || inspection_stamp(&file)? != before { return Err("Inspection candidate changed/device mismatch".into()); }
        verify_named_identity(dir, &name, &file)?;
        if let Some(token) = token { out.push(Candidate { name, identity, links: m.nlink(), token }); }
    }
    out.sort_by(|a,b| a.name.cmp(&b.name));
    Ok(out)
}
fn inspection_layout(nodes: &[InspectionNode], names: &[OsString], candidates: &[Candidate]) -> Result<LayoutRead> {
    let mut layout = LayoutRead::default();
    let mut closures = Closures { assets: Default::default(), incomplete: 0, bytes: 0 };
    let mut budget = Budget::default();
    let count = names.iter().filter(|n| n.as_bytes().starts_with(b"ac-")).count();
    if count > super::pinned::MAX_PIN_CLOSURES { return Err("Pinned closure model/file limit reached (3, including orphan/incomplete slots)".into()); }
    for name in names.iter().filter(|n| closure_slot(n).is_some()) {
        let id = closure_slot(name).ok_or("Invalid closure namespace")?;
        let children: Vec<_> = nodes.iter().filter(|n| n.parent.as_deref() == Some(name.as_os_str())).collect();
        let has = |s: &str| children.iter().any(|n| n.name == s);
        if has("closure.pending.toml") && (has("closure.toml") || !has("asset.glb")) {
            return Err("Invalid inactive pinned descriptor shape".into());
        }
        let mut descriptor = None;
        let mut bytes = None;
        for child in children {
            if child.name == "closure.toml" {
                let d: Descriptor = toml::from_str(inspection_text(child)?).map_err(|e| format!("Malformed pinned descriptor: {e}"))?;
                d.validate()?;
                if d.identity()? != id { return Err("Pinned descriptor/namespace mismatch".into()); }
                descriptor = Some(d);
            } else if child.name == "asset.glb" {
                let raw = child.bytes.as_ref().ok_or("Missing acquired GLB bytes")?.clone();
                closures.bytes = closures.bytes.checked_add(raw.len() as u64).ok_or("Closure bytes overflow")?;
                if closures.bytes > 24_000_000 { return Err("Pinned aggregate encoded budget".into()); }
                bytes = Some(raw);
            } // closure.pending.toml remains bounded opaque bytes; NEVER parsed.
        }
        match (descriptor, bytes) {
            (Some(d), Some(bytes)) => {
                let loaded = d.load(bytes)?;
                budget.charge_pin(&loaded)?;
                closures.assets.insert(id.to_string(), (d, loaded));
            }
            _ => closures.incomplete += 1,
        }
    }
    for name in names.iter().filter(|n| *n != "lock" && closure_slot(n).is_none()) {
        let children: Vec<_> = nodes.iter().filter(|n| n.parent.as_deref() == Some(name.as_os_str())).collect();
        if let Some(id) = decoration_slot(name) {
            let mut active = None;
            let mut pending = false;
            for n in &children {
                let text = inspection_text(n)?; // Ordinary reader also requires UTF-8 pending decorations.
                if n.name == "pending.toml" { pending = true; }
                else {
                    let d: Decoration = toml::from_str(text).map_err(|e| format!("Invalid decoration metadata: {e}"))?;
                    d.validate()?;
                    if d.placement_id != id { return Err("Decoration placement identity/namespace mismatch".into()); }
                    active = Some(d);
                }
            }
            layout.incomplete_records += usize::from(pending || active.is_none());
            if let Some(d) = active { layout.decorations.push(d); }
            continue;
        }
        let Some(n) = children.iter().find(|n| n.name == "record.toml") else { continue; };
        let text = inspection_text(n)?;
        let record: Record = match toml::from_str(text) {
            Ok(r) => r,
            Err(e) => {
                let payload = children.iter().find(|n| n.name == "payload");
                // acquired_node already checked owned namespace/mode/links/ACL,
                // per-file bounds and binding. The token is from that same inode.
                if payload.is_some_and(|n| n.token.is_some()) {
                    layout.incomplete_records += 1;
                    continue;
                }
                return Err(format!("Invalid placement metadata: {e}"));
            }
        };
        let original = OsString::from_vec(record.name);
        if !matches!(record.version, 1 | 2) || record.token.len() != 16 || !valid_name(&original) || !valid_position(&record.position) {
            layout.warnings.push("Invalid placement schema/geometry; record preserved".into());
            continue;
        }
        let Some(identity) = record.identity.filter(|id| record.version == 2 && id[0] != 0 && id[2] < 1_000_000_000) else {
            layout.warnings.push("Legacy or invalid identity record; default placement used; metadata preserved".into());
            continue;
        };
        let matches: Vec<_> = candidates.iter().filter(|c| c.token == record.token && c.identity == identity).collect();
        if matches.len() > 1 || matches.iter().any(|c| c.links != 1) {
            layout.warnings.push("Ambiguous hard-linked placement identity; default placement used".into());
            continue;
        }
        if candidates.iter().any(|c| c.token == record.token && c.identity != identity) {
            layout.warnings.push("Copied placement attribute ignored on a different file identity".into());
        }
        let Some(candidate) = matches.first() else { continue; };
        // No reopening here: exact candidate acquisition was bracketed and is
        // compared again before returning, never lending a second read authority.
        if layout.positions.iter().any(|(n, _)| *n == candidate.name) { return Err("Conflicting active placement records".into()); }
        layout.positions.push((candidate.name.clone(), record.position));
        if record.retained_slot { layout.retained_slots.push(candidate.name.clone()); }
    }
    layout.incomplete_records += bind_closures(&mut layout.decorations, &closures)?;
    layout.decorations.sort_by(|a,b| a.placement_id.cmp(&b.placement_id));
    Ok(layout)
}
