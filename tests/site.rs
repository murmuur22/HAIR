use hair::{Action, World};
use std::path::PathBuf;
#[test]
fn all_pages_reachable_and_external_targets_never_change_current_content() {
    let p = std::env::temp_dir().join(format!("hair-targets-{}", std::process::id()));
    std::fs::create_dir(&p).unwrap();
    std::fs::write(p.join("index.md"), "[external](https://example.invalid)\n").unwrap();
    std::fs::write(p.join("unlinked.md"), "# Newly added, no graph file").unwrap();
    let mut w = World::load_site(&p).unwrap();
    w.act(Action::Navigate, [0.0, -3.0]);
    assert_eq!(w.current, "index.md", "HTTP must not become a local room");
    assert!(w.target_description().contains("not followed"));
    w.cycle_target();
    w.act(Action::Navigate, [0.0, -3.0]);
    assert_eq!(
        w.current, "unlinked.md",
        "explicit collection navigation reaches unlinked pages"
    );
    w.return_to_entry();
    assert_eq!(w.current, "index.md");
    std::fs::remove_dir_all(p).unwrap();
}

#[test]
fn link_destinations_remain_explicit_and_png_pixels_are_real() {
    use hair::content::{Destination, Site};
    let p = std::env::temp_dir().join(format!("hair-links-{}", std::process::id()));
    std::fs::create_dir(&p).unwrap();
    let source = "# Links\n![swatch](swatch.png)\n[web](https://example.invalid/page)\n[escape](../private.md)\n[missing](absent.md)\n";
    std::fs::write(p.join("index.md"), source).unwrap();
    let mut encoder = png::Encoder::new(std::fs::File::create(p.join("swatch.png")).unwrap(), 2, 1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[255, 0, 0, 0, 255, 0])
        .unwrap();
    let site = Site::load(&p).unwrap();
    assert_eq!(
        site.records["index.md"][0].destination,
        Destination::Image("swatch.png".into())
    );
    assert_eq!(
        site.records["index.md"][1].destination,
        Destination::External("https://example.invalid/page".into())
    );
    assert!(matches!(
        site.records["index.md"][2].destination,
        Destination::Invalid(_)
    ));
    assert!(matches!(
        site.records["index.md"][3].destination,
        Destination::Invalid(_)
    ));
    assert_eq!(
        site.images["swatch.png"].rgba,
        vec![255, 0, 0, 255, 0, 255, 0, 255]
    );
    assert_eq!(site.pages["index.md"], source);
    std::fs::remove_dir_all(p).unwrap();
}

#[test]
fn site_refuses_symlink_root_and_bounded_total_entries() {
    let p = std::env::temp_dir().join(format!("hair-budget-{}", std::process::id()));
    std::fs::create_dir(&p).unwrap();
    std::fs::write(p.join("index.md"), "# Budget").unwrap();
    let alias = p.with_extension("alias");
    std::os::unix::fs::symlink(&p, &alias).unwrap();
    let symlink_result = World::load_site(&alias);
    std::fs::remove_file(alias).unwrap();
    for n in 0..129 {
        std::fs::write(p.join(format!("unsupported-{n}.bin")), b"x").unwrap();
    }
    let budget_result = World::load_site(&p);
    std::fs::remove_dir_all(&p).unwrap();
    assert!(
        symlink_result.is_err(),
        "selected symlink root must be refused"
    );
    assert!(
        budget_result.is_err(),
        "unsupported entries must still count against traversal budget"
    );
}

fn fixture() -> PathBuf {
    let p = std::env::temp_dir().join(format!("hair-site-{}", std::process::id()));
    std::fs::create_dir_all(p.join("notes")).unwrap();
    std::fs::write(p.join("index.md"), "# Synthetic\n[Notes](notes/)\n").unwrap();
    std::fs::write(p.join("notes/one.md"), "# One\n[index](index.md)\n").unwrap();
    p
}
#[test]
fn selected_folder_derives_stable_graph_and_navigates_actual_text() {
    let root = fixture();
    let mut w = World::load_site(&root).unwrap();
    assert_eq!(w.current, "index.md");
    assert_eq!(
        w.graph
            .pages
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        vec!["index.md", "notes/one.md"]
    );
    assert_eq!(w.document().unwrap(), "# Synthetic\n[Notes](notes/)\n");
    w.act(Action::Navigate, [0.0, -3.0]);
    assert_eq!(w.current, "notes/one.md");
    assert!(w.document().unwrap().contains("# One"));
    assert_eq!(
        std::fs::read_to_string(root.join("index.md")).unwrap(),
        "# Synthetic\n[Notes](notes/)\n"
    );
    std::fs::remove_dir_all(root).unwrap();
}
