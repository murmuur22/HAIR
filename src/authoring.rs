//! Bounded data-only authoring; originals are held as bytes, never edited.
use crate::{Result, assets::Model, content::read_local, theme::Theme};
use serde::{Deserialize, Serialize};
use std::os::{
    fd::{AsRawFd, FromRawFd},
    unix::fs::{MetadataExt, OpenOptionsExt},
};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub struct Project {
    source: PathBuf,
    bytes: Vec<u8>,
    pub model: Model,
    pub at: [f32; 3],
    pub yaw_degrees: f32,
    pub scale: f32,
    pub solid: bool,
    pub document_role: bool,
    pub door_active: bool,
    retained: BTreeMap<u8, RetainedAsset>,
    surfaces: BTreeMap<String, SurfaceAsset>,
}
#[derive(Clone)]
struct SurfaceAsset {
    edit: crate::materials::Surface,
    texture: Option<(PathBuf, Vec<u8>)>,
}
struct RetainedAsset {
    source: PathBuf,
    bytes: Vec<u8>,
    model: Model,
    edit: Edit,
    door: bool,
}
impl RetainedAsset {
    fn copy(p: &Project) -> Self {
        Self {
            source: p.source.clone(),
            bytes: p.bytes.clone(),
            edit: p.edit(),
            door: p.door_active,
            model: Model {
                vertices: p.model.vertices.clone(),
                textures: p.model.textures.clone(),
                min: p.model.min,
                max: p.model.max,
            },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    #[serde(rename = "pivot")]
    pub at: [f32; 3],
    pub yaw_degrees: f32,
    pub scale: f32,
    #[serde(skip)]
    pub solid: bool,
    pub document_role: bool,
}
impl Edit {
    fn validate(&self) -> Result<()> {
        if self.solid
            || self.at.iter().any(|v| !v.is_finite() || v.abs() > 3.0)
            || !self.scale.is_finite()
            || !(0.1..=4.0).contains(&self.scale)
            || !self.yaw_degrees.is_finite()
            || !(-180.0..=180.0).contains(&self.yaw_degrees)
        {
            return Err(
                "Use asset-local pivot -3..3, uniform scale 0.1..4, rotation -180..180 degrees; solid collision is host-owned and cannot be exported".into(),
            );
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedProject {
    version: u32,
    asset: String,
    original_source: String,
    edit: Edit,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    door: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WholeProject {
    version: u32,
    active_document: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    active_door: bool,
    assets: Vec<SavedProject>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    surfaces: Vec<crate::materials::Surface>,
}
#[derive(Serialize)]
struct Provenance {
    source: String,
    license: &'static str,
    distribution: &'static str,
}
impl Project {
    /// Empty authoring stage; host placeholders remain until model roles are imported.
    pub fn blank() -> Self {
        Self {
            source: PathBuf::new(),
            bytes: Vec::new(),
            model: Model {
                vertices: Vec::new(),
                textures: Vec::new(),
                min: [0.0; 3],
                max: [0.0; 3],
            },
            at: [0.0; 3],
            yaw_degrees: 0.0,
            scale: 1.0,
            solid: false,
            document_role: true,
            door_active: false,
            retained: BTreeMap::new(),
            surfaces: BTreeMap::new(),
        }
    }
    pub fn clear_surface_texture(&mut self, surface: &str) -> Result<Theme> {
        if !["wall", "floor"].contains(&surface) {
            return Err("wall/floor required".into());
        }
        let old = self.surfaces.clone();
        if let Some(a) = self.surfaces.get_mut(surface) {
            a.texture = None;
            a.edit.file = None;
        }
        match self.preview() {
            Ok(t) => Ok(t),
            Err(e) => {
                self.surfaces = old;
                Err(e)
            }
        }
    }
    pub fn surfaces(&self) -> Vec<crate::materials::Surface> {
        self.surfaces.values().map(|s| s.edit.clone()).collect()
    }
    pub fn set_surface(&mut self, edit: crate::materials::Surface) -> Result<Theme> {
        crate::materials::validate(std::slice::from_ref(&edit))?;
        let old = self.surfaces.clone();
        let texture = old.get(&edit.surface).and_then(|a| a.texture.clone());
        if edit.file != texture.as_ref().map(|_| format!("{}.png", edit.surface)) {
            return Err("Use Import PNG / Clear texture, not an arbitrary texture path".into());
        }
        self.surfaces
            .insert(edit.surface.clone(), SurfaceAsset { edit, texture });
        match self.preview() {
            Ok(t) => Ok(t),
            Err(e) => {
                self.surfaces = old;
                Err(e)
            }
        }
    }
    pub fn import_surface(&mut self, surface: &str, source: &Path) -> Result<Theme> {
        if !["wall", "floor"].contains(&surface)
            || source.extension().and_then(|s| s.to_str()) != Some("png")
        {
            return Err("Choose wall/floor and a .png".into());
        }
        let bytes = read_local(
            source.parent().ok_or("PNG parent absent")?,
            source
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("PNG filename must be UTF-8")?,
            4_000_000,
        )?;
        crate::assets::decode_texture(&bytes)?;
        let old = self.surfaces.clone();
        let mut edit =
            old.get(surface)
                .map(|a| a.edit.clone())
                .unwrap_or(crate::materials::Surface {
                    surface: surface.into(),
                    file: None,
                    tint: [1.0; 3],
                    meters_per_tile: 1.0,
                });
        edit.file = Some(format!("{surface}.png"));
        self.surfaces.insert(
            surface.into(),
            SurfaceAsset {
                edit,
                texture: Some((source.canonicalize().map_err(|e| e.to_string())?, bytes)),
            },
        );
        match self.preview() {
            Ok(t) => Ok(t),
            Err(e) => {
                self.surfaces = old;
                Err(e)
            }
        }
    }
    pub fn source(&self) -> &Path {
        &self.source
    }
    pub fn edit(&self) -> Edit {
        Edit {
            at: self.at,
            yaw_degrees: self.yaw_degrees,
            scale: self.scale,
            solid: self.solid,
            document_role: self.document_role,
        }
    }
    fn set_edit(&mut self, edit: Edit) {
        self.at = edit.at;
        self.yaw_degrees = edit.yaw_degrees;
        self.scale = edit.scale;
        self.solid = edit.solid;
        self.document_role = edit.document_role;
    }
    fn select(&mut self, door: bool, document: bool) -> Result<()> {
        if self.door_active == door && (door || self.document_role == document) {
            return Ok(());
        }
        let mut blank = Self::blank();
        blank.door_active = door;
        blank.document_role = document;
        let old = RetainedAsset::copy(self);
        let key = if door { 2 } else { u8::from(document) };
        let next = self
            .retained
            .remove(&key)
            .unwrap_or_else(|| RetainedAsset::copy(&blank));
        if !old.bytes.is_empty() {
            self.retained.insert(
                if old.door {
                    2
                } else {
                    u8::from(old.edit.document_role)
                },
                old,
            );
        }
        self.restore(next);
        Ok(())
    }
    pub fn select_door(&mut self) -> Result<()> {
        self.select(true, true)
    }
    pub fn select_model(&mut self, document: bool) -> Result<()> {
        self.select(false, document)
    }
    pub fn update(&mut self, edit: Edit) -> Result<Theme> {
        edit.validate()?;
        let old = RetainedAsset::copy(self);
        let switching = !self.door_active && edit.document_role != self.document_role;
        let key = u8::from(edit.document_role);
        let existing = switching && self.retained.contains_key(&key);
        if switching {
            let next = self
                .retained
                .remove(&key)
                .unwrap_or_else(|| RetainedAsset::copy(self));
            if !self.bytes.is_empty() {
                self.retained
                    .insert(u8::from(self.document_role), RetainedAsset::copy(self));
            }
            self.restore(next);
        }
        if !existing {
            self.set_edit(edit);
        }
        match self.preview() {
            Ok(theme) => Ok(theme),
            Err(e) => {
                if switching {
                    self.retained.remove(&u8::from(old.edit.document_role));
                }
                self.restore(old);
                Err(e)
            }
        }
    }
    fn restore(&mut self, asset: RetainedAsset) {
        self.door_active = asset.door;
        self.source = asset.source;
        self.bytes = asset.bytes;
        self.model = asset.model;
        self.set_edit(asset.edit);
    }
    pub fn assign_import(&mut self, previous: &Project) {
        self.document_role = previous.document_role;
        self.door_active = previous.door_active;
        if self.door_active {
            self.scale = 1.0;
        }
        self.surfaces = previous.surfaces.clone();
        self.retained = previous
            .retained
            .iter()
            .map(|(role, a)| {
                (
                    *role,
                    RetainedAsset {
                        source: a.source.clone(),
                        bytes: a.bytes.clone(),
                        edit: a.edit,
                        door: a.door,
                        model: Model {
                            vertices: a.model.vertices.clone(),
                            textures: a.model.textures.clone(),
                            min: a.model.min,
                            max: a.model.max,
                        },
                    },
                )
            })
            .collect();
    }
    fn assets(&self) -> Vec<RetainedAsset> {
        let mut out = self
            .retained
            .values()
            .map(|a| RetainedAsset {
                source: a.source.clone(),
                bytes: a.bytes.clone(),
                edit: a.edit,
                door: a.door,
                model: Model {
                    vertices: a.model.vertices.clone(),
                    textures: a.model.textures.clone(),
                    min: a.model.min,
                    max: a.model.max,
                },
            })
            .collect::<Vec<_>>();
        if !self.bytes.is_empty() {
            out.push(RetainedAsset::copy(self));
        }
        out.sort_by_key(|a| !a.edit.document_role);
        out
    }
    pub fn save_new(&self, target: &Path, protected: &[PathBuf]) -> Result<PathBuf> {
        self.preview()?;
        let files = self.files()?;
        let mut assets = Vec::new();
        for a in self.assets() {
            let file = files
                .iter()
                .find(|(name, b)| name.ends_with(".glb") && **b == a.bytes)
                .ok_or("missing model copy")?
                .0
                .clone();
            assets.push(SavedProject {
                version: 2,
                asset: file,
                original_source: a.source.display().to_string(),
                edit: a.edit,
                door: a.door,
            });
        }
        let document = toml::to_string(&WholeProject {
            version: if self.surfaces.is_empty()
                && !self.bytes.is_empty()
                && !self.assets().iter().any(|a| a.door)
                && !self.door_active
            {
                3
            } else {
                4
            },
            active_document: self.document_role,
            active_door: self.door_active,
            assets,
            surfaces: self.surfaces(),
        })
        .map_err(|e| e.to_string())?;
        let mut protected = protected.to_vec();
        protected.extend(self.assets().into_iter().map(|a| a.source));
        protected.extend(
            self.surfaces
                .values()
                .filter_map(|a| a.texture.as_ref().map(|(p, _)| p.clone())),
        );
        let dir = NewDirectory::create(target, &protected)?;
        for (name, bytes) in files
            .iter()
            .filter(|(n, _)| n.ends_with(".glb") || n.ends_with(".png"))
        {
            dir.write(name, bytes)?;
        }
        dir.write("project.toml", document.as_bytes())?;
        dir.check_identity()?;
        Ok(dir.path.join("project.toml"))
    }
    pub fn open(path: &Path) -> Result<Self> {
        let parent = path.parent().ok_or("Project needs a parent")?;
        let file = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Project filename must be UTF-8")?;
        let bytes = read_local(parent, file, 16384)?;
        let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
        let header: toml::Value =
            toml::from_str(text).map_err(|e| format!("Invalid project TOML: {e}"))?;
        let version = header.get("version").and_then(|v| v.as_integer());
        if version == Some(1) {
            return Err("Legacy v1 project preserved, not opened: world placement is NOT asset-local pivot. Import original GLB and save/export into NEW folders (v3 project); no automatic layout conversion".into());
        }
        let (active, active_door, saved, surfaces) = if version == Some(2) {
            let saved: SavedProject = toml::from_str(text).map_err(|e| e.to_string())?;
            if saved.asset != "model.glb" || saved.door {
                return Err("v2 requires relative model.glb and legacy model role".into());
            }
            (saved.edit.document_role, false, vec![saved], Vec::new())
        } else if matches!(version, Some(3 | 4)) {
            let saved: WholeProject = toml::from_str(text).map_err(|e| e.to_string())?;
            if version == Some(3)
                && (header.get("surfaces").is_some()
                    || header.get("active_door").is_some()
                    || saved.assets.iter().any(|a| a.door))
            {
                return Err("surfaces / door-trim require editable v4".into());
            }
            crate::materials::validate(&saved.surfaces)?;
            (
                saved.active_document,
                saved.active_door,
                saved.assets,
                saved.surfaces,
            )
        } else {
            return Err("Unsupported project version; expected v2, v3 or v4".into());
        };
        if (saved.is_empty() && version != Some(4)) || saved.len() > 3 {
            return Err("project aggregate role count must be 0..3 (empty requires v4)".into());
        }
        let mut roles = BTreeMap::new();
        for saved in saved {
            let key = if saved.door {
                2
            } else {
                u8::from(saved.edit.document_role)
            };
            if saved.version != 2
                || !["model.glb", "model-1.glb", "model-2.glb"].contains(&saved.asset.as_str())
                || roles.contains_key(&key)
            {
                return Err("invalid/duplicate project role or relative asset".into());
            }
            saved.edit.validate()?;
            let mut p = Self::import(&parent.join(saved.asset))?;
            p.set_edit(saved.edit);
            p.door_active = saved.door;
            roles.insert(key, RetainedAsset::copy(&p));
        }
        let key = if active_door { 2 } else { u8::from(active) };
        let mut p = if let Some(selected) = roles.remove(&key) {
            let mut p = Self::blank();
            p.restore(selected);
            p
        } else if version == Some(4) {
            let mut p = Self::blank();
            p.source = path.to_path_buf();
            p.document_role = active;
            p.door_active = active_door;
            p
        } else {
            return Err("active project role absent".into());
        };
        p.retained = roles;
        for edit in surfaces {
            let texture = if let Some(file) = &edit.file {
                if file != &format!("{}.png", edit.surface) {
                    return Err("invalid relative project surface file".into());
                }
                Some((
                    parent
                        .join(file)
                        .canonicalize()
                        .map_err(|e| e.to_string())?,
                    read_local(parent, file, 4_000_000)?,
                ))
            } else {
                None
            };
            p.surfaces
                .insert(edit.surface.clone(), SurfaceAsset { edit, texture });
        }
        p.preview()?;
        Ok(p)
    }
    pub fn import(source: &Path) -> Result<Self> {
        if source.extension().and_then(|s| s.to_str()) != Some("glb") {
            return Err("Choose a .glb with embedded PNG base color and UV0; other formats are not supported".into());
        }
        let parent = source.parent().ok_or("model needs a parent directory")?;
        let name = source
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("model filename must be UTF-8")?;
        let bytes = read_local(parent, name, 8_000_000)?;
        let model = crate::assets::decode(&bytes, 10.0)?;
        let extent = (0..3)
            .map(|a| model.max[a] - model.min[a])
            .fold(0.0_f32, f32::max);
        if extent < 0.0001 {
            return Err("Model has no useful size; re-export nondegenerate geometry".into());
        }
        let mut p = Self {
            source: source.canonicalize().map_err(|e| e.to_string())?,
            bytes,
            model,
            at: [0.0; 3],
            yaw_degrees: 0.0,
            scale: 1.0,
            solid: false,
            document_role: true,
            door_active: false,
            retained: BTreeMap::new(),
            surfaces: BTreeMap::new(),
        };
        p.fit();
        Ok(p)
    }
    pub fn fit(&mut self) {
        let extent = (0..3)
            .map(|a| self.model.max[a] - self.model.min[a])
            .fold(0.0_f32, f32::max);
        self.scale = (1.6 / extent).clamp(0.1, 4.0);
    }
    pub fn reset(&mut self) {
        self.at = [0.0; 3];
        self.yaw_degrees = 0.0;
        self.solid = false;
        self.fit();
    }
    fn files(&self) -> Result<BTreeMap<String, Vec<u8>>> {
        let assets = self.assets();
        let mut files = BTreeMap::new();
        let mut manifest = format!(
            "version={}\nname='Workshop object'\n",
            if self.surfaces.is_empty()
                && !self.bytes.is_empty()
                && !assets.iter().any(|a| a.door)
                && !self.door_active
            {
                2
            } else {
                3
            }
        );
        if self.door_active || assets.iter().any(|a| a.door) {
            manifest.push_str("door_trim=true\n");
        }
        let mut unique: Vec<Vec<u8>> = Vec::new();
        let mut provenance = String::new();
        if assets.is_empty() {
            manifest.push_str("roles=[]\n");
        }
        for a in assets {
            a.edit.validate()?;
            let index = unique
                .iter()
                .position(|bytes| bytes == &a.bytes)
                .unwrap_or_else(|| {
                    unique.push(a.bytes.clone());
                    unique.len() - 1
                });
            let file = if index == 0 {
                "model.glb".to_string()
            } else {
                format!("model-{index}.glb")
            };
            let role = if a.door {
                "door-trim"
            } else if a.edit.document_role {
                "document"
            } else {
                "decoration"
            };
            manifest.push_str(&format!("[[roles]]\nrole='{role}'\nfile='{file}'\nmax_extent=10.0\npivot={:?}\nyaw_degrees={}\nscale={}\n", a.edit.at,a.edit.yaw_degrees,a.edit.scale));
            files.insert(file, a.bytes);
            provenance.push_str(&format!("[{role}]\n"));
            provenance.push_str(
                &toml::to_string(&Provenance {
                    source: a.source.display().to_string(),
                    license: "UNKNOWN — supplied original; no redistribution rights inferred",
                    distribution: "Private user export only; check rights before sharing",
                })
                .map_err(|e| e.to_string())?,
            );
        }
        for a in self.surfaces.values() {
            manifest.push_str("[[surfaces]]\n");
            manifest.push_str(&toml::to_string(&a.edit).map_err(|e| e.to_string())?);
            if let Some((source, bytes)) = &a.texture {
                files.insert(
                    a.edit.file.clone().ok_or("missing surface file")?,
                    bytes.clone(),
                );
                provenance.push_str(&format!("[{}]\n", a.edit.surface));
                provenance.push_str(
                    &toml::to_string(&Provenance {
                        source: source.display().to_string(),
                        license: "UNKNOWN — supplied original; no redistribution rights inferred",
                        distribution: "Private user export only; check rights before sharing",
                    })
                    .map_err(|e| e.to_string())?,
                );
            }
        }
        files.insert("pack.toml".into(), manifest.into_bytes());
        files.insert("PROVENANCE.toml".into(), provenance.into_bytes());
        Ok(files)
    }
    pub fn preview(&self) -> Result<Theme> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "hair-authoring-stage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let files = self.files()?;
        let dir = NewDirectory::create(&path, &[])?;
        let result = (|| {
            for (name, bytes) in &files {
                dir.write(name, bytes)?;
            }
            dir.check_identity()?;
            Theme::load(&path.join("pack.toml"), &mut Default::default())
        })();
        // Only remove our identity-bound staging directory; never a rebound path.
        if dir.check_identity().is_ok() {
            let _ = std::fs::remove_dir_all(&path);
        }
        result
    }
    pub fn export(&self, target: &Path, protected: &[PathBuf]) -> Result<PathBuf> {
        // Complete shared-loader/role/collision validation BEFORE target creation.
        self.preview()?;
        let files = self.files()?;
        let mut protected = protected.to_vec();
        protected.extend(self.assets().into_iter().map(|a| a.source));
        protected.extend(
            self.surfaces
                .values()
                .filter_map(|a| a.texture.as_ref().map(|(p, _)| p.clone())),
        );
        let dir = NewDirectory::create(target, &protected)?;
        for (name, bytes) in &files {
            dir.write(name, bytes)?;
        }
        dir.check_identity()?;
        Ok(dir.path.join("pack.toml"))
    }
}

// Darwin descriptor-relative writer, not a general same-user filesystem sandbox.
// Late leaf replacement is refused; directory rebinding cannot redirect writes.
struct NewDirectory {
    path: PathBuf,
    held: File,
}
impl NewDirectory {
    fn create(path: &Path, protected: &[PathBuf]) -> Result<Self> {
        if !path.is_absolute() {
            return Err(
                "Choose an absolute NEW absent directory; existing projects are never overwritten"
                    .into(),
            );
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Output needs a new directory name")?;
        let name_c = leaf(name)?;
        let parent = path
            .parent()
            .ok_or("Output needs a parent")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let path = parent.join(name);
        for p in protected {
            let p = p.canonicalize().map_err(|e| e.to_string())?;
            if path.starts_with(&p) || p.starts_with(&path) {
                return Err(format!(
                    "Output overlaps read-only input/resources.\nConflicts with read-only path: {}\nChoose a NEW sibling folder outside this path; the output must neither contain nor be inside it.",
                    p.display()
                ));
            }
        }
        let parent = OpenOptions::new()
            .read(true)
            .custom_flags(0x100 | 0x0010_0000 | 0x0100_0000)
            .open(parent)
            .map_err(|e| e.to_string())?;
        unsafe extern "C" {
            fn mkdirat(fd: i32, path: *const std::ffi::c_char, mode: u32) -> i32;
            fn openat(fd: i32, path: *const std::ffi::c_char, flags: i32, ...) -> i32;
        }
        // SAFETY: live owned directory descriptor and validated single component.
        if unsafe { mkdirat(parent.as_raw_fd(), name_c.as_ptr(), 0o700) } != 0 {
            return Err(format!(
                "Cannot create NEW output folder: {}\nChoose an existing parent and a NEW folder name. If this folder or symlink already exists, choose a different name; nothing is replaced. Filesystem reason: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: same pinned parent, no-follow directory open, newly owned descriptor.
        let fd = unsafe {
            openat(
                parent.as_raw_fd(),
                name_c.as_ptr(),
                0x100 | 0x0010_0000 | 0x0100_0000,
            )
        };
        if fd < 0 {
            return Err(format!(
                "Cannot hold new output: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self {
            path,
            held: unsafe { File::from_raw_fd(fd) },
        })
    }
    fn write(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let name = leaf(name)?;
        unsafe extern "C" {
            fn openat(fd: i32, path: *const std::ffi::c_char, flags: i32, ...) -> i32;
        }
        // SAFETY: live held directory; O_WRONLY|O_CREAT|O_EXCL|O_NOFOLLOW|O_CLOEXEC.
        let fd = unsafe {
            openat(
                self.held.as_raw_fd(),
                name.as_ptr(),
                1 | 0x200 | 0x800 | 0x100 | 0x0100_0000,
                0o600_u32,
            )
        };
        if fd < 0 {
            return Err(format!(
                "Exclusive export write refused; partial NEW output retained: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: descriptor returned above, uniquely owned.
        let mut file = unsafe { File::from_raw_fd(fd) };
        file.write_all(bytes)
            .and_then(|_| file.flush())
            .map_err(|e| format!("Export I/O error; partial NEW output retained: {e}"))
    }
    fn check_identity(&self) -> Result<()> {
        let a = self.held.metadata().map_err(|e| e.to_string())?;
        let b = std::fs::symlink_metadata(&self.path).map_err(|e| e.to_string())?;
        if b.is_dir() && a.dev() == b.dev() && a.ino() == b.ino() {
            Ok(())
        } else {
            Err("Output directory rebound; no success acknowledged; writes stayed in held directory".into())
        }
    }
}

fn leaf(name: &str) -> Result<std::ffi::CString> {
    if name.is_empty() || name.contains(['/', '\\', ':']) || name == "." || name == ".." {
        return Err("Unsafe export filename".into());
    }
    std::ffi::CString::new(name).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base(label: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("hair-export-{label}-{}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        p
    }
    #[test]
    fn actual_export_writer_refuses_late_regular_file_and_symlink() {
        let p = base("leaves");
        let d = NewDirectory::create(&p.join("new"), &[]).unwrap();
        let victim = p.join("sentinel");
        std::fs::write(&victim, b"sentinel").unwrap();
        std::fs::write(d.path.join("late.glb"), b"existing").unwrap();
        assert!(d.write("late.glb", b"replacement").is_err());
        assert_eq!(std::fs::read(d.path.join("late.glb")).unwrap(), b"existing");
        std::os::unix::fs::symlink(&victim, d.path.join("linked.glb")).unwrap();
        assert!(d.write("linked.glb", b"replacement").is_err());
        assert_eq!(std::fs::read(victim).unwrap(), b"sentinel");
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn actual_export_writer_survives_directory_rebinding_without_redirect() {
        let p = base("rebind");
        let target = p.join("new");
        let d = NewDirectory::create(&target, &[]).unwrap();
        let held = p.join("held");
        std::fs::rename(&target, &held).unwrap();
        std::fs::create_dir(&target).unwrap();
        d.write("model.glb", b"held bytes").unwrap();
        assert_eq!(
            std::fs::read(held.join("model.glb")).unwrap(),
            b"held bytes"
        );
        assert!(!target.join("model.glb").exists());
        assert!(d.check_identity().is_err());
        std::fs::remove_dir_all(p).unwrap();
    }
}
