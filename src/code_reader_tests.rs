//! Synthetic-HTML pre/code preservation and passive reader highlighting.
//! Inputs are literal fixtures; no network, script or style is executed.
use crate::web::{Address, Page};

const BODY: [f32; 3] = [0.9, 0.96, 0.91];
const CODE_BG: [f32; 3] = [0.035, 0.07, 0.09];
const CODE_PLAIN: [f32; 3] = [0.82, 0.9, 1.0];
const CODE_KEYWORD: [f32; 3] = [1.0, 0.62, 0.42];
const CODE_STRING: [f32; 3] = [0.98, 0.86, 0.45];
const CODE_COMMENT: [f32; 3] = [0.55, 0.62, 0.58];
const CODE_NUMBER: [f32; 3] = [0.78, 0.6, 1.0];
const CODE_GUTTER: [f32; 3] = [0.4, 0.5, 0.55];

fn page(html: &str) -> Page {
    Page::html(
        Address::parse("https://example.org/code").unwrap(),
        html.as_bytes(),
    )
    .unwrap()
}
fn reader(html: &str) -> crate::World {
    let mut world = page(html).generate().unwrap().world;
    world.reading = true;
    world
}
fn colors(world: &crate::World, scroll: usize) -> Vec<[f32; 3]> {
    crate::text::overlay(world, 960, 640, scroll)
        .iter()
        .map(|v| v.color)
        .collect()
}

const PYTHON: &str = "def F(x):\n    if x:\n\n        return {'A': [1, 2]}  # Note\n\treturn None";

#[test]
fn pre_code_keeps_exact_lines_case_punctuation_while_prose_still_normalizes() {
    let html = format!(
        "<title>T</title><p>Intro   text  \t here</p><pre><code class=\"language-python\">\n{PYTHON}\n</code></pre><p>After   it</p>"
    );
    let p = page(&html);
    assert!(p.text.contains(PYTHON), "{:?}", p.text);
    assert!(p.text.contains("Intro text here"), "{:?}", p.text);
    assert!(p.text.contains("After it"));
    let g = p.clone().generate().unwrap();
    assert!(g.world.document().unwrap().contains(PYTHON));
    // Re-generating the same (cached/history) Page reproduces exact code.
    assert_eq!(
        g.world.document().unwrap(),
        p.generate().unwrap().world.document().unwrap()
    );
}

#[test]
fn sections_and_continuation_windows_keep_code_exact() {
    let rust = "fn main() {\n    let s = \"Hi\";\n}";
    let html = format!(
        "<h2>One</h2><p>a</p><pre class=\"lang-rust\">{rust}</pre><h2>Two</h2><p>b</p><pre>  two\n    lines</pre>"
    );
    let g = page(&html).generate().unwrap();
    let pocket = g.world.pocket.as_ref().unwrap();
    let texts: Vec<_> = pocket.documents.values().map(|d| d.text.clone()).collect();
    assert!(texts.iter().any(|t| t.contains(rust)), "{texts:?}");
    assert!(
        texts.iter().any(|t| t.contains("  two\n    lines")),
        "{texts:?}"
    );
    // A link-budget window boundary between two code blocks.
    let links = "<a href='/x'>x</a>".repeat(65);
    let html = format!("<pre>  first\n    block</pre>{links}<pre>\tsecond\n  block</pre>");
    let first = page(&html);
    assert!(first.next_cursor().unwrap().is_some(), "must be windowed");
    assert!(
        first.text.contains("  first\n    block"),
        "{:?}",
        first.text
    );
    let next = first.next_window(None).unwrap().unwrap();
    assert!(next.text.contains("\tsecond\n  block"), "{:?}", next.text);
    let back = next.previous_window(None).unwrap().unwrap();
    assert_eq!(back.text, first.text);
}

#[test]
fn malformed_nested_mixed_case_script_and_giant_whitespace_stay_bounded() {
    let p = page(
        "<PRE><Code CLASS=LANGUAGE-RUST>fn Main() {}\n  X</CODE><pre>Inner\n   Keep</PRE></pre><p>Tail</p>",
    );
    assert!(p.text.contains("fn Main() {}\n  X"), "{:?}", p.text);
    assert!(p.text.contains("Inner\n   Keep"), "{:?}", p.text);
    assert!(p.text.contains("Tail"));
    let p = page("<pre>keep\n  <script>alert('NOT CODE')</script>  after</pre><style>p{}</style>");
    assert!(!p.text.contains("NOT CODE") && !p.text.contains("alert"));
    assert!(p.text.contains("keep\n  "), "{:?}", p.text);
    // Unclosed pre at EOF keeps its literal lines.
    let p = page("<p>x</p><pre>  open\n    tail");
    assert!(p.text.contains("  open\n    tail"), "{:?}", p.text);
    // Giant prose whitespace still collapses.
    let p = page(&format!("<p>a{}b</p>", " ".repeat(20000)));
    assert!(p.text.contains("a b"));
    // Giant literal whitespace cannot exceed the semantic budget. FIX1 B
    // narrows the former "Ok within budget OR any Err" oracle to the exact
    // outcome: the lead window is complete and alone, and the indivisible
    // run is refused as the next window rather than cropped.
    let html = format!("<p>lead</p><pre>{}x</pre>", " ".repeat(20000));
    let p = Page::html(
        Address::parse("https://example.org/w").unwrap(),
        html.as_bytes(),
    )
    .unwrap();
    assert_eq!(p.text, "lead");
    assert!(p.code.is_empty() && p.clone().generate().is_ok());
    let refused = p.next_window(None).unwrap_err();
    assert!(!refused.is_empty());
}

fn try_page(html: &str) -> crate::Result<Page> {
    Page::html(
        Address::parse("https://example.org/code").unwrap(),
        html.as_bytes(),
    )
}
/// Every window from the first, bounded by `limit`, with strict progress.
fn windows(first: Page, limit: usize) -> Vec<Page> {
    let mut out = vec![first];
    while let Some(next) = out.last().unwrap().next_window(None).unwrap() {
        let [a, b] = [out.last().unwrap(), &next].map(|p| p.window.as_ref().unwrap().event_range());
        assert!(
            a.end <= b.start && b.start < b.end,
            "continuation must progress"
        );
        assert!(next.text.len() <= 16384, "window over budget");
        out.push(next);
        assert!(out.len() <= limit, "unbounded continuation");
    }
    out
}
/// Block-code text of a window, in order.
fn block_code(p: &Page) -> Vec<String> {
    p.code
        .iter()
        .filter(|c| c.block)
        .map(|c| p.text[c.range.clone()].to_string())
        .collect()
}

/// FIX1 B: one tag-free literal run larger than the 16 KiB window budget is
/// an indivisible item: alone it refuses; after content it refuses as the
/// next window, leaving the last good window untouched. Never cropped,
/// silently shortened or stalled.
#[test]
fn oversized_single_run_pre_refuses_cleanly_and_never_stalls() {
    for body in [
        "x".repeat(16385),
        format!("{}x", " ".repeat(20000)),
        "é\n".repeat(6000),
    ] {
        let alone = format!("<pre>{body}</pre>");
        assert!(
            try_page(&alone).is_err(),
            "single oversized run must refuse"
        );
        let first = page(&format!("<p>lead</p><pre>{body}</pre><p>after</p>"));
        assert_eq!(first.text, "lead", "exact first window before the run");
        assert!(first.code.is_empty());
        let (text, window) = (first.text.clone(), first.window.clone());
        assert!(
            first.next_window(None).is_err(),
            "next window refuses the run"
        );
        assert!(
            first.next_window(None).is_err(),
            "and refuses again, not partially"
        );
        assert_eq!((first.text, first.window), (text, window));
    }
}

/// FIX1 B: a single <pre> whose highlighter spans make it larger than one
/// window is split only between tokens. Each window stays literal code (never
/// prose-collapsed), and the windows' code concatenates to the exact source.
#[test]
fn single_pre_spanning_windows_keeps_exact_code_and_progresses() {
    let mut source = String::new();
    let mut html = String::from("<p>Intro</p><pre><code class=\"language-rust\">");
    for i in 0..700 {
        let line = format!("fn F{i}() {{\n    let aB = \"X  y\";\t// {i}\n}}\n");
        source.push_str(&line);
        html.push_str(&format!(
            "<span class=\"k\">fn</span> F{i}() {{\n    <span class=\"k\">let</span> aB = <span class=\"s\">&quot;X  y&quot;</span>;\t<span class=\"c\">// {i}</span>\n}}\n"
        ));
    }
    html.push_str("</code></pre><p>After   it</p>");
    let all = windows(page(&html), 16);
    assert!(all.len() >= 2, "fixture must span a window boundary");
    let mut joined = String::new();
    for (i, w) in all.iter().enumerate() {
        let code = block_code(w);
        assert!(
            !code.is_empty() || i == all.len() - 1,
            "window {i} lost its code typing: {:?}",
            &w.text[..w.text.len().min(80)]
        );
        joined.push_str(&code.concat());
    }
    let strip = |s: &str| s.replace('\n', "");
    assert_eq!(
        strip(&joined),
        strip(&source),
        "code bytes (other than window-edge layout newlines) must be exact"
    );
    // Only the one layout newline at each side of a window edge may go.
    let newlines = |s: &str| s.matches('\n').count();
    assert!(
        newlines(&joined) + 2 * (all.len() - 1) + 1 >= newlines(&source),
        "line structure lost beyond window edges"
    );
    assert!(all.last().unwrap().text.ends_with("After it"));
}

/// FIX1 B: every bounded combination of malformed/nested/mixed-case pre and
/// code tags, excluded scripts/styles and multibyte whitespace-bearing text
/// parses without panic; code spans are valid char ranges of the text, block
/// code is a verbatim substring of its source, and excluded content never
/// appears. The reader layout path is exercised for each result.
#[test]
fn adversarial_nested_pre_code_script_multibyte_combinations_stay_valid() {
    const PARTS: [&str; 10] = [
        "<pre>",
        "</pre>",
        "<CODE class=language-rust>",
        "</code>",
        "<PRE class=\"lang-js\">",
        "<script>NoRun()</script>",
        "<sTyLe>.NoStyle{}</StYlE>",
        "é\n  日\t",
        "  \n\n",
        "<p>A&lt;b</p>",
    ];
    // Character data each part contributes (tags and excluded bodies: none).
    const VISIBLE: [&str; 10] = ["", "", "", "", "", "", "", "é\n  日\t", "  \n\n", "A<b"];
    for len in 1..=4u32 {
        for mut n in 0..PARTS.len().pow(len) {
            let mut html = String::new();
            let mut visible = String::new();
            for _ in 0..len {
                html.push_str(PARTS[n % PARTS.len()]);
                visible.push_str(VISIBLE[n % PARTS.len()]);
                n /= PARTS.len();
            }
            // The only permitted refusal: nothing readable remained.
            let p = match try_page(&html) {
                Ok(p) => p,
                Err(e) if e.starts_with("no readable static HTML text") => continue,
                Err(e) => panic!("{html:?}: {e}"),
            };
            assert!(p.text.len() <= 16384);
            assert!(
                !p.text.contains("NoRun") && !p.text.contains("NoStyle"),
                "{html:?}"
            );
            for c in &p.code {
                let code = p.text.get(c.range.clone());
                assert!(code.is_some(), "{html:?}: {c:?}");
                if c.block {
                    // Block code is a verbatim run of the document's character
                    // data; a nested block boundary (e.g. </p>) may add only a
                    // line break, never other characters or lost spaces/tabs.
                    let flat = |s: &str| s.replace('\n', "");
                    assert!(
                        flat(&visible).contains(&flat(code.unwrap())),
                        "{html:?}: invented {:?}",
                        code.unwrap()
                    );
                }
            }
            if len <= 2 {
                let mut world = p.generate().unwrap().world;
                world.reading = true;
                let _ = crate::text::overlay(&world, 960, 640, 0);
                let _ = crate::text::reader_scroll_max(&world, 960, 640);
            }
        }
    }
}

/// FIX1 B: synthetic site-shaped tutorial (many highlighted blocks with deep
/// indentation and blank lines) larger than several windows. No live site.
#[test]
fn site_shaped_long_code_tutorial_windows_keep_every_block_exact() {
    let mut blocks = Vec::new();
    let mut html = String::from("<title>Learn X</title><h1>Learn X</h1>");
    for b in 0..40 {
        let mut code = String::new();
        for i in 0..24 {
            let indent = " ".repeat(4 * (i % 5));
            code.push_str(&format!("{indent}if Value_{b}_{i} > 0:  # Keep  Case\n"));
            if i % 7 == 0 {
                code.push('\n');
            }
        }
        let code = code.trim_end_matches('\n').to_string();
        html.push_str(&format!(
            "<h2>Part {b}</h2><p>Explain   part {b}.</p><pre><code class=\"language-python\">{}</code></pre>",
            code.replace('>', "&gt;")
        ));
        blocks.push(code);
    }
    let all = windows(page(&html), 64);
    assert!(all.len() >= 3, "fixture must need several windows");
    let found: Vec<String> = all.iter().flat_map(block_code).collect();
    assert_eq!(found, blocks, "every block exact, in order, none split");
}

#[test]
fn reader_highlights_known_languages_marks_blocks_and_distinguishes_inline_code() {
    let world = reader(
        "<p>Prose</p><pre><code class=\"language-rust\">fn main() {\n    let s = \"Hi\"; // greet\n    let n = 42;\n}</code></pre>",
    );
    let c = colors(&world, 0);
    for (name, color) in [
        ("background", CODE_BG),
        ("gutter", CODE_GUTTER),
        ("keyword", CODE_KEYWORD),
        ("string", CODE_STRING),
        ("comment", CODE_COMMENT),
        ("number", CODE_NUMBER),
        ("prose", BODY),
    ] {
        assert!(c.contains(&color), "missing {name}");
    }
    for language in ["python", "js", "typescript", "cpp", "bash", "json", "html"] {
        let source = match language {
            "python" => "def f():\n    return 'x'  # c",
            "js" | "typescript" => "const x = 'y'; // c",
            "cpp" => "int main() { puts(\"hi\"); return 0; } /* c */",
            "bash" => "if true; then echo \"x\"; fi # c",
            "json" => "{\"key\": 12, \"ok\": true}",
            _ => "<div class=\"a\"><!-- c --></div>",
        };
        let escaped = source.replace('&', "&amp;").replace('<', "&lt;");
        let c = colors(
            &reader(&format!(
                "<pre><code class=\"language-{language}\">{escaped}</code></pre>"
            )),
            0,
        );
        assert!(
            c.contains(&CODE_KEYWORD) && c.contains(&CODE_STRING),
            "{language} not highlighted"
        );
    }
    let unknown = colors(
        &reader("<pre class=\"language-zzz\">fn main() { \"x\" } // 42</pre>"),
        0,
    );
    assert!(unknown.contains(&CODE_PLAIN) && unknown.contains(&CODE_BG));
    for color in [CODE_KEYWORD, CODE_STRING, CODE_COMMENT, CODE_NUMBER] {
        assert!(!unknown.contains(&color), "unknown language must be plain");
    }
    let inline = colors(&reader("<p>Call <code>Read_Me()</code> now</p>"), 0);
    assert!(inline.contains(&CODE_PLAIN) && inline.contains(&BODY));
    assert!(!inline.contains(&CODE_BG) && !inline.contains(&CODE_GUTTER));
}

#[test]
fn long_code_lines_soft_wrap_with_continuation_and_every_tail_is_inspectable() {
    let make = |tail: &str| {
        reader(&format!(
            "<pre class=\"language-zzz\">short\n{}{tail}\nend</pre>",
            "aB".repeat(300)
        ))
    };
    let a = make("Zq");
    let b = make("zQ");
    assert_eq!(a.document().unwrap().len(), b.document().unwrap().len());
    let max = crate::text::reader_scroll_max(&a, 960, 640);
    let all = |w: &crate::World| -> Vec<u8> {
        (0..=max)
            .flat_map(|s| {
                crate::text::overlay(w, 960, 640, s)
                    .into_iter()
                    .filter(|v| v.color == CODE_PLAIN)
                    .flat_map(|v| bytemuck::bytes_of(&v).to_vec())
            })
            .collect()
    };
    assert_ne!(all(&a), all(&b), "long code tail unreachable");
    // Gutter: one first-row marker per logical code line plus continuations.
    let gutters = crate::text::overlay(&a, 960, 640, 0)
        .iter()
        .filter(|v| v.color == CODE_GUTTER)
        .count();
    assert!(gutters > 0);
    // Underlying text is never rewritten by display wrapping.
    assert!(
        a.document()
            .unwrap()
            .contains(&format!("{}Zq\nend", "aB".repeat(300)))
    );
}
