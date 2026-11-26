// Included in placement_store::mac: reuse strict storage, flock and sync authority.
use super::pinned::{Budget, Descriptor, GLB_BYTES};
use std::sync::Arc;

pub(crate) struct PinSource {
    bindings: Vec<(File, OsString, File)>,
    asset: File,
    manifest: File,
    manifest_digest: String,
    bytes: Arc<[u8]>,
    asset_stamp: [i64; 4],
}
fn pin_stamp(file: &File) -> Result<[i64; 4]> {
    let m = file.metadata().map_err(|e| e.to_string())?;
    Ok([m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec()])
}
fn pin_source_file(file: &File) -> Result<()> {
    let m = file.metadata().map_err(|e| e.to_string())?;
    if !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.nlink() != 1
        || std::os::macos::fs::MetadataExt::st_flags(&m) != 0 || m.mode() & 0o7022 != 0 || !clean_attributes(file, false)? {
        return Err("Foreign/shared pin source metadata".into());
    }
    Ok(())
}
fn pin_bytes(mut file: &File, cap: u64) -> Result<Vec<u8>> {
    if file.metadata().map_err(|e| e.to_string())?.len() > cap { return Err("Pin bytes exceed bound".into()); }
    file.rewind().map_err(|e| e.to_string())?;
    let mut bytes = vec![];
    file.take(cap + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > cap { return Err("Pin bytes grew beyond bound".into()); }
    Ok(bytes)
}
impl PinSource {
    fn validate(&self) -> Result<()> {
        for (parent, name, held) in &self.bindings { verify_named_identity(parent, name, held)?; }
        pin_source_file(&self.asset)?;
        pin_source_file(&self.manifest)?;
        let stamp = pin_stamp(&self.asset)?;
        if stamp != self.asset_stamp {
            let m = self.asset.metadata().map_err(|e| e.to_string())?;
            return Err(format!("Stale selected pin source; reselect; asset stamp changed dev={} ino={} selected={:?} current={stamp:?}", m.dev(), m.ino(), self.asset_stamp));
        }
        if pin_bytes(&self.asset, GLB_BYTES)?.as_slice() != self.bytes.as_ref() {
            return Err("Stale selected pin source; reselect; asset bytes changed".into());
        }
        if super::pinned::digest(&pin_bytes(&self.manifest, 32768)?) != self.manifest_digest {
            return Err("Stale selected pin source; reselect; manifest bytes changed".into());
        }
        Ok(())
    }
}
fn pin_open_chain(base: &File, path: &std::path::Path, bindings: &mut Vec<(File, OsString, File)>, directory: bool) -> Result<File> {
    let parts: Vec<_> = path.components().collect();
    if parts.is_empty() || parts.len() > 64 { return Err("Unsafe pin source chain".into()); }
    let mut parent = base.try_clone().map_err(|e| e.to_string())?;
    for (i, part) in parts.iter().enumerate() {
        let std::path::Component::Normal(name) = part else { return Err("Unsafe pin source component".into()); };
        let held = if directory || i + 1 < parts.len() { open(&parent, name, DIR)? } else { regular(&parent, name, READ)? };
        verify_named_identity(&parent, name, &held)?;
        bindings.push((parent, name.to_os_string(), held.try_clone().map_err(|e| e.to_string())?));
        parent = held;
    }
    Ok(parent)
}
pub(super) fn select_pin_asset(theme: &crate::theme::Theme, entry_id: &str) -> Result<CheckedPin> {
    let entry = theme.decorations.entry(entry_id).ok_or("Select an exact catalog entry")?.clone();
    let asset = theme.decorations.asset(&entry.asset).ok_or("Selected catalog asset missing")?;
    asset.spec.validate_fit()?;
    let relative = &asset.spec.file;
    if relative.is_empty() || relative.contains(['\\', ':']) || relative.split('/').count() > 8
        || !relative.ends_with(".glb") || relative.split('/').any(|p| p.is_empty() || p == "." || p == "..") {
        return Err("Unsafe pin source relative path".into());
    }
    let absolute = if theme.source.is_absolute() { theme.source.clone() } else {
        std::env::current_dir().map_err(|e| e.to_string())?.join(&theme.source)
    };
    let root_path = absolute.parent().ok_or("Pin pack parent missing")?;
    let slash = File::open("/").map_err(|e| e.to_string())?;
    let mut bindings = vec![];
    let root = pin_open_chain(&slash, root_path.strip_prefix("/").map_err(|e| e.to_string())?, &mut bindings, true)?;
    ensure_fresh_origin(&root)?;
    let manifest = pin_open_chain(&root, std::path::Path::new(absolute.file_name().ok_or("Pin manifest missing")?), &mut bindings, false)?;
    let file = pin_open_chain(&root, std::path::Path::new(relative), &mut bindings, false)?;
    pin_source_file(&file)?;
    pin_source_file(&manifest)?;
    // Existing safe acquisition is still the byte authority; held descriptors add
    // source identity, no-follow binding and stale-selection revalidation.
    let bytes = crate::content::read_local(root_path, relative, GLB_BYTES)?;
    if bytes.as_slice() != asset.encoded.as_ref() { return Err("Catalog bytes changed; reload and reselect".into()); }
    let descriptor = Descriptor::new(theme.manifest_digest.clone(), asset.spec.clone(), &bytes)?;
    let source = PinSource { bindings, asset_stamp: pin_stamp(&file)?, asset: file, manifest,
        manifest_digest: theme.manifest_digest.clone(), bytes: asset.encoded.clone() };
    source.validate()?;
    // Public catalog fields are UI data, not a receipt. Bind the selected entry
    // and complete fit/asset identity back to the exact safely loaded manifest.
    let manifest_bytes = pin_bytes(&source.manifest, 32768)?;
    let manifest_value: toml::Value = toml::from_str(std::str::from_utf8(&manifest_bytes).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let spec: crate::decoration::Spec = manifest_value.get("decorations").ok_or("Source catalog missing")?
        .clone().try_into().map_err(|e| format!("Source catalog: {e}"))?;
    spec.validate()?;
    if !spec.entries.contains(&entry) || !spec.assets.contains(&asset.spec) {
        return Err("Selected catalog identity/fit changed; reload and reselect".into());
    }
    // Decode again through the authentic safe decoder, never trust public model fields.
    let loaded = descriptor.load(asset.encoded.clone())?;
    Ok(CheckedPin { descriptor, entry, loaded, source })
}
fn closure_slot(name: &OsStr) -> Option<&str> {
    let id = name.to_str()?.strip_prefix("ac-")?;
    super::pinned::valid_digest(id).then_some(id)
}
struct Closures {
    assets: std::collections::BTreeMap<String, (Descriptor, Arc<crate::decoration::LoadedAsset>)>,
    incomplete: usize,
    bytes: u64,
}
fn pin_owned(parent: &File, name: &OsStr, held: &File, kind: libc::mode_t, mode: libc::mode_t) -> Result<()> {
    owned_metadata(parent, name, held, kind, mode)?;
    if !clean_attributes(held, false)? { return Err("Foreign pin storage attributes/provenance".into()); }
    Ok(())
}
fn read_closures(dir: &File, root: &File, names: &[OsString]) -> Result<Closures> {
    let mut out = Closures { assets: Default::default(), incomplete: 0, bytes: 0 };
    let count = names.iter().filter(|n| n.as_bytes().starts_with(b"ac-")).count();
    if count == 0 { return Ok(out); }
    if count > super::pinned::MAX_PIN_CLOSURES { return Err("Pinned closure model/file limit reached (3, including orphan/incomplete slots)".into()); }
    ensure_fresh_origin(dir)?;
    pin_owned(dir, OsStr::new(ROOT), root, libc::S_IFDIR, 0o700)?;
    let lock = regular(root, OsStr::new("lock"), READ)?;
    pin_owned(root, OsStr::new("lock"), &lock, libc::S_IFREG, 0o600)?;
    let mut budget = Budget::default();
    for name in names.iter().filter(|n| n.as_bytes().starts_with(b"ac-")) {
        let id = closure_slot(name).ok_or("Invalid closure namespace")?;
        let tx = open(root, name, DIR)?;
        pin_owned(root, name, &tx, libc::S_IFDIR, 0o700)?;
        let children = entries_bounded(&tx, 2)?;
        // Only this pre-publication shape may carry arbitrary interrupted bytes.
        // Never downgrade a malformed final descriptor (or mixed state) to pending.
        if children.iter().any(|n| n == "closure.pending.toml")
            && (children.iter().any(|n| n == "closure.toml") || !children.iter().any(|n| n == "asset.glb")) {
            return Err("Invalid inactive pinned descriptor shape".into());
        }
        let mut descriptor = None;
        let mut bytes = None;
        for child in &children {
            if child != "closure.toml" && child != "closure.pending.toml" && child != "asset.glb" { return Err("Foreign pinned closure child".into()); }
            let file = regular(&tx, child, READ)?;
            pin_owned(&tx, child, &file, libc::S_IFREG, 0o600)?;
            if child == "closure.pending.toml" {
                pin_bytes(&file, RECORD_BYTES)?; // Bounded opaque evidence; no parse, decode, repair or reuse.
            } else if child == "closure.toml" {
                let text = decoration_file_text(&file)?;
                let d: Descriptor = toml::from_str(&text).map_err(|e| format!("Malformed pinned descriptor: {e}"))?;
                d.validate()?;
                if d.identity()? != id { return Err("Pinned descriptor/namespace mismatch".into()); }
                descriptor = Some(d);
            } else {
                let raw = pin_bytes(&file, GLB_BYTES)?;
                out.bytes = out.bytes.checked_add(raw.len() as u64).ok_or("Closure bytes overflow")?;
                if out.bytes > 24_000_000 { return Err("Pinned aggregate encoded budget".into()); }
                bytes = Some(Arc::<[u8]>::from(raw));
            }
            pin_owned(&tx, child, &file, libc::S_IFREG, 0o600)?;
        }
        match (descriptor, bytes) {
            (Some(d), Some(bytes)) => {
                let loaded = d.load(bytes)?;
                budget.charge_pin(&loaded)?;
                out.assets.insert(id.to_string(), (d, loaded));
            }
            _ => out.incomplete += 1,
        }
        pin_owned(root, name, &tx, libc::S_IFDIR, 0o700)?;
    }
    Ok(out)
}
fn bind_closures(decorations: &mut [Decoration], closures: &Closures) -> Result<usize> {
    let mut used = std::collections::BTreeSet::new();
    for d in decorations {
        if let Some(pin) = &mut d.pinned {
            let (_, asset) = closures.assets.get(&pin.closure).ok_or("Pinned record closure missing/incomplete; no fallback")?;
            if asset.spec.id != pin.entry.asset { return Err("Pinned record asset identity mismatch".into()); }
            pin.loaded = Some(asset.clone());
            used.insert(pin.closure.clone());
        }
    }
    Ok(closures.incomplete + closures.assets.len() - used.len())
}
fn pin_candidate(layout: &LayoutRead, record: &Decoration) -> Result<LayoutRead> {
    if layout.incomplete_records != 0 || !layout.warnings.is_empty() { return Err("Layout needs recovery; pin refused".into()); }
    if layout.decorations.iter().any(|d| d.placement_id == record.placement_id) { return Err("Pin placement identity already exists".into()); }
    let mut candidate = layout.clone();
    candidate.decorations.push(record.clone());
    candidate.decorations.sort_by(|a, b| a.placement_id.cmp(&b.placement_id));
    Ok(candidate)
}
pub(super) fn publish_pin(
    explorer: &Explorer, source: &CheckedPin, placement: &Decoration,
    validate: &mut dyn FnMut(&LayoutRead) -> Result<()>, mut barrier: impl FnMut(u8) -> Result<()>,
) -> Outcome {
    let result = (|| -> Result<Outcome> {
        source.source.validate()?;
        let record = placement.pin(source)?;
        let text = toml::to_string(&record).map_err(|e| e.to_string())?;
        let descriptor_text = source.descriptor.text()?;
        if text.len() as u64 > RECORD_BYTES || descriptor_text.len() as u64 > RECORD_BYTES { return Err("Pinned metadata exceeds record bound".into()); }
        barrier(0)?;
        let dir = explorer.placement_directory()?;
        if missing(&dir, OsStr::new(ROOT)) { mkdir(&dir, OsStr::new(ROOT))?; }
        let root = open(&dir, OsStr::new(ROOT), DIR)?;
        owned_metadata(&dir, OsStr::new(ROOT), &root, libc::S_IFDIR, 0o700)?;
        let lock = layout_lock(&root)?;
        let lock = HeldFlock::acquire(lock, libc::LOCK_EX)?;
        validate_decoration_storage(&dir, &root, &lock)?;
        let initial = read_locked(&dir, &root)?;
        // Read-only exact reuse is not write authority. Hold any existing closure
        // before the first callback, including an unreferenced owned closure.
        let reuse_name = OsString::from(format!("ac-{}", source.closure_identity()?));
        let reused = if missing(&root, &reuse_name) { None } else { ResizeHeldPin::acquire(&root, &record)? };
        let candidate = pin_candidate(&initial, &record)?;
        validate(&candidate)?;
        if let Some(held) = &reused { held.check(&root, &record)?; }
        source.source.validate()?;
        let names = entries(&root)?;
        let closures = read_closures(&dir, &root, &names)?;
        let identity = source.closure_identity()?;
        let closure_name = OsString::from(format!("ac-{identity}"));
        let fresh = !closures.assets.contains_key(&identity);
        if names.iter().filter(|n| *n != "lock").count() + 1 + usize::from(fresh) > MAX_RECORDS {
            return Err("Layout storage limit reached (256 shared slots)".into());
        }
        let mut budget = Budget::default();
        for (_, a) in closures.assets.values() { budget.charge_pin(a)?; }
        if fresh { budget.charge_pin(&source.loaded)?; }
        if pending_bytes(&root, &names)?.checked_add(if fresh { source.descriptor.bytes } else { 0 })
            .ok_or("Storage byte overflow")? > crate::inventory::MAX_TOTAL as u64 { return Err("Layout staging/closure budget reached".into()); }
        if !fresh {
            let (d, a) = &closures.assets[&identity];
            if *d != source.descriptor || a.encoded != source.loaded.encoded { return Err("Pinned closure exact reuse mismatch".into()); }
        } else { mkdir(&root, &closure_name)?; }
        let closure_dir = open(&root, &closure_name, DIR)?;
        let (held_asset, held_descriptor) = if fresh {
            let mut asset = open(&closure_dir, OsStr::new("asset.glb"), CREATE)?;
            remember_fresh_origin(&asset)?;
            #[cfg(test)]
            let encoded = pin_preparation_prefix(&mut asset, &source.loaded.encoded, &mut barrier, 8, 9)?;
            #[cfg(not(test))]
            let encoded = source.loaded.encoded.as_ref();
            asset.write_all(encoded).map_err(|e| e.to_string())?;
            asset.sync_all().map_err(|e| e.to_string())?;
            // Persist asset.glb's directory entry (and the fresh closure slot)
            // BEFORE a pending descriptor can itself become durable. The later
            // prepared-descriptor sync cannot repair this preparation ordering.
            pin_sync_asset_entries(&mut barrier, || sync(&closure_dir), || sync(&root))?;
            barrier(3)?;
            let mut descriptor = open(&closure_dir, OsStr::new("closure.pending.toml"), CREATE)?;
            #[cfg(test)]
            let remaining = pin_preparation_prefix(&mut descriptor, descriptor_text.as_bytes(), &mut barrier, 4, 5)?;
            #[cfg(not(test))]
            let remaining = descriptor_text.as_bytes();
            descriptor.write_all(remaining).map_err(|e| e.to_string())?;
            descriptor.sync_all().map_err(|e| e.to_string())?;
            if !sync(&closure_dir) || !sync(&root) || !sync(&dir) { return Err("Could not sync prepared pin descriptor".into()); }
            barrier(6)?;
            // The held source and prepared inodes, not a reopened same-byte name,
            // are authority. Callbacks must not rebind or alter publication bytes.
            source.source.validate()?;
            pin_owned(&dir, OsStr::new(ROOT), &root, libc::S_IFDIR, 0o700)?;
            pin_owned(&root, OsStr::new("lock"), &lock, libc::S_IFREG, 0o600)?;
            pin_owned(&root, &closure_name, &closure_dir, libc::S_IFDIR, 0o700)?;
            pin_owned(&closure_dir, OsStr::new("asset.glb"), &asset, libc::S_IFREG, 0o600)?;
            pin_owned(&closure_dir, OsStr::new("closure.pending.toml"), &descriptor, libc::S_IFREG, 0o600)?;
            let mut children = entries_bounded(&closure_dir, 2)?;
            children.sort();
            if children != vec![OsString::from("asset.glb"), OsString::from("closure.pending.toml")]
                || pin_bytes(&asset, GLB_BYTES)?.as_slice() != source.loaded.encoded.as_ref()
                || pin_bytes(&descriptor, RECORD_BYTES)? != descriptor_text.as_bytes() {
                return Err("Prepared pin descriptor/asset changed".into());
            }
            let rc = unsafe { libc::renameatx_np(closure_dir.as_raw_fd(), c"closure.pending.toml".as_ptr(), closure_dir.as_raw_fd(), c"closure.toml".as_ptr(), libc::RENAME_EXCL) };
            // No placement has been activated, even if this metadata rename is
            // uncertain. Preserve all evidence and never attempt pin activation.
            if rc != 0 { return Err(error("Pin descriptor publication failed; pin not activated")); }
            barrier(7)?;
            (asset, descriptor)
        } else {
            (regular(&closure_dir, OsStr::new("asset.glb"), READ)?, regular(&closure_dir, OsStr::new("closure.toml"), READ)?)
        };
        if !sync(&closure_dir) || !sync(&root) || !sync(&dir) { return Err("Could not sync owned pin closure".into()); }
        let name = OsString::from(format!("dc-{}", record.placement_id));
        mkdir(&root, &name)?;
        let tx = open(&root, &name, DIR)?;
        let mut pending = open(&tx, OsStr::new("pending.toml"), CREATE)?;
        pending.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        pending.sync_all().map_err(|e| e.to_string())?;
        if !sync(&tx) || !sync(&root) || !sync(&dir) { return Err("Could not sync inactive pinned record".into()); }
        barrier(1)?;
        // Establish our exact inactive evidence BEFORE adjusting diagnostic counts.
        let inspect = || -> Result<LayoutRead> {
            if let Some(held) = &reused { held.check(&root, &record)?; }
            pin_owned(&dir, OsStr::new(ROOT), &root, libc::S_IFDIR, 0o700)?;
            pin_owned(&root, OsStr::new("lock"), &lock, libc::S_IFREG, 0o600)?;
            pin_owned(&root, &closure_name, &closure_dir, libc::S_IFDIR, 0o700)?;
            pin_owned(&closure_dir, OsStr::new("asset.glb"), &held_asset, libc::S_IFREG, 0o600)?;
            pin_owned(&closure_dir, OsStr::new("closure.toml"), &held_descriptor, libc::S_IFREG, 0o600)?;
            pin_owned(&root, &name, &tx, libc::S_IFDIR, 0o700)?;
            pin_owned(&tx, OsStr::new("pending.toml"), &pending, libc::S_IFREG, 0o600)?;
            if entries_bounded(&tx, 1)? != vec![OsString::from("pending.toml")]
                || decoration_file_text(&pending)? != text { return Err("Prepared pinned record changed".into()); }
            let exact = read_closures(&dir, &root, &entries(&root)?)?;
            let (d, a) = exact.assets.get(&identity).ok_or("Prepared closure missing")?;
            if *d != source.descriptor || a.encoded != source.loaded.encoded { return Err("Prepared closure changed".into()); }
            let mut live = read_locked(&dir, &root)?;
            live.incomplete_records = live.incomplete_records.checked_sub(1 + usize::from(fresh))
                .ok_or("Prepared pin diagnostics changed")?;
            if live != initial { return Err("Layout changed during pin publication".into()); }
            Ok(live)
        };
        let live = inspect()?;
        validate(&pin_candidate(&live, &record)?)?;
        source.source.validate()?;
        validate_decoration_storage(&dir, &root, &lock)?;
        inspect()?; // callbacks are not filesystem authority
        let rc = unsafe { libc::renameatx_np(tx.as_raw_fd(), c"pending.toml".as_ptr(), tx.as_raw_fd(), c"decoration.toml".as_ptr(), libc::RENAME_EXCL) };
        if rc != 0 { return Ok(decoration_rename_failure(std::io::Error::last_os_error())); }
        let synced = barrier(2).is_ok() & sync(&pending) & sync(&tx) & sync(&closure_dir) & sync(&root) & sync(&dir);
        Ok(Outcome::Committed { synced })
    })();
    result.unwrap_or_else(Outcome::NotCommitted)
}

// Deterministic exact-prefix crash seams; production still uses one write_all.
pub(super) fn pin_sync_asset_entries(
    barrier: &mut dyn FnMut(u8) -> Result<()>,
    mut closure: impl FnMut() -> bool,
    mut root: impl FnMut() -> bool,
) -> Result<()> {
    // Separate fault boundaries; preserve all existing descriptor crash stages.
    #[cfg(test)]
    barrier(10)?;
    if !closure() { return Err("Could not sync pin asset directory entry".into()); }
    #[cfg(test)]
    barrier(11)?;
    if !root() { return Err("Could not sync fresh pin closure directory entry".into()); }
    let _ = barrier;
    Ok(())
}
#[cfg(test)]
fn pin_preparation_prefix<'a>(file: &mut File, bytes: &'a [u8], barrier: &mut dyn FnMut(u8) -> Result<()>, empty: u8, partial: u8) -> Result<&'a [u8]> {
    barrier(empty)?;
    let split = bytes.len().min(11);
    file.write_all(&bytes[..split]).map_err(|e| e.to_string())?;
    barrier(partial)?;
    Ok(&bytes[split..])
}
