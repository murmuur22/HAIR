use hair::web::{Address, Page};
fn generate(html: &[u8]) -> hair::web::Generated {
    Page::html(Address::parse("https://example.org/pocket").unwrap(), html)
        .unwrap()
        .generate()
        .unwrap()
}
#[test]
fn multiple_sections_generate_connected_reading_areas_not_only_furniture() {
    let g = generate(b"<h1>Orchard</h1><p>Apple trees.</p><h2>Harbour</h2><p>Boats arrive.</p>");
    assert!(
        g.host.geometry.len() >= 2,
        "content-derived pocket needs actual dividing architecture"
    );
    assert!(g.host.blocked([-1.5, 2.4]), "partition must be solid");
    assert!(
        !g.host.blocked([-1.5, -1.0]),
        "passage must remain walkable"
    );
    g.host.check_routes(&[[-4.0, -1.0], [4.0, -1.0]]).unwrap();
    assert!(
        g.host
            .anchors
            .iter()
            .any(|a| a.role == "document" && a.at[0] < -1.5)
    );
    assert!(
        g.host
            .anchors
            .iter()
            .any(|a| a.role == "document" && a.at[0] > -1.5)
    );
    let plain = generate(b"<p>One short unsectioned page.</p>");
    assert!(
        plain.host.geometry.is_empty(),
        "plain content should retain its simpler pocket"
    );
}
