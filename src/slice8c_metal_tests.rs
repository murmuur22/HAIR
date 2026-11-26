//! Explicit serialized offscreen Metal witness. Synthetic input, real App and art;
//! no physical/native/Retina/human/package qualification.
use super::*;
use super::super::recovery::{click_label, prepare};

#[test]
#[ignore = "explicit serialized real Metal; fresh absolute HAIR_SLICE8C_METAL_OUTPUT required; parent transfers/hash-matches/inspects"]
fn slice8c_real_metal_inspector_review_outcome_and_predecessor_witness() {
    let output = PathBuf::from(std::env::var_os("HAIR_SLICE8C_METAL_OUTPUT").expect("explicit unique output"));
    assert!(output.is_absolute());
    assert!(!output.starts_with(Path::new(env!("CARGO_MANIFEST_DIR"))));
    std::fs::create_dir(&output).unwrap(); // exclusive; never overwrite old evidence
    for size in [[1280, 960], [640, 480]] {
        let (f, mut a, d, old) = prepare("pin-relocation");
        let backend = attach(&mut a);
        a.renderer.as_mut().unwrap().resize(size);
        overview(&mut a);
        let before = records(&f.0);
        let scene = a.place_scene_at(0.);
        let prefix = format!("{}x{}", size[0], size[1]);
        // All UI boundaries from here are GPU-owned, with normal App input/HUD.
        click_label(&mut a, "Inspect unfinished placements");
        let inspected = shot(&mut a, &output, &format!("{prefix}-inspection"), &backend, &scene);
        for _ in 0..256 {
            if a.recovery_status().contains(&format!("exact storage-local ID dc-{}", d.placement_id)) { break; }
            tap(&mut a, KeyCode::PageDown);
        }
        click_label(&mut a, "Select attempt");
        let selected = shot(&mut a, &output, &format!("{prefix}-selected-proposed"), &backend, &scene);
        // Capture BOTH controls before asserting; failed frames remain available.
        let overlay = a.recovery_ui();
        let mut mesh = a.themes[0].decoration_preview(&d).unwrap();
        for v in &mut mesh { v.color = [0.55, 0.7, 0.95]; v.texture = 0; }
        let expected = hair::collection::Effect::new(&mesh, &a.pose, a.sidebar_frame()).unwrap().paint(0., false);
        let without = overlay[expected.len().min(overlay.len())..].to_vec();
        let missing = shot_with_overlay(&mut a, &output, &format!("{prefix}-missing-proposed-negative"), &backend, &scene, Some(&without));
        assert_eq!(mesh_bytes(&overlay[..expected.len()]), mesh_bytes(&expected), "real selected authored projection, not generic box");
        assert!(changed_world_pixels_at(&selected, &missing, size) > 0, "authored overlay must affect world-only ROI");
        assert_ne!(inspected, selected);
        assert_eq!(a.themes[0].host.decorations.get(&d.placement_id), old.as_ref());
        let old = old.as_ref().unwrap();
        let omitted = a.themes[0].vertices_for_at_omitting_decoration(&a.world, 0., Some(&old.placement_id));
        a.renderer.as_mut().unwrap().set_scene(&omitted); // NEGATIVE ONLY
        let missing_old = shot(&mut a, &output, &format!("{prefix}-missing-predecessor-negative"), &backend, &omitted);
        assert!(changed_world_pixels_at(&selected, &missing_old, size) > 0, "old predecessor remains rendered until commit");
        a.place_scene_refresh(); // restore via actual App scene consumer
        tap(&mut a, KeyCode::Enter);
        shot(&mut a, &output, &format!("{prefix}-frozen-review"), &backend, &scene);
        assert!(a.local_status.contains("frozen recovery review"), "{}", a.local_status);
        assert_eq!(records(&f.0), before);
        tap(&mut a, KeyCode::Enter);
        let committed_scene = a.place_scene_at(0.);
        shot(&mut a, &output, &format!("{prefix}-committed-outcome"), &backend, &committed_scene);
        assert!(a.local_status.contains("committed") && a.local_status.contains("refresh succeeded"));
        assert_eq!(layout(&a).decorations, vec![d.clone()]);
        tap(&mut a, KeyCode::KeyI);
        shot(&mut a, &output, &format!("{prefix}-full-outcome-I"), &backend, &committed_scene);
        assert!(a.world.document().unwrap().contains("committed"));
        a.renderer.as_ref().unwrap().wait().unwrap();
        println!("SLICE8C_METAL viewport={size:?} selected-art=real predecessor=visible-until-commit review=write-free outcome=Committed(synced) input=synthetic visual-acceptance=PENDING-parent");
    }
}
