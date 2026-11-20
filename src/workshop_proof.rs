//! Finite qualification of the real executable's UI/actions/shared GPU path.
//! Synthetic egui events, not native OS input or human qualification.
use hair::{
    Result, render, resources,
    workshop::{DialogKind, Workshop, WorkshopGpu},
};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[path = "workshop_whole_proof.rs"]
mod whole;

fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn future(path: &Path) -> Result<PathBuf> {
    require(path.is_absolute(), "diagnostic paths must be absolute")?;
    require(
        !path.exists() && !path.is_symlink(),
        "diagnostic project/pack must be NEW absent directories",
    )?;
    let name = path.file_name().ok_or("missing output name")?;
    Ok(path
        .parent()
        .ok_or("missing parent")?
        .canonicalize()
        .map_err(|e| e.to_string())?
        .join(name))
}
fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
pub fn run(
    root: &Path,
    model: &Path,
    project: &Path,
    pack: &Path,
    output: &Path,
    clarity: bool,
) -> Result<()> {
    if model.is_dir() {
        require(!clarity, "whole-pack fixture uses --offscreen-proof only")?;
        return whole::run(root, model, project, pack, output);
    }
    let model = model.canonicalize().map_err(|e| e.to_string())?;
    let input = model.parent().ok_or("missing input parent")?;
    let project = future(project)?;
    let pack = future(pack)?;
    let output_path = output.canonicalize().map_err(|e| e.to_string())?;
    let boundaries = [root, input, &project, &pack, &output_path];
    for i in 0..boundaries.len() {
        for j in i + 1..boundaries.len() {
            // Importing bundled samples is legitimate, but diagnostic outputs never overlap inputs.
            if i == 0 && j == 1 {
                continue;
            }
            require(
                !overlaps(boundaries[i], boundaries[j]),
                "diagnostic output overlap with resources/input/project/pack",
            )?;
        }
    }
    let output = resources::smoke_output(root, &output_path)?;
    println!(
        "RESOURCE_ROOT={}\nSMOKE_OUTPUT={}",
        root.display(),
        output.path().display()
    );
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let before = std::fs::read(&model).map_err(|e| e.to_string())?;
    pollster::block_on(async {
        let instance = render::instance();
        let adapter = render::adapter(&instance, None).await?;
        let mut gpu =
            WorkshopGpu::new(&adapter, wgpu::TextureFormat::Rgba8Unorm, [1200, 900]).await?;
        let target = gpu.target([1200, 900]);
        let view = target.create_view(&Default::default());
        let ctx = egui::Context::default();
        let mut app = Workshop::new(vec![
            root.to_path_buf(),
            input.to_path_buf(),
            output_path.clone(),
        ]);
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "import",
            DialogKind::Import,
            &model,
        )?;
        require(
            app.project
                .as_ref()
                .is_some_and(|p| !p.model.textures.is_empty()),
            "proof requires an embedded PNG/UV textured model",
        )?;
        click(&mut gpu, &ctx, &mut app, &view, "decoration")?;
        require(
            !app.project.as_ref().ok_or("no project")?.document_role,
            "decoration action failed",
        )?;
        for id in ["document", "fit", "rotate-step"] {
            click(&mut gpu, &ctx, &mut app, &view, id)?;
        }
        let edit = app.project.as_ref().ok_or("no project")?.edit();
        require(
            edit.document_role && edit.yaw_degrees == 15.0,
            "purpose/rotation UI action failed",
        )?;
        if clarity {
            return clarity_proof(
                &mut gpu,
                &ctx,
                &mut app,
                (&target, &view),
                &output,
                (input, &project, &pack),
            );
        }
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "save-new",
            DialogKind::SaveNew,
            &project,
        )?;
        require(!app.dirty, "save did not clear dirty state")?;
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "open",
            DialogKind::Open,
            &project.join("project.toml"),
        )?;
        require(
            app.project.as_ref().ok_or("no reopened project")?.edit() == edit && !app.dirty,
            "reopen changed state",
        )?;
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "export",
            DialogKind::Export,
            &pack,
        )?;
        let loaded = hair::theme::Theme::load(&pack.join("pack.toml"), &mut Default::default())?;
        let preview = app.preview.as_ref().ok_or("no preview")?;
        require(
            loaded.textures[0].rgba == preview.textures[0].rgba
                && bytemuck::cast_slice::<_, u8>(&loaded.vertices)
                    == bytemuck::cast_slice::<_, u8>(&preview.vertices),
            "export differs from shared preview",
        )?;
        let pixels = gpu.read_pixels(&target, [1200, 900])?;
        write(&output, "office.png", &pixels)?;
        let rect = app.controls["preview"];
        let mut colors = std::collections::HashSet::new();
        for y in rect.min.y.ceil() as usize..rect.max.y.floor() as usize {
            for x in rect.min.x.ceil() as usize..rect.max.x.floor() as usize {
                colors.insert(&pixels[(y * 1200 + x) * 4..(y * 1200 + x) * 4 + 3]);
            }
        }
        require(colors.len() > 20, "preview is flat")?;
        println!(
            "REAL_EGUI_PREVIEW_COLORS={} TEXTURE={}x{}",
            colors.len(),
            loaded.textures[0].width,
            loaded.textures[0].height
        );
        for v in &mut app.preview.as_mut().ok_or("no preview")?.vertices {
            v.texture = 0;
        }
        app.revision += 1;
        gpu.frame(&ctx, &mut app, raw(), &view, [1200, 900])?;
        let counter = gpu.read_pixels(&target, [1200, 900])?;
        let mut changed = 0;
        for y in rect.min.y.ceil() as usize..rect.max.y.floor() as usize {
            for x in rect.min.x.ceil() as usize..rect.max.x.floor() as usize {
                let i = (y * 1200 + x) * 4;
                if pixels[i..i + 3] != counter[i..i + 3] {
                    changed += 1;
                }
            }
        }
        require(
            changed > 20,
            "texture-only preview counterfactual did not change pixels",
        )?;
        write(&output, "reader-welcome.png", &counter)?;
        println!("TEXTURE_COUNTERFACTUAL_CHANGED_PIXELS={changed}");
        Ok::<(), String>(())
    })?;
    require(
        std::fs::read(&model).map_err(|e| e.to_string())? == before
            && std::fs::read(pack.join("model.glb")).map_err(|e| e.to_string())? == before,
        "original/export bytes changed",
    )?;
    if !clarity {
        println!(
            "WORKSHOP_OFFSCREEN_PASS actions=import,decoration,document,fit,rotate-step,save-new,open,export input=synthetic-egui-pointer native=false human=false"
        );
    }
    Ok(())
}
fn clarity_proof(
    gpu: &mut WorkshopGpu,
    ctx: &egui::Context,
    app: &mut Workshop,
    target: (&wgpu::Texture, &wgpu::TextureView),
    output: &resources::SmokeOutput,
    paths: (&Path, &Path, &Path),
) -> Result<()> {
    let (texture, view) = target;
    let (input, project, pack) = paths;
    let state = |app: &Workshop| {
        (
            app.input_path.clone(),
            app.project_path.clone(),
            app.export_path.clone(),
            app.saved_project.clone(),
            app.exported_pack.clone(),
            app.dirty,
            app.revision,
            app.protected.clone(),
            app.project.as_ref().map(|p| p.edit()),
            app.preview
                .as_ref()
                .map(|p| bytemuck::cast_slice::<_, u8>(&p.vertices).to_vec()),
        )
    };
    for (id, kind, bad, good, action, name) in [
        (
            "save-new",
            DialogKind::SaveNew,
            input,
            project,
            "Saving editable project",
            "office.png",
        ),
        (
            "export",
            DialogKind::Export,
            project,
            pack,
            "Exporting appearance pack",
            "reader-welcome.png",
        ),
    ] {
        let before = state(app);
        click(gpu, ctx, app, view, id)?;
        require(
            app.begin_dialog() == Some(kind),
            "clarity browse request missing",
        )?;
        app.finish_dialog(Ok(Some(bad.to_path_buf())));
        let error = app.error.clone().ok_or("expected writer refusal")?;
        require(
            error.contains(action)
                && error.contains(&bad.display().to_string())
                && error.contains("Conflicts with read-only path:")
                && error.contains("outside"),
            "clarity error is not actionable",
        )?;
        require(
            state(app) == before,
            "refused selection mutated last-good state or paths",
        )?;
        // Draw actual app error; assert real wrapped text shapes, then read GPU pixels.
        for _ in 0..3 {
            gpu.frame(ctx, app, raw(), view, [1200, 900])?;
        }
        let frame = ctx.run_ui(raw(), |ui| app.ui(ui, None));
        let mut text = String::new();
        fn collect(shape: &egui::epaint::Shape, text: &mut String) {
            match shape {
                egui::epaint::Shape::Text(t) => {
                    text.push_str(&t.galley.job.text);
                    text.push('\n');
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for s in shapes {
                        collect(s, text);
                    }
                }
                _ => {}
            }
        }
        for s in &frame.shapes {
            collect(&s.shape, &mut text);
        }
        frame.drop_without_applying_deltas();
        gpu.frame(ctx, app, raw(), view, [1200, 900])?;
        require(
            text.contains(action)
                && text.contains("Conflicts with read-only path:")
                && !text.contains("Saved NEW editable project;")
                && !text.contains("Exported appearance pack."),
            "error not visible or stale success visible",
        )?;
        let rect = app.controls["error-message"];
        require(
            rect.min.x >= 0.0 && rect.max.x <= 1200.0 && rect.min.y < 900.0,
            "error is not wrapped inside the frame",
        )?;
        write(output, name, &gpu.read_pixels(texture, [1200, 900])?)?;
        println!(
            "VISIBLE_CLARITY_ERROR action={action} selected={} wrapped=true\n{error}",
            bad.display()
        );
        click(gpu, ctx, app, view, id)?;
        require(
            app.begin_dialog() == Some(kind),
            "cancel retry request missing",
        )?;
        app.finish_dialog(Ok(None));
        require(
            state(app) == before && app.error.as_ref() == Some(&error),
            "cancel changed error/last-good state",
        )?;
        browse(gpu, ctx, app, view, id, kind, good)?;
        if kind == DialogKind::SaveNew {
            require(
                !app.dirty && app.saved_project.as_deref() == Some(&project.join("project.toml")),
                "saved location/state missing",
            )?;
            click(gpu, ctx, app, view, "rotate-step")?;
            require(app.dirty, "post-save edit must be dirty")?;
        } else {
            require(
                app.dirty && app.exported_pack.as_deref() == Some(&pack.join("pack.toml")),
                "export falsely saved edits or lost location",
            )?;
        }
        println!("CLARITY_RECOVERY kind={kind:?} cancel_preserved=true retry_writer=true");
    }
    let saved = std::fs::read(project.join("project.toml")).map_err(|e| e.to_string())?;
    let exported = std::fs::read(pack.join("pack.toml")).map_err(|e| e.to_string())?;
    for (id, kind, path) in [
        ("save-new", DialogKind::SaveNew, project),
        ("export", DialogKind::Export, pack),
    ] {
        let before = state(app);
        click(gpu, ctx, app, view, id)?;
        require(
            app.begin_dialog() == Some(kind),
            "existing-output request missing",
        )?;
        app.finish_dialog(Ok(Some(path.to_path_buf())));
        require(
            app.error.is_some() && state(app) == before,
            "native selection authorized overwrite",
        )?;
        click(gpu, ctx, app, view, "ack-error")?;
        require(app.error.is_none(), "dismiss error failed")?;
    }
    require(
        std::fs::read(project.join("project.toml")).map_err(|e| e.to_string())? == saved
            && std::fs::read(pack.join("pack.toml")).map_err(|e| e.to_string())? == exported,
        "nooverwrite sentinel changed",
    )?;
    // Display the recovered state with both durable locations and dirty edits.
    for _ in 0..3 {
        gpu.frame(ctx, app, raw(), view, [1200, 900])?;
    }
    write(
        output,
        "reader-workshop.png",
        &gpu.read_pixels(texture, [1200, 900])?,
    )?;
    println!(
        "WORKSHOP_CLARITY_PASS save_error=office.png export_error=reader-welcome.png recovery=reader-workshop.png dirty_after_export=true nooverwrite=true native=false human=false"
    );
    Ok(())
}

// Finite synthetic pointer dispatch + injected panel outcomes, NEVER system UI.
fn browse(
    gpu: &mut WorkshopGpu,
    ctx: &egui::Context,
    app: &mut Workshop,
    target: &wgpu::TextureView,
    id: &str,
    kind: DialogKind,
    path: &Path,
) -> Result<()> {
    let snapshot = |app: &Workshop| {
        (
            app.revision,
            app.dirty,
            app.should_close,
            app.error.clone(),
            app.notice.clone(),
            [
                app.input_path.clone(),
                app.project_path.clone(),
                app.export_path.clone(),
            ],
            app.protected.clone(),
            app.project.as_ref().map(|p| p.edit()),
            app.preview
                .as_ref()
                .map(|p| bytemuck::cast_slice::<_, u8>(&p.vertices).to_vec()),
        )
    };
    let before = snapshot(app);
    let existed = path.exists();
    click(gpu, ctx, app, target, id)?;
    require(
        app.begin_dialog() == Some(kind),
        "browse requested wrong panel kind",
    )?;
    require(app.begin_dialog().is_none(), "duplicate panel request")?;
    app.finish_dialog(Ok(None));
    require(
        snapshot(app) == before && path.exists() == existed,
        "cancel changed state/output",
    )?;
    click(gpu, ctx, app, target, id)?;
    require(
        app.begin_dialog() == Some(kind),
        "browse retry requested wrong panel",
    )?;
    app.finish_dialog(Ok(Some(path.to_path_buf())));
    if let Some(error) = &app.error {
        return Err(error.clone());
    }
    gpu.frame(ctx, app, raw(), target, [1200, 900])?;
    println!(
        "INJECTED_PANEL_RESULT kind={kind:?} cancel_preserved=true selection_routed=true native=false"
    );
    Ok(())
}
fn raw() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 900.0),
        )),
        ..Default::default()
    }
}
fn click(
    gpu: &mut WorkshopGpu,
    ctx: &egui::Context,
    app: &mut Workshop,
    target: &wgpu::TextureView,
    id: &str,
) -> Result<()> {
    // Let egui's auto-sized panels settle after a status/error height change
    // before taking the real widget rectangle for synthetic input.
    for _ in 0..3 {
        gpu.frame(ctx, app, raw(), target, [1200, 900])?;
    }
    let pos = app
        .controls
        .get(id)
        .ok_or_else(|| format!("missing real UI control {id}"))?
        .center();
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
        gpu.frame(ctx, app, input, target, [1200, 900])?;
    }
    gpu.frame(ctx, app, raw(), target, [1200, 900])?;
    // Expected-error flows must be able to click Cancel/Retry/Dismiss. Successful
    // action callers check their own result after the normal completion handler.
    Ok(())
}
fn write(output: &resources::SmokeOutput, name: &str, pixels: &[u8]) -> Result<()> {
    let mut file = output.create_capture(name)?;
    let mut encoder = png::Encoder::new(&mut file, 1200, 900);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png = encoder.write_header().map_err(|e| e.to_string())?;
    png.write_image_data(pixels).map_err(|e| e.to_string())?;
    png.finish().map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())
}
