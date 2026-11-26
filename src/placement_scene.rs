//! Read-only reconciliation of bounded saved preferences into a generated room.
//! Filesystem association is established by placement_store, never by this map.
use crate::{
    Result, World,
    host_scene::{HostScene, PlacedItem},
    local_explorer::{Explorer, Kind},
    placement::{self, Overlap},
    placement_store,
};

pub fn install(explorer: &Explorer, world: &World, host: &mut HostScene) -> Result<Vec<String>> {
    let layout = placement_store::read_layout(explorer)?;
    // Display/physics restoration only: a pending replacement must not hide its
    // checked published predecessor on refresh/cold open. No pending data enters
    // this candidate, and mutating admission below still rejects incomplete data.
    // Malformed/ambiguous active records (warnings/read errors) remain refused.
    if layout.incomplete_records != 0 && layout.warnings.is_empty() && !layout.decorations.is_empty() {
        let mut active = layout.clone();
        active.incomplete_records = 0;
        let mut notes = apply(&active, world, host)?;
        notes.push(format!("{} unfinished records preserved; published decorations remain authoritative; inspect explicitly, no automatic recovery", layout.incomplete_records));
        return Ok(notes);
    }
    apply(&layout, world, host)
}
pub fn apply(
    layout: &placement_store::LayoutRead,
    world: &World,
    host: &mut HostScene,
) -> Result<Vec<String>> {
    if layout.decorations.is_empty() {
        return apply_files(layout, world, host);
    }
    if layout.incomplete_records != 0 || !layout.warnings.is_empty() {
        return Err("Decoration layout requires recovery; records retained".into());
    }
    let mut staged = host.clone();
    staged.decorations.clear();
    for d in &layout.decorations {
        d.validate()?;
        if staged.decorations.insert(d.placement_id.clone(), d.clone()).is_some() {
            return Err("Duplicate decoration placement identity".into());
        }
    }
    let notes = apply_files(layout, world, &mut staged)?;
    if !notes.is_empty() {
        return Err(format!("Decoration layout requires recovery: {}", notes.join(" | ")));
    }
    for d in &layout.decorations {
        decoration_clearance(&staged, d)?;
    }
    staged.check_routes(&[])?;
    *host = staged;
    Ok(notes)
}
// Use the same logical envelope and saved-file placeholder policy in snapshot
// preview and fresh layout admission. Appearance triangles are not authority.
fn decoration_clearance(host: &HostScene, decoration: &placement_store::Decoration) -> Result<()> {
    let mut check = host.clone();
    check.decorations.remove(&decoration.placement_id);
    check.anchors.retain(|a| !host.file_placements.contains_key(&a.id));
    placement::surface::clearance(&check, &decoration.envelope_mesh()?, Overlap::Prevent)
}
/// Partial current-room snapshot only: no storage access, no all-page or fresh
/// listing validation. A success is pending admission, never permission to save.
pub fn validate_decoration_snapshot(
    host: &HostScene,
    decoration: &placement_store::Decoration,
    visitor: [f32; 2],
) -> Result<()> {
    decoration.validate()?;
    if host.decorations.contains_key(&decoration.placement_id) {
        return Err("Decoration placement identity already exists".into());
    }
    decoration_clearance(host, decoration)?;
    let mut projected = host.clone();
    projected.decorations.insert(decoration.placement_id.clone(), decoration.clone());
    projected.check_routes(&[visitor])
}
fn apply_files(
    layout: &placement_store::LayoutRead,
    world: &World,
    host: &mut HostScene,
) -> Result<Vec<String>> {
    let mut notes = layout.warnings.clone();
    if layout.incomplete_records > 0 {
        notes.push(format!(
            "{} incomplete placement records retained; no automatic repair",
            layout.incomplete_records
        ));
    }
    let view = world
        .folder_view
        .as_ref()
        .ok_or("Missing placement directory view")?;
    let baseline = host.clone();
    let mut rejected_names = std::collections::BTreeSet::new();
    // Resolve every saved identity before clearance. Transaction enumeration
    // order must not invalidate an earlier record after deliberate overlap.
    for (name, position) in layout.positions.clone() {
        let Some((id, _)) = view
            .bindings
            .iter()
            .find(|(_, b)| b.entry.kind == Kind::File && b.entry.name == name)
        else {
            continue;
        };
        let attempt = position
            .bounds()
            .map(|bounds| PlacedItem { position, bounds });
        match attempt {
            Ok(item) => {
                host.file_placements.insert(id.clone(), item);
            }
            Err(e) => {
                rejected_names.insert(name.clone());
                notes.push(format!(
                    "{}: saved placement ignored ({e}); record retained",
                    crate::folder_scene::display(&name)
                ));
            }
        }
    }
    loop {
        // Surface records own file transforms, not additional listing-slot bases.
        // Allocate generated slots only to the remaining entries. The reserved
        // metadata directory stays listed/navigable without shifting file slots.
        for (id, bound) in &view.bindings {
            if host.file_placements.contains_key(id)
                || (bound.entry.kind == Kind::Folder && bound.entry.name == ".hair-layout")
            {
                continue;
            }
            if bound.entry.kind != Kind::Folder
                && let Some(anchor) = host.anchors.iter_mut().find(|a| &a.id == id)
            {
                // Use the global rank, not the new page-local listing slot.
                // Insertion can push a generated file across a page boundary.
                let saved_before = layout
                    .positions
                    .iter()
                    .filter(|(name, p)| {
                        *name < bound.entry.name
                            && view.file_names.contains(name)
                            && !layout.retained_slots.contains(name)
                            && !rejected_names.contains(name)
                            && p.validate().is_ok()
                    })
                    .count();
                let slot =
                    bound.slot_index.saturating_sub(saved_before) % crate::folder_scene::PAGE_SIZE;
                anchor.at = [
                    -4.5 + (slot % 4) as f32 * 3.0,
                    0.0,
                    -3.0 + (slot / 4) as f32 * 3.0,
                ];
            }
        }
        let mut rejected = Vec::new();
        for (id, item) in &host.file_placements {
            let mut check = host.clone();
            // Inactive saved-file placeholders are not generated furniture.
            check
                .anchors
                .retain(|a| !host.file_placements.contains_key(&a.id));
            // Restoration is symmetric; either record may explicitly allow
            // overlap. Publication separately enforces the new action's policy.
            check
                .file_placements
                .retain(|other, p| other != id && !p.position.allow_overlap);
            let bounds = &item.bounds;
            let mut mesh = Vec::new();
            crate::scene::cuboid(
                &mut mesh,
                [
                    bounds.at[0],
                    bounds.at[1] + bounds.size[1] * 0.5,
                    bounds.at[2],
                ],
                bounds.size,
                [1.; 3],
            );
            if let Err(e) = placement::surface::clearance(
                &check,
                &mesh,
                if item.position.allow_overlap {
                    Overlap::Allow
                } else {
                    Overlap::Prevent
                },
            ) {
                rejected.push((id.clone(), e));
            }
        }
        if rejected.is_empty() {
            break;
        }
        for (id, e) in rejected {
            host.file_placements.remove(&id);
            rejected_names.insert(view.bindings[&id].entry.name.clone());
            notes.push(format!(
                "{}: saved placement ignored ({e}); record retained",
                crate::folder_scene::display(&view.bindings[&id].entry.name)
            ));
        }
        // Rejected records regain generated slots. Every pass removes a
        // candidate, bounding this work to the page's twelve bindings.
    }
    if let Err(e) = host.check_routes(&[]) {
        *host = baseline;
        notes.push(format!(
            "Saved placement group ignored: {e}; records retained"
        ));
    }
    Ok(notes)
}
/// Validate a complete candidate scene, not an additional decoration. Relocation
/// supplies exactly one same-identity replacement under its writer lock.
/// Rebuild every page from a fresh held-directory listing; retain all other records.
pub fn validate_decoration_candidate(
    explorer: &Explorer,
    candidate: &placement_store::LayoutRead,
    visitor: [f32; 2],
    current_page: usize,
) -> Result<()> {
    if candidate.incomplete_records != 0 || !candidate.warnings.is_empty() {
        return Err("Decoration layout requires recovery; records retained".into());
    }
    let mut fresh = explorer.try_clone()?;
    fresh.refresh()?;
    let snapshot = fresh.current();
    let pages = snapshot.entries.len().max(1).div_ceil(crate::folder_scene::PAGE_SIZE);
    if current_page >= pages {
        return Err("Current decoration page is out of range; refresh and reselect".into());
    }
    for page in 0..pages {
        let (world, mut host) = crate::folder_scene::build(snapshot, page, true)?;
        let notes = apply(candidate, &world, &mut host)?;
        if !notes.is_empty() { return Err(format!("Layout needs recovery: {}", notes.join(" | "))); }
        if page == current_page { host.check_routes(&[visitor])?; }
    }
    Ok(())
}

/// A new floor decoration is present on every page, not owned by a listing slot.
pub fn validate_decoration(
    explorer: &Explorer,
    layout: &placement_store::LayoutRead,
    decoration: &placement_store::Decoration,
    visitor: [f32; 2],
    current_page: usize,
) -> Result<()> {
    decoration.validate()?;
    if layout.decorations.iter().any(|d| d.placement_id == decoration.placement_id) {
        return Err("Decoration placement identity already exists".into());
    }
    let mut projected = layout.clone();
    projected.decorations.push(decoration.clone());
    validate_decoration_candidate(explorer, &projected, visitor, current_page)
}
/// Validate a layout-only replacement, with no hypothetical file insertion.
/// All live saved pages are checked under the same writer lock at confirmation.
pub fn validate_reposition(
    explorer: &Explorer,
    layout: &placement_store::LayoutRead,
    name: &std::ffi::OsStr,
    position: &placement_store::Position,
    visitor: [f32; 2],
    current_page: usize,
) -> Result<usize> {
    position.validate()?;
    let mut fresh = explorer.try_clone()?;
    fresh.refresh()?;
    let snapshot = fresh.current();
    let index = snapshot
        .entries
        .iter()
        .position(|e| e.kind == Kind::File && e.name == name)
        .ok_or("Selected file disappeared; refresh and reselect")?;
    let page = index / crate::folder_scene::PAGE_SIZE;
    let mut projected = placement_store::LayoutRead {
        positions: layout
            .positions
            .iter()
            .filter(|(n, _)| n != name)
            .cloned()
            .collect(),
        decorations: layout.decorations.clone(),
        retained_slots: layout.retained_slots.clone(),
        warnings: layout.warnings.clone(),
        incomplete_records: layout.incomplete_records,
    };
    if retains_generated_slot(&fresh, layout, name)? {
        projected.retained_slots.push(name.into());
    }
    projected.positions.push((name.into(), position.clone()));
    let mut pages = std::collections::BTreeSet::from([page, current_page]);
    if !projected.decorations.is_empty() {
        pages.extend(0..snapshot.entries.len().max(1).div_ceil(crate::folder_scene::PAGE_SIZE));
    }
    for (n, _) in &projected.positions {
        if let Some(i) = snapshot
            .entries
            .iter()
            .position(|e| e.kind == Kind::File && e.name == *n)
        {
            pages.insert(i / crate::folder_scene::PAGE_SIZE);
        }
    }
    for checked in pages {
        let (world, mut host) = crate::folder_scene::build(snapshot, checked, true)?;
        let notes = apply(&projected, &world, &mut host)?;
        if !notes.is_empty() {
            return Err(format!(
                "Reposition needs layout recovery: {}",
                notes.join(" | ")
            ));
        }
        if checked == page {
            let id = world
                .folder_view
                .as_ref()
                .and_then(|v| {
                    v.bindings
                        .iter()
                        .find(|(_, b)| b.entry.name == name)
                        .map(|(id, _)| id)
                })
                .ok_or("Missing selected binding")?;
            if !host.file_placements.contains_key(id) {
                return Err("Reposition not admitted into room".into());
            }
            let mut check = host.clone();
            check.anchors.retain(|a| a.id != *id);
            check.file_placements.remove(id);
            let mesh = position.envelope_mesh()?;
            placement::surface::clearance(
                &check,
                &mesh,
                if position.allow_overlap {
                    Overlap::Allow
                } else {
                    Overlap::Prevent
                },
            )?;
        }
        if checked == page || checked == current_page {
            host.check_routes(&[visitor])?;
        }
    }
    Ok(page)
}
/// Determine rank ownership from actual admission, not a raw record's presence.
/// Read diagnostics remain untouched; this never repairs a record.
pub fn retains_generated_slot(
    explorer: &Explorer,
    layout: &placement_store::LayoutRead,
    name: &std::ffi::OsStr,
) -> Result<bool> {
    if layout.retained_slots.iter().any(|n| n == name) {
        return Ok(true);
    }
    let mut fresh = explorer.try_clone()?;
    fresh.refresh()?;
    let snapshot = fresh.current();
    let index = snapshot.entries.iter().position(|e| e.kind == Kind::File && e.name == name)
        .ok_or("Selected file disappeared; refresh and reselect")?;
    let (world, mut host) = crate::folder_scene::build(snapshot, index / crate::folder_scene::PAGE_SIZE, true)?;
    let _notes = apply(layout, &world, &mut host)?;
    let id = world.folder_view.as_ref().unwrap().bindings.iter()
        .find(|(_, b)| b.entry.name == name).ok_or("Missing selected binding")?.0;
    Ok(!host.file_placements.contains_key(id))
}
/// Validate the projected post-publication room under the publisher's writer lock.
/// The hypothetical entry is confined to this disposable plan, never file authority.
pub fn validate_publication(
    explorer: &Explorer,
    layout: &placement_store::LayoutRead,
    name: &std::ffi::OsStr,
    position: &placement_store::Position,
    visitor: [f32; 2],
    current_page: usize,
) -> Result<usize> {
    let mut fresh = explorer.try_clone()?;
    fresh.refresh()?;
    let mut snapshot = fresh.current().clone();
    if snapshot.entries.iter().any(|e| e.name == name) {
        return Err("Destination name already exists; no overwrite".into());
    }
    snapshot.entries.push(crate::local_explorer::Entry {
        name: name.into(),
        path: snapshot.folder.join(name),
        kind: Kind::File,
    });
    snapshot.entries.sort_by(|a, b| a.name.cmp(&b.name));
    let index = snapshot
        .entries
        .iter()
        .position(|e| e.name == name)
        .ok_or("Missing planned entry")?;
    let page = index / crate::folder_scene::PAGE_SIZE;
    let mut projected = placement_store::LayoutRead {
        positions: layout.positions.clone(),
        decorations: layout.decorations.clone(),
        retained_slots: layout.retained_slots.clone(),
        incomplete_records: layout.incomplete_records,
        warnings: layout.warnings.clone(),
    };
    projected.positions.push((name.into(), position.clone()));
    let current_page =
        current_page.min((snapshot.entries.len() - 1) / crate::folder_scene::PAGE_SIZE);
    let mut pages = std::collections::BTreeSet::from([page, current_page]);
    if !projected.decorations.is_empty() {
        pages.extend(0..snapshot.entries.len().max(1).div_ceil(crate::folder_scene::PAGE_SIZE));
    }
    for (saved, _) in &projected.positions {
        if let Some(i) = snapshot
            .entries
            .iter()
            .position(|e| e.kind == Kind::File && e.name == *saved)
        {
            pages.insert(i / crate::folder_scene::PAGE_SIZE);
        }
    }
    // An insertion can shift saved files across a page boundary into a smaller room.
    // Validate all live saved pages, not just the page receiving the new file.
    for checked_page in pages {
        let (world, mut host) = crate::folder_scene::build(&snapshot, checked_page, true)?;
        let notes = apply(&projected, &world, &mut host)?;
        if !notes.is_empty() {
            return Err(format!(
                "Placement requires layout recovery on page {}: {}",
                checked_page + 1,
                notes.join(" | ")
            ));
        }
        if checked_page == page {
            let id = world
                .folder_view
                .as_ref()
                .and_then(|v| {
                    v.bindings
                        .iter()
                        .find(|(_, b)| b.entry.name == name)
                        .map(|(id, _)| id)
                })
                .ok_or("Missing planned binding")?;
            if !host.file_placements.contains_key(id) {
                return Err("Placement was not admitted into planned room".into());
            }
            // An existing Allow record cannot silently opt a new Prevent action
            // into overlap. Check the pending item against *all* existing files.
            let mut check = host.clone();
            check.anchors.retain(|a| a.id != *id);
            check.file_placements.remove(id);
            let mesh = position.envelope_mesh()?;
            placement::surface::clearance(
                &check,
                &mesh,
                if position.allow_overlap {
                    Overlap::Allow
                } else {
                    Overlap::Prevent
                },
            )?;
        }
        if checked_page == page || checked_page == current_page {
            host.check_routes(&[visitor])?;
        }
    }
    Ok(page)
}
