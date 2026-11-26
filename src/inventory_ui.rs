//! Inventory inspector: bounded projected pack meshes or explicit built-in fallback.
use crate::{assets::Vertex, inventory::Item};
/// Runtime mode is explicit, including an EMPTY persistent cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentsState { Persistent, PersistentLoading, PersistentCancelling, RequiredButUnavailable, SessionOnlyFixture }
pub struct Presentation {
    pub state: DocumentsState,
    pub selection: Option<usize>,
    pub representative: bool,
}
/// Logical UI rectangle used for pointer acquisition as well as preview layout.
pub fn preview_contains(size: [u32; 2], point: [f32; 2]) -> bool {
    let [w, h] = size.map(|n| n as f32);
    let split = (w * 0.4).max(220.0);
    w >= 480.0
        && h >= 360.0
        && point.iter().all(|p| p.is_finite())
        && point[0] >= split + 24.0
        && point[0] < w - 40.0
        && point[1] >= 150.0
        && point[1] < h - 142.0
}
pub fn overlay(items: &[Item], selection: usize, yaw: f32, size: [u32; 2]) -> Vec<Vertex> {
    overlay_with_shape(items, selection, yaw, size, None)
}
pub fn overlay_with_shape(
    items: &[Item],
    selection: usize,
    yaw: f32,
    size: [u32; 2],
    shape: Option<&[Vertex]>,
) -> Vec<Vertex> {
    overlay_internal(items, yaw, size, shape, false, Presentation {
        state: DocumentsState::SessionOnlyFixture, selection: Some(selection.min(items.len().saturating_sub(1))), representative: false,
    })
}
/// Textured item path; caller supplies jointly admitted material bindings.
pub fn overlay_with_item(
    items: &[Item],
    selection: usize,
    yaw: f32,
    size: [u32; 2],
    shape: &[Vertex],
) -> Vec<Vertex> {
    overlay_documents(items, yaw, size, shape, Presentation {
        state: DocumentsState::SessionOnlyFixture, selection: Some(selection.min(items.len().saturating_sub(1))), representative: false,
    })
}
pub fn overlay_documents(items: &[Item], yaw: f32, size: [u32; 2], shape: &[Vertex], presentation: Presentation) -> Vec<Vertex> {
    overlay_internal(items, yaw, size, Some(shape), true, presentation)
}
fn overlay_internal(
    items: &[Item],
    yaw: f32,
    size: [u32; 2],
    shape: Option<&[Vertex]>,
    textured: bool,
    presentation: Presentation,
) -> Vec<Vertex> {
    let [w, h] = size.map(|n| n.max(1) as f32);
    let mut p = crate::text::Painter {
        vertices: Vec::new(),
        width: w,
        height: h,
    };
    if w < 480.0 || h < 360.0 || !yaw.is_finite() {
        p.rect(0.0, 0.0, w, h, [0.025, 0.055, 0.035]);
        if w >= 160.0 && h >= 50.0 {
            p.literal_text(8.0, 8.0, "INVENTORY | B/ESC CLOSE", 1.0, [0.65, 1.0, 0.7]);
            p.literal_text(8.0, 30.0, "ENLARGE WINDOW", 1.0, [0.8, 0.8, 0.6]);
        }
        return p.vertices;
    }
    p.rect(16.0, 16.0, w - 32.0, h - 32.0, [0.025, 0.055, 0.035]);

    p.literal_text(32.0, 32.0, "DOCUMENTS", 2.0, [0.65, 1.0, 0.7]);
    p.literal_text(
        32.0,
        60.0,
        match presentation.state {
            DocumentsState::Persistent => "DISK METADATA (UNCHECKED) | B / ESC CLOSE",
            DocumentsState::PersistentLoading => "DISK BUSY / LOADING | ESC CANCELS | F8 STATUS",
            DocumentsState::PersistentCancelling => "DISK CANCELLING | AWAIT RESULT | F8 STATUS",
            DocumentsState::RequiredButUnavailable => "PERSISTENT DOCUMENTS UNAVAILABLE | F8: HELP",
            DocumentsState::SessionOnlyFixture => "SESSION COPIES | B / ESC CLOSE",
        },
        1.0,
        [0.7, 0.8, 0.7],
    );
    let split = (w * 0.4).max(220.0);
    p.rect(split, 86.0, 1.0, h - 170.0, [0.2, 0.3, 0.2]);
    let rows = ((h - 190.0) / 32.0).max(1.0) as usize;
    let selection = presentation.selection.filter(|&i| i < items.len());
    let offset = selection.unwrap_or(0).saturating_sub(rows - 1);
    for (i, item) in items.iter().enumerate().skip(offset).take(rows) {
        let y = 96.0 + (i - offset) as f32 * 32.0;
        if Some(i) == selection {
            p.rect(28.0, y - 5.0, split - 40.0, 29.0, [0.16, 0.32, 0.18]);
        }
        let label = crate::local_explorer::escape_display(&item.name);
        let cols = ((split - 54.0) / 12.0) as usize;
        p.literal_text(
            36.0,
            y,
            &label.chars().take(cols).collect::<String>(),
            2.0,
            [0.85, 0.9, 0.82],
        );
    }
    if let Some(item) = selection.and_then(|i| items.get(i)) {
        let cols = ((w - split - 60.0) / 12.0) as usize;
        let label = crate::local_explorer::escape_display(&item.name);
        p.literal_text(
            split + 24.0,
            94.0,
            &label.chars().take(cols).collect::<String>(),
            2.0,
            [0.85, 0.95, 0.85],
        );
        p.literal_text(
            split + 24.0,
            124.0,
            &format!("{} BYTES | {}", item.stored_bytes.unwrap_or(item.bytes.len() as u64), if item.stored_bytes.is_some() { "METADATA UNCHECKED" } else { "OWNED COPY" }),
            1.0,
            [0.65, 0.75, 0.65],
        );
        let extension = std::path::Path::new(&item.name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let (dimensions, color) = match extension.as_str() {
            "jpg" | "jpeg" | "png" | "webp" => ([1.4, 1.0, 0.15], [0.65, 0.75, 0.45]),
            "mp4" | "mov" => ([1.3, 0.8, 0.4], [0.4, 0.55, 0.65]),
            "mp3" | "wav" | "flac" => ([1.3, 0.8, 0.25], [0.7, 0.45, 0.25]),
            "rs" | "py" | "js" | "json" => ([0.8, 1.1, 0.35], [0.25, 0.65, 0.4]),
            "txt" | "md" | "pdf" => ([0.9, 1.2, 0.12], [0.85, 0.8, 0.6]),
            _ => ([1.0, 1.0, 1.0], [0.65, 0.4, 0.65]),
        };
        let center = [(split + 24.0 + w - 40.0) * 0.5, (150.0 + h - 142.0) * 0.5];
        let scale = ((w - split - 64.0).min(h - 292.0) * 0.45).max(1.0);
        let projected =
            shape.and_then(|s| project_shape_material(s, yaw, center, scale, [w, h], textured));
        let shape_label = if let Some(mesh) = projected {
            p.vertices.extend(mesh);
            if textured {
                "FILE ITEM | TEXTURED"
            } else if shape.is_some_and(|s| s.iter().any(|v| v.texture != 0)) {
                "THEME SHAPE | BASE COLORS ONLY"
            } else {
                "THEME OBJECT SHAPE"
            }
        } else {
            model(&mut p, center, scale, dimensions, color, yaw);
            if shape.is_some() {
                "MODEL UNAVAILABLE | FALLBACK"
            } else {
                "BUILT-IN REPRESENTATION"
            }
        };
        let shape_label = if presentation.representative { "REPRESENTATIVE | NOT VERIFIED" } else { shape_label };
        p.literal_text(split + 24.0, h - 130.0, shape_label, 1.0, [0.65, 0.8, 0.65]);
        p.literal_text(
            split + 24.0,
            h - 106.0,
            "DRAG / LEFT / RIGHT: SPIN",
            1.0,
            [0.6, 0.8, 0.6],
        );
    } else {
        p.literal_text(
            split + 24.0,
            110.0,
            if presentation.state == DocumentsState::RequiredButUnavailable { "CACHE UNAVAILABLE" } else if items.is_empty() { "NO DOCUMENTS" } else { "SELECT A DOCUMENT" },
            2.0,
            [0.7, 0.85, 0.7],
        );
        p.literal_text(
            split + 24.0,
            142.0,
            if presentation.state == DocumentsState::RequiredButUnavailable { "F8: STORAGE SETTINGS" } else { "X COLLECTS A COPY" },
            1.0,
            [0.7, 0.85, 0.7],
        );
    }
    p.literal_text(
        32.0,
        h - 68.0,
        "UP / DOWN: SELECT | ORIGINAL FILES STAY UNCHANGED",
        1.0,
        [0.7, 0.8, 0.7],
    );
    p.literal_text(
        32.0,
        h - 44.0,
        match presentation.state {
            DocumentsState::Persistent => "DELETE: REMOVE CACHE COPY | F8: STATUS / HELP",
            DocumentsState::PersistentLoading | DocumentsState::PersistentCancelling => "HELD CACHE BUSY | NO MUTATION / ADOPTION | F8 HELP",
            DocumentsState::RequiredButUnavailable => "NO SESSION FALLBACK | F8: STATUS / HELP",
            DocumentsState::SessionOnlyFixture => "INVENTORY IS CURRENTLY SESSION-ONLY",
        },
        1.0,
        [0.8, 0.75, 0.5],
    );
    p.vertices
}
#[cfg(test)]
fn project_shape(
    shape: &[Vertex],
    yaw: f32,
    center: [f32; 2],
    scale: f32,
    viewport: [f32; 2],
) -> Option<Vec<Vertex>> {
    project_shape_material(shape, yaw, center, scale, viewport, false)
}
fn project_shape_material(
    shape: &[Vertex],
    yaw: f32,
    center: [f32; 2],
    scale: f32,
    viewport: [f32; 2],
    textured: bool,
) -> Option<Vec<Vertex>> {
    use glam::{Mat3, Vec3};
    if shape.is_empty()
        || shape.len() > 32766
        || !shape.len().is_multiple_of(3)
        || !yaw.is_finite()
        || shape.iter().any(|v| {
            v.position
                .iter()
                .chain(v.color.iter())
                .any(|n| !n.is_finite())
        })
    {
        return None;
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for v in shape {
        let p = Vec3::from_array(v.position);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let extent = hi - lo;
    if !extent.is_finite() {
        return None;
    }
    let origin = lo + extent * 0.5;
    let radius = shape
        .iter()
        .map(|v| (Vec3::from_array(v.position) - origin).length())
        .fold(0.0, f32::max);
    if !radius.is_finite() || radius <= 0.000001 {
        return None;
    }
    let rotation = Mat3::from_rotation_x(-0.25) * Mat3::from_rotation_y(yaw + 0.35);
    let mut triangles = Vec::with_capacity(shape.len() / 3);
    for triangle in shape.as_chunks::<3>().0 {
        let points =
            triangle.map(|v| rotation * ((Vec3::from_array(v.position) - origin) / radius));
        if (points[1] - points[0])
            .cross(points[2] - points[0])
            .length_squared()
            <= 1.0e-12
        {
            continue;
        }
        let projected = std::array::from_fn::<_, 3, _>(|i| {
            let v = points[i];
            let perspective = 3.0 / (3.0 - v.z);
            Vertex {
                position: [
                    2.0 * (center[0] + v.x * scale * perspective) / viewport[0] - 1.0,
                    1.0 - 2.0 * (center[1] - v.y * scale * perspective) / viewport[1],
                    if textured {
                        0.9 - 1.0 / (3.0 - v.z)
                    } else {
                        0.0
                    },
                ],
                color: triangle[i].color,
                uv: if textured { triangle[i].uv } else { [0.0; 2] },
                texture: if textured { triangle[i].texture } else { 0 },
            }
        });
        triangles.push((points.iter().map(|v| v.z).sum::<f32>(), projected));
    }
    if triangles.is_empty() {
        return None;
    }
    triangles.sort_by(|a, b| a.0.total_cmp(&b.0));
    Some(
        triangles
            .into_iter()
            .flat_map(|(_, vertices)| vertices)
            .collect(),
    )
}
fn model(
    p: &mut crate::text::Painter,
    center: [f32; 2],
    scale: f32,
    size: [f32; 3],
    color: [f32; 3],
    yaw: f32,
) {
    let corners = [
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ]
    .map(|q| {
        let v = glam::Vec3::from_array(q) * glam::Vec3::from_array(size) * 0.5;
        glam::Mat3::from_rotation_x(-0.25) * glam::Mat3::from_rotation_y(yaw + 0.35) * v
    });
    let mut faces = [
        ([0, 1, 2, 3], 0.45),
        ([4, 7, 6, 5], 1.0),
        ([0, 4, 5, 1], 0.5),
        ([3, 2, 6, 7], 0.9),
        ([0, 3, 7, 4], 0.65),
        ([1, 5, 6, 2], 0.75),
    ];
    faces.sort_by(|(a, _), (b, _)| {
        let depth = |q: &[usize; 4]| q.iter().map(|i| corners[*i].z).sum::<f32>();
        depth(a).total_cmp(&depth(b))
    });
    for (face, light) in faces {
        for j in [0, 1, 2, 0, 2, 3] {
            let v = corners[face[j]];
            let perspective = 3.0 / (3.0 - v.z);
            p.vertices.push(Vertex {
                position: [
                    2.0 * (center[0] + v.x * scale * perspective) / p.width - 1.0,
                    1.0 - 2.0 * (center[1] - v.y * scale * perspective) / p.height,
                    0.0,
                ],
                color: color.map(|c| c * light),
                uv: [0.0; 2],
                texture: 0,
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supplied_mesh_is_rotatable_uniformly_fitted_and_confined_to_inspector() {
        let items = [Item {
            id: 1,
            name: "Any.txt".into(),
            bytes: vec![],
            stored_bytes: None,
        }];
        let shape =
            [[-2.0, -1.0, 0.0], [2.0, -1.0, 0.0], [-1.0, 2.0, 1.0]].map(|position| Vertex {
                position,
                color: [0.123, 0.234, 0.345],
                uv: [0.4, 0.6],
                texture: 0,
            });
        let before = bytemuck::cast_slice::<_, u8>(&shape).to_vec();
        let geometry = |mesh: &[Vertex], yaw| {
            overlay_with_shape(&items, 0, yaw, [640, 480], Some(mesh))
                .into_iter()
                .filter(|v| v.color == [0.123, 0.234, 0.345])
                .collect::<Vec<_>>()
        };
        let a = geometry(&shape, 0.0);
        let b = geometry(&shape, 0.7);
        assert_eq!(
            a.len(),
            3,
            "show actual supplied mesh, not a filename-category box"
        );
        assert_eq!(b.len(), 3);
        assert_ne!(
            bytemuck::cast_slice::<_, u8>(&a),
            bytemuck::cast_slice::<_, u8>(&b)
        );
        for v in a.iter().chain(&b) {
            assert!(v.position.iter().all(|n| n.is_finite()));
            assert!(
                (-0.125..=0.875).contains(&v.position[0])
                    && (-0.475..=0.375).contains(&v.position[1]),
                "mesh must not overlap names, footer or list"
            );
        }
        let translated = shape.map(|v| Vertex {
            position: [
                v.position[0] * 2.0 + 4.0,
                v.position[1] * 2.0 + 5.0,
                v.position[2] * 2.0 + 6.0,
            ],
            ..v
        });
        let c = geometry(&translated, 0.0);
        for (x, y) in a.iter().zip(c) {
            for axis in 0..2 {
                assert!(
                    (x.position[axis] - y.position[axis]).abs() < 0.00001,
                    "fit must ignore source translation and uniform units"
                );
            }
        }
        assert_eq!(before, bytemuck::cast_slice::<_, u8>(&shape));
    }
    #[test]
    fn refused_meshes_use_explicit_finite_fallback() {
        let vertex = Vertex {
            position: [0.0; 3],
            color: [0.123, 0.234, 0.345],
            uv: [0.0; 2],
            texture: 0,
        };
        let line = [0.0, 1.0, 2.0]
            .map(|x| Vertex {
                position: [x, 0.0, 0.0],
                ..vertex
            })
            .to_vec();
        let invalid = vec![
            Vertex {
                position: [f32::NAN, 0.0, 0.0],
                ..vertex
            };
            3
        ];
        let oversized = vec![vertex; 32769];
        let items = [Item {
            id: 1,
            name: "Any.txt".into(),
            bytes: vec![],
            stored_bytes: None,
        }];
        for mesh in [vec![], vec![vertex; 3], invalid, oversized, line] {
            assert!(
                project_shape(&mesh, 0.0, [440.0, 252.0], 90.0, [640.0, 480.0]).is_none(),
                "degenerate, oversized or nonfinite geometry must not claim a rendered model"
            );
            let ui = overlay_with_shape(&items, 0, 0.0, [640, 480], Some(&mesh));
            assert!(ui.iter().all(|v| {
                v.position
                    .iter()
                    .chain(v.color.iter())
                    .all(|n| n.is_finite())
            }));
            let mut p = crate::text::Painter {
                vertices: vec![],
                width: 640.0,
                height: 480.0,
            };
            p.literal_text(
                280.0,
                350.0,
                "MODEL UNAVAILABLE | FALLBACK",
                1.0,
                [0.65, 0.8, 0.65],
            );
            assert!(
                ui.windows(p.vertices.len())
                    .any(|v| bytemuck::cast_slice::<_, u8>(v)
                        == bytemuck::cast_slice::<_, u8>(&p.vertices))
            );
        }
    }
    #[test]
    fn mesh_labels_disclose_material_profile_and_vertex_boundary_is_enforced() {
        let items = [Item {
            id: 1,
            name: "Any.txt".into(),
            bytes: vec![],
            stored_bytes: None,
        }];
        let base = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]].map(|position| Vertex {
            position,
            color: [0.123, 0.234, 0.345],
            uv: [0.0; 2],
            texture: 0,
        });
        for (texture, label) in [
            (0, "THEME OBJECT SHAPE"),
            (1, "THEME SHAPE | BASE COLORS ONLY"),
        ] {
            let shape = base.map(|v| Vertex { texture, ..v });
            let ui = overlay_with_shape(&items, 0, 0.0, [640, 480], Some(&shape));
            let mut p = crate::text::Painter {
                vertices: vec![],
                width: 640.0,
                height: 480.0,
            };
            p.literal_text(280.0, 350.0, label, 1.0, [0.65, 0.8, 0.65]);
            assert!(
                ui.windows(p.vertices.len())
                    .any(|v| bytemuck::cast_slice::<_, u8>(v)
                        == bytemuck::cast_slice::<_, u8>(&p.vertices))
            );
        }
        let max = base.into_iter().cycle().take(32766).collect::<Vec<_>>();
        assert_eq!(
            project_shape(&max, 0.0, [440.0, 252.0], 90.0, [640.0, 480.0])
                .unwrap()
                .len(),
            max.len()
        );
        for len in [32767, 32768, 32769] {
            let mesh = base.into_iter().cycle().take(len).collect::<Vec<_>>();
            assert!(project_shape(&mesh, 0.0, [440.0, 252.0], 90.0, [640.0, 480.0]).is_none());
        }
        let invalid = base.map(|v| Vertex {
            color: [f32::NAN; 3],
            ..v
        });
        assert!(project_shape(&invalid, 0.0, [440.0, 252.0], 90.0, [640.0, 480.0]).is_none());
    }
    #[test]
    fn extreme_mesh_stays_clear_of_status_label_across_sizes_and_rotation() {
        let points = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        let faces = [
            [0, 2, 4],
            [2, 1, 4],
            [1, 3, 4],
            [3, 0, 4],
            [2, 0, 5],
            [1, 2, 5],
            [3, 1, 5],
            [0, 3, 5],
        ];
        let mesh = faces
            .into_iter()
            .flatten()
            .map(|i| Vertex {
                position: points[i],
                color: [0.123, 0.234, 0.345],
                uv: [0.0; 2],
                texture: 0,
            })
            .collect::<Vec<_>>();
        let items = [Item {
            id: 1,
            name: "Any.txt".into(),
            bytes: vec![],
            stored_bytes: None,
        }];
        for size in [[480, 360], [640, 480], [960, 640], [1280, 960]] {
            for yaw in [0.0, 0.7, 1.4, 2.4] {
                let [w, h] = size.map(|n| n as f32);
                let split = (w * 0.4).max(220.0);
                let ui = overlay_with_shape(&items, 0, yaw, size, Some(&mesh));
                let vertices = ui
                    .iter()
                    .filter(|v| v.color == [0.123, 0.234, 0.345])
                    .collect::<Vec<_>>();
                assert_eq!(vertices.len(), mesh.len());
                for v in vertices {
                    let x = (v.position[0] + 1.0) * w * 0.5;
                    let y = (1.0 - v.position[1]) * h * 0.5;
                    assert!(
                        x >= split + 24.0 && x <= w - 40.0 && y >= 150.0 && y <= h - 138.0,
                        "model intersects text/padding at {size:?}, yaw {yaw}: ({x},{y})"
                    );
                }
            }
        }
    }
    #[test]
    fn selected_name_is_literal_escaped_in_detail_pane() {
        let item = Item {
            id: 1,
            name: "evil\u{202e}.txt".into(),
            bytes: vec![],
            stored_bytes: None,
        };
        let actual = overlay(std::slice::from_ref(&item), 0, 0.0, [640, 480]);
        let label = crate::local_explorer::escape_display(&item.name);
        let mut p = crate::text::Painter {
            vertices: vec![],
            width: 640.0,
            height: 480.0,
        };
        p.literal_text(
            280.0,
            94.0,
            &label.chars().take(27).collect::<String>(),
            2.0,
            [0.85, 0.95, 0.85],
        );
        assert!(
            actual
                .windows(p.vertices.len())
                .any(|w| bytemuck::cast_slice::<_, u8>(w)
                    == bytemuck::cast_slice::<_, u8>(&p.vertices)),
            "selected file name must escape bidi/control characters like its list row"
        );
    }
    #[test]
    fn undersized_inventory_always_has_a_visible_close_surface() {
        for size in [[320, 240], [120, 40], [1, 1]] {
            let vertices = overlay(&[], 0, 0.0, size);
            assert!(
                !vertices.is_empty(),
                "inventory cannot become an invisible input owner at {size:?}"
            );
            assert!(
                vertices
                    .iter()
                    .all(|v| v.position.iter().all(|n| n.is_finite())
                        && v.position[0].abs() <= 1.0
                        && v.position[1].abs() <= 1.0)
            );
        }
    }
    #[test]
    fn inspect_geometry_is_visible_bounded_and_rotatable() {
        let items = [Item {
            id: 1,
            name: "Photo.jpg".into(),
            bytes: vec![1, 2, 3],
            stored_bytes: None,
        }];
        let first = overlay(&items, 0, 0.0, [640, 480]);
        let rotated = overlay(&items, 0, 0.7, [640, 480]);
        assert!(!first.is_empty());
        assert!(
            first
                .iter()
                .all(|v| v.position.iter().all(|n| n.is_finite())
                    && v.position[0].abs() <= 1.0
                    && v.position[1].abs() <= 1.0)
        );
        assert_ne!(
            bytemuck::cast_slice::<_, u8>(&first),
            bytemuck::cast_slice::<_, u8>(&rotated)
        );
    }
}
