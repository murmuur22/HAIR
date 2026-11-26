use super::*;
use hair::placement_store::{self as store, ResumeStage};
use std::{cell::RefCell, rc::Rc, os::fd::AsRawFd};

pub(super) fn inspect_select(a: &mut App, d: &Decoration) {
    click_label(a, "Inspect unfinished placements");
    let id = format!("exact storage-local ID dc-{}", d.placement_id);
    for _ in 0..256 {
        if a.recovery_status().contains(&id) { break; }
        tap(a, KeyCode::PageDown);
    }
    assert!(a.recovery_status().contains(&id), "{}", a.recovery_status());
    click_label(a, "Select attempt");
    assert!(a.place_preview().is_ok(), "{}", a.local_status);
}
pub(super) fn attrs(root: &Path) -> Vec<(PathBuf, Vec<u8>, Vec<u8>)> {
    let mut out = vec![];
    for path in records(root).keys() {
        let f = std::fs::File::open(path).unwrap();
        let mut names = [0u8; 4096];
        let n = unsafe { libc::flistxattr(f.as_raw_fd(), names.as_mut_ptr().cast(), names.len(), 0) };
        assert!(n >= 0);
        for name in names[..n as usize].split(|b| *b == 0).filter(|s| !s.is_empty()) {
            let c = std::ffi::CString::new(name).unwrap(); let mut bytes = [0u8; 4096];
            let n = unsafe { libc::fgetxattr(f.as_raw_fd(), c.as_ptr(), bytes.as_mut_ptr().cast(), bytes.len(), 0, 0) };
            assert!(n >= 0); out.push((path.clone(), name.to_vec(), bytes[..n as usize].to_vec()));
        }
    }
    out.sort(); out
}
fn point(a: &mut App, label: &str) {
    let (_, r, _) = a.toolbox_controls().into_iter().find(|(_, _, s)| s.contains(label)).unwrap();
    let f = a.inventory_frame();
    a.pointer_moved(Some([f64::from(r[0] + 8.) * f64::from(f.physical[0]) / f64::from(f.full[0]),
        f64::from(r[1] + 8.) * f64::from(f.physical[1]) / f64::from(f.full[1])]));
}
#[test]
fn slice8c_inspection_missing_and_all_cancel_paths_are_write_free_and_modal() {
    let (f, mut a) = open_fixture("slice8c-absent");
    tap(&mut a, KeyCode::KeyP); click_label(&mut a, "Inspect unfinished placements");
    assert!(a.recovery_status().contains("0 unfinished"));
    for _ in 0..4 { a.place_ui(); tap(&mut a, KeyCode::Enter); }
    assert!(!f.0.join(".hair-layout").exists());
    click_label(&mut a, "Close inspection"); assert!(!a.input.captured);
    for reviewed in [false, true] {
        for exit in ["escape", "focus", "refresh", "map", "source", "address", "chooser", "menu", "page"] {
            let (f, mut a, d, _) = prepare("relocation");
            let before = records(&f.0); let xattrs = attrs(&f.0); let host = a.themes[0].host.clone();
            inspect_select(&mut a, &d);
            if reviewed { tap(&mut a, KeyCode::Enter); assert!(a.local_status.contains("frozen recovery review")); }
            a.pointer_moved(Some([300., 450.])); a.place_pointer(true); a.place_pointer(false);
            assert!(!a.input.captured && !a.motion_owned());
            for key in [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD, KeyCode::KeyX, KeyCode::KeyB, KeyCode::KeyH, KeyCode::KeyN] { tap(&mut a, key); }
            a.advance_motion(0.1); assert_eq!(a.themes[0].host, host);
            match exit {
                "escape" => { tap(&mut a, KeyCode::Escape); if reviewed { assert!(a.recovery_open()); tap(&mut a, KeyCode::Escape); } assert!(!a.recovery_open()); }
                "focus" => { a.focus_changed(false); a.focus_changed(true); assert!(!a.recovery_open()); }
                "refresh" => tap(&mut a, KeyCode::F5),
                "map" => tap(&mut a, KeyCode::KeyM),
                "source" => { tap(&mut a, KeyCode::KeyI); assert!(a.world.reading); assert!(a.world.document().unwrap().contains(&d.placement_id)); }
                "address" => tap(&mut a, KeyCode::KeyL),
                "chooser" => { a.chooser_pending = true; tap(&mut a, KeyCode::Enter); a.chooser_pending = false; assert!(!a.recovery_open()); }
                "menu" => { a.menu = Some(hair::presentation::Menu::settings()); tap(&mut a, KeyCode::KeyH); assert!(!a.recovery_open()); }
                "page" => { tap(&mut a, KeyCode::PageDown); assert!(!a.toolbox_controls().iter().any(|(_,_,s)| s.contains("Confirm resume"))); }
                _ => unreachable!(),
            }
            assert_eq!(records(&f.0), before, "{reviewed}/{exit}"); assert_eq!(attrs(&f.0), xattrs);
            println!("SLICE8C cancel reviewed={reviewed} context={exit} writes=none bytes/inodes/xattrs=exact");
        }
    }
}
#[test]
fn slice8c_fresh_enter_and_independent_pointer_confirm_own_intent() {
    let (f, mut a, d, _) = prepare("follow"); inspect_select(&mut a, &d);
    let before = records(&f.0);
    a.key(KeyCode::Enter, true, false).unwrap();
    assert!(a.local_status.contains("frozen recovery review"));
    for repeat in [false, true, true] { a.key(KeyCode::Enter, true, repeat).unwrap(); }
    assert_eq!(records(&f.0), before);
    point(&mut a, "Confirm resume"); a.place_pointer(true); a.pointer_moved(None); a.place_pointer(false);
    assert_eq!(records(&f.0), before, "drag-off release inert");
    point(&mut a, "Confirm resume"); a.place_pointer(true); tap(&mut a, KeyCode::Escape); a.place_pointer(false);
    assert_eq!(records(&f.0), before, "cancelled pointer must not reselect or confirm");
    a.key(KeyCode::Enter, false, false).unwrap();
    click_label(&mut a, "Select attempt"); click_label(&mut a, "Review (no writes)");
    assert_eq!(records(&f.0), before);
    click_label(&mut a, "Confirm resume");
    assert_eq!(a.recovery_test_outcome(), Some("Committed { synced: true }"));
    let status = a.local_status.clone(); tap(&mut a, KeyCode::Enter);
    assert_eq!(a.local_status, status, "extra Enter cannot erase committed qualifiers or retry");
    assert_eq!(layout(&a).decorations, vec![d]);
}
#[test]
fn slice8c_stale_selected_snapshot_context_and_real_preflight_refusals_preserve() {
    for late in [false, true] {
        for change in ["pending", "listing", "visitor", "page", "theme", "missing-tag", "closure", "folder"] {
            let (f, mut a, d, _) = prepare(if change == "closure" { "pin" } else { "follow" });
            inspect_select(&mut a, &d);
            if late { tap(&mut a, KeyCode::Enter); }
            match change {
                "pending" => std::fs::write(f.0.join(format!(".hair-layout/dc-{}/pending.toml", d.placement_id)), b"[partial").unwrap(),
                "listing" => std::fs::write(f.0.join("added.txt"), b"genuine external listing change").unwrap(),
                "visitor" => a.pose.position = d.at,
                "page" => a.local.as_mut().unwrap().page = usize::MAX,
                "theme" => a.themes[0].source = f.0.join("different-theme.toml"),
                "missing-tag" => a.themes[0].decorations.entries.clear(),
                "folder" => {
                    let other = f.0.join("other-owned-folder"); std::fs::create_dir(&other).unwrap();
                    a.local.as_mut().unwrap().explorer = hair::local_explorer::Explorer::new(&other).unwrap();
                }
                "closure" => std::fs::write(f.0.join(format!(".hair-layout/ac-{}/asset.glb", d.closure_identity().unwrap())), b"invalid retained bytes").unwrap(),
                _ => unreachable!(),
            }
            let before = records(&f.0); let xattrs = attrs(&f.0);
            tap(&mut a, KeyCode::Enter);
            assert!(a.local_status.contains("New inspection required"), "{late}/{change}: {}", a.local_status);
            assert!(!a.toolbox_controls().iter().any(|(_,_,s)| s.contains("Confirm resume") || s.contains("Review (no writes)") || s.contains("Select attempt")));
            assert_eq!(records(&f.0), before); assert_eq!(attrs(&f.0), xattrs);
            println!("SLICE8C stale late={late} change={change} stage={:?} status={}", a.recovery_test_outcome(), a.local_status);
        }
    }
}
#[test]
fn slice8c_busy_lock_and_genuine_late_listing_page_changes_reach_actual_app_consumer() {
    for change in ["busy", "Review-listing", "Final-listing", "Final-page-loss"] {
        let (f, mut a, d, _) = prepare_at("follow", [0., 0.]);
        if change == "Final-page-loss" {
            for n in 0..12 { std::fs::write(f.0.join(format!("z{n:02}.txt")), b"owned page fixture").unwrap(); }
            tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::F5); tap(&mut a, KeyCode::PageDown); tap(&mut a, KeyCode::KeyP);
            assert_eq!(a.local.as_ref().unwrap().page, 1);
        }
        inspect_select(&mut a, &d); tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("frozen recovery review"), "{}", a.local_status);
        let before = records(&f.0);
        let stages = Rc::new(RefCell::new(vec![])); let seen = stages.clone(); let root = f.0.clone();
        a.recovery_test_probe(move |stage| {
            seen.borrow_mut().push(stage);
            // Real external filesystem edits ONLY, never a synthetic candidate or
            // fake geometry constraint inside the actual action validator.
            if (change == "Review-listing" && stage == ResumeStage::Review)
                || (change == "Final-listing" && stage == ResumeStage::Final) {
                std::fs::write(root.join("genuine-late.txt"), b"external change").unwrap();
            }
            if change == "Final-page-loss" && stage == ResumeStage::Final {
                for n in 0..12 { std::fs::remove_file(root.join(format!("z{n:02}.txt"))).unwrap(); }
            }
        });
        let lock = std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap();
        if change == "busy" { assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) }, 0); }
        tap(&mut a, KeyCode::Enter);
        if change == "busy" { assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_UN) }, 0); }
        let expected = if change == "busy" { vec![] } else if change.starts_with("Review") { vec![ResumeStage::Review] } else { vec![ResumeStage::Review, ResumeStage::Final] };
        assert_eq!(*stages.borrow(), expected);
        let outcome = a.recovery_test_outcome().unwrap().to_owned();
        assert!(outcome.starts_with("NotCommitted("), "{outcome}");
        if change == "Final-page-loss" { assert!(outcome.contains("out of range")); }
        for (p, value) in before { if !p.file_name().unwrap().to_string_lossy().starts_with('z') { assert_eq!(records(&f.0).get(&p), Some(&value)); } }
        // EX is released even after genuine final refusal.
        let i = store::inspect_placements(&a.local.as_ref().unwrap().explorer).unwrap(); drop(i);
        let stable = records(&f.0); tap(&mut a, KeyCode::Enter); assert_eq!(records(&f.0), stable);
        println!("SLICE8C genuine external={change} reached={:?} actual-outcome={outcome} lock-released=PASS", stages.borrow());
    }
}
#[test]
fn slice8c_unsupported_partial_foreign_and_unrelated_evidence_never_offer_disposition() {
    for kind in ["partial", "binary", "foreign", "unrelated", "orphan"] {
        let (f, mut a, d, _) = prepare(if kind == "orphan" { "pin" } else { "follow" });
        let pending = f.0.join(format!(".hair-layout/dc-{}/pending.toml", d.placement_id));
        match kind {
            "partial" => std::fs::write(&pending, b"version = ").unwrap(),
            "binary" => std::fs::write(&pending, [255, 0]).unwrap(),
            "foreign" => std::fs::write(pending.with_file_name("foreign"), b"preserved").unwrap(),
            "unrelated" => {
                // Explicit fixture transfer of a whole actual-writer unfinished
                // slot, NOT fabricated successful metadata or inferred pairing.
                let (other, _, second, _) = prepare("follow");
                let slot = format!(".hair-layout/dc-{}", second.placement_id);
                std::fs::rename(other.0.join(&slot), f.0.join(&slot)).unwrap();
            }
            "orphan" => { std::fs::remove_file(&pending).unwrap(); std::fs::remove_dir(pending.parent().unwrap()).unwrap(); }
            _ => unreachable!(),
        }
        let before = records(&f.0); let xattrs = attrs(&f.0);
        click_label(&mut a, "Inspect unfinished placements");
        for _ in 0..3 {
            assert!(!a.toolbox_controls().iter().any(|(_,_,s)| s.contains("Select attempt") || s.contains("Confirm resume") || s.contains("Review (no writes)")), "{kind}: {}", a.recovery_status());
            tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::PageDown);
        }
        assert_eq!(records(&f.0), before); assert_eq!(attrs(&f.0), xattrs);
        println!("SLICE8C unsupported={kind} inert=true status={}", a.recovery_status());
    }
}
#[test]
fn slice8c_outcome_ui_consumption_seam_preserves_qualifiers_and_never_retries() {
    // Labelled UI-consumption seam, NOT an exercised rename failure. Actual
    // rename uncertainty/sync outcomes are covered by the storage 8B regressions.
    for outcome in [Outcome::NotCommitted("definite seam".into()), Outcome::Ambiguous("rename seam".into()), Outcome::Committed { synced: false }] {
        let (f, mut a, d, _) = prepare("follow"); inspect_select(&mut a, &d); tap(&mut a, KeyCode::Enter);
        let before = records(&f.0);
        a.recovery_finish_outcome(outcome, 0, &d.placement_id);
        let status = a.local_status.clone(); let trace = a.recovery_test_outcome().unwrap().to_owned();
        for _ in 0..3 { tap(&mut a, KeyCode::Enter); assert_eq!(a.local_status, status); }
        assert!(!a.toolbox_controls().iter().any(|(_,_,s)| s.contains("Confirm resume") || s.contains("Select attempt")));
        tap(&mut a, KeyCode::KeyI); assert!(a.world.document().unwrap().contains(&status));
        assert_eq!(records(&f.0), before);
        println!("SLICE8C SYNTHETIC outcome-consumption={trace}; no claim of actual rename; qualifiers-full-I=PASS");
    }
}
