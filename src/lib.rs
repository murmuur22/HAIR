// Domain behavior remains independent of windows and GPU.
pub mod exploration;
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};
pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub version: u32,
    pub assets: Vec<Asset>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub role: String,
    pub file: String,
    pub footprint: [u32; 2],
    pub max_extent: f32,
}
pub fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty()
        || relative.contains(['\\', ':'])
        || !Path::new(relative)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(format!("unsafe relative path: {relative}"));
    }
    let base = root.canonicalize().map_err(|e| e.to_string())?;
    let target = base
        .join(relative)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !target.starts_with(&base) {
        return Err("path escapes pack (symlink)".into());
    }
    Ok(target)
}
impl Pack {
    pub fn parse(text: &str) -> Result<Self> {
        let pack: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        if pack.version != 1 || pack.assets.is_empty() || pack.assets.len() > 32 {
            return Err("unsupported pack version or size".into());
        }
        let mut roles = std::collections::HashSet::new();
        for a in &pack.assets {
            if ![
                "desk", "chair", "screen", "keyboard", "mouse", "door", "wall", "portal", "floor",
                "lamp",
            ]
            .contains(&a.role.as_str())
                || !roles.insert(&a.role)
            {
                return Err("invalid or duplicate role".into());
            }
            if a.file.contains(['\\', ':'])
                || !Path::new(&a.file)
                    .components()
                    .all(|c| matches!(c, Component::Normal(_)))
                || !a.file.ends_with(".glb")
            {
                return Err("unsafe GLB path".into());
            }
            if a.footprint.iter().any(|n| *n == 0 || *n > 16)
                || !a.max_extent.is_finite()
                || a.max_extent <= 0.0
                || a.max_extent > 10.0
            {
                return Err("invalid geometry constraints".into());
            }
        }
        Ok(pack)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub id: String,
    pub file: String,
    pub next: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub version: u32,
    pub start: String,
    pub pages: Vec<Page>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub page: String,
    pub document_role: String,
    pub document: String,
    pub portal_role: String,
    pub destination: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub version: u32,
    pub grid: f32,
    pub bindings: Vec<Binding>,
}
#[derive(Clone, Copy, Debug)]
pub enum Action {
    Inspect,
    Navigate,
    Close,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReaderSelection {
    pub page: String,
    pub ordinal: usize,
    pub link: content::Link,
}
pub struct World {
    pub folder_view: Option<folder_scene::View>,
    pub graph: Graph,
    pub plan: Plan,
    pub current: String,
    pub reading: bool,
    pub pocket: Option<web::Pocket>,
    documents: std::collections::HashMap<String, String>,
    pub site: Option<content::Site>,
    pub selected: usize,
    pub interaction: Option<interaction::Target>,
    pub explicit_target: Option<interaction::Target>,
    pub doorway_choices: std::collections::BTreeMap<String, web::portals::Choice>,
    pub reader_link: Option<ReaderSelection>,
    pub navigation: navigation::Navigation,
    pub tree_collapsed: bool,
    pub spaces: Option<spaces::Spaces>,
    pub current_space: String,
}
pub mod folder_scene;
impl World {
    /// In-memory folder presentation only: no Site::load, crawl or file bodies.
    pub fn folder_room(source: String, navigation: navigation::Navigation) -> Self {
        Self {
            folder_view: None,
            graph: Graph {
                version: 1,
                start: source.clone(),
                pages: Vec::new(),
            },
            plan: Plan {
                version: 1,
                grid: 0.5,
                bindings: Vec::new(),
            },
            current: source,
            reading: false,
            pocket: None,
            documents: Default::default(),
            site: None,
            selected: 0,
            interaction: None,
            explicit_target: None,
            doorway_choices: Default::default(),
            reader_link: None,
            navigation,
            tree_collapsed: false,
            spaces: None,
            current_space: String::new(),
        }
    }
    pub fn load_site(root: &Path) -> Result<Self> {
        let site = content::Site::load(root)?;
        let spaces = spaces::Spaces::load(root, &site).map_err(|e| format!("host scene: {e}"))?;
        let current_space = spaces
            .as_ref()
            .map(|s| s.entry_space.clone())
            .unwrap_or_default();
        let start = spaces
            .as_ref()
            .map(|s| s.entry_page.clone())
            .unwrap_or_else(|| site.start.clone());
        let pages: Vec<Page> = site
            .pages
            .keys()
            .map(|id| Page {
                id: id.clone(),
                file: id.clone(),
                next: site.links[id]
                    .first()
                    .cloned()
                    .unwrap_or_else(|| site.start.clone()),
            })
            .collect();
        let bindings = pages
            .iter()
            .map(|p| Binding {
                page: p.id.clone(),
                document_role: "desk".into(),
                document: p.id.clone(),
                portal_role: "portal".into(),
                destination: p.next.clone(),
            })
            .collect();
        Ok(Self {
            folder_view: None,
            navigation: navigation::Navigation::load(
                root,
                &pages.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
            )?,
            tree_collapsed: false,
            spaces,
            current_space,
            current: start.clone(),
            graph: Graph {
                version: 1,
                start,
                pages,
            },
            plan: Plan {
                version: 1,
                grid: 0.5,
                bindings,
            },
            reading: false,
            pocket: None,
            documents: Default::default(),
            site: Some(site),
            selected: 0,
            interaction: None,
            explicit_target: None,
            doorway_choices: Default::default(),
            reader_link: None,
        })
    }
    pub fn reader_links(&self) -> Vec<content::Link> {
        if self.folder_view.is_some() {
            return Vec::new();
        }
        if self.reading && self.pocket.as_ref().is_some_and(|p| p.reading.is_some()) {
            return Vec::new();
        }
        if let Some(site) = &self.site {
            return site.records.get(&self.current).cloned().unwrap_or_default();
        }
        let Some(bound) = self.plan.bindings.iter().find(|b| b.page == self.current) else {
            return Vec::new();
        };
        let Some(file) = self
            .graph
            .pages
            .iter()
            .find(|p| p.id == bound.document)
            .map(|p| &p.file)
        else {
            return Vec::new();
        };
        let Ok(links) = content::inline_links(self.document().unwrap_or("")) else {
            return Vec::new();
        };
        links
            .into_iter()
            .map(|(label, raw)| {
                let destination = if raw.starts_with("https://") || raw.starts_with("http://") {
                    content::Destination::External(raw.clone())
                } else if raw.contains(':') {
                    content::Destination::Unsupported(raw.clone())
                } else if raw.is_empty()
                    || raw.len() > 512
                    || raw.contains(['\\', '#', '?', '%'])
                    || raw.starts_with('/')
                    || raw.split('/').any(|p| p == "." || p == "..")
                {
                    content::Destination::Invalid("unsafe/unsupported local path syntax".into())
                } else {
                    let relative = Path::new(file).parent().unwrap_or(Path::new("")).join(&raw);
                    self.graph
                        .pages
                        .iter()
                        .find(|p| Path::new(&p.file) == relative)
                        .map(|p| content::Destination::Page(p.id.clone()))
                        .unwrap_or(content::Destination::Invalid(
                            "missing or unsupported local destination".into(),
                        ))
                };
                content::Link {
                    label,
                    raw,
                    destination,
                }
            })
            .collect()
    }
    pub fn selected_reader_link(&self) -> Option<&ReaderSelection> {
        self.reader_link.as_ref().filter(|s| {
            self.reading
                && s.page == self.current
                && self.reader_links().get(s.ordinal) == Some(&s.link)
        })
    }
    pub fn cycle_reader_link(&mut self, reverse: bool) {
        if !self.reading {
            return;
        }
        let links = self.reader_links();
        let n = links.len();
        let i = self.selected_reader_link().map(|s| s.ordinal);
        self.reader_link = if n == 0 {
            None
        } else {
            let ordinal = match i {
                Some(i) if reverse => (i + n - 1) % n,
                Some(i) => (i + 1) % n,
                None if reverse => n - 1,
                None => 0,
            };
            Some(ReaderSelection {
                page: self.current.clone(),
                ordinal,
                link: links[ordinal].clone(),
            })
        };
    }
    pub fn reader_link_description(&self) -> String {
        if self.folder_view.is_some() {
            return "READ-ONLY | LINKS INACTIVE | EXACT ESCAPED SOURCE ABOVE (UP/DOWN)".into();
        }
        if self.reading
            && self
                .pocket
                .as_ref()
                .is_some_and(|p| p.collection_recovered && p.reading.is_none())
        {
            return "COLLECTION CHANGED | FULL PAGE RESTORED".into();
        }
        if self.reading && self.pocket.as_ref().is_some_and(|p| p.reading.is_some()) {
            return "SECTION COLLECTION | LINKS ON FULL PAGE OBJECT OR PORTALS".into();
        }
        if self.current.starts_with("https://") {
            return match self.selected_reader_link() {
                Some(s) => match &s.link.destination {
                    content::Destination::External(raw) => {
                        format!("E FOLLOW HTTPS {} | {}", raw, s.link.label)
                    }
                    _ => format!("NOT FOLLOWED | {} | {}", s.link.raw, s.link.label),
                },
                None => "TAB / SHIFT TAB SELECT LABELLED HTTPS LINK | E FOLLOW".into(),
            };
        }
        let Some(s) = self.selected_reader_link() else {
            return if self.reader_links().is_empty() {
                "NO DOCUMENT LINKS".into()
            } else {
                "TAB / SHIFT TAB SELECT LINK | E FOLLOW LOCAL".into()
            };
        };
        let label: String = s
            .link
            .label
            .chars()
            .filter(|c| !c.is_control())
            .take(28)
            .collect();
        let kind = match s.link.destination {
            content::Destination::Page(_) => "E FOLLOW LOCAL",
            content::Destination::External(_) => "EXTERNAL - NOT SUPPORTED",
            content::Destination::Image(_) => "IMAGE - NOT FOLLOWED",
            content::Destination::Unsupported(_) => "UNSUPPORTED - NOT FOLLOWED",
            content::Destination::Invalid(_) => "INVALID - NOT FOLLOWED",
        };
        format!(
            "> LINK {}/{}: {} | {}",
            s.ordinal + 1,
            self.reader_links().len(),
            label,
            kind
        )
    }
    pub fn follow_reader_link(&mut self) {
        let Some(s) = self.selected_reader_link() else {
            return;
        };
        let content::Destination::Page(id) = &s.link.destination else {
            return;
        };
        let exists = if let Some(site) = &self.site {
            site.pages.contains_key(id)
        } else {
            self.plan
                .bindings
                .iter()
                .any(|b| &b.page == id && self.documents.contains_key(&b.document))
        };
        if !exists {
            return;
        }
        self.current = id.clone();
        self.selected = 0;
        self.reading = false;
        self.reader_link = None;
        self.explicit_target = None;
        self.interaction = None;
    }
    pub fn targets(&self) -> Vec<content::Link> {
        use content::{Destination, Link};
        let Some(site) = &self.site else {
            return Vec::new();
        };
        let mut result = site.records.get(&self.current).cloned().unwrap_or_default();
        // Explicit collection index, not fabricated hyperlinks. Every supported page remains reachable.
        for id in site.pages.keys().filter(|id| **id != self.current) {
            if !result
                .iter()
                .any(|l| l.destination == Destination::Page(id.clone()))
            {
                result.push(Link {
                    label: format!("Collection: {id}"),
                    raw: id.clone(),
                    destination: Destination::Page(id.clone()),
                });
            }
        }
        result
    }
    pub fn cycle_target(&mut self) {
        let count = self.targets().len();
        if count > 0 {
            self.selected = (self.selected + 1) % count;
        }
    }
    pub fn return_to_entry(&mut self) {
        self.current = self.graph.start.clone();
        self.selected = 0;
        self.reading = false;
        self.reader_link = None;
        self.explicit_target = None;
        self.interaction = None;
    }
    pub fn target_description(&self) -> String {
        if let Some(view) = &self.folder_view {
            return self
                .interaction
                .as_ref()
                .map(|t| t.prompt.clone())
                .unwrap_or_else(|| view.summary());
        }
        if self.current.starts_with("https://") {
            return "Aim at doorway | Tab/Shift+Tab destination | E follows shown address".into();
        }
        if !self.current_space.is_empty() {
            return self
                .interaction
                .as_ref()
                .map(|t| t.prompt.clone())
                .unwrap_or_else(|| {
                    format!(
                        "SPACE {} | E at named markers | R entry",
                        self.current_space
                    )
                });
        }
        use content::Destination;
        if self.site.is_none() {
            return self
                .graph
                .pages
                .iter()
                .find(|p| p.id == self.current)
                .map(|p| p.next.clone())
                .unwrap_or_default();
        }
        self.targets()
            .get(self.selected)
            .map(|l| {
                format!(
                    "{}: {} [{}]",
                    l.label,
                    l.raw,
                    match &l.destination {
                        Destination::Page(_) => "local page",
                        Destination::Image(_) => "image surface",
                        Destination::External(_) => "HTTP not supported / not followed",
                        Destination::Unsupported(_) => "not supported / not followed",
                        Destination::Invalid(_) => "invalid / not followed",
                    }
                )
            })
            .unwrap_or_else(|| "No destination".into())
    }
    pub fn load(root: &Path) -> Result<Self> {
        let graph = std::fs::read_to_string(safe_path(root, "content/graph.toml")?)
            .map_err(|e| e.to_string())?;
        let plan =
            std::fs::read_to_string(safe_path(root, "scene.toml")?).map_err(|e| e.to_string())?;
        Self::parse(root, &graph, &plan)
    }
    pub fn parse(root: &Path, graph: &str, plan: &str) -> Result<Self> {
        let graph: Graph = toml::from_str(graph).map_err(|e| e.to_string())?;
        let plan: Plan = toml::from_str(plan).map_err(|e| e.to_string())?;
        if graph.version != 1
            || plan.version != 1
            || graph.pages.is_empty()
            || graph.pages.len() > 16
            || plan.grid != 0.5
        {
            return Err("unsupported graph/plan".into());
        }
        let mut documents = std::collections::HashMap::new();
        for page in &graph.pages {
            if page.id.is_empty()
                || !page
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
                || !page.file.ends_with(".md")
            {
                return Err("invalid content identity".into());
            }
            let path = safe_path(&safe_path(root, "content")?, &page.file)?;
            if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 16384 {
                return Err("document too large".into());
            }
            let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            if documents.insert(page.id.clone(), text).is_some() {
                return Err("duplicate content ID".into());
            }
        }
        if !documents.contains_key(&graph.start)
            || graph.pages.iter().any(|p| !documents.contains_key(&p.next))
        {
            return Err("dangling graph link".into());
        }
        let mut bound = std::collections::HashSet::new();
        for b in &plan.bindings {
            let page = graph
                .pages
                .iter()
                .find(|p| p.id == b.page)
                .ok_or("unbound page")?;
            if !bound.insert(&b.page)
                || !documents.contains_key(&b.document)
                || b.destination != page.next
                || b.document_role != "desk"
                || b.portal_role != "portal"
            {
                return Err("invalid presentation binding".into());
            }
        }
        if bound.len() != graph.pages.len() {
            return Err("missing presentation".into());
        }
        Ok(Self {
            folder_view: None,
            navigation: navigation::Navigation::flat(
                &graph.pages.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
            ),
            tree_collapsed: false,
            spaces: None,
            current_space: String::new(),
            current: graph.start.clone(),
            graph,
            plan,
            reading: false,
            pocket: None,
            documents,
            site: None,
            selected: 0,
            interaction: None,
            explicit_target: None,
            doorway_choices: Default::default(),
            reader_link: None,
        })
    }
    pub fn document(&self) -> Result<&str> {
        if let Some(view) = &self.folder_view {
            return Ok(&view.reader);
        }
        if self.reading
            && let Some(pocket) = &self.pocket
            && let Some(anchor) = &pocket.reading
        {
            return pocket
                .documents
                .get(anchor)
                .map(|d| d.text.as_str())
                .ok_or("missing reading collection".into());
        }
        if let Some(site) = &self.site {
            return site
                .pages
                .get(&self.current)
                .map(String::as_str)
                .ok_or("missing site page".into());
        }
        let b = self
            .plan
            .bindings
            .iter()
            .find(|b| b.page == self.current)
            .ok_or("missing scene binding")?;
        self.documents
            .get(&b.document)
            .map(String::as_str)
            .ok_or("missing document".into())
    }
    /// Typed code ranges for exactly the text `document()` returns.
    pub fn document_code(&self) -> &[web::CodeSpan] {
        if self.folder_view.is_some() {
            return &[];
        }
        let Some(pocket) = &self.pocket else {
            return &[];
        };
        if self.reading
            && let Some(anchor) = &pocket.reading
        {
            return pocket
                .documents
                .get(anchor)
                .map_or(&[], |d| d.code.as_slice());
        }
        &pocket.code
    }
    pub fn act(&mut self, action: Action, position: [f32; 2]) {
        self.act_at(action, position, [0.0, 1.0], [0.0, -3.5]);
    }
    pub fn act_at(
        &mut self,
        action: Action,
        position: [f32; 2],
        document: [f32; 2],
        portal: [f32; 2],
    ) {
        if matches!(action, Action::Inspect)
            && let Some(pocket) = &mut self.pocket
            && let Some(target) = &self.interaction
            && target.kind == interaction::Kind::Read
            && target.page == self.current
        {
            pocket.reading = pocket
                .documents
                .contains_key(&target.anchor)
                .then(|| target.anchor.clone());
            pocket.collection_recovered = false;
            self.reading = true;
            self.reader_link = None;
            return;
        }
        let near =
            |target: [f32; 2]| (position[0] - target[0]).hypot(position[1] - target[1]) < 1.7;
        match action {
            Action::Close => {
                self.reading = false;
                self.reader_link = None;
            }
            Action::Inspect if near(document) => {
                if let Some(pocket) = &mut self.pocket {
                    pocket.collection_recovered = false;
                }
                self.reading = true;
            }
            Action::Navigate if !self.reading && near(portal) => {
                if self.site.is_some() {
                    if let Some(content::Link {
                        destination: content::Destination::Page(id),
                        ..
                    }) = self.targets().get(self.selected)
                    {
                        self.current = id.clone();
                        self.selected = 0;
                    }
                } else if let Some(b) = self.plan.bindings.iter().find(|b| b.page == self.current) {
                    self.current = b.destination.clone();
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub position: [f32; 2],
    pub yaw: f32,
    pub pitch: f32,
}
impl Default for Pose {
    fn default() -> Self {
        Self {
            position: [0.0, 3.0],
            yaw: 0.0,
            pitch: -0.12,
        }
    }
}
#[derive(Default)]
pub struct Input {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub running: bool,
    pub captured: bool,
}
impl Input {
    pub fn release(&mut self) {
        *self = Self::default();
    }
    pub fn look(&self, pose: &mut Pose, dx: f64, dy: f64) {
        if self.captured && dx.is_finite() && dy.is_finite() {
            pose.yaw = (pose.yaw + dx as f32 * 0.0025).rem_euclid(std::f32::consts::TAU);
            pose.pitch = (pose.pitch - dy as f32 * 0.0025).clamp(-1.5, 1.5);
        }
    }
}
pub fn locomote(pose: &mut Pose, input: &Input, dt: f32) {
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
    let blocked = |p: [f32; 2]| {
        // Conservative cylinder-expanded boxes for this bounded presentation, not general physics.
        let desk = p[0] > -1.0 && p[0] < 1.0 && p[1] > -0.3 && p[1] < 1.2;
        let chair = p[0] > 1.1 && p[0] < 2.1 && p[1] > 1.0 && p[1] < 2.0;
        desk || chair
    };
    for _ in 0..16 {
        for axis in 0..2 {
            let mut candidate = pose.position;
            candidate[axis] = (candidate[axis] + delta[axis]).clamp(-3.75, 3.75);
            if !blocked(candidate) {
                pose.position = candidate;
            }
        }
    }
}

pub mod acquisition;
pub mod assets;
pub mod authoring;
pub mod content;
pub mod door_trim;
pub mod highlight;
pub mod host_scene;
pub mod interaction;
pub mod local_explorer;
pub mod materials;
pub mod navigation;
pub mod presentation;
pub mod render;
pub mod resources;
pub mod scene;
pub mod sky;
pub mod spaces;
pub mod text;
pub mod theme;
pub mod web;
#[cfg(feature = "workshop")]
pub mod workshop;

#[cfg(test)]
mod code_reader_tests;
#[cfg(test)]
mod panorama_load_profile_tests;
#[cfg(test)]
mod ui_scale_render_tests;
#[cfg(test)]
mod web_context_tests;

#[cfg(test)]
mod tests {
    #[test]
    fn reader_links_resolve_bound_bundled_document_files_not_graph_next() {
        let mut w = super::World::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        w.documents.insert("welcome".into(),"[Actual workshop](workshop.md) [Remote](https://example.invalid/private) [Escape](../private.md)".into());
        w.reading = true;
        w.cycle_reader_link(false);
        assert_eq!(
            w.reader_links().len(),
            3,
            "bundled actual links must be parsed without fabricated graph links"
        );
        assert_eq!(
            w.selected_reader_link().unwrap().link.destination,
            super::content::Destination::Page("workshop".into())
        );
        w.follow_reader_link();
        assert_eq!(w.current, "workshop");
        assert!(!w.reading);
    }
    #[test]
    fn actual_assets_load_with_bounds_materials_and_reject_errors() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/kenney");
        let model = super::assets::load(&root.join("desk.glb"), 2.0).unwrap();
        assert!(model.vertices.len() > 100);
        assert!(model.max[1] > 0.38 && model.max[1] < 0.5);
        assert!(model.vertices.iter().any(|v| v.color[0] > v.color[2]));
        assert!(super::assets::load(&root.join("desk.glb"), 0.1).is_err());
        assert!(super::assets::load(&root.join("absent.glb"), 2.0).is_err());
        assert!(super::assets::decode(b"not glb", 2.0).is_err());
        assert!(super::safe_path(&root, "../kenney/desk.glb").is_err());
    }
    #[test]
    fn movement_is_bounded_and_cannot_tunnel_through_desk() {
        let mut pose = super::Pose::default();
        let mut input = super::Input {
            forward: true,
            captured: true,
            ..Default::default()
        };
        for _ in 0..100 {
            super::locomote(&mut pose, &input, 0.1);
        }
        assert!(
            pose.position[1] >= 1.2 && pose.position[1] < 1.5,
            "desk collision {:?}",
            pose.position
        );
        pose.position = [2.5, 3.0];
        for _ in 0..100 {
            super::locomote(&mut pose, &input, 5.0);
        }
        assert!(pose.position[1] >= -3.75 && pose.position[1] < -3.0);
        input.forward = false;
        input.right = true;
        for _ in 0..100 {
            super::locomote(&mut pose, &input, 0.1);
        }
        assert!(pose.position[0] <= 3.75 && pose.position[0] > 3.0);
    }
    #[test]
    fn input_release_and_mouse_look_are_separate_from_actions() {
        let mut pose = super::Pose::default();
        let mut input = super::Input {
            forward: true,
            captured: true,
            ..Default::default()
        };
        input.look(&mut pose, 50.0, 99999.0);
        assert!(pose.yaw > 0.0 && pose.pitch.abs() <= 1.5);
        input.release();
        assert!(!input.captured && !input.forward);
        let before = pose.position;
        super::locomote(&mut pose, &input, 0.1);
        input.look(&mut pose, 100.0, 1.0);
        assert_eq!(before, pose.position);
        assert!((pose.yaw - 0.125).abs() < 0.001);
    }
    #[test]
    fn content_identity_and_navigation_bindings() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut world = super::World::load(root).unwrap();
        assert_eq!(world.current, "welcome");
        assert!(world.document().unwrap().contains("SAMPLE CONTENT"));
        world.act(super::Action::Inspect, [0.0, 2.0]);
        assert!(world.reading);
        world.act(super::Action::Close, [0.0, 2.0]);
        assert!(!world.reading);
        world.act(super::Action::Navigate, [0.0, 3.5]);
        assert_eq!(world.current, "welcome", "remote doorway must not activate");
        world.act(super::Action::Navigate, [0.0, -3.0]);
        assert_eq!(world.current, "workshop");
        assert!(world.document().unwrap().contains("Workshop"));
        world.act(super::Action::Navigate, [0.0, -3.0]);
        assert_eq!(world.current, "welcome");
        assert!(super::World::parse(root, "version = 1\n", "").is_err());
    }
    #[test]
    fn manifest_loads_and_rejects_invalid_pack_data() {
        let text = "version = 1\n[[assets]]\nrole = 'desk'\nfile = 'desk.glb'\nfootprint = [2,1]\nmax_extent = 3.0\n";
        assert!(super::Pack::parse(text).is_ok());
        for bad in [
            text.replace("version = 1", "version = 9"),
            text.replace("desk.glb", "../desk.glb"),
            text.replace("desk.glb", "/desk.glb"),
            text.replace("role = 'desk'", "role = 'script'"),
            text.replace("max_extent = 3.0", "max_extent = -1.0"),
            text.replace("[2,1]", "[0,1]"),
            text.replace("desk.glb", "https://evil/desk.glb"),
            text.replace("desk.glb", "a\\\\desk.glb"),
            "not toml".into(),
        ] {
            assert!(super::Pack::parse(&bad).is_err(), "accepted {bad}");
        }
    }
}
