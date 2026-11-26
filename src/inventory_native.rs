use super::*;
impl App {
    pub(super) fn inventory_frame(&self) -> hair::sidebar::Frame {
        let physical = self.renderer.as_ref().map(|r| r.size).unwrap_or([960, 640]);
        hair::sidebar::Frame::new(physical, self.ui_scale, false)
    }
    pub(super) fn inventory_pointer(&mut self, down: bool) {
        if down
            && self.focused
            && self.inventory_open
            && self.menu.is_none()
            && self.address.is_none()
            && !self.world.reading
            && !self.chooser_pending
            && !self.chooser_active
        {
            self.inventory_drag.press(
                self.cursor,
                self.inventory_frame(),
                self.inventory_selection,
                self.inventory
                    .items()
                    .get(self.inventory_selection)
                    .is_some(),
            );
        } else {
            self.inventory_drag.cancel();
        }
    }
    pub(super) fn pointer_moved(&mut self, point: Option<[f64; 2]>) {
        if point.is_none()
            && let Some(mode) = &mut self.place_mode {
            mode.cancel_pointer();
        }
        self.cursor = point;
        if self.focused
            && self.inventory_open
            && self.menu.is_none()
            && self.address.is_none()
            && !self.world.reading
            && !self.chooser_pending
            && !self.chooser_active
        {
            let delta =
                self.inventory_drag
                    .motion(point, self.inventory_frame(), self.inventory_selection);
            self.inventory_yaw = (self.inventory_yaw + delta).rem_euclid(std::f32::consts::TAU);
        } else {
            self.inventory_drag.cancel();
        }
    }
}
