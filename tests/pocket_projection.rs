use hair::web::{Address, Page};
fn generate(html: &str) -> hair::web::Generated {
    Page::html(
        Address::parse("https://example.org/source?Case=A%2Fb").unwrap(),
        html.as_bytes(),
    )
    .unwrap()
    .generate()
    .unwrap()
}
#[test]
fn blank_heading_does_not_promote_body_text_to_publisher_title() {
    let page = Page::html(
        Address::parse("https://example.org/source").unwrap(),
        b"<h2></h2><p>Body is not a heading</p>",
    )
    .unwrap();
    assert_eq!(
        page.sections[0].title, "",
        "section title must come from heading tokens, not the first body line"
    );
}

#[test]
fn plain_page_keeps_one_full_source_object() {
    let g = generate("<title>Plain</title><p>Unsectioned body.</p>");
    assert!(g.world.pocket.as_ref().unwrap().documents.is_empty());
    assert_eq!(
        g.host
            .anchors
            .iter()
            .filter(|a| a.role == "document")
            .count(),
        1
    );
    assert!(g.world.document().unwrap().contains("Unsectioned body."));
}
#[test]
fn excluded_subtrees_never_create_readable_objects_or_text() {
    let g = generate(
        "<form><button><h1>Secret form</h1>Hidden</button></form><template><h2>Hidden template</h2></template><h1>Visible &amp; real</h1><p>Readable &lt;value&gt;.</p>",
    );
    let pocket = g.world.pocket.as_ref().unwrap();
    assert_eq!(pocket.documents.len(), 1);
    let doc = pocket.documents.values().next().unwrap();
    assert_eq!(doc.title, "Visible & real");
    assert!(doc.text.contains("Readable <value>."));
    assert!(!doc.text.contains("Hidden"));
}
#[test]
fn passive_form_heading_and_body_create_readable_objects() {
    let g = generate(
        "<form><h1>Public form article</h1>Passive body</form><template><h2>Hidden template</h2></template><h1>Visible &amp; real</h1><p>Readable &lt;value&gt;.</p>",
    );
    let pocket = g.world.pocket.as_ref().unwrap();
    assert_eq!(pocket.documents.len(), 2);
    let article = pocket
        .documents
        .values()
        .find(|doc| doc.title == "Public form article")
        .unwrap();
    assert!(article.text.contains("Passive body"));
    let visible = pocket
        .documents
        .values()
        .find(|doc| doc.title == "Visible & real")
        .unwrap();
    assert!(visible.text.contains("Readable <value>."));
    assert!(
        pocket
            .documents
            .values()
            .all(|doc| !doc.text.contains("Hidden template"))
    );
}
#[test]
fn maximum_section_count_retains_every_section_in_bounded_collections() {
    let html = (0..64)
        .map(|i| format!("<h2>Section {i}</h2><p>token-{i:02}-end</p>"))
        .collect::<String>();
    let g = generate(&html);
    let p = g.world.pocket.as_ref().unwrap();
    assert_eq!(p.documents.len(), 8);
    let all = p
        .documents
        .values()
        .map(|d| d.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for i in 0..64 {
        assert_eq!(all.matches(&format!("token-{i:02}-end")).count(), 1);
    }
    assert!(p.documents.values().any(|d| d.title == "Multiple sections"));
}
#[test]
fn reordered_unique_sections_keep_their_objects_and_source_address() {
    let a = generate("<h2>Orchard</h2>Apples<h2>Harbour</h2>Boats<h2>Mountain</h2>Stones");
    let b = generate("<h2>Mountain</h2>Stones<h2>Orchard</h2>Apples<h2>Harbour</h2>Boats");
    assert_eq!(a.host, b.host);
    for (anchor, doc) in &a.world.pocket.as_ref().unwrap().documents {
        let other = &b.world.pocket.as_ref().unwrap().documents[anchor];
        assert_eq!(doc.title, other.title);
        assert!(doc.text.contains("https://example.org/source?Case=A%2Fb"));
    }
}
#[test]
fn duplicate_headings_do_not_drop_distinct_content() {
    let g = generate("<h2>Same</h2>First text<h2>Same</h2>Second text");
    let p = g.world.pocket.as_ref().unwrap();
    assert_eq!(p.documents.len(), 2);
    let texts = p
        .documents
        .values()
        .map(|d| d.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(texts.contains("First text") && texts.contains("Second text"));
}
