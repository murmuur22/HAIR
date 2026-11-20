//! One owned-file -> App keyboard/poll -> shared-renderer framebuffer tracer.
//! No decoded result, viewer transform, material, geometry or world pixels injected.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};

const COLORS: [[u8; 3]; 6] = [
    [241, 17, 53],
    [23, 209, 71],
    [31, 67, 239],
    [233, 181, 29],
    [173, 43, 211],
    [41, 193, 223],
];
const SOURCE: [u32; 2] = [96, 48];

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn exclusive(path: &Path, bytes: &[u8]) {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .unwrap();
    f.write_all(bytes).unwrap();
    f.flush().unwrap();
}
fn fixture_png(decoy: bool) -> Vec<u8> {
    // Central 8x4 landmark measures magnification without the viewport crop
    // masquerading as scale. An off-center 6x6 landmark removes quadrant symmetry.
    let mut rgb = Vec::new();
    for y in 0..SOURCE[1] {
        for x in 0..SOURCE[0] {
            let c = if (44..52).contains(&x) && (22..26).contains(&y) {
                4
            } else if (26..32).contains(&x) && (12..18).contains(&y) {
                5
            } else {
                usize::from(x >= 48) + 2 * usize::from(y >= 24)
            };
            rgb.extend_from_slice(&COLORS[if decoy { (c + 1) % COLORS.len() } else { c }]);
        }
    }
    let mut bytes = Vec::new();
    {
        let mut e = png::Encoder::new(&mut bytes, SOURCE[0], SOURCE[1]);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        w.write_image_data(&rgb).unwrap();
        w.finish().unwrap();
    }
    assert!(bytes.len() < hair::local_explorer::READER_CHUNK_BYTES);
    bytes
}
type FileState = (OsString, u64, u64, u32, u64, i64, i64, Vec<u8>);
fn inventory(dir: &Path) -> (u32, Vec<FileState>) {
    let d = fs::symlink_metadata(dir).unwrap();
    assert!(d.is_dir() && !d.file_type().is_symlink());
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let e = entry.unwrap();
        let m = fs::symlink_metadata(e.path()).unwrap();
        assert!(m.is_file() && !m.file_type().is_symlink());
        files.push((
            e.file_name(),
            m.dev(),
            m.ino(),
            m.mode(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            fs::read(e.path()).unwrap(),
        ));
    }
    files.sort();
    (d.mode(), files)
}
fn separate(a: &Path, b: &Path) -> bool {
    !a.starts_with(b) && !b.starts_with(a)
}
fn key(app: &mut App, code: KeyCode, down: bool, repeat: bool) {
    assert!(!app.keyboard_input(Some(code), down, repeat, None).unwrap());
}
fn tap(app: &mut App, code: KeyCode) {
    key(app, code, true, false);
    key(app, code, false, false);
}
fn aim(app: &mut App, id: &str) {
    let host = &app.themes[app.theme_index].host;
    let a = host.anchors.iter().find(|a| a.id == id).unwrap();
    let inward = host.portal_inward(id);
    app.pose = Pose {
        position: [a.at[0] + 1.05 * inward[0], a.at[2] + 1.05 * inward[1]],
        yaw: (-inward[0]).atan2(inward[1]),
        pitch: ((a.at[1] + a.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    assert!(!host.blocked(app.pose.position));
    app.world.explicit_target = None;
    hair::interaction::refresh(host, &app.pose, &mut app.world);
    assert_eq!(app.world.interaction.as_ref().unwrap().anchor, id);
}
fn capture(app: &mut App, root: &Path, group: &Path, stage: &str) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_local_image()
            .expect("production PNG poll; setup failure is not zoom RED");
        if !app.local_image_pending() {
            break;
        }
        assert!(Instant::now() < deadline, "PNG completion setup deadline");
        std::thread::yield_now();
    }
    assert!(app.world.reading && app.local_image_visible());
    // Renderer::capture uses scroll=0. All captures here are interpreted-image
    // frames; raw/metadata scrolling is checked separately, never miscaptured.
    assert_eq!(app.scroll, 0, "image arrows must pan, not reader-scroll");
    app.update_hud();
    let vertices = app.themes[app.theme_index].vertices_for(&app.world);
    let r = app.renderer.as_mut().unwrap();
    r.set_scene(&vertices);
    let path = group.join(stage);
    fs::create_dir(&path).expect("exclusive stage; never overwrite a capture");
    let output = resources::smoke_output(root, &path).unwrap();
    r.capture(&output, "office.png", &app.pose, &app.world)
        .expect("actual shared GPU capture; setup failure is not feature RED");
    let encoded = fs::read(output.path().join("office.png")).unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(&encoded))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!([info.width, info.height], r.size);
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    pixels.truncate(info.buffer_size());
    println!(
        "ZOOM_CAPTURE {} encoded_sha256={} rgba_sha256={}",
        output.path().join("office.png").display(),
        digest(&encoded),
        digest(&pixels)
    );
    pixels
}
fn near(p: &[u8], color: [u8; 3]) -> bool {
    p[..3].iter().zip(color).all(|(a, b)| a.abs_diff(b) <= 2)
}
fn source_map(pixels: &[u8], [w, h]: [u32; 2]) -> Result<Vec<u8>> {
    if pixels.len() != (w * h * 4) as usize {
        return Err("PNG_ZOOM_FRAME_LENGTH".into());
    }
    let mut map = Vec::with_capacity((w * h) as usize);
    for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
        if p[3] != 255 {
            return Err("PNG_ZOOM_FRAME_OPACITY".into());
        }
        let c = COLORS
            .iter()
            .position(|c| near(p, *c))
            .map_or(0, |c| c as u8 + 1);
        let (x, y) = (i as u32 % w, i as u32 / w);
        // Current real renderer body: x=16..w-16, y=68..h-86.
        // Check the entire frame, not a mask that silently discards chrome leaks.
        if c != 0 && (x < 16 || x >= w - 16 || y < 68 || y >= h - 86) {
            return Err(format!("PNG_ZOOM_BODY_CLIP xy={x},{y} source_color={c}"));
        }
        map.push(c);
    }
    if !map.iter().any(|c| *c != 0) {
        return Err("PNG_ZOOM_SOURCE_PIXELS_MISSING".into());
    }
    Ok(map)
}
fn bounds(map: &[u8], w: u32, color: u8) -> Result<[f32; 4]> {
    let mut b = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for (i, c) in map.iter().enumerate() {
        if (color == 0 && *c != 0) || (color != 0 && *c == color) {
            let (x, y) = ((i as u32 % w) as f32, (i as u32 / w) as f32);
            b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
        }
    }
    if !b.iter().all(|x| x.is_finite()) {
        return Err(format!("PNG_ZOOM_LANDMARK_MISSING color={color}"));
    }
    Ok(b)
}
fn dimensions(b: [f32; 4]) -> [f32; 2] {
    [b[2] - b[0] + 1.0, b[3] - b[1] + 1.0]
}
fn center(b: [f32; 4]) -> [f32; 2] {
    [(b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5]
}
fn initial_fit(map: &[u8], [w, _h]: [u32; 2]) -> Result<()> {
    let all = bounds(map, w, 0)?;
    let [iw, ih] = dimensions(all);
    if iw < 240.0 || ih < 120.0 || (iw / ih - 2.0).abs() > 0.08 {
        return Err(format!("PNG_ZOOM_INITIAL_FIT bounds={all:?}"));
    }
    // Source-specific positions, not simply six colors anywhere in the frame.
    for (c, fx, fy) in [
        (1, 0.15, 0.2),
        (2, 0.85, 0.2),
        (3, 0.15, 0.8),
        (4, 0.85, 0.8),
        (5, 0.5, 0.5),
        (6, 29.0 / 96.0, 15.0 / 48.0),
    ] {
        let x = (all[0] + fx * iw) as u32;
        let y = (all[1] + fy * ih) as u32;
        for yy in y - 1..=y + 1 {
            for xx in x - 1..=x + 1 {
                if map[(yy * w + xx) as usize] != c {
                    return Err(format!(
                        "PNG_ZOOM_INITIAL_SOURCE_POSITION color={c} xy={xx},{yy}"
                    ));
                }
            }
        }
    }
    Ok(())
}
fn magnified(before: &[u8], after: &[u8], w: u32) -> Result<()> {
    let a = dimensions(bounds(before, w, 5)?);
    let b = dimensions(bounds(after, w, 5)?);
    println!("ZOOM_LANDMARK before={a:?} after={b:?}");
    if b[0] <= a[0] + 1.0 || b[1] <= a[1] + 1.0 {
        return Err(format!(
            "PNG_EQUAL_SOURCE_GEOMETRY_UNCHANGED: central source landmark must magnify on both axes; before={a:?} after={b:?}"
        ));
    }
    if (b[0] / a[0] - b[1] / a[1]).abs() > 0.15 {
        return Err(format!("PNG_ZOOM_NONPROPORTIONAL before={a:?} after={b:?}"));
    }
    Ok(())
}
fn raw_bytes(doc: &str) -> Option<Vec<u8>> {
    let body = doc
        .split_once("\nBYTE GROUPS (8 bytes max)\n")?
        .1
        .split_once("\nFULL SOURCE / METADATA:\n")?
        .0;
    let mut bytes = Vec::new();
    for line in body.lines().filter_map(|l| l.strip_prefix("HEX ")) {
        for token in line.split_whitespace() {
            bytes.push(u8::from_str_radix(token, 16).ok()?);
        }
    }
    Some(bytes)
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
#[ignore = "serialized retained Metal zoom tracer; absent HAIR_IMAGE_ZOOM_PROOF_ROOT under owned TMPDIR"]
fn retained_opened_png_equal_pan_reset_changes_source_framebuffer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let temp = std::env::temp_dir().canonicalize().unwrap();
    assert!(temp.starts_with(root.parent().unwrap()) && separate(&temp, &root));
    let requested = PathBuf::from(
        std::env::var_os("HAIR_IMAGE_ZOOM_PROOF_ROOT")
            .expect("explicit absent proof root required"),
    );
    assert!(requested.is_absolute());
    assert_eq!(requested.parent().unwrap().canonicalize().unwrap(), temp);
    let output = temp.join(requested.file_name().unwrap());
    let mut sibling = requested.file_name().unwrap().to_os_string();
    sibling.push(".fixture");
    let fixture = temp.join(sibling);
    assert!(separate(&output, &fixture) && separate(&fixture, &root));
    fs::create_dir(&output).expect("new output only");
    fs::create_dir(&fixture).expect("new owned input only");
    println!(
        "ZOOM_RETAINED output={} fixture={}",
        output.display(),
        fixture.display()
    );
    // Exact live-vs-compiled checks are a subset, NOT a complete source freeze.
    for (name, bytes) in [
        (
            "local_image_zoom_tests.rs",
            include_bytes!("local_image_zoom_tests.rs").as_slice(),
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
        ("render.rs", include_bytes!("render.rs").as_slice()),
        ("text.rs", include_bytes!("text.rs").as_slice()),
        ("shader.wgsl", include_bytes!("shader.wgsl").as_slice()),
    ] {
        assert_eq!(
            fs::read(root.join("src").join(name)).unwrap(),
            bytes,
            "rebuild changed {name}"
        );
        println!("ZOOM_COMPILED_SOURCE src/{name} sha256={}", digest(bytes));
    }
    let name = OsString::from("Zoom Pixel Case\né.PNG");
    let original = fixture_png(false);
    let decoy = fixture_png(true);
    println!(
        "ZOOM_INPUT name={name:?} dimensions={SOURCE:?} sha256={}",
        digest(&original)
    );
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let mut missing = Vec::new();
    for (label, size) in [("normal", [960, 640]), ("small", [640, 480])] {
        let selected = fixture.join(format!("{label}-selected"));
        let held = fixture.join(format!("{label}-held"));
        fs::create_dir(&selected).unwrap();
        exclusive(&selected.join(&name), &original);
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
        // Test-owned rename + live display-path impostor BEFORE actual E.
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
        let group = output.join(label);
        fs::create_dir(&group).unwrap();
        let mut r = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            size,
            &app.themes[0].vertices_for(&app.world),
        ))
        .unwrap();
        r.set_materials(&app.themes[0].textures).unwrap();
        app.renderer = Some(r);
        aim(&mut app, &id);
        let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
        let navigation: Vec<_> = app
            .world
            .navigation
            .rows
            .iter()
            .map(|r| (r.id.clone(), r.label.clone(), r.depth))
            .collect();
        tap(&mut app, KeyCode::KeyE);
        let fit = source_map(&capture(&mut app, &root, &group, "fit"), size).unwrap();
        initial_fit(&fit, size).expect("established genuine source fit prerequisite; not zoom RED");
        key(&mut app, KeyCode::Equal, true, false);
        let zoom = source_map(&capture(&mut app, &root, &group, "equal"), size).unwrap();
        let zoom_contract = magnified(&fit, &zoom, size[0]);
        // Missing zoom is recorded BEFORE secondary controls, so current source
        // reaches a genuine framebuffer assertion at BOTH sizes, not imaginary fields.
        if let Err(error) = &zoom_contract {
            missing.push(format!("{label}: {error}"));
        }
        key(&mut app, KeyCode::Equal, true, false); // duplicate held down, not a new tap
        key(&mut app, KeyCode::Equal, true, true); // OS repeat, not a new tap
        let held_zoom =
            source_map(&capture(&mut app, &root, &group, "equal-held-repeat"), size).unwrap();
        assert_eq!(
            held_zoom, zoom,
            "held/repeat Equal must not make extra zoom steps"
        );
        key(&mut app, KeyCode::Equal, false, false);
        if zoom_contract.is_ok() {
            tap(&mut app, KeyCode::Equal); // independent physical tap after release
            let mut enlarged = source_map(
                &capture(&mut app, &root, &group, "equal-physical-tap"),
                size,
            )
            .unwrap();
            // No prescribed zoom factor. Reach a crop on BOTH axes using measured
            // source scale, with a finite 16-tap qualifier budget, not zoom fields.
            let mut crop_ready = false;
            for n in 0..16 {
                let b = dimensions(bounds(&enlarged, size[0], 5).unwrap());
                if b[0] / 8.0 * 48.0 > (size[1] - 154) as f32 + 8.0 {
                    crop_ready = true;
                    break;
                }
                tap(&mut app, KeyCode::Equal);
                enlarged = source_map(
                    &capture(&mut app, &root, &group, &format!("crop-step-{n}")),
                    size,
                )
                .unwrap();
            }
            if !crop_ready {
                missing.push(format!("{label}: PNG_PAN_CROP_NOT_REACHED_WITHIN_16_TAPS"));
            }
            if crop_ready {
                let base = bounds(&enlarged, size[0], 5).unwrap();
                for (axis, go, back) in [
                    (0, KeyCode::ArrowRight, KeyCode::ArrowLeft),
                    (1, KeyCode::ArrowDown, KeyCode::ArrowUp),
                ] {
                    tap(&mut app, go);
                    let moved = source_map(
                        &capture(&mut app, &root, &group, &format!("pan-{axis}")),
                        size,
                    )
                    .unwrap();
                    let b = bounds(&moved, size[0], 5).unwrap();
                    let delta = center(b)[axis] - center(base)[axis];
                    let orthogonal = center(b)[1 - axis] - center(base)[1 - axis];
                    // Either arrow convention is permitted, but it must translate
                    // the same source landmark along ONLY the selected axis.
                    if delta.abs() < 1.0
                        || orthogonal.abs() > 1.0
                        || dimensions(b)
                            .iter()
                            .zip(dimensions(base))
                            .any(|(a, b)| (*a - b).abs() > 2.0)
                    {
                        missing.push(format!("{label}: PNG_PAN_SOURCE_TRANSLATION axis={axis} base={base:?} actual={b:?}"));
                    }
                    tap(&mut app, back);
                    let returned = source_map(
                        &capture(&mut app, &root, &group, &format!("pan-return-{axis}")),
                        size,
                    )
                    .unwrap();
                    let c = center(bounds(&returned, size[0], 5).unwrap());
                    if c.iter()
                        .zip(center(base))
                        .any(|(a, b)| (*a - b).abs() > 2.0)
                    {
                        missing.push(format!(
                            "{label}: PNG_PAN_OPPOSITE_DID_NOT_RETURN axis={axis}"
                        ));
                    }
                }
            }
            tap(&mut app, KeyCode::Minus);
            let less = source_map(&capture(&mut app, &root, &group, "minus"), size).unwrap();
            if let Err(error) = magnified(&less, &enlarged, size[0]) {
                missing.push(format!("{label}: PNG_MINUS_NOT_REDUCED: {error}"));
            }
            tap(&mut app, KeyCode::Digit0);
            let reset = source_map(&capture(&mut app, &root, &group, "reset"), size).unwrap();
            assert_eq!(
                reset, fit,
                "Digit0 restores exact source framing, not HUD text alone"
            );
            // Extreme physical taps must remain drawable, bounded to body, and
            // recoverable. No chosen max scale or unobservable float field assumed.
            for _ in 0..64 {
                tap(&mut app, KeyCode::Equal);
            }
            source_map(&capture(&mut app, &root, &group, "bounded-zoom"), size).unwrap();
            for (axis, code) in [
                ("right", KeyCode::ArrowRight),
                ("down", KeyCode::ArrowDown),
                ("left", KeyCode::ArrowLeft),
                ("up", KeyCode::ArrowUp),
            ] {
                for _ in 0..64 {
                    tap(&mut app, code);
                }
                source_map(
                    &capture(&mut app, &root, &group, &format!("bounded-{axis}")),
                    size,
                )
                .unwrap();
            }
            for _ in 0..128 {
                tap(&mut app, KeyCode::Minus);
            }
            source_map(&capture(&mut app, &root, &group, "bounded-minus"), size).unwrap();
        }
        tap(&mut app, KeyCode::Digit0);
        tap(&mut app, KeyCode::KeyR); // remains history Back, suppressed by reader modal; NOT reset
        let reset = source_map(
            &capture(&mut app, &root, &group, "reset-before-modes"),
            size,
        )
        .unwrap();
        assert_eq!(reset, fit);
        tap(&mut app, KeyCode::KeyI);
        assert!(!app.local_image_visible());
        let metadata = app.world.document().unwrap().to_string();
        assert!(metadata.contains(&hair::folder_scene::display(&name)));
        assert!(metadata.contains(&hair::folder_scene::display(bound.entry.path.as_os_str())));
        for code in [KeyCode::Equal, KeyCode::Minus, KeyCode::Digit0] {
            tap(&mut app, code);
        }
        assert_eq!(app.world.document().unwrap(), metadata);
        tap(&mut app, KeyCode::KeyI);
        tap(&mut app, KeyCode::KeyV);
        assert!(!app.local_image_visible());
        assert_eq!(
            raw_bytes(app.world.document().unwrap()).as_deref(),
            Some(original.as_slice())
        );
        let raw = app.world.document().unwrap().to_string();
        assert!(raw.contains("LIMIT: 4096 bytes/chunk; no whole-file load"));
        for code in [
            KeyCode::Equal,
            KeyCode::Minus,
            KeyCode::Digit0,
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
        ] {
            tap(&mut app, code);
        }
        assert_eq!(
            app.world.document().unwrap(),
            raw,
            "raw EOF/zero paging and zoom keys inert"
        );
        tap(&mut app, KeyCode::ArrowDown);
        assert_eq!(app.scroll, 1, "raw Down remains scrolling, not image pan");
        key(&mut app, KeyCode::ArrowDown, true, true);
        key(&mut app, KeyCode::ArrowDown, false, false);
        assert_eq!(
            app.scroll, 2,
            "raw Down repeat retains existing scrolling route"
        );
        tap(&mut app, KeyCode::ArrowUp);
        tap(&mut app, KeyCode::ArrowUp);
        assert_eq!(app.scroll, 0);
        tap(&mut app, KeyCode::KeyV);
        assert!(!app.local_image_visible());
        assert!(
            app.world
                .document()
                .unwrap()
                .contains("\nTEXT (links inactive; escapes shown; range only):\n")
        );
        tap(&mut app, KeyCode::KeyV);
        let mode_return =
            source_map(&capture(&mut app, &root, &group, "mode-return"), size).unwrap();
        assert_eq!(mode_return, fit, "I/V leave fit source framing usable");
        tap(&mut app, KeyCode::KeyC); // existing same-handle retry, never repurposed as reset
        let retry =
            source_map(&capture(&mut app, &root, &group, "same-handle-retry"), size).unwrap();
        assert_eq!(
            retry, fit,
            "C retains genuine held original, not display-path impostor"
        );
        assert!(app.world.reading && app.focused && !app.input.captured && !app.input.forward);
        assert!(!app.chooser_active && !app.chooser_pending && app.address.is_none());
        assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
        assert_eq!(app.themes[0].host, host);
        let after = app.world.folder_view.as_ref().unwrap();
        // Reader document is intentionally different; every other folder field is not.
        let mut without_reader = after.clone();
        without_reader.reader = folder.reader.clone();
        assert_eq!(without_reader, folder);
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
        assert!(!app.input.forward && !app.input.captured && app.focused);
        assert!(!app.chooser_active && !app.chooser_pending && app.address.is_none());
        assert_eq!(app.themes[0].host, host);
        assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
        assert_eq!(app.world.current, current);
        assert_eq!(visits(&app), history);
        assert_eq!(inventory(&held), held_before);
        assert_eq!(inventory(&selected), decoy_before);
        app.renderer.as_ref().unwrap().wait().unwrap();
        drop(app);
    }
    drop(adapter);
    drop(instance);
    println!(
        "ZOOM_TRACER_VERDICT missing={missing:?}; synthetic keyboard + actual Metal framebuffer only"
    );
    assert!(
        missing.is_empty(),
        "LOCAL_PNG_ZOOM_PAN_RESET_FRAMEBUFFER: {missing:?}"
    );
}

// Supplementary gate, NOT a replacement or oracle migration for either retained
// tracer. Execute only after the unchanged external r2 below-fit tracer is GREEN.
#[test]
#[ignore = "serialized Metal bounds/modes; absent HAIR_IMAGE_ZOOM_BOUNDS_ROOT under owned TMPDIR"]
fn opened_png_below_fit_bounds_modes_and_physical_releases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let temp = std::env::temp_dir().canonicalize().unwrap();
    assert_eq!(temp.parent(), root.parent());
    assert!(separate(&temp, &root));
    let output = PathBuf::from(
        std::env::var_os("HAIR_IMAGE_ZOOM_BOUNDS_ROOT").expect("explicit absent proof root"),
    );
    assert!(output.is_absolute());
    assert_eq!(output.parent().unwrap().canonicalize().unwrap(), temp);
    let fixture = output.with_extension("fixture");
    assert!(separate(&output, &fixture) && separate(&fixture, &root));
    fs::create_dir(&output).expect("new retained captures only");
    fs::create_dir(&fixture).expect("new owned fixture only");
    let original = fixture_png(false);
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    for (label, size) in [("normal", [960, 640]), ("small", [640, 480])] {
        let selected = fixture.join(label);
        fs::create_dir(&selected).unwrap();
        exclusive(&selected.join("bounds.png"), &original);
        let before = inventory(&selected);
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
        app.complete_folder(Ok(Some(selected))).unwrap();
        let id = app
            .world
            .folder_view
            .as_ref()
            .unwrap()
            .bindings
            .keys()
            .next()
            .unwrap()
            .clone();
        aim(&mut app, &id);
        let host = app.themes[0].host.clone();
        let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
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
        tap(&mut app, KeyCode::KeyE);
        let fit = source_map(&capture(&mut app, &root, &group, "fit"), size).unwrap();
        let view = |app: &App| {
            app.local
                .as_ref()
                .unwrap()
                .reader
                .as_ref()
                .unwrap()
                .image_view()
        };
        let texture = app
            .local
            .as_ref()
            .unwrap()
            .reader
            .as_ref()
            .unwrap()
            .image_texture()
            .unwrap()
            .clone();
        let session_address = app.local.as_ref().unwrap().reader.as_ref().unwrap()
            as *const raw_reader::Session as usize;
        let attempts = app.object_previews.test_model_fallback_progress().0;
        let document = app.world.document().unwrap().to_string();
        key(&mut app, KeyCode::Minus, true, false);
        let once = view(&app);
        assert!(once[0] < 1.0 && once[0] > 0.125);
        key(&mut app, KeyCode::Minus, true, false);
        key(&mut app, KeyCode::Minus, true, true);
        assert_eq!(view(&app), once, "held/repeat minus is not a fresh tap");
        key(&mut app, KeyCode::Minus, false, false);
        tap(&mut app, KeyCode::NumpadSubtract);
        assert!(
            view(&app)[0] < once[0],
            "keypad minus shares below-fit controls"
        );
        tap(&mut app, KeyCode::NumpadAdd);
        assert!((view(&app)[0] - once[0]).abs() < 0.000001);
        for _ in 0..64 {
            tap(&mut app, KeyCode::Minus);
        }
        assert_eq!(view(&app), [0.125, 0.0, 0.0]);
        let minimum = source_map(&capture(&mut app, &root, &group, "minimum"), size).unwrap();
        let fit_bounds = bounds(&fit, size[0], 0).unwrap();
        let min_bounds = bounds(&minimum, size[0], 0).unwrap();
        let expected = dimensions(fit_bounds).map(|v| v * 0.125);
        assert!(
            dimensions(min_bounds)
                .iter()
                .zip(expected)
                .all(|(a, b)| (*a - b).abs() <= 2.0)
        );
        assert!(
            center(min_bounds)
                .iter()
                .zip(center(fit_bounds))
                .all(|(a, b)| (*a - b).abs() <= 1.0)
        );
        let [mw, mh] = dimensions(min_bounds);
        assert!((mw - 2.0 * mh).abs() <= 3.0, "minimum remains proportional");
        for code in [
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::ArrowUp,
            KeyCode::ArrowDown,
        ] {
            for _ in 0..32 {
                tap(&mut app, code);
            }
            assert_eq!(view(&app), [0.125, 0.0, 0.0], "no hidden below-fit pan");
        }
        for _ in 0..64 {
            tap(&mut app, KeyCode::NumpadSubtract);
        }
        let min_held = source_map(
            &capture(&mut app, &root, &group, "minimum-bounded-pan"),
            size,
        )
        .unwrap();
        assert_eq!(
            min_held, minimum,
            "saturated shrink/pan cannot move small pixels"
        );
        tap(&mut app, KeyCode::KeyI);
        let metadata = app.world.document().unwrap().to_string();
        for code in [
            KeyCode::Equal,
            KeyCode::Minus,
            KeyCode::Digit0,
            KeyCode::NumpadAdd,
            KeyCode::NumpadSubtract,
        ] {
            tap(&mut app, code);
        }
        assert_eq!(app.world.document().unwrap(), metadata);
        assert_eq!(view(&app), [0.125, 0.0, 0.0]);
        tap(&mut app, KeyCode::KeyI);
        tap(&mut app, KeyCode::KeyV);
        assert_eq!(
            raw_bytes(app.world.document().unwrap()).as_deref(),
            Some(original.as_slice())
        );
        for code in [
            KeyCode::Equal,
            KeyCode::Minus,
            KeyCode::Digit0,
            KeyCode::NumpadAdd,
            KeyCode::NumpadSubtract,
        ] {
            tap(&mut app, code);
        }
        assert_eq!(view(&app), [0.125, 0.0, 0.0]);
        tap(&mut app, KeyCode::KeyV);
        tap(&mut app, KeyCode::KeyV);
        let returned = source_map(
            &capture(&mut app, &root, &group, "minimum-mode-return"),
            size,
        )
        .unwrap();
        assert_eq!(returned, minimum);
        // Each physical release is delivered while a different mode owns input.
        // Test all zoom bindings and both pan axes, not just a helper setter.
        for raw in [false, true] {
            for (code, axis) in [
                (KeyCode::Minus, 0),
                (KeyCode::Equal, 0),
                (KeyCode::NumpadSubtract, 0),
                (KeyCode::NumpadAdd, 0),
                (KeyCode::ArrowRight, 1),
                (KeyCode::ArrowDown, 2),
            ] {
                tap(&mut app, KeyCode::Digit0);
                for _ in 0..4 {
                    tap(&mut app, KeyCode::Equal);
                }
                key(&mut app, code, true, false);
                let pressed = view(&app);
                tap(&mut app, if raw { KeyCode::KeyV } else { KeyCode::KeyI });
                assert!(!app.local_image_visible());
                key(&mut app, code, false, false);
                tap(&mut app, if raw { KeyCode::KeyV } else { KeyCode::KeyI });
                if raw {
                    tap(&mut app, KeyCode::KeyV);
                }
                assert!(app.local_image_visible());
                tap(&mut app, code);
                assert_ne!(
                    view(&app)[axis],
                    pressed[axis],
                    "release across mode must admit next real press: {code:?}, raw={raw}"
                );
            }
        }
        tap(&mut app, KeyCode::Digit0);
        for _ in 0..64 {
            tap(&mut app, KeyCode::Equal);
        }
        assert_eq!(view(&app), [8.0, 0.0, 0.0]);
        let maximum = source_map(&capture(&mut app, &root, &group, "maximum"), size).unwrap();
        for _ in 0..64 {
            tap(&mut app, KeyCode::NumpadAdd);
        }
        let max_held =
            source_map(&capture(&mut app, &root, &group, "maximum-bounded"), size).unwrap();
        assert_eq!(max_held, maximum);
        for (stage, code, axis, end) in [
            ("right", KeyCode::ArrowRight, 1, 1.0),
            ("down", KeyCode::ArrowDown, 2, 1.0),
            ("left", KeyCode::ArrowLeft, 1, -1.0),
            ("up", KeyCode::ArrowUp, 2, -1.0),
        ] {
            for _ in 0..32 {
                tap(&mut app, code);
            }
            assert_eq!(view(&app)[axis], end);
            source_map(&capture(&mut app, &root, &group, stage), size).unwrap();
        }
        for _ in 0..128 {
            tap(&mut app, KeyCode::Minus);
        }
        assert_eq!(view(&app), [0.125, 0.0, 0.0]);
        let shrunk =
            source_map(&capture(&mut app, &root, &group, "minimum-after-pan"), size).unwrap();
        assert_eq!(shrunk, minimum, "shrinking clears magnified pan");
        tap(&mut app, KeyCode::Digit0);
        let reset = source_map(&capture(&mut app, &root, &group, "fit-reset"), size).unwrap();
        assert_eq!(reset, fit);
        let session = app.local.as_ref().unwrap().reader.as_ref().unwrap();
        assert_eq!(
            session as *const raw_reader::Session as usize,
            session_address
        );
        assert!(Arc::ptr_eq(session.image_texture().unwrap(), &texture));
        assert_eq!(app.world.document().unwrap(), document);
        assert!(!app.images.busy() && !app.local_image_pending() && !app.audio.busy());
        assert_eq!(
            app.object_previews.test_model_fallback_progress().0,
            attempts
        );
        assert_eq!(app.themes[0].host, host);
        assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
        assert!(app.world.reading && app.focused && !app.input.forward && !app.input.captured);
        tap(&mut app, KeyCode::Escape);
        assert!(!app.world.reading && app.local.as_ref().unwrap().reader.is_none());
        assert_eq!(inventory(&fixture.join(label)), before);
        app.renderer.as_ref().unwrap().wait().unwrap();
        println!(
            "BELOW_FIT_BOUNDS_OK {label} minimum={:?}",
            dimensions(min_bounds)
        );
    }
    println!("OPENED_PNG_BELOW_FIT_BOUNDS_PASS: min/max/pan/modes/releases, both sizes");
}
