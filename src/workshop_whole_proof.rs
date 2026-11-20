//! Fixed data-only directory fixture for the production offscreen diagnostic.
//! No scripts, arbitrary configuration, network, or native OS input.
use super::{future, overlaps, raw, require, write};
use hair::{
    Result,
    authoring::Project,
    render, resources,
    workshop::{Command, DialogKind, Edit, Workshop, WorkshopGpu},
};
use std::path::{Path, PathBuf};

const FILES: [&str; 5] = [
    "document.glb",
    "decoration.glb",
    "door.glb",
    "wall.png",
    "floor.png",
];

pub fn run(root: &Path, fixture: &Path, project: &Path, pack: &Path, output: &Path) -> Result<()> {
    require(
        fixture.is_absolute() && !fixture.is_symlink(),
        "fixture must be an absolute non-symlink directory",
    )?;
    let fixture = fixture.canonicalize().map_err(|e| e.to_string())?;
    let project = future(project)?;
    let pack = future(pack)?;
    let output_path = output.canonicalize().map_err(|e| e.to_string())?;
    // A bundled consumer protects the entire bundle, not only Contents/Resources.
    let bundle = if root.file_name().is_some_and(|n| n == "Resources")
        && root
            .parent()
            .is_some_and(|p| p.file_name().is_some_and(|n| n == "Contents"))
    {
        root.parent().and_then(Path::parent).unwrap_or(root)
    } else {
        root
    };
    let inputs = [bundle, fixture.as_path()];
    let outputs = [&project, &pack, &output_path];
    for (i, a) in outputs.iter().enumerate() {
        for b in &inputs {
            require(
                !overlaps(a, b),
                "diagnostic output overlap with resources/bundle/fixture",
            )?;
        }
        for b in &outputs[i + 1..] {
            require(
                !overlaps(a, b),
                "diagnostic output overlap with project/pack/captures",
            )?;
        }
    }
    // Strict bounded leaves, existing decoders and aggregate preview budgets before GPU or writers.
    let bytes = FILES
        .iter()
        .map(|n| {
            hair::content::read_local(
                &fixture,
                n,
                if n.ends_with("png") {
                    4_000_000
                } else {
                    8_000_000
                },
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let mut preflight = Workshop::new(vec![bundle.into(), fixture.clone()]);
    for (i, command) in [
        (0, None),
        (1, Some(Command::SelectModel(false))),
        (2, Some(Command::Door)),
    ] {
        if let Some(command) = command {
            preflight.dispatch(command);
        }
        preflight.request_dialog(DialogKind::Import);
        require(
            preflight.begin_dialog() == Some(DialogKind::Import),
            "preflight panel request",
        )?;
        preflight.finish_dialog(Ok(Some(fixture.join(FILES[i]))));
        if preflight.confirmation_pending() {
            preflight.dispatch(Command::ConfirmDiscard);
        }
        require(
            preflight.error.is_none(),
            preflight.error.as_deref().unwrap_or("invalid fixture"),
        )?;
    }
    for (kind, i) in [(DialogKind::WallTexture, 3), (DialogKind::FloorTexture, 4)] {
        preflight.request_dialog(kind);
        preflight.begin_dialog();
        preflight.finish_dialog(Ok(Some(fixture.join(FILES[i]))));
        require(
            preflight.error.is_none(),
            preflight.error.as_deref().unwrap_or("invalid PNG"),
        )?;
    }
    preflight
        .project
        .as_ref()
        .ok_or("missing preflight project")?
        .preview()?;
    let output = resources::smoke_output(bundle, &output_path)?;
    pollster::block_on(async {
        let instance = render::instance();
        let adapter = render::adapter(&instance, None).await?;
        let mut gpu =
            WorkshopGpu::new(&adapter, wgpu::TextureFormat::Rgba8Unorm, [1200, 900]).await?;
        let target = gpu.target([1200, 900]);
        let view = target.create_view(&Default::default());
        let ctx = egui::Context::default();
        let mut app = Workshop::new(vec![bundle.into(), fixture.clone(), output_path]);
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "import",
            DialogKind::Import,
            &fixture.join(FILES[0]),
        )?;
        click(&mut gpu, &ctx, &mut app, &view, "rotate-step")?;
        let document = app.project.as_ref().ok_or("no document")?.edit();
        require(
            document.yaw_degrees == 15.0,
            "document rotation did not execute",
        )?;
        click(&mut gpu, &ctx, &mut app, &view, "decoration")?;
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "import",
            DialogKind::Import,
            &fixture.join(FILES[1]),
        )?;
        for id in ["fit", "rotate-step", "rotate-step"] {
            click(&mut gpu, &ctx, &mut app, &view, id)?;
        }
        let decoration = app.project.as_ref().ok_or("no decoration")?.edit();
        require(
            !decoration.document_role && decoration.yaw_degrees == 30.0,
            "decoration independent edit failed",
        )?;
        click(&mut gpu, &ctx, &mut app, &view, "door-trim")?;
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "import",
            DialogKind::Import,
            &fixture.join(FILES[2]),
        )?;
        let door = app.project.as_ref().ok_or("no door")?.edit();
        require(
            app.project.as_ref().is_some_and(|p| p.door_active),
            "door selection failed",
        )?;
        for (id, kind, i) in [
            ("wall-png", DialogKind::WallTexture, 3),
            ("floor-png", DialogKind::FloorTexture, 4),
        ] {
            click(
                &mut gpu,
                &ctx,
                &mut app,
                &view,
                if i == 3 { "wall" } else { "floor" },
            )?;
            browse(
                &mut gpu,
                &ctx,
                &mut app,
                &view,
                id,
                kind,
                &fixture.join(FILES[i]),
            )?;
        }
        for id in ["wall-red", "wall-tile", "floor-green", "floor-tile"] {
            click(
                &mut gpu,
                &ctx,
                &mut app,
                &view,
                if id.starts_with("wall") {
                    "wall"
                } else {
                    "floor"
                },
            )?;
            let p = visible(&mut gpu, &ctx, &mut app, &view, id)?;
            let r = app.controls[id];
            let pos = egui::pos2(r.left() + r.width() * 0.25, p.y);
            pointer(&mut gpu, &ctx, &mut app, &view, pos)?;
        }
        let surfaces = app.project.as_ref().ok_or("no surfaces")?.surfaces();
        require(
            surfaces.len() == 2
                && surfaces.iter().all(|s| {
                    s.file.is_some() && s.meters_per_tile > 1.1 && s.tint.iter().any(|t| *t < 0.99)
                }),
            "surface widget edits failed",
        )?;
        let edits = [document, decoration, door];
        check_roles(&mut gpu, &ctx, &mut app, &view, &edits)?;
        browse(
            &mut gpu,
            &ctx,
            &mut app,
            &view,
            "save-new",
            DialogKind::SaveNew,
            &project,
        )?;
        require(!app.dirty, "save did not clear dirty")?;
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
            !app.dirty
                && app
                    .project
                    .as_ref()
                    .is_some_and(|p| p.surfaces() == surfaces),
            "reopen changed surfaces/dirty",
        )?;
        check_roles(&mut gpu, &ctx, &mut app, &view, &edits)?;
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
        let preview = app.preview.as_ref().ok_or("missing shared preview")?;
        require(
            loaded.loaded_roles.len() == 3
                && loaded.host == preview.host
                && bytemuck::cast_slice::<_, u8>(&loaded.vertices)
                    == bytemuck::cast_slice::<_, u8>(&preview.vertices)
                && loaded.textures.len() == preview.textures.len()
                && loaded
                    .textures
                    .iter()
                    .zip(&preview.textures)
                    .all(|(a, b)| a.rgba == b.rgba),
            "export differs from actual shared preview",
        )?;
        let pixels = gpu.read_pixels(&target, [1200, 900])?;
        let rect = app.controls["preview"];
        let mut colors = std::collections::HashSet::new();
        for y in rect.min.y.ceil() as usize..rect.max.y.floor() as usize {
            for x in rect.min.x.ceil() as usize..rect.max.x.floor() as usize {
                let i = (y * 1200 + x) * 4;
                colors.insert([pixels[i], pixels[i + 1], pixels[i + 2]]);
            }
        }
        require(
            colors.len() > 20 && pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255),
            "all-category GPU frame flat/nonopaque",
        )?;
        write(&output, "office.png", &pixels)?;
        println!(
            "WORKSHOP_ALL_CATEGORIES_PASS roles=3 surfaces=2 independent_pre_post_edits={edits:?} surfaces={surfaces:?} preview_colors={} save_open_export=true input=synthetic-egui-pointer panel=synthetic-native-completion native=false human=false",
            colors.len()
        );
        Ok::<(), String>(())
    })?;
    for (i, n) in FILES.iter().enumerate() {
        require(
            hair::content::read_local(&fixture, n, 8_000_000)? == bytes[i],
            "fixture bytes changed",
        )?;
    }
    // Independent persistence decoder, not caller-supplied metadata.
    let reopened = Project::open(&project.join("project.toml"))?;
    reopened.preview()?;
    Ok(())
}
fn check_roles(
    g: &mut WorkshopGpu,
    c: &egui::Context,
    a: &mut Workshop,
    v: &wgpu::TextureView,
    edits: &[Edit; 3],
) -> Result<()> {
    for (i, id) in ["document", "decoration", "door-trim"].iter().enumerate() {
        click(g, c, a, v, id)?;
        let p = a.project.as_ref().ok_or("no project")?;
        require(
            p.edit() == edits[i] && p.door_active == (i == 2),
            "independent role transform lost",
        )?;
    }
    Ok(())
}
fn browse(
    g: &mut WorkshopGpu,
    c: &egui::Context,
    a: &mut Workshop,
    v: &wgpu::TextureView,
    id: &str,
    kind: DialogKind,
    path: &Path,
) -> Result<()> {
    visible(g, c, a, v, id)?;
    // Use the existing diagnostic cancel/selection adapter and native completion handler.
    // Importing a second role legitimately requires the existing discard confirmation.
    click(g, c, a, v, id)?;
    require(a.begin_dialog() == Some(kind), "wrong panel kind")?;
    let state = (a.revision, a.dirty, a.project.as_ref().map(|p| p.edit()));
    a.finish_dialog(Ok(None));
    require(
        (a.revision, a.dirty, a.project.as_ref().map(|p| p.edit())) == state,
        "cancel changed state",
    )?;
    click(g, c, a, v, id)?;
    require(a.begin_dialog() == Some(kind), "wrong retry panel kind")?;
    require(a.begin_dialog().is_none(), "duplicate panel claim")?;
    a.finish_dialog(Ok(Some(PathBuf::from(path))));
    if a.confirmation_pending() {
        click(g, c, a, v, "confirm-discard")?;
    }
    require(
        a.error.is_none(),
        a.error.as_deref().unwrap_or("panel completion error"),
    )?;
    g.frame(c, a, raw(), v, [1200, 900])?;
    Ok(())
}
fn visible(
    g: &mut WorkshopGpu,
    c: &egui::Context,
    a: &mut Workshop,
    v: &wgpu::TextureView,
    id: &str,
) -> Result<egui::Pos2> {
    for _ in 0..3 {
        g.frame(c, a, raw(), v, [1200, 900])?;
    }
    for _ in 0..40 {
        let r = *a
            .controls
            .get(id)
            .ok_or_else(|| format!("missing real control {id}"))?;
        if r.center().y > 55.0 && r.center().y < 740.0 {
            return Ok(r.center());
        }
        let mut input = raw();
        input.events = vec![
            egui::Event::PointerMoved(egui::pos2(200.0, 160.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                delta: egui::vec2(0.0, if r.center().y < 55.0 { 100.0 } else { -100.0 }),
                modifiers: Default::default(),
            },
        ];
        g.frame(c, a, input, v, [1200, 900])?;
        for _ in 0..3 {
            g.frame(c, a, raw(), v, [1200, 900])?;
        }
    }
    Err(format!("unreachable real control {id}"))
}
fn click(
    g: &mut WorkshopGpu,
    c: &egui::Context,
    a: &mut Workshop,
    v: &wgpu::TextureView,
    id: &str,
) -> Result<()> {
    let p = visible(g, c, a, v, id)?;
    pointer(g, c, a, v, p)
}
fn pointer(
    g: &mut WorkshopGpu,
    c: &egui::Context,
    a: &mut Workshop,
    v: &wgpu::TextureView,
    pos: egui::Pos2,
) -> Result<()> {
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
        g.frame(c, a, input, v, [1200, 900])?;
    }
    g.frame(c, a, raw(), v, [1200, 900])?;
    Ok(())
}
