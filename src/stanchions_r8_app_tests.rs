use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(
            temp,
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("tmp")
                .canonicalize()
                .unwrap()
        );
        let f = Self(temp.join(format!("r8-stanchions-{}", std::process::id())));
        std::fs::create_dir(&f.0).unwrap();
        std::fs::create_dir(f.0.join("Child")).unwrap();
        std::fs::write(f.0.join("Note.txt"), b"R8 preserved file bytes").unwrap();
        f
    }
    fn app(&self) -> App {
        let mut a = app();
        a.complete_folder(Ok(Some(self.0.clone()))).unwrap();
        a
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn has_boundary(a: &App) -> bool {
    let t = &a.themes[a.theme_index];
    t.vertices.iter().any(|v| {
        (v.position[0] - t.host.room[2]).abs() < 0.12 && (0.6..1.05).contains(&v.position[1])
    })
}
#[test]
fn r8_actual_folder_app_edges_hold_airborne_with_and_without_cosmetics() {
    let f = Fixture::new();
    for cosmetics in [true, false] {
        for (code, axis, bound) in [
            (KeyCode::KeyA, 0, 0),
            (KeyCode::KeyD, 0, 2),
            (KeyCode::KeyW, 1, 1),
            (KeyCode::KeyS, 1, 3),
        ] {
            let mut a = f.app();
            assert!(
                has_boundary(&a),
                "actual generated App platform must show boundary stanchions"
            );
            let host = a.themes[0].host.clone();
            let file_before = bytemuck::cast_slice::<_, u8>(&a.themes[0].vertices).to_vec();
            if !cosmetics {
                a.themes[0].vertices.clear();
            }
            a.pose.position = [0.0; 2];
            a.pose.position[axis] = host.room[bound] + if bound < 2 { 0.01 } else { -0.01 };
            a.input.captured = true;
            key(&mut a, true, false);
            a.keyboard_input(Some(code), true, false, None).unwrap();
            a.advance_motion(0.25);
            assert!(a.pose.feet_y > 0.5);
            assert!(
                a.pose.position[axis] >= host.room[axis]
                    && a.pose.position[axis] <= host.room[axis + 2]
            );
            a.keyboard_input(Some(code), false, false, None).unwrap();
            frames(&mut a, 240);
            assert_eq!(a.pose.feet_y, 0.0, "ropes are not support surfaces");
            assert_eq!(a.themes[0].host, host);
            if cosmetics {
                assert_eq!(
                    bytemuck::cast_slice::<_, u8>(&a.themes[0].vertices),
                    file_before.as_slice()
                );
            }
        }
    }
    assert_eq!(
        std::fs::read(f.0.join("Note.txt")).unwrap(),
        b"R8 preserved file bytes"
    );
    assert!(!f.0.join(".hair-layout").exists());
}
#[test]
fn r8_actual_app_walks_door_approach_and_e_travels_then_parent_returns() {
    let f = Fixture::new();
    let mut a = f.app();
    let id = a
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == "Child")
        .unwrap()
        .0
        .clone();
    let host = &a.themes[0].host;
    let door = host.anchors.iter().find(|x| x.id == id).unwrap();
    let n = host.portal_inward(&id);
    a.pose = Pose {
        position: [door.at[0] + 1.3 * n[0], door.at[2] + 1.3 * n[1]],
        yaw: (-n[0]).atan2(n[1]),
        pitch: ((door.at[1] + door.size[1] * 0.5 - 1.65) / 1.05).atan(),
        ..Default::default()
    };
    a.input.captured = true;
    let start = a.pose.position;
    a.keyboard_input(Some(KeyCode::KeyW), true, false, None)
        .unwrap();
    a.advance_motion(0.1);
    a.keyboard_input(Some(KeyCode::KeyW), false, false, None)
        .unwrap();
    assert!(
        (a.pose.position[0] - start[0]).hypot(a.pose.position[1] - start[1]) > 0.24,
        "door approach stays usable"
    );
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    assert_eq!(a.world.interaction.as_ref().unwrap().anchor, id);
    tap(&mut a, KeyCode::KeyE);
    assert_eq!(
        a.local.as_ref().unwrap().explorer.current().folder,
        f.0.join("Child")
    );
    tap(&mut a, KeyCode::KeyU);
    assert_eq!(a.local.as_ref().unwrap().explorer.current().folder, f.0);
    assert_eq!(a.pose.feet_y, 0.0);
}

pub(super) fn capture(a: &mut App, root: &Path, name: &str) {
    let dir = root.join(name);
    std::fs::create_dir(&dir).unwrap();
    let output = resources::smoke_output(&a.root, &dir).unwrap();
    a.update_hud();
    let r = a.renderer.as_ref().unwrap();
    // Diagnostic readback, NOT a production smoke certificate. Sparse edge-only
    // views legitimately fail capture()'s unchanged 20-colour diversity guard.
    let [width, height] = r.size;
    let texture = r.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("R8 diagnostic visitor capture"),
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
        &a.pose,
        &a.world,
        0,
    )
    .unwrap();
    let pitch = (width * 4).div_ceil(256) * 256;
    let buffer = r.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("R8 diagnostic readback"),
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
        .map_async(wgpu::MapMode::Read, move |value| {
            let _ = tx.send(value);
        });
    r.wait().unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let pixels = mapped
        .chunks(pitch as usize)
        .flat_map(|row| row[..width as usize * 4].iter().copied())
        .collect::<Vec<_>>();
    let colors = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert!(colors > 2, "nonuniform diagnostic world frame");
    assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    drop(mapped);
    buffer.unmap();
    let mut file = output.create_capture("office.png").unwrap();
    let mut png = png::Encoder::new(&mut file, width, height);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    let mut png = png.write_header().unwrap();
    png.write_image_data(&pixels).unwrap();
    png.finish().unwrap();
    println!(
        "R8_DIAGNOSTIC_ONLY colors={colors} size={width}x{height}; not production smoke/visual acceptance"
    );
    println!(
        "R8_CAPTURE {} pose={:?}",
        dir.join("office.png").display(),
        a.pose
    );
}
#[test]
#[ignore = "explicit serialized real Metal capture, fresh HAIR_R8_CAPTURE_ROOT required"]
fn r8_metal_ordinary_visitor_platform_and_closed_room_captures() {
    let root = PathBuf::from(std::env::var_os("HAIR_R8_CAPTURE_ROOT").unwrap());
    assert!(root.starts_with(std::env::temp_dir()));
    std::fs::create_dir(&root).unwrap();
    let f = Fixture::new();
    let mut a = f.app();
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    println!("R8_ADAPTER {:?}", adapter.get_info());
    for pack in ["office", "archive"] {
        let host = a.themes[0].host.clone();
        a.themes[0] = hair::theme::Theme::load_for_folder_scene(
            &a.root.join(format!("packs/{pack}.toml")),
            &mut Default::default(),
            &host,
        )
        .unwrap();
        assert!(has_boundary(&a));
        let scene = a.themes[0].vertices_for(&a.world);
        let mut r = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [1280, 800],
            &scene,
        ))
        .unwrap();
        r.set_theme_appearance(&a.themes[0]).unwrap();
        a.renderer = Some(r);
        a.pose = Pose {
            position: [0.0, host.room[1] + 4.0],
            pitch: -0.12,
            ..Default::default()
        };
        capture(&mut a, &root, &format!("{pack}-front-travel-gap"));
        a.pose = Pose {
            position: [host.room[2] - 3.0, 0.0],
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: -0.12,
            ..Default::default()
        };
        capture(&mut a, &root, &format!("{pack}-side-edge"));
    }
    let mut a = app();
    a.pose = Pose {
        position: [1.4, -0.8],
        pitch: -0.10,
        ..Default::default()
    };
    let mut r = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [1280, 800],
        &a.themes[0].vertices_for(&a.world),
    ))
    .unwrap();
    r.set_theme_appearance(&a.themes[0]).unwrap();
    a.renderer = Some(r);
    capture(&mut a, &root, "closed-room-open-front");
}
