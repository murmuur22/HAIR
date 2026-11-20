//! Authentic MP4 -> held-folder App scheduler -> pre-E Texture/Theme CPU tracer.
//! That tracer injects no decoder result, cache, material, mesh or framebuffer.
//! The separate Theme-controlled test below covers decoration predicates only.
//! Still only: preserve E's raw default; no playback/audio/device/GPU claim.
use super::*;
use hair::assets::{Texture, Vertex};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};

const FIXTURE_SHA256: &str = "9638f62dd10ef140610a04dd00b132c943e444b407b10307e823824ad78b43a6";
const NAME: &str = "Video Case\né.MP4";
const COLORS: [[u8; 3]; 4] = [[220, 40, 40], [40, 200, 60], [40, 70, 220], [230, 200, 40]];
const EPS: f32 = 0.0001;
const DISPLAY: &str = "\"Video Case\\n\\u{e9}.MP4\"";
const FULL_KIND: &str = "MP4 suffix (tiny H264 still or fallback; E reads bytes; no playback)";

// Tests-only contract migration; the verbatim predecessor is in gpu/archive/.
// Exact multiplicity subtraction, never a color/region-based decoration mask.
fn subtract(scene: &[Vertex], owned: &[Vertex]) -> Vec<Vertex> {
    let mut remaining = owned.to_vec();
    let mut out = Vec::new();
    for v in scene {
        if let Some(i) = remaining
            .iter()
            .position(|e| bytemuck::bytes_of(e) == bytemuck::bytes_of(v))
        {
            remaining.remove(i);
        } else {
            out.push(*v);
        }
    }
    assert!(
        remaining.is_empty(),
        "VIDEO_DECLARED_OWNED_GEOMETRY_MISMATCH"
    );
    out
}
fn status(a: &hair::host_scene::Anchor, success: bool) -> Vec<Vertex> {
    // Independent declaration of the original marker, not actual Theme vertices.
    // Transform its corners (not a rounded new center) to retain exact f32 bits.
    let at = [a.at[0], a.at[1] + a.size[1] + 0.03, a.at[2]];
    let size = [0.45, 0.02, 0.2];
    let corners: Vec<[f32; 3]> = (0..8)
        .map(|i| {
            std::array::from_fn(|j| at[j] + size[j] * if i & (1 << j) == 0 { -0.5 } else { 0.5 })
        })
        .collect();
    let mut vertices = Vec::new();
    for face in [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ] {
        for i in [0, 1, 2, 0, 2, 3] {
            let mut position = corners[face[i]];
            if success {
                position = [
                    a.at[0] + 0.65 + (position[0] - a.at[0]) * 0.5,
                    position[1] - 0.59,
                    a.at[2] - a.size[2] * 0.5 - 0.35 + (position[2] - a.at[2]) * 0.3,
                ];
            }
            vertices.push(Vertex {
                position,
                color: [0.1, 0.8, 0.3],
                uv: [0.0; 2],
                texture: 0,
            });
        }
    }
    vertices
}
fn decorations(
    a: &hair::host_scene::Anchor,
    display: &str,
    kind: &str,
    success: bool,
) -> Vec<Vertex> {
    let at = if success {
        [
            a.at[0] + 0.65,
            a.at[1] + a.size[1] - 0.20,
            a.at[2] - a.size[2] * 0.5 - 0.35,
        ]
    } else {
        [
            a.at[0],
            a.at[1] + a.size[1] + 0.22,
            a.at[2] + a.size[2] * 0.5 + 0.02,
        ]
    };
    let mut out = status(a, success);
    out.extend(hair::text::object_label(display, at, 25));
    out.extend(hair::text::object_label(
        if success { "MP4 still" } else { kind },
        [at[0], at[1] + if success { -0.14 } else { -0.12 }, at[2]],
        if success { 25 } else { 28 },
    ));
    out
}
fn image_decorations(
    a: &hair::host_scene::Anchor,
    display: &str,
    concise: &str,
    image: &Texture,
) -> Vec<Vertex> {
    // Image-only source-plane group. Keep MP4's independent status contract.
    let usable = a.size[1] - 0.06_f32.min(a.size[1] * 0.5) * 2.0;
    let scale = (a.size[0] / image.width as f32).min(usable / image.height as f32);
    let top = a.at[1] + (a.size[1] + image.height as f32 * scale) * 0.5;
    let at = [a.at[0], top + 0.19, a.at[2] + a.size[2] * 0.45];
    let original_y = a.at[1] + a.size[1] + 0.03;
    let mut out = status(a, false);
    for v in &mut out {
        v.position[0] = at[0] + 0.5 + (v.position[0] - a.at[0]) * 0.5;
        v.position[1] = at[1] - 0.13 + (v.position[1] - original_y);
        v.position[2] = at[2] + (v.position[2] - a.at[2]) * 0.1;
    }
    out.extend(hair::text::object_label(display, at, 25));
    out.extend(hair::text::object_label(
        concise,
        [at[0], at[1] - 0.10, at[2]],
        25,
    ));
    out
}
fn image_preservation(
    scene: &[Vertex],
    baseline: &[Vertex],
    body: &[Vertex],
    a: &hair::host_scene::Anchor,
    image: &Texture,
    material: u32,
) {
    let old = decorations(
        a,
        "\"Replacement.png\"",
        "PNG suffix (lazy bounded preview or fallback; E checks source)",
        false,
    );
    let new = image_decorations(a, "\"Replacement.png\"", "PNG image", image);
    let actual = subtract(scene, &new);
    let original = subtract(baseline, &old);
    assert_eq!(outside(&actual, a), outside(&original, a));
    assert_eq!(
        bytemuck::cast_slice::<Vertex, u8>(&subtract(
            &actual,
            &declared_plane(a, image, material, true),
        )),
        bytemuck::cast_slice::<Vertex, u8>(&subtract(&original, body)),
        "VIDEO_REUSED_PNG_IMAGE_PLACARD_UNOWNED_REMAINDER",
    );
}
fn declared_plane(
    a: &hair::host_scene::Anchor,
    image: &Texture,
    material: u32,
    image_fit: bool,
) -> Vec<Vertex> {
    let scale = if image_fit {
        let usable_height = a.size[1] - 0.06_f32.min(a.size[1] * 0.5) * 2.0;
        (a.size[0] / image.width as f32).min(usable_height / image.height as f32)
    } else {
        (a.size[0] / image.width as f32).min(a.size[1] / image.height as f32)
    };
    let [w, h] = [image.width as f32 * scale, image.height as f32 * scale];
    let left = a.at[0] - w * 0.5;
    let top = a.at[1] + (a.size[1] + h) * 0.5;
    let z = a.at[2] + a.size[2] * 0.45;
    [
        (left, top, [0.0, 0.0]),
        (left + w, top, [1.0, 0.0]),
        (left + w, top - h, [1.0, 1.0]),
        (left, top, [0.0, 0.0]),
        (left + w, top - h, [1.0, 1.0]),
        (left, top - h, [0.0, 1.0]),
    ]
    .map(|(x, y, uv)| Vertex {
        position: [x, y, z],
        color: [1.0; 3],
        uv,
        texture: material | 0x8000_0000,
    })
    .to_vec()
}
fn fallback_body(root: &Path, theme: &hair::theme::Theme, id: &str) -> Vec<Vertex> {
    // The bundled pack loader supplies ONLY the unchanged original role body.
    // Skipping this one role removes no host solids (HostScene::vertices has no
    // anchor-dependent geometry). No preview/success setter is used as oracle.
    let mut host = theme.host.clone();
    host.anchors.iter_mut().find(|a| a.id == id).unwrap().role = "image".into();
    let skipped = hair::theme::Theme::load_for_folder_scene(
        &root.join("packs/office.toml"),
        &mut Default::default(),
        &host,
    )
    .unwrap();
    assert!(theme.textures.is_empty() && skipped.textures.is_empty());
    let owned = subtract(&theme.vertices, &skipped.vertices);
    let a = theme.host.anchors.iter().find(|a| a.id == id).unwrap();
    let body = subtract(&owned, &status(a, false));
    assert!(!body.is_empty() && outside(&body, a).is_empty());
    body
}
fn preservation(
    scene: &[Vertex],
    baseline: &[Vertex],
    body: &[Vertex],
    a: &hair::host_scene::Anchor,
    texture: &Texture,
    material: u32,
) {
    let old = decorations(a, DISPLAY, FULL_KIND, false);
    let new = decorations(a, DISPLAY, FULL_KIND, true);
    let actual = subtract(scene, &new);
    let original = subtract(baseline, &old);
    // Retain the original ordered outside-envelope assertion minus ONLY the
    // explicitly declared decorations. Any unexpected outside vertex fails.
    assert_eq!(outside(&actual, a), outside(&original, a));
    // Also compare the entire remainder, removing only the exact source body
    // and declared full plane. No unowned vertex, even inside the envelope, is
    // exempted. Colors, UVs, material, count, multiplicity and order all matter.
    assert_eq!(
        bytemuck::cast_slice::<Vertex, u8>(&subtract(
            &actual,
            &declared_plane(a, texture, material, false)
        )),
        bytemuck::cast_slice::<Vertex, u8>(&subtract(&original, body)),
        "VIDEO_UNOWNED_GEOMETRY_PRESERVATION",
    );
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn identity(m: &fs::Metadata) -> (u64, u64, u32, u64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.mode(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
    )
}
fn authentic_input() -> Vec<u8> {
    let path = PathBuf::from(
        std::env::var_os("HAIR_VIDEO_STILL_FIXTURE")
            .expect("SETUP: explicit authentic read-only MP4 input required"),
    );
    assert!(path.is_absolute(), "SETUP: absolute input required");
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(&path)
        .expect("SETUP: parent must provision the authentic MP4 before execution");
    let before = file.metadata().unwrap();
    assert!(before.is_file());
    assert!(before.len() > 0 && before.len() < hair::local_explorer::READER_CHUNK_BYTES as u64);
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(before.len() + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(identity(&before), identity(&file.metadata().unwrap()));
    assert_eq!(bytes.len() as u64, before.len());
    assert_eq!(
        digest(&bytes),
        FIXTURE_SHA256,
        "SETUP: authentic fixture hash mismatch"
    );
    assert_eq!(&bytes[4..8], b"ftyp");
    println!(
        "VIDEO_AUTHENTIC_INPUT path={} bytes={} sha256={}",
        path.display(),
        bytes.len(),
        digest(&bytes)
    );
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
    fn new(bytes: &[u8]) -> Self {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        let base = PathBuf::from(std::env::var_os("TMPDIR").expect("SETUP: explicit owned TMPDIR"))
            .canonicalize()
            .unwrap();
        assert!(base.starts_with(source.parent().unwrap()));
        assert!(!base.starts_with(&source) && !source.starts_with(&base));
        let path = base.join(format!("v{}", std::process::id()));
        fs::create_dir(&path).expect("SETUP: absent exclusive fixture directory required");
        let fixture = Self(path);
        for child in ["selected", "replacement"] {
            fs::create_dir(fixture.0.join(child)).unwrap();
        }
        exclusive(&fixture.0.join("selected").join(NAME), bytes);
        // A genuine one-pixel PNG is ONLY a subsequent same-slot identity control,
        // never a stand-in MP4 or a positive control for video decoding.
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[97, 31, 173]).unwrap();
            writer.finish().unwrap();
        }
        exclusive(&fixture.0.join("replacement/Replacement.png"), &png);
        println!("VIDEO_OWNED_FIXTURE {}", fixture.0.display());
        fixture
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn compiled_sources(root: &Path) {
    // These are literal source paths, not a guessed composite foundation pin.
    // Parent must separately inventory/pin the complete settled candidate.
    for (path, bytes) in [
        ("src/main.rs", include_bytes!("main.rs").as_slice()),
        (
            "src/local_video_still_tests.rs",
            include_bytes!("local_video_still_tests.rs").as_slice(),
        ),
        (
            "src/folder_native.rs",
            include_bytes!("folder_native.rs").as_slice(),
        ),
        (
            "src/folder_native/object_preview.rs",
            include_bytes!("folder_native/object_preview.rs").as_slice(),
        ),
        (
            "src/folder_native/image.rs",
            include_bytes!("folder_native/image.rs").as_slice(),
        ),
        (
            "src/folder_native/raw_reader.rs",
            include_bytes!("folder_native/raw_reader.rs").as_slice(),
        ),
        (
            "src/local_explorer.rs",
            include_bytes!("local_explorer.rs").as_slice(),
        ),
        (
            "src/folder_scene.rs",
            include_bytes!("folder_scene.rs").as_slice(),
        ),
        ("src/theme.rs", include_bytes!("theme.rs").as_slice()),
        ("src/assets.rs", include_bytes!("assets.rs").as_slice()),
        ("src/render.rs", include_bytes!("render.rs").as_slice()),
        ("src/shader.wgsl", include_bytes!("shader.wgsl").as_slice()),
        ("Cargo.toml", include_bytes!("../Cargo.toml").as_slice()),
        ("Cargo.lock", include_bytes!("../Cargo.lock").as_slice()),
    ] {
        assert_eq!(
            fs::read(root.join(path)).unwrap(),
            bytes,
            "SETUP: rebuild changed {path}"
        );
        println!("VIDEO_COMPILED_SOURCE {path} sha256={}", digest(bytes));
    }
}
fn tap(app: &mut App, key: KeyCode) {
    for down in [true, false] {
        assert!(!app.keyboard_input(Some(key), down, false, None).unwrap());
    }
}
fn aim(app: &mut App, name: &str) -> String {
    let view = app.world.folder_view.as_ref().unwrap();
    assert_eq!(view.bindings.len(), 1);
    let id = view
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == name)
        .unwrap()
        .0
        .clone();
    let host = &app.themes[app.theme_index].host;
    let a = host.anchors.iter().find(|a| a.id == id).unwrap();
    let inward = host.portal_inward(&id);
    app.pose = Pose {
        position: [a.at[0] + 1.05 * inward[0], a.at[2] + 1.05 * inward[1]],
        yaw: (-inward[0]).atan2(inward[1]),
        pitch: ((a.at[1] + a.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    assert!(!host.blocked(app.pose.position));
    app.world.explicit_target = None;
    hair::interaction::refresh(host, &app.pose, &mut app.world);
    assert_eq!(app.world.interaction.as_ref().unwrap().anchor, id);
    id
}
fn published(app: &mut App, id: &str, missing: &str) -> Arc<Texture> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_local_image().expect("normal App production poll");
        assert!(
            !app.world.reading,
            "preview must precede E or follow Escape"
        );
        if let Some(texture) = app.object_previews.cache.get(id) {
            return texture.clone();
        }
        // Reuse the existing read-only scheduler-drained observation. On the
        // current baseline MP4 is ineligible, so this is immediate semantic RED,
        // NOT a missing-feature assertion manufactured by waiting for a timeout.
        let (_, drained) = app.object_previews.test_model_fallback_progress();
        assert!(
            !drained,
            "{missing}: scheduler drained without source texture; status={}",
            app.object_previews.status
        );
        assert!(
            Instant::now() < deadline,
            "SETUP/PROGRESS: scheduler did not drain; NOT the intended RED"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn first_frame(texture: &Texture) {
    assert_eq!(
        [texture.width, texture.height],
        [160, 96],
        "VIDEO_FIRST_FRAME_DIMENSIONS"
    );
    assert_eq!(texture.rgba.len(), 160 * 96 * 4);
    let (pixels, remainder) = texture.rgba.as_chunks::<4>();
    assert!(remainder.is_empty() && pixels.iter().all(|p| p[3] == 255));
    // Source-derived first palette, asymmetric x=56/y=40. Check the interiors
    // of ALL four regions, excluding eight texels around lossy split/borders.
    // Additional small patches below distinguish the unequal split from halves.
    for y in 8..88 {
        for x in 8..152 {
            if (48..64).contains(&x) || (32..48).contains(&y) {
                continue;
            }
            let expected = COLORS[usize::from(x >= 56) + 2 * usize::from(y >= 40)];
            let pixel = pixels[y * 160 + x];
            assert!(
                pixel[..3]
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 28),
                "VIDEO_FIRST_FRAME_PIXELS xy={x},{y} expected={expected:?} actual={pixel:?}"
            );
        }
    }
    // Original native-proof centers, plus x=64 (not a half-width split) and
    // y=44 (not a half-height split). Three-pixel patches avoid the true edge.
    for (cx, cy, color) in [
        (24, 18, 0),
        (110, 18, 1),
        (24, 70, 2),
        (110, 70, 3),
        (64, 18, 1),
        (24, 44, 2),
    ] {
        for y in cy - 1..=cy + 1 {
            for x in cx - 1..=cx + 1 {
                let pixel = pixels[y * 160 + x];
                assert!(
                    pixel[..3]
                        .iter()
                        .zip(COLORS[color])
                        .all(|(a, b)| a.abs_diff(b) <= 28),
                    "VIDEO_ASYMMETRIC_FIRST_FRAME xy={x},{y} actual={pixel:?}"
                );
            }
        }
    }
}
fn outside(scene: &[Vertex], a: &hair::host_scene::Anchor) -> Vec<u8> {
    let vertices: Vec<_> = scene
        .iter()
        .filter(|v| {
            let [x, y, z] = v.position;
            x < a.at[0] - a.size[0] * 0.5 - EPS
                || x > a.at[0] + a.size[0] * 0.5 + EPS
                || y < a.at[1] - EPS
                || y > a.at[1] + a.size[1] + EPS
                || z < a.at[2] - a.size[2] * 0.5 - EPS
                || z > a.at[2] + a.size[2] * 0.5 + EPS
        })
        .copied()
        .collect();
    bytemuck::cast_slice(&vertices).to_vec()
}
fn world_consumer(app: &App, id: &str, texture: &Arc<Texture>, image_fit: bool) -> Vec<Vertex> {
    let theme = &app.themes[app.theme_index];
    let material = theme
        .textures
        .iter()
        .position(|t| Arc::ptr_eq(t, texture))
        .expect("normal App publication must reach Theme materials") as u32
        + 1;
    let scene = theme.vertices_for(&app.world);
    let plane: Vec<_> = scene
        .iter()
        .filter(|v| v.texture == (material | 0x8000_0000))
        .collect();
    assert_eq!(
        plane.len(),
        6,
        "source texture must reach world document plane"
    );
    let a = theme.host.anchors.iter().find(|a| a.id == id).unwrap();
    let complete: Vec<_> = plane.iter().map(|v| **v).collect();
    assert_eq!(
        bytemuck::cast_slice::<Vertex, u8>(&complete),
        bytemuck::cast_slice::<Vertex, u8>(&declared_plane(a, texture, material, image_fit)),
        "VIDEO_COMPLETE_SOURCE_PLANE_UNCHANGED",
    );
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for v in &plane {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }
    let width = max[0] - min[0];
    let height = max[1] - min[1];
    let scale = if image_fit {
        let usable_height = a.size[1] - 0.06_f32.min(a.size[1] * 0.5) * 2.0;
        (a.size[0] / texture.width as f32).min(usable_height / texture.height as f32)
    } else {
        (a.size[0] / texture.width as f32).min(a.size[1] / texture.height as f32)
    };
    assert!((width - texture.width as f32 * scale).abs() <= EPS);
    assert!((height - texture.height as f32 * scale).abs() <= EPS);
    assert!(((min[0] + max[0]) * 0.5 - a.at[0]).abs() <= EPS);
    assert!(((min[1] + max[1]) * 0.5 - a.at[1] - a.size[1] * 0.5).abs() <= EPS);
    assert!((max[2] - min[2]).abs() <= EPS);
    assert!(min[2] >= a.at[2] - a.size[2] * 0.5 - EPS);
    assert!(max[2] <= a.at[2] + a.size[2] * 0.5 + EPS);
    // UV orientation is tied to source row zero, not merely a four-color count.
    for v in plane {
        let expected = [
            (v.position[0] - min[0]) / width,
            (max[1] - v.position[1]) / height,
        ];
        assert!(
            v.uv.into_iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() <= EPS)
        );
    }
    scene
}
fn raw_bytes(document: &str) -> Option<Vec<u8>> {
    let body = document
        .split_once("\nBYTE GROUPS (8 bytes max)\n")?
        .1
        .split_once("\nFULL SOURCE / METADATA:\n")?
        .0;
    let mut bytes = Vec::new();
    for line in body.lines().filter_map(|line| line.strip_prefix("HEX ")) {
        for token in line.split_whitespace() {
            bytes.push(u8::from_str_radix(token, 16).ok()?);
        }
    }
    Some(bytes)
}

#[test]
fn authentic_mp4_before_e_source_rgba_fit_raw_escape_and_reused_slot() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    compiled_sources(&root);
    let bytes = authentic_input();
    let fixture = Fixture::new(&bytes);
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    assert!(app.renderer.is_none()); // CPU publication/geometry only, no output device.
    let selected = fixture.0.join("selected");
    let held = fixture.0.join("held");
    app.complete_folder(Ok(Some(selected.clone()))).unwrap();
    let id = aim(&mut app, NAME);
    let view = app.world.folder_view.as_ref().unwrap().clone();
    let bound = &view.bindings[&id];
    assert_eq!(bound.entry.path, selected.join(NAME));
    let host = app.themes[0].host.clone();
    let anchor = host.anchors.iter().find(|a| a.id == id).unwrap();
    let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
    let current = app.world.current.clone();
    let baseline = app.themes[0].vertices_for(&app.world);
    let body = fallback_body(&root, &app.themes[0], &id);
    let stored_base = bytemuck::cast_slice::<Vertex, u8>(&app.themes[0].vertices).to_vec();
    assert_eq!(hair::folder_scene::display(&bound.entry.name), DISPLAY);
    assert_eq!(hair::folder_scene::kind(&bound.entry), FULL_KIND);
    subtract(&baseline, &decorations(anchor, DISPLAY, FULL_KIND, false));
    let pack_textures = app.themes[0].textures.clone();
    // Only mutate our own fixture: display path is now absent. Product must use
    // the held directory, not reopen the stale displayed absolute path.
    fs::rename(&selected, &held).unwrap();
    let before = identity(&fs::metadata(held.join(NAME)).unwrap());
    assert!(app.object_previews.cache.is_empty());
    let texture = published(&mut app, &id, "VIDEO_STILL_BEFORE_E_MISSING");
    first_frame(&texture); // FIRST source-content assertion, no E/manual cache injection.
    let scene = world_consumer(&app, &id, &texture, false);
    preservation(
        &scene,
        &baseline,
        &body,
        anchor,
        &texture,
        pack_textures.len() as u32 + 1,
    );
    assert_eq!(
        bytemuck::cast_slice::<Vertex, u8>(&app.themes[0].vertices),
        stored_base
    );
    assert_eq!(app.themes[0].host, host);
    assert_eq!(app.world.folder_view.as_ref().unwrap(), &view);
    assert_eq!(app.world.current, current);
    assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
    assert_eq!(app.themes[0].textures.len(), pack_textures.len() + 1);
    assert!(
        pack_textures
            .iter()
            .zip(&app.themes[0].textures)
            .all(|(a, b)| Arc::ptr_eq(a, b))
    );

    tap(&mut app, KeyCode::KeyE);
    assert!(app.world.reading);
    let document = app.world.document().unwrap();
    assert!(
        document.starts_with("RAW STORED BYTES\n"),
        "MP4 E policy preserves existing raw default; no new still viewer"
    );
    assert_eq!(raw_bytes(document).as_deref(), Some(bytes.as_slice()));
    let raw = document.to_string();
    tap(&mut app, KeyCode::KeyI);
    let metadata = app.world.document().unwrap();
    assert!(metadata.contains(&hair::folder_scene::display(bound.entry.path.as_os_str())));
    assert!(metadata.contains(&hair::folder_scene::display(&bound.entry.name)));
    tap(&mut app, KeyCode::KeyI);
    assert_eq!(app.world.document().unwrap(), raw);
    tap(&mut app, KeyCode::Escape);
    assert!(!app.world.reading);
    let restored = published(&mut app, &id, "VIDEO_STILL_AFTER_ESCAPE_MISSING");
    first_frame(&restored);
    let returned = world_consumer(&app, &id, &restored, false);
    preservation(
        &returned,
        &baseline,
        &body,
        anchor,
        &restored,
        pack_textures.len() as u32 + 1,
    );
    assert_eq!(
        bytemuck::cast_slice::<Vertex, u8>(&app.themes[0].vertices),
        stored_base
    );
    assert_eq!(app.themes[0].host, host);
    assert_eq!(
        app.world.folder_view.as_ref().unwrap().bindings,
        view.bindings
    );
    assert_eq!(app.world.current, current);
    assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);

    app.complete_folder(Ok(Some(fixture.0.join("replacement"))))
        .unwrap();
    let next_id = aim(&mut app, "Replacement.png");
    assert_eq!(next_id, id, "must genuinely reuse the document slot");
    assert!(app.object_previews.cache.is_empty());
    let next_baseline = app.themes[0].vertices_for(&app.world);
    let next_body = fallback_body(&root, &app.themes[0], &next_id);
    let next_pack_count = app.themes[0].textures.len();
    let next = published(&mut app, &next_id, "REPLACEMENT_PNG_MISSING");
    assert_eq!([next.width, next.height], [1, 1]);
    assert_eq!(next.rgba, [97, 31, 173, 255]);
    assert!(!Arc::ptr_eq(&next, &texture) && !Arc::ptr_eq(&next, &restored));
    let next_scene = world_consumer(&app, &next_id, &next, true);
    let next_anchor = app.themes[0]
        .host
        .anchors
        .iter()
        .find(|a| a.id == next_id)
        .unwrap();
    image_preservation(
        &next_scene,
        &next_baseline,
        &next_body,
        next_anchor,
        &next,
        next_pack_count as u32 + 1,
    );
    assert_eq!(
        app.world.folder_view.as_ref().unwrap().bindings[&next_id]
            .entry
            .path,
        fixture.0.join("replacement/Replacement.png")
    );
    assert_eq!(identity(&fs::metadata(held.join(NAME)).unwrap()), before);
    assert_eq!(fs::read(held.join(NAME)).unwrap(), bytes);
    assert!(app.renderer.is_none());
    drop(app); // Existing owned-worker retirement before exclusive fixture cleanup.
}

#[test]
fn theme_controlled_admission_only_success_vs_pending_refused_and_png_decoration() {
    // Scope: decoration predicate ONLY. This explicit Theme-controlled one-pixel
    // map is NOT native decoding/publication proof. The authentic tracer above
    // remains the sole MP4 decoding positive, without cache/material injection.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let bytes = authentic_input();
    let fixture = Fixture::new(&bytes);
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    app.complete_folder(Ok(Some(fixture.0.join("selected"))))
        .unwrap();
    let id = aim(&mut app, NAME);
    assert!(app.object_previews.cache.is_empty());
    let anchor = app.themes[0]
        .host
        .anchors
        .iter()
        .find(|a| a.id == id)
        .unwrap()
        .clone();
    let baseline = app.themes[0].vertices_for(&app.world);
    let body = fallback_body(&root, &app.themes[0], &id);
    let ordinary = decorations(&anchor, DISPLAY, FULL_KIND, false);
    subtract(&baseline, &ordinary); // queued intent/MP4 suffix alone stays ordinary
    let pack_count = app.themes[0].textures.len();
    let marker = Arc::new(Texture {
        width: 1,
        height: 1,
        rgba: vec![97, 31, 173, 255],
    });
    let map = std::collections::BTreeMap::from([(id.clone(), marker.clone())]);
    // Real Theme material-capacity refusal: map intent exists but is not admitted.
    app.themes[0].textures.resize(63, marker.clone());
    app.themes[0].set_folder_previews(&map);
    assert_eq!(app.themes[0].textures.len(), 63);
    assert_eq!(
        bytemuck::cast_slice::<Vertex, u8>(&app.themes[0].vertices_for(&app.world)),
        bytemuck::cast_slice::<Vertex, u8>(&baseline),
        "VIDEO_REFUSED_MAP_MUST_KEEP_ENTIRE_ORDINARY_FALLBACK",
    );
    assert!(app.object_previews.cache.is_empty());
    assert!(
        app.world
            .interaction
            .as_ref()
            .unwrap()
            .prompt
            .contains(FULL_KIND)
    );
    tap(&mut app, KeyCode::KeyE);
    assert!(app.world.reading && !app.local_image_visible());
    let raw = app.world.document().unwrap().to_string();
    assert!(raw.starts_with("RAW STORED BYTES\n"));
    assert_eq!(raw_bytes(&raw).as_deref(), Some(bytes.as_slice()));
    tap(&mut app, KeyCode::KeyI);
    let source = app.world.document().unwrap();
    assert!(source.contains(DISPLAY) && source.contains(FULL_KIND));
    assert!(source.contains(&hair::folder_scene::display(
        fixture.0.join("selected").join(NAME).as_os_str(),
    )));
    tap(&mut app, KeyCode::KeyI);
    assert_eq!(app.world.document().unwrap(), raw);
    tap(&mut app, KeyCode::Escape);
    aim(&mut app, NAME);
    assert_eq!(
        bytemuck::cast_slice::<Vertex, u8>(&app.themes[0].vertices_for(&app.world)),
        bytemuck::cast_slice::<Vertex, u8>(&baseline),
    );
    // Release only our explicit capacity setup, then admit through the real
    // Theme method. Scheduler/cache stays empty: this is predicate scope only.
    app.themes[0].textures.truncate(pack_count);
    app.themes[0].set_folder_previews(&map);
    assert_eq!(app.themes[0].textures.len(), pack_count + 1);
    assert!(Arc::ptr_eq(app.themes[0].textures.last().unwrap(), &marker));
    let success = app.themes[0].vertices_for(&app.world);
    preservation(
        &success,
        &baseline,
        &body,
        &anchor,
        &marker,
        pack_count as u32 + 1,
    );
    assert!(app.object_previews.cache.is_empty());
    // Withdraw the admitted map: no extra predicate state may survive.
    app.themes[0].set_folder_previews(&Default::default());
    assert_eq!(app.themes[0].textures.len(), pack_count);
    assert_eq!(
        bytemuck::cast_slice::<Vertex, u8>(&app.themes[0].vertices_for(&app.world)),
        bytemuck::cast_slice::<Vertex, u8>(&baseline),
    );
    // Same slot, genuine regular PNG binding, explicitly Theme-admitted map:
    // expect the new PNG image placard, never private MP4 wording. This remains
    // decoration-predicate coverage, not a PNG decoding positive.
    app.complete_folder(Ok(Some(fixture.0.join("replacement"))))
        .unwrap();
    let next_id = aim(&mut app, "Replacement.png");
    assert_eq!(next_id, id);
    let next_baseline = app.themes[0].vertices_for(&app.world);
    let next_body = fallback_body(&root, &app.themes[0], &next_id);
    let next_anchor = app.themes[0]
        .host
        .anchors
        .iter()
        .find(|a| a.id == next_id)
        .unwrap()
        .clone();
    let next_pack_count = app.themes[0].textures.len();
    app.themes[0].set_folder_previews(&map);
    let png_scene = app.themes[0].vertices_for(&app.world);
    image_preservation(
        &png_scene,
        &next_baseline,
        &next_body,
        &next_anchor,
        &marker,
        next_pack_count as u32 + 1,
    );
    assert!(app.renderer.is_none() && app.object_previews.cache.is_empty());
    println!("VIDEO_THEME_CONTROLLED_DECORATION_ONLY no native decoding claim");
    drop(app);
}
