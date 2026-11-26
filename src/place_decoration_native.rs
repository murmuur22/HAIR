//! Explicit new-decoration source intent; existing records only relocate.
use super::*;

pub(super) struct DecorationEdit {
    pub source: Source,
    pub record: hair::placement_store::Decoration,
    pub pending: Option<hair::placement_store::Decoration>,
    snapshot: Option<Snapshot>,
    pub preview_reason: Option<String>,
    pub resize: bool,
    pub(super) interrupted_resize: bool,
    #[cfg(test)]
    pub validation_probe: Option<Box<dyn Fn(u8) -> Result<()>>>,
}
// Retain inline checked source ownership; boxing would change allocation/failure behavior.
#[allow(clippy::large_enum_variant)]
pub(super) enum Source {
    New(NewChoice),
    Existing(hair::placement_store::Decoration),
}
pub(super) struct NewChoice {
    follow: hair::placement_store::Decoration,
    entry: hair::decoration::Entry,
    asset: hair::decoration::LoadedAsset,
    theme_source: PathBuf,
    pin: Option<hair::placement_store::CheckedPin>,
}
impl DecorationEdit {
    pub(super) fn expected(&self) -> Option<&hair::placement_store::Decoration> {
        match &self.source { Source::New(_) => None, Source::Existing(d) => Some(d) }
    }
    fn pin(&self) -> Option<&hair::placement_store::CheckedPin> {
        match &self.source { Source::New(choice) => choice.pin.as_ref(), Source::Existing(_) => None }
    }
    fn follow_proposal(&self, d: &hair::placement_store::Decoration) -> hair::placement_store::Decoration {
        let Source::New(choice) = &self.source else { return d.clone(); };
        let mut follow = choice.follow.clone();
        follow.at = d.at; follow.yaw = d.yaw; follow.collision = d.collision;
        follow.size = d.size; follow.scaling = d.scaling;
        follow.animation = d.animation;
        follow
    }
    fn disclosure(&self) -> &'static str {
        match &self.source {
            Source::New(c) if c.pin.is_some() => "New pin | folder-owned artwork",
            Source::New(_) => "New Follow theme (save tag only)",
            Source::Existing(d) if d.is_pinned() => "Existing pinned | retained asset",
            Source::Existing(_) => "Existing Follow theme (save tag only)",
        }
    }
}
// Route flood-fill is bounded but not frame work: repeat it only when the
// logical host, candidate, or visitor changes. Never cache final admission.
struct Snapshot {
    host: hair::host_scene::HostScene,
    record: hair::placement_store::Decoration,
    visitor: [f32; 2],
    refusal: Option<String>,
}
impl App {
    // Historical Follow transaction data, intentionally NOT a selectable action.
    // Retains old writer/gray-fallback diagnostics; B tests use real catalog input.
    #[cfg(test)]
    pub(super) fn legacy_follow_fixture(&mut self) {
        let edit = self.place_mode.as_mut().unwrap().decoration_edit.as_mut().unwrap();
        let follow = edit.follow_proposal(&edit.record);
        let Source::New(choice) = &mut edit.source else { panic!("legacy new-transaction fixture only") };
        choice.pin = None;
        edit.record = follow;
    }
    pub(super) fn decoration_start(&mut self) -> Result<()> {
        self.check_room_destination(&self.local.as_ref().ok_or("Choose a local folder first")?.explorer)?;
        let mode = self.place_mode.as_ref().ok_or("Enter Place Mode first")?;
        if mode.edit.as_ref().is_some_and(|e| e.confirming())
            || mode.decoration_edit.as_ref().is_some_and(|e| e.pending.is_some())
        { return Err("Review frozen; cancel first".into()); }
        // No failed catalog action may leave a previously selected file armed.
        self.place_mode.as_mut().unwrap().edit = None;
        if self.local.is_none() { return Err("Choose a local folder first".into()); }
        let entry = self.place_decoration_selection().ok_or("Select a catalog decoration with B first")?;
        let record = hair::placement_store::Decoration::from_entry(entry, [0.; 2], 0., [0.8, 0.85, 0.6])?;
        let theme = &self.themes[self.theme_index];
        theme.decoration_preview(&record)?;
        let asset = theme.decorations.asset(&entry.asset).ok_or("Selected catalog asset missing")?;
        // Catalog selection is the acquisition boundary, not redraw or commit.
        // Keep the checked descriptor-held source through review and publication.
        let pin = hair::placement_store::select_pin_asset(theme, &entry.id)?;
        let retained = pin.decoration(&record)?;
        let choice = NewChoice { follow: record.clone(), entry: entry.clone(), asset: asset.clone(),
            theme_source: theme.source.clone(), pin: Some(pin) };
        let record = retained;
        let mode = self.place_mode.as_mut().unwrap();
        mode.edit = None;
        mode.decoration_edit = Some(DecorationEdit { source: Source::New(choice), record, pending: None, snapshot: None, preview_reason: None, resize: false, interrupted_resize: false,
            #[cfg(test)] validation_probe: None });
        self.local_status = format!("Place Mode | Editing | New pin | folder-owned artwork | {} | exact asset retained on save; arrows yaw; H collision; Enter positions", self.place_palette_status_for_editor());
        self.editor_arrive(None);
        Ok(())
    }
    pub(super) fn decoration_source(&mut self, pin: bool) -> Result<()> {
        let edit = self.place_mode.as_ref().and_then(|m| m.decoration_edit.as_ref()).ok_or("No decoration preview")?;
        if edit.pending.is_some() { return Err("Review frozen; cancel before changing source intent".into()); }
        let Source::New(choice) = &edit.source else { return Err("Existing records only relocate; Follow-theme to Pin conversion is not supported".into()); };
        if !pin { return Err("New decorations require folder-owned artwork; Follow is legacy read compatibility only".into()); }
        if pin == choice.pin.is_some() { return Ok(()); }
        let follow = edit.follow_proposal(&edit.record);
        // Resolve/acquire only on an explicit selection boundary, never redraw.
        // Do not replace last-good intent/preview unless every check succeeds.
        let checked = if pin {
            let theme = &self.themes[self.theme_index];
            let current = theme.decorations.asset(&choice.entry.asset).ok_or("Selected asset changed; cancel and reselect")?;
            if theme.source != choice.theme_source || theme.decorations.entry(&choice.entry.id) != Some(&choice.entry)
                || current.spec != choice.asset.spec || !std::sync::Arc::ptr_eq(&current.model, &choice.asset.model) {
                return Err("Selected catalog identity changed; cancel and reselect".into());
            }
            Some(hair::placement_store::select_pin_asset(theme, &choice.entry.id)?)
        } else { None };
        let record = if let Some(source) = &checked { source.decoration(&follow)? } else { follow };
        if pin { self.themes[self.theme_index].decoration_preview(&record)?; }
        else { choice.asset.placed(&record)?; }
        let edit = self.place_mode.as_mut().unwrap().decoration_edit.as_mut().unwrap();
        let Source::New(choice) = &mut edit.source else { unreachable!() };
        choice.pin = checked;
        edit.record = record;
        edit.snapshot = None;
        edit.preview_reason = None;
        let disclosure = edit.disclosure();
        self.local_status = format!("Place Mode | {} | {disclosure} | choice only; no writes | Enter {}; Escape back/cancel",
            self.editor_phase(), if self.place_mode.as_ref().unwrap().positioning { "reviews" } else { "positions" });
        Ok(())
    }
    pub(super) fn decoration_reposition(&mut self, expected: hair::placement_store::Decoration) -> Result<()> {
        self.check_room_destination(&self.local.as_ref().ok_or("Choose a local folder first")?.explorer)?;
        // Even unavailable appearance must retire a previously selected file.
        self.place_mode.as_mut().unwrap().edit = None;
        self.themes[self.theme_index].decoration_preview(&expected)?;
        let missing = expected.artwork(&self.themes[self.theme_index].decorations)?.is_none();
        self.local_status = format!("Place Mode | decoration reposition | expected id={} | tag={} | size={:?} | {} | arrows yaw; H collision; Enter positions; existing records only relocate; conversion not supported", expected.placement_id, expected.tag, expected.size,
            if expected.is_pinned() { "Existing pinned | retained asset" } else { "Existing Follow theme (save tag only)" });
        if missing { self.local_status.push_str(" | Missing artwork (generated box, not a pin source)"); }
        if let Some(note) = expected.floor_fit_disclosure() { self.local_status.push_str(&format!(" | {note}")); }
        let mode = self.place_mode.as_mut().unwrap();
        mode.edit = None;
        mode.decoration = None;
        mode.decoration_edit = Some(DecorationEdit { record: expected.clone(), source: Source::Existing(expected), pending: None, snapshot: None, preview_reason: None, resize: false, interrupted_resize: false,
            #[cfg(test)] validation_probe: None });
        let mesh = self.decoration_mesh(&self.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().record).ok();
        self.editor_arrive(mesh);
        self.local_status = format!("Editing | {} | Enter positions", self.local_status);
        Ok(())
    }
    // Render omission only. Host, target resolution and physics keep the old record.
    pub(super) fn place_scene_at(&self, seconds: f64) -> Vec<hair::assets::Vertex> {
        let id = self.place_mode.as_ref().and_then(|m| m.decoration_edit.as_ref())
            .and_then(DecorationEdit::expected).map(|d| d.placement_id.as_str());
        let file = self.place_mode.as_ref().and_then(|m| m.edit.as_ref()).map(|e| e.anchor.as_str());
        self.themes[self.theme_index].vertices_for_at_omitting(&self.world, seconds, id, file)
    }
    pub(super) fn place_scene_refresh(&mut self) {
        if self.renderer.is_some() {
            let mesh = self.place_scene_at(self.started.elapsed().as_secs_f64());
            if let Some(renderer) = &mut self.renderer { renderer.set_scene(&mesh); }
        }
    }
    fn decoration_live_page(&self) -> Result<usize> {
        let local = self.local.as_ref().ok_or("Folder closed")?;
        let mut fresh = local.explorer.try_clone()?;
        fresh.refresh()?;
        if local.page >= fresh.current().entries.len().max(1).div_ceil(hair::folder_scene::PAGE_SIZE) {
            return Err("Current decoration page is out of range; refresh and reselect".into());
        }
        Ok(local.page)
    }
    fn decoration_candidate(layout: &hair::placement_store::LayoutRead, expected: &hair::placement_store::Decoration, proposed: &hair::placement_store::Decoration, resize: bool) -> Result<hair::placement_store::LayoutRead> {
        if expected.animation != proposed.animation { expected.validate_animation(proposed)?; }
        else if resize { expected.validate_resize(proposed)?; } else { expected.validate_relocation(proposed)?; }
        let mut candidate = layout.clone();
        let matches: Vec<_> = candidate.decorations.iter().enumerate()
            .filter(|(_, d)| d.placement_id == expected.placement_id).map(|(i, _)| i).collect();
        if matches.len() != 1 { return Err("Expected decoration missing or duplicate; refresh and reselect".into()); }
        let index = matches[0];
        if candidate.decorations[index] != *expected { return Err("Stale expected decoration record; refresh and reselect".into()); }
        candidate.decorations[index] = proposed.clone();
        Ok(candidate)
    }
    pub(super) fn decoration_position(&self) -> Result<hair::placement_store::Decoration> {
        let edit = self.place_mode.as_ref().and_then(|m| m.decoration_edit.as_ref()).ok_or("No decoration preview")?;
        if let Some(d) = &edit.pending { return Ok(d.clone()); }
        let mut d = edit.record.clone();
        let mut floor_ray = self.pose;
        floor_ray.feet_y -= hair::host_scene::FLOOR_TOP;
        d.at = hair::placement::floor_target(&floor_ray).ok_or("Aim at floor within six metres; no implicit pedestal support")?;
        Ok(d)
    }
    pub(super) fn decoration_mesh(&self, d: &hair::placement_store::Decoration) -> Result<Vec<hair::assets::Vertex>> {
        self.decoration_mesh_at(d, 0.)
    }
    pub(super) fn decoration_mesh_at(&self, d: &hair::placement_store::Decoration, seconds: f64) -> Result<Vec<hair::assets::Vertex>> {
        let edit = self.place_mode.as_ref().and_then(|m| m.decoration_edit.as_ref()).ok_or("No decoration preview")?;
        if let Source::New(choice) = &edit.source
            && choice.pin.is_none() {
            let mut mesh=choice.asset.placed(d)?;
            let len=mesh.len(); d.present_artwork(&mut mesh,len,seconds)?;
            return Ok(mesh);
        }
        d.artwork_mesh_at(&self.themes[self.theme_index].decorations, seconds)
    }
    pub(super) fn decoration_editor_textures(&self, d: &hair::placement_store::Decoration) -> Result<Vec<std::sync::Arc<hair::assets::Texture>>> {
        let edit=self.place_mode.as_ref().and_then(|m|m.decoration_edit.as_ref()).ok_or("No decoration preview")?;
        if let Source::New(choice)=&edit.source && choice.pin.is_none() { return Ok(choice.asset.model.textures.clone()); }
        Ok(d.artwork(&self.themes[self.theme_index].decorations)?.map_or(vec![],|(_,a)|a.model.textures.clone()))
    }
    pub(super) fn interrupted_animation(&self) -> bool {
        self.place_mode.as_ref().and_then(|m|m.decoration_edit.as_ref()).is_some_and(|e|
            e.interrupted_resize && e.expected().is_some_and(|d|d.animation!=e.record.animation))
    }
    pub(super) fn decoration_enter(&mut self) -> Result<()> {
        self.check_room_destination(&self.local.as_ref().ok_or("Folder closed")?.explorer)?;
        let d = self.decoration_position()?;
        let page = self.decoration_live_page()?;
        let local = self.local.as_ref().ok_or("Folder closed")?;
        let theme = &self.themes[self.theme_index];
        let edit = self.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap();
        let expected = edit.expected().cloned();
        let follow = edit.follow_proposal(&d);
        let disclosure = edit.disclosure();
        if edit.pending.is_none() {
            let layout = hair::placement_store::read_layout(&local.explorer)?;
            if let Some(expected) = &expected {
                let candidate = Self::decoration_candidate(&layout, expected, &d, edit.resize)?;
                theme.validate_decoration_candidate_layout(&candidate)?;
                hair::placement_scene::validate_decoration_candidate(&local.explorer, &candidate, self.pose.position, page)?;
            } else {
                if let Some(source) = edit.pin() { theme.validate_pin_layout(&layout, source, &follow)?; }
                else { theme.validate_decoration_layout(&layout, &d)?; }
                hair::placement_scene::validate_decoration(&local.explorer, &layout, &d, self.pose.position, page)?;
            }
            self.place_mode.as_mut().unwrap().decoration_edit.as_mut().unwrap().pending = Some(d);
            self.decoration_release_pointer();
            self.local_status = if let Some(expected) = &expected {
                format!("Place Mode | frozen decoration reposition review | expected id={} | {disclosure} | fresh Enter saves; Escape back to Editing; no writes yet", expected.placement_id)
            } else { format!("Place Mode | frozen decoration review | {disclosure} | fresh Enter saves; Escape back to Editing; no writes yet") };
            return Ok(());
        }
        let outcome = if let Some(expected) = &expected {
            #[cfg(test)]
            let mut stage = 0;
            let validate = |candidate: &hair::placement_store::LayoutRead| {
                self.check_room_destination(&local.explorer)?;
                self.decoration_live_page()?;
                theme.validate_decoration_candidate_layout(candidate)?;
                hair::placement_scene::validate_decoration_candidate(&local.explorer, candidate, self.pose.position, page)?;
                #[cfg(test)]
                { stage += 1; if let Some(probe) = &edit.validation_probe { probe(stage)?; } }
                Ok(())
            };
            if expected.animation != d.animation {
                let intent = hair::placement_store::DecorationAnimation::new(expected, &d)?;
                hair::placement_store::animate_decoration_checked(&local.explorer, &intent, validate)
            } else if edit.resize {
                let intent = hair::placement_store::DecorationResize::new(expected, &d)?;
                hair::placement_store::resize_decoration_checked(&local.explorer, &intent, validate)
            } else { hair::placement_store::relocate_decoration_checked(&local.explorer, expected, &d, validate) }
        } else if let Some(source) = edit.pin() {
            // Safe Theme wrapper: complete appearance preflight AND both locked
            // callbacks. Never substitute the generic storage convenience API.
            theme.publish_pin_checked(&local.explorer, source, &follow, |candidate| {
                self.check_room_destination(&local.explorer)?;
                self.decoration_live_page()?;
                hair::placement_scene::validate_decoration_candidate(&local.explorer, candidate, self.pose.position, page)
            })
        } else {
            hair::placement_store::place_decoration_checked(&local.explorer, &d, |layout| {
                self.check_room_destination(&local.explorer)?;
                self.decoration_live_page()?;
                theme.validate_decoration_layout(layout, &d)?;
                hair::placement_scene::validate_decoration(&local.explorer, layout, &d, self.pose.position, page)
            })
        };
        match outcome {
            hair::placement_store::Outcome::Committed { synced } => {
                self.place_mode.as_mut().unwrap().decoration_edit = None;
                let mode = self.place_mode.take();
                let refreshed = self.placement_refresh(page);
                self.place_mode = mode;
                self.local_status = match refreshed {
                    Ok(()) => format!("Place Mode | {} | {disclosure} | id={} collision={}{}", if expected.is_some() { "decoration reposition saved" } else if d.is_pinned() { "decoration pinned" } else { "decoration saved" }, d.placement_id, d.collision,
                        if synced { "" } else { "; sync uncertain; inspect before retry" }),
                    Err(e) => format!("{} committed{}; refresh failed: {e}; reopen, do not repeat", if expected.is_some() { "Decoration reposition" } else if d.is_pinned() { "Decoration pin" } else { "Decoration add" }, if synced { "" } else { "; sync uncertain" }),
                };
            }
            hair::placement_store::Outcome::NotCommitted(e) => {
                let edit = self.place_mode.as_mut().unwrap().decoration_edit.as_mut().unwrap();
                edit.pending = None;
                edit.interrupted_resize = e.starts_with("Pending resize retained;") || e.starts_with("Pending animation retained;");
                return Err(e);
            }
            hair::placement_store::Outcome::Ambiguous(e) => {
                self.place_mode.as_mut().unwrap().decoration_edit = None;
                self.local_status = format!("Decoration {} result uncertain; inspect/reopen before retry: {e}", if expected.is_some() { "reposition" } else if d.is_pinned() { "pin" } else { "add" });
            }
        }
        // Clearing an uncertain/committed edit also retires its visual omission,
        // even if refresh failed. This renders current authority, never rollback.
        if expected.is_some() { self.place_scene_refresh(); }
        Ok(())
    }
    fn decoration_release_pointer(&mut self) {
        let mode = self.place_mode.take();
        self.release_navigation();
        self.place_mode = mode;
    }
    pub(super) fn decoration_controls(&self) -> Vec<(place_mode_native::Control, [f32; 4], &'static str)> {
        use place_mode_native::Control;
        let frame = self.inventory_frame();
        let x = (frame.full[0] as f32 - 308.).max(8.);
        let e = self.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap();
        let d = e.pending.as_ref().unwrap_or(&e.record);
        let controls = [
            (Control::FollowTheme, if d.is_pinned() { "F: Follow theme" } else { "F: Follow theme (selected)" }),
            (Control::PinAsset, if d.is_pinned() { "V: Pin selected (this folder)" } else { "V: Pin this asset to this folder" }),
            (Control::Left, "Left: rotate -"),
            (Control::Right, "Right: rotate +"),
            (Control::Collision, if d.collision { "H: decoration collision ON" } else { "H: decoration collision OFF" }),
            (Control::Enter, if e.pending.is_some() { if e.expected().is_some() { "Enter: save reposition" } else if d.is_pinned() { "Enter: save pin" } else { "Enter: save decoration" } } else if !self.place_mode.as_ref().unwrap().positioning { "Enter: begin positioning" } else { "Enter: review (no writes)" }),
            (Control::ReleasePointer, "Tab: release mouse / toolbox"),
            (Control::Cancel, "Esc: back / cancel"),
            (Control::Exit, "P: exit Place Mode"),
        ].into_iter()
            .filter(|(c, _)| (e.pending.is_none() || matches!(c, Control::Enter | Control::Cancel | Control::Exit))
                && !matches!(c, Control::FollowTheme)
                && (e.expected().is_none() || !matches!(c, Control::FollowTheme | Control::PinAsset)))
            .enumerate().map(|(i, (c, s))| (c, [x, 196. + i as f32 * 22., x + 296., 216. + i as f32 * 22.], s)).collect();
        self.editor_controls_layout(controls)
    }
    pub(super) fn decoration_footer(&self) -> Option<String> {
        let e = self.place_mode.as_ref()?.decoration_edit.as_ref()?;
        Some(format!("{}\nEsc back/cancel | P exit | I full status\n{}",
            if e.pending.is_some() { "Frozen: no writes; fresh Enter saves" }
            else { "Preview: partial snapshot; no writes" }, self.local_status))
    }
    pub(super) fn decoration_ui(&mut self) -> Vec<hair::assets::Vertex> {
        let mut out = vec![];
        let position = if self.place_mode.as_ref().unwrap().positioning { self.decoration_position() }
            else { Ok(self.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().record.clone()) };
        let appearance = position.as_ref().map_err(Clone::clone).and_then(|d| self.decoration_mesh_at(d,self.started.elapsed().as_secs_f64()));
        let mut reason = position.as_ref().err().cloned();
        if let Ok(d) = &position && self.place_mode.as_ref().unwrap().positioning {
            let theme = &self.themes[self.theme_index];
            let e = self.place_mode.as_mut().unwrap().decoration_edit.as_mut().unwrap();
            if !e.snapshot.as_ref().is_some_and(|s| s.host == theme.host && s.record == *d && s.visitor == self.pose.position) {
                let mut host = theme.host.clone();
                if let Some(expected) = e.expected() { host.decorations.remove(&expected.placement_id); }
                e.snapshot = Some(Snapshot {
                    host: theme.host.clone(), record: d.clone(), visitor: self.pose.position,
                    refusal: hair::placement_scene::validate_decoration_snapshot(&host, d, self.pose.position).err(),
                });
            }
            reason = e.snapshot.as_ref().unwrap().refusal.clone();
            match appearance {
                Ok(mut mesh) => {
                    // Even a clear snapshot is neutral: fresh all-page/layout
                    // checks run only on review and again under the commit lock.
                    for v in &mut mesh { v.color = if reason.is_none() { [0.65, 0.7, 0.8] } else { [1., 0.2, 0.15] }; v.texture = 0; }
                    out = hair::collection::Effect::new(&mesh, &self.pose, self.sidebar_frame()).map(|e| e.paint(0., false)).unwrap_or_default();
                }
                Err(e) => reason = Some(e),
            }
        }
        self.place_mode.as_mut().unwrap().decoration_edit.as_mut().unwrap().preview_reason = reason.clone();
        let e = self.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap();
        let d = e.pending.as_ref().unwrap_or(&e.record);
        let mut lines = vec![
            format!("PLACE MODE | {} | FLOOR ONLY", self.editor_phase()),
            if self.input.captured { "LOOK CAPTURED | TAB TOOLBOX".into() } else { "MOUSE FREE | CLICK WORLD: LOOK".into() },
            e.disclosure().into(),
            if e.expected().is_some() { if e.resize { "Explicit resize; conversion not supported".into() } else { "Relocate only; conversion not supported".into() } }
            else { "Folder-owned exact artwork; retained on save".into() },
            format!("Yaw {:.2} | collision {}{}", d.yaw, d.collision, if e.pending.is_some() { " | FROZEN" } else { "" }),
            "Snapshot: partial current-room checks".into(),
            if e.pending.is_some() { "FROZEN: no writes; fresh Enter saves".into() }
            else { "Pending fresh review + final validation".into() },
            "I status (cancels) | Esc back/cancel | P exit".into(),
        ];
        if e.interrupted_resize {
            lines[2] = if self.interrupted_animation() { "Pending animation retained; not resumable".into() } else { "Pending resize retained; not resumable".into() };
            lines[3] = "Recovery required | old active retained".into();
        }
        if let Some(expected) = e.expected() {
            lines.push(format!("Expected ID: {}", expected.placement_id));
        } else if let Some(source) = e.pin() {
            lines.push(format!("Entry {} | asset {}", source.entry().id, source.asset_identity()));
            lines.push(format!("Closure: {}", &d.closure_identity().unwrap()[..16]));
        }
        if let Some(reason) = reason {
            let chars: Vec<_> = format!("Snapshot refused: {reason}").chars().collect();
            for chunk in chars.chunks(49).take(3) { lines.push(chunk.iter().collect()); }
            if chars.len() > 147 {
                let last = lines.last_mut().unwrap();
                last.pop(); last.push('~'); // Full reason remains available through I.
            }
        }
        let controls = self.decoration_controls();
        let labels: Vec<_> = controls.iter().map(|(_, r, s)| (*r, *s)).collect();
        lines.push(if e.record.artwork(&self.themes[self.theme_index].decorations).ok().flatten().is_none() {
            "Missing artwork (not a pin source)".into()
        } else { "Actual decoration artwork".into() });
        if e.expected().is_none() { lines.extend(self.palette_lines()); }
        out.extend(self.editor_toolbox(&lines, &labels));
        out
    }
}
