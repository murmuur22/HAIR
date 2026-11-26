#[test]
#[ignore = "child-only abrupt relocation termination / cold reopen fixture"]
fn slice5a_process_child() {
    let root = PathBuf::from(std::env::var_os("HAIR_SLICE5A_ROOT").unwrap());
    assert!(root.starts_with(std::env::temp_dir()));
    let e = Explorer::new(&root).unwrap();
    let action = std::env::var("HAIR_SLICE5A_ACTION").unwrap();
    let layout = read_layout(&e).unwrap();
    assert_eq!(layout.decorations.len(), 1);
    let d = &layout.decorations[0];
    if action == "read-old" || action == "read-new" {
        assert_eq!(d.at, if action == "read-old" { [-2., 0.] } else { [2., 0.] });
        assert_eq!(layout.incomplete_records, usize::from(action == "read-old"));
        return;
    }
    assert_eq!(layout.incomplete_records, 0);
    let stage: u8 = action.parse().unwrap(); assert!([1, 2].contains(&stage));
    let mut proposed = d.clone(); proposed.at = [2., 0.];
    let _ = mac::relocate_decoration(&e, d, &proposed, &mut |_| Ok(()), |at| {
        if at == stage { unsafe { libc::_exit(73) }; }
        Ok(())
    });
    panic!("child did not reach requested crash boundary");
}
#[test]
fn slice5a_actual_process_termination_and_cold_reopen_preserve_activation_boundary() {
    for stage in [1, 2] {
        let (f, e, d, proposed) = slice5a_saved();
        let before = slice4c1_record_snapshot(&f.0);
        let run = |action: &str| std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "placement_store::tests::slice5a_process_child", "--ignored", "--nocapture"])
            .env("HAIR_SLICE5A_ROOT", &f.0).env("HAIR_SLICE5A_ACTION", action).output().unwrap();
        let output = run(&stage.to_string());
        assert_eq!(output.status.code(), Some(73), "{}", String::from_utf8_lossy(&output.stderr));
        let snapshot = slice5a_snapshot(&f.0);
        let output = run(if stage == 1 { "read-old" } else { "read-new" });
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(slice5a_snapshot(&f.0), snapshot, "cold reopen must not repair or delete pending evidence");
        let layout = read_layout(&e).unwrap();
        if stage == 1 {
            assert_eq!(layout.decorations, vec![d]); assert_eq!(layout.incomplete_records, 1);
            assert_eq!(slice4c1_record_snapshot(&f.0), before);
        } else { assert_eq!(layout.decorations, vec![proposed]); assert_eq!(layout.incomplete_records, 0); }
    }
}
