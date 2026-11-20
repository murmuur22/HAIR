//! One owned media worker per existing runtime; reservation survives cancellation.
//! No source path/network/reopening, detached worker or unbounded job queue.
use hair::{
    Result,
    assets::{LocalImageFormat, Texture},
    local_explorer::ReaderFile,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;

pub(super) type Token = Arc<AtomicBool>;
pub(super) fn token() -> Token {
    Arc::new(AtomicBool::new(false))
}
pub(super) fn cancel(token: &Token) {
    token.store(true, Ordering::Relaxed);
}
// Per-runtime observations at the real text read_chunk call, NOT OS-wide I/O
// auditing. At most 16 successful chunk summaries; no source bytes are retained.
#[cfg(test)]
#[derive(Default)]
pub(super) struct TextReadAudit {
    pub calls: usize,
    pub chunks: Vec<(u64, usize, u64, bool)>,
}
pub(super) enum Output {
    Image(Texture),
    Model(hair::assets::Model),
}
#[derive(Default)]
pub(crate) struct Runtime {
    // Cancelled work keeps its slot until joined; no hard time/heap bound.
    job: Option<(Token, JoinHandle<Result<Output>>)>,
    // SAME logical work reservation survives join while native tracked state
    // remains. Arc owns no scheduler/cache back-reference; callbacks own only
    // their native ledger. All jobs, not just video, obey this occupied slot.
    native_slot: Arc<std::sync::Mutex<super::video_still::Slot>>,
    #[cfg(test)]
    pub(super) text_reads: Arc<std::sync::Mutex<TextReadAudit>>,
    // Read-only test observation at successful video worker completion, NOT
    // native ownership/retirement accounting or a substituted result.
    #[cfg(test)]
    pub(super) video_successes: Arc<std::sync::atomic::AtomicUsize>,
}
impl Runtime {
    #[cfg(test)]
    pub(super) fn test_finished(&self) -> bool {
        self.job.as_ref().is_some_and(|(_, job)| job.is_finished())
    }
    pub(super) fn native_retirement_pending(&self) -> bool {
        // Never contend with an active synchronous FFI call on the UI thread.
        self.job.is_none()
            && (super::video_still::abandoned()
                || self.native_slot.lock().map_or(true, |slot| slot.occupied()))
    }
    pub(super) fn busy(&self) -> bool {
        self.job.is_some() || self.native_retirement_pending()
    }
    /// Held-byte time-zero still; SAME worker and shared reservation as images.
    pub(super) fn start_video(&mut self, token: Token, file: ReaderFile) -> Result<()> {
        if self.busy() {
            return Err("object/native work reservation occupied".into());
        }
        let worker_token = token.clone();
        let slot = self.native_slot.clone();
        #[cfg(test)]
        let video_successes = self.video_successes.clone();
        let job = std::thread::Builder::new()
            .name("held-video-still".into())
            .spawn(move || {
                let bytes = file.read_video_still_bytes(&worker_token)?;
                if worker_token.load(Ordering::Relaxed) {
                    return Err("MP4 still request cancelled".into());
                }
                let image = slot
                    .lock()
                    .map_err(|_| "MP4 still reservation poisoned")?
                    .decode(&bytes)?;
                // Native async observation/retirement is not interruptible by
                // this token. Both stale success AND error are gated upstream.
                if worker_token.load(Ordering::Relaxed) {
                    return Err("MP4 still request cancelled".into());
                }
                #[cfg(test)]
                video_successes.fetch_add(1, Ordering::Release);
                Ok(Output::Image(image))
            })
            .map_err(|e| format!("MP4 still worker creation refused: {e}"))?;
        self.job = Some((token, job));
        Ok(())
    }
    pub(super) fn start(&mut self, token: Token, file: ReaderFile) -> Result<()> {
        self.start_with(token, file, None)
    }
    // Before-E suffix admission must agree with the actual held-byte signature.
    pub(super) fn start_object(
        &mut self,
        token: Token,
        file: ReaderFile,
        expected: LocalImageFormat,
    ) -> Result<()> {
        self.start_with(token, file, Some(expected))
    }
    fn start_with(
        &mut self,
        token: Token,
        file: ReaderFile,
        expected: Option<LocalImageFormat>,
    ) -> Result<()> {
        if self.busy() {
            return Err("image worker reservation occupied".into());
        }
        let worker_token = token.clone();
        let job = std::thread::Builder::new()
            .name("held-image".into())
            .spawn(move || {
                if worker_token.load(Ordering::Relaxed) {
                    return Err("image request cancelled".into());
                }
                let bytes = file.read_image_bytes(&worker_token)?;
                if worker_token.load(Ordering::Relaxed) {
                    return Err("image request cancelled".into());
                }
                if expected.is_some_and(|format| LocalImageFormat::detect(&bytes) != Some(format)) {
                    return Err("object image suffix/signature mismatch; fallback retained".into());
                }
                let image = hair::assets::decode_local_image(&bytes)?;
                // Stale/cancelled pixels never commit. Decode is not interruptible.
                if worker_token.load(Ordering::Relaxed) {
                    return Err("image request cancelled".into());
                }
                Ok(Output::Image(image))
            })
            .map_err(|e| format!("image worker creation refused: {e}"))?;
        self.job = Some((token, job));
        Ok(())
    }
    /// Uses the SAME sole scheduler slot, token and reservation as image/text.
    pub(super) fn start_model(&mut self, token: Token, file: ReaderFile) -> Result<()> {
        if self.busy() {
            return Err("object worker reservation occupied".into());
        }
        let worker_token = token.clone();
        let job = std::thread::Builder::new()
            .name("held-static-model".into())
            .spawn(move || {
                let bytes = file.read_model_bytes(&worker_token)?;
                if worker_token.load(Ordering::Relaxed) {
                    return Err("model request cancelled".into());
                }
                let model = hair::assets::decode_local_preview_model(&bytes)?;
                if worker_token.load(Ordering::Relaxed) {
                    return Err("model request cancelled".into());
                }
                Ok(Output::Model(model))
            })
            .map_err(|e| format!("model worker creation refused: {e}"))?;
        self.job = Some((token, job));
        Ok(())
    }
    /// Shares the object scheduler's SOLE worker/reservation with PNG/JPEG.
    /// Text never calls the whole-image acquisition API or any format parser.
    pub(super) fn start_text(&mut self, token: Token, file: ReaderFile) -> Result<()> {
        if self.busy() {
            return Err("object worker reservation occupied".into());
        }
        let worker_token = token.clone();
        #[cfg(test)]
        let text_reads = self.text_reads.clone();
        let job = std::thread::Builder::new()
            .name("held-text-prefix".into())
            .spawn(move || {
                if worker_token.load(Ordering::Relaxed) {
                    return Err("text excerpt cancelled".into());
                }
                // Exactly offset zero: <=4096 bytes, <=8 positional-read attempts,
                // baseline regular-file stamp checked before AND after the read.
                // File length never controls allocation; no lookahead/other pages.
                #[cfg(test)]
                {
                    text_reads.lock().unwrap().calls += 1;
                }
                let chunk = file.read_chunk(0)?;
                #[cfg(test)]
                {
                    let mut audit = text_reads.lock().unwrap();
                    if audit.chunks.len() < 16 {
                        audit
                            .chunks
                            .push((chunk.start, chunk.used, chunk.observed_len, chunk.eof));
                    }
                }
                if worker_token.load(Ordering::Relaxed) {
                    return Err("text excerpt cancelled".into());
                }
                if chunk.used == 0 {
                    return Err("empty text prefix; pack fallback retained".into());
                }
                // At most 10 ASCII escape glyphs per source byte (<=40960).
                let mut escaped = String::new();
                super::raw_reader::escaped_bytes(&mut escaped, chunk.body());
                if escaped.trim().is_empty() {
                    return Err("blank text prefix; pack fallback retained".into());
                }
                let texture = hair::text::literal_excerpt_texture(&escaped, !chunk.eof);
                if worker_token.load(Ordering::Relaxed) {
                    return Err("text excerpt cancelled".into());
                }
                Ok(Output::Image(texture))
            })
            .map_err(|e| format!("text worker creation refused: {e}"))?;
        self.job = Some((token, job));
        Ok(())
    }
    // Keep the opened-image and old test hooks texture-only, without weakening
    // any acquisition/raster assertion. Only the room scheduler consumes models.
    pub(super) fn poll(&mut self) -> Option<(Token, Result<Texture>)> {
        self.poll_object().map(|(token, result)| {
            (
                token,
                result.and_then(|output| match output {
                    Output::Image(texture) => Ok(texture),
                    Output::Model(_) => Err("model result is not an opened image".into()),
                }),
            )
        })
    }
    pub(super) fn poll_object(&mut self) -> Option<(Token, Result<Output>)> {
        if !self.job.as_ref().is_some_and(|(_, job)| job.is_finished()) {
            return None;
        }
        let (token, job) = self.job.take()?;
        Some((
            token,
            job.join()
                .unwrap_or_else(|_| Err("image worker panicked; result discarded".into())),
        ))
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some((token, job)) = self.job.take() {
            cancel(&token);
            // App retirement only; can wait for OS I/O/decode/cleanup.
            let _ = job.join();
        }
    }
}

pub(super) struct State {
    pub token: Token,
    pub requested: bool,
    pub body: bool,
    pub texture: Option<Arc<Texture>>,
    pub view: [f32; 3],
    pub format: LocalImageFormat,
    pub status: String,
}
impl State {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let format = LocalImageFormat::detect(bytes)?;
        Some(Self {
            token: token(),
            requested: true,
            body: true,
            texture: None,
            view: [1.0, 0.0, 0.0],
            format,
            status: format!(
                "{} pending: bounded same-handle acquisition and decode on worker",
                format.name()
            ),
        })
    }
}
impl Drop for State {
    fn drop(&mut self) {
        cancel(&self.token);
    }
}
