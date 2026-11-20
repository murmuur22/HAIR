use super::*;
#[test]
#[ignore = "finite Metal required by actual T adapter; no native window"]
fn whole_pack_feedback_tracks_actual_e_and_t_adapter_without_losing_pack() {
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut a = app();
    let world = World::load_site(&root.join("examples/GreenleafReadingRoom-appearance")).unwrap();
    let scene = &world.spaces.as_ref().unwrap().spaces[0].scene;
    let mut cache = Default::default();
    a.themes = ["office", "archive"]
        .iter()
        .map(|n| {
            hair::theme::Theme::load_for_scene(
                &root.join(format!("packs/{n}.toml")),
                &mut cache,
                scene,
            )
            .unwrap()
        })
        .collect();
    a.world = world;
    let mut renderer = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [640, 480],
        &a.themes[0].vertices_for(&a.world),
    ))
    .unwrap();
    renderer.set_materials(&a.themes[0].textures).unwrap();
    a.renderer = Some(renderer);
    for (door, space) in [
        ("door-notice", "notice-alcove"),
        ("door-workshop", "workshop-room"),
        ("door-reading", "reading-room"),
    ] {
        aim(&mut a, door);
        press(&mut a, KeyCode::KeyE);
        assert_eq!(a.world.current_space, space);
        assert_eq!(
            a.themes[a.theme_index].host,
            a.world.spaces.as_ref().unwrap().space(space).unwrap().scene
        );
        let before = a.pose;
        let previous = a.theme_index;
        press(&mut a, KeyCode::KeyT);
        assert_ne!(a.theme_index, previous);
        assert_eq!(a.pose.position, before.position);
        let text = a.themes[a.theme_index].feedback(&a.world);
        assert!(
            text.contains("HERE 4/4 HOST 12/12 UNMATCHED 0/0 FALLBACK 0/0"),
            "{text}"
        );
    }
}

fn app() -> App {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let world = World::load_site(&root.join("examples/GreenleafReadingRoom-spaces")).unwrap();
    let host = &world
        .spaces
        .as_ref()
        .unwrap()
        .space(&world.current_space)
        .unwrap()
        .scene;
    let mut cache = hair::theme::AssetCache::default();
    let themes = ["office", "archive"]
        .iter()
        .map(|n| {
            hair::theme::Theme::load_for_scene(
                &root.join(format!("packs/{n}.toml")),
                &mut cache,
                host,
            )
            .unwrap()
        })
        .collect();
    App::new(root.into(), themes, 0, world, false)
}
fn press(a: &mut App, k: KeyCode) {
    a.key(k, false, false).unwrap();
    a.key(k, true, false).unwrap();
}
fn aim(a: &mut App, anchor: &str) {
    let d = a.themes[a.theme_index]
        .host
        .anchors
        .iter()
        .find(|d| d.id == anchor)
        .unwrap();
    a.pose = Pose {
        position: [d.at[0], d.at[2] + 1.1],
        yaw: 0.0,
        pitch: -0.1,
    };
}
#[test]
fn complete_cycle_reader_return_failure_and_entry_reset() {
    let mut a = app();
    for (door, space, page) in [
        ("door-notice", "notice-alcove", "noticeboard.md"),
        ("door-workshop", "workshop-room", "workshop-notes.md"),
        ("door-reading", "reading-room", "index.md"),
    ] {
        aim(&mut a, door);
        press(&mut a, KeyCode::KeyE);
        assert_eq!(a.world.current_space, space);
        assert_eq!(a.world.current, page);
        a.pose = Pose {
            position: [0.0, 1.35],
            yaw: 0.0,
            pitch: -0.5,
        };
        press(&mut a, KeyCode::KeyE);
        assert!(a.world.reading);
        assert_eq!(a.world.current, page);
        press(&mut a, KeyCode::Tab);
        assert!(a.world.reader_link.is_some());
        press(&mut a, KeyCode::KeyM);
        let collapse = a.world.tree_collapsed;
        let before = (
            a.world.current.clone(),
            a.world.current_space.clone(),
            a.pose.position,
            a.pose.yaw,
            a.pose.pitch,
            a.world.reader_link.clone(),
            a.scroll,
        );
        let id = if let hair::content::Destination::Page(id) =
            &a.world.reader_link.as_ref().unwrap().link.destination
        {
            id.clone()
        } else {
            panic!()
        };
        let removed = a.world.site.as_mut().unwrap().pages.remove(&id).unwrap();
        press(&mut a, KeyCode::KeyE);
        assert_eq!(
            before,
            (
                a.world.current.clone(),
                a.world.current_space.clone(),
                a.pose.position,
                a.pose.yaw,
                a.pose.pitch,
                a.world.reader_link.clone(),
                a.scroll
            )
        );
        assert!(a.world.reading);
        assert_eq!(a.world.tree_collapsed, collapse);
        a.world.site.as_mut().unwrap().pages.insert(id, removed);
        press(&mut a, KeyCode::Escape);
    }
    aim(&mut a, "door-workshop");
    press(&mut a, KeyCode::KeyE);
    assert_eq!(a.world.current_space, "workshop-room");
    press(&mut a, KeyCode::KeyR);
    assert_eq!(a.world.current_space, "reading-room");
    assert_eq!(a.themes[0].host.name, "Reading room");
    assert_eq!(a.pose.position, [-1.25, -1.9]);
}
#[test]
fn two_document_markers_in_one_space_keep_exact_page_and_tree_identity() {
    let mut a = app();
    let site = a.world.site.as_mut().unwrap();
    site.pages.insert(
        "appendix.md".into(),
        "# Appendix\\nExplicit second bound page".into(),
    );
    site.records.insert("appendix.md".into(), vec![]);
    let g = a.world.spaces.as_mut().unwrap();
    let s = &mut g.spaces[0];
    s.pages.push("appendix.md".into());
    s.documents.push(hair::spaces::Document {
        anchor: "appendix-station".into(),
        page: "appendix.md".into(),
    });
    let mut anchor = s.scene.anchors[0].clone();
    anchor.id = "appendix-station".into();
    anchor.at[0] = -2.0;
    s.scene.anchors.push(anchor);
    g.validate(a.world.site.as_ref().unwrap()).unwrap();
    let host = g.spaces[0].scene.clone();
    a.themes = a
        .themes
        .iter()
        .map(|t| {
            hair::theme::Theme::load_for_scene(
                &t.source,
                &mut hair::theme::AssetCache::default(),
                &host,
            )
            .unwrap()
        })
        .collect();
    a.world.navigation = hair::navigation::Navigation::flat(&[
        "index.md".into(),
        "appendix.md".into(),
        "noticeboard.md".into(),
        "workshop-notes.md".into(),
    ]);
    a.pose = Pose {
        position: [-2.0, 1.35],
        yaw: 0.0,
        pitch: -0.5,
    };
    press(&mut a, KeyCode::KeyE);
    assert_eq!(a.world.current, "appendix.md");
    assert!(a.world.document().unwrap().contains("Appendix"));
    assert_eq!(a.world.current_space, "reading-room");
    assert_eq!(a.world.navigation.current_index(&a.world.current), Some(1));
    press(&mut a, KeyCode::Escape);
    a.pose.position = [0.0, 1.35];
    press(&mut a, KeyCode::KeyE);
    assert_eq!(a.world.current, "index.md");
    assert_eq!(a.world.navigation.current_index(&a.world.current), Some(0));
}
#[test]
fn door_failure_focus_self_link_and_return_precedence() {
    let mut a = app();
    aim(&mut a, "door-notice");
    press(&mut a, KeyCode::Tab);
    let target = a.world.explicit_target.clone();
    let before = (
        a.world.current.clone(),
        a.world.current_space.clone(),
        a.pose.position,
        a.pose.yaw,
        a.pose.pitch,
        a.world.reading,
    );
    a.world.spaces.as_mut().unwrap().spaces[0].doors[0].arrival = "absent".into();
    press(&mut a, KeyCode::KeyE);
    assert_eq!(target, a.world.explicit_target);
    assert_eq!(
        before,
        (
            a.world.current.clone(),
            a.world.current_space.clone(),
            a.pose.position,
            a.pose.yaw,
            a.pose.pitch,
            a.world.reading
        )
    );
    a.world.spaces.as_mut().unwrap().spaces[0].doors[0].arrival = "from-reading".into();
    a.focus_changed(false);
    assert!(a.world.explicit_target.is_none());
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert_eq!(a.world.current_space, "reading-room");
    a.focus_changed(true);
    press(&mut a, KeyCode::KeyE);
    assert_eq!(a.world.current_space, "notice-alcove");
    a.pose = Pose {
        position: [0.0, 1.35],
        yaw: 0.0,
        pitch: -0.5,
    };
    press(&mut a, KeyCode::KeyE);
    a.world
        .site
        .as_mut()
        .unwrap()
        .records
        .get_mut("noticeboard.md")
        .unwrap()
        .insert(
            0,
            hair::content::Link {
                label: "Self".into(),
                raw: "noticeboard.md".into(),
                destination: hair::content::Destination::Page("noticeboard.md".into()),
            },
        );
    press(&mut a, KeyCode::Tab);
    press(&mut a, KeyCode::KeyE);
    assert!(!a.world.reading);
    assert_eq!(a.world.current, "noticeboard.md");
    assert_eq!(a.pose.position, [-1.25, -2.9]);
    aim(&mut a, "door-reading");
    press(&mut a, KeyCode::KeyE);
    assert_eq!(a.pose.position, [-1.25, -1.9]);
    aim(&mut a, "door-workshop");
    press(&mut a, KeyCode::KeyE);
    aim(&mut a, "door-reading");
    press(&mut a, KeyCode::KeyE);
    assert_eq!(
        a.pose.position,
        [1.25, -1.9],
        "connecting arrival, not remembered pose or entry reset"
    );
}
#[test]
#[ignore = "serialized optimized Metal explicit three-space input proof"]
fn rendered_three_space_cycle_reskins_and_reader_links() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = PathBuf::from(std::env::var_os("HAIR_SPACES_CAPTURE_DIR").expect("capture root"));
    let mut a = app();
    let pack = PathBuf::from(std::env::var_os("HAIR_SPACES_PACK").expect("CC0 user pack"));
    a.themes.push(
        hair::theme::Theme::load_for_scene(
            &pack,
            &mut hair::theme::AssetCache::default(),
            &a.themes[0].host,
        )
        .unwrap(),
    );
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    let mut r = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [960, 640],
        &a.themes[0].vertices_for(&a.world),
    ))
    .unwrap();
    r.set_materials(&a.themes[0].textures).unwrap();
    a.renderer = Some(r);
    let mut serial = 0;
    for (door, space, page) in [
        ("door-notice", "notice-alcove", "noticeboard.md"),
        ("door-workshop", "workshop-room", "workshop-notes.md"),
        ("door-reading", "reading-room", "index.md"),
    ] {
        aim(&mut a, door);
        hair::interaction::refresh(&a.themes[a.theme_index].host, &a.pose, &mut a.world);
        assert!(a.world.interaction.as_ref().unwrap().prompt.contains(space));
        // Actual E adapter, then declared arrival orientation; never a renamed current room.
        press(&mut a, KeyCode::KeyE);
        assert_eq!(a.world.current_space, space);
        assert_eq!(a.world.current, page);
        assert_eq!(
            a.world.navigation.current_index(page),
            Some(if page == "index.md" {
                0
            } else if page == "noticeboard.md" {
                1
            } else {
                2
            })
        );
        for _ in 0..3 {
            let dir = base.join(format!("{serial:02}-{space}-{}", a.theme_index));
            std::fs::create_dir(&dir).unwrap();
            serial += 1;
            let out = resources::smoke_output(root, &dir).unwrap();
            capture_overview(&mut a, &out);
            let before = (
                a.world.current.clone(),
                a.world.current_space.clone(),
                a.pose.position,
                a.pose.yaw,
                a.pose.pitch,
                a.world.reading,
                a.world.reader_link.clone(),
                a.world.explicit_target.clone(),
                a.world.tree_collapsed,
            );
            let host = a.themes[a.theme_index].host.clone();
            let old = a.theme_index;
            press(&mut a, KeyCode::KeyT);
            assert_ne!(old, a.theme_index);
            assert_eq!(host, a.themes[a.theme_index].host);
            assert_eq!(
                before,
                (
                    a.world.current.clone(),
                    a.world.current_space.clone(),
                    a.pose.position,
                    a.pose.yaw,
                    a.pose.pitch,
                    a.world.reading,
                    a.world.reader_link.clone(),
                    a.world.explicit_target.clone(),
                    a.world.tree_collapsed
                )
            );
            for x in -32..=32 {
                for z in -32..=32 {
                    let p = [x as f32 * 0.125, z as f32 * 0.125];
                    assert_eq!(a.themes[old].blocked(p), a.themes[a.theme_index].blocked(p));
                }
            }
        }
        a.pose = Pose {
            position: [0.0, 1.35],
            yaw: 0.0,
            pitch: -0.5,
        };
        press(&mut a, KeyCode::Tab);
        let selected = a.world.explicit_target.clone();
        assert!(selected.is_some());
        hair::interaction::refresh(&a.themes[a.theme_index].host, &a.pose, &mut a.world);
        let dir = base.join(format!("{serial:02}-bound-read"));
        std::fs::create_dir(&dir).unwrap();
        serial += 1;
        let out = resources::smoke_output(root, &dir).unwrap();
        crate::tree_tests::capture(&a, &out, "office.png", [960, 640]);
        press(&mut a, KeyCode::KeyE);
        assert!(a.world.reading);
        assert_eq!(a.world.current, page);
        press(&mut a, KeyCode::Tab);
        let link = a.world.reader_link.clone();
        press(&mut a, KeyCode::KeyM);
        assert!(a.world.tree_collapsed);
        for _ in 0..3 {
            let pose = (a.pose.position, a.pose.yaw, a.pose.pitch);
            let host = a.themes[a.theme_index].host.clone();
            let page = a.world.current.clone();
            let content = a.world.document().unwrap().to_string();
            let routes = format!("{:?}", a.world.spaces);
            press(&mut a, KeyCode::KeyT);
            assert_eq!(pose, (a.pose.position, a.pose.yaw, a.pose.pitch));
            assert_eq!(host, a.themes[a.theme_index].host);
            assert_eq!(page, a.world.current);
            assert_eq!(content, a.world.document().unwrap());
            assert_eq!(routes, format!("{:?}", a.world.spaces));
            assert_eq!(link, a.world.reader_link);
            assert!(a.world.reading);
            assert_eq!(selected, a.world.explicit_target);
            assert!(a.world.tree_collapsed);
        }
        crate::tree_tests::capture(&a, &out, "reader-welcome.png", [960, 640]);
        press(&mut a, KeyCode::KeyM);
        press(&mut a, KeyCode::Escape);
    }
    // Reader link maps exact page ownership to default arrival. Same-page link retains reset intent.
    a.pose = Pose {
        position: [0.0, 1.35],
        yaw: 0.0,
        pitch: -0.5,
    };
    press(&mut a, KeyCode::KeyE);
    press(&mut a, KeyCode::Tab);
    press(&mut a, KeyCode::KeyE);
    assert_eq!(a.world.current_space, "notice-alcove");
    assert_eq!(a.pose.position, [-1.25, -2.9]);
    let dir = base.join("final-reader-follow");
    std::fs::create_dir(&dir).unwrap();
    capture_overview(&mut a, &resources::smoke_output(root, &dir).unwrap());
    a.renderer.as_ref().unwrap().wait().unwrap();
    println!(
        "SPACES_METAL_INPUT_PASS three distinct hosts, E bound read/door/return, actual page tree raster, M collapse, reader link, three vocabularies in each space, pose+host+collision+bindings+selection retained; captures={}",
        serial + 4
    );
}
fn capture_overview(a: &mut App, out: &resources::SmokeOutput) {
    let arrival = a.pose;
    let room = a.themes[a.theme_index].host.room;
    // Arrival is asserted and T-tested in place. Turn/walk to a full-room diagnostic
    // view for the unchanged generic20-color capture guard; sparse arrival18 refuses.
    a.pose = Pose {
        position: [0.0, room[3] - 0.5],
        yaw: 0.0,
        pitch: -0.18,
    };
    println!(
        "SPACE_CAPTURE_VIEW arrival={arrival:?} overview={:?} host={:?}",
        a.pose, a.themes[a.theme_index].host
    );
    crate::tree_tests::capture(a, out, "office.png", [960, 640]);
    a.pose = arrival;
}
#[test]
fn explicit_door_commits_destination_host_and_connecting_arrival() {
    let mut a = app();
    aim(&mut a, "door-notice");
    press(&mut a, KeyCode::KeyE);
    assert_eq!(a.world.current_space, "notice-alcove");
    assert_eq!(a.world.current, "noticeboard.md");
    assert_eq!(a.themes[0].host.name, "Noticeboard collection alcove");
    assert_eq!(a.pose.position, [-1.25, -2.9]);
    assert!((a.pose.yaw - std::f32::consts::PI).abs() < 0.001);
    assert!(!a.themes[0].blocked(a.pose.position));
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert_eq!(a.world.current_space, "notice-alcove", "held E inert");
}
