// Public behavioral tracer: the existing real writer exits at prepared relocation.
#[test]
fn slice8a_public_inspection_finds_actual_interrupted_relocation() {
    let (f, e, d, _) = slice5a_saved();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "placement_store::tests::slice5a_process_child", "--ignored", "--nocapture"])
        .env("HAIR_SLICE5A_ROOT", &f.0).env("HAIR_SLICE5A_ACTION", "1").output().unwrap();
    assert_eq!(output.status.code(), Some(73), "writer must actually reach prepared boundary: {}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let before = slice5a_snapshot(&f.0);
    let inspection = inspect_placements(&e).unwrap();
    assert_eq!(inspection.layout(), &read_layout(&e).unwrap());
    assert_eq!(inspection.layout().decorations, vec![d.clone()]);
    assert_eq!(inspection.attempts().len(), 1, "public inventory must identify the concrete retained attempt, not just count it");
    let attempt = &inspection.attempts()[0];
    assert_eq!(attempt.identity(), format!("dc-{}", d.placement_id));
    assert_eq!(attempt.kind(), AttemptKind::DecorationRelocation);
    assert_eq!(attempt.state(), AttemptState::DefinitelyInactive);
    assert_eq!(attempt.active_predecessor(), Some(true));
    assert_eq!(slice5a_snapshot(&f.0), before);
    let exact = slice8a_snapshot(&f.0);
    validate_inspection(&e, &inspection).unwrap();
    slice8a_cold(&f.0, &inspection);
    assert_eq!(slice8a_snapshot(&f.0), exact);
}
