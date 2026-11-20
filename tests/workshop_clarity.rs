#![cfg(feature = "workshop")]
use hair::workshop::{Command, DialogKind, Workshop};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn raw() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 900.0),
        )),
        ..Default::default()
    }
}
fn click(ctx: &egui::Context, app: &mut Workshop, id: &str) {
    for _ in 0..3 {
        ctx.run_ui(raw(), |ui| app.ui(ui, None))
            .drop_without_applying_deltas();
    }
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
fn select(
    ctx: &egui::Context,
    app: &mut Workshop,
    id: &str,
    kind: DialogKind,
    path: Option<PathBuf>,
) {
    click(ctx, app, id);
    assert_eq!(app.begin_dialog(), Some(kind));
    assert_eq!(app.begin_dialog(), None);
    app.finish_dialog(Ok(path));
}
fn fixture(label: &str) -> (PathBuf, egui::Context, Workshop, PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-clarity-{label}-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    let input = base.join("Downloads");
    fs::create_dir(&input).unwrap();
    let model = input.join("sample.glb");
    fs::copy(root.join("assets/kenney/desk.glb"), &model).unwrap();
    let ctx = egui::Context::default();
    let mut app = Workshop::new(vec![root.to_path_buf()]);
    select(
        &ctx,
        &mut app,
        "import",
        DialogKind::Import,
        Some(model.clone()),
    );
    assert!(app.error.is_none(), "{:?}", app.error);
    (base, ctx, app, model)
}

#[test]
fn saving_wrong_parent_explains_actual_conflict_and_retry_keeps_last_good_state() {
    let (base, ctx, mut app, model) = fixture("save");
    let parent = model.parent().unwrap();
    let before = (
        app.project_path.clone(),
        app.export_path.clone(),
        app.input_path.clone(),
        app.revision,
        app.dirty,
        app.project.as_ref().unwrap().edit(),
        app.protected.clone(),
    );
    select(
        &ctx,
        &mut app,
        "save-new",
        DialogKind::SaveNew,
        Some(parent.to_path_buf()),
    );
    let e = app.error.as_ref().expect("ancestor output must refuse");
    assert!(
        e.contains("Saving editable project"),
        "action-specific error missing: {e}"
    );
    assert!(
        e.contains(&parent.display().to_string()),
        "selected destination missing: {e}"
    );
    assert!(
        e.contains(&model.display().to_string()),
        "actual conflict missing: {e}"
    );
    assert!(
        e.contains("outside") && e.contains("NEW folder"),
        "recovery missing: {e}"
    );
    assert!(
        !app.notice.contains("Saved"),
        "refusal cannot announce success"
    );
    assert_eq!(
        (
            app.project_path.clone(),
            app.export_path.clone(),
            app.input_path.clone(),
            app.revision,
            app.dirty,
            app.project.as_ref().unwrap().edit(),
            app.protected.clone()
        ),
        before
    );
    let failure = app.error.clone();
    select(&ctx, &mut app, "save-new", DialogKind::SaveNew, None);
    assert_eq!(app.error, failure);
    assert_eq!(app.project_path, before.0);
    let target = parent.join("HAIR Project");
    select(
        &ctx,
        &mut app,
        "save-new",
        DialogKind::SaveNew,
        Some(target.clone()),
    );
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(!app.dirty);
    assert_eq!(
        fs::read(target.join("model.glb")).unwrap(),
        fs::read(&model).unwrap()
    );
    assert!(hair::authoring::Project::open(&target.join("project.toml")).is_ok());
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn typed_save_and_panel_failures_name_action_and_selected_destination_without_mutation() {
    let (base, ctx, mut app, model) = fixture("typed");
    app.project_path = base.join("missing-parent/new").display().to_string();
    let before = (
        app.project_path.clone(),
        app.revision,
        app.project.as_ref().unwrap().edit(),
    );
    app.dispatch(Command::SaveNew);
    let e = app.error.as_ref().unwrap();
    assert!(
        e.contains("Saving editable project") && e.contains(&before.0),
        "{e}"
    );
    assert_eq!(
        (
            app.project_path.clone(),
            app.revision,
            app.project.as_ref().unwrap().edit()
        ),
        before
    );
    click(&ctx, &mut app, "export");
    assert_eq!(app.begin_dialog(), Some(DialogKind::Export));
    app.finish_dialog(Err("injected native panel unavailable".into()));
    assert!(
        app.error
            .as_ref()
            .unwrap()
            .contains("Exporting appearance pack")
    );
    assert!(app.dirty && app.saved_project.is_none() && app.exported_pack.is_none());
    let input = app.input_path.clone();
    select(
        &ctx,
        &mut app,
        "import",
        DialogKind::Import,
        Some(model.with_extension("bad")),
    );
    assert_eq!(app.input_path, input);
    select(
        &ctx,
        &mut app,
        "open",
        DialogKind::Open,
        Some(base.join("pack.toml")),
    );
    assert_eq!(app.project_path, before.0);
    assert!(!app.confirmation_pending());
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn selected_existing_symlink_and_protected_outputs_preserve_sentinels_and_paths() {
    let (base, ctx, mut app, model) = fixture("matrix");
    let protected = base.join("Read Only Resources");
    fs::create_dir(&protected).unwrap();
    let keep = protected.join("keep.txt");
    fs::write(&keep, b"RESOURCE SENTINEL").unwrap();
    app.protected.push(protected.clone());
    let existing = base.join("Existing Folder");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("keep.txt"), b"OUTPUT SENTINEL").unwrap();
    let existing_file = base.join("Existing File");
    fs::write(&existing_file, b"FILE SENTINEL").unwrap();
    let alias = base.join("Resources Alias");
    std::os::unix::fs::symlink(&protected, &alias).unwrap();
    let dangling = base.join("Dangling Output");
    std::os::unix::fs::symlink(base.join("absent"), &dangling).unwrap();
    let before = (
        app.project_path.clone(),
        app.export_path.clone(),
        app.revision,
        app.dirty,
        app.project.as_ref().unwrap().edit(),
        app.protected.clone(),
        bytemuck::cast_slice::<_, u8>(&app.preview.as_ref().unwrap().vertices).to_vec(),
    );
    for (id, kind, action) in [
        ("save-new", DialogKind::SaveNew, "Saving editable project"),
        ("export", DialogKind::Export, "Exporting appearance pack"),
    ] {
        for target in [
            base.clone(),
            protected.clone(),
            protected.join("NEW"),
            alias.join("NEW"),
            existing.clone(),
            existing_file.clone(),
            alias.clone(),
            dangling.clone(),
        ] {
            select(&ctx, &mut app, id, kind, Some(target.clone()));
            let e = app.error.as_ref().unwrap();
            assert!(
                e.contains(action) && e.contains(&target.display().to_string()),
                "{e}"
            );
            assert!(e.contains("NEW folder"), "{e}");
            let text = visible_text(&ctx, &mut app);
            assert!(text.contains(action), "{text}");
            assert!(
                !text.contains("Exported appearance pack.")
                    && !text.contains("Saved NEW editable project;")
            );
            let rect = app.controls["error-message"];
            assert!(
                rect.min.x >= 0.0 && rect.max.x <= 1200.0,
                "unwrapped error: {rect:?}"
            );
            assert_eq!(
                (
                    app.project_path.clone(),
                    app.export_path.clone(),
                    app.revision,
                    app.dirty,
                    app.project.as_ref().unwrap().edit(),
                    app.protected.clone(),
                    bytemuck::cast_slice::<_, u8>(&app.preview.as_ref().unwrap().vertices).to_vec()
                ),
                before
            );
            let error = app.error.clone();
            select(&ctx, &mut app, id, kind, None);
            assert_eq!(app.error, error);
        }
    }
    assert!(!protected.join("NEW").exists());
    assert_eq!(fs::read(keep).unwrap(), b"RESOURCE SENTINEL");
    assert_eq!(
        fs::read(existing.join("keep.txt")).unwrap(),
        b"OUTPUT SENTINEL"
    );
    assert_eq!(fs::read(existing_file).unwrap(), b"FILE SENTINEL");
    assert_eq!(
        fs::read(&model).unwrap(),
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/kenney/desk.glb")).unwrap()
    );
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn cancelling_dirty_open_keeps_saved_and_exported_locations_and_selection_paths() {
    let (base, ctx, mut app, _) = fixture("discard");
    let first = base.join("First Project");
    let second = base.join("Second Project");
    select(
        &ctx,
        &mut app,
        "save-new",
        DialogKind::SaveNew,
        Some(first.clone()),
    );
    app.project
        .as_ref()
        .unwrap()
        .save_new(&second, &[])
        .unwrap();
    select(
        &ctx,
        &mut app,
        "export",
        DialogKind::Export,
        Some(base.join("First Pack")),
    );
    app.dispatch(Command::Fit);
    let before = (
        app.input_path.clone(),
        app.project_path.clone(),
        app.export_path.clone(),
        app.saved_project.clone(),
        app.exported_pack.clone(),
        app.revision,
    );
    select(
        &ctx,
        &mut app,
        "open",
        DialogKind::Open,
        Some(second.join("project.toml")),
    );
    assert!(app.confirmation_pending());
    click(&ctx, &mut app, "cancel-discard");
    assert!(!app.confirmation_pending());
    assert_eq!(
        (
            app.input_path.clone(),
            app.project_path.clone(),
            app.export_path.clone(),
            app.saved_project.clone(),
            app.exported_pack.clone(),
            app.revision
        ),
        before
    );
    assert!(app.dirty);
    select(
        &ctx,
        &mut app,
        "open",
        DialogKind::Open,
        Some(second.join("project.toml")),
    );
    click(&ctx, &mut app, "confirm-discard");
    assert_eq!(app.saved_project, Some(second.join("project.toml")));
    assert_eq!(
        app.project_path,
        second.join("project.toml").display().to_string()
    );
    assert!(app.exported_pack.is_none() && !app.dirty);
    fs::remove_dir_all(base).unwrap();
}

fn visible_text(ctx: &egui::Context, app: &mut Workshop) -> String {
    let out = ctx.run_ui(raw(), |ui| app.ui(ui, None));
    fn text(shape: &egui::epaint::Shape, out: &mut String) {
        match shape {
            egui::epaint::Shape::Text(t) => {
                out.push_str(&t.galley.job.text);
                out.push('\n');
            }
            egui::epaint::Shape::Vec(shapes) => {
                for s in shapes {
                    text(s, out);
                }
            }
            _ => {}
        }
    }
    let mut result = String::new();
    for s in &out.shapes {
        text(&s.shape, &mut result);
    }
    out.drop_without_applying_deltas();
    result
}

#[test]
fn export_refusal_then_sibling_retry_shows_separate_saved_and_exported_locations() {
    let (base, ctx, mut app, model) = fixture("export");
    let parent = model.parent().unwrap();
    let saved = parent.join("HAIR Project");
    select(
        &ctx,
        &mut app,
        "save-new",
        DialogKind::SaveNew,
        Some(saved.clone()),
    );
    assert!(app.error.is_none());
    app.dispatch(Command::Edit(hair::workshop::Edit {
        yaw_degrees: 30.0,
        ..app.project.as_ref().unwrap().edit()
    }));
    let before = (
        app.project_path.clone(),
        app.export_path.clone(),
        app.revision,
        app.protected.clone(),
    );
    select(
        &ctx,
        &mut app,
        "export",
        DialogKind::Export,
        Some(saved.clone()),
    );
    let e = app.error.as_ref().unwrap();
    assert!(
        e.contains("Exporting appearance pack") && e.contains(&saved.display().to_string()),
        "{e}"
    );
    assert!(e.contains("NEW folder") && e.contains("outside"), "{e}");
    assert_eq!(
        (
            app.project_path.clone(),
            app.export_path.clone(),
            app.revision,
            app.protected.clone()
        ),
        before
    );
    assert!(app.dirty);
    let text = visible_text(&ctx, &mut app);
    assert!(
        text.contains("Exporting appearance pack"),
        "error must be visible: {text}"
    );
    assert!(
        !text.contains("Saved NEW editable project:"),
        "stale success must not accompany refusal: {text}"
    );
    let pack = parent.join("HAIR Appearance Pack");
    select(
        &ctx,
        &mut app,
        "export",
        DialogKind::Export,
        Some(pack.clone()),
    );
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.dirty, "export is not a save");
    assert!(pack.join("pack.toml").is_file() && !pack.join("project.toml").exists());
    let text = visible_text(&ctx, &mut app);
    assert!(
        text.contains("Editable project: HAIR Project"),
        "saved location missing: {text}"
    );
    assert!(
        text.contains("Last exported pack: HAIR Appearance Pack"),
        "export location missing: {text}"
    );
    assert!(text.contains("Export does not save edits"), "{text}");
    let original = fs::read(pack.join("pack.toml")).unwrap();
    let before_path = app.export_path.clone();
    select(
        &ctx,
        &mut app,
        "export",
        DialogKind::Export,
        Some(pack.clone()),
    );
    let e = app.error.as_ref().unwrap();
    assert!(
        e.contains("different name") && e.contains(&pack.display().to_string()),
        "{e}"
    );
    assert_eq!(app.export_path, before_path);
    assert_eq!(fs::read(pack.join("pack.toml")).unwrap(), original);
    select(&ctx, &mut app, "export", DialogKind::Export, None);
    assert!(app.dirty);
    fs::remove_dir_all(base).unwrap();
}
