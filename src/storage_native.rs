//! Documents/settings adapter. No recovery or damaged-deletion capability is retained by UI.
use super::*;
use hair::{collection_store::{Failure, PayloadStatus, Policy}, presentation::{Item, Menu, View}};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
mod jobs;
/// Implementation defaults, not owner-prescribed values; independent of consumer caps.
pub(super) const DEFAULT_ITEM_BYTES: u64 = 256 * 1024 * 1024;
pub(super) const DEFAULT_DISK_BYTES: u64 = 2 * 1024 * 1024 * 1024;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Preferences {
    pub root: Option<Vec<u8>>,
    pub item_bytes: u64,
    pub disk_bytes: u64,
}
impl Default for Preferences {
    fn default() -> Self { Self { root: None, item_bytes: DEFAULT_ITEM_BYTES, disk_bytes: DEFAULT_DISK_BYTES } }
}
impl Preferences {
    pub fn policy(&self) -> Result<Policy> { Policy::new(self.item_bytes, self.disk_bytes) }
    pub fn path(&self) -> Option<PathBuf> { self.root.as_ref().map(|bytes| PathBuf::from(std::ffi::OsString::from_vec(bytes.clone()))) }
}
#[derive(Default)]
pub(super) struct State {
    pub preferences: Preferences,
    pub selected: Option<u64>,
    pub removal: Option<hair::collection_store::Item>,
    pub edit: Option<(bool, String)>,
    pub scroll: usize,
    pub report: String,
    pub chooser: bool,
    pub required: bool,
    pub preferences_blocked: bool,
    pub preferences_unknown: bool,
    pub preference_notice: String,
    pub cache_blocked: bool,
    job: Option<jobs::Job>,
    generation: u64,
    epoch: u64,
    queued_preview: Option<u64>,
    outcome: String,
    activation: jobs::Activation,
    shutting_down: bool,
    #[cfg(test)]
    pub worker_gate: Option<WorkerGate>,
}
/// Fixture barrier at the worker's backend operation boundary, not a timer.
#[cfg(test)]
pub(super) struct WorkerGate {
    pub entered: std::sync::mpsc::Sender<()>,
    pub release: std::sync::mpsc::Receiver<()>,
    pub after: bool,
    pub body: bool,
    pub panic_on_return: bool,
}
fn failure(failure: Failure) -> String {
    match failure {
        Failure::Refused(e) => format!("Not committed; evidence preserved: {e}. Damaged/unsafe items need separate inspection; damaged-removal UI unfinished (no bypass)."),
        Failure::Ambiguous(e) => format!("Commit UNKNOWN; inspect/reopen before any new action, not blanket Retry: {e}"),
    }
}
pub(super) fn removal_status(outcome: &hair::collection_store::RemovalOutcome) -> String {
    format!("Removal COMMITTED: item-{:016x}; payload removed={}; cleanup={}; sync={}; remaining={}. Originals and independently placed room art unchanged. No automatic recovery or Retry.",
        outcome.removed_id, outcome.payload_removed,
        outcome.cleanup_error.as_deref().unwrap_or("complete"),
        outcome.sync_warning.as_deref().unwrap_or("confirmed"),
        outcome.remaining.as_ref().map(|n| format!("{n:?}")).unwrap_or_else(|| "UNKNOWN (inspection failed)".into()))
}
impl App {
    pub(super) fn documents_active(&self) -> bool {
        self.inventory.is_persistent() && !self.storage.cache_blocked
    }
    pub(super) fn check_resources_destination(destination: &hair::local_explorer::Explorer, resources: &hair::local_explorer::Explorer) -> Result<()> {
        if destination.is_within_explorer(resources)? {
            return Err("Bundled Resources cannot receive room files or layout edits".into());
        }
        Ok(())
    }
    pub(super) fn check_room_destination(&self, destination: &hair::local_explorer::Explorer) -> Result<()> {
        self.inventory.check_placement_destination(destination)?;
        Self::check_resources_destination(destination, self.resources_directory.as_ref().map_err(Clone::clone)?)
    }
    fn check_cache_adoption(&self, candidate: &std::fs::File) -> Result<()> {
        hair::local_explorer::Explorer::check_cache_directory(candidate)?;
        let resources = self.resources_directory.as_ref().map_err(Clone::clone)?;
        if resources.is_within_directory(candidate)? || resources.contains_directory(candidate)? {
            return Err("Cache cannot overlap bundled Resources; nothing initialized".into());
        }
        if let Some(local) = &self.local
            && (local.explorer.has_visited_directory(candidate)? || local.explorer.is_within_directory(candidate)?) {
                return Err("Cache cannot initialize a current/visited room or its owner ancestor; nothing initialized".into());
            }
        Ok(())
    }
    pub(super) fn restore_documents_at(&mut self, root: &Path) -> Result<()> {
        self.require_documents_idle()?;
        if !self.inventory.is_persistent() && !self.inventory.items().is_empty() {
            return Err("Carried session copies retained. Reopen a fresh session to opt in; implicit migration is not implemented.".into());
        }
        let next = hair::inventory::Inventory::persistent_checked(root, self.storage.preferences.policy()?, |held| self.check_cache_adoption(held))?;
        self.inventory = next;
        self.storage.preferences.root = Some(root.as_os_str().as_bytes().to_vec());
        self.storage.epoch = self.storage.epoch.wrapping_add(1);
        self.storage.activation = jobs::Activation::None;
        self.storage.queued_preview = None;
        self.storage.generation = self.storage.generation.wrapping_add(1);
        self.storage.cache_blocked = false;
        self.storage.required = true;
        self.storage.selected = None;
        self.storage.removal = None;
        self.inventory_selection = 0;
        self.prepare_document_selection();
        Ok(())
    }
    pub(super) fn refresh_documents(&mut self) {
        if let Err(e) = self.refresh_document_selection() {
            self.local_status = format!("Documents refresh refused; snapshot is not current: {e}");
        }
    }
    fn refresh_document_selection(&mut self) -> Result<()> {
        self.refresh_document_metadata()?;
        if self.inventory.is_persistent() { self.prepare_document_selection(); }
        Ok(())
    }
    fn refresh_document_metadata(&mut self) -> Result<()> {
        self.require_documents_idle()?;
        let selected = self.storage.selected;
        if let Err(e) = self.inventory.refresh() {
            if self.inventory.is_persistent() {
                self.storage.cache_blocked = true;
                // Retain only the stable ID for an explicit fresh refresh/plan;
                // it authorizes nothing while the snapshot is blocked.
                self.storage.removal = None;
                self.inventory_selection = usize::MAX;
                self.placement_action = None;
            }
            return Err(e);
        }
        self.storage.cache_blocked = false;
        if self.inventory.is_persistent()
            && let Some(id) = selected {
            if let Some(index) = self.inventory.items().iter().position(|i| i.id == id) {
                self.inventory_selection = index;
            } else {
                self.storage.selected = None;
                self.inventory_selection = usize::MAX;
                self.local_status = "Selected cache identity disappeared; select again before acting".into();
                return Ok(());
            }
        }
        Ok(())
    }
    pub(super) fn prepare_document_selection(&mut self) {
        if self.storage.cache_blocked {
            self.storage.selected = None;
            self.inventory_selection = usize::MAX;
            return;
        }
        self.storage.selected = self.inventory.items().get(self.inventory_selection).map(|i| i.id);
        if let Some(id) = self.storage.selected
            && let Err(e) = self.request_document_preview(id) {
                self.local_status = format!("Document remains collected; preview unavailable: {e}");
            }
        if self.storage.selected.is_none() { self.cancel_document_preview(); }
    }
    pub(super) fn plan_document_removal(&mut self) {
        if let Err(e) = self.require_documents_idle() { self.local_status = e; return; }
        let Some(id) = self.storage.selected else { self.local_status = "Select a current cache identity first".into(); return; };
        let result = (|| {
            // Fresh removal planning must not start a competing preview worker.
            self.refresh_document_metadata()?;
            if self.storage.selected != Some(id) { return Err("Selected identity disappeared; select again".into()); }
            let store = self.inventory.store().ok_or("Persistent cache unavailable")?;
            let observed = store.inspect()?.into_iter().find(|i| i.item.id == id).ok_or("Selected identity disappeared; select again")?;
            if observed.status != PayloadStatus::Unchecked {
                return Err(format!("Damaged/unavailable document: {:?}. Separate damaged inspection/confirmation UI unfinished; no destructive button. Preserve cache and inspect manually.", observed.status));
            }
            Ok(observed.item)
        })();
        match result {
            Ok(item) => {
                self.storage.removal = Some(item);
                self.storage.scroll = 0;
                if let Err(e) = self.request_document_preview(id) { self.local_status = e; }
            }
            Err(e) => { self.local_status = e; self.open_storage(); }
        }
    }
    pub(super) fn confirm_document_removal(&mut self) {
        if let Err(e) = self.require_documents_idle() { self.local_status = e; return; }
        let Some(expected) = self.storage.removal.take() else { return; };
        // remove reloads fresh locked state and refuses stale view. No lock was held while thinking.
        if self.storage.selected != Some(expected.id) || !self.inventory.store().is_some_and(|s| s.items().contains(&expected)) {
            self.local_status = "Removal refused: selection changed; select and confirm again".into();
            return;
        }
        self.local_status = match self.inventory.remove(expected.id) {
            Ok(outcome) => {
                self.storage.selected = None;
                self.inventory_selection = usize::MAX;
                removal_status(&outcome)
            }
            Err(e) => failure(e),
        };
        self.open_storage();
    }
    pub(super) fn open_storage(&mut self) {
        let status = self.local_status.clone();
        self.release_navigation();
        // Opening details is inspection, never operation cancellation. Keep the
        // durable outcome whose details the user explicitly requested.
        self.local_status = status;
        self.storage.scroll = 0;
        self.menu = Some(Menu { view: View::Storage, focus: 0 });
        self.storage.report = self.inventory.store().filter(|_| self.documents_active()).map(|store| {
            match store.interrupted_import_metadata(&std::sync::atomic::AtomicBool::new(false)) {
                Ok(report) => format!("Non-authorizing recovery report: required read budget {} candidate payload bytes per pass; candidates: {}", report.required_candidate_bytes,
                    report.candidates.iter().map(|c| format!("{}: {} actual bytes", hair::local_explorer::escape_display(&c.name), c.actual_bytes)).collect::<Vec<_>>().join("; ")),
                Err(e) => format!("Recovery inspection unavailable: {e}. Unsafe LIVE + evidence requires manual repair; preserve all evidence."),
            }
        }).unwrap_or_else(|| if self.inventory.is_busy() {
            "Persistent Documents BUSY: held cache authority retained by one worker. Metadata is last-known; no mutation/removal/adoption until reconciliation. Escape cancels cooperatively.".into()
        } else if self.storage.required {
            "Required persistent Documents unavailable/blocked. Configured path is not active; no silent session fallback. Choose or explicitly reopen a compatible private cache.".into()
        } else { "Session-only fixture; no cache root supplied. Choose an explicit existing private folder to opt in.".into() });
    }
    pub(super) fn claim_storage_request(&mut self) -> bool {
        if self.focused && self.storage.chooser && !self.chooser_active && !self.smoke && self.frames > 0 {
            self.storage.chooser = false;
            self.chooser_pending = false;
            self.chooser_active = true;
            self.release_navigation();
            true
        } else { false }
    }
    pub(super) fn complete_storage_selection(&mut self, selection: Result<Option<PathBuf>>) {
        self.storage.chooser = false;
        self.chooser_pending = false;
        self.chooser_active = false;
        self.release_navigation();
        let result: Result<bool> = (|| {
            let Some(root) = selection? else { return Ok(false); };
            self.require_documents_idle()?;
            self.preflight_storage_preferences()?;
            if !root.is_absolute() { return Err("Choose an absolute existing private cache folder".into()); }
            if !self.inventory.is_persistent() && !self.inventory.items().is_empty() {
                return Err("Session copies retained; no implicit migration. Reopen before opting in.".into());
            }
            let next = hair::inventory::Inventory::persistent_checked(&root, self.storage.preferences.policy()?, |held| self.check_cache_adoption(held))?;
            let mut preferences = self.storage.preferences.clone();
            preferences.root = Some(root.as_os_str().as_bytes().to_vec());
            let warning = self.save_storage_preferences(&preferences).map_err(|e| {
                self.storage.preference_notice = format!("Selected candidate may remain initialized at {}; preserve it, no automatic cleanup. {e}", hair::local_explorer::escape_display(root.as_os_str()));
                e
            })?;
            self.storage.preference_notice = warning.unwrap_or_default();
            self.inventory = next;
            self.storage.preferences = preferences;
            self.storage.epoch = self.storage.epoch.wrapping_add(1);
            self.storage.activation = jobs::Activation::None;
            self.storage.queued_preview = None;
            self.storage.generation = self.storage.generation.wrapping_add(1);
            self.storage.cache_blocked = false;
            self.storage.required = true;
            self.storage.selected = None;
            self.storage.removal = None;
            self.inventory_selection = 0;
            self.prepare_document_selection();
            Ok(true)
        })();
        self.local_status = match result {
            Ok(true) => "Adopted selected cache. Previous cache untouched: no move, copy, merge or deletion. Originals and room art unchanged.".into(),
            Ok(false) => "Cache chooser cancelled; current store and preferences unchanged".into(),
            Err(e) => format!("Cache not adopted; current store retained: {e}"),
        };
        self.open_storage();
    }
    pub(super) fn storage_text_input(&mut self, code: Option<KeyCode>, down: bool, repeat: bool, text: Option<&str>) -> bool {
        if self.storage.edit.is_none() { return false; }
        if !down {
            if let Some(code) = code { self.menu_held.remove(&code); self.release_latch(code); }
            return true;
        }
        if !self.focused || repeat || code.is_some_and(|code| !self.menu_held.insert(code)) { return true; }
        match code {
            Some(KeyCode::PageUp) => { self.storage.scroll = self.storage.scroll.saturating_sub(8); }
            Some(KeyCode::PageDown) => { self.storage.scroll = self.storage.scroll.saturating_add(8); }
            Some(KeyCode::Escape) => { self.storage.edit = None; }
            Some(KeyCode::Backspace) => { self.storage.edit.as_mut().unwrap().1.pop(); }
            Some(KeyCode::Enter | KeyCode::NumpadEnter) => {
                let (item, raw) = self.storage.edit.clone().unwrap();
                let result = raw.parse::<u64>().map_err(|_| "Enter positive whole bytes (no units)".to_string()).and_then(|value| {
                    let mut preferences = self.storage.preferences.clone();
                    if item { preferences.item_bytes = value; } else { preferences.disk_bytes = value; }
                    let policy = preferences.policy()?;
                    self.require_documents_idle()?;
                    let warning = self.save_storage_preferences(&preferences)?;
                    self.storage.preference_notice = warning.unwrap_or_default();
                    self.storage.preferences = preferences;
                    self.inventory.set_policy(policy);
                    Ok(())
                });
                match result {
                    Ok(()) => { self.storage.edit = None; self.local_status = "Limits saved; lowering never deletes existing records. Consumer budgets unchanged.".into(); }
                    Err(e) => { self.local_status = format!("Limits unchanged: {e}"); }
                }
            }
            _ => {
                if let Some(text) = text {
                    let raw = &mut self.storage.edit.as_mut().unwrap().1;
                    if text.bytes().all(|b| b.is_ascii_digit()) && raw.len().saturating_add(text.len()) <= 19 { raw.push_str(text); }
                    else { self.local_status = "Use positive decimal bytes, at most 19 digits; Escape cancels".into(); }
                }
            }
        }
        true
    }
    pub(super) fn storage_activate(&mut self, item: Item) {
        match item {
            Item::StorageItemLimit => self.storage.edit = Some((true, String::new())),
            Item::StorageDiskLimit => self.storage.edit = Some((false, String::new())),
            Item::StorageChoose => {
                self.storage.chooser = true; self.chooser_pending = true; self.release_navigation();
            }
            _ => {}
        }
    }
    pub(super) fn storage_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(item) = &self.storage.removal {
            lines.push("CONFIRM REMOVE EXACT CACHE ITEM".into());
            lines.push(format!("Name: {}", hair::local_explorer::escape_display(&item.name)));
            lines.push(format!("Identity: item-{:016x} (raw ID {})", item.id, item.id));
            lines.push(format!("Cache: {}", self.storage.preferences.path().map(|p| hair::local_explorer::escape_display(p.as_os_str())).unwrap_or_default()));
            lines.push("Cache-only removal. Originals and independently placed room art stay unchanged.".into());
            lines.push("ENTER: confirm healthy removal | ESC: cancel. No lock held while you decide.".into());
        } else {
            lines.push(format!("{}: {}", if self.documents_active() { "Active path" } else { "No active disk cache; configured path" }, self.storage.preferences.path().map(|p| hair::local_explorer::escape_display(p.as_os_str())).unwrap_or_else(|| "SESSION ONLY (no persistent root)".into())));
            lines.push(format!("Logical used: {} bytes / {} documents ({}manifest metadata; bytes UNCHECKED)", self.inventory.items().iter().filter_map(|i| i.stored_bytes).sum::<u64>(), self.inventory.items().len(), if self.storage.cache_blocked || self.inventory.is_busy() { "STALE last-known; no action authority; " } else { "" }));
            lines.push(format!("Per-file cap: {} bytes; aggregate disk cap: {} bytes", self.storage.preferences.item_bytes, self.storage.preferences.disk_bytes));
            lines.push("Implementation defaults: 268435456 / 2147483648 bytes. Limits independent of preview/reader/placement budgets.".into());
            lines.push("Lowering never deletes. Choose adopts existing private compatible folder; NO migration, merge or old-cache cleanup.".into());
            lines.push("Documents: B opens list, arrows select, Delete plans cache-only removal, F8 shows full status here.".into());
            if let Some(id) = self.storage.selected && let Some(preview) = self.inventory.preview(id) && let Some(warning) = &preview.warning { lines.push(warning.clone()); }
            lines.push(format!("Status: {}", self.local_status));
            if !self.storage.preference_notice.is_empty() { lines.push(self.storage.preference_notice.clone()); }
            if !self.storage.outcome.is_empty() { lines.push(self.storage.outcome.clone()); }
            if self.inventory.is_busy() { lines.push(format!("{} | Persistent metadata retained; ESC cancels, no blocking join", self.document_activity())); }
            lines.push(self.storage.report.clone());
            lines.push("Recovery UI deferred: candidate names / actual sizes / required per-pass read budget above are DISPLAY ONLY; no discard authority.".into());
            lines.push("Recovery payload read work: 3x candidate bytes (inspect + fresh confirm + unlink boundary), plus bounded metadata. Repeated inspection adds work.".into());
            lines.push("Cancel is a checkpoint, NOT a syscall/fsync deadline. Held-lock busy: wait for compatible build operation, then explicitly reopen/reselect.".into());
            lines.push("Unsafe LIVE + evidence: manual repair; preserve evidence. No auto-cleanup, error-text discard or blanket Retry. Damaged-removal UI unfinished.".into());
            if let Some((item, raw)) = &self.storage.edit { lines.insert(0, format!("EDIT {} BYTES: {raw}_ | Enter saves, Escape cancels", if *item { "PER-FILE" } else { "DISK CAP" })); }
        }
        lines
    }
    pub(super) fn storage_panel(&self, physical: [u32; 2]) -> Vec<hair::assets::Vertex> {
        hair::storage_ui::overlay(physical, self.ui_scale, &self.storage_lines(), self.storage.scroll,
            if self.storage.removal.is_none() { self.menu.as_ref() } else { None })
    }
}
