use std::path::PathBuf;
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn inventory_uses_pack_document_mesh_even_when_directory_is_empty() {
    let dir = std::env::temp_dir().join(format!("hair-inventory-theme-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let fixture = Fixture(dir);
    let explorer = hair::local_explorer::Explorer::new(&fixture.0).unwrap();
    let (world, host) = hair::folder_scene::build(explorer.current(), 0, false).unwrap();
    assert!(
        world
            .folder_view
            .as_ref()
            .unwrap()
            .bindings
            .values()
            .all(|b| b.entry.kind != hair::local_explorer::Kind::File)
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let theme = hair::theme::Theme::load_for_folder_scene(
        &root.join("packs/office.toml"),
        &mut Default::default(),
        &host,
    )
    .unwrap();
    let actual = theme.inventory_document_shape();
    assert!(
        !actual.is_empty(),
        "inventory must retain pack mesh even with no world document instances"
    );
    let expected =
        hair::assets::decode(&std::fs::read(root.join("packs/desk.glb")).unwrap(), 3.0).unwrap();
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(actual),
        bytemuck::cast_slice::<_, u8>(&expected.vertices),
        "office role has zero yaw; use actual model geometry, not a generated stand-in"
    );
    assert!(actual.len() <= 32766);
    let missing = fixture.0.join("missing.toml");
    std::fs::write(&missing, "version=2\nname=\"No document role\"\nroles=[]\n").unwrap();
    let absent =
        hair::theme::Theme::load_for_folder_scene(&missing, &mut Default::default(), &host)
            .unwrap();
    assert!(absent.inventory_document_shape().is_empty());
    std::fs::copy(root.join("packs/desk.glb"), fixture.0.join("desk.glb")).unwrap();
    let turned = fixture.0.join("turned.toml");
    std::fs::write(&turned,"version=2\nname=\"Quarter turn\"\n[[roles]]\nrole=\"document\"\nfile=\"desk.glb\"\nmax_extent=3.0\nscale=2.0\nyaw_degrees=90.0\npivot=[0.0,0.0,0.0]\n").unwrap();
    let rotated =
        hair::theme::Theme::load_for_folder_scene(&turned, &mut Default::default(), &host).unwrap();
    let mesh = rotated.inventory_document_shape();
    assert_eq!(mesh.len(), expected.vertices.len());
    for (actual, source) in mesh.iter().zip(&expected.vertices) {
        assert!(
            (actual.position[0] - source.position[2]).abs() < 0.000001
                && (actual.position[1] - source.position[1]).abs() < 0.000001
                && (actual.position[2] + source.position[0]).abs() < 0.000001
        );
        assert_eq!(actual.color, source.color);
    }
}
