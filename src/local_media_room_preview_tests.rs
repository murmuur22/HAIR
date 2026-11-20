//! Exact delivered room -> production App publication/input -> real offscreen frame.
//! File-only preparation: no execution receipt is implied. No positive cache or
//! scene/material injection. Optional copied-source observer is read-only/cfg(test).
//! Scope: App redraw pre-surface sequence + offscreen draw; not native window,
//! event-loop/surface acquisition, packaged executable or owner-machine proof.
use super::*;
use hair::assets::{Texture, Vertex};
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};

const VIDEO: &str = "Tiny Still.mp4";
const FILES: [(&str, usize, &str); 7] = [
    (
        "Matte Tetrahedron.glb",
        928,
        "d9be33887a525c8bb6a517577cbf819a8a301253c719a7ac5b3f6b0a0a81981d",
    ),
    (
        "Quadrants.jpg",
        1000,
        "60ed51d80655f627b39efee03a0b5b3e67d1a87d2324a616769c17ed90979df1",
    ),
    (
        "Quadrants.png",
        153,
        "5eaf7d9b2b8525e40bc8f7c7c0188cede38dcc9317360ddcca2b226495caaf12",
    ),
    (
        "Quiet Tone.wav",
        64044,
        "5760d006b6101d97d6035167f9ff7a28ae9067ad61bfe30c41234cff10daf7bf",
    ),
    (
        "Raw Sample.bin",
        8376,
        "c04ea4cac6e866f6b35649593faa4220c77212450166f34c4de4b1fd7999a29b",
    ),
    (
        "Read me.txt",
        104,
        "5779e58daf69d63ad1472ea4a9dbba630f0290d11f2d46d9a103592dffa46281",
    ),
    (
        VIDEO,
        2455,
        "9638f62dd10ef140610a04dd00b132c943e444b407b10307e823824ad78b43a6",
    ),
];
const COLORS: [[u8; 3]; 4] = [[220, 40, 40], [40, 200, 60], [40, 70, 220], [230, 200, 40]];
const SIZE: [u32; 2] = [960, 640];
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn stamp(m: &fs::Metadata) -> (u64, u64, u32, u64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.mode(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
    )
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
fn verified(path: &Path, len: usize, sha: &str) -> Vec<u8> {
    let mut f = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .expect("SETUP: exact owned public fixture required");
    let before = f.metadata().unwrap();
    assert!(before.is_file());
    assert_eq!(before.len(), len as u64);
    assert_eq!(before.mode() & 0o777, 0o644);
    let mut bytes = Vec::new();
    Read::by_ref(&mut f)
        .take(len as u64 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(stamp(&before), stamp(&f.metadata().unwrap()));
    assert_eq!(bytes.len(), len);
    assert_eq!(
        hash(&bytes),
        sha,
        "SETUP: fixture identity {}",
        path.display()
    );
    bytes
}
fn members(path: &Path) -> Vec<OsString> {
    let mut names: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    names.sort();
    names
}
fn id(app: &App, name: &str) -> String {
    app.world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == name)
        .expect("SETUP: real room binding")
        .0
        .clone()
}
fn aim(app: &mut App, name: &str) {
    let id = id(app, name);
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
    // Do not refresh here: normal frame update must observe the target change.
}
fn tap(app: &mut App, key: KeyCode) {
    for down in [true, false] {
        assert!(!app.keyboard_input(Some(key), down, false, None).unwrap());
    }
}
fn tick(app: &mut App) {
    // Production redraw order before surface acquisition. No unconditional scene
    // reupload: only the SAME target-change predicate as App::redraw is mirrored.
    app.poll_web().unwrap();
    app.poll_local_image().unwrap();
    if !app.world.reading {
        app.themes[app.theme_index].locomote(&mut app.pose, &app.input, 0.0);
    }
    let old = app.world.interaction.clone();
    hair::interaction::refresh(&app.themes[app.theme_index].host, &app.pose, &mut app.world);
    if old != app.world.interaction
        && let Some(r) = &mut app.renderer
    {
        r.set_scene(&app.themes[app.theme_index].vertices_for(&app.world));
    }
    app.update_hud();
}
fn ledger(app: &App, stage: &str, poll: usize) -> String {
    let (attempts, drained) = app.object_previews.test_model_fallback_progress();
    let raster: Vec<_> = app
        .object_previews
        .cache
        .iter()
        .map(|(id, t)| (id, t.width, t.height, t.rgba.len(), hash(&t.rgba)))
        .collect();
    let models: Vec<_> = app
        .object_previews
        .models
        .iter()
        .map(|(id, m)| {
            (
                id,
                m.vertices.len(),
                m.vertices.capacity(),
                hash(bytemuck::cast_slice(&m.vertices)),
            )
        })
        .collect();
    let raster_bytes: usize = app
        .object_previews
        .cache
        .values()
        .map(|t| t.rgba.len())
        .sum();
    let model_bytes: usize = app
        .object_previews
        .models
        .values()
        .map(|m| {
            m.vertices.capacity() * std::mem::size_of::<Vertex>()
                + std::mem::size_of::<hair::assets::Model>()
        })
        .sum();
    let theme = &app.themes[app.theme_index];
    let admission: Vec<_> = app
        .object_previews
        .cache
        .iter()
        .map(|(id, t)| (id, theme.textures.iter().position(|x| Arc::ptr_eq(x, t))))
        .collect();
    let line = format!(
        "stage={stage} poll={poll} attempts={attempts} drained={drained} raster_bytes={raster_bytes} model_charge={model_bytes} items={} theme_materials={} admission={admission:?} raster={raster:?} models={models:?} reading={} image_pending={} image_visible={} renderer={} status={:?}",
        raster.len() + models.len(),
        theme.textures.len(),
        app.world.reading,
        app.local_image_pending(),
        app.local_image_visible(),
        app.renderer.is_some(),
        app.object_previews.status
    );
    println!("ROOM_LEDGER {line}");
    line
}
fn settle(app: &mut App, stage: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last = String::new();
    let mut poll = 0;
    loop {
        tick(app);
        // Emit on semantic transitions, not every millisecond; full private
        // snapshots share this bounded transition log when parent enables them.
        let key = format!(
            "{:?}:{}:{}:{}:{}",
            app.object_previews.test_model_fallback_progress(),
            app.object_previews.cache.len(),
            app.object_previews.models.len(),
            app.local_image_pending(),
            app.object_previews.status
        );
        if key != last {
            ledger(app, stage, poll);
            last = key;
        }
        if app.object_previews.test_model_fallback_progress().1 && !app.local_image_pending() {
            break;
        }
        if Instant::now() >= deadline {
            ledger(
                app,
                &format!("{stage}-observation-window-ended-NOT-drained"),
                poll,
            );
            break; // keep evidence and test the exact missing-source symptom
        }
        poll += 1;
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn first_frame(t: &Texture) -> Result<()> {
    if [t.width, t.height] != [160, 96] || t.rgba.len() != 160 * 96 * 4 {
        return Err("VIDEO_SOURCE_DIMENSIONS".into());
    }
    if t.rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 255) {
        return Err("VIDEO_SOURCE_ALPHA".into());
    }
    for y in 8usize..88 {
        for x in 8usize..152 {
            if (48..64).contains(&x) || (32..48).contains(&y) {
                continue;
            }
            let c = COLORS[usize::from(x >= 56) + 2 * usize::from(y >= 40)];
            let p = &t.rgba[(y * 160 + x) * 4..][..4];
            if !p[..3].iter().zip(c).all(|(a, b)| a.abs_diff(b) <= 28) {
                return Err(format!(
                    "VIDEO_FIRST_FRAME_PIXELS x={x} y={y} actual={p:?} expected={c:?}"
                ));
            }
        }
    }
    for (x, y, c) in [
        (24usize, 18usize, 0),
        (110, 18, 1),
        (24, 70, 2),
        (110, 70, 3),
        (64, 18, 1),
        (24, 44, 2),
    ] {
        let p = &t.rgba[(y * 160 + x) * 4..][..4];
        if !p[..3]
            .iter()
            .zip(COLORS[c])
            .all(|(a, b)| a.abs_diff(b) <= 28)
        {
            return Err(format!("VIDEO_ASYMMETRIC_LANDMARK {x},{y} {p:?}"));
        }
    }
    Ok(())
}
fn expected_plane(a: &hair::host_scene::Anchor, t: &Texture, material: u32) -> Vec<Vertex> {
    let scale = (a.size[0] / t.width as f32).min(a.size[1] / t.height as f32);
    let (w, h) = (t.width as f32 * scale, t.height as f32 * scale);
    let left = a.at[0] - w * 0.5;
    let top = a.at[1] + (a.size[1] + h) * 0.5;
    let z = a.at[2] + a.size[2] * 0.45;
    [
        (left, top, [0., 0.]),
        (left + w, top, [1., 0.]),
        (left + w, top - h, [1., 1.]),
        (left, top, [0., 0.]),
        (left + w, top - h, [1., 1.]),
        (left, top - h, [0., 1.]),
    ]
    .map(|(x, y, uv)| Vertex {
        position: [x, y, z],
        color: [1.; 3],
        uv,
        texture: material | 0x8000_0000,
    })
    .to_vec()
}
fn contains_exact(scene: &[Vertex], expected: &[Vertex]) -> bool {
    let mut available = std::collections::BTreeMap::<Vec<u8>, usize>::new();
    for v in scene {
        *available.entry(bytemuck::bytes_of(v).to_vec()).or_default() += 1;
    }
    for v in expected {
        let Some(count) = available.get_mut(bytemuck::bytes_of(v)) else {
            return false;
        };
        if *count == 0 {
            return false;
        }
        *count -= 1;
    }
    true
}
fn placard(a: &hair::host_scene::Anchor) -> Vec<Vertex> {
    let at = [
        a.at[0] + 0.65,
        a.at[1] + a.size[1] - 0.20,
        a.at[2] - a.size[2] * 0.5 - 0.35,
    ];
    let mut out = hair::text::object_label(&hair::folder_scene::display(OsStr::new(VIDEO)), at, 25);
    out.extend(hair::text::object_label(
        "MP4 still",
        [at[0], at[1] - 0.14, at[2]],
        25,
    ));
    // Independent declaration of the compact private status marker, including
    // original corner operation order (not a rounded replacement center).
    let origin = [a.at[0], a.at[1] + a.size[1] + 0.03, a.at[2]];
    let size = [0.45, 0.02, 0.2];
    let corners: Vec<[f32; 3]> = (0..8)
        .map(|i| {
            std::array::from_fn(|j| {
                origin[j] + size[j] * if i & (1 << j) == 0 { -0.5 } else { 0.5 }
            })
        })
        .collect();
    for face in [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ] {
        for i in [0, 1, 2, 0, 2, 3] {
            let p = corners[face[i]];
            out.push(Vertex {
                position: [
                    a.at[0] + 0.65 + (p[0] - a.at[0]) * 0.5,
                    p[1] - 0.59,
                    a.at[2] - a.size[2] * 0.5 - 0.35 + (p[2] - a.at[2]) * 0.3,
                ],
                color: [0.1, 0.8, 0.3],
                uv: [0.; 2],
                texture: 0,
            });
        }
    }
    out
}
fn check_video(app: &App) -> Result<()> {
    let id = id(app, VIDEO);
    let t = app
        .object_previews
        .cache
        .get(&id)
        .ok_or("VIDEO_STILL_BEFORE_E_OR_RETURN_MISSING")?;
    first_frame(t)?;
    let theme = &app.themes[app.theme_index];
    let material = theme
        .textures
        .iter()
        .position(|x| Arc::ptr_eq(x, t))
        .ok_or("VIDEO_THEME_ADMISSION_MISSING")? as u32
        + 1;
    let a = theme.host.anchors.iter().find(|a| a.id == id).unwrap();
    let scene = theme.vertices_for(&app.world);
    let plane: Vec<_> = scene
        .iter()
        .filter(|v| v.texture == (material | 0x8000_0000))
        .copied()
        .collect();
    if bytemuck::cast_slice::<Vertex, u8>(&plane)
        != bytemuck::cast_slice::<Vertex, u8>(&expected_plane(a, t, material))
    {
        return Err("VIDEO_COMPLETE_SOURCE_PLANE_MISSING_OR_WRONG".into());
    }
    if !contains_exact(&scene, &placard(a)) {
        return Err("VIDEO_PRIVATE_PLACARD_MISSING_OR_WRONG".into());
    }
    Ok(())
}
fn peers(app: &App, room: &Path) -> Result<()> {
    for name in ["Quadrants.png", "Quadrants.jpg", "Read me.txt"] {
        let bytes = fs::read(room.join(name)).unwrap();
        let expected = if name.ends_with("txt") {
            hair::text::literal_excerpt_texture(std::str::from_utf8(&bytes).unwrap(), false)
        } else {
            hair::assets::decode_local_image(&bytes)?
        };
        let id = id(app, name);
        let actual = app
            .object_previews
            .cache
            .get(&id)
            .ok_or_else(|| format!("PEER_MISSING {name}"))?;
        if [actual.width, actual.height] != [expected.width, expected.height]
            || actual.rgba != expected.rgba
        {
            return Err(format!("PEER_ASSET_DECODE_EQUALITY {name}"));
        }
        let theme = &app.themes[app.theme_index];
        let material = theme
            .textures
            .iter()
            .position(|t| Arc::ptr_eq(t, actual))
            .ok_or("PEER_THEME_ADMISSION")? as u32
            + 1;
        if theme
            .vertices_for(&app.world)
            .iter()
            .filter(|v| v.texture == (material | 0x8000_0000))
            .count()
            != 6
        {
            return Err(format!("PEER_WORLD_PLANE {name}"));
        }
    }
    let name = "Matte Tetrahedron.glb";
    let expected = hair::assets::decode_local_model(&fs::read(room.join(name)).unwrap())?;
    let model_id = id(app, name);
    let actual = app
        .object_previews
        .models
        .get(&model_id)
        .ok_or("PEER_MODEL_MISSING")?;
    if actual.min != expected.min
        || actual.max != expected.max
        || bytemuck::cast_slice::<Vertex, u8>(&actual.vertices)
            != bytemuck::cast_slice::<Vertex, u8>(&expected.vertices)
    {
        return Err("PEER_MODEL_ASSET_DECODE_EQUALITY".into());
    }
    let theme = &app.themes[app.theme_index];
    let a = theme
        .host
        .anchors
        .iter()
        .find(|a| a.id == model_id)
        .unwrap();
    let ext: [f32; 3] = std::array::from_fn(|i| expected.max[i] - expected.min[i]);
    let scale = (0..3)
        .filter(|i| ext[*i] > 0.)
        .map(|i| a.size[i] / ext[i])
        .fold(f32::INFINITY, f32::min);
    let center = [a.at[0], a.at[1] + a.size[1] * 0.5, a.at[2]];
    let fitted: Vec<_> = expected
        .vertices
        .iter()
        .map(|v| {
            let mut v = *v;
            for (i, component) in center.iter().enumerate() {
                v.position[i] = *component
                    + (v.position[i] - (expected.min[i] + expected.max[i]) * 0.5) * scale;
            }
            v
        })
        .collect();
    if !contains_exact(&theme.vertices_for(&app.world), &fitted) {
        return Err("PEER_MODEL_WORLD_CONSUMER".into());
    }
    for name in ["Quiet Tone.wav", "Raw Sample.bin", "Empty Child"] {
        let id = id(app, name);
        if app.object_previews.cache.contains_key(&id)
            || app.object_previews.models.contains_key(&id)
        {
            return Err(format!("INELIGIBLE_PREVIEW {name}"));
        }
    }
    Ok(())
}
fn attach(app: &mut App) {
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    // Only initialization geometry, exactly as App::initialize. All subsequent
    // scene/material updates belong to production poll or target-change update.
    app.renderer = Some(
        pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            SIZE,
            &app.themes[app.theme_index].vertices_for(&app.world),
        ))
        .unwrap(),
    );
}
fn frame(app: &App, output: &Path, stage: &str) -> Result<Vec<u8>> {
    let r = app.renderer.as_ref().ok_or("SETUP: renderer absent")?;
    let [w, h] = r.size;
    let texture = r.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("room tracer actual frame"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
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
        app.scroll,
    )?;
    let pitch = (w * 4).div_ceil(256) * 256;
    let buffer = r.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("room tracer readback"),
        size: (pitch * h) as u64,
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
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
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
    let pixels: Vec<u8> = mapped
        .chunks(pitch as usize)
        .flat_map(|row| row[..w as usize * 4].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    // Diagnostic-only exclusive PNG, explicitly NOT Renderer::capture generic
    // certification. No production writer/threshold changes or silent overwrite.
    let path = output.join(format!("{stage}.png"));
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&path)
        .unwrap();
    let mut encoder = png::Encoder::new(file, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&pixels).unwrap();
    writer.finish().unwrap();
    println!(
        "ROOM_FRAME path={} rgba_sha256={} generic_certification=NOT_RUN",
        path.display(),
        hash(&pixels)
    );
    Ok(pixels)
}
fn frame_witness(app: &App, pixels: &[u8]) -> Result<()> {
    use glam::{Mat4, Vec3};
    if pixels.len() != (SIZE[0] * SIZE[1] * 4) as usize {
        return Err("VIDEO_FRAME_LENGTH".into());
    }
    let a = &app.themes[app.theme_index]
        .host
        .anchors
        .iter()
        .find(|a| a.id == id(app, VIDEO))
        .unwrap();
    let t = app
        .object_previews
        .cache
        .get(&id(app, VIDEO))
        .ok_or("VIDEO_FRAME_SOURCE_MISSING")?;
    let plane = expected_plane(a, t, 1);
    let p = &app.pose;
    let eye = Vec3::new(p.position[0], 1.65, p.position[1]);
    let direction = Vec3::new(
        p.yaw.sin() * p.pitch.cos(),
        p.pitch.sin(),
        -p.yaw.cos() * p.pitch.cos(),
    );
    let m = Mat4::perspective_rh(
        65.0_f32.to_radians(),
        SIZE[0] as f32 / SIZE[1] as f32,
        0.05,
        50.,
    ) * Mat4::look_to_rh(eye, direction, Vec3::Y);
    let mut counts = [0usize; 4];
    for (i, (x, y)) in [(24., 18.), (110., 18.), (24., 70.), (110., 70.)]
        .into_iter()
        .enumerate()
    {
        let point = Vec3::new(
            plane[0].position[0] + (plane[1].position[0] - plane[0].position[0]) * (x + 0.5) / 160.,
            plane[0].position[1] + (plane[2].position[1] - plane[0].position[1]) * (y + 0.5) / 96.,
            plane[0].position[2],
        );
        let q = m * point.extend(1.);
        if q.w <= 0. {
            return Err("VIDEO_FRAME_BEHIND_CAMERA".into());
        }
        let q = q / q.w;
        let cx = ((q.x + 1.) * 0.5 * SIZE[0] as f32).floor() as i32;
        let cy = ((1. - q.y) * 0.5 * SIZE[1] as f32).floor() as i32;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x >= SIZE[0] as i32 || y >= SIZE[1] as i32 {
                    continue;
                }
                let pixel = &pixels[((y as u32 * SIZE[0] + x as u32) * 4) as usize..][..4];
                if pixel[3] == 255
                    && pixel[..3]
                        .iter()
                        .zip(COLORS[i])
                        .all(|(a, b)| a.abs_diff(b) <= 28)
                {
                    counts[i] += 1;
                }
            }
        }
        println!(
            "ROOM_FRAME_WITNESS quadrant={i} xy={cx},{cy} matching_patch_pixels={}",
            counts[i]
        );
    }
    if counts.iter().any(|n| *n < 5) {
        return Err(format!(
            "VIDEO_NORMAL_FRAME_QUADRANTS {counts:?}; classify occlusion/reference before repair"
        ));
    }
    Ok(())
}
fn stage(
    app: &mut App,
    room: &Path,
    output: &Path,
    name: &str,
    mixed: bool,
    errors: &mut Vec<String>,
) {
    aim(app, VIDEO);
    settle(app, name);
    ledger(app, name, usize::MAX);
    let cpu = check_video(app);
    println!("ROOM_VIDEO_RESULT stage={name} cpu={cpu:?}");
    if let Err(e) = cpu {
        errors.push(format!("{name}: {e}"));
    }
    if mixed && let Err(e) = peers(app, room) {
        println!("ROOM_PEER_RESULT {name}: {e}");
        errors.push(format!("{name}: {e}"));
    }
    if app.renderer.is_some() {
        match frame(app, output, name).and_then(|pixels| {
            let result = frame_witness(app, &pixels);
            assert!(
                frame_witness(app, &vec![0; pixels.len()]).is_err(),
                "blank-frame negative"
            );
            result
        }) {
            Ok(()) => (),
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    if app.object_previews.test_model_fallback_progress().1 {
        let before = app.object_previews.test_model_fallback_progress();
        for _ in 0..32 {
            tick(app);
        }
        assert_eq!(
            before,
            app.object_previews.test_model_fallback_progress(),
            "ordinary polls must not retry drained work"
        );
    }
}

#[test]
#[ignore = "parent-only exact packaged fixtures; real serial native/GPU, no audio output"]
fn packaged_mp4_complete_room_normal_publication_and_image_return() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let input = PathBuf::from(
        std::env::var_os("HAIR_MEDIA_ROOM_FIXTURE").expect("SETUP: exact seven-file room"),
    );
    let input = input.canonicalize().unwrap();
    let base =
        PathBuf::from(std::env::var_os("TMPDIR").expect("SETUP: explicit execution-owned TMPDIR"))
            .canonicalize()
            .unwrap();
    assert!(
        base.starts_with(root.parent().unwrap())
            && !base.starts_with(&root)
            && !root.starts_with(&base)
    );
    assert!(!base.starts_with(&input) && !input.starts_with(&base));
    let case = std::env::var("HAIR_ROOM_CASE").unwrap_or_else(|_| "mixed".into());
    let phase = std::env::var("HAIR_ROOM_PHASE").unwrap_or_else(|_| "all".into());
    let renderer = std::env::var("HAIR_ROOM_RENDERER").unwrap_or_else(|_| "attached".into());
    assert!(["mixed", "single"].contains(&case.as_str()));
    assert!(["all", "initial", "post-png", "post-jpeg"].contains(&phase.as_str()));
    assert!(["attached", "absent"].contains(&renderer.as_str()));
    assert!(
        case != "single" || phase == "initial",
        "single-MP4 comparison has no image lifecycle"
    );
    let mixed = case == "mixed";
    let output = base.join(format!("r{}", std::process::id()));
    fs::create_dir(&output).expect("SETUP: absent exclusive owned output");
    let room = output.join("room");
    fs::create_dir(&room).unwrap();
    let mut expected: Vec<OsString> = FILES.iter().map(|(n, _, _)| OsString::from(n)).collect();
    expected.push("Empty Child".into());
    expected.sort();
    assert_eq!(members(&input), expected);
    assert!(members(&input.join("Empty Child")).is_empty());
    let mut original = Vec::new();
    let mut copied = Vec::new();
    for (name, len, sha) in FILES {
        let path = input.join(name);
        let bytes = verified(&path, len, sha);
        let m = fs::symlink_metadata(&path).unwrap();
        original.push((name, stamp(&m)));
        if mixed || name == VIDEO {
            let target = room.join(name);
            exclusive(&target, &bytes);
            fs::set_permissions(&target, m.permissions()).unwrap();
            fs::File::options()
                .write(true)
                .open(&target)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(m.modified().unwrap()))
                .unwrap();
            let cm = fs::metadata(&target).unwrap();
            copied.push((name, stamp(&cm)));
            println!(
                "ROOM_SOURCE name={name:?} source={:?} copied={:?} sha256={sha}",
                stamp(&m),
                stamp(&cm)
            );
        }
    }
    fs::create_dir(room.join("Empty Child")).unwrap();
    let child_meta = fs::symlink_metadata(input.join("Empty Child")).unwrap();
    assert!(child_meta.is_dir() && !child_meta.file_type().is_symlink());
    fs::set_permissions(room.join("Empty Child"), child_meta.permissions()).unwrap();
    fs::File::open(room.join("Empty Child"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(child_meta.modified().unwrap()))
        .unwrap();
    let child_stamp = stamp(&fs::metadata(room.join("Empty Child")).unwrap());
    for (name, bytes) in [
        ("main", include_bytes!("main.rs").as_slice()),
        (
            "tracer",
            include_bytes!("local_media_room_preview_tests.rs").as_slice(),
        ),
        (
            "scheduler",
            include_bytes!("folder_native/object_preview.rs").as_slice(),
        ),
        (
            "native",
            include_bytes!("../native/video_still.m").as_slice(),
        ),
    ] {
        println!("ROOM_COMPILED_SOURCE {name} sha256={}", hash(bytes));
    }
    println!(
        "ROOM_SELECTION case={case} phase={phase} renderer={renderer} output={} surface=native-NOT-exercised audio=no-Play",
        output.display()
    );
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    if renderer == "attached" {
        attach(&mut app);
    }
    app.complete_folder(Ok(Some(room.clone()))).unwrap();
    assert_eq!(
        app.world.folder_view.as_ref().unwrap().bindings.len(),
        if mixed { 8 } else { 2 }
    );
    assert!(app.object_previews.cache.is_empty() && app.object_previews.models.is_empty());
    let vid = id(&app, VIDEO);
    let anchor = app.themes[0]
        .host
        .anchors
        .iter()
        .find(|a| a.id == vid)
        .unwrap();
    assert!(
        !contains_exact(&app.themes[0].vertices_for(&app.world), &placard(anchor)),
        "pending is NOT MP4-still success"
    );
    println!(
        "ROOM_CURRENT_BINDINGS {:?}",
        app.world.folder_view.as_ref().unwrap().bindings
    );
    let host = app.themes[0].host.clone();
    let mut errors = Vec::new();
    stage(&mut app, &room, &output, "before-e", mixed, &mut errors);
    if mixed && phase != "initial" {
        for (name, label) in [
            ("Quadrants.png", "post-png"),
            ("Quadrants.jpg", "post-jpeg"),
        ] {
            if phase != "all" && phase != label {
                continue;
            }
            aim(&mut app, name);
            tick(&mut app);
            assert_eq!(
                app.world.interaction.as_ref().unwrap().anchor,
                id(&app, name)
            );
            tap(&mut app, KeyCode::KeyE);
            assert!(app.world.reading);
            settle(&mut app, &format!("{label}-opened"));
            assert!(
                app.local_image_visible(),
                "SETUP: genuine opened image required"
            );
            tap(&mut app, KeyCode::KeyI);
            tick(&mut app);
            assert!(!app.local_image_visible());
            assert!(app.world.document().unwrap().contains(name));
            tap(&mut app, KeyCode::KeyI);
            tick(&mut app);
            assert!(app.local_image_visible());
            tap(&mut app, KeyCode::Escape);
            assert!(!app.world.reading);
            stage(&mut app, &room, &output, label, mixed, &mut errors);
        }
    }
    assert_eq!(
        app.themes[0].host, host,
        "host anchors/collision/layout preserved"
    );
    // Oracle negatives are local values ONLY, never App/cache/material input.
    let wrong = Texture {
        width: 160,
        height: 96,
        rgba: [97, 31, 173, 255].repeat(160 * 96),
    };
    assert!(
        first_frame(&wrong).is_err(),
        "uniform-purple fallback cannot satisfy MP4 source oracle"
    );
    let halves = Texture {
        width: 160,
        height: 96,
        rgba: (0..96)
            .flat_map(|y| {
                (0..160).flat_map(move |x| {
                    let c = COLORS[usize::from(x >= 80) + 2 * usize::from(y >= 48)];
                    [c[0], c[1], c[2], 255]
                })
            })
            .collect(),
    };
    assert!(
        first_frame(&halves).is_err(),
        "wrong split cannot satisfy source landmarks"
    );
    drop(app); // owned worker retirement, not a universal native-drain assertion
    for (name, before) in original {
        let (_, len, sha) = FILES.iter().find(|(n, _, _)| *n == name).unwrap();
        verified(&input.join(name), *len, sha);
        assert_eq!(stamp(&fs::metadata(input.join(name)).unwrap()), before);
    }
    for (name, before) in copied {
        let (_, len, sha) = FILES.iter().find(|(n, _, _)| *n == name).unwrap();
        verified(&room.join(name), *len, sha);
        assert_eq!(stamp(&fs::metadata(room.join(name)).unwrap()), before);
    }
    assert_eq!(
        stamp(&fs::metadata(room.join("Empty Child")).unwrap()),
        child_stamp
    );
    assert_eq!(
        stamp(&fs::metadata(input.join("Empty Child")).unwrap()),
        stamp(&child_meta)
    );
    assert!(members(&room.join("Empty Child")).is_empty());
    assert_eq!(members(&input), expected);
    exclusive(&output.join("RESULT.txt"),format!("sourceWritesFinished=true\nexecution=actual test reached final assertion\ncase={case} phase={phase} renderer={renderer}\nerrors={errors:#?}\nownerFailureResolved=false\n").as_bytes());
    assert!(
        errors.is_empty(),
        "MEDIA_ROOM_MP4_TRACER_RED {errors:#?}; evidence retained at {}",
        output.display()
    );
}
