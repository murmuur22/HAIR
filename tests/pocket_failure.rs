use hair::{
    interaction::{Kind, Target},
    web::{Address, Page},
};
fn fixture() -> hair::web::Generated {
    Page::html(
        Address::parse("https://example.org/source").unwrap(),
        b"<p>Body</p><a href='/next#one'>One</a><a href='/next#two'>Two</a>",
    )
    .unwrap()
    .generate()
    .unwrap()
}
#[test]
fn failure_warning_belongs_to_exact_destination_not_entire_group() {
    let mut g = fixture();
    let anchor = g.doors[0].anchor.clone();
    let v = hair::web::portals::view(&g.world, &anchor).unwrap();
    g.world.pocket.as_mut().unwrap().failed_arrival =
        Some((v.destination.clone(), "Timed out".into()));
    assert_eq!(
        hair::web::portals::view(&g.world, &anchor)
            .unwrap()
            .last_failure
            .as_deref(),
        Some("Timed out")
    );
    hair::web::portals::cycle(&mut g.world, &anchor, false);
    assert!(
        hair::web::portals::view(&g.world, &anchor)
            .unwrap()
            .last_failure
            .is_none()
    );
    hair::web::portals::cycle(&mut g.world, &anchor, true);
    assert!(
        hair::web::portals::view(&g.world, &anchor)
            .unwrap()
            .last_failure
            .is_some()
    );
}
#[test]
fn failed_portal_panel_retains_viewport_bounds_at_maximum_address_and_label() {
    let destination = format!("https://example.org/{}", "x".repeat(2000));
    let html = format!("<p>Body</p><a href='{destination}'>{}</a>", "A".repeat(512));
    let mut g = Page::html(
        Address::parse("https://example.org/source").unwrap(),
        html.as_bytes(),
    )
    .unwrap()
    .generate()
    .unwrap();
    let anchor = g.doors[0].anchor.clone();
    g.world.pocket.as_mut().unwrap().failed_arrival = Some((
        destination.clone(),
        format!("\n\r\u{1b}{}", "Reason ".repeat(1000)),
    ));
    g.world.interaction = Some(Target {
        anchor: anchor.clone(),
        kind: Kind::Enter,
        page: g.world.current.clone(),
        destination: Some(destination),
        prompt: "Follow".into(),
    });
    let v = hair::web::portals::view(&g.world, &anchor).unwrap();
    assert!(v.last_failure.as_ref().unwrap().chars().count() <= 96);
    assert!(
        !v.last_failure
            .as_ref()
            .unwrap()
            .chars()
            .any(char::is_control)
    );
    for (w, h) in [(960, 640), (640, 480)] {
        let vertices = hair::text::portal_overlay(&g.world, w, h);
        assert!(!vertices.is_empty());
        assert!(vertices.iter().all(|v| {
            v.position
                .iter()
                .all(|x| x.is_finite() && (-1.0..=1.0).contains(x))
        }));
    }
}
