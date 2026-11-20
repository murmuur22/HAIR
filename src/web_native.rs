//! One bounded worker. No blocking owner Drop, network, pack loading or unfinished join in callbacks.
use super::*;
use hair::{
    acquisition::Cancellation,
    web::{Address, Door, Generated, HtmlSession, Page, WindowAction, WindowDoor},
};
use std::{sync::Arc, thread::JoinHandle};
#[path = "web_visited.rs"]
pub(super) mod visited;
// Count admission, separate from the unchanged process-wide held-source ledger.
// Visited cache retains only first-window Pages; current/history alone own scenes.
const MAX_HISTORY: usize = 8;
pub(super) const FAILURE_RED_DURATION: Duration = Duration::from_secs(2);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum FeedbackPhase {
    #[default]
    Idle,
    Pending,
    Success,
    Failure,
    Cancelled,
}
/// One terminal observation, independent of Runtime so initial failure survives
/// restoring bundled interaction. New explicit work replaces the visible observation.
pub(super) struct FeedbackRecord {
    phase: FeedbackPhase,
    destination: String,
    reason: Option<String>,
    at: Instant,
    cached: bool,
}
impl Default for FeedbackRecord {
    fn default() -> Self {
        Self {
            phase: FeedbackPhase::Idle,
            destination: String::new(),
            reason: None,
            at: Instant::now(),
            cached: false,
        }
    }
}
/// Renderer-facing state only; no geometry, animation delay or input gate.
#[allow(dead_code)] // Full destination/timing/reason are for the separate renderer slice.
pub(super) struct Feedback<'a> {
    pub phase: FeedbackPhase,
    pub destination: &'a str,
    pub reason: Option<&'a str>,
    pub elapsed: Duration,
    pub cached: bool,
    pub red_eligible: bool,
    pub retiring: bool,
}
const MAX_PREPARED_THEMES: usize = 8;
const MAX_CONTINUATION_VERTICES: usize = 2_400_000;
pub(super) type Fetcher = fn(Address, &Cancellation, Instant) -> Result<Page>;
pub(super) struct Prepared {
    pub generated: Generated,
    pub themes: Vec<hair::theme::Theme>,
    /// Cache hits additionally prepare their CPU scenes on the same worker.
    cached_scenes: Option<Vec<Vec<hair::assets::Vertex>>>,
    continuation: Option<ContinuationArrival>,
}
struct ContinuationArrival {
    pose: Pose,
    scenes: Vec<Vec<hair::assets::Vertex>>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Travel {
    Next,
    Previous,
}
impl Travel {
    fn reciprocal(self) -> &'static str {
        match self {
            Self::Next => "web-previous-window",
            Self::Previous => "web-next-window",
        }
    }
}
/// Session visited addresses: order-preserving union, oldest dropped at the bound.
fn merge_visited(old: &[String], new: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for address in old.iter().chain(new) {
        out.retain(|a| a != address);
        out.push(address.clone());
    }
    let excess = out.len().saturating_sub(hair::web::MAX_VISITED_ADDRESSES);
    out.drain(..excess);
    out
}
pub(super) fn prepare(
    a: Address,
    sources: &[PathBuf],
    fetch: Fetcher,
    cancel: &Cancellation,
) -> Result<Prepared> {
    let deadline = Instant::now() + Duration::from_secs(10);
    admit_themes(sources)?;
    cancel.check(deadline)?;
    let generated = fetch(a, cancel, deadline)?.generate()?;
    prepare_generated(generated, sources, cancel, deadline, None)
}
fn admit_themes(sources: &[PathBuf]) -> Result<()> {
    if sources.is_empty() || sources.len() > MAX_PREPARED_THEMES {
        return Err("web preparation requires one to eight appearance packs".into());
    }
    Ok(())
}
fn prepare_generated(
    generated: Generated,
    sources: &[PathBuf],
    cancel: &Cancellation,
    deadline: Instant,
    travel: Option<Travel>,
) -> Result<Prepared> {
    cancel.check(deadline)?;
    let mut cache = hair::theme::AssetCache::default();
    let themes = sources
        .iter()
        .map(|p| {
            cancel.check(deadline)?;
            hair::theme::Theme::load_for_scene(p, &mut cache, &generated.host)
        })
        .collect::<Result<Vec<_>>>()?;
    let continuation = if let Some(travel) = travel {
        let host = &generated.host;
        let anchor = host
            .anchors
            .iter()
            .find(|a| a.id == travel.reciprocal())
            .ok_or("continuation has no reciprocal doorway")?;
        let inward = host.portal_inward(&anchor.id);
        // The same doorway-local clearance policy used by folder arrivals, not a world Z offset.
        let pose = Pose {
            position: [
                anchor.at[0] + inward[0] * 1.05,
                anchor.at[2] + inward[1] * 1.05,
            ],
            yaw: inward[0].atan2(-inward[1]),
            pitch: 0.0,
        };
        if host.blocked(pose.position) {
            return Err("continuation doorway arrival is blocked".into());
        }
        host.check_routes(&[pose.position])?;
        let mut scenes = Vec::new();
        let mut count = 0usize;
        for theme in &themes {
            cancel.check(deadline)?;
            let scene = theme.vertices_for(&generated.world);
            count = count
                .checked_add(scene.len())
                .ok_or("continuation scene accounting overflow")?;
            if count > MAX_CONTINUATION_VERTICES {
                return Err("continuation staged scene vertex budget exceeded".into());
            }
            scenes.push(scene);
        }
        Some(ContinuationArrival { pose, scenes })
    } else {
        None
    };
    cancel.check(deadline)?;
    Ok(Prepared {
        generated,
        themes,
        continuation,
        cached_scenes: None,
    })
}
fn prepare_cached(page: Arc<Page>, sources: &[PathBuf], cancel: &Cancellation) -> Result<Prepared> {
    let deadline = Instant::now() + Duration::from_secs(10);
    admit_themes(sources)?;
    cancel.check(deadline)?;
    // Clone only this saved first-window projection, on the worker, not the callback.
    // Its immutable HtmlSession remains shared with current/history/cache.
    let mut prepared =
        prepare_generated((*page).clone().generate()?, sources, cancel, deadline, None)?;
    let mut scenes = Vec::new();
    let mut count = 0usize;
    for theme in &prepared.themes {
        cancel.check(deadline)?;
        let scene = theme.vertices_for(&prepared.generated.world);
        count = count
            .checked_add(scene.len())
            .ok_or("cached scene accounting overflow")?;
        if count > MAX_CONTINUATION_VERTICES {
            return Err("cached staged scene vertex budget exceeded".into());
        }
        scenes.push(scene);
    }
    cancel.check(deadline)?;
    prepared.cached_scenes = Some(scenes);
    Ok(prepared)
}
fn prepare_window(
    source: Arc<HtmlSession>,
    action: WindowAction,
    sources: &[PathBuf],
    cancel: &Cancellation,
) -> Result<Prepared> {
    let deadline = Instant::now() + Duration::from_secs(10);
    admit_themes(sources)?;
    cancel.check(deadline)?;
    let (cursor, travel) = match action {
        WindowAction::Next(cursor) => (cursor, Travel::Next),
        WindowAction::Previous(cursor) => (cursor, Travel::Previous),
    };
    // The only acquisition here is from the held Arc. No Fetcher is passed to this path.
    let page = source.window(&cursor, Some((cancel, deadline)))?;
    cancel.check(deadline)?;
    prepare_generated(page.generate()?, sources, cancel, deadline, Some(travel))
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Initial,
    Follow,
    Refresh,
    Continuation(Travel),
}
pub(super) struct Job {
    handle: JoinHandle<Result<Prepared>>,
    cancel: Cancellation,
    discard: bool,
    mode: Mode,
    generation: u64,
    destination: String,
    cached: bool,
    started: Instant,
}
impl Job {
    #[cfg(test)]
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }
}
pub(super) struct Snapshot {
    world: World,
    themes: Vec<hair::theme::Theme>,
    pose: Pose,
    doors: Vec<Door>,
    page: Option<Page>,
    window_doors: Vec<WindowDoor>,
    scroll: usize,
}
pub(super) struct Runtime {
    pub doors: Vec<Door>,
    pub page: Option<Page>,
    pub window_doors: Vec<WindowDoor>,
    pub cache: visited::VisitedCache,
    generation: u64,
    pub history: Vec<Snapshot>,
    pub pending: Option<Job>,
    pub status: String,
    pub fetcher: Fetcher,
    pub back_held: bool,
    pub refresh_held: bool,
}
impl Runtime {
    pub fn new(doors: Vec<Door>, fetcher: Fetcher) -> Self {
        Self {
            doors,
            page: None,
            window_doors: Vec::new(),
            cache: Default::default(),
            generation: 0,
            history: Vec::new(),
            pending: None,
            status: "R return | F5 refresh | Tab/E links | T pack".into(),
            fetcher,
            back_held: false,
            refresh_held: false,
        }
    }
    #[cfg(test)]
    pub fn invalidate_generation_for_test(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
    fn cancel(&mut self) {
        if let Some(j) = &mut self.pending {
            j.discard = true;
            j.cancel.cancel();
        }
    }
    /// Called only AFTER run_app returns, never from an event handler or Drop.
    pub fn shutdown(&mut self) {
        self.cancel();
        if let Some(j) = self.pending.take() {
            let _ = j.handle.join();
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.cancel();
    }
} // no join; normal path uses explicit post-loop shutdown
impl FeedbackRecord {
    fn terminal(
        phase: FeedbackPhase,
        destination: &str,
        reason: Option<&str>,
        cached: bool,
    ) -> Self {
        Self {
            phase,
            destination: destination.into(),
            reason: reason.map(str::to_owned),
            at: Instant::now(),
            cached,
        }
    }
}
impl App {
    #[allow(dead_code)] // State-only API: procedural rendering is deliberately a later slice.
    pub(super) fn web_feedback(&self, now: Instant) -> Feedback<'_> {
        if let Some(j) = self.web.as_ref().and_then(|w| w.pending.as_ref())
            && !(j.discard
                && self.web_completion.phase == FeedbackPhase::Failure
                && self.web_completion.at >= j.started)
        {
            return Feedback {
                phase: if j.discard {
                    FeedbackPhase::Cancelled
                } else {
                    FeedbackPhase::Pending
                },
                destination: &j.destination,
                reason: None,
                elapsed: now.saturating_duration_since(j.started),
                cached: j.cached,
                red_eligible: false,
                retiring: j.discard,
            };
        }
        let record = &self.web_completion;
        let elapsed = now.saturating_duration_since(record.at);
        Feedback {
            phase: record.phase,
            destination: &record.destination,
            reason: record.reason.as_deref(),
            elapsed,
            cached: record.cached,
            red_eligible: record.phase == FeedbackPhase::Failure && elapsed < FAILURE_RED_DURATION,
            retiring: self
                .web
                .as_ref()
                .and_then(|w| w.pending.as_ref())
                .is_some_and(|j| j.discard),
        }
    }
    // Validation/admission errors are terminal only when there is no real job.
    // A repeated request cannot replace the feedback for a running/retiring worker.
    fn web_request_error(&mut self, destination: &str, error: &str) {
        if self.web.as_ref().is_none_or(|w| w.pending.is_none()) {
            self.web_completion =
                FeedbackRecord::terminal(FeedbackPhase::Failure, destination, Some(error), false);
        }
    }
    pub(super) fn request_window(&mut self, action: WindowAction) -> Result<()> {
        let result = self.request_window_inner(action);
        if let Err(e) = &result {
            let destination = self.world.current.clone();
            self.web_request_error(&destination, e);
        }
        result
    }
    fn request_window_inner(&mut self, action: WindowAction) -> Result<()> {
        let web = self.web.as_mut().ok_or("not web mode")?;
        if web.pending.is_some() {
            return Err("preparation/retirement pending; try again after completion".into());
        }
        let page = web
            .page
            .as_ref()
            .ok_or("current held Page unavailable; explicit F5 resets source")?;
        if page.address.original != self.world.current {
            return Err("current Page/address mismatch".into());
        }
        let (cursor, travel, checked) = match &action {
            WindowAction::Next(cursor) => (cursor, Travel::Next, page.next_cursor()?),
            WindowAction::Previous(cursor) => (cursor, Travel::Previous, page.previous_cursor()?),
        };
        if checked.as_ref() != Some(cursor) || !web.window_doors.iter().any(|d| d.action == action)
        {
            return Err("stale continuation binding refused".into());
        }
        let room_number = cursor.index() + 1;
        let source = page.source.as_ref().ok_or("missing held source")?.clone();
        let sources = self
            .themes
            .iter()
            .map(|t| t.source.clone())
            .collect::<Vec<_>>();
        admit_themes(&sources)?;
        let cancel = Cancellation::default();
        let worker_cancel = cancel.clone();
        let handle = std::thread::Builder::new()
            .name("hair-web-window".into())
            .spawn(move || prepare_window(source, action, &sources, &worker_cancel))
            .map_err(|e| e.to_string())?;
        web.pending = Some(Job {
            handle,
            cancel,
            discard: false,
            mode: Mode::Continuation(travel),
            generation: web.generation,
            destination: self.world.current.clone(),
            cached: false,
            started: Instant::now(),
        });
        web.status = format!(
            "Preparing held-response room {room_number}; no fetch; last good place retained; R cancels"
        );
        self.release();
        Ok(())
    }
    pub(super) fn request_web(&mut self, raw: &str, mode: Mode) -> Result<()> {
        let result = self.request_web_inner(raw, mode);
        if let Err(e) = &result {
            self.web_request_error(raw, e);
        }
        result
    }
    fn request_web_inner(&mut self, raw: &str, mode: Mode) -> Result<()> {
        if matches!(mode, Mode::Continuation(_)) {
            return Err("continuation requires a typed held-response action, not a URL".into());
        }
        let address = Address::parse(raw)?;
        let web = self.web.as_mut().ok_or("not web mode")?;
        if web.pending.is_some() {
            return Err("acquisition/retirement pending; destination unfinished, try again after completion".into());
        }
        if mode == Mode::Follow && web.history.len() >= MAX_HISTORY {
            return Err(
                "eight-page history budget reached; R returns before further acquisition".into(),
            );
        }
        let sources = self
            .themes
            .iter()
            .map(|t| t.source.clone())
            .collect::<Vec<_>>();
        admit_themes(&sources)?;
        let cancel = Cancellation::default();
        let worker_cancel = cancel.clone();
        let fetch = web.fetcher;
        // Follow re-enters the saved FIRST window, never a remembered continuation
        // ordinal or scene. Refresh and Initial always call the Fetcher.
        let saved = if mode == Mode::Follow {
            web.cache.get(&address.original)
        } else {
            None
        };
        let cached = saved.is_some();
        let handle = std::thread::Builder::new()
            .name("hair-web-page".into())
            .spawn(move || match saved {
                Some(page) => prepare_cached(page, &sources, &worker_cancel),
                None => prepare(address, &sources, fetch, &worker_cancel),
            })
            .map_err(|e| e.to_string())?;
        web.pending = Some(Job {
            handle,
            cancel,
            discard: false,
            mode,
            generation: web.generation,
            destination: raw.to_owned(),
            cached,
            started: Instant::now(),
        });
        web.status = if cached {
            format!(
                "Preparing cached source {raw}; first room arrival; no fetch; last good place retained; R cancels"
            )
        } else {
            format!("Loading {raw}; last good place retained; R cancels")
        };
        self.release();
        Ok(())
    }
    pub(super) fn poll_web(&mut self) -> Result<()> {
        let Some(web) = &mut self.web else {
            return Ok(());
        };
        if !web.pending.as_ref().is_some_and(|j| j.handle.is_finished()) {
            return Ok(());
        }
        let j = web.pending.take().unwrap();
        let result = j
            .handle
            .join()
            .unwrap_or_else(|_| Err("page worker panicked".into()));
        if j.discard || j.generation != web.generation {
            // A real Back appearance failure after cancelling this job is a newer
            // observation; retirement must not erase its retained reason/red timing.
            if self.web_completion.phase != FeedbackPhase::Failure
                || self.web_completion.at < j.started
            {
                self.web_completion = FeedbackRecord::terminal(
                    FeedbackPhase::Cancelled,
                    &j.destination,
                    None,
                    j.cached,
                );
                web.status = "Cancelled; last good place retained".into();
            }
            if j.mode == Mode::Initial && !self.world.current.starts_with("https://") {
                // Retire the cancelled worker before restoring bundled interaction.
                self.web = None;
            }
            return Ok(());
        }
        let mut p = match result {
            Ok(p) => p,
            Err(e) => {
                if j.mode == Mode::Follow
                    && let Some(pocket) = &mut self.world.pocket
                {
                    let mut chars = e.chars().filter(|c| !c.is_control());
                    let mut reason: String = chars.by_ref().take(95).collect();
                    if chars.next().is_some() {
                        reason.push('~');
                    }
                    pocket.failed_arrival = Some((j.destination.clone(), reason));
                }
                self.web_completion = FeedbackRecord::terminal(
                    FeedbackPhase::Failure,
                    &j.destination,
                    Some(&e),
                    j.cached,
                );
                web.status = format!("Refused: {e}; last good place retained");
                eprintln!("WEB {}", web.status);
                if j.mode == Mode::Initial && !self.world.current.starts_with("https://") {
                    let message = web.status.clone();
                    self.web = None;
                    if self.focused {
                        // Async entry must obey the same focus/input boundary as L.
                        self.release();
                        self.address_error = Some(message);
                        self.address = Some(String::new());
                    }
                }
                return Ok(());
            }
        };
        for (next, previous) in p.themes.iter_mut().zip(&self.themes) {
            if previous
                .sky_feedback()
                .is_some_and(|s| s.starts_with("SKY_REFUSED"))
            {
                next.retain_load_failure(previous);
            }
        }
        let mut removed_selections = 0;
        let mut reader_recovered = false;
        // F5 acquires a NEW response of the exact URL. A later room has no ordinal
        // authority in those bytes: explicitly reset to room 1, never rebind index N.
        let source_reset = j.mode == Mode::Refresh
            && web
                .page
                .as_ref()
                .and_then(|p| p.window.as_ref())
                .is_some_and(|w| w.index() > 0);
        if j.mode == Mode::Refresh && !source_reset {
            let old_host = &self.themes[self.theme_index].host;
            let host = &p.generated.host;
            if p.generated.world.current != self.world.current
                || host.room != old_host.room
                || host.spawn != old_host.spawn
                || host.height != old_host.height
                || host.geometry != old_host.geometry
                || host.overhead != old_host.overhead
                || host.blocked(self.pose.position)
            {
                let reason = "refresh changed shared room or safe pose";
                self.web_completion = FeedbackRecord::terminal(
                    FeedbackPhase::Failure,
                    &j.destination,
                    Some(reason),
                    j.cached,
                );
                web.status = format!("Refused: {reason}; last good place retained");
                return Ok(());
            }
            // Refresh updates source content, not the visitor's navigation transaction.
            // Rebind surviving reader selection by source/target identity, never old ordinal.
            let next = &mut p.generated.world;
            next.reading = self.world.reading;
            if let (Some(old), Some(new)) = (&self.world.pocket, &mut next.pocket)
                && let Some((target, reason)) = &old.failed_arrival
                && next
                    .site
                    .as_ref()
                    .and_then(|s| s.records.get(&next.current))
                    .is_some_and(|links| {
                        links.iter().any(|l| {
                            l.destination == hair::content::Destination::External(target.clone())
                        })
                    })
            {
                new.failed_arrival = Some((target.clone(), reason.clone()));
            }
            if let (Some(old), Some(new)) = (&self.world.pocket, &mut next.pocket) {
                new.collection_recovered = old.collection_recovered;
                if let Some(selected) = old.reading.as_ref().and_then(|a| old.documents.get(a)) {
                    // Overflow grouping can hide duplicate titles in either pocket.
                    let has_overflow = old
                        .documents
                        .values()
                        .chain(new.documents.values())
                        .any(|d| d.title == "Multiple sections");
                    let unique_origin = !has_overflow
                        && selected.title != "Multiple sections"
                        && old
                            .documents
                            .values()
                            .filter(|d| d.title == selected.title)
                            .count()
                            == 1;
                    let mut matches = new
                        .documents
                        .iter()
                        .filter(|(_, d)| d.title == selected.title);
                    let first = matches.next().map(|(anchor, _)| anchor.clone());
                    new.reading = if unique_origin && matches.next().is_none() {
                        first
                    } else {
                        None
                    };
                    reader_recovered = new.reading.is_none();
                    new.collection_recovered = reader_recovered;
                }
            }
            next.tree_collapsed = self.world.tree_collapsed;
            next.reader_link = self.world.selected_reader_link().and_then(|s| {
                next.reader_links()
                    .into_iter()
                    .enumerate()
                    .find_map(|(ordinal, link)| {
                        (link.raw == s.link.raw && link.destination == s.link.destination).then(
                            || hair::ReaderSelection {
                                page: next.current.clone(),
                                ordinal,
                                link,
                            },
                        )
                    })
            });
            removed_selections = hair::web::portals::retain(&self.world, next);
            next.explicit_target = self.world.explicit_target.clone();
            hair::interaction::refresh(host, &self.pose, next);
        }
        // GPU material validation before committing page/history; pack decode was on the worker.
        if let Some(r) = &mut self.renderer
            && let Err(e) = r.set_theme_appearance(&p.themes[self.theme_index])
        {
            self.web_completion = FeedbackRecord::terminal(
                FeedbackPhase::Failure,
                &j.destination,
                Some(&e),
                j.cached,
            );
            web.status = format!("Refused: {e}; last good place retained");
            return Ok(());
        }
        if j.mode == Mode::Follow
            && let Some(pocket) = &mut self.world.pocket
            && pocket
                .failed_arrival
                .as_ref()
                .is_some_and(|(target, _)| target == &j.destination)
        {
            pocket.failed_arrival = None;
        }
        let continuation = p.continuation.take();
        // All fallible worker/arrival/material staging is complete before these swaps.
        let old = Snapshot {
            world: std::mem::replace(&mut self.world, p.generated.world),
            themes: std::mem::replace(&mut self.themes, p.themes),
            pose: self.pose,
            doors: std::mem::replace(&mut web.doors, p.generated.doors),
            page: web.page.replace(p.generated.page),
            window_doors: std::mem::replace(&mut web.window_doors, p.generated.window_doors),
            scroll: self.scroll,
        };
        // Return-door context mirrors exactly what R would restore: a Follow
        // pushes the old page; same-page work keeps the existing history top.
        let previous = match j.mode {
            Mode::Follow => Some(hair::web::PreviousPage {
                // Generated rooms store the fetched title as their first line.
                title: old.page.as_ref().map_or_else(
                    || {
                        old.world
                            .site
                            .as_ref()
                            .and_then(|s| s.pages.get(&old.world.current))
                            .and_then(|text| text.lines().next())
                            .unwrap_or_default()
                            .to_string()
                    },
                    |p| p.title.clone(),
                ),
                address: old.world.current.clone(),
            }),
            Mode::Refresh | Mode::Continuation(_) => {
                old.world.pocket.as_ref().and_then(|p| p.previous.clone())
            }
            Mode::Initial => None,
        };
        let mut history = match j.mode {
            Mode::Initial => Vec::new(),
            _ => old
                .world
                .pocket
                .as_ref()
                .map(|p| p.history.clone())
                .unwrap_or_default(),
        };
        if j.mode == Mode::Follow
            && let Some(previous) = &previous
        {
            history.push(previous.clone());
        }
        if j.mode != Mode::Initial {
            // The navigation panel's M state is a visitor preference, not page data.
            self.world.tree_collapsed = old.world.tree_collapsed;
        }
        let old_visited = old.world.pocket.as_ref().map(|p| p.visited.as_slice());
        if let Some(pocket) = &mut self.world.pocket {
            pocket.previous = previous;
            pocket.history = history;
            pocket.visited = merge_visited(old_visited.unwrap_or(&[]), &pocket.visited);
        }
        web.generation = web.generation.wrapping_add(1);
        if j.mode == Mode::Follow {
            web.history.push(old);
        }
        // Admit only committed first-window versions. Continuation never accumulates
        // Pages/scenes in the visited cache, and cached arrivals reuse its saved Page.
        // Admission is optional: cache eviction/drop cannot refuse active/history state.
        if !matches!(j.mode, Mode::Continuation(_))
            && !j.cached
            && let Some(page) = &web.page
        {
            let _ = web.cache.admit(page);
        }
        self.web_completion =
            FeedbackRecord::terminal(FeedbackPhase::Success, &j.destination, None, j.cached);
        // Continuation drops the preceding scene/materials instead of pushing page history.
        if let Some(arrival) = &continuation {
            self.pose = arrival.pose;
        } else if j.mode != Mode::Refresh || source_reset {
            self.themes[self.theme_index].respawn(&mut self.pose);
        }
        if j.mode != Mode::Refresh || reader_recovered || source_reset {
            self.scroll = 0;
        }
        // Any committed document change drops a sub-row wheel remainder.
        self.wheel_rows = 0.0;
        web.status = if j.cached {
            "Cached stable source; first room arrival (no fetch) | R exact history return | F5 fetches fresh source".into()
        } else if source_reset {
            "Refreshed exact URL; new held source; reset to room 1 | R return | F5 refresh".into()
        } else if matches!(j.mode, Mode::Continuation(_)) {
            "Held-response room ready (no fetch) | Next/Previous doors | R page return | F5 resets source".into()
        } else if reader_recovered {
            "Reading collection changed or was removed; full page restored | R return | F5 refresh"
                .into()
        } else if removed_selections > 0 {
            format!(
                "{removed_selections} selected destination(s) removed; recovered to first surviving target where available; R return | F5 refresh"
            )
        } else {
            "R return | F5 refresh | Tab/Shift+Tab doorway destinations | T pack".into()
        };
        println!(
            "WEB_ARRIVAL source={} history={} pack={}",
            self.world.current,
            web.history.len(),
            self.themes[self.theme_index].name
        );
        self.release();
        if let Some(r) = &mut self.renderer {
            if let Some(arrival) = &continuation {
                // CPU scene preparation was on the worker; callback only uploads the ready scene.
                r.set_scene(&arrival.scenes[self.theme_index]);
            } else if let Some(scenes) = &p.cached_scenes {
                r.set_scene(&scenes[self.theme_index]);
            } else {
                r.set_scene(&self.themes[self.theme_index].vertices_for(&self.world));
            }
        }
        Ok(())
    }
    pub(super) fn return_web(&mut self) -> Result<()> {
        let web = self.web.as_mut().ok_or("not web mode")?;
        let continuation_pending = web
            .pending
            .as_ref()
            .is_some_and(|j| matches!(j.mode, Mode::Continuation(_)));
        if let Some(j) = &web.pending {
            self.web_completion =
                FeedbackRecord::terminal(FeedbackPhase::Cancelled, &j.destination, None, j.cached);
        }
        web.cancel();
        if continuation_pending {
            // Continuation cancellation retains this exact window and page history.
            web.status =
                "Cancelling; last good place retained; waiting for worker retirement".into();
            self.release();
            return Ok(());
        }
        let Some(s) = web.history.last() else {
            web.status = if web.pending.is_some() {
                "Cancelling; at surviving entry; last good place retained; waiting for worker retirement".into()
            } else {
                "At surviving entry; last good place retained".into()
            };
            return Ok(());
        };
        if let Some(r) = &mut self.renderer
            && let Err(e) = r.set_theme_appearance(&s.themes[self.theme_index])
        {
            self.web_completion =
                FeedbackRecord::terminal(FeedbackPhase::Failure, &s.world.current, Some(&e), false);
            web.status = format!("Refused: {e}; last good place retained");
            return Ok(());
        }
        let s = web.history.pop().unwrap();
        let collapsed = self.world.tree_collapsed;
        let visited = self
            .world
            .pocket
            .as_ref()
            .map(|p| p.visited.clone())
            .unwrap_or_default();
        self.world = s.world;
        self.world.tree_collapsed = collapsed;
        if let Some(pocket) = &mut self.world.pocket {
            pocket.visited = merge_visited(&pocket.visited, &visited);
        }
        self.themes = s.themes;
        self.pose = s.pose;
        self.scroll = s.scroll;
        // The restored row is exact; a remainder from the left page is not.
        self.wheel_rows = 0.0;
        web.doors = s.doors;
        web.page = s.page;
        web.window_doors = s.window_doors;
        web.generation = web.generation.wrapping_add(1);
        if web.pending.is_none() {
            self.web_completion =
                FeedbackRecord::terminal(FeedbackPhase::Success, &self.world.current, None, false);
        }
        web.status =
            "Returned from local history; exact held window restored; F5 resets source".into();
        self.release();
        if let Some(r) = &mut self.renderer {
            r.set_scene(&self.themes[self.theme_index].vertices_for(&self.world));
        }
        Ok(())
    }
    pub(super) fn interact_web(&mut self) -> Result<()> {
        if !self.world.current.starts_with("https://") {
            return Err(
                "initial web destination unfinished; bundled last-good entry retained".into(),
            );
        }
        if self.world.reading {
            let target = self
                .world
                .selected_reader_link()
                .and_then(|s| {
                    if let hair::content::Destination::External(raw) = &s.link.destination {
                        Some(raw.clone())
                    } else {
                        None
                    }
                })
                .ok_or("select a supported HTTPS reader link with Tab")?;
            return self.request_web(&target, Mode::Follow);
        }
        let Some(t) = hair::interaction::resolve_with_windows(
            &self.themes[self.theme_index].host,
            &self.pose,
            &self.world,
            self.web.as_ref().map_or(&[], |w| w.window_doors.as_slice()),
        ) else {
            return Ok(());
        };
        if t.kind == hair::interaction::Kind::Read {
            self.world.interaction = Some(t);
            self.world.act(Action::Inspect, self.pose.position);
            self.scroll = 0;
            self.wheel_rows = 0.0;
            self.release();
            return Ok(());
        }
        if t.anchor == "return-history" {
            return self.return_web();
        }
        if let Some(action) = self.web.as_ref().and_then(|web| {
            web.window_doors
                .iter()
                .find(|door| door.anchor == t.anchor)
                .map(|door| door.action.clone())
        }) {
            return self.request_window(action);
        }
        self.request_web(
            t.destination.as_deref().ok_or("unbound web door")?,
            Mode::Follow,
        )
    }
}
