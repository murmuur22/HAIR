//! Temporary, write-free floor-placement interaction. Publication authority stays
//! in Inventory/placement_store; preview never owns a pathname or writes metadata.
use super::*;
pub(super) struct Action {
    pub item: u64,
    pub destination: [u64; 2],
    pub yaw: f32,
    pub move_item: bool,
    pub overlap: hair::placement::Overlap,
    pub properties: hair::placement::assembly::Properties,
    pub pending: Option<hair::placement_store::Position>,
    pub notice: Option<String>,
}
const SIZE: [f32; 3] = [0.8, 0.85, 0.28];
impl App {
    pub(super) fn begin_placement(&mut self) -> Result<()> {
        if self.storage.cache_blocked { return Err("Persistent Documents unavailable; explicitly reopen before placement".into()); }
        if self.inventory.is_persistent() {
            let selected = self.storage.selected.ok_or("Select a current cache identity first")?;
            if !self.inventory.items().get(self.inventory_selection).is_some_and(|i| i.id == selected) {
                return Err("Selection changed; select the cache item again".into());
            }
            if !self.inventory.is_busy() && !self.inventory.refresh_selected(selected)? {
                self.storage.selected = None;
                self.inventory_selection = usize::MAX;
                return Err("Selected identity disappeared/replaced; select again before placement".into());
            }
            if self.inventory.preview(selected).is_none() || self.inventory.is_busy() {
                self.request_document_preview(selected)?;
                return Err("Selected preview loading/cancelling; wait, then press P again. No placement authority entered".into());
            }
        }
        self.check_room_destination(&self.local.as_ref().ok_or("Choose a local directory before placement")?.explorer)?;
        let item = self
            .inventory
            .items()
            .get(self.inventory_selection)
            .ok_or("Select an inventory item first")?
            .id;
        let destination = self
            .local
            .as_ref()
            .ok_or("Choose a local directory before placement")?
            .explorer
            .map_identity()?;
        self.release_navigation();
        self.inventory_open = false;
        let mut mode = place_mode_native::Mode::default();
        mode.inventory_return = true;
        self.place_mode = Some(mode);
        self.placement_action = Some(Action {
            item,
            destination,
            yaw: 0.0,
            move_item: false,
            overlap: hair::placement::Overlap::Prevent,
            properties: Default::default(),
            pending: None,
            notice: None,
        });
        let name = self.inventory.items().iter().find(|i| i.id == item).map(|i| hair::folder_scene::display(&i.name)).unwrap_or_default();
        self.local_status = format!("Place Mode | Editing {name} | {}; arrows rotate; Enter positions; Escape returns to File inventory; no writes", self.document_placement_label(false));
        Ok(())
    }
    fn placement_target(&self) -> Result<hair::placement::surface::Contact> {
        let action = self
            .placement_action
            .as_ref()
            .ok_or("No temporary placement action")?;
        let local = self.local.as_ref().ok_or("Placement directory closed")?;
        if local.explorer.map_identity()? != action.destination {
            return Err("Placement directory changed".into());
        }
        self.surface_target(None)
    }
    pub(super) fn surface_target(
        &self,
        exclude: Option<&str>,
    ) -> Result<hair::placement::surface::Contact> {
        let theme = &self.themes[self.theme_index];
        let mut meshes = vec![theme.host.vertices()];
        let mut count = meshes[0].len();
        if let Some(view) = &self.world.folder_view {
            for (id, bound) in &view.bindings {
                if bound.entry.kind != hair::local_explorer::Kind::File {
                    continue;
                }
                if exclude == Some(id.as_str()) {
                    continue;
                }
                let item = if exclude == Some(id.as_str()) {
                    vec![]
                } else {
                    theme.file_item_shape(id, &bound.entry.name)
                };
                for mesh in [theme.collection_shape(id), item] {
                    count = count
                        .checked_add(mesh.len())
                        .ok_or("Surface work overflow")?;
                    if count > hair::placement::surface::MAX_VERTICES {
                        return Err("Surface work budget reached".into());
                    }
                    meshes.push(mesh);
                }
            }
        }
        let refs = meshes.iter().map(Vec::as_slice).collect::<Vec<_>>();
        hair::placement::surface::target(&self.pose, &refs)?
            .ok_or("Look down or aim at a surface within reach".into())
    }
    pub(super) fn placement_preview(&self) -> Result<Vec<hair::assets::Vertex>> {
        self.placement_preview_at(0.)
    }
    pub(super) fn placement_preview_at(&self, seconds: f64) -> Result<Vec<hair::assets::Vertex>> {
        self.check_room_destination(&self.local.as_ref().ok_or("Placement directory closed")?.explorer)?;
        if let Some(position) = self
            .placement_action
            .as_ref()
            .and_then(|a| a.pending.as_ref())
        {
            let action = self.placement_action.as_ref().unwrap();
            if self
                .local
                .as_ref()
                .ok_or("Placement directory closed")?
                .explorer
                .map_identity()?
                != action.destination
            {
                return Err("Placement directory changed".into());
            }
            return self.themes[self.theme_index].assembly_preview_at(
                position,
                &self
                    .inventory
                    .preview(action.item)
                    .ok_or("Inventory item no longer available")?
                    .model
                    .vertices,
                seconds,
            );
        }
        let at = self.placement_target()?;
        let action = self
            .placement_action
            .as_ref()
            .ok_or("No placement action")?;
        let preview = self
            .inventory
            .preview(action.item)
            .ok_or("Inventory item no longer available")?;
        self.themes[self.theme_index].assembly_preview_at(
            &hair::placement_store::Position {
                at: [at.at[0], at.at[2]],
                yaw: action.yaw,
                size: action.properties.scaled_size(SIZE)?,
                allow_overlap: action.overlap == hair::placement::Overlap::Allow,
                surface: Some(at),
                properties: action.properties,
            },
            &preview.model.vertices,
            seconds,
        )
    }
    pub(super) fn placement_ui(&mut self) -> Vec<hair::assets::Vertex> {
        let Some(action) = self.placement_action.as_ref() else {
            return vec![];
        };
        let directory_valid = self
            .local
            .as_ref()
            .and_then(|local| local.explorer.map_identity().ok())
            == Some(action.destination);
        let item_valid = self
            .inventory
            .items()
            .iter()
            .any(|item| item.id == action.item)
            && self.inventory.preview(action.item).is_some();
        if !directory_valid || !item_valid {
            self.release_navigation();
            self.local_status =
                "Placement cancelled: directory or inventory authority changed; copy retained"
                    .into();
            return vec![];
        }
        if self.place_mode.as_ref().is_some_and(|m| !m.positioning) { return vec![]; }
        let geometry = match self.placement_preview() {
            Ok(v) => v,
            Err(e) => {
                return self.placement_hud(&e);
            }
        };
        let action = self.placement_action.as_ref().expect("active action");
        let clearance = hair::placement::surface::clearance(
            &self.themes[self.theme_index].host,
            &geometry,
            action.overlap,
        );
        let color = if clearance.is_ok() {
            [0.1, 0.9, 0.5]
        } else {
            [1.0, 0.2, 0.15]
        };
        let reason = clearance.err().unwrap_or_else(|| "Surface clear".into());
        // Never feed the bobbed presentation back into clearance or contact.
        let geometry = if action.properties.animated {
            self.placement_preview_at(self.started.elapsed().as_secs_f64())
                .unwrap_or(geometry)
        } else {
            geometry
        };
        let mesh: Vec<_> = geometry
            .into_iter()
            .map(|v| hair::assets::Vertex {
                color,
                texture: 0,
                ..v
            })
            .collect();
        let mut overlay = hair::collection::Effect::new(&mesh, &self.pose, self.sidebar_frame())
            .map(|effect| effect.paint(0.0, false))
            .unwrap_or_default();
        overlay.extend(self.placement_hud(&reason));
        overlay
    }
    fn placement_hud(&mut self, reason: &str) -> Vec<hair::assets::Vertex> {
        let Some(action) = self.placement_action.as_ref() else {
            return vec![];
        };
        let mode = self.document_placement_label(action.move_item);
        let overlaps = if action.overlap == hair::placement::Overlap::Allow {
            "ON"
        } else {
            "OFF"
        };
        let confirming = action.pending.is_some();
        let title = format!(
            "{}{mode} | Overlaps {overlaps} (C)\n{}\n{}\nJ animation {} | K label {}",
            if confirming { "Confirm " } else { "Preview " },
            action.properties.base_label(),
            action.properties.collision_label(),
            if action.properties.animated {
                "ON"
            } else {
                "OFF"
            },
            if action.properties.label { "ON" } else { "OFF" }
        );
        let detail = if confirming {
            let name = self
                .inventory
                .items()
                .iter()
                .find(|i| i.id == action.item)
                .map(|i| hair::folder_scene::display(&i.name))
                .unwrap_or_default();
            let destination = self
                .local
                .as_ref()
                .map(|l| hair::folder_scene::display(l.explorer.current().folder.as_os_str()))
                .unwrap_or_default();
            format!("{name} -> {destination}")
        } else {
            reason.to_owned()
        };
        let detail = action.notice.clone().unwrap_or(detail);
        self.local_status = format!(
            "{title} | {detail} | {}",
            if confirming {
                "Enter confirms; Escape cancels; no writes yet"
            } else {
                "Enter reviews; no writes yet"
            }
        );
        hair::placement::hud(self.sidebar_frame().full, &title, &detail, confirming)
    }
    fn placement_enter(&mut self) -> Result<()> {
        use hair::placement_store::{Outcome, Position};
        let action = self
            .placement_action
            .as_ref()
            .ok_or("No placement action")?;
        let local = self.local.as_ref().ok_or("Placement directory closed")?;
        if local.explorer.map_identity()? != action.destination {
            return Err("Placement directory changed; inventory retained".into());
        }
        self.check_room_destination(&local.explorer)?;
        let name = self
            .inventory
            .items()
            .iter()
            .find(|i| i.id == action.item)
            .ok_or("Inventory item missing")?
            .name
            .clone();
        if action.pending.is_none() {
            let contact = self.placement_target()?;
            let position = Position {
                at: [contact.at[0], contact.at[2]],
                yaw: action.yaw,
                size: action.properties.scaled_size(SIZE)?,
                allow_overlap: action.overlap == hair::placement::Overlap::Allow,
                surface: Some(contact),
                properties: action.properties,
            };
            // Use the same host-owned file + optional base envelope as final
            // publication clearance, independent of theme art and collision.
            self.inventory
                .preview(action.item)
                .ok_or("Inventory preview missing")?;
            let mesh = position.envelope_mesh()?;
            hair::placement::surface::clearance(
                &self.themes[self.theme_index].host,
                &mesh,
                action.overlap,
            )?;
            let layout = hair::placement_store::read_layout(&local.explorer)?;
            self.themes[self.theme_index].validate_layout_decorations(&layout)?;
            hair::placement_scene::validate_publication(
                &local.explorer,
                &layout,
                &name,
                &position,
                self.pose.position,
                local.page,
            )?;
            let mut action = self.placement_action.take().unwrap();
            action.pending = Some(position);
            action.notice = None;
            let mode = self.place_mode.take();
            self.release_navigation();
            self.place_mode = mode;
            self.placement_action = Some(action);
            self.menu_held.insert(KeyCode::Enter);
            self.placement_hud("Enter confirms the file and saved position; Escape cancels");
            return Ok(());
        }
        let position = action.pending.as_ref().unwrap().clone();
        let visitor = self.pose.position;
        let mut page = 0;
        let resources = self.resources_directory.as_ref().map_err(Clone::clone)?;
        let outcome = self.inventory.place_checked(
            hair::inventory::PlacementRequest {
                id: action.item,
                destination: &local.explorer,
                name: &name,
                move_item: action.move_item,
                position: &position,
                cancelled: &std::sync::atomic::AtomicBool::new(false),
            },
            |layout| {
                Self::check_resources_destination(&local.explorer, resources)?;
                self.themes[self.theme_index].validate_layout_decorations(layout)?;
                page = hair::placement_scene::validate_publication(
                    &local.explorer,
                    layout,
                    &name,
                    &position,
                    visitor,
                    local.page,
                )?;
                Ok(())
            },
        );
        match outcome {
            Outcome::Committed { synced } => {

                self.placement_action = None;
                self.release_navigation();
                let refresh = self.placement_refresh(page);
                self.local_status = match refresh {
                    Ok(()) => format!(
                        "Placed {} with saved 3D position{}",
                        hair::folder_scene::display(&name),
                        if synced {
                            ""
                        } else {
                            "; sync uncertain, inspect before retrying"
                        }
                    ),
                    Err(e) => format!(
                        "File and position committed; scene refresh failed ({e}); use F5, do not repeat placement"
                    ),
                };
            }
            Outcome::NotCommitted(e) => {
                if let Some(a) = self.placement_action.as_mut() {
                    a.pending = None;
                }
                return Err(format!("Not placed; inventory retained: {e}"));
            }
            Outcome::Ambiguous(e) => {
                self.release_navigation();
                self.local_status = format!(
                    "Placement result uncertain; inventory retained. Inspect directory before retrying: {e}"
                );
            }
        }
        if let Some(notice) = self.inventory.notice() { self.local_status.push_str(&format!("; {notice}")); }
        Ok(())
    }
    pub(super) fn placement_key(
        &mut self,
        code: KeyCode,
        down: bool,
        repeat: bool,
    ) -> Option<Result<bool>> {
        self.placement_action.as_ref()?;
        if !down {
            match code {
                KeyCode::KeyW => self.input.forward = false,
                KeyCode::KeyS => self.input.back = false,
                KeyCode::KeyA => self.input.left = false,
                KeyCode::KeyD => self.input.right = false,
                _ => {}
            }
            self.menu_held.remove(&code);
            self.release_latch(code);
            return Some(Ok(false));
        }
        let fresh = self.menu_held.insert(code);
        if !self.focused || repeat || !fresh {
            return Some(Ok(false));
        }
        if code == KeyCode::Enter {
            if let Err(e) = self.placement_enter() {
                if let Some(a) = self.placement_action.as_mut() {
                    a.notice = Some(e.clone());
                }
                self.local_status = e;
            }
            return Some(Ok(false));
        }
        if self
            .placement_action
            .as_ref()
            .is_some_and(|a| a.pending.is_some())
            && !matches!(code, KeyCode::Escape | KeyCode::KeyB)
        {
            return Some(Ok(false));
        }
        if let Some(a) = self.placement_action.as_mut() {
            a.notice = None;
        }
        match code {
            KeyCode::Escape | KeyCode::KeyB => {
                self.placement_action = None;
                self.release_navigation();
                self.local_status = "Placement cancelled; inventory copy retained".into();
            }
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                let a = self.placement_action.as_mut().unwrap();
                a.yaw = (a.yaw
                    + if code == KeyCode::ArrowRight {
                        0.2
                    } else {
                        -0.2
                    })
                .rem_euclid(std::f32::consts::TAU);
            }
            KeyCode::KeyC => {
                let a = self.placement_action.as_mut().unwrap();
                a.overlap = if a.overlap == hair::placement::Overlap::Prevent {
                    hair::placement::Overlap::Allow
                } else {
                    hair::placement::Overlap::Prevent
                };
            }
            KeyCode::KeyV => {
                let a = self.placement_action.as_mut().unwrap();
                a.move_item = !a.move_item;
            }
            KeyCode::KeyG => {
                let a = self.placement_action.as_mut().unwrap();
                a.properties.base = !a.properties.base;
            }
            KeyCode::KeyH => {
                let a = self.placement_action.as_mut().unwrap();
                if a.properties.base {
                    a.properties.base_collision = !a.properties.base_collision;
                }
            }
            KeyCode::KeyJ => {
                let a = self.placement_action.as_mut().unwrap();
                a.properties.set_mode(if a.properties.animated { hair::placement::animation::Mode::None } else { hair::placement::animation::Mode::Breathe });
            }
            KeyCode::KeyK => {
                let a = self.placement_action.as_mut().unwrap();
                a.properties.label = !a.properties.label;
            }
            KeyCode::KeyW => self.input.forward = self.input.captured,
            KeyCode::KeyS => self.input.back = self.input.captured,
            KeyCode::KeyA => self.input.left = self.input.captured,
            KeyCode::KeyD => self.input.right = self.input.captured,
            _ => {}
        }
        Some(Ok(false))
    }
}
