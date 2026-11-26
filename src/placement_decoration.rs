// Included in placement_store::mac: same held-directory and nonblocking flock.
fn decoration_slot(name: &OsStr) -> Option<&str> {
    let text = name.to_str()?;
    let id = text.strip_prefix("dc-")?;
    super::decoration_record::valid_id(id).then_some(id)
}
fn decoration_text(tx: &File, name: &OsStr) -> Result<String> {
    let file = regular(tx, name, READ)?;
    owned_metadata(tx, name, &file, libc::S_IFREG, 0o600)?;
    decoration_file_text(&file)
}
fn decoration_file_text(mut file: &File) -> Result<String> {
    if !clean_attributes(file, false)? || file.metadata().map_err(|e| e.to_string())?.len() > RECORD_BYTES {
        return Err("Foreign or oversized decoration metadata".into());
    }
    file.rewind().map_err(|e| e.to_string())?;
    let mut text = String::new();
    file.take(RECORD_BYTES + 1).read_to_string(&mut text).map_err(|e| e.to_string())?;
    if text.len() as u64 > RECORD_BYTES { return Err("Decoration metadata grew beyond bound".into()); }
    Ok(text)
}
fn read_decoration_slot(dir: &File, root: &File, name: &OsStr) -> Result<(Option<Decoration>, bool)> {
    let id = decoration_slot(name).ok_or("Invalid decoration namespace")?;
    let tx = open(root, name, DIR)?;
    owned_metadata(root, name, &tx, libc::S_IFDIR, 0o700)?;
    let children = entries_bounded(&tx, 2)?;
    if children.is_empty() { return Ok((None, true)); }
    ensure_fresh_origin(dir)?;
    let mut active = None;
    let mut pending = false;
    for child in children {
        if child != "pending.toml" && child != "decoration.toml" {
            return Err("Foreign decoration storage child".into());
        }
        let text = decoration_text(&tx, &child)?;
        if child == "pending.toml" { pending = true; } // Never repair/reuse, even after a clean refusal.
        else { active = Some(text); }
    }
    let Some(text) = active else { return Ok((None, true)); };
    let record: Decoration = toml::from_str(&text).map_err(|e| format!("Invalid decoration metadata: {e}"))?;
    record.validate()?;
    if record.placement_id != id { return Err("Decoration placement identity/namespace mismatch".into()); }
    Ok((Some(record), pending))
}
fn validate_decoration_storage(dir: &File, root: &File, lock: &File) -> Result<()> {
    owned_metadata(dir, OsStr::new(ROOT), root, libc::S_IFDIR, 0o700)?;
    owned_metadata(root, OsStr::new("lock"), lock, libc::S_IFREG, 0o600)?;
    // Refuse foreign legacy children too; do not repair, reclaim or migrate them.
    for name in entries(root)?.iter().filter(|n| *n != "lock") {
        if closure_slot(name).is_some() { continue; } // read_locked validates the complete bounded inventory.
        if decoration_slot(name).is_some() { read_decoration_slot(dir, root, name)?; continue; }
        let raw = name.as_bytes();
        if raw.len() != 35 || !raw.starts_with(b"tx-") || !raw[3..].iter().all(u8::is_ascii_hexdigit) {
            return Err("Foreign layout namespace".into());
        }
        let tx = open(root, name, DIR)?;
        owned_metadata(root, name, &tx, libc::S_IFDIR, 0o700)?;
        for child in entries_bounded(&tx, 2)? {
            if child != "record.toml" && child != "payload" { return Err("Foreign layout transaction child".into()); }
            let file = regular(&tx, &child, READ)?;
            owned_metadata(&tx, &child, &file, libc::S_IFREG, 0o600)?;
        }
    }
    Ok(())
}
pub(super) fn place_decoration(
    explorer: &Explorer,
    decoration: &Decoration,
    validate: &mut dyn FnMut(&LayoutRead) -> Result<()>,
    mut barrier: impl FnMut(u8) -> Result<()>,
) -> Outcome {
    let result = (|| -> Result<Outcome> {
        decoration.validate()?;
        if decoration.animation.is_some() { return Err("New decorations require absent animation; saved legacy modes are read-only".into()); }
        if decoration.is_pinned() { return Err("Pinned publication requires explicit checked asset source".into()); }
        let text = toml::to_string(decoration).map_err(|e| e.to_string())?;
        if text.len() as u64 > RECORD_BYTES { return Err("Decoration record exceeds bound".into()); }
        barrier(0)?;
        let dir = explorer.placement_directory()?;
        if missing(&dir, OsStr::new(ROOT)) { mkdir(&dir, OsStr::new(ROOT))?; }
        let root = open(&dir, OsStr::new(ROOT), DIR)?;
        owned_metadata(&dir, OsStr::new(ROOT), &root, libc::S_IFDIR, 0o700)?;
        let lock = layout_lock(&root)?;
        owned_metadata(&root, OsStr::new("lock"), &lock, libc::S_IFREG, 0o600)?;
        let lock = HeldFlock::acquire(lock, libc::LOCK_EX)?;
        validate_decoration_storage(&dir, &root, &lock)?;
        let layout = read_locked(&dir, &root)?;
        if layout.incomplete_records != 0 || !layout.warnings.is_empty() { return Err("Layout needs recovery; no decoration activated".into()); }
        validate(&layout)?;
        let names = entries(&root)?;
        if names.iter().filter(|n| *n != "lock").count() >= MAX_RECORDS { return Err("Layout storage limit reached (256 shared slots)".into()); }
        if pending_bytes(&root, &names)? > crate::inventory::MAX_TOTAL as u64 { return Err("Layout staging budget reached".into()); }
        let name = OsString::from(format!("dc-{}", decoration.placement_id));
        mkdir(&root, &name)?; // Exclusive: never overwrite an existing identity/attempt.
        let tx = open(&root, &name, DIR)?;
        let mut file = open(&tx, OsStr::new("pending.toml"), CREATE)?;
        remember_fresh_origin(&file)?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        if !sync(&tx) || !sync(&root) || !sync(&dir) { return Err("Could not sync inactive decoration metadata".into()); }
        barrier(1)?;
        // Final fresh geometry revalidation while holding the SAME exclusive lock.
        // Exclude exactly this owned inactive attempt from the diagnostics count.
        let mut live = read_locked(&dir, &root)?;
        live.incomplete_records = live.incomplete_records.checked_sub(1).ok_or("Prepared decoration slot changed")?;
        if live.incomplete_records != 0 || !live.warnings.is_empty() { return Err("Layout changed; recovery required".into()); }
        validate(&live)?;
        validate_decoration_storage(&dir, &root, &lock)?;
        owned_metadata(&root, &name, &tx, libc::S_IFDIR, 0o700)?;
        owned_metadata(&tx, OsStr::new("pending.toml"), &file, libc::S_IFREG, 0o600)?;
        if decoration_file_text(&file)? != text { return Err("Prepared decoration metadata changed".into()); }
        verify_named_identity(&tx, OsStr::new("pending.toml"), &file)?;
        let rc = unsafe { libc::renameatx_np(tx.as_raw_fd(), c"pending.toml".as_ptr(), tx.as_raw_fd(), c"decoration.toml".as_ptr(), libc::RENAME_EXCL) };
        if rc != 0 { return Ok(decoration_rename_failure(std::io::Error::last_os_error())); }
        // Rename is activation. No postcommit failure may masquerade as rollback.
        let synced = barrier(2).is_ok() & sync(&file) & sync(&tx) & sync(&root) & sync(&dir);
        Ok(Outcome::Committed { synced })
    })();
    result.unwrap_or_else(Outcome::NotCommitted)
}
pub(super) fn decoration_rename_failure(e: std::io::Error) -> Outcome {
    // No cleanup/truncation: preserve metadata even for definite failures.
    match e.raw_os_error() {
        Some(libc::EACCES | libc::EPERM | libc::EEXIST | libc::ENOSPC | libc::EDQUOT | libc::EROFS | libc::ENOENT | libc::EXDEV | libc::EINVAL | libc::ENOTDIR | libc::EISDIR | libc::ENOTEMPTY | libc::ELOOP | libc::ENAMETOOLONG | libc::EBADF | libc::ENOTSUP) => Outcome::NotCommitted(format!("Decoration activation refused: {e}")),
        _ => Outcome::Ambiguous(format!("Decoration activation uncertain: {e}; inspect before retry")),
    }
}
#[cfg(test)]
mod decoration_tests {
    use super::*;
    include!("placement_decoration_tests.rs");
    include!("placement_store/scratch_origin_tests.rs");
}
