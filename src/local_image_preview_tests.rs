//! First image vertical: synthetic native App input -> existing production renderer.
//! No image renderer, decoder result, or framebuffer is supplied by this harness.
//! Production polling for opened images exists; pre-E preview acceptance is still required.
use super::*;
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, io::Write, os::unix::ffi::OsStringExt, os::unix::fs::MetadataExt};

const COLORS: [[u8; 3]; 4] = [[241, 17, 53], [23, 209, 71], [31, 67, 239], [233, 181, 29]];

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn exclusive_file(path: &Path, bytes: &[u8]) {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
}
fn fixture_png(rotated: bool) -> Vec<u8> {
    // RGB, static, no alpha/profile conversion claim. Four different corners;
    // 48x24 rather than square, so rotation, reflection and stretch are observable.
    let mut rgb = Vec::new();
    for y in 0..24 {
        for x in 0..48 {
            let quadrant = usize::from(x >= 24) + 2 * usize::from(y >= 12);
            rgb.extend_from_slice(&COLORS[if rotated { 3 - quadrant } else { quadrant }]);
        }
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 48, 24);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&rgb).unwrap();
        writer.finish().unwrap();
    }
    assert!(bytes.len() < hair::local_explorer::READER_CHUNK_BYTES);
    bytes
}

type FileState = (OsString, u64, u64, u32, u64, i64, i64, Vec<u8>);
fn inventory(folder: &Path) -> (u32, Vec<FileState>) {
    let directory = fs::symlink_metadata(folder).unwrap();
    assert!(directory.is_dir() && !directory.file_type().is_symlink());
    let mut files = Vec::new();
    for entry in fs::read_dir(folder).unwrap() {
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
fn capture(app: &mut App, output: &resources::SmokeOutput, name: &str) -> Result<Vec<u8>> {
    // These ARE App's existing consumers: update_hud, Theme::vertices_for,
    // set_scene and Renderer::draw (called by capture, with scroll=0).
    // Never inject fixture pixels into materials/geometry/UI or replace World.
    assert_eq!(app.scroll, 0);
    // Async acquisition/decode is now production behavior. Drive ONLY the same
    // consumer used by App::redraw; bounded wait, no state/result injection.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_local_image()?;
        if !app.local_image_pending() {
            break;
        }
        if Instant::now() >= deadline {
            return Err("production PNG completion deadline exceeded".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    app.update_hud();
    let vertices = app.themes[app.theme_index].vertices_for(&app.world);
    let renderer = app.renderer.as_mut().unwrap();
    renderer.set_scene(&vertices);
    renderer.capture(output, name, &app.pose, &app.world)?;
    let path = output.path().join(name);
    let encoded = fs::read(&path).map_err(|e| e.to_string())?;
    let mut reader = png::Decoder::new(std::io::Cursor::new(&encoded))
        .read_info()
        .map_err(|e| e.to_string())?;
    let mut pixels = vec![0; reader.output_buffer_size().ok_or("capture size missing")?];
    let info = reader.next_frame(&mut pixels).map_err(|e| e.to_string())?;
    if [info.width, info.height] != renderer.size
        || info.color_type != png::ColorType::Rgba
        || info.bit_depth != png::BitDepth::Eight
    {
        return Err("capture must be actual viewport RGBA8".into());
    }
    pixels.truncate(info.buffer_size());
    println!(
        "IMAGE_CAPTURE {} encoded_sha256={} rgba_sha256={} size={:?}",
        path.display(),
        digest(&encoded),
        digest(&pixels),
        renderer.size
    );
    Ok(pixels)
}
fn near(pixel: &[u8], color: [u8; 3]) -> bool {
    pixel[..3]
        .iter()
        .zip(color)
        .all(|(a, b)| a.abs_diff(b) <= 2)
}
fn opened_image_pixels(pixels: &[u8], [w, h]: [u32; 2]) -> Result<()> {
    if pixels.len() != (w * h * 4) as usize || pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
    {
        return Err("bad framebuffer length/opacity".into());
    }
    // Locate only the four input-specific colors, not a generic green document,
    // filename, palette-diversity threshold, or source-derived fake screenshot.
    let mut bounds = [w, h, 0, 0];
    let mut counts = [0_usize; 4];
    for (index, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
        if let Some(corner) = COLORS.iter().position(|c| near(pixel, *c)) {
            counts[corner] += 1;
            let (x, y) = (index as u32 % w, index as u32 / w);
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x),
                bounds[3].max(y),
            ];
        }
    }
    if counts.iter().any(|n| *n < 256) {
        return Err(format!(
            "PNG_IMAGE_PIXELS_MISSING: expected four actual source corner colors; counts={counts:?}; PNG byte text is not an image"
        ));
    }
    let [left, top, right, bottom] = bounds;
    let (width, height) = (right - left + 1, bottom - top + 1);
    // A larger fitted viewer, not a 48x24 icon. Interior palette bounds can lose
    // a few pixels to linear edge filtering; tolerate 4%, not square stretching.
    if width < 240 || height < 120 || (width as f32 / height as f32 - 2.0).abs() > 0.08 {
        return Err(format!(
            "PNG_VIEWER_FIT: expected large 2:1 image, bounds={bounds:?}"
        ));
    }
    if left < 12 || top < 64 || right >= w - 12 || bottom >= h - 82 {
        return Err(format!(
            "PNG_VIEWER_BOUNDS: image enters viewport/chrome/footer, bounds={bounds:?}"
        ));
    }
    // 5x5 patches well inside each quadrant: orientation AND spatial coherence,
    // including the renamed-held-directory decoy's reversed-corner negative.
    for (corner, (fx, fy)) in [(0.2_f32, 0.25_f32), (0.8, 0.25), (0.2, 0.75), (0.8, 0.75)]
        .into_iter()
        .enumerate()
    {
        let cx = left + (width as f32 * fx) as u32;
        let cy = top + (height as f32 * fy) as u32;
        for y in cy - 2..=cy + 2 {
            for x in cx - 2..=cx + 2 {
                let at = ((y * w + x) * 4) as usize;
                if !near(&pixels[at..at + 4], COLORS[corner]) {
                    return Err(format!(
                        "PNG_SOURCE_ORIENTATION: corner={corner} xy={x},{y} expected={:?} actual={:?}",
                        COLORS[corner],
                        &pixels[at..at + 4]
                    ));
                }
            }
        }
    }
    println!("IMAGE_PIXEL_CONTRACT bounds={bounds:?} source_aspect=2 corner_counts={counts:?}");
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

#[test]
#[ignore = "serialized retained Metal image tracer; explicit absent HAIR_IMAGE_PROOF_ROOT under owned TMPDIR"]
fn retained_local_png_e_opened_framebuffer_has_source_pixels_and_fit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let owned = root.parent().unwrap();
    let temp = std::env::temp_dir().canonicalize().unwrap();
    assert!(
        temp.starts_with(owned) && separate(&temp, &root),
        "owned separate TMPDIR required"
    );
    let requested = PathBuf::from(
        std::env::var_os("HAIR_IMAGE_PROOF_ROOT").expect("explicit absent retained root"),
    );
    assert!(requested.is_absolute());
    assert_eq!(requested.parent().unwrap().canonicalize().unwrap(), temp);
    let output = temp.join(requested.file_name().unwrap());
    let mut sibling = requested.file_name().unwrap().to_os_string();
    sibling.push(".fixture");
    let fixture = temp.join(sibling);
    assert!(separate(&output, &fixture) && separate(&output, &root) && separate(&fixture, &root));
    fs::create_dir(&output).expect("absent output only; never reuse evidence");
    fs::create_dir(&fixture).expect("absent sibling input fixture only");
    println!(
        "IMAGE_RETAINED output={} fixture={}",
        output.display(),
        fixture.display()
    );
    for (name, bytes) in [
        ("main.rs", include_bytes!("main.rs").as_slice()),
        (
            "local_image_preview_tests.rs",
            include_bytes!("local_image_preview_tests.rs").as_slice(),
        ),
        (
            "folder_native.rs",
            include_bytes!("folder_native.rs").as_slice(),
        ),
        ("render.rs", include_bytes!("render.rs").as_slice()),
        ("shader.wgsl", include_bytes!("shader.wgsl").as_slice()),
    ] {
        assert_eq!(
            fs::read(root.join("src").join(name)).unwrap(),
            bytes,
            "rebuild changed source {name}"
        );
        println!("IMAGE_COMPILED_SOURCE src/{name} sha256={}", digest(bytes));
    }
    // APFS rejects invalid UTF-8 with EILSEQ. Keep literal newline/non-ASCII and
    // raw OsString association here; other Unix filesystems also exercise FF.
    let name = OsString::from_vec(if cfg!(target_os = "macos") {
        b"Pixel Case\n\xc3\xa9.PNG".to_vec()
    } else {
        b"Pixel Case\n\xff.PNG".to_vec()
    });
    let original = fixture_png(false);
    let decoy = fixture_png(true);
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let mut missing = Vec::new();
    for (label, size) in [("normal", [960, 640]), ("small", [640, 480])] {
        let selected = fixture.join(format!("{label}-selected"));
        let held = fixture.join(format!("{label}-held"));
        fs::create_dir(&selected).unwrap();
        exclusive_file(&selected.join(&name), &original);
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
        assert_eq!(bound.entry.kind, hair::local_explorer::Kind::File);
        assert_eq!(bound.entry.path, selected.join(&name));
        let host = app.themes[0].host.clone();
        // Deliberate TEST-OWNED mutation before E; the product must use the held
        // directory + raw component, not the now-valid display-path impostor.
        fs::rename(&selected, &held).unwrap();
        fs::create_dir(&selected).unwrap();
        exclusive_file(&selected.join(&name), &decoy);
        let held_before = inventory(&held);
        let decoy_before = inventory(&selected);
        let group = output.join(label);
        fs::create_dir(&group).unwrap();
        assert!(separate(&group, &fixture));
        let captures = resources::smoke_output(&root, &group).unwrap();
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
        assert!(app.world.reading);
        let pixels = capture(&mut app, &captures, "office.png")
            .expect("actual E-opened GPU capture; setup errors are not behavioral RED");
        if let Err(error) = opened_image_pixels(&pixels, size) {
            missing.push(format!("{label}: {error}"));
        }
        // Existing modal controls must remain isolated, including after adding
        // a media viewer. No hypothetical zoom/pan/scheduler API is called.
        for code in [
            KeyCode::KeyW,
            KeyCode::KeyE,
            KeyCode::KeyU,
            KeyCode::KeyR,
            KeyCode::F5,
            KeyCode::KeyO,
            KeyCode::PageDown,
            KeyCode::KeyL,
            KeyCode::Tab,
        ] {
            tap(&mut app, code);
        }
        assert!(app.world.reading && !app.input.forward && !app.input.captured);
        assert!(!app.chooser_pending && app.address.is_none());
        assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
        assert_eq!(
            app.themes[0].host, host,
            "viewer cannot move host anchors/transforms/collision"
        );
        assert_eq!(app.world.folder_view.as_ref().unwrap().bindings[&id], bound);
        tap(&mut app, KeyCode::KeyI);
        let metadata = app.world.document().unwrap();
        assert!(metadata.contains(&hair::folder_scene::display(&name)));
        assert!(metadata.contains(&hair::folder_scene::display(bound.entry.path.as_os_str())));
        capture(&mut app, &captures, "reader-welcome.png").unwrap();
        tap(&mut app, KeyCode::KeyI);
        // Permit a viewer->raw transition on existing V while preserving the
        // current two-state text/raw map; finite search, not a speculative API.
        let mut actual = raw_bytes(app.world.document().unwrap());
        for _ in 0..3 {
            if actual.is_some() {
                break;
            }
            tap(&mut app, KeyCode::KeyV);
            actual = raw_bytes(app.world.document().unwrap());
        }
        assert_eq!(
            actual.as_deref(),
            Some(original.as_slice()),
            "actual App raw source must be ALL original encoded bytes, not decoy path bytes"
        );
        let raw = app.world.document().unwrap().to_string();
        assert!(raw.contains("LIMIT: 4096 bytes/chunk; no whole-file load"));
        tap(&mut app, KeyCode::ArrowRight);
        assert_eq!(
            app.world.document().unwrap(),
            raw,
            "small PNG EOF stays inert"
        );
        capture(&mut app, &captures, "reader-workshop.png").unwrap();
        tap(&mut app, KeyCode::Escape);
        assert!(!app.world.reading);
        assert_eq!(app.themes[0].host, host);
        assert_eq!(inventory(&held), held_before);
        assert_eq!(inventory(&selected), decoy_before);
        app.renderer.as_ref().unwrap().wait().unwrap();
        drop(app); // reader/source, theme textures and renderer released before verdict
        drop(captures);
    }
    drop(adapter);
    drop(instance);
    println!(
        "IMAGE_TRACER_VERDICT missing={missing:?}; retained inputs/captures are synthetic-input + actual GPU only; no native-human/comfort claim"
    );
    assert!(
        missing.is_empty(),
        "LOCAL_PNG_E_FRAMEBUFFER: {missing:?}; raw PNG text is not decoded image content"
    );
}
