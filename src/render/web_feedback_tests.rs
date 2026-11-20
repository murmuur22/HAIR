//! Prepared CPU bounds/static/escaping checks; not executed by the writer.
use super::*;
fn view(mode: Mode, seconds: f32) -> Snapshot {
    Snapshot::new(
        mode,
        "https://example.org/Case?q=A%2Fb#Tail",
        Some("refused\n\x1b[31m\u{202e}"),
        seconds,
        false,
        false,
        true,
        0,
        false,
    )
}
#[test]
fn vortex_geometry_is_finite_bounded_and_static_alternative_is_time_invariant() {
    for size in [[960, 640], [640, 480], [0, 0], [1, 1], [4096, 4096]] {
        for mode in [
            Mode::Hidden,
            Mode::Pending,
            Mode::FailureRed,
            Mode::FailureSettled,
            Mode::Cancelled,
        ] {
            for time in [0.0, 0.25, f32::NAN, f32::INFINITY, f32::MAX] {
                let mut vertices = Vec::new();
                view(mode, time).paint(&mut vertices, size, false);
                assert!(vertices.len() <= MAX_VERTICES);
                assert!(vertices.len().is_multiple_of(3));
                assert!(vertices.iter().all(|v| {
                    v.position
                        .iter()
                        .all(|x| x.is_finite() && (-1.0..=1.0).contains(x))
                        && v.color
                            .iter()
                            .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
                }));
                let mut a = view(mode, time);
                a.static_motion = true;
                let mut b = view(mode, 0.0);
                b.static_motion = true;
                let mut av = Vec::new();
                let mut bv = Vec::new();
                a.paint(&mut av, size, false);
                b.paint(&mut bv, size, false);
                assert_eq!(
                    bytemuck::cast_slice::<_, u8>(&av),
                    bytemuck::cast_slice::<_, u8>(&bv)
                );
            }
        }
    }
}
#[test]
fn vortex_animation_moves_without_changing_text_and_settled_failure_has_no_swirl() {
    let mut early = Vec::new();
    let mut later = Vec::new();
    view(Mode::Pending, 0.0).paint(&mut early, [640, 480], false);
    view(Mode::Pending, 0.5).paint(&mut later, [640, 480], false);
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&early),
        bytemuck::cast_slice::<_, u8>(&later)
    );
    early.clear();
    later.clear();
    view(Mode::FailureSettled, 0.0).paint(&mut early, [640, 480], false);
    view(Mode::FailureSettled, 1.0).paint(&mut later, [640, 480], false);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&early),
        bytemuck::cast_slice::<_, u8>(&later)
    );
}
#[test]
fn vortex_untrusted_text_work_is_capped_and_controls_are_escaped_not_interpreted() {
    let huge = "\u{202e}\n\x1b".repeat(100_000);
    let v = Snapshot::new(
        Mode::FailureSettled,
        &huge,
        Some(&huge),
        0.0,
        false,
        false,
        true,
        usize::MAX,
        false,
    );
    assert!(v.destination.len() <= MAX_DESTINATION_DISPLAY);
    assert!(v.reason.len() <= MAX_REASON_DISPLAY);
    assert!(v.destination.is_ascii() && !v.destination.chars().any(char::is_control));
    assert!(v.reason.contains("\\u{202e}") && v.reason.contains("\\x0a"));
    assert!(v.destination.ends_with("[excerpt]"));
    assert!(v.reason.ends_with("[excerpt]"));
    let mut vertices = Vec::new();
    v.paint(&mut vertices, [640, 480], false);
    assert!(vertices.len() <= MAX_VERTICES);
}
#[test]
fn vortex_exact_destination_tail_is_reachable_with_bounded_rows_at_both_sizes() {
    let destination = format!("https://example.org/{}?q=A%2Fb#ExactTail", "Ab".repeat(980));
    for size in [[960, 640], [640, 480]] {
        let mut v = Snapshot::new(
            Mode::Pending,
            &destination,
            None,
            0.0,
            false,
            false,
            true,
            0,
            false,
        );
        assert_eq!(v.destination, destination);
        let mut observed = String::new();
        for row in 0..150 {
            v.scroll = row;
            observed.push_str(&v.visible_rows(size).join(""));
        }
        assert!(observed.contains("#ExactTail"));
        assert_eq!(v.destination, destination);
    }
}
