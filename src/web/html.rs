//! HTML5 tokenizer plus a deliberately small, bounded semantic projection, not a DOM/browser.
use super::{Address, Completeness, HtmlSession, Page, WindowLimit};
use crate::{
    Result,
    content::{Destination, Link},
};
use html5ever::tokenizer::{
    BufferQueue, StartTag, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
    states::RawKind,
};
use std::cell::RefCell;

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
// Normalize BEFORE semantic admission, independently of tokenizer chunking.
// Only a pending separator is retained: leading/trailing whitespace costs no
// room bytes, but an interword space/newline is charged with its next scalar.
// Body newlines match lines().map(clean); labels/titles collapse all whitespace.
fn append_normalized(
    text: &mut String,
    pending: &mut Option<char>,
    s: &str,
    limit: usize,
    lines: bool,
) -> bool {
    for c in s.chars() {
        if c.is_whitespace() {
            if !text.is_empty() {
                if lines && c == '\n' {
                    *pending = Some('\n');
                } else if pending.is_none() {
                    *pending = Some(' ');
                }
            }
            continue;
        }
        let needed = usize::from(pending.is_some()) + c.len_utf8();
        if needed > limit - text.len() {
            return false;
        }
        if let Some(separator) = pending.take() {
            text.push(separator);
        }
        text.push(c);
    }
    true
}
fn resolve(base: &Address, raw: &str, image: bool) -> Destination {
    if raw.len() > 2048
        || !raw.is_ascii()
        || raw
            .bytes()
            .any(|b| b.is_ascii_control() || b == b' ' || b == b'\\')
    {
        return Destination::Invalid("literal link syntax refused (raw retained)".into());
    }
    for (i, b) in raw.bytes().enumerate() {
        if b == b'%'
            && (i + 2 >= raw.len()
                || !raw.as_bytes()[i + 1].is_ascii_hexdigit()
                || !raw.as_bytes()[i + 2].is_ascii_hexdigit())
        {
            return Destination::Invalid("malformed percent escape (raw retained)".into());
        }
    }
    // Authority-bearing references must pass the same literal validator as
    // direct entry BEFORE URL join can erase empty userinfo or normalize bytes.
    // Relative paths/queries/fragments have no authority and retain URL semantics.
    let literal = if raw.starts_with("//") {
        Some(format!("https:{raw}"))
    } else if raw
        .get(..6)
        .is_some_and(|s| s.eq_ignore_ascii_case("https:"))
    {
        Some(raw.to_owned())
    } else {
        None
    };
    if let Some(literal) = literal
        && let Err(e) = Address::parse(&literal)
    {
        return Destination::Invalid(e);
    }
    let Ok(url) = base.url.join(raw) else {
        return Destination::Invalid("unresolvable link".into());
    };
    if url.scheme() != "https" {
        return Destination::Unsupported(url.to_string());
    }
    match Address::parse(url.as_str()) {
        Ok(a) if image && a.url.path().to_ascii_lowercase().ends_with(".png") => {
            Destination::Image(a.original)
        }
        Ok(a) if !image => Destination::External(a.original),
        Ok(a) => Destination::Unsupported(a.original),
        Err(e) => Destination::Invalid(e),
    }
}
// Length-only rollback: no semantic tail, Page clones or tokenizer snapshots.
// A text run between tags, complete title, anchor or heading is indivisible.
struct Checkpoint {
    text: usize,
    title: usize,
    text_separator: Option<char>,
    title_separator: Option<char>,
    links: usize,
    images: usize,
    headings: usize,
    sections: usize,
    code: usize,
    before_event: usize,
}
/// Raw-text code range before final whitespace normalization.
struct RawCode {
    range: std::ops::Range<usize>,
    language: Option<&'static str>,
    block: bool,
}
struct State {
    page: Page,
    // Constant-size deferred whitespace, never a raw/semantic tail buffer.
    text_separator: Option<char>,
    title_separator: Option<char>,
    anchor_separator: Option<char>,
    heading_separator: Option<char>,
    checkpoint: Option<Checkpoint>,
    events: usize,
    start_event: usize,
    stack: Vec<String>,
    // Exclusion ancestry is not subject to paragraph/list optional ends or
    // generic ancestor truncation. This is a conservative projection, not a DOM.
    suppressed: Vec<String>,
    count: usize,
    error: Option<String>,
    anchor: Option<(String, String)>,
    heading: Option<String>,
    section_starts: Vec<(usize, String)>,
    code: Vec<RawCode>,
    /// Open outermost `<pre>`: language and lazily recorded raw start.
    block: Option<(Option<&'static str>, Option<usize>)>,
    /// Open inline `<code>` outside pre: lazily recorded raw start.
    inline: Option<Option<usize>>,
}
impl State {
    fn push_code(&mut self, code: RawCode) {
        if code.range.is_empty() {
            return;
        }
        if self.code.len() >= super::MAX_CODE_SPANS {
            let warning = "code styling limit reached; later code shown as plain text";
            if !self.page.warnings.iter().any(|w| w == warning) {
                self.page.warnings.push(warning.into());
            }
            return;
        }
        self.code.push(code);
    }
    /// Close code regions whose element left the (projected) tag stack.
    fn sync_code(&mut self) {
        if self.block.is_some() && !self.stack.iter().any(|n| n == "pre") {
            if let Some((language, Some(start))) = self.block.take() {
                let end = self.page.text.len();
                self.push_code(RawCode {
                    range: start..end,
                    language,
                    block: true,
                });
                if !self.page.text.is_empty() {
                    self.text_separator = Some('\n');
                }
            }
            self.block = None;
        }
        if self.inline.is_some() && !self.stack.iter().any(|n| n == "code") {
            if let Some(Some(start)) = self.inline.take() {
                let end = self.page.text.len();
                self.push_code(RawCode {
                    range: start..end,
                    language: None,
                    block: false,
                });
            }
            self.inline = None;
        }
    }
    fn collecting(&self) -> bool {
        self.page.completeness == Completeness::Complete && self.events > self.start_event
    }
    fn protected_item(&self) -> bool {
        self.anchor.is_some() || self.heading.is_some() || self.stack.iter().any(|n| n == "title")
    }
    fn begin_item(&mut self) {
        self.checkpoint = Some(Checkpoint {
            text: self.page.text.len(),
            title: self.page.title.len(),
            text_separator: self.text_separator,
            title_separator: self.title_separator,
            links: self.page.links.len(),
            images: self.page.images.len(),
            headings: self.page.headings.len(),
            sections: self.section_starts.len(),
            code: self.code.len(),
            before_event: self.events - 1,
        });
    }
    fn cutoff(&mut self, limit: WindowLimit) {
        if !self.collecting() {
            return;
        }
        // Every collecting token starts or belongs to a transaction. A missing
        // checkpoint is an internal refusal, never permission to publish a crop.
        let Some(c) = self.checkpoint.take() else {
            self.error = Some("HTML semantic boundary unavailable".into());
            return;
        };
        self.page.text.truncate(c.text);
        self.page.title.truncate(c.title);
        self.text_separator = c.text_separator;
        self.title_separator = c.title_separator;
        self.anchor_separator = None;
        self.heading_separator = None;
        self.page.links.truncate(c.links);
        self.page.images.truncate(c.images);
        self.page.headings.truncate(c.headings);
        self.section_starts.truncate(c.sections);
        self.code.truncate(c.code);
        if let Some((_, start)) = &mut self.block
            && start.is_some_and(|s| s >= c.text)
        {
            *start = None;
        }
        if let Some(start) = &mut self.inline
            && start.is_some_and(|s| s >= c.text)
        {
            *start = None;
        }
        self.anchor = None;
        self.heading = None;
        self.page.completeness = Completeness::FirstWindow {
            limit,
            before_event: c.before_event,
        };
    }
    fn ignored(&self) -> bool {
        !self.suppressed.is_empty()
    }
    fn finish_anchor(&mut self) {
        if let Some((raw, text)) = self.anchor.take() {
            self.page.links.push(Link {
                label: if clean(&text).is_empty() {
                    raw.clone()
                } else {
                    clean(&text)
                },
                destination: resolve(&self.page.address, &raw, false),
                raw,
            });
        }
    }
    // A separator becomes stored/charged only if readable text follows it.
    // Inside pre, block tags/br are literal line breaks between started text.
    fn separator(&mut self) {
        if let Some((_, start)) = self.block {
            if start.is_some()
                && !self.ignored()
                && self.collecting()
                && !self.page.text.ends_with('\n')
            {
                if self.page.text.len() < 16384 {
                    self.page.text.push('\n');
                } else {
                    self.cutoff(WindowLimit::Text);
                }
            }
            return;
        }
        self.text("\n");
    }
    fn text(&mut self, s: &str) {
        if self.ignored() || !self.collecting() {
            return;
        }
        if self.stack.iter().any(|s| s == "title") {
            if !append_normalized(
                &mut self.page.title,
                &mut self.title_separator,
                s,
                512,
                false,
            ) {
                self.cutoff(WindowLimit::Title);
            }
            return;
        }
        if self.stack.iter().any(|s| s == "head") {
            return;
        }
        if self.block.is_some() {
            // Literal pre text: exact whitespace, case and punctuation, charged
            // byte-for-byte against the same window budget. A block separator
            // starts it on its own line; separators inside pre are not structure.
            let separator = self.text_separator.take().is_some() && !self.page.text.is_empty();
            let needed = usize::from(separator) + s.len();
            if needed > 16384 - self.page.text.len() {
                self.cutoff(WindowLimit::Text);
                return;
            }
            if separator {
                self.page.text.push('\n');
            }
            if let Some((_, start @ None)) = &mut self.block {
                *start = Some(self.page.text.len());
            }
            self.page.text.push_str(s);
        } else {
            let before = self.page.text.len();
            if !append_normalized(
                &mut self.page.text,
                &mut self.text_separator,
                s,
                16384,
                true,
            ) {
                self.cutoff(WindowLimit::Text);
                return;
            }
            if let Some(start @ None) = &mut self.inline
                && self.page.text.len() > before
            {
                let skip = matches!(self.page.text.as_bytes()[before], b' ' | b'\n');
                *start = Some(before + usize::from(skip && before > 0));
            }
        }
        // Labels have their own whitespace state: body context must not erase
        // an internal separator or inject an outside leading/trailing one.
        if let Some((_, text)) = &mut self.anchor
            && !append_normalized(text, &mut self.anchor_separator, s, 16384, false)
        {
            self.cutoff(WindowLimit::Text);
            return;
        }
        if let Some(text) = &mut self.heading
            && !append_normalized(text, &mut self.heading_separator, s, 16384, false)
        {
            self.cutoff(WindowLimit::Text);
        }
    }
}
struct Sink(RefCell<State>);
impl TokenSink for Sink {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut s = self.0.borrow_mut();
        if s.error.is_some() {
            return TokenSinkResult::Continue;
        }
        // Character chunk boundaries are an input transport detail, not nodes.
        if !matches!(token, Token::CharacterTokens(_) | Token::ParseError(_)) {
            s.count += 1;
        }
        if s.count > 8192 {
            s.error = Some("HTML token/node budget exceeds 8192".into());
            return TokenSinkResult::Continue;
        }
        s.events += 1;
        let is_tag = matches!(token, Token::TagToken(_));
        if s.collecting()
            && !s.ignored()
            && (is_tag && !s.protected_item()
                || matches!(token, Token::CharacterTokens(_)) && s.checkpoint.is_none())
        {
            s.begin_item();
        }
        let mut result = TokenSinkResult::Continue;
        match token {
            Token::CharacterTokens(text) => s.text(&text),
            Token::TagToken(tag) => {
                let name = tag.name.to_string();
                let raw_state = || match name.as_str() {
                    "script" => TokenSinkResult::RawData(RawKind::ScriptData),
                    "style" | "iframe" | "noembed" | "xmp" => {
                        TokenSinkResult::RawData(RawKind::Rawtext)
                    }
                    "title" | "textarea" => TokenSinkResult::RawData(RawKind::Rcdata),
                    _ => TokenSinkResult::Continue,
                };
                let excluded = matches!(
                    name.as_str(),
                    "script"
                        | "style"
                        | "template"
                        | "svg"
                        | "math"
                        | "noscript"
                        | "button"
                        | "select"
                        | "textarea"
                        | "datalist"
                        | "iframe"
                        | "object"
                ) || (name == "form" && s.ignored());
                // Forms are inert grouping outside exclusions. Inside a template
                // or control, retain their boundary so crossed ends fail closed.
                if tag.kind == StartTag && (s.ignored() || excluded) {
                    if excluded {
                        // HTML ignores nested form starts in the same template
                        // scope; HTML self-closing syntax does NOT close form or
                        // template. Foreign svg/math self-closing syntax does.
                        let scope = s
                            .suppressed
                            .iter()
                            .rposition(|n| n == "template")
                            .map_or(0, |i| i + 1);
                        let nested_form =
                            name == "form" && s.suppressed[scope..].iter().any(|n| n == "form");
                        if !nested_form
                            && !(tag.self_closing && matches!(name.as_str(), "svg" | "math"))
                        {
                            if s.suppressed.len() + s.stack.len() >= 64 {
                                s.error = Some("HTML suppression depth budget".into());
                                return TokenSinkResult::Continue;
                            }
                            s.suppressed.push(name.clone());
                        }
                        if matches!(name.as_str(), "script" | "form" | "iframe")
                            && !s.page.warnings.iter().any(|w| w.contains("scripts/forms"))
                        {
                            s.page.warnings.push("scripts/forms/embedded behavior and base/metadata links are unsupported; no dependencies fetched".into());
                        }
                    }
                    return raw_state();
                }
                if tag.kind != StartTag && s.ignored() {
                    // Close a visible anchor without admitting suppressed text or
                    // releasing its boundary. Otherwise following public text can
                    // silently become part of the preceding link label.
                    if name == "a" {
                        s.finish_anchor();
                    }
                    // A crossing/ancestor end tag must not erase any boundary.
                    if s.suppressed.last() == Some(&name) {
                        s.suppressed.pop();
                    }
                    return TokenSinkResult::Continue;
                }
                let attr = |key: &str| {
                    tag.attrs
                        .iter()
                        .find(|a| &*a.name.local == key)
                        .map(|a| a.value.to_string())
                };
                if tag.attrs.len() > 64 || tag.attrs.iter().any(|a| a.value.len() > 2048) {
                    s.error = Some("HTML attribute budget".into());
                    return result;
                }
                let block = matches!(
                    name.as_str(),
                    "p" | "div"
                        | "section"
                        | "article"
                        | "li"
                        | "br"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "pre"
                );
                // Passive language hint only; attribute text is never evaluated.
                let language = || {
                    attr("class")
                        .as_deref()
                        .and_then(crate::highlight::language)
                        .or_else(|| {
                            attr("data-lang")
                                .as_deref()
                                .and_then(crate::highlight::language)
                        })
                };
                if tag.kind == StartTag {
                    // Optional HTML end tags are closed locally; this is not CSS layout or a DOM.
                    if matches!(name.as_str(), "p" | "li")
                        && let Some(i) = s.stack.iter().rposition(|n| n == &name)
                    {
                        s.stack.truncate(i);
                        s.sync_code();
                    }
                    if block {
                        s.separator();
                    }
                    if name == "pre" && s.block.is_none() {
                        s.block = Some((language(), None));
                    } else if name == "code" {
                        if let Some((hint, _)) = &mut s.block {
                            if hint.is_none() {
                                *hint = language();
                            }
                        } else if s.inline.is_none() {
                            s.inline = Some(None);
                        }
                    }
                    let ignored = !s.collecting();
                    if !ignored && name == "a" {
                        s.finish_anchor();
                        if let Some(raw) = attr("href") {
                            s.anchor = Some((raw, String::new()));
                            s.anchor_separator = None;
                        }
                    }
                    if !ignored && matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
                    {
                        let start = s.page.text.len();
                        s.section_starts.push((start, String::new()));
                        s.heading = Some(String::new());
                        s.heading_separator = None;
                    }
                    if !ignored && name == "img" {
                        let alt = attr("alt").unwrap_or_default();
                        s.text(&alt);
                        if let Some(raw) = attr("src")
                            && s.collecting()
                        {
                            let destination = resolve(&s.page.address, &raw, true);
                            s.page.images.push(Link {
                                label: alt,
                                raw,
                                destination,
                            });
                        }
                    }
                    if matches!(name.as_str(),"script"|"form"|"iframe"|"base"|"link") && !s.page.warnings.iter().any(|w|w=="scripts/forms/embedded behavior and base/metadata links are unsupported; no dependencies fetched") {
                        s.page.warnings.push("scripts/forms/embedded behavior and base/metadata links are unsupported; no dependencies fetched".into());
                    }
                    if !matches!(
                        name.as_str(),
                        "form"
                            | "area"
                            | "base"
                            | "br"
                            | "col"
                            | "embed"
                            | "hr"
                            | "img"
                            | "input"
                            | "link"
                            | "meta"
                            | "param"
                            | "source"
                            | "track"
                            | "wbr"
                    ) {
                        s.stack.push(name.clone());
                    }
                    result = raw_state();
                } else {
                    if name == "a" {
                        s.finish_anchor();
                    }
                    if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
                        && let Some(h) = s.heading.take()
                    {
                        let title = clean(&h);
                        if let Some((_, section_title)) = s.section_starts.last_mut() {
                            *section_title = title.clone();
                        }
                        s.page.headings.push(title);
                    }
                    if let Some(i) = s.stack.iter().rposition(|n| n == &name) {
                        s.stack.truncate(i);
                    }
                    s.sync_code();
                    if block {
                        s.separator();
                    }
                }
            }
            _ => {}
        }
        // Structural rejection remains active after collection stops. Raw-data
        // responses and suppression ancestry above also run over the entire tail.
        if s.stack.len() > 64 {
            s.error = Some("HTML semantic depth/text/link/image/heading budget exceeded".into());
        } else if s.collecting() {
            let limit = if s.page.links.len() + usize::from(s.anchor.is_some()) > 64 {
                Some(WindowLimit::Links)
            } else if s.page.images.len() > 16 {
                Some(WindowLimit::Images)
            } else if s.page.headings.len() > 64 || s.section_starts.len() > 64 {
                Some(WindowLimit::Headings)
            } else {
                None
            };
            if let Some(limit) = limit {
                s.cutoff(limit);
            }
        }
        if is_tag && !s.protected_item() {
            s.checkpoint = None;
        }
        result
    }
}
/// Prose lines are cleaned and empty prose lines dropped exactly as before;
/// pre blocks keep every byte (one leading/trailing newline is HTML layout).
/// Returns the text, a raw-byte -> output-byte map (len raw+1) and code spans.
fn normalize(raw: &str, code: &[RawCode]) -> (String, Vec<usize>, Vec<super::CodeSpan>) {
    let mut out = String::with_capacity(raw.len());
    let mut map = vec![0; raw.len() + 1];
    let mut spans = Vec::new();
    let prose = |out: &mut String, map: &mut Vec<usize>, range: std::ops::Range<usize>| {
        let mut line_start = range.start;
        for line in raw[range.clone()].split('\n') {
            let line_end = line_start + line.len();
            let mut cleaned = String::new();
            let mut pending = false;
            let base = out.len() + usize::from(!out.is_empty());
            for (i, c) in line.char_indices() {
                if c.is_whitespace() {
                    pending = !cleaned.is_empty();
                } else {
                    if pending {
                        cleaned.push(' ');
                        pending = false;
                    }
                    cleaned.push(c);
                }
                map[line_start + i] =
                    base + cleaned.len() - c.len_utf8() * usize::from(!c.is_whitespace());
            }
            if cleaned.is_empty() {
                for slot in &mut map[line_start..line_end] {
                    *slot = out.len();
                }
            } else {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&cleaned);
            }
            // The terminating newline (or range end) maps after this line.
            if line_end < range.end || line_end == raw.len() {
                map[line_end] = out.len();
            }
            line_start = line_end + 1;
        }
    };
    let mut cursor = 0;
    for block in code.iter().filter(|c| c.block) {
        if block.range.start < cursor || block.range.end > raw.len() {
            continue;
        }
        prose(&mut out, &mut map, cursor..block.range.start);
        let mut literal = block.range.clone();
        if raw[literal.clone()].starts_with('\n') {
            literal.start += 1;
        }
        if literal.start < literal.end && raw[literal.clone()].ends_with('\n') {
            literal.end -= 1;
        }
        if !literal.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            let start = out.len();
            for i in block.range.clone() {
                map[i] = start + i.saturating_sub(literal.start).min(literal.len());
            }
            out.push_str(&raw[literal]);
            spans.push(super::CodeSpan {
                range: start..out.len(),
                language: block.language.map(str::to_owned),
                block: true,
            });
        } else {
            for i in block.range.clone() {
                map[i] = out.len();
            }
        }
        map[block.range.end] = out.len();
        cursor = block.range.end;
    }
    prose(&mut out, &mut map, cursor..raw.len());
    map[raw.len()] = out.len();
    let mut inline: Vec<_> = code
        .iter()
        .filter(|c| !c.block)
        .filter_map(|c| {
            let range = map[c.range.start]..map[c.range.end];
            let valid = !range.is_empty() && out.get(range.clone()).is_some();
            valid.then_some(super::CodeSpan {
                range,
                language: None,
                block: false,
            })
        })
        .collect();
    spans.append(&mut inline);
    spans.sort_by_key(|s| s.range.start);
    (out, map, spans)
}
pub(super) fn extract(
    address: Address,
    bytes: &[u8],
    control: super::WindowControl<'_>,
) -> Result<Page> {
    let (mut page, scalars, tokens, events) = project(address, bytes, 0, control)?;
    let source = HtmlSession::retain(&page.address, bytes, scalars, tokens, page.completeness)?;
    super::WindowPosition::attach(&mut page, &source, 0, 0, events)?;
    if let Some((cancel, deadline)) = control {
        cancel.check(deadline)?;
    }
    Ok(page)
}
/// Replay ALL bytes from the original beginning to reconstruct tokenizer/raw,
/// suppression and tag ancestry. Only semantics before start_event are ignored.
/// Every tail guard runs again; no slice is treated as independent HTML.
pub(super) fn replay(
    source: &std::sync::Arc<HtmlSession>,
    index: usize,
    start_event: usize,
    control: super::WindowControl<'_>,
) -> Result<Page> {
    let (mut page, scalars, tokens, events) = project(
        source.address().clone(),
        source.encoded(),
        start_event,
        control,
    )?;
    if scalars != source.validated_scalars() || tokens != source.validated_tokens() {
        return Err("held HTML replay validation mismatch".into());
    }
    super::WindowPosition::attach(&mut page, source, index, start_event, events)?;
    if let Some((cancel, deadline)) = control {
        cancel.check(deadline)?;
    }
    Ok(page)
}
fn project(
    address: Address,
    bytes: &[u8],
    start_event: usize,
    control: super::WindowControl<'_>,
) -> Result<(Page, usize, usize, usize)> {
    if let Some((cancel, deadline)) = control {
        cancel.check(deadline)?;
    }
    if bytes.len() > 262144 {
        return Err("HTML input exceeds 256 KiB".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "only UTF-8 HTML supported")?;
    let state = State {
        text_separator: None,
        title_separator: None,
        anchor_separator: None,
        heading_separator: None,
        checkpoint: None,
        events: 0,
        start_event,
        page: Page {
            window: None,
            completeness: Completeness::Complete,
            source: None,
            sections: Vec::new(),
            address,
            title: String::new(),
            text: String::new(),
            headings: Vec::new(),
            links: Vec::new(),
            images: Vec::new(),
            warnings: Vec::new(),
            code: Vec::new(),
            section_code: Vec::new(),
        },
        stack: Vec::new(),
        suppressed: Vec::new(),
        count: 0,
        error: None,
        anchor: None,
        heading: None,
        section_starts: Vec::new(),
        code: Vec::new(),
        block: None,
        inline: None,
    };
    let tokenizer = Tokenizer::new(Sink(RefCell::new(state)), TokenizerOpts::default());
    // Semantic cutoff does NOT stop tokenization. Each original scalar still
    // crosses the vendor construction guard and full-tail structural validator.
    let input = BufferQueue::default();
    let mut scalars = 0;
    for c in text.chars() {
        if let Some((cancel, deadline)) = control {
            cancel.check(deadline)?;
        }
        scalars += 1;
        let mut bytes = [0; 4];
        tokenizer.feed_bounded(&input, c.encode_utf8(&mut bytes))?;
        if let Some(e) = &tokenizer.sink.0.borrow().error {
            return Err(e.clone());
        }
    }
    tokenizer.end();
    let mut state = tokenizer.sink.0.into_inner();
    if let Some(e) = state.error.take() {
        return Err(e);
    }
    state.finish_anchor();
    // Unclosed pre/code at EOF keep their collected literal range.
    state.stack.clear();
    state.sync_code();
    state.page.title = clean(&state.page.title);
    let (text, map, code) = normalize(&state.page.text, &state.code);
    for (i, (start, title)) in state.section_starts.iter().enumerate() {
        let end = state
            .section_starts
            .get(i + 1)
            .map(|s| s.0)
            .unwrap_or(state.page.text.len());
        // Sections start/end at line boundaries; drop only that separator.
        let mut range = map[*start]..map[end];
        if text[range.clone()].starts_with('\n') {
            range.start += 1;
        }
        if range.start < range.end && text[range.clone()].ends_with('\n') {
            range.end -= 1;
        }
        if !range.is_empty() {
            state.page.sections.push(super::Section {
                title: title.clone(),
                text: text[range.clone()].to_string(),
            });
            state.page.section_code.push(
                code.iter()
                    .filter(|c| c.range.start >= range.start && c.range.end <= range.end)
                    .map(|c| super::CodeSpan {
                        range: c.range.start - range.start..c.range.end - range.start,
                        ..c.clone()
                    })
                    .collect(),
            );
        }
    }
    state.page.text = text;
    state.page.code = code;
    if state.page.text.is_empty() {
        return Err(if state.collecting() {
            "no readable static HTML text; script-only sites unsupported"
        } else {
            "no readable first window; indivisible semantic item exceeds window budget"
        }
        .into());
    }
    if let Some(disclosure) = state.page.completeness.disclosure() {
        state.page.warnings.push(disclosure.into());
    }
    if let Some((cancel, deadline)) = control {
        cancel.check(deadline)?;
    }
    Ok((state.page, scalars, state.count, state.events))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The map is len raw+1; at every raw char boundary (the only offsets
    /// tokens produce) it is monotonic and lands on an output char boundary.
    fn check_map(raw: &str, out: &str, map: &[usize]) {
        assert_eq!(map.len(), raw.len() + 1);
        let at: Vec<usize> = (0..=raw.len())
            .filter(|i| raw.is_char_boundary(*i))
            .map(|i| map[i])
            .collect();
        assert!(at.windows(2).all(|w| w[0] <= w[1]), "{raw:?}: {map:?}");
        assert!(
            at.iter().all(|m| out.is_char_boundary(*m)),
            "{raw:?}: {map:?}"
        );
    }
    /// FIX1 B: bounded, deterministic exhaustive combinations of multibyte,
    /// CRLF, Unicode-space and newline fragments. Prose output equals the old
    /// `lines().map(clean)` projection (empty lines dropped) exactly.
    #[test]
    fn normalize_prose_matches_old_lines_map_clean_for_all_bounded_combinations() {
        const PARTS: [&str; 9] = [
            "a",
            "Zé",
            "日本",
            " ",
            "\t",
            "\n",
            "\r\n",
            "\u{a0}",
            "\u{2003}x",
        ];
        let old = |raw: &str| {
            raw.lines()
                .map(clean)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        };
        let mut cases = 0;
        for len in 0..=5u32 {
            for mut n in 0..PARTS.len().pow(len) {
                let mut raw = String::new();
                for _ in 0..len {
                    raw.push_str(PARTS[n % PARTS.len()]);
                    n /= PARTS.len();
                }
                let (out, map, spans) = normalize(&raw, &[]);
                assert_eq!(out, old(&raw), "{raw:?}");
                assert!(spans.is_empty());
                check_map(&raw, &out, &map);
                cases += 1;
            }
        }
        assert_eq!(cases, 1 + 9 + 81 + 729 + 6561 + 59049);
    }
    /// Literal blocks between adversarial prose keep exact bytes (multibyte,
    /// tabs, CR, blank lines); stale, overlapping or out-of-range block ranges
    /// are skipped rather than slicing or panicking.
    #[test]
    fn normalize_literal_blocks_are_exact_and_bad_ranges_never_panic() {
        let code = "\n\tfn Ünï() {\r\n\n    \"日本\" }  \n";
        let raw = format!("  lead\u{a0} é \n{code}\n tail\t");
        let start = raw.find('\n').unwrap() + 1;
        let block = RawCode {
            range: start..start + code.len(),
            language: Some("rust"),
            block: true,
        };
        let (out, map, spans) = normalize(&raw, &[block]);
        let literal = &code[1..code.len() - 1];
        assert_eq!(out, format!("lead é\n{literal}\ntail"));
        assert_eq!(spans.len(), 1);
        assert_eq!(&out[spans[0].range.clone()], literal);
        check_map(&raw, &out, &map);
        // Out-of-range, reversed (start > end), empty-at-end and overlapping.
        let reversed = std::ops::Range { start: 5, end: 3 };
        for range in [0..raw.len() + 9, reversed, raw.len()..raw.len(), 2..4] {
            let ranges = [
                RawCode {
                    range: 1..start + 2,
                    language: None,
                    block: true,
                },
                RawCode {
                    range: range.clone(),
                    language: None,
                    block: true,
                },
                // Inline ranges are only ever in-bounds token offsets.
                RawCode {
                    range: range.start.min(raw.len())..range.end.min(raw.len()),
                    language: None,
                    block: false,
                },
            ];
            let (out, map, spans) = normalize(&raw, &ranges);
            check_map(&raw, &out, &map);
            assert!(spans.iter().all(|s| out.get(s.range.clone()).is_some()));
        }
    }
    #[test]
    fn controlled_parse_refuses_cancelled_or_expired_work() {
        let address = Address::parse("https://example.org/a").unwrap();
        let cancel = crate::acquisition::Cancellation::default();
        assert!(cancel.cancel());
        assert!(
            extract(
                address.clone(),
                b"<p>Readable</p>",
                Some((
                    &cancel,
                    std::time::Instant::now() + std::time::Duration::from_secs(1)
                ))
            )
            .is_err()
        );
        let cancel = crate::acquisition::Cancellation::default();
        assert!(
            extract(
                address.clone(),
                b"<p>Readable</p>",
                Some((&cancel, std::time::Instant::now()))
            )
            .is_err()
        );
        assert!(
            extract(
                address,
                b"<p>Readable</p>",
                Some((
                    &cancel,
                    std::time::Instant::now() + std::time::Duration::from_secs(1)
                ))
            )
            .is_ok()
        );
    }
}
