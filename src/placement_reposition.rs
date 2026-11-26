// Included inside placement_store::mac; same no-follow and cooperative storage primitives.
pub(super) fn select_existing(explorer: &Explorer, name: &OsStr) -> Result<Existing> {
    if !valid_name(name)
        || !explorer
            .current()
            .entries
            .iter()
            .any(|e| e.name == name && e.kind == crate::local_explorer::Kind::File)
    {
        return Err("Select a listed regular file; refresh if it changed".into());
    }
    let dir = explorer.placement_directory()?;
    let file = regular(&dir, name, READ)?;
    if file.metadata().map_err(|e| e.to_string())?.nlink() != 1 {
        return Err("Hard-linked files cannot be repositioned safely".into());
    }
    let target = Existing {
        name: name.into(),
        directory: explorer.map_identity()?,
        identity: persistent_identity(&file)?,
        token: strict_token(&file)?,
        file,
    };
    validate_existing(explorer, &target)?;
    Ok(target)
}
pub(super) fn validate_existing(explorer: &Explorer, target: &Existing) -> Result<()> {
    if explorer.map_identity()? != target.directory {
        return Err("Selected folder changed; reselect the file".into());
    }
    let dir = explorer.placement_directory()?;
    verify_named_identity(&dir, &target.name, &target.file)?;
    let live = regular(&dir, &target.name, READ)?;
    if persistent_identity(&live)? != target.identity
        || strict_token(&live)? != target.token
        || live.metadata().map_err(|e| e.to_string())?.nlink() != 1
    {
        return Err("Selected file identity or saved layout changed; refresh and reselect".into());
    }
    Ok(())
}
fn owned_metadata(
    parent: &File,
    name: &OsStr,
    held: &File,
    kind: libc::mode_t,
    mode: libc::mode_t,
) -> Result<()> {
    let st = stat_name(parent, name)?.ok_or("Layout storage disappeared")?;
    if !owned_shape(&st, kind, mode) || !no_extended_acl(held)? {
        return Err("Foreign/shared layout storage; no metadata changed".into());
    }
    verify_named_identity(parent, name, held)
}
pub(super) fn reposition(
    explorer: &Explorer,
    target: &Existing,
    position: &Position,
    validate: &mut dyn FnMut(&LayoutRead) -> Result<()>,
    mut barrier: impl FnMut(u8) -> Result<()>,
) -> Outcome {
    let result = (|| -> Result<Outcome> {
        position.validate()?;
        validate_existing(explorer, target)?;
        barrier(0)?;
        let dir = explorer.placement_directory()?;
        if missing(&dir, OsStr::new(ROOT)) {
            mkdir(&dir, OsStr::new(ROOT))?;
        }
        let root = open(&dir, OsStr::new(ROOT), DIR)?;
        owned_metadata(&dir, OsStr::new(ROOT), &root, libc::S_IFDIR, 0o700)?;
        let lock = layout_lock(&root)?;
        owned_metadata(&root, OsStr::new("lock"), &lock, libc::S_IFREG, 0o600)?;
        let lock = HeldFlock::acquire(lock, libc::LOCK_EX)?;
        let layout = read_locked(&dir, &root)?;

        if layout.incomplete_records != 0 || !layout.warnings.is_empty() {
            return Err("Layout needs recovery; records retained, no reposition".into());
        }
        validate(&layout)?;
        let names = entries(&root)?;
        if names.iter().filter(|n| *n != "lock").count() >= MAX_RECORDS {
            return Err("Layout storage limit reached; recovery required".into());
        }
        if pending_bytes(&root, &names)? > crate::inventory::MAX_TOTAL as u64 {
            return Err("Layout staging budget reached".into());
        }
        // Refuse foreign record/transaction shapes before writing. Never truncate them.
        for n in names.iter().filter(|n| *n != "lock") {
            if decoration_slot(n).is_some() {
                continue; // Inactive decoration slots were strictly read above.
            }
            let tx = open(&root, n, DIR)?;
            owned_metadata(&root, n, &tx, libc::S_IFDIR, 0o700)?;
            for child in entries_bounded(&tx, 2)? {
                if child != "record.toml" && child != "payload" {
                    return Err("Foreign layout transaction entry".into());
                }
                let file = regular(&tx, &child, READ)?;
                owned_metadata(&tx, &child, &file, libc::S_IFREG, 0o600)?;
            }
        }
        let mut id = [0u8; 16];
        if unsafe { libc::getentropy(id.as_mut_ptr().cast(), id.len()) } != 0 {
            return Err(error("Generate layout activation"));
        }
        let name = OsString::from(format!(
            "tx-{}",
            id.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ));
        mkdir(&root, &name)?;
        let tx = open(&root, &name, DIR)?;
        let record = Record {
            version: 2,
            name: target.name.as_bytes().to_vec(),
            token: id.to_vec(),
            identity: Some(target.identity),
            retained_slot: crate::placement_scene::retains_generated_slot(explorer, &layout, &target.name)?,
            position: position.clone(),
        };
        let text = toml::to_string(&record).map_err(|e| e.to_string())?;
        if text.len() as u64 > RECORD_BYTES {
            return Err("Layout record exceeds bound".into());
        }
        let mut file = open(&tx, OsStr::new("record.toml"), CREATE)?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        if !sync(&tx) || !sync(&root) || !sync(&dir) {
            return Err("Could not sync inactive reposition record".into());
        }
        barrier(1)?;
        // Fresh scene/route admission after preparation under the same lock.
        let live = read_locked(&dir, &root)?;
        if live.incomplete_records != 0 || !live.warnings.is_empty() {
            return Err("Layout changed; recovery required before reposition".into());
        }
        validate(&live)?;
        // Recheck exact prepared bytes and every name-to-inode binding after callbacks.
        validate_existing(explorer, target)?;
        owned_metadata(&dir, OsStr::new(ROOT), &root, libc::S_IFDIR, 0o700)?;
        owned_metadata(&root, OsStr::new("lock"), &lock, libc::S_IFREG, 0o600)?;
        owned_metadata(&root, &name, &tx, libc::S_IFDIR, 0o700)?;
        owned_metadata(&tx, OsStr::new("record.toml"), &file, libc::S_IFREG, 0o600)?;
        if read_record_text(&tx)? != text {
            return Err("Prepared reposition metadata changed; original retained".into());
        }
        // A fresh record cannot conflict with a live record until this token switch.
        // Old v2 records become dormant; no owner metadata is overwritten or migrated.
        let r = unsafe {
            libc::fsetxattr(
                target.file.as_raw_fd(),
                ATTR.as_ptr(),
                id.as_ptr().cast(),
                id.len(),
                0,
                if target.token.is_some() {
                    libc::XATTR_REPLACE
                } else {
                    libc::XATTR_CREATE
                },
            )
        };
        if r != 0 {
            let err = std::io::Error::last_os_error();
            return Ok(
                if matches!(
                    err.raw_os_error(),
                    Some(libc::EIO | libc::ETIMEDOUT | libc::ESTALE)
                ) {
                    Outcome::Ambiguous(format!("Activation uncertain: {err}; inspect before retry"))
                } else {
                    Outcome::NotCommitted(format!("Layout activation refused: {err}"))
                },
            );
        }
        // Once activated, a sync/verification error is never reported as rollback.
        let synced = strict_token(&target.file).ok() == Some(Some(id.to_vec()))
            && barrier(2).is_ok()
            && sync(&target.file)
            && sync(&tx)
            && sync(&root)
            && sync(&dir);
        Ok(Outcome::Committed { synced })
    })();
    result.unwrap_or_else(Outcome::NotCommitted)
}
