//! Production folder actions shared by winit and the explicitly synthetic proof.
use super::*;
use hair::{
    folder_scene,
    local_explorer::{Explorer, Kind, MAX_HISTORY},
};
use std::{
    collections::{HashSet, VecDeque},
    ffi::OsString,
};

#[derive(Clone)]
struct ChildDeparture {
    name: OsString,
    /// Actual host doorway at departure; coordinates are not entry identity.
    doorway: [f32; 2],
}
#[derive(Clone)]
struct Visit {
    page: usize,
    pose: Pose,
    /// Present only when this visit departed through an actual raw child entry.
    child: Option<ChildDeparture>,
}
// Single App-owned audio controller; device output requires explicit reader intent.
pub(super) mod audio;
pub(super) mod image;
#[cfg(test)]
#[path = "local_image_zoom_tests.rs"]
mod image_zoom_tests;
#[cfg(test)]
#[path = "local_jpeg_object_tests.rs"]
mod jpeg_object_tests;
#[cfg(test)]
#[path = "local_jpeg_preview_tests.rs"]
mod jpeg_preview_tests;
#[cfg(test)]
#[path = "local_audio_controls_tests.rs"]
mod local_audio_controls_tests_r1;
pub(super) mod object_preview;
#[cfg(test)]
#[path = "local_object_preview_tests.rs"]
mod object_preview_tests;
#[cfg(test)]
#[path = "folder_native/raw_reader_qualification_tests.rs"]
mod png_qualification_tests;
mod raw_reader;
#[cfg(test)]
#[path = "raw_reader_qualification_tests.rs"]
mod raw_reader_qualification_tests;
mod video_still;

pub(super) struct Local {
    pub explorer: Explorer,
    reader: Option<raw_reader::Session>,
    pub page: usize,
    // Exactly aligned with Explorer's bounded handle history, not an ancestry cache.
    poses: VecDeque<Visit>,
}
#[derive(Clone, Copy)]
enum Arrival {
    Continuation(folder_scene::Continuation),
    Child(usize),
    BackChild {
        index: usize,
        doorway: [f32; 2],
        departure: Pose,
    },
}
#[derive(Clone, Copy)]
enum Change {
    Enter(usize),
    Up,
    Back,
    Refresh,
    Page(usize),
    Continue(folder_scene::Continuation),
}
impl App {
    /// Claimed only after an interactive presented frame. Injection takes this
    /// same claim then uses complete_folder without showing an OS panel.
    pub(super) fn claim_folder_request(&mut self) -> bool {
        if self.chooser_pending && !self.chooser_active && !self.smoke && self.frames > 0 {
            self.chooser_pending = false;
            self.chooser_active = true;
            self.modal_reset();
            true
        } else {
            false
        }
    }
    fn modal_reset(&mut self) {
        self.release();
        // Keep the claiming O latch until its physical release. Other pre-modal
        // held actions are cleared and synthetic repeats remain suppressed.
        self.input = Input::default();
        self.interact_held = false;
        self.tab_held = false;
        self.tree_held = false;
        self.shifts = [false; 2];
        self.local_held.retain(|key| *key == KeyCode::KeyO);
        self.address = None;
        self.address_error = None;
    }
    pub(super) fn complete_folder(&mut self, selection: Result<Option<PathBuf>>) -> Result<()> {
        self.chooser_pending = false;
        self.chooser_active = false;
        self.release();
        let Some(path) = selection? else {
            self.local_status = "Folder selection cancelled; last good place retained".into();
            return Ok(());
        };
        if self.web.as_ref().is_some_and(|web| web.pending.is_some()) {
            return Err(
                "HTTPS worker still active; wait for completion before choosing a folder".into(),
            );
        }
        let explorer = Explorer::new(&path)?;
        let local = Local {
            explorer,
            page: 0,
            reader: None,
            poses: VecDeque::new(),
        };
        self.install_local(local, None, None)?;
        // No workers remain at this boundary; drop HTTPS history/state, never
        // manufacture a URL or carry its link/image authority into folder mode.
        self.web = None;
        self.address = None;
        Ok(())
    }
    fn install_local(
        &mut self,
        local: Local,
        retained_pose: Option<Pose>,
        arrival: Option<Arrival>,
    ) -> Result<()> {
        let (world, host) = folder_scene::build(
            local.explorer.current(),
            local.page,
            self.world.tree_collapsed,
        )?;
        let doorway_pose = if let Some(arrival) = arrival {
            let view = world.folder_view.as_ref().ok_or("Missing folder view")?;
            let anchor = match arrival {
                Arrival::Continuation(action) => host
                    .anchors
                    .iter()
                    .find(|a| view.continuations.get(&a.id) == Some(&action.reciprocal())),
                Arrival::Child(index) | Arrival::BackChild { index, .. } => {
                    host.anchors.iter().find(|a| {
                        view.bindings
                            .get(&a.id)
                            .is_some_and(|b| b.index == index && b.entry.kind == Kind::Folder)
                    })
                }
            }
            .ok_or("Destination has no matching return doorway; last good place retained")?;
            let pose = match arrival {
                Arrival::BackChild {
                    doorway, departure, ..
                } => {
                    let destination = [anchor.at[0], anchor.at[2]];
                    // Rebind identity first, then translate the saved offset.
                    // Avoid arithmetic entirely for an unchanged doorway so the
                    // exact saved position/yaw/pitch survive unchanged listings.
                    if destination == doorway {
                        departure
                    } else {
                        Pose {
                            position: [
                                departure.position[0] + (destination[0] - doorway[0]),
                                departure.position[1] + (destination[1] - doorway[1]),
                            ],
                            ..departure
                        }
                    }
                }
                // Parent/continuation policy is deliberately not Back's policy.
                _ => {
                    let inward = host.portal_inward(&anchor.id);
                    Pose {
                        position: [
                            anchor.at[0] + inward[0] * 1.05,
                            anchor.at[2] + inward[1] * 1.05,
                        ],
                        yaw: inward[0].atan2(-inward[1]),
                        pitch: 0.0,
                    }
                }
            };
            if host.blocked(pose.position) {
                return Err("Doorway arrival is blocked; last good place retained".into());
            }
            host.check_routes(&[pose.position])?;
            Some(pose)
        } else {
            None
        };
        let mut cache = hair::theme::AssetCache::default();
        let themes = self
            .themes
            .iter()
            .map(|t| {
                let mut staged =
                    hair::theme::Theme::load_for_folder_scene(&t.source, &mut cache, &host)?;
                staged.retain_load_failure(t);
                Ok(staged)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut pose = Pose::default();
        themes[self.theme_index].respawn(&mut pose);
        if let Some(old) = retained_pose.filter(|p| !host.blocked(p.position)) {
            pose = old;
        }
        if let Some(arrival) = doorway_pose {
            pose = arrival;
        }
        if let Some(renderer) = &mut self.renderer {
            renderer.set_theme_appearance(&themes[self.theme_index])?;
            renderer.set_scene(&themes[self.theme_index].vertices_for(&world));
        }
        // All fallible scene/theme/material staging completed before model commit.
        self.discard_local_reader();
        self.local = Some(local);
        self.world = world;
        self.themes = themes;
        if let Some(view) = &self.world.folder_view {
            self.object_previews.room(view);
        }
        self.pose = pose;
        self.scroll = 0;
        self.wheel_rows = 0.0;
        self.release();
        self.local_status = match arrival {
            Some(Arrival::Continuation(_)) => {
                "Read-only | same folder; arrived inside paired doorway facing inward".into()
            }
            Some(Arrival::Child(_)) => {
                "Read-only | Parent; originating child room and doorway restored".into()
            }
            Some(Arrival::BackChild { .. }) => {
                "Read-only | Back; child room and doorway-relative departure pose restored".into()
            }
            None => {
                "Read-only | safe prior pose retained when possible; otherwise room spawn".into()
            }
        };
        Ok(())
    }
    fn local_change(&mut self, change: Change) -> Result<()> {
        let old = self.local.as_ref().ok_or("No folder selected")?;
        let mut staged = Local {
            explorer: old.explorer.try_clone()?,
            page: old.page,
            // Staging never duplicates the reader handle. Failed staging leaves old intact.
            reader: None,
            poses: old.poses.clone(),
        };
        let mut pose = Some(self.pose);
        let mut arrival = None;
        let mut child = None;
        let mut unmatched_back_child = false;
        match change {
            Change::Enter(index) => {
                staged.explorer.enter(index)?;
                let view = self
                    .world
                    .folder_view
                    .as_ref()
                    .ok_or("Missing folder view")?;
                let anchor = self.themes[self.theme_index]
                    .host
                    .anchors
                    .iter()
                    .find(|anchor| {
                        view.bindings.get(&anchor.id).is_some_and(|bound| {
                            bound.index == index && bound.entry.kind == Kind::Folder
                        })
                    })
                    .ok_or("Child departure has no matching doorway; last good place retained")?;
                child = Some(ChildDeparture {
                    name: old.explorer.current().entries[index].name.clone(),
                    doorway: [anchor.at[0], anchor.at[2]],
                });
                staged.page = 0;
                pose = None;
            }
            Change::Up => {
                if !staged.explorer.up()? {
                    self.local_status = "Filesystem root; Parent is a no-op".into();
                    return Ok(());
                }
                // Arbitrary Up defaults explicitly to page zero / room spawn.
                // Only an existing held Back handle can authorize context reuse.
                // Search the OLD aligned deque: Up appended a new visit in staged.
                staged.page = 0;
                pose = None;
                for (index, visit) in old.poses.iter().enumerate().rev() {
                    let Some(departure) = &visit.child else {
                        continue;
                    };
                    // For an older departure, the following retained visit pins
                    // the child that was actually entered. Do not borrow context
                    // from an unrelated child of the same matching parent.
                    let same_child = index + 1 == old.poses.len()
                        || old
                            .explorer
                            .history_matches_current(index + 1, &old.explorer)?;
                    if same_child
                        && old
                            .explorer
                            .history_matches_current(index, &staged.explorer)?
                    {
                        // Refresh may have reordered/removed entries. Rebind the
                        // raw child name to its actual current page/door; never
                        // reuse a slot now occupied by a different name or kind.
                        if let Some(index) = staged
                            .explorer
                            .current()
                            .entries
                            .iter()
                            .position(|e| e.kind == Kind::Folder && e.name == departure.name)
                        {
                            staged.page = index / folder_scene::PAGE_SIZE;
                            arrival = Some(Arrival::Child(index));
                        }
                        break;
                    }
                }
            }
            Change::Back => {
                if !staged.explorer.back()? {
                    self.local_status = "No previous visited directory".into();
                    return Ok(());
                }
                let previous = staged.poses.pop_back();
                staged.page = previous.as_ref().map(|v| v.page).unwrap_or(0);
                pose = previous.as_ref().map(|v| v.pose);
                if let Some(Visit {
                    pose: departure,
                    child: Some(child),
                    ..
                }) = previous
                {
                    // Core Back has already loaded this exact held visit. Do not
                    // search by display path or borrow another parent's context.
                    if let Some(index) = staged
                        .explorer
                        .current()
                        .entries
                        .iter()
                        .position(|e| e.kind == Kind::Folder && e.name == child.name)
                    {
                        staged.page = index / folder_scene::PAGE_SIZE;
                        arrival = Some(Arrival::BackChild {
                            index,
                            doorway: child.doorway,
                            departure,
                        });
                    } else {
                        // Missing/renamed/nonfolder raw child is not the sibling
                        // now occupying its former sorted slot.
                        staged.page = 0;
                        pose = None;
                        unmatched_back_child = true;
                    }
                }
            }
            Change::Refresh => staged.explorer.refresh()?,
            Change::Page(page) => staged.page = page,
            Change::Continue(action) => {
                let view = self
                    .world
                    .folder_view
                    .as_ref()
                    .ok_or("Missing folder view")?;
                staged.page = action.page(view).ok_or("Continuation is unavailable")?;
                arrival = Some(Arrival::Continuation(action));
                pose = None;
            }
        }
        if matches!(change, Change::Enter(_) | Change::Up) {
            if staged.poses.len() == MAX_HISTORY {
                staged.poses.pop_front();
            }
            staged.poses.push_back(Visit {
                page: old.page,
                pose: self.pose,
                child,
            });
        }
        // Refresh/Back can reduce the number of pages.
        staged.page = staged.page.min(
            staged
                .explorer
                .current()
                .entries
                .len()
                .div_ceil(folder_scene::PAGE_SIZE)
                .max(1)
                - 1,
        );
        let unmatched_parent = matches!(change, Change::Up) && arrival.is_none();
        self.install_local(staged, pose, arrival)?;
        if unmatched_parent {
            self.local_status =
                "Read-only | Parent; no matching child context, first room at safe spawn".into();
        } else if unmatched_back_child {
            self.local_status =
                "Read-only | Back; no matching child context, first room at safe spawn".into();
        }
        Ok(())
    }
    pub(super) fn local_interact(&mut self, metadata_only: bool) -> Result<()> {
        if self.world.reading {
            return Ok(());
        }
        let target = hair::interaction::resolve(
            &self.themes[self.theme_index].host,
            &self.pose,
            &self.world,
        );
        // Resolve eligibility exactly as the prompt does. Parent uses the same
        // staged handle-relative Up as U, never a displayed destination string.
        if !metadata_only
            && let Some(target) = &target
            && self
                .world
                .folder_view
                .as_ref()
                .and_then(|v| v.parent.as_deref())
                == Some(target.anchor.as_str())
        {
            return self.local_change(Change::Up);
        }
        // Continuations have their own same-folder action binding, not an entry.
        if !metadata_only
            && let Some(target) = &target
            && let Some(view) = &self.world.folder_view
            && let Some(action) = view.continuations.get(&target.anchor)
        {
            return self.local_change(Change::Continue(*action));
        }
        let bound = target
            .as_ref()
            .and_then(|t| self.world.folder_view.as_ref()?.bindings.get(&t.anchor))
            .cloned();
        if !metadata_only
            && let Some(bound) = &bound
            && bound.entry.kind == Kind::Folder
        {
            return self.local_change(Change::Enter(bound.index));
        }
        let mut text = if let Some(bound) = &bound {
            folder_scene::metadata(&bound.entry)
        } else if metadata_only {
            format!(
                "CURRENT DIRECTORY (display only; may become stale):\n{}\n\nO choose | PgUp/PgDn | L inactive | Shift run\nU Parent | R Back | F5 refresh | I source\n\nSTATUS (full):\n{}\n",
                self.world.current, self.local_status
            )
        } else {
            return Ok(());
        };
        if metadata_only && let (Some(target), Some(bound)) = (&target, &bound) {
            // Opt-in world I snapshot only. Preserve shared metadata as the
            // exact prefix and leave E/raw-reader documents and controls alone.
            text.push_str(&self.object_previews.inspection_snapshot(
                &target.anchor,
                bound,
                &self.themes[self.theme_index],
            ));
        }
        if !metadata_only && let Some(bound) = &bound {
            if bound.entry.kind == Kind::File {
                let local = self.local.as_mut().ok_or("No retained folder")?;
                let session =
                    raw_reader::Session::open(local.explorer.open_reader(bound.index), text);
                text = session.document()?;
                local.reader = Some(session);
            } else {
                text.push_str("\nBody/preview unsupported for this kind. Links and special files are never opened.\n");
            }
        }
        self.world
            .folder_view
            .as_mut()
            .ok_or("Missing folder view")?
            .reader = text;
        self.world.reading = true;
        self.world.reader_link = None;
        self.world.interaction = None;
        self.scroll = 0;
        self.wheel_rows = 0.0;
        self.release();
        // A Space held in world mode must not become a new audio press on E.
        self.local_held.retain(|key| *key == KeyCode::Space);
        self.tab_held = false;
        self.shifts = [false; 2];
        Ok(())
    }
    pub(super) fn discard_local_reader(&mut self) {
        self.object_previews.invalidate();
        for theme in &mut self.themes {
            theme.set_folder_previews(&Default::default());
            theme.set_folder_models(&Default::default());
        }
        if let Some(renderer) = &mut self.renderer {
            // The next poll restores baseline object materials/geometry.
            renderer.clear_viewer();
            renderer.set_scene(&self.themes[self.theme_index].vertices_for(&self.world));
        }
        if let Some(local) = &mut self.local {
            // State Drop cancels audio identity immediately. Redraw (even with no
            // reader) disposes output and drains work; App Controller Drop is final owner.
            local.reader = None;
        }
        if let Some(renderer) = &mut self.renderer {
            renderer.clear_viewer();
        }
    }
    /// Intent-only cancellation from focus/capture/address/chooser release.
    /// No synchronous native operations in any key handler.
    pub(super) fn stop_local_audio(&mut self) {
        if let Some(session) = self.local.as_mut().and_then(|local| local.reader.as_mut()) {
            let changed = session
                .audio
                .as_ref()
                .is_some_and(|audio| audio.desired != audio::Desired::Stopped || audio.requested);
            if changed {
                session.stop_audio();
                match session.document() {
                    Ok(document) => {
                        if let Some(view) = &mut self.world.folder_view {
                            view.reader = document;
                        }
                    }
                    Err(error) => self.local_status = format!("Reader formatting refused: {error}"),
                }
            }
        }
    }
    /// Always service the singleton, including cancelled jobs with no current reader.
    /// Acquisition/output occurs only here after an admitted explicit play request.
    pub(super) fn poll_local_audio(&mut self) -> Result<()> {
        use audio::{Desired, Observed};
        let admitted = self.world.reading
            && self.focused
            && self.address.is_none()
            && !self.chooser_pending
            && !self.chooser_active;
        let session = self.local.as_mut().and_then(|local| local.reader.as_mut());
        let Some(session) = session else {
            self.audio.poll_explicit(None, Desired::Stopped);
            return Ok(());
        };
        let Some(state) = session.audio.as_ref() else {
            self.audio.poll_explicit(None, Desired::Stopped);
            return Ok(());
        };
        let before = (
            state.observed,
            state.error.clone(),
            state.controls,
            state.info,
            state.applied,
        );
        let visible = admitted && session.audio_visible();
        if !visible {
            session.stop_audio();
        }
        // Existing bounded same-held-source validation precedes active seek/reopen.
        let validate = session.audio.as_ref().is_some_and(|state| {
            state.desired != Desired::Stopped && self.audio.owns(&state.token)
        });
        if validate {
            let result = session
                .audio_source()
                .and_then(|file| file.read_chunk(0).map(|_| ()));
            if result.is_err() {
                session.stop_audio();
                session.audio.as_mut().unwrap().fail(
                    "held audio source changed or became unreadable; C retries same handle".into(),
                );
            }
        }
        let state = session.audio.as_mut().unwrap();
        // Only settle Pause with no acquisition/track; never decode solely to pause.
        state.settle_unacquired_pause(self.audio.owns(&state.token));
        if state.requested && self.audio.owns(&state.token) {
            state.requested = false;
        }
        // A terminal track retains its sole PCM while stopped, never a live output.
        let current = if visible && !state.token.load(std::sync::atomic::Ordering::Acquire) {
            Some(state.token.clone())
        } else {
            None
        };
        let desired = if current.is_some() {
            state.desired
        } else {
            Desired::Stopped
        };
        self.audio
            .poll_controls(current.as_ref(), desired, state.controls);
        let request = visible && state.requested && state.desired == Desired::Play;
        if request && !self.audio.busy() {
            let token = state.token.clone();
            let result = session
                .audio_source()
                .and_then(|file| self.audio.acquire_after_explicit_play(token, file));
            let state = session.audio.as_mut().unwrap();
            state.requested = false;
            if let Err(error) = result {
                state.fail(error);
            }
        }
        let state = session.audio.as_mut().unwrap();
        if let Some((info, applied)) = self.audio.snapshot(&state.token) {
            state.info = Some(info);
            state.controls.seconds = state.controls.seconds.min(info.duration);
            state.applied = Some(applied);
        } else {
            state.applied = None;
        }
        if self.audio.disabled() {
            state.fail(
                self.audio
                    .error
                    .clone()
                    .unwrap_or_else(|| "audio output disabled after disposal failure".into()),
            );
        } else if !state.requested && state.desired != Desired::Stopped {
            state.observed = self.audio.observed;
            if self.audio.observed == Observed::Error {
                state.fail(
                    self.audio
                        .error
                        .clone()
                        .unwrap_or_else(|| "audio refused".into()),
                );
            } else if self.audio.observed == Observed::Ended {
                state.desired = Desired::Stopped;
            }
        } else if state.desired == Desired::Stopped && self.audio.owns(&state.token) {
            // Ready after an effective ended seek invalidates plain replay, even
            // when multiple adjustments return to the old prepared endpoint.
            // Do not let repeated stopped polls restore that stale Ended marker.
            if state.observed != Observed::Ready || self.audio.observed != Observed::Ended {
                state.observed = self.audio.observed;
            }
        }
        if before
            != (
                state.observed,
                state.error.clone(),
                state.controls,
                state.info,
                state.applied,
            )
            && let Some(view) = &mut self.world.folder_view
        {
            view.reader = session.document()?;
        }
        Ok(())
    }
    /// Shared normal-redraw and capture consumer. Joins ONLY finished work;
    /// cancelled jobs keep the sole slot until complete. One current reader intent.
    pub(super) fn poll_local_image(&mut self) -> Result<()> {
        self.poll_local_audio()?;
        let changed = self.object_previews.poll(
            self.local.as_ref().map(|local| &local.explorer),
            self.world.folder_view.as_ref(),
            self.world.reading || self.chooser_active || self.chooser_pending,
        );
        if changed {
            for theme in &mut self.themes {
                theme.set_folder_previews(&self.object_previews.cache);
                theme.set_folder_models(&self.object_previews.models);
            }
        }
        // Pointer-identity material cache makes this a no-upload operation on
        // ordinary redraw. Also restores mappings after renderer creation/reskin.
        if let Some(renderer) = &mut self.renderer {
            renderer.set_materials(&self.themes[self.theme_index].textures)?;
            if changed {
                renderer.set_scene(&self.themes[self.theme_index].vertices_for(&self.world));
            }
        }
        let completion = self.images.poll();
        let session = self.local.as_mut().and_then(|local| local.reader.as_mut());
        if let Some(session) = session {
            let mut changed = false;
            if let Some((token, result)) = completion {
                session.image_complete(&token, result);
                changed = true;
            }
            if !self.images.busy()
                && let Some(request) = session.image_request()
            {
                changed = true;
                let result = request.and_then(|(token, file)| self.images.start(token, file));
                if let Err(error) = result {
                    session.image_failed(error);
                }
            }
            if let Some(renderer) = &mut self.renderer {
                if let Err(error) = renderer.set_viewer_texture(session.image_texture()) {
                    renderer.clear_viewer();
                    session.image_failed(error);
                    changed = true;
                }
                renderer.viewer_visible = self.world.reading && session.image_visible();
                renderer.viewer_view = session.image_view();
            }
            // No repeated 64 KiB raw-document formatting on ordinary redraws.
            if changed && let Some(view) = &mut self.world.folder_view {
                view.reader = session.document()?;
            }
        } else if let Some(renderer) = &mut self.renderer {
            renderer.clear_viewer();
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn local_image_pending(&self) -> bool {
        self.images.busy()
            || self
                .local
                .as_ref()
                .and_then(|l| l.reader.as_ref())
                .is_some_and(|s| s.image_pending())
    }
    pub(super) fn local_image_visible(&self) -> bool {
        self.world.reading
            && self
                .local
                .as_ref()
                .and_then(|l| l.reader.as_ref())
                .is_some_and(|s| s.image_visible())
    }
    /// Scoped ahead of folder I handling and the generic reader allowlist.
    pub(super) fn reader_key(&mut self, code: KeyCode, down: bool, repeat: bool) -> bool {
        // Release bookkeeping must survive a switch to metadata/raw while held.
        if !down && matches!(code, KeyCode::Space | KeyCode::ArrowUp | KeyCode::ArrowDown) {
            self.local_held.remove(&code);
        }
        let image_pan =
            self.local_image_visible() && matches!(code, KeyCode::ArrowUp | KeyCode::ArrowDown);
        if !image_pan
            && !matches!(
                code,
                KeyCode::Space
                    | KeyCode::Equal
                    | KeyCode::NumpadAdd
                    | KeyCode::Minus
                    | KeyCode::NumpadSubtract
                    | KeyCode::Digit0
                    | KeyCode::ArrowLeft
                    | KeyCode::ArrowRight
                    | KeyCode::KeyV
                    | KeyCode::KeyC
                    | KeyCode::KeyI
            )
        {
            return false;
        }
        if !down {
            self.local_held.remove(&code);
        }
        let active = self.world.reading
            && self.address.is_none()
            && !self.chooser_active
            && !self.chooser_pending
            && self
                .local
                .as_ref()
                .is_some_and(|local| local.reader.is_some());
        if !active {
            if code == KeyCode::Space && down {
                self.local_held.insert(code);
            }
            return false;
        }
        if !down || !self.focused || repeat || !self.local_held.insert(code) {
            return true;
        }
        let session = self.local.as_mut().unwrap().reader.as_mut().unwrap();
        // Space in raw, metadata, images or text is wholly inert, not a reformat.
        if code == KeyCode::Space && !session.audio_visible() {
            return true;
        }
        let scroll = session.action(code, self.scroll);
        if let Some(renderer) = &mut self.renderer {
            if session.image_texture().is_none() {
                renderer.clear_viewer();
            }
            renderer.viewer_visible = session.image_visible();
            renderer.viewer_view = session.image_view();
        }
        match session.document() {
            Ok(document) => {
                if let Some(view) = &mut self.world.folder_view {
                    view.reader = document;
                }
                if let Some(scroll) = scroll {
                    self.scroll = scroll;
                }
                let [width, height] = self.ui_viewport();
                self.scroll = self.scroll.min(hair::text::local_reader_scroll_max(
                    &self.world,
                    width,
                    height,
                ));
            }
            Err(error) => {
                self.local_status =
                    format!("Reader formatting refused: {error}; last good view retained");
            }
        }
        true
    }
    /// Returns Some for a locally owned key, None for the unchanged visitor map.
    pub(super) fn folder_key(
        &mut self,
        code: KeyCode,
        down: bool,
        repeat: bool,
    ) -> Option<Result<bool>> {
        let always = code == KeyCode::KeyO;
        let local = self.local.is_some();
        if !always
            && !(local
                && matches!(
                    code,
                    KeyCode::KeyU
                        | KeyCode::KeyR
                        | KeyCode::F5
                        | KeyCode::PageUp
                        | KeyCode::PageDown
                        | KeyCode::KeyI
                        | KeyCode::KeyL
                ))
        {
            return None;
        }
        if !down {
            self.local_held.remove(&code);
            return Some(Ok(false));
        }
        if !self.focused || repeat || !self.local_held.insert(code) {
            return Some(Ok(false));
        }
        if self.world.reading || self.chooser_active || self.chooser_pending {
            return Some(Ok(false));
        }
        let result = match code {
            KeyCode::KeyO => {
                self.modal_reset();
                self.local_held.insert(KeyCode::KeyO);
                self.chooser_pending = true;
                Ok(())
            }
            KeyCode::KeyU => self.local_change(Change::Up),
            KeyCode::KeyR => self.local_change(Change::Back),
            KeyCode::F5 => self.local_change(Change::Refresh),
            KeyCode::PageUp | KeyCode::PageDown => {
                let view = self.world.folder_view.as_ref().expect("local view");
                let page = if code == KeyCode::PageUp {
                    view.page.saturating_sub(1)
                } else {
                    (view.page + 1).min(view.pages() - 1)
                };
                if page == view.page {
                    Ok(())
                } else {
                    self.local_change(Change::Page(page))
                }
            }
            KeyCode::KeyI => self.local_interact(true),
            KeyCode::KeyL => {
                self.local_status =
                    "L HTTPS address inactive in folder mode; restart with --url for web reading"
                        .into();
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.local_status = format!("Refused: {error}; last good place retained");
        }
        Some(Ok(false))
    }
}

// Kept here so production action fields have one explicit bounded set type.
pub(super) type Held = HashSet<KeyCode>;
