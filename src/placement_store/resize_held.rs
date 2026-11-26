// Included in mac. Hold the exact owned pin namespace, provenance and bytes
// across both locked stages, including byte-identical inode replacements.
struct ResizeHeldNode {
    file: File,
    stamp: [i128; 13],
    origin: Option<Vec<u8>>,
}
impl ResizeHeldNode {
    fn acquire(parent: &File, name: &OsStr, directory: bool) -> Result<Self> {
        let file = if directory { open(parent, name, DIR)? } else { regular(parent, name, READ)? };
        pin_owned(parent, name, &file, if directory { libc::S_IFDIR } else { libc::S_IFREG }, if directory { 0o700 } else { 0o600 })?;
        Ok(Self { stamp: inspection_stamp(&file)?, origin: provenance(&file)?, file })
    }
    fn check(&self, parent: &File, name: &OsStr, directory: bool) -> Result<()> {
        pin_owned(parent, name, &self.file, if directory { libc::S_IFDIR } else { libc::S_IFREG }, if directory { 0o700 } else { 0o600 })?;
        if inspection_stamp(&self.file)? != self.stamp || provenance(&self.file)? != self.origin {
            return Err("Held resize source changed".into());
        }
        Ok(())
    }
}
struct ResizeHeldPin { name: OsString, dir: ResizeHeldNode, descriptor: ResizeHeldNode, asset: ResizeHeldNode }
impl ResizeHeldPin {
    fn acquire(root: &File, d: &Decoration) -> Result<Option<Self>> {
        let Some(id) = d.closure_identity() else { return Ok(None); };
        let name = OsString::from(format!("ac-{id}"));
        let dir = ResizeHeldNode::acquire(root, &name, true)?;
        let descriptor = ResizeHeldNode::acquire(&dir.file, OsStr::new("closure.toml"), false)?;
        let asset = ResizeHeldNode::acquire(&dir.file, OsStr::new("asset.glb"), false)?;
        let held = Self { name, dir, descriptor, asset };
        held.check(root, d)?;
        Ok(Some(held))
    }
    fn check(&self, root: &File, d: &Decoration) -> Result<()> {
        self.dir.check(root, &self.name, true)?;
        self.descriptor.check(&self.dir.file, OsStr::new("closure.toml"), false)?;
        self.asset.check(&self.dir.file, OsStr::new("asset.glb"), false)?;
        let descriptor: Descriptor = toml::from_str(&decoration_file_text(&self.descriptor.file)?).map_err(|e|e.to_string())?;
        if Some(descriptor.identity()?.as_str()) != d.closure_identity() { return Err("Held resize closure identity changed".into()); }
        let bytes = pin_bytes(&self.asset.file, GLB_BYTES)?;
        let loaded = d.pinned.as_ref().and_then(|p|p.loaded.as_ref()).ok_or("Decoration pin not acquired")?;
        if bytes.as_slice() != loaded.encoded.as_ref() { return Err("Held resize pin bytes changed".into()); }
        Ok(())
    }
}
