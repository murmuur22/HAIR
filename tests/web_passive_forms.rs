use hair::web::{Address, Page};
fn parse(html: &str) -> Page {
    Page::html(
        Address::parse("https://example.org/po/").unwrap(),
        html.as_bytes(),
    )
    .unwrap()
}
#[test]
fn controls_do_not_project_text_destinations_or_images_even_outside_forms() {
    let page = parse(
        "<p>Public header</p><button form=f formaction='/erase'>Delete<a href='/bad'>Bad</a></button><select><option>Secret choice</option></select><textarea><a href='/raw'>Raw secret</a></textarea><datalist><option>Secret list</option></datalist><input type=image src='/control.png' value='Secret value'><p>Public tail</p>",
    );
    assert!(page.text.contains("Public header") && page.text.contains("Public tail"));
    for secret in ["Delete", "Bad", "Secret", "Raw secret"] {
        assert!(
            !page.text.contains(secret),
            "control content leaked: {}",
            page.text
        );
    }
    assert!(page.links.is_empty());
    assert!(page.images.is_empty());
}
#[test]
fn static_articles_inside_forms_remain_readable_without_submission_authority() {
    let page = parse(
        "<p>Public header</p><form id='delform' action='/delete' method=post><article><h2>Paper crane</h2><p>Fold a paper crane.</p><a href='/po/thread/123'>Read thread</a></article><input value='Secret value'><button formaction='/erase'>Delete</button><textarea>Secret textarea</textarea></form>",
    );
    assert!(
        page.text.contains("Fold a paper crane."),
        "passive form body discarded"
    );
    assert_eq!(page.links.len(), 1);
    assert_eq!(page.links[0].raw, "/po/thread/123");
    assert!(!page.text.contains("Secret") && !page.text.contains("Delete"));
    assert!(page.images.is_empty());
    assert!(page.warnings.iter().any(|w| w.contains("scripts/forms")));
}
#[test]
fn transparent_forms_preserve_crossed_ancestry_and_do_not_consume_depth() {
    for html in [
        format!("{}<p>Read</p>", "<form>".repeat(100)),
        "<div><form><p>Read</div>Tail</form><p>After</p>".to_owned(),
        "<form/><p>Read</p></form>".to_owned(),
    ] {
        let page = parse(&html);
        assert!(page.text.contains("Read"));
        if html.contains("Tail") {
            assert!(page.text.contains("Tail") && page.text.contains("After"));
        }
    }
}
#[test]
fn excluded_ancestry_remains_fail_closed_with_transparent_outer_form() {
    for hidden in [
        "<form><template><form></template><a href='/hidden'>Hidden</a></form></template></form>",
        "<form><button></form><a href='/hidden'>Hidden</a></button>",
        "<form><textarea/><a href='/hidden'>Hidden</a></textarea></form>",
        "<form><script>const x = '<form><a href=/hidden>Hidden</a>';</script></form>",
    ] {
        let page = parse(&format!(
            "<p>Before</p>{hidden}<p>After</p><a href='/ok'>Okay</a>"
        ));
        assert!(!page.text.contains("Hidden"));
        assert!(page.text.contains("Before") && page.text.contains("After"));
        assert_eq!(page.links.len(), 1);
        assert_eq!(page.links[0].raw, "/ok");
    }
    // Malformed crossed or unclosed controls deliberately over-suppress rather
    // than leaking prefilled data. This projector is not a browser DOM.
    let page = parse("<p>Before</p><button><select></button>Hidden");
    assert_eq!(page.text, "Before");
}
#[test]
fn form_links_continue_and_reverse_replay_without_controls() {
    let mut html = "<form action='/delete'><p>Public header</p>".to_owned();
    for i in 0..65 {
        html.push_str(&format!(
            "<a href='/po/thread/{i}'>Thread {i}</a><button>Delete</button>"
        ));
    }
    html.push_str("<p>Public tail</p></form>");
    let first = parse(&html);
    assert_eq!(first.links.len(), 64);
    let second = first.next_window(None).unwrap().unwrap();
    assert_eq!(second.links.len(), 1);
    assert_eq!(second.links[0].raw, "/po/thread/64");
    assert!(second.text.contains("Public tail"));
    assert!(second.next_window(None).unwrap().is_none());
    let replay = second.previous_window(None).unwrap().unwrap();
    assert_eq!(replay.text, first.text);
    assert_eq!(replay.links, first.links);
    assert_eq!(replay.window, first.window);
    assert!(std::sync::Arc::ptr_eq(
        first.source.as_ref().unwrap(),
        second.source.as_ref().unwrap()
    ));
    for page in [&first, &second, &replay] {
        assert!(!page.text.contains("Delete"));
    }
}
#[test]
fn former_whole_form_exclusions_now_keep_passive_content() {
    for (html, raw) in [
        (
            "<p>Visible<form><p>Hidden<a href='/submit'>Hidden</a></form>",
            Some("/submit"),
        ),
        ("<p>Visible</p><form><form><p>Hidden</form>", None),
        (
            "<p>Visible</p><form/><a href='/hidden'>Hidden</a></form>",
            Some("/hidden"),
        ),
    ] {
        let page = parse(&format!("{html}<p>After</p>"));
        assert!(
            page.text.contains("Visible")
                && page.text.contains("Hidden")
                && page.text.contains("After")
        );
        assert_eq!(page.links.len(), usize::from(raw.is_some()));
        if let Some(raw) = raw {
            assert_eq!(page.links[0].raw, raw);
        }
    }
}
#[test]
fn crossed_anchor_end_inside_control_does_not_extend_visible_link_label() {
    let page = parse("<p>Before</p><a href='/ok'>Read<button></a>Delete</button>Tail");
    assert_eq!(page.links.len(), 1);
    assert_eq!(page.links[0].label, "Read");
    assert!(page.text.contains("Tail") && !page.text.contains("Delete"));
}
