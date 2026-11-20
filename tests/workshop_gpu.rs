#![cfg(feature = "workshop")]
use hair::{
    render, resources,
    workshop::{Command, Workshop, WorkshopGpu},
};
use std::{fs, path::Path};

#[test]
#[ignore = "finite real egui + Metal + private original + actual visitor; local only"]
fn private_ui_to_textured_preview_save_export_and_current_visitor() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("private-inputs/planet.glb");
    let before = fs::read(&source).unwrap();
    let base = std::env::temp_dir().join(format!("hair-workshop-ui-proof-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    let captures = base.join("workshop");
    fs::create_dir(&captures).unwrap();
    let output = resources::smoke_output(root, &captures).unwrap();
    pollster::block_on(async {
        let instance = render::instance();
        let adapter = render::adapter(&instance, None).await.unwrap();
        let mut gpu = WorkshopGpu::new(&adapter, wgpu::TextureFormat::Rgba8Unorm, [1200, 900])
            .await
            .unwrap();
        let target = gpu.target([1200, 900]);
        let view = target.create_view(&Default::default());
        let ctx = egui::Context::default();
        let mut app = Workshop::new(vec![root.to_path_buf()]);
        app.input_path = source.display().to_string();
        click(&mut gpu, &ctx, &mut app, &view, "import");
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.project.as_ref().unwrap().model.textures[0].width, 512);
        click(&mut gpu, &ctx, &mut app, &view, "decoration");
        click(&mut gpu, &ctx, &mut app, &view, "document");
        click(&mut gpu, &ctx, &mut app, &view, "fit");
        click(&mut gpu, &ctx, &mut app, &view, "rotate-step");
        let mut edit = app.project.as_ref().unwrap().edit();
        edit.at = [0.25, 0.2, 0.25];
        edit.scale *= 0.9;
        app.dispatch(Command::Edit(edit));
        assert!(app.error.is_none(), "{:?}", app.error);
        app.project_path = base.join("project").display().to_string();
        click(&mut gpu, &ctx, &mut app, &view, "save-new");
        assert!(!app.dirty, "{:?}", app.error);
        app.export_path = base.join("pack").display().to_string();
        click(&mut gpu, &ctx, &mut app, &view, "export");
        assert!(app.error.is_none(), "{:?}", app.error);
        let pixels = gpu.read_pixels(&target, [1200, 900]).unwrap();
        write(&output, "office.png", &pixels);
        let rect = app.controls["preview"];
        let mut colors = std::collections::HashSet::new();
        for y in rect.min.y.ceil() as usize..rect.max.y.floor() as usize {
            for x in rect.min.x.ceil() as usize..rect.max.x.floor() as usize {
                colors.insert(&pixels[(y * 1200 + x) * 4..(y * 1200 + x) * 4 + 3]);
            }
        }
        assert!(
            colors.len() > 200,
            "shared textured preview too flat: {}",
            colors.len()
        );
        println!("REAL_EGUI_PREVIEW_COLORS={} rect={rect:?}", colors.len());
        let loaded =
            hair::theme::Theme::load(&base.join("pack/pack.toml"), &mut Default::default())
                .unwrap();
        assert_eq!(
            loaded.textures[0].rgba,
            app.preview.as_ref().unwrap().textures[0].rgba
        );
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&loaded.vertices),
            bytemuck::cast_slice::<_, u8>(&app.preview.as_ref().unwrap().vertices)
        );
        for v in &mut app.preview.as_mut().unwrap().vertices {
            v.texture = 0;
        }
        app.revision += 1;
        gpu.frame(&ctx, &mut app, raw(), &view, [1200, 900])
            .unwrap();
        let counter = gpu.read_pixels(&target, [1200, 900]).unwrap();
        write(&output, "reader-welcome.png", &counter);
        assert_ne!(
            pixels, counter,
            "texture-disabled preview counterfactual must differ"
        );
        let reopened = hair::authoring::Project::open(&base.join("project/project.toml")).unwrap();
        assert_eq!(reopened.edit(), edit);
        println!(
            "UI_ACTIONS=import,decoration,document,fit,rotate-step,save-new,export via egui pointer events; position/scale via actual app Edit action; synthetic input not human/native"
        );
    });
    assert_eq!(fs::read(base.join("pack/model.glb")).unwrap(), before);
    let visitor = base.join("visitor");
    fs::create_dir(&visitor).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_hair"))
        .env("HAIR_RESOURCE_ROOT", root)
        .args(["--pack"])
        .arg(base.join("pack/pack.toml"))
        .arg("--site")
        .arg(root.join("examples/site-a"))
        .arg("--headless-smoke")
        .arg("--smoke-output")
        .arg(&visitor)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    println!("{stdout}\n{stderr}");
    assert!(result.status.success());
    assert!(!stderr.contains("PACK_FALLBACK"));
    assert!(stdout.contains("THEME=Workshop object"));
    assert!(stdout.contains("THEME_SWITCH_PRESERVED"));
    assert!(stdout.contains("reading=true"));
    assert_eq!(fs::read(&source).unwrap(), before);
    println!("WORKSHOP_UI_E2E={}", base.display());
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
) {
    gpu.frame(ctx, app, raw(), target, [1200, 900]).unwrap();
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
        gpu.frame(ctx, app, input, target, [1200, 900]).unwrap();
    }
    gpu.frame(ctx, app, raw(), target, [1200, 900]).unwrap();
}
fn write(output: &resources::SmokeOutput, name: &str, pixels: &[u8]) {
    use std::io::Write;
    let mut file = output.create_capture(name).unwrap();
    let mut encoder = png::Encoder::new(&mut file, 1200, 900);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png = encoder.write_header().unwrap();
    png.write_image_data(pixels).unwrap();
    png.finish().unwrap();
    file.flush().unwrap();
}
