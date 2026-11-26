//! Bounded cosmetic clone; owns no file or navigation authority.
use crate::{Pose, assets::Vertex, sidebar::Frame};
pub const SECONDS: f32 = 0.65;
#[derive(Clone)]
pub struct Effect {
    pub frame: Frame,
    pub textures: Vec<std::sync::Arc<crate::assets::Texture>>,
    source: Vec<Vertex>,
}
impl Effect {
    pub fn new(model: &[Vertex], pose: &Pose, frame: Frame) -> Option<Self> {
        use glam::{Mat4, Vec3};
        if model.is_empty()
            || model.len() > 32766
            || !model.len().is_multiple_of(3)
            || model.iter().any(|v| {
                v.position
                    .iter()
                    .chain(v.color.iter())
                    .any(|n| !n.is_finite())
            })
            || ![
                pose.position[0],
                pose.position[1],
                pose.feet_y,
                pose.yaw,
                pose.pitch,
            ]
            .iter()
            .all(|n| n.is_finite())
        {
            return None;
        }
        let eye = Vec3::new(pose.position[0], pose.eye_height(), pose.position[1]);
        let direction = Vec3::new(
            pose.yaw.sin() * pose.pitch.cos(),
            pose.pitch.sin(),
            -pose.yaw.cos() * pose.pitch.cos(),
        );
        let [w, h] = frame.physical.map(|v| v.max(1) as f32);
        let left = frame.left as f32;
        let content = (w - left).max(1.0);
        let matrix = Mat4::perspective_rh(65.0_f32.to_radians(), content / h, 0.05, 50.0)
            * Mat4::look_to_rh(eye, direction, Vec3::Y);
        let mut triangles = Vec::new();
        for triangle in model.as_chunks::<3>().0 {
            let clip =
                [0, 1, 2].map(|i| matrix * Vec3::from_array(triangle[i].position).extend(1.0));
            if clip
                .iter()
                .any(|p| !p.is_finite() || p.w <= 0.05 || p.z < 0.0 || p.z > p.w)
            {
                continue;
            }
            let vertices = std::array::from_fn::<_, 3, _>(|i| {
                let p = clip[i] / clip[i].w;
                Vertex {
                    position: [
                        (2.0 * (left + (p.x.clamp(-1.0, 1.0) + 1.0) * content * 0.5) / w - 1.0)
                            .clamp(-1.0, 1.0),
                        p.y.clamp(-1.0, 1.0),
                        0.0,
                    ],
                    color: triangle[i].color,
                    uv: triangle[i].uv,
                    texture: triangle[i].texture,
                }
            });
            triangles.push((clip.iter().map(|p| p.z / p.w).sum::<f32>(), vertices));
        }
        triangles.sort_by(|a, b| b.0.total_cmp(&a.0));
        let source = triangles
            .into_iter()
            .flat_map(|(_, v)| v)
            .collect::<Vec<_>>();
        (!source.is_empty()).then_some(Self {
            frame,
            source,
            textures: vec![],
        })
    }
    pub fn paint(&self, age: f32, static_motion: bool) -> Vec<Vertex> {
        self.paint_to(age, static_motion, [0.78, -0.72])
    }
    pub fn paint_to(&self, age: f32, static_motion: bool, target: [f32; 2]) -> Vec<Vertex> {
        if static_motion || !age.is_finite() || !(0.0..SECONDS).contains(&age) {
            return vec![];
        }
        let t = age / SECONDS;
        let eased = 1.0 - (1.0 - t).powi(3);
        let scale = (1.0 - t).powi(2);
        let center = [0, 1].map(|axis| {
            self.source.iter().map(|v| v.position[axis]).sum::<f32>() / self.source.len() as f32
        });

        let point = [
            center[0] + (target[0] - center[0]) * eased,
            center[1] + (target[1] - center[1]) * eased + 0.18 * (t * std::f32::consts::PI).sin(),
        ];
        self.source
            .iter()
            .map(|v| Vertex {
                position: [
                    (point[0] + (v.position[0] - center[0]) * scale).clamp(-1.0, 1.0),
                    (point[1] + (v.position[1] - center[1]) * scale).clamp(-1.0, 1.0),
                    0.0,
                ],
                ..*v
            })
            .collect()
    }
}
pub fn badge(frame: Frame, count: usize, recent: bool) -> Vec<Vertex> {
    let [w, h] = frame.full.map(|n| n as f32);
    if w < 240.0 || h < 180.0 {
        return vec![];
    }
    let mut p = crate::text::Painter {
        vertices: vec![],
        width: w,
        height: h,
    };
    let y = h * 0.86;
    p.rect(
        w - 184.0,
        y - 20.0,
        168.0,
        40.0,
        if recent {
            [0.08, 0.28, 0.12]
        } else {
            [0.025, 0.07, 0.04]
        },
    );
    p.literal_text(
        w - 176.0,
        y - 12.0,
        &format!("B INVENTORY {}", count.min(crate::inventory::MAX_ITEMS)),
        1.0,
        [0.65, 1.0, 0.7],
    );
    p.literal_text(
        w - 176.0,
        y + 4.0,
        if recent {
            "COPY COLLECTED"
        } else {
            "X COLLECTS COPY"
        },
        1.0,
        [0.7, 0.85, 0.7],
    );
    p.vertices
}
#[cfg(test)]
mod tests {
    use super::*;
    fn mesh() -> Vec<Vertex> {
        [[-0.3, 1.35, -3.0], [0.3, 1.35, -3.0], [0.0, 1.95, -3.0]]
            .map(|position| Vertex {
                position,
                color: [0.7; 3],
                uv: [0.0; 2],
                texture: 0,
            })
            .to_vec()
    }
    #[test]
    fn projected_clone_moves_shrinks_and_expires_without_mutating_source() {
        let model = mesh();
        let before = bytemuck::cast_slice::<_, u8>(&model).to_vec();
        for on in [false, true] {
            let frame = Frame::new([960, 640], 100, on);
            let e = Effect::new(
                &model,
                &Pose {
                    eye_offset: 1.65,
                    feet_y: 0.0,
                    position: [0.0, 0.0],
                    yaw: 0.0,
                    pitch: 0.0,
                },
                frame,
            )
            .unwrap();
            let a = e.paint(0.0, false);
            let b = e.paint(0.4, false);
            assert_eq!(a.len(), 3);
            assert_eq!(b.len(), 3);
            let mean = |v: &[Vertex]| v.iter().map(|n| n.position[0]).sum::<f32>() / v.len() as f32;
            assert!(
                (mean(&a) - if on { 0.25 } else { 0.0 }).abs() < 0.0001,
                "projection must use reserved content width"
            );
            assert!(mean(&b) > mean(&a));
            let width = |v: &[Vertex]| {
                v.iter()
                    .map(|n| n.position[0])
                    .fold(f32::NEG_INFINITY, f32::max)
                    - v.iter()
                        .map(|n| n.position[0])
                        .fold(f32::INFINITY, f32::min)
            };
            assert!(width(&b) < width(&a));
            assert!(e.paint(SECONDS, false).is_empty());
            assert!(e.paint(0.2, true).is_empty());
            assert!(e.paint(f32::NAN, false).is_empty());
            assert!(
                b.iter()
                    .all(|v| v.position.iter().all(|n| n.is_finite() && n.abs() <= 1.0))
            );
        }
        assert_eq!(before, bytemuck::cast_slice::<_, u8>(&model));
    }
    #[test]
    fn invalid_and_behind_camera_geometry_is_not_animated() {
        let frame = Frame::new([960, 640], 100, false);
        assert!(Effect::new(&[], &Pose::default(), frame).is_none());
        let mut behind = mesh();
        for v in &mut behind {
            v.position[2] = 3.0;
        }
        assert!(Effect::new(&behind, &Pose::default(), frame).is_none());
        let mut invalid = mesh();
        invalid[0].color[0] = f32::NAN;
        assert!(Effect::new(&invalid, &Pose::default(), frame).is_none());
    }
}
