use super::*;

fn catalog(a: &mut App, root: &Path) {
    for name in ["chairDesk.glb", "desk.glb"] {
        std::fs::copy(a.root.join("packs").join(name), root.join(name)).unwrap();
    }
    let text = "version=3\nname='Palette test'\nroles=[]\n\
        [[decorations.assets]]\nid='chair-mesh'\nfile='chairDesk.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0.0,0.0,0.0]\n\
        [[decorations.assets]]\nid='desk-mesh'\nfile='desk.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0.0,0.0,0.0]\n\
        [[decorations.entries]]\nid='chair'\ntag='hair.seating.chair'\nasset='chair-mesh'\nname='Desk chair'\n\
        [[decorations.entries]]\nid='desk'\ntag='hair.furniture.desk'\nasset='desk-mesh'\nname='Writing desk'\n";
    std::fs::write(root.join("pack.toml"), text).unwrap();
    a.themes[0] = hair::theme::Theme::load_for_folder_scene(
        &root.join("pack.toml"),
        &mut Default::default(),
        &a.themes[0].host,
    )
    .unwrap();
}
#[test]
fn slice4a_app_b_selects_actual_active_theme_object_without_acquisition_or_writes() {
    let (f, mut a) = open_fixture("slice4a-select");
    let pack = fixture("slice4a-pack");
    catalog(&mut a, &pack.0);
    let host = a.themes[0].host.clone();
    tap(&mut a, KeyCode::KeyP);
    a.input.captured = true;
    tap(&mut a, KeyCode::KeyB);
    assert!(!a.input.captured);
    tap(&mut a, KeyCode::ArrowDown);
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("desk-mesh"), "{}", a.local_status);
    assert!(
        a.local_status.contains("hair.furniture.desk"),
        "{}",
        a.local_status
    );
    assert!(a.inventory.items().is_empty());
    assert!(!a.inventory_open);
    assert!(!f.0.join(".hair-layout").exists());
    assert_eq!(a.themes[0].host, host);
    assert_eq!(std::fs::read(f.0.join("A.txt")).unwrap(), b"A.txt");
}

fn click_palette(a: &mut App, c: place_mode_native::Control) {
    let (_, r, _) = a
        .palette_controls()
        .into_iter()
        .find(|(control, _, _)| *control == c)
        .unwrap();
    let frame = a.inventory_frame();
    a.pointer_moved(Some([
        f64::from((r[0] + r[2]) * 0.5) * f64::from(frame.physical[0]) / f64::from(frame.full[0]),
        f64::from((r[1] + r[3]) * 0.5) * f64::from(frame.physical[1]) / f64::from(frame.full[1]),
    ]));
    a.place_pointer(true);
    a.place_pointer(false);
}

fn assert_literal(mesh: &[hair::assets::Vertex], size: [u32; 2], at: [f32; 2], text: &str) {
    let template = hair::placement::toolbox::paint(
        size,
        &[],
        &[(
            [at[0] - 6.0, at[1] - 7.0, at[0] + 310.0, at[1] + 13.0],
            text,
        )],
        None,
    );
    let expected = &template[12..]; // omit only panel and button rectangle
    assert!(!expected.is_empty());
    assert!(
        mesh.windows(expected.len())
            .any(|part| bytemuck::cast_slice::<_, u8>(part)
                == bytemuck::cast_slice::<_, u8>(expected)),
        "literal text absent from actual App paint: {text}"
    );
}

#[test]
fn slice4a_maximum_identity_is_disclosed_and_stale_or_held_input_never_selects() {
    use place_mode_native::Control;
    let (_f, mut a) = open_fixture("slice4a-identities");
    let pack = fixture("slice4a-long-pack");
    catalog(&mut a, &pack.0);
    let mut value: toml::Value =
        toml::from_str(&std::fs::read_to_string(pack.0.join("pack.toml")).unwrap()).unwrap();
    let id = "c".repeat(48);
    let asset = "a".repeat(48);
    let tag = "t".repeat(96);
    value["decorations"]["assets"][0]["id"] = asset.clone().into();
    value["decorations"]["entries"][0]["asset"] = asset.clone().into();
    value["decorations"]["entries"][0]["id"] = id.clone().into();
    value["decorations"]["entries"][0]["tag"] = tag.clone().into();
    std::fs::write(pack.0.join("pack.toml"), toml::to_string(&value).unwrap()).unwrap();
    a.themes[0] = hair::theme::Theme::load_for_folder_scene(
        &pack.0.join("pack.toml"),
        &mut Default::default(),
        &a.themes[0].host,
    )
    .unwrap();
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyB);
    let (_, r, _) = a
        .palette_controls()
        .into_iter()
        .find(|(c, _, _)| *c == Control::Choice(0))
        .unwrap();
    a.pointer_moved(Some([f64::from(r[0] + 10.0), f64::from(r[1] + 10.0)]));
    a.place_pointer(true);
    tap(&mut a, KeyCode::ArrowDown); // invalidates pressed row before release
    a.place_pointer(false);
    assert!(a.place_decoration_selection().is_none());
    a.key(KeyCode::Enter, true, true).unwrap();
    assert!(a.place_decoration_selection().is_none());
    a.key(KeyCode::Enter, false, false).unwrap();
    tap(&mut a, KeyCode::ArrowUp);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.place_decoration_selection().unwrap().id, id);
    let lines = a.palette_lines();
    assert!(lines.len() <= 11);
    let joined = lines.join("");
    for field in [&id, &tag, &asset] {
        assert!(joined.contains(field));
    }
    let frame = a.inventory_frame();
    let mesh = a.place_ui();
    assert_eq!(a.editor_phase(), "Editing");
    let x = (frame.full[0] as f32 - 632.0).max(0.0) + 8.0;
    for (i, line) in lines.iter().enumerate() {
        // Retained source disclosure adds entry/closure rows before palette data.
        assert_literal(&mesh, frame.full, [x, 190.0 + i as f32 * 10.0], line);
    }
}

#[test]
fn slice4a_palette_pointer_keyboard_escape_and_empty_catalog_own_input() {
    use place_mode_native::Control;
    let (f, mut a) = open_fixture("slice4a-input");
    tap(&mut a, KeyCode::KeyP);
    a.input.captured = true;
    tap(&mut a, KeyCode::KeyB);
    for key in [
        KeyCode::KeyW,
        KeyCode::KeyX,
        KeyCode::KeyC,
        KeyCode::KeyG,
        KeyCode::KeyH,
        KeyCode::KeyJ,
        KeyCode::KeyK,
        KeyCode::Space,
    ] {
        tap(&mut a, key);
    }
    assert!(!a.input.forward && !a.input.captured);
    assert!(a.place_mode.as_ref().unwrap().edit.is_none());
    assert!(a.place_decoration_selection().is_none());
    a.pointer_moved(Some([20.0, 450.0]));
    a.place_pointer(true);
    a.place_pointer(false);
    assert!(!a.input.captured, "palette blocks world click-through");
    click_palette(&mut a, Control::Choice(1));
    assert_eq!(a.editor_phase(), "Editing");
    assert_eq!(a.place_decoration_selection().unwrap().asset, "office-desk");
    assert_eq!(a.place_decoration_selection().unwrap().id, "desk");
    tap(&mut a, KeyCode::Escape); // cancel Editing, then reopen the catalog
    assert_eq!(a.editor_phase(), "Toolbox");
    tap(&mut a, KeyCode::KeyB);
    click_palette(&mut a, Control::Exit);
    assert!(a.place_mode.is_none());
    assert!(!a.input.captured);
    assert!(a.inventory.items().is_empty());
    assert!(!f.0.join(".hair-layout").exists());

    let empty_pack = fixture("slice4a-empty");
    std::fs::write(
        empty_pack.0.join("pack.toml"),
        "version=2\nname='Legacy'\nroles=[]\n",
    )
    .unwrap();
    a.themes[0] = hair::theme::Theme::load_for_folder_scene(
        &empty_pack.0.join("pack.toml"),
        &mut Default::default(),
        &a.themes[0].host,
    )
    .unwrap();
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyB);
    assert!(a.local_status.contains("no selectable decoration"));
    for key in [
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Enter,
    ] {
        tap(&mut a, key);
    }
    assert!(a.place_decoration_selection().is_none());
    assert!(
        a.palette_controls()
            .iter()
            .all(|(c, _, _)| !matches!(c, Control::Choice(_)))
    );
    assert!(!a.place_ui().is_empty());
    tap(&mut a, KeyCode::Escape);
    assert!(a.place_mode.is_some());
    tap(&mut a, KeyCode::Escape);
    assert!(a.place_mode.is_none());
}

#[test]
fn slice4a_frozen_file_review_refuses_palette_and_keeps_exact_pending_assembly() {
    let (f, mut a) = open_fixture("slice4a-frozen");
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2.0, 0.0]);
    tap(&mut a, KeyCode::Enter);
    let frozen = a
        .place_mode
        .as_ref()
        .unwrap()
        .edit
        .as_ref()
        .unwrap()
        .reviewed_position()
        .unwrap()
        .clone();
    tap(&mut a, KeyCode::KeyB);
    assert!(a.local_status.contains("frozen"));
    assert_eq!(
        a.place_mode
            .as_ref()
            .unwrap()
            .edit
            .as_ref()
            .unwrap()
            .reviewed_position()
            .unwrap(),
        &frozen
    );
    assert!(a.palette_controls().len() > 1); // real catalog exists, not an empty-pack refusal
    assert!(a.place_decoration_selection().is_none());
    assert!(!f.0.join(".hair-layout").exists());
    tap(&mut a, KeyCode::Escape);
    assert_eq!(a.editor_phase(), "Editing");
    assert!(!a.place_mode.as_ref().unwrap().edit.as_ref().unwrap().confirming());
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.place_decoration_selection().unwrap().id, "chair");
    assert_eq!(a.editor_phase(), "Editing", "catalog Enter must not position the former file");
    assert!(a.place_mode.as_ref().unwrap().edit.is_none());
    assert!(!f.0.join(".hair-layout").exists());
}

#[test]
fn slice4a_metadata_selection_does_not_read_repair_or_overwrite_malformed_layout() {
    let (f, mut a) = open_fixture("slice4a-malformed");
    std::fs::write(f.0.join(".hair-layout"), b"foreign malformed layout bytes").unwrap();
    let original_ino = std::fs::metadata(f.0.join(".hair-layout")).unwrap().ino();
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::ArrowDown);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.place_decoration_selection().unwrap().id, "desk");
    tap(&mut a, KeyCode::Enter);
    assert_eq!(
        std::fs::read(f.0.join(".hair-layout")).unwrap(),
        b"foreign malformed layout bytes"
    );
    assert_eq!(
        std::fs::metadata(f.0.join(".hair-layout")).unwrap().ino(),
        original_ino
    );
}

#[test]
fn slice4a_context_ownership_focus_loss_and_actual_theme_switch_do_not_rebind_selection() {
    let (f, mut a) = open_fixture("slice4a-context");
    let theme = hair::theme::Theme::load_for_folder_scene(
        &a.root.join("packs/archive.toml"),
        &mut Default::default(),
        &a.themes[0].host,
    )
    .unwrap();
    a.themes.push(theme);
    let host = a.themes[0].host.clone();
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyB);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(
        a.place_decoration_selection().unwrap().asset,
        "office-chair"
    );
    a.focus_changed(false);
    assert!(a.place_mode.is_none());
    a.focus_changed(true);
    a.menu = Some(hair::presentation::Menu::settings());
    a.activate_menu(hair::presentation::Item::Theme);
    assert_eq!(a.theme_index, 1);
    a.activate_menu(hair::presentation::Item::Resume);
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyB);
    assert!(a.place_decoration_selection().is_none());
    tap(&mut a, KeyCode::Enter);
    assert_eq!(
        a.place_decoration_selection().unwrap().asset,
        "archive-closed"
    );
    assert_eq!(a.themes[1].host, host);
    for context in 0..5 {
        match context {
            0 => a.world.reading = true,
            1 => a.menu = Some(hair::presentation::Menu::settings()),
            2 => a.address = Some(String::new()),
            3 => a.chooser_pending = true,
            _ => a.chooser_active = true,
        }
        assert!(a.place_key(KeyCode::KeyB, true, false).is_none());
        let id = a.place_decoration_selection().unwrap().id.clone();
        a.pointer_moved(Some([900.0, 210.0]));
        a.place_pointer(true);
        a.place_pointer(false);
        assert_eq!(a.place_decoration_selection().unwrap().id, id);
        a.world.reading = false;
        a.menu = None;
        a.address = None;
        a.chooser_pending = false;
        a.chooser_active = false;
    }
    a.focus_changed(false);
    assert!(a.place_mode.is_none());
    assert!(!f.0.join(".hair-layout").exists());
}

#[test]
fn slice4a_small_scaled_palette_paginates_all_real_entries_and_exit_never_overlaps() {
    use place_mode_native::Control;
    let (f, mut a) = open_fixture("slice4a-small");
    let pack = fixture("slice4a-pages");
    catalog(&mut a, &pack.0);
    let mut value: toml::Value =
        toml::from_str(&std::fs::read_to_string(pack.0.join("pack.toml")).unwrap()).unwrap();
    let entries = value["decorations"]["entries"].as_array_mut().unwrap();
    for i in 2..32 {
        let mut entry = entries[i % 2].clone(); // each row references one of TWO REAL distinct meshes
        entry["id"] = format!("variant-{i:02}").into();
        entry["tag"] = format!("hair.variant-{i:02}").into();
        entry["name"] = format!("Variant {i:02}").into();
        entries.push(entry);
    }
    std::fs::write(pack.0.join("pack.toml"), toml::to_string(&value).unwrap()).unwrap();
    a.themes[0] = hair::theme::Theme::load_for_folder_scene(
        &pack.0.join("pack.toml"),
        &mut Default::default(),
        &a.themes[0].host,
    )
    .unwrap();
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    a.renderer = Some(
        pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [640, 480],
            &a.themes[0].vertices_for(&a.world),
        ))
        .unwrap(),
    );
    for size in [[640, 480], [1600, 1000]] {
        for scale in hair::presentation::STEPS {
            a.renderer.as_mut().unwrap().resize(size);
            a.set_ui_scale(scale);
            tap(&mut a, KeyCode::KeyP);
            tap(&mut a, KeyCode::KeyB);
            let mut seen = std::collections::BTreeSet::new();
            for _ in 0..40 {
                let controls = a.palette_controls();
                let frame = a.inventory_frame();
                for (i, (_, r, _)) in controls.iter().enumerate() {
                    assert!(
                        r[0] >= 0.0
                            && r[1] >= 70.0
                            && r[2] <= frame.full[0] as f32
                            && r[3] <= frame.full[1] as f32
                    );
                    for (_, other, _) in controls.iter().skip(i + 1) {
                        assert!(
                            r[2] <= other[0]
                                || other[2] <= r[0]
                                || r[3] <= other[1]
                                || other[3] <= r[1],
                            "palette controls overlap: {r:?} {other:?}"
                        );
                    }
                }
                let mesh = a.place_ui();
                assert!(
                    mesh.iter()
                        .all(|v| v.position[0].abs() <= 1.0 && v.position[1].abs() <= 1.0)
                );
                for (_, r, text) in &controls {
                    assert_literal(&mesh, frame.full, [r[0] + 6.0, r[1] + 7.0], text);
                }
                for (c, _, _) in &controls {
                    if let Control::Choice(i) = c {
                        click_palette(&mut a, *c);
                        let selected = a.place_decoration_selection().unwrap();
                        assert_eq!(selected, &a.themes[0].decorations.entries[*i]);
                        assert!(
                            !a.themes[0]
                                .decorations
                                .asset(&selected.asset)
                                .unwrap()
                                .model
                                .vertices
                                .is_empty()
                        );
                        seen.insert(selected.id.clone());
                        assert_eq!(a.editor_phase(), "Editing");
                        // A choice closes the catalog into Editing. Cancel and
                        // reopen via actual input, then return to the same page.
                        tap(&mut a, KeyCode::Escape);
                        assert_eq!(a.editor_phase(), "Toolbox");
                        tap(&mut a, KeyCode::KeyB);
                        for _ in 0..32 {
                            if a.palette_controls().iter().any(|(c, _, _)| *c == Control::Choice(*i)) { break; }
                            tap(&mut a, KeyCode::PageDown);
                        }
                        assert!(a.palette_controls().iter().any(|(c, _, _)| *c == Control::Choice(*i)));
                    }
                }
                if controls.iter().any(|(c, _, _)| *c == Control::Next) {
                    click_palette(&mut a, Control::Next);
                } else {
                    break;
                }
            }
            assert_eq!(seen.len(), 32);
            for _ in 0..40 {
                tap(&mut a, KeyCode::PageDown);
            }
            assert!(
                !a.palette_controls()
                    .iter()
                    .any(|(c, _, _)| *c == Control::Next)
            );
            for _ in 0..40 {
                tap(&mut a, KeyCode::PageUp);
            }
            assert!(
                !a.palette_controls()
                    .iter()
                    .any(|(c, _, _)| *c == Control::Previous)
            );
            click_palette(&mut a, Control::Exit);
            assert!(a.place_mode.is_none());
        }
    }
    assert!(a.inventory.items().is_empty());
    assert!(!f.0.join(".hair-layout").exists());
}
