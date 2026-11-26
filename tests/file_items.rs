use hair::file_items::{Catalog, Class};
use std::{ffi::OsStr, sync::Arc};
#[test]
fn classes_share_bounded_embedded_models_not_furniture() {
    let c = Catalog::builtin().unwrap();
    for (name, class) in [
        ("a.PDF", Class::Document),
        ("a.docx", Class::Document),
        ("a.mp4", Class::Video),
        ("a.flac", Class::Audio),
        ("a.rs", Class::Code),
        ("a.png", Class::Image),
        ("a.glb", Class::Model),
        ("a.bin", Class::Other),
    ] {
        assert_eq!(Class::of(OsStr::new(name)), class);
        let a = c.representation(OsStr::new(name));
        let b = c.representation(OsStr::new(name));
        assert!(Arc::ptr_eq(&a, &b));
        assert!(!a.vertices.is_empty() && a.vertices.len() <= 32766);
        assert!(a.textures.len() <= 1);
        assert!(a.vertices.iter().any(|v| v.texture != 0));
        assert!(
            a.vertices
                .iter()
                .all(|v| v.texture as usize <= a.textures.len())
        );
    }
    assert!(!Arc::ptr_eq(
        &c.representation(OsStr::new("a.pdf")),
        &c.representation(OsStr::new("a.wav"))
    ));
    assert!(c.retained_bytes() < 8 * 1024 * 1024);
}
#[test]
fn world_item_is_above_stationary_base_and_collection_excludes_furniture() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let generic =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let theme = hair::theme::Theme::load_for_folder_scene(
        &root.join("packs/office.toml"),
        &mut Default::default(),
        &generic.host,
    )
    .unwrap();
    let a = theme
        .host
        .anchors
        .iter()
        .find(|a| a.role == "document")
        .unwrap();
    let shape = theme.file_item_shape(&a.id, OsStr::new("notes.pdf"));
    assert!(!shape.is_empty());
    assert!(
        shape
            .iter()
            .all(|v| v.position[1] >= a.at[1] + a.size[1] + 0.1)
    );
    assert!(shape.iter().any(|v| v.texture != 0));
    assert!(
        shape
            .iter()
            .all(|v| (v.texture & 0x7fffffff) as usize <= theme.textures.len())
    );
    assert_ne!(shape.len(), theme.collection_shape(&a.id).len());
    assert!(
        theme
            .file_item_shape("missing", OsStr::new("a.pdf"))
            .is_empty()
    );
}
#[test]
fn folder_scene_draws_items_in_addition_to_unchanged_bases() {
    let dir = std::env::temp_dir().join(format!("hair-file-items-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("notes.pdf"), b"fixture").unwrap();
    let explorer = hair::local_explorer::Explorer::new(&dir).unwrap();
    let (world, host) = hair::folder_scene::build(explorer.current(), 0, false).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let theme = hair::theme::Theme::load_for_folder_scene(
        &root.join("packs/office.toml"),
        &mut Default::default(),
        &host,
    )
    .unwrap();
    let (id, b) = world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == "notes.pdf")
        .unwrap();
    let output = theme.vertices_for(&world);
    for v in theme.file_item_shape(id, &b.entry.name) {
        assert!(
            output
                .iter()
                .any(|o| bytemuck::bytes_of(o) == bytemuck::bytes_of(&v)),
            "floating file item missing"
        );
    }
    for v in theme.collection_shape(id) {
        assert!(
            output
                .iter()
                .any(|o| bytemuck::bytes_of(o) == bytemuck::bytes_of(&v)),
            "stationary base changed"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn admitted_text_and_video_content_reaches_class_item_without_replacing_base() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let base = theme
        .host
        .anchors
        .iter()
        .find(|a| a.role == "document")
        .unwrap()
        .clone();
    let furniture = theme.collection_shape(&base.id);
    let texture = Arc::new(hair::assets::Texture {
        width: 2,
        height: 1,
        rgba: vec![97, 31, 173, 255, 0, 200, 40, 255],
    });
    theme.set_folder_previews(&[(base.id.clone(), texture.clone())].into_iter().collect());
    let slot = theme
        .textures
        .iter()
        .position(|t| Arc::ptr_eq(t, &texture))
        .unwrap() as u32
        + 1;
    for name in ["notes.txt", "film.mp4"] {
        let shape = theme.file_item_shape(&base.id, OsStr::new(name));
        let source = Catalog::builtin().unwrap().representation(OsStr::new(name));
        assert_eq!(
            shape.len(),
            source.vertices.len(),
            "keep authored class silhouette"
        );
        assert!(
            shape.iter().any(|v| v.texture == (slot | 0x8000_0000)),
            "admitted real content must not be replaced by demo art"
        );
        for (a, b) in shape.iter().zip(&source.vertices) {
            assert_eq!(a.uv, b.uv);
            assert_eq!(a.color, b.color);
        }
        assert!(
            shape
                .iter()
                .all(|v| v.position[1] >= base.at[1] + base.size[1] + 0.1)
        );
    }
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&furniture),
        bytemuck::cast_slice::<_, u8>(&theme.collection_shape(&base.id))
    );
}
#[test]
fn textured_inspector_preserves_uv_material_and_stays_in_pane() {
    let c = Catalog::builtin().unwrap();
    let m = c.representation(OsStr::new("a.wav"));
    let items = [hair::inventory::Item {
        id: 0,
        name: "a.wav".into(),
        bytes: vec![],
        stored_bytes: None,
    }];
    let output = hair::inventory_ui::overlay_with_item(&items, 0, 0.0, [640, 480], &m.vertices);
    let textured = output.iter().filter(|v| v.texture != 0).collect::<Vec<_>>();
    assert!(!textured.is_empty());
    assert!(
        textured
            .iter()
            .all(|v| v.position[2] > 0.0 && v.position[2] < 1.0),
        "inspector model must carry real depth separately from flat UI"
    );
    assert!(textured.iter().any(|v| v.uv != [0.0; 2]));
    assert!(textured.iter().all(|v| v.position[0] >= 0.0
        && v.position[0] <= 1.0
        && v.position[1] <= 0.4
        && v.position[1] >= -0.4));
}
#[test]
fn inspector_depth_is_projective_not_linear_eye_depth() {
    use hair::assets::Vertex;
    let shape = [
        ([-1.0, -1.0, 0.0], [1.0, 0.0, 0.0]),
        ([1.0, -1.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
    ]
    .map(|(position, color)| Vertex {
        position,
        color,
        uv: [0.0; 2],
        texture: 1,
    });
    let items = [hair::inventory::Item {
        id: 0,
        name: "test.glb".into(),
        bytes: vec![],
        stored_bytes: None,
    }];
    let output = hair::inventory_ui::overlay_with_item(&items, 0, 0.0, [640, 480], &shape);
    let origin = glam::Vec3::new(0.0, 0.0, 0.5);
    let radius = shape
        .iter()
        .map(|v| (glam::Vec3::from_array(v.position) - origin).length())
        .fold(0.0_f32, f32::max);
    let rotation = glam::Mat3::from_rotation_x(-0.25) * glam::Mat3::from_rotation_y(0.35);
    for source in shape {
        let v = rotation * ((glam::Vec3::from_array(source.position) - origin) / radius);
        let expected = 0.9 - 1.0 / (3.0 - v.z);
        let actual = output
            .iter()
            .find(|p| p.texture == 1 && p.color == source.color)
            .unwrap();
        assert!(
            (actual.position[2] - expected).abs() < 1e-6,
            "perspective depth must be affine in reciprocal clip-w, not eye z"
        );
    }
}
#[test]
fn acquired_copy_retains_its_own_content_preview_after_source_change() {
    let dir = std::env::temp_dir().join(format!("hair-owned-items-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let bytes = include_bytes!("../examples/site-a/swatch.png");
    std::fs::write(dir.join("source.png"), bytes).unwrap();
    let explorer = hair::local_explorer::Explorer::new(&dir).unwrap();
    let index = explorer
        .current()
        .entries
        .iter()
        .position(|e| e.name == "source.png")
        .unwrap();
    let mut inventory = hair::inventory::Inventory::default();
    let id = inventory.collect(&explorer, index).unwrap();
    let preview = inventory.preview(id).unwrap();
    assert!(preview.content_derived, "{:?}", preview.warning);
    let expected =
        hair::file_items::image_plane(Arc::new(hair::assets::decode_local_image(bytes).unwrap()))
            .unwrap();
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&preview.model.vertices),
        bytemuck::cast_slice::<_, u8>(&expected.vertices)
    );
    std::fs::write(dir.join("source.png"), b"changed").unwrap();
    assert_eq!(inventory.items()[0].bytes, bytes);
    assert!(inventory.preview(id).unwrap().content_derived);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn collection_keeps_item_materials_and_sampled_uvs() {
    let source = [
        hair::assets::Vertex {
            position: [-0.2, 1.4, -3.0],
            color: [0.8, 0.6, 0.3],
            uv: [0.0, 0.0],
            texture: 2,
        },
        hair::assets::Vertex {
            position: [0.2, 1.4, -3.0],
            color: [0.8, 0.6, 0.3],
            uv: [1.0, 0.0],
            texture: 2,
        },
        hair::assets::Vertex {
            position: [0.0, 1.8, -3.0],
            color: [0.8, 0.6, 0.3],
            uv: [0.5, 1.0],
            texture: 2,
        },
    ];
    let effect = hair::collection::Effect::new(
        &source,
        &hair::Pose::default(),
        hair::sidebar::Frame::new([640, 480], 100, false),
    )
    .unwrap();
    let drawn = effect.paint(0.1, false);
    for (a, b) in drawn.iter().zip(&source) {
        assert_eq!(a.texture, b.texture);
        assert_eq!(a.uv, b.uv);
        assert_eq!(a.color, b.color);
    }
}
#[test]
fn owned_image_preserves_pixels_aspect_and_refused_input_is_explicit() {
    let c = Catalog::builtin().unwrap();
    let source = hair::assets::Texture {
        width: 4,
        height: 2,
        rgba: vec![255; 32],
    };
    let plane = hair::file_items::image_plane(Arc::new(source)).unwrap();
    assert_eq!(plane.vertices.len(), 6);
    assert!(
        plane.vertices.iter().all(|v| v.texture == 0x8000_0001),
        "actual image items must retain straight-alpha media routing"
    );
    let extent = [0, 1].map(|i| plane.max[i] - plane.min[i]);
    assert!((extent[0] / extent[1] - 2.0).abs() < 1e-5);
    assert_eq!(plane.textures[0].rgba, vec![255; 32]);
    let p = c.owned(OsStr::new("bad.png"), b"not image");
    assert!(p.warning.is_some());
    assert!(!p.content_derived);
    let m = c.owned(OsStr::new("bad.glb"), b"not glb");
    assert!(m.warning.is_some());
    assert!(!m.content_derived);
}
