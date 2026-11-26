//! FILE ONLY RED preparation. CPU geometry, not framebuffer/legibility certification.
//! One actual App -> scheduler -> Theme tracer, using the corrected-r5 fixture/pose.
//! Shadow Worlds below are observations only; never assigned to App or uploaded.
use super::*;
use glam::{DMat4, DVec2, DVec3, DVec4, Mat4, Vec3};
use hair::{assets::Vertex, folder_scene, local_explorer::Snapshot};
use std::{collections::HashMap, fs, io::Write, os::unix::fs::OpenOptionsExt};

const NAME: &str = "m.glb";
const DISPLAY: &str = "\"m.glb\"";
const FULL_KIND: &str = "GLB suffix (bounded static mesh or fallback; E reads bytes)";
const POINTS: [[f32; 3]; 4] = [
    [1.0, 2.0, 3.0],
    [5.0, 2.0, 3.0],
    [1.0, 4.0, 3.0],
    [2.0, 2.5, 9.0],
];
const INDICES: [usize; 12] = [0, 2, 1, 0, 1, 3, 1, 2, 3, 2, 0, 3];

fn fixture_bytes() -> Vec<u8> {
    let mut bin = Vec::new();
    for p in POINTS {
        for n in p {
            bin.extend_from_slice(&n.to_le_bytes());
        }
    }
    for _ in POINTS {
        for n in [0.0_f32, 1.0, 0.0] {
            bin.extend_from_slice(&n.to_le_bytes());
        }
    }
    for i in INDICES {
        bin.extend_from_slice(&(i as u16).to_le_bytes());
    }
    assert_eq!(bin.len(), 120);
    let mut json = br#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"buffers":[{"byteLength":120}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":48,"target":34962},{"buffer":0,"byteOffset":48,"byteLength":48,"target":34962},{"buffer":0,"byteOffset":96,"byteLength":24,"target":34963}],"accessors":[{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","min":[1,2,3],"max":[5,4,9]},{"bufferView":1,"componentType":5126,"count":4,"type":"VEC3"},{"bufferView":2,"componentType":5123,"count":12,"type":"SCALAR"}],"materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0.91,0.13,0.67,1],"metallicFactor":0,"roughnessFactor":1},"doubleSided":true}],"meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1},"indices":2,"material":0,"mode":4}]}]}"#.to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bytes = Vec::new();
    for n in [0x4654_6c67_u32, 2, (28 + json.len() + bin.len()) as u32] {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0x4e4f_534a_u32.to_le_bytes());
    bytes.extend_from_slice(&json);
    bytes.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0x004e_4942_u32.to_le_bytes());
    bytes.extend_from_slice(&bin);
    bytes
}

struct Fixture(PathBuf);
impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        let temp = PathBuf::from(std::env::var_os("TMPDIR").expect("owned TMPDIR required"))
            .canonicalize()
            .unwrap();
        assert!(temp.starts_with(source.parent().unwrap()));
        assert!(!temp.starts_with(&source) && !source.starts_with(&temp));
        let path = temp.join(format!("model-presentation-{}", std::process::id()));
        fs::create_dir(&path).expect("exclusive absent fixture; do not reuse a failed run");
        let fixture = Self(path);
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(fixture.0.join(NAME))
            .unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
        fixture
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("MODEL_PRESENTATION_FIXTURE_RETAINED {}", self.0.display());
        } else {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn tap(app: &mut App, key: KeyCode) {
    for down in [true, false] {
        assert!(!app.keyboard_input(Some(key), down, false, None).unwrap());
    }
}
fn refresh(app: &mut App) {
    hair::interaction::refresh(&app.themes[app.theme_index].host, &app.pose, &mut app.world);
    app.update_hud(); // same ordinary redraw refresh as corrected-r5, no scene upload
}
fn wait_model(app: &mut App) -> Vec<Vertex> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_local_image().unwrap();
        assert!(!app.world.reading);
        if app.object_previews.models.contains_key("entry-0") {
            refresh(app);
            return app.themes[app.theme_index].vertices_for(&app.world);
        }
        assert!(
            Instant::now() < deadline,
            "SETUP_MODEL_PUBLICATION: {}",
            app.object_previews.status
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn source_color() -> [f32; 3] {
    let light = 0.45 + 0.55 * Vec3::Y.dot(Vec3::new(-0.3, 0.85, 0.4).normalize());
    [0.91, 0.13, 0.67].map(|c| c * light)
}
fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    a.into_iter().zip(b).all(|(a, b)| (a - b).abs() < 0.0001)
}
fn is_source(v: &Vertex) -> bool {
    v.texture == 0 && near(v.color, source_color())
}
fn reference_mesh(a: &hair::host_scene::Anchor) -> Vec<Vertex> {
    let size = [0.8, 0.85, 0.28];
    let scale = (0..3)
        .map(|i| size[i] / [4.0, 2.0, 6.0][i])
        .fold(f32::INFINITY, f32::min);
    let center = [a.at[0], a.at[1] + a.size[1] + 0.12 + scale, a.at[2]];
    INDICES
        .iter()
        .map(|&index| Vertex {
            position: std::array::from_fn(|i| {
                center[i] + (POINTS[index][i] - [3.0, 3.0, 6.0][i]) * scale
            }),
            color: source_color(),
            uv: [0.0; 2],
            texture: 0,
        })
        .collect()
}
fn check_mesh(scene: &[Vertex], expected: &[Vertex]) {
    let actual: Vec<_> = scene.iter().filter(|v| is_source(v)).collect();
    assert_eq!(
        actual.len(),
        expected.len(),
        "source mesh cannot be removed/shrunk to evade overlap"
    );
    for (a, e) in actual.iter().zip(expected) {
        assert!(near(a.position, e.position) && near(a.color, e.color));
        assert_eq!((a.uv, a.texture), (e.uv, e.texture));
    }
}

// Triangle multiset subtraction, not vertex offsets or color-only label picking.
fn difference(a: &[Vertex], b: &[Vertex]) -> Vec<Vertex> {
    let mut counts = HashMap::<Vec<u8>, usize>::new();
    assert!(a.len().is_multiple_of(3) && b.len().is_multiple_of(3));
    for t in b.as_chunks::<3>().0.iter() {
        *counts
            .entry(bytemuck::cast_slice::<Vertex, u8>(t).to_vec())
            .or_default() += 1;
    }
    let mut out = Vec::new();
    for t in a.as_chunks::<3>().0.iter() {
        let key = bytemuck::cast_slice::<Vertex, u8>(t);
        if let Some(n) = counts.get_mut(key)
            && *n > 0
        {
            *n -= 1;
        } else {
            out.extend_from_slice(t);
        }
    }
    out
}
fn shadow(snapshot: &Snapshot, app: &App) -> World {
    let (mut world, host) = folder_scene::build(snapshot, 0, app.world.tree_collapsed).unwrap();
    assert_eq!(host, app.themes[app.theme_index].host);
    world.interaction = app.world.interaction.clone();
    world
}
fn decorations(app: &App, snapshot: &Snapshot, scene: &[Vertex]) -> [Vec<Vertex>; 2] {
    let theme = &app.themes[app.theme_index];
    let mut nameless = shadow(snapshot, app);
    nameless
        .folder_view
        .as_mut()
        .unwrap()
        .bindings
        .get_mut("entry-0")
        .unwrap()
        .entry
        .name = format!("{}.glb", " ".repeat(64)).into();
    // The existing quoted/25-column name producer emits only a far-left quote
    // and far-right truncation glyph for this probe. Unlike display("") == "\"\"",
    // its ink cannot subtract real central filename cells. No disk/name mutation;
    // path/kind/model/anchor stay identical. Validate the full literal below.
    let filename = difference(scene, &theme.vertices_for(&nameless));
    let mut unbound = shadow(snapshot, app);
    unbound.folder_view.as_mut().unwrap().bindings.clear();
    let labels: Vec<_> = difference(scene, &theme.vertices_for(&unbound))
        .into_iter()
        .filter(|v| !is_source(v))
        .collect();
    let kind = difference(&labels, &filename);
    // This isolated office scene has exactly one document. The status cuboid's
    // producer uses this unique flat material; floor/portal/source/labels do not.
    // Presence is guarded, but placement/size/count are deliberately NOT pinned.
    let marker: Vec<_> = scene
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|t| {
            t.iter()
                .all(|v| v.texture == 0 && v.color == [0.1, 0.8, 0.3])
        })
        .flatten()
        .copied()
        .collect();
    assert!(
        marker.is_empty(),
        "class item replaces obsolete duplicate status cuboid; full status remains in I"
    );
    for (role, mesh) in [("filename", &filename), ("type", &kind)] {
        assert!(
            !mesh.is_empty(),
            "MODEL_PRESENTATION_METADATA_REMOVED role={role}"
        );
    }
    // Same literal glyph topology, up to translation/rotation/uniform scaling;
    // no fixed product placement, glyph height or text/world scale is required.
    let literal = hair::text::object_label(DISPLAY, [0.0; 3], 25);
    assert_eq!(
        filename.len(),
        literal.len(),
        "complete literal filename glyphs"
    );
    let origin = Vec3::from_array(filename[0].position);
    let base = Vec3::from_array(literal[0].position);
    let ratio = Vec3::from_array(filename[1].position).distance(origin)
        / Vec3::from_array(literal[1].position).distance(base);
    assert!(ratio.is_finite() && ratio > 0.0);
    for (a, e) in filename.iter().zip(&literal) {
        let actual = Vec3::from_array(a.position).distance(origin);
        let expected = Vec3::from_array(e.position).distance(base) * ratio;
        assert!(
            (actual - expected).abs() < 0.0001,
            "literal filename shape changed"
        );
    }
    [filename, kind]
}

fn matrix(p: &Pose, [w, h]: [u32; 2]) -> DMat4 {
    let eye = Vec3::new(p.position[0], 1.65, p.position[1]);
    let forward = Vec3::new(
        p.yaw.sin() * p.pitch.cos(),
        p.pitch.sin(),
        -p.yaw.cos() * p.pitch.cos(),
    );
    let m = Mat4::perspective_rh(65.0_f32.to_radians(), w as f32 / h as f32, 0.05, 50.0)
        * Mat4::look_to_rh(eye, forward, Vec3::Y);
    DMat4::from_cols_array(&m.to_cols_array().map(f64::from))
}
fn plane(p: DVec4, i: usize) -> f64 {
    match i {
        0 => p.x + p.w,
        1 => p.w - p.x,
        2 => p.y + p.w,
        3 => p.w - p.y,
        4 => p.z,
        5 => p.w - p.z,
        _ => unreachable!(),
    }
}
fn screen(p: DVec4, size: [u32; 2]) -> DVec2 {
    assert!(p.w > 0.0);
    DVec2::new(
        (p.x / p.w + 1.0) * 0.5 * f64::from(size[0]),
        (1.0 - p.y / p.w) * 0.5 * f64::from(size[1]),
    )
}
fn projected(t: &[Vertex], m: DMat4, size: [u32; 2]) -> Vec<DVec2> {
    let mut poly: Vec<_> = t
        .iter()
        .map(|v| m * DVec3::from_array(v.position.map(f64::from)).extend(1.0))
        .collect();
    for i in 0..6 {
        let mut next = Vec::new();
        if poly.is_empty() {
            break;
        }
        for j in 0..poly.len() {
            let a = poly[j];
            let b = poly[(j + 1) % poly.len()];
            let da = plane(a, i);
            let db = plane(b, i);
            if da >= 0.0 {
                next.push(a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                next.push(a.lerp(b, da / (da - db)));
            }
        }
        poly = next;
    }
    poly.into_iter().map(|p| screen(p, size)).collect()
}
fn cross(a: DVec2, b: DVec2) -> f64 {
    a.x * b.y - a.y * b.x
}
fn signed_area(p: &[DVec2]) -> f64 {
    (0..p.len())
        .map(|i| cross(p[i], p[(i + 1) % p.len()]))
        .sum::<f64>()
        * 0.5
}
fn overlap(subject: &[DVec2], clip: &[DVec2]) -> f64 {
    if clip.len() < 3 {
        return 0.0;
    }
    let sign = signed_area(clip).signum();
    let mut poly = subject.to_vec();
    for i in 0..clip.len() {
        let a = clip[i];
        let edge = clip[(i + 1) % clip.len()] - a;
        let mut next = Vec::new();
        if poly.is_empty() {
            break;
        }
        for j in 0..poly.len() {
            let u = poly[j];
            let v = poly[(j + 1) % poly.len()];
            let du = sign * cross(edge, u - a);
            let dv = sign * cross(edge, v - a);
            if du >= 0.0 {
                next.push(u);
            }
            if (du >= 0.0) != (dv >= 0.0) {
                next.push(u.lerp(v, du / (du - dv)));
            }
        }
        poly = next;
    }
    signed_area(&poly).abs()
}
fn check_projection(
    app: &App,
    expected: &[Vertex],
    parts: &[Vec<Vertex>; 2],
    size: [u32; 2],
    stage: &str,
    failures: &mut Vec<String>,
) {
    let m = matrix(&app.pose, size);
    let source: Vec<_> = expected
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| projected(t, m, size))
        .collect();
    assert!(
        source
            .iter()
            .all(|p| p.len() == 3 && signed_area(p).abs() > 0.0)
    );
    for v in expected {
        let p = m * DVec3::from_array(v.position.map(f64::from)).extend(1.0);
        assert!(
            (0..6).all(|i| plane(p, i) > 0.0),
            "whole reference source must remain in view"
        );
    }
    for (role, mesh) in ["filename", "type", "status"].into_iter().zip(parts) {
        let mut first = None;
        let mut max_area = 0.0_f64;
        for (index, t) in mesh.as_chunks::<3>().0.iter().enumerate() {
            let poly = projected(t, m, size);
            let area = source
                .iter()
                .map(|s| overlap(&poly, s))
                .fold(0.0_f64, f64::max);
            // Numerical area epsilon only (one millionth pixel squared), NOT a
            // readability/occlusion allowance and NOT a source-edge mask.
            if area > 0.000001 {
                first.get_or_insert(index);
                max_area = max_area.max(area);
            }
        }
        if let Some(triangle) = first {
            failures.push(format!("{stage}/{size:?}: MODEL_PRESENTATION_INTERIOR_OVERLAP role={role} triangle={triangle} max_pair_area_px2={max_area:.6}"));
        }
        // Reachable concise metadata cannot extend beyond the approach viewport.
        // Check unclipped geometry, so clipping cannot manufacture a passing label.
        let in_view = mesh.iter().all(|v| {
            let p = m * DVec3::from_array(v.position.map(f64::from)).extend(1.0);
            (0..6).all(|i| plane(p, i) >= 0.0)
        });
        if !in_view {
            failures.push(format!(
                "{stage}/{size:?}: MODEL_PRESENTATION_METADATA_OFFSCREEN role={role}"
            ));
        } else if role == "status" {
            let max_area = mesh
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| signed_area(&projected(t, m, size)).abs())
                .fold(0.0_f64, f64::max);
            if max_area < 1.0 {
                failures.push(format!(
                    "{stage}/{size:?}: MODEL_PRESENTATION_SUBPIXEL_STATUS"
                ));
            }
        } else {
            // Each actual bitmap ink cell needs a physical pixel on BOTH axes.
            // This is an anti-subpixel evasion floor, not human readability proof
            // or an arbitrary exact font/world scale. Fit and non-overlap remain
            // independent requirements; actual GPU/human review is still needed.
            assert!(mesh.len().is_multiple_of(6));
            if mesh.as_chunks::<6>().0.iter().any(|cell| {
                let p: Vec<_> = cell
                    .iter()
                    .map(|v| {
                        screen(
                            m * DVec3::from_array(v.position.map(f64::from)).extend(1.0),
                            size,
                        )
                    })
                    .collect();
                p[0].distance(p[1]) < 1.0 || p[1].distance(p[2]) < 1.0
            }) {
                failures.push(format!(
                    "{stage}/{size:?}: MODEL_PRESENTATION_SUBPIXEL_LABEL role={role}"
                ));
            }
        }
    }
    // Actual closed-world overlay, not a reconstructed prompt. Only the actual
    // centre crosshair may cover source; no filename/type/prompt/UI masks.
    let ui = hair::text::overlay(&app.world, size[0], size[1], 0);
    for t in ui.as_chunks::<3>().0.iter() {
        let poly: Vec<_> = t
            .iter()
            .map(|v| {
                DVec2::new(
                    (f64::from(v.position[0]) + 1.0) * 0.5 * f64::from(size[0]),
                    (1.0 - f64::from(v.position[1])) * 0.5 * f64::from(size[1]),
                )
            })
            .collect();
        let center = DVec2::new(f64::from(size[0]) * 0.5, f64::from(size[1]) * 0.5);
        let crosshair = t.iter().all(|v| v.color == [0.2, 1.0, 0.4])
            && poly
                .iter()
                .all(|p| (p.x - center.x).abs() <= 2.001 && (p.y - center.y).abs() <= 2.001);
        if !crosshair && source.iter().any(|s| overlap(&poly, s) > 0.000001) {
            failures.push(format!(
                "{stage}/{size:?}: MODEL_PRESENTATION_UI_OBSTRUCTION"
            ));
            break;
        }
    }
}

fn body(world: &World, size: [u32; 2], scroll: usize) -> Vec<Vertex> {
    hair::text::overlay(world, size[0], size[1], scroll)
        .into_iter()
        .filter(|v| v.color == [0.9, 0.96, 0.91])
        .collect()
}
fn inspect_metadata(app: &mut App, snapshot: &Snapshot, source: &str) {
    tap(app, KeyCode::KeyI);
    assert!(app.world.reading && app.world.reader_link.is_none());
    let document = app.world.document().unwrap().to_string();
    for needle in [
        DISPLAY,
        source,
        FULL_KIND,
        "No writes, execution, default-open or link following.",
    ] {
        let start = document
            .find(needle)
            .expect("actual I retains complete literal metadata/capability");
        let tail = start + needle.len() - 1; // ASCII fixture/metadata, including closing quote
        for size in [[960, 640], [640, 480]] {
            let mut erased = shadow(snapshot, app);
            erased.reading = true;
            erased.folder_view.as_mut().unwrap().reader = document.clone();
            erased
                .folder_view
                .as_mut()
                .unwrap()
                .reader
                .replace_range(tail..tail + 1, " ");
            let mut seen = false;
            // Actual keyboard scrolling, never assign App.scroll or reader text.
            // App without renderer uses 960x640 key bounds; all metadata tails
            // must nevertheless be rendered/reachable at BOTH requested sizes.
            loop {
                let ink = body(&app.world, size, app.scroll);
                if bytemuck::cast_slice::<Vertex, u8>(&ink)
                    != bytemuck::cast_slice::<Vertex, u8>(&body(&erased, size, app.scroll))
                {
                    assert!(!ink.is_empty());
                    assert!(ink.iter().all(|v| (-1.0..=1.0).contains(&v.position[0])
                        && (-1.0..=1.0).contains(&v.position[1])));
                    seen = true;
                    break;
                }
                let previous = app.scroll;
                tap(app, KeyCode::ArrowDown);
                if app.scroll == previous {
                    break;
                }
            }
            assert!(
                seen,
                "actual I tail glyph must be reachable: {needle:?} size={size:?}"
            );
            while app.scroll > 0 {
                tap(app, KeyCode::ArrowUp);
            }
        }
    }
    tap(app, KeyCode::Escape);
    assert!(!app.world.reading);
}
fn raw(document: &str) -> Option<Vec<u8>> {
    document
        .split_once("\nBYTE GROUPS (8 bytes max)\n")?
        .1
        .split_once("\nFULL SOURCE / METADATA:\n")?
        .0
        .lines()
        .filter_map(|l| l.strip_prefix("HEX "))
        .flat_map(str::split_whitespace)
        .map(|s| u8::from_str_radix(s, 16).ok())
        .collect()
}

#[test]
fn local_static_model_content_is_unobstructed_at_approach() {
    // Oracle controls in the ONE focused test: boundary contact isn't interior;
    // a tiny positive interior overlap must fail; winding is irrelevant.
    let square = [DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y];
    assert_eq!(overlap(&square, &square.map(|p| p + DVec2::X)), 0.0);
    assert!((overlap(&square, &square) - 1.0).abs() < 1e-12);
    let mut reverse = square;
    reverse.reverse();
    assert!((overlap(&square, &reverse) - 1.0).abs() < 1e-12);
    assert!(overlap(&square, &square.map(|p| p + DVec2::new(0.999, 0.0))) > 0.0009);
    let bytes = fixture_bytes();
    let decoded =
        hair::assets::decode(&bytes, 10.0).expect("valid original corrected-r5 static GLB");
    assert_eq!(
        (decoded.min, decoded.max),
        ([1.0, 2.0, 3.0], [5.0, 4.0, 9.0])
    );
    assert_eq!(decoded.vertices.len(), 12);
    assert!(decoded.textures.is_empty());
    for (v, index) in decoded.vertices.iter().zip(INDICES) {
        assert_eq!(v.position, POINTS[index]);
        assert!(is_source(v));
    }
    let fixture = Fixture::new(&bytes);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    app.complete_folder(Ok(Some(fixture.0.clone()))).unwrap();
    let snapshot = hair::local_explorer::snapshot(&fixture.0).unwrap();
    assert_eq!(snapshot.entries.len(), 1);
    let view = app.world.folder_view.as_ref().unwrap().clone();
    let bound = view.bindings["entry-0"].clone();
    assert_eq!(bound.entry.name, NAME);
    assert_eq!(bound.entry.path, fixture.0.join(NAME));
    assert_eq!(folder_scene::display(&bound.entry.name), DISPLAY);
    let host = app.themes[0].host.clone();
    let a = host.anchors.iter().find(|a| a.id == "entry-0").unwrap();
    // Corrected-r5 foundation's host-owned anchor/envelope, not a proposed
    // metadata location or scale. Moving the host must not evade this RED.
    assert_eq!(a.at, [-4.5, 0.0, -3.0]);
    assert_eq!(a.size, [0.8, 0.8, 0.6]);
    assert_eq!(a.role, "document");
    assert_eq!(host.room, [-10.5, -6.0, 10.5, 6.0]);
    assert!(host.open_platform && host.geometry.is_empty() && host.overhead.is_empty());
    let n = host.portal_inward(&a.id);
    app.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [a.at[0] + 1.05 * n[0], a.at[2] + 1.05 * n[1]],
        yaw: (-n[0]).atan2(n[1]),
        pitch: ((a.at[1] + a.size[1] + 0.12 + 0.28 / 6.0 - 1.65) / 1.05).atan(),
    };
    assert!(!host.blocked(app.pose.position));
    app.world.explicit_target = None;
    refresh(&mut app);
    let current = app.world.current.clone();
    let navigation = app.world.navigation.clone();
    let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
    let source = folder_scene::display(bound.entry.path.as_os_str());
    let expected = reference_mesh(a);
    let mut failures = Vec::new();
    // Collect intended presentation failures until AFTER metadata and lifecycle
    // checks, so the baseline teaches both sizes and all preserved contracts.
    for stage in ["before-e", "escape-return", "inspect-return"] {
        let scene = wait_model(&mut app);
        check_mesh(&scene, &expected);
        assert_eq!(app.themes[0].host, host); // envelope, collision, routes, anchor
        assert_eq!(
            app.world.folder_view.as_ref().unwrap().bindings,
            view.bindings
        );
        assert_eq!(app.world.current, current);
        assert_eq!(app.world.navigation, navigation);
        assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
        assert!(app.world.explicit_target.is_none());
        let target = app.world.interaction.as_ref().unwrap();
        assert_eq!(target.anchor, "entry-0");
        assert_eq!(target.destination.as_deref(), Some(source.as_str()));
        assert!(
            target.prompt.contains(DISPLAY)
                && target.prompt.contains("E")
                && target.prompt.contains("I")
        );
        let parts = decorations(&app, &snapshot, &scene);
        for size in [[960, 640], [640, 480]] {
            check_projection(&app, &expected, &parts, size, stage, &mut failures);
        }
        if stage == "before-e" {
            tap(&mut app, KeyCode::KeyE);
            assert!(app.world.reading);
            let doc = app.world.document().unwrap();
            assert!(doc.starts_with("RAW STORED BYTES\n"));
            assert_eq!(raw(doc).as_deref(), Some(bytes.as_slice()));
            assert!(doc.contains(&source));
            tap(&mut app, KeyCode::Escape);
            assert!(!app.world.reading);
        } else if stage == "escape-return" {
            inspect_metadata(&mut app, &snapshot, &source);
        }
    }
    assert_eq!(fs::read(fixture.0.join(NAME)).unwrap(), bytes);
    drop(app); // join actual owned workers before fixture destruction
    assert!(
        failures.is_empty(),
        "MODEL_PRESENTATION_RED\n{}",
        failures.join("\n")
    );
}
