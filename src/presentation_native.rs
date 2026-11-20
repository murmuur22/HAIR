//! App adapter for the paused presentation menu. Opened only by world Escape/Q
//! after address and reader contexts have consumed their own Escape.
use super::App;
use hair::{
    Result,
    presentation::{Item, Menu, View},
};
use winit::keyboard::KeyCode;

impl App {
    pub(super) fn open_menu(&mut self, menu: Menu, code: KeyCode) {
        // Movement, modifiers and capture are released; media/reader/history are not touched.
        self.release_navigation();
        self.world.explicit_target = None;
        self.menu_held.clear();
        self.menu_held.insert(code);
        self.menu = Some(menu);
    }
    fn close_menu(&mut self) {
        // Keys still physically held stay suppressed (menu_held) until released.
        self.menu = None;
        self.release_navigation();
    }
    /// Release bookkeeping for keys whose press was consumed by the menu.
    fn release_latch(&mut self, code: KeyCode) {
        match code {
            KeyCode::KeyE => self.interact_held = false,
            KeyCode::Tab => self.tab_held = false,
            KeyCode::KeyM => self.tree_held = false,
            KeyCode::KeyL => self.address_held = false,
            KeyCode::F8 => self.web_details_held = false,
            KeyCode::Escape => self.address_escape_held = false,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                self.shifts[usize::from(code == KeyCode::ShiftRight)] = false;
            }
            KeyCode::KeyR | KeyCode::F5 => {
                if let Some(web) = &mut self.web {
                    if code == KeyCode::KeyR {
                        web.back_held = false;
                    } else {
                        web.refresh_held = false;
                    }
                }
            }
            _ => {}
        }
        self.local_held.remove(&code);
    }
    /// Some while the menu (or a key held across it) owns this event.
    pub(super) fn menu_key(
        &mut self,
        code: KeyCode,
        down: bool,
        repeat: bool,
    ) -> Option<Result<bool>> {
        if self.menu.is_none() {
            if !self.menu_held.contains(&code) {
                return None;
            }
            if !down {
                self.menu_held.remove(&code);
                self.release_latch(code);
            }
            return Some(Ok(false));
        }
        if !down {
            self.menu_held.remove(&code);
            self.release_latch(code);
            return Some(Ok(false));
        }
        let fresh = self.menu_held.insert(code);
        if matches!(code, KeyCode::ShiftLeft | KeyCode::ShiftRight) {
            self.shifts[usize::from(code == KeyCode::ShiftRight)] = self.focused;
            return Some(Ok(false));
        }
        if !self.focused || repeat || !fresh {
            return Some(Ok(false));
        }
        let shift = self.shifts.iter().any(|held| *held);
        let menu = self.menu.as_mut().unwrap();
        match code {
            KeyCode::ArrowUp => menu.step(true),
            KeyCode::ArrowDown => menu.step(false),
            KeyCode::Tab => menu.step(shift),
            KeyCode::KeyQ => *menu = Menu::quit_confirm(),
            KeyCode::ArrowLeft | KeyCode::ArrowRight if menu.view == View::Settings => {
                let larger = code == KeyCode::ArrowRight;
                self.set_ui_scale(hair::presentation::step_scale(self.ui_scale, larger));
            }
            KeyCode::Escape => match menu.view {
                View::Settings => {
                    *menu = Menu {
                        view: View::Pause,
                        focus: 1,
                    }
                }
                View::Pause => self.close_menu(),
            },
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let item = menu.focused();
                return Some(Ok(self.activate_menu(item)));
            }
            _ => {}
        }
        Some(Ok(false))
    }
    /// Shared by keyboard Enter and pointer clicks; true requests exit.
    pub(super) fn activate_menu(&mut self, item: Item) -> bool {
        let Some(menu) = &mut self.menu else {
            return false;
        };
        match item {
            Item::Resume => self.close_menu(),
            Item::Settings => *menu = Menu::settings(),
            Item::Back => {
                *menu = Menu {
                    view: View::Pause,
                    focus: 1,
                }
            }
            Item::Quit => {
                self.release();
                return true;
            }
            Item::Smaller => {
                self.set_ui_scale(hair::presentation::step_scale(self.ui_scale, false))
            }
            Item::Larger => self.set_ui_scale(hair::presentation::step_scale(self.ui_scale, true)),
            Item::Reset => self.set_ui_scale(100),
        }
        false
    }
    /// Left click at a physical window position; true requests exit.
    pub(super) fn menu_click(&mut self, point: [f64; 2]) -> bool {
        let Some(menu) = self.menu else {
            return false;
        };
        if !self.focused {
            return false;
        }
        let physical = self.renderer.as_ref().map_or([960, 640], |r| r.size);
        let Some(item) = hair::presentation::hit(&menu, physical, self.ui_scale, point) else {
            return false;
        };
        if let Some(menu) = &mut self.menu {
            menu.focus = menu.items().iter().position(|i| *i == item).unwrap_or(0);
        }
        self.activate_menu(item)
    }
    /// Logical screen-space viewport used by HUD layout and reader metrics.
    pub(super) fn ui_viewport(&self) -> [u32; 2] {
        let physical = self.renderer.as_ref().map_or([960, 640], |r| r.size);
        hair::presentation::ui_viewport(physical, self.ui_scale)
    }
    /// Applies immediately: renderer layout and any open local reader's scroll bound.
    pub(super) fn set_ui_scale(&mut self, percent: u16) {
        self.ui_scale = percent.clamp(100, 200);
        if let Some(renderer) = &mut self.renderer {
            renderer.ui_scale = self.ui_scale;
        }
        if self.world.reading && self.local.is_some() {
            let [width, height] = self.ui_viewport();
            self.scroll = self.scroll.min(hair::text::local_reader_scroll_max(
                &self.world,
                width,
                height,
            ));
        }
    }
}
