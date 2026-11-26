//! Wrapped, scrollable Storage panel using the existing bitmap painter.
use crate::{assets::Vertex, presentation::{self, Item, Menu}};
/// Geometry shared by the actual panel and layout assertions.
pub fn text_frame(physical: [u32; 2], percent: u16) -> ([u32; 2], usize, usize) {
    let viewport = presentation::ui_viewport(physical, percent);
    (viewport, (viewport[0].saturating_sub(32) / 6).max(1) as usize,
        (viewport[1].saturating_sub(200) / 12).max(1) as usize)
}
pub fn overlay(physical: [u32; 2], percent: u16, lines: &[String], scroll: usize, menu: Option<&Menu>) -> Vec<Vertex> {
    let ([w, h], columns, rows) = text_frame(physical, percent);
    if w < 320 || h < 240 { return Vec::new(); }
    let mut p = crate::text::Painter { vertices: Vec::new(), width: w as f32, height: h as f32 };
    p.rect(0., 0., w as f32, h as f32, [0.015, 0.04, 0.025]);
    p.literal_text(16., 12., "STORAGE / DOCUMENTS", 2., [0.6, 1., 0.7]);
    let wrapped = lines.iter().flat_map(|line| {
        let chars = line.chars().collect::<Vec<_>>();
        chars.chunks(columns).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>()
    }).collect::<Vec<_>>();
    let offset = scroll.min(wrapped.len().saturating_sub(rows));
    for (i, line) in wrapped.iter().skip(offset).take(rows).enumerate() {
        p.literal_text(16., 46. + i as f32 * 12., line, 1., [0.85, 0.9, 0.85]);
    }
    p.literal_text(16., h.saturating_sub(148) as f32, "PGUP/PGDN: ALL PATH / HELP / ERRORS", 1., [0.6, 1., 0.7]);
    if let Some(menu) = menu {
        for (item, [x, y, bw, bh]) in presentation::layout(menu, percent, physical) {
            p.rect(x, y, bw, bh, if item == menu.focused() { presentation::FOCUS_COLOR } else { presentation::BUTTON_COLOR });
            let label = match item { Item::StorageItemLimit => "EDIT PER-FILE LIMIT (BYTES)", Item::StorageDiskLimit => "EDIT DISK CAP (BYTES)", Item::StorageChoose => "CHOOSE CACHE FOLDER...", _ => "BACK" };
            p.literal_text(x + 8., y + 7., label, 1., [0.85, 0.9, 0.85]);
        }
    }
    p.vertices
}
