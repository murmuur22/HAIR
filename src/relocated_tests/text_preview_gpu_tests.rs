//! External child of folder_qualification_tests::sparse; see ../IMPLEMENTATION.md.
//! Successor of media-text-excerpts/gpu/corrected-r3/text_preview_gpu_tests.rs.
//! Explicit presentation-contract delta: 6x body, 36/54 advances, all 7 font rows.
//! Reuses the existing scoped oracle's real readback, UI raster and projection.
//! Positive frames use only actual App input/poll/Theme/Renderer consumers.
use super::*;
use std::os::unix::fs::OpenOptionsExt;

const BODY: &str = "# aB9\nx\t\\\né\n";
const LINES: [&str; 3] = ["# aB9", "x\\x09\\\\", "\\u{E9}"];

fn exclusive_bytes(path: &Path, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
}

// Success removes only the exclusive synthetic fixture, after App/worker drop.
// Panic preserves it for diagnosis. Captures are never automatically removed.
struct OwnedFixture(PathBuf);
impl Drop for OwnedFixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("TEXT_GPU_FIXTURE_RETAINED {}", self.0.display());
        } else if let Err(error) = fs::remove_dir_all(&self.0) {
            panic!("owned text fixture cleanup: {error}");
        }
    }
}

fn wait_preview(app: &mut App, id: &str) -> Arc<hair::assets::Texture> {
    let start = Instant::now();
    for poll in 1..=256 {
        app.poll_local_image().unwrap();
        assert!(!app.world.reading);
        if let Some(texture) = app.object_previews.cache.get(id) {
            println!(
                "TEXT_GPU_POLL polls={poll} elapsed_ms={}",
                start.elapsed().as_millis()
            );
            return texture.clone();
        }
        if start.elapsed() >= Duration::from_secs(5) {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!(
        "TEXT_GPU_PREVIEW_TIMEOUT status={}",
        app.object_previews.status
    );
}

// Independent font geometry, never passed to the renderer or used as a texture.
fn literal_cells(texture: &hair::assets::Texture) {
    assert_eq!([texture.width, texture.height], [592, 240]);
    assert_eq!(texture.rgba.len(), 592 * 240 * 4);
    assert!(texture.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    for (line, text) in LINES.iter().enumerate() {
        for (column, c) in text.chars().enumerate() {
            let mut mask = [[false; 5]; 7];
            let glyph = hair::text::object_label(&c.to_string(), [0.0; 3], 1);
            for quad in glyph.as_chunks::<6>().0 {
                let x = quad
                    .iter()
                    .map(|v| v.position[0])
                    .fold(f32::INFINITY, f32::min);
                let y = quad
                    .iter()
                    .map(|v| v.position[1])
                    .fold(f32::NEG_INFINITY, f32::max);
                let col = ((x + 0.036) / 0.012).round() as usize;
                let row = (-y / 0.012).round() as usize;
                assert!(col < 5 && row < 7);
                mask[row][col] = true;
            }
            for (row, cells) in mask.iter().enumerate() {
                for (col, ink) in cells.iter().enumerate() {
                    for dy in 0..6 {
                        for dx in 0..6 {
                            let x = 8 + column * 36 + col * 6 + dx;
                            let y = 32 + line * 54 + row * 6 + dy;
                            let at = (y * 592 + x) * 4;
                            assert_eq!(
                                &texture.rgba[at..at + 4],
                                if *ink {
                                    &[20, 42, 25, 255]
                                } else {
                                    &[242, 246, 235, 255]
                                },
                                "literal source glyph {c:?}, line={line} cell={col},{row}"
                            );
                        }
                    }
                }
            }
        }
    }
}

fn plane(app: &App, id: &str, texture: &Arc<hair::assets::Texture>) -> Vec<Vertex> {
    let theme = &app.themes[app.theme_index];
    let material = theme
        .textures
        .iter()
        .position(|t| Arc::ptr_eq(t, texture))
        .unwrap() as u32
        + 1;
    let vertices: Vec<_> = theme
        .vertices_for(&app.world)
        .into_iter()
        .filter(|v| v.texture == (material | 0x8000_0000))
        .collect();
    assert_eq!(
        vertices.len(),
        6,
        "actual world document texture consumption"
    );
    let anchor = theme.host.anchors.iter().find(|a| a.id == id).unwrap();
    assert_eq!(anchor.role, "document");
    for vertex in &vertices {
        let [x, y, z] = vertex.position;
        assert!(
            vertex
                .position
                .iter()
                .chain(vertex.uv.iter())
                .all(|v| v.is_finite())
        );
        assert!(x >= anchor.at[0] - anchor.size[0] * 0.5 - 0.0001);
        assert!(x <= anchor.at[0] + anchor.size[0] * 0.5 + 0.0001);
        assert!(y >= anchor.at[1] - 0.0001 && y <= anchor.at[1] + anchor.size[1] + 0.0001);
        assert!((z - anchor.at[2]).abs() <= anchor.size[2] * 0.5 + 0.0001);
        assert!(vertex.uv.iter().all(|v| (0.0..=1.0).contains(v)));
        assert_eq!(vertex.color, [1.0; 3]);
    }
    assert_eq!(vertices[0].uv, [0.0, 0.0]);
    assert_eq!(vertices[1].uv, [1.0, 0.0]);
    assert_eq!(vertices[2].uv, [1.0, 1.0]);
    vertices
}

fn prepare_frame(app: &mut App) {
    app.poll_local_image().unwrap();
    hair::interaction::refresh(&app.themes[app.theme_index].host, &app.pose, &mut app.world);
    app.update_hud();
    let vertices = app.themes[app.theme_index].vertices_for(&app.world);
    app.renderer.as_mut().unwrap().set_scene(&vertices);
    assert_eq!(app.scroll, 0);
}

fn ui_contract(app: &App, pixels: &[u8]) -> Result<Vec<Option<[u8; 3]>>> {
    let renderer = app.renderer.as_ref().unwrap();
    let [w, h] = renderer.size;
    if pixels.len() != (w * h * 4) as usize || pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
    {
        return Err("TEXT_GPU_LENGTH_OR_ALPHA".into());
    }
    let mut quads = hair::text::overlay(&app.world, w, h, app.scroll);
    if !app.world.reading {
        quads.extend(hair::text::pack_overlay(&renderer.pack_feedback, w, h));
    }
    // Existing scoped semantic UI oracle, not a palette/presence-only test.
    let ui = super::ui_mask(&quads, [w, h]);
    for (i, expected) in ui.iter().enumerate() {
        if let Some(expected) = expected
            && !super::close(&pixels[i * 4..i * 4 + 3], *expected)
        {
            return Err(format!(
                "TEXT_GPU_CONTROL_RASTER x={} y={}",
                i % w as usize,
                i / w as usize
            ));
        }
    }
    Ok(ui)
}

fn linear(v: u8) -> f32 {
    let v = v as f32 / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

// A narrow source-derived glyph oracle, not a second scene renderer. Compare
// actual framebuffer pixels on the independently CPU-checked source glyph rows
// against the real linear-filter/sRGB material contract after inverse projection.
// No reference pixels are written to a GPU resource or substituted for readback.
fn glyph_frame(
    app: &App,
    texture: &hair::assets::Texture,
    quad: &[Vertex],
    pixels: &[u8],
) -> Result<()> {
    let ui = ui_contract(app, pixels)?;
    if app.world.reading {
        return Err("TEXT_GPU_EXPECTED_CLOSED_WORLD".into());
    }
    let size @ [w, h] = app.renderer.as_ref().unwrap().size;
    let matrix = super::matrix(&app.pose, size);
    let inverse = matrix.inverse();
    let inverse64 = glam::DMat4::from_cols_array(&matrix.to_cols_array().map(f64::from)).inverse();
    let eye = Vec3::new(app.pose.position[0], 1.65, app.pose.position[1]);
    let eye64 = glam::DVec3::new(f64::from(eye.x), f64::from(eye.y), f64::from(eye.z));
    let left = quad[0].position[0];
    let top = quad[0].position[1];
    let width = quad[1].position[0] - left;
    let height = top - quad[2].position[1];
    let z = quad[0].position[2];
    let mut counts = [[0_usize; 3]; 3];
    for y in 0..h {
        for x in 0..w {
            let far = inverse.project_point3(Vec3::new(
                2.0 * (x as f32 + 0.5) / w as f32 - 1.0,
                1.0 - 2.0 * (y as f32 + 0.5) / h as f32,
                1.0,
            ));
            let ray = far - eye;
            let hit = eye + ray * ((z - eye.z) / ray.z);
            let tx = (hit.x - left) / width * texture.width as f32 - 0.5;
            let ty = (top - hit.y) / height * texture.height as f32 - 0.5;
            // Deliberate new presentation regions, not reference calibration:
            // 36px character advance, final 30px ink cell, all 7 rows at 6px.
            let Some(line) = (0..3).find(|row| {
                let y0 = 32.0 + *row as f32 * 54.0;
                tx >= 8.0
                    && tx < 8.0 + LINES[*row].len() as f32 * 36.0 - 6.0
                    && ty >= y0
                    && ty < y0 + 7.0 * 6.0
            }) else {
                continue;
            };
            let index = (y * w + x) as usize;
            if ui[index].is_some() {
                return Err(format!("TEXT_GPU_SOURCE_OBSTRUCTED line={line} xy={x},{y}"));
            }
            // Retain corrected-r3's sampling-only f64 inversion of the EXACT
            // uploaded f32 matrix. Selection above stays f32, with only the
            // explicitly enlarged presentation regions changed. Threshold is 5.
            // Parent executed forward-f64 and inverse-f64 controls on the old
            // same-selected samples; no production renderer defect was found.
            let far64 = inverse64.project_point3(glam::DVec3::new(
                2.0 * (f64::from(x) + 0.5) / f64::from(w) - 1.0,
                1.0 - 2.0 * (f64::from(y) + 0.5) / f64::from(h),
                1.0,
            ));
            let ray64 = far64 - eye64;
            let hit64 = eye64 + ray64 * ((f64::from(z) - eye64.z) / ray64.z);
            let tx = ((hit64.x - f64::from(left)) / f64::from(width) * f64::from(texture.width)
                - 0.5) as f32;
            let ty = ((f64::from(top) - hit64.y) / f64::from(height) * f64::from(texture.height)
                - 0.5) as f32;
            let ix = tx.floor() as usize;
            let iy = ty.floor() as usize;
            let fx = tx.fract();
            let fy = ty.fract();
            let mut expected = [0.0_f32; 3];
            for (dx, dy, weight) in [
                (0, 0, (1.0 - fx) * (1.0 - fy)),
                (1, 0, fx * (1.0 - fy)),
                (0, 1, (1.0 - fx) * fy),
                (1, 1, fx * fy),
            ] {
                let at = ((iy + dy) * texture.width as usize + ix + dx) * 4;
                for (channel, value) in expected.iter_mut().enumerate() {
                    *value += linear(texture.rgba[at + channel]) * weight;
                }
            }
            let expected = super::rgb(expected);
            let actual = &pixels[index * 4..index * 4 + 3];
            if !actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 5) {
                return Err(format!(
                    "TEXT_GPU_LITERAL_GLYPH line={line} xy={x},{y} expected={expected:?} actual={actual:?}"
                ));
            }
            counts[line][0] += 1;
            counts[line][1] += usize::from(expected[0] < 160);
            counts[line][2] += usize::from(expected[0] > 220);
        }
    }
    if counts.iter().any(|c| c[0] < 12 || c[1] < 2 || c[2] < 2) {
        return Err(format!("TEXT_GPU_GLYPH_WITNESSES_INSUFFICIENT {counts:?}"));
    }
    println!("TEXT_GPU_SCOPED_GLYPHS total_dark_paper={counts:?}");
    Ok(())
}

fn retain(app: &App, root: &Path, group: &Path, stage: &str, pixels: &[u8], scoped: &Result<()>) {
    let renderer = app.renderer.as_ref().unwrap();
    let size = renderer.size;
    let directory = group.join(stage);
    fs::create_dir(&directory).expect("exclusive absent capture stage");
    let output = resources::smoke_output(root, &directory).unwrap();
    // Exercise the real generic guard independently on another actual frame.
    let generic = renderer.capture(&output, "office.png", &app.pose, &app.world);
    println!("TEXT_GPU_GENERIC {stage}: {generic:?}; SCOPED={scoped:?}");
    if generic.is_err() {
        // Never turn an unrelated GPU/writer failure into a sparse success.
        let colors: std::collections::HashSet<_> = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| [p[0], p[1], p[2]])
            .collect();
        let minimum = if app.world.reading { 2 } else { 20 };
        assert!(
            colors.len() < minimum,
            "non-diversity generic failure: {generic:?}"
        );
        // Existing scoped semantic writer pattern; unchanged allowlist/O_EXCL.
        // A failed semantic frame is retained as diagnostic-only, never certified.
        let mut file = std::io::BufWriter::new(output.create_capture("office.png").unwrap());
        let mut encoder = png::Encoder::new(&mut file, size[0], size[1]);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut png = encoder.write_header().unwrap();
        png.write_image_data(pixels).unwrap();
        png.finish().unwrap();
        file.flush().unwrap();
    }
    // The generic path rendered a second time. Require its PNG to decode to the
    // exact semantic readback rather than silently pairing different frames.
    let encoded = fs::read(output.path().join("office.png")).unwrap();
    let mut decoder = png::Decoder::new(std::io::Cursor::new(encoded))
        .read_info()
        .unwrap();
    let mut decoded = vec![0; decoder.output_buffer_size().unwrap()];
    let info = decoder.next_frame(&mut decoded).unwrap();
    assert_eq!([info.width, info.height], size);
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    assert_eq!(&decoded[..info.buffer_size()], pixels);
    assert!(
        output.create_capture("office.png").is_err(),
        "exclusive repeat-write refusal"
    );
    exclusive_bytes(&group.join(format!("{stage}.rgba")), pixels);
    let receipt = format!(
        "stage={stage}\nsize={size:?}\nreading={}\npose={:?}\ntarget={:?}\nrgba_sha256={}\ngeneric={generic:?}\nscoped={scoped:?}\nvisualAcceptance=PENDING_ACTUAL_IMAGE_LOADING\n",
        app.world.reading,
        app.pose,
        app.world.interaction,
        super::digest(pixels)
    );
    exclusive_bytes(&group.join(format!("{stage}.txt")), receipt.as_bytes());
}

// Counterfactual rendering only: never used to populate positive App state.
fn negative_controls(
    app: &mut App,
    texture: &hair::assets::Texture,
    quad: &[Vertex],
    positive: &[u8],
) {
    let renderer = app.renderer.as_ref().unwrap();
    let blank = super::readback(renderer, &app.pose, None, Some(wgpu::Color::BLACK)).unwrap();
    assert!(glyph_frame(app, texture, quad, &blank).is_err());
    let no_ui = super::readback(renderer, &app.pose, None, None).unwrap();
    assert!(glyph_frame(app, texture, quad, &no_ui).is_err());
    let mut wrong_pose = app.pose;
    wrong_pose.yaw += std::f32::consts::PI;
    let wrong_camera = super::readback(renderer, &wrong_pose, Some(&app.world), None).unwrap();
    assert!(glyph_frame(app, texture, quad, &wrong_camera).is_err());
    assert!(glyph_frame(app, texture, quad, &positive[..positive.len() - 1]).is_err());
    let mut alpha = positive.to_vec();
    alpha[3] = 0;
    assert!(glyph_frame(app, texture, quad, &alpha).is_err());
    let scene = app.themes[app.theme_index].vertices_for(&app.world);
    let missing_plane: Vec<_> = scene
        .iter()
        .copied()
        .filter(|v| v.texture != quad[0].texture)
        .collect();
    app.renderer.as_mut().unwrap().set_scene(&missing_plane);
    let missing = super::readback(
        app.renderer.as_ref().unwrap(),
        &app.pose,
        Some(&app.world),
        None,
    )
    .unwrap();
    assert!(glyph_frame(app, texture, quad, &missing).is_err());
    let hidden: Vec<_> = scene
        .iter()
        .take(3)
        .map(|v| Vertex {
            position: [100.0; 3],
            ..*v
        })
        .collect();
    app.renderer.as_mut().unwrap().set_scene(&hidden);
    let hud_only = super::readback(
        app.renderer.as_ref().unwrap(),
        &app.pose,
        Some(&app.world),
        None,
    )
    .unwrap();
    assert!(glyph_frame(app, texture, quad, &hud_only).is_err());
    app.renderer.as_mut().unwrap().set_scene(&scene);
    // Build a counterfactual copy from the owned snapshot, without mutating App.
    let snapshot = super::super::snapshot(app);
    let (mut wrong_world, _) =
        hair::folder_scene::build(&snapshot, 0, app.world.tree_collapsed).unwrap();
    wrong_world.interaction = None;
    let missing_prompt = super::readback(
        app.renderer.as_ref().unwrap(),
        &app.pose,
        Some(&wrong_world),
        None,
    )
    .unwrap();
    assert!(glyph_frame(app, texture, quad, &missing_prompt).is_err());
    println!(
        "TEXT_GPU_NEGATIVES_REJECTED black/no-UI/wrong-camera/truncated/alpha/missing-plane/HUD-only/missing-prompt; not full legacy sparse-world matrix"
    );
}

fn raw(document: &str) -> Option<Vec<u8>> {
    let body = document
        .split_once("\nBYTE GROUPS (8 bytes max)\n")?
        .1
        .split_once("\nFULL SOURCE / METADATA:\n")?
        .0;
    body.lines()
        .filter_map(|line| line.strip_prefix("HEX "))
        .flat_map(str::split_whitespace)
        .map(|v| u8::from_str_radix(v, 16).ok())
        .collect()
}

#[test]
#[ignore = "explicit serialized offscreen Metal; absent HAIR_TEXT_PROOF_ROOT under owned TMPDIR; parent must inspect retained images"]
fn retained_literal_text_before_e_reader_raw_escape_frames() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let temp = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit owned TMPDIR"))
        .canonicalize()
        .unwrap();
    assert!(temp.starts_with(root.parent().unwrap()) && super::super::separate(&temp, &root));
    let requested =
        PathBuf::from(std::env::var_os("HAIR_TEXT_PROOF_ROOT").expect("fresh absent proof root"));
    assert!(requested.is_absolute());
    assert_eq!(requested.parent().unwrap().canonicalize().unwrap(), temp);
    let output = temp.join(requested.file_name().unwrap());
    let mut leaf = requested.file_name().unwrap().to_os_string();
    leaf.push(".fixture");
    let fixture_path = temp.join(leaf);
    for path in [&output, &fixture_path] {
        assert!(
            fs::symlink_metadata(path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        );
    }
    fs::create_dir(&output).unwrap();
    fs::create_dir(&fixture_path).unwrap();
    let fixture = OwnedFixture(fixture_path);
    let mut pins = super::scoped_inputs(&root);
    for (name, compiled) in [
        (
            "src/folder_native/object_preview.rs",
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/folder_native/object_preview.rs"
            ))
            .as_slice(),
        ),
        (
            "src/folder_native/image.rs",
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/folder_native/image.rs"
            ))
            .as_slice(),
        ),
        (
            "src/folder_native/raw_reader.rs",
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/folder_native/raw_reader.rs"
            ))
            .as_slice(),
        ),
    ] {
        assert_eq!(fs::read(root.join(name)).unwrap(), compiled);
        writeln!(pins, "{} {name}", super::digest(compiled)).unwrap();
    }
    exclusive_bytes(&output.join("SCOPED-INPUTS.txt"), pins.as_bytes());
    assert_eq!(
        fs::read(file!()).unwrap(),
        include_bytes!("text_preview_gpu_tests.rs")
    );
    exclusive_bytes(
        &output.join("TEST-SOURCE.rs"),
        include_bytes!("text_preview_gpu_tests.rs"),
    );
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    println!("TEXT_GPU_ADAPTER {:?}", adapter.get_info());
    let mut failures = Vec::new();
    // These existing sizes allow reuse of the unchanged sparse readback helper.
    for (label, size) in [("normal", [960, 640]), ("small", [640, 480])] {
        let folder = fixture.0.join(label);
        fs::create_dir(&folder).unwrap();
        exclusive_bytes(&folder.join("n.txt"), BODY.as_bytes());
        let before = super::super::inventory(&folder);
        let mut app = super::super::app(&folder);
        let id = app
            .world
            .folder_view
            .as_ref()
            .unwrap()
            .bindings
            .iter()
            .find(|(_, b)| b.entry.name == "n.txt")
            .unwrap()
            .0
            .clone();
        super::super::aim(&mut app, &id);
        let host = app.themes[app.theme_index].host.clone();
        let binding = app.world.folder_view.as_ref().unwrap().bindings[&id].clone();
        let current = app.world.current.clone();
        let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
        let target = app.world.interaction.clone();
        let navigation = app.world.navigation.clone();
        let mut renderer = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            size,
            &app.themes[app.theme_index].vertices_for(&app.world),
        ))
        .unwrap();
        renderer
            .set_materials(&app.themes[app.theme_index].textures)
            .unwrap();
        app.renderer = Some(renderer);
        let group = output.join(label);
        fs::create_dir(&group).unwrap();
        for stage in ["before-e", "escape-return"] {
            let texture = wait_preview(&mut app, &id);
            literal_cells(&texture);
            let quad = plane(&app, &id, &texture);
            prepare_frame(&mut app);
            let pixels = super::readback(
                app.renderer.as_ref().unwrap(),
                &app.pose,
                Some(&app.world),
                None,
            )
            .unwrap();
            let scoped = glyph_frame(&app, &texture, &quad, &pixels);
            retain(&app, &root, &group, stage, &pixels, &scoped);
            if stage == "before-e" && scoped.is_ok() {
                negative_controls(&mut app, &texture, &quad, &pixels);
            }
            if let Err(error) = scoped {
                failures.push(format!("{label}/{stage}: {error}"));
            }
            if stage == "before-e" {
                super::super::tap(&mut app, KeyCode::KeyE);
                assert!(app.world.reading);
                let document = app.world.document().unwrap();
                assert!(document.starts_with("TEXT: UTF-8 interpretation"));
                assert!(document.contains("# aB9\nx\\x09\\\\\n\\u{E9}\n"));
                assert!(
                    document.contains(&hair::folder_scene::display(binding.entry.path.as_os_str()))
                );
                for reader_stage in ["reader-text", "reader-raw"] {
                    prepare_frame(&mut app);
                    let pixels = super::readback(
                        app.renderer.as_ref().unwrap(),
                        &app.pose,
                        Some(&app.world),
                        None,
                    )
                    .unwrap();
                    let scoped = ui_contract(&app, &pixels).map(|_| ());
                    retain(&app, &root, &group, reader_stage, &pixels, &scoped);
                    if let Err(error) = scoped {
                        failures.push(format!("{label}/{reader_stage}: {error}"));
                    }
                    if reader_stage == "reader-text" {
                        super::super::tap(&mut app, KeyCode::KeyV);
                        assert_eq!(
                            raw(app.world.document().unwrap()).as_deref(),
                            Some(BODY.as_bytes())
                        );
                    }
                }
                super::super::tap(&mut app, KeyCode::Escape);
                assert!(!app.world.reading);
            }
        }
        assert_eq!(app.themes[app.theme_index].host, host);
        assert_eq!(
            app.world.folder_view.as_ref().unwrap().bindings[&id],
            binding
        );
        assert_eq!(app.world.current, current);
        assert_eq!(app.world.navigation, navigation);
        assert_eq!(app.world.interaction, target);
        assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
        assert!(!host.blocked(app.pose.position));
        host.check_routes(&[app.pose.position]).unwrap();
        assert_eq!(super::super::inventory(&folder), before);
        app.renderer.as_ref().unwrap().wait().unwrap();
        drop(app); // Retire/join real owned workers before deleting synthetic inputs.
        assert_eq!(super::super::inventory(&folder), before);
    }
    exclusive_bytes(&output.join("AUTOMATED-RESULT.txt"), format!(
        "scoped_failures={failures:#?}\nvisualAcceptance=PENDING\nNativeWindow=NOT_EXECUTED\nNo package/general sparse-world certification.\n"
    ).as_bytes());
    assert!(failures.is_empty(), "TEXT_GPU_SCOPED_FAILURES {failures:?}");
}
