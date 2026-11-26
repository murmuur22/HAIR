use super::*;
use hair::directory_map::{Confirmation, Target};
impl App {
    /// Shared inert query for visual hover, hand cursor and click routing.
    pub(super) fn directory_map_action(
        &self,
        point: Option<[f64; 2]>,
    ) -> Option<hair::directory_map_view::Action> {
        if !self.focused
            || self.input.captured
            || self.map_mode != hair::exploration::MapMode::Expanded
            || self.local.is_none()
            || self.menu.is_some()
            || self.world.reading
            || self.address.is_some()
            || self.chooser_pending
            || self.chooser_active
        {
            return None;
        }
        let point = point?;
        let physical = self.renderer.as_ref().map_or([960, 640], |r| r.size);
        let [w, h] = self.ui_viewport();
        let p = [
            point[0] as f32 * w as f32 / physical[0] as f32,
            point[1] as f32 * h as f32 / physical[1] as f32,
        ];
        let view = hair::directory_map_view::View::new(
            &self.directory_map,
            w,
            h,
            true,
            self.route_pan,
            self.map_history_offset,
        );
        hair::directory_map_view::action_at(&view, w, h, self.map_selection.is_some(), p)
    }
    pub(super) fn directory_map_pointer(&mut self, point: Option<[f64; 2]>, down: bool) {
        use hair::directory_map_view::Action;
        let fresh = down && !self.map_mouse_held;
        self.map_mouse_held = down;
        if !fresh {
            return;
        }
        match self.directory_map_action(point) {
            Some(Action::Cancel) => self.map_selection = None,
            Some(Action::Travel) if Instant::now() >= self.map_confirm_ready_at => {
                let _ = self.directory_map_confirm();
            }
            Some(Action::Older) => self.directory_history_scroll(true),
            Some(Action::Newer) => self.directory_history_scroll(false),
            Some(Action::Destination(target)) => {
                let _ = self.directory_map_select(target);
            }
            _ => {}
        }
    }
    pub(super) fn directory_history_scroll(&mut self, older: bool) {
        let max = self.directory_map.history.len().saturating_sub(1);
        self.map_history_offset = if older {
            (self.map_history_offset + 1).min(max)
        } else {
            self.map_history_offset.saturating_sub(1)
        };
    }
    pub(super) fn directory_map_select(&mut self, target: Target) -> Result<()> {
        if !self.focused
            || self.map_mode != hair::exploration::MapMode::Expanded
            || self.local.is_none()
            || self.world.reading
            || self.menu.is_some()
            || self.map_selection.is_some()
        {
            return Err("Map selection unavailable".into());
        }
        let (id, name, page) = match target {
            Target::Node(i) => {
                let n = self
                    .directory_map
                    .nodes
                    .get(i)
                    .ok_or("Room no longer mapped")?;
                (n.id, n.name.clone(), n.page)
            }
            Target::History(i) => {
                let n = self
                    .directory_map
                    .history
                    .get(i)
                    .ok_or("Visit no longer retained")?;
                (n.id, n.name.clone(), n.page)
            }
        };
        self.release();
        self.map_confirm_travel = false;
        self.map_confirm_ready_at = Instant::now() + Duration::from_millis(350);
        self.map_selection = Some(Confirmation {
            target,
            id,
            name,
            page,
            error: None,
        });
        Ok(())
    }
    pub(super) fn directory_map_confirm(&mut self) -> Result<()> {
        if !self.focused || self.map_mode != hair::exploration::MapMode::Expanded {
            return Err("Map confirmation unavailable".into());
        }
        let selection = self
            .map_selection
            .clone()
            .ok_or("Select a destination before confirming")?;
        match self.local_map_travel(&selection) {
            Ok(()) => {
                self.map_selection = None;
                self.map_mode = if self.minimap_enabled {
                    hair::exploration::MapMode::Compact
                } else {
                    hair::exploration::MapMode::Hidden
                };
                self.release_navigation();
                Ok(())
            }
            Err(e) => {
                if let Some(selection) = &mut self.map_selection {
                    selection.error = Some(e.clone());
                }
                self.local_status = format!("Refused map travel: {e}");
                Err(e)
            }
        }
    }
}
