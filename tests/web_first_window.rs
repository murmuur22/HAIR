//! Owned synthetic first-window controls. Prepared by a file-only writer; not executed.
use hair::{
    content::Destination,
    web::{Address, Completeness, Page, WindowLimit},
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

fn parse(html: &str) -> hair::Result<Page> {
    Page::html(
        Address::parse("https://example.org/Board?Case=A%2Fb").unwrap(),
        html.as_bytes(),
    )
}
fn links(n: usize) -> String {
    let mut html =
        String::from("<title>Small &amp; real</title><h2>Reading</h2><p>Readable prefix</p>");
    for i in 0..n {
        html.push_str(&format!(
            "<a href='/Thread/{i}?Case=AB%2Fcd#Part'>Link {i} &amp; <b>whole</b></a>"
        ));
    }
    html
}

#[test]
fn first_window_preserves_boundary_identity_and_generated_disclosure() {
    let complete = parse(&links(64)).unwrap();
    let input = links(65);
    let mut partial = parse(&input).unwrap();
    assert_eq!(complete.completeness, Completeness::Complete);
    assert!(matches!(
        partial.completeness,
        Completeness::FirstWindow {
            limit: WindowLimit::Links,
            ..
        }
    ));
    assert_eq!(partial.text, complete.text);
    assert_eq!(partial.links, complete.links);
    assert_eq!(partial.links.len(), 64);
    for (i, link) in partial.links.iter().enumerate() {
        assert_eq!(link.label, format!("Link {i} & whole"));
        assert_eq!(link.raw, format!("/Thread/{i}?Case=AB%2Fcd#Part"));
        assert_eq!(
            link.destination,
            Destination::External(format!("https://example.org/Thread/{i}?Case=AB%2Fcd#Part"))
        );
    }
    let held = partial.source.as_ref().unwrap().clone();
    assert_eq!(held.retained_charge(), input.len() + 8192);
    assert_eq!(held.validated_scalars(), input.chars().count());
    assert_eq!(held.encoded(), input.as_bytes());
    assert_eq!(
        held.sha256(),
        <[u8; 32]>::from(Sha256::digest(input.as_bytes()))
    );
    assert_eq!(held.address(), &partial.address);
    assert_eq!(held.completeness(), partial.completeness);
    assert_eq!(held.policy_version(), 1);
    assert_eq!(held.validated_bytes(), input.len());
    assert!(held.validated_tokens() > 130);
    let cloned = partial.clone();
    assert!(Arc::ptr_eq(cloned.source.as_ref().unwrap(), &held));
    let warnings = partial.warnings.join(" ");
    assert!(warnings.contains("incomplete") && warnings.contains("more content"));
    // Generator disclosure is host-owned typed meaning, not dependent on a caller
    // preserving an arbitrary warning String.
    partial.warnings.clear();
    let mut generated = partial.generate().unwrap();
    assert!(Arc::ptr_eq(
        generated
            .world
            .pocket
            .as_ref()
            .unwrap()
            .source
            .as_ref()
            .unwrap(),
        &held
    ));
    let full = generated.world.document().unwrap().to_ascii_lowercase();
    assert!(full.contains("incomplete") && full.contains("more content"));
    let cursor = generated.page.next_cursor().unwrap().unwrap();
    assert_eq!(cursor.index(), 1);
    assert!(generated.page.previous_cursor().unwrap().is_none());
    assert_eq!(generated.window_doors.len(), 1);
    let binding = &generated.window_doors[0];
    assert_eq!(binding.anchor, "web-next-window");
    assert_eq!(
        binding.action,
        hair::web::WindowAction::Next(cursor.clone())
    );
    let door = generated
        .host
        .anchors
        .iter()
        .find(|a| a.id == binding.anchor)
        .unwrap();
    assert_eq!(door.role, "portal");
    assert_eq!(door.binding, "explicit-door");
    assert!(door.at[0] > generated.host.room[2]);
    assert!(
        generated
            .host
            .wall_apertures
            .iter()
            .any(|a| a.anchor == binding.anchor && a.wall == hair::host_scene::Wall::Right)
    );
    assert_eq!(generated.host.portal_inward(&binding.anchor), [-1.0, 0.0]);
    assert!(full.contains("room 1"));
    let range = generated.page.window.as_ref().unwrap().event_range();
    assert!(full.contains(&format!("source events {}..{}", range.start, range.end)));
    let hash = held
        .sha256()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert!(full.contains(&format!("snapshot sha256 {hash}")));
    let last = held.window(&cursor, None).unwrap();
    assert_eq!(last.links.len(), 1);
    assert_eq!(last.links[0].label, "Link 64 & whole");
    assert_eq!(last.links[0].raw, "/Thread/64?Case=AB%2Fcd#Part");
    assert_eq!(
        last.links[0].destination,
        Destination::External("https://example.org/Thread/64?Case=AB%2Fcd#Part".into())
    );
    assert!(Arc::ptr_eq(last.source.as_ref().unwrap(), &held));
    assert_eq!(last.completeness, Completeness::Complete);
    assert!(last.next_cursor().unwrap().is_none());
    assert_eq!(generated.world.reader_links(), generated.page.links);
    // Section readers must not lose the disclosure when reader_links is suppressed.
    let pocket = generated.world.pocket.as_mut().unwrap();
    for doc in pocket.documents.values() {
        assert!(doc.text.contains("incomplete") && doc.text.contains("more content"));
        assert!(!doc.text.contains("retains all page links"));
    }
}

#[test]
fn complete_small_page_keeps_decoding_sections_labels_and_reader_bytes() {
    let input = "<title>Small &amp; real</title><p>Before</p><h2>Heading</h2><p>Café &lt;value&gt;</p><a href='#Part'>A &amp; B</a>";
    let p = parse(input).unwrap();
    assert_eq!(p.completeness, Completeness::Complete);
    assert!(p.warnings.is_empty());
    assert_eq!(p.title, "Small & real");
    assert_eq!(p.text, "Before\nHeading\nCafé <value>\nA & B");
    assert_eq!(p.headings, ["Heading"]);
    assert_eq!(p.sections[0].title, "Heading");
    assert_eq!(p.sections[0].text, "Heading\nCafé <value>\nA & B");
    assert_eq!(p.links[0].label, "A & B");
    let expected = format!("{}\n{}\n\n{}", p.title, p.address.original, p.text);
    assert_eq!(p.generate().unwrap().world.document().unwrap(), expected);
}

#[test]
fn semantic_cutoff_does_not_skip_full_tail_guards() {
    let prefix = links(65);
    let valid_tail = format!(
        "{}tail{}<p {}>After</p>",
        "<div>".repeat(64),
        "</div>".repeat(64),
        (0..64).map(|i| format!("a{i}=x ")).collect::<String>()
    );
    let accepted = parse(&format!("{prefix}{valid_tail}")).unwrap();
    assert_eq!(accepted.links.len(), 64);
    assert!(!accepted.text.contains("After"));
    for tail in [
        format!("{}deep{}", "<div>".repeat(65), "</div>".repeat(65)),
        format!(
            "{}hidden{}",
            "<template>".repeat(65),
            "</template>".repeat(65)
        ),
        "<br>".repeat(8193),
        format!(
            "<p {}>tail</p>",
            (0..65).map(|i| format!("a{i}=x ")).collect::<String>()
        ),
        format!(
            "<template><p a='{}'>hidden</p></template>",
            "x".repeat(2049)
        ),
        format!("<!--{}-->", "x".repeat(2049)),
        format!("<!DOCTYPE html PUBLIC '{}'>", "x".repeat(2049)),
        format!("<{}>", "x".repeat(2049)),
        "x".repeat(262145),
    ] {
        assert!(
            parse(&format!("{prefix}{tail}")).is_err(),
            "tail guard bypass"
        );
    }
    let mut invalid = prefix.into_bytes();
    invalid.extend_from_slice(b"<p>\xff</p>");
    assert!(Page::html(Address::parse("https://example.org/a").unwrap(), &invalid).is_err());
}

#[test]
fn text_overflow_rolls_back_whole_anchor_heading_and_entity_run() {
    let prefix = format!("<p>{}</p>", "x".repeat(16320));
    for item in [
        format!("<a href='/whole'>Café &amp; {}</a>", "z".repeat(100)),
        format!("<h2>Café &amp; {}</h2>", "z".repeat(100)),
        format!("Café &amp; {}", "z".repeat(100)),
    ] {
        let p = parse(&format!("{prefix}{item}<p>uncollected tail</p>")).unwrap();
        assert_eq!(p.text, "x".repeat(16320));
        assert!(p.links.is_empty() && p.headings.is_empty() && p.sections.is_empty());
        assert!(matches!(
            p.completeness,
            Completeness::FirstWindow {
                limit: WindowLimit::Text,
                ..
            }
        ));
        assert!(!p.text.contains("Caf") && !p.text.contains("uncollected"));
    }
}

#[test]
fn other_semantic_caps_bound_the_window_without_inflation() {
    let images = format!(
        "<p>Before</p>{}<p>tail</p>",
        (0..17)
            .map(|i| format!("<img src='/p{i}.png' alt='Image{i}'>"))
            .collect::<String>()
    );
    let p = parse(&images).unwrap();
    assert_eq!(p.images.len(), 16);
    assert!(!p.text.contains("Image16") && !p.text.contains("tail"));
    assert!(matches!(
        p.completeness,
        Completeness::FirstWindow {
            limit: WindowLimit::Images,
            ..
        }
    ));
    p.generate().unwrap();
    let headings = format!(
        "<p>Before</p>{}",
        (0..65)
            .map(|i| format!("<h2>Heading{i}</h2>text{i}"))
            .collect::<String>()
    );
    let p = parse(&headings).unwrap();
    assert_eq!(p.headings.len(), 64);
    assert_eq!(p.sections.len(), 64);
    assert!(!p.text.contains("Heading64"));
    p.generate().unwrap();
    let p = parse(&format!(
        "<p>Before</p><title>{}</title><p>tail</p>",
        "x".repeat(513)
    ))
    .unwrap();
    assert!(p.title.is_empty());
    assert_eq!(p.text, "Before");
    assert!(matches!(
        p.completeness,
        Completeness::FirstWindow {
            limit: WindowLimit::Title,
            ..
        }
    ));
    p.generate().unwrap();
}

#[test]
fn suppressed_tail_remains_raw_and_cannot_reopen_collection() {
    let prefix = links(65);
    let tail = "<template><script>let x='<a href=/hidden>Hidden</a>';</script><form><a href='/secret'>Secret</a></form></template><p>After</p>";
    let p = parse(&format!("{prefix}{tail}")).unwrap();
    assert_eq!(p.links.len(), 64);
    assert!(!p.text.contains("Hidden") && !p.text.contains("Secret") && !p.text.contains("After"));
    assert!(p.warnings.iter().any(|w| w.contains("scripts/forms")));
}
