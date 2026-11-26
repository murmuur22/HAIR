//! Host-owned bounded shared space, independent of appearance/content bytes.
use crate::{Result, assets::Vertex, content::read_local};
use serde::Deserialize;
use std::{
    collections::{HashSet, VecDeque},
    path::Path,
};

// The slab is centered on logical floor zero. Decoration art and placement rays
// use its visible top; persisted logical envelopes retain their historic y=0.
pub const FLOOR_THICKNESS: f32 = 0.1;
pub const FLOOR_TOP: f32 = FLOOR_THICKNESS * 0.5;

#[derive(Clone, Debug, PartialEq, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub id: String,
    pub role: String,
    pub at: [f32; 3],
    pub size: [f32; 3],
    pub binding: String,
}
#[derive(Clone, Debug, PartialEq, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    pub at: [f32; 3],
    pub size: [f32; 3],
}
/// Only the three closed host walls can carry a bounded doorway opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Wall {
    Rear,
    Left,
    Right,
}
#[derive(Clone, Debug, PartialEq, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct WallAperture {
    /// Exact host anchor identity, never an appearance or content binding.
    pub anchor: String,
    pub wall: Wall,
    /// Centred on the anchor's wall-tangent coordinate, starting at the floor.
    pub width: f32,
    pub height: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedItem {
    pub position: crate::placement_store::Position,
    pub bounds: Geometry,
}
#[derive(Clone, Debug, PartialEq, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostScene {
    pub version: u32,
    pub name: String,
    pub room: [f32; 4],
    pub spawn: [f32; 2],
    pub height: f32,
    pub anchors: Vec<Anchor>,
    pub geometry: Vec<Geometry>,
    /// Host-only visual/occluding solids above a fixed 2.2m walking clearance.
    /// Absent in legacy scenes; never inferred from model/pack appearance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overhead: Vec<Geometry>,
    /// Explicit host-owned openings; legacy/shared scenes retain all solid walls.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wall_apertures: Vec<WallAperture>,
    /// Host-owned floor platform without enclosing slabs. Legacy scenes default closed.
    /// Doorway basis metadata and floor-edge movement bounds remain authoritative.
    #[serde(default)]
    pub open_platform: bool,
    /// Runtime-only preferences reconciled against a held immediate directory.
    /// Authored host TOML cannot provide file identity or placements.
    #[serde(skip)]
    pub file_placements: std::collections::BTreeMap<String, PlacedItem>,
    /// Exact runtime file bindings; authored TOML cannot make furniture non-solid.
    #[serde(skip)]
    pub file_anchors: HashSet<String>,
    /// Runtime decoration ownership; never a file binding or listing slot.
    #[serde(skip)]
    pub decorations: std::collections::BTreeMap<String, crate::placement_store::Decoration>,
}
/// Proper door-local rotation: tangent X, up Y, inward +Z. Only the host
/// chooses this basis; asset-local yaw/pivot/fit happen before placement.
#[derive(Clone, Copy)]
pub(crate) struct PortalBasis {
    inward: [f32; 2],
}
impl PortalBasis {
    pub(crate) fn canonical_anchor(self, a: &Anchor) -> Anchor {
        let mut local = a.clone();
        local.at = [0.0; 3];
        if self.inward[0] != 0.0 {
            local.size.swap(0, 2);
        }
        local
    }
    pub(crate) fn place(self, vertices: &mut [Vertex], at: [f32; 3]) {
        let [nx, nz] = self.inward;
        for v in vertices {
            let [x, y, z] = v.position;
            v.position = [at[0] + nz * x + nx * z, at[1] + y, at[2] - nx * x + nz * z];
        }
    }
}
impl HostScene {
    pub(crate) fn portal_basis(&self, id: &str) -> PortalBasis {
        PortalBasis {
            inward: self.portal_inward(id),
        }
    }
    pub fn shared_default() -> Self {
        Self::parse(include_str!("../host-scene.toml")).expect("validated bundled shared default")
    }
    /// Required bundle/development resource. Absence is damage, not site intent.
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("host-scene.toml");
        let bytes = read_local(root, "host-scene.toml", 32768)
            .map_err(|e| format!("host scene {}: {e}", path.display()))?;
        let scene = Self::parse(
            std::str::from_utf8(&bytes).map_err(|e| format!("host scene UTF-8: {e}"))?,
        )?;
        println!(
            "HOST_SCENE={} path={} anchors={:?}",
            scene.name,
            path.display(),
            scene.anchors
        );
        Ok(scene)
    }
    /// Only a genuinely absent host in an explicitly selected external site defaults.
    pub fn load_site(root: &Path) -> Result<Self> {
        let path = root.join("host-scene.toml");
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                println!(
                    "HOST_SCENE_DEFAULT deterministic shared default (no host-scene.toml); independent of appearance"
                );
                Ok(Self::shared_default())
            }
            Err(e) => Err(format!("host scene {}: {e}", path.display())),
            Ok(_) => Self::load(root),
        }
    }
    pub fn parse(text: &str) -> Result<Self> {
        Self::parse_profile(text, false)
    }
    /// Only deterministic generated web rooms may use the larger bounded profile.
    /// Authored/local host files remain restricted to versions 1 and 2.
    pub(crate) fn parse_generated(text: &str) -> Result<Self> {
        Self::parse_profile(text, true)
    }
    fn parse_profile(text: &str, generated: bool) -> Result<Self> {
        if text.len() > if generated { 65536 } else { 32768 } {
            return Err("host scene encoded byte budget exceeded".into());
        }
        let s: Self = toml::from_str(text).map_err(|e| format!("host scene TOML: {e}"))?;
        if generated && s.version != 3 {
            return Err("generated host requires internal version 3".into());
        }
        if s.open_platform && s.version != 2 {
            return Err("open platform requires host scene version 2".into());
        }
        let finite = |v: f32| v.is_finite() && v.abs() <= if generated { 64.0 } else { 16.0 };
        if !(matches!(s.version, 1 | 2) || (generated && s.version == 3))
            || s.name.is_empty()
            || s.name.len() > 80
            || s.anchors.len() > if generated { 80 } else { 64 }
            || s.geometry.len().saturating_add(s.overhead.len()) > 64
            || !s.room.iter().chain(s.spawn.iter()).all(|v| finite(*v))
            || !s.height.is_finite()
            || !(1.0..=4.0).contains(&s.height)
            || s.room[2] - s.room[0] < 4.0
            || s.room[3] - s.room[1] < 4.0
        {
            return Err("host scene version/name/count/finite room bounds invalid".into());
        }
        let valid_box = |at: [f32; 3], size: [f32; 3]| {
            at.iter().all(|v| finite(*v))
                && size.iter().all(|v| v.is_finite() && *v > 0.0 && *v <= 8.0)
                && at[1] >= 0.0
                && at[1] + size[1] <= s.height
                && (0..3).all(|i| at[i].abs() + size[i] <= if generated { 64.0 } else { 16.0 })
        };
        let mut ids = HashSet::new();
        let mut roles = HashSet::new();
        for a in &s.anchors {
            if matches!(s.version, 2 | 3) {
                roles.insert(a.role.as_str());
            }
            if a.id.is_empty()
                || a.id.len() > 48
                || !a.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                || !ids.insert(&a.id)
                || (s.version == 1 && !roles.insert(a.role.as_str()))
                || !valid_box(a.at, a.size)
                || !(matches!(
                    (a.role.as_str(), a.binding.as_str()),
                    ("image", "current-image") | ("decoration", "none")
                ) || (s.version == 1
                    && matches!(
                        (a.role.as_str(), a.binding.as_str()),
                        ("document", "current-document") | ("portal", "selected-target")
                    ))
                    || (matches!(s.version, 2 | 3)
                        && matches!(
                            (a.role.as_str(), a.binding.as_str()),
                            ("document", "explicit-page") | ("portal", "explicit-door")
                        )))
            {
                return Err(
                    "host scene invalid/duplicate anchor identity, role, binding or envelope"
                        .into(),
                );
            }
            if matches!(a.role.as_str(), "portal" | "image")
                && intersects(rect(a.at, a.size, 0.0), s.room)
            {
                return Err("host scene portal/image envelope intrudes into shared navigation; place beyond room bounds".into());
            }
            if matches!(a.role.as_str(), "document" | "decoration") {
                let b = rect(a.at, a.size, 0.22);
                if b[0] < s.room[0] || b[1] < s.room[1] || b[2] > s.room[2] || b[3] > s.room[3] {
                    return Err("host scene object envelope/clearance leaves room".into());
                }
            }
        }
        if !["document", "portal", "image"]
            .iter()
            .all(|r| roles.contains(r))
            || s.geometry.iter().any(|g| !valid_box(g.at, g.size))
        {
            return Err(
                "host scene requires document/portal/image bindings and valid static geometry"
                    .into(),
            );
        }
        if s.overhead.iter().any(|g| {
            !valid_box(g.at, g.size)
                || g.at[1] < 2.2
                || s.anchors.iter().any(|a| {
                    intersects(rect(a.at, a.size, 0.0), rect(g.at, g.size, 0.0))
                        && a.at[1] < g.at[1] + g.size[1]
                        && g.at[1] < a.at[1] + a.size[1]
                })
        }) {
            return Err(
                "host overhead requires valid boxes, >=2.2m clearance and no anchor overlap".into(),
            );
        }
        if matches!(s.version, 2 | 3) {
            let boxes: Vec<_> = s
                .anchors
                .iter()
                .map(|a| (a.at, a.size))
                .chain(s.geometry.iter().map(|g| (g.at, g.size)))
                .collect();
            for (i, (at, size)) in boxes.iter().take(s.anchors.len()).enumerate() {
                if boxes.iter().skip(i + 1).any(|(b, t)| {
                    intersects(rect(*at, *size, 0.0), rect(*b, *t, 0.0))
                        && at[1] < b[1] + t[1]
                        && b[1] < at[1] + size[1]
                }) {
                    return Err("host envelope/static geometry overlap".into());
                }
            }
            for role in ["document", "portal", "decoration", "image"] {
                let n = s.anchors.iter().filter(|a| a.role == role).count();
                if n > if role == "image" {
                    1
                } else if generated && role == "portal" {
                    67
                } else {
                    16
                } {
                    return Err("explicit anchor role multiplicity exceeded".into());
                }
            }
        }
        s.check_wall_apertures()?;
        s.check_connections()?;
        Ok(s)
    }
    fn check_wall_apertures(&self) -> Result<()> {
        let generated = self.version == 3;
        if self.wall_apertures.len() > if generated { 64 } else { 3 }
            || (!self.wall_apertures.is_empty() && !matches!(self.version, 2 | 3))
        {
            return Err("host wall apertures require version 2 and at most three openings (internal generated profile capped at 64)".into());
        }
        let [left, front, right, rear] = self.room;
        let mut walls = HashSet::new();
        let mut anchors = HashSet::new();
        let mut total_width = 0.0;
        for opening in &self.wall_apertures {
            let Some(anchor) = self.anchors.iter().find(|a| a.id == opening.anchor) else {
                return Err("host wall aperture references an absent anchor".into());
            };
            if (!generated && !walls.insert(opening.wall))
                || !anchors.insert(&opening.anchor)
                || anchor.role != "portal"
                || anchor.binding != "explicit-door"
                || anchor.at[1].abs() > 0.0001
            {
                return Err(
                    "host wall aperture requires a unique wall and floor-level explicit portal"
                        .into(),
                );
            }
            let (tangent, normal, low, high, outward) = match opening.wall {
                Wall::Rear => (0, 2, left, right, anchor.at[2] - rear),
                Wall::Left => (2, 0, front, rear, left - anchor.at[0]),
                Wall::Right => (2, 0, front, rear, anchor.at[0] - right),
            };
            // Legacy wall slabs occupy boundary+0.1 .. boundary+0.3 outward.
            // The portal must clear that slab by 0..0.2m, not float far away or
            // intrude into navigation. Tangent alignment follows its exact centre.
            let near_face = outward - anchor.size[normal] * 0.5;
            let half = opening.width * 0.5;
            if !opening.width.is_finite()
                || !opening.height.is_finite()
                || !(1.0..=2.4).contains(&opening.width)
                || !(1.8..=2.4).contains(&opening.height)
                || opening.width < anchor.size[tangent] + 0.1
                || opening.width > anchor.size[tangent] + 0.4
                || opening.height < anchor.size[1] + 0.1
                || opening.height > anchor.size[1] + 0.4
                || opening.height > self.height - 0.2
                || anchor.size[normal] > 0.8
                || !(0.2999..=0.5001).contains(&near_face)
                || anchor.at[tangent] - half < low + 0.4
                || anchor.at[tangent] + half > high - 0.4
                || opening.width > (high - low) * 0.25
            {
                return Err("host wall aperture envelope/alignment/opening size invalid".into());
            }
            total_width += opening.width;
        }
        if total_width > if generated { 108.0 } else { 6.0 } {
            return Err("host wall aperture total opening width exceeds 6m budget (internal generated profile capped at 108m)".into());
        }
        if generated {
            for wall in [Wall::Rear, Wall::Left, Wall::Right] {
                let tangent = if wall == Wall::Rear { 0 } else { 2 };
                let mut intervals: Vec<_> = self
                    .wall_apertures
                    .iter()
                    .filter(|o| o.wall == wall)
                    .map(|o| {
                        let a = self.anchors.iter().find(|a| a.id == o.anchor).unwrap();
                        [a.at[tangent] - o.width * 0.5, a.at[tangent] + o.width * 0.5]
                    })
                    .collect();
                intervals.sort_by(|a, b| a[0].total_cmp(&b[0]));
                let width: f32 = intervals.iter().map(|v| v[1] - v[0]).sum();
                let span = if wall == Wall::Rear {
                    right - left
                } else {
                    rear - front
                };
                if width > span * 0.75 || intervals.windows(2).any(|v| v[1][0] - v[0][1] < 0.59) {
                    return Err("generated wall openings overlap, lack jamb clearance or exceed wall coverage".into());
                }
            }
        }
        Ok(())
    }
    /// Host-owned inward XZ direction of an explicit doorway. The open front
    /// remains the legacy default; appearance packs cannot change this basis.
    pub fn portal_inward(&self, id: &str) -> [f32; 2] {
        match self
            .wall_apertures
            .iter()
            .find(|a| a.anchor == id)
            .map(|a| a.wall)
        {
            Some(Wall::Rear) => [0.0, -1.0],
            Some(Wall::Left) => [1.0, 0.0],
            Some(Wall::Right) => [-1.0, 0.0],
            None => [0.0, 1.0],
        }
    }
    pub fn anchor(&self, role: &str) -> &Anchor {
        self.anchors
            .iter()
            .find(|a| a.role == role)
            .expect("validated required host anchor")
    }
    pub fn target_anchor(&self, a: &Anchor) -> Anchor {
        let mut target = a.clone();
        if let Some(p) = self.file_placements.get(&a.id)
            && let Ok(b) = p.position.file_bounds() {
            target.at = b.at;
            target.size = b.size;
        }
        target
    }
    /// Explicitly enabled placement bases only. Unplaced listing envelopes keep
    /// their document-role visuals but never acquire collision ownership.
    pub fn file_bases(&self) -> Vec<(String, Geometry)> {
        self.anchors.iter().filter(|a| self.file_anchors.contains(&a.id)).filter_map(|a| {
            let p = self.file_placements.get(&a.id)?;
            p.position.base_bounds().ok().flatten().map(|g| (a.id.clone(), g))
        }).collect()
    }
    pub fn solid_file_bases(&self) -> Vec<Geometry> {
        self.file_bases().into_iter().filter(|(id, _)| self.file_placements.get(id)
            .is_some_and(|p| p.position.properties.base_collision)).map(|(_, g)| g).collect()
    }
    /// Visibility/overlap envelopes, shared with runtime aim. Collision OFF
    /// permits walking through decor, not looking through its visible volume.
    pub fn decoration_bounds(&self) -> impl Iterator<Item = Geometry> + '_ {
        self.decorations.values().filter_map(|d| d.bounds().ok())
    }
    pub fn solid_decorations(&self) -> impl Iterator<Item = Geometry> + '_ {
        self.decorations.values().filter(|d| d.collision).filter_map(|d| d.bounds().ok())
    }
    /// Generated/saved files match the player's non-solid target policy,
    /// regardless of overlap preference. Authored envelopes retain their legacy
    /// conservative navigation bounds; placement admission has separate checks.
    pub fn blocked(&self, p: [f32; 2]) -> bool {
        p.iter().any(|v| !v.is_finite())
            || p[0] < self.room[0]
            || p[1] < self.room[1]
            || p[0] > self.room[2]
            || p[1] > self.room[3]
            || self
                .anchors
                .iter()
                .filter(|a| {
                    matches!(a.role.as_str(), "document" | "decoration")
                        && !self.file_anchors.contains(&a.id)
                })
                .any(|a| inside(p, rect(a.at, a.size, 0.22)))
            || self
                .geometry
                .iter()
                .any(|g| inside(p, rect(g.at, g.size, 0.22)))
            || self.solid_file_bases().iter().any(|g| inside(p, rect(g.at, g.size, 0.22)))
            || self.solid_decorations().any(|g| inside(p, rect(g.at, g.size, 0.22)))
    }
    fn check_connections(&self) -> Result<()> {
        self.check_routes(&[])
    }
    pub fn check_routes(&self, arrivals: &[[f32; 2]]) -> Result<()> {
        if self.blocked(self.spawn) {
            return Err("host scene spawn collides or leaves room".into());
        }
        let mut seen = HashSet::from([[0_i32, 0_i32]]);
        let mut q = VecDeque::from([[0_i32, 0_i32]]);
        let goals: Vec<_> = self
            .anchors
            .iter()
            .filter(|a| matches!(a.role.as_str(), "document" | "portal"))
            .map(|a| self.target_anchor(a))
            .collect();
        let walls = self.walls();
        let mut reached = vec![false; goals.len()];
        let mut arrived = vec![false; arrivals.len()];
        while let Some(c) = q.pop_front() {
            if seen.len() > if self.version == 3 { 32768 } else { 17000 } {
                return Err("host scene connection work budget exceeded".into());
            }
            let p = [
                self.spawn[0] + c[0] as f32 * 0.25,
                self.spawn[1] + c[1] as f32 * 0.25,
            ];
            for (i, a) in goals.iter().enumerate() {
                if (p[0] - a.at[0]).hypot(p[1] - a.at[2]) < 1.4 {
                    let eye = glam::Vec3::new(p[0], crate::player::eye_offset(self), p[1]);
                    let centre = glam::Vec3::from_array(a.at) + glam::Vec3::Y * a.size[1] * 0.5;
                    if self.version == 1
                        || !(self
                            .geometry
                            .iter()
                            .chain(self.overhead.iter())
                            .chain(walls.iter())
                            .any(|g| {
                                crate::interaction::segment_hits_box(eye, centre, g.at, g.size)
                            })
                            // Sight uses the same envelopes and segment policy
                            // as runtime aim; only blocked() filters for solidity.
                            || self.decoration_bounds().any(|g| {
                                crate::interaction::segment_hits_box(eye, centre, g.at, g.size)
                            })
                            || self
                                .anchors
                                .iter()
                                .filter(|b| {
                                    b.id != a.id
                                        && matches!(b.role.as_str(), "document" | "decoration")
                                        && !self.file_placements.contains_key(&b.id)
                                })
                                .any(|b| {
                                    crate::interaction::segment_hits_box(eye, centre, b.at, b.size)
                                }))
                    {
                        reached[i] = true;
                    }
                }
            }
            for (i, a) in arrivals.iter().enumerate() {
                if (p[0] - a[0]).hypot(p[1] - a[1]) < 0.18 {
                    arrived[i] = true;
                }
            }
            for d in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
                let n = [c[0] + d[0], c[1] + d[1]];
                let at = [
                    self.spawn[0] + n[0] as f32 * 0.25,
                    self.spawn[1] + n[1] as f32 * 0.25,
                ];
                if !self.blocked(at) && seen.insert(n) {
                    q.push_back(n)
                }
            }
        }
        if reached.iter().chain(arrived.iter()).any(|v| !*v) {
            return Err("host scene document/portal unreachable from spawn".into());
        }
        Ok(())
    }
    /// Authoritative wall solids for drawing, targeting and route validation.
    /// Openings split a slab into two full-height jamb solids and one lintel;
    /// without metadata the three legacy slabs are returned unchanged.
    /// An explicit open platform keeps its floor and bounds but has no wall solids.
    pub fn walls(&self) -> Vec<Geometry> {
        if self.open_platform {
            return Vec::new();
        }
        if self.version == 3 {
            let mut base = self.clone();
            base.version = 2;
            base.wall_apertures.clear();
            let mut out = Vec::new();
            for (wall, slab) in [Wall::Rear, Wall::Left, Wall::Right]
                .into_iter()
                .zip(base.walls())
            {
                let tangent = if wall == Wall::Rear { 0 } else { 2 };
                let mut openings: Vec<_> = self
                    .wall_apertures
                    .iter()
                    .filter(|o| o.wall == wall)
                    .collect();
                openings.sort_by(|a, b| {
                    let centre = |o: &WallAperture| {
                        self.anchors
                            .iter()
                            .find(|x| x.id == o.anchor)
                            .map_or(0.0, |x| x.at[tangent])
                    };
                    centre(a).total_cmp(&centre(b))
                });
                let mut cursor = slab.at[tangent] - slab.size[tangent] * 0.5;
                let end = slab.at[tangent] + slab.size[tangent] * 0.5;
                for opening in openings {
                    let anchor = self
                        .anchors
                        .iter()
                        .find(|a| a.id == opening.anchor)
                        .expect("validated generated opening");
                    let low = anchor.at[tangent] - opening.width * 0.5;
                    let high = anchor.at[tangent] + opening.width * 0.5;
                    if low > cursor {
                        let mut jamb = slab.clone();
                        jamb.at[tangent] = (cursor + low) * 0.5;
                        jamb.size[tangent] = low - cursor;
                        out.push(jamb);
                    }
                    let mut lintel = slab.clone();
                    lintel.at[tangent] = anchor.at[tangent];
                    lintel.at[1] = opening.height;
                    lintel.size[tangent] = opening.width;
                    lintel.size[1] = self.height - opening.height;
                    out.push(lintel);
                    cursor = high;
                }
                if end > cursor {
                    let mut jamb = slab;
                    jamb.at[tangent] = (cursor + end) * 0.5;
                    jamb.size[tangent] = end - cursor;
                    out.push(jamb);
                }
            }
            return out;
        }
        let [x0, z0, x1, z1] = self.room;
        let slabs = [
            (
                Wall::Rear,
                Geometry {
                    at: [(x0 + x1) * 0.5, 0.0, z1 + 0.2],
                    size: [x1 - x0 + 0.4, self.height, 0.2],
                },
            ),
            (
                Wall::Left,
                Geometry {
                    at: [x0 - 0.2, 0.0, (z0 + z1) * 0.5],
                    size: [0.2, self.height, z1 - z0],
                },
            ),
            (
                Wall::Right,
                Geometry {
                    at: [x1 + 0.2, 0.0, (z0 + z1) * 0.5],
                    size: [0.2, self.height, z1 - z0],
                },
            ),
        ];
        let mut out = Vec::with_capacity(3 + self.wall_apertures.len() * 2);
        for (wall, slab) in slabs {
            let opening = self.wall_apertures.iter().find(|a| a.wall == wall);
            let Some((opening, anchor)) = opening.and_then(|opening| {
                self.anchors
                    .iter()
                    .find(|a| a.id == opening.anchor)
                    .map(|anchor| (opening, anchor))
            }) else {
                out.push(slab);
                continue;
            };
            let tangent = if wall == Wall::Rear { 0 } else { 2 };
            let start = slab.at[tangent] - slab.size[tangent] * 0.5;
            let end = slab.at[tangent] + slab.size[tangent] * 0.5;
            let low = anchor.at[tangent] - opening.width * 0.5;
            let high = anchor.at[tangent] + opening.width * 0.5;
            for (from, to) in [(start, low), (high, end)] {
                let mut jamb = slab.clone();
                jamb.at[tangent] = (from + to) * 0.5;
                jamb.size[tangent] = to - from;
                out.push(jamb);
            }
            let mut lintel = slab;
            lintel.at[tangent] = anchor.at[tangent];
            lintel.at[1] = opening.height;
            lintel.size[tangent] = opening.width;
            lintel.size[1] = self.height - opening.height;
            out.push(lintel);
        }
        out
    }
    pub fn vertices(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        let [x0, z0, x1, z1] = self.room;
        crate::scene::cuboid(
            &mut out,
            [(x0 + x1) * 0.5, 0.0, (z0 + z1) * 0.5],
            [x1 - x0 + 0.4, FLOOR_THICKNESS, z1 - z0 + 0.4],
            [0.22, 0.24, 0.21],
        );
        for g in self.walls() {
            let color = if g.at[2] > z1 {
                [0.50, 0.52, 0.46]
            } else {
                [0.46, 0.49, 0.43]
            };
            crate::scene::cuboid(
                &mut out,
                [g.at[0], g.at[1] + g.size[1] * 0.5, g.at[2]],
                g.size,
                color,
            );
        }
        for g in self.geometry.iter().chain(self.overhead.iter()) {
            crate::scene::cuboid(
                &mut out,
                [g.at[0], g.at[1] + g.size[1] * 0.5, g.at[2]],
                g.size,
                [0.35, 0.40, 0.34],
            )
        }
        out
    }
}
fn rect(at: [f32; 3], size: [f32; 3], pad: f32) -> [f32; 4] {
    [
        at[0] - size[0] * 0.5 - pad,
        at[2] - size[2] * 0.5 - pad,
        at[0] + size[0] * 0.5 + pad,
        at[2] + size[2] * 0.5 + pad,
    ]
}
fn inside(p: [f32; 2], b: [f32; 4]) -> bool {
    p[0] > b[0] && p[1] > b[1] && p[0] < b[2] && p[1] < b[3]
}
fn intersects(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}
#[cfg(test)]
#[path = "host_scene_r9_tests.rs"]
mod r9_tests;
#[cfg(test)]
mod generated_profile_contract {
    use super::*;
    fn dense() -> HostScene {
        let html = (0..64)
            .map(|i| format!("<a href='/target-{i}'>Link</a>"))
            .collect::<String>();
        crate::web::Page::html(
            crate::web::Address::parse("https://test.example/").unwrap(),
            html.as_bytes(),
        )
        .unwrap()
        .generate()
        .unwrap()
        .host
    }
    #[test]
    fn generated_profile_refuses_oversize_nonfinite_overlap_and_duplicate_openings() {
        let good = dense();
        let encode = |s: &HostScene| toml::to_string(s).unwrap();
        assert_eq!(HostScene::parse_generated(&encode(&good)).unwrap(), good);
        assert!(HostScene::parse(&encode(&good)).is_err());
        let mut authored = good.clone();
        authored.version = 2;
        assert!(HostScene::parse(&encode(&authored)).is_err());
        for mutation in 0..7 {
            let mut bad = good.clone();
            match mutation {
                0 => bad.room[3] = 65.0,
                1 => bad.anchors[0].at[0] = f32::NAN,
                2 => bad.anchors[1].at = bad.anchors[0].at,
                3 => bad.wall_apertures.push(bad.wall_apertures[0].clone()),
                4 => bad.anchors[0].binding = "execute-script".into(),
                5 => bad.spawn = [bad.room[2] + 1.0, 0.0],
                _ => bad.anchors[0].size[0] = 9.0,
            }
            assert!(
                HostScene::parse_generated(&encode(&bad)).is_err(),
                "mutation {mutation}"
            );
        }
        assert!(HostScene::parse_generated(&" ".repeat(65_537)).is_err());
    }
}
