//! Shared physical/logical frame and scrollable immediate-directory row geometry.
use crate::directory_map_view::{Rect, contains};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub physical: [u32; 2],
    pub full: [u32; 2],
    pub content: [u32; 2],
    pub left: u32,
    pub sidebar: f32,
}
impl Frame {
    pub fn new(physical: [u32; 2], percent: u16, enabled: bool) -> Self {
        let physical = physical.map(|v| v.max(1));
        let full = crate::presentation::ui_viewport(physical, percent);
        let width = if enabled && full[0] >= 480 && full[1] >= 240 {
            (full[0] / 4).clamp(180, 280).min(full[0] - 320)
        } else {
            0
        };
        let left = (width as f64 * physical[0] as f64 / full[0].max(1) as f64).floor() as u32;
        Self {
            physical,
            full,
            content: [full[0] - width, full[1]],
            left,
            sidebar: width as f32,
        }
    }
    pub fn content_point(self, p: [f64; 2]) -> Option<[f32; 2]> {
        if !p.iter().all(|v| v.is_finite())
            || p[0] < f64::from(self.left)
            || p[0] >= f64::from(self.physical[0])
            || p[1] < 0.0
            || p[1] >= f64::from(self.physical[1])
        {
            return None;
        }
        Some([
            (p[0] as f32 - self.left as f32) * self.content[0] as f32
                / (self.physical[0] - self.left).max(1) as f32,
            p[1] as f32 * self.content[1] as f32 / self.physical[1].max(1) as f32,
        ])
    }
    pub fn full_point(self, p: [f64; 2]) -> Option<[f32; 2]> {
        if !p.iter().all(|v| v.is_finite())
            || p[0] < 0.0
            || p[1] < 0.0
            || p[0] >= self.physical[0] as f64
            || p[1] >= self.physical[1] as f64
        {
            return None;
        }
        Some([
            p[0] as f32 * self.full[0] as f32 / self.physical[0].max(1) as f32,
            p[1] as f32 * self.full[1] as f32 / self.physical[1].max(1) as f32,
        ])
    }
}
pub const ROW: f32 = 26.0;
pub struct List {
    pub rows: Vec<(usize, Rect)>,
    pub start: usize,
    pub max_start: usize,
}
impl List {
    pub fn new(frame: Frame, count: usize, offset: usize) -> Self {
        let slots = ((frame.full[1] as f32 - 132.0) / ROW).max(1.0) as usize;
        let max_start = count.saturating_sub(slots);
        let start = offset.min(max_start);
        let rows = if frame.left == 0 {
            vec![]
        } else {
            (start..count.min(start + slots))
                .enumerate()
                .map(|(row, i)| {
                    (
                        i,
                        [
                            8.0,
                            100.0 + row as f32 * ROW,
                            frame.sidebar - 8.0,
                            124.0 + row as f32 * ROW,
                        ],
                    )
                })
                .collect()
        };
        Self {
            rows,
            start,
            max_start,
        }
    }
    pub fn hit(&self, p: [f32; 2]) -> Option<usize> {
        self.rows
            .iter()
            .find(|(_, r)| contains(*r, p))
            .map(|(i, _)| *i)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sidebar_frame_reserves_space_and_off_is_exact_full_viewport() {
        for size in [[640, 480], [960, 640], [1280, 960], [1920, 1280]] {
            for scale in crate::presentation::STEPS {
                let off = Frame::new(size, scale, false);
                assert_eq!(off.left, 0);
                assert_eq!(off.sidebar, 0.0);
                assert_eq!(off.content, crate::presentation::ui_viewport(size, scale));
                let on = Frame::new(size, scale, true);
                assert!(on.left > 0);
                assert!(on.content[0] >= 320);
                assert_eq!(on.content[1], off.content[1]);
                assert!(on.content_point([on.left as f64 - 0.1, 50.0]).is_none());
                let center = [(on.left + size[0]) as f64 * 0.5, size[1] as f64 * 0.5];
                let p = on.content_point(center).unwrap();
                assert!((p[0] - on.content[0] as f32 * 0.5).abs() < 0.01);
                assert_eq!(off.content_point([-1.0, 0.0]), None);
            }
        }
    }
    #[test]
    fn sidebar_rows_scroll_to_every_immediate_entry_with_shared_hits() {
        let frame = Frame::new([960, 640], 100, true);
        let mut seen = std::collections::BTreeSet::new();
        for offset in 0..85 {
            let list = List::new(frame, 85, offset);
            for (i, r) in list.rows.iter() {
                assert_eq!(
                    list.hit([(r[0] + r[2]) * 0.5, (r[1] + r[3]) * 0.5]),
                    Some(*i)
                );
                assert!(r[3] <= frame.full[1] as f32 - 32.0);
                seen.insert(*i);
            }
        }
        assert_eq!(seen.len(), 85);
        assert!(
            List::new(Frame::new([960, 640], 100, false), 85, 0)
                .rows
                .is_empty()
        );
    }
}
