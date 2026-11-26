//! Real offscreen renderer witness driven by App Space/update, not OS input.
use super::*;

fn capture(a: &mut App, root: &std::path::Path, name: &str) -> Vec<u8> {
    let dir = root.join(name);
    std::fs::create_dir(&dir).unwrap();
    let output = resources::smoke_output(&a.root, &dir).unwrap();
    a.update_hud();
    let renderer = a.renderer.as_ref().unwrap();
    renderer
        .capture(&output, "office.png", &a.pose, &a.world)
        .unwrap();
    renderer.wait().unwrap();
    let mut reader = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(dir.join("office.png")).unwrap(),
    ))
    .read_info()
    .unwrap();
    let mut out = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut out).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    out.truncate(info.buffer_size());
    out
}

#[test]
#[ignore = "explicit serialized Metal witness; requires fresh HAIR_R4_CAPTURE_ROOT under scratch"]
fn app_space_camera_matches_translated_world_and_landing_restores_view() {
    let root = PathBuf::from(
        std::env::var_os("HAIR_R4_CAPTURE_ROOT").expect("explicit scratch capture root"),
    );
    assert!(root.starts_with(std::env::temp_dir()));
    std::fs::create_dir(&root).unwrap();
    let mut a = app();
    a.pose.position[0] += 0.25;
    let scene = a.themes[0].vertices_for(&a.world);
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    println!("R4_ADAPTER {:?}", adapter.get_info());
    let mut renderer = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [960, 640],
        &scene,
    ))
    .unwrap();
    renderer.set_theme_appearance(&a.themes[0]).unwrap();
    a.renderer = Some(renderer);
    let floor = capture(&mut a, &root, "floor");
    a.input.captured = true;
    key(&mut a, true, false);
    frames(&mut a, 24);
    let height = a.pose.feet_y;
    assert!(height > 0.9);
    let air = capture(&mut a, &root, "air");
    let changed = floor
        .as_chunks::<4>().0.iter()
        .zip(air.as_chunks::<4>().0.iter())
        .enumerate()
        .filter(|(i, (a, b))| (100..500).contains(&(i / 960)) && a != b)
        .count();
    assert!(
        changed > 1000,
        "world view must really change with App jump: {changed}"
    );
    let translated = scene
        .iter()
        .map(|v| {
            let mut v = *v;
            v.position[1] -= height;
            v
        })
        .collect::<Vec<_>>();
    a.renderer.as_mut().unwrap().set_scene(&translated);
    a.pose.feet_y = 0.0;
    let oracle = capture(&mut a, &root, "translated-world-oracle");
    let mismatches = air
        .as_chunks::<4>().0.iter()
        .zip(oracle.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 2))
        .count();
    // Only floating-point raster boundaries may differ, never a camera-frame shift.
    assert!(
        mismatches < 200,
        "raised camera must equal inverse world translation: {mismatches}"
    );
    a.pose.feet_y = height;
    a.renderer.as_mut().unwrap().set_scene(&scene);
    frames(&mut a, 240);
    assert!(a.player.grounded && a.pose.feet_y == 0.0);
    let landed = capture(&mut a, &root, "landed");
    assert_eq!(
        floor, landed,
        "landing restores exact initial renderer view; held Space does not bounce"
    );
    println!(
        "R4_OFFSCREEN_METAL height={height} changed_world_samples={changed} translated_oracle_mismatches={mismatches}; actual App adapters, synthetic input, NOT native/comfort acceptance"
    );
}
