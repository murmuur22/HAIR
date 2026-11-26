//! Explicit owner-requested live browsing, never part of offline regression.
use super::*;
use hair::web::WindowAction;

fn tap(a: &mut App, key: KeyCode) {
    a.key(key, true, false).unwrap();
    a.key(key, false, false).unwrap();
}
fn settle(a: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while a.web.as_ref().unwrap().pending.is_some() {
        a.poll_web().unwrap();
        assert!(
            Instant::now() < deadline,
            "bounded live browsing completion"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn shot(a: &mut App, output: &Path, label: &str, full: bool) {
    shot_at(a, output, label, full, [1280, 960]);
}
fn shot_at(a: &mut App, output: &Path, label: &str, full: bool, size: [u32; 2]) {
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let mut renderer = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        size,
        &a.themes[a.theme_index].vertices_for(&a.world),
    ))
    .unwrap();
    renderer
        .set_theme_appearance(&a.themes[a.theme_index])
        .unwrap();
    a.renderer = Some(renderer);
    a.ui_scale = 100;
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [0.6, -1.5],
        yaw: std::f32::consts::PI,
        pitch: -0.2,
    };
    a.world.explicit_target = None;
    a.world.interaction = None;
    if full {
        tap(a, KeyCode::KeyM);
    }
    a.update_hud();
    let dir = output.join(label);
    std::fs::create_dir(&dir).unwrap();
    let target = resources::smoke_output(&a.root, &dir).unwrap();
    a.renderer
        .as_ref()
        .unwrap()
        .capture(&target, "office.png", &a.pose, &a.world)
        .unwrap();
    a.renderer.take().unwrap().wait().unwrap();
    if full {
        tap(a, KeyCode::KeyM);
    }
}

#[test]
#[ignore = "explicit owner-approved live HTTPS tour + Metal; HAIR_LEARNX_TOUR_DIR"]
fn live_learnx_browsing_and_back_populates_route_map() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output =
        PathBuf::from(std::env::var_os("HAIR_LEARNX_TOUR_DIR").expect("explicit output directory"));
    assert!(output.is_dir() && !output.join("TOUR.txt").exists());
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let world = World::load(&root).unwrap();
    let mut a = App::new(root, vec![theme], 0, world, false);
    a.web = Some(web_native::Runtime::new(vec![], hair::web::fetch));
    let homepage = "https://learnxinyminutes.com/";
    a.request_web(homepage, web_native::Mode::Initial).unwrap();
    settle(&mut a);
    assert_eq!(
        a.world.current, homepage,
        "live initial homepage must commit"
    );
    let home_layout = a.exploration.rooms[0].layout.clone();
    let mut visited = std::collections::HashSet::new();
    let mut journal = vec![format!("INITIAL {homepage}")];
    let mut successes = 0;
    let mut inside_captured = false;
    for window in 0..3 {
        let targets: Vec<_> = a
            .web
            .as_ref()
            .unwrap()
            .doors
            .iter()
            .filter_map(|door| {
                let address = Address::parse(&door.resource).ok()?;
                let path = address.url.path().trim_matches('/');
                (address.url.host_str() == Some("learnxinyminutes.com")
                    && !path.is_empty()
                    && !path.contains('/')
                    && address.url.fragment().is_none())
                .then(|| door.resource.clone())
            })
            .take(10)
            .collect();
        println!("TOUR_CANDIDATES window={window} targets={targets:?}");
        let home_room = a.world.pocket.as_ref().unwrap().room;
        let mut in_window = 0;
        for target in targets {
            if visited.contains(&target) {
                continue;
            }
            a.request_web(&target, web_native::Mode::Follow).unwrap();
            settle(&mut a);
            if a.world.current != target {
                let line = format!("REFUSED {target}: {}", a.web.as_ref().unwrap().status);
                println!("{line}");
                journal.push(line);
                continue;
            }
            visited.insert(target.clone());
            successes += 1;
            in_window += 1;
            let title = a.web.as_ref().unwrap().page.as_ref().unwrap().title.clone();
            let line = format!("VISIT {successes} {target} {title:?}");
            println!("{line}");
            journal.push(line);
            if successes >= 7 && !inside_captured {
                shot(&mut a, &output, "inside-visited-page", true);
                inside_captured = true;
            }
            // Actual R key restores the held snapshot, not another homepage request.
            tap(&mut a, KeyCode::KeyR);
            assert_eq!(a.world.current, homepage);
            assert_eq!(a.world.pocket.as_ref().unwrap().room, home_room);
            journal.push(format!("BACK homepage window={window}"));
            if in_window == 3 {
                break;
            }
        }
        if window < 2 {
            let next = a
                .web
                .as_ref()
                .unwrap()
                .window_doors
                .iter()
                .find(|d| matches!(d.action, WindowAction::Next(_)))
                .expect("homepage continuation")
                .action
                .clone();
            a.request_window(next).unwrap();
            settle(&mut a);
            assert_eq!(a.world.current, homepage);
            assert_eq!(a.world.pocket.as_ref().unwrap().room.unwrap().0, window + 1);
            journal.push(format!("NEXT held homepage window={}", window + 1));
        }
    }
    assert!(
        successes >= 6,
        "need actual committed visits, not a fabricated populated graph: {successes}"
    );
    if !inside_captured {
        let target = visited.iter().next().unwrap().clone();
        a.request_web(&target, web_native::Mode::Follow).unwrap();
        settle(&mut a);
        assert_eq!(a.world.current, target);
        shot(&mut a, &output, "inside-visited-page", true);
        tap(&mut a, KeyCode::KeyR);
        journal.push(format!("REVISIT {target} then BACK"));
    }
    while a.world.pocket.as_ref().unwrap().room.unwrap().0 > 0 {
        let previous = a
            .web
            .as_ref()
            .unwrap()
            .window_doors
            .iter()
            .find(|d| matches!(d.action, WindowAction::Previous(_)))
            .expect("held previous window")
            .action
            .clone();
        a.request_window(previous).unwrap();
        settle(&mut a);
        journal.push("PREVIOUS held homepage window".into());
    }
    assert_eq!(a.exploration.current, Some(0));
    assert_eq!(
        a.exploration.rooms[0].layout, home_layout,
        "held homepage geometry remains unchanged"
    );
    assert_eq!(
        a.exploration.rooms.len(),
        successes + 3,
        "one node per actually visited page/window"
    );
    shot(&mut a, &output, "back-at-homepage", true);
    shot_at(
        &mut a,
        &output,
        "back-at-homepage-minimum",
        true,
        [640, 480],
    );
    shot(&mut a, &output, "populated-minimap", false);
    journal.push(format!("FINAL committed_documents={successes} visited_rooms={} observed_edges={} recent_entries={}",a.exploration.rooms.len(),a.exploration.edges.len(),a.exploration.recent.len()));
    for (i, room) in a.exploration.rooms.iter().enumerate() {
        journal.push(format!(
            "ROOM {} cell={:?} title={:?}",
            i + 1,
            room.cell,
            room.title
        ));
    }
    println!("{}", journal.join("\n"));
    std::fs::write(output.join("TOUR.txt"), journal.join("\n")).unwrap();
}
