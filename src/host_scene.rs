//! Host-owned bounded shared space, independent of appearance/content bytes.
use crate::{Result, assets::Vertex, content::read_local};
use serde::Deserialize;
use std::{
    collections::{HashSet, VecDeque},
    path::Path,
};

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
        if text.len() > 32768 {
            return Err("host scene exceeds 32768 bytes".into());
        }
        let s: Self = toml::from_str(text).map_err(|e| format!("host scene TOML: {e}"))?;
        if s.open_platform && s.version != 2 {
            return Err("open platform requires host scene version 2".into());
        }
        let finite = |v: f32| v.is_finite() && v.abs() <= 16.0;
        if !matches!(s.version, 1 | 2)
            || s.name.is_empty()
            || s.name.len() > 80
            || s.anchors.len() > 64
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
                && (0..3).all(|i| at[i].abs() + size[i] <= 16.0)
        };
        let mut ids = HashSet::new();
        let mut roles = HashSet::new();
        for a in &s.anchors {
            if s.version == 2 {
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
                    || (s.version == 2
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
        if s.version == 2 {
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
                if n > if role == "image" { 1 } else { 16 } {
                    return Err("explicit anchor role multiplicity exceeded".into());
                }
            }
        }
        s.check_wall_apertures()?;
        s.check_connections()?;
        Ok(s)
    }
    fn check_wall_apertures(&self) -> Result<()> {
        if self.wall_apertures.len() > 3 || (!self.wall_apertures.is_empty() && self.version != 2) {
            return Err("host wall apertures require version 2 and at most three openings".into());
        }
        let [left, front, right, rear] = self.room;
        let mut walls = HashSet::new();
        let mut anchors = HashSet::new();
        let mut total_width = 0.0;
        for opening in &self.wall_apertures {
            let Some(anchor) = self.anchors.iter().find(|a| a.id == opening.anchor) else {
                return Err("host wall aperture references an absent anchor".into());
            };
            if !walls.insert(opening.wall)
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
        if total_width > 6.0 {
            return Err("host wall aperture total opening width exceeds 6m budget".into());
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
    pub fn blocked(&self, p: [f32; 2]) -> bool {
        p.iter().any(|v| !v.is_finite())
            || p[0] < self.room[0]
            || p[1] < self.room[1]
            || p[0] > self.room[2]
            || p[1] > self.room[3]
            || self
                .anchors
                .iter()
                .filter(|a| matches!(a.role.as_str(), "document" | "decoration"))
                .any(|a| inside(p, rect(a.at, a.size, 0.22)))
            || self
                .geometry
                .iter()
                .any(|g| inside(p, rect(g.at, g.size, 0.22)))
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
            .collect();
        let walls = self.walls();
        let mut reached = vec![false; goals.len()];
        let mut arrived = vec![false; arrivals.len()];
        while let Some(c) = q.pop_front() {
            if seen.len() > 17000 {
                return Err("host scene connection work budget exceeded".into());
            }
            let p = [
                self.spawn[0] + c[0] as f32 * 0.25,
                self.spawn[1] + c[1] as f32 * 0.25,
            ];
            for (i, a) in goals.iter().enumerate() {
                if (p[0] - a.at[0]).hypot(p[1] - a.at[2]) < 1.4 {
                    let eye = glam::Vec3::new(p[0], 1.65, p[1]);
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
                            || self
                                .anchors
                                .iter()
                                .filter(|b| {
                                    b.id != a.id
                                        && matches!(b.role.as_str(), "document" | "decoration")
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
            [x1 - x0 + 0.4, 0.1, z1 - z0 + 0.4],
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
