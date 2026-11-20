use hair::{authoring::Project, materials::Surface, theme::Theme};
#[test]
fn material_only_project_and_clear_texture_preserve_tint_tile_roundtrip() {
    let d = std::env::temp_dir().join(format!("hair-material-only-{}", std::process::id()));
    std::fs::create_dir(&d).unwrap();
    let png = d.join("input.png");
    let f = std::fs::File::create(&png).unwrap();
    let mut e = png::Encoder::new(f, 1, 1);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .unwrap()
        .write_image_data(&[200, 80, 20, 255])
        .unwrap();
    let mut p = Project::blank();
    assert!(p.model.vertices.is_empty());
    p.set_surface(Surface {
        surface: "wall".into(),
        file: None,
        tint: [0.7, 0.3, 0.2],
        meters_per_tile: 2.5,
    })
    .unwrap();
    p.import_surface("wall", &png).unwrap();
    let edit = p.surfaces()[0].clone();
    p.clear_surface_texture("wall").unwrap();
    let cleared = p.surfaces()[0].clone();
    assert!(cleared.file.is_none());
    assert_eq!(
        (cleared.tint, cleared.meters_per_tile),
        (edit.tint, edit.meters_per_tile)
    );
    let saved = p.save_new(&d.join("saved"), &[]).unwrap();
    let q = Project::open(&saved).unwrap();
    assert_eq!(q.surfaces(), p.surfaces());
    let exported = q.export(&d.join("export"), &[]).unwrap();
    let t = Theme::load(&exported, &mut Default::default()).unwrap();
    assert!(t.loaded_roles.is_empty());
    assert!(t.textures.is_empty());
    assert!(!d.join("export/wall.png").exists());
    assert!(t.vertices[36..144].iter().all(|v| v.color == edit.tint));
    std::fs::remove_dir_all(d).unwrap();
}
