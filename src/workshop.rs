//! Workshop actions own the same Project used by preview, persistence and export.
pub use crate::authoring::Edit;
use crate::{Result, authoring::Project, theme::Theme};
use std::path::{Path, PathBuf};
#[path = "workshop_gpu.rs"]
mod gpu;
#[path = "workshop_ui.rs"]
mod ui;
pub use gpu::WorkshopGpu;
#[path = "workshop_camera.rs"]
mod camera;
pub use camera::InspectionCamera;

/// A UI request, not permission to bypass the core loader or exclusive writers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKind {
    WallTexture,
    FloorTexture,
    Import,
    Open,
    SaveNew,
    Export,
}

fn selection_error(kind: DialogKind, selected: Option<&Path>, error: &str) -> String {
    let (action, help) = match kind {
        DialogKind::WallTexture | DialogKind::FloorTexture => (
            "Importing surface PNG",
            "Choose a local static PNG, 1..1024 pixels per axis; last-good texture is retained on errors.",
        ),
        DialogKind::Import => ("Importing model", "Choose an existing supported .glb file."),
        DialogKind::Open => (
            "Opening editable project",
            "Choose project.toml inside an existing editable project folder.",
        ),
        DialogKind::SaveNew => (
            "Saving editable project",
            "Choose an existing parent and a NEW folder name; do not create the target first. Existing folders are never replaced.",
        ),
        DialogKind::Export => (
            "Exporting appearance pack",
            "Choose an existing parent and a separate NEW folder name; do not create the target first. Export does not save edits. Existing folders are never replaced.",
        ),
    };
    let destination = selected
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "No destination returned by the panel".into());
    format!(
        "{action} refused.\nSelected destination: {destination}\n{error}\n{help}\nLast good project and unsaved-change state retained."
    )
}

pub enum Command {
    Door,
    SelectModel(bool),
    NewMaterial,
    ClearSurface(&'static str),
    Import,
    Open,
    SaveNew,
    Export,
    Edit(Edit),
    Surface(crate::materials::Surface),
    Fit,
    Reset,
    RequestQuit,
    ConfirmDiscard,
    Cancel,
    AcknowledgeError,
}
struct Replacement {
    project: Project,
    preview: Theme,
    dirty: bool,
    paths: [String; 3],
}
enum Pending {
    Replace(Box<Replacement>),
    Quit,
}
pub struct Workshop {
    pub project: Option<Project>,
    pub preview: Option<Theme>,
    pub input_path: String,
    pub project_path: String,
    pub export_path: String,
    pub saved_project: Option<PathBuf>,
    pub exported_pack: Option<PathBuf>,
    pub dirty: bool,
    pub error: Option<String>,
    pub notice: String,
    pub protected: Vec<PathBuf>,
    pub revision: u64,
    pub should_close: bool,
    pub controls: std::collections::BTreeMap<&'static str, egui::Rect>,
    pub inspection: crate::Pose,
    pub camera: InspectionCamera,
    pub(crate) category: &'static str,
    keyboard_was_editing: bool,
    pending: Option<Pending>,
    dialog: Option<(DialogKind, bool)>,
}
impl Workshop {
    pub fn new(protected: Vec<PathBuf>) -> Self {
        Self {
            project: None,
            preview: None,
            input_path: String::new(),
            project_path: String::new(),
            export_path: String::new(),
            saved_project: None,
            exported_pack: None,
            dirty: false,
            error: None,
            notice: "Import a local GLB to begin.".into(),
            protected,
            revision: 0,
            should_close: false,
            pending: None,
            dialog: None,
            controls: Default::default(),
            camera: InspectionCamera::default(),
            category: "document",
            keyboard_was_editing: false,
            inspection: crate::Pose {
                position: [0.0, 3.5],
                yaw: 0.0,
                pitch: -0.2,
            },
        }
    }
    pub fn frame_camera(&mut self) {
        self.camera.frame();
        self.inspection = self.camera.pose;
    }
    pub fn request_dialog(&mut self, kind: DialogKind) {
        self.camera.release();
        if self.dialog.is_none() && self.pending.is_none() && !self.should_close {
            self.dialog = Some((kind, false));
        }
    }
    /// Claim once, on the native main thread (or an injected offscreen test).
    pub fn begin_dialog(&mut self) -> Option<DialogKind> {
        let (kind, active) = self.dialog.as_mut()?;
        if *active {
            return None;
        }
        *active = true;
        Some(*kind)
    }
    /// A cancelled panel changes nothing except its in-flight request bookkeeping.
    pub fn finish_dialog(&mut self, result: Result<Option<PathBuf>>) {
        let Some((kind, true)) = self.dialog else {
            return;
        };
        self.dialog = None;
        let selected = result.as_ref().ok().and_then(|p| p.clone());
        let old_paths = (
            self.input_path.clone(),
            self.project_path.clone(),
            self.export_path.clone(),
        );
        let result = (|| {
            let Some(path) = result? else {
                return Ok(());
            };
            let text = path
                .to_str()
                .ok_or("Selected path must be UTF-8; no lossy path conversion is performed")?;
            match kind {
                DialogKind::WallTexture | DialogKind::FloorTexture => {
                    let surface = if kind == DialogKind::WallTexture {
                        "wall"
                    } else {
                        "floor"
                    };
                    let mut blank = Project::blank();
                    let p = self.project.as_mut().unwrap_or(&mut blank);
                    let preview = p.import_surface(surface, &path)?;
                    if self.project.is_none() {
                        self.project = Some(blank);
                    }
                    self.preview = Some(preview);
                    self.revision += 1;
                    self.dirty = true;
                    self.error = None;
                    self.notice = format!(
                        "Imported {surface} PNG. Wall, floor and all model roles are retained together; unsaved changes."
                    );
                    Ok(())
                }
                DialogKind::Import => {
                    self.input_path = text.into();
                    self.apply(Command::Import)
                }
                DialogKind::Open => {
                    if path.file_name().and_then(|s| s.to_str()) != Some("project.toml") {
                        return Err("Choose the editable folder's project.toml".into());
                    }
                    self.project_path = text.into();
                    self.apply(Command::Open)
                }
                DialogKind::SaveNew => {
                    self.project_path = text.into();
                    self.apply(Command::SaveNew)
                }
                DialogKind::Export => {
                    self.export_path = text.into();
                    self.apply(Command::Export)
                }
            }
        })();
        if result.is_err() || self.pending.is_some() {
            (self.input_path, self.project_path, self.export_path) = old_paths;
        }
        if let Err(error) = result {
            self.error = Some(selection_error(kind, selected.as_deref(), &error));
        }
    }
    pub fn confirmation_pending(&self) -> bool {
        self.pending.is_some()
    }
    fn install(&mut self, project: Project, preview: Theme, dirty: bool) {
        self.frame_camera();
        if !dirty && let Some(parent) = project.source().parent() {
            self.protected.push(parent.to_path_buf());
        }
        self.saved_project = if dirty {
            None
        } else {
            project.source().parent().map(|p| p.join("project.toml"))
        };
        self.exported_pack = None;
        if !matches!(self.category, "wall" | "floor") {
            self.category = if project.door_active {
                "door-trim"
            } else if project.document_role {
                "document"
            } else {
                "decoration"
            };
        }
        self.project = Some(project);
        self.preview = Some(preview);
        self.dirty = dirty;
        self.revision += 1;
        self.error = None;
        self.notice = if dirty {
            "Imported; editable project has unsaved changes."
        } else {
            "Opened editable project; no unsaved changes."
        }
        .into();
    }
    pub fn dispatch(&mut self, command: Command) {
        self.camera.release();
        if self.dialog.is_some() {
            return;
        }
        if matches!(command, Command::Import) && matches!(self.category, "wall" | "floor") {
            let kind = if self.category == "wall" {
                DialogKind::WallTexture
            } else {
                DialogKind::FloorTexture
            };
            self.request_dialog(kind);
            if self.begin_dialog().is_some() {
                self.finish_dialog(Ok(Some(PathBuf::from(&self.input_path))));
            }
            return;
        }
        let selection = match &command {
            Command::SaveNew => Some((DialogKind::SaveNew, PathBuf::from(&self.project_path))),
            Command::Export => Some((DialogKind::Export, PathBuf::from(&self.export_path))),
            _ => None,
        };
        if let Err(error) = self.apply(command) {
            self.error = Some(if let Some((kind, path)) = selection {
                selection_error(kind, Some(&path), &error)
            } else {
                format!("{error}. Last good project retained; correct the input and retry.")
            });
        }
    }
    fn apply(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Door | Command::SelectModel(_) => {
                let p = self.project.as_mut().ok_or("Start a pack first")?;
                match command {
                    Command::Door => p.select_door()?,
                    Command::SelectModel(document) => p.select_model(document)?,
                    _ => unreachable!(),
                }
                self.preview = Some(p.preview()?);
                self.revision += 1;
                self.dirty = true;
                self.error = None;
                self.notice="Selected independent role. Door trim replaces only three bounded jamb/lintel slots; host frame and passage stay visible. Import GLB for this role.".into();
            }
            Command::NewMaterial => {
                let project = Project::blank();
                let preview = project.preview()?;
                if self.dirty {
                    self.pending = Some(Pending::Replace(Box::new(Replacement {
                        project,
                        preview,
                        dirty: true,
                        paths: [String::new(), String::new(), String::new()],
                    })));
                } else {
                    self.install(project, preview, true);
                }
            }
            Command::ClearSurface(surface) => {
                self.preview = Some(
                    self.project
                        .as_mut()
                        .ok_or("Start a pack first")?
                        .clear_surface_texture(surface)?,
                );
                self.revision += 1;
                self.dirty = true;
                self.error = None;
                self.notice = format!(
                    "Cleared {surface} texture; tint and meters per tile retained. Unsaved changes."
                );
            }
            Command::Import | Command::Open => {
                let imported = matches!(command, Command::Import);
                let mut project = if imported {
                    Project::import(Path::new(&self.input_path))?
                } else {
                    Project::open(Path::new(&self.project_path))?
                };
                if imported {
                    if let Some(previous) = &self.project {
                        project.assign_import(previous);
                    } else {
                        let mut selected = Project::blank();
                        match self.category {
                            "door-trim" => selected.select_door()?,
                            "decoration" => selected.select_model(false)?,
                            _ => {}
                        }
                        project.assign_import(&selected);
                    }
                }
                let preview = project.preview()?;
                if self.dirty {
                    self.pending = Some(Pending::Replace(Box::new(Replacement {
                        project,
                        preview,
                        dirty: imported,
                        paths: [
                            self.input_path.clone(),
                            self.project_path.clone(),
                            self.export_path.clone(),
                        ],
                    })));
                } else {
                    self.install(project, preview, imported);
                }
            }
            Command::Edit(edit) => {
                if self
                    .project
                    .as_ref()
                    .is_some_and(|p| p.document_role != edit.document_role)
                {
                    self.frame_camera();
                }
                self.preview = Some(self.project.as_mut().ok_or("Import first")?.update(edit)?);
                self.revision += 1;
                self.dirty = true;
                self.error = None;
                self.notice =
                    "Appearance validated inside fixed host envelopes; anchors and navigation unchanged. Unsaved changes."
                        .into();
            }
            Command::Surface(edit) => {
                self.preview = Some(
                    self.project
                        .as_mut()
                        .ok_or("Import first")?
                        .set_surface(edit)?,
                );
                self.revision += 1;
                self.dirty = true;
                self.error = None;
                self.notice="Whole-pack surface appearance updated; geometry, openings and model roles unchanged. Unsaved changes.".into();
            }
            Command::Fit | Command::Reset => {
                let p = self.project.as_ref().ok_or("Import first")?;
                let mut edit = p.edit();
                let extent = (0..3)
                    .map(|i| p.model.max[i] - p.model.min[i])
                    .fold(0.0_f32, f32::max);
                edit.scale = (1.6 / extent).clamp(0.1, 4.0);
                if matches!(command, Command::Reset) {
                    edit.at = [0.0; 3];
                    edit.yaw_degrees = 0.0;
                    edit.solid = false;
                }
                self.apply(Command::Edit(edit))?;
            }
            Command::SaveNew => {
                self.frame_camera();
                let path = self
                    .project
                    .as_ref()
                    .ok_or("Import first")?
                    .save_new(Path::new(&self.project_path), &self.protected)?;
                self.dirty = false;
                self.error = None;
                self.protected
                    .push(path.parent().ok_or("Missing saved parent")?.to_path_buf());
                self.notice =
                    "Saved NEW editable project; further edits need another NEW copy.".into();
                self.saved_project = Some(path);
            }
            Command::Export => {
                let path = self
                    .project
                    .as_ref()
                    .ok_or("Import first")?
                    .export(Path::new(&self.export_path), &self.protected)?;
                self.error = None;
                self.notice = "Exported appearance pack. Export does not save edits; save a NEW editable copy to keep them.".into();
                self.exported_pack = Some(path);
            }
            Command::RequestQuit => {
                if self.dirty {
                    self.pending = Some(Pending::Quit);
                } else {
                    self.should_close = true;
                }
            }
            Command::ConfirmDiscard => match self.pending.take() {
                Some(Pending::Replace(next)) => {
                    [self.input_path, self.project_path, self.export_path] = next.paths;
                    self.install(next.project, next.preview, next.dirty);
                }
                Some(Pending::Quit) => self.should_close = true,
                None => {}
            },
            Command::Cancel => self.pending = None,
            Command::AcknowledgeError => self.error = None,
        }
        Ok(())
    }
}
