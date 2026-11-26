//! Shared appearance-catalog vocabulary for visitor and future Workshop/layout consumers.
//! No placement, source-file identity, publication format or persistence authority lives here.
use crate::{Result, assets::Model};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};

pub const MAX_ENTRIES: usize = 32;
pub const MAX_ASSETS: usize = 3;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub assets: Vec<Asset>,
    pub entries: Vec<Entry>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    /// Pack-local asset identity, independent of its relative file path.
    pub id: String,
    pub file: String,
    pub max_extent: f32,
    pub scale: f32,
    pub yaw_degrees: f32,
    pub pivot: [f32; 3],
}
impl Asset {
    pub(crate) fn validate_fit(&self) -> Result<()> {
        if !self.max_extent.is_finite() || self.max_extent <= 0. || self.max_extent > 10.
            || !self.scale.is_finite() || !(0.1..=4.).contains(&self.scale)
            || !self.yaw_degrees.is_finite() || !(-180. ..=180.).contains(&self.yaw_degrees)
            || !self.pivot.iter().all(|v| v.is_finite() && v.abs() <= 3.) {
            return Err("Invalid pinned asset fit".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Pack-local choice identity. Never use an array index as saved identity.
    pub id: String,
    /// Stable semantic registry identity, shared across themes (not an asset path).
    pub tag: String,
    pub asset: String,
    pub name: String,
    #[serde(default)]
    pub defaults: Defaults,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    /// Suggested decoration-only collision, not host/file collision authority.
    pub collision: bool,
}
fn token(value: &str, max: usize, dotted: bool) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || (dotted && b == b'.')
        })
        && !value.ends_with(['.', '-'])
        && !value.contains("..")
}
/// Shared semantic-tag grammar, including persisted Follow-theme records.
pub fn valid_tag(value: &str) -> bool {
    token(value, 96, true)
}
impl Spec {
    pub fn validate(&self) -> Result<()> {
        if self.assets.len() > MAX_ASSETS || self.entries.len() > MAX_ENTRIES {
            return Err("decoration catalog exceeds 3 assets / 32 entries".into());
        }
        let mut assets = BTreeSet::new();
        for a in &self.assets {
            if !token(&a.id, 48, false) || !assets.insert(a.id.as_str()) {
                return Err("invalid/duplicate decoration asset ID".into());
            }
            // File, fit, encoded/decoded profile and payload budgets are checked by
            // the SAME acquisition path as appearance roles, not a second loader.
        }
        let mut ids = BTreeSet::new();
        let mut tags = BTreeSet::new();
        for e in &self.entries {
            if !token(&e.id, 48, false)
                || !ids.insert(e.id.as_str())
                || !valid_tag(&e.tag)
                || !tags.insert(e.tag.as_str())
                || e.name.trim().is_empty()
                || e.name.chars().count() > 40
                || e.name.chars().any(char::is_control)
                || !assets.contains(e.asset.as_str())
            {
                return Err(
                    "invalid/duplicate decoration entry ID/tag/name or missing asset reference"
                        .into(),
                );
            }
        }
        if self
            .assets
            .iter()
            .any(|a| !self.entries.iter().any(|e| e.asset == a.id))
        {
            return Err("unreferenced decoration asset".into());
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct Catalog {
    pub entries: Vec<Entry>,
    pub assets: Vec<LoadedAsset>,
}
#[derive(Clone)]
pub struct LoadedAsset {
    pub spec: Asset,
    pub model: Arc<Model>,
    // Exact bytes from the safe loader, not a path or reconstructed model.
    pub(crate) encoded: Arc<[u8]>,
}
impl Catalog {
    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }
    pub fn asset(&self, id: &str) -> Option<&LoadedAsset> {
        self.assets.iter().find(|a| a.spec.id == id)
    }
}
impl LoadedAsset {
    /// Art fits the persisted envelope; no visual dimensions become physics.
    pub fn placed(&self, d: &crate::placement_store::Decoration) -> Result<Vec<crate::assets::Vertex>> {
        d.validate()?;
        let turn = glam::Mat3::from_rotation_y(self.spec.yaw_degrees.to_radians());
        let model: Vec<_> = self.model.vertices.iter().map(|v| crate::assets::Vertex {
            position: (turn * glam::Vec3::from_array(v.position)).to_array(),
            ..*v
        }).collect();
        d.fit_artwork(&model)
    }
}
