//! Test-only JPEG object tracer. No production support or injected preview state.
//! Helpers are local because the existing sibling tracers' helpers are private;
//! their modules, test bodies, fixture, capture writer and guards stay unchanged.
use super::*;
use glam::{Mat4, Vec3};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};

const JPEG: &[u8] = include_bytes!("../tests/fixtures/synthetic-jpeg-quadrants.jpg");
const JPEG_SHA256: &str = "60ed51d80655f627b39efee03a0b5b3e67d1a87d2324a616769c17ed90979df1";
// Unchanged fixture's saturated interiors; NOT a generated half-split image.
// Source dimensions 96x64, real boundaries x=40, y=24. Lossy tolerance stays 48.
const COLORS: [[u8; 3]; 4] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]];
const PATCHES: [(usize, f32, f32); 8] = [
    (0, 0.15, 0.15),
    (0, 0.30, 0.25),
    (1, 0.50, 0.18),
    (1, 0.82, 0.26),
    (2, 0.18, 0.46),
    (2, 0.30, 0.82),
    (3, 0.55, 0.55),
    (3, 0.82, 0.82),
];
// Harness observation bound only: NOT an asserted production scheduler budget,
// CPU/kernel deadline, interruption guarantee, or heap/worker qualification.
const OBSERVATION_FRAMES: usize = 256;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
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
fn comment_decoy() -> Vec<u8> {
    // Same provenance and codestream as the existing opened tracer. Valid COM
    // after SOI changes exact RAW identity only; decoded colors are identical.
    let comment = b"TEST-OWNED DISPLAY-PATH IMPOSTOR; SAME CODESTREAM";
    assert!(JPEG.starts_with(&[0xff, 0xd8]));
    let mut bytes = JPEG[..2].to_vec();
    bytes.extend_from_slice(&[0xff, 0xfe]);
    bytes.extend_from_slice(&u16::try_from(comment.len() + 2).unwrap().to_be_bytes());
    bytes.extend_from_slice(comment);
    bytes.extend_from_slice(&JPEG[2..]);
    assert_ne!(bytes.as_slice(), JPEG);
    bytes
}
type FileState = (OsString, u64, u64, u32, u64, i64, i64, Vec<u8>);
fn inventory(dir: &Path) -> (u32, Vec<FileState>) {
    let directory = fs::symlink_metadata(dir).unwrap();
    assert!(directory.is_dir() && !directory.file_type().is_symlink());
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let m = fs::symlink_metadata(entry.path()).unwrap();
        assert!(m.is_file() && !m.file_type().is_symlink());
        files.push((
            entry.file_name(),
            m.dev(),
            m.ino(),
            m.mode(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            fs::read(entry.path()).unwrap(),
        ));
    }
    files.sort();
    (directory.mode(), files)
}
fn separate(a: &Path, b: &Path) -> bool {
    !a.starts_with(b) && !b.starts_with(a)
}
fn tap(app: &mut App, code: KeyCode) {
    for down in [true, false] {
        assert!(!app.keyboard_input(Some(code), down, false, None).unwrap());
    }
}
fn matrix(p: &Pose, [w, h]: [u32; 2]) -> Mat4 {
    // Same visitor camera as Renderer and the existing source-pixel object test.
    let eye = Vec3::new(p.position[0], 1.65, p.position[1]);
    let forward = Vec3::new(
        p.yaw.sin() * p.pitch.cos(),
        p.pitch.sin(),
        -p.yaw.cos() * p.pitch.cos(),
    );
    Mat4::perspective_rh(65.0_f32.to_radians(), w as f32 / h as f32, 0.05, 50.0)
        * Mat4::look_to_rh(eye, forward, Vec3::Y)
}
fn project(m: Mat4, p: Vec3, [w, h]: [u32; 2]) -> [f32; 2] {
    let clip = m * p.extend(1.0);
    assert!(clip.w > 0.05, "source envelope must be in front of camera");
    let ndc = clip.truncate() / clip.w;
    [
        (ndc.x + 1.0) * 0.5 * w as f32,
        (1.0 - ndc.y) * 0.5 * h as f32,
    ]
}
fn near(pixel: &[u8], color: [u8; 3]) -> bool {
    pixel[..3]
        .iter()
        .zip(color)
        .all(|(a, b)| a.abs_diff(b) <= 48)
}
fn frame(app: &mut App) -> Result<Vec<u8>> {
    // Exact normal-redraw production poll/geometry/material/HUD consumers.
    // No App::redraw call: that needs a native surface and ActiveEventLoop.
    app.poll_local_image()?;
    hair::interaction::refresh(&app.themes[app.theme_index].host, &app.pose, &mut app.world);
    app.update_hud();
    let vertices = app.themes[app.theme_index].vertices_for(&app.world);
    let r = app.renderer.as_mut().unwrap();
    r.set_scene(&vertices);
    let [width, height] = r.size;
    let texture = r.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("JPEG object actual framebuffer"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    r.draw(
        &texture.create_view(&Default::default()),
        &app.pose,
        &app.world,
        0,
    )?;
    let pitch = (width * 4).div_ceil(256) * 256;
    let buffer = r.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("JPEG object actual readback"),
        size: (pitch * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = r.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    r.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    r.wait()?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| e.to_string())?;
    let pixels = mapped
        .chunks(pitch as usize)
        .flat_map(|row| row[..width as usize * 4].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}
fn retain(root: &Path, group: &Path, stage: &str, size: [u32; 2], pixels: &[u8]) {
    let path = group.join(stage);
    fs::create_dir(&path).expect("exclusive capture stage; never replace evidence");
    let output = resources::smoke_output(root, &path).unwrap();
    let unique: HashSet<_> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    println!(
        "JPEG_OBJECT_GENERIC_DIVERSITY stage={stage} colors={} threshold=20 classification={} (scoped readback, not generic certification)",
        unique.len(),
        if unique.len() < 20 { "FAIL" } else { "PASS" }
    );
    // Protected writer's actual allowlist/held-directory/O_EXCL policy unchanged.
    // As in the PNG object tracer, keep a semantically failing sparse frame;
    // never weaken Renderer::capture or call this a generic smoke PASS.
    let file = output.create_capture("office.png").unwrap();
    let mut encoder = png::Encoder::new(file, size[0], size[1]);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(pixels).unwrap();
    writer.finish().unwrap();
    println!(
        "JPEG_OBJECT_CAPTURE {} rgba_sha256={}",
        output.path().join("office.png").display(),
        digest(pixels)
    );
}
fn object_pixels(
    pixels: &[u8],
    size: [u32; 2],
    pose: &Pose,
    a: &hair::host_scene::Anchor,
) -> Result<()> {
    let [w, h] = size;
    if pixels.len() != (w * h * 4) as usize || pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
    {
        return Err("JPEG_OBJECT_FRAMEBUFFER_LENGTH_OR_OPACITY".into());
    }
    let m = matrix(pose, size);
    let inv = m.inverse();
    let mut roi = [w as f32, h as f32, 0.0_f32, 0.0_f32];
    for dx in [-0.5, 0.5] {
        for dy in [0.0, 1.0] {
            for dz in [-0.5, 0.5] {
                let p = project(
                    m,
                    Vec3::from_array(a.at)
                        + Vec3::new(dx * a.size[0], dy * a.size[1], dz * a.size[2]),
                    size,
                );
                roi = [
                    roi[0].min(p[0]),
                    roi[1].min(p[1]),
                    roi[2].max(p[0]),
                    roi[3].max(p[1]),
                ];
            }
        }
    }
    let mut counts = [0_usize; 4];
    let mut hits = Vec::new();
    // Include a small exterior halo: a proportional image stretched past this
    // envelope must not pass merely because we cropped it to the anchor first.
    for y in (roi[1] - 8.0).floor().max(0.0) as u32..=((roi[3] + 8.0).ceil() as u32).min(h - 1) {
        for x in (roi[0] - 8.0).floor().max(0.0) as u32..=((roi[2] + 8.0).ceil() as u32).min(w - 1)
        {
            let at = ((y * w + x) * 4) as usize;
            if let Some(c) = COLORS
                .iter()
                .position(|color| near(&pixels[at..at + 4], *color))
            {
                counts[c] += 1;
                hits.push([x, y]);
            }
        }
    }
    if counts.iter().any(|n| *n < 16) {
        return Err(format!(
            "JPEG_OBJECT_SOURCE_PIXELS_MISSING: before-E four genuine JPEG interiors required; anchor={} counts={counts:?} roi={roi:?}",
            a.id
        ));
    }
    // Undo perspective as in the original object oracle. Do NOT require a 3:2
    // screen-space rectangle when looking down from the real 1.65m eye height.
    // Accept a vertical image plane anywhere inside the unchanged depth volume.
    let eye = Vec3::new(pose.position[0], 1.65, pose.position[1]);
    for step in 0..=32 {
        let z = a.at[2] - a.size[2] * 0.5 + a.size[2] * step as f32 / 32.0;
        let mut b = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for [x, y] in &hits {
            let far = inv.project_point3(Vec3::new(
                2.0 * (*x as f32 + 0.5) / w as f32 - 1.0,
                1.0 - 2.0 * (*y as f32 + 0.5) / h as f32,
                1.0,
            ));
            let ray = far - eye;
            let p = eye + ray * ((z - eye.z) / ray.z);
            b = [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)];
        }
        let (iw, ih) = (b[2] - b[0], b[3] - b[1]);
        let epsilon = 0.018;
        if iw < a.size[0] * 0.5
            || ih <= 0.0
            || (iw / ih - 1.5).abs() > 0.15
            || b[0] < a.at[0] - a.size[0] * 0.5 - epsilon
            || b[2] > a.at[0] + a.size[0] * 0.5 + epsilon
            || b[1] < a.at[1] - epsilon
            || b[3] > a.at[1] + a.size[1] + epsilon
            || ((b[0] + b[2]) * 0.5 - a.at[0]).abs() > a.size[0] * 0.1
            || ((b[1] + b[3]) * 0.5 - a.at[1] - a.size[1] * 0.5).abs() > a.size[1] * 0.1
        {
            continue;
        }
        let mut good = true;
        // Includes x=.50/top and left/y=.46: a fabricated half-split quadrant
        // card cannot satisfy the real fixture's asymmetric interior oracle.
        for (c, fx, fy) in PATCHES {
            let p = project(m, Vec3::new(b[0] + fx * iw, b[3] - fy * ih, z), size);
            let (cx, cy) = (p[0].round() as i32, p[1].round() as i32);
            for y in cy - 1..=cy + 1 {
                for x in cx - 1..=cx + 1 {
                    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                        good = false;
                    } else {
                        let at = ((y as u32 * w + x as u32) * 4) as usize;
                        good &= near(&pixels[at..at + 4], COLORS[c]);
                    }
                }
            }
        }
        if good {
            println!(
                "JPEG_OBJECT_FIT anchor={} counts={counts:?} plane_z={z} bounds={b:?}",
                a.id
            );
            return Ok(());
        }
    }
    Err(format!(
        "JPEG_OBJECT_FIT_OR_ORIENTATION: anchor={} counts={counts:?}; no centered 96:64 fit inside host envelope; possible stretching/neighbor intrusion",
        a.id
    ))
}
fn observe(app: &mut App, size: [u32; 2], a: &hair::host_scene::Anchor) -> (Vec<u8>, Result<()>) {
    let start = Instant::now();
    let deadline = start + Duration::from_secs(5);
    for n in 1..=OBSERVATION_FRAMES {
        let pixels = frame(app).expect("actual App poll/Metal readback setup, not semantic RED");
        assert!(!app.world.reading && app.local.as_ref().unwrap().reader.is_none());
        assert!(!app.local_image_visible());
        let contract = object_pixels(&pixels, size, &app.pose, a);
        if contract.is_ok() || n == OBSERVATION_FRAMES || Instant::now() >= deadline {
            println!(
                "JPEG_OBJECT_OBSERVE anchor={} frames={n} elapsed_ms={} result={contract:?}",
                a.id,
                start.elapsed().as_millis()
            );
            return (pixels, contract);
        }
        std::thread::yield_now();
    }
    unreachable!()
}
fn drain_opened(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_local_image().unwrap();
        if !app.local_image_pending() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "opened-image setup timeout, not before-E RED"
        );
        std::thread::yield_now();
    }
}
fn opened_pixels(pixels: &[u8], [w, h]: [u32; 2]) -> Result<()> {
    if pixels.len() != (w * h * 4) as usize || pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
    {
        return Err("JPEG_OPENED_LENGTH_OR_OPACITY".into());
    }
    // Use the actual renderer's unchanged opened-body fit; this is a lifecycle
    // control, not a replacement or weakening of the original opened tracer.
    let scale = ((w - 32) as f32 / 96.0).min((h - 154) as f32 / 64.0);
    let (iw, ih) = (96.0 * scale, 64.0 * scale);
    let left = (w as f32 - iw) * 0.5;
    let top = 68.0 + ((h - 154) as f32 - ih) * 0.5;
    for (c, fx, fy) in PATCHES {
        let cx = (left + iw * fx) as u32;
        let cy = (top + ih * fy) as u32;
        for y in cy - 4..=cy + 4 {
            for x in cx - 4..=cx + 4 {
                let at = ((y * w + x) * 4) as usize;
                if !near(&pixels[at..at + 4], COLORS[c]) {
                    return Err(format!("JPEG_OPENED_SOURCE_CONTROL: region={c} xy={x},{y}"));
                }
            }
        }
    }
    Ok(())
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
fn require_original_raw(app: &mut App) {
    for _ in 0..3 {
        if let Some(bytes) = raw_bytes(app.world.document().unwrap()) {
            assert_eq!(
                bytes.as_slice(),
                JPEG,
                "all original held bytes, never display-path COM"
            );
            return;
        }
        tap(app, KeyCode::KeyV);
    }
    panic!("actual bounded V route failed to expose original JPEG raw bytes");
}
fn labels(app: &App) -> Vec<u8> {
    // Same above-envelope geometry witness as the original PNG object tracer.
    let vertices: Vec<_> = app.themes[app.theme_index]
        .vertices_for(&app.world)
        .into_iter()
        .filter(|v| v.position[1] > 0.90)
        .collect();
    bytemuck::cast_slice(&vertices).to_vec()
}
type VisitState = (usize, [f32; 2], f32, f32, Option<(OsString, [f32; 2])>);
fn visits(app: &App) -> Vec<VisitState> {
    app.local
        .as_ref()
        .unwrap()
        .poses
        .iter()
        .map(|v| {
            (
                v.page,
                v.pose.position,
                v.pose.yaw,
                v.pose.pitch,
                v.child.as_ref().map(|c| (c.name.clone(), c.doorway)),
            )
        })
        .collect()
}

#[test]
#[ignore = "serialized retained Metal JPEG object tracer; absent HAIR_JPEG_OBJECT_PROOF_ROOT under owned TMPDIR"]
fn retained_jpeg_before_e_has_source_pixels_inside_document_envelope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let temp = std::env::temp_dir().canonicalize().unwrap();
    assert!(temp.starts_with(root.parent().unwrap()) && separate(&temp, &root));
    let requested = PathBuf::from(
        std::env::var_os("HAIR_JPEG_OBJECT_PROOF_ROOT")
            .expect("explicit fresh absent retained proof root"),
    );
    assert!(requested.is_absolute());
    assert_eq!(requested.parent().unwrap().canonicalize().unwrap(), temp);
    let output = temp.join(requested.file_name().unwrap());
    let mut sibling = requested.file_name().unwrap().to_os_string();
    sibling.push(".fixture");
    let fixture = temp.join(sibling);
    assert!(separate(&output, &fixture) && separate(&output, &root) && separate(&fixture, &root));
    for path in [&output, &fixture] {
        assert!(
            fs::symlink_metadata(path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        );
    }
    fs::create_dir(&output).expect("fresh retained output only");
    fs::create_dir(&fixture).expect("fresh separate exclusive fixture root only");
    assert_eq!(JPEG.len(), 1000);
    assert_eq!(digest(JPEG), JPEG_SHA256);
    assert_eq!(
        fs::read(root.join("tests/fixtures/synthetic-jpeg-quadrants.jpg")).unwrap(),
        JPEG
    );
    // Compiled/live subset witness only; parent still owns full source manifests.
    for (name, bytes) in [
        (
            "local_jpeg_object_tests.rs",
            include_bytes!("local_jpeg_object_tests.rs").as_slice(),
        ),
        (
            "folder_native.rs",
            include_bytes!("folder_native.rs").as_slice(),
        ),
        (
            "folder_native/object_preview.rs",
            include_bytes!("folder_native/object_preview.rs").as_slice(),
        ),
        (
            "folder_native/image.rs",
            include_bytes!("folder_native/image.rs").as_slice(),
        ),
        (
            "folder_native/raw_reader.rs",
            include_bytes!("folder_native/raw_reader.rs").as_slice(),
        ),
        ("assets.rs", include_bytes!("assets.rs").as_slice()),
        (
            "assets/jpeg.rs",
            include_bytes!("assets/jpeg.rs").as_slice(),
        ),
        (
            "local_explorer.rs",
            include_bytes!("local_explorer.rs").as_slice(),
        ),
        (
            "folder_scene.rs",
            include_bytes!("folder_scene.rs").as_slice(),
        ),
        ("theme.rs", include_bytes!("theme.rs").as_slice()),
        ("main.rs", include_bytes!("main.rs").as_slice()),
        ("render.rs", include_bytes!("render.rs").as_slice()),
        ("text.rs", include_bytes!("text.rs").as_slice()),
        ("shader.wgsl", include_bytes!("shader.wgsl").as_slice()),
        ("resources.rs", include_bytes!("resources.rs").as_slice()),
    ] {
        assert_eq!(
            fs::read(root.join("src").join(name)).unwrap(),
            bytes,
            "rebuild changed {name}"
        );
        println!(
            "JPEG_OBJECT_COMPILED_SOURCE src/{name} sha256={}",
            digest(bytes)
        );
    }
    assert_eq!(hair::local_explorer::READER_CHUNK_BYTES, 4096);
    assert_eq!(hair::local_explorer::MAX_BODY_BYTES, 4 * 1024 * 1024);
    let names = [
        OsString::from("A JPEG Pixel Case\né.JPG"),
        OsString::from("Z JPEG Pixel Case\né.jpeg"),
    ];
    let decoy = comment_decoy();
    assert!(decoy.len() < hair::local_explorer::READER_CHUNK_BYTES);
    println!(
        "JPEG_OBJECT_INPUT output={} fixture={} names={names:?} dimensions=96x64 split=40,24 sha256={} decoy_sha256={} decoy=same_codestream_plus_COM_RAW_IDENTITY_ONLY",
        output.display(),
        fixture.display(),
        digest(JPEG),
        digest(&decoy)
    );
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let mut failures = Vec::new();
    for (label, size) in [("normal", [960, 640]), ("small", [640, 480])] {
        let selected = fixture.join(format!("{label}-selected"));
        let held = fixture.join(format!("{label}-held"));
        fs::create_dir(&selected).unwrap();
        for name in &names {
            exclusive(&selected.join(name), JPEG);
        }
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
        app.complete_folder(Ok(Some(selected.clone()))).unwrap();
        assert_eq!(app.world.folder_view.as_ref().unwrap().bindings.len(), 2);
        // Deliberate owned fixture mutation BEFORE first poll/E. Product retains
        // the real directory descriptor; the displayed path now names a decoy.
        fs::rename(&selected, &held).unwrap();
        fs::create_dir(&selected).unwrap();
        for name in &names {
            exclusive(&selected.join(name), &decoy);
        }
        let held_before = inventory(&held);
        let decoy_before = inventory(&selected);
        let host = app.themes[0].host.clone();
        let current = app.world.current.clone();
        let history = visits(&app);
        let page = app.local.as_ref().unwrap().page;
        let navigation: Vec<_> = app
            .world
            .navigation
            .rows
            .iter()
            .map(|r| (r.id.clone(), r.label.clone(), r.depth))
            .collect();
        let mut renderer = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            size,
            &app.themes[0].vertices_for(&app.world),
        ))
        .unwrap();
        renderer.set_materials(&app.themes[0].textures).unwrap();
        app.renderer = Some(renderer);
        let group = output.join(label);
        fs::create_dir(&group).unwrap();
        for (extension, name) in ["jpg", "jpeg"].into_iter().zip(&names) {
            let view = app.world.folder_view.as_ref().unwrap().clone();
            let (id, bound) = view
                .bindings
                .iter()
                .find(|(_, b)| &b.entry.name == name)
                .unwrap();
            assert_eq!(bound.entry.kind, Kind::File);
            assert_eq!(bound.entry.path, selected.join(name));
            let a = host.anchors.iter().find(|a| &a.id == id).unwrap().clone();
            assert_eq!(a.role, "document");
            // Existing source-pixel object test convention: aim ONLY the camera,
            // never reposition an anchor/desk/preview or alter a collision solid.
            let distance = 1.25_f32;
            app.pose = Pose {
                eye_offset: 1.65,
                feet_y: 0.0,
                position: [a.at[0], a.at[2] + a.size[2] * 0.5 + distance],
                yaw: 0.0,
                pitch: ((a.at[1] + a.size[1] * 0.5 - 1.65) / distance).atan(),
            };
            assert!(!host.blocked(app.pose.position));
            host.check_routes(&[app.pose.position]).unwrap();
            hair::interaction::refresh(&host, &app.pose, &mut app.world);
            assert_eq!(&app.world.interaction.as_ref().unwrap().anchor, id);
            let target = app.world.interaction.clone();
            let explicit = app.world.explicit_target.clone();
            let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
            let label_geometry = labels(&app);
            let entry_group = group.join(extension);
            fs::create_dir(&entry_group).unwrap();
            let (pixels, contract) = observe(&mut app, size, &a);
            retain(&root, &entry_group, "before-e", size, &pixels);
            if let Err(error) = contract {
                failures.push(format!("{label}/{extension}/before-e: {error}"));
            }
            assert_eq!(
                app.themes[0].host, host,
                "host anchors/collision/routes unchanged"
            );
            assert_eq!(app.world.folder_view.as_ref().unwrap(), &view);
            assert_eq!(app.world.interaction, target);
            assert_eq!(app.world.explicit_target, explicit);
            assert_eq!(labels(&app), label_geometry);
            // Do not short-circuit at intended missing-object RED: retain opened,
            // raw identity and Escape rehydration evidence for BOTH viewport sizes.
            tap(&mut app, KeyCode::KeyE);
            assert!(app.world.reading && app.local.as_ref().unwrap().reader.is_some());
            drain_opened(&mut app);
            let opened = frame(&mut app).unwrap();
            retain(&root, &entry_group, "opened", size, &opened);
            if let Err(error) = opened_pixels(&opened, size) {
                failures.push(format!("{label}/{extension}/opened: {error}"));
            }
            assert!(
                app.local_image_visible(),
                "accepted opened JPEG is a prerequisite control"
            );
            assert!(app.world.document().unwrap().starts_with("JPEG IMAGE VIEW"));
            tap(&mut app, KeyCode::KeyI);
            let metadata = app.world.document().unwrap().to_string();
            assert!(metadata.contains("METADATA VIEW"));
            assert!(metadata.contains(&folder_scene::display(name)));
            assert!(metadata.contains(&folder_scene::display(bound.entry.path.as_os_str())));
            assert!(!app.local_image_visible());
            tap(&mut app, KeyCode::KeyI);
            require_original_raw(&mut app);
            assert!(
                app.world
                    .document()
                    .unwrap()
                    .contains("Observed length: 1000 bytes | EOF (next inert)")
            );
            assert!(
                app.world
                    .document()
                    .unwrap()
                    .contains("LIMIT: 4096 bytes/chunk; no whole-file load")
            );
            let raw = frame(&mut app).unwrap();
            retain(&root, &entry_group, "raw-original", size, &raw);
            tap(&mut app, KeyCode::KeyC); // retry SAME held file, not display-path lookup
            drain_opened(&mut app);
            require_original_raw(&mut app);
            let mut without_reader = app.world.folder_view.as_ref().unwrap().clone();
            without_reader.reader = view.reader.clone();
            assert_eq!(without_reader, view);
            tap(&mut app, KeyCode::Escape);
            assert!(!app.world.reading && !app.local_image_visible());
            assert!(app.local.as_ref().unwrap().reader.is_none());
            let (rehydrated, contract) = observe(&mut app, size, &a);
            retain(&root, &entry_group, "escape-rehydrated", size, &rehydrated);
            if let Err(error) = contract {
                failures.push(format!("{label}/{extension}/escape: {error}"));
            }
            // Reader text may legitimately remain cached after close; restore no
            // test state. Normalize only a COPY when comparing non-reader fields.
            let mut closed = app.world.folder_view.as_ref().unwrap().clone();
            closed.reader = view.reader.clone();
            assert_eq!(
                closed, view,
                "bindings, raw names, source and routes unchanged"
            );
            assert_eq!(app.world.folder_view.as_ref().unwrap().bindings[id], *bound);
            assert_eq!(app.themes[0].host, host);
            assert!(host.file_anchors.contains(&a.id));
            assert!(!host.blocked([a.at[0], a.at[2]]) && !host.blocked(app.pose.position));
            host.check_routes(&[app.pose.position]).unwrap();
            assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
            assert_eq!(app.world.interaction, target);
            assert_eq!(app.world.explicit_target, explicit);
            assert_eq!(labels(&app), label_geometry);
            assert_eq!(app.world.current, current);
            assert_eq!(app.local.as_ref().unwrap().page, page);
            assert_eq!(visits(&app), history);
            assert_eq!(
                app.world
                    .navigation
                    .rows
                    .iter()
                    .map(|r| (r.id.clone(), r.label.clone(), r.depth))
                    .collect::<Vec<_>>(),
                navigation
            );
            assert!(app.focused && !app.input.forward && !app.input.captured);
            assert!(!app.chooser_active && !app.chooser_pending && app.address.is_none());
            assert!(app.web.is_none() && app.world.reader_link.is_none());
            assert_eq!(inventory(&held), held_before, "original source nonmutation");
            assert_eq!(inventory(&selected), decoy_before, "COM decoy nonmutation");
            println!(
                "JPEG_OBJECT_CONTROLS_COMPLETE {label}/{extension}: E, original raw/source, same-handle C, Escape, host/binding/selection/collision/pose/history/nonmutation"
            );
        }
        app.renderer.as_ref().unwrap().wait().unwrap();
        drop(app);
        assert_eq!(inventory(&held), held_before);
        assert_eq!(inventory(&selected), decoy_before);
    }
    drop(adapter);
    drop(instance);
    println!(
        "JPEG_OBJECT_FINAL_SEMANTIC_FAILURES {failures:?}; retained frames are actual GPU readback; no generic/resource/package acceptance"
    );
    exclusive(&output.join("SEMANTIC-RESULT.txt"), format!(
        "JPEG object semantic failures: {failures:#?}\nBoth viewport/extension loops completed. COM differs in raw bytes only, not pixels.\n"
    ).as_bytes());
    assert!(
        failures.is_empty(),
        "LOCAL_JPEG_BEFORE_E_OBJECT_FRAMEBUFFER: {failures:?}; opened JPEG pixels/raw bytes are not before-E world image support"
    );
}
