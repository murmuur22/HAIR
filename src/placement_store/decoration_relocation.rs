// Included in placement_store::mac. Only owned metadata is atomically replaced;
// no filesystem-file token, owner-file operation, revision history or new slot.
include!("resize_held.rs");
pub(super) fn relocate_decoration(
    explorer: &Explorer,
    expected: &Decoration,
    proposed: &Decoration,
    validate: &mut dyn FnMut(&LayoutRead) -> Result<()>,
    barrier: impl FnMut(u8) -> Result<()>,
) -> Outcome {
    update_decoration(explorer, expected, proposed, false, validate, barrier)
}
pub(super) fn update_decoration(
    explorer: &Explorer, expected: &Decoration, proposed: &Decoration, resize: bool,
    validate: &mut dyn FnMut(&LayoutRead) -> Result<()>,
    barrier: impl FnMut(u8) -> Result<()>,
) -> Outcome {
    update_decoration_kind(explorer, expected, proposed, if resize { DecorationUpdate::Resize } else { DecorationUpdate::Relocate }, validate, barrier)
}
pub(super) fn update_decoration_kind(
    explorer: &Explorer, expected: &Decoration, proposed: &Decoration, kind: DecorationUpdate,
    validate: &mut dyn FnMut(&LayoutRead) -> Result<()>,
    mut barrier: impl FnMut(u8) -> Result<()>,
) -> Outcome {
    let mut prepared_resize = false;
    let result = (|| -> Result<Outcome> {
        kind.validate(expected, proposed)?;
        let text = toml::to_string(proposed).map_err(|e| e.to_string())?;
        if text.len() as u64 > RECORD_BYTES { return Err("Decoration record exceeds bound".into()); }
        barrier(0)?;
        let dir = explorer.placement_directory()?;
        // Relocation never creates a missing layout, lock or identity slot.
        let root = open(&dir, OsStr::new(ROOT), DIR)?;
        owned_metadata(&dir, OsStr::new(ROOT), &root, libc::S_IFDIR, 0o700)?;
        let lock = regular(&root, OsStr::new("lock"), REWRITE)?;
        owned_metadata(&root, OsStr::new("lock"), &lock, libc::S_IFREG, 0o600)?;
        let lock = HeldFlock::acquire(lock, libc::LOCK_EX)?;
        validate_decoration_storage(&dir, &root, &lock)?;
        let layout = read_locked(&dir, &root)?;
        let held_pin = ResizeHeldPin::acquire(&root, expected)?;
        let candidate = layout.updating_decoration_kind(expected, proposed, kind)?;
        if pending_bytes(&root, &entries(&root)?)? > crate::inventory::MAX_TOTAL as u64 {
            return Err("Layout staging budget reached".into());
        }
        let name = OsString::from(format!("dc-{}", expected.placement_id));
        let tx = open(&root, &name, DIR)?;
        owned_metadata(&root, &name, &tx, libc::S_IFDIR, 0o700)?;
        let old = regular(&tx, OsStr::new("decoration.toml"), READ)?;
        owned_metadata(&tx, OsStr::new("decoration.toml"), &old, libc::S_IFREG, 0o600)?;
        let old_text = decoration_file_text(&old)?;
        let actual: Decoration = toml::from_str(&old_text).map_err(|e| e.to_string())?;
        if actual != *expected { return Err("Stale expected decoration record".into()); }
        let old_origin = provenance(&old)?;
        validate(&candidate)?;
        let mut file = open(&tx, OsStr::new("pending.toml"), CREATE)?;
        remember_fresh_origin(&file)?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        prepared_resize = matches!(kind, DecorationUpdate::Resize | DecorationUpdate::Animation);
        if !sync(&tx) || !sync(&root) || !sync(&dir) {
            return Err("Could not sync inactive decoration relocation".into());
        }
        barrier(1)?;
        if let Some(pin) = &held_pin { pin.check(&root, expected)?; }
        // Exclude only our held, exclusive-created pending record. Any other
        // incomplete slot still refuses. The active old record remains visible.
        let mut live = read_locked(&dir, &root)?;
        owned_metadata(&tx, OsStr::new("pending.toml"), &file, libc::S_IFREG, 0o600)?;
        live.incomplete_records = live.incomplete_records.checked_sub(1)
            .ok_or("Prepared decoration relocation changed")?;
        let candidate = live.updating_decoration_kind(expected, proposed, kind)?;
        let mut listing = explorer.try_clone()?;
        listing.refresh()?;
        let before_final_listing = listing.current().clone();
        validate(&candidate)?;
        // Callbacks are not file authority. Recheck exact bytes and every held
        // name-to-inode binding after the final callback, including the source.
        validate_decoration_storage(&dir, &root, &lock)?;
        if let Some(pin) = &held_pin { pin.check(&root, expected)?; }
        if provenance(&old)? != old_origin { return Err("Expected decoration provenance changed".into()); }
        owned_metadata(&root, &name, &tx, libc::S_IFDIR, 0o700)?;
        owned_metadata(&tx, OsStr::new("decoration.toml"), &old, libc::S_IFREG, 0o600)?;
        owned_metadata(&tx, OsStr::new("pending.toml"), &file, libc::S_IFREG, 0o600)?;
        if decoration_file_text(&old)? != old_text { return Err("Expected decoration metadata changed".into()); }
        if decoration_file_text(&file)? != text { return Err("Prepared decoration metadata changed".into()); }
        let mut final_live = read_locked(&dir, &root)?;
        final_live.incomplete_records = final_live.incomplete_records.checked_sub(1)
            .ok_or("Prepared decoration relocation changed")?;
        if final_live != live { return Err("Layout changed during final relocation validation".into()); }
        listing.refresh()?;
        if listing.current() != &before_final_listing { return Err("External listing changed during final decoration validation".into()); }
        // Atomic replacement of the checked active metadata only. This is not
        // hostile same-UID namespace CAS; completed rebinding is detected above.
        let rc = unsafe { libc::renameatx_np(tx.as_raw_fd(), c"pending.toml".as_ptr(),
            tx.as_raw_fd(), c"decoration.toml".as_ptr(), 0) };
        if rc != 0 { return Ok(decoration_rename_failure(std::io::Error::last_os_error())); }
        let synced = barrier(2).is_ok() & sync(&file) & sync(&tx) & sync(&root) & sync(&dir);
        Ok(Outcome::Committed { synced })
    })();
    result.unwrap_or_else(|e| Outcome::NotCommitted(if prepared_resize && matches!(kind, DecorationUpdate::Animation) {
        format!("Pending animation retained; not resumable; recovery required. No animation change activated by this attempt: {e}")
    } else if prepared_resize {
        format!("Pending resize retained; not resumable; recovery required. No resize activated by this attempt: {e}")
    } else { e }))
}
