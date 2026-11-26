use super::*;
impl App {
    pub(super) fn sidebar_frame(&self) -> hair::sidebar::Frame {
        hair::sidebar::Frame::new(
            self.renderer.as_ref().map_or([960, 640], |r| r.size),
            self.ui_scale,
            self.local.is_some()
                && !self.world.tree_collapsed
                && self.map_mode != hair::exploration::MapMode::Expanded,
        )
    }
    fn sidebar_available(&self) -> bool {
        self.focused
            && !self.input.captured
            && !self.world.tree_collapsed
            && self.local.is_some()
            && !self.inventory_open
            && self.place_mode.is_none()
            && self.placement_action.is_none()
            && self.menu.is_none()
            && self.address.is_none()
            && !self.chooser_pending
            && !self.chooser_active
            && self.map_mode != hair::exploration::MapMode::Expanded
    }
    pub(super) fn sidebar_hit(&self, point: Option<[f64; 2]>) -> Option<usize> {
        if !self.sidebar_available() {
            return None;
        }
        let frame = self.sidebar_frame();
        let snapshot = self.sidebar_snapshot.as_ref()?;
        let i = hair::sidebar::List::new(frame, snapshot.entries.len(), self.sidebar_offset)
            .hit(frame.full_point(point?)?)?;
        (snapshot.entries[i].kind == hair::local_explorer::Kind::File).then_some(i)
    }
    pub(super) fn sidebar_pointer(&mut self, pressed: bool, point: Option<[f64; 2]>) -> bool {
        let frame = self.sidebar_frame();
        let inside = point
            .and_then(|p| frame.full_point(p))
            .is_some_and(|p| p[0] < frame.sidebar);
        if pressed {
            self.sidebar_press = None;
            if !self.sidebar_available() || !inside {
                return false;
            }
            if let Some(i) = self.sidebar_hit(point) {
                self.sidebar_press =
                    Some((i, self.sidebar_snapshot.as_ref().unwrap().clone(), frame));
            }
            return true;
        }
        if let Some((i, snapshot, old_frame)) = self.sidebar_press.take() {
            if frame == old_frame
                && self.sidebar_hit(point) == Some(i)
                && self
                    .sidebar_snapshot
                    .as_ref()
                    .is_some_and(|now| std::sync::Arc::ptr_eq(now, &snapshot))
                && let Err(e) = self.sidebar_open_entry(i)
            {
                self.local_status = format!("File not opened: {e}");
            }
            return true;
        }
        false
    }
    pub(super) fn sidebar_scroll(&mut self, delta: MouseScrollDelta) -> bool {
        let frame = self.sidebar_frame();
        let inside = self
            .cursor
            .and_then(|p| frame.full_point(p))
            .is_some_and(|p| p[0] < frame.sidebar);
        if !self.sidebar_available() || !inside {
            self.sidebar_wheel = 0.0;
            return false;
        }
        self.sidebar_press = None;
        let rows = match delta {
            MouseScrollDelta::LineDelta(_, y) => -f64::from(y) * 3.0,
            MouseScrollDelta::PixelDelta(p) => {
                -p.y * f64::from(frame.full[1])
                    / f64::from(frame.physical[1])
                    / f64::from(hair::sidebar::ROW)
            }
        };
        if rows.is_finite() {
            self.sidebar_wheel = (self.sidebar_wheel + rows).clamp(-4096.0, 4096.0);
            let whole = self.sidebar_wheel.trunc() as i64;
            self.sidebar_wheel -= whole as f64;
            let count = self
                .sidebar_snapshot
                .as_ref()
                .map_or(0, |s| s.entries.len());
            let max = hair::sidebar::List::new(frame, count, self.sidebar_offset).max_start;
            self.sidebar_offset =
                (self.sidebar_offset as i64 + whole).clamp(0, max as i64) as usize;
        }
        true
    }
    pub(super) fn sidebar_open_entry(&mut self, index: usize) -> Result<()> {
        if !self.focused
            || self.place_mode.is_some()
            || self.placement_action.is_some()
            || self.input.captured
            || self.world.tree_collapsed
            || self.menu.is_some()
            || self.address.is_some()
            || self.chooser_pending
            || self.chooser_active
            || self.map_mode == hair::exploration::MapMode::Expanded
        {
            return Err("Sidebar unavailable".into());
        }
        self.local_entry_reader(index)
    }
}
