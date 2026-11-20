use hair::{
    content::Destination,
    web::{Address, Page},
};
#[test]
fn literal_authority_precedes_resolution() {
    let base = Address::parse("https://example.org/Dir/a?q=AB%2Fcd#top").unwrap();
    for raw in [
        "https://@example.org/a",
        "HTTPS://@example.org/a",
        "hTtPs://u:p@example.org/a",
        "//@example.org/a",
        "//u:p@example.org/a",
        "https://%40example.org/a",
        "//%40example.org/a",
        "https://example.org%2f.evil/a",
        "https:@example.org/a",
        "https://EXAMPLE.org/a",
        "HTTPS://example.org/a",
        "//EXAMPLE.org/a",
        "https://example.org:443/a",
        "https:///example.org/a",
        "https://example.org/a/../b",
    ] {
        let p = Page::html(
            base.clone(),
            format!("<p>Visible</p><a href='{raw}'>Link</a>").as_bytes(),
        )
        .unwrap();
        assert_eq!(p.links[0].raw, raw);
        assert!(
            matches!(p.links[0].destination, Destination::Invalid(_)),
            "{raw}: {:?}",
            p.links[0]
        );
    }
    for (raw, expected) in [
        (
            "https://other.example/Case?q=AB%2Fcd#Part",
            "https://other.example/Case?q=AB%2Fcd#Part",
        ),
        (
            "//other.example/Case?q=AB%2Fcd#Part",
            "https://other.example/Case?q=AB%2Fcd#Part",
        ),
        (
            "../Next?q=AB%2Fcd#Part",
            "https://example.org/Next?q=AB%2Fcd#Part",
        ),
        ("?q=AB%2Fcd", "https://example.org/Dir/a?q=AB%2Fcd"),
        ("#Part", "https://example.org/Dir/a?q=AB%2Fcd#Part"),
        (
            "/%40path?q=user@example.org#@fragment",
            "https://example.org/%40path?q=user@example.org#@fragment",
        ),
        ("", "https://example.org/Dir/a?q=AB%2Fcd"),
    ] {
        let p = Page::html(
            base.clone(),
            format!("<p>Visible</p><a href='{raw}'>Link</a>").as_bytes(),
        )
        .unwrap();
        assert_eq!(p.links[0].raw, raw);
        assert_eq!(
            p.links[0].destination,
            Destination::External(expected.into())
        );
        assert_eq!(p.address, base);
    }
}
