#[test]
fn collection_shape_clones_document_role_without_mutating_source_or_host() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let before = bytemuck::cast_slice::<_, u8>(&theme.vertices).to_vec();
    let anchor = theme
        .host
        .anchors
        .iter()
        .find(|a| a.role == "document")
        .unwrap();
    let copy = theme.collection_shape(&anchor.id);
    assert!(
        !copy.is_empty(),
        "successful collection needs a theme-object visual clone"
    );
    assert!(copy.len() <= 32766 && copy.len().is_multiple_of(3));
    assert!(copy.iter().all(|v| {
        theme
            .vertices
            .iter()
            .any(|original| bytemuck::bytes_of(original) == bytemuck::bytes_of(v))
    }));
    assert_eq!(before, bytemuck::cast_slice::<_, u8>(&theme.vertices));
    let portal = theme
        .host
        .anchors
        .iter()
        .find(|a| a.role == "portal")
        .unwrap();
    assert!(
        theme.collection_shape(&portal.id).is_empty(),
        "physical routes cannot become pickups"
    );
    assert!(theme.collection_shape("missing").is_empty());
}
