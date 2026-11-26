//! Strict data-only owned GLB closure. A digest is identity, never authentication.
use crate::{Result, decoration::{Asset, Entry, LoadedAsset}};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub(crate) const GLB_BYTES: u64 = 8_000_000;
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn valid_digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Descriptor {
    version: u32,
    kind: String,
    pub source_manifest: String,
    pub asset: Asset,
    pub sha256: String,
    pub bytes: u64,
}
impl Descriptor {
    pub fn new(manifest: String, mut asset: Asset, bytes: &[u8]) -> Result<Self> {
        asset.file = "asset.glb".into();
        let d = Self { version: 1, kind: "embedded-glb-closure".into(), source_manifest: manifest,
            asset, sha256: digest(bytes), bytes: bytes.len() as u64 };
        d.validate()?;
        Ok(d)
    }
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 || self.kind != "embedded-glb-closure"
            || !valid_digest(&self.source_manifest) || !valid_digest(&self.sha256)
            || self.bytes == 0 || self.bytes > GLB_BYTES || self.asset.file != "asset.glb"
        { return Err("Invalid pinned closure descriptor".into()); }
        self.asset.validate_fit()?;
        // Validate the catalog-local asset identity without granting path authority.
        let spec = crate::decoration::Spec { assets: vec![self.asset.clone()], entries: vec![Entry {
            id: "check".into(), tag: "hair.check".into(), asset: self.asset.id.clone(), name: "Check".into(), defaults: Default::default(),
        }] };
        spec.validate()
    }
    pub fn text(&self) -> Result<String> { toml::to_string(self).map_err(|e| e.to_string()) }
    pub fn identity(&self) -> Result<String> { Ok(digest(self.text()?.as_bytes())) }
    pub fn load(&self, bytes: Arc<[u8]>) -> Result<Arc<LoadedAsset>> {
        self.validate()?;
        if self.bytes != bytes.len() as u64 || self.sha256 != digest(&bytes) {
            return Err("Pinned closure bytes/digest mismatch".into());
        }
        let model = Arc::new(crate::assets::decode(&bytes, self.asset.max_extent)?);
        Ok(Arc::new(LoadedAsset { spec: self.asset.clone(), model, encoded: bytes }))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PinRef {
    pub closure: String,
    pub entry: Entry,
    #[serde(skip)]
    pub loaded: Option<Arc<LoadedAsset>>,
}
impl std::fmt::Debug for PinRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinRef").field("closure", &self.closure).field("entry", &self.entry).finish()
    }
}
impl PartialEq for PinRef {
    fn eq(&self, other: &Self) -> bool { self.closure == other.closure && self.entry == other.entry }
}
impl PinRef {
    pub fn validate(&self) -> Result<()> {
        if !valid_digest(&self.closure) { return Err("Invalid pinned closure identity".into()); }
        let a = Asset { id: self.entry.asset.clone(), file: "asset.glb".into(), max_extent: 1., scale: 1., yaw_degrees: 0., pivot: [0.; 3] };
        crate::decoration::Spec { assets: vec![a], entries: vec![self.entry.clone()] }.validate()
    }
}
/// Checked explicit source selection. Fields are not construction authority for callers.
/// Its held no-follow source must remain the same inode and bytes until commit.
pub struct CheckedPin {
    pub(crate) descriptor: Descriptor,
    pub(crate) entry: Entry,
    pub(crate) loaded: Arc<LoadedAsset>,
    #[cfg(target_os = "macos")]
    pub(crate) source: super::mac::PinSource,
}
impl CheckedPin {
    pub fn closure_identity(&self) -> Result<String> { self.descriptor.identity() }
    pub fn source_digest(&self) -> &str { &self.descriptor.sha256 }
    pub fn asset_identity(&self) -> &str { &self.descriptor.asset.id }
    pub fn entry(&self) -> &Entry { &self.entry }
    /// Data-only candidate for explicit selection preview/geometry validation.
    /// This grants no persistent activation; checked publication is still required.
    pub fn decoration(&self, placement: &super::Decoration) -> Result<super::Decoration> {
        placement.pin(self)
    }
}
pub(crate) const MAX_PIN_CLOSURES: usize = 3;
const MAX_COMBINED_MODELS: usize = 6;

/// Partitioned model counts, but ONE shared logical resource budget, including
/// warm models, unmatched assets and distinct duplicate identities. Not an RSS bound.
#[derive(Clone, Copy, Default)]
pub(crate) struct Budget {
    pub theme_models: usize,
    pub pin_models: usize,
    pub encoded: usize,
    pub vertices: usize,
    pub textures: usize,
    pub texture_bytes: usize,
    pub extra_retained: usize,
}
impl Budget {
    /// Pack snapshot plus unique closure identities, never content-hash sharing.
    /// Resolve EVERY reference before deduplication so an unacquired duplicate
    /// cannot borrow another placement's authority. No reads or body decoding.
    pub fn with_pins<'a>(mut self, decorations: impl IntoIterator<Item = &'a super::Decoration>) -> Result<Self> {
        self.validate()?;
        let mut ids = std::collections::BTreeSet::new();
        let empty = crate::decoration::Catalog::default();
        for d in decorations {
            if d.is_pinned() {
                let (_, asset) = d.resolve(&empty)?;
                if ids.insert(d.closure_identity()) { self.charge_pin(asset)?; }
            }
        }
        Ok(self)
    }
    pub fn charge_pin(&mut self, asset: &LoadedAsset) -> Result<()> {
        self.pin_models = self.pin_models.checked_add(1).ok_or("Pin model count overflow")?;
        self.encoded = self.encoded.checked_add(asset.encoded.len()).ok_or("Encoded budget overflow")?;
        self.vertices = self.vertices.checked_add(asset.model.vertices.len()).ok_or("Vertex budget overflow")?;
        self.textures = self.textures.checked_add(asset.model.textures.len()).ok_or("Texture count overflow")?;
        for t in &asset.model.textures {
            self.texture_bytes = self.texture_bytes.checked_add(t.rgba.len()).ok_or("Texture budget overflow")?;
        }
        self.validate()
    }
    pub fn validate(&self) -> Result<()> {
        let retained = self.vertices.checked_mul(std::mem::size_of::<crate::assets::Vertex>())
            .and_then(|n| n.checked_add(self.texture_bytes)).and_then(|n| n.checked_add(self.encoded))
            .and_then(|n| n.checked_add(self.extra_retained)).ok_or("Retained budget overflow")?;
        let models = self.theme_models.checked_add(self.pin_models).ok_or("Combined model count overflow")?;
        if self.theme_models > crate::decoration::MAX_ASSETS || self.pin_models > MAX_PIN_CLOSURES
            || models > MAX_COMBINED_MODELS || self.encoded > 24_000_000 || self.vertices > 300_000
            || self.textures > 64 || self.texture_bytes > 32_000_000 || retained > 64_000_000 {
            return Err("aggregate whole-pack budget: <=3 theme model sources, <=3 distinct pin closures, <=6 combined models; shared totals <=24 MB encoded, 300000 decoded vertices, 32 MB/64 textures, 64 MB retained payload".into());
        }
        Ok(())
    }
}
