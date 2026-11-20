//! R1 file-only regression preparation; parent owns compilation and execution.
use hair::{
    content::Destination,
    web::{Address, Completeness, Page},
};
use std::sync::Arc;

fn parse(input: &str) -> hair::Result<Page> {
    Page::html(
        Address::parse("https://example.org/Whitespace?Case=AB%2Fcd#Tail")?,
        input.as_bytes(),
    )
}

#[test]
fn scalar_feeds_preserve_interword_title_heading_and_anchor_whitespace() {
    let p = parse("<title> \tExact&nbsp;title&#32;&amp;\n words </title><p> Before<span> \t</span>after&nbsp;<a href='/Exact?Case=AB%2Fcd#Tail'> A&#32;<b>&amp;</b>&nbsp;B </a> end </p><h2> Heading&nbsp;<i>two</i> words </h2><p>Body\r\nnext\t word</p>").unwrap();
    assert_eq!(p.title, "Exact title & words");
    assert_eq!(
        p.text,
        "Before after A & B end\nHeading two words\nBody\nnext word"
    );
    assert_eq!(p.headings, ["Heading two words"]);
    assert_eq!(p.links.len(), 1);
    assert_eq!(p.links[0].label, "A & B");
    assert_eq!(p.links[0].raw, "/Exact?Case=AB%2Fcd#Tail");
    assert_eq!(
        p.links[0].destination,
        Destination::External("https://example.org/Exact?Case=AB%2Fcd#Tail".into())
    );
    assert_eq!(p.sections.len(), 1);
    assert_eq!(p.sections[0].title, "Heading two words");
    assert_eq!(p.sections[0].text, "Heading two words\nBody\nnext word");
    assert_eq!(p.completeness, Completeness::Complete);
}

#[test]
fn normalization_only_runs_do_not_consume_a_room_or_title_allowance() {
    let gap = " \t\n&nbsp;&#32;".repeat(2000);
    let input = format!(
        "<title>{gap}Exact{gap}title{gap}</title><p>{gap}{}{gap}</p>{gap}",
        "x".repeat(16384)
    );
    let p = parse(&input).unwrap();
    assert_eq!(p.title, "Exact title");
    assert_eq!(p.text, "x".repeat(16384));
    assert_eq!(p.completeness, Completeness::Complete);
    assert!(p.next_cursor().unwrap().is_none());
}

#[test]
fn normalized_pending_separators_and_lengths_rollback_with_a_whole_heading() {
    let gap = " \t&nbsp;&#32;".repeat(1800);
    let input = format!(
        "<p>{}</p>{gap}<h2>{gap}Café&nbsp;<a href='/whole'> &amp; <b>whole</b>&#32;words{gap}</a>{gap}</h2><p>Tail words</p>",
        "x".repeat(16380)
    );
    let first = parse(&input).unwrap();
    assert_eq!(first.text, "x".repeat(16380));
    assert!(first.links.is_empty() && first.headings.is_empty() && first.sections.is_empty());
    let next = first.next_window(None).unwrap().unwrap();
    assert_eq!(next.text, "Café & whole words\nTail words");
    assert_eq!(next.headings, ["Café & whole words"]);
    assert_eq!(next.links.len(), 1);
    assert_eq!(next.links[0].label, "& whole words");
    assert_eq!(next.links[0].raw, "/whole");
    assert_eq!(next.sections.len(), 1);
    assert_eq!(next.sections[0].text, next.text);
    assert_eq!(next.completeness, Completeness::Complete);
    assert!(next.next_cursor().unwrap().is_none());
    assert!(Arc::ptr_eq(
        first.source.as_ref().unwrap(),
        next.source.as_ref().unwrap()
    ));
    assert!(
        first.window.as_ref().unwrap().event_range().end
            <= next.window.as_ref().unwrap().event_range().start
    );
    let restored = next.previous_window(None).unwrap().unwrap();
    assert_eq!(restored.text, first.text);
    assert_eq!(restored.window, first.window);
    assert_eq!(restored.links, first.links);
    assert_eq!(restored.headings, first.headings);
    assert!(restored.sections.is_empty());
}

#[test]
fn genuinely_oversized_normalized_items_still_refuse_without_cropping() {
    for item in [
        format!("<p>{}</p>", "x".repeat(16385)),
        format!("<a href='/big'>{} y</a>", "x".repeat(16383)),
        format!("<h2>{}&#32;y</h2>", "x".repeat(16383)),
        format!("<title>{} y</title>", "x".repeat(511)),
        format!("<p>{}</p>", "é".repeat(8193)),
    ] {
        assert!(parse(&item).is_err());
        let first = parse(&format!("<p>Last good</p>{item}")).unwrap();
        let before_text = first.text.clone();
        let before_window = first.window.clone();
        assert!(first.next_window(None).is_err());
        assert_eq!(first.text, before_text);
        assert_eq!(first.window, before_window);
    }
}
