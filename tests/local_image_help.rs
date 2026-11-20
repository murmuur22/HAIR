use hair::{assets::Vertex, folder_scene, local_explorer::Snapshot, text};

fn pixel_bounds(q: &[Vertex], width: u32, height: u32) -> [f32; 4] {
    let mut b = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for v in q {
        let x = (v.position[0] + 1.0) * width as f32 * 0.5;
        let y = (1.0 - v.position[1]) * height as f32 * 0.5;
        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    b
}

#[test]
fn opened_image_recovery_controls_retain_readable_two_pixel_glyphs() {
    let snapshot = Snapshot {
        folder: "/ImageHelpFixture".into(),
        entries: Vec::new(),
    };
    let (world, _) = folder_scene::build(&snapshot, 0, false).unwrap();
    for [width, height] in [[960, 640], [640, 480]] {
        let vertices = text::image_overlay(&world, width, height);
        let mut rows = [0_usize; 2];
        for q in vertices.as_chunks::<6>().0 {
            if q[0].color != [0.6, 1.0, 0.7] {
                continue;
            }
            let b = pixel_bounds(q, width, height);
            assert!(
                (b[2] - b[0] - 2.0).abs() < 0.001 && (b[3] - b[1] - 2.0).abs() < 0.001,
                "image recovery controls regressed to tiny glyphs at {width}x{height}: {b:?}"
            );
            assert!(b[0] >= 16.0 && b[2] <= width as f32 - 16.0);
            assert!(b[1] > height as f32 - 82.0 && b[3] < height as f32 - 16.0);
            let row = usize::from(b[1] >= height as f32 - 48.0 - 0.001);
            rows[row] += 1;
        }
        assert!(
            rows.iter().all(|n| *n > 0),
            "both mode/source and close/retry help rows must remain"
        );
    }
}
