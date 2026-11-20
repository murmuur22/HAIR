use hair::web::{Address, Page};
fn parse(bytes: &[u8]) -> hair::Result<Page> {
    Page::html(Address::parse("https://example.org/a").unwrap(), bytes)
}
#[test]
fn tokenizer_budget_controls() {
    for suffix in [">", "", " '", "=\"unterminated", "=&#x41;"] {
        let mut html = String::from("<p>Visible</p><div");
        // Duplicate attributes must also have bounded tokenization work.
        for _ in 0..20000 {
            html.push_str(" a=x");
        }
        html.push_str(suffix);
        assert!(
            parse(html.as_bytes()).is_err(),
            "unbounded malformed/duplicate tag {suffix}"
        );
    }
    let attrs: String = (0..64)
        .map(|i| format!(" a{i}=\"{}\"", "x".repeat(2048)))
        .collect();
    assert_eq!(
        parse(format!("<p{attrs}>Normal</p>").as_bytes())
            .unwrap()
            .text,
        "Normal"
    );
    for html in [
        format!("<p a='{}'>Visible</p>", "x".repeat(2049)),
        format!("<p a='{}'>Visible</p>", "&#x41;".repeat(2049)),
        format!("<p {}>Visible</p>", "z".repeat(2049)),
        format!("<!--{}--><p>Visible</p>", "x".repeat(20000)),
        format!(
            "<!DOCTYPE html PUBLIC '{}'><p>Visible</p>",
            "x".repeat(20000)
        ),
    ] {
        assert!(parse(html.as_bytes()).is_err());
    }
    assert!(parse(b"<p>Visible\xff</p>").is_err());
    for html in [
        "<!-- <p a='not a tag'> --><p>Visible &amp; normal</p>",
        "<script>const q = '<a href=\"/bad\">bad</a>';</script><p>Visible &amp; normal</p>",
        "<style>p:before{content:'<a href=/bad>'}</style><p>Visible &amp; normal</p>",
        "<p title='literal > &amp; < not markup'>Visible &amp; normal",
        "<p title=unquoted>Visible &amp; normal",
    ] {
        let p = parse(html.as_bytes()).unwrap();
        assert_eq!(p.text, "Visible & normal");
        assert!(p.links.is_empty());
    }
    // Streaming must not turn 16 KiB of ordinary text into >8192 semantic nodes.
    // Paragraph boundary newlines also charge the existing 16 KiB text budget.
    assert_eq!(
        parse(format!("<p>{}</p>", "x".repeat(16382)).as_bytes())
            .unwrap()
            .text
            .len(),
        16382
    );
}

#[test]
fn suppression_is_not_an_xml_or_optional_paragraph_stack() {
    for hidden in [
        "<div><p>Visible</p><template></div><a href='/hidden'>Hidden</a></template>",
        "<p>Visible<button><p>Hidden<a href='/submit'>Hidden</a></button>",
        "<p>Visible</p><template><template></div><a href='/hidden'>Hidden</a></template></template>",
        "<p>Visible</p><select><select><p>Hidden</select></select>",
        "<p>Visible</p><template><form></template><a href='/hidden'>Hidden</a></form></template>",
        "<p>Visible</p><template/><a href='/hidden'>Hidden</a></template>",
        "<p>Visible</p><button/><a href='/hidden'>Hidden</a></button>",
        "<p>Visible</p><template><script>let a='</template><a href=/hidden>Hidden</a>';</script></template>",
        "<p>Visible</p><template><object></template><a href='/hidden'>Hidden</a></object></template>",
    ] {
        // A crossing outer end cannot discard an inner exclusion boundary.
        let p = parse(format!("{hidden}<p>After</p><a href='/good'>Good</a>").as_bytes()).unwrap();
        assert!(!p.text.contains("Hidden"), "{hidden}: {}", p.text);
        assert!(p.text.contains("Visible"));
        assert!(
            p.text.contains("After"),
            "valid trailing content lost: {hidden}"
        );
        assert_eq!(p.links.len(), 1, "{hidden}: {:?}", p.links);
        assert_eq!(p.links[0].raw, "/good");
    }
    for html in [
        "<p>Visible<p>After<a href='/good'>Good</a>",
        "<svg/><p>Visible</p><p>After</p><a href='/good'>Good</a>",
        "<math/><p>Visible</p><p>After</p><a href='/good'>Good</a>",
        "<p>Visible</p><form><p>Hidden</p><form>Hidden</form><p>After</p><a href='/good'>Good</a>",
    ] {
        let p = parse(html.as_bytes()).unwrap();
        assert!(p.text.contains("Visible"));
        assert!(p.text.contains("After"));
        assert_eq!(p.links.len(), 1);
        assert_eq!(p.links[0].raw, "/good");
    }
}
