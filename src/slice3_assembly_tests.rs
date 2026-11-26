use super::*;

fn assembly_labels(base: bool, collision: bool) -> [&'static str; 2] {
    [
        if base { "G: base ON" } else { "G: base OFF" },
        if !base {
            "H: base collision unavailable (base OFF)"
        } else if collision {
            "H: base collision ON"
        } else {
            "H: base collision OFF"
        },
    ]
}

fn saved_records(root: &std::path::Path) -> Vec<(PathBuf, Vec<u8>, u64)> {
    let Ok(entries) = std::fs::read_dir(root.join(".hair-layout")) else {
        return vec![];
    };
    let mut records = vec![];
    for entry in entries {
        let path = entry.unwrap().path().join("record.toml");
        if path.is_file() {
            records.push((
                path.clone(),
                std::fs::read(&path).unwrap(),
                std::fs::metadata(&path).unwrap().ino(),
            ));
        }
    }
    records.sort_by(|a, b| a.0.cmp(&b.0));
    records
}

fn assert_label_drawn(mesh: &[hair::assets::Vertex], size: [u32; 2], at: [f32; 2], label: &str, scale: f32) {
    // Public painter path: omit its panel/button rectangles, retain the literal
    // glyphs. Scaling about the top-left gives an independent full-text witness.
    let template = hair::placement::toolbox::paint(size, &[], &[(
        [at[0] / scale - 6., at[1] / scale - 7., at[0] / scale + 290., at[1] / scale + 13.], label,
    )], None);
    let mut expected = template[12..].to_vec();
    for v in &mut expected {
        v.position[0] = (v.position[0] + 1.) * scale - 1.;
        v.position[1] = 1. - (1. - v.position[1]) * scale;
    }
    assert!(!expected.is_empty());
    assert!(expected.iter().all(|v| v.position[0].abs() <= 1. && v.position[1].abs() <= 1.));
    assert!(mesh.windows(expected.len()).any(|part| {
        part.iter().zip(&expected).all(|(v, e)| v.color == e.color && v.position.into_iter().zip(e.position).all(|(a, b)| (a - b).abs() < 0.00001))
    }), "full label must actually be painted: {label}");
}

fn assert_toolbox_assembly(a: &mut App, base: bool, collision: bool) {
    use place_mode_native::Control;
    for (control, label) in [Control::Base, Control::Collision].into_iter().zip(assembly_labels(base, collision)) {
        reveal_control(a,control);
        let mesh = a.place_ui();
        let (_, r, actual) = a.toolbox_controls().into_iter().find(|(c, _, _)| *c == control)
            .expect("base and collision state remain accessible and painted, including review");
        assert_eq!(actual, label);
        assert_label_drawn(&mesh, a.inventory_frame().full, [r[0] + 6., r[1] + 3.], label, 1.);
    }
}

#[test]
fn slice3f_selected_assembly_states_persist_and_review_is_frozen() {
    use place_mode_native::Control;
    for base in [false, true] {
        for collision in [false, true] {
            let (f, mut a) = open_fixture("slice3f-selected-state");
            let ino = std::fs::metadata(f.0.join("A.txt")).unwrap().ino();
            aim_file(&mut a, "A.txt");
            tap(&mut a, KeyCode::KeyP);
            tap(&mut a, KeyCode::KeyX);
            begin_positioning(&mut a);
            aim_floor(&mut a, [2., 0.]);
            assert_toolbox_assembly(&mut a, false, true);
            click(&mut a, Control::Collision); // unavailable, not a hidden physics toggle
            click(&mut a, Control::Base);
            if !collision { click(&mut a, Control::Collision); }
            if !base { click(&mut a, Control::Base); }
            tap(&mut a, KeyCode::KeyJ);
            tap(&mut a, KeyCode::KeyK);
            assert_toolbox_assembly(&mut a, base, collision);
            tap(&mut a, KeyCode::Enter);
            let frozen = a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().reviewed_position().unwrap().clone();
            assert_eq!((frozen.properties.base, frozen.properties.base_collision), (base, collision));
            assert_toolbox_assembly(&mut a, base, collision);
            for code in [KeyCode::KeyG, KeyCode::KeyH, KeyCode::KeyJ, KeyCode::KeyK, KeyCode::KeyC, KeyCode::ArrowRight] {
                tap(&mut a, code);
            }
            click(&mut a, Control::Base);
            click(&mut a, Control::Collision);
            assert_eq!(a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().reviewed_position(), Some(&frozen));
            assert!(!f.0.join(".hair-layout").exists());
            tap(&mut a, KeyCode::Enter);
            assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
            let mut reopened = app();
            reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
            aim_file(&mut reopened, "A.txt");
            tap(&mut reopened, KeyCode::KeyP);
            tap(&mut reopened, KeyCode::KeyX);
            assert_toolbox_assembly(&mut reopened, base, collision);
            let before = saved_records(&f.0);
            begin_positioning(&mut reopened);
            aim_floor(&mut reopened, [3., 0.]);
            tap(&mut reopened, KeyCode::Enter);
            assert_toolbox_assembly(&mut reopened, base, collision);
            tap(&mut reopened, KeyCode::Escape);
            assert_eq!(saved_records(&f.0), before, "cancel preserves saved bytes and inodes");
            assert_eq!(std::fs::metadata(f.0.join("A.txt")).unwrap().ino(), ino);
            assert_eq!(std::fs::read(f.0.join("A.txt")).unwrap(), b"A.txt");
            assert_eq!(hair::player::blocked_at(&reopened.themes[0].host, [2., 0.], 0.), base && collision);
        }
    }
}

fn inventory_fixture() -> (Fixture, Fixture, App, u64) {
    let source = fixture("slice3f-inventory-source");
    let dest = fixture("slice3f-inventory-dest");
    std::fs::write(source.0.join("Placed.txt"), b"immutable source").unwrap();
    let ino = std::fs::metadata(source.0.join("Placed.txt")).unwrap().ino();
    let mut a = app();
    let explorer = hair::local_explorer::Explorer::new(&source.0).unwrap();
    a.inventory.collect(&explorer, 0).unwrap();
    a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
    aim_floor(&mut a, [2., 0.]);
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::KeyP);
    begin_positioning(&mut a);
    (source, dest, a, ino)
}

fn assert_inventory_assembly(a: &mut App, base: bool, collision: bool) {
    // Inventory now shares the real Place Mode panel, not the old bottom HUD.
    assert_toolbox_assembly(a, base, collision);
}

#[test]
fn slice3f_inventory_assembly_discloses_copy_move_review_and_reopened_states() {
    for move_item in [false, true] {
        for base in [false, true] {
            for collision in [false, true] {
                let (source, dest, mut a, ino) = inventory_fixture();
                assert_inventory_assembly(&mut a, false, true);
                tap(&mut a, KeyCode::KeyH); // base off: retains latent collision choice
                tap(&mut a, KeyCode::KeyG);
                if !collision { tap(&mut a, KeyCode::KeyH); }
                if !base { tap(&mut a, KeyCode::KeyG); }
                tap(&mut a, KeyCode::KeyJ);
                tap(&mut a, KeyCode::KeyK);
                if move_item { tap(&mut a, KeyCode::KeyV); }
                assert_inventory_assembly(&mut a, base, collision);
                tap(&mut a, KeyCode::Enter);
                let frozen = a.placement_action.as_ref().unwrap().pending.clone().unwrap();
                assert_eq!((frozen.properties.base, frozen.properties.base_collision), (base, collision));
                assert_inventory_assembly(&mut a, base, collision);
                for code in [KeyCode::KeyG, KeyCode::KeyH, KeyCode::KeyJ, KeyCode::KeyK, KeyCode::KeyC, KeyCode::KeyV, KeyCode::ArrowRight] {
                    tap(&mut a, code);
                }
                assert_eq!(a.placement_action.as_ref().unwrap().pending.as_ref(), Some(&frozen));
                assert_eq!(a.placement_action.as_ref().unwrap().properties, frozen.properties);
                assert!(!dest.0.join("Placed.txt").exists() && !dest.0.join(".hair-layout").exists());
                tap(&mut a, KeyCode::Enter);
                assert!(a.local_status.starts_with("Placed "), "{}", a.local_status);
                assert_eq!(a.inventory.items().len(), usize::from(!move_item));
                let mut reopened = app();
                reopened.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
                assert_eq!(hair::placement_store::read_layout(&reopened.local.as_ref().unwrap().explorer).unwrap().positions[0].1, frozen);
                aim_file(&mut reopened, "Placed.txt");
                tap(&mut reopened, KeyCode::KeyP);
                tap(&mut reopened, KeyCode::KeyX);
                assert_toolbox_assembly(&mut reopened, base, collision);
                assert_eq!(hair::player::blocked_at(&reopened.themes[0].host, [2., 0.], 0.), base && collision);
                assert_eq!(std::fs::read(source.0.join("Placed.txt")).unwrap(), b"immutable source");
                assert_eq!(std::fs::metadata(source.0.join("Placed.txt")).unwrap().ino(), ino);
                assert_eq!(std::fs::read(dest.0.join("Placed.txt")).unwrap(), b"immutable source");
            }
        }
    }
}

#[test]
fn slice3f_first_inventory_enter_checks_logical_file_and_base_not_only_art() {
    for base in [false, true] {
        for collision in [false, true] {
            let (_source, dest, mut a, _) = inventory_fixture();
            tap(&mut a, KeyCode::KeyG);
            if !collision { tap(&mut a, KeyCode::KeyH); }
            if !base { tap(&mut a, KeyCode::KeyG); }
            // Side obstacle outside the thin file art, inside the authoritative
            // envelope (and, with base ON, below the lifted file entirely).
            let g = hair::host_scene::Geometry { at: [2.3, 0., 0.12], size: [0.1, 0.4, 0.02] };
            a.themes[0].host.geometry.push(g);
            let position = hair::placement_store::Position {
                at: [2., 0.], yaw: 0., size: [0.8, 0.85, 0.28], allow_overlap: false,
                surface: Some(hair::placement::surface::Contact { at: [2., 0., 0.], normal: [0., 1., 0.] }),
                properties: a.placement_action.as_ref().unwrap().properties,
            };
            let art = position.transform(&a.inventory.preview(a.placement_action.as_ref().unwrap().item).unwrap().model.vertices).unwrap();
            assert!(hair::placement::surface::clearance(&a.themes[0].host, &art, hair::placement::Overlap::Prevent).is_ok());
            assert!(hair::placement::surface::clearance(&a.themes[0].host, &position.envelope_mesh().unwrap(), hair::placement::Overlap::Prevent).is_err());
            tap(&mut a, KeyCode::Enter);
            assert!(a.placement_action.as_ref().unwrap().pending.is_none(), "first Enter must reject the same logical assembly bounds as final clearance");
            assert!(a.local_status.contains("structural geometry"), "{}", a.local_status);
            tap(&mut a, KeyCode::Escape);
            assert!(!dest.0.join(".hair-layout").exists() && !dest.0.join("Placed.txt").exists());
            assert_eq!(a.inventory.items().len(), 1);
        }
    }
}

#[test]
fn slice3f_assembly_labels_and_controls_paint_at_small_window_scaling() {
    use place_mode_native::Control;
    let (_f, mut a) = open_fixture("slice3f-small-controls");
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    a.renderer = Some(pollster::block_on(render::Renderer::new(&adapter, wgpu::TextureFormat::Rgba8UnormSrgb, [640, 480], &a.themes[0].vertices_for(&a.world))).unwrap());
    for (physical, scale) in [([640, 480], 100), ([640, 480], 200), ([1600, 1000], 200)] {
        a.renderer.as_mut().unwrap().resize(physical);
        a.set_ui_scale(scale);
        for base in [false, true] {
            for collision in [false, true] {
                aim_file(&mut a, "A.txt");
                tap(&mut a, KeyCode::KeyP);
                tap(&mut a, KeyCode::KeyX);
                begin_positioning(&mut a);
                aim_floor(&mut a, [2., 0.]);
                click(&mut a, Control::Base);
                if !collision { click(&mut a, Control::Collision); }
                if !base { click(&mut a, Control::Base); }
                assert_toolbox_assembly(&mut a, base, collision);
                tap(&mut a, KeyCode::Enter);
                assert_toolbox_assembly(&mut a, base, collision);
                tap(&mut a, KeyCode::Escape);
                tap(&mut a, KeyCode::KeyP);
                let (_source, _dest, mut inventory, _) = inventory_fixture();
                inventory.renderer = a.renderer.take();
                inventory.set_ui_scale(scale);
                tap(&mut inventory, KeyCode::KeyG);
                if !collision { tap(&mut inventory, KeyCode::KeyH); }
                if !base { tap(&mut inventory, KeyCode::KeyG); }
                assert_inventory_assembly(&mut inventory, base, collision);
                tap(&mut inventory, KeyCode::Enter);
                assert_inventory_assembly(&mut inventory, base, collision);
                tap(&mut inventory, KeyCode::Escape);
                assert!(!_dest.0.join(".hair-layout").exists() && !_dest.0.join("Placed.txt").exists());
                a.renderer = inventory.renderer.take();
            }
        }
    }
}

#[test]
fn slice3f_both_pathways_revalidate_late_overlap_without_touching_prior_records() {
    for inventory in [false, true] {
        for base in [false, true] {
            for collision in [false, true] {
                let (source, dest, mut a, ino) = if inventory {
                    inventory_fixture()
                } else {
                    let (f, mut a) = open_fixture("slice3f-late-selected");
                    let ino = std::fs::metadata(f.0.join("A.txt")).unwrap().ino();
                    aim_file(&mut a, "A.txt");
                    tap(&mut a, KeyCode::KeyP);
                    tap(&mut a, KeyCode::KeyX);
                    begin_positioning(&mut a);
                    aim_floor(&mut a, [2., 0.]);
                    (fixture("slice3f-unused-source"), f, a, ino)
                };
                tap(&mut a, KeyCode::KeyG);
                if !collision { tap(&mut a, KeyCode::KeyH); }
                if !base { tap(&mut a, KeyCode::KeyG); }
                tap(&mut a, KeyCode::Enter);
                let frozen = if inventory {
                    a.placement_action.as_ref().unwrap().pending.clone().unwrap()
                } else {
                    a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().reviewed_position().unwrap().clone()
                };
                // Publish another file only after the real first Enter. Its
                // Allow policy must not silently opt this Prevent edit in.
                let mut competitor = frozen.clone();
                competitor.allow_overlap = true;
                let outcome = hair::placement_store::publish(
                    &a.local.as_ref().unwrap().explorer,
                    std::ffi::OsStr::new("Rival.txt"), b"late rival", &competitor,
                    &std::sync::atomic::AtomicBool::new(false),
                );
                assert!(matches!(outcome, hair::placement_store::Outcome::Committed { .. }), "{outcome:?}");
                let prior = saved_records(&dest.0);
                assert!(!prior.is_empty());
                let rival_ino = std::fs::metadata(dest.0.join("Rival.txt")).unwrap().ino();
                tap(&mut a, KeyCode::Enter);
                assert!(a.local_status.contains("overlap"), "{}", a.local_status);
                assert_eq!(saved_records(&dest.0), prior);
                assert_eq!(std::fs::metadata(dest.0.join("Rival.txt")).unwrap().ino(), rival_ino);
                assert_eq!(std::fs::read(dest.0.join("Rival.txt")).unwrap(), b"late rival");
                if inventory {
                    assert_eq!(a.inventory.items().len(), 1);
                    assert!(!dest.0.join("Placed.txt").exists());
                    assert_eq!(std::fs::metadata(source.0.join("Placed.txt")).unwrap().ino(), ino);
                    assert_eq!(std::fs::read(source.0.join("Placed.txt")).unwrap(), b"immutable source");
                } else {
                    assert_eq!(std::fs::metadata(dest.0.join("A.txt")).unwrap().ino(), ino);
                    assert_eq!(std::fs::read(dest.0.join("A.txt")).unwrap(), b"A.txt");
                }
                tap(&mut a, KeyCode::Escape);
                assert_eq!(saved_records(&dest.0), prior);
            }
        }
    }
}

#[test]
fn slice3_base_toggle_is_real_assembly_through_review_reopen_and_off() {
    let (f, mut a) = open_fixture("slice3-assembly");
    let source = std::fs::read(f.0.join("A.txt")).unwrap();
    let ino = std::fs::metadata(f.0.join("A.txt")).unwrap().ino();
    let unrelated = file_mesh(&a, "Z.txt");
    let furniture = base_mesh(&a, "Z.txt");
    assert!(!unrelated.is_empty(), "unplaced file visual is preserved");
    assert!(
        !furniture.is_empty(),
        "unplaced document-role visual is preserved"
    );
    assert!(
        a.themes[0].host.file_bases().is_empty(),
        "listing visuals do not imply physics bases"
    );
    let z_id = a.world.folder_view.as_ref().unwrap().bindings.iter()
        .find(|(_, b)| b.entry.name == std::ffi::OsStr::new("Z.txt")).unwrap().0;
    let z = a.themes[0].host.anchors.iter().find(|an| &an.id == z_id).unwrap();
    let z_at = [z.at[0], z.at[2]];
    assert!(!hair::player::blocked_at(&a.themes[0].host, z_at, 0.));
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]);
    let file_only = a.place_preview().unwrap();
    tap(&mut a, KeyCode::KeyG);
    let assembly = a.place_preview().unwrap();
    assert!(assembly.len() > file_only.len(), "G must add the actual document base to the preview");
    tap(&mut a, KeyCode::ArrowRight);
    tap(&mut a, KeyCode::Enter);
    assert!(!f.0.join(".hair-layout").exists(), "review is write-free");
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
    let moved_base = base_mesh(&a, "A.txt");
    assert!(!moved_base.is_empty());
    assert!(hair::player::blocked_at(&a.themes[0].host, [2., 0.], 0.));
    assert!(!hair::player::blocked_at(&a.themes[0].host, [-4.5, -3.], 0.), "dormant listing slot is not a collider");
    let mut reopened = app();
    reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(base_mesh(&reopened, "A.txt"), moved_base);
    assert_eq!(file_mesh(&reopened, "Z.txt"), unrelated);
    assert_eq!(base_mesh(&reopened, "Z.txt"), furniture);
    assert!(
        !hair::player::blocked_at(&reopened.themes[0].host, z_at, 0.),
        "unrelated listing stays non-solid after reopen"
    );
    aim_file(&mut reopened, "A.txt");
    tap(&mut reopened, KeyCode::KeyP);
    tap(&mut reopened, KeyCode::KeyX);
    begin_positioning(&mut reopened);
    aim_floor(&mut reopened, [3., 0.]);
    tap(&mut reopened, KeyCode::KeyG);
    tap(&mut reopened, KeyCode::Enter);
    tap(&mut reopened, KeyCode::Enter);
    assert!(reopened.local_status.contains("reposition saved"), "{}", reopened.local_status);
    assert!(base_mesh(&reopened, "A.txt").is_empty());
    assert!(!hair::player::blocked_at(&reopened.themes[0].host, [2., 0.], 0.));
    assert_eq!(std::fs::read(f.0.join("A.txt")).unwrap(), source);
    assert_eq!(std::fs::metadata(f.0.join("A.txt")).unwrap().ino(), ino);
}

#[test]
fn slice3_inventory_copy_and_move_base_preview_and_reopen() {
    for move_item in [false, true] {
        let source = fixture("slice3-inventory-source");
        let dest = fixture("slice3-inventory-dest");
        std::fs::write(source.0.join("Placed.txt"), b"original source").unwrap();
        let mut a = app();
        let e = hair::local_explorer::Explorer::new(&source.0).unwrap();
        a.inventory.collect(&e, 0).unwrap();
        a.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
        aim_floor(&mut a, [2., 0.]);
        tap(&mut a, KeyCode::KeyB);
        tap(&mut a, KeyCode::KeyP);
        begin_positioning(&mut a);
        let before = a.placement_preview().unwrap();
        tap(&mut a, KeyCode::KeyG);
        assert!(a.placement_preview().unwrap().len() > before.len(), "inventory needs the same real base choice");
        if move_item { tap(&mut a, KeyCode::KeyV); }
        tap(&mut a, KeyCode::Enter);
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.starts_with("Placed "), "{}", a.local_status);
        assert_eq!(a.inventory.items().len(), usize::from(!move_item));
        assert!(!base_mesh(&a, "Placed.txt").is_empty());
        let mut reopened = app();
        reopened.complete_folder(Ok(Some(dest.0.clone()))).unwrap();
        assert_eq!(base_mesh(&reopened, "Placed.txt"), base_mesh(&a, "Placed.txt"));
        assert!(hair::player::blocked_at(&reopened.themes[0].host, [2., 0.], 0.));
        assert_eq!(std::fs::read(source.0.join("Placed.txt")).unwrap(), b"original source");
    }
}
