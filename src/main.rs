mod collection_native;
#[cfg(test)]
mod directory_map_app_tests;
mod directory_travel;
mod folder_dialog;
mod folder_native;
mod folder_proof;
mod inventory_native;
mod storage_native;
#[cfg(test)]
mod storage_documents_tests;
#[cfg(test)]
mod item_app_capture_tests;
mod placement_native;
mod player_settings;
#[cfg(test)]
mod player_settings_r10_tests;
#[cfg(test)]
mod player_settings_r11_tests;
#[cfg(test)]
mod slice6c_texture_fixture;
mod presentation_native;
mod sidebar_input;
mod web_native;
use hair::{Action, Input, Pose, Result, World};
#[cfg(test)]
mod hallway_tests;
#[cfg(test)]
mod spaces_tests;
#[cfg(test)]
mod tree_tests;
#[cfg(test)]
mod web_native_tests;
use hair::{render, resources};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("HAIR ERROR: {error}");
        std::process::exit(1);
    }
}
fn take_url(args: &mut Vec<String>) -> Result<Option<hair::web::Address>> {
    let count = args.iter().filter(|s| *s == "--url").count();
    if count == 0 {
        return Ok(None);
    }
    if count != 1 || args.iter().any(|s| s == "--site") {
        return Err("select exactly one --url, without --site".into());
    }
    let index = args.iter().position(|s| s == "--url").unwrap();
    let raw = args
        .get(index + 1)
        .ok_or("--url requires a literal HTTPS page")?;
    let address = hair::web::Address::parse(raw)?;
    args.drain(index..=index + 1);
    Ok(Some(address))
}
fn take_folder(args: &mut Vec<std::ffi::OsString>) -> Result<Option<PathBuf>> {
    let mut selectors = 0;
    for flag in ["--folder", "--site", "--url"] {
        let count = args.iter().filter(|s| *s == flag).count();
        if count > 1 {
            return Err(format!("duplicate selector {flag}"));
        }
        selectors += count;
        if let Some(index) = args.iter().position(|s| s == flag) {
            let value = args
                .get(index + 1)
                .ok_or_else(|| format!("{flag} requires a value"))?;
            if value.is_empty() || value.to_string_lossy().starts_with("--") {
                return Err(format!("{flag} requires a value, not another option"));
            }
        }
    }
    if selectors > 1 {
        return Err("select only one of --folder, --site, or --url".into());
    }
    let Some(index) = args.iter().position(|s| s == "--folder") else {
        return Ok(None);
    };
    let folder = PathBuf::from(args.remove(index + 1));
    args.remove(index);
    Ok(Some(folder))
}
fn run() -> Result<()> {
    let mut os_args: Vec<_> = std::env::args_os().skip(1).collect();
    let folder_arg = take_folder(&mut os_args)?;
    // Only the new folder selector accepts arbitrary OS path bytes. Preserve the
    // existing UTF-8 site/URL parser without turning folder display into identity.
    let mut args: Vec<String> = os_args
        .into_iter()
        .map(|arg| {
            arg.into_string()
                .map_err(|_| "non-UTF-8 argument outside --folder".to_string())
        })
        .collect::<Result<_>>()?;
    let folder_proof = args.iter().any(|s| s == "--folder-flow-proof");
    if folder_proof && folder_arg.is_none() {
        return Err("--folder-flow-proof requires --folder INPUT".into());
    }
    let url_arg = take_url(&mut args)?;
    if url_arg.is_some()
        && !args
            .iter()
            .all(|s| !s.starts_with("--") || s == "--pack" || s == "--help")
    {
        return Err("--url supports native visitor + optional --pack only; no local smoke/report substitution".into());
    }
    let site_arg = if let Some(index) = args.iter().position(|s| s == "--site") {
        if index + 1 >= args.len() {
            return Err("--site requires an explicitly selected folder path".into());
        }
        let value = PathBuf::from(args.remove(index + 1));
        args.remove(index);
        Some(value)
    } else {
        None
    };
    let pack_arg = if let Some(index) = args.iter().position(|s| s == "--pack") {
        if index + 1 >= args.len() {
            return Err("--pack requires a local TOML manifest path".into());
        }
        let value = PathBuf::from(args.remove(index + 1));
        args.remove(index);
        Some(value)
    } else {
        None
    };
    let smoke = args.first().is_some_and(|s| s == "--headless-smoke");
    let output_arg = if smoke || folder_proof {
        if args.len() != 3
            || args[1] != "--smoke-output"
            || (folder_proof && args[0] != "--folder-flow-proof")
        {
            return Err("headless smoke output requires --headless-smoke --smoke-output /absolute/existing/separate/directory; Resources are read-only".into());
        }
        Some(Path::new(&args[2]))
    } else {
        if args.len() > 1
            || args.first().is_some_and(|s| {
                !["--window-smoke", "--resource-report", "--help"].contains(&s.as_str())
            })
        {
            return Err("usage: hair [--window-smoke|--resource-report|--help] or hair --headless-smoke --smoke-output /absolute/existing/separate/directory".into());
        }
        None
    };
    if args.first().is_some_and(|s| s == "--help") {
        println!(
            "Reader scrolling: Up/Down scroll one row and repeat while held (native key repeat; after focus loss press again). Mouse wheel scrolls 3 rows per notch; trackpad pixels accumulate smoothly; both clamp to the content and are ignored outside an open reader, in the paused menu, address entry, chooser and image viewer. Web reader keeps <pre> code blocks exactly (indentation, blank lines, case, punctuation) with passive colouring for Python, Rust, JavaScript/TypeScript, C/C++, shell, JSON and HTML/XML; other languages stay plain. Code rows show '|' at a source line start and '>' on soft-wrapped continuation rows; the text itself is never rewritten. In web rooms, M opens/closes the full-screen route map and Escape closes it before the pause menu. Shift+M independently toggles the minimap. Arrow keys pan the full-screen map. The map uses schematic preset room shapes, stable room numbers, recent traversal and the held-history Back destination; no exact position marker. File tree/link listing visibility is in Settings, not M; reader/address/menu input remains owned by those contexts. Compact minimaps yield to actionable door disclosure; the modal full-screen map covers other HUD layers. WEB LINKS/HISTORY: '<' visitor history (R), '=' same-page held-response rooms, '>' current page, '*' visited and '-' unvisited links, '^' other origin; no crawl or preload. The rear-wall Return door is the same typed history return as R and names the previous page when one exists."
        );
        println!(
            "Web feedback: green/teal Loading… reflects real pending work; R cancels. Red failure swirl lasts at most two seconds without delaying restoration; reason persists. F8 toggles exact destination/error details in the world; Up/Down scroll. Reader/address retain their controls: Escape closes that context before F8 (and before R while typing). HAIR_STATIC_WEB_FEEDBACK=1 freezes the swirl; no audio, strobe or success wait. Feedback replaces only the selected-portal preview while visible; it is attempt-owned, not tied to a mutable door slot."
        );
        println!(
            "HAIR {}\n--url https://host/page: bounded static HTML spatial reader. L opens transient HTTPS address entry; type the literal URL, Enter loads, Backspace edits, Up/Down scroll a long entry, Escape dismisses. Address focus isolates world controls and is dismissed on focus loss. HTTPS/public DNS only; no scripts/forms/login/styles execution or recursive preload. Tab/Shift+Tab cycles destinations inside the aimed grouped doorway; E reads/follows exactly the shown destination. Door panels show the source-link title, exact ASCII address (one too long for the panel keeps its start and exact end with an omitted-character count; reader Tab shows it whole) and a labelled abstract URL-fingerprint placeholder (not a site image); no preview pages/images are fetched. Reader Tab/E selects/follows labelled HTTPS links; R returns retained visitor history or cancels; F5 refreshes; Settings > Appearance selects local pack vocabulary. Acquisition/pack preparation run on one background worker; unfinished/failed destinations retain last good place. Initial load retains labelled bundled entry until ready. At most 8 retained history pages, 64 link records/page, 8 fixed grouped doorway slots/page (all supported full targets retained). Optional handmade adapter/Markdown/live hosting unqualified.\n--site /selected/folder: bounded local Markdown + PNG, restart to reload.\n--pack /selected/pack.toml: local data-only pack, invalid pack falls back to office.\nWASD move; click mouse-look; E interacts with the near, aimed, visible green document or local portal (crosshair prompt). F is disabled. Hold/repeat cannot chain actions; release E to re-arm. Tab/Shift+Tab cycles eligible nearby world targets, with a subtle current-selection highlight; aim is used when no valid explicit target is selected. Reader: Tab/Shift+Tab selects actual document links, E follows supported local pages, Up/Down scroll, Escape closes; world movement/R/target cycling remain isolated. External/image/unsupported links are labelled and not followed. Tab requires physical release and repeat/duplicate presses cannot cycle. M shows/hides the informational site outline; M requires release, ignores repeat/unfocused input and preserves page/reader/selection. Collapse survives reskins and local navigation. R returns to entry outside the reader; Settings > Appearance selects reskins. Lost focus clears movement and actions. Theme switch retains shared scene/anchors/collision, camera pose and page/reader/target, and releases mouse.\nEscape closes address entry, then reader, then opens the paused menu (Resume, Settings, Quit; Up/Down/Tab/Shift+Tab + Enter or click; Escape backs out). Q opens the menu with Quit focused; Enter/click confirms. Settings: UI scale 100-200% for HUD/reader/address/menus (world labels unchanged), session only; small windows apply less and say so.\n--headless-smoke --smoke-output /absolute/existing/separate/directory renders three GPU images without overwriting captures; --window-smoke attempts eight native frames.\n--resource-report validates and prints resolved paths without a GPU/window.\nHAIR_RESOURCE_ROOT is an explicit absolute development/test override; otherwise use Contents/Resources beside the bundle executable. No development/cwd fallback.\nBoth smokes are finite; neither substitutes for human input playtesting.",
            env!("CARGO_PKG_VERSION")
        );
        println!(
            "--folder PATH: native read-only folder explorer; --resource-report lists the immediate snapshot (cap {}). Ordinary launch is bundled DEMO, no folder selected: press O for the macOS directory chooser. Folder keys: E inspect/enter aimed object; I full source metadata (or current directory); U Parent beyond start; R visited Back; F5 refresh; physical Next/Previous doors continue the SAME directory; PgUp/PgDn remain optional; M full-screen directory map; Shift+M minimap; listing in Settings; Settings > Appearance for packs; B inventory; X collects an owned file copy; hold Shift to run. Rear Parent/entry, front subfolders, side continuation doors, files in the middle. Reader: Left/Right 4096-byte chunks, V text/raw, I metadata/body, C retry SAME handle from zero, Up/Down scroll, Escape close; no world/directory/link actions. Regular-file stored bytes and supported UTF-8 interpretation regardless of extension; static bounded 8-bit PNG E-viewer recognized from bytes on one background worker (4 MiB encoded, 1024 axes). V image/raw/text, I source/body, Escape close; other files retain raw/text. No playback, decryption or execution. L inactive in folder mode. No before-E image/media/model/PDF preview in this slice. ~ marks shortened HUD labels; I opens scrollable exact escaped OS names/paths. --folder INPUT --folder-flow-proof --smoke-output ABS_EXISTING_EMPTY_OUTPUT runs the shared production adapter and renderer with synthetic input, NOT native-human input.",
            hair::local_explorer::MAX_ENTRIES
        );
        return Ok(());
    }
    if folder_arg.is_some() && (smoke || args.first().is_some_and(|s| s == "--window-smoke")) {
        return Err(
            "--folder uses native visitor or --folder-flow-proof, not bundled smoke substitution"
                .into(),
        );
    }
    if folder_arg.is_some() {
        println!(
            "HAIR {} read-only local folder snapshot; bundled resources validated separately",
            env!("CARGO_PKG_VERSION")
        );
    } else {
        println!(
            "HAIR {} read-only visitor; local bundled entry retained while requested web page loads",
            env!("CARGO_PKG_VERSION")
        );
    }
    let executable = std::env::current_exe().map_err(|e| format!("current executable: {e}"))?;
    let override_root = std::env::var_os("HAIR_RESOURCE_ROOT");
    let root = resources::resolve_root(&executable, override_root.as_deref().map(Path::new))?;
    println!("RESOURCE_ROOT={}", root.display());
    if site_arg.is_none() {
        for path in resources::paths(&root).map_err(|e| resources::resource_error(&root, e))? {
            println!("RESOURCE_PATH={}", path.display());
        }
    }
    let world = if let Some(site) = site_arg.as_deref() {
        println!("CONTENT_ROOT={}", site.display());
        World::load_site(site)?
    } else {
        World::load(&root).map_err(|e| resources::resource_error(&root, e))?
    };
    let host = if let Some(spaces) = &world.spaces {
        spaces.space(&world.current_space)?.scene.clone()
    } else if let Some(site) = site_arg.as_deref() {
        hair::host_scene::HostScene::load_site(site)?
    } else {
        let host = hair::host_scene::HostScene::load(&root)
            .map_err(|e| resources::resource_error(&root, e))?;
        println!("RESOURCE_PATH={}", root.join("host-scene.toml").display());
        host
    };
    let mut cache = hair::theme::AssetCache::default();
    let mut themes = Vec::new();
    for name in ["packs/office.toml", "packs/archive.toml"] {
        let path = root.join(name);
        let theme = hair::theme::Theme::load_for_scene(&path, &mut cache, &host).map_err(|e| {
            resources::resource_error(&root, format!("built-in pack {}: {e}", path.display()))
        })?;
        // Mandatory bundled vocabulary is not an optional user role replacement.
        // Never certify a damaged app as healthy because a placeholder can render.
        if theme
            .diagnostics
            .iter()
            .any(|d| d.starts_with("ROLE_FALLBACK"))
        {
            return Err(resources::resource_error(
                &root,
                format!(
                    "required built-in pack {} incomplete: {}",
                    path.display(),
                    theme.diagnostics.join("; ")
                ),
            ));
        }
        themes.push(theme);
        println!("RESOURCE_PATH={}", path.display());
    }
    let mut theme_index = 0;
    if let Some(path) = pack_arg.as_deref() {
        match hair::theme::Theme::load_for_scene(path, &mut cache, &host) {
            Ok(theme) => {
                themes.push(theme);
                theme_index = themes.len() - 1;
            }
            Err(e) => {
                themes[0].load_failed = true;
                if e.contains("SKY_REFUSED") {
                    themes[0].diagnostics.push(e.clone());
                }
                eprintln!(
                    "PACK_FALLBACK: {}: {e}; keeping built-in office; supply a supported local TOML+GLB pack",
                    path.display()
                );
            }
        }
    }
    println!(
        "THEME={} ASSET_LOADS={}",
        themes[theme_index].name, cache.loads
    );
    println!(
        "PACK_FEEDBACK {} host={}",
        themes[theme_index].feedback(&world).replace('\n', " | "),
        world
            .spaces
            .as_ref()
            .map(|s| s
                .spaces
                .iter()
                .map(|s| s.scene.anchors.iter().filter(|a| a.role != "image").count())
                .sum::<usize>())
            .unwrap_or_else(|| host.anchors.iter().filter(|a| a.role != "image").count())
    );
    drop(cache);
    if let Some(folder) = folder_arg
        .as_deref()
        .filter(|_| args.first().is_some_and(|s| s == "--resource-report"))
    {
        // All mandatory resource/scene/pack validation above remains in force.
        // Bundled pages are app-health inputs here, not the selected folder's contents.
        let snapshot = hair::local_explorer::snapshot(folder)?;
        println!(
            "LOCAL_FOLDER={}",
            hair::local_explorer::escape_display(snapshot.folder.as_os_str())
        );
        for entry in &snapshot.entries {
            println!(
                "LOCAL_ENTRY kind={} name={}",
                entry.kind.label(),
                hair::local_explorer::escape_display(&entry.name)
            );
        }
        println!("RESOURCE_REPORT_OK");
        return Ok(());
    }
    for page in &world.graph.pages {
        println!("CONTENT_PAGE={} NEXT={}", page.id, page.next);
    }
    if args.first().is_some_and(|s| s == "--resource-report") {
        println!("RESOURCE_REPORT_OK");
        return Ok(());
    }
    if let Some(output) = output_arg {
        let location = output
            .canonicalize()
            .map_err(|e| format!("smoke output {}: {e}", output.display()))?;
        let selected_roots = site_arg
            .iter()
            .chain(folder_arg.iter())
            .map(|p| p.as_path())
            .chain(pack_arg.iter().filter_map(|p| p.parent()));
        for selected in selected_roots {
            if let Ok(selected) = selected.canonicalize()
                && (location.starts_with(&selected) || selected.starts_with(&location))
            {
                return Err(
                    "smoke output must be separate from selected content/pack roots and their ancestors"
                        .into(),
                );
            }
        }
        if folder_proof {
            let pack_roots = pack_arg
                .as_ref()
                .map(|path| {
                    path.canonicalize()
                        .map_err(|e| format!("proof selected pack: {e}"))
                })
                .transpose()?;
            if let Some(pack) = pack_roots.as_ref().and_then(|p| p.parent())
                && (location.starts_with(pack) || pack.starts_with(&location))
            {
                return Err("folder proof output overlaps resolved custom pack root".into());
            }
            if std::fs::read_dir(&location)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
            {
                return Err("folder proof requires an existing empty output directory".into());
            }
        }
        let output = resources::smoke_output(&root, output)?;
        println!("SMOKE_OUTPUT={}", output.path().display());
        if folder_proof {
            let app = App::new(root, themes, theme_index, world, false);
            return pollster::block_on(folder_proof::run(
                app,
                folder_arg.as_deref().ok_or("missing input folder")?,
                &output,
            ));
        }
        return pollster::block_on(headless(&output, world, &themes, theme_index))
            .map_err(|e| resources::resource_error(&root, e));
    }
    let smoke = args.first().is_some_and(|s| s == "--window-smoke");
    let events = EventLoop::new().map_err(|e| format!("native event loop: {e}"))?;
    let mut app = App::new(root, themes, theme_index, world, smoke);
    if !smoke {
        app.restore_native_preferences();
    }
    if let Some(folder) = folder_arg {
        app.complete_folder(Ok(Some(folder)))?;
    }
    if let Some(address) = url_arg {
        app.web = Some(web_native::Runtime::new(Vec::new(), hair::web::fetch));
        app.request_web(&address.original, web_native::Mode::Initial)?;
    }
    let native_result = events
        .run_app(&mut app)
        .map_err(|e| format!("native run: {e}"));
    // Blocking retirement is strictly outside all winit event handlers.
    let mut documents_settled = app.shutdown_documents(Duration::from_millis(250));
    if let Some(web) = &mut app.web {
        web.shutdown();
    }
    // Other post-loop retirement may allow a timed-out Documents job to finish.
    // Harvest it without adding another wait or dropping a completed held Store.
    if !documents_settled { documents_settled = app.shutdown_documents(Duration::ZERO); }
    println!("DOCUMENTS_SHUTDOWN settled={documents_settled}; {}", app.local_status);
    native_result?;
    if let Some(error) = app.error {
        return Err(error);
    }
    if smoke && app.frames != 8 {
        return Err(format!("native smoke incomplete: {} frames", app.frames));
    }
    println!(
        "{} native frames={} synthetic_actions={} human_input_unverified",
        if documents_settled { "CONTROLLED_TEARDOWN" } else { "UNRESOLVED_TEARDOWN Documents; preserve evidence; see DOCUMENTS_SHUTDOWN" }, app.frames, smoke
    );
    Ok(())
}
async fn headless(
    output: &resources::SmokeOutput,
    mut world: World,
    themes: &[hair::theme::Theme],
    mut index: usize,
) -> Result<()> {
    let instance = render::instance();
    let adapter = render::adapter(&instance, None).await?;
    let mut renderer = render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [960, 640],
        &themes[index].vertices_for(&world),
    )
    .await?;
    renderer.set_theme_appearance(&themes[index])?;
    renderer.error_probe().await?;
    let mut pose = Pose::default();
    themes[index].respawn(&mut pose);
    pose.position[0] += 0.25;
    pose.yaw = 0.12;
    pose.pitch = -0.2;
    // Select a real local link (or explicitly labelled collection target), never HTTP/image.
    if world.site.is_some()
        && let Some(target) = world
            .targets()
            .iter()
            .position(|l| matches!(l.destination, hair::content::Destination::Page(_)))
    {
        world.selected = target;
    }
    themes[index].act(&mut world, Action::Navigate, themes[index].portal);
    renderer.set_scene(&themes[index].vertices_for(&world));
    renderer.capture(output, "office.png", &pose, &world)?;
    println!(
        "CAPTURE_BINDING office.png theme={} page={} spatial=true pose={pose:?}",
        themes[index].name, world.current
    );
    themes[index].act(&mut world, Action::Inspect, themes[index].document);
    if !world.reading {
        return Err("smoke inspect failed".into());
    }
    renderer.set_scene(&themes[index].vertices_for(&world));
    renderer.capture(output, "reader-welcome.png", &pose, &world)?;
    println!(
        "CAPTURE_BINDING reader-welcome.png theme={} page={} source={:?}",
        themes[index].name,
        world.current,
        world.document()?
    );
    let before = (
        world.current.clone(),
        world.document()?.to_string(),
        world.target_description(),
        world.reading,
        world.selected,
    );
    let old_index = index;
    let pose_before = (pose.position, pose.yaw, pose.pitch);
    switch_theme(themes, &mut index, &mut pose, &mut renderer, &world)?;
    if pose_before != (pose.position, pose.yaw, pose.pitch)
        || themes[old_index].host != themes[index].host
    {
        return Err("shared reskin changed pose or scene".into());
    }
    for x in -64..=64 {
        for z in -64..=64 {
            let p = [x as f32 * 0.25, z as f32 * 0.25];
            if themes[old_index].blocked(p) != themes[index].blocked(p) {
                return Err("shared reskin changed collision".into());
            }
        }
    }
    println!(
        "SHARED_RESKIN_PRESERVED pose=true scene=true anchors=true collision=true before={pose_before:?} after={pose:?}"
    );
    if before
        != (
            world.current.clone(),
            world.document()?.to_string(),
            world.target_description(),
            world.reading,
            world.selected,
        )
    {
        return Err("theme switch corrupted current content".into());
    }
    println!(
        "THEME_SWITCH_PRESERVED page={} target={:?} reading={} theme={} pose=preserved-shared-host",
        world.current,
        world.target_description(),
        world.reading,
        themes[index].name
    );
    // Capture the second spatial interpretation visibly; reader preservation was checked above.
    world.reading = false;
    renderer.capture(output, "reader-workshop.png", &pose, &world)?;
    println!(
        "CAPTURE_BINDING reader-workshop.png theme={} page={} spatial=true",
        themes[index].name, world.current
    );
    renderer.wait()?;
    drop(renderer);
    drop(adapter);
    drop(instance);
    println!(
        "GPU_FRAMES=3; synthetic abstract actions; headless, not native input\nCONTROLLED_TEARDOWN headless"
    );
    Ok(())
}
fn interact(theme: &hair::theme::Theme, world: &mut World, pose: &Pose) {
    hair::interaction::refresh(&theme.host, pose, world);
    if let Some(target) = &world.interaction {
        let action = match target.kind {
            hair::interaction::Kind::Read => Action::Inspect,
            hair::interaction::Kind::Enter => Action::Navigate,
        };
        theme.act(world, action, pose.position);
        hair::interaction::refresh(&theme.host, pose, world);
    }
}

fn switch_theme(
    themes: &[hair::theme::Theme],
    index: &mut usize,
    pose: &mut Pose,
    renderer: &mut render::Renderer,
    world: &World,
) -> Result<()> {
    let next = (*index + 1) % themes.len();
    themes[next].reskin_from(&themes[*index], pose)?;
    renderer.set_theme_appearance(&themes[next])?;
    renderer.set_scene(&themes[next].vertices_for(world));
    *index = next;
    println!(
        "THEME={} page={} pose retained; shared host scene/anchors/collision and content/reader/target retained",
        themes[*index].name, world.current
    );
    Ok(())
}
#[cfg(test)]
mod folder_continuation_tests;
#[cfg(test)]
mod folder_hud_tests;
#[cfg(test)]
mod folder_qualification_tests;
#[cfg(test)]
mod folder_reader_tests;
#[cfg(test)]
mod folder_room_flow_tests;
#[cfg(test)]
mod local_audio_playback_tests;
#[cfg(test)]
mod local_capability_help_tests;
#[cfg(test)]
mod local_image_placard_tests;
#[cfg(test)]
mod local_image_preview_tests;
#[cfg(all(test, target_os = "macos"))]
mod local_media_room_preview_tests;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod local_model_fallback_tests;
#[cfg(test)]
mod local_model_presentation_tests;
#[cfg(test)]
mod local_model_preview_tests;
#[cfg(test)]
mod local_text_excerpt_tests;
#[cfg(test)]
mod local_video_still_tests;
#[cfg(test)]
mod player_app_tests;
#[cfg(test)]
mod presentation_menu_tests;
#[cfg(test)]
mod raw_reader_first_regression;
#[cfg(test)]
mod raw_reader_flow_tests;
#[cfg(test)]
mod reader_scroll_tests;
#[cfg(test)]
mod sprint_tests;

mod exploration_native;
mod place_mode_native;
mod placement_editor_native;
mod place_decoration_native;
mod recovery_native;
struct App {
    audio: folder_native::audio::Controller,
    images: folder_native::image::Runtime,
    object_previews: folder_native::object_preview::Runtime,
    local: Option<folder_native::Local>,
    local_status: String,
    local_held: folder_native::Held,
    chooser_pending: bool,
    chooser_active: bool,
    web: Option<web_native::Runtime>,
    directory_map: std::sync::Arc<hair::directory_map::Map>,
    map_selection: Option<hair::directory_map::Confirmation>,
    map_mouse_held: bool,
    sidebar_snapshot: Option<std::sync::Arc<hair::local_explorer::Snapshot>>,
    sidebar_offset: usize,
    sidebar_wheel: f64,
    sidebar_press: Option<(
        usize,
        std::sync::Arc<hair::local_explorer::Snapshot>,
        hair::sidebar::Frame,
    )>,
    map_history_offset: usize,
    map_confirm_travel: bool,
    map_confirm_ready_at: Instant,
    exploration: std::sync::Arc<hair::exploration::Map>,
    map_mode: hair::exploration::MapMode,
    minimap_enabled: bool,
    route_pan: [i32; 2],
    web_completion: web_native::FeedbackRecord,
    web_details: bool,
    web_detail_scroll: usize,
    web_details_held: bool,
    static_web_feedback: bool,
    root: PathBuf,
    // Acquired once from the actual bootstrap resource root. Never rebound by
    // reopening a display pathname during preview or locked publication.
    resources_directory: Result<hair::local_explorer::Explorer>,
    themes: Vec<hair::theme::Theme>,
    theme_index: usize,
    world: World,
    pose: Pose,
    player: hair::player::Player,
    jump_held: bool,
    input: Input,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    renderer: Option<render::Renderer>,
    smoke: bool,
    frames: u32,
    frame_attempts: u32,
    started: Instant,
    previous: Instant,
    scroll: usize,
    error: Option<String>,
    focused: bool,
    interact_held: bool,
    tab_held: bool,
    tree_held: bool,
    shifts: [bool; 2],
    address: Option<String>,
    address_error: Option<String>,
    address_held: bool,
    address_escape_held: bool,
    /// Address-entry rows scrolled up from the cursor tail; edits reset it.
    address_scroll: usize,
    /// Paused presentation menu; owns all keys/clicks while open.
    menu: Option<hair::presentation::Menu>,
    /// Keys seen down while the menu was open stay inert until physical release.
    menu_held: folder_native::Held,
    /// Visitor presentation state, never part of a public world/host schema.
    ui_scale: u16,
    fog: hair::fog::Fog,
    preferences: Option<hair::acquisition::PreferenceFile>,
    storage: storage_native::State,
    appearance: hair::appearance::Policy,
    inventory: hair::inventory::Inventory,
    inventory_open: bool,
    placement_action: Option<placement_native::Action>,
    place_mode: Option<place_mode_native::Mode>,
    place_p_held: bool,
    recovery_enter_held: bool,
    inventory_selection: usize,
    inventory_yaw: f32,
    inventory_drag: hair::inventory_pointer::Drag,
    collection_feedback: Option<(Instant, hair::collection::Effect)>,
    /// Last physical pointer position inside the window.
    cursor: Option<[f64; 2]>,
    /// Up/Down repeats suppressed until a fresh press (set by focus loss).
    reader_arrows_suppressed: [bool; 2],
    /// Fractional reader rows from wheel/trackpad deltas, bounded and finite.
    wheel_rows: f64,
}
impl App {
    fn new(
        root: PathBuf,
        themes: Vec<hair::theme::Theme>,
        theme_index: usize,
        mut world: World,
        smoke: bool,
    ) -> Self {
        world.tree_collapsed = true;
        let mut pose = Pose::default();
        themes[theme_index].respawn(&mut pose);
        if let (Some(spaces), Some(site)) = (&world.spaces, &world.site) {
            pose = spaces
                .destination(
                    &spaces.entry_space,
                    &spaces.entry_page,
                    &spaces.entry_arrival,
                    site,
                )
                .expect("validated entry")
                .1;
        }
        let mut exploration = hair::exploration::Map::default();
        exploration_native::observe(
            &mut exploration,
            &world,
            &themes[theme_index].host,
            hair::exploration::Connection::Portal,
            false,
        );
        Self {
            audio: Default::default(),
            images: Default::default(),
            object_previews: Default::default(),
            local: None,
            local_status: String::new(),
            local_held: Default::default(),
            chooser_pending: false,
            chooser_active: false,
            web: None,
            directory_map: Default::default(),
            map_selection: None,
            map_mouse_held: false,
            sidebar_snapshot: None,
            sidebar_offset: 0,
            sidebar_wheel: 0.0,
            sidebar_press: None,
            map_history_offset: 0,
            map_confirm_travel: false,
            map_confirm_ready_at: Instant::now(),
            exploration: std::sync::Arc::new(exploration),
            map_mode: Default::default(),
            minimap_enabled: true,
            route_pan: [0, 0],
            web_completion: Default::default(),
            web_details: false,
            web_detail_scroll: 0,
            web_details_held: false,
            // Explicit process-local static alternative; never reads publisher settings.
            static_web_feedback: std::env::var_os("HAIR_STATIC_WEB_FEEDBACK")
                .is_some_and(|s| s == "1"),
            resources_directory: hair::local_explorer::Explorer::new(&root),
            root,
            themes,
            theme_index,
            world,
            pose,
            player: Default::default(),
            jump_held: false,
            input: Input::default(),
            window: None,
            surface: None,
            config: None,
            renderer: None,
            smoke,
            frames: 0,
            frame_attempts: 0,
            started: Instant::now(),
            previous: Instant::now(),
            scroll: 0,
            error: None,
            focused: true,
            interact_held: false,
            tab_held: false,
            tree_held: false,
            shifts: [false; 2],
            address: None,
            address_error: None,
            address_held: false,
            address_escape_held: false,
            address_scroll: 0,
            menu: None,
            menu_held: Default::default(),
            ui_scale: 100,
            fog: Default::default(),
            preferences: None,
            storage: Default::default(),
            appearance: Default::default(),
            inventory: Default::default(),
            inventory_open: false,
            placement_action: None,
            place_mode: None,
            place_p_held: false,
            recovery_enter_held: false,
            inventory_selection: 0,
            inventory_yaw: 0.0,
            inventory_drag: Default::default(),
            collection_feedback: None,
            cursor: None,
            reader_arrows_suppressed: [false; 2],
            wheel_rows: 0.0,
        }
    }
    /// Prepare and validate all destination state before replacing last-good state.
    fn navigate_explicit(&mut self, space: &str, page: &str, arrival: &str) -> Result<()> {
        let graph = self.world.spaces.as_ref().ok_or("no explicit spaces")?;
        let site = self.world.site.as_ref().ok_or("no local content")?;
        graph.validate(site)?;
        let (destination, pose) = graph.destination(space, page, arrival, site)?;
        let mut cache = hair::theme::AssetCache::default();
        let prepared = self
            .themes
            .iter()
            .map(|t| {
                let mut next =
                    hair::theme::Theme::load_for_scene(&t.source, &mut cache, &destination.scene)?;
                next.retain_load_failure(t);
                Ok(next)
            })
            .collect::<Result<Vec<_>>>()?;
        if let Some(renderer) = &mut self.renderer {
            renderer.set_theme_appearance(&prepared[self.theme_index])?;
        }
        self.themes = prepared;
        self.world.current_space = space.into();
        self.world.current = page.into();
        self.pose = pose;
        self.player = Default::default();
        self.world.reading = false;
        self.world.reader_link = None;
        self.world.explicit_target = None;
        self.world.interaction = None;
        self.world.selected = 0;
        self.scroll = 0;
        self.wheel_rows = 0.0;
        self.release();
        if let Some(renderer) = &mut self.renderer {
            renderer.set_scene(&self.themes[self.theme_index].vertices_for(&self.world));
        }
        println!(
            "SPACE_ARRIVAL space={space} page={page} arrival={arrival} pose={:?}",
            self.pose
        );
        Ok(())
    }
    fn explicit_interact(&mut self) -> Result<()> {
        if self.world.reading {
            let Some(selection) = self.world.selected_reader_link() else {
                return Ok(());
            };
            let hair::content::Destination::Page(page) = &selection.link.destination else {
                return Ok(());
            };
            let graph = self.world.spaces.as_ref().ok_or("no spaces")?;
            let s = graph.owner(page)?;
            let (space, page, arrival) = (s.id.clone(), page.clone(), s.default_arrival.clone());
            return self.navigate_explicit(&space, &page, &arrival);
        }
        // Resolve without changing selection on a failed transaction.
        let Some(target) = hair::interaction::resolve(
            &self.themes[self.theme_index].host,
            &self.pose,
            &self.world,
        ) else {
            return Ok(());
        };
        let graph = self.world.spaces.as_ref().ok_or("no spaces")?;
        let s = graph.space(&self.world.current_space)?;
        match target.kind {
            hair::interaction::Kind::Read => {
                let d = s
                    .documents
                    .iter()
                    .find(|d| d.anchor == target.anchor)
                    .ok_or("unbound document")?;
                let page = d.page.clone();
                if !s.pages.contains(&page)
                    || !self
                        .world
                        .site
                        .as_ref()
                        .ok_or("no site")?
                        .pages
                        .contains_key(&page)
                {
                    return Err("missing bound page".into());
                }
                self.world.current = page;
                self.world.reading = true;
                self.world.reader_link = None;
                self.world.interaction = None;
                self.scroll = 0;
                self.wheel_rows = 0.0;
                self.release();
                if let Some(renderer) = &mut self.renderer {
                    renderer.set_scene(&self.themes[self.theme_index].vertices_for(&self.world));
                }
                Ok(())
            }
            hair::interaction::Kind::Enter => {
                let d = s
                    .doors
                    .iter()
                    .find(|d| d.anchor == target.anchor)
                    .ok_or("unbound door")?;
                let (space, page, arrival) = (d.space.clone(), d.page.clone(), d.arrival.clone());
                self.navigate_explicit(&space, &page, &arrival)
            }
        }
    }
    fn interact_key(&mut self, down: bool, repeat: bool) {
        if !down {
            self.interact_held = false;
            return;
        }
        if !self.focused || repeat || self.interact_held {
            return;
        }
        self.interact_held = true;
        if self.local.is_some() {
            if let Err(error) = self.local_interact(false) {
                self.local_status = format!("Refused: {error}; last good folder retained");
            }
            return;
        }
        if self.web.is_some() {
            if let Err(e) = self.interact_web() {
                self.web.as_mut().unwrap().status =
                    format!("Refused: {e}; last good place retained");
            }
            return;
        }
        if self.world.spaces.is_some() {
            if let Err(e) = self.explicit_interact() {
                eprintln!("LOCAL_NAVIGATION_REFUSED: {e}; last good state retained");
            }
            return;
        }
        let before = self.world.current.clone();
        let was_reading = self.world.reading;
        if was_reading {
            self.world.follow_reader_link();
        } else {
            interact(&self.themes[self.theme_index], &mut self.world, &self.pose);
        }
        if self.world.reading && !was_reading {
            self.scroll = 0;
            self.release();
        }
        if self.world.current != before || self.world.reading != was_reading {
            self.wheel_rows = 0.0;
        }
        // A supported self-link is still a completed navigation transaction.
        // Failed/unsupported follows keep the reader open and never enter arrival.
        if self.world.current != before || (was_reading && !self.world.reading) {
            self.themes[self.theme_index].respawn(&mut self.pose);
            self.player = Default::default();
            if let Some(renderer) = &mut self.renderer {
                renderer.set_scene(&self.themes[self.theme_index].vertices_for(&self.world));
            }
            self.world.interaction = None;
            self.scroll = 0;
            println!("LOCAL_NAVIGATION completed; local page arrival");
        }
        if let Some(renderer) = &mut self.renderer {
            renderer.set_scene(&self.themes[self.theme_index].vertices_for(&self.world));
        }
    }
    fn web_visual_at(&self, now: Instant) -> render::web_feedback::Snapshot {
        use render::web_feedback::{Mode, Snapshot};
        use web_native::FeedbackPhase;
        let feedback = self.web_feedback(now);
        let mode = match feedback.phase {
            FeedbackPhase::Pending => Mode::Pending,
            FeedbackPhase::Failure if feedback.red_eligible => Mode::FailureRed,
            FeedbackPhase::Failure => Mode::FailureSettled,
            FeedbackPhase::Cancelled => Mode::Cancelled,
            FeedbackPhase::Idle | FeedbackPhase::Success => Mode::Hidden,
        };
        if mode == Mode::Hidden || self.local.is_some() {
            return Snapshot::default();
        }
        Snapshot::new(
            mode,
            feedback.destination,
            feedback.reason,
            (feedback.elapsed.as_secs_f64() % 120.0) as f32,
            feedback.cached,
            feedback.retiring,
            self.web_details,
            self.web_detail_scroll,
            self.static_web_feedback,
        )
    }
    fn inventory_overlay_for(&self, size: [u32; 2]) -> Option<Vec<hair::assets::Vertex>> {
        if self.storage.removal.is_some() { return Some(self.storage_panel(size)); }
        self.inventory_open.then(|| {
            let shape = self
                .inventory
                .items()
                .get(self.inventory_selection)
                .and_then(|item| self.inventory.preview(item.id))
                .map(|preview| preview.model.vertices.as_slice())
                .unwrap_or(&[]);
            let state = if self.inventory.is_busy() {
                    if self.document_activity().starts_with("Cancelling") { hair::inventory_ui::DocumentsState::PersistentCancelling }
                    else { hair::inventory_ui::DocumentsState::PersistentLoading }
                } else if self.documents_active() { hair::inventory_ui::DocumentsState::Persistent }
                else if self.storage.required { hair::inventory_ui::DocumentsState::RequiredButUnavailable }
                else { hair::inventory_ui::DocumentsState::SessionOnlyFixture };
            let selected = self.inventory.items().get(self.inventory_selection);
            hair::inventory_ui::overlay_documents(
                if self.storage.cache_blocked { &[] } else { self.inventory.items() },
                self.inventory_yaw,
                hair::sidebar::Frame::new(size, self.ui_scale, false).full,
                shape,
                hair::inventory_ui::Presentation {
                    state,
                    selection: selected.map(|_| self.inventory_selection),
                    representative: selected.is_some_and(|item| self.inventory.preview(item.id).is_none_or(|p| !p.content_derived || p.warning.is_some())),
                },
            )
        })
    }
    fn update_hud(&mut self) {
        let image_visible = self.local_image_visible();
        let visual = self.web_visual_at(Instant::now());
        let context_owned = self.world.reading || self.address.is_some() || self.inventory_open;
        let map_hover = self.directory_map_action(self.cursor);
        let sidebar_hover = self.sidebar_hit(self.cursor);
        let sidebar_enabled = self.sidebar_frame().left > 0;
        let collection_overlay = self.collection_ui_at(Instant::now());
        let mut placement_overlay = self.placement_ui();
        placement_overlay.extend(self.place_ui());
        let (editor_preview, editor_textures) = self.editor_render();
        if let Some((at, effect)) = self.place_mode.as_ref().and_then(|m| m.pickup.as_ref()) {
            let frame = self.inventory_frame();
            let target = [1. - 320. / frame.full[0] as f32, 1. - 296. / frame.full[1] as f32];
            placement_overlay.extend(effect.paint_to(at.elapsed().as_secs_f32(), false, target));
        }
        let inventory_overlay = self
            .renderer
            .as_ref()
            .and_then(|r| self.inventory_overlay_for(r.size));
        let decoration_footer = self.decoration_footer();
        let storage_overlay = self.renderer.as_ref().and_then(|r| {
            self.menu.is_some_and(|m| m.view == hair::presentation::View::Storage).then(|| self.storage_panel(r.size))
        });
        if let Some(window) = &self.window {
            window.set_cursor(if map_hover.is_some() || sidebar_hover.is_some() {
                winit::window::CursorIcon::Pointer
            } else {
                winit::window::CursorIcon::Default
            });
        }
        let item_textures = if self.editor_held() && self.place_context() {
            editor_textures
        } else if self.inventory_open {
            self.inventory
                .items()
                .get(self.inventory_selection)
                .and_then(|item| self.inventory.preview(item.id))
                .map(|p| p.model.textures.clone())
                .unwrap_or_default()
        } else {
            self.collection_feedback
                .as_ref()
                .map(|(_, effect)| effect.textures.clone())
                .unwrap_or_default()
        };
        if let Some(renderer) = &mut self.renderer {
            if let Err(e) = renderer.set_item_materials(&item_textures) {
                self.local_status = format!("Inventory preview refused: {e}");
                renderer.editor_preview.clear();
            } else {
                renderer.editor_preview = editor_preview;
            }
            renderer.ui_scale = self.ui_scale;
            renderer.set_fog(self.fog);
            renderer.menu = self.menu;
            renderer.storage_overlay = storage_overlay;
            renderer.collection_overlay = collection_overlay;
            renderer.placement_overlay = placement_overlay;
            renderer.appearance_policy = self.appearance.clone();
            let next = &self.themes[(self.theme_index + 1) % self.themes.len()];
            renderer.appearance_name = if next.unavailable_reason().is_some() {
                format!("NEXT UNAVAILABLE: {}", next.name)
            } else {
                self.themes[self.theme_index].name.clone()
            };
            renderer.inventory_overlay = inventory_overlay;
            renderer.route_pan = self.route_pan;
            renderer.route_back = self.web.as_ref().and_then(|w| w.back_room_id());
            renderer.map_selection = self.map_selection.clone();
            renderer.map_history_offset = self.map_history_offset;
            renderer.map_confirm_travel = self.map_confirm_travel;
            renderer.map_hover = map_hover;
            renderer.sidebar_snapshot = if self.local.is_some() {
                self.sidebar_snapshot.clone()
            } else {
                None
            };
            renderer.sidebar_enabled = sidebar_enabled;
            renderer.sidebar_offset = self.sidebar_offset;
            renderer.sidebar_hover = sidebar_hover;
            renderer.directory_map = (self.place_mode.is_none()
                && self.local.is_some()
                && !self.world.reading
                && self.address.is_none()
                && self.menu.is_none()
                && !self.inventory_open
                && self.placement_action.is_none())
            .then(|| (self.directory_map.clone(), self.map_mode));
            renderer.exploration = (self.place_mode.is_none()
                && self.web.is_some()
                && self.world.pocket.is_some()
                && !self.world.reading
                && self.address.is_none()
                && self.menu.is_none()
                && !self.inventory_open
                && self.placement_action.is_none())
            .then(|| (self.exploration.clone(), self.map_mode));
            renderer.set_web_feedback(&visual, context_owned);
            renderer.viewer_visible = image_visible;
            renderer.address_entry = self.address.as_ref().map(|s| {
                (
                    s.clone(),
                    self.address_error
                        .as_deref()
                        .map(render::web_feedback::display_status),
                    self.address_scroll,
                )
            });
            renderer.pack_feedback = if let Some(footer) = decoration_footer {
                footer
            } else if let Some(view) = &self.world.folder_view {
                format!(
                    "{} | {}\nU Parent | R Back | F5 refresh | I source | P Place Mode\n{}",
                    view.summary(),
                    self.themes[self.theme_index].name,
                    self.local_status
                )
            } else if let Some(web) = &self.web {
                format!(
                    "L address | {}\n{}",
                    render::web_feedback::display_status(&web.status),
                    self.themes[self.theme_index].name
                )
            } else {
                format!(
                    "O choose folder (no folder selected; bundled DEMO or explicit --site) | L HTTPS\n{} | {}",
                    self.themes[self.theme_index].feedback(&self.world),
                    self.local_status
                )
            };
            if (self.world.folder_view.is_some() || self.web.is_some())
                && let Some(sky) = self.themes[self.theme_index].sky_feedback()
            {
                renderer.pack_feedback.push('\n');
                renderer.pack_feedback.push_str(sky);
            }
        }
    }
    fn keyboard_input(
        &mut self,
        code: Option<KeyCode>,
        down: bool,
        repeat: bool,
        text: Option<&str>,
    ) -> Result<bool> {
        // Physical Space release survives address-entry and reader-mode routing.
        if self.storage_text_input(code, down, repeat, text) { return Ok(false); }
        if code == Some(KeyCode::Space) {
            if !down {
                self.local_held.remove(&KeyCode::Space);
            } else if self.address.is_some() {
                self.local_held.insert(KeyCode::Space);
            }
        }
        let editing = self.address.is_some();
        let exit = if let Some(code) = code {
            self.key(code, down, repeat)?
        } else {
            false
        };
        if editing
            && self.address.is_some()
            && down
            && !matches!(
                code,
                Some(
                    KeyCode::Enter
                        | KeyCode::NumpadEnter
                        | KeyCode::Escape
                        | KeyCode::Backspace
                        | KeyCode::Tab
                )
            )
            && let Some(text) = text
        {
            self.address_text(text);
        }
        if exit {
            self.discard_local_reader();
        }
        Ok(exit)
    }
    fn address_text(&mut self, text: &str) {
        if !self.focused {
            return;
        }
        let Some(raw) = &mut self.address else {
            return;
        };
        self.address_scroll = 0;
        if raw.len().saturating_add(text.len()) > 2048 || text.chars().any(char::is_control) {
            self.address_error = Some("Input rejected (2048-byte limit/no controls); Backspace to edit or Escape to restart".into());
            return;
        }
        raw.push_str(text);
    }
    // The native event path and deterministic executable proof use this same adapter.
    // true requests exit; rendering errors remain errors rather than swallowed actions.
    fn key(&mut self, code: KeyCode, down: bool, repeat: bool) -> Result<bool> {
        if code == KeyCode::Escape && down && !repeat && self.focused {
            self.cancel_documents();
        }
        if code == KeyCode::Enter {
            let held = self.recovery_enter_held;
            self.recovery_enter_held = down;
            if self.recovery_open() && down && (held || repeat) { return Ok(false); }
        }
        // Track Space before every modal early return, including direct adapter
        // callers. Context changes never turn a held key into a fresh jump.
        if code == KeyCode::Space {
            let fresh = down && !repeat && !self.jump_held;
            self.jump_held = down;
            if fresh && self.jump_owned() && self.input.captured {
                self.player.jump();
            }
        }
        if let Some(result) = self.place_key(code, down, repeat) {
            return result;
        }
        if let Some(result) = self.placement_key(code, down, repeat) {
            return result;
        }
        // The paused menu owns every key; keys held across it stay inert until release.
        if let Some(result) = self.menu_key(code, down, repeat) {
            return result;
        }
        if self.inventory_open {
            if !down {
                self.menu_held.remove(&code);
                self.release_latch(code);
                return Ok(false);
            }
            let fresh = self.menu_held.insert(code);
            if !self.focused || repeat || !fresh {
                return Ok(false);
            }
            if self.storage.removal.is_some() {
                match code {
                    KeyCode::Enter | KeyCode::NumpadEnter => self.confirm_document_removal(),
                    KeyCode::Escape | KeyCode::KeyB => { self.storage.removal = None; self.refresh_documents(); }
                    KeyCode::PageUp => self.storage.scroll = self.storage.scroll.saturating_sub(8),
                    KeyCode::PageDown => self.storage.scroll = self.storage.scroll.saturating_add(8),
                    _ => {}
                }
                return Ok(false);
            }
            match code {
                KeyCode::KeyP => {
                    if let Err(e) = self.begin_placement() {
                        self.local_status = format!("Placement refused: {e}");
                    }
                }
                KeyCode::Escape | KeyCode::KeyB => {
                    self.inventory_open = false;
                    self.release_navigation();
                }
                KeyCode::ArrowUp => {
                    self.inventory_drag.cancel();
                    self.inventory_selection = self.inventory_selection.min(self.inventory.items().len()).saturating_sub(1);
                    self.prepare_document_selection();
                }
                KeyCode::ArrowDown => {
                    self.inventory_drag.cancel();
                    self.inventory_selection = self.inventory_selection.checked_add(1).unwrap_or(0)
                        .min(self.inventory.items().len().saturating_sub(1));
                    self.prepare_document_selection();
                }
                KeyCode::Delete if self.inventory.is_persistent() => self.plan_document_removal(),
                KeyCode::F8 => self.open_storage(),
                KeyCode::ArrowLeft => {
                    self.inventory_yaw =
                        (self.inventory_yaw - 0.2).rem_euclid(std::f32::consts::TAU)
                }
                KeyCode::ArrowRight => {
                    self.inventory_yaw =
                        (self.inventory_yaw + 0.2).rem_euclid(std::f32::consts::TAU)
                }
                _ => {}
            }
            return Ok(false);
        }
        if self.map_mode == hair::exploration::MapMode::Expanded {
            if !down {
                self.menu_held.remove(&code);
                self.release_latch(code);
                return Ok(false);
            }
            let fresh = self.menu_held.insert(code);
            if matches!(code, KeyCode::ShiftLeft | KeyCode::ShiftRight) {
                self.shifts[usize::from(code == KeyCode::ShiftRight)] = self.focused;
                return Ok(false);
            }
            if !self.focused || repeat || !fresh {
                return Ok(false);
            }
            if self.map_selection.is_some() {
                match code {
                    KeyCode::Escape => self.map_selection = None,
                    KeyCode::Tab => self.map_confirm_travel = !self.map_confirm_travel,
                    KeyCode::Enter | KeyCode::NumpadEnter => {
                        if self.map_confirm_travel {
                            let _ = self.directory_map_confirm();
                        } else {
                            self.map_selection = None;
                        }
                    }
                    _ => {}
                }
                return Ok(false);
            }
            if self.local.is_some() && matches!(code, KeyCode::BracketLeft | KeyCode::BracketRight)
            {
                self.directory_history_scroll(code == KeyCode::BracketLeft);
                return Ok(false);
            }
            let limit = if self.local.is_some() {
                hair::directory_map::MAX_NODES as i32
            } else {
                24
            };
            match code {
                KeyCode::ArrowLeft => self.route_pan[0] = (self.route_pan[0] - 1).max(-limit),
                KeyCode::ArrowRight => self.route_pan[0] = (self.route_pan[0] + 1).min(limit),
                KeyCode::ArrowUp => self.route_pan[1] = (self.route_pan[1] - 1).max(-limit),
                KeyCode::ArrowDown => self.route_pan[1] = (self.route_pan[1] + 1).min(limit),
                _ => {}
            }
            if code == KeyCode::KeyM && !self.tree_held {
                self.tree_held = true;
                if self.shifts.iter().any(|v| *v) {
                    self.minimap_enabled = !self.minimap_enabled;
                    return Ok(false);
                }
                self.map_mode = if self.minimap_enabled {
                    hair::exploration::MapMode::Compact
                } else {
                    hair::exploration::MapMode::Hidden
                };
                self.release_navigation();
            } else if code == KeyCode::Escape {
                self.map_mode = if self.minimap_enabled {
                    hair::exploration::MapMode::Compact
                } else {
                    hair::exploration::MapMode::Hidden
                };
                self.release_navigation();
            }
            return Ok(false);
        }
        // F8 is nonmodal. Track physical presses/releases in every context so a
        // key held while typing/reading cannot become a fresh world action.
        let web_details_press = code == KeyCode::F8 && down && !self.web_details_held;
        if code == KeyCode::F8 {
            self.web_details_held = down && self.focused;
        }
        if code == KeyCode::KeyL && !down {
            self.address_held = false;
        }
        if code == KeyCode::Escape && self.address_escape_held {
            if !down {
                self.address_escape_held = false;
            }
            return Ok(false);
        }
        if self.address.is_some() {
            // Entry consumes actions, not physical release bookkeeping. Keep held-key
            // suppression until release, including keys pressed before entry opened.
            if !down {
                match code {
                    KeyCode::KeyE => self.interact_held = false,
                    KeyCode::Tab => self.tab_held = false,
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                        self.shifts[usize::from(code == KeyCode::ShiftRight)] = false;
                    }
                    _ => {}
                }
            }
            // Arrows held in entry stay inert in the reader below until a
            // physical release or fresh press, like focus loss.
            if down && matches!(code, KeyCode::ArrowUp | KeyCode::ArrowDown) {
                self.reader_arrows_suppressed[usize::from(code == KeyCode::ArrowUp)] = true;
            }
            if self.focused && down {
                match code {
                    KeyCode::Escape if !repeat => {
                        self.address = None;
                        self.address_error = None;
                        self.address_scroll = 0;
                        self.address_escape_held = true;
                        self.release();
                    }
                    // Held arrows inspect long input/error rows; the renderer
                    // clamps to the wrapped content, this only bounds the count.
                    KeyCode::ArrowUp => {
                        // Same logical viewport and displayed error as the renderer.
                        let [width, height] = self.ui_viewport();
                        let max = hair::text::address_scroll_max(
                            self.address.as_deref().unwrap_or_default(),
                            self.address_error
                                .as_deref()
                                .map(render::web_feedback::display_status)
                                .as_deref(),
                            width,
                            height,
                        );
                        self.address_scroll = self.address_scroll.saturating_add(1).min(max);
                    }
                    KeyCode::ArrowDown => {
                        self.address_scroll = self.address_scroll.saturating_sub(1);
                    }
                    KeyCode::Delete => {
                        self.address.as_mut().unwrap().clear();
                        self.address_error = None;
                        self.address_scroll = 0;
                    }
                    KeyCode::Backspace => {
                        self.address.as_mut().unwrap().pop();
                        self.address_error = None;
                        self.address_scroll = 0;
                    }
                    KeyCode::Enter | KeyCode::NumpadEnter
                        if !repeat && self.address_error.is_none() =>
                    {
                        let raw = self.address.as_ref().unwrap().clone();
                        let result = hair::web::Address::parse(&raw).and_then(|_| {
                            if self.web.is_none() {
                                self.web =
                                    Some(web_native::Runtime::new(Vec::new(), hair::web::fetch));
                            }
                            let mode = if self.world.current.starts_with("https://") {
                                web_native::Mode::Follow
                            } else {
                                web_native::Mode::Initial
                            };
                            self.request_web(&raw, mode)
                        });
                        match result {
                            Ok(()) => {
                                self.address = None;
                            }
                            Err(e) => {
                                let message = format!("Refused: {e}; last good place retained");
                                self.address_error = Some(message.clone());
                                if let Some(web) = &mut self.web {
                                    web.status = message;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            return Ok(false);
        }
        if self.reader_key(code, down, repeat) {
            return Ok(false);
        }
        if let Some(result) = self.folder_key(code, down, repeat) {
            return result;
        }
        if self.chooser_active || self.chooser_pending {
            if !down {
                if code == KeyCode::KeyE {
                    self.interact_held = false;
                }
                if code == KeyCode::Tab {
                    self.tab_held = false;
                }
            }
            return Ok(false);
        }
        if code == KeyCode::KeyB && down && !repeat && self.focused && !self.world.reading {
            self.release_navigation();
            self.refresh_documents();
            self.inventory_open = true;
            self.menu_held.insert(code);
            return Ok(false);
        }
        if code == KeyCode::KeyX {
            if !down {
                self.local_held.remove(&code);
            } else if self.focused && !self.world.reading && !repeat && self.local_held.insert(code)
            {
                if self.storage.cache_blocked || (self.storage.required && !self.inventory.is_persistent()) {
                    self.local_status = "Not collected: persistent Documents unavailable; open Player Settings > Storage settings. No silent session fallback.".into();
                    return Ok(false);
                }
                let index = hair::interaction::resolve(
                    &self.themes[self.theme_index].host,
                    &self.pose,
                    &self.world,
                )
                .and_then(|t| {
                    self.world
                        .folder_view
                        .as_ref()?
                        .bindings
                        .get(&t.anchor)
                        .map(|b| (b.index, t.anchor.clone()))
                });
                if let (Some((index, anchor)), Some(local)) = (index, self.local.as_ref()) {
                    self.collection_feedback = None;
                    if self.inventory.is_persistent() {
                        if let Err(error) = self.start_document_collect(index) { self.local_status = error; }
                        return Ok(false);
                    }
                    self.local_status = match self.inventory.collect(&local.explorer, index) {
                        Ok(id) => {
                            self.inventory_selection = self.inventory.items().iter().position(|item| item.id == id).unwrap_or(0);
                            self.storage.selected = Some(id);
                            let Some(preview) = self.inventory.preview(id) else {
                                self.local_status = "Collection committed; preview unavailable. Open Documents (B), F8 for details.".into();
                                return Ok(false);
                            };
                            let model = self.themes[self.theme_index]
                                .host
                                .anchors
                                .iter()
                                .find(|a| a.id == anchor)
                                .map(|a| hair::file_items::above_base(&preview.model.vertices, a))
                                .unwrap_or_default();
                            self.collection_feedback = hair::collection::Effect::new(
                                &model,
                                &self.pose,
                                self.sidebar_frame(),
                            )
                            .map(|mut effect| {
                                effect.textures = preview.model.textures.clone();
                                (Instant::now(), effect)
                            });
                            format!(
                                "Collected copy | INVENTORY {} items | original unchanged{}",
                                self.inventory.items().len(),
                                self.inventory.notice().map(|n| format!("; {n}")).unwrap_or_default()
                            )
                        }
                        Err(error) => if self.inventory.is_persistent() { error } else { format!("Not collected: {error}") },
                    };
                }
            }
            return Ok(false);
        }
        if self.local.is_some() && matches!(code, KeyCode::Escape | KeyCode::KeyT) {
            if !down {
                self.local_held.remove(&code);
                return Ok(false);
            }
            if repeat || !self.focused || !self.local_held.insert(code) {
                return Ok(false);
            }
        }
        if self.local.is_none()
            && !self.world.reading
            && matches!(code, KeyCode::F8 | KeyCode::ArrowUp | KeyCode::ArrowDown)
            && !matches!(
                self.web_feedback(Instant::now()).phase,
                web_native::FeedbackPhase::Idle | web_native::FeedbackPhase::Success
            )
        {
            if code == KeyCode::F8 {
                if self.focused && !repeat && web_details_press {
                    self.web_details = !self.web_details;
                    self.web_detail_scroll = 0;
                }
                return Ok(false);
            }
            if self.web_details {
                if down && self.focused {
                    if code == KeyCode::ArrowDown {
                        self.web_detail_scroll = self.web_detail_scroll.saturating_add(1).min(1024);
                    } else {
                        self.web_detail_scroll = self.web_detail_scroll.saturating_sub(1);
                    }
                }
                return Ok(false);
            }
        }
        if code == KeyCode::KeyL {
            if down && self.focused && !repeat && !self.address_held {
                self.address_held = true;
                self.address = Some(String::new());
                self.address_scroll = 0;
                self.release();
            }
            return Ok(false);
        }
        if self.web.is_some() && matches!(code, KeyCode::KeyR | KeyCode::F5) {
            let web = self.web.as_mut().unwrap();
            let held = if code == KeyCode::KeyR {
                &mut web.back_held
            } else {
                &mut web.refresh_held
            };
            if !down {
                *held = false;
                return Ok(false);
            }
            if !self.focused || repeat || *held {
                return Ok(false);
            }
            *held = true;
            let result = if code == KeyCode::KeyR {
                self.return_web()
            } else {
                let raw = self.world.current.clone();
                self.request_web(&raw, web_native::Mode::Refresh)
            };
            if let Err(e) = result {
                self.web.as_mut().unwrap().status =
                    format!("Refused: {e}; last good place retained");
            }
            return Ok(false);
        }
        if code == KeyCode::KeyM {
            if !down {
                self.tree_held = false;
            } else if self.focused && !repeat && !self.tree_held {
                self.tree_held = true;
                if ((self.web.is_some() && self.world.pocket.is_some()) || self.local.is_some())
                    && !self.world.reading
                {
                    if self.shifts.iter().any(|held| *held) {
                        self.minimap_enabled = !self.minimap_enabled;
                        self.map_mode = if self.minimap_enabled {
                            hair::exploration::MapMode::Compact
                        } else {
                            hair::exploration::MapMode::Hidden
                        };
                    } else {
                        self.map_mode = hair::exploration::MapMode::Expanded;
                        self.map_selection = None;
                        self.map_history_offset = 0;
                        self.route_pan = [0, 0];
                        self.release_navigation();
                        self.world.explicit_target = None;
                    }
                }
            }
            return Ok(false);
        }
        if code == KeyCode::KeyE {
            self.interact_key(down, repeat);
            return Ok(false);
        }
        if matches!(code, KeyCode::ShiftLeft | KeyCode::ShiftRight) {
            if self.focused {
                self.shifts[usize::from(code == KeyCode::ShiftRight)] = down;
                self.input.running = self.shifts.iter().any(|held| *held)
                    && !self.world.reading
                    && self.address.is_none()
                    && !self.chooser_pending
                    && !self.chooser_active;
            }
            return Ok(false);
        }
        if code == KeyCode::Tab {
            if !down {
                self.tab_held = false;
                return Ok(false);
            }
            if !self.focused || repeat || self.tab_held {
                return Ok(false);
            }
            self.tab_held = true;
            if self.world.reading {
                self.world.cycle_reader_link(self.shifts.iter().any(|v| *v));
            } else {
                hair::interaction::cycle_with_windows(
                    &self.themes[self.theme_index].host,
                    &self.pose,
                    &mut self.world,
                    self.shifts.iter().any(|v| *v),
                    self.web.as_ref().map_or(&[], |w| w.window_doors.as_slice()),
                );
                if let Some(renderer) = &mut self.renderer {
                    renderer.set_scene(&self.themes[self.theme_index].vertices_for(&self.world));
                }
            }
            return Ok(false);
        }
        if !self.focused {
            return Ok(false);
        }
        // Reader movement and world actions remain isolated.
        if self.world.reading
            && !matches!(
                code,
                KeyCode::Escape
                    | KeyCode::KeyQ
                    | KeyCode::KeyT
                    | KeyCode::ArrowDown
                    | KeyCode::ArrowUp
            )
        {
            return Ok(false);
        }
        match code {
            KeyCode::KeyW => self.input.forward = down,
            KeyCode::KeyS => self.input.back = down,
            KeyCode::KeyA => self.input.left = down,
            KeyCode::KeyD => self.input.right = down,
            // Every reader accepts held-key repeats (native OS repeat). Keep this
            // before the legacy repeat guard without changing other visitor controls.
            KeyCode::ArrowDown | KeyCode::ArrowUp if self.world.reading => {
                let index = usize::from(code == KeyCode::ArrowUp);
                if !down || !repeat {
                    self.reader_arrows_suppressed[index] = false;
                }
                if down && !self.reader_arrows_suppressed[index] {
                    self.scroll_reader(if index == 0 { 1 } else { -1 });
                }
            }
            _ if !down || repeat => {}
            KeyCode::Escape => {
                if self.world.reading {
                    self.world.act(Action::Close, self.pose.position);
                    self.wheel_rows = 0.0;
                    self.discard_local_reader();
                    // Closing an interpreted reader retires that generation,
                    // then lazily restores current-room objects without F5.
                    if let Some(view) = &self.world.folder_view {
                        self.object_previews.room(view);
                    }
                    self.release();
                } else if self.input.captured {
                    // Unlocking is its own action. menu_held consumes repeats
                    // (including duplicate non-repeat downs) until release.
                    self.release_navigation();
                    self.menu_held.insert(code);
                } else {
                    // Only a fresh Escape with a free pointer opens the menu.
                    self.open_menu(hair::presentation::Menu::pause(), code);
                }
            }
            KeyCode::KeyQ => {
                self.open_menu(hair::presentation::Menu::quit_confirm(), code);
            }
            KeyCode::KeyR => {
                if let Some(g) = &self.world.spaces {
                    let (s, p, a) = (
                        g.entry_space.clone(),
                        g.entry_page.clone(),
                        g.entry_arrival.clone(),
                    );
                    if let Err(e) = self.navigate_explicit(&s, &p, &a) {
                        eprintln!("ENTRY_REFUSED {e}");
                    }
                    return Ok(false);
                }
                self.world.return_to_entry();
                self.themes[self.theme_index].respawn(&mut self.pose);
                self.player = Default::default();
                self.scroll = 0;
                self.wheel_rows = 0.0;
                if let Some(renderer) = &mut self.renderer {
                    renderer.set_scene(&self.themes[self.theme_index].vertices_for(&self.world));
                }
            }
            _ => {}
        }
        Ok(false)
    }
    /// Moves the open reader by whole rows, clamped to its last content row in
    /// the same logical viewport the renderer lays it out in.
    fn scroll_reader(&mut self, rows: i64) {
        let [width, height] = self.ui_viewport();
        let max = hair::text::reader_scroll_max(&self.world, width, height);
        let target = (self.scroll.min(max) as i64).saturating_add(rows);
        self.scroll = target.clamp(0, max as i64) as usize;
    }
    /// Reader wheel/trackpad adapter; the native MouseWheel event calls only this.
    /// Lines scroll three rows; pixels accumulate against the drawn row pitch.
    /// World, menu, address, chooser, focus loss and image viewing never scroll.
    fn mouse_wheel(&mut self, delta: MouseScrollDelta) {
        if self.sidebar_scroll(delta) {
            return;
        }
        const WHEEL_ROWS_BOUND: f64 = 4096.0;
        if !self.focused
            || !self.world.reading
            || self.menu.is_some()
            || self.address.is_some()
            || self.chooser_pending
            || self.chooser_active
            || self.local_image_visible()
        {
            self.wheel_rows = 0.0;
            return;
        }
        let [width, height] = self.ui_viewport();
        let rows = match delta {
            MouseScrollDelta::LineDelta(x, y) if x.is_finite() && y.is_finite() => {
                -f64::from(y) * 3.0
            }
            MouseScrollDelta::PixelDelta(p) if p.x.is_finite() && p.y.is_finite() => {
                let physical = self.renderer.as_ref().map_or(640, |r| r.size[1]);
                let pitch = f64::from(hair::text::reader_row_height(&self.world, width, height))
                    * f64::from(physical)
                    / f64::from(height.max(1));
                -p.y / pitch.max(1.0)
            }
            _ => return,
        };
        self.wheel_rows = (self.wheel_rows + rows).clamp(-WHEEL_ROWS_BOUND, WHEEL_ROWS_BOUND);
        let whole = self.wheel_rows.trunc();
        self.wheel_rows -= whole;
        self.scroll_reader(whole as i64);
        let max = hair::text::reader_scroll_max(&self.world, width, height);
        // No backlog beyond either end: reversing direction responds at once.
        if (self.scroll == 0 && self.wheel_rows < 0.0)
            || (self.scroll >= max && self.wheel_rows > 0.0)
        {
            self.wheel_rows = 0.0;
        }
    }
    fn focus_changed(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.cancel_documents(); // Explicit existing focus-loss contract.
            self.storage.chooser = false;
            self.place_p_held = false;
            self.sidebar_press = None;
            self.sidebar_wheel = 0.0;
            self.map_selection = None;
            self.map_mouse_held = true;
            self.reader_arrows_suppressed = [true; 2];
            self.wheel_rows = 0.0;
            self.local_held.clear();
            // An open menu stays paused; only its physical-key bookkeeping resets.
            self.menu_held.clear();
            if !self.chooser_active {
                self.chooser_pending = false;
            }
            self.address = None;
            self.address_error = None;
            self.address_scroll = 0;
            self.address_held = false;
            self.address_escape_held = false;
            self.web_details_held = false;
            if let Some(web) = &mut self.web {
                web.back_held = false;
                web.refresh_held = false;
            }
            self.interact_held = false;
            self.tab_held = false;
            self.tree_held = false;
            self.shifts = [false; 2];
            self.world.explicit_target = None;
            self.world.interaction = None;
            if let Some(renderer) = &mut self.renderer {
                renderer.set_scene(&self.themes[self.theme_index].vertices_for(&self.world));
            }
            self.release();
        }
    }
    fn release(&mut self) {
        self.place_mode = None;
        self.place_scene_refresh();
        // Safety releases (focus/address/chooser/exit) also cancel audio intent.
        // No device calls here: redraw retires output; App-owned Controller Drop
        // performs final synchronous disposal/join when no more redraw is possible.
        self.stop_local_audio();
        self.release_navigation();
    }
    fn capture_look(&mut self) {
        self.input.release();
        self.shifts = [false; 2];
        self.sidebar_press = None;
        if let Some(window) = &self.window {
            match window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
            {
                Ok(()) => {
                    self.input.captured = true;
                    window.set_cursor_visible(false);
                }
                Err(e) => {
                    self.local_status =
                        format!("Mouse capture unavailable: {e}; use toolbox or Escape")
                }
            }
        }
        #[cfg(test)]
        if self.window.is_none() {
            self.input.captured = true;
        }
    }
    fn release_navigation(&mut self) {
        // Input ownership only: changing UI is not consent to cancel disk work.
        if let Some(mode) = &mut self.place_mode {
            mode.edit = None;
        }
        self.player.velocity = 0.0;
        self.inventory_drag.cancel();
        self.placement_action = None;
        self.collection_feedback = None;
        self.sidebar_press = None;
        self.sidebar_wheel = 0.0;
        self.input.release();
        self.shifts = [false; 2];
        if let Some(window) = &self.window {
            if let Err(e) = window.set_cursor_grab(CursorGrabMode::None) {
                eprintln!("cursor release: {e}");
            }
            window.set_cursor_visible(true);
        }
    }
    fn fail(&mut self, events: &ActiveEventLoop, error: String) {
        self.discard_local_reader();
        self.release();
        self.error = Some(error);
        events.exit();
    }
    fn initialize(&mut self, events: &ActiveEventLoop) -> Result<()> {
        let window = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_title(format!(
                            "HAIR {} - local office - Escape menu / settings / quit",
                            env!("CARGO_PKG_VERSION")
                        ))
                        .with_inner_size(winit::dpi::LogicalSize::new(960, 640))
                        .with_min_inner_size(winit::dpi::LogicalSize::new(640, 480)),
                )
                .map_err(|e| format!("native window: {e}"))?,
        );
        let instance = render::instance();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("native surface: {e}"))?;
        let adapter = pollster::block_on(render::adapter(&instance, Some(&surface)))?;
        let size = window.inner_size();
        let config = surface
            .get_default_config(
                &adapter,
                size.width.clamp(1, 4096),
                size.height.clamp(1, 4096),
            )
            .ok_or("no surface configuration")?;
        let mut renderer = pollster::block_on(render::Renderer::new(
            &adapter,
            config.format,
            [config.width, config.height],
            &self.themes[self.theme_index].vertices_for(&self.world),
        ))
        .map_err(|e| resources::resource_error(&self.root, e))?;
        renderer.set_theme_appearance(&self.themes[self.theme_index])?;
        surface.configure(&renderer.device, &config);
        println!(
            "NATIVE_WINDOW_CREATED size={}x{} format={:?}",
            config.width, config.height, config.format
        );
        self.window = Some(window);
        self.surface = Some(surface);
        self.config = Some(config);
        self.renderer = Some(renderer);
        self.previous = Instant::now();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        Ok(())
    }
    fn advance_motion(&mut self, dt: f32) {
        hair::player::fit_camera(&self.themes[self.theme_index].host, &mut self.pose);
        if self.motion_owned() {
            self.player.advance(
                &self.themes[self.theme_index].host,
                &mut self.pose,
                &self.input,
                dt,
            );
        }
    }
    fn motion_owned(&self) -> bool {
        self.focused
            && !self.recovery_open()
            && !(self.editor_held() && self.editor_phase() != "Positioning")
            && !self
                .place_mode
                .as_ref()
                .is_some_and(|m| m.edit.as_ref().is_some_and(|e| e.confirming()))
            && !self.world.reading
            && self.menu.is_none()
            && !self.inventory_open
            && self.address.is_none()
            && !self
                .placement_action
                .as_ref()
                .is_some_and(|a| a.pending.is_some())
            && !self.chooser_active
            && !self.chooser_pending
            && self.map_mode != hair::exploration::MapMode::Expanded
    }
    fn jump_owned(&self) -> bool {
        self.motion_owned() && self.placement_action.is_none() && self.place_mode.is_none()
    }
    fn redraw(&mut self, events: &ActiveEventLoop) -> Result<()> {
        self.poll_documents();
        self.poll_web()?;
        self.poll_local_image()?;
        if let (Some(window), Some(web)) = (&self.window, &self.web) {
            window.set_title(&format!(
                "HAIR | {} | {} | Escape menu / settings / quit",
                self.world.current, web.status
            ));
        }
        let dt = self.previous.elapsed().as_secs_f32();
        self.previous = Instant::now();
        self.advance_motion(dt);
        if self.smoke {
            match self.frames {
                2 => {
                    self.world.act(Action::Inspect, [0.0, 2.0]);
                    println!("NATIVE_SYNTHETIC_INSPECT reading={}", self.world.reading);
                }
                4 => {
                    self.world.act(Action::Close, [0.0, 2.0]);
                    self.world.act(Action::Navigate, [0.0, -3.0]);
                    println!("NATIVE_SYNTHETIC_NAVIGATION page={}", self.world.current);
                }
                5 => self.world.act(Action::Inspect, [0.0, 2.0]),
                _ => {}
            }
        }
        let old_target = self.world.interaction.clone();
        hair::interaction::refresh_with_windows(
            &self.themes[self.theme_index].host,
            &self.pose,
            &mut self.world,
            self.web.as_ref().map_or(&[], |w| w.window_doors.as_slice()),
        );
        // Only the active visible room spends render-only animation work. No
        // acquisition, host mutation or filesystem reconciliation occurs here.
        let animate_files = self.focused
            && self.place_context()
            && self.themes[self.theme_index].has_file_animation(&self.world);
        if old_target != self.world.interaction || animate_files {
            self.place_scene_refresh();
        }
        self.update_hud();
        let surface = self.surface.as_ref().ok_or("missing surface")?;
        let renderer = self.renderer.as_ref().ok_or("missing renderer")?;
        self.frame_attempts += 1;
        let acquired = surface.get_current_texture();
        if self.frame_attempts == 1 {
            println!("NATIVE_FIRST_ACQUIRE={acquired:?}");
        }
        let frame = match acquired {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated => {
                surface.configure(
                    &renderer.device,
                    self.config.as_ref().ok_or("missing config")?,
                );
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            error => {
                return Err(format!(
                    "surface acquire: {error:?}; restart to recreate lost surface"
                ));
            }
        };
        renderer.draw(
            &frame.texture.create_view(&Default::default()),
            &self.pose,
            &self.world,
            self.scroll,
        )?;
        self.window
            .as_ref()
            .ok_or("missing window")?
            .pre_present_notify();
        renderer.queue.present(frame);
        self.frames += 1;
        if self.claim_storage_request() {
            let selection = folder_dialog::present_storage();
            self.complete_storage_selection(selection);
            self.previous = Instant::now();
            return Ok(());
        }
        if self.claim_folder_request() {
            let selection = folder_dialog::present();
            if let Err(error) = self.complete_folder(selection) {
                self.local_status =
                    format!("Folder chooser refused: {error}; last good place retained");
            }
            self.previous = Instant::now();
            return Ok(());
        }
        if self.smoke && self.frames == 8 {
            self.renderer.as_ref().ok_or("missing renderer")?.wait()?;
            self.release();
            events.exit();
        }
        Ok(())
    }
}
impl ApplicationHandler for App {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.window.is_none()
            && let Err(error) = self.initialize(events)
        {
            self.fail(events, error);
        }
    }
    fn window_event(&mut self, events: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.discard_local_reader();
                self.release();
                events.exit();
            }
            WindowEvent::Occluded(value) => println!("NATIVE_OCCLUDED={value}"),
            WindowEvent::Focused(focused) => self.focus_changed(focused),
            WindowEvent::Resized(size) => {
                if let Some(mode) = &mut self.place_mode {
                    mode.cancel_pointer();
                }
                self.inventory_drag.cancel();
                self.sidebar_press = None;
                self.sidebar_wheel = 0.0;
                if size.width > 0
                    && size.height > 0
                    && let (Some(renderer), Some(surface), Some(config)) =
                        (&mut self.renderer, &self.surface, &mut self.config)
                {
                    config.width = size.width.min(4096);
                    config.height = size.height.min(4096);
                    renderer.resize([config.width, config.height]);
                    surface.configure(&renderer.device, config);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_moved(Some([position.x, position.y]));
            }
            WindowEvent::CursorLeft { .. } => self.pointer_moved(None),
            WindowEvent::MouseWheel { delta, .. } => self.mouse_wheel(delta),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } if self.place_mode.is_some() && self.place_context() => {
                self.place_pointer(state == ElementState::Pressed);
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } if self.inventory_open => {
                self.inventory_pointer(state == ElementState::Pressed);
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } if self.map_mode == hair::exploration::MapMode::Expanded && self.local.is_some() => {
                self.directory_map_pointer(self.cursor, state == ElementState::Pressed);
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                self.map_mouse_held = false;
                self.sidebar_pointer(false, self.cursor);
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            } if self.sidebar_pointer(true, self.cursor) => {}
            // The open menu owns clicks: physical pointer hit-tests the rendered
            // buttons; it never captures the cursor (Resume waits for a fresh click).
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if self.menu.is_some() => {
                if let Some(point) = self.cursor
                    && self.menu_click(point)
                {
                    self.discard_local_reader();
                    events.exit();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if self.focused
                && self.address.is_none()
                && self.menu.is_none()
                && !self.inventory_open
                && self.map_mode != hair::exploration::MapMode::Expanded
                && !self.world.reading
                && !self.smoke
                && !self.chooser_pending
                && !self.chooser_active =>
            {
                if let Some(window) = &self.window {
                    self.map_mouse_held = true;
                    match window
                        .set_cursor_grab(CursorGrabMode::Locked)
                        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                    {
                        Ok(()) => {
                            self.input.captured = true;
                            window.set_cursor_visible(false);
                        }
                        Err(e) => {
                            eprintln!("cursor capture unavailable: {e}; Escape/Q remain available")
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let code = match event.physical_key {
                    PhysicalKey::Code(c) => Some(c),
                    _ => None,
                };
                match self.keyboard_input(
                    code,
                    event.state == ElementState::Pressed,
                    event.repeat,
                    event.text.as_deref(),
                ) {
                    Ok(true) => events.exit(),
                    Ok(false) => {}
                    Err(error) => self.fail(events, error),
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw(events) {
                    self.fail(events, error);
                }
            }
            _ => {}
        }
    }
    fn device_event(
        &mut self,
        _events: &ActiveEventLoop,
        _id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.focused
            && self.map_mode != hair::exploration::MapMode::Expanded
            && !self.world.reading
            && self.menu.is_none()
            && !self.inventory_open
        {
            self.input.look(&mut self.pose, delta.0, delta.1);
        }
    }
    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        if self.smoke && self.started.elapsed() > Duration::from_secs(20) {
            self.fail(
                events,
                format!(
                    "native smoke deadline exceeded after {} frames",
                    self.frames
                ),
            );
            return;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        events.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
    }
    fn exiting(&mut self, _events: &ActiveEventLoop) {
        self.cancel_documents(); // Nonblocking request; post-loop helper settles.
        self.discard_local_reader();
        self.release();
        if let Some(renderer) = &self.renderer
            && let Err(error) = renderer.wait()
        {
            self.error = Some(error);
        }
        self.surface = None;
        self.renderer = None;
        self.window = None;
    }
}

#[cfg(test)]
mod contextual_tests;

#[cfg(test)]
mod navigation_probe;

#[cfg(test)]
mod navigation_tests {
    use super::*;
    fn fixture() -> (hair::theme::Theme, World) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let theme = hair::theme::Theme::load(
            &root.join("packs/office.toml"),
            &mut hair::theme::AssetCache::default(),
        )
        .unwrap();
        (theme, World::load(root).unwrap())
    }
    #[test]
    fn unified_e_enters_near_aimed_portal() {
        let (theme, mut world) = fixture();
        let pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [0.0, -3.0],
            yaw: 0.0,
            pitch: -0.1,
        };
        interact(&theme, &mut world, &pose);
        assert_eq!(
            world.current, "workshop",
            "E must follow the aimed local door, not only inspect"
        );
    }
    #[test]
    fn e_key_latch_survives_reader_release_and_ignores_unfocused_actions() {
        let (theme, world) = fixture();
        let mut app = App::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            vec![theme],
            0,
            world,
            false,
        );
        app.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [0.0, 1.8],
            yaw: 0.0,
            pitch: -0.5,
        };
        app.interact_key(true, false);
        assert!(app.world.reading);
        app.world.reading = false; // Escape while E remains physically down.
        app.interact_key(true, false);
        assert!(
            !app.world.reading,
            "duplicate press without release must not re-open reader"
        );
        app.interact_key(false, false);
        app.focused = false;
        app.interact_key(true, false);
        assert!(!app.world.reading, "unfocused input must not act");
    }
    #[test]
    fn repeated_e_does_not_cascade_after_navigation() {
        let (theme, world) = fixture();
        let mut app = App::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            vec![theme],
            0,
            world,
            false,
        );
        app.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [0.0, -3.0],
            yaw: 0.0,
            pitch: -0.1,
        };
        app.interact_key(true, false);
        assert_eq!(app.world.current, "workshop");
        app.pose.position = [0.0, -3.0];
        app.interact_key(true, true);
        assert_eq!(app.world.current, "workshop");
    }
    #[test]
    fn reader_isolates_world_keys_and_focus_adapter_resets_input() {
        let (theme, _) = fixture();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let world = World::load_site(&root.join("examples/site-a")).unwrap();
        let mut app = App::new(root.to_path_buf(), vec![theme], 0, world, false);
        app.world.current = "notes/workshop.md".into();
        app.world.reading = true;
        let before = (
            app.world.current.clone(),
            app.world.selected,
            app.pose.position,
        );
        for code in [
            KeyCode::KeyW,
            KeyCode::Tab,
            KeyCode::KeyR,
            KeyCode::KeyF,
            KeyCode::KeyE,
        ] {
            app.key(code, true, false).unwrap();
        }
        assert_eq!(
            before,
            (
                app.world.current.clone(),
                app.world.selected,
                app.pose.position
            ),
            "reader must isolate world navigation/target keys"
        );
        assert!(
            app.world.selected_reader_link().is_some(),
            "Tab now selects an actual reader link, not a world destination"
        );
        assert!(app.world.explicit_target.is_none());
        assert!(!app.input.forward, "reader must not latch movement");
        app.input.forward = true;
        app.input.captured = true;
        app.focus_changed(false);
        assert!(!app.input.forward && !app.input.captured && !app.interact_held);
        app.key(KeyCode::KeyW, true, false).unwrap();
        assert!(!app.input.forward);
        app.focus_changed(true);
        app.key(KeyCode::Escape, true, false).unwrap();
        assert!(!app.world.reading);
        app.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [0.0, 1.8],
            yaw: 0.0,
            pitch: -0.5,
        };
        app.key(KeyCode::KeyE, true, false).unwrap();
        assert!(app.world.reading, "focus reset permits a fresh press");
        app.key(KeyCode::Escape, true, false).unwrap();
        app.key(KeyCode::KeyE, true, true).unwrap();
        assert!(!app.world.reading);
        app.key(KeyCode::KeyE, false, false).unwrap();
        app.key(KeyCode::KeyE, true, false).unwrap();
        assert!(app.world.reading, "real release re-arms E");
    }
    #[test]
    fn legacy_f_is_not_an_alternate_interaction() {
        let (theme, world) = fixture();
        let mut app = App::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            vec![theme],
            0,
            world,
            false,
        );
        app.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [0.0, -3.0],
            yaw: 0.0,
            pitch: -0.1,
        };
        app.key(KeyCode::KeyF, true, false).unwrap();
        assert_eq!(app.world.current, "welcome");
        app.key(KeyCode::KeyE, true, false).unwrap();
        assert_eq!(app.world.current, "workshop");
    }
    #[test]
    #[ignore = "serialized offscreen Metal; requires HAIR_INTERACTION_CAPTURE_DIR"]
    fn rendered_first_slice_input_tracer() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = std::env::var_os("HAIR_INTERACTION_CAPTURE_DIR")
            .expect("explicit separate existing capture directory");
        let output = resources::smoke_output(root, Path::new(&path)).unwrap();
        let arrival_path = Path::new(&path).join("arrival");
        let arrival_output = resources::smoke_output(root, &arrival_path).unwrap();
        let mut cache = hair::theme::AssetCache::default();
        let themes = ["packs/office.toml", "packs/archive.toml"]
            .iter()
            .map(|name| hair::theme::Theme::load(&root.join(name), &mut cache).unwrap())
            .collect();
        let mut app = App::new(
            root.to_path_buf(),
            themes,
            0,
            World::load(root).unwrap(),
            false,
        );
        let instance = render::instance();
        let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
        let mut renderer = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [960, 640],
            &app.themes[0].vertices_for(&app.world),
        ))
        .unwrap();
        renderer.set_materials(&app.themes[0].textures).unwrap();
        app.renderer = Some(renderer);
        let capture = |app: &mut App, output: &resources::SmokeOutput, name: &str| {
            app.world.interaction = hair::interaction::resolve(
                &app.themes[app.theme_index].host,
                &app.pose,
                &app.world,
            );
            app.renderer
                .as_ref()
                .unwrap()
                .capture(output, name, &app.pose, &app.world)
                .unwrap();
            println!(
                "INPUT_CAPTURE {name} page={} reading={} target={:?}",
                app.world.current, app.world.reading, app.world.interaction
            );
        };
        app.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [0.0, 1.8],
            yaw: 0.0,
            pitch: -0.5,
        };
        capture(&mut app, &output, "office.png");
        assert_eq!(
            app.world.interaction.as_ref().unwrap().anchor,
            "main-document"
        );
        app.key(KeyCode::KeyE, true, false).unwrap();
        assert!(app.world.reading);
        capture(&mut app, &output, "reader-welcome.png");
        app.key(KeyCode::KeyR, true, false).unwrap();
        app.key(KeyCode::Tab, true, false).unwrap();
        assert!(app.world.reading && app.world.current == "welcome");
        app.key(KeyCode::Escape, true, false).unwrap();
        app.key(KeyCode::KeyE, true, false).unwrap();
        app.key(KeyCode::KeyE, true, true).unwrap();
        assert!(!app.world.reading, "held E must not reopen after Escape");
        app.focus_changed(false);
        app.key(KeyCode::KeyE, true, false).unwrap();
        assert!(!app.world.reading && !app.input.captured);
        app.focus_changed(true);
        app.key(KeyCode::KeyE, false, false).unwrap();
        app.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [0.0, -3.0],
            yaw: 0.0,
            pitch: -0.1,
        };
        app.world.interaction =
            hair::interaction::resolve(&app.themes[app.theme_index].host, &app.pose, &app.world);
        navigation_probe::capture_door(
            app.renderer.as_ref().unwrap(),
            &output,
            &app.pose,
            &app.world,
        )
        .unwrap();
        println!(
            "INPUT_CAPTURE reader-workshop.png page=welcome reading=false semantic_fixed_door=true"
        );
        assert_eq!(
            app.world
                .interaction
                .as_ref()
                .unwrap()
                .destination
                .as_deref(),
            Some("workshop")
        );
        app.key(KeyCode::KeyF, true, false).unwrap();
        assert_eq!(app.world.current, "welcome");
        app.key(KeyCode::KeyE, true, false).unwrap();
        assert_eq!(app.world.current, "workshop");
        assert!(app.world.interaction.is_none());
        capture(&mut app, &arrival_output, "office.png");
        let before = (
            app.pose.position,
            app.pose.yaw,
            app.pose.pitch,
            app.world.current.clone(),
            app.world.selected,
            app.world.reading,
        );
        app.key(KeyCode::KeyT, true, false).unwrap();
        assert_eq!(
            before,
            (
                app.pose.position,
                app.pose.yaw,
                app.pose.pitch,
                app.world.current.clone(),
                app.world.selected,
                app.world.reading
            )
        );
        app.world.interaction =
            hair::interaction::resolve(&app.themes[app.theme_index].host, &app.pose, &app.world);
        navigation_probe::capture_archive_arrival(
            app.renderer.as_ref().unwrap(),
            &arrival_output,
            &app.pose,
            &app.world,
        )
        .unwrap();
        println!(
            "INPUT_CAPTURE arrival/reader-workshop.png page=workshop reading=false semantic_archive_arrival=true"
        );
        app.renderer.as_ref().unwrap().wait().unwrap();
        println!(
            "FIRST_SLICE_INPUT_TRACER_PASS shared executable key/focus adapter; real offscreen GPU, not OS/native/human input"
        );
    }
    #[test]
    fn unified_e_refuses_document_behind_camera() {
        let (theme, mut world) = fixture();
        let pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [0.0, 1.8],
            yaw: std::f32::consts::PI,
            pitch: -0.4,
        };
        interact(&theme, &mut world, &pose);
        assert!(
            !world.reading,
            "near alone must not read an object behind the camera"
        );
    }
}
