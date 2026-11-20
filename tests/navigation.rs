use hair::World;
use std::path::Path;
#[test]
fn navigation_optional_file_absent_only_fallback_and_safe_read() {
    use std::os::unix::fs::symlink;
    let p = std::env::temp_dir().join(format!("hair-tree-load-{}", std::process::id()));
    std::fs::create_dir(&p).unwrap();
    std::fs::write(
        p.join("index.md"),
        "# Home\n[Next](next.md) [External](https://example.invalid)",
    )
    .unwrap();
    std::fs::write(p.join("next.md"), "# Next").unwrap();
    assert_eq!(
        World::load_site(&p).unwrap().navigation.title,
        "Pages (flat)"
    );
    let nav = p.join("site-navigation.toml");
    std::fs::write(&nav,"version=1\nname='Host'\n[[pages]]\nid='index.md'\nlabel='Home'\n[[pages]]\nid='next.md'\nlabel='Next'\nparent='index.md'\n").unwrap();
    assert_eq!(
        World::load_site(&p).unwrap().navigation.rows[1].depth,
        1,
        "declared hierarchy must load"
    );
    for text in [
        "broken".into(),
        "x".repeat(16385),
        String::from_utf8(vec![0]).unwrap(),
    ] {
        std::fs::write(&nav, text).unwrap();
        assert!(World::load_site(&p).is_err());
    }
    std::fs::remove_file(&nav).unwrap();
    symlink(p.join("index.md"), &nav).unwrap();
    assert!(World::load_site(&p).is_err());
    std::fs::remove_file(&nav).unwrap();
    std::fs::create_dir(&nav).unwrap();
    assert!(World::load_site(&p).is_err());
    std::fs::remove_dir(&nav).unwrap();
    let status = std::process::Command::new("mkfifo")
        .arg(&nav)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(World::load_site(&p).is_err());
    std::fs::remove_file(&nav).unwrap();
    std::fs::remove_dir_all(&p).unwrap();
}

#[test]
fn navigation_flat_state_tracks_real_page_not_link_graph() {
    let mut w = World::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(w.navigation.title, "Pages (flat)");
    assert_eq!(w.navigation.rows.len(), 2);
    assert_eq!(w.navigation.current_index(&w.current), Some(0));
    w.act(hair::Action::Navigate, [0.0, -3.0]);
    assert_eq!(w.navigation.current_index(&w.current), Some(1));
    assert!(w.navigation.rows.iter().all(|r| r.depth == 0));
    assert!(!w.tree_collapsed);
}
#[test]
fn navigation_declared_tree_validates_host_intent_and_bounded_work() {
    use hair::navigation::Navigation;
    let ids = vec!["a.md".into(), "b.md".into(), "c.md".into()];
    let text = "version=1\nname='Greenleaf'\n[[pages]]\nid='a.md'\nlabel='Home'\n[[pages]]\nid='b.md'\nlabel='Books'\nparent='a.md'\n[[pages]]\nid='c.md'\nlabel='Visit'\nparent='a.md'\n";
    let nav = Navigation::parse(text, &ids).unwrap();
    assert_eq!(nav.title, "Host: Greenleaf");
    assert_eq!(
        nav.rows.iter().map(|r| r.depth).collect::<Vec<_>>(),
        [0, 1, 1]
    );
    for bad in [
        text.replace("version=1", "version=2"),
        text.replace("id='b.md'", "id='missing.md'"),
        text.replace("id='b.md'", "id='c.md'"),
        text.replace("parent='a.md'", "parent='b.md'"),
        text.replace("label='Home'", "label='Home'\nparent='b.md'"),
        text.replace("label='Home'", "label='Home'\nparent='absent'"),
        text.replace("label='Home'", "label='Home'\nparent='c.md'\nparent='b.md'"),
        text.replace("label='Home'", "label=''"),
        text.replace("name='Greenleaf'", "name=''"),
        text.replace("label='Home'", &format!("label='{}'", "x".repeat(81))),
        text.replace("label='Home'", "label='Home'\nroom='not-in-this-slice'"),
        "x".repeat(16385),
        format!(
            "{text}{}",
            "[[pages]]\nid='a.md'\nlabel='duplicate'\n".repeat(33)
        ),
    ] {
        assert!(Navigation::parse(&bad, &ids).is_err(), "accepted {bad}");
    }
    let many: Vec<String> = (0..32).map(|i| format!("p{i}.md")).collect();
    let chain = |n: usize| {
        let mut s = String::from("version=1\nname='Depth'\n");
        for i in 0..n {
            s += &format!("[[pages]]\nid='p{i}.md'\nlabel='P'\n");
            if i > 0 {
                s += &format!("parent='p{}.md'\n", i - 1);
            }
        }
        s
    };
    assert!(Navigation::parse(&chain(9), &many[..9]).is_ok()); // root0 + depth8
    assert!(Navigation::parse(&chain(10), &many[..10]).is_err());
    assert!(
        Navigation::parse(text, &many).is_err(),
        "declared outline must cover all known pages"
    );
}
