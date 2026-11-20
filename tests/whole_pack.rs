use hair::authoring::Project;
use std::path::Path;
#[test]
fn legacy_v2_project_positive_control_and_status_negative_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-v2-control-{}", std::process::id()));
    std::fs::create_dir(&base).unwrap();
    std::fs::copy(root.join("assets/kenney/desk.glb"), base.join("model.glb")).unwrap();
    let text = "version=2\nasset='model.glb'\noriginal_source='retained'\n[edit]\npivot=[0,0,0]\nyaw_degrees=25.0\nscale=1.0\ndocument_role=false\n";
    std::fs::write(base.join("project.toml"), text).unwrap();
    let p = Project::open(&base.join("project.toml")).unwrap();
    assert!(!p.document_role);
    assert_eq!(p.yaw_degrees, 25.0);
    assert_eq!(
        std::fs::read_to_string(base.join("project.toml")).unwrap(),
        text
    );
    let pack = p.export(&base.join("pack"), &[root.into()]).unwrap();
    let world = hair::World::load_site(&root.join("examples/GreenleafReadingRoom-spaces")).unwrap();
    let theme = hair::theme::Theme::load_for_scene(
        &pack,
        &mut Default::default(),
        &world.spaces.as_ref().unwrap().spaces[0].scene,
    )
    .unwrap();
    let status = theme.feedback(&world);
    assert!(
        status.contains("NO MATCHING ROLES")
            && status.contains("HERE 0/3 HOST 0/9 UNMATCHED 1/1 FALLBACK 3/9"),
        "{status}"
    );
    for (width, height) in [(640, 480), (960, 640)] {
        let verts = hair::text::pack_overlay(&status, width, height);
        assert!(!verts.is_empty());
        assert!(
            verts
                .iter()
                .all(|v| v.position[0].abs() <= 1.0 && v.position[1].abs() <= 1.0)
        );
    }
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hair"))
        .env("HAIR_RESOURCE_ROOT", root)
        .arg("--pack")
        .arg(base.join("missing.toml"))
        .arg("--resource-report")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("PACK office | LOAD FAILED; FALLBACK"),
        "{stdout}"
    );
    std::fs::remove_dir_all(base).unwrap();
}

fn large_glb(color: &str) -> Vec<u8> {
    let mut bin = Vec::new();
    for v in [
        0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
    ] {
        bin.extend(v.to_le_bytes());
    }
    for i in 0..180000u32 {
        bin.extend((i % 3).to_le_bytes());
    }
    let mut json=format!(r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":720000}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5125,"count":180000,"type":"SCALAR"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[{color},1]}}}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2,"material":0}}]}}],"nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#,bin.len()).into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut glb = Vec::new();
    for n in [
        0x46546c67u32,
        2,
        (28 + json.len() + bin.len()) as u32,
        json.len() as u32,
        0x4e4f534a,
    ] {
        glb.extend(n.to_le_bytes());
    }
    glb.extend(json);
    glb.extend((bin.len() as u32).to_le_bytes());
    glb.extend(0x004e4942u32.to_le_bytes());
    glb.extend(bin);
    glb
}
#[test]
fn unmatched_models_still_count_against_whole_pack_decoded_budget() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-whole-budget-{}", std::process::id()));
    std::fs::create_dir(&base).unwrap();
    let mut manifest = "version=2\nname='aggregate test'\n".to_string();
    for (role, color) in [("document", "1,0,0"), ("decoration", "0,0,1")] {
        let bytes = large_glb(color);
        assert_eq!(
            hair::assets::decode(&bytes, 3.0).unwrap().vertices.len(),
            180000
        );
        std::fs::write(base.join(format!("{role}.glb")), bytes).unwrap();
        manifest.push_str(&format!("[[roles]]\nrole='{role}'\nfile='{role}.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0,0,0]\n"));
    }
    std::fs::write(base.join("pack.toml"), manifest).unwrap();
    let w = hair::World::load_site(&root.join("examples/GreenleafReadingRoom-spaces")).unwrap();
    let result = hair::theme::Theme::load_for_scene(
        &base.join("pack.toml"),
        &mut Default::default(),
        &w.spaces.unwrap().spaces[0].scene,
    );
    assert!(
        result.is_err(),
        "unmatched decoration must not bypass aggregate decoded model budget"
    );
    let unsupported = std::fs::read_to_string(base.join("pack.toml"))
        .unwrap()
        .replace("role='decoration'", "role='future-role'");
    std::fs::write(base.join("pack.toml"), unsupported).unwrap();
    let world = hair::World::load_site(&root.join("examples/GreenleafReadingRoom-spaces")).unwrap();
    let mut cache = hair::theme::AssetCache::default();
    for _ in 0..2 {
        assert!(
            hair::theme::Theme::load_for_scene(
                &base.join("pack.toml"),
                &mut cache,
                &world.spaces.as_ref().unwrap().spaces[0].scene
            )
            .is_err(),
            "unsupported declared model must count, including warm cache"
        );
    }
    std::fs::remove_dir_all(base).unwrap();
}
#[test]
fn visitor_reports_current_and_whole_host_application_totals() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hair"))
        .env("HAIR_RESOURCE_ROOT", root)
        .arg("--site")
        .arg(root.join("examples/GreenleafReadingRoom-appearance"))
        .arg("--resource-report")
        .output()
        .unwrap();
    assert!(out.status.success());
    let log = String::from_utf8_lossy(&out.stdout);
    assert!(
        log.contains("PACK_FEEDBACK") && log.contains("host=12"),
        "missing current/whole host totals: {log}"
    );
}

#[test]
fn new_greenleaf_decoration_envelopes_preserve_all_old_navigation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let old = hair::World::load_site(&root.join("examples/GreenleafReadingRoom-spaces")).unwrap();
    let new = hair::World::load_site(&root.join("examples/GreenleafReadingRoom-appearance"))
        .expect("new superseding decoration example must exist");
    for (a, b) in old
        .spaces
        .unwrap()
        .spaces
        .iter()
        .zip(new.spaces.unwrap().spaces.iter())
    {
        assert_eq!(a.scene.geometry, b.scene.geometry);
        assert_eq!(a.scene.room, b.scene.room);
        assert_eq!(a.scene.spawn, b.scene.spawn);
        assert_eq!(
            a.scene.anchors,
            b.scene
                .anchors
                .iter()
                .filter(|x| x.role != "decoration")
                .cloned()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            b.scene
                .anchors
                .iter()
                .filter(|a| a.role == "decoration")
                .count(),
            1
        );
        for x in -80..80 {
            for z in -80..80 {
                let p = [x as f32 / 16.0, z as f32 / 16.0];
                assert_eq!(
                    a.scene.blocked(p),
                    b.scene.blocked(p),
                    "new envelope changes clearance {p:?}"
                );
            }
        }
    }
}
#[test]
fn unmatched_loaded_roles_are_not_reported_as_applied() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let w = hair::World::load_site(&root.join("examples/GreenleafReadingRoom-spaces")).unwrap();
    let t = hair::theme::Theme::load_for_scene(
        &root.join("packs/office.toml"),
        &mut Default::default(),
        &w.spaces.unwrap().spaces[0].scene,
    )
    .unwrap();
    assert!(
        t.diagnostics
            .iter()
            .any(|d| d.contains("ROLE_UNMATCHED decoration")),
        "loaded decoration has no anchor and must be reported: {:?}",
        t.diagnostics
    );
}

#[test]
fn multi_role_save_open_retains_both_roles_and_deduplicates_bytes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-whole-save-{}", std::process::id()));
    std::fs::create_dir(&base).unwrap();
    let mut p = Project::import(&root.join("assets/kenney/desk.glb")).unwrap();
    p.update(hair::authoring::Edit {
        yaw_degrees: 30.0,
        ..p.edit()
    })
    .unwrap();
    p.update(hair::authoring::Edit {
        document_role: false,
        yaw_degrees: -45.0,
        ..p.edit()
    })
    .unwrap();
    let saved = p
        .save_new(&base.join("project"), &[root.to_path_buf()])
        .unwrap();
    let mut reopened = Project::open(&saved).unwrap();
    assert!(!reopened.document_role);
    assert_eq!(reopened.yaw_degrees, -45.0);
    reopened
        .update(hair::authoring::Edit {
            document_role: true,
            ..reopened.edit()
        })
        .unwrap();
    assert_eq!(
        reopened.yaw_degrees, 30.0,
        "saved project lost inactive role"
    );
    let pack = reopened
        .export(&base.join("pack"), &[root.to_path_buf()])
        .unwrap();
    let text = std::fs::read_to_string(pack).unwrap();
    assert_eq!(text.matches("[[roles]]").count(), 2);
    assert_eq!(
        std::fs::read_dir(base.join("pack"))
            .unwrap()
            .filter(|p| p
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|s| s == "glb"))
            .count(),
        1
    );
    let provenance = std::fs::read_to_string(base.join("pack/PROVENANCE.toml")).unwrap();
    assert!(
        toml::from_str::<toml::Value>(&provenance).is_ok(),
        "multi-role provenance must be valid TOML"
    );
    std::fs::remove_dir_all(base).unwrap();
}

#[cfg(feature = "workshop")]
#[test]
fn actual_egui_import_assigns_active_role_without_losing_other_model() {
    use hair::workshop::{Command, DialogKind, Workshop};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut app = Workshop::new(vec![root.to_path_buf()]);
    let ctx = egui::Context::default();
    fn click(ctx: &egui::Context, app: &mut Workshop, id: &str) {
        let raw = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            ..Default::default()
        };
        ctx.run_ui(raw(), |ui| app.ui(ui, None))
            .drop_without_applying_deltas();
        let pos = app.controls[id].center();
        for pressed in [true, false] {
            let mut input = raw();
            input.events = vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ];
            ctx.run_ui(input, |ui| app.ui(ui, None))
                .drop_without_applying_deltas();
        }
    }
    click(&ctx, &mut app, "import");
    assert_eq!(app.begin_dialog(), Some(DialogKind::Import));
    app.finish_dialog(Ok(Some(root.join("assets/kenney/desk.glb"))));
    click(&ctx, &mut app, "rotate-step");
    click(&ctx, &mut app, "decoration");
    click(&ctx, &mut app, "import");
    assert_eq!(app.begin_dialog(), Some(DialogKind::Import));
    app.finish_dialog(Ok(Some(root.join("assets/kenney/chairDesk.glb"))));
    if app.confirmation_pending() {
        app.dispatch(Command::ConfirmDiscard);
    }
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(
        !app.project.as_ref().unwrap().document_role,
        "import must target selected decoration role"
    );
    click(&ctx, &mut app, "document");
    assert_eq!(
        app.project.as_ref().unwrap().source(),
        root.join("assets/kenney/desk.glb")
    );
    assert_eq!(app.project.as_ref().unwrap().yaw_degrees, 15.0);
    click(&ctx, &mut app, "decoration");
    assert_eq!(
        app.project.as_ref().unwrap().source(),
        root.join("assets/kenney/chairDesk.glb")
    );
}

#[test]
fn switching_roles_retains_independent_models_edits_in_export() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut p = Project::import(&root.join("assets/kenney/desk.glb")).unwrap();
    let document = hair::authoring::Edit {
        yaw_degrees: 30.0,
        ..p.edit()
    };
    p.update(document).unwrap();
    p.update(hair::authoring::Edit {
        document_role: false,
        yaw_degrees: -45.0,
        ..document
    })
    .unwrap();
    p.update(hair::authoring::Edit {
        document_role: true,
        ..p.edit()
    })
    .unwrap();
    assert_eq!(
        p.yaw_degrees, 30.0,
        "switch back must restore document edits, not carry decoration rotation"
    );
    let preview = p.preview().unwrap();
    assert!(!preview.diagnostics.iter().any(|d|d.contains("ROLE_FALLBACK document")||d.contains("ROLE_FALLBACK decoration")),"both roles must coexist: {:?}",preview.diagnostics);
}
