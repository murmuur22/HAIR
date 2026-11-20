//! Private binary-test-only fixed door probe. Never compiled into either shipped executable.
use glam::Vec3;
use hair::{Pose, Result, World, assets::Vertex, render::Renderer, resources::SmokeOutput};
use std::{path::Path, time::Duration};
const W: u32 = 960;
const H: u32 = 640;
fn pose() -> Pose {
    Pose {
        position: [0.0, -3.0],
        yaw: 0.0,
        pitch: -0.1,
    }
}
fn fixture() -> (hair::theme::Theme, World) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let theme = hair::theme::Theme::load(
        &root.join("packs/office.toml"),
        &mut hair::theme::AssetCache::default(),
    )
    .unwrap();
    let mut world = World::load(root).unwrap();
    world.interaction = hair::interaction::resolve(&theme.host, &pose(), &world);
    (theme, world)
}
// Readback only: no output/writer access and no way to certify a frame here.
fn readback(
    renderer: &Renderer,
    camera: &Pose,
    world: Option<&World>,
    clear: Option<wgpu::Color>,
) -> Result<Vec<u8>> {
    if renderer.size != [W, H] {
        return Err("door probe dimensions".into());
    }
    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fixed door contract readback"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    if let Some(color) = clear {
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        renderer.queue.submit([encoder.finish()]);
    } else if let Some(world) = world {
        renderer.draw(&view, camera, world, 0)?;
    } else {
        renderer.draw_preview(&view, camera)?;
    }
    let pitch = (W * 4).div_ceil(256) * 256;
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("door contract pixels"),
        size: (pitch * H) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
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
                rows_per_image: Some(H),
            },
        },
        wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    renderer.wait()?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| e.to_string())?;
    let pixels = mapped
        .chunks(pitch as usize)
        .flat_map(|r| r[..(W * 4) as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}
fn srgb(c: f32) -> u8 {
    let value = if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (value * 255.0).round() as u8
}
fn rgb(c: [f32; 3]) -> [u8; 3] {
    c.map(srgb)
}
fn close(actual: &[u8], expected: [u8; 3]) -> bool {
    actual
        .iter()
        .take(3)
        .zip(expected)
        .all(|(a, b)| a.abs_diff(b) <= 1)
}
// Independent source-triangle ray oracle for the one fixed camera/projection.
fn source_ray(vertices: &[Vertex], x: u32, y: u32, camera: &Pose) -> ([u8; 3], bool, bool) {
    let tan = (65.0_f32.to_radians() * 0.5).tan();
    let vx = (2.0 * (x as f32 + 0.5) / W as f32 - 1.0) * tan * W as f32 / H as f32;
    let vy = (1.0 - 2.0 * (y as f32 + 0.5) / H as f32) * tan;
    let right = Vec3::new(camera.yaw.cos(), 0.0, camera.yaw.sin());
    let forward = Vec3::new(
        camera.yaw.sin() * camera.pitch.cos(),
        camera.pitch.sin(),
        -camera.yaw.cos() * camera.pitch.cos(),
    );
    let direction = right * vx + right.cross(forward) * vy + forward;
    let eye = Vec3::new(camera.position[0], 1.65, camera.position[1]);
    let mut distance = f32::INFINITY;
    let mut color = [0.04, 0.06, 0.05];
    let mut marker = false;
    for triangle in vertices.as_chunks::<3>().0 {
        let [a, b, c]: [Vec3; 3] = std::array::from_fn(|i| Vec3::from_array(triangle[i].position));
        let e1 = b - a;
        let e2 = c - a;
        let p = direction.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-9 {
            continue;
        }
        let t = eye - a;
        let u = t.dot(p) / det;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = t.cross(e1);
        let v = direction.dot(q) / det;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let d = e2.dot(q) / det;
        if d < 0.05 || d >= distance {
            continue;
        }
        distance = d;
        color = std::array::from_fn(|i| {
            (1.0 - u - v) * triangle[0].color[i]
                + u * triangle[1].color[i]
                + v * triangle[2].color[i]
        });
        marker = triangle.iter().all(|v| v.color == [0.08, 0.6, 0.18]);
    }
    (rgb(color), distance.is_finite(), marker)
}
fn qualify(pixels: &[u8], camera: &Pose, world: &World) -> Result<()> {
    let fixed = pose();
    let (theme, expected_world) = fixture();
    if camera.position != fixed.position
        || camera.yaw != fixed.yaw
        || camera.pitch != fixed.pitch
        || world.current != "welcome"
        || world.reading
        || world.selected != 0
        || world.site.is_some()
        || world.interaction != expected_world.interaction
    {
        return Err("fixed door contract pose/page/target/prompt mismatch".into());
    }
    if pixels.len() != (W * H * 4) as usize || pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
    {
        return Err("door readback dimensions/opaque bounds".into());
    }
    let vertices = theme.vertices_for(&expected_world);
    if vertices.iter().any(|v| v.texture != 0) {
        return Err("fixed untextured door fixture required".into());
    }
    let (mut hits, mut markers, mut rays) = (0, 0, 0);
    for y in [70, 120, 220, 300, 400, 500, 590] {
        for x in [30, 160, 300, 480, 660, 800, 930] {
            let (expected, hit, marker) = source_ray(&vertices, x, y, camera);
            let index = ((y * W + x) * 4) as usize;
            if !close(&pixels[index..index + 3], expected) {
                return Err(format!("door geometry/marker ray mismatch at {x},{y}"));
            }
            rays += 1;
            hits += usize::from(hit);
            markers += usize::from(marker);
        }
    }
    if hits < 8 || markers == 0 {
        return Err("source rays must cover visible geometry and host marker".into());
    }
    let clear = rgb([0.04, 0.06, 0.05]);
    let nonblank = (64..610)
        .filter(|y| !(330..360).contains(y))
        .flat_map(|y| (0..W).map(move |x| ((y * W + x) * 4) as usize))
        .filter(|i| !close(&pixels[*i..*i + 3], clear))
        .count();
    if nonblank < 10000 {
        return Err("door nonblank scene-area coverage".into());
    }
    // Exact expected bitmap foreground AND background; not merely any green HUD pixels.
    let prompt = "E - Enter workshop";
    if expected_world
        .interaction
        .as_ref()
        .map(|t| t.prompt.as_str())
        != Some(prompt)
    {
        return Err("fixed fixture no longer supplies the expected Enter workshop prompt".into());
    }
    let x0 = (W as i32 / 2 - prompt.len() as i32 * 6) - 6;
    let x1 = x0 + prompt.len() as i32 * 12 + 12;
    let (y0, y1) = (334, 356);
    let mut expected = vec![rgb([0.015, 0.04, 0.025]); ((x1 - x0) * (y1 - y0)) as usize];
    let mut foreground = 0;
    for quad in hair::text::overlay(&expected_world, W, H, 0)
        .as_chunks::<6>()
        .0
    {
        if quad[0].color != [0.6, 1.0, 0.7] {
            continue;
        }
        let xs: Vec<i32> = quad
            .iter()
            .map(|v| ((v.position[0] + 1.0) * 0.5 * W as f32).round() as i32)
            .collect();
        let ys: Vec<i32> = quad
            .iter()
            .map(|v| ((1.0 - v.position[1]) * 0.5 * H as f32).round() as i32)
            .collect();
        for y in (*ys.iter().min().unwrap()).max(y0)..(*ys.iter().max().unwrap()).min(y1) {
            for x in (*xs.iter().min().unwrap()).max(x0)..(*xs.iter().max().unwrap()).min(x1) {
                expected[((y - y0) * (x1 - x0) + x - x0) as usize] = rgb(quad[0].color);
                foreground += 1;
            }
        }
    }
    if foreground < 800 {
        return Err("expected prompt glyph coverage missing".into());
    }
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y as u32 * W + x as u32) * 4) as usize;
            if !close(
                &pixels[i..i + 3],
                expected[((y - y0) * (x1 - x0) + x - x0) as usize],
            ) {
                return Err(format!("door expected prompt raster mismatch at {x},{y}"));
            }
        }
    }
    println!(
        "DOOR_SEMANTIC_OK rays={rays} geometry_hits={hits} marker_hits={markers} prompt_foreground={foreground} nonblank_scene_pixels={nonblank} dimensions={W}x{H} opaque=true"
    );
    Ok(())
}
// Only this fixed semantic contract can reach the protected writer; no skip switch/name parameter.
pub(super) fn capture_door(
    renderer: &Renderer,
    output: &SmokeOutput,
    camera: &Pose,
    world: &World,
) -> Result<()> {
    let pixels = readback(renderer, camera, Some(world), None)?;
    qualify(&pixels, camera, world)?;
    write_qualified(output, &pixels)?;
    println!(
        "SEMANTIC_DOOR_CAPTURE {} actual source-ray/marker/prompt contract",
        output.path().join("reader-workshop.png").display()
    );
    Ok(())
}
fn write_qualified(output: &SmokeOutput, pixels: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut writer = std::io::BufWriter::new(output.create_capture("reader-workshop.png")?);
    let mut png = png::Encoder::new(&mut writer, W, H);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    let mut png = png.write_header().map_err(|e| e.to_string())?;
    png.write_image_data(pixels).map_err(|e| e.to_string())?;
    png.finish().map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    Ok(())
}
fn arrival_fixture() -> (hair::theme::Theme, World) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let theme = hair::theme::Theme::load(
        &root.join("packs/archive.toml"),
        &mut hair::theme::AssetCache::default(),
    )
    .unwrap();
    let mut world = World::load(root).unwrap();
    world.current = "workshop".into();
    (theme, world)
}
fn qualify_arrival(pixels: &[u8], camera: &Pose, world: &World) -> Result<()> {
    let fixed = Pose::default();
    let (theme, expected_world) = arrival_fixture();
    if camera.position != fixed.position
        || camera.yaw != fixed.yaw
        || camera.pitch != fixed.pitch
        || world.current != "workshop"
        || world.reading
        || world.selected != 0
        || world.site.is_some()
        || world.interaction.is_some()
    {
        return Err("fixed archive arrival pose/page/state mismatch".into());
    }
    if pixels.len() != (W * H * 4) as usize || pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
    {
        return Err("archive dimensions/opacity".into());
    }
    let vertices = theme.vertices_for(&expected_world);
    if vertices.iter().any(|v| v.texture != 0) {
        return Err("fixed untextured archive required".into());
    }
    let mut hits = 0;
    for y in [70, 120, 220, 300, 400, 500, 590] {
        for x in [30, 160, 300, 480, 660, 800, 930] {
            let (expected, hit, _) = source_ray(&vertices, x, y, camera);
            let i = ((y * W + x) * 4) as usize;
            if !close(&pixels[i..i + 3], expected) {
                return Err(format!("archive source ray mismatch {x},{y}"));
            }
            hits += usize::from(hit);
        }
    }
    if hits < 8 {
        return Err("archive visible source coverage".into());
    }
    let clear = rgb([0.04, 0.06, 0.05]);
    let nonblank = (64..610)
        .flat_map(|y| (0..W).map(move |x| ((y * W + x) * 4) as usize))
        .filter(|i| !close(&pixels[*i..*i + 3], clear))
        .count();
    if nonblank < 10000 {
        return Err("archive nonblank scene area".into());
    }
    // Compare the complete expected current-page/help header raster, including background.
    let mut header = vec![rgb([0.015, 0.04, 0.025]); (W * 52) as usize];
    for quad in hair::text::overlay(&expected_world, W, H, 0)
        .as_chunks::<6>()
        .0
    {
        let xs: Vec<i32> = quad
            .iter()
            .map(|v| ((v.position[0] + 1.0) * 0.5 * W as f32).round() as i32)
            .collect();
        let ys: Vec<i32> = quad
            .iter()
            .map(|v| ((1.0 - v.position[1]) * 0.5 * H as f32).round() as i32)
            .collect();
        for y in (*ys.iter().min().unwrap()).max(0)..(*ys.iter().max().unwrap()).min(52) {
            for x in (*xs.iter().min().unwrap()).max(0)..(*xs.iter().max().unwrap()).min(W as i32) {
                header[(y * W as i32 + x) as usize] = rgb(quad[0].color);
            }
        }
    }
    for (i, expected) in header.into_iter().enumerate() {
        if !close(&pixels[i * 4..i * 4 + 3], expected) {
            return Err("archive expected Workshop header raster mismatch".into());
        }
    }
    println!(
        "ARCHIVE_SEMANTIC_OK rays=49 geometry_hits={hits} header_pixels={} nonblank_scene_pixels={nonblank} dimensions={W}x{H} opaque=true",
        W * 52
    );
    Ok(())
}
pub(super) fn capture_archive_arrival(
    renderer: &Renderer,
    output: &SmokeOutput,
    camera: &Pose,
    world: &World,
) -> Result<()> {
    let pixels = readback(renderer, camera, Some(world), None)?;
    qualify_arrival(&pixels, camera, world)?;
    write_qualified(output, &pixels)?;
    println!(
        "SEMANTIC_ARCHIVE_CAPTURE {}",
        output.path().join("reader-workshop.png").display()
    );
    Ok(())
}
#[test]
fn bundled_instruction_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut world = World::load(root).unwrap();
    assert!(
        world
            .document()
            .unwrap()
            .contains("Walk to the green doorway and press E.")
    );
    world.act(hair::Action::Navigate, [0.0, -3.0]);
    assert!(
        world
            .document()
            .unwrap()
            .contains("The doorway returns to Welcome with E.")
    );
}

#[test]
#[ignore = "serialized actual Metal semantic contract positive and counterfactuals"]
fn semantic_door_oracle() {
    let (theme, mut world) = fixture();
    let instance = hair::render::instance();
    let adapter = pollster::block_on(hair::render::adapter(&instance, None)).unwrap();
    let mut renderer = pollster::block_on(Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [W, H],
        &theme.vertices_for(&world),
    ))
    .unwrap();
    renderer.set_materials(&theme.textures).unwrap();
    let pixels = readback(&renderer, &pose(), Some(&world), None).unwrap();
    qualify(&pixels, &pose(), &world).expect("exact legitimate sparse door view must qualify");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path =
        std::env::var_os("HAIR_CONTRACT_CAPTURE_DIR").expect("explicit separate fresh output");
    let output = hair::resources::smoke_output(root, Path::new(&path)).unwrap();
    let error = renderer
        .capture(&output, "reader-workshop.png", &pose(), &world)
        .unwrap_err();
    assert!(
        // Slice3's black/white outline contributes three additional actual colors.
        // The generic 20-color scene guard still rejects this exact sparse view.
        error.contains("GPU image suspiciously flat: 18 colors"),
        "{error}"
    );
    assert!(!output.path().join("reader-workshop.png").exists());
    println!("GENERIC_GUARD_NEGATIVE_PASS unchanged exact sparse view still refuses before write");
    let black = readback(&renderer, &pose(), None, Some(wgpu::Color::BLACK)).unwrap();
    assert!(
        qualify(&black, &pose(), &world).is_err(),
        "uniform actual black GPU frame"
    );
    // Renderer requires a nonempty vertex buffer. Clip actual source triangles behind
    // the fixed camera for clear/HUD-only counterfactuals, without changing camera.
    let hidden: Vec<Vertex> = theme
        .vertices
        .iter()
        .take(3)
        .map(|v| Vertex {
            position: [v.position[0], v.position[1], v.position[2] + 100.0],
            ..*v
        })
        .collect();
    renderer.set_scene(&hidden);
    let clear = readback(&renderer, &pose(), None, None).unwrap();
    assert!(
        qualify(&clear, &pose(), &world).is_err(),
        "uniform actual clear GPU frame"
    );
    let hud_only = readback(&renderer, &pose(), Some(&world), None).unwrap();
    assert!(
        qualify(&hud_only, &pose(), &world).is_err(),
        "HUD-only actual frame"
    );
    let no_marker: Vec<Vertex> = theme
        .vertices_for(&world)
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|t| !t.iter().all(|v| v.color == [0.08, 0.6, 0.18]))
        .flatten()
        .copied()
        .collect();
    renderer.set_scene(&no_marker);
    let frame = readback(&renderer, &pose(), Some(&world), None).unwrap();
    assert!(
        qualify(&frame, &pose(), &world).is_err(),
        "missing actual marker"
    );
    renderer.set_scene(&theme.vertices_for(&world));
    world.interaction.as_mut().unwrap().prompt = "E - Enter nowhere".into();
    let wrong_prompt = readback(&renderer, &pose(), Some(&world), None).unwrap();
    let (_, valid) = fixture();
    assert!(
        qualify(&wrong_prompt, &pose(), &valid).is_err(),
        "incorrect actual prompt pixels"
    );
    world.interaction = None;
    let absent_prompt = readback(&renderer, &pose(), Some(&world), None).unwrap();
    assert!(
        qualify(&absent_prompt, &pose(), &valid).is_err(),
        "missing actual prompt"
    );
    let wrong = Pose { yaw: 0.2, ..pose() };
    let frame = readback(&renderer, &wrong, Some(&valid), None).unwrap();
    assert!(
        qualify(&frame, &pose(), &valid).is_err(),
        "actual camera/scene mismatch"
    );
    assert!(
        qualify(&pixels, &wrong, &valid).is_err(),
        "mismatched declared pose"
    );
    assert!(
        qualify(&pixels[..pixels.len() - 1], &pose(), &valid).is_err(),
        "truncated actual readback"
    );
    let mut alpha = pixels.clone();
    alpha[3] = 0;
    assert!(
        qualify(&alpha, &pose(), &valid).is_err(),
        "nonopaque actual readback corruption"
    );
    capture_door(&renderer, &output, &pose(), &valid).unwrap();
    let (archive, arrival) = arrival_fixture();
    let camera = Pose::default();
    renderer.set_materials(&archive.textures).unwrap();
    renderer.set_scene(&archive.vertices_for(&arrival));
    let frame = readback(&renderer, &camera, Some(&arrival), None).unwrap();
    qualify_arrival(&frame, &camera, &arrival).expect("exact sparse archive arrival must qualify");
    let arrival_path = Path::new(&path).join("arrival");
    let arrival_output = hair::resources::smoke_output(root, &arrival_path).unwrap();
    // Slice3 HUD makes this arrival 22 colors, above the unchanged20-color
    // generic scene threshold. Assert the exact new palette and both consumers.
    let colors: std::collections::HashSet<_> = frame
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    assert_eq!(colors.len(), 22, "explicit new HUD arrival palette");
    let generic_path = Path::new(&path).join("arrival-generic");
    std::fs::create_dir(&generic_path).unwrap();
    let generic_output = hair::resources::smoke_output(root, &generic_path).unwrap();
    renderer
        .capture(&generic_output, "reader-workshop.png", &camera, &arrival)
        .unwrap();
    assert!(generic_output.path().join("reader-workshop.png").is_file());
    assert!(!arrival_output.path().join("reader-workshop.png").exists());
    renderer.set_scene(&hidden);
    let hud = readback(&renderer, &camera, Some(&arrival), None).unwrap();
    assert!(
        qualify_arrival(&hud, &camera, &arrival).is_err(),
        "archive HUD-only actual frame"
    );
    renderer.set_scene(&archive.vertices_for(&arrival));
    let wrong_page = World::load(root).unwrap();
    let frame = readback(&renderer, &camera, Some(&wrong_page), None).unwrap();
    assert!(
        qualify_arrival(&frame, &camera, &arrival).is_err(),
        "archive incorrect actual page header"
    );
    assert!(
        qualify_arrival(&pixels, &pose(), &arrival).is_err(),
        "archive mismatched pose"
    );
    capture_archive_arrival(&renderer, &arrival_output, &camera, &arrival).unwrap();
    println!(
        "ARCHIVE_ORACLE_PASS positive fixed reskin; negative actual HUD-only/wrong-header/pose; explicit22-color new HUD generic+semantic acceptance; generic20-color threshold unchanged"
    );
    println!(
        "SEMANTIC_ORACLE_PASS positive exact sparse view; negative black/clear/HUD-only/missing-marker/wrong-prompt/missing-prompt/actual-pose/declared-pose/truncated/alpha"
    );
}
