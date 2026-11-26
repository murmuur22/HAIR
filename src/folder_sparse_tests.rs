// Private binary-test-only supplement; not a replacement for the failed r3 matrix.
// Current-source fixture/normal migration: ../../slice4/TEST-PREP.md.
// Historical capture guards, semantic witness counts and negatives stay intact.
use super::*;
use glam::{Mat4, Vec3};
use hair::{assets::Vertex, local_explorer::Entry, theme::Theme};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::{collections::BTreeMap, io::Write, time::Duration};

#[cfg(test)]
#[path = "relocated_tests/text_preview_gpu_tests.rs"]
mod text_gpu_external;

#[cfg(test)]
#[path = "relocated_tests/model_presentation_gpu_tests.rs"]
mod model_presentation_external;

#[test]
#[ignore = "serialized retained Metal supplement; explicit absent HAIR_SPARSE_PROOF_ROOT required"]
fn exact_r3_local_world_sparse_supplement() {
    run_sparse_suite();
}

const TEXT_BYTES: &[u8] =
    b"POST-BUILD SYNTHETIC TEXT\nCase and spaces are literal.\nNo recompilation needed.\n";
const INITIAL_STATUS: &str =
    "Read-only | safe prior pose retained when possible; otherwise room spawn";
const CONTINUED_STATUS: &str =
    "Read-only | same folder; arrived inside paired doorway facing inward";
const CHILD_PROMPT: &str = "E - Enter \"Child 024\" [folder] | I source";

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn expected_snapshot(fixture: &Path) -> Snapshot {
    let mut entries: Vec<Entry> = (0..25)
        .map(|n| Entry {
            name: format!("Child {n:03}").into(),
            path: fixture.join(format!("Child {n:03}")),
            kind: Kind::Folder,
        })
        .collect();
    for name in ["Read Case.txt", MKV_NAME] {
        entries.push(Entry {
            name: name.into(),
            path: fixture.join(name),
            kind: Kind::File,
        });
    }
    Snapshot {
        folder: fixture.to_path_buf(),
        entries,
    }
}
fn rgb(c: [f32; 3]) -> [u8; 3] {
    c.map(|v| {
        let s = if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u8
    })
}
fn close(a: &[u8], b: [u8; 3]) -> bool {
    a.iter().take(3).zip(b).all(|(a, b)| a.abs_diff(b) <= 1)
}
fn matrix(p: &Pose, [w, h]: [u32; 2]) -> Mat4 {
    let eye = Vec3::new(p.position[0], 1.65, p.position[1]);
    let forward = Vec3::new(
        p.yaw.sin() * p.pitch.cos(),
        p.pitch.sin(),
        -p.yaw.cos() * p.pitch.cos(),
    );
    Mat4::perspective_rh(65.0_f32.to_radians(), w as f32 / h as f32, 0.05, 50.0)
        * Mat4::look_to_rh(eye, forward, Vec3::Y)
}
fn project(m: Mat4, p: [f32; 3], [w, h]: [u32; 2]) -> Option<[f32; 2]> {
    let clip = m * Vec3::from_array(p).extend(1.0);
    if clip.w <= 0.05 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    Some([
        (ndc.x + 1.0) * 0.5 * w as f32,
        (1.0 - ndc.y) * 0.5 * h as f32,
    ])
}
fn quad_bounds(q: &[Vertex], [w, h]: [u32; 2]) -> [i32; 4] {
    let mut b = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
    for v in q {
        let x = ((v.position[0] + 1.0) * 0.5 * w as f32).round() as i32;
        let y = ((1.0 - v.position[1]) * 0.5 * h as f32).round() as i32;
        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    b
}
// Independent rectangle coverage in UI draw order; no synthesized scene image.
fn ui_mask(quads: &[Vertex], size: [u32; 2]) -> Vec<Option<[u8; 3]>> {
    let [w, h] = size;
    let mut mask = vec![None; (w * h) as usize];
    assert!(quads.len().is_multiple_of(6));
    for q in quads.as_chunks::<6>().0 {
        assert!(q.iter().all(|v| v.color == q[0].color && v.texture == 0));
        let [x0, y0, x1, y1] = quad_bounds(q, size);
        for y in y0.max(0)..y1.min(h as i32) {
            for x in x0.max(0)..x1.min(w as i32) {
                mask[(y as u32 * w + x as u32) as usize] = Some(rgb(q[0].color));
            }
        }
    }
    mask
}
struct Triangle {
    a: Vec3,
    e1: Vec3,
    e2: Vec3,
    colors: [[f32; 3]; 3],
    owner: String,
    bounds: [f32; 4],
}
struct Ray {
    x: u32,
    y: u32,
    color: [u8; 3],
    owner: String,
}
struct Contract {
    size: [u32; 2],
    pose: Pose,
    snapshot: Snapshot,
    world: World,
    theme: Theme,
    feedback: String,
    vertices: Vec<Vertex>,
    ui: Vec<Option<[u8; 3]>>,
    rays: Vec<Ray>,
    metadata: String,
}
impl Contract {
    fn new(
        root: &Path,
        fixture: &Path,
        pack: &str,
        size: [u32; 2],
        page: usize,
        collapsed: bool,
        aimed: bool,
    ) -> Self {
        assert!(["office", "archive"].contains(&pack));
        assert!([[960, 640], [640, 480]].contains(&size));
        assert!(
            matches!(
                (page, collapsed, aimed),
                (0, false, false) | (2, false, false) | (2, true, false) | (2, false, true)
            ),
            "only the four explicit local-view contracts"
        );
        let snapshot = expected_snapshot(fixture);
        let (mut world, host) = folder_scene::build(&snapshot, page, collapsed).unwrap();
        assert_eq!(host.room, [-10.5, -6.0, 10.5, 6.0]);
        assert_eq!(host.height, 3.0);
        let pose = if aimed {
            let anchor = host.anchors.iter().find(|a| a.id == "entry-0").unwrap();
            let n = host.portal_inward(&anchor.id);
            Pose {
                eye_offset: 1.65,
                feet_y: 0.0,
                position: [anchor.at[0] + 1.05 * n[0], anchor.at[2] + 1.05 * n[1]],
                yaw: (-n[0]).atan2(n[1]),
                pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
            }
        } else {
            context_pose()
        };
        assert!(!host.blocked(pose.position));
        world.interaction = hair::interaction::resolve(&host, &pose, &world);
        if aimed {
            let t = world.interaction.as_ref().unwrap();
            assert_eq!(t.anchor, "entry-0");
            assert_eq!(t.prompt, CHILD_PROMPT);
            assert_eq!(
                t.destination,
                Some(folder_scene::display(fixture.join("Child 024").as_os_str()))
            );
        } else {
            assert!(
                world.interaction.is_none(),
                "overview must not invent a prompt"
            );
        }
        let theme = Theme::load_for_folder_scene(
            &root.join(format!("packs/{pack}.toml")),
            &mut hair::theme::AssetCache::default(),
            &host,
        )
        .unwrap();
        assert!(!theme.load_failed);
        assert!(theme.textures.is_empty());
        let vertices = theme.vertices_for(&world);
        assert!(vertices.iter().all(|v| v.texture == 0));
        let status = if page == 0 {
            INITIAL_STATUS
        } else {
            CONTINUED_STATUS
        };
        let feedback = format!(
            "{} | {pack}\nU Parent | R Back | F5 refresh | I source\n{status}",
            world.folder_view.as_ref().unwrap().summary()
        );
        let mut quads = hair::text::overlay(&world, size[0], size[1], 0);
        quads.extend(hair::text::pack_overlay(&feedback, size[0], size[1]));
        let ui = ui_mask(&quads, size);
        let mut metadata = format!(
            "SOURCE_DERIVED_EXPECTATION not_framebuffer_evidence version=1\nfixture={fixture:?}\npack={pack}\nsize={size:?}\npose={pose:?}\npage={page} collapsed={collapsed} reading=false aimed={aimed}\nsource={:?}\nview={:?}\nnavigation={:?}\ntarget={:?}\nfeedback={feedback:?}\nscene_sha256={}\nui_quads_sha256={}\nui_covered_pixels={}\n",
            world.current,
            world.folder_view,
            world.navigation,
            world.interaction,
            digest(bytemuck::cast_slice(&vertices)),
            digest(bytemuck::cast_slice(&quads)),
            ui.iter().filter(|p| p.is_some()).count()
        );
        let triangles = source_triangles(&theme, &vertices, &pose, size);
        let rays = source_witnesses(&triangles, &theme, &pose, size, &ui, page, aimed);
        for r in &rays {
            writeln!(
                metadata,
                "ray {},{} rgb={:?} owner={}",
                r.x, r.y, r.color, r.owner
            )
            .unwrap();
        }
        Self {
            size,
            pose,
            snapshot,
            world,
            theme,
            feedback,
            vertices,
            ui,
            rays,
            metadata,
        }
    }
    fn qualify(
        &self,
        pixels: &[u8],
        pose: &Pose,
        world: &World,
        feedback: &str,
        theme: &Theme,
    ) -> Result<()> {
        if pose.position != self.pose.position
            || pose.yaw != self.pose.yaw
            || pose.pitch != self.pose.pitch
            || world.current != self.world.current
            || world.folder_view != self.world.folder_view
            || world.navigation != self.world.navigation
            || world.tree_collapsed != self.world.tree_collapsed
            || world.interaction != self.world.interaction
            || world.explicit_target.is_some()
            || world.reading
            || world.selected != self.world.selected
            || world.site.is_some()
            || world.pocket.is_some()
            || world.spaces.is_some()
            || world.current_space != self.world.current_space
            || feedback != self.feedback
            || theme.source != self.theme.source
            || theme.host != self.theme.host
            || theme.load_failed
            || theme.name != self.theme.name
        {
            return Err(
                "local semantic declared pose/source/page/tree/prompt/pack/HUD mismatch".into(),
            );
        }
        let [w, h] = self.size;
        if pixels.len() != (w * h * 4) as usize
            || pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
        {
            return Err("local semantic dimensions/opaque readback mismatch".into());
        }
        let clear = rgb([0.04, 0.06, 0.05]);
        let scene_pixels = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(i, p)| self.ui[*i].is_none() && !close(&p[..3], clear))
            .count();
        if scene_pixels < 10_000 {
            return Err("local semantic nonblank scene-area coverage".into());
        }
        for r in &self.rays {
            let i = ((r.y * w + r.x) * 4) as usize;
            if !close(&pixels[i..i + 3], r.color) {
                return Err(format!(
                    "local semantic geometry/absent-prompt ray {},{} owner={} expected={:?} actual={:?}",
                    r.x,
                    r.y,
                    r.owner,
                    r.color,
                    &pixels[i..i + 3]
                ));
            }
        }
        for (i, expected) in self.ui.iter().enumerate() {
            if let Some(expected) = expected
                && !close(&pixels[i * 4..i * 4 + 3], *expected)
            {
                return Err(format!(
                    "local semantic header/tree/prompt/HUD raster mismatch {},{}",
                    i % w as usize,
                    i / w as usize
                ));
            }
        }
        Ok(())
    }
    fn valid(&self, pixels: &[u8]) -> Result<()> {
        self.qualify(pixels, &self.pose, &self.world, &self.feedback, &self.theme)
    }
}

fn source_triangles(
    theme: &Theme,
    vertices: &[Vertex],
    pose: &Pose,
    size: [u32; 2],
) -> Vec<Triangle> {
    let m = matrix(pose, size);
    let shell = theme.host.vertices().len();
    vertices
        .as_chunks::<3>()
        .0
        .iter()
        .enumerate()
        .map(|(n, t)| {
            let owner = if n * 3 < shell {
                if t.iter().all(|v| v.color == [0.22, 0.24, 0.21]) {
                    "floor".into()
                } else {
                    "wall".into()
                }
            } else if n * 3 >= theme.vertices.len() {
                "label".into()
            } else {
                let centre = (Vec3::from_array(t[0].position)
                    + Vec3::from_array(t[1].position)
                    + Vec3::from_array(t[2].position))
                    / 3.0;
                let a = theme
                    .host
                    .anchors
                    .iter()
                    .find(|a| {
                        let n = theme.host.portal_inward(&a.id);
                        let dx = centre.x - a.at[0];
                        let dz = centre.z - a.at[2];
                        let tangent = n[1] * dx - n[0] * dz;
                        let inward = n[0] * dx + n[1] * dz;
                        let (width, depth) = if n[0] == 0.0 {
                            (a.size[0], a.size[2])
                        } else {
                            (a.size[2], a.size[0])
                        };
                        !a.id.starts_with("vocabulary")
                            && tangent.abs() <= width * 0.5 + 0.001
                            && inward >= -depth * 0.5 - 0.001
                            && inward <= depth * 0.5 + 0.04
                    })
                    .expect("source model/marker must belong to one real anchor");
                let marker = t
                    .iter()
                    .all(|v| v.color == [0.08, 0.6, 0.18] || v.color == [0.1, 0.8, 0.3]);
                format!("{}:{}", if marker { "marker" } else { "model" }, a.id)
            };
            let mut bounds = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            for v in t {
                if let Some([x, y]) = project(m, v.position, size) {
                    bounds = [
                        bounds[0].min(x),
                        bounds[1].min(y),
                        bounds[2].max(x),
                        bounds[3].max(y),
                    ];
                } else {
                    bounds = [
                        f32::NEG_INFINITY,
                        f32::NEG_INFINITY,
                        f32::INFINITY,
                        f32::INFINITY,
                    ];
                    break;
                }
            }
            let a = Vec3::from_array(t[0].position);
            Triangle {
                a,
                e1: Vec3::from_array(t[1].position) - a,
                e2: Vec3::from_array(t[2].position) - a,
                colors: [t[0].color, t[1].color, t[2].color],
                owner,
                bounds,
            }
        })
        .collect()
}
fn source_ray(triangles: &[Triangle], pose: &Pose, size: [u32; 2], x: u32, y: u32) -> (Ray, bool) {
    let [w, h] = size;
    let tan = (65.0_f32.to_radians() * 0.5).tan();
    let vx = (2.0 * (x as f32 + 0.5) / w as f32 - 1.0) * tan * w as f32 / h as f32;
    let vy = (1.0 - 2.0 * (y as f32 + 0.5) / h as f32) * tan;
    let right = Vec3::new(pose.yaw.cos(), 0.0, pose.yaw.sin());
    let forward = Vec3::new(
        pose.yaw.sin() * pose.pitch.cos(),
        pose.pitch.sin(),
        -pose.yaw.cos() * pose.pitch.cos(),
    );
    let direction = right * vx + right.cross(forward) * vy + forward;
    let eye = Vec3::new(pose.position[0], 1.65, pose.position[1]);
    let mut distance = 50.0;
    let mut color = [0.04, 0.06, 0.05];
    let mut owner = "clear".to_string();
    let mut stable = true;
    for t in triangles {
        let [x0, y0, x1, y1] = t.bounds;
        if x as f32 + 0.5 < x0 || x as f32 + 0.5 > x1 || y as f32 + 0.5 < y0 || y as f32 + 0.5 > y1
        {
            continue;
        }
        let p = direction.cross(t.e2);
        let det = t.e1.dot(p);
        if det.abs() < 1e-9 {
            continue;
        }
        let offset = eye - t.a;
        let u = offset.dot(p) / det;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = offset.cross(t.e1);
        let v = direction.dot(q) / det;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let d = t.e2.dot(q) / det;
        if d < 0.05 || d >= distance {
            continue;
        }
        distance = d;
        color = std::array::from_fn(|i| {
            (1.0 - u - v) * t.colors[0][i] + u * t.colors[1][i] + v * t.colors[2][i]
        });
        owner.clone_from(&t.owner);
        stable = u.min(v).min(1.0 - u - v) > 0.015;
    }
    (
        Ray {
            x,
            y,
            color: rgb(color),
            owner,
        },
        stable,
    )
}
fn source_witnesses(
    triangles: &[Triangle],
    theme: &Theme,
    pose: &Pose,
    size: [u32; 2],
    ui: &[Option<[u8; 3]>],
    page: usize,
    aimed: bool,
) -> Vec<Ray> {
    let [w, h] = size;
    let mut points = BTreeSet::new();
    for y in (64..h - 90).step_by(24) {
        for x in (8..w - 8).step_by(24) {
            points.insert((x, y));
        }
    }
    // Positive AND negative prompt-space evidence. Source-fixed samples, never palette shopping.
    for y in (h / 2 + 14..h / 2 + 36).step_by(3) {
        for x in (8..w - 8).step_by(6) {
            points.insert((x, y));
        }
    }
    let required = if page == 2 && !aimed {
        vec!["entry-0", "entry-1", "entry-2"]
    } else {
        vec!["entry-0"]
    };
    let m = matrix(pose, size);
    for a in theme
        .host
        .anchors
        .iter()
        .filter(|a| required.contains(&a.id.as_str()))
    {
        let mut b = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for dx in [-0.5, 0.5] {
            for dy in [0.0, 1.1] {
                for dz in [-0.5, 0.6] {
                    if let Some([x, y]) = project(
                        m,
                        [
                            a.at[0] + dx * a.size[0],
                            a.at[1] + dy * a.size[1],
                            a.at[2] + dz * a.size[2],
                        ],
                        size,
                    ) {
                        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                    }
                }
            }
        }
        for y in ((b[1].floor() as i32).max(64)..(b[3].ceil() as i32).min(h as i32 - 90))
            .step_by(if aimed { 12 } else { 3 })
        {
            for x in ((b[0].floor() as i32).max(0)..(b[2].ceil() as i32).min(w as i32))
                .step_by(if aimed { 12 } else { 3 })
            {
                points.insert((x as u32, y as u32));
            }
        }
    }
    let mut counts = BTreeMap::<String, usize>::new();
    let mut rays = Vec::new();
    for (x, y) in points {
        if ui[(y * w + x) as usize].is_some() {
            continue;
        }
        let (ray, stable) = source_ray(triangles, pose, size, x, y);
        if !stable {
            continue;
        }
        // Keep the complete coarse lattice/prompt samples, plus bounded semantic witnesses.
        let count = counts.entry(ray.owner.clone()).or_default();
        if *count < 32
            || (x >= 8 && (x - 8).is_multiple_of(24) && y >= 64 && (y - 64).is_multiple_of(24))
            || ((h / 2 + 14..h / 2 + 36).contains(&y) && x >= 8 && (x - 8).is_multiple_of(6))
        {
            *count += 1;
            rays.push(ray);
        }
    }
    for id in required {
        assert!(
            counts.get(&format!("model:{id}")).copied().unwrap_or(0) >= 8,
            "unexecuted source visibility contract needs >=8 fitted model rays: {id} {size:?} aimed={aimed} counts={counts:?}"
        );
    }
    let markers: usize = counts
        .iter()
        .filter(|(k, _)| k.starts_with("marker:"))
        .map(|(_, n)| n)
        .sum();
    assert!(
        markers >= 2,
        "must observe actual host marker surfaces {counts:?}"
    );
    // A close-up aimed at the doorway has no floor in its frustum. Require
    // shell witnesses only for the declared room-context views; aimed views
    // retain the independent model/marker and full prompt raster contracts.
    if !aimed {
        assert!(
            counts.get("floor").copied().unwrap_or(0) >= 20,
            "world shell coverage {counts:?}"
        );
    }
    println!(
        "SPARSE_SOURCE_WITNESSES size={size:?} page={page} aimed={aimed} counts={counts:?}; not framebuffer evidence"
    );
    rays
}

// Same real Renderer draw -> RGBA8-sRGB texture -> aligned copy -> map path as
// capture/navigation_probe. No PNG or output access here and no CPU scene pixels.
fn readback(
    renderer: &render::Renderer,
    camera: &Pose,
    world: Option<&World>,
    clear: Option<wgpu::Color>,
) -> Result<Vec<u8>> {
    let [w, h] = renderer.size;
    if ![[960, 640], [640, 480]].contains(&[w, h]) {
        return Err("fixed local readback dimensions".into());
    }
    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("local sparse contract actual framebuffer"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    if let Some(color) = clear {
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        renderer.queue.submit([encoder.finish()]);
    } else if let Some(world) = world {
        renderer.draw(&view, camera, world, 0)?;
    } else {
        renderer.draw_preview(&view, camera)?;
    }
    let pitch = (w * 4).div_ceil(256) * 256;
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("local sparse actual readback"),
        size: (pitch * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    renderer.wait()?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| e.to_string())?;
    let pixels = mapped
        .chunks(pitch as usize)
        .flat_map(|r| r[..(w * 4) as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}
fn counter_world(c: &Contract) -> World {
    let v = c.world.folder_view.as_ref().unwrap();
    let (mut w, _) = folder_scene::build(&c.snapshot, v.page, c.world.tree_collapsed).unwrap();
    w.interaction = c.world.interaction.clone();
    w
}
fn rejected(c: &Contract, pixels: &[u8], label: &str) {
    let error = c.valid(pixels).unwrap_err();
    println!(
        "SPARSE_NEGATIVE_OK {label} actual_frame_sha256={} reason={error}",
        digest(pixels)
    );
}
fn negatives(c: &Contract, renderer: &mut render::Renderer, positive: &[u8], root: &Path) {
    let black = readback(renderer, &c.pose, None, Some(wgpu::Color::BLACK)).unwrap();
    rejected(c, &black, "blank-black-GPU");
    let hidden: Vec<Vertex> = c
        .vertices
        .iter()
        .take(3)
        .map(|v| Vertex {
            position: [100.0, 100.0, 100.0],
            ..*v
        })
        .collect();
    renderer.set_scene(&hidden);
    rejected(
        c,
        &readback(renderer, &c.pose, None, None).unwrap(),
        "clear-only-GPU",
    );
    rejected(
        c,
        &readback(renderer, &c.pose, Some(&c.world), None).unwrap(),
        "HUD-only-GPU",
    );
    let triangles = source_triangles(&c.theme, &c.vertices, &c.pose, c.size);
    let missing_models: Vec<Vertex> = c
        .vertices
        .as_chunks::<3>()
        .0
        .iter()
        .zip(&triangles)
        .filter(|(_, t)| !t.owner.starts_with("model:"))
        .flat_map(|(t, _)| t.iter().copied())
        .collect();
    renderer.set_scene(&missing_models);
    rejected(
        c,
        &readback(renderer, &c.pose, Some(&c.world), None).unwrap(),
        "models-removed-markers-labels-HUD-retained-GPU",
    );
    let other = if c.theme.name == "office" {
        "archive"
    } else {
        "office"
    };
    let wrong_pack = Theme::load_for_folder_scene(
        &root.join(format!("packs/{other}.toml")),
        &mut hair::theme::AssetCache::default(),
        &c.theme.host,
    )
    .unwrap();
    renderer.set_scene(&wrong_pack.vertices_for(&c.world));
    rejected(
        c,
        &readback(renderer, &c.pose, Some(&c.world), None).unwrap(),
        "wrong-pack-mesh-valid-declaration-GPU",
    );
    assert!(
        c.qualify(positive, &c.pose, &c.world, &c.feedback, &wrong_pack)
            .is_err()
    );
    println!("SPARSE_NEGATIVE_OK declared-pack");
    renderer.set_scene(&c.vertices);
    let wrong_pose = Pose {
        yaw: c.pose.yaw + 0.2,
        ..c.pose
    };
    rejected(
        c,
        &readback(renderer, &wrong_pose, Some(&c.world), None).unwrap(),
        "wrong-camera-valid-declaration-GPU",
    );
    assert!(
        c.qualify(positive, &wrong_pose, &c.world, &c.feedback, &c.theme)
            .is_err()
    );
    println!("SPARSE_NEGATIVE_OK declared-pose");
    let mut w = counter_world(c);
    w.current = "WRONG SOURCE - synthetic counterfactual".into();
    rejected(
        c,
        &readback(renderer, &c.pose, Some(&w), None).unwrap(),
        "wrong-source-header-valid-declaration-GPU",
    );
    assert!(
        c.qualify(positive, &c.pose, &w, &c.feedback, &c.theme)
            .is_err()
    );
    println!("SPARSE_NEGATIVE_OK declared-source");
    let mut w = counter_world(c);
    w.tree_collapsed = !w.tree_collapsed;
    rejected(
        c,
        &readback(renderer, &c.pose, Some(&w), None).unwrap(),
        "wrong-tree-valid-declaration-GPU",
    );
    let v = c.world.folder_view.as_ref().unwrap();
    let (w, host) = folder_scene::build(
        &c.snapshot,
        if v.page == 0 { 2 } else { 0 },
        c.world.tree_collapsed,
    )
    .unwrap();
    let wrong_page = Theme::load_for_folder_scene(
        &c.theme.source,
        &mut hair::theme::AssetCache::default(),
        &host,
    )
    .unwrap();
    renderer.set_scene(&wrong_page.vertices_for(&w));
    rejected(
        c,
        &readback(renderer, &c.pose, Some(&w), None).unwrap(),
        "wrong-page-mesh-and-tree-valid-declaration-GPU",
    );
    assert!(
        c.qualify(positive, &c.pose, &w, &c.feedback, &c.theme)
            .is_err()
    );
    println!("SPARSE_NEGATIVE_OK declared-page");
    renderer.set_scene(&c.vertices);
    renderer.pack_feedback = "WRONG PAGE | WRONG PACK\nWRONG HELP\nWRONG STATUS".into();
    rejected(
        c,
        &readback(renderer, &c.pose, Some(&c.world), None).unwrap(),
        "wrong-bottom-HUD-valid-declaration-GPU",
    );
    renderer.pack_feedback.clone_from(&c.feedback);
    let mut w = counter_world(c);
    w.interaction = Some(hair::interaction::Target {
        anchor: "entry-0".into(),
        kind: hair::interaction::Kind::Enter,
        page: c.world.current.clone(),
        destination: Some("synthetic wrong destination".into()),
        prompt: "E - WRONG PROMPT".into(),
    });
    rejected(
        c,
        &readback(renderer, &c.pose, Some(&w), None).unwrap(),
        if c.world.interaction.is_some() {
            "wrong-prompt-valid-declaration-GPU"
        } else {
            "unexpected-prompt-valid-declaration-GPU"
        },
    );
    if c.world.interaction.is_some() {
        w.interaction = None;
        rejected(
            c,
            &readback(renderer, &c.pose, Some(&w), None).unwrap(),
            "missing-prompt-valid-declaration-GPU",
        );
    }
    renderer.resize(if c.size == [960, 640] {
        [640, 480]
    } else {
        [960, 640]
    });
    rejected(
        c,
        &readback(renderer, &c.pose, Some(&c.world), None).unwrap(),
        "wrong-dimensions-actual-GPU",
    );
    renderer.resize(c.size);
    rejected(
        c,
        &positive[..positive.len() - 1],
        "truncated-actual-readback",
    );
    let mut alpha = positive.to_vec();
    alpha[3] = 0;
    rejected(c, &alpha, "nonopaque-actual-readback-corruption");
    renderer.set_scene(&c.vertices);
    renderer.pack_feedback.clone_from(&c.feedback);
}
// No API is added to Renderer or SmokeOutput. As in navigation_probe, only the
// fixed contract may pass actual bytes to the unchanged exclusive/no-follow leaf writer.
fn write_qualified(
    c: &Contract,
    output: &resources::SmokeOutput,
    name: &str,
    pixels: &[u8],
) -> Result<()> {
    c.valid(pixels)?;
    let mut writer = std::io::BufWriter::new(output.create_capture(name)?);
    let mut png = png::Encoder::new(&mut writer, c.size[0], c.size[1]);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    let mut png = png.write_header().map_err(|e| e.to_string())?;
    png.write_image_data(pixels).map_err(|e| e.to_string())?;
    png.finish().map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    Ok(())
}
fn exclusive_text(path: &Path, text: &str) {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .unwrap();
    file.write_all(text.as_bytes()).unwrap();
    file.flush().unwrap();
}
fn scoped_inputs(root: &Path) -> String {
    // Pin executable-relevant source against compile-time bytes, not just a
    // post-build file hash that could mislabel an old test executable.
    let compiled: &[(&str, &[u8])] = &[
        ("src/main.rs", include_bytes!("main.rs")),
        ("src/lib.rs", include_bytes!("lib.rs")),
        ("src/folder_native.rs", include_bytes!("folder_native.rs")),
        ("src/folder_scene.rs", include_bytes!("folder_scene.rs")),
        ("src/local_explorer.rs", include_bytes!("local_explorer.rs")),
        ("src/theme.rs", include_bytes!("theme.rs")),
        ("src/assets.rs", include_bytes!("assets.rs")),
        ("src/host_scene.rs", include_bytes!("host_scene.rs")),
        ("src/text.rs", include_bytes!("text.rs")),
        ("src/render.rs", include_bytes!("render.rs")),
        ("src/resources.rs", include_bytes!("resources.rs")),
        ("src/shader.wgsl", include_bytes!("shader.wgsl")),
        ("src/interaction.rs", include_bytes!("interaction.rs")),
        ("src/navigation.rs", include_bytes!("navigation.rs")),
        ("src/scene.rs", include_bytes!("scene.rs")),
        (
            "src/folder_qualification_tests.rs",
            include_bytes!("folder_qualification_tests.rs"),
        ),
        (
            "src/folder_sparse_tests.rs",
            include_bytes!("folder_sparse_tests.rs"),
        ),
        ("Cargo.toml", include_bytes!("../Cargo.toml")),
        ("Cargo.lock", include_bytes!("../Cargo.lock")),
    ];
    let mut text =
        String::from("SCOPED_COMPILED_SOURCE_AND_PACK_INPUTS not a full repository freeze\n");
    for (path, bytes) in compiled {
        assert_eq!(
            fs::read(root.join(path)).unwrap().as_slice(),
            *bytes,
            "rebuild: source changed after compilation: {path}"
        );
        writeln!(text, "{} {path}", digest(bytes)).unwrap();
    }
    for leaf in [
        "office.toml",
        "archive.toml",
        "wallDoorway.glb",
        "desk.glb",
        "chairDesk.glb",
        "bookcaseOpen.glb",
        "bookcaseOpenLow.glb",
        "bookcaseClosedDoors.glb",
    ] {
        let path = root.join("packs").join(leaf);
        let bytes = fs::read(&path).unwrap();
        writeln!(text, "{} packs/{leaf}", digest(&bytes)).unwrap();
    }
    text
}
fn check_fixture(fixture: &Path) {
    let expected = expected_snapshot(fixture);
    assert_eq!(hair::local_explorer::snapshot(fixture).unwrap(), expected);
    assert_eq!(fs::read(fixture.join("Read Case.txt")).unwrap(), TEXT_BYTES);
    assert_eq!(fs::read(fixture.join(MKV_NAME)).unwrap(), MKV_BYTES);
    for n in 0..25 {
        let path = fixture.join(format!("Child {n:03}"));
        assert!(fs::symlink_metadata(&path).unwrap().is_dir());
        assert!(
            fs::read_dir(path).unwrap().next().is_none(),
            "synthetic empty child required"
        );
    }
}
fn run_sparse_suite() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let owned = root.parent().unwrap();
    let pins = scoped_inputs(&root);
    let requested = PathBuf::from(
        std::env::var_os("HAIR_SPARSE_PROOF_ROOT")
            .expect("explicit absent output, e.g. slice4/sparse-proof-r1"),
    );
    assert!(requested.is_absolute());
    let parent = requested.parent().unwrap().canonicalize().unwrap();
    assert_eq!(
        parent,
        owned.join("slice4").canonicalize().unwrap(),
        "owned current slice4 outputs only; predecessor proofs remain untouched"
    );
    let output = parent.join(requested.file_name().unwrap());
    let mut fixture_name = requested.file_name().unwrap().to_os_string();
    fixture_name.push(".fixture");
    let fixture = parent.join(fixture_name);
    assert!(separate(&output, &root) && separate(&output, &fixture) && separate(&fixture, &root));
    fs::create_dir(&output).expect("retain evidence; new group must be absent");
    // The successor owns fresh post-build input, never a missing/mutable
    // predecessor slice7 capture fixture. Keep it a SEPARATE sibling, not under
    // capture output or resources; retain exact bytes/modes/membership checks.
    fs::create_dir(&fixture).expect("exclusive post-build fixture, never reuse leftovers");
    for n in 0..25 {
        fs::create_dir(fixture.join(format!("Child {n:03}"))).unwrap();
    }
    fs::write(fixture.join("Read Case.txt"), TEXT_BYTES).unwrap();
    fs::write(fixture.join(MKV_NAME), MKV_BYTES).unwrap();
    assert_eq!(
        fixture.canonicalize().unwrap(),
        fixture,
        "no fixture aliases"
    );
    check_fixture(&fixture);
    let before = inventory(&fixture);
    exclusive_text(&output.join("SCOPED-INPUTS.txt"), &pins);
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let mut qualified = 0;
    let mut generic_rejected = 0;
    for (label, size) in [("normal", [960, 640]), ("small", [640, 480])] {
        for pack in ["office", "archive"] {
            let group = capture_group(&root, &fixture, &output, &format!("{label}-{pack}-context"));
            let aim_group = capture_group(&root, &fixture, &output, &format!("{label}-{pack}-aim"));
            let generic =
                capture_group(&root, &fixture, &output, &format!("{label}-{pack}-generic"));
            let mut a = app(&fixture);
            let mut renderer = pollster::block_on(render::Renderer::new(
                &adapter,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                size,
                &a.themes[0].vertices_for(&a.world),
            ))
            .unwrap();
            renderer.set_materials(&a.themes[0].textures).unwrap();
            a.renderer = Some(renderer);
            if pack == "archive" {
                tap(&mut a, KeyCode::KeyT);
            }
            for (page, collapsed, aimed, name) in [
                (0, false, false, "office.png"),
                (2, false, false, "reader-welcome.png"),
                (2, true, false, "reader-workshop.png"),
                (2, false, true, "office.png"),
            ] {
                while view(&a).page < page {
                    door(&mut a, "folder-next");
                }
                if a.world.tree_collapsed != collapsed {
                    tap(&mut a, KeyCode::KeyM);
                }
                if aimed {
                    aim(&mut a, "entry-0");
                } else {
                    a.pose = context_pose();
                    a.world.explicit_target = None;
                    hair::interaction::refresh(
                        &a.themes[a.theme_index].host,
                        &a.pose,
                        &mut a.world,
                    );
                }
                assert_bindings(&a);
                let c = Contract::new(&root, &fixture, pack, size, page, collapsed, aimed);
                assert_eq!(snapshot(&a), c.snapshot);
                a.update_hud();
                let actual_vertices = a.themes[a.theme_index].vertices_for(&a.world);
                assert_eq!(
                    digest(bytemuck::cast_slice(&actual_vertices)),
                    digest(bytemuck::cast_slice(&c.vertices)),
                    "actual App mesh must equal independently derived fixture mesh"
                );
                let target_group = if aimed { &aim_group } else { &group };
                exclusive_text(
                    &target_group.path().join(format!("{name}.oracle.txt")),
                    &format!(
                        "scoped_inputs_sha256={}\n{}",
                        digest(pins.as_bytes()),
                        c.metadata
                    ),
                );
                let r = a.renderer.as_mut().unwrap();
                assert_eq!(r.size, c.size);
                assert!(r.address_entry.is_none());
                r.set_scene(&actual_vertices);
                let pixels = readback(r, &a.pose, Some(&a.world), None).unwrap();
                // Real image negatives use the valid declaration. CPU state refusals
                // above are not a substitute for these counterfactual framebuffer checks.
                negatives(&c, r, &pixels, &root);
                c.qualify(
                    &pixels,
                    &a.pose,
                    &a.world,
                    &r.pack_feedback,
                    &a.themes[a.theme_index],
                )
                .expect("actual App framebuffer must satisfy exact scoped semantics");
                if !aimed {
                    let colors: BTreeSet<_> = pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|p| [p[0], p[1], p[2]])
                        .collect();
                    let result = r.capture(&generic, name, &a.pose, &a.world);
                    if colors.len() < 20 {
                        let error = result.unwrap_err();
                        assert_eq!(
                            error,
                            format!("GPU image suspiciously flat: {} colors", colors.len())
                        );
                        assert!(!generic.path().join(name).exists());
                        generic_rejected += 1;
                        println!(
                            "SPARSE_GENERIC_REJECTED {} colors={} unchanged_guard=true",
                            generic.path().join(name).display(),
                            colors.len()
                        );
                    } else {
                        result.unwrap();
                        println!(
                            "SPARSE_GENERIC_ACCEPTED {} colors={} unchanged_guard=true",
                            generic.path().join(name).display(),
                            colors.len()
                        );
                    }
                }
                write_qualified(&c, target_group, name, &pixels).unwrap();
                // Exercise the same PNG decoder/tree raster as the generic proof.
                tree_pixels(&a, &target_group.path().join(name), size);
                let png_hash = digest(&fs::read(target_group.path().join(name)).unwrap());
                assert!(
                    write_qualified(&c, target_group, name, &pixels)
                        .unwrap_err()
                        .contains("refusing overwrite")
                );
                assert_eq!(
                    digest(&fs::read(target_group.path().join(name)).unwrap()),
                    png_hash,
                    "refused overwrite preserves PNG bytes"
                );
                if page == 0 {
                    let guard = capture_group(
                        &root,
                        &fixture,
                        &output,
                        &format!("{label}-{pack}-writer-guard"),
                    );
                    std::os::unix::fs::symlink(
                        fixture.join("Read Case.txt"),
                        guard.path().join("office.png"),
                    )
                    .unwrap();
                    assert!(
                        write_qualified(&c, &guard, "office.png", &pixels)
                            .unwrap_err()
                            .contains("refusing overwrite")
                    );
                    assert!(
                        fs::symlink_metadata(guard.path().join("office.png"))
                            .unwrap()
                            .file_type()
                            .is_symlink()
                    );
                    assert_eq!(fs::read(fixture.join("Read Case.txt")).unwrap(), TEXT_BYTES);
                    println!(
                        "SPARSE_WRITER_NEGATIVE_OK post-preflight-symlink no-follow/exclusive refusal; retained link is NOT a capture"
                    );
                }
                println!(
                    "SPARSE_SEMANTIC_CAPTURE {} sha256={png_hash} framebuffer_sha256={} scoped_inputs_sha256={} pack={pack} page={page} collapsed={collapsed} aimed={aimed} generic_matrix_r3_still_failed=true",
                    target_group.path().join(name).display(),
                    digest(&pixels),
                    digest(pins.as_bytes())
                );
                qualified += 1;
            }
            a.renderer.as_ref().unwrap().wait().unwrap();
            assert_eq!(inventory(&fixture), before);
        }
    }
    assert_eq!(qualified, 16);
    assert!(
        generic_rejected > 0,
        "this exact supplemental fixture must still exercise sparse generic refusal; diagnose without changing threshold if not"
    );
    assert_eq!(scoped_inputs(&root), pins);
    check_fixture(&fixture);
    assert_eq!(inventory(&fixture), before);
    exclusive_text(
        &output.join("SUPPLEMENT-COMPLETE.txt"),
        &format!(
            "supplemental_semantic_frames={qualified}\ngeneric_rejections={generic_rejected}\nscoped_inputs_sha256={}\nNOT generic-matrix PASS; NOT human visual approval; NOT native input; NOT user-ready\n",
            digest(pins.as_bytes())
        ),
    );
    println!(
        "SPARSE_SUPPLEMENT_EXECUTED semantic_frames={qualified} generic_rejections={generic_rejected} root={}; parent must inspect exact PNGs; no user-ready claim",
        output.display()
    );
}
