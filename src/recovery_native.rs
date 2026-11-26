//! Deliberate, sampled inspection. Display/preview state is never a capability.
use super::*;
use hair::placement_store::{self as store, Decoration, PlacementInspection, ResumeStage};
use place_mode_native::Control;

pub(super) struct Panel {
    inspection: Option<PlacementInspection>,
    // Indices into this ONE inspection; Published rows are counted separately.
    rows: Vec<(usize, Option<String>)>,
    published: usize,
    cursor: usize,
    selected: Option<Intent>,
    review: Option<Context>,
    notice: String,
    #[cfg(test)]
    stage_probe: Option<Box<dyn Fn(ResumeStage)>>,
    #[cfg(test)]
    outcome_trace: Option<String>,
}
struct Intent {
    index: usize,
    proposal: Decoration,
    predecessor: Option<Decoration>,
    mesh: Vec<hair::assets::Vertex>,
    context: Context,
}
#[derive(PartialEq)]
struct Context {
    theme: usize,
    source: PathBuf,
    host: hair::host_scene::HostScene,
    listing: hair::local_explorer::Snapshot,
    page: usize,
    pose: [f32; 6],
    entries: Vec<hair::decoration::Entry>,
    // Preserve appearance/source identity without copying the theme or borrowing it.
    assets: Vec<(hair::decoration::Asset, usize)>,
}
impl App {
    pub(super) fn recovery_open(&self) -> bool {
        self.place_mode.as_ref().is_some_and(|m| m.recovery.is_some())
    }
    fn recovery_panel(&self) -> &Panel { self.place_mode.as_ref().unwrap().recovery.as_ref().unwrap() }
    fn recovery_panel_mut(&mut self) -> &mut Panel { self.place_mode.as_mut().unwrap().recovery.as_mut().unwrap() }
    fn recovery_context(&self) -> Result<Context> {
        if !self.place_context() { return Err("Recovery context no longer owns input; inspect again".into()); }
        let local = self.local.as_ref().ok_or("Folder closed; inspect again")?;
        let t = &self.themes[self.theme_index];
        Ok(Context {
            theme: self.theme_index, source: t.source.clone(), host: t.host.clone(),
            listing: local.explorer.current().clone(), page: local.page,
            pose: [self.pose.position[0], self.pose.position[1], self.pose.feet_y, self.pose.eye_offset, self.pose.yaw, self.pose.pitch],
            entries: t.decorations.entries.clone(),
            assets: t.decorations.assets.iter().map(|a| (a.spec.clone(), Arc::as_ptr(&a.model) as usize)).collect(),
        })
    }
    pub(super) fn recovery_control(&mut self, control: Control) -> Result<()> {
        if !self.place_context() { return Ok(()); }
        if let Some(mode) = &mut self.place_mode { mode.cancel_pointer(); }
        if control == Control::InspectRecovery {
            self.recovery_inspect();
            return Ok(());
        }
        if !self.recovery_open() { return Ok(()); }
        match control {
            Control::SelectRecovery => self.recovery_select(),
            Control::RecoveryReview if self.recovery_panel().review.is_none() => self.recovery_enter(),
            Control::RecoveryConfirm if self.recovery_panel().review.is_some() => self.recovery_enter(),
            Control::RecoveryPrevious => { self.recovery_browse(-1); Ok(()) }
            Control::RecoveryNext => { self.recovery_browse(1); Ok(()) }
            Control::RecoveryClose => { self.recovery_close(); Ok(()) }
            _ => Ok(()),
        }
    }
    fn recovery_inspect(&mut self) {
        // Reinspection retires old intent BEFORE acquisition, including on refusal.
        self.release_navigation();
        let mode = self.place_mode.as_mut().unwrap();
        mode.edit = None; mode.decoration_edit = None; mode.recovery = None;
        let acquired = self.local.as_ref().ok_or_else(|| "Choose a local folder first".to_owned())
            .and_then(|l| store::inspect_placements(&l.explorer));
        let mut panel = Panel { inspection: None, rows: vec![], published: 0, cursor: 0,
            selected: None, review: None, notice: String::new(),
            #[cfg(test)] stage_probe: None,
            #[cfg(test)] outcome_trace: None,
        };
        match acquired {
            Ok(i) => {
                for (index, row) in i.attempts().iter().enumerate() {
                    if row.state() == store::AttemptState::Published { panel.published += 1; }
                    else { panel.rows.push((index, i.select_decoration_resume(index).err())); }
                }
                panel.notice = format!("Recovery inspection: {} unfinished storage entries; {} published entries hidden. Sampled/private/non-authorizing. No writes. Missing storage stays absent. Unrelated changing files or absent birth time can refuse availability; cold file-only provenance remains unqualified. Unsupported evidence preserved; no discard/cleanup and no budgets freed.", panel.rows.len(), panel.published);
                panel.inspection = Some(i);
            }
            Err(e) => panel.notice = format!("Recovery inspection unavailable: {e}. Evidence preserved; no writes, cleanup or automatic retries."),
        }
        self.local_status = panel.notice.clone();
        self.place_mode.as_mut().unwrap().recovery = Some(panel);
    }
    fn recovery_browse(&mut self, delta: i64) {
        let p = self.recovery_panel_mut();
        p.selected = None; p.review = None;
        p.cursor = (p.cursor as i64 + delta).clamp(0, p.rows.len().saturating_sub(1) as i64) as usize;
        self.local_status = self.recovery_status();
    }
    fn recovery_select(&mut self) -> Result<()> {
        self.recovery_panel_mut().selected = None;
        self.recovery_panel_mut().review = None;
        let p = self.recovery_panel();
        let i = p.inspection.as_ref().ok_or("New inspection required")?;
        let index = p.rows.get(p.cursor).ok_or("No unfinished attempt")?.0;
        let local = self.local.as_ref().ok_or("Folder closed")?;
        self.check_room_destination(&local.explorer)?;
        store::validate_inspection(&local.explorer, i)?;
        let s = i.select_decoration_resume(index)?;
        let mesh = self.themes[self.theme_index].decoration_preview(s.proposal())?;
        let intent = Intent { index, proposal: s.proposal().clone(), predecessor: s.predecessor().cloned(), mesh,
            context: self.recovery_context()? };
        self.recovery_panel_mut().selected = Some(intent);
        self.local_status = format!("Recovery inspection | immutable intent selected; neutral PROPOSED authored-mesh overlay only. Old active predecessor remains authoritative. Selection is write-free and not admission. Enter reviews, then a released fresh Enter resumes.\n{}", self.recovery_status());
        Ok(())
    }
    fn recovery_enter(&mut self) -> Result<()> {
        let result = self.recovery_enter_inner();
        if let Err(e) = &result { self.recovery_refusal(e); }
        result
    }
    fn recovery_enter_inner(&mut self) -> Result<()> {
        self.check_room_destination(&self.local.as_ref().ok_or("Folder closed")?.explorer)?;
        let p = self.recovery_panel();
        let intent = p.selected.as_ref().ok_or("Select a supported attempt; new inspection required after any outcome")?;
        let context = self.recovery_context()?;
        if context != intent.context || p.review.as_ref().is_some_and(|r| *r != context) {
            return Err("Folder/theme/page/listing/visitor or source context changed; inspect and select again".into());
        }
        if p.review.is_none() {
            let i = p.inspection.as_ref().ok_or("New inspection required")?;
            let local = self.local.as_ref().ok_or("Folder closed")?;
            // Independent sampled SH validation ends before scene preflight. This
            // is NOT a nested SH wrapper inside the exclusive checked action.
            store::validate_inspection(&local.explorer, i)?;
            let selection = i.select_decoration_resume(intent.index)?;
            if selection.proposal() != &intent.proposal || selection.predecessor() != intent.predecessor.as_ref() {
                return Err("Selected immutable intent changed; inspect again".into());
            }
            let candidate = selection.candidate_layout()?;
            self.themes[self.theme_index].validate_decoration_candidate_layout(&candidate)?;
            self.recovery_validate_scene(&candidate, &context)?;
            self.recovery_panel_mut().review = Some(context);
            self.local_status = format!("frozen recovery review | sampled nonmutating appearance/budget + fresh all-page geometry/routes/visitor preflight passed; NOT atomic storage authorization. No writes yet. Fresh released Enter / independent Confirm resumes, revalidating under EX at Review and Final. Escape cancels review.\n{}", self.recovery_status());
            return Ok(());
        }
        let page = context.page;
        let id = intent.proposal.placement_id.clone();
        let outcome = self.recovery_resume_checked();
        self.recovery_finish_outcome(outcome, page, &id);
        Ok(())
    }
    pub(super) fn recovery_finish_outcome(&mut self, outcome: store::Outcome, page: usize, id: &str) {
        #[cfg(test)] { self.recovery_panel_mut().outcome_trace = Some(format!("{outcome:?}")); }
        // Every outcome retires ALL acquired authority; never wire Retry to this.
        let p = self.recovery_panel_mut();
        p.selected = None; p.review = None; p.inspection = None; p.rows.clear(); p.published = 0;
        let notice = match outcome {
            store::Outcome::Committed { synced } => {
                let mode = self.place_mode.take();
                let refresh = self.placement_refresh(page);
                self.place_mode = mode;
                format!("Recovery committed: resumed same-ID {id}{}; {}. New inspection required; do not repeat the old action.",
                    if synced { "" } else { "; sync uncertain" },
                    match refresh { Ok(()) => "refresh succeeded".into(), Err(e) => format!("refresh failed: {e}; reopen; commit was NOT rolled back") })
            }
            store::Outcome::NotCommitted(e) => format!("Recovery not committed: {e}. Evidence preserved. New inspection required; no old-action retry."),
            store::Outcome::Ambiguous(e) => format!("Recovery result uncertain (Ambiguous): {e}. Inspect/reopen before further action; evidence preserved. New inspection required; no old-action retry."),
        };
        self.recovery_panel_mut().notice = notice.clone();
        self.local_status = notice;
    }
    /// L1: the ONLY App call to Theme's checked resume. It always reuses the
    /// actual fresh all-page scene consumer at BOTH stages; no test-only oracle,
    /// appearance-only callback, nested read_layout/SH wrapper or unchecked path.
    fn recovery_resume_checked(&self) -> store::Outcome {
        let result = (|| -> Result<store::Outcome> {
            let p = self.recovery_panel();
            let i = p.inspection.as_ref().ok_or("New inspection required")?;
            let intent = p.selected.as_ref().ok_or("No recovery selection")?;
            let review = p.review.as_ref().ok_or("Review required")?;
            if self.recovery_context()? != *review { return Err("Stale recovery review; inspect again".into()); }
            let selection = i.select_decoration_resume(intent.index)?;
            if selection.proposal() != &intent.proposal || selection.predecessor() != intent.predecessor.as_ref() {
                return Err("Frozen recovery intent differs; inspect again".into());
            }
            Ok(self.themes[self.theme_index].resume_decoration_checked(
                &self.local.as_ref().ok_or("Folder closed")?.explorer, &selection,
                &std::sync::atomic::AtomicBool::new(false), |stage, candidate| {
                    #[cfg(test)] if let Some(probe) = &p.stage_probe { probe(stage); }
                    match stage { ResumeStage::Review | ResumeStage::Final => {
                        self.check_room_destination(&self.local.as_ref().ok_or("Folder closed")?.explorer)?;
                        self.recovery_validate_scene(candidate, review)
                    } }
                }))
        })();
        result.unwrap_or_else(store::Outcome::NotCommitted)
    }
    fn recovery_validate_scene(&self, candidate: &store::LayoutRead, expected: &Context) -> Result<()> {
        if self.recovery_context()? != *expected { return Err("Stale recovery context; inspect again".into()); }
        let local = self.local.as_ref().ok_or("Folder closed")?;
        // Fresh listing, never clamped. Scene oracle also checks range and every
        // page, including doors/spawn/ceiling/overlap and current visitor routes.
        let mut fresh = local.explorer.try_clone()?; fresh.refresh()?;
        if local.page >= fresh.current().entries.len().max(1).div_ceil(hair::folder_scene::PAGE_SIZE) {
            return Err("Current recovery page is out of range; refresh and inspect again".into());
        }
        if fresh.current() != &expected.listing { return Err("Fresh recovery listing changed; refresh and inspect again".into()); }
        hair::placement_scene::validate_decoration_candidate(&local.explorer, candidate, self.pose.position, local.page)
    }
    pub(super) fn recovery_refusal(&mut self, e: &str) {
        if !self.recovery_open() { return; }
        let p = self.recovery_panel_mut();
        p.selected = None; p.review = None;
        // Preflight failure also retires the inspection rather than leaving a
        // stale display capability armed behind a refusal message.
        p.inspection = None; p.rows.clear(); p.published = 0;
        p.notice = format!("Recovery refused: {e}. Evidence preserved; no action committed by this request. New inspection required; no cleanup or budgets freed.");
        self.local_status = p.notice.clone();
    }
    pub(super) fn recovery_key(&mut self, code: KeyCode) -> Result<()> {
        match code {
            KeyCode::Enter if self.recovery_panel().selected.is_some() => self.recovery_enter(),
            KeyCode::Escape => {
                if self.recovery_panel().review.is_some() {
                    let p = self.recovery_panel_mut(); p.review = None; p.selected = None;
                    self.local_status = "Recovery review cancelled; view unchanged, evidence preserved; no writes or budgets freed. Select again to review; Escape closes inspection.".into();
                } else { self.recovery_close(); }
                Ok(())
            }
            KeyCode::KeyP => { self.place_exit(); Ok(()) }
            KeyCode::PageUp | KeyCode::ArrowUp => { self.recovery_browse(-1); Ok(()) }
            KeyCode::PageDown | KeyCode::ArrowDown => { self.recovery_browse(1); Ok(()) }
            _ => Ok(()), // Own movement, manipulation, catalog and context clicks.
        }
    }
    fn recovery_close(&mut self) {
        let notice = self.recovery_panel().notice.clone();
        self.place_mode.as_mut().unwrap().recovery = None;
        self.local_status = format!("{notice}\nInspection closed; view unchanged; this does NOT retire attempts, delete evidence or free budgets. I full status.");
    }
    pub(super) fn recovery_preview(&self) -> Result<Vec<hair::assets::Vertex>> {
        self.check_room_destination(&self.local.as_ref().ok_or("Folder closed")?.explorer)?;
        let intent = self.recovery_panel().selected.as_ref().ok_or("No proposed recovery overlay selected")?;
        Ok(intent.mesh.clone())
    }
    pub(super) fn recovery_status(&self) -> String {
        let p = self.recovery_panel();
        let mut status = p.notice.clone();
        if let Some(i) = &p.inspection
            && let Some((index, reason)) = p.rows.get(p.cursor) {
            let row = &i.attempts()[*index];
            status.push_str(&format!("\nAttempt {}/{} | exact storage-local ID {} | kind {:?} | state {:?} | active predecessor {}\n{}\n{}", p.cursor + 1, p.rows.len(), row.identity(), row.kind(), row.state(),
                match row.active_predecessor() { Some(true) => "known active", Some(false) => "known absent", None => "unknown / unproven" }, row.status(),
                reason.as_deref().unwrap_or("Supported complete pending decoration: select, review, then explicit resume; no automatic action")));
        }
        if let Some(s) = &p.selected {
            let d = &s.proposal;
            status.push_str(&format!("\nPROPOSED, not active: ID={} tag={} at={:?} yaw={} size={:?} collision={}\nSource: {} | closure={:?} | predecessor={:?}", d.placement_id, d.tag, d.at, d.yaw, d.size, d.collision,
                if d.is_pinned() { "retained pin (not current-theme fallback)" } else { "Follow theme (semantic tag)" }, d.closure_identity(), s.predecessor));
        }
        status
    }
    pub(super) fn recovery_controls(&self) -> Vec<(Control, [f32; 4], &'static str)> {
        let p = self.recovery_panel();
        let mut controls = vec![];
        if p.review.is_some() { controls.push((Control::RecoveryConfirm, "Confirm resume (fresh Enter)")); }
        else if p.selected.is_some() { controls.push((Control::RecoveryReview, "Review (no writes) / Enter")); }
        else if p.rows.get(p.cursor).is_some_and(|(_, e)| e.is_none()) {
            controls.push((Control::SelectRecovery, "Select attempt (no writes)"));
        }
        if p.review.is_none() {
            if p.cursor > 0 { controls.push((Control::RecoveryPrevious, "PgUp: previous unfinished entry")); }
            if p.cursor + 1 < p.rows.len() { controls.push((Control::RecoveryNext, "PgDn: next unfinished entry")); }
            controls.push((Control::InspectRecovery, "Inspect unfinished placements again"));
        }
        controls.push((Control::RecoveryClose, "Close inspection (leave unchanged)"));
        let x = (self.inventory_frame().full[0] as f32 - 308.).max(8.);
        controls.into_iter().enumerate().map(|(i, (c, s))| (c, [x, 264. + i as f32 * 24., x + 296., 286. + i as f32 * 24.], s)).collect()
    }
    pub(super) fn recovery_ui(&self) -> Vec<hair::assets::Vertex> {
        let p = self.recovery_panel();
        let mut out = vec![];
        if let Ok(mut mesh) = self.recovery_preview() {
            for v in &mut mesh { v.color = [0.55, 0.7, 0.95]; v.texture = 0; }
            // Distinct proposed authored mesh. Never omit the active predecessor.
            out = hair::collection::Effect::new(&mesh, &self.pose, self.sidebar_frame()).map(|e| e.paint(0., false)).unwrap_or_default();
        }
        let mut lines = vec!["UNFINISHED PLACEMENTS | RECOVERY".into(),
            if p.inspection.is_some() { format!("{} unfinished | {} published hidden", p.rows.len(), p.published) }
                else { "Inventory retired; new inspection required".into() },
            "Sampled/private; preserved; NOT permission".into(),
            "I FULL STATUS / SOURCE (cancels view)".into()];
        if let Some(i) = &p.inspection {
            if let Some((index, reason)) = p.rows.get(p.cursor) {
                let row = &i.attempts()[*index];
                lines.push(format!("{}/{} {}", p.cursor + 1, p.rows.len(), row.identity()));
                lines.push(format!("Kind: {:?}", row.kind())); lines.push(format!("State: {:?}", row.state()));
                lines.push(format!("Predecessor: {}", match row.active_predecessor() { Some(true) => "known active (unchanged)", Some(false) => "known absent", None => "unknown / unproven" }));
                if let Some(reason) = reason {
                    if row.kind() == store::AttemptKind::DecorationResize {
                        lines.push("Pending resize retained; not resumable".into());
                        lines.push("Recovery required | no automatic action".into());
                    } else {
                        lines.extend(format!("REFUSED: {reason}").chars().collect::<Vec<_>>().chunks(49).take(3).map(|c| c.iter().collect()));
                    }
                } else { lines.push(if p.review.is_some() { "FROZEN review; fresh Enter confirms".into() } else { "Complete pending intent | Review then resume".into() }); }
            } else { lines.push("No unfinished entries; missing storage stays absent".into()); }
        } else {
            lines.push("New inspection required | I full outcome".into());
            lines.extend(p.notice.chars().collect::<Vec<_>>().chunks(49).take(3).map(|c| c.iter().collect()));
        }
        if let Some(s) = &p.selected { lines.push(format!("PROPOSED: {} | I tag/pose/source", if s.proposal.is_pinned() { "retained pin" } else { "Follow theme" })); }
        let controls = self.recovery_controls();
        let labels: Vec<_> = controls.iter().map(|(_, r, s)| (*r, *s)).collect();
        out.extend(hair::placement::toolbox::paint(self.inventory_frame().full, &lines, &labels, None));
        out
    }
    #[cfg(test)]
    pub(super) fn recovery_test_probe(&mut self, probe: impl Fn(ResumeStage) + 'static) {
        self.recovery_panel_mut().stage_probe = Some(Box::new(probe));
    }
    #[cfg(test)]
    pub(super) fn recovery_test_outcome(&self) -> Option<&str> {
        self.recovery_panel().outcome_trace.as_deref()
    }
}
