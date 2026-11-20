// A glanceable excerpt is not a dense miniature document. Six source pixels per
// font cell doubles/triples the failed small-world glyph footprint without moving
// the shared host envelope. Full contents remain available through E.
#[test]
fn literal_excerpt_first_glyph_uses_six_pixel_cells_for_world_readability() {
    let texture = hair::text::literal_excerpt_texture("# aB9\nx\\x09\\\\\n\\u{E9}\n", false);
    let glyph = hair::text::object_label("#", [0.0; 3], 1);
    let mut mask = [[false; 5]; 7];
    for quad in glyph.as_chunks::<6>().0 {
        let x = quad
            .iter()
            .map(|v| v.position[0])
            .fold(f32::INFINITY, f32::min);
        let y = quad
            .iter()
            .map(|v| v.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let col = ((x + 0.036) / 0.012).round() as usize;
        let row = (-y / 0.012).round() as usize;
        assert!(row < 7 && col < 5);
        mask[row][col] = true;
    }
    for (row, cells) in mask.iter().enumerate() {
        for (col, ink) in cells.iter().enumerate() {
            for dy in 0..6 {
                for dx in 0..6 {
                    let x = 8 + col * 6 + dx;
                    let y = 32 + row * 6 + dy;
                    let at = ((y * texture.width as usize) + x) * 4;
                    let expected = if *ink {
                        [20, 42, 25, 255]
                    } else {
                        [242, 246, 235, 255]
                    };
                    assert_eq!(
                        &texture.rgba[at..at + 4],
                        &expected,
                        "WORLD_TEXT_READABILITY: six-pixel glyph cell {col},{row} subpixel {dx},{dy}"
                    );
                }
            }
        }
    }
}
