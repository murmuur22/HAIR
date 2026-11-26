//! Deliberate layout editing. Collection placement remains a separate inventory action.
use super::*;

#[derive(Default)]
pub(super) struct Mode {
    pub positioning: bool,
    pub editor_expanded: bool,
    pub editor_page: usize,
    pub inventory_return: bool,
    pub pickup: Option<(Instant, hair::collection::Effect)>,
    pub edit: Option<Edit>,
    pub decoration_edit: Option<place_decoration_native::DecorationEdit>,
    pub recovery: Option<recovery_native::Panel>,
    palette: bool,
    palette_cursor: usize,
    // Transient metadata, not a placement record or file acquisition.
    pub(super) decoration: Option<(PathBuf, String)>,
    press: Option<(Control, hair::sidebar::Frame)>,
    mouse_held: bool,
}
impl Mode {
    pub(super) fn cancel_pointer(&mut self) {
        self.press = None;
    }
}
pub(super) struct Edit {
    pub(super) target: hair::placement_store::Existing,
    pub(super) anchor: String,
    pub(super) model: Vec<hair::assets::Vertex>,
    pub(super) yaw: f32,
    pub(super) size: [f32; 3],
    overlap: hair::placement::Overlap,
    pub(super) properties: hair::placement::assembly::Properties,
    pub(super) pending: Option<hair::placement_store::Position>,
}
impl Edit {
    pub(super) fn confirming(&self) -> bool {
        self.pending.is_some()
    }
    #[cfg(test)]
    pub(super) fn reviewed_position(&self) -> Option<&hair::placement_store::Position> {
        self.pending.as_ref()
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Control {
    Left,
    Right,
    Overlap,
    Base,
    Collision,
    Animation,
    SetAnimation(hair::placement::animation::Mode),
    Label,
    Enter,
    Cancel,
    Palette,
    Exit,
    Choice(usize),
    Previous,
    Next,
    PlaceDecoration,
    FollowTheme,
    PinAsset,
    InspectRecovery,
    SelectRecovery,
    RecoveryPrevious,
    RecoveryNext,
    RecoveryReview,
    RecoveryConfirm,
    RecoveryClose,
    FileInventory,
    ReleasePointer,
    EditorDetails,
    EditorPrevious,
    EditorNext,
    Scale(u16),
}
impl Control {
    fn key(self) -> KeyCode {
        match self {
            Self::Left => KeyCode::ArrowLeft,
            Self::Right => KeyCode::ArrowRight,
            Self::Overlap => KeyCode::KeyC,
            Self::Base => KeyCode::KeyG,
            Self::Collision => KeyCode::KeyH,
            Self::Animation => KeyCode::KeyJ,
            Self::Label => KeyCode::KeyK,
            Self::Enter => KeyCode::Enter,
            Self::Cancel => KeyCode::Escape,
            Self::Palette => KeyCode::KeyB,
            Self::Exit => KeyCode::KeyP,
            Self::FileInventory => KeyCode::KeyY,
            Self::ReleasePointer => KeyCode::Tab,
            Self::Previous => KeyCode::PageUp,
            Self::Next => KeyCode::PageDown,
            Self::Choice(_) => KeyCode::Enter,
            Self::PlaceDecoration => KeyCode::KeyN,
            Self::FollowTheme => KeyCode::KeyF,
            Self::PinAsset => KeyCode::KeyV,
            Self::SetAnimation(_) | Self::Scale(_) | Self::EditorDetails | Self::EditorPrevious | Self::EditorNext
            | Self::InspectRecovery | Self::SelectRecovery | Self::RecoveryPrevious
            | Self::RecoveryNext | Self::RecoveryReview | Self::RecoveryConfirm
            | Self::RecoveryClose => unreachable!("recovery controls have no conflicting hotkey"),
        }
    }
}
impl App {
    pub(super) fn place_context(&self) -> bool {
        self.focused
            && self.menu.is_none()
            && self.address.is_none()
            && !self.world.reading
            && !self.chooser_pending
            && !self.chooser_active
            && !self.inventory_open
            && self.map_mode != hair::exploration::MapMode::Expanded
    }
    pub(super) fn place_exit(&mut self) {
        let recovery_status = self.recovery_open().then(|| self.local_status.clone());
        self.place_mode = None;
        self.place_scene_refresh();
        self.release_navigation();
        self.local_status = recovery_status.map_or_else(
            || "Place Mode exited; unconfirmed preview discarded".into(),
            |s| format!("{s}\nPlace Mode exited; recovery evidence unchanged; I full status"));
    }
    pub(super) fn place_key(
        &mut self,
        code: KeyCode,
        down: bool,
        repeat: bool,
    ) -> Option<Result<bool>> {
        let fresh_p = code == KeyCode::KeyP && down && !self.place_p_held;
        if code == KeyCode::KeyP {
            self.place_p_held = down;
        }
        if !self.place_context() {
            if self.recovery_open() {
                self.place_mode = None;
                self.local_status = "Recovery inspection cancelled by input-context change; evidence preserved; new inspection required".into();
            }
            return None;
        }
        if self.place_mode.is_none() {
            if code != KeyCode::KeyP || self.placement_action.is_some() {
                return None;
            }
            if !down {
                self.menu_held.remove(&code);
            }
            if fresh_p && !repeat {
                self.release_navigation();
                self.place_mode = Some(Mode::default());
                self.local_status = "Place Mode | X selects existing file; B decorations; Escape toolbox; P exit; I source".into();
            }
            return Some(Ok(false));
        }
        if !down {
            self.menu_held.remove(&code);
            self.release_latch(code);
            match code {
                KeyCode::KeyW => self.input.forward = false,
                KeyCode::KeyS => self.input.back = false,
                KeyCode::KeyA => self.input.left = false,
                KeyCode::KeyD => self.input.right = false,
                _ => {}
            }
            return Some(Ok(false));
        }
        if repeat || (code == KeyCode::KeyP && !fresh_p) || !self.menu_held.insert(code) {
            return Some(Ok(false));
        }
        // Explicit context/navigation requests discard the edit before passing ownership on.
        if !((self.place_mode.as_ref().is_some_and(|m| m.palette) || self.recovery_open())
            && matches!(code, KeyCode::PageUp | KeyCode::PageDown))
            && matches!(
                code,
                KeyCode::KeyM
                    | KeyCode::KeyI
                    | KeyCode::KeyO
                    | KeyCode::KeyU
                    | KeyCode::KeyR
                    | KeyCode::F5
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::KeyQ
                    | KeyCode::KeyL
                    | KeyCode::KeyT
                    | KeyCode::KeyE
            )
        {
            // I must inspect the full authoritative refusal, not the generic
            // exit message. Snapshot diagnostics are separate from that status.
            let source_status = if code == KeyCode::KeyI && self.recovery_open() {
                Some(format!("{}\n{}", self.local_status, self.recovery_status()))
            } else if code == KeyCode::KeyI && self.local_status.starts_with("Recovery") {
                Some(self.local_status.clone())
            } else { (code == KeyCode::KeyI).then(|| {
                self.place_mode.as_ref().and_then(|m| m.decoration_edit.as_ref()).map(|e| {
                    let mut status = self.local_status.clone();
                    if let Some(reason) = &e.preview_reason {
                        status.push_str(&format!("\nPREVIEW (partial snapshot): {reason}"));
                    }
                    status
                })
            }).flatten() };
            self.place_exit();
            if let Some(status) = source_status { self.local_status = status; }
            self.menu_held.remove(&code);
            return None;
        }
        let result = self.place_action(code);
        if let Err(e) = result {
            if self.recovery_open() { self.recovery_refusal(&e); }
            else { self.place_refusal(&e); }
        }
        Some(Ok(false))
    }
    fn place_refusal(&mut self, error: &str) {
        self.local_status = if let Some(e) = self.place_mode.as_ref().and_then(|m| m.decoration_edit.as_ref()) {
            format!("{error}; {}", if e.pending.is_some() {
                "no writes yet; fresh Enter saves; Escape cancels; P exit; I full status"
            } else {
                if e.expected().is_some() { "no relocation committed by this attempt; inspect/reselect; pending evidence may require recovery; Escape cancel; P exit; I full status" }
                else if e.record.is_pinned() { "no pin committed by this attempt; inspect before retry; pending evidence may require recovery; Escape cancel; P exit; I full status" }
                else { "no new decoration committed by this attempt; inspect before retry; pending evidence may require recovery; Escape cancel; P exit; I full status" }
            })
        } else {
            format!("Place Mode: {error}; original retained; adjust or cancel")
        };
    }
    fn place_release_pointer(&mut self) {
        if let Some(mode) = &mut self.place_mode {
            mode.cancel_pointer();
        }
        let mode = self.place_mode.take();
        let action = self.placement_action.take();
        self.release_navigation();
        self.place_mode = mode;
        self.placement_action = action;
    }
    fn place_action(&mut self, code: KeyCode) -> Result<()> {
        if let Some(mode) = &mut self.place_mode {
            mode.cancel_pointer();
        }
        if self.recovery_open() { return self.recovery_key(code); }
        if code == KeyCode::Tab {
            self.place_release_pointer();
            if let Some(m) = &mut self.place_mode { m.editor_expanded = false; }
            self.local_status = format!("Place Mode | {} | mouse released; click world to look; Escape back/cancel", self.editor_phase());
            return Ok(());
        }
        if code == KeyCode::Escape && self.editor_held() {
            return self.editor_back();
        }
        if code == KeyCode::KeyY { return self.editor_inventory(); }
        if code == KeyCode::Enter && self.editor_held()
            && !self.place_mode.as_ref().unwrap().palette
            && !self.place_mode.as_ref().unwrap().positioning {
            self.place_mode.as_mut().unwrap().positioning = true;
            self.place_mode.as_mut().unwrap().editor_expanded = false;
            self.place_mode.as_mut().unwrap().editor_page = 0;
            self.capture_look();
            self.local_status = "Place Mode | Positioning | aim at a surface; Tab releases mouse for toolbox; Escape back to Editing; Enter reviews (no writes)".into();
            return Ok(());
        }
        if self.placement_action.is_some() && !matches!(code, KeyCode::KeyP | KeyCode::Escape) {
            // The outer Place Mode latch already admitted this fresh press.
            self.menu_held.remove(&code);
            let _ = self.placement_key(code, true, false);
            return Ok(());
        }
        if self.place_mode.as_ref().is_some_and(|m| m.palette)
            && !matches!(code, KeyCode::Escape | KeyCode::KeyB | KeyCode::KeyP)
        {
            match code {
                KeyCode::ArrowUp => self.place_palette_move(-1),
                KeyCode::ArrowDown => self.place_palette_move(1),
                KeyCode::PageUp => self.place_palette_move(-(self.palette_slots() as i64)),
                KeyCode::PageDown => self.place_palette_move(self.palette_slots() as i64),
                KeyCode::Enter => {
                    self.place_choose(self.place_mode.as_ref().unwrap().palette_cursor)?
                }
                _ => {} // Palette owns input; never confirm a file or acquire bytes.
            }
            return Ok(());
        }
        if self.place_mode.as_ref().is_some_and(|m| m.decoration_edit.is_some())
            && !matches!(code, KeyCode::KeyP | KeyCode::Escape)
        {
            let e = self.place_mode.as_mut().unwrap().decoration_edit.as_mut().unwrap();
            if code == KeyCode::Enter { return self.decoration_enter(); }
            if e.pending.is_some() { return Err("Review frozen; cancel before adjusting".into()); }
            match code {
                KeyCode::KeyF => return self.decoration_source(false),
                KeyCode::KeyV => return self.decoration_source(true),
                KeyCode::ArrowLeft => e.record.yaw = (e.record.yaw - 0.2).rem_euclid(std::f32::consts::TAU),
                KeyCode::ArrowRight => e.record.yaw = (e.record.yaw + 0.2).rem_euclid(std::f32::consts::TAU),
                KeyCode::KeyH => e.record.collision = !e.record.collision,
                KeyCode::KeyW => self.input.forward = self.input.captured,
                KeyCode::KeyS => self.input.back = self.input.captured,
                KeyCode::KeyA => self.input.left = self.input.captured,
                KeyCode::KeyD => self.input.right = self.input.captured,
                _ => {}
            }
            return Ok(());
        }
        match code {
            KeyCode::KeyN => self.decoration_start()?,
            KeyCode::KeyP => self.place_exit(),
            KeyCode::Escape => {
                if self.input.captured {
                    self.place_release_pointer();
                    self.local_status = "Place Mode | mouse released for toolbox; Escape cancels selection, then exits; next Escape menu".into();
                } else if self.place_mode.as_ref().is_some_and(|m| m.palette) {
                    self.place_mode.as_mut().unwrap().palette = false;
                } else if self.place_mode.as_mut().unwrap().decoration_edit.take().is_some() {
                    self.place_scene_refresh();
                    self.local_status = "Decoration preview cancelled; no writes".into();
                } else if self.place_mode.as_mut().unwrap().edit.take().is_some() {
                    self.local_status =
                        "Place Mode | selection cancelled; original unchanged; X select; P exit"
                            .into();
                } else {
                    self.place_exit();
                }
            }
            KeyCode::KeyB => {
                if self
                    .place_mode
                    .as_ref()
                    .and_then(|m| m.edit.as_ref())
                    .is_some_and(Edit::confirming)
                {
                    return Err("Review frozen; cancel before opening decoration palette".into());
                }
                self.place_release_pointer();
                let mode = self.place_mode.as_mut().unwrap();
                mode.palette = !mode.palette;
                self.local_status = self.place_palette_status();
            }
            KeyCode::KeyX => self.place_select()?,
            KeyCode::Enter => self.place_enter()?,
            KeyCode::ArrowLeft
            | KeyCode::ArrowRight
            | KeyCode::KeyC
            | KeyCode::KeyG
            | KeyCode::KeyH
            | KeyCode::KeyJ
            | KeyCode::KeyK => {
                if let Some(e) = self.place_mode.as_mut().and_then(|m| m.edit.as_mut()) {
                    if e.pending.is_some() {
                        return Err("Review is frozen; cancel before adjusting".into());
                    }
                    if code == KeyCode::KeyG {
                        e.properties.base = !e.properties.base;
                    } else if code == KeyCode::KeyH {
                        if e.properties.base {
                            e.properties.base_collision = !e.properties.base_collision;
                        }
                    } else if code == KeyCode::KeyJ {
                        e.properties.set_mode(if e.properties.animated { hair::placement::animation::Mode::None } else { hair::placement::animation::Mode::Breathe });
                    } else if code == KeyCode::KeyK {
                        e.properties.label = !e.properties.label;
                    } else if code == KeyCode::KeyC {
                        e.overlap = if e.overlap == hair::placement::Overlap::Prevent {
                            hair::placement::Overlap::Allow
                        } else {
                            hair::placement::Overlap::Prevent
                        };
                    } else {
                        e.yaw = (e.yaw
                            + if code == KeyCode::ArrowRight {
                                0.2
                            } else {
                                -0.2
                            })
                        .rem_euclid(std::f32::consts::TAU);
                    }
                }
            }
            KeyCode::KeyW => self.input.forward = self.input.captured,
            KeyCode::KeyS => self.input.back = self.input.captured,
            KeyCode::KeyA => self.input.left = self.input.captured,
            KeyCode::KeyD => self.input.right = self.input.captured,
            _ => {}
        }
        Ok(())
    }
    pub(super) fn place_palette_status_for_editor(&self) -> String { self.place_palette_status() }
    fn place_palette_status(&self) -> String {
        let theme = &self.themes[self.theme_index];
        match self.place_decoration_selection() {
            Some(e) => format!(
                "Place Mode | {} | {} | entry={} tag={} asset={} collision={} | selection only; B/Esc close; N Place selected decoration",
                theme.name, e.name, e.id, e.tag, e.asset, e.defaults.collision
            ),
            None if theme.decorations.entries.is_empty() => format!(
                "Place Mode | {} has no selectable decoration entries; legacy appearance unchanged",
                theme.name
            ),
            None => format!(
                "Place Mode | {} decoration catalog: {} choices | arrows browse; Enter/click selects without saving",
                theme.name,
                theme.decorations.entries.len()
            ),
        }
    }
    pub(super) fn place_decoration_selection(&self) -> Option<&hair::decoration::Entry> {
        let (source, id) = self.place_mode.as_ref()?.decoration.as_ref()?;
        let theme = &self.themes[self.theme_index];
        (source == &theme.source)
            .then(|| theme.decorations.entry(id))
            .flatten()
    }
    fn palette_slots(&self) -> usize {
        ((self.inventory_frame().full[1] as f32 - 296.0) / 24.0).clamp(1.0, 32.0) as usize
    }
    fn place_palette_move(&mut self, delta: i64) {
        let count = self.themes[self.theme_index].decorations.entries.len();
        let m = self.place_mode.as_mut().unwrap();
        m.palette_cursor =
            (m.palette_cursor as i64 + delta).clamp(0, count.saturating_sub(1) as i64) as usize;
        m.cancel_pointer();
    }
    fn place_choose(&mut self, index: usize) -> Result<()> {
        let theme = &self.themes[self.theme_index];
        let entry = theme
            .decorations
            .entries
            .get(index)
            .ok_or("No decoration entry to select")?;
        let m = self.place_mode.as_mut().unwrap();
        if m.edit.as_ref().is_some_and(Edit::confirming)
            || m.decoration_edit.as_ref().is_some_and(|e| e.pending.is_some()) {
            return Err("Review frozen; cancel first".into());
        }
        m.decoration = Some((theme.source.clone(), entry.id.clone()));
        m.palette_cursor = index;
        m.edit = None;
        m.decoration_edit = None;
        m.palette = false;
        self.decoration_start()?;
        Ok(())
    }
    pub(super) fn palette_controls(&self) -> Vec<(Control, [f32; 4], String)> {
        let frame = self.inventory_frame();
        let x = (frame.full[0] as f32 - 308.0).max(8.0);
        let h = frame.full[1] as f32;
        let slots = self.palette_slots();
        let entries = &self.themes[self.theme_index].decorations.entries;
        let cursor = self
            .place_mode
            .as_ref()
            .map_or(0, |m| m.palette_cursor)
            .min(entries.len().saturating_sub(1));
        let start = cursor / slots * slots;
        let mut controls: Vec<_> = entries
            .iter()
            .enumerate()
            .skip(start)
            .take(slots)
            .map(|(i, e)| {
                let y = 196.0 + (i - start) as f32 * 24.0;
                (
                    Control::Choice(i),
                    [x, y, x + 296.0, y + 22.0],
                    format!(
                        "{} {}",
                        if self
                            .place_decoration_selection()
                            .is_some_and(|s| s.id == e.id)
                        {
                            "*"
                        } else if i == cursor {
                            ">"
                        } else {
                            " "
                        },
                        e.name
                    ),
                )
            })
            .collect();
        if start > 0 {
            controls.push((
                Control::Previous,
                [x, h - 92.0, x + 144.0, h - 70.0],
                "PgUp: previous".into(),
            ));
        }
        if start + slots < entries.len() {
            controls.push((
                Control::Next,
                [x + 150.0, h - 92.0, x + 296.0, h - 70.0],
                "PgDn: next".into(),
            ));
        }
        controls.push((
            Control::Palette,
            [x, h - 64.0, x + 296.0, h - 44.0],
            "B / Esc: close palette".into(),
        ));
        controls.push((
            Control::Exit,
            [x, h - 38.0, x + 296.0, h - 18.0],
            "P: exit Place Mode".into(),
        ));
        controls
    }
    pub(super) fn palette_lines(&self) -> Vec<String> {
        let theme = &self.themes[self.theme_index];
        let mut lines = vec![
            "DECORATIONS | SELECT ONLY; NOT PLACED".into(),
            format!("Theme: {}", theme.name),
        ];
        if let Some(e) = self.place_decoration_selection() {
            for field in [
                format!("Name: {}", e.name),
                format!("Entry: {}", e.id),
                format!("Tag: {}", e.tag),
                format!("Asset: {}", e.asset),
                format!("Collision default: {}", e.defaults.collision),
            ] {
                let chars: Vec<_> = field.chars().collect();
                lines.extend(
                    chars
                        .chunks(49)
                        .map(|chunk| chunk.iter().collect::<String>()),
                );
            }
        } else {
            lines.push(if theme.decorations.entries.is_empty() {
                "No catalog entries in this theme".into()
            } else {
                "Arrows browse; Enter / click select".into()
            });
            lines.push("MOUSE FREE | no placement / layout writes".into());
            lines.push("PgUp / PgDn pages | B / Esc close".into());
        }
        lines
    }
    fn place_select(&mut self) -> Result<()> {
        self.check_room_destination(&self.local.as_ref().ok_or("Choose a local folder first")?.explorer)?;
        if self
            .place_mode
            .as_ref()
            .is_some_and(|m| m.edit.as_ref().is_some_and(|e| e.pending.is_some()))
        {
            return Err("Review frozen; cancel selection first".into());
        }
        if let Some(d) = hair::interaction::decoration_target(&self.themes[self.theme_index].host, &self.pose, &self.world) {
            return self.decoration_reposition(d.clone());
        }
        let aimed = hair::interaction::resolve(
            &self.themes[self.theme_index].host,
            &self.pose,
            &self.world,
        )
        .ok_or("Aim at an existing file object within reach")?;
        let bound = self
            .world
            .folder_view
            .as_ref()
            .and_then(|v| v.bindings.get(&aimed.anchor))
            .ok_or("Select a local file, not a doorway")?;
        if bound.entry.kind != hair::local_explorer::Kind::File {
            return Err("Only regular file objects can be repositioned".into());
        }
        let target = hair::placement_store::select_existing(
            &self
                .local
                .as_ref()
                .ok_or("Choose a local folder first")?
                .explorer,
            &bound.entry.name,
        )?;
        let theme = &self.themes[self.theme_index];
        // Current rendered file mesh, never its furniture or acquired bytes.
        let model = theme.file_item_model(&aimed.anchor, &bound.entry.name);
        if model.is_empty() {
            return Err("File preview unavailable".into());
        }
        let yaw = theme
            .host
            .file_placements
            .get(&aimed.anchor)
            .map_or(0.0, |p| p.position.yaw);
        let saved = theme.host.file_placements.get(&aimed.anchor);
        let size = saved.map_or([0.8, 0.85, 0.28], |p| p.position.size);
        let overlap = if saved.is_some_and(|p| p.position.allow_overlap) {
            hair::placement::Overlap::Allow
        } else {
            hair::placement::Overlap::Prevent
        };
        self.local_status = format!(
            "Place Mode | Editing {} | layout-only; arrows rotate; C overlap; G base; H base collision; J animation; K label; Enter positions",
            hair::folder_scene::display(target.name())
        );
        self.place_mode.as_mut().unwrap().edit = Some(Edit {
            target,
            anchor: aimed.anchor,
            model,
            yaw,
            size,
            overlap,
            properties: saved.map_or(Default::default(), |p| p.position.properties),
            pending: None,
        });
        self.place_mode.as_mut().unwrap().decoration = None;
        let e = self.place_mode.as_ref().unwrap().edit.as_ref().unwrap();
        let mesh = self.themes[self.theme_index].file_item_shape(&e.anchor, e.target.name());
        self.editor_arrive(Some(mesh));
        Ok(())
    }
    pub(super) fn place_contact(&self) -> Result<hair::placement::surface::Contact> {
        self.surface_target(
            self.place_mode
                .as_ref()
                .and_then(|m| m.edit.as_ref())
                .map(|e| e.anchor.as_str()),
        )
    }
    pub(super) fn place_preview(&self) -> Result<Vec<hair::assets::Vertex>> {
        self.place_preview_at(0.)
    }
    pub(super) fn place_preview_at(&self, seconds: f64) -> Result<Vec<hair::assets::Vertex>> {
        self.check_room_destination(&self.local.as_ref().ok_or("Folder closed")?.explorer)?;
        if self.recovery_open() { return self.recovery_preview(); }
        if self.placement_action.is_some() { return self.placement_preview_at(seconds); }
        if self.place_mode.as_ref().is_some_and(|m| m.decoration_edit.is_some()) {
            return self.decoration_mesh(&self.decoration_position()?);
        }
        let e = self
            .place_mode
            .as_ref()
            .and_then(|m| m.edit.as_ref())
            .ok_or("X selects an aimed existing file")?;
        if let Some(p) = &e.pending {
            return self.themes[self.theme_index].assembly_preview_at(p, &e.model, seconds);
        }
        let contact = self.place_contact()?;
        self.themes[self.theme_index].assembly_preview_at(
            &hair::placement_store::Position {
                at: [contact.at[0], contact.at[2]],
                yaw: e.yaw,
                size: e.size,
                allow_overlap: e.overlap == hair::placement::Overlap::Allow,
                surface: Some(contact),
                properties: e.properties,
            },
            &e.model,
            seconds,
        )
    }
    fn place_enter(&mut self) -> Result<()> {
        let e = self
            .place_mode
            .as_ref()
            .and_then(|m| m.edit.as_ref())
            .ok_or("Select a file with X first")?;
        let local = self.local.as_ref().ok_or("Folder closed")?;
        self.check_room_destination(&local.explorer)?;
        hair::placement_store::validate_existing(&local.explorer, &e.target)?;
        match e.pending.as_ref() {
            None => {
                let contact = self.place_contact()?;
                let p = hair::placement_store::Position {
                    at: [contact.at[0], contact.at[2]],
                    yaw: e.yaw,
                    size: e.size,
                    allow_overlap: e.overlap == hair::placement::Overlap::Allow,
                    surface: Some(contact),
                    properties: e.properties,
                };
                let layout = hair::placement_store::read_layout(&local.explorer)?;
                self.themes[self.theme_index].validate_layout_decorations(&layout)?;
                hair::placement_scene::validate_reposition(
                    &local.explorer,
                    &layout,
                    e.target.name(),
                    &p,
                    self.pose.position,
                    local.page,
                )?;
                self.place_mode
                    .as_mut()
                    .unwrap()
                    .edit
                    .as_mut()
                    .unwrap()
                    .pending = Some(p);
                self.place_release_pointer();
                self.local_status = "Place Mode | frozen review; fresh Enter confirms layout only; Escape back to Editing; no writes yet".into();
            }
            Some(p) => {
                let mut page = local.page;
                let outcome = hair::placement_store::reposition_checked(
                    &local.explorer,
                    &e.target,
                    p,
                    |layout| {
                        self.check_room_destination(&local.explorer)?;
                        self.themes[self.theme_index].validate_layout_decorations(layout)?;
                        page = hair::placement_scene::validate_reposition(
                            &local.explorer,
                            layout,
                            e.target.name(),
                            p,
                            self.pose.position,
                            local.page,
                        )?;
                        Ok(())
                    },
                );
                match outcome {
                    hair::placement_store::Outcome::Committed { synced } => {
                        self.place_mode.as_mut().unwrap().edit = None;
                        let mode = self.place_mode.take();
                        let refreshed = self.placement_refresh(page);
                        self.place_mode = mode;
                        self.local_status = match refreshed {
                            Ok(()) => format!(
                                "Place Mode | reposition saved; file bytes and identity unchanged{}",
                                if synced {
                                    ""
                                } else {
                                    "; sync uncertain; inspect before retry"
                                }
                            ),
                            Err(err) => format!(
                                "Place Mode | layout committed; refresh failed: {err}; F5, do not repeat"
                            ),
                        };
                    }
                    hair::placement_store::Outcome::NotCommitted(err) => {
                        self.place_mode
                            .as_mut()
                            .unwrap()
                            .edit
                            .as_mut()
                            .unwrap()
                            .pending = None;
                        return Err(err);
                    }
                    hair::placement_store::Outcome::Ambiguous(err) => {
                        self.place_mode.as_mut().unwrap().edit = None;
                        self.local_status = format!(
                            "Place Mode | result uncertain; inspect/reopen before retry: {err}"
                        );
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn toolbox_controls(&self) -> Vec<(Control, [f32; 4], &'static str)> {
        if self.recovery_open() { return self.recovery_controls(); }
        if self.placement_action.is_some() { return self.editor_inventory_controls(); }
        if self.place_mode.as_ref().is_some_and(|m| m.decoration_edit.is_some()) {
            return self.decoration_controls();
        }
        let frame = self.inventory_frame();
        let x = (frame.full[0] as f32 - 308.0).max(8.0);
        let edit = self.place_mode.as_ref().and_then(|m| m.edit.as_ref());
        let confirming = edit.is_some_and(Edit::confirming);
        let properties = edit.map_or(Default::default(), |e| e.properties);
        let controls = [
            (Control::FileInventory, "File inventory"),
            (Control::ReleasePointer, "Tab: release mouse / toolbox"),
            (Control::InspectRecovery, "Inspect unfinished placements"),
            (Control::PlaceDecoration, "N: Place selected decoration"),
            (Control::Left, "Left: rotate -"),
            (Control::Right, "Right: rotate +"),
            (Control::Overlap, "C: toggle overlaps"),
            (Control::Base, properties.base_label()),
            (Control::Collision, properties.collision_label()),
            (
                Control::Animation,
                if edit.is_some_and(|e| e.properties.animated) {
                    "J: file animation ON"
                } else {
                    "J: file animation OFF"
                },
            ),
            (
                Control::Label,
                if edit.is_none_or(|e| e.properties.label) {
                    "K: floating label ON"
                } else {
                    "K: floating label OFF"
                },
            ),
            (
                Control::Enter,
                if confirming {
                    "Enter: save file position"
                } else if !self.place_mode.as_ref().unwrap().positioning {
                    "Enter: begin positioning"
                } else {
                    "Enter: review (no writes)"
                },
            ),
            (Control::Cancel, "Esc: back / cancel"),
            (Control::Palette, "B: theme decorations"),
            (Control::Exit, "P: exit Place Mode"),
        ]
        .into_iter()
        .filter(|(c, _)| match c {
            Control::InspectRecovery => edit.is_none() && self.local.is_some(),
            Control::PlaceDecoration => self.place_decoration_selection().is_some() && !confirming,
            Control::Left | Control::Right | Control::Overlap => edit.is_some() && !confirming,
            // Review still discloses the assembly's physics choices. Actions
            // continue through place_action's frozen-review mutation guard.
            Control::Base | Control::Collision => edit.is_some(),
            Control::Animation | Control::Label => edit.is_some() && !confirming,
            Control::Enter => edit.is_some(),
            _ => true,
        })
        .enumerate()
        .map(|(i, (c, s))| {
            (
                c,
                [
                    x,
                    196.0 + i as f32 * 22.0,
                    x + 296.0,
                    216.0 + i as f32 * 22.0,
                ],
                s,
            )
        })
        .collect();
        self.editor_controls_layout(controls)
    }
    fn toolbox_hit(&self, point: Option<[f64; 2]>) -> Option<Control> {
        let p = self.inventory_frame().full_point(point?)?;
        if self.place_mode.as_ref().is_some_and(|m| m.palette) {
            return self
                .palette_controls()
                .into_iter()
                .find(|(_, r, _)| hair::directory_map_view::contains(*r, p))
                .map(|(c, _, _)| c);
        }
        self.toolbox_controls()
            .into_iter()
            .find(|(_, r, _)| hair::directory_map_view::contains(*r, p))
            .map(|(c, _, _)| c)
    }
    pub(super) fn place_pointer(&mut self, down: bool) {
        if !self.place_context() || self.place_mode.is_none() {
            return;
        }
        if self.input.captured {
            if !down {
                let mode = self.place_mode.as_mut().unwrap();
                mode.mouse_held = false;
                mode.press = None;
            }
            return;
        }
        let frame = self.inventory_frame();
        let hit = self.toolbox_hit(self.cursor);
        if down {
            let m = self.place_mode.as_mut().unwrap();
            if m.mouse_held {
                return;
            }
            m.mouse_held = true;
            m.press = hit.map(|c| (c, frame));
            if hit.is_none() {
                // A fresh click outside chrome only recaptures; never acts on a file.
                let inside = self
                    .cursor
                    .and_then(|p| frame.full_point(p))
                    .is_some_and(|p| {
                        if self.editor_compact() {
                            p[1] >= 70. && p[1] <= 116.
                        } else {
                            p[0] >= (frame.full[0] as f32 - if self.editor_held() { 632.0 } else { 316.0 }).max(0.0)
                                && p[1] >= 70.0
                                && p[1] <= frame.full[1] as f32 - if self.editor_held() { 104.0 } else { 8.0 }
                        }
                    });
                if !inside
                    && self.cursor.and_then(|p| frame.full_point(p)).is_some()
                    && !self.place_mode.as_ref().is_some_and(|m| m.palette)
                    && !self.recovery_open()
                {
                    if let Some(m) = &mut self.place_mode { m.editor_expanded = false; }
                    self.capture_look();
                }
            }
        } else {
            let m = self.place_mode.as_mut().unwrap();
            m.mouse_held = false;
            if let Some((control, old)) = m.press.take()
                && old == frame && hit == Some(control) {
                let result = match control {
                    Control::SetAnimation(mode) => self.editor_animation(mode),
                    Control::Scale(percent) => self.editor_scale(percent),
                    Control::EditorDetails | Control::EditorPrevious | Control::EditorNext => {
                        let m = self.place_mode.as_mut().unwrap();
                        match control {
                            Control::EditorDetails => { m.editor_expanded = !m.editor_expanded; m.editor_page = 0; }
                            Control::EditorPrevious => m.editor_page = m.editor_page.saturating_sub(1),
                            Control::EditorNext => m.editor_page = m.editor_page.saturating_add(1),
                            _ => unreachable!(),
                        }
                        m.cancel_pointer();
                        Ok(())
                    }
                    Control::InspectRecovery | Control::SelectRecovery | Control::RecoveryPrevious
                    | Control::RecoveryNext | Control::RecoveryReview | Control::RecoveryConfirm
                    | Control::RecoveryClose => self.recovery_control(control),
                    Control::Choice(i) => self.place_choose(i),
                    _ => self.place_action(control.key()),
                };
                if let Err(e) = result {
                    if self.recovery_open() { self.recovery_refusal(&e); }
                    else { self.place_refusal(&e); }
                }
            }
        }
    }
    pub(super) fn place_ui(&mut self) -> Vec<hair::assets::Vertex> {
        if self.place_mode.is_none() || !self.place_context() {
            return vec![];
        }
        if self.recovery_open() { return self.recovery_ui(); }
        if self.place_mode.as_ref().is_some_and(|m| m.decoration_edit.is_some()) {
            return self.decoration_ui();
        }
        if self.placement_action.is_some() { return self.editor_inventory_ui(); }
        let mut out = vec![];
        if self.place_mode.as_ref().unwrap().edit.is_some() && self.place_mode.as_ref().unwrap().positioning {
            match self.place_preview() {
                Ok(mesh) => {
                    let mut host = self.themes[self.theme_index].host.clone();
                    let e = self.place_mode.as_ref().unwrap().edit.as_ref().unwrap();
                    host.file_placements.remove(&e.anchor);
                    host.anchors.retain(|a| a.id != e.anchor);
                    let color =
                        if hair::placement::surface::clearance(&host, &mesh, e.overlap).is_ok() {
                            [0.1, 0.9, 0.5]
                        } else {
                            [1.0, 0.2, 0.15]
                        };
                    // Clearance above used the stationary authoritative preview.
                    let mesh = if e.properties.animated {
                        self.place_preview_at(self.started.elapsed().as_secs_f64())
                            .unwrap_or(mesh)
                    } else {
                        mesh
                    };
                    let mesh: Vec<_> = mesh
                        .into_iter()
                        .map(|v| hair::assets::Vertex {
                            color,
                            texture: 0,
                            ..v
                        })
                        .collect();
                    out = hair::collection::Effect::new(&mesh, &self.pose, self.sidebar_frame())
                        .map(|e| e.paint(0.0, false))
                        .unwrap_or_default();
                }
                Err(err) => {
                    self.local_status = format!("Place Mode | {err}; cancel or reselect");
                }
            }
        }
        let frame = self.inventory_frame();
        let e = self.place_mode.as_ref().unwrap().edit.as_ref();
        if self.place_mode.as_ref().unwrap().palette {
            let lines = self.palette_lines();
            let controls = self.palette_controls();
            let labels: Vec<_> = controls.iter().map(|(_, r, s)| (*r, s.as_str())).collect();
            out.extend(hair::placement::toolbox::paint(
                frame.full, &lines, &labels, None,
            ));
            return out;
        }
        let selected = e
            .map(|e| hair::folder_scene::display(e.target.name()))
            .unwrap_or_else(|| {
                self.place_decoration_selection().map_or_else(
                    || "X: select aimed file".into(),
                    |e| format!("Decoration: {} (not placed)", e.name),
                )
            });
        let status = if self.input.captured {
            "LOOK CAPTURED | TAB TOOLBOX"
        } else {
            "MOUSE FREE | CLICK WORLD: LOOK"
        };
        let detail = e
            .map(|e| {
                format!(
                    "Yaw {:.2} | overlaps {:?}{}",
                    e.yaw,
                    e.overlap,
                    if e.pending.is_some() { " | REVIEW" } else { "" }
                )
            })
            .unwrap_or_else(|| "Layout-only | I source / inspect".into());
        let lines = [
            format!("PLACE MODE | {}", self.editor_phase()),
            status.into(),
            selected,
            detail,
            "Tab mouse free | Esc back/cancel | I source".into(),
        ];
        let controls: Vec<_> = self
            .toolbox_controls()
            .into_iter()
            .map(|(_, r, s)| (r, s))
            .collect();
        out.extend(self.editor_toolbox(&lines, &controls));
        out
    }
}
