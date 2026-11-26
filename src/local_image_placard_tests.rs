//! FILE ONLY preparation: one actual App image-placard regression, not GPU proof.
//! Positive content comes only from the held current-room scheduler. Shadow Worlds
//! are read-only geometry/reader observations, never installed into App or Theme.
use super::*;
use glam::{DMat4, DVec2, DVec3, DVec4, Mat4, Vec3};
use hair::{
    assets::{Texture, Vertex},
    folder_scene,
    local_explorer::Snapshot,
};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs, io::Write, os::unix::fs::OpenOptionsExt};

const JPEG: &[u8] = include_bytes!("../tests/fixtures/synthetic-jpeg-quadrants.jpg");
const JPEG_SHA256: &str = "60ed51d80655f627b39efee03a0b5b3e67d1a87d2324a616769c17ed90979df1";
const PNG_NAME: &str = "Plcd7\"Q\n.PNG";
const PNG_DISPLAY: &str = "\"Plcd7\\\"Q\\n.PNG\"";
const JPEG_NAME: &str = "Plcd8\"R\n.JpEg";
const JPEG_DISPLAY: &str = "\"Plcd8\\\"R\\n.JpEg\"";
const SIZES: [[u32; 2]; 2] = [[960, 640], [640, 480]];
const AREA_EPS: f64 = 0.000001; // numerical area only, not an occlusion allowance

fn purple_png() -> Vec<u8> {
    // Genuine encoded 1x1 RGB source, matching the retained replacement's color.
    // It is an image fixture, not invented decoded JPEG or a fake framebuffer.
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[97, 31, 173]).unwrap();
        writer.finish().unwrap();
    }
    bytes
}
fn exclusive(path: &Path, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
}
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        let base = PathBuf::from(std::env::var_os("TMPDIR").expect("SETUP: explicit owned TMPDIR"))
            .canonicalize()
            .unwrap();
        assert!(base.starts_with(source.parent().unwrap()));
        assert!(!base.starts_with(&source) && !source.starts_with(&base));
        let path = base.join(format!("image-placard-{}", std::process::id()));
        fs::create_dir(&path).expect("SETUP: exclusive absent fixture directory");
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("IMAGE_PLACARD_FIXTURE_RETAINED {}", self.0.display());
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
    app.update_hud();
}
fn published(app: &mut App) -> Arc<Texture> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_local_image().unwrap();
        assert!(!app.world.reading);
        if let Some(texture) = app.object_previews.cache.get("entry-0") {
            let texture = texture.clone();
            refresh(app);
            return texture;
        }
        let (_, drained) = app.object_previews.test_model_fallback_progress();
        assert!(
            !drained,
            "SETUP_IMAGE_PUBLICATION: drained without image: {}",
            app.object_previews.status
        );
        assert!(
            Instant::now() < deadline,
            "SETUP_IMAGE_PROGRESS: not intended visibility RED"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn shadow(app: &App, snapshot: &Snapshot) -> World {
    let (mut world, host) = folder_scene::build(snapshot, 0, app.world.tree_collapsed).unwrap();
    assert_eq!(host, app.themes[app.theme_index].host);
    world.interaction = app.world.interaction.clone();
    world
}
// Ordered triangle multiset difference with multiplicity, as in the model test.
fn difference(a: &[Vertex], b: &[Vertex]) -> Vec<Vertex> {
    assert!(a.len().is_multiple_of(3) && b.len().is_multiple_of(3));
    let mut counts = HashMap::<Vec<u8>, usize>::new();
    for t in b.as_chunks::<3>().0 {
        *counts
            .entry(bytemuck::cast_slice::<Vertex, u8>(t).to_vec())
            .or_default() += 1;
    }
    let mut out = Vec::new();
    for t in a.as_chunks::<3>().0 {
        if let Some(n) = counts.get_mut(bytemuck::cast_slice::<Vertex, u8>(t))
            && *n > 0
        {
            *n -= 1;
        } else {
            out.extend_from_slice(t);
        }
    }
    out
}
fn exact_without(scene: &[Vertex], owned: &[Vertex]) -> Vec<u8> {
    let rest = difference(scene, owned);
    assert_eq!(
        scene.len() - rest.len(),
        owned.len(),
        "every declared owned triangle must be found with multiplicity"
    );
    bytemuck::cast_slice::<Vertex, u8>(&rest).to_vec()
}
fn same_literal(actual: &[Vertex], text: &str) {
    let reference = hair::text::object_label(text, [0.0; 3], 25);
    assert_eq!(
        actual.len(),
        reference.len(),
        "complete literal filename including quotes/escapes"
    );
    let origin = Vec3::from_array(actual[0].position);
    let base = Vec3::from_array(reference[0].position);
    let scale = Vec3::from_array(actual[1].position).distance(origin)
        / Vec3::from_array(reference[1].position).distance(base);
    assert!(scale.is_finite() && scale > 0.0);
    for (a, e) in actual.iter().zip(reference) {
        // Topology and distances allow translation/rotation/uniform scale, NOT
        // a hardcoded future placard origin, glyph height or video/model setter.
        assert!(
            (Vec3::from_array(a.position).distance(origin)
                - Vec3::from_array(e.position).distance(base) * scale)
                .abs()
                < 0.0001
        );
    }
}
fn parts(app: &App, snapshot: &Snapshot, scene: &[Vertex], display: &str) -> [Vec<Vertex>; 2] {
    let mut unbound = shadow(app, snapshot);
    unbound.folder_view.as_mut().unwrap().bindings.clear();
    let labels: Vec<_> = difference(scene, &app.themes[0].vertices_for(&unbound))
        .into_iter()
        .filter(|v| v.texture == 0 && v.color == [0.6, 1.0, 0.7])
        .collect();
    // Theme emits the one document's filename followed by kind. Unbinding only
    // a shadow isolates the COMPLETE label ranges, including shared quote ink;
    // unlike an empty-name differential it cannot lose the literal delimiters.
    let filename_count = hair::text::object_label(display, [0.0; 3], 25).len();
    assert!(
        labels.len() > filename_count,
        "image filename and type must both remain present"
    );
    let filename = labels[..filename_count].to_vec();
    same_literal(&filename, display);
    let kind = labels[filename_count..].to_vec();
    let status: Vec<_> = scene
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
        status.is_empty(),
        "class image replaces the duplicate status cuboid; public I metadata remains checked"
    );
    [filename, kind]
}
fn source_plane(app: &App, texture: &Arc<Texture>) -> Vec<Vertex> {
    let theme = &app.themes[0];
    let material = theme
        .textures
        .iter()
        .position(|t| Arc::ptr_eq(t, texture))
        .expect("SETUP: real scheduler payload must reach Theme admission")
        as u32
        + 1;
    let a = theme
        .host
        .anchors
        .iter()
        .find(|a| a.id == "entry-0")
        .unwrap();
    let usable_height = 0.85_f32;
    let scale = (0.8 / texture.width as f32).min(usable_height / texture.height as f32);
    if [texture.width, texture.height] == [96, 64] {
        let old_scale = (a.size[0] / texture.width as f32).min(a.size[1] / texture.height as f32);
        assert_eq!(
            scale.to_bits(),
            old_scale.to_bits(),
            "width-limited JPEG exact old fit"
        );
    }
    let (w, h) = (texture.width as f32 * scale, texture.height as f32 * scale);
    let left = a.at[0] - w * 0.5;
    let top = a.at[1] + a.size[1] + 0.12 + h;
    let z = a.at[2];
    let vertex = |x, y, uv| Vertex {
        position: [x, y, z],
        color: [1.0; 3],
        uv,
        texture: material | 0x8000_0000,
    };
    let tl = vertex(left, top, [0.0, 0.0]);
    let tr = vertex(left + w, top, [1.0, 0.0]);
    let br = vertex(left + w, top - h, [1.0, 1.0]);
    let bl = vertex(left, top - h, [0.0, 1.0]);
    let expected = vec![tl, tr, br, tl, br, bl];
    let actual: Vec<_> = theme
        .vertices_for(&app.world)
        .into_iter()
        .filter(|v| v.texture == (material | 0x8000_0000))
        .collect();
    assert_eq!(
        bytemuck::cast_slice::<Vertex, u8>(&actual),
        bytemuck::cast_slice::<Vertex, u8>(&expected),
        "source plane, proportional fit, UV orientation and material route are immutable"
    );
    expected
}
// Same uploaded f32 visitor matrix, then f64 clipping/intersection arithmetic as
// local_model_presentation_tests. No camera/layout change to evade the contract.
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
fn screen(p: DVec4, [w, h]: [u32; 2]) -> DVec2 {
    assert!(p.w > 0.0);
    DVec2::new(
        (p.x / p.w + 1.0) * 0.5 * f64::from(w),
        (1.0 - p.y / p.w) * 0.5 * f64::from(h),
    )
}
fn projected(t: &[Vertex], m: DMat4, size: [u32; 2]) -> Vec<DVec2> {
    let mut poly: Vec<_> = t
        .iter()
        .map(|v| m * DVec3::from_array(v.position.map(f64::from)).extend(1.0))
        .collect();
    for i in 0..6 {
        let mut next = Vec::new();
        for j in 0..poly.len() {
            let a = poly[j];
            let b = poly[(j + 1) % poly.len()];
            let (da, db) = (plane(a, i), plane(b, i));
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
fn area(p: &[DVec2]) -> f64 {
    (0..p.len())
        .map(|i| cross(p[i], p[(i + 1) % p.len()]))
        .sum::<f64>()
        * 0.5
}
fn overlap(subject: &[DVec2], clip: &[DVec2]) -> f64 {
    if clip.len() < 3 {
        return 0.0;
    }
    let sign = area(clip).signum();
    let mut poly = subject.to_vec();
    for i in 0..clip.len() {
        let a = clip[i];
        let edge = clip[(i + 1) % clip.len()] - a;
        let mut next = Vec::new();
        for j in 0..poly.len() {
            let (u, v) = (poly[j], poly[(j + 1) % poly.len()]);
            let (du, dv) = (sign * cross(edge, u - a), sign * cross(edge, v - a));
            if du >= 0.0 {
                next.push(u);
            }
            if (du >= 0.0) != (dv >= 0.0) {
                next.push(u.lerp(v, du / (du - dv)));
            }
        }
        poly = next;
    }
    area(&poly).abs()
}
fn projection(
    app: &App,
    source: &[Vertex],
    parts: &[Vec<Vertex>; 2],
    size: [u32; 2],
    stage: &str,
    failures: &mut Vec<String>,
) {
    let m = matrix(&app.pose, size);
    let clip = |v: &Vertex| m * DVec3::from_array(v.position.map(f64::from)).extend(1.0);
    assert!(
        source
            .iter()
            .all(|v| (0..6).all(|i| plane(clip(v), i) > 0.0)),
        "whole useful source remains in view"
    );
    let source: Vec<_> = source
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| projected(t, m, size))
        .collect();
    assert!(source.iter().all(|p| p.len() == 3 && area(p).abs() > 1.0));
    let mut ui = hair::text::overlay(&app.world, size[0], size[1], 0);
    // CPU App has no Renderer. Mirror its EXACT current closed-folder HUD
    // composition (main.rs::update_hud + render.rs::draw_frame), not a prompt-only
    // approximation. Source-only assumption; actual Metal composition is a gate.
    let feedback = format!(
        "{} | {}\nU Parent | R Back | F5 refresh | I source\n{}",
        app.world.folder_view.as_ref().unwrap().summary(),
        app.themes[0].name,
        app.local_status
    );
    ui.extend(hair::text::pack_overlay(&feedback, size[0], size[1]));
    let ui: Vec<_> = ui
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| {
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
            (poly, crosshair)
        })
        .collect();
    if ui
        .iter()
        .any(|(p, crosshair)| !crosshair && source.iter().any(|s| overlap(p, s) > AREA_EPS))
    {
        failures.push(format!("{stage}/{size:?}: IMAGE_CONTENT_UI_OBSTRUCTION"));
    }
    let projected_parts: Vec<Vec<_>> = parts
        .iter()
        .map(|mesh| {
            mesh.as_chunks::<3>()
                .0
                .iter()
                .map(|t| projected(t, m, size))
                .collect()
        })
        .collect();
    let extent = |polys: &[Vec<DVec2>]| {
        polys.iter().flatten().fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |b, p|
            [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)])
    };
    println!("IMAGE_PLACARD_PROJECTION {stage}/{size:?} source={:?} filename={:?} type={:?}", extent(&source), extent(&projected_parts[0]), extent(&projected_parts[1]));
    for (index, (role, mesh)) in ["filename", "type", "status"]
        .into_iter()
        .zip(parts)
        .enumerate()
    {
        let polygons = &projected_parts[index];
        if mesh.iter().any(|v| (0..6).any(|i| plane(clip(v), i) < 0.0)) {
            failures.push(format!(
                "{stage}/{size:?}: IMAGE_METADATA_OFFSCREEN role={role}"
            ));
        }
        if polygons
            .iter()
            .any(|p| source.iter().any(|s| overlap(p, s) > AREA_EPS))
        {
            failures.push(format!(
                "{stage}/{size:?}: IMAGE_METADATA_CONTENT_OVERLAP role={role}"
            ));
        }
        // ALL actual overlay triangles, including filled HUD/tree/feedback panels;
        // there is no crosshair exemption for filename/type/status decorations.
        if polygons
            .iter()
            .any(|p| ui.iter().any(|(u, _)| overlap(p, u) > AREA_EPS))
        {
            let blockers: Vec<_> = ui.iter().filter(|(u, _)| polygons.iter().any(|p| overlap(p, u) > AREA_EPS)).map(|(u, _)| u.clone()).collect();
            println!("IMAGE_PLACARD_BLOCKERS {stage}/{size:?} {role} region={:?}", extent(&blockers));
            failures.push(format!(
                "{stage}/{size:?}: IMAGE_METADATA_OPAQUE_UI_OVERLAP role={role}"
            ));
        }
        if role == "status" {
            if polygons
                .iter()
                .map(|p| area(p).abs())
                .fold(0.0_f64, f64::max)
                < 1.0
            {
                failures.push(format!("{stage}/{size:?}: IMAGE_STATUS_SUBPIXEL"));
            }
        } else {
            assert!(mesh.len().is_multiple_of(6));
            // One projected pixel per bitmap-cell axis is only an anti-evasion
            // floor, NOT a human legibility certification or a fixed layout.
            if mesh.as_chunks::<6>().0.iter().any(|cell| {
                let p = projected(&cell[..3], m, size);
                p.len() != 3 || p[0].distance(p[1]) < 1.0 || p[1].distance(p[2]) < 1.0
            }) {
                failures.push(format!(
                    "{stage}/{size:?}: IMAGE_LABEL_SUBPIXEL_OR_CLIPPED role={role}"
                ));
            }
        }
        for (other, other_polygons) in projected_parts.iter().enumerate().take(3).skip(index + 1) {
            if polygons
                .iter()
                .any(|p| other_polygons.iter().any(|q| overlap(p, q) > AREA_EPS))
            {
                failures.push(format!(
                    "{stage}/{size:?}: IMAGE_DECORATIONS_OVERLAP roles={index},{other}"
                ));
            }
        }
    }
    // Concise type: at most the same 25-column horizontal bitmap budget as a
    // filename, independent of future wording/location/font scale. Full public
    // capability text remains separately checked through actual I below.
    let kind = &parts[1];
    let step = Vec3::from_array(kind[1].position).distance(Vec3::from_array(kind[0].position));
    let width = kind
        .iter()
        .map(|v| v.position[0])
        .fold(f32::NEG_INFINITY, f32::max)
        - kind
            .iter()
            .map(|v| v.position[0])
            .fold(f32::INFINITY, f32::min);
    if width > step * 6.0 * 25.0 + 0.0001 {
        failures.push(format!("{stage}/{size:?}: IMAGE_TYPE_NOT_CONCISE"));
    }
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
fn opened(app: &mut App, format: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_local_image().unwrap();
        if !app.local_image_pending() {
            break;
        }
        assert!(Instant::now() < deadline, "SETUP_OPENED_IMAGE_PROGRESS");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(app.local_image_visible());
    assert!(
        app.world
            .document()
            .unwrap()
            .starts_with(&format!("{format} IMAGE VIEW\n"))
    );
}
fn body(world: &World, size: [u32; 2], scroll: usize) -> Vec<u8> {
    let ink: Vec<_> = hair::text::overlay(world, size[0], size[1], scroll)
        .into_iter()
        .filter(|v| v.color == [0.9, 0.96, 0.91])
        .collect();
    bytemuck::cast_slice::<Vertex, u8>(&ink).to_vec()
}
fn inspect(app: &mut App, snapshot: &Snapshot, needles: &[&str]) {
    tap(app, KeyCode::KeyI);
    assert!(app.world.reading && app.world.reader_link.is_none());
    assert!(!app.local_image_visible());
    let document = app.world.document().unwrap().to_string();
    for needle in needles {
        let tail = document
            .find(needle)
            .expect("actual I retains complete literal source/kind")
            + needle.len()
            - 1;
        for size in SIZES {
            let mut erased = shadow(app, snapshot);
            erased.reading = true;
            erased.folder_view.as_mut().unwrap().reader = document.clone();
            erased
                .folder_view
                .as_mut()
                .unwrap()
                .reader
                .replace_range(tail..tail + 1, " ");
            let mut seen = false;
            loop {
                if body(&app.world, size, app.scroll) != body(&erased, size, app.scroll) {
                    seen = true;
                    break;
                }
                let before = app.scroll;
                tap(app, KeyCode::ArrowDown);
                if app.scroll == before {
                    break;
                }
            }
            assert!(
                seen,
                "I's full literal tail reachable through actual scroll: {needle:?} {size:?}"
            );
            while app.scroll > 0 {
                tap(app, KeyCode::ArrowUp);
            }
        }
    }
    tap(app, KeyCode::Escape);
    assert!(!app.world.reading);
}

#[test]
fn local_image_placard_is_visible_at_ordinary_approach() {
    placard_trace(None);
}
fn placard_trace(metal: Option<&Path>) {
    // Tiny local oracle controls, not a new broad test pile.
    let square = [DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y];
    assert_eq!(overlap(&square, &square.map(|p| p + DVec2::X)), 0.0);
    assert!((overlap(&square, &square) - 1.0).abs() < 1e-12);
    let mut reversed = square;
    reversed.reverse();
    assert!((overlap(&square, &reversed) - 1.0).abs() < 1e-12);
    assert!(overlap(&square, &square.map(|p| p + DVec2::new(0.999, 0.0))) > 0.0009);
    assert_eq!(
        format!("{:x}", Sha256::digest(JPEG)),
        JPEG_SHA256,
        "authentic existing JPEG fixture"
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = Fixture::new();
    let png = purple_png();
    let mut failures = Vec::new();
    for (format, name, display, bytes, dimensions) in [
        ("PNG", PNG_NAME, PNG_DISPLAY, png.as_slice(), [1, 1]),
        ("JPEG", JPEG_NAME, JPEG_DISPLAY, JPEG, [96, 64]),
    ] {
        assert!(display.len() <= 25 && display.is_ascii());
        assert!(bytes.len() < hair::local_explorer::READER_CHUNK_BYTES);
        // Decoder control is an observation only; NEVER inserted into a cache.
        let reference =
            hair::assets::decode_local_image(bytes).expect("SETUP: genuine encoded fixture");
        assert_eq!([reference.width, reference.height], dimensions);
        if format == "PNG" {
            assert_eq!(reference.rgba, [97, 31, 173, 255]);
        }
        let selected = fixture.0.join(format!("{format}-selected"));
        let held = fixture.0.join(format!("{format}-held"));
        fs::create_dir(&selected).unwrap();
        exclusive(&selected.join(name), bytes);
        let theme =
            hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default())
                .unwrap();
        let mut app = App::new(
            root.clone(),
            vec![theme],
            0,
            World::load(&root).unwrap(),
            false,
        );
        assert!(app.renderer.is_none());
        app.complete_folder(Ok(Some(selected.clone()))).unwrap();
        if app.world.tree_collapsed {
            // Actual Settings control, not M (which owns the map).
            for code in [
                KeyCode::Escape,
                KeyCode::ArrowDown,
                KeyCode::Enter,
                KeyCode::Enter, // Settings > Player Settings
                KeyCode::ArrowDown,
                KeyCode::ArrowDown,
                KeyCode::ArrowDown,
                KeyCode::Enter,
                KeyCode::Escape,
                KeyCode::Escape,
                KeyCode::Escape,
            ] {
                tap(&mut app, code);
            }
        }
        assert!(!app.world.tree_collapsed);
        let snapshot = hair::local_explorer::snapshot(&selected).unwrap();
        assert_eq!(snapshot.entries.len(), 1);
        let view = app.world.folder_view.as_ref().unwrap().clone();
        let bound = &view.bindings["entry-0"];
        assert_eq!(bound.entry.name, name);
        assert_eq!(bound.entry.path, selected.join(name));
        assert_eq!(folder_scene::display(&bound.entry.name), display);
        let full_kind = folder_scene::kind(&bound.entry);
        assert!(full_kind.starts_with(&format!("{format} suffix (")));
        let source = folder_scene::display(bound.entry.path.as_os_str());
        let host = app.themes[0].host.clone();
        let a = host.anchors.iter().find(|a| a.id == "entry-0").unwrap();
        assert_eq!(
            (a.at, a.size, a.role.as_str()),
            ([-4.5, 0.0, -3.0], [0.8, 0.8, 0.6], "document")
        );
        let inward = host.portal_inward(&a.id);
        app.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [a.at[0] + 1.05 * inward[0], a.at[2] + 1.05 * inward[1]],
            yaw: (-inward[0]).atan2(inward[1]),
            pitch: ((a.at[1]
                + a.size[1]
                + 0.12
                + (0.8 / dimensions[0] as f32).min(0.85 / dimensions[1] as f32)
                    * dimensions[1] as f32
                    * 0.5
                - 1.65)
                / 1.05)
                .atan(),
        };
        assert!(host.file_anchors.contains(&a.id));
        assert!(!host.blocked(app.pose.position) && !host.blocked([a.at[0], a.at[2]]));
        host.check_routes(&[app.pose.position]).unwrap();
        app.world.explicit_target = None;
        refresh(&mut app);
        let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
        let current = app.world.current.clone();
        let navigation = app.world.navigation.clone();
        let stored_base = bytemuck::cast_slice::<Vertex, u8>(&app.themes[0].vertices).to_vec();
        assert!(app.object_previews.cache.is_empty());
        // Own fixture mutation BEFORE first polling: displayed absolute path is
        // absent. Positive PNG/JPEG must come through the already held directory.
        fs::rename(&selected, &held).unwrap();
        assert!(!selected.exists());
        let mut stable_remainder = None;
        if metal.is_some() { metal_witness::attach(&mut app); }
        for stage in ["before-e", "escape-return", "i-return"] {
            let texture = published(&mut app);
            assert_eq!([texture.width, texture.height], dimensions);
            assert_eq!(
                texture.rgba, reference.rgba,
                "held source pixels, no injected cache positive"
            );
            if format == "JPEG" {
                // Authentic fixture has asymmetric splits x=40/y=24, not halves.
                for (x, y, rgb) in [
                    (14_usize, 10_usize, [255_u8, 0, 0]),
                    (78, 10, [0, 255, 0]),
                    (14, 52, [0, 0, 255]),
                    (78, 52, [255, 255, 0]),
                    (48, 10, [0, 255, 0]),
                    (14, 29, [0, 0, 255]),
                ] {
                    let at = (y * 96 + x) * 4;
                    assert!(
                        texture.rgba[at..at + 3]
                            .iter()
                            .zip(rgb)
                            .all(|(actual, expected)| actual.abs_diff(expected) <= 48),
                        "genuine JPEG source interiors"
                    );
                }
            }
            assert_eq!(app.themes[0].host, host);
            assert_eq!(
                app.world.folder_view.as_ref().unwrap().bindings,
                view.bindings
            );
            assert_eq!(app.world.current, current);
            assert_eq!(app.world.navigation, navigation);
            assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
            assert!(app.world.explicit_target.is_none());
            assert_eq!(
                bytemuck::cast_slice::<Vertex, u8>(&app.themes[0].vertices),
                stored_base
            );
            let target = app.world.interaction.as_ref().unwrap();
            assert_eq!(target.anchor, "entry-0");
            assert_eq!(target.destination.as_deref(), Some(source.as_str()));
            assert!(
                target.prompt.contains(display)
                    && target.prompt.contains("E")
                    && target.prompt.contains("I")
            );
            let scene = app.themes[0].vertices_for(&app.world);
            let plane = source_plane(&app, &texture);
            let decorations = parts(&app, &snapshot, &scene, display);
            let kind_prefix = hair::text::object_label(format, [0.0; 3], 25).len();
            assert!(
                decorations[1].len() >= kind_prefix,
                "recognizable image format must remain in concise type"
            );
            same_literal(&decorations[1][..kind_prefix], format);
            let mut owned = plane.clone();
            for part in &decorations {
                owned.extend_from_slice(part);
            }
            let remainder = exact_without(&scene, &owned);
            if let Some(first) = &stable_remainder {
                assert_eq!(
                    &remainder, first,
                    "ordered unowned scene unchanged across lifecycle"
                );
            } else {
                stable_remainder = Some(remainder);
            }
            for size in SIZES {
                projection(
                    &app,
                    &plane,
                    &decorations,
                    size,
                    &format!("{format}/{stage}"),
                    &mut failures,
                );
                if let Some(output) = metal {
                    metal_witness::observe(&mut app, output, &format!("{format}-{stage}-{}x{}", size[0], size[1]), size, &decorations);
                }
            }
            println!(
                "IMAGE_PLACARD_OBSERVED {format}/{stage} pose={:?} name={display} cumulative_visibility_failures={}",
                app.pose,
                failures.len()
            );
            if stage == "before-e" {
                tap(&mut app, KeyCode::KeyE);
                assert!(app.world.reading);
                assert!(
                    app.world
                        .document()
                        .unwrap()
                        .starts_with("RAW STORED BYTES\n"),
                    "binary raw default before async opened pixels"
                );
                opened(&mut app, format);
                // Preserve existing image controls through actual input. Detailed
                // transform/pixel acceptance remains the unchanged image tests.
                for key in [
                    KeyCode::Equal,
                    KeyCode::ArrowRight,
                    KeyCode::ArrowDown,
                    KeyCode::Minus,
                    KeyCode::Digit0,
                ] {
                    tap(&mut app, key);
                    assert!(app.local_image_visible());
                }
                // FIX1 C: the opened image viewer owns the wheel; no reader
                // scroll or remainder accrues behind it.
                let (scroll, view) = (app.scroll, app.world.document().unwrap().to_string());
                for delta in [
                    winit::event::MouseScrollDelta::LineDelta(0.0, -3.0),
                    winit::event::MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(
                        0.0, -13.5,
                    )),
                ] {
                    app.mouse_wheel(delta);
                }
                assert!(app.local_image_visible());
                assert_eq!((app.scroll, app.wheel_rows), (scroll, 0.0));
                assert_eq!(app.world.document().unwrap(), view);
                tap(&mut app, KeyCode::KeyI);
                let metadata = app.world.document().unwrap();
                assert!(
                    metadata.contains(display)
                        && metadata.contains(&source)
                        && metadata.contains(full_kind)
                );
                assert!(!app.local_image_visible());
                tap(&mut app, KeyCode::KeyI);
                assert!(app.local_image_visible());
                tap(&mut app, KeyCode::KeyV);
                let document = app.world.document().unwrap().to_string();
                assert_eq!(raw(&document).as_deref(), Some(bytes));
                assert!(!app.local_image_visible());
                tap(&mut app, KeyCode::ArrowRight);
                assert_eq!(
                    app.world.document().unwrap(),
                    document,
                    "small raw EOF inert"
                );
                tap(&mut app, KeyCode::Escape);
                assert!(!app.world.reading && !app.local_image_visible());
            } else if stage == "escape-return" {
                inspect(&mut app, &snapshot, &[display, &source, full_kind]);
            }
        }
        assert_eq!(fs::read(held.join(name)).unwrap(), bytes);
        drop(app); // join owned workers before deleting any fixture
    }
    // All setup/content/interaction preservation checks precede the sole intended
    // regression failure. No fixture/import/timeout is labelled presentation RED.
    assert!(
        failures.is_empty(),
        "IMAGE_PLACARD_PRESENTATION_RED\n{}",
        failures.join("\n")
    );
}

#[path = "owner_r1b_caption_metal_tests.rs"]
mod metal_witness;
