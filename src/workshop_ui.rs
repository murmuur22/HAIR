use super::{Command, DialogKind, Workshop};

impl Workshop {
    fn button(&mut self, ui: &mut egui::Ui, id: &'static str, label: &str) -> bool {
        let response = ui
            .push_id(id, |ui| {
                ui.add(egui::Button::new(label).wrap_mode(egui::TextWrapMode::Extend))
            })
            .inner;
        self.controls.insert(id, response.rect);
        response.clicked()
    }
    pub fn ui(&mut self, root: &mut egui::Ui, preview: Option<egui::TextureId>) {
        let ctx = root.ctx().clone();
        self.controls.clear();
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            if self.camera.captured || self.keyboard_was_editing || ctx.egui_wants_keyboard_input()
            {
                self.camera.release();
            } else if self.confirmation_pending() {
                self.dispatch(Command::Cancel);
            } else {
                self.dispatch(Command::RequestQuit);
            }
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(path) = dropped {
            self.camera.release();
            self.input_path = path.display().to_string();
            self.dispatch(Command::Import);
        }
        egui::Panel::top("workshop-title").show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("HAIR Workshop");
                ui.label(format!("{}  •  local authoring", env!("CARGO_PKG_VERSION")));
                ui.label(if self.dirty {
                    "Unsaved changes"
                } else {
                    "No unsaved changes"
                });
                if self.button(ui, "quit", "Quit") {
                    self.dispatch(Command::RequestQuit);
                }
            });
        });
        egui::Panel::bottom("workshop-status").show(root, |ui| {
            if self.error.is_some() && self.button(ui, "ack-error", "Dismiss error") { self.dispatch(Command::AcknowledgeError); }
            egui::ScrollArea::vertical().id_salt("global-status-scroll").max_height(100.0).show(ui, |ui| {
            if let Some(error) = self.error.clone() {
                egui::ScrollArea::vertical().id_salt("error-scroll").max_height(180.0).show(ui, |ui| {
                    let display=if error.contains("Unsafe door-trim") {
                        if self.project.as_ref().is_some_and(|p|!p.model.vertices.is_empty()) {"Door trim must keep the doorway clear. Use the supplied frame template; your last working model is unchanged.".to_owned()}
                        else {"Door trim must keep the doorway clear. Use the supplied frame template; no trim model was imported and the last working preview is unchanged.".to_owned()}
                    } else {error.clone()};
                    let r = ui.add(egui::Label::new(egui::RichText::new(display).color(egui::Color32::from_rgb(255, 170, 120))).wrap());
                    if error.contains("Unsafe door-trim") {egui::CollapsingHeader::new("Technical diagnostics").show(ui,|ui| {ui.label(&error);});}
                    self.controls.insert("error-message", r.rect);
                });
            } else {
                ui.add(egui::Label::new(&self.notice).wrap());
            }
            for (label, path, empty) in [
                ("Editable project", &self.saved_project, "Not saved yet"),
                ("Last exported pack", &self.exported_pack, "Not exported yet"),
            ] {
                let folder = path.as_ref().and_then(|p| p.parent()).and_then(|p| p.file_name()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| empty.into());
                let response = ui.add(egui::Label::new(format!("{label}: {folder}")).wrap());
                if let Some(path) = path { response.on_hover_text(path.display().to_string()); }
            }
            ui.small("Export does not save edits. Saved location and last export are separate; unsaved changes still need a NEW editable copy.");
            ui.small("Embedded PNG base color + UV0 only. Specular/IOR/full PBR, animation and VR are not supported. Private exports: check rights before sharing.");
            });
        });
        let panel_width = 380.0_f32.min((root.available_width() - 20.0).max(1.0));
        egui::Panel::left("workshop-controls").exact_size(panel_width).show(root, |ui| {
            let authoring=egui::ScrollArea::vertical().id_salt("authoring-scroll").show(ui, |ui| {
                ui.heading("Project");
                ui.horizontal_wrapped(|ui| {
                    if self.button(ui,"new-material","New pack") { self.dispatch(Command::NewMaterial); }
                    if self.button(ui,"open","Open project…") { self.request_dialog(DialogKind::Open); }
                });
                ui.horizontal_wrapped(|ui| {
                    if self.button(ui,"save-new","Save NEW copy…") { self.request_dialog(DialogKind::SaveNew); }
                    if self.button(ui,"export","Export pack…") { self.request_dialog(DialogKind::Export); }
                });
                ui.small("Save and Export use separate NEW folders under an existing parent. Export does not save edits.");
                ui.separator();
                let columns=if ui.available_width()<330.0 {2}else{3};
                for row in [("document","Document object"),("decoration","Decoration"),("door-trim","Door trim"),("wall","Wall"),("floor","Floor")].chunks(columns) {
                ui.horizontal(|ui| {
                    for &(id,label) in row {
                        let r=ui.push_id(id,|ui|ui.add(egui::Button::new(label).selected(self.category==id).wrap_mode(egui::TextWrapMode::Extend))).inner;
                        self.controls.insert(id,r.rect);
                        if r.clicked() && self.dialog.is_none() && !self.confirmation_pending() {
                            self.camera.release();
                            self.category=id;
                            if let Some(p)=self.project.as_mut() {
                                let result=match id { "document"=>p.select_model(true),"decoration"=>p.select_model(false),"door-trim"=>p.select_door(),_=>Ok(()) };
                                if let Err(e)=result {self.error=Some(e);}
                            }
                        }
                    }
                });
                }
                let label=match self.category {"document"=>"Document object","decoration"=>"Decoration","door-trim"=>"Door trim","wall"=>"Wall",_=>"Floor"};
                let r=ui.heading(format!("Editing: {label}")); self.controls.insert("editor-heading",r.rect);
                if matches!(self.category,"wall"|"floor") {
                    let surface=self.category;
                    let (id,kind)=if surface=="wall" {("wall-png",DialogKind::WallTexture)}else{("floor-png",DialogKind::FloorTexture)};
                    if self.button(ui,id,&format!("Import {label} PNG…")) {
                        self.request_dialog(kind);
                    }
                    ui.add_enabled_ui(self.project.is_some(),|ui| {
                        if self.button(ui,if surface=="wall" {"wall-clear"}else{"floor-clear"},"Clear texture (keep tint / tiling)") { self.dispatch(Command::ClearSurface(surface)); }
                        let mut edit=self.project.as_ref().and_then(|p|p.surfaces().into_iter().find(|s|s.surface==surface)).unwrap_or(crate::materials::Surface{surface:surface.into(),file:None,tint:[1.0;3],meters_per_tile:1.0});
                        ui.small(edit.file.as_deref().unwrap_or("No texture — tint only"));
                        let old=edit.clone();
                        for (i,label) in ["Linear red","Linear green","Linear blue"].iter().enumerate() {
                            let r=ui.push_id((surface,label),|ui|ui.add(egui::Slider::new(&mut edit.tint[i],0.0..=1.0).text(*label))).inner;
                            self.controls.insert(if surface=="wall" {["wall-red","wall-green","wall-blue"][i]}else{["floor-red","floor-green","floor-blue"][i]},r.rect);
                        }
                        let r=ui.push_id(surface,|ui|ui.add(egui::Slider::new(&mut edit.meters_per_tile,0.1..=16.0).text("Meters per tile"))).inner;
                        self.controls.insert(if surface=="wall" {"wall-tile"}else{"floor-tile"},r.rect);
                        if edit!=old {self.dispatch(Command::Surface(edit));}
                    });
                    ui.small("World meters per tile; PNG changes appearance only, never host geometry.");
                } else {
                    if self.button(ui,"import",&format!("Import {label} GLB…")) {
                        self.request_dialog(DialogKind::Import);
                    }
                    if let Some(p)=&self.project {
                        ui.label(format!("Loaded: {}",p.source().file_name().unwrap_or_default().to_string_lossy()));
                    }
                    let door=self.category=="door-trim";
                    if door {ui.label("Door trim fits a fixed host opening. Whole-frame resize, scale, yaw and pivot editing are unsupported for this limited trim role. Use the supplied frame template; the doorway must stay clear.");}
                    let edit=self.project.as_ref().map(|p|p.edit());
                    ui.add_enabled_ui(edit.is_some() && !door,|ui| {
                        let mut edit=edit.unwrap_or(crate::authoring::Project::blank().edit()); let old=edit;
                        let r=ui.add(egui::Slider::new(&mut edit.scale,0.1..=4.0).text("Uniform scale ×"));self.controls.insert("model-scale",r.rect);
                        let r=ui.add(egui::Slider::new(&mut edit.yaw_degrees,-180.0..=180.0).text("Asset orientation °"));self.controls.insert("model-yaw",r.rect);
                        for (i,axis) in ["Pivot X","Pivot Y","Pivot Z"].iter().enumerate() {let r=ui.add(egui::Slider::new(&mut edit.at[i],-3.0..=3.0).text(*axis));self.controls.insert(["model-pivot-x","model-pivot-y","model-pivot-z"][i],r.rect);}
                        if self.button(ui,"rotate-step","Rotate +15°") {edit.yaw_degrees=(edit.yaw_degrees+15.0).clamp(-180.0,180.0);}
                        if edit!=old {self.dispatch(Command::Edit(edit));}
                        ui.horizontal_wrapped(|ui| {
                            if self.button(ui,"fit","Resize object: 1.6 m") {self.dispatch(Command::Fit);}
                            if self.button(ui,"reset","Reset asset fit") {self.dispatch(Command::Reset);}
                        });
                    });
                    if door && let Some(e)=edit {ui.small(format!("Retained values (read-only): scale {}, yaw {}°, pivot {:?}",e.scale,e.yaw_degrees,e.at));}
                    if !door {ui.small("Asset-local edits only. Host envelopes may shrink objects; positions and collision stay fixed.");}
                }
                ui.separator();
                let advanced = egui::CollapsingHeader::new("Advanced full paths (optional)")
                    .default_open(false).show(ui, |ui| {
                        ui.label(if matches!(self.category,"wall"|"floor") {"PNG full path:"}else{"GLB full path:"});
                        let r = ui.add(egui::TextEdit::singleline(&mut self.input_path).id(egui::Id::unique("import-path")).desired_width(f32::INFINITY));
                        self.controls.insert("import-path", r.rect);
                        if self.button(ui, "import-path-action", "Import typed path") { self.dispatch(Command::Import); }
                        ui.label("Save: NEW folder path. Open: project.toml path.");
                        ui.add(egui::TextEdit::singleline(&mut self.project_path).id(egui::Id::unique("project-path")).desired_width(f32::INFINITY));
                        ui.horizontal(|ui| {
                            if self.button(ui, "save-path-action", "Save typed NEW folder") { self.dispatch(Command::SaveNew); }
                            if self.button(ui, "open-path-action", "Open typed path") { self.dispatch(Command::Open); }
                        });
                        ui.label("Separate NEW export folder path:");
                        ui.add(egui::TextEdit::singleline(&mut self.export_path).id(egui::Id::unique("export-path")).desired_width(f32::INFINITY));
                        if self.button(ui, "export-path-action", "Export typed NEW folder") { self.dispatch(Command::Export); }
                    });
                self.controls.insert("advanced-paths", advanced.header_response.rect);
            });
            self.controls.insert("authoring-viewport",authoring.inner_rect);
        });
        egui::CentralPanel::default().show(root, |ui| {
            ui.heading("Fixed host preview stage (not exported)");
            let mode = if self.camera.free_fly { "Free-fly: click preview to capture" } else { "Standard inspection (UI controls)" };
            if self.button(ui, "camera-mode", mode) {
                self.frame_camera();
                self.camera.free_fly = !self.camera.free_fly;
            }
            ui.label(if self.camera.captured { "Mouse captured — Escape releases (does not quit)" } else { "Mouse released — UI available" });
            ui.small("Free-fly: mouse look, WASD move, Q down / E up. 2.5 m/s; no collision. Escape releases; Frame / Reset view recovers stage. Camera is never saved/exported.");
            if self.button(ui, "frame-camera", "Frame / Reset view") { self.frame_camera(); }
            ui.add_enabled_ui(!self.camera.free_fly, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.inspection.position[0], -6.0..=6.0).text("Camera X"));
                ui.add(egui::Slider::new(&mut self.inspection.position[1], 1.0..=8.0).text("Distance m"));
            });
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.inspection.yaw, -1.0..=1.0).text("Look left/right rad"));
                ui.add(egui::Slider::new(&mut self.inspection.pitch, -1.0..=0.7).text("Look up/down rad"));
            });
            });
            if let Some(texture) = preview {
                let width = ui.available_width();
                let height = (width * 0.75).min(ui.available_height().max(1.0));
                let r = ui.image((texture, egui::vec2(width, height)));
                self.controls.insert("preview", r.rect);
            } else {
                ui.label("Import a model to display its supported materials here.");
            }
        });
        if self.confirmation_pending() {
            self.camera.release();
            egui::Modal::new(egui::Id::unique("discard-confirmation")).show(&ctx, |ui| {
                ui.heading("Discard unsaved changes?");
                ui.label("Cancel to save a NEW editable copy first. Original files are never overwritten.");
                if self.button(ui, "cancel-discard", "Cancel — keep editing") { self.dispatch(Command::Cancel); }
                if self.button(ui, "confirm-discard", "Discard changes and continue") { self.dispatch(Command::ConfirmDiscard); }
            });
        }
        self.keyboard_was_editing = ctx.text_edit_focused();
        // Same egui-winit event stream in native and synthetic renderer frames.
        let blocked = self.dialog.is_some()
            || self.confirmation_pending()
            || self.should_close
            || ctx.egui_wants_keyboard_input()
            || ctx.egui_is_using_pointer()
            || ctx.input(|i| {
                !i.focused || !i.raw.hovered_files.is_empty() || !i.raw.dropped_files.is_empty()
            });
        let over_preview = ctx
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|p| self.controls.get("preview").is_some_and(|r| r.contains(p)));
        if blocked || !self.camera.free_fly || !over_preview {
            self.camera.release();
        }
        if !blocked && !escape && over_preview && self.camera.free_fly {
            // Capture begins on a fresh press, never from held keys or focus return.
            let was_captured = self.camera.captured;
            if !was_captured
                && ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
            {
                self.camera.capture();
            }
            if was_captured {
                let events = ctx.input(|i| i.events.clone());
                for event in events {
                    match event {
                        egui::Event::Key {
                            key,
                            physical_key,
                            pressed,
                            repeat: false,
                            ..
                        } => self.camera.key(physical_key.unwrap_or(key), pressed),
                        egui::Event::MouseMoved(d) => self.camera.look(d.x, d.y),
                        _ => {}
                    }
                }
                self.camera.step(ctx.input(|i| i.unstable_dt));
            }
        }
    }
}
