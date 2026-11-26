//! Native flat-overlay toolbox painting; geometry is also used by App hit testing.
use crate::{assets::Vertex, text::Painter};
/// Compact Positioning chrome. Full diagnostics and properties remain available
/// by explicit disclosure; the world below y=116 is unobscured by this panel.
pub fn positioning(size: [u32; 2], hint: &str, controls: &[([f32; 4], &str)]) -> Vec<Vertex> {
    let [w, h] = size.map(|v| v as f32);
    let mut p = Painter { vertices: vec![], width: w, height: h };
    p.rect(0., 70., w, 46., [0.025, 0.045, 0.035]);
    p.literal_text(8., 78., hint, 1., [0.8, 0.9, 0.82]);
    for (r, label) in controls {
        p.rect(r[0], r[1], r[2]-r[0], r[3]-r[1], [0.09, 0.16, 0.12]);
        p.literal_text(r[0]+6., r[1]+3., label, 1., [0.8, 0.9, 0.82]);
    }
    p.vertices
}
/// Reserved live-preview rectangle above a compact, hit-tested property panel.
pub fn editor(size: [u32; 2], lines: &[String], controls: &[([f32; 4], &str)]) -> Vec<Vertex> {
    let [w, h] = size.map(|v| v as f32);
    let x = (w - 316.).max(0.);
    let mut p = Painter { vertices: vec![], width: w, height: h };
    p.rect(x, 70., w - x, h - 174., [0.025, 0.045, 0.035]);
    p.rect(x + 8., 88., 296., 120., [0.012, 0.022, 0.018]);
    // Keep complete bounded diagnostics beside the preview, not truncated into
    // five rows or underneath property buttons / the authoritative footer.
    let detail_x = (w - 632.).max(0.);
    p.rect(detail_x, 70., 308., h - 174., [0.025, 0.045, 0.035]);
    let rows = ((h - 184.) / 10.).max(0.) as usize;
    for (i, line) in lines.iter().take(rows).enumerate() {
        p.literal_text(detail_x + 8., 80. + i as f32 * 10., &line.chars().take(49).collect::<String>(), 1., [0.8, 0.9, 0.82]);
    }
    for (r, label) in controls {
        p.rect(r[0], r[1], r[2] - r[0], r[3] - r[1], [0.09, 0.16, 0.12]);
        p.literal_text(r[0] + 6., r[1] + 3., label, 1., [0.8, 0.9, 0.82]);
    }
    p.vertices
}
pub fn paint(
    size: [u32; 2],
    lines: &[String],
    controls: &[([f32; 4], &str)],
    palette: Option<&str>,
) -> Vec<Vertex> {
    let [w, h] = size.map(|v| v.max(1) as f32);
    let x = (w - 316.0).max(0.0);
    let mut p = Painter {
        vertices: vec![],
        width: w,
        height: h,
    };
    let bottom = controls
        .last()
        .map_or(412.0, |(r, _)| (r[3] + 8.0).max(412.0));
    p.rect(x, 70.0, w - x, bottom - 70.0, [0.025, 0.045, 0.035]);
    // Catalog identity disclosure wraps to at most eleven short rows. The
    // ordinary five-line file toolbox keeps its historical metrics unchanged.
    let pitch = if lines.len() > 5 { 10.0 } else { 21.0 };
    for (i, line) in lines.iter().take(11).enumerate() {
        p.literal_text(
            x + 8.0,
            80.0 + i as f32 * pitch,
            &line.chars().take(49).collect::<String>(),
            1.0,
            [0.8, 0.9, 0.82],
        );
    }
    for (r, label) in controls.iter().take(36) {
        p.rect(r[0], r[1], r[2] - r[0], r[3] - r[1], [0.09, 0.16, 0.12]);
        let columns = ((r[2] - r[0] - 12.0) / 6.0).max(0.0) as usize;
        p.literal_text(
            r[0] + 6.0,
            r[1] + 7.0,
            &label.chars().take(columns).collect::<String>(),
            1.0,
            [0.8, 0.9, 0.82],
        );
    }
    if let Some(text) = palette {
        p.rect(0.0, h - 70.0, w, 70.0, [0.025, 0.045, 0.035]);
        for (i, chunk) in text
            .chars()
            .take(1024)
            .collect::<Vec<_>>()
            .chunks(((w - 16.0) / 6.0).max(1.0) as usize)
            .take(4)
            .enumerate()
        {
            p.literal_text(
                8.0,
                h - 62.0 + i as f32 * 14.0,
                &chunk.iter().collect::<String>(),
                1.0,
                [0.8, 0.9, 0.82],
            );
        }
    }
    p.vertices
}
