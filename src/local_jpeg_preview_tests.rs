//! One real JPEG held-source -> App E/input/poll -> shared-renderer tracer.
//! File-only preparation: no fabricated decode result, texture or framebuffer.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};

const JPEG: &[u8] = include_bytes!("../tests/fixtures/synthetic-jpeg-quadrants.jpg");
const JPEG_SHA256: &str = "60ed51d80655f627b39efee03a0b5b3e67d1a87d2324a616769c17ed90979df1";
const COLORS: [[u8; 3]; 4] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]];

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
fn comment_impostor() -> Vec<u8> {
    // Valid JPEG COM immediately after SOI; compressed image bytes UNCHANGED.
    // This distinguishes authority through exact raw bytes, NOT different pixels.
    assert!(JPEG.starts_with(&[0xff, 0xd8]));
    let comment = b"TEST-OWNED DISPLAY-PATH IMPOSTOR; SAME CODESTREAM";
    let length = u16::try_from(comment.len() + 2).unwrap();
    let mut bytes = Vec::with_capacity(JPEG.len() + comment.len() + 4);
    bytes.extend_from_slice(&JPEG[..2]);
    bytes.extend_from_slice(&[0xff, 0xfe]);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(comment);
    bytes.extend_from_slice(&JPEG[2..]);
    assert_ne!(bytes.as_slice(), JPEG);
    assert!(bytes.len() < hair::local_explorer::READER_CHUNK_BYTES);
    bytes
}
type FileState = (OsString, u64, u64, u32, u64, i64, i64, Vec<u8>);
fn inventory(dir: &Path) -> (u32, Vec<FileState>) {
    let directory = fs::symlink_metadata(dir).unwrap();
    assert!(directory.is_dir() && !directory.file_type().is_symlink());
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let meta = fs::symlink_metadata(entry.path()).unwrap();
        assert!(meta.is_file() && !meta.file_type().is_symlink());
        files.push((
            entry.file_name(),
            meta.dev(),
            meta.ino(),
            meta.mode(),
            meta.len(),
            meta.mtime(),
            meta.mtime_nsec(),
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
fn aim(app: &mut App, id: &str) {
    let host = &app.themes[app.theme_index].host;
    let anchor = host.anchors.iter().find(|a| a.id == id).unwrap();
    let inward = host.portal_inward(id);
    app.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [
            anchor.at[0] + 1.05 * inward[0],
            anchor.at[2] + 1.05 * inward[1],
        ],
        yaw: (-inward[0]).atan2(inward[1]),
        pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    assert!(!host.blocked(app.pose.position));
    app.world.explicit_target = None;
    hair::interaction::refresh(host, &app.pose, &mut app.world);
    assert_eq!(app.world.interaction.as_ref().unwrap().anchor, id);
}
fn drain(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_local_image()
            .expect("real production image consumer");
        if !app.local_image_pending() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "image completion setup deadline, not JPEG pixel RED"
        );
        std::thread::yield_now();
    }
}
fn capture(app: &mut App, root: &Path, group: &Path, stage: &str) -> Vec<u8> {
    drain(app);
    // Do NOT require image_visible: current JPEG is raw fallback, which must be
    // captured honestly and reach the aggregate missing-source-pixels assertion.
    assert!(app.world.reading);
    assert_eq!(
        app.scroll, 0,
        "capture uses the real renderer's scroll=0 seam"
    );
    app.update_hud();
    let vertices = app.themes[app.theme_index].vertices_for(&app.world);
    let renderer = app.renderer.as_mut().unwrap();
    renderer.set_scene(&vertices);
    let path = group.join(stage);
    fs::create_dir(&path).expect("fresh exclusive capture stage");
    let output = resources::smoke_output(root, &path).unwrap();
    renderer
        .capture(&output, "office.png", &app.pose, &app.world)
        .expect("actual shared GPU capture; setup failure is not JPEG RED");
    let encoded = fs::read(output.path().join("office.png")).unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(&encoded))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!([info.width, info.height], renderer.size);
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    pixels.truncate(info.buffer_size());
    println!(
        "JPEG_CAPTURE {} encoded_sha256={} rgba_sha256={} size={:?}",
        output.path().join("office.png").display(),
        digest(&encoded),
        digest(&pixels),
        renderer.size
    );
    pixels
}
fn near(pixel: &[u8], color: [u8; 3]) -> bool {
    // Coarse saturated regions, not equality between lossy JPEG and source RGB.
    pixel[..3]
        .iter()
        .zip(color)
        .all(|(a, b)| a.abs_diff(b) <= 48)
}
fn opened_pixels(pixels: &[u8], [w, h]: [u32; 2]) -> Result<()> {
    if pixels.len() != (w * h * 4) as usize || pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
    {
        return Err("JPEG_FRAME_LENGTH_OR_OPACITY".into());
    }
    let mut counts = [0_usize; 4];
    let mut bounds = [w, h, 0, 0];
    let mut leaked = false;
    for (i, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
        if let Some(color) = COLORS.iter().position(|c| near(pixel, *c)) {
            counts[color] += 1;
            let (x, y) = (i as u32 % w, i as u32 / w);
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x),
                bounds[3].max(y),
            ];
            leaked |= x < 16 || x >= w - 16 || y < 68 || y >= h - 86;
        }
    }
    println!("JPEG_PIXEL_MEASUREMENT size={w}x{h} counts={counts:?} bounds={bounds:?}");
    if counts.iter().any(|n| *n < 1024) {
        return Err(format!(
            "JPEG_SOURCE_PIXELS_MISSING: four large coherent source regions required; counts={counts:?}; raw JPEG bytes are not rendered image content"
        ));
    }
    if leaked {
        return Err(format!(
            "JPEG_BODY_CLIP: source-color interior outside body; bounds={bounds:?}"
        ));
    }
    let [left, top, right, bottom] = bounds;
    let (iw, ih) = ((right - left + 1) as f32, (bottom - top + 1) as f32);
    let scale = ((w - 32) as f32 / 96.0).min((h - 154) as f32 / 64.0);
    let (expected_w, expected_h) = (96.0 * scale, 64.0 * scale);
    let expected_left = (w as f32 - expected_w) * 0.5;
    let expected_top = 68.0 + ((h - 154) as f32 - expected_h) * 0.5;
    if iw < 240.0
        || ih < 160.0
        || (iw / ih - 1.5).abs() > 0.06
        || (iw - expected_w).abs() > 4.0
        || (ih - expected_h).abs() > 4.0
        || (left as f32 - expected_left).abs() > 4.0
        || (top as f32 - expected_top).abs() > 4.0
    {
        return Err(format!(
            "JPEG_LARGE_3_TO_2_FIT: bounds={bounds:?} expected={expected_left},{expected_top},{expected_w},{expected_h}"
        ));
    }
    // Real source split x=40/96, y=24/64, NOT centered halves. Multiple 9x9
    // patches inside every region; x=.50/top and left/y=.46 reject half-split art.
    for (corner, fx, fy) in [
        (0, 0.15_f32, 0.15_f32),
        (0, 0.30, 0.25),
        (1, 0.50, 0.18),
        (1, 0.82, 0.26),
        (2, 0.18, 0.46),
        (2, 0.30, 0.82),
        (3, 0.55, 0.55),
        (3, 0.82, 0.82),
    ] {
        let cx = left + (iw * fx) as u32;
        let cy = top + (ih * fy) as u32;
        for y in cy - 4..=cy + 4 {
            for x in cx - 4..=cx + 4 {
                let at = ((y * w + x) * 4) as usize;
                if !near(&pixels[at..at + 4], COLORS[corner]) {
                    return Err(format!(
                        "JPEG_COHERENT_SOURCE_ORIENTATION: region={corner} xy={x},{y} actual={:?}",
                        &pixels[at..at + 4]
                    ));
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
    // Ground V in the ACTUAL document: unsupported JPEG currently has TWO modes.
    // Future supported images may have three. Do not prescribe a JPEG cycle yet.
    for _ in 0..3 {
        if let Some(actual) = raw_bytes(app.world.document().unwrap()) {
            assert_eq!(
                actual.as_slice(),
                JPEG,
                "ALL original held JPEG bytes, never COM impostor"
            );
            return;
        }
        tap(app, KeyCode::KeyV);
    }
    panic!("bounded actual V route did not expose authoritative JPEG raw bytes");
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
#[ignore = "serialized retained Metal JPEG tracer; absent HAIR_JPEG_PROOF_ROOT under owned TMPDIR"]
fn retained_local_jpeg_e_opened_framebuffer_has_source_pixels_and_fit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let temp = std::env::temp_dir().canonicalize().unwrap();
    assert!(temp.starts_with(root.parent().unwrap()) && separate(&temp, &root));
    let requested = PathBuf::from(
        std::env::var_os("HAIR_JPEG_PROOF_ROOT")
            .expect("explicit fresh absent retained proof root"),
    );
    assert!(requested.is_absolute());
    assert_eq!(requested.parent().unwrap().canonicalize().unwrap(), temp);
    let output = temp.join(requested.file_name().unwrap());
    let mut sibling = requested.file_name().unwrap().to_os_string();
    sibling.push(".fixture");
    let fixture = temp.join(sibling);
    assert!(separate(&output, &fixture) && separate(&output, &root) && separate(&fixture, &root));
    assert!(fs::symlink_metadata(&output).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound));
    assert!(
        fs::symlink_metadata(&fixture).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
    );
    fs::create_dir(&output).expect("new proof root only; never overwrite evidence");
    fs::create_dir(&fixture).expect("new owned fixture sibling only");
    println!(
        "JPEG_RETAINED output={} fixture={}",
        output.display(),
        fixture.display()
    );
    // Compiled-vs-live subset only, not an entire source freeze/artifact identity.
    for (name, bytes) in [
        (
            "local_jpeg_preview_tests.rs",
            include_bytes!("local_jpeg_preview_tests.rs").as_slice(),
        ),
        ("main.rs", include_bytes!("main.rs").as_slice()),
        (
            "folder_native.rs",
            include_bytes!("folder_native.rs").as_slice(),
        ),
        (
            "folder_native/raw_reader.rs",
            include_bytes!("folder_native/raw_reader.rs").as_slice(),
        ),
        (
            "folder_native/image.rs",
            include_bytes!("folder_native/image.rs").as_slice(),
        ),
        (
            "local_explorer.rs",
            include_bytes!("local_explorer.rs").as_slice(),
        ),
        ("assets.rs", include_bytes!("assets.rs").as_slice()),
        ("render.rs", include_bytes!("render.rs").as_slice()),
        ("text.rs", include_bytes!("text.rs").as_slice()),
        ("shader.wgsl", include_bytes!("shader.wgsl").as_slice()),
    ] {
        assert_eq!(
            fs::read(root.join("src").join(name)).unwrap(),
            bytes,
            "rebuild changed {name}"
        );
        println!("JPEG_COMPILED_SOURCE src/{name} sha256={}", digest(bytes));
    }
    assert_eq!(
        fs::read(root.join("tests/fixtures/synthetic-jpeg-quadrants.jpg")).unwrap(),
        JPEG
    );
    assert_eq!(JPEG.len(), 1000);
    assert_eq!(digest(JPEG), JPEG_SHA256);
    assert_eq!(hair::local_explorer::READER_CHUNK_BYTES, 4096);
    assert_eq!(hair::local_explorer::MAX_BODY_BYTES, 4 * 1024 * 1024);
    let name = OsString::from("JPEG Pixel Case\né.JpG");
    let decoy = comment_impostor();
    println!(
        "JPEG_INPUT name={name:?} dimensions=96x64 split=40,24 sha256={} impostor_sha256={} impostor=same_codestream_plus_COM",
        digest(JPEG),
        digest(&decoy)
    );
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let mut missing = Vec::new();
    for (label, size) in [("normal", [960, 640]), ("small", [640, 480])] {
        let selected = fixture.join(format!("{label}-selected"));
        let held = fixture.join(format!("{label}-held"));
        fs::create_dir(&selected).unwrap();
        exclusive(&selected.join(&name), JPEG);
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
        let view = app.world.folder_view.as_ref().unwrap();
        assert_eq!(view.bindings.len(), 1);
        let (id, bound) = view
            .bindings
            .iter()
            .find(|(_, b)| b.entry.name == name)
            .unwrap();
        let (id, bound) = (id.clone(), bound.clone());
        assert_eq!(bound.entry.kind, Kind::File);
        assert_eq!(bound.entry.path, selected.join(&name));
        // Deliberate owned-input setup, BEFORE E: real directory handle survives.
        fs::rename(&selected, &held).unwrap();
        fs::create_dir(&selected).unwrap();
        exclusive(&selected.join(&name), &decoy);
        let held_before = inventory(&held);
        let decoy_before = inventory(&selected);
        let host = app.themes[0].host.clone();
        let folder = app.world.folder_view.as_ref().unwrap().clone();
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
        let group = output.join(label);
        fs::create_dir(&group).unwrap();
        let mut renderer = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            size,
            &app.themes[0].vertices_for(&app.world),
        ))
        .unwrap();
        renderer.set_materials(&app.themes[0].textures).unwrap();
        app.renderer = Some(renderer);
        aim(&mut app, &id);
        let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
        tap(&mut app, KeyCode::KeyE);
        assert!(app.world.reading && app.local.as_ref().unwrap().reader.is_some());
        let pixels = capture(&mut app, &root, &group, "opened");
        if let Err(error) = opened_pixels(&pixels, size) {
            missing.push(format!("{label}: {error}"));
        }
        // Do not demand a PNG label. If JPEG becomes visible, an old PNG claim is
        // dishonest; aggregate it without concealing the primary pixel outcome.
        let document = app.world.document().unwrap();
        if app.local_image_visible() && document.lines().any(|line| line.contains("PNG")) {
            missing.push(format!(
                "{label}: JPEG_LABEL_MUST_NOT_CLAIM_PNG: visible JPEG has PNG-labelled document"
            ));
        }
        for code in [
            KeyCode::KeyW,
            KeyCode::KeyE,
            KeyCode::KeyU,
            KeyCode::KeyR,
            KeyCode::F5,
            KeyCode::KeyO,
            KeyCode::PageDown,
            KeyCode::PageUp,
            KeyCode::KeyL,
            KeyCode::Tab,
        ] {
            tap(&mut app, code);
        }
        tap(&mut app, KeyCode::KeyI);
        let metadata = app.world.document().unwrap().to_string();
        assert!(metadata.contains("METADATA VIEW"));
        assert!(metadata.contains(&folder_scene::display(&name)));
        assert!(metadata.contains(&folder_scene::display(bound.entry.path.as_os_str())));
        assert!(raw_bytes(&metadata).is_none() && !app.local_image_visible());
        capture(&mut app, &root, &group, "metadata");
        tap(&mut app, KeyCode::KeyI);
        require_original_raw(&mut app);
        let raw = app.world.document().unwrap().to_string();
        assert!(raw.contains("LIMIT: 4096 bytes/chunk; no whole-file load"));
        assert!(raw.contains("Observed length: 1000 bytes | EOF (next inert)"));
        assert!(raw.contains(&folder_scene::display(bound.entry.path.as_os_str())));
        for code in [
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::Equal,
            KeyCode::Minus,
            KeyCode::Digit0,
        ] {
            tap(&mut app, code);
        }
        assert_eq!(
            app.world.document().unwrap(),
            raw,
            "raw zero/EOF paging and image-only keys inert"
        );
        capture(&mut app, &root, &group, "raw");
        tap(&mut app, KeyCode::KeyV);
        assert!(
            app.world
                .document()
                .unwrap()
                .contains("\nTEXT (links inactive; escapes shown; range only):\n")
        );
        assert!(!app.local_image_visible());
        require_original_raw(&mut app);
        tap(&mut app, KeyCode::KeyC); // existing retry SAME opened handle, never reset/reopen path
        drain(&mut app);
        require_original_raw(&mut app);
        capture(&mut app, &root, &group, "same-handle-retry-raw");
        assert!(app.world.reading && app.focused && !app.input.forward && !app.input.captured);
        assert!(!app.chooser_active && !app.chooser_pending && app.address.is_none());
        assert!(app.web.is_none() && app.world.reader_link.is_none());
        assert_eq!(app.themes[0].host, host);
        assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
        let mut without_reader = app.world.folder_view.as_ref().unwrap().clone();
        without_reader.reader = folder.reader.clone();
        assert_eq!(without_reader, folder);
        assert_eq!(app.world.folder_view.as_ref().unwrap().bindings[&id], bound);
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
        tap(&mut app, KeyCode::Escape);
        assert!(!app.world.reading && !app.local_image_visible());
        assert!(app.local.as_ref().unwrap().reader.is_none());
        assert!(app.focused && !app.input.forward && !app.input.captured);
        assert!(!app.chooser_active && !app.chooser_pending && app.address.is_none());
        assert_eq!(app.themes[0].host, host);
        assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
        assert_eq!(app.world.current, current);
        assert_eq!(app.local.as_ref().unwrap().page, page);
        assert_eq!(visits(&app), history);
        app.renderer.as_ref().unwrap().wait().unwrap();
        drop(app);
        assert_eq!(inventory(&held), held_before, "original source nonmutation");
        assert_eq!(
            inventory(&selected),
            decoy_before,
            "impostor source nonmutation"
        );
        println!(
            "JPEG_CONTROLS_COMPLETE {label}: original raw/source, same-handle C, host/binding/pose/history, close and nonmutation"
        );
    }
    drop(adapter);
    drop(instance);
    println!(
        "JPEG_TRACER_VERDICT missing={missing:?}; actual GPU only; generic sparse-world FAIL remains separate"
    );
    assert!(missing.is_empty(), "LOCAL_JPEG_E_FRAMEBUFFER: {missing:?}");
}
