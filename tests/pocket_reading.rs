use hair::web::{Address, Page};

#[test]
fn constructed_sections_obey_cumulative_projection_budget() {
    let mut page = Page::html(
        Address::parse("https://example.org/pocket").unwrap(),
        b"<p>Body</p>",
    )
    .unwrap();
    page.sections = vec![hair::web::Section {
        title: "Section".into(),
        text: "x".repeat(16385),
    }];
    assert!(
        page.generate().is_err(),
        "public Page fields must not bypass derived text bounds"
    );
}

#[test]
fn html_sections_become_additional_readable_objects() {
    let g = Page::html(Address::parse("https://example.org/pocket").unwrap(),
        b"<title>Library</title><h1>Orchard</h1><p>Apple trees.</p><h2>Harbour</h2><p>Boats arrive.</p><h2>Mountain</h2><p>Stone paths.</p>")
        .unwrap().generate().unwrap();
    let documents: Vec<_> = g
        .host
        .anchors
        .iter()
        .filter(|a| a.role == "document")
        .collect();
    assert!(
        documents.len() >= 3,
        "multiple independently readable objects required, got {}",
        documents.len()
    );
    assert!(
        g.world.document().unwrap().contains("Apple trees."),
        "full-page object retained"
    );
}

#[test]
fn aimed_section_object_opens_only_its_bound_content() {
    let mut g = Page::html(Address::parse("https://example.org/pocket").unwrap(),
        b"<h1>Orchard</h1><p>Apple trees.</p><h2>Harbour</h2><p>Boats arrive.</p><a href='/next'>Next page</a>")
        .unwrap().generate().unwrap();
    let key = g
        .world
        .pocket
        .as_ref()
        .unwrap()
        .documents
        .iter()
        .find(|(_, d)| d.text.contains("Apple trees."))
        .unwrap()
        .0
        .clone();
    let anchor = g.host.anchors.iter().find(|a| a.id == key).unwrap().clone();
    let pose = hair::Pose {
        position: [anchor.at[0], anchor.at[2] + 1.2],
        yaw: 0.0,
        pitch: -0.5,
    };
    hair::interaction::refresh(&g.host, &pose, &mut g.world);
    assert_eq!(g.world.interaction.as_ref().unwrap().anchor, anchor.id);
    assert!(
        g.world
            .interaction
            .as_ref()
            .unwrap()
            .prompt
            .contains("Orchard"),
        "prompt must name the section being opened"
    );
    g.world.act(hair::Action::Inspect, pose.position);
    assert!(
        g.world.reading,
        "aimed section must open, independent of main desk proximity"
    );
    assert!(g.world.document().unwrap().contains("Apple trees."));
    assert!(!g.world.document().unwrap().contains("Boats arrive."));
    assert!(
        g.world.reader_links().is_empty(),
        "section must not advertise unrelated whole-page reader links"
    );
    assert!(
        g.world.reader_link_description().contains("FULL PAGE"),
        "section reader must explain where links remain available"
    );
    g.world.act(hair::Action::Close, pose.position);
    assert!(
        g.world.document().unwrap().contains("Boats arrive."),
        "full source remains inspectable when reader closes"
    );
}
