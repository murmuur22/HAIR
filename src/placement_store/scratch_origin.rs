// Included in placement_store::mac. Scratch authority is independent of visited storage.
fn supported_origin_device(dev: u64, origin: Option<&FreshOrigin>) -> Result<bool> {
    match origin {
        Some(origin) if dev != origin.dev => Err(
            "Unsupported different-device placement origin: this process supports one device; restart HAIR for the intended device with private same-device scratch; no cross-device sampling attempted".into()),
        Some(_) => Ok(true),
        None => Ok(false),
    }
}
fn scratch_path(path: &OsStr) -> Result<CString> {
    let raw = path.as_bytes();
    // Remove ONLY terminal separators so O_NOFOLLOW sees the real leaf. Do not
    // canonicalize symlinks, collapse dot/dot-dot, or turn traversal into authority.
    let end = raw.iter().rposition(|b| *b != b'/').map_or(0, |i| i + 1);
    let leaf_path = &raw[..end];
    if !raw.starts_with(b"/") || end == 0
        || leaf_path.split(|b| *b == b'/').any(|p| p == b"." || p == b"..")
    {
        return Err("Open private provenance sampling directory: unsafe scratch path".into());
    }
    c(OsStr::from_bytes(leaf_path))
}
fn trusted_ancestor_shape(st: &libc::stat, euid: u32) -> bool {
    const SF_NOUNLINK: u32 = 0x0010_0000; // SDK sys/stat.h; not exported by pinned libc.
    let root_owned = st.st_uid == 0;
    st.st_mode & libc::S_IFMT == libc::S_IFDIR
        && (root_owned || st.st_uid == euid)
        && (st.st_mode & 0o022 == 0 || (root_owned && st.st_mode & libc::S_ISVTX != 0))
        // Observed macOS root/Users have SF_NOUNLINK; private may be UF_HIDDEN.
        // These do not grant writes. No other flags are silently accepted, and
        // system nounlink is accepted only on root-owned ancestors, never leaf.
        && st.st_flags & !(libc::UF_HIDDEN | if root_owned { SF_NOUNLINK } else { 0 }) == 0
}
unsafe extern "C" {
    fn acl_valid(acl: *mut libc::c_void) -> libc::c_int;
    fn acl_get_entry(acl: *mut libc::c_void, entry_id: libc::c_int, entry: *mut *mut libc::c_void) -> libc::c_int;
    fn acl_get_tag_type(entry: *mut libc::c_void, tag: *mut libc::c_int) -> libc::c_int;
}
fn restrictive_ancestor_acl(file: &File) -> Result<bool> {
    let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), 0x100) };
    if acl.is_null() {
        let e = std::io::Error::last_os_error();
        return if e.raw_os_error() == Some(libc::ENOENT) { Ok(true) }
            else { Err(format!("Inspect provenance scratch ancestor ACL: {e}")) };
    }
    let result = (|| {
        if unsafe { acl_valid(acl) } != 0 { return Err(error("Validate provenance scratch ancestor ACL")); }
        // SDK ACL_MAX_ENTRIES=128. Darwin returns 0 for an entry and -1/EINVAL
        // at end (not the POSIX/Linux 1/0 convention). Validate the ACL first.
        for index in 0..=128 {
            let mut entry = std::ptr::null_mut();
            if unsafe { acl_get_entry(acl, if index == 0 { 0 } else { -1 }, &mut entry) } != 0 {
                let e = std::io::Error::last_os_error();
                return if e.raw_os_error() == Some(libc::EINVAL) { Ok(true) }
                    else { Err(format!("Enumerate provenance scratch ancestor ACL: {e}")) };
            }
            if index == 128 || entry.is_null() { return Ok(false); }
            let mut tag = 0;
            if unsafe { acl_get_tag_type(entry, &mut tag) } != 0 {
                return Err(error("Inspect provenance scratch ancestor ACL tag"));
            }
            // Deny-only ACLs (including the standard home deny-delete entry)
            // cannot grant authority. Refuse every allow or unknown tag, even
            // read-only/inherit-only grants; never alter ACLs to obtain access.
            if tag != 2 { return Ok(false); } // SDK ACL_EXTENDED_DENY.
        }
        Ok(false)
    })();
    if unsafe { acl_free(acl) } != 0 { return Err(error("Release provenance scratch ancestor ACL")); }
    result
}
fn validate_scratch_ancestry(scratch: &File, visited: &std::fs::Metadata) -> Result<()> {
    let mut ancestor = open(scratch, OsStr::new("."), DIR)?;
    for _ in 0..256 {
        let here = ancestor.metadata().map_err(|e| e.to_string())?;
        if (here.dev(), here.ino()) == (visited.dev(), visited.ino()) {
            return Err("Provenance scratch overlaps visited directory; no probe created".into());
        }
        let st = stat_name(&ancestor, OsStr::new("."))?.ok_or("Provenance scratch ancestor disappeared")?;
        if !trusted_ancestor_shape(&st, unsafe { libc::geteuid() }) || !restrictive_ancestor_acl(&ancestor)? {
            return Err("Untrusted provenance scratch ancestor; no probe created; visited storage unchanged".into());
        }
        let parent = open(&ancestor, OsStr::new(".."), DIR)?;
        let above = parent.metadata().map_err(|e| e.to_string())?;
        if (here.dev(), here.ino()) == (above.dev(), above.ino()) { return Ok(()); }
        ancestor = parent;
    }
    Err("Provenance scratch ancestry limit; no probe created".into())
}
fn remove_owned_sample(scratch: &File, name: &OsStr, sample: &File) -> Result<()> {
    // No unlink after uncertain identity/ownership/shape/ACL inspection. This is
    // cooperative validation, not a hostile same-UID atomic namespace CAS.
    owned_metadata(scratch, name, sample, libc::S_IFREG, 0o600)?;
    let name = c(name)?;
    if unsafe { libc::unlinkat(scratch.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        return Err(error("Remove owned provenance sampling entry"));
    }
    Ok(())
}
fn sample_fresh_origin(
    scratch: &File,
    dev: u64,
    inspect: impl FnOnce(&File, &OsStr, &File) -> Result<Option<Vec<u8>>>,
) -> Result<FreshOrigin> {
    let mut id = [0u8; 16];
    if unsafe { libc::getentropy(id.as_mut_ptr().cast(), id.len()) } != 0 {
        return Err(error("Generate provenance sampling name"));
    }
    let name = OsString::from(format!("hair-origin-{}",
        id.iter().map(|b| format!("{b:02x}")).collect::<String>()));
    let sample = open(scratch, &name, CREATE)?;
    let result = (|| {
        owned_metadata(scratch, &name, &sample, libc::S_IFREG, 0o600)?;
        let origin = FreshOrigin {
            dev: sample.metadata().map_err(|e| e.to_string())?.dev(),
            value: inspect(scratch, &name, &sample)?,
        };
        if origin.dev != dev { return Err("Fresh provenance sample crossed device".into()); }
        Ok(origin)
    })();
    // Exactly one cleanup attempt on success AND error. A provenance read error
    // must not leak a verified owned probe on every repeated cold read refusal.
    match (result, remove_owned_sample(scratch, &name, &sample)) {
        (result, Ok(())) => result,
        (Ok(_), Err(cleanup)) => Err(format!("Provenance sample cleanup refused; entry preserved: {cleanup}")),
        (Err(cause), Err(cleanup)) => Err(format!("{cause}; provenance sample cleanup refused; entry preserved: {cleanup}")),
    }
}
fn ensure_fresh_origin(visited: &File) -> Result<()> {
    let visited = visited.metadata().map_err(|e| e.to_string())?;
    let dev = visited.dev();
    if supported_origin_device(dev, FRESH_ORIGIN.get())? { return Ok(()); }
    // Never create a probe in visited storage, even when scratch is unavailable.
    let path = scratch_path(std::env::temp_dir().as_os_str())?;
    let fd = unsafe { libc::open(path.as_ptr(), DIR) };
    if fd < 0 { return Err(error("Open private provenance sampling directory")); }
    let scratch = unsafe { File::from_raw_fd(fd) };
    let m = scratch.metadata().map_err(|e| e.to_string())?;
    let st = stat_name(&scratch, OsStr::new("."))?.ok_or("Provenance sampling directory disappeared")?;
    if m.dev() != dev || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o022 != 0 || st.st_flags != 0 || !no_extended_acl(&scratch)?
    {
        return Err("Private same-device provenance sampling unavailable; visited storage unchanged".into());
    }
    validate_scratch_ancestry(&scratch, &visited)?;
    let origin = sample_fresh_origin(&scratch, dev, |_, _, sample| provenance(sample))?;
    let _ = FRESH_ORIGIN.set(origin);
    // Another initializer may have won: do not report success on its device.
    supported_origin_device(dev, FRESH_ORIGIN.get())?;
    Ok(())
}
