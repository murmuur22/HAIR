//! Edit-first presentation and phase ownership. Never grants publication authority.
use super::*;
use hair::assets::Vertex;
use place_mode_native::Control;
type ControlRow = (Control, [f32; 4], &'static str);

impl App {
    pub(super) fn document_placement_label(&self, move_item: bool) -> &'static str {
        if self.inventory.is_persistent() { "COPY; source retained (Move unavailable)" }
        else if move_item { "Move" } else { "Copy (V toggles Move)" }
    }
    pub(super) fn editor_animation_mode(&self) -> hair::placement::animation::Mode {
        self.placement_action.as_ref().map(|a|a.properties.mode())
            .or_else(|| self.place_mode.as_ref()?.edit.as_ref().map(|e|e.properties.mode()))
            .or_else(|| self.place_mode.as_ref()?.decoration_edit.as_ref().map(|e|e.record.animation_mode()))
            .unwrap_or_default()
    }
    pub(super) fn editor_animation(&mut self, mode: hair::placement::animation::Mode) -> Result<()> {
        if self.place_mode.as_ref().is_some_and(|m| m.decoration_edit.is_some()) {
            return Err("Animation changes are document-only; saved decoration animation is preserved".into());
        }
        if !self.place_context() || self.editor_phase() == "Review" { return Err("Review frozen; cancel before animation changes".into()); }
        if let Some(a) = &mut self.placement_action { a.properties.set_mode(mode); }
        else if let Some(e) = self.place_mode.as_mut().and_then(|m|m.edit.as_mut()) { e.properties.set_mode(mode); }
        else { return Err("Select a file first".into()); }
        Ok(())
    }
    pub(super) fn editor_scale(&mut self, percent: u16) -> Result<()> {
        if !self.place_context() || self.editor_phase() == "Review" { return Err("Review frozen; cancel before scaling".into()); }
        if let Some(a) = &mut self.placement_action {
            let reference = a.properties.scaling.map_or([0.8, 0.85, 0.28], |s| s.reference);
            a.properties.scaling = Some(hair::placement::scaling::Scaling::new(reference, percent)?);
        } else if let Some(e) = self.place_mode.as_mut().and_then(|m| m.decoration_edit.as_mut()) {
            e.record.set_scale(percent)?;
            e.resize = true;
        } else {
            let e = self.place_mode.as_mut().and_then(|m| m.edit.as_mut()).ok_or("Select a file first")?;
            let reference = e.properties.scaling.map_or(e.size, |s| s.reference);
            let scaling = hair::placement::scaling::Scaling::new(reference, percent)?;
            e.size = scaling.size()?;
            e.properties.scaling = Some(scaling);
        }
        self.local_status = format!("Place Mode | {} | Size {percent}% | original dimensions retained; no writes", self.editor_phase());
        Ok(())
    }
    pub(super) fn editor_scale_percent(&self) -> u16 {
        self.placement_action.as_ref().map(|a| a.properties.scale_percent())
            .or_else(|| self.place_mode.as_ref()?.edit.as_ref().map(|e| e.properties.scale_percent()))
            .or_else(|| self.place_mode.as_ref()?.decoration_edit.as_ref().map(|e| e.record.scale_percent())).unwrap_or(100)
    }
    pub(super) fn editor_held(&self) -> bool {
        self.place_mode.as_ref().is_some_and(|m| m.edit.is_some() || m.decoration_edit.is_some())
            || self.placement_action.is_some()
    }
    pub(super) fn editor_phase(&self) -> &'static str {
        if self.place_mode.as_ref().is_some_and(|m| m.edit.as_ref().is_some_and(|e| e.confirming())
            || m.decoration_edit.as_ref().is_some_and(|e| e.pending.is_some()))
            || self.placement_action.as_ref().is_some_and(|a| a.pending.is_some()) { "Review" }
        else if self.editor_held() && self.place_mode.as_ref().is_some_and(|m| m.positioning) { "Positioning" }
        else if self.editor_held() { "Editing" } else { "Toolbox" }
    }
    pub(super) fn editor_arrive(&mut self, mesh: Option<Vec<Vertex>>) {
        let effect = mesh.and_then(|mut mesh| {
            for v in &mut mesh { v.texture = 0; }
            hair::collection::Effect::new(&mesh, &self.pose, self.sidebar_frame())
        });
        let action = self.placement_action.take();
        let mut mode = self.place_mode.take().unwrap_or_default();
        self.release_navigation();
        mode.positioning = false;
        mode.editor_expanded = false;
        mode.editor_page = 0;
        mode.cancel_pointer();
        mode.pickup = effect.map(|e| (Instant::now(), e));
        self.place_mode = Some(mode);
        self.placement_action = action;
        self.place_scene_refresh();
    }
    pub(super) fn editor_back(&mut self) -> Result<()> {
        let mut mode = self.place_mode.take().ok_or("No editor")?;
        let mut action = self.placement_action.take();
        self.release_navigation();
        if mode.positioning {
            mode.positioning = false;
            mode.editor_expanded = false;
            mode.editor_page = 0;
            if let Some(e) = &mut mode.edit { e.pending = None; }
            if let Some(e) = &mut mode.decoration_edit { e.pending = None; }
            if let Some(a) = &mut action { a.pending = None; a.notice = None; }
            mode.cancel_pointer();
            self.place_mode = Some(mode);
            self.placement_action = action;
            self.local_status = "Place Mode | Editing | properties retained; Enter positions; Escape cancels; no writes".into();
        } else {
            let inventory_return = mode.inventory_return;
            self.place_mode = Some(Default::default());
            self.inventory_open = inventory_return;
            self.local_status = "Place Mode | selection cancelled; original unchanged; inventory retained; no writes".into();
        }
        self.place_scene_refresh();
        Ok(())
    }
    pub(super) fn editor_inventory(&mut self) -> Result<()> {
        if self.editor_phase() == "Review" { return Err("Review frozen; cancel first".into()); }
        self.refresh_documents();
        self.place_mode = Some(Default::default());
        self.place_scene_refresh();
        self.release_navigation();
        self.inventory_open = true;
        self.local_status = "File inventory | Up/Down selects carried file; P edits selected file; B/Escape closes; no writes".into();
        Ok(())
    }
    pub(super) fn editor_controls_layout(&self, mut controls: Vec<ControlRow>) -> Vec<ControlRow> {
        if !self.editor_held() { return controls; }
        if self.editor_phase() != "Review" {
            let mut sizes = vec![
                (Control::SetAnimation(hair::placement::animation::Mode::None), [0.; 4], "Animation: None"),
                (Control::SetAnimation(hair::placement::animation::Mode::Breathe), [0.; 4], "Animation: Breathe"),
                (Control::SetAnimation(hair::placement::animation::Mode::BreatheRotate), [0.; 4], "Animation: Breathe + rotate"),
                (Control::Scale(50), [0.; 4], "Size: 50%"),
                (Control::Scale(75), [0.; 4], "Size: 75%"),
                (Control::Scale(100), [0.; 4], "Size: 100% (reset)"),
                (Control::Scale(125), [0.; 4], "Size: 125%"),
                (Control::Scale(150), [0.; 4], "Size: 150%"),
            ];
            // New animation intent is document-only; legacy decoration modes
            // remain readable and preserved without load-time writes.
            if self.place_mode.as_ref().is_some_and(|m| m.decoration_edit.is_some()) {
                sizes.retain(|(c, _, _)| !matches!(c, Control::SetAnimation(_)));
            }
            sizes.extend(controls);
            controls = sizes;
        }
        // Phase/cancel remain visible on the first page even when the new size
        // stops require property pagination. Never hide the safe escape route.
        let (mut phase, properties): (Vec<_>, Vec<_>) = controls.into_iter()
            .partition(|(c,_,_)| matches!(c, Control::Enter | Control::Cancel));
        phase.extend(properties);
        controls = phase;
        let [w, h] = self.inventory_frame().full.map(|v| v as f32);
        if self.editor_compact() {
            return vec![
                (Control::Enter, [8., 92., 200., 112.], "Enter: review (no writes)"),
                (Control::EditorDetails, [208., 92., w - 208., 112.], "Properties / full diagnostics"),
                (Control::Cancel, [w - 200., 92., w - 8., 112.], "Esc: back / cancel"),
            ];
        }
        let x = (w - 308.).max(8.);
        let collapse = self.editor_phase() == "Positioning" && self.editor_needs_compact();
        let top = if collapse { 236. } else { 214. };
        let slots = ((h - 104. - top) / 22.).floor().max(1.) as usize;
        let paged = controls.len() > slots;
        let capacity = if paged { slots.saturating_sub(1).max(1) } else { slots };
        let pages = controls.len().div_ceil(capacity);
        let page = self.place_mode.as_ref().unwrap().editor_page.min(pages.saturating_sub(1));
        controls = controls.into_iter().skip(page * capacity).take(capacity).collect();
        for (i, (_, r, _)) in controls.iter_mut().enumerate() {
            let y = top + i as f32 * 22.;
            *r = [x, y, w - 12., y + 20.];
        }
        if collapse { controls.push((Control::EditorDetails, [x, 214., w - 12., 234.], "Collapse to world positioning")); }
        if paged {
            let y = top + capacity as f32 * 22.;
            if page > 0 { controls.push((Control::EditorPrevious, [x, y, x + 144., y + 20.], "Previous properties")); }
            if page + 1 < pages { controls.push((Control::EditorNext, [w - 156., y, w - 12., y + 20.], "More properties")); }
        }
        controls
    }
    fn editor_needs_compact(&self) -> bool {
        let w = self.inventory_frame().full[0] as f32;
        // The full diagnostics column starts at w-632. Protect the central
        // crosshair plus an eight-pixel margin, including scaled HUD widths.
        w - 632. <= w * 0.5 + 8.
    }
    pub(super) fn editor_compact(&self) -> bool {
        self.editor_phase() == "Positioning" && self.editor_needs_compact()
            && self.place_mode.as_ref().is_some_and(|m| !m.editor_expanded)
    }
    pub(super) fn editor_toolbox(&self, lines: &[String], controls: &[([f32; 4], &str)]) -> Vec<Vertex> {
        let size = self.inventory_frame().full;
        if !self.editor_held() { return hair::placement::toolbox::paint(size, lines, controls, None); }
        if self.editor_compact() {
            if self.place_mode.as_ref().and_then(|m|m.decoration_edit.as_ref()).is_some_and(|e|e.interrupted_resize) {
                return hair::placement::toolbox::positioning(size,
                    if self.interrupted_animation() { "Pending animation retained; not resumable; recovery required | I full status" }
                    else { "Pending resize retained; not resumable; recovery required | I full status" }, controls);
            }
            let hint = if self.input.captured { "POSITIONING | Tab releases mouse | Enter reviews | I full status (cancels)" }
                else { "POSITIONING | Click world: look | Open properties below | I full status (cancels)" };
            let hint = format!("{} | Size {}% | {hint}", self.editor_animation_mode().label(), self.editor_scale_percent());
            return hair::placement::toolbox::positioning(size, &hint, controls);
        }
        let mut lines = lines.to_vec();
        if let Some(title) = lines.first_mut() { *title=format!("PLACE | {} | {} | {}%", self.editor_phase(), self.editor_animation_mode().label(), self.editor_scale_percent()); }
        if self.editor_animation_mode()==hair::placement::animation::Mode::BreatheRotate {
            lines.push("Rotate: art fitted inside saved envelope".into());
        }
        hair::placement::toolbox::editor(size, &lines, controls)
    }
    pub(super) fn editor_inventory_controls(&self) -> Vec<ControlRow> {
        let a = self.placement_action.as_ref().unwrap();
        let mut rows = vec![
            (Control::ReleasePointer, [0.; 4], "Tab: release mouse / toolbox"),
            (Control::Left, [0.; 4], "Left: rotate -"),
            (Control::Right, [0.; 4], "Right: rotate +"),
            (Control::PinAsset, [0.; 4], if self.inventory.is_persistent() { "V: COPY; Move unavailable" } else if a.move_item { "V: Move carried item" } else { "V: Copy carried item" }),
            (Control::Overlap, [0.; 4], "C: toggle overlaps"),
            (Control::Base, [0.; 4], a.properties.base_label()),
            (Control::Collision, [0.; 4], a.properties.collision_label()),
            (Control::Animation, [0.; 4], if a.properties.animated { "J: file animation ON" } else { "J: file animation OFF" }),
            (Control::Label, [0.; 4], if a.properties.label { "K: floating label ON" } else { "K: floating label OFF" }),
            (Control::Enter, [0.; 4], if a.pending.is_some() { "Enter: publish file + position" } else if self.editor_phase() == "Editing" { "Enter: begin positioning" } else { "Enter: review (no writes)" }),
            (Control::Cancel, [0.; 4], "Esc: back / cancel"),
            (Control::Exit, [0.; 4], "P: exit Place Mode"),
        ];
        if a.pending.is_some() { rows.retain(|(c, _, _)| matches!(c, Control::Base | Control::Collision | Control::Enter | Control::Cancel | Control::Exit)); }
        self.editor_controls_layout(rows)
    }
    pub(super) fn editor_inventory_ui(&mut self) -> Vec<Vertex> {
        let a = self.placement_action.as_ref().unwrap();
        let name = self.inventory.items().iter().find(|i| i.id == a.item)
            .map(|i| hair::folder_scene::display(&i.name)).unwrap_or_else(|| "Unavailable item".into());
        let lines = [format!("PLACE MODE | {}", self.editor_phase()), name,
            format!("{} | yaw {:.2} | overlaps {:?}", self.document_placement_label(a.move_item), a.yaw, a.overlap),
            "No writes until released fresh confirm".into(),
            "Tab toolbox | Esc back/cancel | I source".into()];
        self.local_status = if let Some(notice) = &a.notice { notice.clone() }
            else { format!("{} | {} | {} | no writes until final explicit confirm", lines[0], lines[1], lines[2]) };
        let controls = self.editor_inventory_controls();
        let labels = controls.iter().map(|(_, r, s)| (*r, *s)).collect::<Vec<_>>();
        self.editor_toolbox(&lines, &labels)
    }
    /// Geometry in a stable reference frame. Selection/async admission may replace
    /// the art, but rotation/animation never recomputes camera bounds or host pose.
    // Keep the existing mesh/material/reference-frame return contract intact.
    #[allow(clippy::type_complexity)]
    pub(super) fn editor_geometry(&self, seconds: f64) -> Result<(Vec<Vertex>, Vec<std::sync::Arc<hair::assets::Texture>>, [f32; 3])> {
        let theme = &self.themes[self.theme_index];
        if let Some(a) = &self.placement_action {
            let model = &self.inventory.preview(a.item).ok_or("Inventory preview missing")?.model;
            let p = hair::placement_store::Position { at: [0.; 2], yaw: a.yaw, size: a.properties.scaled_size([0.8, 0.85, 0.28])?, allow_overlap: false, surface: None, properties: a.properties };
            // Inventory model materials use their own one-based namespace.
            let mut mesh = theme.assembly_preview_at(&p, &model.vertices, seconds)?;
            let item_len = p.transform(&model.vertices)?.len();
            let mut textures = model.textures.clone();
            remap_theme(&mut mesh[item_len..], &theme.textures, &mut textures)?;
            return Ok((mesh, textures, [1.2, if a.properties.base { 1.9 } else { 0.85 }, 1.2]));
        }
        let m = self.place_mode.as_ref().ok_or("No editor")?;
        if let Some(e) = &m.edit {
            let p = hair::placement_store::Position { at: [0.; 2], yaw: e.yaw, size: e.size, allow_overlap: false, surface: None, properties: e.properties };
            let mut mesh = theme.assembly_preview_at(&p, &e.model, seconds)?;
            let mut textures = vec![];
            remap_theme(&mut mesh, &theme.textures, &mut textures)?;
            let reference = e.properties.scaling.map_or(e.size, |s| s.reference);
            return Ok((mesh, textures, [reference[0].max(1.2), reference[1] + if e.properties.base { 0.92 } else { 0. }, reference[2].max(1.2)]));
        }
        let e = m.decoration_edit.as_ref().ok_or("No held editor item")?;
        let mut d = e.record.clone(); d.at = [0.; 2];
        let mesh = self.decoration_mesh_at(&d, seconds)?;
        let textures = self.decoration_editor_textures(&d)?;
        if textures.len()>4 { return Err("Editor selected material budget (4)".into()); }
        Ok((mesh, textures, d.reference_size()))
    }
    pub(super) fn editor_render(&mut self) -> (Vec<Vertex>, Vec<std::sync::Arc<hair::assets::Texture>>) {
        if !self.editor_held() || !self.place_context() || self.editor_compact() { return (vec![], vec![]); }
        match self.editor_geometry(self.started.elapsed().as_secs_f64()) {
            Ok((mesh, textures, reference)) => (project(&mesh, reference, self.inventory_frame().full), textures),
            Err(e) => { self.local_status = format!("Editor preview refused: {e}; no writes; cancel/reselect"); (vec![], vec![]) }
        }
    }
    pub(super) fn editor_preview_arrival(&mut self) {
        let theme = &self.themes[self.theme_index];
        if let Some(e) = self.place_mode.as_mut().and_then(|m| m.edit.as_mut()) {
            // Room generation/binding changes cancel via existing navigation
            // ownership. Never resolve a retained name against another row.
            if self.world.folder_view.as_ref().and_then(|v| v.bindings.get(&e.anchor))
                .is_some_and(|b| b.entry.name == e.target.name()) {
                e.model = theme.file_item_model(&e.anchor, e.target.name());
            }
        }
    }
}
fn remap_theme(mesh: &mut [Vertex], source: &[std::sync::Arc<hair::assets::Texture>], textures: &mut Vec<std::sync::Arc<hair::assets::Texture>>) -> Result<()> {
    for v in mesh {
        let id = (v.texture & 0x7fff_ffff) as usize;
        if id == 0 { continue; }
        let t = source.get(id - 1).ok_or("Editor material identity missing")?;
        let i = match textures.iter().position(|old| std::sync::Arc::ptr_eq(old, t)) {
            Some(i) => i,
            None => { if textures.len() >= 4 { return Err("Editor selected material budget (4)".into()); } textures.push(t.clone()); textures.len() - 1 }
        };
        v.texture = (i as u32 + 1) | (v.texture & 0x8000_0000);
    }
    Ok(())
}
fn project(mesh: &[Vertex], reference: [f32; 3], size: [u32; 2]) -> Vec<Vertex> {
    use glam::{Mat3, Vec3};
    if mesh.is_empty() || mesh.len() > 300_000 || !mesh.len().is_multiple_of(3)
        || mesh.iter().any(|v| v.position.iter().any(|n| !n.is_finite())) { return vec![]; }
    let radius = Vec3::from_array(reference).length().max(0.01) * 0.75;
    let origin = Vec3::new(0., reference[1] * 0.5, 0.);
    let rotation = Mat3::from_rotation_x(-0.25) * Mat3::from_rotation_y(0.35);
    let [w, h] = size.map(|v| v as f32);
    let center = [w - 160., 148.];
    mesh.iter().map(|v| {
        let q = rotation * (Vec3::from_array(v.position) - origin) / radius;
        let perspective = 3. / (3. - q.z);
        Vertex { position: [2. * (center[0] + q.x * 52. * perspective) / w - 1.,
            1. - 2. * (center[1] - q.y * 52. * perspective) / h, 0.9 - 1. / (3. - q.z)], ..*v }
    }).collect()
}
