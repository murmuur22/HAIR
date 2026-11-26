//! Pointer ownership for the session inventory preview, in the rendered UI frame.
use crate::sidebar::Frame;

#[derive(Default)]
pub struct Drag {
    active: Option<(Frame, usize, [f32; 2])>,
}
impl Drag {
    pub fn press(
        &mut self,
        point: Option<[f64; 2]>,
        frame: Frame,
        selection: usize,
        has_item: bool,
    ) {
        self.cancel();
        if has_item
            && let Some(point) = point.and_then(|p| frame.full_point(p))
            && crate::inventory_ui::preview_contains(frame.full, point)
        {
            self.active = Some((frame, selection, point));
        }
    }
    pub fn motion(&mut self, point: Option<[f64; 2]>, frame: Frame, selection: usize) -> f32 {
        let Some((start_frame, start_selection, previous)) = self.active else {
            return 0.0;
        };
        if frame != start_frame || selection != start_selection {
            self.cancel();
            return 0.0;
        }
        let Some(point) = point.and_then(|p| frame.full_point(p)) else {
            self.cancel();
            return 0.0;
        };
        self.active = Some((frame, selection, point));
        (point[0] - previous[0]) * 0.01
    }
    pub fn cancel(&mut self) {
        self.active = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> Frame {
        Frame::new([960, 640], 100, false)
    }
    #[test]
    fn dragging_the_preview_changes_only_yaw_by_pointer_displacement() {
        let mut drag = Drag::default();
        drag.press(Some([650.0, 250.0]), frame(), 2, true);
        assert!((drag.motion(Some([700.0, 300.0]), frame(), 2) - 0.5).abs() < 0.0001);
        assert!((drag.motion(Some([680.0, 300.0]), frame(), 2) + 0.2).abs() < 0.0001);
    }
    #[test]
    fn list_header_footer_empty_inventory_and_unclicked_motion_are_inert() {
        for point in [[60.0, 250.0], [650.0, 100.0], [650.0, 550.0]] {
            let mut drag = Drag::default();
            drag.press(Some(point), frame(), 0, true);
            assert_eq!(drag.motion(Some([700.0, 300.0]), frame(), 0), 0.0);
        }
        let mut drag = Drag::default();
        assert_eq!(drag.motion(Some([700.0, 300.0]), frame(), 0), 0.0);
        drag.press(Some([650.0, 250.0]), frame(), 0, false);
        assert_eq!(drag.motion(Some([700.0, 300.0]), frame(), 0), 0.0);
    }
    #[test]
    fn cancel_cursor_exit_and_selection_change_stop_the_gesture() {
        let mut drag = Drag::default();
        drag.press(Some([650.0, 250.0]), frame(), 0, true);
        drag.cancel();
        assert_eq!(drag.motion(Some([700.0, 300.0]), frame(), 0), 0.0);
        drag.press(Some([650.0, 250.0]), frame(), 0, true);
        assert_eq!(drag.motion(None, frame(), 0), 0.0);
        assert_eq!(drag.motion(Some([700.0, 300.0]), frame(), 0), 0.0);
        drag.press(Some([650.0, 250.0]), frame(), 0, true);
        assert_eq!(drag.motion(Some([700.0, 300.0]), frame(), 1), 0.0);
        assert_eq!(drag.motion(Some([710.0, 300.0]), frame(), 0), 0.0);
    }
    #[test]
    fn physical_pointer_uses_the_same_scaled_frame_as_the_inventory_mesh() {
        let f = Frame::new([1920, 1280], 200, false);
        assert_eq!(f.full, frame().full);
        let mut drag = Drag::default();
        drag.press(Some([1300.0, 500.0]), f, 0, true);
        assert!((drag.motion(Some([1400.0, 500.0]), f, 0) - 0.5).abs() < 0.0001);
        assert_eq!(drag.motion(Some([1400.0, 500.0]), frame(), 0), 0.0);
        assert_eq!(drag.motion(Some([1420.0, 500.0]), f, 0), 0.0);
    }
    #[test]
    fn invalid_coordinates_and_small_window_cannot_poison_rotation() {
        let mut drag = Drag::default();
        for point in [[f64::NAN, 250.0], [f64::INFINITY, 250.0], [-1.0, 250.0]] {
            drag.press(Some(point), frame(), 0, true);
            assert_eq!(drag.motion(Some([700.0, 300.0]), frame(), 0), 0.0);
        }
        let small = Frame::new([320, 240], 100, false);
        drag.press(Some([250.0, 150.0]), small, 0, true);
        assert_eq!(drag.motion(Some([260.0, 150.0]), small, 0), 0.0);
        drag.press(Some([650.0, 250.0]), frame(), 0, true);
        assert_eq!(drag.motion(Some([f64::NAN, 250.0]), frame(), 0), 0.0);
        assert_eq!(drag.motion(Some([700.0, 300.0]), frame(), 0), 0.0);
    }
}
