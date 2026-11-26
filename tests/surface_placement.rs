use hair::{
    assets::Vertex,
    placement::{
        self,
        surface::{self, Contact},
    },
};
fn v(p: [f32; 3]) -> Vertex {
    Vertex {
        position: p,
        color: [0.2, 0.7, 0.4],
        uv: [0.3, 0.8],
        texture: 7,
    }
}
fn model() -> Vec<Vertex> {
    vec![v([-1., 2., -0.5]), v([1., 2., -0.5]), v([-1., 4., 0.5])]
}
#[test]
fn planar_items_inside_a_solid_are_not_mistaken_for_mere_support_contact() {
    let mut host = hair::host_scene::HostScene::shared_default();
    host.room = [-5., -5., 5., 5.];
    host.spawn = [4., 4.];
    host.height = 3.;
    host.open_platform = true;
    host.anchors.clear();
    host.geometry.clear();
    host.overhead.clear();
    host.anchors.push(hair::host_scene::Anchor {
        id: "support".into(),
        role: "document".into(),
        at: [0.; 3],
        size: [1.; 3],
        binding: "explicit-page".into(),
    });
    let plane = vec![v([-0.2, 0.2, 0.]), v([0.2, 0.2, 0.]), v([0.2, 0.8, 0.])];
    assert!(
        surface::clearance(&host, &plane, placement::Overlap::Prevent).is_err(),
        "a zero-thickness image can still penetrate an object"
    );
    surface::clearance(&host, &plane, placement::Overlap::Allow).unwrap();
    let touching = plane
        .iter()
        .map(|v| Vertex {
            position: [v.position[0], v.position[1], 0.5],
            ..*v
        })
        .collect::<Vec<_>>();
    surface::clearance(&host, &touching, placement::Overlap::Prevent).unwrap();
}
#[test]
fn near_unit_saved_normals_share_render_and_collision_frame() {
    let size = [0.8, 0.85, 0.28];
    let corners = (0..8)
        .map(|i| {
            [
                size[0] * if i & 1 == 0 { -0.5 } else { 0.5 },
                size[1] * if i & 2 == 0 { 0.0 } else { 1.0 },
                size[2] * if i & 4 == 0 { -0.5 } else { 0.5 },
            ]
        })
        .collect::<Vec<_>>();
    let mut cube = vec![];
    for face in [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ] {
        for i in [0, 1, 2, 0, 2, 3] {
            cube.push(Vertex {
                position: corners[face[i]],
                color: [0.2, 0.7, 0.4],
                uv: [0.; 2],
                texture: 0,
            });
        }
    }
    let p = hair::placement_store::Position {
        properties: Default::default(),
        at: [2., -1.],
        yaw: 0.7,
        size,
        allow_overlap: true,
        surface: Some(Contact {
            at: [2., 5., -1.],
            normal: [0., 0.60002, 0.8],
        }),
    };
    let rendered = p.transform(&cube).unwrap();
    let b = p.bounds().unwrap();
    let low = [
        b.at[0] - b.size[0] * 0.5,
        b.at[1],
        b.at[2] - b.size[2] * 0.5,
    ];
    for (axis, _) in low.iter().enumerate() {
        let min = rendered
            .iter()
            .map(|v| v.position[axis])
            .fold(f32::INFINITY, f32::min);
        let max = rendered
            .iter()
            .map(|v| v.position[axis])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (min - low[axis]).abs() < 0.000001,
            "axis {axis}: mesh min {min}, collision min {}",
            low[axis]
        );
        assert!((max - (low[axis] + b.size[axis])).abs() < 0.000001);
    }
}
#[test]
fn bottom_contacts_top_side_underside_and_sloped_surfaces() {
    for normal in [[0., 1., 0.], [1., 0., 0.], [0., -1., 0.], [0., 0.6, 0.8]] {
        let contact = Contact {
            at: [1., 5., -2.],
            normal,
        };
        let before = model();
        let out = surface::attached(&before, &contact, 0.7, [0.8, 0.85, 0.28]).unwrap();
        let n = glam::Vec3::from_array(normal);
        let origin = glam::Vec3::from_array(contact.at);
        for (src, dst) in before.iter().zip(&out) {
            assert_eq!(
                (src.color, src.uv, src.texture),
                (dst.color, dst.uv, dst.texture)
            );
            let distance = (glam::Vec3::from_array(dst.position) - origin).dot(n);
            if src.position[1] == 2.0 {
                assert!(distance.abs() < 1e-5, "actual bottom lies in support plane");
            } else {
                assert!(distance > 0.0, "item extends outward from contact face");
            }
        }
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&before),
            bytemuck::cast_slice::<_, u8>(&model())
        );
    }
    let c = Contact {
        at: [2., 0., -1.],
        normal: [0., 1., 0.],
    };
    let old = placement::grounded(&model(), [2., -1.], 0.3, [0.8, 0.85, 0.28]).unwrap();
    let new = surface::attached(&model(), &c, 0.3, [0.8, 0.85, 0.28]).unwrap();
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&new),
        bytemuck::cast_slice::<_, u8>(&old),
        "floor remains the existing grounded transform"
    );
}
#[test]
fn ray_selects_nearest_actual_triangle_face_not_its_bounding_box() {
    let front = vec![v([-1., 0., 0.]), v([1., 0., 0.]), v([0., 3., 0.])];
    let far = front
        .iter()
        .map(|a| Vertex {
            position: [a.position[0], a.position[1], -1.],
            ..*a
        })
        .collect::<Vec<_>>();
    let pose = hair::Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [0., 2.],
        yaw: 0.,
        pitch: 0.,
    };
    let hit = surface::target(&pose, &[&far, &front]).unwrap().unwrap();
    assert!((hit.at[2]).abs() < 1e-6);
    assert_eq!(hit.normal, [0., 0., 1.]);
    let miss = hair::Pose {
        position: [0.9, 2.],
        ..pose
    };
    assert!(
        surface::target(&miss, &[&front]).unwrap().is_none(),
        "point inside AABB but outside triangle is not a surface hit"
    );
    let floor = hair::Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [0., 2.],
        yaw: 0.,
        pitch: -0.8,
    };
    let hit = surface::target(&floor, &[]).unwrap().unwrap();
    assert_eq!(hit.normal, [0., 1., 0.]);
    let at = placement::floor_target(&floor).unwrap();
    assert!((hit.at[0] - at[0]).abs() < 1e-5 && (hit.at[2] - at[1]).abs() < 1e-5);
}
#[test]
fn surface_clearance_allows_support_contact_but_not_penetration_or_world_escape() {
    let mut host = hair::host_scene::HostScene::shared_default();
    host.room = [-5., -5., 5., 5.];
    host.spawn = [4., 4.];
    host.height = 3.;
    host.open_platform = true;
    host.anchors.clear();
    host.geometry.clear();
    host.overhead.clear();
    host.anchors.push(hair::host_scene::Anchor {
        id: "support".into(),
        role: "document".into(),
        at: [0.; 3],
        size: [1.; 3],
        binding: "explicit-page".into(),
    });
    let mesh = surface::attached(
        &model(),
        &Contact {
            at: [0., 1., 0.],
            normal: [0., 1., 0.],
        },
        0.,
        [0.8, 0.85, 0.28],
    )
    .unwrap();
    surface::clearance(&host, &mesh, placement::Overlap::Prevent).unwrap();
    let penetrating = mesh
        .iter()
        .map(|v| Vertex {
            position: [v.position[0], v.position[1] - 0.1, v.position[2]],
            ..*v
        })
        .collect::<Vec<_>>();
    assert!(surface::clearance(&host, &penetrating, placement::Overlap::Prevent).is_err());
    surface::clearance(&host, &penetrating, placement::Overlap::Allow).unwrap();
    host.geometry.push(hair::host_scene::Geometry {
        at: [0., 1.2, 0.],
        size: [1., 0.5, 1.],
    });
    assert!(surface::clearance(&host, &mesh, placement::Overlap::Allow).is_err());
    host.geometry.clear();
    let below = surface::attached(
        &model(),
        &Contact {
            at: [0., -0.1, 0.],
            normal: [0., 1., 0.],
        },
        0.,
        [0.8, 0.85, 0.28],
    )
    .unwrap();
    assert!(surface::clearance(&host, &below, placement::Overlap::Allow).is_err());
    let tall = surface::attached(
        &model(),
        &Contact {
            at: [0., 4., 0.],
            normal: [0., 1., 0.],
        },
        0.,
        [0.8, 0.85, 0.28],
    )
    .unwrap();
    surface::clearance(&host, &tall, placement::Overlap::Allow).unwrap();
    host.open_platform = false;
    assert!(surface::clearance(&host, &tall, placement::Overlap::Allow).is_err());
}
#[test]
fn invalid_contacts_and_unbounded_surface_work_are_refused() {
    for normal in [[0.; 3], [0., 2., 0.], [f32::NAN, 1., 0.]] {
        assert!(
            surface::attached(
                &model(),
                &Contact {
                    at: [0.; 3],
                    normal
                },
                0.,
                [1.; 3]
            )
            .is_err()
        );
    }
    assert!(
        surface::attached(
            &model(),
            &Contact {
                at: [f32::NAN, 0., 0.],
                normal: [0., 1., 0.]
            },
            0.,
            [1.; 3]
        )
        .is_err()
    );
    let large = vec![v([0.; 3]); surface::MAX_VERTICES + 3];
    let pose = hair::Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [0., 2.],
        yaw: 0.,
        pitch: 0.,
    };
    assert!(surface::target(&pose, &[&large]).is_err());
}
