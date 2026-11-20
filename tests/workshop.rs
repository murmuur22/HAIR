#![cfg(feature = "workshop")]
use hair::workshop::{Command, Workshop};
use std::path::Path;

#[test]
fn browse_import_requests_panel_instead_of_using_advanced_path() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut app = Workshop::new(vec![root.to_path_buf()]);
    app.input_path = root.join("assets/kenney/desk.glb").display().to_string();
    ui_click(&egui::Context::default(), &mut app, "import");
    assert!(
        app.project.is_none(),
        "browse must request a panel, not import a stale path"
    );
    assert!(app.error.is_none());
    assert_eq!(app.begin_dialog(), Some(hair::workshop::DialogKind::Import));
    assert_eq!(app.begin_dialog(), None, "request may only be claimed once");
}

#[test]
fn selected_import_routes_real_loader_and_cancel_is_inert() {
    use hair::workshop::DialogKind;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut app = Workshop::new(vec![root.to_path_buf()]);
    app.request_dialog(DialogKind::Import);
    assert_eq!(app.begin_dialog(), Some(DialogKind::Import));
    app.finish_dialog(Ok(Some(root.join("assets/kenney/desk.glb"))));
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.project.is_some() && app.dirty);
    let before = (
        app.revision,
        app.project.as_ref().unwrap().edit(),
        app.notice.clone(),
        app.input_path.clone(),
    );
    for kind in [
        DialogKind::Import,
        DialogKind::Open,
        DialogKind::SaveNew,
        DialogKind::Export,
    ] {
        app.request_dialog(kind);
        app.request_dialog(DialogKind::Import);
        assert_eq!(app.begin_dialog(), Some(kind));
        app.finish_dialog(Ok(None));
        assert_eq!(app.begin_dialog(), None);
        assert_eq!(
            (
                app.revision,
                app.project.as_ref().unwrap().edit(),
                app.notice.clone(),
                app.input_path.clone()
            ),
            before
        );
        assert!(app.dirty && !app.confirmation_pending());
    }
    app.request_dialog(DialogKind::Import);
    app.begin_dialog();
    app.finish_dialog(Ok(Some(root.join("Cargo.toml"))));
    assert!(app.error.as_ref().unwrap().contains(".glb"));
    assert_eq!(app.revision, before.0);
    app.request_dialog(DialogKind::Import);
    app.begin_dialog();
    app.finish_dialog(Err("injected panel failure".into()));
    assert!(
        app.error
            .as_ref()
            .unwrap()
            .contains("injected panel failure")
    );
    assert_eq!(app.revision, before.0);
}

#[test]
fn browse_save_new_then_open_round_trip_uses_folder_writer() {
    use hair::workshop::DialogKind;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-dialog-roundtrip-{}", std::process::id()));
    std::fs::create_dir(&base).unwrap();
    let mut app = Workshop::new(vec![root.to_path_buf()]);
    app.input_path = root.join("assets/kenney/desk.glb").display().to_string();
    app.dispatch(Command::Import);
    app.dispatch(Command::Edit(hair::workshop::Edit {
        yaw_degrees: 30.0,
        ..app.project.as_ref().unwrap().edit()
    }));
    let edit = app.project.as_ref().unwrap().edit();
    let ctx = egui::Context::default();
    ui_click(&ctx, &mut app, "save-new");
    assert_eq!(app.begin_dialog(), Some(DialogKind::SaveNew));
    assert_eq!(std::fs::read_dir(&base).unwrap().count(), 0);
    app.finish_dialog(Ok(Some(base.join("new project"))));
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(!app.dirty && base.join("new project/project.toml").is_file());
    assert!(base.join("new project/model.glb").is_file());
    app.dispatch(Command::Fit);
    ui_click(&ctx, &mut app, "open");
    assert_eq!(app.begin_dialog(), Some(DialogKind::Open));
    app.finish_dialog(Ok(Some(base.join("new project/project.toml"))));
    assert!(app.confirmation_pending() && app.dirty);
    app.request_dialog(DialogKind::Import);
    assert_eq!(
        app.begin_dialog(),
        None,
        "no panels during discard confirmation"
    );
    app.dispatch(Command::Cancel);
    assert!(app.dirty);
    ui_click(&ctx, &mut app, "open");
    assert_eq!(app.begin_dialog(), Some(DialogKind::Open));
    app.finish_dialog(Ok(Some(base.join("new project/project.toml"))));
    app.dispatch(Command::ConfirmDiscard);
    assert!(!app.dirty && app.project.as_ref().unwrap().edit() == edit);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn in_flight_panel_blocks_actions_and_existing_or_forbidden_outputs_refuse() {
    use hair::workshop::DialogKind;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-dialog-refusal-{}", std::process::id()));
    std::fs::create_dir(&base).unwrap();
    let mut app = Workshop::new(vec![root.to_path_buf()]);
    app.input_path = root.join("assets/kenney/desk.glb").display().to_string();
    app.dispatch(Command::Import);
    let revision = app.revision;
    app.request_dialog(DialogKind::SaveNew);
    app.begin_dialog();
    app.dispatch(Command::Fit);
    app.dispatch(Command::RequestQuit);
    assert_eq!(
        app.revision, revision,
        "no edits while a native request is in flight"
    );
    assert!(!app.confirmation_pending());
    app.finish_dialog(Ok(None));
    let sentinel = base.join("sentinel");
    std::fs::write(&sentinel, b"original").unwrap();
    for kind in [DialogKind::SaveNew, DialogKind::Export] {
        for path in [
            base.clone(),
            sentinel.clone(),
            root.join("private-inputs/forbidden-native"),
            base.join("absent-parent/new"),
        ] {
            app.request_dialog(kind);
            assert_eq!(app.begin_dialog(), Some(kind));
            app.finish_dialog(Ok(Some(path.clone())));
            assert!(app.error.is_some(), "must refuse {}", path.display());
            if path == base || path == sentinel || path == base.join("absent-parent/new") {
                assert!(
                    app.error.as_ref().unwrap().contains("existing parent"),
                    "{:?}",
                    app.error
                );
                assert!(
                    app.error.as_ref().unwrap().contains("NEW folder"),
                    "{:?}",
                    app.error
                );
            }
            assert_eq!(app.revision, revision);
            assert!(app.dirty);
        }
        app.request_dialog(kind);
        app.begin_dialog();
        app.finish_dialog(Ok(None));
        assert!(!base.join("absent-parent").exists());
    }
    assert_eq!(std::fs::read(sentinel).unwrap(), b"original");
    assert!(!root.join("private-inputs/forbidden-native").exists());
    assert_eq!(std::fs::read_dir(&base).unwrap().count(), 1);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn save_new_reopen_preserves_transforms_role_and_private_bytes() {
    use hair::workshop::Edit;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("assets/kenney/desk.glb");
    let before = std::fs::read(&source).unwrap();
    let base = std::env::temp_dir().join(format!("hair-ui-save-{}", std::process::id()));
    std::fs::create_dir(&base).unwrap();
    let mut app = Workshop::new(vec![root.to_path_buf()]);
    app.input_path = source.display().to_string();
    app.dispatch(Command::Import);
    app.dispatch(Command::Edit(Edit {
        at: [0.5, 0.2, -0.5],
        scale: 0.8,
        yaw_degrees: 45.0,
        solid: false,
        document_role: false,
    }));
    assert!(app.error.is_none(), "{:?}", app.error);
    let vertices = app.preview.as_ref().unwrap().vertices.clone();
    app.project_path = base.join("new-project").display().to_string();
    app.dispatch(Command::SaveNew);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(!app.dirty);
    assert_eq!(
        std::fs::read(base.join("new-project/model.glb")).unwrap(),
        before
    );
    let document = base.join("new-project/project.toml");
    let original = std::fs::read(&document).unwrap();
    app.dispatch(Command::SaveNew);
    assert!(app.error.is_some());
    assert_eq!(std::fs::read(&document).unwrap(), original);
    let mut reopened = Workshop::new(vec![root.to_path_buf()]);
    reopened.project_path = document.display().to_string();
    reopened.dispatch(Command::Open);
    assert!(reopened.error.is_none(), "{:?}", reopened.error);
    assert!(!reopened.dirty);
    let p = reopened.project.as_ref().unwrap();
    assert_eq!(p.at, [0.5, 0.2, -0.5]);
    assert_eq!(p.yaw_degrees, 45.0);
    assert!(!p.document_role);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&vertices),
        bytemuck::cast_slice::<_, u8>(&reopened.preview.as_ref().unwrap().vertices)
    );
    reopened.export_path = base.join("pack").display().to_string();
    reopened.dispatch(Command::Export);
    assert!(reopened.error.is_none(), "{:?}", reopened.error);
    let theme =
        hair::theme::Theme::load(&base.join("pack/pack.toml"), &mut Default::default()).unwrap();
    let host = hair::host_scene::HostScene::shared_default();
    let document = host.anchor("document");
    assert_eq!(theme.document, [document.at[0], document.at[2]]);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&vertices),
        bytemuck::cast_slice::<_, u8>(&theme.vertices)
    );
    assert_eq!(std::fs::read(&source).unwrap(), before);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn rejected_edits_and_outputs_preserve_state_and_exit_requires_discard() {
    use hair::workshop::Edit;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut app = Workshop::new(vec![root.to_path_buf()]);
    app.input_path = root.join("assets/kenney/desk.glb").display().to_string();
    app.dispatch(Command::Import);
    let old = app.project.as_ref().unwrap().edit();
    let base = std::env::temp_dir().join(format!("hair-ui-refuse-{}", std::process::id()));
    std::fs::create_dir(&base).unwrap();
    app.dispatch(Command::Edit(Edit {
        scale: f32::NAN,
        ..old
    }));
    assert!(app.error.is_some());
    assert_eq!(app.project.as_ref().unwrap().edit(), old);
    app.project_path = root
        .join("private-inputs/must-not-create")
        .display()
        .to_string();
    app.dispatch(Command::SaveNew);
    assert!(app.error.is_some());
    assert!(!Path::new(&app.project_path).exists());
    app.export_path = base.display().to_string();
    app.dispatch(Command::Export);
    assert!(app.error.is_some());
    assert_eq!(std::fs::read_dir(&base).unwrap().count(), 0);
    app.project_path = base.join("malformed.toml").display().to_string();
    std::fs::write(&app.project_path, "version=999\nasset='../escape.glb'\n").unwrap();
    app.dispatch(Command::Open);
    assert!(app.error.is_some());
    assert_eq!(app.project.as_ref().unwrap().edit(), old);
    app.dispatch(Command::RequestQuit);
    assert!(!app.should_close);
    assert!(app.confirmation_pending());
    app.dispatch(Command::Cancel);
    assert!(!app.confirmation_pending());
    app.dispatch(Command::Import);
    assert!(app.confirmation_pending());
    app.dispatch(Command::Cancel);
    assert_eq!(app.project.as_ref().unwrap().edit(), old);
    app.dispatch(Command::Fit);
    assert!(app.error.is_none());
    app.dispatch(Command::Reset);
    assert_eq!(app.project.as_ref().unwrap().at, [0.0; 3]);
    app.dispatch(Command::RequestQuit);
    app.dispatch(Command::ConfirmDiscard);
    assert!(app.should_close);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn real_egui_clicks_route_import_purpose_rotation_export_and_safe_escape() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-egui-actions-{}", std::process::id()));
    std::fs::create_dir(&base).unwrap();
    let ctx = egui::Context::default();
    let mut app = Workshop::new(vec![root.to_path_buf()]);
    app.input_path = root.join("assets/kenney/desk.glb").display().to_string();
    ui_click(&ctx, &mut app, "import");
    assert_eq!(app.begin_dialog(), Some(hair::workshop::DialogKind::Import));
    app.finish_dialog(Ok(Some(Path::new(&app.input_path).to_path_buf())));
    assert!(app.project.is_some(), "{:?}", app.error);
    ui_click(&ctx, &mut app, "decoration");
    assert!(!app.project.as_ref().unwrap().document_role);
    ui_click(&ctx, &mut app, "rotate-step");
    assert_eq!(app.project.as_ref().unwrap().yaw_degrees, 15.0);
    ui_click(&ctx, &mut app, "fit");
    app.export_path = base.join("pack").display().to_string();
    ui_click(&ctx, &mut app, "export");
    assert_eq!(app.begin_dialog(), Some(hair::workshop::DialogKind::Export));
    app.finish_dialog(Ok(Some(base.join("pack"))));
    assert!(!base.join("pack/project.toml").exists());
    assert!(base.join("pack/pack.toml").is_file(), "{:?}", app.error);
    assert!(
        app.dirty,
        "export must not pretend editable state was saved"
    );
    let mut input = raw_input();
    input.events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    });
    ctx.run_ui(input, |ctx| app.ui(ctx, None))
        .drop_without_applying_deltas();
    assert!(app.confirmation_pending());
    assert!(!app.should_close);
    ui_click(&ctx, &mut app, "cancel-discard");
    assert!(!app.should_close);
    std::fs::remove_dir_all(base).unwrap();
}

fn raw_input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 900.0),
        )),
        ..Default::default()
    }
}
fn ui_click(ctx: &egui::Context, app: &mut Workshop, id: &str) {
    ctx.run_ui(raw_input(), |ctx| app.ui(ctx, None))
        .drop_without_applying_deltas();
    let point = app.controls[id].center();
    for pressed in [true, false] {
        let mut input = raw_input();
        input.events = vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            },
        ];
        ctx.run_ui(input, |ctx| app.ui(ctx, None))
            .drop_without_applying_deltas();
    }
}

#[test]
#[ignore = "real paired executable diagnostic with private local texture; not native/human input"]
fn actual_workshop_executable_offscreen_save_reopen_export() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-exe-proof-{}", std::process::id()));
    std::fs::create_dir(&base).unwrap();
    let out = base.join("captures");
    std::fs::create_dir(&out).unwrap();
    let p = std::process::Command::new(env!("CARGO_BIN_EXE_hair-workshop"))
        .env("HAIR_RESOURCE_ROOT", root)
        .arg("--offscreen-proof")
        .arg(root.join("private-inputs/planet.glb"))
        .arg(base.join("project"))
        .arg(base.join("pack"))
        .arg(&out)
        .output()
        .unwrap();
    println!(
        "{}{}",
        String::from_utf8_lossy(&p.stdout),
        String::from_utf8_lossy(&p.stderr)
    );
    assert!(p.status.success());
    assert!(String::from_utf8_lossy(&p.stdout).contains("WORKSHOP_OFFSCREEN_PASS"));
    assert!(out.join("office.png").is_file());
    assert!(base.join("project/project.toml").is_file());
    assert!(base.join("pack/pack.toml").is_file());
}

#[test]
fn offscreen_diagnostic_rejects_input_overlap_before_mutation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-ui-diagnostic-{}", std::process::id()));
    std::fs::create_dir(&base).unwrap();
    let p = std::process::Command::new(env!("CARGO_BIN_EXE_hair-workshop"))
        .env("HAIR_RESOURCE_ROOT", root)
        .arg("--offscreen-proof")
        .arg(root.join("assets/kenney/desk.glb"))
        .arg(base.join("project"))
        .arg(base.join("pack"))
        .arg(root.join("assets/kenney"))
        .output()
        .unwrap();
    assert!(!p.status.success());
    assert!(
        String::from_utf8_lossy(&p.stderr).contains("overlap"),
        "{}",
        String::from_utf8_lossy(&p.stderr)
    );
    assert_eq!(std::fs::read_dir(&base).unwrap().count(), 0);
    std::fs::remove_dir(base).unwrap();
}

#[test]
fn separate_native_executable_has_truthful_help_and_version() {
    let version = std::process::Command::new(env!("CARGO_BIN_EXE_hair-workshop"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        concat!("HAIR Workshop ", env!("CARGO_PKG_VERSION"))
    );
    let help = std::process::Command::new(env!("CARGO_BIN_EXE_hair-workshop"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("Save NEW copy")
    );
}

#[test]
fn save_refuses_non_roundtrippable_transform_before_creating_directory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut p = hair::authoring::Project::import(&root.join("assets/kenney/desk.glb")).unwrap();
    let target = std::env::temp_dir().join(format!("hair-save-invalid-{}", std::process::id()));
    p.yaw_degrees = 720.0;
    assert!(p.save_new(&target, &[root.to_path_buf()]).is_err());
    assert!(
        !target.exists(),
        "invalid project must not publish a directory"
    );
}

#[test]
fn path_text_input_is_not_camera_input() {
    let ctx = egui::Context::default();
    let mut app = Workshop::new(vec![]);
    ctx.run_ui(raw_input(), |ui| app.ui(ui, None))
        .drop_without_applying_deltas();
    assert!(
        !app.controls.contains_key("import-path"),
        "normal browsing must not show mandatory path fields"
    );
    ui_click(&ctx, &mut app, "advanced-paths");
    ui_click(&ctx, &mut app, "import-path");
    let before = app.inspection;
    let mut input = raw_input();
    input
        .events
        .push(egui::Event::Text("wasd /a/model.glb".into()));
    ctx.run_ui(input, |ui| app.ui(ui, None))
        .drop_without_applying_deltas();
    assert_eq!(app.input_path, "wasd /a/model.glb");
    assert_eq!(app.inspection.position, before.position);
    assert_eq!(app.inspection.yaw, before.yaw);
}

#[test]
fn import_action_keeps_last_good_dirty_project_on_failure() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut app = Workshop::new(vec![root.to_path_buf()]);
    app.input_path = root.join("assets/kenney/desk.glb").display().to_string();
    app.dispatch(Command::Import);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.dirty);
    let vertices = app.preview.as_ref().unwrap().vertices.len();
    let scale = app.project.as_ref().unwrap().scale;
    app.input_path = root.join("assets/kenney/absent.glb").display().to_string();
    app.dispatch(Command::Import);
    assert!(app.error.is_some());
    assert_eq!(app.project.as_ref().unwrap().scale, scale);
    assert_eq!(app.preview.as_ref().unwrap().vertices.len(), vertices);
    assert!(app.dirty);
    app.dispatch(Command::AcknowledgeError);
    assert!(app.error.is_none());
}
