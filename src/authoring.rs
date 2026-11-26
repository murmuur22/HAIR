//! Bounded data-only authoring; originals are held as bytes, never edited.
use crate::{Result, assets::Model, content::read_local, theme::Theme};
use serde::{Deserialize, Serialize};
use std::os::{
    fd::{AsRawFd, FromRawFd},
    unix::fs::{MetadataExt, OpenOptionsExt},
};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone)]
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
    decorations: Option<crate::decoration::Spec>,
    catalog_sources: BTreeMap<String, CatalogSource>,
    sky: Option<crate::sky::SkySpec>,
    sky_source: Option<SkySource>,
}
/// Separate appearance closure, never a model-cache or Pin resource. At most
/// 4,000,000 original bytes, shared on clone; old/new closures coexist only
/// during a transaction. This retention is additional to adapter work admission.
#[derive(Clone)]
struct SkySource {
    source: PathBuf,
    bytes: std::sync::Arc<[u8]>,
}
#[derive(Clone)]
struct CatalogSource {
    source: PathBuf,
    bytes: std::sync::Arc<[u8]>,
}
#[derive(Clone)]
struct SurfaceAsset {
    edit: crate::materials::Surface,
    texture: Option<(PathBuf, Vec<u8>)>,
}
#[derive(Clone)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    decorations: Option<crate::decoration::Spec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sky: Option<crate::sky::SkySpec>,
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
            decorations: None,
            catalog_sources: BTreeMap::new(),
            sky: None,
            sky_source: None,
        }
    }
    /// Explicit shared sky declaration; absence is exact Legacy, not a colour.
    pub fn sky(&self) -> Option<&crate::sky::SkySpec> {
        self.sky.as_ref()
    }
    pub fn set_sky_colour(&mut self, rgb: [f64; 3]) -> Result<Theme> {
        crate::sky::SkyAppearance::Colour(rgb).validate()?;
        self.commit_sky(Some(crate::sky::SkySpec::Colour { rgb }), None)
    }
    /// Acquire one bounded static PNG, retaining exact original bytes. The shared
    /// visitor loader applies Fill/default background; no options/schema fork.
    pub fn import_sky_panorama(&mut self, source: &Path) -> Result<Theme> {
        let source = read_sky_source(source)?;
        self.commit_sky(
            Some(crate::sky::SkySpec::Panorama {
                file: "sky.png".into(),
            }),
            Some(source),
        )
    }
    /// Clearing is an explicit edit; it restores historical absence/version selection.
    pub fn clear_sky(&mut self) -> Result<Theme> {
        self.commit_sky(None, None)
    }
    fn commit_sky(
        &mut self,
        sky: Option<crate::sky::SkySpec>,
        source: Option<SkySource>,
    ) -> Result<Theme> {
        let old = std::mem::replace(&mut self.sky, sky);
        let old_source = std::mem::replace(&mut self.sky_source, source);
        match self.preview() {
            Ok(theme) => Ok(theme),
            Err(error) => {
                self.sky = old;
                self.sky_source = old_source;
                Err(error)
            }
        }
    }
    /// Shared visitor vocabulary, absent on legacy role-only projects.
    pub fn catalog(&self) -> Option<&crate::decoration::Spec> {
        self.decorations.as_ref()
    }
    /// Import one explicit asset and its first choice together: unused asset
    /// declarations are never an intermediate accepted state.
    pub fn import_catalog_asset(
        &mut self,
        asset: crate::decoration::Asset,
        entry: crate::decoration::Entry,
        source: &Path,
    ) -> Result<Theme> {
        let mut spec = self.decorations.clone().unwrap_or_default();
        if self.catalog_sources.contains_key(&asset.file) {
            return Err(
                "Catalog file already retained; reuse its asset or explicitly replace its source"
                    .into(),
            );
        }
        let file = asset.file.clone();
        spec.assets.push(asset);
        spec.entries.push(entry);
        validate_catalog(&spec)?;
        let mut sources = self.catalog_sources.clone();
        sources.insert(file, read_catalog_source(source)?);
        self.commit_catalog(spec, sources)
    }
    /// Metadata edits, choice reuse, removal and rebinding are one complete
    /// candidate. Files must already be retained; this never follows a new path.
    pub fn set_catalog(&mut self, spec: crate::decoration::Spec) -> Result<Theme> {
        self.commit_catalog(spec, self.catalog_sources.clone())
    }
    /// Replace an explicitly named retained closure, affecting all declarations
    /// sharing that exact file. Equal bytes at another path confer no authority.
    pub fn replace_catalog_source(&mut self, asset_id: &str, source: &Path) -> Result<Theme> {
        let spec = self.decorations.clone().ok_or("No catalog")?;
        let asset = spec
            .assets
            .iter()
            .find(|a| a.id == asset_id)
            .ok_or("Catalog asset absent")?;
        let mut sources = self.catalog_sources.clone();
        sources.insert(asset.file.clone(), read_catalog_source(source)?);
        self.commit_catalog(spec, sources)
    }
    /// Remove a choice and retire its asset only when no remaining choice uses it.
    pub fn remove_catalog_entry(&mut self, id: &str) -> Result<Theme> {
        let mut spec = self.decorations.clone().ok_or("No catalog")?;
        if !spec.entries.iter().any(|e| e.id == id) {
            return Err("Catalog entry absent".into());
        }
        spec.entries.retain(|e| e.id != id);
        spec.assets
            .retain(|a| spec.entries.iter().any(|e| e.asset == a.id));
        self.set_catalog(spec)
    }
    /// Explicitly remove the asset AND its referencing choices atomically.
    /// Use set_catalog for an atomic rebind that keeps those choices instead.
    pub fn remove_catalog_asset(&mut self, id: &str) -> Result<Theme> {
        let mut spec = self.decorations.clone().ok_or("No catalog")?;
        if !spec.assets.iter().any(|a| a.id == id) {
            return Err("Catalog asset absent".into());
        }
        spec.entries.retain(|e| e.asset != id);
        spec.assets.retain(|a| a.id != id);
        self.set_catalog(spec)
    }
    fn commit_catalog(
        &mut self,
        spec: crate::decoration::Spec,
        mut sources: BTreeMap<String, CatalogSource>,
    ) -> Result<Theme> {
        validate_catalog(&spec)?;
        if spec.assets.iter().any(|a| !sources.contains_key(&a.file)) {
            return Err("Catalog asset has no retained source; use explicit import".into());
        }
        sources.retain(|file, _| spec.assets.iter().any(|a| &a.file == file));
        let old_spec = self.decorations.replace(spec);
        let old_sources = std::mem::replace(&mut self.catalog_sources, sources);
        match self.preview() {
            Ok(theme) => Ok(theme),
            Err(error) => {
                self.decorations = old_spec;
                self.catalog_sources = old_sources;
                Err(error)
            }
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
        self.decorations = previous.decorations.clone();
        self.catalog_sources = previous.catalog_sources.clone();
        self.sky = previous.sky.clone();
        self.sky_source = previous.sky_source.clone();
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
                .find(|(name, b)| is_role_file(name) && **b == a.bytes)
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
                && self.decorations.is_none()
                && self.sky.is_none()
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
            decorations: self.decorations.clone(),
            sky: self.sky.clone(),
        })
        .map_err(|e| e.to_string())?;
        if document.len() > 16384 {
            return Err("Editable project exceeds 16384 bytes".into());
        }
        let mut protected = protected.to_vec();
        protected.extend(self.assets().into_iter().map(|a| a.source));
        protected.extend(self.catalog_protected());
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
            dir.write_output(name, bytes)?;
        }
        dir.write_output("project.toml", document.as_bytes())?;
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
        let (active, active_door, saved, surfaces, decorations, sky) = if version == Some(2) {
            let saved: SavedProject = toml::from_str(text).map_err(|e| e.to_string())?;
            if saved.asset != "model.glb" || saved.door {
                return Err("v2 requires relative model.glb and legacy model role".into());
            }
            (
                saved.edit.document_role,
                false,
                vec![saved],
                Vec::new(),
                None,
                None,
            )
        } else if matches!(version, Some(3 | 4)) {
            let saved: WholeProject = toml::from_str(text).map_err(|e| e.to_string())?;
            if version == Some(3)
                && (header.get("surfaces").is_some()
                    || header.get("decorations").is_some()
                    || header.get("sky").is_some()
                    || header.get("active_door").is_some()
                    || saved.assets.iter().any(|a| a.door))
            {
                return Err("surfaces / door-trim / decorations / sky require editable v4".into());
            }
            crate::materials::validate(&saved.surfaces)?;
            (
                saved.active_document,
                saved.active_door,
                saved.assets,
                saved.surfaces,
                saved.decorations,
                saved.sky,
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
        if let Some(spec) = decorations {
            validate_catalog(&spec)?;
            for asset in &spec.assets {
                if !p.catalog_sources.contains_key(&asset.file) {
                    p.catalog_sources.insert(
                        asset.file.clone(),
                        // Our saved closure uses the same project-root reader as
                        // roles/surfaces, not external selected-source policy.
                        CatalogSource {
                            source: parent.join(&asset.file),
                            bytes: read_local(parent, &asset.file, 8_000_000)?.into(),
                        },
                    );
                }
            }
            p.decorations = Some(spec);
        }
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
        if let Some(crate::sky::SkySpec::Panorama { file }) = &sky {
            validate_project_sky_file(file)?;
            // Saved closures use project-root semantics, NOT external selected
            // source ancestor policy. Leaf safety and finite reads still apply.
            p.sky_source = Some(SkySource {
                source: parent.join(file),
                bytes: read_sky_bytes(parent, file)?.into(),
            });
        }
        p.sky = sky;
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
            decorations: None,
            catalog_sources: BTreeMap::new(),
            sky: None,
            sky_source: None,
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
    fn files(&self) -> Result<BTreeMap<String, Cow<'_, [u8]>>> {
        let assets = self.assets();
        let mut files = BTreeMap::new();
        let mut manifest = format!(
            "version={}\nname='Workshop object'\n",
            if self.surfaces.is_empty()
                && self.decorations.is_none()
                && self.sky.is_none()
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
            files.insert(file, Cow::Owned(a.bytes));
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
                    Cow::Borrowed(bytes.as_slice()),
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
        if let Some(spec) = &self.decorations {
            validate_catalog(spec)?;
            #[derive(Serialize)]
            struct Catalog<'a> {
                decorations: &'a crate::decoration::Spec,
            }
            manifest.push_str(
                &toml::to_string(&Catalog { decorations: spec }).map_err(|e| e.to_string())?,
            );
            for asset in &spec.assets {
                let source = self
                    .catalog_sources
                    .get(&asset.file)
                    .ok_or("Catalog asset has no retained source; use explicit import")?;
                files.insert(asset.file.clone(), Cow::Borrowed(source.bytes.as_ref()));
            }
            for (file, source) in &self.catalog_sources {
                provenance.push_str(&format!("[catalog.\"{file}\"]\n"));
                provenance.push_str(
                    &toml::to_string(&Provenance {
                        source: source.source.display().to_string(),
                        license: "UNKNOWN — supplied original; no redistribution rights inferred",
                        distribution: "Private user export only; check rights before sharing",
                    })
                    .map_err(|e| e.to_string())?,
                );
            }
        }
        if let Some(spec) = &self.sky {
            if let crate::sky::SkySpec::Panorama { file } = spec {
                validate_project_sky_file(file)?;
                let source = self
                    .sky_source
                    .as_ref()
                    .ok_or("Sky panorama has no retained source")?;
                // The fixed portable leaf cannot alias role/catalog/surface or
                // manifest/provenance output. Check before insertion regardless.
                if files.keys().any(|name| name.eq_ignore_ascii_case(file)) {
                    return Err("Sky output filename collision".into());
                }
                files.insert(file.clone(), Cow::Borrowed(source.bytes.as_ref()));
                provenance.push_str("[sky]\n");
                provenance.push_str(
                    &toml::to_string(&Provenance {
                        source: source.source.display().to_string(),
                        license: "UNKNOWN — supplied original; no redistribution rights inferred",
                        distribution: "Private user export only; check rights before sharing",
                    })
                    .map_err(|e| e.to_string())?,
                );
            }
            #[derive(Serialize)]
            struct Sky<'a> {
                sky: &'a crate::sky::SkySpec,
            }
            manifest.push_str(&toml::to_string(&Sky { sky: spec }).map_err(|e| e.to_string())?);
        }
        files.insert("pack.toml".into(), Cow::Owned(manifest.into_bytes()));
        files.insert(
            "PROVENANCE.toml".into(),
            Cow::Owned(provenance.into_bytes()),
        );
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
        result.map_err(|e| format!("Preview validation failed; no public output created: {e}"))
    }
    pub fn export(&self, target: &Path, protected: &[PathBuf]) -> Result<PathBuf> {
        // Complete shared-loader/role/collision validation BEFORE target creation.
        self.preview()?;
        let files = self.files()?;
        let mut protected = protected.to_vec();
        protected.extend(self.assets().into_iter().map(|a| a.source));
        protected.extend(self.catalog_protected());
        protected.extend(
            self.surfaces
                .values()
                .filter_map(|a| a.texture.as_ref().map(|(p, _)| p.clone())),
        );
        let dir = NewDirectory::create(target, &protected)?;
        for (name, bytes) in &files {
            dir.write_output(name, bytes)?;
        }
        dir.check_identity()?;
        Ok(dir.path.join("pack.toml"))
    }
    fn catalog_protected(&self) -> Vec<PathBuf> {
        // Both catalog and Sky held closures use this deleted-source policy.
        // Deleted disposable originals are not a reopening dependency. Existing
        // inputs still participate in overlap refusal; outputs are always NEW.
        self.catalog_sources
            .values()
            .filter(|s| s.source.try_exists().unwrap_or(true))
            .map(|s| s.source.clone())
            .chain(
                self.sky_source
                    .iter()
                    .filter(|s| s.source.try_exists().unwrap_or(true))
                    .map(|s| s.source.clone()),
            )
            .collect()
    }
}

fn validate_project_sky_file(file: &str) -> Result<()> {
    if file != "sky.png" {
        return Err(
            "Editable panorama closure requires reserved sky.png; no path/filename aliases".into(),
        );
    }
    Ok(())
}
fn read_sky_bytes(root: &Path, file: &str) -> Result<Vec<u8>> {
    let bytes = read_local(root, file, 4_000_000)?;
    if bytes.capacity() > 8 * 1024 * 1024 {
        return Err("Sky encoded source capacity exceeds existing 8 MiB work allowance".into());
    }
    Ok(bytes)
}
fn read_sky_source(source: &Path) -> Result<SkySource> {
    if source.extension().and_then(|s| s.to_str()) != Some("png") {
        return Err("Choose a supported local .png for sky".into());
    }
    let absolute = if source.is_absolute() {
        source.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(source)
    };
    let relative = absolute
        .strip_prefix("/")
        .map_err(|e| e.to_string())?
        .to_str()
        .ok_or("Sky source must be UTF-8")?;
    // Pin every selected-source ancestor, as for catalog acquisition. Do not
    // canonicalize away symlink/traversal evidence before this read.
    let bytes = read_sky_bytes(Path::new("/"), relative)?;
    Ok(SkySource {
        source: absolute,
        bytes: bytes.into(),
    })
}

fn is_role_file(file: &str) -> bool {
    ["model.glb", "model-1.glb", "model-2.glb"].contains(&file)
}
fn validate_catalog(spec: &crate::decoration::Spec) -> Result<()> {
    spec.validate()?;
    let mut files = BTreeMap::new();
    for asset in &spec.assets {
        asset.validate_fit()?;
        // Flat editable closure; no role-path alias or byte-based authority merge.
        // Caller chooses the explicit relative filename, not an inferred identity.
        if !asset.file.ends_with(".glb")
            || asset.file.len() > 96
            || !asset
                .file
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err("Catalog requires an explicit flat non-role .glb filename <=96 bytes (ASCII letters/digits/-/_/.)".into());
        }
        leaf(&asset.file)?;
        let folded = asset.file.to_ascii_lowercase();
        // APFS and case-sensitive exports must accept the same closure. Exact
        // acquired-path reuse by distinct asset IDs remains intentional.
        if is_role_file(&folded)
            || files
                .insert(folded, asset.file.as_str())
                .is_some_and(|previous| previous != asset.file)
        {
            return Err("Catalog case-insensitive filename collision with a reserved role or distinct catalog path".into());
        }
    }
    Ok(())
}
fn read_catalog_source(source: &Path) -> Result<CatalogSource> {
    if source.extension().and_then(|s| s.to_str()) != Some("glb") {
        return Err("Choose a supported local .glb".into());
    }
    let absolute = if source.is_absolute() {
        source.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(source)
    };
    let relative = absolute
        .strip_prefix("/")
        .map_err(|e| e.to_string())?
        .to_str()
        .ok_or("Catalog source must be UTF-8")?;
    // Start at the root descriptor so symlink ancestors of the selected file
    // cannot be hidden by canonicalizing its parent before acquisition.
    let bytes = read_local(Path::new("/"), relative, 8_000_000)?;
    Ok(CatalogSource {
        source: absolute,
        bytes: bytes.into(),
    })
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
                "Exclusive new-file write refused: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: descriptor returned above, uniquely owned.
        let mut file = unsafe { File::from_raw_fd(fd) };
        file.write_all(bytes)
            .and_then(|_| file.flush())
            .map_err(|e| format!("New-file I/O error: {e}"))
    }
    fn write_output(&self, name: &str, bytes: &[u8]) -> Result<()> {
        self.write(name, bytes)
            .map_err(|e| format!("{e}; partial NEW output retained: {}", self.path.display()))
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
        let error = d.write("late.glb", b"replacement").unwrap_err();
        assert!(error.contains("Exclusive new-file write refused"));
        assert!(!error.contains("partial NEW output retained"));
        let error = d.write_output("late.glb", b"replacement").unwrap_err();
        assert!(error.contains("partial NEW output retained"));
        assert!(error.contains(&d.path.display().to_string()));
        assert_eq!(std::fs::read(d.path.join("late.glb")).unwrap(), b"existing");
        std::os::unix::fs::symlink(&victim, d.path.join("linked.glb")).unwrap();
        assert!(d.write("linked.glb", b"replacement").is_err());
        std::fs::hard_link(&victim, d.path.join("hardlinked.glb")).unwrap();
        assert!(d.write_output("hardlinked.glb", b"replacement").is_err());
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
    #[test]
    fn sky_growth_after_metadata_refuses_selected_and_saved_closures_without_replacement() {
        use crate::content::read_test_hook::AFTER_METADATA;
        let base = base("sky-growth").canonicalize().unwrap();
        let source = base.join("selected.png");
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[20, 80, 40, 255].repeat(2))
                .unwrap();
        }
        std::fs::write(&source, &bytes).unwrap();
        let mut project = Project::blank();
        let good = project.import_sky_panorama(&source).unwrap();
        let held = project.sky_source.as_ref().unwrap().bytes.clone();
        let relative = source
            .strip_prefix("/")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let growing = source.clone();
        AFTER_METADATA.with(|slot| {
            *slot.borrow_mut() = Some((
                relative,
                Box::new(move || {
                    OpenOptions::new()
                        .write(true)
                        .open(growing)
                        .unwrap()
                        .set_len(4_000_001)
                        .unwrap();
                }),
            ))
        });
        let error = project.import_sky_panorama(&source).err().unwrap();
        assert!(error.contains("grew beyond budget"), "{error}");
        assert_eq!(project.preview().unwrap().sky, good.sky);
        assert!(std::sync::Arc::ptr_eq(
            &held,
            &project.sky_source.as_ref().unwrap().bytes
        ));
        let cloned = project.clone();
        assert!(std::sync::Arc::ptr_eq(
            &held,
            &cloned.sky_source.as_ref().unwrap().bytes
        ));
        let saved = project.save_new(&base.join("saved"), &[]).unwrap();
        let leaf = saved.parent().unwrap().join("sky.png");
        AFTER_METADATA.with(|slot| {
            *slot.borrow_mut() = Some((
                "sky.png".into(),
                Box::new(move || {
                    OpenOptions::new()
                        .write(true)
                        .open(leaf)
                        .unwrap()
                        .set_len(4_000_001)
                        .unwrap();
                }),
            ))
        });
        let error = Project::open(&saved).err().unwrap();
        assert!(error.contains("grew beyond budget"), "{error}");
        AFTER_METADATA.with(|slot| assert!(slot.borrow().is_none()));
        assert_eq!(&*held, bytes);
        // Owned fixture intentionally retained for evidence, including grown inputs.
        println!("SLICE7C_GROWTH_FIXTURE={}", base.display());
    }
}
