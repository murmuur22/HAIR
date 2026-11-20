//! Bounded presentation and single-owner reader session; no filesystem paths opened here.
use hair::Result;
use hair::local_explorer::{READER_CHUNK_BYTES, ReaderChunk, ReaderFile};
use std::fmt::Write;

const DOCUMENT_LIMIT: usize = 256 * 1024;
const HELP: &str = "Left/Right chunk | V text/raw | I metadata/body\nC retry SAME handle from zero | Up/Down scroll | Escape close\n";

pub(super) struct Session {
    file: Option<ReaderFile>,
    chunk: Option<ReaderChunk>,
    metadata: String,
    raw: bool,
    metadata_only: bool,
    warning: Option<String>,
    body_scroll: usize,
    metadata_scroll: usize,
    image: Option<super::image::State>,
    pub(super) audio: Option<super::audio::State>,
}
impl Session {
    pub(super) fn open(file: Result<ReaderFile>, metadata: String) -> Self {
        let (file, result) = match file {
            Ok(file) => {
                let result = file.read_chunk(0);
                (Some(file), result)
            }
            Err(error) => (None, Err(error)),
        };
        let (chunk, warning) = match result {
            Ok(chunk) => (Some(chunk), None),
            Err(error) => (None, Some(error)),
        };
        // Default text is a property of this chunk's bytes, never its extension.
        let raw = chunk.as_ref().is_none_or(|c| !plain_text(c.body()));
        let image = chunk
            .as_ref()
            .filter(|c| c.start == 0)
            .and_then(|c| super::image::State::from_bytes(c.body()));
        Self {
            audio: chunk
                .as_ref()
                .filter(|c| c.start == 0)
                .and_then(|c| super::audio::State::from_bytes(c.body())),
            file,
            chunk,
            metadata,
            raw,
            metadata_only: false,
            warning,
            body_scroll: 0,
            metadata_scroll: 0,
            image,
        }
    }
    pub(super) fn audio_visible(&self) -> bool {
        !self.metadata_only
            && self.warning.is_none()
            && self.audio.as_ref().is_some_and(|audio| audio.body)
    }
    pub(super) fn audio_source(&self) -> Result<ReaderFile> {
        self.file
            .as_ref()
            .ok_or_else(|| "audio has no held source".to_string())
            .and_then(ReaderFile::try_clone)
    }
    pub(super) fn stop_audio(&mut self) {
        if let Some(audio) = &mut self.audio {
            audio.stop();
        }
    }
    fn audio_document(&self) -> Result<String> {
        if self.metadata.len() > DOCUMENT_LIMIT / 2 {
            return Err("reader metadata exceeds document budget".into());
        }
        let audio = self.audio.as_ref().unwrap();
        let mut out = String::with_capacity(4096);
        writeln!(out, "WAV AUDIO\nAUDIO STATUS: {}", audio.observed.label()).unwrap();
        out.push_str("Space play/pause | V audio/raw/text | I source/body\nC retry SAME handle | Escape close\n");
        out.push_str("AUDIO BODY CONTROLS (fresh physical presses only):\nLeft/Right seek request -/+5 s (0..300 s pending)\n+/- volume request 5% (0..100%; system unchanged)\n");
        if let Some(info) = audio.info {
            writeln!(
                out,
                "AUDIO SEEK REQUEST: {:.6} s (target; decoded duration {:.6} s; clamped to source)",
                audio.controls.seconds, info.duration
            )
            .unwrap();
            writeln!(
                out,
                "DECODED SOURCE: {} frames at {} Hz",
                info.frames, info.rate
            )
            .unwrap();
        } else {
            writeln!(
                out,
                "AUDIO SEEK REQUEST: {:.0} s (pending; duration unknown)",
                audio.controls.seconds
            )
            .unwrap();
        }
        writeln!(
            out,
            "AUDIO VOLUME REQUEST: {}% (pending)",
            audio.controls.volume
        )
        .unwrap();
        if let Some(applied) = audio.applied {
            writeln!(
                out,
                "AUDIO FRAME START: {} (prepared source PCM offset; not current playback position)",
                applied.frame
            )
            .unwrap();
            if let Some(volume) = applied.queue_gain {
                writeln!(
                    out,
                    "AUDIO QUEUE GAIN: {volume}% (set succeeded; not acoustic measurement)"
                )
                .unwrap();
            } else {
                out.push_str("AUDIO QUEUE GAIN: not applied (no open output)\n");
            }
        } else {
            out.push_str("AUDIO FRAME START: not prepared; queue gain not applied\n");
        }
        out.push_str("Playback status is observed separately above; no measured acoustic position.\nOpening/adjusting never decodes or outputs until explicit Space play.\n");
        if let Some(error) = &audio.error {
            writeln!(out, "Audio refused: {error}; stored bytes/source retained.").unwrap();
        }
        out.push_str("I/V retain seek/volume preferences but cancel playback. C resets to 0 s / 100%.\nReturning never resumes; press Space again. Focus loss also stops.\nPaused seeks prepare only; playing seeks preserve existing play intent.\nEnd targets do not open an empty queue; explicit replay starts from zero.\n");
        out.push_str("\nFULL SOURCE / METADATA:\n");
        if let Some(c) = &self.chunk {
            writeln!(out, "RANGE [0x{:016X}, 0x{:016X})\nObserved length: {} bytes\nLIMIT: {} bytes/chunk; no whole-file load", c.start, c.end, c.observed_len, READER_CHUNK_BYTES).unwrap();
        }
        out.push_str("RAW/TEXT CONTEXT ONLY (after V):\n");
        out.push_str(HELP);
        out.push_str(&self.metadata);
        out.push_str("\nAudio: bounded PCM16LE mono/stereo 8000..96000 Hz, 4 MiB encoded/PCM.\nOther WAV profiles refused on play; source bytes remain accessible with V.\nQueue-only gain; default 100% (normal source level); system volume unchanged.\nNo MP3, microphone, execution, links or default-open.\nLive filesystem, NOT immutable snapshot. Same opened handle until close.\n");
        if out.len() > DOCUMENT_LIMIT {
            return Err("reader formatted document exceeds budget".into());
        }
        Ok(out)
    }
    pub(super) fn image_visible(&self) -> bool {
        !self.metadata_only
            && self.warning.is_none()
            && self
                .image
                .as_ref()
                .is_some_and(|image| image.body && image.texture.is_some())
    }
    pub(super) fn image_texture(&self) -> Option<&std::sync::Arc<hair::assets::Texture>> {
        self.image.as_ref()?.texture.as_ref()
    }
    pub(super) fn image_view(&self) -> [f32; 3] {
        self.image.as_ref().map_or([1.0, 0.0, 0.0], |i| i.view)
    }
    pub(super) fn image_pending(&self) -> bool {
        self.warning.is_none() && self.image.as_ref().is_some_and(|i| i.requested)
    }
    pub(super) fn image_request(&mut self) -> Option<Result<(super::image::Token, ReaderFile)>> {
        if !self.image_pending() {
            return None;
        }
        let image = self.image.as_mut()?;
        image.requested = false;
        let token = image.token.clone();
        Some(
            self.file
                .as_ref()
                .ok_or_else(|| "image has no held source".to_string())
                .and_then(ReaderFile::try_clone)
                .map(|file| (token, file)),
        )
    }
    pub(super) fn image_complete(
        &mut self,
        token: &super::image::Token,
        result: Result<hair::assets::Texture>,
    ) {
        let Some(image) = &mut self.image else {
            return;
        };
        if !std::sync::Arc::ptr_eq(&image.token, token)
            || self.warning.is_some()
            || token.load(std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        match result {
            Ok(texture) => {
                image.status = match image.format {
                    hair::assets::LocalImageFormat::Png => format!(
                        "PNG {}x{} | 8-bit sRGB samples; alpha over dark background",
                        texture.width, texture.height
                    ),
                    hair::assets::LocalImageFormat::Jpeg => format!(
                        "JPEG {}x{} | baseline SOF0 8-bit gray/YCbCr; assumed sRGB | 4 MiB encoded/RGBA, 1024 axes",
                        texture.width, texture.height
                    ),
                };
                image.texture = Some(std::sync::Arc::new(texture));
            }
            Err(error) => {
                image.texture = None;
                image.status = format!(
                    "{} refused: {error}; stored bytes/source retained",
                    image.format.name()
                );
            }
        }
    }
    pub(super) fn image_failed(&mut self, error: String) {
        if let Some(image) = &mut self.image {
            image.requested = false;
            image.texture = None;
            image.status = format!(
                "{} refused: {error}; stored bytes/source retained",
                image.format.name()
            );
        }
    }
    pub(super) fn document(&self) -> Result<String> {
        if self.audio_visible() {
            return self.audio_document();
        }
        if self.metadata.len() > DOCUMENT_LIMIT / 2 {
            return Err("reader metadata exceeds document budget".into());
        }
        let mut out = String::with_capacity(64 * 1024);
        if self.image_visible() {
            writeln!(
                out,
                "{} IMAGE VIEW",
                self.image.as_ref().unwrap().format.name()
            )
            .unwrap();
        } else {
            out.push_str(if self.raw {
                "RAW STORED BYTES\n"
            } else {
                "TEXT: UTF-8 interpretation of this byte range\n"
            });
        }
        out.push_str("L/R chunk | V text/raw | I info\n");
        if let Some(image) = &self.image {
            writeln!(
                out,
                "{}\nV image/raw/text | I source/body | Escape close",
                image.status
            )
            .unwrap();
        }
        if let Some(error) = &self.warning {
            // Debug escaping prevents controls/bidi/error text from becoming authority.
            let escaped: String = error
                .chars()
                .take(2048)
                .flat_map(char::escape_default)
                .collect();
            writeln!(
                out,
                "STALE / READ ERROR: {escaped}\nLast good bytes/range retained; paging suspended."
            )
            .unwrap();
        }
        if let Some(c) = &self.chunk {
            writeln!(out, "Bytes {}..{} (end excl.)", c.start, c.end).unwrap();
            writeln!(
                out,
                "Chunk {}/{} | {}",
                c.used,
                READER_CHUNK_BYTES,
                if c.eof { "EOF" } else { "MORE" }
            )
            .unwrap();
            if c.used == 0 {
                out.push_str("Empty byte range / EOF\n");
            }
            if !self.metadata_only && !self.image_visible() {
                if self.raw {
                    raw_body(&mut out, c);
                } else {
                    text_body(&mut out, c);
                }
            }
        } else {
            out.push_str("NO BODY READ: no successful byte range; encoding unknown.\n");
            if self.file.is_none() {
                out.push_str(
                    "No safe handle acquired. Escape then E explicitly tries acquisition again.\n",
                );
            }
        }
        if self.metadata_only {
            out.push_str("METADATA VIEW (I returns to retained body)\n");
        }
        out.push_str("\nFULL SOURCE / METADATA:\n");
        // Keep technical details and complete controls outside the interpreted
        // text-body region, including in metadata-only and failed-read views.
        if let Some(c) = &self.chunk {
            writeln!(out, "RANGE [0x{:016X}, 0x{:016X})\nObserved length: {} bytes | {}\nLIMIT: {} bytes/chunk; no whole-file load", c.start, c.end,
                c.observed_len, if c.eof { "EOF (next inert)" } else { "MORE" }, READER_CHUNK_BYTES).unwrap();
        } else {
            writeln!(out, "RANGE: unavailable (no successful byte range)\nObserved length: unknown\nLIMIT: {} bytes/chunk; no whole-file load", READER_CHUNK_BYTES).unwrap();
        }
        out.push_str(HELP);
        out.push_str(&self.metadata);
        if let Some(image) = &self.image {
            match image.format {
                hair::assets::LocalImageFormat::Png => out.push_str("\nPNG adapter: static 8-bit gray/RGB/gray-alpha/RGBA, 4 MiB encoded, 1024 axes.\nICC/orientation/custom colorspace refused; untagged samples assumed sRGB.\nText/ICC metadata never expanded; original stored bytes unchanged.\n"),
                hair::assets::LocalImageFormat::Jpeg => out.push_str("\nJPEG adapter (macOS): baseline SOF0, 8-bit gray/conventional 3-component YCbCr; one all-component scan.\n4 MiB encoded/RGBA, 1024 axes; 4096 markers, 256 KiB headers, 128 KiB ancillary.\nOnly ordinary JFIF without thumbnail and opaque COM metadata admitted.\nProgressive/lossless/12-bit/CMYK/ICC/EXIF/orientation/custom/hierarchical/multiframe refused.\nUntagged samples assumed sRGB; native color conversion is not color-accuracy certification.\nNative private heaps/work and cleanup have no hard bound; cancellation checked before/after, not interruptible.\nOriginal stored bytes unchanged; no path reopen or default-open.\n"),
            }
        }
        out.push_str("\nLive filesystem, NOT an immutable snapshot; metadata checks cannot detect every edit.\nNo universal kernel/network-filesystem I/O timeout. Same opened handle until close.\n");
        out.push_str(if self.image.is_some() {
            "No execution, markup authority, links, upload or default-open.\n"
        } else {
            "Stored bytes only: no decoding media, execution, markup authority, links, upload or default-open.\n"
        });
        out.push_str("Text: LF is a line break; controls/backslash/unsupported glyphs use escapes.\nInvalid or chunk-boundary UTF-8 bytes use \\xNN individually; no bytes dropped.\nRaw hex is authoritative; ASCII column uses '.' for nonprintable/non-ASCII bytes.\nFormatted document limit: 262144 bytes.\n");
        if out.len() > DOCUMENT_LIMIT {
            return Err("reader formatted document exceeds budget".into());
        }
        Ok(out)
    }
    /// Returns a replacement scroll only when the document's context changed.
    /// Failures retain last good chunk, mode and scroll, and only add stale status.
    pub(super) fn action(&mut self, key: winit::keyboard::KeyCode, scroll: usize) -> Option<usize> {
        use winit::keyboard::KeyCode;
        if key == KeyCode::Space {
            if self.audio_visible() {
                self.audio.as_mut().unwrap().toggle();
            }
            return None;
        }
        // Override only a visible audio body; raw/text paging and image controls
        // keep their existing mappings. Adjustment has no acquisition authority.
        if self.audio_visible() && self.audio.as_mut().unwrap().adjust(key) {
            return None;
        }
        if self.audio.is_some() {
            match key {
                KeyCode::KeyI => self.stop_audio(),
                KeyCode::KeyV => {
                    self.stop_audio();
                    let audio = self.audio.as_mut().unwrap();
                    if audio.body {
                        audio.body = false;
                        self.raw = true;
                    } else if self.raw {
                        self.raw = false;
                    } else {
                        audio.body = true;
                    }
                    return None;
                }
                KeyCode::KeyC => {
                    // Cancel before retry changes the held-handle baseline.
                    self.audio = None;
                    let result = self.non_image_action(key, scroll);
                    if self.warning.is_none() {
                        self.audio = self
                            .chunk
                            .as_ref()
                            .filter(|c| c.start == 0)
                            .and_then(|c| super::audio::State::from_bytes(c.body()));
                    }
                    return result;
                }
                // Raw/text paging retains interpretation/preferences but never
                // playback intent. Inert EOF arrows must not discard controls.
                KeyCode::ArrowLeft | KeyCode::ArrowRight => self.stop_audio(),
                _ => {}
            }
        }
        if self.image_visible() {
            let view = &mut self.image.as_mut().unwrap().view;
            match key {
                KeyCode::Equal | KeyCode::NumpadAdd => view[0] = (view[0] * 1.25).min(8.0),
                // Fit is the reset, not the minimum: retain a finite 1/8-fit image.
                KeyCode::Minus | KeyCode::NumpadSubtract => view[0] = (view[0] / 1.25).max(0.125),
                KeyCode::ArrowLeft => view[1] = (view[1] - 0.125).max(-1.0),
                KeyCode::ArrowRight => view[1] = (view[1] + 0.125).min(1.0),
                KeyCode::ArrowUp => view[2] = (view[2] - 0.125).max(-1.0),
                KeyCode::ArrowDown => view[2] = (view[2] + 0.125).min(1.0),
                KeyCode::Digit0 => *view = [1.0, 0.0, 0.0],
                _ => return self.non_image_action(key, scroll),
            }
            // At or below fit neither axis can overflow the body. Do not retain
            // invisible pan that could reappear on the next enlargement.
            if view[0] <= 1.0 {
                view[1] = 0.0;
                view[2] = 0.0;
            }
            return Some(0);
        }
        self.non_image_action(key, scroll)
    }
    fn non_image_action(&mut self, key: winit::keyboard::KeyCode, scroll: usize) -> Option<usize> {
        use winit::keyboard::KeyCode;
        match key {
            KeyCode::KeyI => {
                if self.metadata_only {
                    self.metadata_scroll = scroll;
                } else {
                    self.body_scroll = scroll;
                }
                self.metadata_only = !self.metadata_only;
                return Some(if self.metadata_only {
                    self.metadata_scroll
                } else {
                    self.body_scroll
                });
            }
            KeyCode::KeyV => {
                if let Some(image) = &mut self.image {
                    if image.body {
                        image.body = false;
                        self.raw = true;
                    } else if self.raw {
                        self.raw = false;
                    } else {
                        image.body = true;
                    }
                } else {
                    self.raw = !self.raw;
                }
                return None;
            }
            KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::KeyC => {}
            _ => return None,
        }
        if key != KeyCode::KeyC && (self.warning.is_some() || self.metadata_only) {
            return None;
        }
        let Some(file) = &mut self.file else {
            return None;
        };
        let result = if key == KeyCode::KeyC {
            // Retire old pixels/generation BEFORE accepting a new same-handle baseline.
            self.image = None;
            file.retry_from_zero()
        } else {
            let c = self.chunk.as_ref()?;
            let offset = if key == KeyCode::ArrowRight {
                if c.eof {
                    return None;
                }
                c.end
            } else {
                if c.start == 0 {
                    return None;
                }
                c.start.saturating_sub(READER_CHUNK_BYTES as u64)
            };
            file.read_chunk(offset)
        };
        match result {
            Ok(chunk) => {
                // Formatting must also succeed before replacing the committed view.
                let previous = self.chunk.replace(chunk);
                let warning = self.warning.take();
                if let Err(error) = self.document() {
                    self.chunk = previous;
                    self.image = None;
                    self.warning = Some(error);
                    return None;
                }
                drop(warning);
                if key == KeyCode::KeyC {
                    self.image = self
                        .chunk
                        .as_ref()
                        .and_then(|c| super::image::State::from_bytes(c.body()));
                }
                self.body_scroll = 0;
                if self.metadata_only { None } else { Some(0) }
            }
            Err(error) => {
                self.image = None;
                self.warning = Some(error);
                None
            }
        }
    }
}
fn plain_text(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok_and(|s| {
        s.chars()
            .all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
    })
}
fn raw_body(out: &mut String, chunk: &ReaderChunk) {
    out.push_str("\nBYTE GROUPS (8 bytes max)\n");
    for (row, bytes) in chunk.body().chunks(8).enumerate() {
        writeln!(out, "OFFSET {:016X}", chunk.start + (row * 8) as u64).unwrap();
        out.push_str("HEX ");
        for (i, byte) in bytes.iter().enumerate() {
            if i != 0 {
                out.push(' ');
            }
            write!(out, "{byte:02X}").unwrap();
        }
        out.push_str("\nASCII ");
        for &b in bytes {
            out.push(if (0x20..=0x7e).contains(&b) {
                b as char
            } else {
                '.'
            });
        }
        out.push_str("\n\n");
    }
}
fn text_body(out: &mut String, chunk: &ReaderChunk) {
    out.push_str("\nTEXT (links inactive; escapes shown; range only):\n");
    // No lookaround: boundary fragments are explicitly byte-escaped, independently
    // on each page. This never infers whole-file encoding or drops continuation bytes.
    escaped_bytes(out, chunk.body());
    out.push('\n');
}
/// Shared literal byte projection; callers supply at most one held 4096-byte chunk.
/// No parsing, link extraction, case folding or UTF-8 replacement/loss.
pub(super) fn escaped_bytes(out: &mut String, bytes: &[u8]) {
    let mut remaining = bytes;
    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(text) => {
                escaped_text(out, text);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                if valid > 0 {
                    // from_utf8 above established this exact prefix is valid.
                    escaped_text(out, std::str::from_utf8(&remaining[..valid]).unwrap());
                }
                let bad = error.error_len().unwrap_or(remaining.len() - valid);
                for b in &remaining[valid..valid + bad] {
                    write!(out, "\\x{b:02X}").unwrap();
                }
                remaining = &remaining[valid + bad..];
            }
        }
    }
}
fn escaped_text(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '\n' => out.push('\n'),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            c if c.is_ascii() => {
                write!(out, "\\x{:02X}", c as u32).unwrap();
            }
            c => {
                write!(out, "\\u{{{:X}}}", c as u32).unwrap();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{Seek, SeekFrom, Write as IoWrite},
        path::PathBuf,
    };
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn text_escapes_controls_invalid_bytes_and_boundary_fragments_without_loss() {
        let mut chunk = ReaderChunk {
            bytes: [0; READER_CHUNK_BYTES],
            used: 9,
            start: 0,
            end: 9,
            observed_len: 9,
            eof: true,
        };
        chunk.bytes[..9].copy_from_slice(&[b'A', b'\n', 0, 0x1b, 0xff, 0xe2, 0x82, b'\\', 0x7f]);
        let mut out = String::new();
        text_body(&mut out, &chunk);
        assert!(
            out.contains("A\n\\x00\\x1B\\xFF\\xE2\\x82\\\\\\x7F"),
            "{out:?}"
        );
        assert!(!out.contains('\0') && !out.contains('\u{fffd}'));
        let mut unicode = String::new();
        escaped_text(&mut unicode, "é\u{202e}");
        assert_eq!(unicode, "\\u{E9}\\u{202E}");
        let mut raw = String::new();
        raw_body(&mut raw, &chunk);
        assert_eq!(
            raw,
            "\nBYTE GROUPS (8 bytes max)\nOFFSET 0000000000000000\nHEX 41 0A 00 1B FF E2 82 5C\nASCII A......\\\n\nOFFSET 0000000000000008\nHEX 7F\nASCII .\n\n"
        );
    }
    #[test]
    fn compact_chrome_retains_details_after_content_in_every_view() {
        let mut chunk = ReaderChunk {
            bytes: [0; READER_CHUNK_BYTES],
            used: 4,
            start: 0,
            end: 4,
            observed_len: 4,
            eof: true,
        };
        chunk.bytes[..4].copy_from_slice(b"abc\n");
        let mut session = Session {
            file: None,
            chunk: Some(chunk),
            metadata: "FULL ORIGINAL PATH /fixture/notes.json\n".into(),
            raw: false,
            metadata_only: false,
            warning: None,
            body_scroll: 0,
            metadata_scroll: 0,
            image: None,
            audio: None,
        };
        let doc = session.document().unwrap();
        assert!(doc.starts_with("TEXT: UTF-8 interpretation of this byte range\nL/R chunk | V text/raw | I info\nBytes 0..4 (end excl.)\nChunk 4/4096 | EOF\n\nTEXT (links inactive; escapes shown; range only):\n"));
        let body = doc
            .split_once("\nTEXT (links inactive; escapes shown; range only):\n")
            .unwrap()
            .1
            .split_once("\n\nFULL SOURCE / METADATA:\n")
            .unwrap()
            .0;
        assert_eq!(body, "abc\n");
        session.warning = Some("owned stale warning".into());
        for empty in [false, true] {
            if empty {
                let c = session.chunk.as_mut().unwrap();
                c.used = 0;
                c.end = 0;
                c.observed_len = 0;
            }
            for raw in [false, true] {
                session.raw = raw;
                for metadata_only in [false, true] {
                    session.metadata_only = metadata_only;
                    let doc = session.document().unwrap();
                    let (initial, details) = doc.split_once("\nFULL SOURCE / METADATA:\n").unwrap();
                    assert!(initial.contains("STALE / READ ERROR: owned stale warning"));
                    assert!(initial.contains("Last good bytes/range retained; paging suspended."));
                    assert!(!initial.contains(HELP) && !initial.contains("RANGE ["));
                    let end = if empty { 0 } else { 4 };
                    assert!(details.contains(&format!("RANGE [0x0000000000000000, 0x{end:016X})\nObserved length: {end} bytes | EOF (next inert)")));
                    assert!(details.contains("LIMIT: 4096 bytes/chunk; no whole-file load"));
                    assert!(details.contains(HELP));
                    assert!(details.contains(&session.metadata));
                    assert_eq!(initial.contains("METADATA VIEW"), metadata_only);
                    if empty {
                        assert!(initial.contains("Empty byte range / EOF"));
                    }
                }
            }
        }
        let mut failed = Session::open(Err("owned acquisition refusal".into()), session.metadata);
        for raw in [false, true] {
            failed.raw = raw;
            for metadata_only in [false, true] {
                failed.metadata_only = metadata_only;
                let doc = failed.document().unwrap();
                assert!(doc.contains("STALE / READ ERROR: owned acquisition refusal"));
                assert!(doc.contains("NO BODY READ: no successful byte range"));
                assert!(doc.contains(
                    "RANGE: unavailable (no successful byte range)\nObserved length: unknown"
                ));
                assert!(doc.contains("LIMIT: 4096 bytes/chunk; no whole-file load"));
                assert!(doc.contains(HELP) && doc.contains(&failed.metadata));
                assert!(!doc.contains("\nOFFSET "));
            }
        }
    }
    #[test]
    fn raw_octets_and_offsets_fit_small_expanded_reader_columns() {
        let mut chunk = ReaderChunk {
            bytes: [0; READER_CHUNK_BYTES],
            used: 16,
            start: 0,
            end: 16,
            observed_len: 16,
            eof: true,
        };
        chunk.bytes[..16].copy_from_slice(b"0123456789ABCDEF");
        let mut out = String::new();
        raw_body(&mut out, &chunk);
        let sidebar = (640.0_f32 * 0.32).min(260.0) + 12.0;
        let columns = ((640.0 - sidebar - 48.0) / 12.0) as usize;
        assert!(
            out.lines().all(|line| line.chars().count() <= columns),
            "raw data rows must fit the actual 640x480 expanded-reader budget ({columns}), not wrap offsets into hex/ASCII: {out:?}"
        );
        assert!(out.contains("30 31 32 33 34 35 36 37"));
        assert!(out.contains("0000000000000000"));
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn bounded_large_reader_retains_handle_and_retries_only_that_inode() {
        use hair::local_explorer::{Explorer, MAX_BODY_BYTES};
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("hair-held-reader-{}-{unique}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let fixture = Fixture(path);
        let original = fixture.0.join("data.bin");
        let moved = fixture.0.join("held.bin");
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&original)
            .unwrap();
        file.set_len(MAX_BODY_BYTES + 8193).unwrap();
        file.write_all(b"ORIGINAL").unwrap();
        file.seek(SeekFrom::Start(4096)).unwrap();
        file.write_all(b"SECOND!!").unwrap();
        drop(file);
        let explorer = Explorer::new(&fixture.0).unwrap();
        assert!(explorer.read_entry(0, MAX_BODY_BYTES + 1).is_err());
        assert!(explorer.read_entry(0, MAX_BODY_BYTES).is_err());
        let mut reader = explorer.open_reader(0).unwrap();
        let first = reader.read_chunk(0).unwrap();
        assert_eq!(first.used, 4096);
        assert_eq!(&first.body()[..8], b"ORIGINAL");
        assert_eq!(first.observed_len, MAX_BODY_BYTES + 8193);
        assert!(reader.read_chunk(u64::MAX).is_err());
        fs::rename(&original, &moved).unwrap();
        fs::write(&original, b"REPLACEMENT").unwrap();
        // Rename may change ctime: explicitly accept that on the held handle.
        assert_eq!(&reader.retry_from_zero().unwrap().body()[..8], b"ORIGINAL");
        assert_eq!(&reader.read_chunk(4096).unwrap().body()[..8], b"SECOND!!");
        fs::write(&moved, b"CHANGED").unwrap();
        assert!(reader.read_chunk(0).is_err());
        let retried = reader.retry_from_zero().unwrap();
        assert_eq!(retried.body(), b"CHANGED");
        assert!(retried.eof);
        assert_eq!(fs::read(&original).unwrap(), b"REPLACEMENT");
        drop(reader);
        drop(explorer);
        drop(fixture);
    }
}
