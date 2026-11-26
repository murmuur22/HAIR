use super::*;
impl App {
    pub(super) fn collection_ui_at(&mut self, now: Instant) -> Vec<hair::assets::Vertex> {
        if !self.focused
            || self.local.is_none()
            || self.world.reading
            || self.menu.is_some()
            || self.inventory_open
            || self.place_mode.is_some()
            || self.address.is_some()
            || self.chooser_pending
            || self.chooser_active
            || self.map_mode == hair::exploration::MapMode::Expanded
        {
            self.collection_feedback = None;
            return vec![];
        }
        let frame = self.sidebar_frame();
        if self
            .collection_feedback
            .as_ref()
            .is_some_and(|(start, effect)| {
                effect.frame != frame
                    || now.saturating_duration_since(*start).as_secs_f32()
                        >= hair::collection::SECONDS
            })
        {
            self.collection_feedback = None;
        }
        let mut out = hair::collection::badge(
            frame,
            self.inventory.items().len(),
            self.collection_feedback.is_some(),
        );
        if let Some((start, effect)) = &self.collection_feedback {
            out.extend(effect.paint(
                now.saturating_duration_since(*start).as_secs_f32(),
                self.static_web_feedback,
            ));
        }
        out
    }
}
