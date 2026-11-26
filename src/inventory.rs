//! Session-owned carried copies. Acquiring never removes a source file.
use crate::{Result, local_explorer::{Explorer, Kind, MAX_BODY_BYTES}};
use std::{ffi::{OsStr, OsString}, sync::atomic::{AtomicBool, Ordering}};
pub const MAX_ITEMS: usize = 128;
pub const MAX_TOTAL: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct Item {
    pub id: u64,
    pub name: OsString,
    pub bytes: Vec<u8>,
    /// Manifest logical length. Disk records never retain their bodies here.
    pub stored_bytes: Option<u64>,
}
pub struct PlacementRequest<'a> {
    pub id: u64,
    pub destination: &'a Explorer,
    pub name: &'a OsStr,
    pub move_item: bool,
    pub position: &'a crate::placement_store::Position,
    pub cancelled: &'a AtomicBool,
}
#[derive(Default)]
pub struct Inventory {
    items: Vec<Item>,
    next: u64,
    previews: std::collections::BTreeMap<u64, crate::file_items::Preview>,
    store: Option<crate::collection_store::Store>,
    /// Exactly one worker owns the held Store; UI retains bounded manifest metadata.
    busy: bool,
    /// A worker failed without returning its held authority. Never reopen by path.
    lost_authority: bool,
    notice: Option<String>,
}
impl Inventory {
    pub fn persistent(root: &std::path::Path, policy: crate::collection_store::Policy) -> Result<Self> {
        Self::persistent_checked(root, policy, |_| Ok(()))
    }
    pub fn persistent_checked(root: &std::path::Path, policy: crate::collection_store::Policy,
        check: impl FnOnce(&std::fs::File) -> Result<()>) -> Result<Self> {
        let store = crate::collection_store::Store::open_checked(root, policy, check)?;
        let mut inventory = Self { store: Some(store), ..Self::default() };
        inventory.refresh()?;
        Ok(inventory)
    }
    pub fn is_persistent(&self) -> bool { self.store.is_some() || self.busy || self.lost_authority }
    pub fn is_busy(&self) -> bool { self.busy }
    pub fn has_lost_authority(&self) -> bool { self.lost_authority }
    pub fn mark_worker_lost(&mut self) {
        assert!(self.busy && self.store.is_none());
        self.busy = false;
        self.lost_authority = true;
        self.previews.clear();
    }
    fn available(&self) -> Result<()> {
        if self.lost_authority { return Err("Documents authority lost; activation UNKNOWN. Preserve evidence and safely restart; no session fallback".into()); }
        if self.busy { return Err("Documents busy; wait for worker reconciliation, no session fallback".into()); }
        Ok(())
    }
    /// Transfers held authority, never reopens a pathname or creates a cache.
    pub fn take_worker(&mut self) -> Result<Self> {
        self.available()?;
        if self.store.is_none() { return Err("Persistent Documents unavailable".into()); }
        self.busy = true;
        self.previews.clear();
        Ok(Self { store: self.store.take(), items: self.items.clone(), ..Self::default() })
    }
    pub fn return_worker(&mut self, worker: Self) {
        assert!(self.busy && worker.store.is_some() && !worker.busy);
        *self = worker;
    }
    pub fn clear_previews(&mut self) { self.previews.clear(); }
    pub fn store(&self) -> Option<&crate::collection_store::Store> { self.store.as_ref() }
    pub fn check_placement_destination(&self, destination: &Explorer) -> Result<()> {
        self.available()?;
        if let Some(store) = &self.store { store.check_placement_destination(destination)?; }
        Ok(())
    }
    pub fn set_policy(&mut self, policy: crate::collection_store::Policy) {
        if let Some(store) = &mut self.store { store.set_policy(policy); }
    }
    pub fn notice(&self) -> Option<&str> { self.notice.as_deref() }
    fn update_items(&mut self) {
        if let Some(store) = &self.store {
            self.items = store.items().iter().map(|item| Item {
                id: item.id, name: item.name.clone(), bytes: Vec::new(), stored_bytes: Some(item.bytes),
            }).collect();
        }
    }
    pub fn refresh(&mut self) -> Result<()> {
        self.available()?;
        let Some(store) = &mut self.store else { return Ok(()); };
        store.refresh()?;
        self.update_items();
        self.previews.clear();
        Ok(())
    }
    /// Metadata-only admission check, preserving a preview solely for the exact
    /// immutable manifest item. Missing/replaced IDs never inherit its geometry.
    pub fn refresh_selected(&mut self, id: u64) -> Result<bool> {
        self.available()?;
        let before = self.store.as_ref().and_then(|store| store.items().iter().find(|i| i.id == id)).cloned();
        let preview = self.previews.remove(&id);
        self.refresh()?;
        let same = before.is_some() && self.store.as_ref().and_then(|store| store.items().iter().find(|i| i.id == id)) == before.as_ref();
        if same && let Some(preview) = preview { self.previews.insert(id, preview); }
        Ok(same)
    }
    /// Only one selected preview; storage policy never authorizes consumer allocations.
    pub fn prepare_selected(&mut self, id: u64) -> Result<()> {
        self.prepare_selected_cancelled(id, &AtomicBool::new(false))
    }
    pub fn prepare_selected_cancelled(&mut self, id: u64, cancelled: &AtomicBool) -> Result<()> {
        self.prepare_selected_checkpoint(id, cancelled, || {})
    }
    /// Checkpoint receives progress only, never payload/staging authority.
    pub fn prepare_selected_checkpoint(&mut self, id: u64, cancelled: &AtomicBool, mut chunk_read: impl FnMut()) -> Result<()> {
        self.available()?;
        let check = || if cancelled.load(Ordering::Relaxed) { Err("Preview cancelled".to_string()) } else { Ok(()) };
        check()?;
        let Some(store) = &self.store else { return Ok(()); };
        let item = self.items.iter().find(|item| item.id == id).ok_or("Select a current document")?;
        let catalog = crate::file_items::Catalog::builtin()?;
        check()?;
        let mut preview = match Self::selected_bytes_checkpoint(store, id, cancelled, &mut chunk_read) {
            Ok(bytes) => { check()?; catalog.owned(&item.name, &bytes) },
            Err(error) => {
                check()?;
                crate::file_items::Preview {
                    model: catalog.representation(&item.name), content_derived: false,
                    warning: Some(format!("Representative only; document remains collected: {error}")),
                }
            },
        };
        check()?;
        let retained = preview.model.vertices.len() * std::mem::size_of::<crate::assets::Vertex>()
            + preview.model.textures.iter().map(|t| t.rgba.len()).sum::<usize>();
        if preview.content_derived && retained > 16 * 1024 * 1024 {
            preview = crate::file_items::Preview { model: catalog.representation(&item.name), content_derived: false,
                warning: Some("Inventory preview budget; representative only".into()) };
        }
        check()?;
        self.previews.clear();
        self.previews.insert(id, preview);
        Ok(())
    }
    fn selected_bytes(store: &crate::collection_store::Store, id: u64, cancelled: &AtomicBool) -> Result<Vec<u8>> {
        Self::selected_bytes_checkpoint(store, id, cancelled, &mut || {})
    }
    fn selected_bytes_checkpoint(store: &crate::collection_store::Store, id: u64, cancelled: &AtomicBool,
        chunk_read: &mut impl FnMut()) -> Result<Vec<u8>> {
        use std::io::Write;
        struct Bounded<'a, F>(Vec<u8>, &'a mut F);
        impl<F: FnMut()> Write for Bounded<'_, F> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.0.len().checked_add(bytes.len()).is_none_or(|n| n as u64 > MAX_BODY_BYTES) {
                    return Err(std::io::Error::other("Selected consumer byte budget exceeded"));
                }
                self.0.try_reserve(bytes.len()).map_err(std::io::Error::other)?;
                self.0.extend_from_slice(bytes);
                (self.1)();
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        let item = store.items().iter().find(|item| item.id == id).ok_or("Document no longer present")?;
        if item.bytes > MAX_BODY_BYTES { return Err("Existing selected consumer byte budget exceeded".into()); }
        let mut output = Bounded(Vec::new(), chunk_read);
        store.copy_to(id, &mut output, cancelled)?;
        Ok(output.0)
    }
    pub fn remove(&mut self, id: u64) -> std::result::Result<crate::collection_store::RemovalOutcome, crate::collection_store::Failure> {
        self.available().map_err(crate::collection_store::Failure::Refused)?;
        let store = self.store.as_mut().ok_or_else(|| crate::collection_store::Failure::Refused("Session copies have no persistent removal action".into()))?;
        let outcome = store.remove(id)?;
        self.items.retain(|item| item.id != id);
        self.previews.remove(&id);
        Ok(outcome)
    }
    pub fn items(&self) -> &[Item] { &self.items }
    pub fn preview(&self, id: u64) -> Option<&crate::file_items::Preview> { self.previews.get(&id) }
    /// Worker core captures immutable source name/reader before any navigation.
    pub fn collect_held(&mut self, name: &OsStr, held: &crate::local_explorer::ReaderFile,
        cancelled: &AtomicBool) -> std::result::Result<u64, crate::collection_store::Failure> {
        self.collect_held_checkpoint(name, held, cancelled, || {})
    }
    pub fn collect_held_checkpoint(&mut self, name: &OsStr, held: &crate::local_explorer::ReaderFile,
        cancelled: &AtomicBool, chunk_written: impl FnMut()) -> std::result::Result<u64, crate::collection_store::Failure> {
        self.available().map_err(crate::collection_store::Failure::Refused)?;
        let store = self.store.as_mut().ok_or_else(|| crate::collection_store::Failure::Refused("Persistent Documents unavailable".into()))?;
        let commit = store.import_document_checkpoint(name, held, cancelled, chunk_written)?;
        let id = commit.value.id;
        self.notice = commit.sync_warning;
        self.update_items();
        Ok(id)
    }
    pub fn collect(&mut self, source: &Explorer, index: usize) -> Result<u64> {
        self.available()?;
        self.notice = None;
        if self.store.is_some() {
            let entry = source.current().entries.get(index).ok_or("File no longer listed")?;
            if entry.kind != Kind::File { return Err("Only regular files can be collected".into()); }
            let held = source.open_reader(index)?;
            let id = self.collect_held(&entry.name, &held, &AtomicBool::new(false))
                .map_err(|failure| match failure {
                    crate::collection_store::Failure::Refused(e) => format!("Not committed; evidence preserved: {e}"),
                    crate::collection_store::Failure::Ambiguous(e) => format!("Commit UNKNOWN; inspect/reopen, do not repeat blindly: {e}"),
                })?;
            if let Err(e) = self.prepare_selected(id) { self.notice = Some(format!("Committed; preview unavailable: {e}; {:?}", self.notice)); }
            return Ok(id);
        }
        if self.items.len() >= MAX_ITEMS { return Err("Inventory full (128 items)".into()); }
        let entry = source.current().entries.get(index).ok_or("File no longer listed")?;
        if entry.kind != Kind::File { return Err("Only regular files can be collected".into()); }
        let name = entry.name.clone();
        let bytes = source.read_entry(index, MAX_BODY_BYTES)?;
        let total = self.items.iter().map(|i| i.bytes.len()).sum::<usize>();
        if total.checked_add(bytes.len()).is_none_or(|n| n > MAX_TOTAL) { return Err("Inventory byte budget exceeded".into()); }
        let id = self.next;
        let next = id.checked_add(1).ok_or("Inventory identity exhausted")?;
        self.items.try_reserve(1).map_err(|_| "Inventory allocation refused")?;
        let catalog = crate::file_items::Catalog::builtin()?;
        let mut preview = catalog.owned(&name, &bytes);
        let cost = |p: &crate::file_items::Preview| {
            if p.content_derived {
                p.model.vertices.len() * std::mem::size_of::<crate::assets::Vertex>()
                    + p.model.textures.iter().map(|t| t.rgba.len()).sum::<usize>()
            } else { 0 }
        };
        let retained = self.previews.values().map(cost).sum::<usize>();
        if retained.saturating_add(cost(&preview)) > 16 * 1024 * 1024 {
            preview = crate::file_items::Preview { model: catalog.representation(&name),
                warning: Some("Inventory preview budget; representative only".into()), content_derived: false };
        }
        self.previews.insert(id, preview);
        self.items.push(Item { id, name, bytes, stored_bytes: None });
        self.next = next;
        Ok(id)
    }
    pub fn place_checked(&mut self, request: PlacementRequest<'_>,
        mut validate: impl FnMut(&crate::placement_store::LayoutRead) -> Result<()>) -> crate::placement_store::Outcome {
        use crate::placement_store::{self, Outcome};
        if let Err(e) = self.available() { return Outcome::NotCommitted(e); }
        self.notice = None;
        if let Some(store) = &self.store {
            if let Err(e) = store.check_placement_destination(request.destination) { return Outcome::NotCommitted(e); }
            // Final bounded export/publication remains synchronous, outside the async preview slice.
            let bytes = match Self::selected_bytes(store, request.id, request.cancelled) {
                Ok(bytes) => bytes,
                Err(e) => return Outcome::NotCommitted(e),
            };
            let outcome = placement_store::publish_checked(request.destination, request.name, &bytes, request.position, request.cancelled, |layout| {
                store.check_placement_destination(request.destination)?;
                validate(layout)
            });
            if request.move_item && matches!(outcome, Outcome::Committed { .. }) {
                self.notice = Some("Placement committed as COPY; persistent source retained. Move consumption is not implemented; remove explicitly in Documents.".into());
            }
            return outcome;
        }
        let Some(index) = self.items.iter().position(|i| i.id == request.id) else {
            return Outcome::NotCommitted("Inventory item no longer present".into());
        };
        let outcome = placement_store::publish_checked(request.destination, request.name, &self.items[index].bytes,
            request.position, request.cancelled, validate);
        if request.move_item && matches!(outcome, Outcome::Committed { .. }) {
            self.items.remove(index);
            self.previews.remove(&request.id);
        }
        outcome
    }
    /// Bounded file-and-position action; ambiguous outcomes retain the item.
    pub fn place(&mut self, id: u64, destination: &Explorer, name: &OsStr, move_item: bool,
        position: &crate::placement_store::Position, cancelled: &AtomicBool) -> crate::placement_store::Outcome {
        use crate::placement_store::{self, Outcome};
        if let Err(e) = self.available() { return Outcome::NotCommitted(e); }
        if self.store.is_some() {
            return self.place_checked(PlacementRequest { id, destination, name, move_item, position, cancelled }, |_| Ok(()));
        }
        let Some(index) = self.items.iter().position(|i| i.id == id) else {
            return Outcome::NotCommitted("Inventory item no longer present".into());
        };
        let outcome = placement_store::publish(destination, name, &self.items[index].bytes, position, cancelled);
        if move_item && matches!(outcome, Outcome::Committed { .. }) {
            self.items.remove(index);
            self.previews.remove(&id);
        }
        outcome
    }
}
// End-to-end copy/move/failure coverage lives with the macOS transaction adapter
// in placement_store::tests, including source preservation and reopened positions.
