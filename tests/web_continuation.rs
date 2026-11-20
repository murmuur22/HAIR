//! Owned synthetic component controls. PREPARED, not executed by the file-only writer.
use hair::{
    content::Destination,
    web::{Address, Completeness, Page},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn parse(input: &str) -> hair::Result<Page> {
    Page::html(
        Address::parse("https://example.org/Board?Case=A%2Fb").unwrap(),
        input.as_bytes(),
    )
}
fn fixture(n: usize) -> String {
    let mut s = String::from("<title>Rooms &amp; source</title><p>Readable start</p>");
    for i in 0..n {
        s.push_str(&format!(
            "<a href='/Exact/{i}?Case=AB%2Fcd#Tail'>Link {i} &amp; <b>whole</b></a>"
        ));
    }
    s.push_str("<p>Readable final tail</p>");
    s
}
fn same(a: &Page, b: &Page) {
    assert_eq!(a.text, b.text);
    assert_eq!(a.title, b.title);
    assert_eq!(a.headings, b.headings);
    assert_eq!(a.links, b.links);
    assert_eq!(a.images, b.images);
    assert_eq!(a.completeness, b.completeness);
    assert_eq!(a.window, b.window);
    assert_eq!(
        a.sections
            .iter()
            .map(|s| (&s.title, &s.text))
            .collect::<Vec<_>>(),
        b.sections
            .iter()
            .map(|s| (&s.title, &s.text))
            .collect::<Vec<_>>()
    );
    assert!(Arc::ptr_eq(
        a.source.as_ref().unwrap(),
        b.source.as_ref().unwrap()
    ));
}
#[test]
fn source_order_reverse_replay_and_physical_actions_share_one_response() {
    let first = parse(&fixture(130)).unwrap();
    let second = first.next_window(None).unwrap().unwrap();
    let third = second.next_window(None).unwrap().unwrap();
    assert_eq!(
        [first.links.len(), second.links.len(), third.links.len()],
        [64, 64, 2]
    );
    let mut i = 0;
    for page in [&first, &second, &third] {
        assert!(Arc::ptr_eq(
            first.source.as_ref().unwrap(),
            page.source.as_ref().unwrap()
        ));
        assert_eq!(page.address, first.address);
        for link in &page.links {
            assert_eq!(link.label, format!("Link {i} & whole"));
            assert_eq!(
                link.destination,
                Destination::External(format!("https://example.org/Exact/{i}?Case=AB%2Fcd#Tail"))
            );
            i += 1;
        }
    }
    assert_eq!(i, 130);
    let expected_text = format!(
        "Readable start\n{}\nReadable final tail",
        (0..130)
            .map(|i| format!("Link {i} & whole"))
            .collect::<String>()
    );
    assert_eq!(
        format!("{}{}{}", first.text, second.text, third.text),
        expected_text
    );
    assert!(third.text.contains("Readable final tail"));
    assert_eq!(third.completeness, Completeness::Complete);
    assert!(third.next_window(None).unwrap().is_none());
    assert!(first.previous_window(None).unwrap().is_none());
    same(&second, &third.previous_window(None).unwrap().unwrap());
    same(&first, &second.previous_window(None).unwrap().unwrap());
    for (page, previous, next) in [
        (&first, false, true),
        (&second, true, true),
        (&third, true, false),
    ] {
        let g = page.clone().generate().unwrap();
        assert_eq!(
            g.host.anchors.iter().any(|a| a.id == "web-previous-window"),
            previous
        );
        assert_eq!(
            g.host.anchors.iter().any(|a| a.id == "web-next-window"),
            next
        );
        assert_eq!(
            g.window_doors.len(),
            usize::from(previous) + usize::from(next)
        );
        assert_eq!(
            g.host.portal_inward("web-next-window"),
            if next { [-1.0, 0.0] } else { [0.0, 1.0] }
        );
        let disclosure = g.world.document().unwrap();
        assert!(disclosure.contains("Room ") && disclosure.contains("source events"));
        assert!(disclosure.contains(&first.address.original));
        assert!(disclosure.contains(if next { "more content" } else { "EOF" }));
        assert_eq!(g.world.reader_links(), page.links);
        g.host.check_routes(&[]).unwrap();
    }
    let a = first.clone().generate().unwrap();
    let b = second.generate().unwrap();
    for door in &a.doors {
        if let Some(other) = b.doors.iter().find(|d| d.anchor == door.anchor) {
            assert_eq!(
                a.host.anchors.iter().find(|h| h.id == door.anchor),
                b.host.anchors.iter().find(|h| h.id == other.anchor)
            );
        }
    }
}
#[test]
fn cursor_binds_exact_session_and_rejects_public_metadata_changes() {
    let a = parse(&fixture(65)).unwrap();
    let b = parse(&fixture(65)).unwrap();
    let cursor = a.next_cursor().unwrap().unwrap();
    assert!(b.source.as_ref().unwrap().window(&cursor, None).is_err());
    let mut forged = a.clone();
    forged.source = b.source.clone();
    assert!(forged.next_window(None).is_err());
    assert!(forged.generate().is_err());
    let mut forged = a;
    forged.completeness = Completeness::Complete;
    assert!(forged.next_window(None).is_err());
    assert!(forged.generate().is_err());
}
#[test]
fn later_indivisible_overflow_refuses_without_modifying_last_good() {
    for item in [
        format!("<a href='/big'>{}</a>", "x".repeat(16385)),
        format!("<h2>{}</h2>", "x".repeat(16385)),
        format!("<title>{}</title>", "x".repeat(513)),
    ] {
        let first = parse(&format!("{}{}", fixture(64), item)).unwrap();
        let before = first.clone();
        assert!(first.next_window(None).is_err());
        same(&first, &before);
    }
}
#[test]
fn each_window_has_independent_text_image_heading_guards_and_context() {
    for input in [
        format!("<p>{}</p><p>Café &amp; tail</p>", "x".repeat(16380)),
        (0..33)
            .map(|i| format!("<img src='/p{i}.png' alt='Image{i}'>"))
            .collect(),
        (0..130)
            .map(|i| format!("<h2>Heading{i}</h2>text{i}"))
            .collect(),
    ] {
        let mut p = parse(&input).unwrap();
        let mut count = 0;
        loop {
            assert!(
                p.text.len() <= 16384
                    && p.images.len() <= 16
                    && p.headings.len() <= 64
                    && p.title.len() <= 512
                    && p.links.len() <= 64
            );
            p.clone().generate().unwrap();
            count += 1;
            assert!(count < 10);
            match p.next_window(None).unwrap() {
                Some(next) => p = next,
                None => break,
            }
        }
        assert!(count > 1);
    }
    let s = format!(
        "{}<template><script>let x='<a href=/hidden>Hidden</a>';</script><form><a href='/secret'>Secret</a></form></template><p>Café &amp; final</p>",
        fixture(65)
    );
    let first = parse(&s).unwrap();
    let next = first.next_window(None).unwrap().unwrap();
    assert!(!next.text.contains("Hidden") && !next.text.contains("Secret"));
    assert!(next.text.contains("Café & final"));
    assert_eq!(next.links.len(), 1);
}
#[test]
fn small_complete_room_is_unchanged_and_cancel_deadline_preserved() {
    let p =
        parse("<title>Small &amp; real</title><p>Before</p><a href='#Part'>A &amp; B</a>").unwrap();
    assert_eq!(p.text, "Before\nA & B");
    assert!(p.next_window(None).unwrap().is_none());
    let expected = format!("{}\n{}\n\n{}", p.title, p.address.original, p.text);
    let g = p.generate().unwrap();
    assert_eq!(g.world.document().unwrap(), expected);
    assert!(g.window_doors.is_empty());
    let p = parse(&fixture(130)).unwrap();
    let cancel = hair::acquisition::Cancellation::default();
    cancel.cancel();
    assert!(
        p.next_window(Some((&cancel, Instant::now() + Duration::from_secs(1))))
            .is_err()
    );
    let cancel = hair::acquisition::Cancellation::default();
    assert!(p.next_window(Some((&cancel, Instant::now()))).is_err());
    let next = p.next_window(None).unwrap().unwrap();
    assert!(
        next.previous_window(Some((&cancel, Instant::now())))
            .is_err()
    );
}
#[test]
fn full_tail_structural_negatives_still_reject_before_any_arrival() {
    for tail in [
        format!("{}deep{}", "<div>".repeat(65), "</div>".repeat(65)),
        "<br>".repeat(8193),
        format!(
            "<p {}>tail</p>",
            (0..65).map(|i| format!("a{i}=x ")).collect::<String>()
        ),
        format!(
            "<template><p a='{}'>hidden</p></template>",
            "x".repeat(2049)
        ),
    ] {
        assert!(parse(&format!("{}{tail}", fixture(65))).is_err());
    }
}
