//! Bounded host-authored local spaces; page IDs are exact loaded content IDs.
use crate::{
    Pose, Result,
    content::{Site, read_local},
    host_scene::HostScene,
};
use serde::Deserialize;
use std::{
    collections::{HashSet, VecDeque},
    path::Path,
};
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spaces {
    pub version: u32,
    pub name: String,
    pub entry_space: String,
    pub entry_page: String,
    pub entry_arrival: String,
    pub spaces: Vec<Space>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub pages: Vec<String>,
    pub default_page: String,
    pub default_arrival: String,
    pub scene: HostScene,
    pub arrivals: Vec<Arrival>,
    pub documents: Vec<Document>,
    pub doors: Vec<Door>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Arrival {
    pub id: String,
    pub at: [f32; 2],
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub door: String,
}
impl Arrival {
    pub fn pose(&self) -> Pose {
        Pose {
            position: self.at,
            yaw: self.yaw_degrees.to_radians(),
            pitch: self.pitch_degrees.to_radians(),
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub anchor: String,
    pub page: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Door {
    pub anchor: String,
    pub space: String,
    pub page: String,
    pub arrival: String,
}
fn id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 48 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
impl Spaces {
    pub fn load(root: &Path, site: &Site) -> Result<Option<Self>> {
        match std::fs::symlink_metadata(root.join("host-scene.toml")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
            Ok(_) => {}
        }
        let bytes = read_local(root, "host-scene.toml", 131072)?;
        let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
        let header: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
        if header.get("version").and_then(|v| v.as_integer()) == Some(1) {
            HostScene::parse(text)?;
            return Ok(None);
        }
        Ok(Some(Self::parse(text, site)?))
    }
    pub fn parse(text: &str, site: &Site) -> Result<Self> {
        if text.len() > 131072 {
            return Err("spaces exceeds 128KiB".into());
        }
        let graph: Self = toml::from_str(text).map_err(|e| format!("explicit host TOML: {e}"))?;
        graph.validate(site)?;
        Ok(graph)
    }
    pub fn space(&self, id: &str) -> Result<&Space> {
        self.spaces
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| format!("missing space {id}"))
    }
    pub fn owner(&self, page: &str) -> Result<&Space> {
        self.spaces
            .iter()
            .find(|s| s.pages.iter().any(|p| p == page))
            .ok_or_else(|| format!("no declared space for {page}"))
    }
    pub fn destination(
        &self,
        space: &str,
        page: &str,
        arrival: &str,
        site: &Site,
    ) -> Result<(&Space, Pose)> {
        let s = self.space(space)?;
        if !site.pages.contains_key(page) || !s.pages.iter().any(|p| p == page) {
            return Err("destination page ownership invalid".into());
        }
        let a = s
            .arrivals
            .iter()
            .find(|a| a.id == arrival)
            .ok_or("missing arrival")?;
        if s.scene.blocked(a.at) {
            return Err("arrival blocked".into());
        }
        Ok((s, a.pose()))
    }
    pub fn validate(&self, site: &Site) -> Result<()> {
        if self.version != 2
            || self.name.is_empty()
            || self.name.len() > 80
            || self.spaces.is_empty()
            || self.spaces.len() > 8
        {
            return Err("unsupported spaces version/name/count".into());
        }
        let mut ids = HashSet::new();
        let mut owned = HashSet::new();
        for s in &self.spaces {
            if !id(&s.id)
                || !ids.insert(&s.id)
                || s.name.is_empty()
                || s.name.len() > 80
                || s.pages.is_empty()
                || s.pages.len() > 32
                || s.arrivals.is_empty()
                || s.arrivals.len() > 16
                || s.documents.is_empty()
                || s.documents.len() > 16
                || s.doors.is_empty()
                || s.doors.len() > 16
                || s.scene.version != 2
            {
                return Err("space identity/count/scene version invalid".into());
            }
            // Reuse exact envelope and finite/bounded host validation, without user file writes.
            let value = toml::to_string(&s.scene).map_err(|e| e.to_string())?;
            HostScene::parse(&value)?;
            for p in &s.pages {
                if !site.pages.contains_key(p) || !owned.insert(p) {
                    return Err("page unknown or owned by multiple spaces".into());
                }
            }
            if !s.pages.contains(&s.default_page)
                || !s.arrivals.iter().any(|a| a.id == s.default_arrival)
            {
                return Err("missing space default page/arrival".into());
            }
            let mut bound = HashSet::new();
            for d in &s.documents {
                if !s.pages.contains(&d.page)
                    || !bound.insert(&d.anchor)
                    || !s
                        .scene
                        .anchors
                        .iter()
                        .any(|a| a.id == d.anchor && a.role == "document")
                {
                    return Err("invalid/duplicate document binding".into());
                }
            }
            if s.pages
                .iter()
                .any(|p| !s.documents.iter().any(|d| &d.page == p))
            {
                return Err("page has no document marker".into());
            }
            for d in &s.doors {
                if !bound.insert(&d.anchor)
                    || !s
                        .scene
                        .anchors
                        .iter()
                        .any(|a| a.id == d.anchor && a.role == "portal")
                {
                    return Err("invalid/duplicate door binding".into());
                }
            }
            if s.scene
                .anchors
                .iter()
                .filter(|a| matches!(a.role.as_str(), "document" | "portal"))
                .any(|a| !bound.contains(&a.id))
            {
                return Err("unbound host interaction".into());
            }
            let mut arrivals = HashSet::new();
            s.scene
                .check_routes(&s.arrivals.iter().map(|a| a.at).collect::<Vec<_>>())?;
            for a in &s.arrivals {
                if !id(&a.id)
                    || !arrivals.insert(&a.id)
                    || a.at.iter().any(|v| !v.is_finite())
                    || !a.yaw_degrees.is_finite()
                    || a.yaw_degrees.abs() > 180.0
                    || !a.pitch_degrees.is_finite()
                    || a.pitch_degrees.abs() > 60.0
                    || s.scene.blocked(a.at)
                {
                    return Err("arrival identity/finite/collision invalid".into());
                }
                let door = s
                    .scene
                    .anchors
                    .iter()
                    .find(|d| d.id == a.door && d.role == "portal")
                    .ok_or("arrival has no connecting door")?;
                let dx = a.at[0] - door.at[0];
                let dz = a.at[1] - door.at[2];
                let yaw = a.yaw_degrees.to_radians();
                if dx.hypot(dz) >= 1.7
                    || dx.hypot(dz) < 0.3
                    || yaw.sin() * dx - yaw.cos() * dz < 0.25
                {
                    return Err("arrival must be near connecting door and face inward".into());
                }
            }
        }
        if owned.len() != site.pages.len() {
            return Err("all loaded pages require explicit single ownership".into());
        }
        self.destination(
            &self.entry_space,
            &self.entry_page,
            &self.entry_arrival,
            site,
        )?;
        for s in &self.spaces {
            for d in &s.doors {
                self.destination(&d.space, &d.page, &d.arrival, site)?;
            }
        }
        // Both directed reachability directions: each bounded space participates in return loop.
        for reverse in [false, true] {
            let mut seen = HashSet::from([self.entry_space.as_str()]);
            let mut q = VecDeque::from([self.entry_space.as_str()]);
            while let Some(at) = q.pop_front() {
                for s in &self.spaces {
                    for d in &s.doors {
                        let (from, to) = if reverse {
                            (d.space.as_str(), s.id.as_str())
                        } else {
                            (s.id.as_str(), d.space.as_str())
                        };
                        if from == at && seen.insert(to) {
                            q.push_back(to)
                        }
                    }
                }
            }
            if seen.len() != self.spaces.len() {
                return Err("space dead end or disconnected return route".into());
            }
        }
        Ok(())
    }
}
