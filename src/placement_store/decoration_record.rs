//! Independent owned decoration identity/envelope. V1 Follow-theme bytes remain
//! unchanged; v2 pinned references require a strictly acquired owned GLB closure.
use crate::{Result, decoration::{Catalog, Entry, LoadedAsset}};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decoration {
    version: u32,
    kind: String,
    pub placement_id: String,
    pub tag: String,
    pub at: [f32; 2],
    pub yaw: f32,
    /// Saved logical envelope, independent of the current theme mesh/defaults.
    pub size: [f32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scaling: Option<crate::placement::scaling::Scaling>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<crate::placement::animation::Mode>,
    pub collision: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) pinned: Option<super::pinned::PinRef>,
}
impl Decoration {
    /// Relocation changes only pose/yaw/collision, never semantic identity or size.
    pub fn validate_relocation(&self, proposed: &Self) -> Result<()> {
        self.validate()?;
        proposed.validate()?;
        if self.placement_id != proposed.placement_id || self.tag != proposed.tag || self.size != proposed.size || self.scaling != proposed.scaling || self.animation != proposed.animation || self.pinned != proposed.pinned {
            return Err("Decoration relocation must preserve placement identity, tag and logical size".into());
        }
        if self == proposed { return Err("Decoration relocation is unchanged".into()); }
        Ok(())
    }
    pub fn scale_percent(&self) -> u16 { self.scaling.map_or(100, |s| s.percent) }
    pub fn animation_mode(&self) -> crate::placement::animation::Mode { self.animation.unwrap_or_default() }
    /// New animation intent is document-only, including requests to clear a
    /// historical mode. Saved records use validate(), never this mutation gate.
    pub fn validate_animation(&self, _proposed: &Self) -> Result<()> {
        Err("Animation changes are document-only; saved decoration animation is preserved".into())
    }
    /// Read-only classification of already-authored historical pending evidence.
    /// Not new intent or publication authority; preserve the original allowlist.
    pub(super) fn validate_legacy_animation(&self, proposed: &Self) -> Result<()> {
        self.validate()?; proposed.validate()?;
        if self.animation == proposed.animation { return Err("Animation intent requires an explicit mode change".into()); }
        let mut normalized = proposed.clone(); normalized.animation = self.animation;
        if normalized == *self { return Ok(()); }
        if self.size != normalized.size || self.scaling != normalized.scaling { self.validate_resize(&normalized) }
        else { self.validate_relocation(&normalized) }
    }
    pub fn reference_size(&self) -> [f32; 3] { self.scaling.map_or(self.size, |s| s.reference) }
    pub fn set_scale(&mut self, percent: u16) -> Result<()> {
        self.validate()?;
        let scaling = crate::placement::scaling::Scaling::new(self.reference_size(), percent)?;
        self.size = scaling.size()?;
        self.scaling = Some(scaling);
        Ok(())
    }
    /// Explicit bounded resize; compare all fields except permitted pose/collision
    /// and derived proportional size/percentage, never arbitrary record editing.
    pub fn validate_resize(&self, proposed: &Self) -> Result<()> {
        self.validate()?; proposed.validate()?;
        let scale = proposed.scaling.ok_or("Resize requires explicit proportional metadata")?;
        if scale.reference != self.reference_size() { return Err("Resize must retain original dimensions".into()); }
        let mut allowed = self.clone();
        allowed.at = proposed.at; allowed.yaw = proposed.yaw; allowed.collision = proposed.collision;
        allowed.set_scale(scale.percent)?;
        if allowed != *proposed { return Err("Resize must preserve exact decoration identity and source".into()); }
        if self == proposed { return Err("Decoration resize is unchanged".into()); }
        Ok(())
    }
    /// A fresh preview identity; this does not read or write layout storage.
    pub fn from_entry(entry: &Entry, at: [f32; 2], yaw: f32, size: [f32; 3]) -> Result<Self> {
        #[cfg(not(target_os = "macos"))]
        { let _ = (entry, at, yaw, size); Err("Decoration identity requires macOS".into()) }
        #[cfg(target_os = "macos")]
        {
            let mut id = [0u8; 16];
            if unsafe { libc::getentropy(id.as_mut_ptr().cast(), id.len()) } != 0 {
                return Err(format!("Generate decoration identity: {}", std::io::Error::last_os_error()));
            }
            let record = Self {
                version: 1,
                kind: "decoration-follow-theme".into(),
                placement_id: id.iter().map(|b| format!("{b:02x}")).collect(),
                tag: entry.tag.clone(), at, yaw, size, collision: entry.defaults.collision,
                pinned: None, scaling: None, animation: None,
            };
            record.validate()?;
            Ok(record)
        }
    }
    pub fn validate(&self) -> Result<()> {
        if let Some(s) = self.scaling { s.validate_size(self.size)?; }
        let appearance_valid = match &self.pinned {
            None => self.version == 1 && self.kind == "decoration-follow-theme",
            Some(pin) => {
                pin.validate()?;
                self.version == 2 && self.kind == "decoration-pinned" && pin.entry.tag == self.tag
            }
        };
        if !appearance_valid
            || !valid_id(&self.placement_id) || !crate::decoration::valid_tag(&self.tag)
            || !self.at.iter().all(|v| v.is_finite() && v.abs() <= 10000.)
            || !self.yaw.is_finite() || self.yaw.abs() > std::f32::consts::TAU
            || !self.size.iter().all(|v| v.is_finite() && *v > 0. && *v <= 16.)
        { return Err(if self.pinned.is_none() { "Invalid Follow-theme decoration record" } else { "Invalid pinned decoration kind/schema/geometry" }.into()); }
        Ok(())
    }
    pub fn is_pinned(&self) -> bool { self.pinned.is_some() }
    pub fn closure_identity(&self) -> Option<&str> { self.pinned.as_ref().map(|p| p.closure.as_str()) }
    pub(crate) fn pin(&self, source: &super::CheckedPin) -> Result<Self> {
        self.validate()?;
        if self.animation.is_some() { return Err("New decorations require absent animation; saved legacy modes are read-only".into()); }
        if self.is_pinned() || self.tag != source.entry.tag { return Err("Pin selection/tag mismatch".into()); }
        let mut d = self.clone();
        d.version = 2;
        d.kind = "decoration-pinned".into();
        d.pinned = Some(super::pinned::PinRef { closure: source.closure_identity()?, entry: source.entry.clone(), loaded: Some(source.loaded.clone()) });
        d.validate()?;
        Ok(d)
    }
    pub fn bounds(&self) -> Result<crate::host_scene::Geometry> {
        self.validate()?;
        let (s, c) = self.yaw.sin_cos();
        Ok(crate::host_scene::Geometry {
            at: [self.at[0], 0., self.at[1]],
            size: [c.abs() * self.size[0] + s.abs() * self.size[2], self.size[1],
                s.abs() * self.size[0] + c.abs() * self.size[2]],
        })
    }
    pub fn envelope_mesh(&self) -> Result<Vec<crate::assets::Vertex>> {
        let b = self.bounds()?;
        let mut mesh = vec![];
        crate::scene::cuboid(&mut mesh, [b.at[0], b.at[1] + b.size[1] * 0.5, b.at[2]], b.size, [1.; 3]);
        Ok(mesh)
    }
    /// Tiny historical envelopes have no positive space above the slab. Keep
    /// their original art/envelope, not an invented collidable above-floor proxy.
    pub fn floor_fit_disclosure(&self) -> Option<&'static str> {
        (self.size[1] <= crate::host_scene::FLOOR_TOP).then_some(
            "Above-floor fit unavailable: saved height <= 0.05m; historical art retained inside original envelope, floor-occluded; no above-floor support geometry")
    }
    fn display_bottom(&self) -> f32 {
        if self.floor_fit_disclosure().is_some() { 0. } else { crate::host_scene::FLOOR_TOP }
    }
    /// Uniform display fit only; logical bounds remain the saved envelope.
    /// Validate source triangles at the existing maximum permitted envelope
    /// BEFORE applying tiny destination sizes. Unit-scale validation would reject
    /// thin art that formerly passed in a larger valid saved envelope.
    pub(crate) fn fit_artwork(&self, model: &[crate::assets::Vertex]) -> Result<Vec<crate::assets::Vertex>> {
        self.validate()?;
        let mut mesh = crate::placement::grounded(model, [0.; 2], 0., [16.; 3])?;
        let bottom = self.display_bottom();
        let available = [self.size[0], self.size[1] - bottom, self.size[2]];
        let extent: [f64; 3] = std::array::from_fn(|axis| {
            let lo = mesh.iter().map(|v| f64::from(v.position[axis])).fold(f64::INFINITY, f64::min);
            let hi = mesh.iter().map(|v| f64::from(v.position[axis])).fold(f64::NEG_INFINITY, f64::max);
            hi - lo
        });
        let scale = extent.into_iter().zip(available).filter(|(e, _)| *e > 0.)
            .map(|(e, bound)| f64::from(bound) / e).fold(f64::INFINITY, f64::min);
        if !scale.is_finite() || scale <= 0. { return Err("Decoration uniform fit refused".into()); }
        let (s, c) = f64::from(self.yaw).sin_cos();
        for v in &mut mesh {
            let [x, y, z] = v.position.map(|p| f64::from(p) * scale);
            v.position = [(c * x + s * z + f64::from(self.at[0])) as f32,
                (y + f64::from(bottom)) as f32,
                (-s * x + c * z + f64::from(self.at[1])) as f32];
            if !v.position.iter().all(|p| p.is_finite()) { return Err("Decoration fit overflow".into()); }
        }
        Ok(mesh)
    }
    /// Exact semantic lookup, never theme-list order or an arbitrary substitute.
    pub fn resolve<'a>(&'a self, catalog: &'a Catalog) -> Result<(&'a Entry, &'a LoadedAsset)> {
        self.artwork(catalog)?.ok_or_else(|| format!("Theme is missing decoration tag {}; saved record retained", self.tag))
    }
    /// Only a valid absent exact Follow tag is procedural. Invalid/ambiguous
    /// records, broken catalog references and unacquired pins remain errors.
    pub fn artwork<'a>(&'a self, catalog: &'a Catalog) -> Result<Option<(&'a Entry, &'a LoadedAsset)>> {
        self.validate()?;
        if let Some(pin) = &self.pinned {
            let asset = pin.loaded.as_deref().ok_or("Pinned closure not acquired from owned storage; no theme fallback")?;
            if asset.spec.id != pin.entry.asset { return Err("Pinned appearance asset identity mismatch".into()); }
            return Ok(Some((&pin.entry, asset)));
        }
        let mut matches = catalog.entries.iter().filter(|e| e.tag == self.tag);
        let Some(entry) = matches.next() else { return Ok(None); };
        if matches.next().is_some() { return Err("Ambiguous theme decoration tag".into()); }
        let asset = catalog.asset(&entry.asset).ok_or("Theme decoration asset missing")?;
        Ok(Some((entry, asset)))
    }
    /// Shared render/review policy, not a catalog choice or retained GLB source.
    pub fn artwork_mesh(&self, catalog: &Catalog) -> Result<Vec<crate::assets::Vertex>> {
        if let Some((_, asset)) = self.artwork(catalog)? { return asset.placed(self); }
        let mut mesh = vec![];
        let bottom = self.display_bottom();
        let scale = f64::from(self.size[1] - bottom) / f64::from(self.size[1]);
        let size = self.size.map(|n| (f64::from(n) * scale) as f32);
        crate::scene::cuboid(&mut mesh, [0., size[1] * 0.5, 0.], size, [0.5; 3]);
        let turn = glam::Mat3::from_rotation_y(self.yaw);
        let b = self.bounds()?;
        for v in &mut mesh {
            v.position = (turn * glam::Vec3::from_array(v.position) + glam::Vec3::new(self.at[0], bottom, self.at[1])).to_array();
        }
        let factor = self.scaling.map_or(1.,|s|s.factor());
        let mut y = b.at[1] + self.size[1] + (0.22 + (self.tag.len().div_ceil(32) + 1) as f32 * 0.12) * factor;
        let mut label = |text: &str| {
            let at = [b.at[0], y, b.at[2] + b.size[2] * 0.5 + 0.02 * factor];
            let mut label = crate::text::object_label(text, [0.;3], 32);
            for v in &mut label { v.position = std::array::from_fn(|i|at[i]+v.position[i]*factor); }
            mesh.extend(label);
            y -= 0.12 * factor;
        };
        label("Missing artwork");
        // Exact tag grammar is ASCII, bounded to 96 bytes. No truncation of ID.
        for chunk in self.tag.as_bytes().chunks(32) { label(std::str::from_utf8(chunk).unwrap()); }
        label(&self.placement_id);
        Ok(mesh)
    }
    pub fn artwork_mesh_at(&self, catalog: &Catalog, seconds: f64) -> Result<Vec<crate::assets::Vertex>> {
        let art_len = self.artwork(catalog)?.map_or(36, |(_,a)|a.model.vertices.len());
        let mut mesh = self.artwork_mesh(catalog)?;
        self.present_artwork(&mut mesh, art_len, seconds)?;
        Ok(mesh)
    }
    /// Fit into an inscribed cylinder before rotating, never widen the saved
    /// collider/envelope on redraw. Missing-art captions translate but never spin.
    pub fn present_artwork(&self, mesh: &mut [crate::assets::Vertex], art_len: usize, seconds: f64) -> Result<()> {
        self.validate()?;
        let mode = self.animation_mode();
        if mode == crate::placement::animation::Mode::None { return Ok(()); }
        if art_len > mesh.len() { return Err("Decoration artwork range changed".into()); }
        let bottom = self.display_bottom();
        let available = self.size[1] - bottom;
        let amplitude = mode.amplitude(available);
        let offset = mode.offset(seconds) * amplitude / 0.024;
        let unturn = glam::Mat3::from_rotation_y(-self.yaw);
        let origin = glam::Vec3::new(self.at[0],bottom,self.at[1]);
        let mut factor = (available-amplitude) / available;
        if mode == crate::placement::animation::Mode::BreatheRotate {
            let radius = mesh[..art_len].iter().map(|v| {
                let p=unturn*(glam::Vec3::from_array(v.position)-origin); p.x.hypot(p.z)
            }).fold(0.,f32::max);
            if radius > 0. { factor=factor.min(self.size[0].min(self.size[2])*0.4999/radius); }
        }
        let turn = glam::Mat3::from_rotation_y(self.yaw + mode.angle(seconds));
        for v in &mut mesh[..art_len] {
            let p = turn * (unturn * (glam::Vec3::from_array(v.position)-origin) * factor) + origin + glam::Vec3::Y*offset;
            if !p.is_finite() { return Err("Decoration presentation overflow".into()); }
            v.position=p.to_array();
        }
        for v in &mut mesh[art_len..] { v.position[1]+=offset; }
        Ok(())
    }
}
pub(super) fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
