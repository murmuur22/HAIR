use hair::text;

#[test]
fn bottom_status_bitmap_glyph_has_readable_default_height_at_supported_sizes() {
    // TwoPixelBitmapGlyph14 proposal: the existing 5x7 H uses every raster row.
    // Two pixels per bitmap cell give a 14px ink height instead of today's 7px.
    // This named minimum is a measurable default-size proposal, not a claim of
    // universal accessibility, logical-point size, Retina comfort or pixel proof.
    const MIN_INK_HEIGHT_PX: f32 = 14.0;
    let mut failures = Vec::new();
    for (width, height) in [(960, 640), (640, 480)] {
        // Public function used directly by Renderer::draw_frame for status/help.
        // A single H avoids confusing line pitch or panel height with glyph size.
        let ui = text::pack_overlay("H", width, height);
        let ink: Vec<_> = ui.iter().filter(|v| v.color == [0.85; 3]).collect();
        assert!(!ink.is_empty(), "status glyph missing at {width}x{height}");
        let mut top = f32::INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for v in ink {
            assert!(v.position.iter().all(|p| p.is_finite()));
            assert!((-1.0..=1.0).contains(&v.position[0]));
            assert!((-1.0..=1.0).contains(&v.position[1]));
            let y = (1.0 - v.position[1]) * 0.5 * height as f32;
            top = top.min(y);
            bottom = bottom.max(y);
        }
        let actual = bottom - top;
        println!(
            "STATUS_GLYPH {width}x{height} inkHeightPx={actual} proposedMinimum={MIN_INK_HEIGHT_PX}"
        );
        if actual + 0.01 < MIN_INK_HEIGHT_PX {
            failures.push((width, height, actual));
        }
    }
    assert!(
        failures.is_empty(),
        "bottom status/help text is too small (TwoPixelBitmapGlyph14): {failures:?}"
    );
}
