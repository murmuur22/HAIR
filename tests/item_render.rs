//! Explicit Metal readback of item-local texture routing. Synthetic inputs, actual GPU output.
use hair::assets::{Texture, Vertex};
use std::{path::PathBuf, sync::Arc};
#[test]
#[ignore = "serialized Metal; explicit absent HAIR_ITEM_CAPTURE_ROOT under owned scratch"]
fn item_overlay_alpha_and_local_texture_namespace_match_source_pixels() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = PathBuf::from(
        std::env::var_os("HAIR_ITEM_CAPTURE_ROOT").expect("explicit absent owned output"),
    );
    assert!(output.starts_with(std::env::temp_dir()));
    std::fs::create_dir(&output).unwrap();
    let instance = hair::render::instance();
    let adapter = pollster::block_on(hair::render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let world = hair::World::load(&root).unwrap();
    let pose = hair::Pose::default();
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    for (label, rgba, expected) in [
        ("transparent", [255, 0, 255, 0], [44, 44, 44]),
        ("opaque", [97, 31, 173, 255], [97, 31, 173]),
    ] {
        let vertex = |x, y, uv| Vertex {
            position: [x, y, 0.0],
            color: [1.0; 3],
            uv,
            texture: 0x8000_0001,
        };
        let (a, b, c, d) = (
            vertex(-0.5, 0.5, [0.0, 0.0]),
            vertex(0.5, 0.5, [1.0, 0.0]),
            vertex(0.5, -0.5, [1.0, 1.0]),
            vertex(-0.5, -0.5, [0.0, 1.0]),
        );
        for collection in [false, true] {
            let mut renderer = pollster::block_on(hair::render::Renderer::new(
                &adapter,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                [640, 480],
                &theme.vertices,
            ))
            .unwrap();
            renderer.set_theme_appearance(&theme).unwrap();
            let texture = Arc::new(Texture {
                width: 1,
                height: 1,
                rgba: rgba.to_vec(),
            });
            renderer.set_item_materials(&[texture]).unwrap();
            let quad = vec![a, b, c, a, c, d];
            if collection {
                renderer.collection_overlay = quad;
            } else {
                renderer.inventory_overlay = Some(quad);
            }
            renderer.sidebar_enabled = false;
            let directory = output.join(format!(
                "{label}-{}",
                if collection {
                    "collection"
                } else {
                    "inventory"
                }
            ));
            std::fs::create_dir(&directory).unwrap();
            let target = hair::resources::smoke_output(&root, &directory).unwrap();
            renderer
                .capture(&target, "office.png", &pose, &world)
                .unwrap();
            renderer.wait().unwrap();
            let file = std::fs::File::open(directory.join("office.png")).unwrap();
            let mut reader = png::Decoder::new(std::io::BufReader::new(file))
                .read_info()
                .unwrap();
            let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut pixels).unwrap();
            assert_eq!((info.width, info.height), (640, 480));
            let channels = info.color_type.samples();
            let at = (240 * 640 + 320) * channels;
            let actual = &pixels[at..at + 3];
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
                "{label} collection={collection} sampled={actual:?} expected={expected:?}"
            );
        }
    }
    // Both opaque surfaces cover the centre. The farther one is deliberately
    // submitted LAST, so painter ordering is not a sufficient depth witness.
    let mut renderer = pollster::block_on(hair::render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [640, 480],
        &theme.vertices,
    ))
    .unwrap();
    renderer.set_theme_appearance(&theme).unwrap();
    let mut mesh = vec![];
    for (depth, color) in [(0.2, [1.0, 0.0, 0.0]), (0.8, [0.0, 1.0, 0.0])] {
        let v = |x, y| Vertex {
            position: [x, y, depth],
            color,
            uv: [0.0; 2],
            texture: 0,
        };
        let (a, b, c, d) = (v(-0.5, 0.5), v(0.5, 0.5), v(0.5, -0.5), v(-0.5, -0.5));
        mesh.extend([a, b, c, a, c, d]);
    }
    renderer.inventory_overlay = Some(mesh);
    let directory = output.join("depth");
    std::fs::create_dir(&directory).unwrap();
    let target = hair::resources::smoke_output(&root, &directory).unwrap();
    renderer
        .capture(&target, "office.png", &pose, &world)
        .unwrap();
    renderer.wait().unwrap();
    let mut reader = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(directory.join("office.png")).unwrap(),
    ))
    .read_info()
    .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    let at = (240 * 640 + 320) * info.color_type.samples();
    assert_eq!(
        &pixels[at..at + 3],
        &[255, 0, 0],
        "near inspector surface must occlude a farther surface submitted later"
    );
}
