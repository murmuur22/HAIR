//! Private visitor preference schema; separate from host/layout and owner files.
use super::*;
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u32,
    ui_scale: u16,
    tree_visible: bool,
    fog: hair::fog::Fog,
    #[serde(default)]
    storage: storage_native::Preferences,
}
impl App {
    pub(super) fn restore_preferences_at(&mut self, parent: &Path) -> Result<()> {
        self.storage.preferences_blocked = true;
        let store = hair::acquisition::PreferenceFile::open(parent)?;
        let bytes = store.read()?;
        if let Some(bytes) = bytes {
            let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
            let saved: Saved =
                toml::from_str(text).map_err(|e| format!("Invalid Player Settings: {e}"))?;
            if saved.version != 1 {
                return Err("Unsupported Player Settings version; use a compatible build or separate preferences location. Existing file preserved.".into());
            }
            let fog = saved.fog.validated()?;
            saved.storage.policy()?;
            // Validate everything before changing any runtime preference.
            self.fog = fog;
            self.ui_scale = saved.ui_scale.clamp(100, 200);
            self.world.tree_collapsed = !saved.tree_visible;
            self.storage.preferences = saved.storage;
            if let Some(r) = &mut self.renderer {
                r.ui_scale = self.ui_scale;
                r.set_fog(self.fog);
            }
        }
        self.preferences = Some(store);
        self.storage.preferences_blocked = false;
        Ok(())
    }
    pub(super) fn restore_native_preferences(&mut self) {
        self.storage.required = true;
        // Only ordinary native launch opts in. Tests/headless/window-smoke never
        // read or write real user preferences; tests inject an owned fixture root.
        let result = std::env::var_os("HOME")
            .ok_or("HOME unavailable".into())
            .and_then(|home| {
                let parent = PathBuf::from(home).join("Library/Application Support");
                let parent = parent.canonicalize().map_err(|e| e.to_string())?;
                self.restore_preferences_at(&parent)?;
                let path = if let Some(path) = self.storage.preferences.path() { path } else {
                    use std::os::unix::fs::DirBuilderExt;
                    let path = parent.join("HAIR-Documents-v1");
                    match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                        Ok(()) => { std::fs::File::open(&parent).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?; }
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(e) => return Err(e.to_string()),
                    }
                    path
                };
                self.restore_documents_at(&path)
            });
        if let Err(e) = result {
            self.local_status = format!("Player Settings / Documents unavailable: {e}. No session-only fallback collection; open Storage settings for details.");
        }
    }
    pub(super) fn player_preferences_changed(&mut self) {
        self.sidebar_press = None;
        self.sidebar_wheel = 0.0;
        self.inventory_drag.cancel();
        if let Some(r) = &mut self.renderer {
            r.set_fog(self.fog);
            r.ui_scale = self.ui_scale;
        }
        if self.storage.preferences_blocked || self.storage.preferences_unknown {
            self.local_status = "Player Settings active for session only; preferences blocked/UNKNOWN, file preserved".into();
            return;
        }
        let Some(store) = &self.preferences else {
            return;
        };
        let saved = Saved {
            version: 1,
            ui_scale: self.ui_scale,
            tree_visible: !self.world.tree_collapsed,
            fog: self.fog,
            storage: self.storage.preferences.clone(),
        };
        match toml::to_string(&saved) {
            Ok(text) => {
                let outcome = store.write_outcome(text.as_bytes());
                match self.reconcile_storage_preference_write(outcome) {
                    Ok(Some(warning)) => { self.storage.preference_notice = warning.clone(); self.local_status = warning; }
                    Ok(None) => {}
                    Err(e) => { self.local_status = format!("Player Settings active for session; {e}"); }
                }
            }
            Err(e) => { self.local_status = format!("Player Settings active for session; serialization failed: {e}"); }
        }
    }
    pub(super) fn preflight_storage_preferences(&self) -> Result<()> {
        if self.storage.preferences_blocked || self.storage.preferences_unknown {
            return Err("Preferences unavailable, unsupported or UNKNOWN; preserve file and reopen with compatible settings before changing Storage".into());
        }
        Ok(())
    }
    pub(super) fn reconcile_storage_preference_write(&mut self, outcome: hair::acquisition::PreferenceWrite) -> Result<Option<String>> {
        match outcome {
            hair::acquisition::PreferenceWrite::NotCommitted(e) => Err(format!("Preference NOT COMMITTED; old settings retained: {e}")),
            hair::acquisition::PreferenceWrite::Committed { sync_warning } => Ok(sync_warning.map(|e| format!("Preferences COMMITTED; durability sync warning: {e}"))),
            hair::acquisition::PreferenceWrite::Unknown(e) => {
                self.storage.preferences_unknown = true;
                self.storage.preferences_blocked = true;
                self.storage.cache_blocked = true;
                self.storage.removal = None;
                self.placement_action = None;
                Err(format!("Preference activation UNKNOWN; storage blocked. Preserve both caches and settings; safely restart and inspect/reopen before retrying: {e}"))
            }
        }
    }
    pub(super) fn save_storage_preferences(&mut self, storage: &storage_native::Preferences) -> Result<Option<String>> {
        self.preflight_storage_preferences()?;
        let Some(store) = &self.preferences else { return Ok(None); };
        let saved = Saved { version: 1, ui_scale: self.ui_scale, tree_visible: !self.world.tree_collapsed, fog: self.fog, storage: storage.clone() };
        let text = toml::to_string(&saved).map_err(|e| e.to_string())?;
        let outcome = store.write_outcome(text.as_bytes());
        self.reconcile_storage_preference_write(outcome)
    }
}
