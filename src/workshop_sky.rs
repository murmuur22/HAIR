//! Draft numbers are session-only; Project owns the sole authoritative SkySpec.
use super::{Command, DialogKind, Workshop};
use crate::{Result, sky::SkySpec, theme::Theme};
use std::path::Path;

pub(super) struct Snapshot {
    revision: u64,
    view_revision: u64,
    category: &'static str,
    kind: DialogKind,
}
impl Workshop {
    pub(super) fn load_sky_draft(&mut self) {
        let rgb = match self.project.as_ref().and_then(|p| p.sky()) {
            Some(SkySpec::Colour { rgb }) => *rgb,
            _ => [0.04, 0.06, 0.05],
        };
        self.sky_draft = rgb.map(|v| v.to_string());
    }
    pub(super) fn sky_colour_values(&self) -> Result<[f64; 3]> {
        let mut rgb = [0.; 3];
        for (i, value) in self.sky_draft.iter().enumerate() {
            rgb[i] = value.trim().parse::<f64>().ok()
                .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
                .ok_or_else(|| format!("Sky {}: enter a finite linear-light number in 0..1", ["red", "green", "blue"][i]))?;
        }
        Ok(rgb)
    }
    pub(super) fn sky_snapshot(&self, kind: DialogKind) -> Snapshot {
        Snapshot { revision: self.revision, view_revision: self.view_revision, category: self.category, kind }
    }
    pub(super) fn finish_sky_pick(&mut self, snapshot: Snapshot, path: &Path) -> Result<()> {
        if snapshot.kind != DialogKind::SkyPanorama || snapshot.category != "sky"
            || self.category != snapshot.category || self.revision != snapshot.revision
            || self.view_revision != snapshot.view_revision {
            return Err("Sky context changed while chooser was open; selected file was not applied".into());
        }
        let theme = self.project.as_mut().ok_or("Start a pack first")?.import_sky_panorama(path)?;
        self.sky_committed(theme, "Imported panorama; exact PNG source copy retained as sky.png");
        Ok(())
    }
    pub(super) fn sky_committed(&mut self, theme: Theme, action: &str) {
        self.preview = Some(theme);
        self.revision += 1;
        self.dirty = true;
        self.error = None;
        self.notice = format!("{action}. Appearance only; unsaved changes. Save NEW copy and Export NEW pack are separate.");
    }
    pub(super) fn sky_ui(&mut self, ui: &mut egui::Ui) {
        let status = match self.project.as_ref().and_then(|p| p.sky()) {
            None => "Current: Legacy — no explicit sky; no source copy.".to_owned(),
            Some(SkySpec::Colour { rgb }) => format!("Current: explicit linear-light colour RGB {rgb:?}; no PNG source copy."),
            Some(SkySpec::Panorama { file }) => format!("Current: panorama — retained source copy {file}. Original bytes, not a live file link."),
        };
        let r = ui.add(egui::Label::new(status).wrap()); self.controls.insert("sky-status", r.rect);
        ui.small("Draft only until Apply. RGB values are linear light, NOT sRGB picker bytes. Selecting Sky never creates a sky.");
        ui.add_enabled_ui(self.project.is_some(), |ui| {
            for (i, (id, label)) in [("sky-red", "Linear red [0, 1]"), ("sky-green", "Linear green [0, 1]"), ("sky-blue", "Linear blue [0, 1]")].into_iter().enumerate() {
                ui.label(label);
                let r = ui.add(egui::TextEdit::singleline(&mut self.sky_draft[i]).id(egui::Id::unique(id)).desired_width(f32::INFINITY));
                self.controls.insert(id, r.rect);
            }
            if self.button(ui, "sky-apply", "Apply colour") { self.dispatch(Command::ApplySkyColour); }
            if self.button(ui, "sky-png", "Import panorama PNG…") { self.request_dialog(DialogKind::SkyPanorama); }
            if self.button(ui, "sky-clear", "Clear to Legacy (remove explicit sky)") { self.dispatch(Command::ClearSky); }
        });
        if self.project.is_none() { ui.label("Start with New pack or Open project. Sky selection alone changes nothing."); }
        ui.small("Appearance only: no host, file-body, collision or layout changes. Retained preview has no source reads or decode on redraw. Save copies bytes; deleting the original after Save does not prevent reopen/export.");
        egui::CollapsingHeader::new("Sky input, versions, resources and provenance").show(ui, |ui| {
            ui.label("Static 8-bit PNG: gray/RGB/gray-alpha/RGBA; 8-bit indexed must be noninterlaced. No APNG, 16-bit or colour-profile conversion. Samples interpreted as sRGB. Up to 4,000,000 encoded bytes, 1..4096 per axis, 4,194,304 pixels. Any aspect ratio: centered Fill; alpha composites over default linear [0.04, 0.06, 0.05]. Output opaque, at most 1024×512. No independent crop/background/options controls, seam repair or 360° correctness guarantee.");
            ui.label("Explicit Sky requires editable v4 / appearance v3. Clear restores Legacy absence and existing version selection; no migration or predecessor downgrade promise. Exact selected bytes retained (≤4 MB), separate from model/material/Pin budgets. Adapter: 52 MiB logical work admission, ≤2 MiB decoded output; old/new closures can coexist per edit. Not a total RSS/GPU guarantee.");
            ui.label("Selected-source no-follow checks remain authoritative: symlink ancestors/leaves, traversal and special files refuse. Save NEW / Export NEW never replace existing/protected targets. Reopened provenance names the saved closure, not a guaranteed original machine path. Check source rights before sharing.");
        });
    }
}
