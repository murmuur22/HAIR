//! Explicit-intent audio owner. App retains the only controller across reader generations.
pub(super) mod pcm;
use hair::{Result, local_explorer::ReaderFile};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};
pub(super) type Token = Arc<AtomicBool>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Desired {
    Stopped,
    Play,
    Pause,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Observed {
    Ready,
    Acquiring,
    Prepared,
    PlayRequested,
    PauseRequested,
    Playing,
    Paused,
    Ended,
    Error,
}
impl Observed {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Acquiring => "ACQUIRING",
            Self::Prepared => "READY (DECODE RETAINED; OUTPUT NOT OPENED)",
            Self::PlayRequested => "PLAY REQUESTED",
            Self::PauseRequested => "PAUSE REQUESTED",
            Self::Playing => "PLAYING",
            Self::Paused => "PAUSED",
            Self::Ended => "ENDED",
            Self::Error => "ERROR",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Controls {
    pub seconds: f64,
    pub volume: u8,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            seconds: 0.0,
            volume: 100,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Info {
    pub frames: usize,
    pub rate: u32,
    pub duration: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Applied {
    pub frame: usize,
    pub queue_gain: Option<u8>,
}
/// Signature/intent only on E. This state has no worker or native authority.
pub(super) struct State {
    pub token: Token,
    pub desired: Desired,
    pub(super) observed: Observed,
    pub requested: bool,
    pub body: bool,
    pub error: Option<String>,
    pub controls: Controls,
    pub info: Option<Info>,
    pub applied: Option<Applied>,
}
impl State {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        pcm::signature(bytes).then(|| Self {
            token: Arc::new(AtomicBool::new(false)),
            desired: Desired::Stopped,
            observed: Observed::Ready,
            requested: false,
            body: true,
            error: None,
            controls: Controls::default(),
            info: None,
            applied: None,
        })
    }
    pub fn toggle(&mut self) {
        if self.desired == Desired::Play {
            self.desired = Desired::Pause;
            self.observed = Observed::PauseRequested;
        } else {
            // Replay is an explicit press, not an EOF/seek side effect.
            if self.observed == Observed::Ended {
                self.controls.seconds = 0.0;
            }
            if cancelled(&self.token) {
                self.token = Arc::new(AtomicBool::new(false));
            }
            self.desired = Desired::Play;
            self.observed = Observed::PlayRequested;
            self.requested = true;
            self.error = None;
        }
    }
    pub fn adjust(&mut self, key: winit::keyboard::KeyCode) -> bool {
        use winit::keyboard::KeyCode;
        let limit = self.info.map_or(300.0, |i| i.duration);
        let previous_seconds = self.controls.seconds;
        match key {
            KeyCode::ArrowLeft => self.controls.seconds = (self.controls.seconds - 5.0).max(0.0),
            KeyCode::ArrowRight => self.controls.seconds = (self.controls.seconds + 5.0).min(limit),
            KeyCode::Equal => {
                self.controls.volume = self.controls.volume.saturating_add(5).min(100)
            }
            KeyCode::Minus => self.controls.volume = self.controls.volume.saturating_sub(5),
            _ => return false,
        }
        if self.controls.seconds != previous_seconds && self.observed == Observed::Ended {
            // An effective seek is not plain replay, even before the next App poll.
            // Ready invalidates the replay marker without claiming an applied offset.
            self.observed = Observed::Ready;
        }
        true
    }
    /// Only the pre-acquisition race is settled here. A real job/track still receives Pause.
    pub fn settle_unacquired_pause(&mut self, owns: bool) {
        if !owns && self.desired == Desired::Pause {
            self.desired = Desired::Stopped;
            self.requested = false;
            self.observed = Observed::Ready;
        }
    }
    pub fn stop(&mut self) {
        self.token.store(true, Ordering::Release);
        self.desired = Desired::Stopped;
        self.requested = false;
        self.observed = Observed::Ready;
        self.error = None;
        self.applied = None;
    }
    pub fn fail(&mut self, error: String) {
        self.desired = Desired::Stopped;
        self.requested = false;
        self.observed = Observed::Error;
        self.applied = None;
        self.error = Some(
            error
                .chars()
                .take(256)
                .flat_map(char::escape_default)
                .collect(),
        );
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.token.store(true, Ordering::Release);
    }
}
/// Validate before float->integer conversion or pointer arithmetic. Duration is derived
/// from real decoded frames; no wall clock or unqualified queue timeline is used.
fn pcm_tail(p: &pcm::Pcm, frame: usize) -> Result<&[i16]> {
    if p.samples.is_empty()
        || p.samples.len() > pcm::PCM_CAP / 2
        || !matches!(p.channels, 1 | 2)
        || !(8000..=96000).contains(&p.rate)
        || !p.samples.len().is_multiple_of(p.channels as usize)
        || p.frames != p.samples.len() / p.channels as usize
        || !p.duration.is_finite()
        || p.duration <= 0.0
        || p.duration != p.frames as f64 / f64::from(p.rate)
        || frame > p.frames
    {
        return Err("invalid validated PCM contract".into());
    }
    let sample = frame
        .checked_mul(p.channels as usize)
        .ok_or("PCM frame overflow")?;
    p.samples
        .get(sample..)
        .ok_or_else(|| "PCM frame outside source".into())
}
fn frame_for_seconds(p: &pcm::Pcm, seconds: f64) -> Result<usize> {
    pcm_tail(p, 0)?;
    if !seconds.is_finite() || seconds < 0.0 {
        return Err("invalid seek seconds".into());
    }
    if seconds >= p.duration {
        return Ok(p.frames);
    }
    let frame = (seconds * f64::from(p.rate)).floor();
    if !frame.is_finite() || frame < 0.0 || frame > p.frames as f64 {
        return Err("invalid seek frame".into());
    }
    Ok(frame as usize)
}
fn gain(volume: u8) -> Result<f32> {
    if volume > 100 {
        return Err("invalid queue gain".into());
    }
    Ok(f32::from(volume) / 100.0)
}
struct Job {
    token: Token,
    handle: JoinHandle<Result<(ReaderFile, pcm::Pcm)>>,
}
/// Retains the sole decoded PCM. Replacements borrow a suffix, never read/decode again.
struct Track {
    token: Token,
    _source: ReaderFile,
    pcm: pcm::Pcm,
    output: Option<Output>,
    desired: Desired,
    start: usize,
    queue_gain: Option<u8>,
    ended: bool,
}
pub(crate) struct Controller {
    job: Option<Job>,
    track: Option<Track>,
    pub(super) observed: Observed,
    pub error: Option<String>,
    poisoned: bool,
}
impl Default for Controller {
    fn default() -> Self {
        Self {
            job: None,
            track: None,
            observed: Observed::Ready,
            error: None,
            poisoned: false,
        }
    }
}
fn cancelled(t: &Token) -> bool {
    t.load(Ordering::Acquire)
}
impl Controller {
    pub fn busy(&self) -> bool {
        self.job.is_some() || self.track.is_some() || self.poisoned
    }
    pub fn disabled(&self) -> bool {
        self.poisoned
    }
    pub fn owns(&self, token: &Token) -> bool {
        self.job
            .as_ref()
            .is_some_and(|j| Arc::ptr_eq(&j.token, token))
            || self
                .track
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(&t.token, token))
    }
    pub(super) fn snapshot(&self, token: &Token) -> Option<(Info, Applied)> {
        let t = self.track.as_ref()?;
        if cancelled(&t.token) || !Arc::ptr_eq(&t.token, token) {
            return None;
        }
        Some((
            Info {
                frames: t.pcm.frames,
                rate: t.pcm.rate,
                duration: t.pcm.duration,
            },
            Applied {
                frame: t.start,
                queue_gain: t.queue_gain,
            },
        ))
    }
    /// E/open must not call this. Cancelled work retains its sole slot until joined.
    pub fn acquire_after_explicit_play(&mut self, token: Token, file: ReaderFile) -> Result<()> {
        if self.busy() {
            return Err("audio controller still occupied/draining".into());
        }
        if cancelled(&token) {
            return Err("audio generation retired".into());
        }
        let worker = token.clone();
        let handle = std::thread::Builder::new()
            .name("held-pcm".into())
            .spawn(move || {
                if cancelled(&worker) {
                    return Err("audio cancelled".into());
                }
                let bytes = file.read_image_bytes(&worker)?;
                let decoded = pcm::decode(&bytes, &worker)?;
                if cancelled(&worker) {
                    return Err("audio cancelled".into());
                }
                Ok((file, decoded))
            })
            .map_err(|_| "audio worker creation refused")?;
        self.job = Some(Job { token, handle });
        self.observed = Observed::Acquiring;
        self.error = None;
        Ok(())
    }
    fn fail(&mut self, message: String) {
        self.observed = Observed::Error;
        self.error = Some(message.chars().take(256).collect());
    }
    /// Compatibility entry point for stop/drain and prior no-device regressions.
    pub(super) fn poll_explicit(&mut self, current: Option<&Token>, desired: Desired) {
        self.poll_controls(current, desired, Controls::default());
    }
    pub(super) fn poll_controls(
        &mut self,
        current: Option<&Token>,
        desired: Desired,
        controls: Controls,
    ) {
        if self.track.as_ref().is_some_and(|t| {
            cancelled(&t.token) || current.is_none_or(|c| !Arc::ptr_eq(c, &t.token))
        }) {
            self.retire();
        }
        if let Some(job) = &self.job
            && (current.is_none_or(|c| !Arc::ptr_eq(c, &job.token))
                || desired == Desired::Stopped
                || self.poisoned)
        {
            job.token.store(true, Ordering::Release);
        }
        if self.job.as_ref().is_some_and(|j| j.handle.is_finished()) {
            let job = self.job.take().unwrap();
            let result = job
                .handle
                .join()
                .unwrap_or_else(|_| Err("audio worker panicked".into()));
            if !self.poisoned
                && !cancelled(&job.token)
                && current.is_some_and(|c| Arc::ptr_eq(c, &job.token))
                && desired != Desired::Stopped
            {
                match result {
                    Ok((source, pcm)) => {
                        self.track = Some(Track {
                            token: job.token,
                            _source: source,
                            pcm,
                            output: None,
                            desired: Desired::Stopped,
                            start: 0,
                            queue_gain: None,
                            ended: false,
                        });
                        self.observed = Observed::Prepared;
                    }
                    Err(error) => self.fail(error),
                }
            }
        }
        if self.poisoned {
            return;
        }
        let Some(mut track) = self.track.take() else {
            return;
        };
        // Stopped is not permission to keep a live output. A terminal retained track
        // has already closed; it can accept a prepared target without starting.
        if desired == Desired::Stopped && track.output.is_some() {
            self.retire_track(track);
            return;
        }
        let target = match gain(controls.volume)
            .and_then(|_| frame_for_seconds(&track.pcm, controls.seconds))
        {
            Ok(frame) => frame,
            Err(error) => {
                self.refuse_track(track, error);
                return;
            }
        };
        if target != track.start {
            // Once-only successful close is a prerequisite for replacement. Taking
            // output consumes the Rust handle even on failure; poison forbids reopen.
            if !self.close_output(&mut track) {
                track.token.store(true, Ordering::Release);
                return;
            }
            track.start = target;
            track.ended = false;
            track.desired = Desired::Stopped;
            self.observed = Observed::Prepared;
        }
        if desired == Desired::Stopped || (desired == Desired::Pause && track.output.is_none()) {
            self.observed = if track.ended {
                Observed::Ended
            } else {
                Observed::Prepared
            };
            self.track = Some(track);
            return;
        }
        if target == track.pcm.frames {
            if !self.close_output(&mut track) {
                track.token.store(true, Ordering::Release);
                return;
            }
            track.ended = true;
            track.desired = Desired::Stopped;
            self.observed = Observed::Ended;
            self.track = Some(track);
            return;
        }
        if track.output.is_none() {
            match Output::new_at(&track.pcm, track.start) {
                Ok(output) => {
                    track.output = Some(output);
                    track.ended = false;
                }
                Err((message, output)) => {
                    track.output = output;
                    self.refuse_track(track, message);
                    return;
                }
            }
        }
        if track.queue_gain != Some(controls.volume) {
            if let Err(error) = track.output.as_mut().unwrap().gain(controls.volume) {
                self.refuse_track(track, error);
                return;
            }
            track.queue_gain = Some(controls.volume);
        }
        let output = track.output.as_mut().unwrap();
        let result = if desired != track.desired {
            let r = output.set(desired);
            if r.is_ok() {
                track.desired = desired;
            }
            r.and_then(|_| output.state())
        } else {
            output.state()
        };
        match result {
            Ok(2) => {
                if !self.close_output(&mut track) {
                    track.token.store(true, Ordering::Release);
                    return;
                }
                track.ended = true;
                track.desired = Desired::Stopped;
                self.observed = Observed::Ended;
            }
            Ok(1) if desired == Desired::Play => self.observed = Observed::Playing,
            Ok(0) if desired == Desired::Pause => self.observed = Observed::Paused,
            Ok(_) => {
                self.observed = if desired == Desired::Play {
                    Observed::PlayRequested
                } else {
                    Observed::PauseRequested
                }
            }
            Err(error) => {
                self.refuse_track(track, error);
                return;
            }
        }
        self.track = Some(track);
    }
    fn record_close(&mut self, result: Result<()>) {
        if let Err(error) = result {
            self.poisoned = true;
            self.fail(error);
        }
    }
    fn close_output(&mut self, track: &mut Track) -> bool {
        if let Some(mut output) = track.output.take() {
            self.record_close(output.close());
        }
        track.queue_gain = None;
        !self.poisoned
    }
    fn retire_track(&mut self, mut track: Track) {
        track.token.store(true, Ordering::Release);
        self.close_output(&mut track);
        if !self.poisoned {
            self.observed = Observed::Ready;
        }
    }
    fn refuse_track(&mut self, track: Track, message: String) {
        self.retire_track(track);
        if !self.poisoned {
            self.fail(message);
        }
    }
    pub fn retire(&mut self) {
        if let Some(job) = &self.job {
            job.token.store(true, Ordering::Release);
        }
        // Take permanently BEFORE closing. Repeated retire/Drop cannot retry a failed handle.
        if let Some(track) = self.track.take() {
            self.retire_track(track);
        }
        if !self.poisoned {
            self.observed = Observed::Ready;
        }
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.retire();
        if let Some(job) = self.job.take() {
            let _ = job.handle.join();
        }
    }
}
/// A close attempt consumes the handle irrespective of success. Failed disposal
/// quarantines the C allocation, not a retryable Rust pointer.
#[cfg(any(test, target_os = "macos"))]
#[derive(Debug, PartialEq, Eq)]
enum Cleanup {
    Live,
    Closed,
    Quarantined,
}
#[cfg(any(test, target_os = "macos"))]
fn close_once<T>(
    handle: &mut Option<T>,
    cleanup: &mut Cleanup,
    dispose: impl FnOnce(T) -> i32,
) -> i32 {
    let Some(handle) = handle.take() else {
        return 0;
    };
    let status = dispose(handle);
    *cleanup = if status == 0 {
        Cleanup::Closed
    } else {
        Cleanup::Quarantined
    };
    status
}
#[cfg(any(test, target_os = "macos"))]
fn consume_creation_uncertainty<T>(
    uncertain: &mut bool,
    handle: &mut Option<T>,
    cleanup: &mut Cleanup,
) -> bool {
    if !std::mem::take(uncertain) {
        return false;
    }
    // C already quarantined the callback context, including a null marker.
    let _ = handle.take();
    *cleanup = Cleanup::Quarantined;
    true
}
#[cfg(all(target_os = "macos", not(test)))]
struct Output {
    handle: Option<std::ptr::NonNull<std::ffi::c_void>>,
    cleanup: Cleanup,
    creation_uncertain: bool,
}
#[cfg(all(target_os = "macos", not(test)))]
unsafe extern "C" {
    fn hair_pcm_open(
        samples: *const i16,
        count: u32,
        channels: u32,
        rate: u32,
        out: *mut *mut std::ffi::c_void,
        ownership: *mut u32,
    ) -> i32;
    fn hair_pcm_play(p: *mut std::ffi::c_void) -> i32;
    fn hair_pcm_pause(p: *mut std::ffi::c_void) -> i32;
    fn hair_pcm_gain(p: *mut std::ffi::c_void, gain: f32) -> i32;
    fn hair_pcm_state(p: *mut std::ffi::c_void, state: *mut u32) -> i32;
    fn hair_pcm_close(p: *mut std::ffi::c_void) -> i32;
}
#[cfg(all(target_os = "macos", not(test)))]
impl Output {
    fn new_at(p: &pcm::Pcm, frame: usize) -> std::result::Result<Self, (String, Option<Self>)> {
        let samples = pcm_tail(p, frame).map_err(|e| (e, None))?;
        if samples.is_empty() {
            return Err(("empty PCM tail has no output".into(), None));
        }
        let mut ptr = std::ptr::null_mut();
        let mut ownership = 0;
        // SAFETY: validated channel-aligned live source suffix; C copies it.
        let s = unsafe {
            hair_pcm_open(
                samples.as_ptr(),
                samples.len() as u32,
                p.channels,
                p.rate,
                &mut ptr,
                &mut ownership,
            )
        };
        let handle = std::ptr::NonNull::new(ptr);
        let output = if handle.is_some() || ownership == 2 {
            Some(Self {
                handle,
                cleanup: Cleanup::Live,
                creation_uncertain: ownership == 2,
            })
        } else {
            None
        };
        match (s, ownership, output) {
            (0, 1, Some(output)) => Ok(output),
            (status, _, output) => Err((format!("AudioQueue acquisition error {status}"), output)),
        }
    }
    fn gain(&mut self, volume: u8) -> Result<()> {
        let value = gain(volume)?;
        let ptr = self.handle.ok_or("audio output retired")?.as_ptr();
        // SAFETY: exclusively owned live queue; finite validated 0..1 gain.
        let s = unsafe { hair_pcm_gain(ptr, value) };
        if s == 0 {
            Ok(())
        } else {
            Err(format!("AudioQueue gain error {s}"))
        }
    }
    fn set(&mut self, desired: Desired) -> Result<()> {
        if desired == Desired::Stopped {
            return self.close();
        }
        let ptr = self.handle.ok_or("audio output retired")?.as_ptr();
        // SAFETY: live exclusively owned queue. Only redraw accesses it.
        let s = unsafe {
            match desired {
                Desired::Play => hair_pcm_play(ptr),
                Desired::Pause => hair_pcm_pause(ptr),
                Desired::Stopped => unreachable!(),
            }
        };
        if s == 0 {
            Ok(())
        } else {
            Err(format!("AudioQueue control error {s}"))
        }
    }
    fn state(&self) -> Result<u32> {
        let ptr = self.handle.ok_or("audio output retired")?.as_ptr();
        let mut state = 0;
        // SAFETY: owned live context and writable UInt32.
        let s = unsafe { hair_pcm_state(ptr, &mut state) };
        if s == 0 {
            Ok(state)
        } else {
            Err(format!("AudioQueue observation error {s}"))
        }
    }
    fn close(&mut self) -> Result<()> {
        if consume_creation_uncertainty(
            &mut self.creation_uncertain,
            &mut self.handle,
            &mut self.cleanup,
        ) {
            return Err("AudioQueue creation ownership uncertain (-9); output disabled".into());
        }
        let s = close_once(&mut self.handle, &mut self.cleanup, |ptr| {
            // SAFETY: called once outside callbacks. Failed C disposal quarantines.
            unsafe { hair_pcm_close(ptr.as_ptr()) }
        });
        if s == 0 {
            Ok(())
        } else {
            Err(format!("AudioQueue disposal error {s}; output disabled"))
        }
    }
}
#[cfg(all(target_os = "macos", not(test)))]
impl Drop for Output {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
#[cfg(any(test, not(target_os = "macos")))]
struct Output;
#[cfg(any(test, not(target_os = "macos")))]
impl Output {
    #[cfg(test)]
    fn new(p: &pcm::Pcm) -> std::result::Result<Self, (String, Option<Self>)> {
        Self::new_at(p, 0)
    }
    fn new_at(p: &pcm::Pcm, frame: usize) -> std::result::Result<Self, (String, Option<Self>)> {
        pcm_tail(p, frame).map_err(|e| (e, None))?;
        Err((
            if cfg!(test) {
                "DENIED_TEST_AUDIO_OUTPUT"
            } else {
                "audio output unsupported on this platform"
            }
            .into(),
            None,
        ))
    }
    fn gain(&mut self, volume: u8) -> Result<()> {
        gain(volume)?;
        Err("audio output denied".into())
    }
    fn set(&mut self, _: Desired) -> Result<()> {
        Err("audio output denied".into())
    }
    fn state(&self) -> Result<u32> {
        Err("audio output denied".into())
    }
    fn close(&mut self) -> Result<()> {
        Ok(())
    }
}
#[cfg(test)]
#[path = "audio/controls_tests.rs"]
mod controls_tests;
#[cfg(all(test, target_os = "macos"))]
#[path = "audio/theme_appearance_tests.rs"]
mod theme_appearance_tests;
#[cfg(all(test, target_os = "macos"))]
#[path = "audio/unity_default_tests.rs"]
mod unity_default_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_test_output_is_denied_not_simulated() {
        let p = pcm::Pcm {
            samples: vec![123],
            channels: 1,
            rate: 8000,
            frames: 1,
            duration: 1.0 / 8000.0,
        };
        let result = Output::new(&p);
        assert!(
            matches!(result, Err((ref message, None)) if message == "DENIED_TEST_AUDIO_OUTPUT")
        );
    }
    #[test]
    fn failed_disposal_consumes_handle_and_never_retries() {
        let mut handle = Some(17_u32);
        let mut cleanup = Cleanup::Live;
        let mut attempts = 0;
        assert_eq!(
            close_once(&mut handle, &mut cleanup, |_| {
                attempts += 1;
                -77
            }),
            -77
        );
        assert_eq!(cleanup, Cleanup::Quarantined);
        assert!(handle.is_none());
        assert_eq!(
            close_once(&mut handle, &mut cleanup, |_| {
                attempts += 1;
                0
            }),
            0
        );
        assert_eq!(attempts, 1);
        assert_eq!(cleanup, Cleanup::Quarantined);
    }
    #[test]
    fn held_wav_pause_completion_retains_pcm_until_explicit_replay_denied() {
        use hair::local_explorer::Explorer;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hair-audio-prepared-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        let mut bytes = Vec::from(&b"RIFF"[..]);
        bytes.extend_from_slice(&40_u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&8000_u32.to_le_bytes());
        bytes.extend_from_slice(&16000_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&4_u32.to_le_bytes());
        bytes.extend_from_slice(&123_i16.to_le_bytes());
        bytes.extend_from_slice(&(-456_i16).to_le_bytes());
        std::fs::write(path.join("held.bin"), bytes).unwrap();
        let explorer = Explorer::new(&path).unwrap();
        let token = Arc::new(AtomicBool::new(false));
        let mut controller = Controller::default();
        controller
            .acquire_after_explicit_play(token.clone(), explorer.open_reader(0).unwrap())
            .unwrap();
        // Synchronize the actual worker, not a timer/output simulation.
        while !controller.job.as_ref().unwrap().handle.is_finished() {
            std::thread::yield_now();
        }
        controller.poll_explicit(Some(&token), Desired::Pause);
        assert_eq!(controller.observed, Observed::Prepared);
        let track = controller.track.as_ref().unwrap();
        assert!(track.output.is_none());
        assert_eq!(track.pcm.samples, [123, -456]);
        assert!(controller.job.is_none());
        controller.poll_explicit(Some(&token), Desired::Pause);
        assert_eq!(controller.observed, Observed::Prepared);
        controller.poll_explicit(Some(&token), Desired::Play);
        assert_eq!(controller.observed, Observed::Error);
        assert_eq!(
            controller.error.as_deref(),
            Some("DENIED_TEST_AUDIO_OUTPUT")
        );
        assert!(controller.track.is_none() && controller.job.is_none());
        // Narrow production-policy boundary, NOT simulated Output or Apple
        // failure: consume both uncertain-creation markers exactly once and
        // feed their failure into the same controller retirement poison path.
        for initial in [None, Some(17_u32)] {
            let mut handle = initial;
            let mut uncertain = true;
            let mut cleanup = Cleanup::Live;
            assert!(consume_creation_uncertainty(
                &mut uncertain,
                &mut handle,
                &mut cleanup,
            ));
            assert!(handle.is_none());
            assert_eq!(cleanup, Cleanup::Quarantined);
            assert!(!consume_creation_uncertainty(
                &mut uncertain,
                &mut handle,
                &mut cleanup,
            ));
            let mut dispose_attempts = 0;
            assert_eq!(
                close_once(&mut handle, &mut cleanup, |_| {
                    dispose_attempts += 1;
                    0
                }),
                0
            );
            assert_eq!(dispose_attempts, 0);
            controller.record_close(Err("uncertain creation policy (-9)".into()));
            assert!(controller.disabled() && controller.busy());
            let replacement = Arc::new(AtomicBool::new(false));
            assert!(
                controller
                    .acquire_after_explicit_play(
                        replacement.clone(),
                        explorer.open_reader(0).unwrap(),
                    )
                    .is_err()
            );
            controller.poll_explicit(Some(&replacement), Desired::Play);
            controller.retire();
            assert!(controller.disabled());
            assert_eq!(controller.observed, Observed::Error);
            assert!(controller.track.is_none() && controller.job.is_none());
        }
        drop(controller);
        drop(explorer);
        std::fs::remove_dir_all(path).unwrap();
    }
}
