use hair::{
    folder_scene, interaction,
    local_explorer::{Entry, Kind, Snapshot},
    text,
};
#[test]
fn local_file_prompt_does_not_cover_the_central_object_preview() {
    for name in ["image.png", "notes.txt"] {
        let snapshot = Snapshot {
            folder: "/owned-prompt-fixture".into(),
            entries: vec![Entry {
                name: name.into(),
                path: format!("/owned-prompt-fixture/{name}").into(),
                kind: Kind::File,
            }],
        };
        let (mut world, _) = folder_scene::build(&snapshot, 0, false).unwrap();
        world.interaction = Some(interaction::Target {
            anchor: "entry-0".into(),
            kind: interaction::Kind::Read,
            page: String::new(),
            destination: None,
            prompt: format!("E - Inspect {name}"),
        });
        assert_eq!(
            world.target_description(),
            world.interaction.as_ref().unwrap().prompt
        );
        for [width, height] in [[960, 640], [640, 480]] {
            let vertices = text::overlay(&world, width, height, 0);
            // The literal bottom interaction cue is required even when the image's
            // duplicate above-footer panel is deliberately omitted.
            let footer: Vec<_> = vertices
                .iter()
                .filter(|v| v.color == [0.1, 0.95, 0.35])
                .collect();
            assert!(!footer.is_empty(), "literal interaction footer must remain");
            for v in footer {
                let y = (1.0 - v.position[1]) * height as f32 * 0.5;
                assert!(y >= height as f32 - 29.0 && y < height as f32 - 12.0);
            }
            let mut found = 0;
            for q in vertices.as_chunks::<6>().0 {
                if q[0].color != [0.6, 1.0, 0.7] {
                    continue;
                }
                // The same literal-text color is also used by the upper tree rows.
                let y_max = q
                    .iter()
                    .map(|v| (1.0 - v.position[1]) * height as f32 * 0.5)
                    .fold(0.0_f32, f32::max);
                if y_max < height as f32 * 0.3 {
                    continue;
                }
                found += 1;
                for v in q {
                    let y = (1.0 - v.position[1]) * height as f32 * 0.5;
                    assert!(
                        y > height as f32 * 0.7,
                        "local interaction prompt covers central content at {width}x{height}: y={y}"
                    );
                    assert!(
                        y < height as f32 - 96.0,
                        "prompt conflicts with bottom status"
                    );
                }
            }
            if name == "image.png" {
                assert_eq!(found, 0, "image must not duplicate its literal footer cue");
            } else {
                assert!(found > 0, "non-image prompt must remain visible");
            }
        }
    }
}
