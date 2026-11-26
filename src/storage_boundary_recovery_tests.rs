//! Real pending writer evidence and actual released input, not injected outcomes.
use super::*;
use hair::placement_store::ResumeStage;
use std::os::unix::fs::PermissionsExt;

struct RestoreRoom { original: PathBuf, moved: PathBuf }
impl Drop for RestoreRoom {
    fn drop(&mut self) {
        if self.moved.exists() && !self.original.exists() {
            std::fs::rename(&self.moved, &self.original).unwrap();
        }
    }
}

fn relative_records(root: &Path) -> std::collections::BTreeMap<PathBuf, (u64, Vec<u8>)> {
    records(root).into_iter().map(|(p, v)| (p.strip_prefix(root).unwrap().to_path_buf(), v)).collect()
}
fn bootstrap(resources: &Path, room: &Path) -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut a = App::new(resources.into(), vec![theme], 0, World::load(&root).unwrap(), false);
    a.complete_folder(Ok(Some(room.into()))).unwrap();
    tap(&mut a, KeyCode::KeyP);
    a
}
#[test]
fn storage_boundary_recovery_late_reparent_refuses_at_every_authority_boundary() {
    for target in ["cache", "Resources"] {
        for phase in ["selection", "preview", "review", "confirm", "Review", "Final"] {
            let (f, old_app, d, _) = prepare("pin-relocation");
            drop(old_app);
            let roots = fixture("storage-boundary-recovery-roots");
            let resources = roots.0.join("Resources");
            let cache = roots.0.join("cache");
            std::fs::create_dir(&resources).unwrap(); std::fs::create_dir(&cache).unwrap();
            std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o700)).unwrap();
            let mut a = bootstrap(&resources, &f.0);
            a.restore_documents_at(&cache).unwrap();
            let cache_before = relative_records(&cache);
            if phase == "selection" {
                click_label(&mut a, "Inspect unfinished placements");
                for _ in 0..256 {
                    if a.recovery_status().contains(&format!("exact storage-local ID dc-{}", d.placement_id)) { break; }
                    tap(&mut a, KeyCode::PageDown);
                }
            } else { controls::inspect_select(&mut a, &d); }
            if matches!(phase, "confirm" | "Review" | "Final") {
                tap(&mut a, KeyCode::Enter);
                assert!(a.local_status.contains("frozen recovery review"), "{}", a.local_status);
            }
            let before = relative_records(&f.0);
            let moved = (if target == "cache" { &cache } else { &resources }).join("late-room");
            let _restore = RestoreRoom { original: f.0.clone(), moved: moved.clone() };
            if matches!(phase, "Review" | "Final") {
                let from = f.0.clone(); let to = moved.clone();
                a.recovery_test_probe(move |stage| {
                    if (phase == "Review" && stage == ResumeStage::Review) || (phase == "Final" && stage == ResumeStage::Final) {
                        std::fs::rename(&from, &to).unwrap();
                    }
                });
            } else { std::fs::rename(&f.0, &moved).unwrap(); }
            match phase {
                "selection" => click_label(&mut a, "Select attempt"),
                "preview" => assert!(a.place_preview().is_err(), "protected recovery overlay must refuse"),
                _ => tap(&mut a, KeyCode::Enter),
            }
            // An existing fail-safe may refuse before a requested callback.
            let actual = if moved.exists() { &moved } else { &f.0 };
            assert_eq!(relative_records(actual), before, "{target}/{phase}: {}", a.local_status);
            assert!(!a.local_status.contains("Recovery committed:"), "{}", a.local_status);
            if phase == "selection" { assert!(a.place_preview().is_err()); }
            if phase == "preview" { tap(&mut a, KeyCode::Enter); }
            assert_eq!(relative_records(actual), before);
            if moved.exists() { std::fs::rename(&moved, &f.0).unwrap(); }
            assert_eq!(relative_records(&cache), cache_before, "cache evidence unchanged");
            assert!(std::fs::read_dir(&resources).unwrap().next().is_none());
            println!("STORAGE-BOUNDARY recovery target={target} phase={phase} evidence/originals/cache=unchanged status={}", a.local_status);
        }
    }
}
#[test]
fn storage_boundary_recovery_legitimate_sibling_resumes_with_resources_rebound() {
    let (f, old_app, d, _) = prepare("pin-relocation"); drop(old_app);
    let roots = fixture("storage-boundary-recovery-positive");
    let resources = roots.0.join("Resources");
    std::fs::create_dir(&resources).unwrap();
    let mut a = bootstrap(&resources, &f.0);
    std::fs::rename(&resources, roots.0.join("held-Resources")).unwrap();
    std::fs::create_dir(&resources).unwrap();
    controls::inspect_select(&mut a, &d);
    tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert_eq!(a.recovery_test_outcome(), Some("Committed { synced: true }"));
    assert_eq!(layout(&a).decorations, vec![d]);
    assert_eq!(std::fs::read(f.0.join("A.txt")).unwrap(), b"A.txt");
    assert_eq!(std::fs::read(f.0.join("Z.txt")).unwrap(), b"Z.txt");
}
