//! One pre-E tracer. Only fixture encoding and GPU readback live in this harness.
//! No decoded result, preview state, material, scene vertex or pixel is injected.
use super::*;
use glam::{Mat4, Vec3};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};

const COLORS: [[u8; 3]; 4] = [[241, 17, 53], [23, 209, 71], [31, 67, 239], [233, 181, 29]];
const DECOY: [[u8; 3]; 4] = [
    [149, 41, 193],
    [47, 173, 197],
    [199, 91, 43],
    [109, 137, 61],
];
const SIZE: [u32; 2] = [48, 24];
const FRAME_BUDGET: usize = 256;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn fixture_png(colors: &[[u8; 3]; 4]) -> Vec<u8> {
    let mut rgb = Vec::new();
    for y in 0..SIZE[1] {
        for x in 0..SIZE[0] {
            rgb.extend_from_slice(
                &colors[usize::from(x >= SIZE[0] / 2) + 2 * usize::from(y >= SIZE[1] / 2)],
            );
        }
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, SIZE[0], SIZE[1]);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&rgb).unwrap();
        writer.finish().unwrap();
    }
    assert!(bytes.len() < hair::local_explorer::READER_CHUNK_BYTES);
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
type FileState = (u64, u64, u32, u64, i64, i64, Vec<u8>);
fn state(path: &Path) -> FileState {
    let m = fs::symlink_metadata(path).unwrap();
    assert!(m.is_file() && !m.file_type().is_symlink());
    (
        m.dev(),
        m.ino(),
        m.mode(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        fs::read(path).unwrap(),
    )
}
fn separate(a: &Path, b: &Path) -> bool {
    !a.starts_with(b) && !b.starts_with(a)
}
fn matrix(p: &Pose, [w, h]: [u32; 2]) -> Mat4 {
    // Exact Renderer::draw_frame visitor camera, including the real eye height.
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
    assert!(clip.w > 0.05);
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
        .all(|(a, b)| a.abs_diff(b) <= 3)
}
fn pixel_contract(
    pixels: &[u8],
    size: [u32; 2],
    pose: &Pose,
    a: &hair::host_scene::Anchor,
) -> Result<()> {
    let [w, h] = size;
    if pixels.len() != (w * h * 4) as usize || pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
    {
        return Err("PNG_OBJECT_FRAMEBUFFER_LENGTH_OR_OPACITY".into());
    }
    let m = matrix(pose, size);
    let inv = m.inverse();
    // Project the ACTUAL host-owned volume, not the whole frame or a green mask.
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
    let mut hits = Vec::new();
    let mut counts = [0_usize; 4];
    let mut wrong = 0;
    for y in roi[1].floor().max(0.0) as u32..=(roi[3].ceil() as u32).min(h - 1) {
        for x in roi[0].floor().max(0.0) as u32..=(roi[2].ceil() as u32).min(w - 1) {
            let at = ((y * w + x) * 4) as usize;
            let p = &pixels[at..at + 4];
            if let Some(c) = COLORS.iter().position(|color| near(p, *color)) {
                counts[c] += 1;
                hits.push([x, y]);
            }
            if DECOY.iter().any(|color| near(p, *color)) {
                wrong += 1;
            }
        }
    }
    if counts.iter().any(|n| *n < 16) {
        return Err(format!(
            "PNG_OBJECT_SOURCE_PIXELS_MISSING: pre-E expected four genuine source-color regions in anchor {}; counts={counts:?} roi={roi:?}",
            a.id
        ));
    }
    if wrong >= 16 {
        return Err(format!("PNG_OBJECT_WRONG_SOURCE: decoy pixels={wrong}"));
    }
    // Undo perspective rather than incorrectly requiring a 2:1 screen-space box
    // while looking down from 1.65m. Accept a vertical preview plane anywhere
    // INSIDE the existing depth envelope; do not prescribe a new product API.
    let mut fits = Vec::new();
    for step in 0..=16 {
        let z = a.at[2] - a.size[2] * 0.5 + a.size[2] * step as f32 / 16.0;
        let eye = Vec3::new(pose.position[0], 1.65, pose.position[1]);
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
        let iw = b[2] - b[0];
        let ih = b[3] - b[1];
        let aspect = SIZE[0] as f32 / SIZE[1] as f32;
        let epsilon = 0.012;
        if iw < a.size[0] * 0.5
            || ih <= 0.0
            || (iw / ih - aspect).abs() > 0.15
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
        for (c, (fx, fy)) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
            .into_iter()
            .enumerate()
        {
            let p = project(m, Vec3::new(b[0] + fx * iw, b[3] - fy * ih, z), size);
            let (cx, cy) = (p[0].round() as i32, p[1].round() as i32);
            for y in cy - 1..=cy + 1 {
                for x in cx - 1..=cx + 1 {
                    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                        good = false;
                        continue;
                    }
                    let at = ((y as u32 * w + x as u32) * 4) as usize;
                    good &= near(&pixels[at..at + 4], COLORS[c]);
                }
            }
        }
        if good {
            fits.push((z, b));
        }
    }
    if fits.is_empty() {
        return Err(format!(
            "PNG_OBJECT_FIT_OR_ORIENTATION: counts={counts:?}; no centered proportional source patches fit actual host envelope"
        ));
    }
    Ok(())
}

fn frame(app: &mut App) -> Result<Vec<u8>> {
    // Existing redraw consumers only; App::redraw itself requires a native surface
    // and ActiveEventLoop. No new preview seam is assumed. Future scheduling must
    // be reachable through the normal production poll consumer, not this test.
    app.poll_local_image()?;
    hair::interaction::refresh(&app.themes[app.theme_index].host, &app.pose, &mut app.world);
    app.update_hud();
    let vertices = app.themes[app.theme_index].vertices_for(&app.world);
    let r = app.renderer.as_mut().unwrap();
    r.set_scene(&vertices);
    let [width, height] = r.size;
    let texture = r.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pre-E actual framebuffer"),
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
        label: Some("pre-E actual readback"),
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
fn retain(output: &resources::SmokeOutput, size: [u32; 2], pixels: &[u8]) {
    // Preserve the protected writer's real allowlist/O_EXCL/held-directory policy.
    // Unlike Renderer::capture, this scoped readback must not abort on <20 colors
    // before the useful source-pixel assertion. Generic FAIL is not reclassified.
    let unique: HashSet<_> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    println!(
        "OBJECT_GENERIC_DIVERSITY colors={} threshold=20 classification={} (scoped readback only; original generic FAIL unchanged)",
        unique.len(),
        if unique.len() < 20 { "FAIL" } else { "PASS" }
    );
    let file = output.create_capture("office.png").unwrap();
    let mut encoder = png::Encoder::new(file, size[0], size[1]);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(pixels).unwrap();
    writer.finish().unwrap();
    println!(
        "OBJECT_CAPTURE {} rgba_sha256={}",
        output.path().join("office.png").display(),
        digest(pixels)
    );
}
fn labels(app: &App) -> Vec<u8> {
    // Actual Theme label geometry above document envelopes; not fabricated quads.
    let vertices: Vec<_> = app.themes[app.theme_index]
        .vertices_for(&app.world)
        .into_iter()
        .filter(|v| v.position[1] > 0.90)
        .collect();
    bytemuck::cast_slice(&vertices).to_vec()
}

#[test]
#[ignore = "serialized retained Metal pre-E tracer; absent HAIR_OBJECT_PREVIEW_PROOF_ROOT under owned TMPDIR"]
fn retained_png_before_e_has_source_pixels_inside_document_envelope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let owned = root.parent().unwrap();
    let temp = std::env::temp_dir().canonicalize().unwrap();
    assert!(temp.starts_with(owned) && separate(&temp, &root));
    let requested = PathBuf::from(
        std::env::var_os("HAIR_OBJECT_PREVIEW_PROOF_ROOT")
            .expect("explicit absent retained output root"),
    );
    assert!(requested.is_absolute());
    assert_eq!(requested.parent().unwrap().canonicalize().unwrap(), temp);
    let output = temp.join(requested.file_name().unwrap());
    let mut sibling = requested.file_name().unwrap().to_os_string();
    sibling.push(".fixture");
    let fixture = temp.join(sibling);
    assert!(separate(&output, &fixture) && separate(&fixture, &root));
    fs::create_dir(&output).expect("new output only; never overwrite evidence");
    fs::create_dir(&fixture).expect("new owned fixture only");
    let name = OsString::from("A Pixel Case\né.PNG"); // Valid UTF-8 on APFS, literal raw component retained.
    let decoy_name = OsString::from("Z Other Colors.png");
    let original = fixture_png(&COLORS);
    exclusive(&fixture.join(&name), &original);
    exclusive(&fixture.join(&decoy_name), &fixture_png(&DECOY));
    let before = [
        state(&fixture.join(&name)),
        state(&fixture.join(&decoy_name)),
    ];
    let fixture_mode = fs::metadata(&fixture).unwrap().mode();
    println!(
        "OBJECT_INPUT fixture={} name={name:?} sha256={} dimensions={SIZE:?}",
        fixture.display(),
        digest(&original)
    );
    for (name, bytes) in [
        (
            "local_object_preview_tests.rs",
            include_bytes!("local_object_preview_tests.rs").as_slice(),
        ),
        (
            "folder_native.rs",
            include_bytes!("folder_native.rs").as_slice(),
        ),
        ("main.rs", include_bytes!("main.rs").as_slice()),
        ("theme.rs", include_bytes!("theme.rs").as_slice()),
        ("render.rs", include_bytes!("render.rs").as_slice()),
        ("shader.wgsl", include_bytes!("shader.wgsl").as_slice()),
    ] {
        assert_eq!(
            fs::read(root.join("src").join(name)).unwrap(),
            bytes,
            "rebuild changed source {name}"
        );
        println!("OBJECT_COMPILED_SOURCE src/{name} sha256={}", digest(bytes));
    }
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let mut missing = Vec::new();
    for (label, size) in [("normal", [960, 640]), ("small", [640, 480])] {
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
        app.complete_folder(Ok(Some(fixture.clone()))).unwrap();
        let view = app.world.folder_view.as_ref().unwrap().clone();
        assert_eq!(view.bindings.len(), 2);
        let (id, bound) = view
            .bindings
            .iter()
            .find(|(_, b)| b.entry.name == name)
            .unwrap();
        assert_eq!(bound.entry.path, fixture.join(&name));
        assert_eq!(bound.entry.kind, Kind::File);
        assert!(view.bindings.values().any(|b| b.entry.name == decoy_name));
        let host = app.themes[0].host.clone();
        let a = host.anchors.iter().find(|a| &a.id == id).unwrap().clone();
        assert_eq!(a.role, "document");
        let distance = 1.25_f32;
        app.pose = Pose {
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
        let current = app.world.current.clone();
        let rows: Vec<_> = app
            .world
            .navigation
            .rows
            .iter()
            .map(|r| (r.id.clone(), r.label.clone(), r.depth))
            .collect();
        let label_geometry = labels(&app);
        let group = output.join(label);
        fs::create_dir(&group).unwrap();
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
        let start = Instant::now();
        let deadline = start + Duration::from_secs(5);
        let mut pixels = Vec::new();
        let mut contract = Err("PNG_OBJECT_SOURCE_PIXELS_MISSING: no frame".to_string());
        let mut frames = 0;
        for _ in 0..FRAME_BUDGET {
            pixels =
                frame(&mut app).expect("actual App poll/Metal readback setup; not behavioral RED");
            frames += 1;
            assert!(
                !app.world.reading && app.local.as_ref().unwrap().reader.is_none(),
                "pre-E work cannot open the reader"
            );
            assert!(!app.local_image_visible());
            contract = pixel_contract(&pixels, size, &app.pose, &a);
            if contract.is_ok() || Instant::now() >= deadline {
                break;
            }
            std::thread::yield_now(); // Yield to real workers, no blind sleep or invented completion.
        }
        retain(&captures, size, &pixels);
        println!(
            "OBJECT_WAIT {label} frames={frames}/{FRAME_BUDGET} elapsed_ms={} deadline_ms=5000 result={contract:?}",
            start.elapsed().as_millis()
        );
        if let Err(error) = contract {
            missing.push(format!("{label}: {error}"));
        }
        assert_eq!(
            app.themes[0].host, host,
            "preview cannot change anchors/collision/routes"
        );
        host.check_routes(&[app.pose.position]).unwrap();
        assert!(host.blocked([a.at[0], a.at[2]]) && !host.blocked(app.pose.position));
        assert_eq!(
            app.world.folder_view.as_ref().unwrap(),
            &view,
            "raw identity, routes and source labels unchanged"
        );
        assert_eq!(app.world.interaction, target);
        assert_eq!(app.world.explicit_target, explicit);
        assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
        assert_eq!(app.world.current, current);
        assert_eq!(
            app.world
                .navigation
                .rows
                .iter()
                .map(|r| (r.id.clone(), r.label.clone(), r.depth))
                .collect::<Vec<_>>(),
            rows
        );
        assert_eq!(
            labels(&app),
            label_geometry,
            "actual above-envelope labels unchanged"
        );
        assert!(!app.chooser_pending && !app.chooser_active && app.address.is_none());
        assert!(!app.world.reading && app.world.reader_link.is_none());
        assert_eq!(
            [
                state(&fixture.join(&name)),
                state(&fixture.join(&decoy_name))
            ],
            before
        );
        assert_eq!(fs::read_dir(&fixture).unwrap().count(), 2);
        assert_eq!(fs::metadata(&fixture).unwrap().mode(), fixture_mode);
        app.renderer.as_ref().unwrap().wait().unwrap();
        drop(app);
    }
    drop(adapter);
    drop(instance);
    assert!(
        missing.is_empty(),
        "LOCAL_PNG_BEFORE_E_OBJECT_FRAMEBUFFER: {missing:?}; an opened viewer or PNG byte text is not a pre-E world image"
    );
}
