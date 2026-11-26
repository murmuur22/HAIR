//! FILE ONLY corrected-r5. Distinct external successor; r3/r4 remain read only.
//! Independent declared source fit and host/marker/literal-label depth reference.
//! References are NEVER uploaded as a positive. Parent owns compilation and Metal.
use super::*;
use glam::{DMat4, DVec2, DVec3, DVec4};
use std::os::unix::fs::OpenOptionsExt;

const POINTS: [[f32; 3]; 4] = [
    [1.0, 2.0, 3.0], [5.0, 2.0, 3.0],
    [1.0, 4.0, 3.0], [2.0, 2.5, 9.0],
];
const INDICES: [usize; 12] = [0, 2, 1, 0, 1, 3, 1, 2, 3, 2, 0, 3];
const NAME: &str = "m.glb";
const DISPLAY: &str = "\"m.glb\"";
const KIND: &str = "GLB suffix (bounded static mesh or fallback; E reads bytes)";
const PROMPT: &str = "E - Inspect \"m.glb\" [GLB suffix (bounded static mesh or fallback; E reads bytes)] | I source";
const EPS: f32 = 0.0001;

fn exclusive(path: &Path, bytes: &[u8]) {
    let mut f = fs::OpenOptions::new().write(true).create_new(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(path).unwrap();
    f.write_all(bytes).unwrap();
    f.flush().unwrap();
}
struct OwnedFixture(PathBuf);
impl Drop for OwnedFixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("MODEL_GPU_FIXTURE_RETAINED {}", self.0.display());
        } else {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn fixture_glb() -> Vec<u8> {
    let mut bin = Vec::new();
    for p in POINTS { for v in p { bin.extend_from_slice(&v.to_le_bytes()); } }
    for _ in POINTS {
        for v in [0.0_f32, 1.0, 0.0] { bin.extend_from_slice(&v.to_le_bytes()); }
    }
    for i in INDICES { bin.extend_from_slice(&(i as u16).to_le_bytes()); }
    assert_eq!(bin.len(), 120);
    let mut json = br#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"buffers":[{"byteLength":120}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":48,"target":34962},{"buffer":0,"byteOffset":48,"byteLength":48,"target":34962},{"buffer":0,"byteOffset":96,"byteLength":24,"target":34963}],"accessors":[{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","min":[1,2,3],"max":[5,4,9]},{"bufferView":1,"componentType":5126,"count":4,"type":"VEC3"},{"bufferView":2,"componentType":5123,"count":12,"type":"SCALAR"}],"materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0.91,0.13,0.67,1],"metallicFactor":0,"roughnessFactor":1},"doubleSided":true}],"meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1},"indices":2,"material":0,"mode":4}]}]}"#.to_vec();
    while !json.len().is_multiple_of(4) { json.push(b' '); }
    let mut out = Vec::new();
    for n in [0x4654_6c67_u32, 2, (28 + json.len() + bin.len()) as u32] {
        out.extend_from_slice(&n.to_le_bytes());
    }
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4e4f_534a_u32.to_le_bytes()); out.extend_from_slice(&json);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004e_4942_u32.to_le_bytes()); out.extend_from_slice(&bin);
    assert!(out.len() < hair::local_explorer::READER_CHUNK_BYTES);
    out
}
fn source_color() -> [f32; 3] {
    let light = 0.45 + 0.55 * Vec3::Y.dot(Vec3::new(-0.3, 0.85, 0.4).normalize());
    [0.91, 0.13, 0.67].map(|c| c * light)
}
fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    a.into_iter().zip(b).all(|(a, b)| (a - b).abs() <= EPS)
}
fn source_vertex(v: &Vertex) -> bool { v.texture == 0 && near(v.color, source_color()) }
fn expected_mesh(a: &hair::host_scene::Anchor) -> Vec<Vertex> {
    let center = [a.at[0], a.at[1] + a.size[1] * 0.5, a.at[2]];
    let extent = [4.0, 2.0, 6.0];
    let scale = (0..3).map(|i| a.size[i] / extent[i]).fold(f32::INFINITY, f32::min);
    assert!(a.size[2] / 6.0 < a.size[0] / 4.0);
    assert!(a.size[2] / 6.0 < a.size[1] / 2.0);
    INDICES.iter().map(|i| Vertex {
        position: std::array::from_fn(|j| center[j] + (POINTS[*i][j] - [3.0, 3.0, 6.0][j]) * scale),
        color: source_color(), uv: [0.0; 2], texture: 0,
    }).collect()
}
fn assert_mesh(scene: &[Vertex], expected: &[Vertex]) {
    let mut actual: Vec<_> = scene.iter().filter(|v| source_vertex(v)).collect();
    assert_eq!(actual.len(), 12, "actual source tetrahedron, not marker/quad");
    for e in expected {
        let i = actual.iter().position(|v| near(v.position, e.position)
            && near(v.color, e.color) && v.uv == e.uv && v.texture == e.texture)
            .expect("exact centered uniform XYZ fit/material");
        actual.remove(i);
    }
}
fn outside_bytes(scene: &[Vertex], a: &hair::host_scene::Anchor) -> Vec<u8> {
    let center = [a.at[0], a.at[1] + a.size[1] * 0.5, a.at[2]];
    let outside: Vec<_> = scene.iter().filter(|v| (0..3).any(|i|
        (v.position[i] - center[i]).abs() > a.size[i] * 0.5 + EPS)).copied().collect();
    bytemuck::cast_slice::<Vertex, u8>(&outside).to_vec()
}
fn wait_model(app: &mut App, id: &str) {
    let start = Instant::now();
    for poll in 1..=256 {
        app.poll_local_image().unwrap();
        assert!(!app.world.reading, "lazy world publication must precede E");
        if app.object_previews.models.contains_key(id) {
            println!("MODEL_GPU_PUBLISHED polls={poll} elapsed_ms={}", start.elapsed().as_millis());
            return;
        }
        if start.elapsed() >= Duration::from_secs(5) { break; }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("MODEL_GPU_PRE_E_TIMEOUT {}", app.object_previews.status);
}
fn prepare(app: &mut App) {
    app.poll_local_image().unwrap();
    hair::interaction::refresh(&app.themes[app.theme_index].host, &app.pose, &mut app.world);
    app.update_hud();
    let scene = app.themes[app.theme_index].vertices_for(&app.world);
    app.renderer.as_mut().unwrap().set_scene(&scene);
}
fn pixels(app: &App) -> Vec<u8> {
    super::readback(app.renderer.as_ref().unwrap(), &app.pose, Some(&app.world), None).unwrap()
}
fn ui_contract(app: &App, pixels: &[u8]) -> Result<Vec<Option<[u8; 3]>>> {
    let renderer = app.renderer.as_ref().unwrap(); let [w, h] = renderer.size;
    if pixels.len() != (w * h * 4) as usize || pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255) {
        return Err("MODEL_GPU_LENGTH_OR_ALPHA".into());
    }
    let mut quads = hair::text::overlay(&app.world, w, h, app.scroll);
    if !app.world.reading { quads.extend(hair::text::pack_overlay(&renderer.pack_feedback, w, h)); }
    let mask = super::ui_mask(&quads, [w, h]);
    for (i, color) in mask.iter().enumerate() {
        if let Some(color) = color && !super::close(&pixels[i * 4..i * 4 + 3], *color) {
            return Err(format!("MODEL_GPU_UI_RASTER xy={},{}", i % w as usize, i / w as usize));
        }
    }
    Ok(mask)
}
struct Projection { matrix: DMat4, inverse: DMat4, eye: DVec3, size: [u32; 2] }
impl Projection {
    fn new(app: &App) -> Self {
        let size = app.renderer.as_ref().unwrap().size;
        let matrix = DMat4::from_cols_array(&super::matrix(&app.pose, size).to_cols_array().map(f64::from));
        Self { matrix, inverse: matrix.inverse(), eye: DVec3::new(f64::from(app.pose.position[0]),
            1.65_f32 as f64, f64::from(app.pose.position[1])), size }
    }
    fn project(&self, p: [f32; 3]) -> DVec2 {
        let clip = self.matrix * DVec3::from_array(p.map(f64::from)).extend(1.0);
        assert!(clip.w > 0.05 && clip.z >= 0.0 && clip.z < clip.w);
        let p = clip.truncate() / clip.w;
        DVec2::new((p.x + 1.0) * 0.5 * f64::from(self.size[0]),
            (1.0 - p.y) * 0.5 * f64::from(self.size[1]))
    }
    fn ray(&self, x: u32, y: u32) -> DVec3 {
        self.inverse.project_point3(DVec3::new(
            2.0 * (f64::from(x) + 0.5) / f64::from(self.size[0]) - 1.0,
            1.0 - 2.0 * (f64::from(y) + 0.5) / f64::from(self.size[1]), 1.0)) - self.eye
    }
}
fn hit(eye: DVec3, ray: DVec3, tri: &[Vertex]) -> bool {
    let a = DVec3::from_array(tri[0].position.map(f64::from));
    let e1 = DVec3::from_array(tri[1].position.map(f64::from)) - a;
    let e2 = DVec3::from_array(tri[2].position.map(f64::from)) - a;
    let p = ray.cross(e2); let det = e1.dot(p);
    if det.abs() < 1e-12 { return false; }
    let t = eye - a; let u = t.dot(p) / det; let q = t.cross(e1); let v = ray.dot(q) / det;
    u >= 0.0 && v >= 0.0 && u + v <= 1.0 && e2.dot(q) / det > 0.0
}
fn edge_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let d = b - a;
    if d.length_squared() < 1e-12 { return p.distance(a); }
    p.distance(a + d * ((p - a).dot(d) / d.length_squared()).clamp(0.0, 1.0))
}
fn bounds(points: &[DVec2], size: [u32; 2], pad: f64) -> [u32; 4] {
    let lo = points.iter().copied().fold(DVec2::splat(f64::INFINITY), DVec2::min);
    let hi = points.iter().copied().fold(DVec2::splat(f64::NEG_INFINITY), DVec2::max);
    assert!(lo.x > pad && lo.y > pad && hi.x < f64::from(size[0]) - pad
        && hi.y < f64::from(size[1]) - pad, "source projection must fit viewport");
    [(lo.x - pad).floor() as u32, (lo.y - pad).floor() as u32,
        (hi.x + pad).ceil() as u32, (hi.y + pad).ceil() as u32]
}

// Independent cuboid emission: no scene::cuboid/private library access.
fn cuboid(out: &mut Vec<Vertex>, at: [f32; 3], size: [f32; 3], color: [f32; 3]) {
    let corners: [[f32; 3]; 8] = std::array::from_fn(|i| std::array::from_fn(|j|
        at[j] + size[j] * if i & (1 << j) == 0 { -0.5 } else { 0.5 }));
    for face in [[0,1,3,2], [4,6,7,5], [0,4,5,1], [2,3,7,6], [0,2,6,4], [1,5,7,3]] {
        for i in [0,1,2,0,2,3] {
            out.push(Vertex { position: corners[face[i]], color, uv: [0.0; 2], texture: 0 });
        }
    }
}
fn marker_mesh(a: &hair::host_scene::Anchor) -> Vec<Vertex> {
    let mut out = Vec::new();
    cuboid(&mut out, [a.at[0], a.at[1] + a.size[1] + 0.03, a.at[2]],
        [0.45, 0.02, 0.2], [0.1, 0.8, 0.3]); out
}
fn marker_vertex(v: &Vertex, a: &hair::host_scene::Anchor) -> bool {
    v.texture == 0 && v.color == [0.1, 0.8, 0.3] && (0..3).all(|i| {
        let center = [a.at[0], a.at[1] + a.size[1] + 0.03, a.at[2]];
        (v.position[i] - center[i]).abs() <= [0.225, 0.01, 0.1][i] + EPS
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner { Source, Marker, Filename, KindLabel, Host }
struct Reference { vertices: Vec<Vertex>, owners: Vec<Owner>, entry: Entry, anchor: hair::host_scene::Anchor }
impl Reference {
    fn append(&mut self, vertices: &[Vertex], owner: Owner) {
        assert!(vertices.len().is_multiple_of(3));
        self.vertices.extend_from_slice(vertices);
        self.owners.extend(std::iter::repeat_n(owner, vertices.len() / 3));
    }
    fn declared(root: &Path, declared: &Snapshot, host: &hair::host_scene::HostScene) -> Self {
        assert_eq!(declared.entries.len(), 1);
        let entry = declared.entries[0].clone();
        assert_eq!(entry.name.as_os_str(), std::ffi::OsStr::new(NAME)); assert_eq!(entry.kind, Kind::File);
        assert_eq!(hair::folder_scene::display(&entry.name), DISPLAY);
        assert_eq!(hair::folder_scene::kind(&entry), KIND);
        assert_eq!(DISPLAY.chars().count(), 7);
        assert_eq!(host.room, [-10.5, -6.0, 10.5, 6.0]);
        assert!(host.open_platform && host.geometry.is_empty() && host.overhead.is_empty());
        assert!(host.walls().is_empty());
        let a = host.anchors.iter().find(|a| a.id == "entry-0").unwrap().clone();
        assert_eq!(a.at, [-4.5, 0.0, -3.0]); assert_eq!(a.size, [0.8, 0.8, 0.6]);
        let mut r = Self { vertices: Vec::new(), owners: Vec::new(), entry, anchor: a.clone() };
        let mut floor = Vec::new();
        cuboid(&mut floor, [0.0; 3], [21.4, 0.1, 12.4], [0.22, 0.24, 0.21]);
        r.append(&floor, Owner::Host);
        // Pinned office manifest, independent asset fit and rear-facing parent.
        // Never use Theme::vertices_for, actual payload or App's model as oracle.
        let manifest: toml::Value = toml::from_str(&fs::read_to_string(root.join("packs/office.toml")).unwrap()).unwrap();
        assert_eq!(manifest["version"].as_integer(), Some(2));
        assert_eq!(manifest["name"].as_str(), Some("office"));
        assert!(manifest.get("surfaces").is_none() && manifest.get("door_trim").is_none());
        let roles = manifest["roles"].as_array().unwrap(); assert_eq!(roles.len(), 3);
        let role = roles.iter().find(|v| v["role"].as_str() == Some("portal")).unwrap();
        assert_eq!(role["file"].as_str(), Some("wallDoorway.glb"));
        assert_eq!(role["scale"].as_float(), Some(2.0));
        assert_eq!(role["yaw_degrees"].as_float(), Some(0.0));
        assert_eq!(role["max_extent"].as_float(), Some(3.0));
        assert!(role["pivot"].as_array().unwrap().iter().all(|v| v.as_float() == Some(0.0)));
        let parent = host.anchors.iter().find(|a| a.id == "folder-parent").unwrap();
        assert_eq!(parent.at, [0.0, 0.0, 6.7]); assert_eq!(parent.size, [1.45, 2.0, 0.6]);
        // Public decoder is shared for the declared bundled asset, not the source fixture fit.
        let model = hair::assets::decode(&fs::read(root.join("packs/wallDoorway.glb")).unwrap(), 3.0).unwrap();
        assert!(model.textures.is_empty() && model.vertices.iter().all(|v| v.texture == 0));
        let min = Vec3::from_array(model.min); let max = Vec3::from_array(model.max);
        let origin = Vec3::new((min.x + max.x) * 0.5, min.y, (min.z + max.z) * 0.5);
        let lo = (min - origin) * 2.0; let hi = (max - origin) * 2.0;
        let bottom = lo.y.min(0.0);
        let extent = [lo.x.abs().max(hi.x.abs()) * 2.0, hi.y - bottom, lo.z.abs().max(hi.z.abs()) * 2.0];
        let scale = (0..3).filter(|i| extent[*i] > 0.000001)
            .map(|i| parent.size[i] / extent[i]).fold(1.0_f32, f32::min);
        let mut portal: Vec<_> = model.vertices.iter().map(|v| {
            let mut p = (Vec3::from_array(v.position) - origin) * 2.0; p.y -= bottom; p *= scale;
            Vertex { position: [parent.at[0] - p.x, parent.at[1] + p.y, parent.at[2] - p.z], ..*v }
        }).collect();
        let mut panel = Vec::new();
        cuboid(&mut panel, [0.0, parent.size[1] * 0.5, parent.size[2] * 0.5 + 0.02],
            [0.45, 0.25, 0.02], [0.08, 0.6, 0.18]);
        for v in &mut panel {
            let [x,y,z] = v.position; v.position = [parent.at[0] - x, parent.at[1] + y, parent.at[2] - z];
        }
        portal.extend(panel); r.append(&portal, Owner::Host);
        r.append(&expected_mesh(&a), Owner::Source); r.append(&marker_mesh(&a), Owner::Marker);
        let mut parent_label = hair::text::object_label("Parent - filesystem (U)",
            [0.0, parent.size[1] + 0.22, parent.size[2] * 0.5 + 0.02], 32);
        for v in &mut parent_label {
            let [x,y,z] = v.position; v.position = [parent.at[0] - x, parent.at[1] + y, parent.at[2] - z];
        }
        r.append(&parent_label, Owner::Host);
        let at = [a.at[0], a.at[1] + a.size[1] + 0.22, a.at[2] + a.size[2] * 0.5 + 0.02];
        // Share only the public font primitive; literal, basis and placement are independent.
        r.append(&hair::text::object_label(DISPLAY, at, 25), Owner::Filename);
        r.append(&hair::text::object_label(KIND, [at[0], at[1] - 0.12, at[2]], 28), Owner::KindLabel);
        r
    }
    fn state(&self, app: &App) -> Result<()> {
        let view = app.world.folder_view.as_ref().ok_or("MODEL_GPU_MISSING_VIEW")?;
        let binding = view.bindings.get("entry-0").ok_or("MODEL_GPU_MISSING_BINDING")?;
        let target = app.world.interaction.as_ref().ok_or("MODEL_GPU_MISSING_SELECTION")?;
        let display = hair::folder_scene::display(&self.entry.name);
        let prompt = format!("E - Inspect {display} [{}] | I source", hair::folder_scene::kind(&self.entry));
        if binding.entry != self.entry || display != DISPLAY || prompt != PROMPT
            || target.anchor != "entry-0" || target.prompt != prompt
            || target.destination.as_deref() != Some(hair::folder_scene::display(self.entry.path.as_os_str()).as_str())
            || app.world.explicit_target.is_some() {
            return Err("MODEL_GPU_WRONG_FILENAME_SELECTION".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Clip { p: DVec4, color: DVec3 }
fn plane(p: DVec4, i: usize) -> f64 {
    match i { 0 => p.x + p.w, 1 => p.w - p.x, 2 => p.y + p.w,
        3 => p.w - p.y, 4 => p.z, 5 => p.w - p.z, _ => unreachable!() }
}
fn cross(a: DVec2, b: DVec2) -> f64 { a.x * b.y - a.y * b.x }
struct RasterTriangle { screen: [DVec2; 3], coverage: [DVec2; 3], clip: [Clip; 3], owner: Owner }
struct Raster { triangles: Vec<RasterTriangle> }
#[derive(Clone, Copy, Debug)]
struct Surface { owner: Owner, depth: f32, color: [u8; 3] }
impl Raster {
    fn new(p: &Projection, r: &Reference) -> Self {
        let mut triangles = Vec::new();
        for (t, owner) in r.vertices.chunks_exact(3).zip(&r.owners) {
            let mut polygon: Vec<_> = t.iter().map(|v| Clip {
                p: p.matrix * DVec3::from_array(v.position.map(f64::from)).extend(1.0),
                color: DVec3::from_array(v.color.map(f64::from)),
            }).collect();
            // Real homogeneous six-plane clipping, including camera-crossing host
            // floor triangles. Unlike r4 diagnostics, nothing is silently omitted.
            for i in 0..6 {
                let mut next = Vec::new();
                if polygon.is_empty() { break; }
                for j in 0..polygon.len() {
                    let a = polygon[j]; let b = polygon[(j + 1) % polygon.len()];
                    let da = plane(a.p, i); let db = plane(b.p, i);
                    if da >= 0.0 { next.push(a); }
                    if (da >= 0.0) != (db >= 0.0) {
                        let u = da / (da - db);
                        next.push(Clip { p: a.p.lerp(b.p, u), color: a.color.lerp(b.color, u) });
                    }
                }
                polygon = next;
            }
            for j in 1..polygon.len().saturating_sub(1) {
                let mut clip = [polygon[0], polygon[j], polygon[j + 1]];
                if clip.iter().any(|v| v.p.w <= 0.0) { panic!("MODEL_GPU_INVALID_CLIPPED_W"); }
                let mut screen = clip.map(|v| DVec2::new((v.p.x / v.p.w + 1.0) * 0.5 * f64::from(p.size[0]),
                    (1.0 - v.p.y / v.p.w) * 0.5 * f64::from(p.size[1])));
                // Candidate numerical raster policy: fixed 8 fractional bits and
                // top-left coverage, NOT a color tolerance or an occluder mask.
                // Parent must exercise this at small y=181 (r4 continuous oracle
                // disagreed on 8 marker-boundary samples). Never calibrate to RGB.
                let mut coverage = screen.map(|v| DVec2::new(
                    (v.x * 256.0).round_ties_even(), (v.y * 256.0).round_ties_even()) / 256.0);
                let area = cross(coverage[1] - coverage[0], coverage[2] - coverage[0]);
                if area == 0.0 { continue; }
                if area < 0.0 { clip.swap(1,2); screen.swap(1,2); coverage.swap(1,2); }
                triangles.push(RasterTriangle { screen, coverage, clip, owner: *owner });
            }
        }
        Self { triangles }
    }
    fn nearest(&self, x: u32, y: u32) -> Option<Surface> {
        let q = DVec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
        let mut nearest: Option<Surface> = None;
        for t in &self.triangles {
            if (0..3).any(|i| {
                let a = t.coverage[i]; let b = t.coverage[(i+1)%3];
                let d = b - a; let e = cross(d, q - a);
                e < 0.0 || (e == 0.0 && !(d.y < 0.0 || (d.y == 0.0 && d.x > 0.0)))
            }) { continue; }
            let s = t.screen; let area = cross(s[1] - s[0], s[2] - s[0]);
            if area.abs() < 1e-12 { continue; }
            let b = [cross(s[1] - q, s[2] - q) / area,
                cross(s[2] - q, s[0] - q) / area, cross(s[0] - q, s[1] - q) / area];
            let depth = (0..3).map(|i| b[i] * t.clip[i].p.z / t.clip[i].p.w).sum::<f64>() as f32;
            if !(0.0..1.0).contains(&depth) || nearest.is_some_and(|n| depth >= n.depth) { continue; }
            let divisor = (0..3).map(|i| b[i] / t.clip[i].p.w).sum::<f64>();
            let color = (0..3).map(|i| t.clip[i].color * (b[i] / t.clip[i].p.w)).sum::<DVec3>() / divisor;
            nearest = Some(Surface { owner: t.owner, depth, color: super::rgb(color.to_array().map(|v| v as f32)) });
        }
        nearest
    }
}
fn assert_declared_scene(scene: &[Vertex], r: &Reference) {
    assert_eq!(scene.len(), r.vertices.len(), "no unreferenced host/label/marker geometry");
    for (actual, expected) in scene.iter().zip(&r.vertices) {
        assert!(near(actual.position, expected.position) && near(actual.color, expected.color)
            && actual.uv == expected.uv && actual.texture == expected.texture,
            "independently declared scene differs: actual={actual:?} expected={expected:?}");
    }
}

// Supporting CPU reference controls execute inside the ONE selected ignored
// test. They do not render and are never reported as GPU negatives.
fn raster_reference_controls() {
    let p = Projection { matrix: DMat4::IDENTITY, inverse: DMat4::IDENTITY,
        eye: DVec3::ZERO, size: [4,4] };
    let mut r = Reference { vertices: Vec::new(), owners: Vec::new(),
        entry: Entry { name: NAME.into(), path: PathBuf::from("/declared/m.glb"), kind: Kind::File },
        anchor: hair::host_scene::Anchor { id: "entry-0".into(), role: "document".into(),
            at: [-4.5,0.0,-3.0], size: [0.8,0.8,0.6], binding: "explicit-page".into() } };
    let triangle = |depth, color| [Vertex { position: [-1.0,1.0,depth], color, uv: [0.0;2], texture: 0 },
        Vertex { position: [1.0,1.0,depth], color, uv: [0.0;2], texture: 0 },
        Vertex { position: [-1.0,-1.0,depth], color, uv: [0.0;2], texture: 0 }];
    r.append(&triangle(0.8,source_color()),Owner::Source);
    r.append(&triangle(0.2,[0.1,0.8,0.3]),Owner::Marker);
    let raster = Raster::new(&p,&r);
    assert_eq!(raster.nearest(0,0).unwrap().owner,Owner::Marker,"nearest, not ANY source hit");
    assert!(raster.nearest(3,3).is_none(),"outside triangle is not a witness");
    r.append(&triangle(0.2,[0.6,1.0,0.7]),Owner::KindLabel);
    assert_eq!(Raster::new(&p,&r).nearest(0,0).unwrap().owner,Owner::Marker,"strict Less preserves earlier tie");
    let mut reversed = triangle(0.1,[0.6,1.0,0.7]); reversed.swap(1,2);
    r.append(&reversed,Owner::Filename);
    assert_eq!(Raster::new(&p,&r).nearest(0,0).unwrap().owner,Owner::Filename,"no culling");
    let mut clipped = triangle(0.05,[0.22,0.24,0.21]);
    clipped[0].position = [-3.0,3.0,-0.1];
    r.append(&clipped,Owner::Host);
    assert_eq!(Raster::new(&p,&r).nearest(1,0).unwrap().owner,Owner::Host,"near/side clipping retained");
    println!("MODEL_GPU_REFERENCE_CPU_CONTROLS passed; NOT_GPU_NEGATIVES");
}

fn fixed(size: [u32; 2]) -> (u32,u32) {
    match size { [960,640] => (424,237), [640,480] => (278,178), _ => panic!("r3 viewport only") }
}
fn pixel(frame: &[u8], size: [u32; 2], x: u32, y: u32) -> &[u8] {
    let i = ((y * size[0] + x) * 4) as usize; &frame[i..i+4]
}
struct Witness { x: u32, y: u32, owner: Owner, color: [u8; 3], exterior: bool }
struct Contract { p: Projection, raster: Raster, witnesses: Vec<Witness>, fixed: (u32,u32) }
impl Contract {
    fn new(app: &App, mesh: &[Vertex], r: &Reference) -> Self {
        let p = Projection::new(app); let raster = Raster::new(&p, r);
        let projected: Vec<_> = mesh.iter().map(|v| p.project(v.position)).collect();
        let [x0,y0,x1,y1] = bounds(&projected, p.size, 4.0);
        let fixed = fixed(p.size); let mut witnesses = Vec::new();
        for y in y0..y1 { for x in x0..x1 {
            let point = DVec2::new(f64::from(x)+0.5, f64::from(y)+0.5);
            // EXACT original fixed 1px source-edge exclusion, no label/UI mask.
            if projected.chunks_exact(3).any(|t| (0..3).any(|i|
                edge_distance(point, t[i], t[(i+1)%3]) < 1.0)) { continue; }
            let inside = mesh.chunks_exact(3).any(|t| hit(p.eye, p.ray(x,y), t));
            let surface = raster.nearest(x,y).unwrap_or(Surface {
                owner: Owner::Host, depth: 1.0, color: super::rgb([0.04,0.06,0.05]),
            });
            assert!(inside || surface.owner != Owner::Source, "exterior/source coverage reference mismatch");
            witnesses.push(Witness { x, y, owner: surface.owner, color: surface.color, exterior: !inside });
        } }
        assert!(witnesses.iter().any(|w| (w.x,w.y) == fixed && w.owner == Owner::Marker),
            "original r3 pixel must remain an asserted marker witness");
        Self { p, raster, witnesses, fixed }
    }
    fn marker(&self, app: &App, frame: &[u8]) -> Result<()> {
        let (x,y) = self.fixed; let [w,h] = self.p.size;
        if frame.len() != (w*h*4) as usize { return Err("MODEL_GPU_LENGTH_OR_ALPHA".into()); }
        let surface = self.raster.nearest(x,y).ok_or("MODEL_GPU_MARKER_REFERENCE_MISSING")?;
        if surface.owner != Owner::Marker || surface.color != super::rgb([0.1,0.8,0.3]) {
            return Err("MODEL_GPU_MARKER_REFERENCE_WRONG".into());
        }
        let mut quads = hair::text::overlay(&app.world,w,h,app.scroll);
        quads.extend(hair::text::pack_overlay(&app.renderer.as_ref().unwrap().pack_feedback,w,h));
        if super::ui_mask(&quads,[w,h])[(y*w+x) as usize].is_some()
            || !super::close(pixel(frame,[w,h],x,y), surface.color) {
            return Err(format!("MODEL_GPU_MARKER_WITNESS xy={x},{y} expected={:?} actual={:?}",
                surface.color, pixel(frame,[w,h],x,y)));
        }
        Ok(())
    }
    fn sources(&self, ui: &[Option<[u8;3]>], frame: &[u8]) -> Result<usize> {
        if frame.len() != (self.p.size[0]*self.p.size[1]*4) as usize {
            return Err("MODEL_GPU_LENGTH_OR_ALPHA".into());
        }
        let mut count = 0;
        for w in self.witnesses.iter().filter(|w| w.owner == Owner::Source) {
            if ui[(w.y*self.p.size[0]+w.x) as usize].is_some() { continue; }
            if !super::close(pixel(frame,self.p.size,w.x,w.y),super::rgb(source_color())) {
                return Err(format!("MODEL_GPU_SOURCE_SILHOUETTE owner=SOURCE xy={},{} expected={:?} actual={:?}",
                    w.x,w.y,super::rgb(source_color()),pixel(frame,self.p.size,w.x,w.y)));
            }
            count += 1;
        }
        if count < 24 { return Err(format!("MODEL_GPU_WITNESSES frontmost_SOURCE={count}")); }
        Ok(count)
    }
}
fn model_frame(app: &App, r: &Reference, c: &Contract, pixels: &[u8]) -> Result<()> {
    let ui = ui_contract(app,pixels)?;
    if app.world.reading { return Err("MODEL_GPU_EXPECTED_WORLD".into()); }
    r.state(app)?; c.marker(app,pixels)?;
    let inside = c.sources(&ui,pixels)?; let mut outside = 0; let mut occluders = 0;
    let mut kind_witnesses = 0; let mut marker_witnesses = 0;
    for w in &c.witnesses {
        let [width,height] = c.p.size;
        let i = (w.y*width+w.x) as usize;
        let crosshair = w.x >= width/2-2 && w.x < width/2+2 && w.y >= height/2-2 && w.y < height/2+2;
        if !w.exterior && ui[i].is_some() && !crosshair {
            return Err(format!("MODEL_GPU_SOURCE_OBSTRUCTED xy={},{}",w.x,w.y));
        }
        if ui[i].is_some() { continue; }
        if !super::close(pixel(pixels,c.p.size,w.x,w.y),w.color) {
            return Err(format!("MODEL_GPU_DEPTH_SURFACE owner={:?} xy={},{} exterior={} expected={:?} actual={:?}",
                w.owner,w.x,w.y,w.exterior,w.color,pixel(pixels,c.p.size,w.x,w.y)));
        }
        // Exterior witnesses remain non-source, plus now assert their actual
        // independently declared frontmost surface instead of merely not-magenta.
        if w.exterior {
            if super::close(pixel(pixels,c.p.size,w.x,w.y),super::rgb(source_color())) {
                return Err(format!("MODEL_GPU_SOURCE_SILHOUETTE owner=EXTERIOR xy={},{}",w.x,w.y));
            }
            outside += 1;
        } else if w.owner != Owner::Source {
            occluders += 1;
            if w.owner == Owner::KindLabel { kind_witnesses += 1; }
            if w.owner == Owner::Marker { marker_witnesses += 1; }
        }
    }
    if inside < 24 || outside < 24 || occluders == 0 || kind_witnesses == 0 || marker_witnesses == 0 {
        return Err(format!("MODEL_GPU_WITNESSES inside={inside} outside={outside} occluders={occluders} kindlabel={kind_witnesses} marker={marker_witnesses}"));
    }
    filename_frame(app,r,c,pixels,&ui)?;
    println!("MODEL_GPU_SOURCE_WITNESSES frontmost_SOURCE={inside} exterior={outside} asserted_occluders={occluders} kindlabel={kind_witnesses} marker={marker_witnesses} original_marker={:?}",c.fixed);
    Ok(())
}
fn filename_frame(app: &App, r: &Reference, c: &Contract, pixels: &[u8], ui: &[Option<[u8;3]>]) -> Result<()> {
    let a = &r.anchor; let p = &c.p;
    let literal = hair::folder_scene::display(&r.entry.name);
    assert_eq!(literal,DISPLAY); assert_eq!(literal.len(),7);
    let at = [a.at[0], a.at[1]+a.size[1]+0.22, a.at[2]+a.size[2]*0.5+0.02];
    let glyphs = hair::text::object_label(&literal,at,25);
    let left = at[0] - literal.len() as f32 * 6.0 * 0.012 * 0.5;
    let mut counts = [0_usize;7];
    for cell in glyphs.chunks_exact(6) {
        let projected: Vec<_> = cell.iter().map(|v| p.project(v.position)).collect();
        let [x0,y0,x1,y1] = bounds(&projected,p.size,0.0);
        let xmin = cell.iter().map(|v| v.position[0]).fold(f32::INFINITY,f32::min);
        let xmax = cell.iter().map(|v| v.position[0]).fold(f32::NEG_INFINITY,f32::max);
        let ymin = cell.iter().map(|v| v.position[1]).fold(f32::INFINITY,f32::min);
        let ymax = cell.iter().map(|v| v.position[1]).fold(f32::NEG_INFINITY,f32::max);
        let column = (((xmin-left)/0.012+0.01)/6.0).floor() as usize;
        assert!(column < literal.len());
        for y in y0..y1 { for x in x0..x1 {
            let ray = p.ray(x,y); let t = (f64::from(at[2])-p.eye.z)/ray.z; let q = p.eye+t*ray;
            if t <= 0.0 || q.x <= f64::from(xmin) || q.x >= f64::from(xmax)
                || q.y <= f64::from(ymin) || q.y >= f64::from(ymax) { continue; }
            let point = DVec2::new(f64::from(x)+0.5,f64::from(y)+0.5);
            let corners = [projected[0],projected[1],projected[2],projected[5]];
            if (0..4).any(|i| edge_distance(point,corners[i],corners[(i+1)%4]) < 0.15) { continue; }
            if ui[(y*p.size[0]+x) as usize].is_some()
                || !c.raster.nearest(x,y).is_some_and(|v| v.owner == Owner::Filename)
                || !super::close(pixel(pixels,p.size,x,y),super::rgb(cell[0].color)) {
                return Err(format!("MODEL_GPU_FILENAME_OBSTRUCTED_OR_WRONG column={column} xy={x},{y}"));
            }
            counts[column] += 1;
        } }
    }
    if counts.iter().any(|n| *n < 4) { return Err(format!("MODEL_GPU_FILENAME_INSUFFICIENT {counts:?}")); }
    r.state(app)?;
    println!("MODEL_GPU_FILENAME_CELL_WITNESSES literal={literal:?} {counts:?}"); Ok(())
}

fn retain(app: &App, root: &Path, group: &Path, stage: &str, frame: &[u8], scoped: &Result<()>) {
    let dir = group.join(stage);
    fs::create_dir(&dir).expect("exclusive absent stage");
    let output = resources::smoke_output(root, &dir).unwrap();
    let renderer = app.renderer.as_ref().unwrap();
    let generic = renderer.capture(&output, "office.png", &app.pose, &app.world);
    let colors: std::collections::HashSet<_> = frame.as_chunks::<4>().0.iter().map(|p| [p[0], p[1], p[2]]).collect();
    if let Err(error) = &generic {
        assert_eq!(error, &format!("GPU image suspiciously flat: {} colors", colors.len()),
            "never swallow GPU/IO/writer errors as sparse success");
        assert!(colors.len() < if app.world.reading { 2 } else { 20 });
        if scoped.is_ok() {
            // Semantic supplemental writer, only AFTER contract passes; unchanged
            // allowlist, retained directory/O_EXCL/no-follow, finish and flush.
            let mut file = std::io::BufWriter::new(output.create_capture("office.png").unwrap());
            let mut encoder = png::Encoder::new(&mut file, renderer.size[0], renderer.size[1]);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut png = encoder.write_header().unwrap();
            png.write_image_data(frame).unwrap();
            png.finish().unwrap();
            file.flush().unwrap();
        }
    }
    if generic.is_ok() || scoped.is_ok() {
        let encoded = fs::read(output.path().join("office.png")).unwrap();
        let mut decoder = png::Decoder::new(std::io::Cursor::new(encoded)).read_info().unwrap();
        let mut decoded = vec![0; decoder.output_buffer_size().unwrap()];
        let info = decoder.next_frame(&mut decoded).unwrap();
        assert_eq!([info.width, info.height], renderer.size);
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        assert_eq!(&decoded[..info.buffer_size()], frame, "generic second draw must equal semantic readback");
        assert!(output.create_capture("office.png").is_err(), "repeat writer must refuse");
    }
    exclusive(&group.join(format!("{stage}.rgba")), frame);
    exclusive(&group.join(format!("{stage}.txt")), format!(
        "stage={stage}\nsize={:?}\nreading={}\nscroll={}\npose={:?}\ntarget={:?}\nrgba_sha256={}\ngeneric={generic:?}\nscoped={scoped:?}\nvision=PENDING_PARENT_ACTUAL_IMAGE_INSPECTION\nPNG absent if both guards fail; RGBA is actual uncertified readback.\n",
        renderer.size, app.world.reading, app.scroll, app.pose, app.world.interaction, super::digest(frame)).as_bytes());
    println!("MODEL_GPU_CAPTURE {stage} generic={generic:?} scoped={scoped:?}");
}
fn reject(app: &App, r: &Reference, c: &Contract, frame: &[u8], label: &str,
    source_failure_required: bool, log: &mut String) {
    // A whole-contract rejection is not enough: demand a separate rejection on
    // positive-reference frontmost SOURCE pixels, never unrelated HUD/marker.
    if source_failure_required {
        let positive_ui = super::ui_mask(&{
            let mut q = hair::text::overlay(&app.world,c.p.size[0],c.p.size[1],app.scroll);
            q.extend(hair::text::pack_overlay(&app.renderer.as_ref().unwrap().pack_feedback,c.p.size[0],c.p.size[1])); q
        },c.p.size);
        let reason = c.sources(&positive_ui,frame).expect_err(label);
        assert!(reason.starts_with("MODEL_GPU_SOURCE_SILHOUETTE owner=SOURCE"), "{label}: {reason}");
        writeln!(log,"{label} specifically_frontmost_SOURCE={reason}").unwrap();
    }
    let error = model_frame(app,r,c,frame).expect_err(label);
    writeln!(log,"{label} sha256={} reason={error}",super::digest(frame)).unwrap();
}
fn upload_negative(app: &mut App, vertices: &[Vertex], group: &Path, label: &str) -> Vec<u8> {
    exclusive(&group.join(format!("{label}.upload-vertices.bin")),bytemuck::cast_slice::<Vertex,u8>(vertices));
    app.renderer.as_mut().unwrap().set_scene(vertices);
    let frame = pixels(app); exclusive(&group.join(format!("{label}.rgba")),&frame); frame
}
fn restore(app: &mut App, scene: &[Vertex], positive: &[u8], label: &str, log: &mut String) {
    app.renderer.as_mut().unwrap().set_scene(scene);
    assert_eq!(pixels(app),positive,"{label} restore exact actual GPU frame");
    writeln!(log,"{label} RESTORE_BYTE_IDENTICAL=true").unwrap();
}
fn negatives(app: &mut App, r: &Reference, c: &Contract, fallback: &[Vertex], positive: &[u8], group: &Path) {
    let mut log = String::from("Real GPU counterfactuals unless explicitly NOT-GPU. Upload bytes are CPU boundary evidence, NOT mapped GPU memory.\n");
    let renderer = app.renderer.as_ref().unwrap();
    let blank = super::readback(renderer,&app.pose,None,Some(wgpu::Color::BLACK)).unwrap();
    reject(app,r,c,&blank,"blank-GPU",false,&mut log);
    let no_ui = super::readback(renderer,&app.pose,None,None).unwrap();
    reject(app,r,c,&no_ui,"no-UI-GPU",false,&mut log);
    let mut wrong_pose = app.pose; wrong_pose.yaw += std::f32::consts::PI;
    let wrong = super::readback(renderer,&wrong_pose,Some(&app.world),None).unwrap();
    reject(app,r,c,&wrong,"wrong-pose-GPU",true,&mut log);
    reject(app,r,c,&positive[..positive.len()-1],"truncated-buffer-NOT-GPU",false,&mut log);
    let mut alpha = positive.to_vec(); alpha[3] = 0;
    reject(app,r,c,&alpha,"alpha-corruption-NOT-GPU",false,&mut log);
    let scene = app.themes[app.theme_index].vertices_for(&app.world);
    let missing: Vec<_> = scene.chunks_exact(3).filter(|t| !t.iter().all(source_vertex)).flatten().copied().collect();
    assert_eq!(scene.len()-missing.len(),12);
    let hidden: Vec<_> = scene.iter().take(3).map(|v| Vertex { position: [100.0;3], ..*v }).collect();
    let wrong_color: Vec<_> = scene.iter().map(|v| if source_vertex(v) {
        Vertex { color: [0.1,0.8,0.2], ..*v }
    } else { *v }).collect();
    let wrong_geometry: Vec<_> = scene.iter().map(|v| if source_vertex(v) {
        Vertex { position: [v.position[0]+0.25,v.position[1],v.position[2]], ..*v }
    } else { *v }).collect();
    for (label,vertices) in [("missing-model-GPU",missing.as_slice()),("generic-placeholder-GPU",fallback),
        ("wrong-source-color-GPU",wrong_color.as_slice()),("wrong-geometry-GPU",wrong_geometry.as_slice()),
        ("HUD-only-GPU",hidden.as_slice())] {
        let frame = upload_negative(app,vertices,group,label);
        reject(app,r,c,&frame,label,true,&mut log); restore(app,&scene,positive,label,&mut log);
    }
    let marker_count = scene.iter().filter(|v| marker_vertex(v,&r.anchor)).count(); assert_eq!(marker_count,36);
    let removed_marker: Vec<_> = scene.chunks_exact(3).filter(|t| !t.iter().all(|v| marker_vertex(v,&r.anchor)))
        .flatten().copied().collect(); assert_eq!(scene.len()-removed_marker.len(),36);
    let wrong_marker: Vec<_> = scene.iter().map(|v| if marker_vertex(v,&r.anchor) {
        Vertex { color: [0.8,0.1,0.1], ..*v }
    } else { *v }).collect();
    for (label,vertices) in [("removed-marker-GPU",removed_marker.as_slice()),("wrong-color-marker-GPU",wrong_marker.as_slice())] {
        let frame = upload_negative(app,vertices,group,label);
        let reason = c.marker(app,&frame).expect_err(label);
        assert!(reason.starts_with("MODEL_GPU_MARKER_WITNESS"),"{label} must reject at ORIGINAL r3 marker pixel: {reason}");
        writeln!(log,"{label} specifically_original_marker={reason}").unwrap();
        reject(app,r,c,&frame,label,false,&mut log); restore(app,&scene,positive,label,&mut log);
    }
    let snapshot = super::super::snapshot(app);
    let (mut wrong_world,_) = hair::folder_scene::build(&snapshot,0,false).unwrap();
    wrong_world.interaction = app.world.interaction.clone();
    wrong_world.interaction.as_mut().unwrap().prompt = "E - Inspect x.glb [wrong source] | I source".into();
    let wrong_prompt = super::readback(app.renderer.as_ref().unwrap(),&app.pose,Some(&wrong_world),None).unwrap();
    reject(app,r,c,&wrong_prompt,"wrong-filename-prompt-GPU",false,&mut log);
    wrong_world.interaction = None;
    let no_prompt = super::readback(app.renderer.as_ref().unwrap(),&app.pose,Some(&wrong_world),None).unwrap();
    reject(app,r,c,&no_prompt,"missing-prompt-GPU",false,&mut log);
    assert_eq!(pixels(app),positive,"prompt/camera counterfactuals restore actual positive");
    exclusive(&group.join("NEGATIVES.txt"),log.as_bytes());
}
fn raw(document: &str) -> Option<Vec<u8>> {
    document.split_once("\nBYTE GROUPS (8 bytes max)\n")?.1
        .split_once("\nFULL SOURCE / METADATA:\n")?.0.lines()
        .filter_map(|l| l.strip_prefix("HEX ")).flat_map(str::split_whitespace)
        .map(|s| u8::from_str_radix(s,16).ok()).collect()
}
fn source_reader_frame(app: &App, source: &Path, frame: &[u8]) -> Result<()> {
    ui_contract(app,frame)?;
    let [w,h] = app.renderer.as_ref().unwrap().size;
    if !app.world.reading || app.world.tree_collapsed || app.scroll != 0 {
        return Err("MODEL_GPU_EXPECTED_INITIAL_EXPANDED_SOURCE_READER".into());
    }
    let sidebar = (w as f32*0.32).min(260.0)+12.0;
    let columns = ((w as f32-sidebar-48.0)/12.0).max(1.0) as usize;
    let rows = ((h as f32-80.0-84.0)/18.0).max(1.0) as usize;
    let mut wrapped = Vec::new();
    for line in app.world.document().map_err(|_| "MODEL_GPU_SOURCE_DOCUMENT_MISSING")?.lines() {
        let chars: Vec<_> = line.chars().collect();
        if chars.is_empty() { wrapped.push(String::new()); }
        for chunk in chars.chunks(columns) { wrapped.push(chunk.iter().collect::<String>()); }
    }
    let visible = wrapped.iter().take(rows).map(String::as_str).collect::<String>();
    let source = hair::folder_scene::display(source.as_os_str());
    if !visible.contains(NAME) || !visible.contains(&source) {
        return Err(format!("MODEL_GPU_FULL_SOURCE_NOT_VISIBLE rows={rows} columns={columns}"));
    }
    println!("MODEL_GPU_SOURCE_READER full_source_visible=true rows={rows} columns={columns}"); Ok(())
}
fn writer_guards(root: &Path, output: &Path) {
    let dir = output.join("writer-guards"); fs::create_dir(&dir).unwrap();
    let guard = resources::smoke_output(root,&dir).unwrap();
    let target = output.join("WRITER-SENTINEL.txt");
    exclusive(&target,b"owned sentinel: must not be replaced\n");
    std::os::unix::fs::symlink(&target,dir.join("office.png")).unwrap();
    assert!(guard.create_capture("office.png").is_err(),"post-preflight symlink must refuse");
    assert_eq!(fs::read(&target).unwrap(),b"owned sentinel: must not be replaced\n");
    assert!(fs::symlink_metadata(dir.join("office.png")).unwrap().file_type().is_symlink());
    assert!(guard.create_capture("model.png").is_err(),"fixed-name allowlist unchanged");
    exclusive(&output.join("WRITER-GUARDS.txt"),
        b"post-preflight-symlink=REFUSED\nsentinel=UNCHANGED\nnonallowlisted-name=REFUSED\nwriter-guards/office.png is a retained symlink, NOT a semantic capture.\n");
}
fn pin_inputs(root: &Path, output: &Path) {
    writer_guards(root,output); let mut pins = super::scoped_inputs(root);
    macro_rules! pin {
        ($name:literal) => {{
            let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/",$name));
            assert_eq!(fs::read(root.join($name)).unwrap().as_slice(),bytes.as_slice(),"rebuild changed source");
            writeln!(pins,"{} {}",super::digest(bytes),$name).unwrap();
        }};
    }
    pin!("src/assets/local_model.rs"); pin!("src/folder_native/object_preview.rs");
    pin!("src/folder_native/image.rs"); pin!("src/folder_native/raw_reader.rs"); pin!("src/local_model_preview_tests.rs");
    pin!("src/theme.rs"); pin!("src/folder_scene.rs"); pin!("src/host_scene.rs");
    pin!("src/text.rs"); pin!("src/render.rs"); pin!("src/shader.wgsl");
    pin!("packs/office.toml"); pin!("packs/wallDoorway.glb");
    let own = include_bytes!("model_preview_gpu_tests.rs");
    assert_eq!(fs::read(file!()).unwrap(),own);
    writeln!(pins,"{} external/corrected-r5/model_preview_gpu_tests.rs",super::digest(own)).unwrap();
    exclusive(&output.join("SCOPED-INPUTS.txt"),pins.as_bytes()); exclusive(&output.join("TEST-SOURCE.rs"),own);
}

#[test]
#[ignore = "explicit serialized offscreen Metal; absent HAIR_MODEL_PROOF_ROOT under owned TMPDIR; parent must inspect actual images"]
fn corrected_static_glb_depth_literal_before_e_raw_escape_frames() {
    raster_reference_controls();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).canonicalize().unwrap();
    let temp = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit owned TMPDIR")).canonicalize().unwrap();
    assert!(temp.starts_with(root.parent().unwrap()) && super::super::separate(&temp,&root));
    let requested = PathBuf::from(std::env::var_os("HAIR_MODEL_PROOF_ROOT").expect("fresh absent proof root"));
    assert!(requested.is_absolute()); assert_eq!(requested.parent().unwrap().canonicalize().unwrap(),temp);
    let output = temp.join(requested.file_name().unwrap());
    let mut leaf = requested.file_name().unwrap().to_os_string(); leaf.push(".fixture");
    let fixture_path = temp.join(leaf);
    for path in [&output,&fixture_path] {
        assert!(fs::symlink_metadata(path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound));
    }
    fs::create_dir(&output).unwrap(); fs::create_dir(&fixture_path).unwrap();
    let fixture = OwnedFixture(fixture_path); pin_inputs(&root,&output);
    let bytes = fixture_glb(); let model = hair::assets::decode(&bytes,10.0).expect("same valid original matte tetrahedron profile");
    assert_eq!(model.min,[1.0,2.0,3.0]); assert_eq!(model.max,[5.0,4.0,9.0]);
    assert_eq!(model.vertices.len(),12); assert!(model.textures.is_empty());
    for (v,index) in model.vertices.iter().zip(INDICES) { assert_eq!(v.position,POINTS[index]); assert!(source_vertex(v)); }
    exclusive(&output.join("FIXTURE.glb"),&bytes);
    let instance = render::instance(); let adapter = pollster::block_on(render::adapter(&instance,None)).unwrap();
    assert_eq!(adapter.get_info().backend,wgpu::Backend::Metal);
    exclusive(&output.join("ADAPTER.txt"),format!("{:?}\n",adapter.get_info()).as_bytes());
    let mut failures = Vec::new();
    for (label,size) in [("normal",[960,640]),("small",[640,480])] {
        let folder = fixture.0.join(label); fs::create_dir(&folder).unwrap(); exclusive(&folder.join(NAME),&bytes);
        let before = super::super::inventory(&folder);
        let declared = Snapshot { folder: folder.clone(), entries: vec![Entry { name: NAME.into(),path: folder.join(NAME),kind: Kind::File }] };
        assert_eq!(hair::local_explorer::snapshot(&folder).unwrap(),declared);
        let (declared_world,declared_host) = hair::folder_scene::build(&declared,0,false).unwrap();
        let reference = Reference::declared(&root,&declared,&declared_host);
        let mut app = super::super::app(&folder);
        assert_eq!(app.world.folder_view,declared_world.folder_view);
        assert_eq!(app.themes[app.theme_index].host,declared_host);
        super::super::aim(&mut app,"entry-0");
        assert_eq!(app.pose.position,[-4.5,-1.95]); assert_eq!(app.pose.yaw,-0.0);
        assert_eq!(app.pose.pitch,-0.87213653); assert!(app.world.explicit_target.is_none());
        let host = app.themes[app.theme_index].host.clone();
        let a = host.anchors.iter().find(|a| a.id == "entry-0").unwrap(); let mesh = expected_mesh(a);
        let binding = app.world.folder_view.as_ref().unwrap().bindings["entry-0"].clone();
        assert_eq!(binding.entry,declared.entries[0]); reference.state(&app).unwrap();
        assert_eq!(app.world.interaction.as_ref().unwrap().destination.as_deref(),
            Some(hair::folder_scene::display(folder.join(NAME).as_os_str()).as_str()));
        let current = app.world.current.clone(); let navigation = app.world.navigation.clone();
        let pose = (app.pose.position,app.pose.yaw,app.pose.pitch); let target = app.world.interaction.clone();
        let baseline = app.themes[app.theme_index].vertices_for(&app.world); assert!(!baseline.iter().any(source_vertex));
        let materials = app.themes[app.theme_index].textures.clone(); assert!(materials.is_empty());
        assert_eq!(app.themes[app.theme_index].source.canonicalize().unwrap(),root.join("packs/office.toml").canonicalize().unwrap());
        let mut renderer = pollster::block_on(render::Renderer::new(&adapter,wgpu::TextureFormat::Rgba8UnormSrgb,size,&baseline)).unwrap();
        renderer.set_materials(&materials).unwrap(); app.renderer = Some(renderer);
        let group = output.join(label); fs::create_dir(&group).unwrap();
        let contract = Contract::new(&app,&mesh,&reference);
        exclusive(&group.join("REFERENCE.vertices.bin"),bytemuck::cast_slice::<Vertex,u8>(&reference.vertices));
        let mut witnesses = format!("Independent declared CPU reference, NOT framebuffer evidence. All original non-edge band pixels classified; only exact 4x4 crosshair may cover source.\ncoverage_policy=CANDIDATE_8_FRACTIONAL_BITS_TOP_LEFT_UNVERIFIED_METAL\nsize={size:?}\nexact_f32_matrix_as_f64={:?}\noriginal_fixed={:?} nearest={:?}\nsmall_boundary_probe_279_181={:?}\n",
            contract.p.matrix.to_cols_array(), contract.fixed,
            contract.raster.nearest(contract.fixed.0,contract.fixed.1),
            if size == [640,480] { contract.raster.nearest(279,181) } else { None });
        for w in &contract.witnesses { writeln!(witnesses,"xy={},{} owner={:?} exterior={} rgb={:?}",w.x,w.y,w.owner,w.exterior,w.color).unwrap(); }
        exclusive(&group.join("REFERENCE-WITNESSES.txt"),witnesses.as_bytes());
        for stage in ["before-e","escape-return"] {
            wait_model(&mut app,"entry-0");
            // Mirror the normal redraw's interaction refresh, without uploading a scene.
            hair::interaction::refresh(&app.themes[app.theme_index].host, &app.pose, &mut app.world);
            app.update_hud();
            // Qualify normal publication BEFORE test-only prepare/reupload.
            let published = pixels(&app);
            let publication = model_frame(&app,&reference,&contract,&published);
            exclusive(&group.join(format!("{stage}.publication.rgba")),&published);
            exclusive(&group.join(format!("{stage}.publication.txt")),format!("publication={publication:?}\nNo manual reupload before this frame.\n").as_bytes());
            let published_no_ui = super::readback(app.renderer.as_ref().unwrap(),&app.pose,None,None).unwrap();
            prepare(&mut app);
            assert_eq!(super::readback(app.renderer.as_ref().unwrap(),&app.pose,None,None).unwrap(),published_no_ui,
                "normal publication and harness prepare identical no-UI actual GPU frame");
            assert_mesh(&app.themes[app.theme_index].vertices_for(&app.world),&mesh);
            assert_declared_scene(&app.themes[app.theme_index].vertices_for(&app.world),&reference);
            assert_eq!(outside_bytes(&app.themes[app.theme_index].vertices_for(&app.world),a),outside_bytes(&baseline,a),
                "outside-envelope geometry/filename/markers unchanged");
            let frame = pixels(&app); let scoped = model_frame(&app,&reference,&contract,&frame);
            retain(&app,&root,&group,stage,&frame,&scoped);
            if let Err(error) = publication { failures.push(format!("{label}/{stage}/publication: {error}")); }
            if stage == "before-e" && scoped.is_ok() { negatives(&mut app,&reference,&contract,&baseline,&frame,&group); }
            if let Err(error) = scoped { failures.push(format!("{label}/{stage}: {error}")); }
            if stage == "before-e" {
                super::super::tap(&mut app,KeyCode::KeyE); assert!(app.world.reading);
                let doc = app.world.document().unwrap(); assert!(doc.starts_with("RAW STORED BYTES\n"));
                assert_eq!(raw(doc).as_deref(),Some(bytes.as_slice()));
                assert!(doc.contains(&hair::folder_scene::display(binding.entry.path.as_os_str())));
                prepare(&mut app); let frame = pixels(&app); let scoped = ui_contract(&app,&frame).map(|_| ());
                retain(&app,&root,&group,"reader-raw",&frame,&scoped);
                if let Err(error) = scoped { failures.push(format!("{label}/reader-raw: {error}")); }
                super::super::tap(&mut app,KeyCode::Escape); assert!(!app.world.reading);
            }
        }
        super::super::tap(&mut app,KeyCode::KeyI); assert!(app.world.reading);
        assert!(app.world.document().unwrap().contains(&hair::folder_scene::display(binding.entry.path.as_os_str())));
        prepare(&mut app); let frame = pixels(&app); let scoped = source_reader_frame(&app,&binding.entry.path,&frame);
        retain(&app,&root,&group,"reader-source",&frame,&scoped);
        if let Err(error) = scoped { failures.push(format!("{label}/reader-source: {error}")); }
        super::super::tap(&mut app,KeyCode::Escape); wait_model(&mut app,"entry-0"); prepare(&mut app);
        assert_mesh(&app.themes[app.theme_index].vertices_for(&app.world),&mesh);
        assert_eq!(outside_bytes(&app.themes[app.theme_index].vertices_for(&app.world),a),outside_bytes(&baseline,a),
            "outside-envelope geometry/filename/markers unchanged");
        assert_eq!(app.themes[app.theme_index].host,host);
        assert_eq!(app.world.folder_view.as_ref().unwrap().bindings["entry-0"],binding);
        assert_eq!(app.world.current,current); assert_eq!(app.world.navigation,navigation);
        assert_eq!(app.world.interaction,target); assert_eq!((app.pose.position,app.pose.yaw,app.pose.pitch),pose);
        assert!(!host.blocked(app.pose.position)); host.check_routes(&[app.pose.position]).unwrap();
        assert!(materials.iter().zip(&app.themes[app.theme_index].textures).all(|(x,y)| Arc::ptr_eq(x,y)));
        assert_eq!(materials.len(),app.themes[app.theme_index].textures.len());
        assert_eq!(super::super::inventory(&folder),before); app.renderer.as_ref().unwrap().wait().unwrap(); drop(app);
        assert_eq!(super::super::inventory(&folder),before);
    }
    exclusive(&output.join("AUTOMATED-RESULT.txt"),format!(
        "scoped_failures={failures:#?}\nvision=PENDING_PARENT_ACTUAL_IMAGE_INSPECTION\nnative_window=NOT_EXECUTED\nNumerical/depth qualification is NOT visual comfort. Intrusive marker/kindlabel needs separate product followup.\nNo general GLB/sparse-world/package/safety acceptance.\n").as_bytes());
    assert!(failures.is_empty(),"MODEL_GPU_SCOPED_FAILURES {failures:?}");
}
