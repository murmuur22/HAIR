//! Executable proof: real App key/completion/actions and shared GPU renderer.
//! Safe aim poses and chooser completions are injected, NOT OS/native-human input.
use super::*;
use hair::{
    folder_scene::{self, BoundEntry},
    local_explorer::Explorer,
};
use std::collections::BTreeSet;
fn require(value: bool, why: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(format!("folder flow assertion: {why}"))
    }
}
fn tap(app: &mut App, key: KeyCode) -> Result<()> {
    require(!app.key(key, true, false)?, "unexpected exit from key")?;
    require(!app.key(key, false, false)?, "unexpected exit from release")
}
fn pose_key(app: &App) -> ([f32; 2], f32, f32) {
    (app.pose.position, app.pose.yaw, app.pose.pitch)
}
fn snapshot(app: &App) -> Result<hair::local_explorer::Snapshot> {
    Ok(app
        .local
        .as_ref()
        .ok_or("proof lost local model")?
        .explorer
        .current()
        .clone())
}
fn aim(app: &mut App, anchor: &str) -> Result<BoundEntry> {
    let host = &app.themes[app.theme_index].host;
    let a = host
        .anchors
        .iter()
        .find(|a| a.id == anchor)
        .ok_or("proof missing anchor")?;
    let bound = app
        .world
        .folder_view
        .as_ref()
        .and_then(|v| v.bindings.get(anchor))
        .ok_or("proof missing raw binding")?
        .clone();
    app.pose = Pose {
        position: [a.at[0], a.at[2] + 1.05],
        yaw: 0.0,
        pitch: ((a.at[1] + a.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    require(
        !host.blocked(app.pose.position),
        "synthetic aim pose must be collision safe",
    )?;
    app.world.explicit_target = None;
    hair::interaction::refresh(host, &app.pose, &mut app.world);
    let target = app
        .world
        .interaction
        .as_ref()
        .ok_or("proof aimed object not eligible")?;
    let expected_source = folder_scene::display(bound.entry.path.as_os_str());
    require(
        target.anchor == anchor && target.destination.as_deref() == Some(expected_source.as_str()),
        "prompt/activation raw path mismatch",
    )?;
    require(
        target
            .prompt
            .contains(&folder_scene::display(&bound.entry.name)),
        "prompt must show bound raw name",
    )?;
    if let Some(renderer) = &mut app.renderer {
        renderer.set_scene(&app.themes[app.theme_index].vertices_for(&app.world));
    }
    Ok(bound)
}
fn find(app: &mut App, name: &str) -> Result<BoundEntry> {
    let index = snapshot(app)?
        .entries
        .iter()
        .position(|entry| entry.name == name)
        .ok_or_else(|| format!("proof fixture missing {name}"))?;
    let page = index / folder_scene::PAGE_SIZE;
    while app.world.folder_view.as_ref().ok_or("missing view")?.page < page {
        tap(app, KeyCode::PageDown)?;
    }
    while app.world.folder_view.as_ref().ok_or("missing view")?.page > page {
        tap(app, KeyCode::PageUp)?;
    }
    let anchor = app
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.index == index)
        .map(|(id, _)| id.clone())
        .ok_or("page binding missing")?;
    aim(app, &anchor)
}
fn draw_frame(app: &mut App) -> Result<()> {
    app.update_hud();
    let renderer = app.renderer.as_ref().ok_or("proof missing renderer")?;
    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("folder adapter synthetic frame; no native surface"),
        size: wgpu::Extent3d {
            width: 960,
            height: 640,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    renderer.draw(
        &texture.create_view(&Default::default()),
        &app.pose,
        &app.world,
        app.scroll,
    )?;
    renderer.wait()?;
    app.frames += 1;
    Ok(())
}
fn capture(app: &mut App, output: &resources::SmokeOutput, name: &str) -> Result<()> {
    app.update_hud();
    let renderer = app.renderer.as_mut().ok_or("proof missing renderer")?;
    renderer.set_scene(&app.themes[app.theme_index].vertices_for(&app.world));
    renderer.capture(output, name, &app.pose, &app.world)
}
pub(super) async fn run(mut app: App, input: &Path, output: &resources::SmokeOutput) -> Result<()> {
    let instance = render::instance();
    let adapter = render::adapter(&instance, None).await?;
    let mut renderer = render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [960, 640],
        &app.themes[app.theme_index].vertices_for(&app.world),
    )
    .await?;
    renderer.set_materials(&app.themes[app.theme_index].textures)?;
    renderer.error_probe().await?;
    app.renderer = Some(renderer);
    draw_frame(&mut app)?;
    app.key(KeyCode::KeyW, true, false)?;
    app.input.captured = true;
    app.key(KeyCode::KeyO, true, false)?;
    require(
        !app.input.captured && !app.input.forward,
        "chooser releases movement and capture before claim",
    )?;
    require(
        app.claim_folder_request(),
        "O request must be claimable after frame",
    )?;
    require(
        !app.claim_folder_request(),
        "chooser claim must be single-use",
    )?;
    app.complete_folder(Ok(Some(input.to_path_buf())))?;
    app.key(KeyCode::KeyO, true, true)?;
    app.key(KeyCode::KeyO, true, false)?;
    require(!app.chooser_pending, "held O cannot reopen modal")?;
    app.key(KeyCode::KeyO, false, false)?;
    let original = snapshot(&app)?;
    require(
        original.entries.len() >= 77,
        "fixture must exercise more than 64 entries",
    )?;
    require(
        app.web.is_none() && app.world.pocket.is_none() && app.world.site.is_none(),
        "folder has no HTTPS/reader-link/image authority",
    )?;
    for completion in [Ok(None), Err("injected panel error".to_string())] {
        tap(&mut app, KeyCode::KeyO)?;
        require(app.claim_folder_request(), "new O claim")?;
        let before = (
            snapshot(&app)?,
            pose_key(&app),
            app.theme_index,
            app.world.tree_collapsed,
        );
        let was_error = completion.is_err();
        let result = app.complete_folder(completion);
        require(
            result.is_err() == was_error,
            "completion error must be surfaced",
        )?;
        require(
            before
                == (
                    snapshot(&app)?,
                    pose_key(&app),
                    app.theme_index,
                    app.world.tree_collapsed,
                ),
            "cancel/error retains last good state",
        )?;
    }
    tap(&mut app, KeyCode::KeyM)?;
    let collapsed = app.world.tree_collapsed;
    app.key(KeyCode::KeyM, true, false)?;
    let toggled = app.world.tree_collapsed;
    app.key(KeyCode::KeyM, true, false)?;
    app.key(KeyCode::KeyM, true, true)?;
    require(
        app.world.tree_collapsed == toggled,
        "M hold/repeat must toggle only once",
    )?;
    app.key(KeyCode::KeyM, false, false)?;
    tap(&mut app, KeyCode::KeyM)?;
    require(app.world.tree_collapsed == collapsed, "M release re-arms")?;

    find(&mut app, "Child")?;
    let previous_pose = pose_key(&app);
    app.key(KeyCode::KeyE, true, false)?;
    let child = snapshot(&app)?;
    require(
        child.folder == original.folder.join("Child") && child.entries.is_empty(),
        "E enters real empty child",
    )?;
    require(
        app.world.folder_view.as_ref().unwrap().bindings.is_empty(),
        "empty folder has no fabricated objects bound",
    )?;
    app.key(KeyCode::KeyE, true, false)?;
    app.key(KeyCode::KeyE, true, true)?;
    require(
        snapshot(&app)? == child && !app.world.reading,
        "held E must not chain",
    )?;
    app.key(KeyCode::KeyE, false, false)?;
    app.key(KeyCode::KeyR, true, false)?;
    require(
        snapshot(&app)? == original && pose_key(&app) == previous_pose,
        "Back returns visited folder and safe prior pose",
    )?;
    app.key(KeyCode::KeyR, true, false)?;
    app.key(KeyCode::KeyR, true, true)?;
    require(snapshot(&app)? == original, "held Back must not chain")?;
    app.key(KeyCode::KeyR, false, false)?;
    app.key(KeyCode::KeyU, true, false)?;
    require(
        snapshot(&app)?.folder == original.folder.parent().ok_or("fixture has no parent")?,
        "Parent must go beyond starting folder",
    )?;
    let parent = snapshot(&app)?;
    app.key(KeyCode::KeyU, true, true)?;
    app.key(KeyCode::KeyU, true, false)?;
    require(snapshot(&app)? == parent, "Parent hold/repeat")?;
    app.key(KeyCode::KeyU, false, false)?;
    tap(&mut app, KeyCode::KeyR)?;
    require(snapshot(&app)? == original, "Back after Parent")?;
    let mut root = Explorer::new(Path::new("/"))?;
    let root_before = root.current().clone();
    require(
        !root.up()? && root.current() == &root_before && !root.back()?,
        "CPU root Parent is no-op without Back history",
    )?;
    drop(root);

    let mut reached = BTreeSet::new();
    let pages = app.world.folder_view.as_ref().unwrap().pages();
    for page in 0..pages {
        require(
            app.world.folder_view.as_ref().unwrap().page == page,
            "PgDn exact page",
        )?;
        let bindings = app.world.folder_view.as_ref().unwrap().bindings.clone();
        for (anchor, bound) in bindings {
            let actual = aim(&mut app, &anchor)?;
            require(
                actual == bound && original.entries.get(bound.index) == Some(&bound.entry),
                "all raw binding indices reachable through real resolver",
            )?;
            reached.insert(bound.index);
        }
        if page + 1 < pages {
            app.key(KeyCode::PageDown, true, false)?;
            app.key(KeyCode::PageDown, true, false)?;
            app.key(KeyCode::PageDown, true, true)?;
            require(
                app.world.folder_view.as_ref().unwrap().page == page + 1,
                "PgDn hold/repeat no chaining",
            )?;
            app.key(KeyCode::PageDown, false, false)?;
        }
    }
    require(
        reached.len() == original.entries.len(),
        "every enumerated entry reached, no 8/64 truncation",
    )?;
    tap(&mut app, KeyCode::PageDown)?;
    require(
        app.world.folder_view.as_ref().unwrap().page == pages - 1,
        "last page edge no-op",
    )?;
    while app.world.folder_view.as_ref().unwrap().page > 0 {
        tap(&mut app, KeyCode::PageUp)?;
    }
    tap(&mut app, KeyCode::PageUp)?;
    require(
        app.world.folder_view.as_ref().unwrap().page == 0,
        "first page edge no-op",
    )?;
    require(
        app.world.tree_collapsed == collapsed,
        "tree state retained across navigation/pages",
    )?;

    find(&mut app, "Read me.txt")?;
    // Capture failures are retained, not allowed to skip any subsequent state assertion.
    let mut image_errors = Vec::new();
    require(
        !app.world.reading,
        "office capture must be closed-reader real selected folder",
    )?;
    if let Err(error) = capture(&mut app, output, "office.png") {
        image_errors.push(error);
    }
    let selected_source =
        folder_scene::display(&snapshot(&app)?.folder.join("Read me.txt").into_os_string());
    tap(&mut app, KeyCode::KeyE)?;
    require(
        app.world.reading
            && app.world.document()?.contains("LOCAL FILE ORIGINAL")
            && app.world.document()?.contains(&selected_source),
        "E displays original bounded read_entry bytes and exact selected path",
    )?;
    let reader_state = (
        snapshot(&app)?,
        app.world.folder_view.clone(),
        pose_key(&app),
    );
    for key in [
        KeyCode::KeyU,
        KeyCode::KeyR,
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::F5,
        KeyCode::KeyO,
        KeyCode::KeyE,
        KeyCode::Tab,
        KeyCode::KeyW,
        KeyCode::KeyL,
    ] {
        tap(&mut app, key)?;
    }
    require(
        reader_state
            == (
                snapshot(&app)?,
                app.world.folder_view.clone(),
                pose_key(&app),
            )
            && !app.chooser_pending
            && !app.input.forward,
        "reader isolates world/chooser/link/page controls",
    )?;
    draw_frame(&mut app)?;
    if let Err(error) = capture(&mut app, output, "reader-workshop.png") {
        image_errors.push(error);
    }
    tap(&mut app, KeyCode::Escape)?;
    require(
        !app.world.reading,
        "Escape closes selected reader, not application",
    )?;
    tap(&mut app, KeyCode::F5)?;
    require(
        snapshot(&app)? == original,
        "F5 re-enumerates current folder without fixture mutation",
    )?;
    require(
        app.world.tree_collapsed == collapsed,
        "refresh retains M state",
    )?;
    app.pose = Pose {
        position: app.themes[app.theme_index].host.spawn,
        ..Pose::default()
    };
    tap(&mut app, KeyCode::KeyI)?;
    require(
        app.world.reading && app.world.document()?.contains(&app.world.current),
        "I exposes full current path even without a target",
    )?;
    tap(&mut app, KeyCode::Escape)?;

    app.key(KeyCode::KeyW, true, false)?;
    app.key(KeyCode::KeyU, true, false)?;
    app.key(KeyCode::KeyO, true, false)?;
    app.focus_changed(false);
    require(
        !app.input.forward
            && !app.input.captured
            && app.local_held.is_empty()
            && !app.chooser_pending,
        "focus loss clears movement/actions and unclaimed modal",
    )?;
    let unfocused = snapshot(&app)?;
    for key in [
        KeyCode::KeyE,
        KeyCode::KeyU,
        KeyCode::KeyO,
        KeyCode::PageDown,
    ] {
        tap(&mut app, key)?;
    }
    require(
        snapshot(&app)? == unfocused && !app.chooser_pending,
        "unfocused actions ignored",
    )?;
    app.focus_changed(true);
    // U above was a real action, so use real Back to recover the starting folder.
    tap(&mut app, KeyCode::KeyR)?;
    require(
        snapshot(&app)? == original,
        "Back re-armed after focus loss",
    )?;
    find(&mut app, "Read me.txt")?;
    let before_t = (
        snapshot(&app)?,
        app.world.folder_view.clone(),
        pose_key(&app),
        app.world.tree_collapsed,
    );
    let old_theme = app.theme_index;
    tap(&mut app, KeyCode::KeyT)?;
    require(
        app.theme_index == (old_theme + 1) % app.themes.len(),
        "T must actually switch renderer-backed appearance",
    )?;
    require(
        before_t
            == (
                snapshot(&app)?,
                app.world.folder_view.clone(),
                pose_key(&app),
                app.world.tree_collapsed,
            ),
        "T keeps raw paths/bindings/pose/tree",
    )?;
    draw_frame(&mut app)?;
    require(
        app.web.is_none() && !app.world.current.starts_with("https://"),
        "local flow cannot inherit web state",
    )?;
    app.renderer.as_ref().unwrap().error_probe().await?;
    app.renderer.as_ref().unwrap().wait()?;
    println!(
        "FOLDER_FLOW_ASSERTIONS navigation=checked pages=checked reader=checked focus=checked appearance=checked; no input mutation by product"
    );
    if !image_errors.is_empty() {
        return Err(format!(
            "navigation assertions completed; protected capture errors: {}",
            image_errors.join("; ")
        ));
    }
    println!(
        "CAPTURE_BINDING office.png = selected folder, closed reader, aimed Read me.txt role object; reader-workshop.png = same file's original text in actual reader (historical filename, not Workshop)"
    );
    println!(
        "NATIVE_ADAPTER_SYNTHETIC shared App key/completion/actions + offscreen shared renderer; NOT native-human input, no desktop surface/unlock/wake"
    );
    println!("LOCAL_FOLDER_FLOW_OK");
    Ok(())
}
