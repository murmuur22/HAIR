//! Appearance-only role vocabulary composed inside a separately loaded host scene.
use crate::{
    Action, Input, Pose, Result, World,
    assets::{Model, Vertex},
    content::read_local,
    host_scene::HostScene,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    name: String,
    roles: Vec<Role>,
    #[serde(default)]
    door_trim: bool,
    #[serde(default)]
    surfaces: Vec<crate::materials::Surface>,
    #[serde(default)]
    sky: Option<crate::sky::SkySpec>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Role {
    role: String,
    file: String,
    max_extent: f32,
    scale: f32,
    yaw_degrees: f32,
    pivot: [f32; 3],
}
#[derive(Default)]
pub struct AssetCache {
    models: BTreeMap<PathBuf, Model>,
    pub loads: usize,
    bytes: usize,
    encoded: BTreeMap<PathBuf, Vec<u8>>,
}
#[cfg(test)]
impl AssetCache {
    pub(crate) fn assert_sky_did_not_spend_model_cache(&self) {
        assert_eq!(self.loads, 0);
        assert_eq!(self.bytes, 0);
        assert!(self.models.is_empty());
        assert!(self.encoded.is_empty());
    }
}
pub struct Theme {
    pub name: String,
    pub source: PathBuf,
    pub vertices: Vec<Vertex>,
    pub textures: Vec<std::sync::Arc<crate::assets::Texture>>,
    pub sky: crate::sky::SkyAppearance,
    pub document: [f32; 2],
    pub portal: [f32; 2],
    pub host: HostScene,
    pub diagnostics: Vec<String>,
    pub loaded_roles: Vec<String>,
    pub load_failed: bool,
    // Only the exact appearance role geometry, not the host desk or status marker.
    document_geometry: BTreeMap<String, std::ops::Range<usize>>,
    // Status is decoration, never part of the replaceable source-model envelope.
    document_status_geometry: BTreeMap<String, std::ops::Range<usize>>,
    // Only this suffix is replaceable; constructor/late-installed pack slots survive.
    folder_material_count: usize,
    // Current-document Arc requests survive admission refusal, not page history.
    requested_folder_previews: BTreeMap<String, std::sync::Arc<crate::assets::Texture>>,
    requested_folder_models: BTreeMap<String, std::sync::Arc<Model>>,
    folder_previews: BTreeMap<String, (u32, std::sync::Arc<crate::assets::Texture>)>,
    folder_models: BTreeMap<String, std::sync::Arc<Model>>,
    folder_model_materials: BTreeMap<String, Vec<u32>>,
}
impl Theme {
    /// Only explicit sky packs/refusals add status; legacy success text stays unchanged.
    pub fn sky_feedback(&self) -> Option<&str> {
        self.diagnostics
            .iter()
            .find(|d| d.starts_with("SKY_REFUSED") || d.starts_with("SKY_STATUS"))
            .map(String::as_str)
    }
    /// Recomposition from the surviving fallback source must retain its refusal.
    pub fn retain_load_failure(&mut self, previous: &Self) {
        self.load_failed = previous.load_failed;
        self.diagnostics.extend(
            previous
                .diagnostics
                .iter()
                .filter(|d| d.starts_with("SKY_REFUSED"))
                .cloned(),
        );
    }
    /// Applied counts are anchors; unmatched counts are loaded role names, not models.
    pub fn feedback(&self, world: &World) -> String {
        let scenes: Vec<&HostScene> = world
            .spaces
            .as_ref()
            .map(|s| s.spaces.iter().map(|s| &s.scene).collect())
            .unwrap_or_else(|| vec![&self.host]);
        let visible = |a: &crate::host_scene::Anchor| {
            world.folder_view.is_none()
                || !matches!(
                    a.id.as_str(),
                    crate::folder_scene::INERT_PORTAL | "vocabulary-document" | "vocabulary-image"
                )
        };
        let relevant = |a: &&crate::host_scene::Anchor| {
            visible(a) && ["document", "portal", "decoration"].contains(&a.role.as_str())
        };
        let here = self.host.anchors.iter().filter(relevant).count();
        let matches = |a: &crate::host_scene::Anchor| {
            visible(a)
                && self
                    .loaded_roles
                    .iter()
                    .any(|r| r == &a.role || (r == "door-trim" && a.role == "portal"))
        };
        let applied = self.host.anchors.iter().filter(|a| matches(a)).count();
        let host = scenes
            .iter()
            .flat_map(|s| &s.anchors)
            .filter(relevant)
            .count();
        let host_applied = scenes
            .iter()
            .flat_map(|s| &s.anchors)
            .filter(|a| matches(a))
            .count();
        let unmatched = self
            .loaded_roles
            .iter()
            .filter(|r| {
                !self.host.anchors.iter().any(|a| {
                    visible(a) && (&a.role == *r || (*r == "door-trim" && a.role == "portal"))
                })
            })
            .count();
        let host_unmatched = self
            .loaded_roles
            .iter()
            .filter(|r| {
                !scenes.iter().any(|s| {
                    s.anchors.iter().any(|a| {
                        visible(a) && (&a.role == *r || (*r == "door-trim" && a.role == "portal"))
                    })
                })
            })
            .count();
        let status = if self.load_failed {
            "LOAD FAILED; FALLBACK"
        } else if applied == 0 {
            "NO MATCHING ROLES"
        } else {
            "LOADED"
        };
        let feedback = format!(
            "PACK {} | {status} {}\nHERE {applied}/{here} HOST {host_applied}/{host} UNMATCHED {unmatched}/{host_unmatched} FALLBACK {}/{}",
            self.name,
            self.loaded_roles.len(),
            here - applied,
            host - host_applied
        );
        match self.sky_feedback() {
            Some(sky) => format!("{feedback}\n{sky}"),
            None => feedback,
        }
    }
    pub fn load(path: &Path, cache: &mut AssetCache) -> Result<Self> {
        Self::load_for_scene(path, cache, &HostScene::shared_default())
    }
    pub fn load_for_scene(path: &Path, cache: &mut AssetCache, host: &HostScene) -> Result<Self> {
        Self::load_for_scene_impl(path, cache, host, false)
    }
    /// Local pages retain required vocabulary, but an inert terminal portal must
    /// not become a visible destination. Other hosts keep their authored anchors.
    pub fn load_for_folder_scene(
        path: &Path,
        cache: &mut AssetCache,
        host: &HostScene,
    ) -> Result<Self> {
        Self::load_for_scene_impl(path, cache, host, true)
    }
    fn load_for_scene_impl(
        path: &Path,
        cache: &mut AssetCache,
        host: &HostScene,
        folder: bool,
    ) -> Result<Self> {
        let root = path
            .parent()
            .ok_or("pack needs parent")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let file = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("pack filename must be UTF-8")?;
        let bytes = read_local(&root, file, 32768)?;
        let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
        // Inspect version first so legacy users receive a precise non-destructive refusal.
        let header: toml::Value =
            toml::from_str(text).map_err(|e| format!("appearance pack TOML: {e}"))?;
        if !matches!(
            header.get("version").and_then(|v| v.as_integer()),
            Some(2 | 3)
        ) {
            return Err("appearance pack version 2 or 3 required; legacy combined v1 room/placement packs are incompatible. Original unchanged; layout conversion is not supported in this slice; reimport the original GLB in Workshop and export a NEW appearance pack".into());
        }
        if header.get("version").and_then(|v| v.as_integer()) == Some(2)
            && header.get("sky").is_some()
        {
            return Err("SKY_REFUSED: sky requires appearance version 3; v2 input is never implicitly upgraded".into());
        }
        let data: Manifest = toml::from_str(text).map_err(|e| {
            let prefix = if header.get("sky").is_some() {
                "SKY_REFUSED: "
            } else {
                ""
            };
            format!(
                "{prefix}appearance pack {}: layout/unknown fields forbidden: {e}",
                path.display()
            )
        })?;
        // Sky resources have a separate finite budget, never world material slots.
        // Colour allocates no image; panorama is decoded once into immutable held pixels.
        let sky = data
            .sky
            .map(|spec| spec.load(&root))
            .transpose()?
            .unwrap_or_default();
        crate::materials::validate(&data.surfaces)?;
        if data.version == 2
            && (header.get("surfaces").is_some() || header.get("door_trim").is_some())
        {
            return Err(
                "surfaces require appearance version 3; v2 input is never implicitly upgraded"
                    .into(),
            );
        }
        if !matches!(data.version, 2 | 3)
            || data.name.is_empty()
            || data.name.len() > 40
            || data.roles.len() > 16
            || data
                .roles
                .iter()
                .map(|r| &r.file)
                .collect::<HashSet<_>>()
                .len()
                > 3
        {
            return Err("appearance pack version/name/16 role budget invalid".into());
        }
        let mut roles = BTreeMap::new();
        let mut diagnostics = Vec::new();
        match &sky {
            crate::sky::SkyAppearance::Colour(_) => {
                diagnostics.push("SKY_STATUS LOADED SKY COLOUR (static linear RGB)".into());
            }
            crate::sky::SkyAppearance::Panorama(_) => {
                diagnostics.push("SKY_STATUS LOADED SKY PANORAMA (static pack-local PNG)".into());
            }
            crate::sky::SkyAppearance::Legacy => {}
        }
        let mut names = HashSet::new();
        for r in data.roles {
            if !names.insert(r.role.clone()) {
                return Err("duplicate appearance role".into());
            }
            if !["document", "portal", "decoration", "door-trim"].contains(&r.role.as_str()) {
                diagnostics.push(format!(
                    "ROLE_FALLBACK unsupported role {:?}; model budgeted but not applied, host unchanged",
                    r.role
                ));
            }
            let valid = r.max_extent.is_finite()
                && r.max_extent > 0.0
                && r.max_extent <= 10.0
                && r.scale.is_finite()
                && (0.1..=4.0).contains(&r.scale)
                && r.yaw_degrees.is_finite()
                && (-180.0..=180.0).contains(&r.yaw_degrees)
                && r.pivot.iter().all(|v| v.is_finite() && v.abs() <= 3.0)
                && r.file.ends_with(".glb");
            let loaded: Result<PathBuf> = (|| {
                if !valid
                    || r.file.is_empty()
                    || r.file.contains(['\\', ':'])
                    || Path::new(&r.file).components().count() > 8
                    || !Path::new(&r.file)
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)))
                {
                    return Err(
                        "invalid asset-local fit constraints or unsafe relative GLB path".into(),
                    );
                }
                let key = root.join(&r.file);
                let bytes = read_local(&root, &r.file, 8_000_000)?;
                if cache.encoded.get(&key) != Some(&bytes) {
                    let old = cache.encoded.get(&key).map_or(0, Vec::len);
                    if cache.bytes - old + bytes.len() > 32_000_000 {
                        return Err("aggregate pack models exceed 32 MB".into());
                    }
                    let model = crate::assets::decode(&bytes, r.max_extent)?;
                    cache.bytes = cache.bytes - old + bytes.len();
                    cache.loads += 1;
                    cache.encoded.insert(key.clone(), bytes);
                    cache.models.insert(key.clone(), model);
                }
                let m = &cache.models[&key];
                if (0..3).any(|i| m.max[i] - m.min[i] > r.max_extent) {
                    return Err("cached model exceeds declared bounds".into());
                }
                Ok(key)
            })();
            match loaded {
                Ok(key) => {
                    if r.role == "door-trim" {
                        if data.version != 3 {
                            return Err("door-trim requires appearance v3".into());
                        }
                        crate::door_trim::qualify(
                            &cache.models[&key],
                            r.pivot,
                            r.yaw_degrees,
                            r.scale,
                        )?;
                    }
                    roles.insert(r.role.clone(), (r, key));
                }
                Err(e) => {
                    if data.version == 3 {
                        return Err(format!("strict material pack role {}: {e}", r.role));
                    }
                    diagnostics.push(format!("ROLE_FALLBACK {}: {e}; host placeholder", r.role))
                }
            }
        }
        let keys: HashSet<_> = roles.values().map(|(_, key)| key).collect();
        let model_vertices: usize = keys
            .iter()
            .map(|key| cache.models[*key].vertices.len())
            .sum();
        let texture_bytes: usize = keys
            .iter()
            .flat_map(|key| &cache.models[*key].textures)
            .map(|t| t.rgba.len())
            .sum();
        let texture_count: usize = keys
            .iter()
            .map(|key| cache.models[*key].textures.len())
            .sum();
        let encoded_bytes: usize = keys.iter().map(|key| cache.encoded[*key].len()).sum();
        if keys.len() > 3
            || model_vertices > 300_000
            || texture_bytes + crate::materials::decoded_budget(&data.surfaces) > 32_000_000
            || texture_count + data.surfaces.iter().filter(|s| s.file.is_some()).count() > 64
            || encoded_bytes + data.surfaces.iter().filter(|s| s.file.is_some()).count() * 4_000_000
                > 24_000_000
            || model_vertices * std::mem::size_of::<Vertex>()
                + texture_bytes
                + encoded_bytes
                + crate::materials::decoded_budget(&data.surfaces)
                + data.surfaces.iter().filter(|s| s.file.is_some()).count() * 4_000_000
                > 64_000_000
        {
            return Err("aggregate whole-pack budget: <=3 models, 24 MB encoded, 300000 decoded vertices, 32 MB/64 textures, 64 MB retained payload".into());
        }
        roles.retain(|role, _| {
            ["document", "portal", "decoration", "door-trim"].contains(&role.as_str())
        });
        if roles.contains_key("door-trim") && roles.contains_key("portal") {
            return Err("Choose door-trim OR legacy portal, not both".into());
        }
        let has_trim = data.door_trim || roles.contains_key("door-trim");
        if has_trim && roles.contains_key("portal") {
            return Err("Door trim cannot coexist with legacy portal appearance".into());
        }
        let matches = |role: &str, anchor: &str| {
            role == anchor || (role == "door-trim" && anchor == "portal")
        };
        let visible = |a: &crate::host_scene::Anchor| {
            !folder
                || !matches!(
                    a.id.as_str(),
                    crate::folder_scene::INERT_PORTAL | "vocabulary-document" | "vocabulary-image"
                )
        };
        let loaded_roles: Vec<String> = roles.keys().cloned().collect();
        for role in roles.keys() {
            if !host
                .anchors
                .iter()
                .any(|a| visible(a) && matches(role, &a.role))
            {
                diagnostics.push(format!(
                    "ROLE_UNMATCHED {role}: loaded but no matching anchors in current space"
                ));
            }
        }
        diagnostics.push(format!(
            "PACK_STATUS loaded={} applied={}",
            roles.len(),
            host.anchors
                .iter()
                .filter(|a| visible(a) && roles.keys().any(|r| matches(r, &a.role)))
                .count()
        ));
        let mut vertices = host.vertices();
        let mut document_geometry = BTreeMap::new();
        let mut document_status_geometry = BTreeMap::new();
        let mut textures = crate::materials::load(&root, &data.surfaces)?;
        crate::materials::apply(&mut vertices, &data.surfaces, host.walls().len());
        for a in &host.anchors {
            // The local terminal-page portal is required schema vocabulary, not
            // a destination. Never draw a false Next door (model/frame or panel).
            if a.role == "image" || !visible(a) {
                continue;
            }
            // Generate the entire portal (legacy model, trim/fallback, panel)
            // in canonical width/height/depth, then rigidly place it once. Trim's
            // normalized aperture qualification remains entirely door-local.
            let basis = (a.role == "portal").then(|| host.portal_basis(&a.id));
            let at = a.at;
            let local = basis.map(|b| b.canonical_anchor(a));
            let a = local.as_ref().unwrap_or(a);
            let start = vertices.len();
            if a.role == "portal" && has_trim {
                crate::door_trim::frame(&mut vertices, a);
                if let Some((r, key)) = roles.get("door-trim") {
                    let m = &cache.models[key];
                    if vertices.len() + m.vertices.len() > 300_000 {
                        return Err("appearance scene exceeds 300000 vertices".into());
                    }
                    let offset = textures.len() as u32;
                    textures.extend(m.textures.iter().cloned());
                    let trim = crate::door_trim::qualify(m, r.pivot, r.yaw_degrees, r.scale)?;
                    crate::door_trim::append(&mut vertices, &trim, a, offset);
                } else {
                    diagnostics.push(format!(
                        "ROLE_FALLBACK door-trim absent; open host frame at {}",
                        a.id
                    ));
                }
            } else if let Some((r, key)) = roles.get(&a.role) {
                let m = &cache.models[key];
                if vertices.len() + m.vertices.len() > 300_000 {
                    return Err("appearance scene exceeds 300000 vertices".into());
                }
                let offset = textures.len() as u32;
                textures.extend(m.textures.iter().cloned());
                use glam::Vec3;
                let rot = glam::Mat4::from_rotation_y(r.yaw_degrees.to_radians());
                let mut min = Vec3::splat(f32::INFINITY);
                let mut max = Vec3::splat(f32::NEG_INFINITY);
                for v in &m.vertices {
                    let p = rot.transform_point3(Vec3::from_array(v.position));
                    min = min.min(p);
                    max = max.max(p)
                }
                let origin = Vec3::new((min.x + max.x) * 0.5, min.y, (min.z + max.z) * 0.5);
                let pivot = Vec3::from_array(r.pivot);
                let lo = (min - origin + pivot) * r.scale;
                let hi = (max - origin + pivot) * r.scale;
                let bottom = lo.y.min(0.0);
                let ext = Vec3::new(
                    lo.x.abs().max(hi.x.abs()) * 2.0,
                    hi.y - bottom,
                    lo.z.abs().max(hi.z.abs()) * 2.0,
                );
                let factor = (0..3)
                    .filter(|i| ext[*i] > 0.000001)
                    .map(|i| a.size[i] / ext[i])
                    .fold(1.0_f32, f32::min);
                if factor < 1.0 {
                    diagnostics.push(format!(
                        "ROLE_FIT {} uniformly shrunk by {factor}; anchor {} unchanged",
                        a.role, a.id
                    ))
                }
                for v in &m.vertices {
                    let mut p = (rot.transform_point3(Vec3::from_array(v.position)) - origin
                        + pivot)
                        * r.scale;
                    p.y -= bottom;
                    p = p * factor + Vec3::from_array(a.at);
                    if !p.is_finite() {
                        return Err("nonfinite fitted appearance".into());
                    }
                    vertices.push(Vertex {
                        position: p.to_array(),
                        texture: if v.texture == 0 {
                            0
                        } else {
                            v.texture + offset
                        },
                        ..*v
                    });
                }
            } else {
                diagnostics.push(format!(
                    "ROLE_FALLBACK {} absent/invalid; host placeholder at {}",
                    a.role, a.id
                ));
                crate::scene::cuboid(
                    &mut vertices,
                    [a.at[0], a.at[1] + a.size[1] * 0.3, a.at[2]],
                    [a.size[0] * 0.6, a.size[1] * 0.6, a.size[2] * 0.6],
                    [0.40, 0.48, 0.36],
                );
            }
            if a.role == "document" {
                document_geometry.insert(a.id.clone(), start..vertices.len());
                let status_start = vertices.len();
                crate::scene::cuboid(
                    &mut vertices,
                    [a.at[0], a.at[1] + a.size[1] + 0.03, a.at[2]],
                    [0.45, 0.02, 0.2],
                    [0.1, 0.8, 0.3],
                );
                document_status_geometry.insert(a.id.clone(), status_start..vertices.len());
            } else if a.role == "portal" {
                crate::scene::cuboid(
                    &mut vertices,
                    [
                        a.at[0],
                        a.at[1] + a.size[1] * 0.5,
                        a.at[2] + a.size[2] * 0.5 + 0.02,
                    ],
                    [0.45, 0.25, 0.02],
                    [0.08, 0.6, 0.18],
                );
            }
            if let Some(basis) = basis {
                basis.place(&mut vertices[start..], at);
            }
        }
        if textures.len() > 64 || textures.iter().map(|t| t.rgba.len()).sum::<usize>() > 32_000_000
        {
            return Err("scene decoded texture budget exceeds 32 MB / 64 images".into());
        }
        for d in &diagnostics {
            eprintln!("{d}")
        }
        let d = host.anchor("document");
        let p = host.anchor("portal");
        Ok(Self {
            name: data.name,
            source: path.to_path_buf(),
            vertices,
            textures,
            sky,
            document: [d.at[0], d.at[2]],
            portal: [p.at[0], p.at[2]],
            host: host.clone(),
            diagnostics,
            loaded_roles,
            load_failed: false,
            document_geometry,
            document_status_geometry,
            folder_material_count: 0,
            requested_folder_previews: BTreeMap::new(),
            requested_folder_models: BTreeMap::new(),
            folder_previews: BTreeMap::new(),
            folder_models: BTreeMap::new(),
            folder_model_materials: BTreeMap::new(),
        })
    }
    pub fn blocked(&self, p: [f32; 2]) -> bool {
        self.host.blocked(p)
    }
    pub fn respawn(&self, pose: &mut Pose) {
        *pose = Pose {
            position: self.host.spawn,
            ..Pose::default()
        }
    }
    /// Validate before a renderer switches; no mutation/respawn is needed for a reskin.
    pub fn reskin_from(&self, previous: &Self, pose: &Pose) -> Result<()> {
        if self.host != previous.host {
            return Err("reskin refused: host scene changed; explicit navigation required".into());
        }
        if self.blocked(pose.position) {
            return Err("reskin refused: current pose is outside shared navigation".into());
        }
        Ok(())
    }
    pub fn act(&self, world: &mut World, action: Action, position: [f32; 2]) {
        world.act_at(action, position, self.document, self.portal)
    }
    pub fn locomote(&self, pose: &mut Pose, input: &Input, dt: f32) {
        if !input.captured || !dt.is_finite() {
            return;
        }
        let local = glam::Vec2::new(
            (input.right as i32 - input.left as i32) as f32,
            (input.back as i32 - input.forward as i32) as f32,
        )
        .normalize_or_zero();
        let (s, c) = pose.yaw.sin_cos();
        let delta = glam::Vec2::new(c * local.x - s * local.y, s * local.x + c * local.y)
            * dt.clamp(0.0, 0.25)
            * 2.5
            * if input.running { 2.0 } else { 1.0 }
            / 16.0;
        for _ in 0..16 {
            for axis in 0..2 {
                let mut p = pose.position;
                p[axis] += delta[axis];
                if !self.blocked(p) {
                    pose.position = p
                }
            }
        }
    }
    fn label_for(
        &self,
        text: &str,
        anchor: &crate::host_scene::Anchor,
        columns: usize,
        y_offset: f32,
    ) -> Vec<Vertex> {
        let basis = (anchor.role == "portal").then(|| self.host.portal_basis(&anchor.id));
        let local = basis.map(|b| b.canonical_anchor(anchor));
        let a = local.as_ref().unwrap_or(anchor);
        let mut vertices = crate::text::object_label(
            text,
            [
                a.at[0],
                a.at[1] + a.size[1] + 0.22 + y_offset,
                a.at[2] + a.size[2] * 0.5 + 0.02,
            ],
            columns,
        );
        if let Some(basis) = basis {
            basis.place(&mut vertices, anchor.at);
        }
        vertices
    }
    /// Read-only CPU admission observation for I; never GPU/presentation proof.
    /// Match the retained source Arc, not texture-list presence or a reused id.
    pub fn folder_preview_admitted(
        &self,
        id: &str,
        image: Option<&std::sync::Arc<crate::assets::Texture>>,
        model: Option<&std::sync::Arc<Model>>,
    ) -> bool {
        image.is_some_and(|image| {
            self.folder_previews
                .get(id)
                .is_some_and(|(_, admitted)| std::sync::Arc::ptr_eq(image, admitted))
        }) || model.is_some_and(|model| {
            self.folder_models
                .get(id)
                .is_some_and(|admitted| std::sync::Arc::ptr_eq(model, admitted))
        })
    }
    /// Replace bounded current-document requests, not an admission-filtered history.
    /// Either setter rebuilds BOTH classes; raster order and priority stay first.
    pub fn set_folder_previews(
        &mut self,
        previews: &BTreeMap<String, std::sync::Arc<crate::assets::Texture>>,
    ) {
        self.requested_folder_previews = self
            .document_geometry
            .keys()
            .filter_map(|id| previews.get(id).map(|image| (id.clone(), image.clone())))
            .take(crate::folder_scene::PAGE_SIZE)
            .collect();
        self.rebuild_folder_materials();
    }
    pub fn set_folder_models(&mut self, models: &BTreeMap<String, std::sync::Arc<Model>>) {
        self.requested_folder_models = self
            .document_geometry
            .keys()
            .filter_map(|id| models.get(id).map(|model| (id.clone(), model.clone())))
            .take(crate::folder_scene::PAGE_SIZE)
            .collect();
        self.rebuild_folder_materials();
    }
    // Reserve an upper bound for existing local labels (5x7 cells, six vertices
    // per lit cell), parent/continuation labels, selection and door fingerprints.
    // Keep all base geometry charged, even when a replacement would remove it.
    // This intentionally refuses some near-cap scenes rather than borrowing space
    // from fallback/raster geometry or from decorations emitted by vertices_for.
    fn folder_vertex_reservation(&self) -> Option<usize> {
        let labels = self
            .host
            .anchors
            .len()
            .checked_mul(25 + 28)?
            .checked_mul(5 * 7 * 6)?;
        let navigation = (32_usize + 2 * 40).checked_mul(5 * 7 * 6)?;
        let fingerprints = self
            .host
            .anchors
            .iter()
            .filter(|a| a.binding == "explicit-door")
            .count()
            .checked_mul(8 * 8 * 6)?;
        self.vertices
            .len()
            .checked_add(labels)?
            .checked_add(navigation)?
            .checked_add(fingerprints)?
            .checked_add(2 * 36)?
            .checked_add(self.document_geometry.len().checked_mul(6)?)
    }
    // Validate the EXACT existing centered uniform nonzero-axis XYZ fit before
    // admitting status/materials. No positions, UVs, colors or pixels are copied.
    fn local_model_fits(model: &Model, a: &crate::host_scene::Anchor) -> bool {
        if model.vertices.is_empty()
            || model.vertices.len() > 32_768
            || model.textures.len() > 1
            || crate::assets::retained_model_bytes(model)
                .filter(|n| *n <= 4 * 1024 * 1024)
                .is_none()
            || !a.at.iter().all(|n| n.is_finite())
            || !a.size.iter().all(|n| n.is_finite() && *n > 0.0)
            || (0..3).any(|i| {
                !model.min[i].is_finite()
                    || !model.max[i].is_finite()
                    || model.max[i] < model.min[i]
            })
        {
            return false;
        }
        for image in &model.textures {
            let cost = usize::try_from(image.width)
                .ok()
                .and_then(|w| {
                    usize::try_from(image.height)
                        .ok()
                        .and_then(|h| w.checked_mul(h))
                })
                .and_then(|n| n.checked_mul(4));
            if image.width == 0
                || image.height == 0
                || image.width > 512
                || image.height > 512
                || cost != Some(image.rgba.len())
                || image.rgba.len() > 1024 * 1024
                || image
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] != 255)
            {
                return false;
            }
        }
        let extent: [f32; 3] = std::array::from_fn(|i| model.max[i] - model.min[i]);
        if extent.iter().any(|n| !n.is_finite()) {
            return false;
        }
        let scale = (0..3)
            .filter(|i| extent[*i] > 0.0)
            .map(|i| a.size[i] / extent[i])
            .fold(f32::INFINITY, f32::min);
        if !scale.is_finite() || scale <= 0.0 {
            return false;
        }
        let center = [a.at[0], a.at[1] + a.size[1] * 0.5, a.at[2]];
        model.vertices.iter().all(|v| {
            usize::try_from(v.texture)
                .ok()
                .is_some_and(|index| index <= model.textures.len())
                && v.uv.iter().chain(v.color.iter()).all(|n| n.is_finite())
                && (0..3).all(|i| {
                    v.position[i].is_finite()
                        && v.position[i] >= model.min[i]
                        && v.position[i] <= model.max[i]
                        && (center[i]
                            + (v.position[i] - (model.min[i] + model.max[i]) * 0.5) * scale)
                            .is_finite()
                })
        })
    }
    fn rebuild_folder_materials(&mut self) {
        self.folder_previews.clear();
        self.folder_models.clear();
        self.folder_model_materials.clear();
        // Derive the actual surviving pack prefix, including pack slots appended
        // before the first setter. Underflow cannot justify deleting ANY material.
        let Some(pack_len) = self.textures.len().checked_sub(self.folder_material_count) else {
            return; // fail closed: keep all slots, publish no local success
        };
        self.textures.truncate(pack_len);
        self.folder_material_count = 0;
        let Some(mut bytes) = self
            .textures
            .iter()
            .try_fold(4_usize * 1024 * 1024, |sum, t| {
                sum.checked_add(t.rgba.len())
            })
            .filter(|n| *n <= 32_000_000)
        else {
            return;
        };
        for (id, image) in &self.requested_folder_previews {
            let Some(next_bytes) = bytes
                .checked_add(image.rgba.len())
                .filter(|n| *n <= 32_000_000)
            else {
                continue;
            };
            let Some(material) = self
                .textures
                .len()
                .checked_add(1)
                .filter(|n| *n <= 63)
                .and_then(|n| u32::try_from(n).ok())
            else {
                continue;
            };
            bytes = next_bytes;
            self.textures.push(image.clone());
            self.folder_material_count += 1; // bounded by 63 checked above
            self.folder_previews
                .insert(id.clone(), (material | 0x8000_0000, image.clone()));
        }
        let Some(mut count) = self.folder_vertex_reservation().filter(|n| *n <= 300_000) else {
            return; // rasters keep their inherited presentation; no model expansion
        };
        for (id, model) in &self.requested_folder_models {
            if self.folder_previews.contains_key(id) {
                continue;
            }
            let Some(a) = self
                .host
                .anchors
                .iter()
                .find(|a| &a.id == id && a.role == "document")
            else {
                continue;
            };
            if !Self::local_model_fits(model, a) {
                continue;
            }
            let Some(next_count) = count
                .checked_add(model.vertices.len())
                .filter(|n| *n <= 300_000)
            else {
                continue;
            };
            let Some(next_len) = self
                .textures
                .len()
                .checked_add(model.textures.len())
                .filter(|n| *n <= 63)
            else {
                continue;
            };
            let Some(next_bytes) = model
                .textures
                .iter()
                .try_fold(bytes, |sum, image| sum.checked_add(image.rgba.len()))
                .filter(|n| *n <= 32_000_000)
            else {
                continue;
            };
            // Mapping is staged and checked BEFORE any image/model/status publish.
            // Local zero stays white; scene indices are lit, without media high bit.
            let Some(mapping) = std::iter::once(Some(0_u32))
                .chain(
                    (self.textures.len()..next_len)
                        .map(|index| index.checked_add(1).and_then(|n| u32::try_from(n).ok())),
                )
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            self.textures.extend(model.textures.iter().cloned());
            self.folder_material_count = next_len - pack_len;
            bytes = next_bytes;
            count = next_count;
            self.folder_model_materials.insert(id.clone(), mapping);
            self.folder_models.insert(id.clone(), model.clone());
        }
    }
    // A suffix alone is not a successful thumbnail. Use Theme's admitted
    // texture map, not scheduler intent/status or the transient aimed target.
    // Refused video and reused non-video slots retain ordinary pack decoration.
    fn has_video_still(&self, id: &str, entry: &crate::local_explorer::Entry) -> bool {
        self.folder_previews.contains_key(id)
            && entry.kind == crate::local_explorer::Kind::File
            && Path::new(&entry.name)
                .extension()
                .is_some_and(|suffix| suffix.as_encoded_bytes().eq_ignore_ascii_case(b"mp4"))
    }
    // Only an admitted texture AND the current regular raw image suffix select
    // image fitting. Text-generated textures and MP4 stills keep their own
    // presentation; App's existing binding/generation checks own source identity.
    fn admitted_image_kind(
        &self,
        id: &str,
        entry: &crate::local_explorer::Entry,
    ) -> Option<&'static str> {
        if !self.folder_previews.contains_key(id) || entry.kind != crate::local_explorer::Kind::File
        {
            return None;
        }
        let suffix = Path::new(&entry.name).extension()?.as_encoded_bytes();
        if suffix.eq_ignore_ascii_case(b"png") {
            Some("PNG image")
        } else if suffix.eq_ignore_ascii_case(b"jpg") || suffix.eq_ignore_ascii_case(b"jpeg") {
            Some("JPEG image")
        } else {
            None
        }
    }
    // A symmetric content-only inset clears the retained local floor without
    // moving the host envelope or its center. Too-short envelopes keep fallback.
    fn image_fit_scale(
        a: &crate::host_scene::Anchor,
        image: &crate::assets::Texture,
    ) -> Option<f32> {
        if image.width == 0
            || image.height == 0
            || !a.size[..2].iter().all(|v| v.is_finite() && *v > 0.0)
        {
            return None;
        }
        let inset = 0.06_f32.min(a.size[1] * 0.5);
        let usable_height = a.size[1] - inset * 2.0;
        if !usable_height.is_finite() || usable_height <= 0.0 {
            return None;
        }
        let scale = (a.size[0] / image.width as f32).min(usable_height / image.height as f32);
        (scale.is_finite()
            && scale > 0.0
            && image.width as f32 * scale > 0.0
            && image.height as f32 * scale > 0.0)
            .then_some(scale)
    }
    // Successful image decorations require both admission/current suffix and a
    // drawable inset fit; an unfittable image retains ordinary pack decoration.
    // Center the metadata above the actual fitted image, in its source plane.
    // These offsets place decorations only; source fitting remains independent.
    fn image_placard(
        &self,
        id: &str,
        entry: &crate::local_explorer::Entry,
    ) -> Option<(&'static str, [f32; 3])> {
        let kind = self.admitted_image_kind(id, entry)?;
        let a = self
            .host
            .anchors
            .iter()
            .find(|a| a.id == id && a.role == "document")?;
        let (_, image) = self.folder_previews.get(id)?;
        let scale = Self::image_fit_scale(a, image)?;
        let top = a.at[1] + (a.size[1] + image.height as f32 * scale) * 0.5;
        Some((kind, [a.at[0], top + 0.19, a.at[2] + a.size[2] * 0.45]))
    }
    pub fn vertices_for(&self, world: &World) -> Vec<Vertex> {
        let mut out = self.vertices.clone();
        if let Some(view) = &world.folder_view {
            // Move only admitted local-model status decoration, before any source
            // splices change offsets. The stored base scene and other media stay
            // unchanged. Key this to the admitted payload, like its presentation,
            // not to the displayed basename or transient interaction selection.
            for id in self.folder_models.keys() {
                if let Some(a) = self.host.anchors.iter().find(|a| &a.id == id)
                    && let Some(range) = self.document_status_geometry.get(id)
                {
                    let z = a.at[2] - a.size[2] * 0.5 - 0.35;
                    for v in &mut out[range.clone()] {
                        v.position[0] = a.at[0] + (v.position[0] - a.at[0]) * 0.5;
                        v.position[1] -= 0.18;
                        v.position[2] = z + (v.position[2] - a.at[2]) * 0.3;
                    }
                }
            }
            // Successful MP4 stills use their own placard column. Move only
            // recorded status vertices before source splices alter the offsets;
            // do not move the image, pack fallback, host or model decoration.
            for (id, bound) in &view.bindings {
                if self.has_video_still(id, &bound.entry)
                    && let Some(a) = self
                        .host
                        .anchors
                        .iter()
                        .find(|a| &a.id == id && a.role == "document")
                    && let Some(range) = self.document_status_geometry.get(id)
                {
                    let z = a.at[2] - a.size[2] * 0.5 - 0.35;
                    for v in &mut out[range.clone()] {
                        v.position[0] = a.at[0] + 0.65 + (v.position[0] - a.at[0]) * 0.5;
                        v.position[1] -= 0.59;
                        v.position[2] = z + (v.position[2] - a.at[2]) * 0.3;
                    }
                }
            }
            // Only admitted current images move their recorded 36-vertex status
            // cuboid into the compact group above the fitted source. Keep its
            // topology/order and leave pending/refused/fallback markers unchanged.
            for (id, bound) in &view.bindings {
                if let Some((_, at)) = self.image_placard(id, &bound.entry)
                    && let Some(a) = self
                        .host
                        .anchors
                        .iter()
                        .find(|a| &a.id == id && a.role == "document")
                    && let Some(range) = self.document_status_geometry.get(id)
                {
                    let original_y = a.at[1] + a.size[1] + 0.03;
                    for v in &mut out[range.clone()] {
                        v.position[0] = at[0] + 0.5 + (v.position[0] - a.at[0]) * 0.5;
                        v.position[1] = at[1] - 0.13 + (v.position[1] - original_y);
                        v.position[2] = at[2] + (v.position[2] - a.at[2]) * 0.1;
                    }
                }
            }
            let mut replacements = Vec::new();
            for (id, (material, image)) in &self.folder_previews {
                let Some(bound) = view.bindings.get(id) else {
                    continue;
                };
                let Some(a) = self
                    .host
                    .anchors
                    .iter()
                    .find(|a| &a.id == id && a.role == "document")
                else {
                    continue;
                };
                let Some(range) = self.document_geometry.get(id) else {
                    continue;
                };
                // Inset only admitted current regular PNG/JPEG content. Other
                // texture previews retain the exact full-envelope fit arithmetic.
                let scale = if self.admitted_image_kind(id, &bound.entry).is_some() {
                    let Some(scale) = Self::image_fit_scale(a, image) else {
                        continue; // honest pack shape and ordinary decorations
                    };
                    scale
                } else {
                    (a.size[0] / image.width as f32).min(a.size[1] / image.height as f32)
                };
                // Same X/Y center, plane depth, source UVs and material as before.
                let [w, h] = [image.width as f32 * scale, image.height as f32 * scale];
                let left = a.at[0] - w * 0.5;
                let top = a.at[1] + (a.size[1] + h) * 0.5;
                let z = a.at[2] + a.size[2] * 0.45;
                let v = |x, y, uv| Vertex {
                    position: [x, y, z],
                    color: [1.0; 3],
                    uv,
                    texture: *material,
                };
                let tl = v(left, top, [0.0, 0.0]);
                let tr = v(left + w, top, [1.0, 0.0]);
                let br = v(left + w, top - h, [1.0, 1.0]);
                let bl = v(left, top - h, [0.0, 1.0]);
                replacements.push((range.clone(), vec![tl, tr, br, tl, br, bl]));
            }
            for (id, model) in &self.folder_models {
                if !view.bindings.contains_key(id) {
                    continue;
                }
                let Some(a) = self
                    .host
                    .anchors
                    .iter()
                    .find(|a| &a.id == id && a.role == "document")
                else {
                    continue;
                };
                let Some(range) = self.document_geometry.get(id) else {
                    continue;
                };
                let Some(materials) = self.folder_model_materials.get(id) else {
                    continue; // only a jointly admitted complete mapping can draw
                };
                let extent: [f32; 3] = std::array::from_fn(|i| model.max[i] - model.min[i]);
                let scale = (0..3)
                    .filter(|i| extent[*i] > 0.0)
                    .map(|i| a.size[i] / extent[i])
                    .fold(f32::INFINITY, f32::min);
                if !scale.is_finite() || scale <= 0.0 {
                    continue; // fully degenerate geometry keeps pack fallback
                }
                let center = [a.at[0], a.at[1] + a.size[1] * 0.5, a.at[2]];
                let vertices = model
                    .vertices
                    .iter()
                    .map(|v| Vertex {
                        position: std::array::from_fn(|i| {
                            center[i]
                                + (v.position[i] - (model.min[i] + model.max[i]) * 0.5) * scale
                        }),
                        texture: usize::try_from(v.texture)
                            .ok()
                            .and_then(|index| materials.get(index))
                            .copied()
                            .unwrap_or(0),
                        ..*v // preserve decoder lit colors and source UVs exactly
                    })
                    .collect::<Vec<_>>();
                replacements.push((range.clone(), vertices));
            }
            // Reverse offsets preserve every vertex outside the replaced source
            // ranges. Only admitted model/video/image status decorations above move;
            // desks, other media/host markers and selection remain unchanged.
            replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
            for (range, vertices) in replacements {
                out.splice(range, vertices);
            }
        }
        if let Some(view) = &world.folder_view {
            if let Some(id) = &view.parent
                && let Some(a) = self.host.anchors.iter().find(|a| &a.id == id)
            {
                out.extend(self.label_for(crate::folder_scene::PARENT_LABEL, a, 32, 0.0));
            }
            for (id, action) in &view.continuations {
                if action.page(view).is_some()
                    && let Some(a) = self.host.anchors.iter().find(|a| &a.id == id)
                {
                    out.extend(self.label_for(&action.label(view), a, 40, 0.0));
                }
            }
            for (id, bound) in &view.bindings {
                if let Some(a) = self.host.anchors.iter().find(|a| &a.id == id) {
                    let name = crate::folder_scene::display(&bound.entry.name);
                    if self.folder_models.contains_key(id) {
                        // Move the admitted-model labels beside the silhouette,
                        // below the opaque top HUD and right of the tree on the
                        // ordinary approach. Keep the existing bitmap cell size,
                        // literal filename and independently placed status marker.
                        let at = [
                            a.at[0] + 1.0,
                            a.at[1] + a.size[1] - 0.2,
                            a.at[2] - a.size[2] * 0.5 - 0.35,
                        ];
                        out.extend(crate::text::object_label(&name, at, 25));
                        out.extend(crate::text::object_label(
                            "GLB static mesh",
                            [at[0], at[1] - 0.14, at[2]],
                            25,
                        ));
                    } else if a.role == "document" && self.has_video_still(id, &bound.entry) {
                        // Reuse the static-model strategy's rear depth, but a
                        // nearer-to-center column fits a full 25-cell name at
                        // the small approach view without shrinking its glyphs.
                        // Public kind/limitations remain in the I reader.
                        let at = [
                            a.at[0] + 0.65,
                            a.at[1] + a.size[1] - 0.2,
                            a.at[2] - a.size[2] * 0.5 - 0.35,
                        ];
                        out.extend(crate::text::object_label(&name, at, 25));
                        out.extend(crate::text::object_label(
                            "MP4 still",
                            [at[0], at[1] - 0.14, at[2]],
                            25,
                        ));
                    } else if a.role == "document"
                        && let Some((kind, at)) = self.image_placard(id, &bound.entry)
                    {
                        // One source-plane group above the actual image, with
                        // unchanged literal glyph size/budget and public I metadata.
                        // Seven 0.012m rows leave the kind's bottom above the source;
                        // the compact status sits to its right, not at the crosshair.
                        out.extend(crate::text::object_label(&name, at, 25));
                        out.extend(crate::text::object_label(
                            kind,
                            [at[0], at[1] - 0.10, at[2]],
                            25,
                        ));
                    } else {
                        out.extend(self.label_for(&name, a, 25, 0.0));
                        out.extend(self.label_for(
                            crate::folder_scene::kind(&bound.entry),
                            a,
                            28,
                            -0.12,
                        ));
                    }
                }
            }
        }
        // Abstract URL fingerprints are portal appearance, never host solids/routes.
        for a in self
            .host
            .anchors
            .iter()
            .filter(|a| a.binding == "explicit-door")
        {
            if let Some(v) = crate::web::portals::view(world, &a.id) {
                let basis = self.host.portal_basis(&a.id);
                let at = a.at;
                let local = basis.canonical_anchor(a);
                let a = &local;
                let start = out.len();
                let pattern = crate::web::portals::fallback(&v.destination);
                let w = a.size[0] * 0.76;
                let h = a.size[1] * 0.62;
                for (i, color) in pattern.into_iter().enumerate() {
                    let x = (i % 8) as f32;
                    let y = (i / 8) as f32;
                    for (dx, dy) in [
                        (0.0, 0.0),
                        (1.0, 0.0),
                        (1.0, 1.0),
                        (0.0, 0.0),
                        (1.0, 1.0),
                        (0.0, 1.0),
                    ] {
                        out.push(Vertex {
                            position: [
                                a.at[0] - w * 0.5 + (x + dx) * w / 8.0,
                                a.at[1] + 0.4 + (y + dy) * h / 8.0,
                                a.at[2] + a.size[2] * 0.5 + 0.065,
                            ],
                            color,
                            uv: [0.0; 2],
                            texture: 0,
                        });
                    }
                }
                basis.place(&mut out[start..], at);
            }
        }
        if let Some(target) = &world.interaction
            && world.explicit_target.as_ref() == Some(target)
            && let Some(a) = self.host.anchors.iter().find(|a| a.id == target.anchor)
        {
            let basis = (a.role == "portal").then(|| self.host.portal_basis(&a.id));
            let at = a.at;
            let local = basis.map(|b| b.canonical_anchor(a));
            let a = local.as_ref().unwrap_or(a);
            let start = out.len();
            let (y, z, h) = if a.role == "document" {
                (a.at[1] + a.size[1] + 0.045, a.at[2], 0.03)
            } else {
                (
                    a.at[1] + a.size[1] * 0.5,
                    a.at[2] + a.size[2] * 0.5 + 0.04,
                    0.29,
                )
            };
            for x in [-0.25, 0.25] {
                crate::scene::cuboid(
                    &mut out,
                    [a.at[0] + x, y, z],
                    [0.025, h, 0.24],
                    [0.55, 0.95, 0.65],
                );
            }
            if let Some(basis) = basis {
                basis.place(&mut out[start..], at);
            }
        }
        if let Some(site) = &world.site {
            let image = site.records.get(&world.current).and_then(|links| {
                links.iter().find_map(|l| {
                    if let crate::content::Destination::Image(id) = &l.destination {
                        site.images.get(id)
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = image {
                self.image(&mut out, image)
            }
        }
        out
    }
    fn image(&self, out: &mut Vec<Vertex>, image: &crate::content::Image) {
        let a = self.host.anchor("image");
        let w = image.width.min(64);
        let h = image.height.min(64);
        for y in 0..h {
            for x in 0..w {
                let i = ((y * image.height / h * image.width + x * image.width / w) * 4) as usize;
                let pixel = &image.rgba[i..i + 4];
                let color =
                    std::array::from_fn(|a| pixel[a] as f32 / 255.0 * pixel[3] as f32 / 255.0);
                for (dx, dy) in [
                    (0.0, 0.0),
                    (1.0, 0.0),
                    (1.0, 1.0),
                    (0.0, 0.0),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ] {
                    out.push(Vertex {
                        position: [
                            a.at[0] - a.size[0] * 0.5 + (x as f32 + dx) / w as f32 * a.size[0],
                            a.at[1] + a.size[1] - (y as f32 + dy) / h as f32 * a.size[1],
                            a.at[2] + a.size[2] * 0.5,
                        ],
                        color,
                        uv: [0.0; 2],
                        texture: 0,
                    });
                }
            }
        }
    }
}
