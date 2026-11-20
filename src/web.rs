//! Bounded read-only web page adapter; no directory inventory or executable behavior.
use crate::Result;
mod generate;
mod html;
mod pocket;
mod session;
pub use session::{
    HTML_SOURCE_POOL_BYTES, HTML_SOURCE_POOL_SESSIONS, HtmlSession, WindowCursor, WindowPosition,
};
/// Optional cooperative preparation control. No worker or acquisition is started here.
pub type WindowControl<'a> = Option<(&'a crate::acquisition::Cancellation, std::time::Instant)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowLimit {
    Text,
    Links,
    Images,
    Headings,
    Title,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completeness {
    Complete,
    /// A semantic callback boundary under policy v1, NOT a raw-byte cursor.
    /// Kept under its original name for compatibility; also used by later windows.
    /// Continuation authority comes only from the opaque WindowPosition.
    FirstWindow {
        limit: WindowLimit,
        before_event: usize,
    },
}
impl Completeness {
    pub fn disclosure(self) -> Option<&'static str> {
        match self {
            Self::Complete => None,
            Self::FirstWindow { .. } => Some(
                "incomplete window: more content exists in the held source; continuation prepares another bounded room without fetching destinations",
            ),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Section {
    pub title: String,
    pub text: String,
}
/// Typed byte range of preserved code in one reader document. Never encoded in
/// the text itself; the reader validates ranges before styling them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeSpan {
    pub range: std::ops::Range<usize>,
    /// Normalized known language family, or None for passive plain fallback.
    pub language: Option<String>,
    /// `<pre>` block (exact lines) versus inline `<code>` inside prose.
    pub block: bool,
}
/// At most this many code spans per window; later code stays readable as plain text.
pub const MAX_CODE_SPANS: usize = 256;
#[derive(Default)]
pub struct Pocket {
    /// Shared once per response, including history; never cloned per section.
    pub source: Option<std::sync::Arc<HtmlSession>>,
    pub documents: std::collections::BTreeMap<String, PocketDocument>,
    pub reading: Option<String>,
    /// Refresh restored the full page; cleared by an explicit document Inspect.
    pub collection_recovered: bool,
    /// Most recent actual failed journey in this pocket, never a safety score.
    pub failed_arrival: Option<(String, String)>,
    /// Code ranges of the full-page reader document.
    pub code: Vec<CodeSpan>,
    /// The page the typed visitor-history return restores; None at the
    /// initial page. Display context only, never a navigation authority.
    pub previous: Option<PreviousPage>,
    /// Visitor-history trail, oldest first (at most the 8-page history).
    pub history: Vec<PreviousPage>,
    /// Exact addresses arrived at in this session (bounded, display only).
    pub visited: Vec<String>,
    /// Same held-response rooms: (room index, previous exists, next exists).
    pub room: Option<(usize, bool, bool)>,
}
/// Session visited-address bound for the navigation panel.
pub const MAX_VISITED_ADDRESSES: usize = 64;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviousPage {
    pub title: String,
    pub address: String,
}
pub struct PocketDocument {
    pub title: String,
    pub text: String,
    pub code: Vec<CodeSpan>,
}
pub mod portals;
/// Blocking acquisition seam for a CLI or a dedicated worker, NEVER a render/event callback.
pub fn fetch(
    address: Address,
    cancel: &crate::acquisition::Cancellation,
    deadline: std::time::Instant,
) -> Result<Page> {
    let source = crate::acquisition::Source {
        original: address.original.clone(),
        url: address.url.clone(),
    };
    let transport = crate::acquisition::transport::Transport::public(&source, cancel, deadline)?;
    let (bytes, mime) = transport.get_page(cancel, deadline)?;
    if mime != "text/html" {
        return Err("first tracer requires text/html; Markdown adapter pending".into());
    }
    html::extract(address, &bytes, Some((cancel, deadline)))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub original: String,
    pub url: reqwest::Url,
}
impl Address {
    pub fn parse(raw: &str) -> Result<Self> {
        if raw.is_empty()
            || raw.len() > 2048
            || !raw.starts_with("https://")
            || !raw.is_ascii()
            || raw
                .bytes()
                .any(|b| b.is_ascii_control() || b == b' ' || b == b'\\')
        {
            return Err("literal ASCII HTTPS page required (no whitespace/backslashes)".into());
        }
        let bytes = raw.as_bytes();
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'%'
                && (i + 2 >= bytes.len()
                    || !bytes[i + 1].is_ascii_hexdigit()
                    || !bytes[i + 2].is_ascii_hexdigit())
            {
                return Err("malformed percent escape".into());
            }
        }
        let url = reqwest::Url::parse(raw).map_err(|e| e.to_string())?;
        let authority = raw[8..].split(['/', '?', '#']).next().unwrap_or("");
        if authority.contains('@')
            || !url.username().is_empty()
            || url.password().is_some()
            || url.as_str() != raw
        {
            return Err(
                "credentials or implicit URL normalization refused; supply literal serialized URL"
                    .into(),
            );
        }
        // Reuse only the hostname syntax policy, never the directory/path policy.
        crate::acquisition::Source::parse(&format!(
            "https://{}/",
            url.host_str().ok_or("missing hostname")?
        ))?;
        Ok(Self {
            original: raw.into(),
            url,
        })
    }
}
#[derive(Clone, Debug)]
pub struct Page {
    /// Opaque session-bound room position; None for manually constructed pages.
    pub window: Option<WindowPosition>,
    pub completeness: Completeness,
    /// None is permitted for manually constructed pages; parsed pages retain source.
    pub source: Option<std::sync::Arc<HtmlSession>>,
    pub sections: Vec<Section>,
    pub address: Address,
    pub title: String,
    pub text: String,
    pub headings: Vec<String>,
    pub links: Vec<crate::content::Link>,
    pub images: Vec<crate::content::Link>,
    pub warnings: Vec<String>,
    /// Code ranges in `text`.
    pub code: Vec<CodeSpan>,
    /// Code ranges per entry of `sections`, index-aligned; missing means none.
    pub section_code: Vec<Vec<CodeSpan>>,
}
/// The full displayed selected target, validated against current source links.
pub fn door_destination(world: &crate::World, anchor: &str) -> Option<String> {
    portals::view(world, anchor).map(|v| v.destination)
}
pub struct Generated {
    pub page: Page,
    pub world: crate::World,
    pub host: crate::host_scene::HostScene,
    pub doors: Vec<Door>,
    /// Same-response actions, deliberately separate from network-link Doors.
    /// App must retain and dispatch these bindings; E integration is not supplied here.
    pub window_doors: Vec<WindowDoor>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowAction {
    Next(WindowCursor),
    Previous(WindowCursor),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowDoor {
    pub anchor: String,
    pub label: String,
    pub action: WindowAction,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Door {
    pub anchor: String,
    pub resource: String,
    pub destinations: Vec<String>,
    pub cross_origin: bool,
}
impl Page {
    pub fn generate(self) -> Result<Generated> {
        generate::build(self)
    }
    fn position(&self) -> Result<Option<&WindowPosition>> {
        match (&self.source, &self.window) {
            (Some(source), Some(position)) => {
                position.validate(source, &self.address, self.completeness)?;
                Ok(Some(position))
            }
            (None, None) if self.completeness == Completeness::Complete => Ok(None),
            _ => Err("HTML window/source metadata missing or mismatched".into()),
        }
    }
    pub fn next_cursor(&self) -> Result<Option<WindowCursor>> {
        Ok(self.position()?.and_then(WindowPosition::next))
    }
    pub fn previous_cursor(&self) -> Result<Option<WindowCursor>> {
        Ok(self.position()?.and_then(WindowPosition::previous))
    }
    /// Prepare only the requested room from the same response. On any failure
    /// self remains untouched; the caller must keep its last good room installed.
    pub fn next_window(&self, control: WindowControl<'_>) -> Result<Option<Self>> {
        self.prepare(self.next_cursor()?, control)
    }
    pub fn previous_window(&self, control: WindowControl<'_>) -> Result<Option<Self>> {
        self.prepare(self.previous_cursor()?, control)
    }
    fn prepare(
        &self,
        cursor: Option<WindowCursor>,
        control: WindowControl<'_>,
    ) -> Result<Option<Self>> {
        if let Some((cancel, deadline)) = control {
            cancel.check(deadline)?;
        }
        cursor
            .map(|c| {
                self.source
                    .as_ref()
                    .ok_or("missing held HTML source")?
                    .window(&c, control)
            })
            .transpose()
    }
    /// Readable room/range metadata, separate from literal HTTPS identity.
    /// Complete single-room pages intentionally retain their original reader bytes.
    pub fn window_disclosure(&self) -> Result<Option<String>> {
        Ok(self.position()?.and_then(WindowPosition::disclosure))
    }
    /// Validate the full bounded UTF-8 input and collect its first semantic window.
    /// Oversized indivisible items refuse rather than publish cropped content.
    pub fn html(address: Address, bytes: &[u8]) -> Result<Self> {
        html::extract(address, bytes, None)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_world_has_directed_doors_not_folder_parents_and_pack_owns_no_routes() {
        let raw = "https://example.org/no/folder/ancestry?q=AB%2Fcd#top";
        let html=b"<title>One</title><h1>Content</h1><a href='#a'>A</a><a href='#b'>B</a><a href='#a'>Again</a><a href='https://other.example/two'>Second origin</a>";
        let g = Page::html(Address::parse(raw).unwrap(), html)
            .unwrap()
            .generate()
            .unwrap();
        assert_eq!(g.world.current, raw);
        assert!(g.world.document().unwrap().contains("Content"));
        assert_eq!(g.world.graph.pages.len(), 1);
        assert_eq!(g.world.reader_links().len(), 4);
        assert_eq!(g.doors.len(), 2);
        assert_eq!(
            g.doors.iter().map(|d| d.destinations.len()).sum::<usize>(),
            3
        );
        assert_eq!(g.doors.iter().filter(|d| d.cross_origin).count(), 1);
        assert!(g.host.anchors.iter().any(|a| a.id == "return-history"));
        assert!(g.host.anchors.iter().filter(|a| a.role == "portal").count() >= 3);
        let repeat = Page::html(Address::parse(raw).unwrap(), html)
            .unwrap()
            .generate()
            .unwrap();
        assert_eq!(g.host, repeat.host);
        assert_eq!(g.doors, repeat.doors);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut cache = crate::theme::AssetCache::default();
        for pack in ["packs/office.toml", "packs/archive.toml"] {
            let theme =
                crate::theme::Theme::load_for_scene(&root.join(pack), &mut cache, &g.host).unwrap();
            assert_eq!(theme.host, g.host);
            assert!(!theme.vertices_for(&g.world).is_empty());
            assert_eq!(g.world.current, raw);
            assert_eq!(g.doors, repeat.doors);
        }
    }
    #[test]
    fn planner_refuses_over_budget_pages_even_if_constructed_outside_parser() {
        let mut p = Page::html(
            Address::parse("https://example.org/a").unwrap(),
            b"<p>One</p><a href='/a'>Cycle</a>",
        )
        .unwrap();
        p.links = vec![p.links[0].clone(); 65];
        assert!(p.generate().is_err());
    }
    #[test]
    fn generated_cycles_relative_query_fragments_and_eight_door_limit() {
        let mut html = String::from(
            "<p>Semantic text</p><a href='./one?q=AB%2Fcd#x'>cycle</a><a href='#y'>fragment</a><a href='https://second.example/two'>other</a>",
        );
        for i in 0..12 {
            html.push_str(&format!("<a href='/p{i}'>P{i}</a>"));
        }
        let page = Page::html(
            Address::parse("https://example.org/one?q=AB%2Fcd#x").unwrap(),
            html.as_bytes(),
        )
        .unwrap();
        assert_eq!(page.links.len(), 15);
        assert_eq!(
            page.links[0].destination,
            crate::content::Destination::External("https://example.org/one?q=AB%2Fcd#x".into())
        );
        let generated = page.generate().unwrap();
        assert!(generated.doors.len() <= 8);
        assert_eq!(
            generated
                .doors
                .iter()
                .map(|d| d.destinations.len())
                .sum::<usize>(),
            15
        );
        let repeated = format!("<p>read</p>{}", "<a href='/a'>a</a>".repeat(65));
        let first = Page::html(
            Address::parse("https://example.org/a").unwrap(),
            repeated.as_bytes(),
        )
        .unwrap();
        assert_eq!(first.links.len(), 64);
        assert!(matches!(
            first.completeness,
            Completeness::FirstWindow {
                limit: WindowLimit::Links,
                ..
            }
        ));
        let held = first.source.as_ref().unwrap().clone();
        assert_eq!(held.encoded(), repeated.as_bytes());
        let cursor = first.next_cursor().unwrap().unwrap();
        assert_eq!(cursor.index(), 1);
        let g = first.clone().generate().unwrap();
        assert_eq!(g.window_doors.len(), 1);
        assert_eq!(g.window_doors[0].anchor, "web-next-window");
        assert_eq!(g.window_doors[0].action, WindowAction::Next(cursor.clone()));
        assert!(
            g.host
                .anchors
                .iter()
                .any(|a| a.id == "web-next-window" && a.binding == "explicit-door")
        );
        assert_eq!(g.host.portal_inward("web-next-window"), [-1.0, 0.0]);
        let last = held.window(&cursor, None).unwrap();
        assert_eq!(last.links.len(), 1);
        assert_eq!(last.links[0].raw, "/a");
        assert_eq!(last.links[0].label, "a");
        assert_eq!(
            last.links[0].destination,
            crate::content::Destination::External("https://example.org/a".into())
        );
        assert_eq!(last.completeness, Completeness::Complete);
        assert!(last.next_cursor().unwrap().is_none());
        assert!(std::sync::Arc::ptr_eq(last.source.as_ref().unwrap(), &held));
        let previous = last.previous_cursor().unwrap().unwrap();
        let g = last.clone().generate().unwrap();
        assert_eq!(g.window_doors.len(), 1);
        assert_eq!(g.window_doors[0].action, WindowAction::Previous(previous));
        assert!(g.world.document().unwrap().contains("EOF"));
        let back = last.previous_window(None).unwrap().unwrap();
        assert_eq!(back.window, first.window);
        assert_eq!(back.completeness, first.completeness);
        assert_eq!(back.address, first.address);
        assert_eq!(back.title, first.title);
        assert_eq!(back.text, first.text);
        assert_eq!(back.headings, first.headings);
        assert_eq!(back.links, first.links);
        assert_eq!(back.images, first.images);
        assert_eq!(
            back.sections
                .iter()
                .map(|s| (&s.title, &s.text))
                .collect::<Vec<_>>(),
            first
                .sections
                .iter()
                .map(|s| (&s.title, &s.text))
                .collect::<Vec<_>>()
        );
        assert!(std::sync::Arc::ptr_eq(back.source.as_ref().unwrap(), &held));
        for html in [
            format!("<p>{}</p>", "x".repeat(16385)),
            format!("{}text{}", "<div>".repeat(65), "</div>".repeat(65)),
            format!("<p>read</p>{}", "<br>".repeat(8193)),
        ] {
            assert!(
                Page::html(
                    Address::parse("https://example.org/a").unwrap(),
                    html.as_bytes()
                )
                .is_err()
            );
        }
    }
    #[test]
    fn ordinary_html_extracts_semantics_and_directed_links_without_companion() {
        let p = Page::html(Address::parse("https://example.org/articles/One?q=1#top").unwrap(), br#"<!doctype html><html><head><title>Small &amp; plain</title><style>.hidden { content: 'NOT BODY' }</style></head><body><h1>Read me</h1><p>Ordinary <b>HTML</b> text.</p><img src='/pic.png?rev=1' alt='Green leaf'><a href='../two?token=AB%2Fcd#part'>Next &amp; more</a><a href='#other'>Here</a><a href='https://second.example/landing'>Elsewhere</a><script>let x = '<a href="/evil">NOT BODY</a>';</script></body></html>"#).unwrap();
        assert_eq!(p.title, "Small & plain");
        assert_eq!(p.headings, ["Read me"]);
        assert!(p.text.contains("Ordinary HTML text."), "{}", p.text);
        assert!(p.text.contains("Green leaf"));
        assert!(!p.text.contains("NOT BODY"));
        assert_eq!(p.links.len(), 3);
        assert_eq!(p.links[0].raw, "../two?token=AB%2Fcd#part");
        assert_eq!(
            p.links[0].destination,
            crate::content::Destination::External(
                "https://example.org/two?token=AB%2Fcd#part".into()
            )
        );
        assert_eq!(p.links[0].label, "Next & more");
        assert_eq!(
            p.images[0].destination,
            crate::content::Destination::Image("https://example.org/pic.png?rev=1".into())
        );
        assert!(p.warnings.iter().any(|s| s.contains("scripts")));
    }
    #[test]
    fn literal_page_identity_keeps_query_and_fragment_without_directory_rules() {
        let raw = "https://example.org/Articles/one.html?token=ABC%2Fdef&x=1#part";
        let a = Address::parse(raw).unwrap();
        assert_eq!(a.original, raw);
        assert_eq!(a.url.as_str(), raw);
        for bad in [
            " https://example.org/",
            "https://u:p@example.org/",
            "http://example.org/",
            "https://127.0.0.1/",
            "https://localhost/",
            "https://example.org/a\\b",
            "https://example.org/a%zz",
            "https://example.org/a/../b",
            "https://EXAMPLE.org/",
            "https://example.org:443/",
        ] {
            assert!(Address::parse(bad).is_err(), "{bad}");
        }
    }
}
