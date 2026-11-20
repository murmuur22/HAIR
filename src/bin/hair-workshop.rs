#[cfg(target_os = "macos")]
#[path = "../native_dialogs.rs"]
mod native_dialogs;

#[path = "../workshop_proof.rs"]
mod proof;

use hair::{
    Result, render, resources,
    workshop::{Command, Workshop, WorkshopGpu},
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("HAIR WORKSHOP ERROR: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag] if flag == "--version" => {
            println!("HAIR Workshop {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        [flag] if flag == "--help" => {
            println!(
                "HAIR Workshop {}\nNative local egui companion. Drop a GLB or browse with the native macOS Open panel; optional advanced full paths remain available. Role, Resize/Reset, asset-local pivot/orientation controls update the shared HAIR preview; host owns all locations/collision. Save NEW copy writes an editable project with private model bytes; Open project reopens project.toml. Export writes a supported pack to an explicitly selected NEW folder; it does not save the project. Existing folders are never overwritten. Escape/Quit/window close ask before discarding unsaved changes. Free-fly inspection: select the visible mode button then click the preview to capture; mouse look, WASD, Q down / E up. Escape releases capture before quit logic; text focus never moves the camera. Frame / Reset view recovers the fixed stage. Camera session is not saved/exported. No VR, full PBR, network, or arbitrary pack scripts.\nDevelopment: set absolute HAIR_RESOURCE_ROOT; bundled: executable-relative Contents/Resources. No cwd fallback.",
                env!("CARGO_PKG_VERSION")
            );
            return Ok(());
        }
        [] => {}
        [flag, _, _, _, _]
            if matches!(
                flag.as_str(),
                "--offscreen-proof" | "--offscreen-clarity-proof"
            ) => {}
        _ => return Err(
            "usage: hair-workshop [--help|--version]; use visible controls for ordinary authoring"
                .into(),
        ),
    }
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let override_root = std::env::var_os("HAIR_RESOURCE_ROOT");
    let root = resources::resolve_root(&executable, override_root.as_deref().map(Path::new))?;
    // The fixed authoring stage is not site layout, but a damaged bundled host
    // must still refuse before either native startup or diagnostic GPU/output.
    hair::host_scene::HostScene::load(&root).map_err(|e| resources::resource_error(&root, e))?;
    resources::paths(&root).map_err(|e| resources::resource_error(&root, e))?;
    if let [flag, model, project, pack, output] = args.as_slice()
        && matches!(
            flag.as_str(),
            "--offscreen-proof" | "--offscreen-clarity-proof"
        )
    {
        return proof::run(
            &root,
            Path::new(model),
            Path::new(project),
            Path::new(pack),
            Path::new(output),
            flag == "--offscreen-clarity-proof",
        );
    }
    let events = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = Native {
        app: Workshop::new(vec![root]),
        ctx: egui::Context::default(),
        window: None,
        input: None,
        surface: None,
        config: None,
        gpu: None,
        fatal: None,
        occluded: false,
        grabbed: false,
    };
    events.run_app(&mut app).map_err(|e| e.to_string())?;
    if let Some(error) = app.fatal {
        return Err(error);
    }
    Ok(())
}
struct Native {
    app: Workshop,
    ctx: egui::Context,
    window: Option<Arc<Window>>,
    input: Option<egui_winit::State>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    gpu: Option<WorkshopGpu>,
    fatal: Option<String>,
    occluded: bool,
    grabbed: bool,
}
impl Native {
    fn sync_capture(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        if self.app.camera.captured == self.grabbed {
            return;
        }
        if self.app.camera.captured {
            if window
                .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                .is_err()
            {
                self.app.camera.release();
                self.app.notice =
                    "Mouse capture unavailable; use standard inspection or Frame / Reset view."
                        .into();
                return;
            }
            window.set_cursor_visible(false);
            self.grabbed = true;
        } else {
            let _ = window.set_cursor_grab(winit::window::CursorGrabMode::None);
            window.set_cursor_visible(true);
            self.grabbed = false;
        }
    }
    fn initialize(&mut self, events: &ActiveEventLoop) -> Result<()> {
        let window = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_title(format!(
                            "HAIR Workshop {} — local authoring",
                            env!("CARGO_PKG_VERSION")
                        ))
                        .with_inner_size(winit::dpi::LogicalSize::new(1200, 900))
                        .with_min_inner_size(winit::dpi::LogicalSize::new(900, 700)),
                )
                .map_err(|e| e.to_string())?,
        );
        let instance = render::instance();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let adapter = pollster::block_on(render::adapter(&instance, Some(&surface)))?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(
                &adapter,
                size.width.clamp(1, 4096),
                size.height.clamp(1, 4096),
            )
            .ok_or("No native surface configuration")?;
        config.format = surface
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(|f| !f.is_srgb())
            .ok_or("No egui-compatible non-sRGB surface format")?;
        let gpu = pollster::block_on(WorkshopGpu::new(
            &adapter,
            config.format,
            [config.width, config.height],
        ))?;
        surface.configure(&gpu.shared.device, &config);
        self.input = Some(egui_winit::State::new(
            self.ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(4096),
        ));
        self.surface = Some(surface);
        self.config = Some(config);
        self.gpu = Some(gpu);
        window.request_redraw();
        self.window = Some(window);
        Ok(())
    }
    fn redraw(&mut self) -> Result<()> {
        let window = self.window.as_ref().ok_or("Missing native window")?.clone();
        if self.occluded || window.inner_size().width == 0 || window.inner_size().height == 0 {
            return Ok(());
        }
        let surface = self.surface.as_ref().ok_or("Missing native surface")?;
        let config = self.config.as_ref().ok_or("Missing native config")?;
        let gpu = self.gpu.as_mut().ok_or("Missing shared renderer")?;
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated => {
                surface.configure(&gpu.shared.device, config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                self.occluded = true;
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout => return Ok(()),
            error => {
                return Err(format!(
                    "Surface unavailable: {error:?}; restart the workshop"
                ));
            }
        };
        let input = self.input.as_mut().ok_or("Missing egui input")?;
        let output = gpu.frame(
            &self.ctx,
            &mut self.app,
            input.take_egui_input(&window),
            &frame.texture.create_view(&Default::default()),
            [config.width, config.height],
        )?;
        input.handle_platform_output(&window, output);
        window.pre_present_notify();
        gpu.shared.queue.present(frame);
        // No GPU frame/egui borrow is live during the system modal loop. This is
        // only the interactive winit callback, never the offscreen proof path.
        // UI may have requested a native panel: release OS capture before AppKit.
        self.sync_capture();
        if let Some(kind) = self.app.begin_dialog() {
            #[cfg(target_os = "macos")]
            let result = native_dialogs::present(kind);
            #[cfg(not(target_os = "macos"))]
            let result = Err(format!(
                "Native macOS panels unavailable for {kind:?}; use advanced paths"
            ));
            self.app.finish_dialog(result);
            window.request_redraw();
        }
        Ok(())
    }
}
impl ApplicationHandler for Native {
    fn new_events(&mut self, _events: &ActiveEventLoop, cause: winit::event::StartCause) {
        if matches!(cause, winit::event::StartCause::ResumeTimeReached { .. })
            && !self.occluded
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
    }
    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.window.is_none()
            && let Err(e) = self.initialize(events)
        {
            self.fatal = Some(e);
            events.exit();
        }
    }
    fn window_event(&mut self, events: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let (Some(input), Some(window)) = (&mut self.input, &self.window) {
            let _ = input.on_window_event(window, &event);
        }
        match event {
            WindowEvent::Focused(false) | WindowEvent::Occluded(true) => {
                self.app.camera.release();
                self.sync_capture();
                if matches!(event, WindowEvent::Occluded(true)) {
                    self.occluded = true;
                }
            }
            WindowEvent::CloseRequested => self.app.dispatch(Command::RequestQuit),
            WindowEvent::Occluded(value) => self.occluded = value,
            WindowEvent::Resized(size) => {
                if size.width > 0
                    && size.height > 0
                    && let (Some(surface), Some(config), Some(gpu)) =
                        (&self.surface, &mut self.config, &self.gpu)
                {
                    config.width = size.width.clamp(1, 4096);
                    config.height = size.height.clamp(1, 4096);
                    surface.configure(&gpu.shared.device, config);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.redraw() {
                    self.fatal = Some(e);
                    events.exit();
                }
            }
            _ => {}
        }
        if self.app.should_close {
            self.app.camera.release();
            self.sync_capture();
            events.exit();
        } else if !matches!(event, WindowEvent::RedrawRequested)
            && !self.occluded
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
    }
    fn device_event(
        &mut self,
        _events: &ActiveEventLoop,
        _id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if self.grabbed
            && self.app.camera.captured
            && let winit::event::DeviceEvent::MouseMotion { delta } = event
            && let Some(input) = &mut self.input
        {
            input.on_mouse_motion(delta);
        }
    }
    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        if self.occluded {
            events.set_control_flow(ControlFlow::Wait);
        } else {
            events.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(33),
            ));
        }
    }
}
