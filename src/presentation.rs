//! Presentation pause menu and screen-space UI scale.
//! World geometry/camera and world-space labels are never scaled here.

/// Requested screen-space scale steps. 100% is the historical physical-pixel HUD.
pub const STEPS: [u16; 5] = [100, 125, 150, 175, 200];
/// Smallest logical HUD viewport accepted when enlarging. Every screen-space
/// layout (reader 2x text, address entry, outline) is unchanged at or above it.
pub const MIN_LOGICAL: [u32; 2] = [800, 500];
pub fn step_scale(percent: u16, larger: bool) -> u16 {
    let percent = percent.clamp(STEPS[0], STEPS[STEPS.len() - 1]);
    if larger {
        STEPS.into_iter().find(|s| *s > percent).unwrap_or(percent)
    } else {
        STEPS
            .into_iter()
            .rev()
            .find(|s| *s < percent)
            .unwrap_or(percent)
    }
}
/// Applied factor: requested, capped so the logical viewport never drops below
/// MIN_LOGICAL. Windows already below it keep the exact 100% baseline.
pub fn applied(percent: u16, physical: [u32; 2]) -> f32 {
    let requested = f32::from(percent.clamp(STEPS[0], STEPS[STEPS.len() - 1])) / 100.0;
    let room = (physical[0] as f32 / MIN_LOGICAL[0] as f32)
        .min(physical[1] as f32 / MIN_LOGICAL[1] as f32)
        .max(1.0);
    requested.min(room)
}
/// Logical screen-space viewport; normalized overlay vertices are unchanged,
/// so every logical pixel covers `applied` physical pixels.
pub fn ui_viewport(physical: [u32; 2], percent: u16) -> [u32; 2] {
    let scale = applied(percent, physical);
    if scale <= 1.0 {
        return physical;
    }
    // Float division may land a hair below the floor; never under-size the HUD.
    [0, 1].map(|i| ((physical[i] as f32 / scale).floor() as u32).clamp(MIN_LOGICAL[i], physical[i]))
}
pub const BUTTON_COLOR: [f32; 3] = [0.06, 0.16, 0.1];
pub const FOCUS_COLOR: [f32; 3] = [0.2, 0.55, 0.32];
const TEXT: [f32; 3] = [0.85, 0.85, 0.85];
const ACCENT: [f32; 3] = [0.6, 1.0, 0.7];
const PANEL: [f32; 3] = [0.015, 0.04, 0.025];
const GLYPH: f32 = 2.0;
const LINE: f32 = 20.0;
const BUTTON: f32 = 28.0;
const GAP: f32 = 8.0;
/// Requested and (when smaller) actually applied percentage, one line each.
pub fn scale_label(percent: u16, physical: [u32; 2]) -> String {
    let percent = percent.clamp(STEPS[0], STEPS[STEPS.len() - 1]);
    let applied = (applied(percent, physical) * 100.0).round() as u16;
    if applied < percent {
        format!("UI SCALE {percent}%\nWINDOW LIMIT: {applied}% APPLIED")
    } else {
        format!("UI SCALE {percent}%")
    }
}
fn lines(menu: &Menu, percent: u16, physical: [u32; 2]) -> Vec<(String, [f32; 3])> {
    match menu.view {
        View::Pause => vec![
            ("PAUSED".into(), ACCENT),
            ("UP/DOWN/TAB + ENTER | ESC RESUMES".into(), TEXT),
        ],
        View::Appearance => vec![
            ("APPEARANCE".into(), ACCENT),
            ("CATEGORY OVERRIDE: THEME FIRST".into(), TEXT),
            ("OFF: DIRECTORY FIRST | ESC BACK".into(), TEXT),
        ],
        View::Settings => vec![
            ("SETTINGS".into(), ACCENT),
            ("UP/DOWN/TAB + ENTER | ESC BACK".into(), TEXT),
        ],
        View::Storage => vec![("STORAGE SETTINGS".into(), ACCENT)],
        View::PlayerSettings => {
            let mut out = vec![("PLAYER SETTINGS".into(), ACCENT)];
            out.extend(
                scale_label(percent, physical)
                    .lines()
                    .map(|l| (l.into(), ACCENT)),
            );
            out.push(("SCALES HUD, READER, ADDRESS, MENUS".into(), TEXT));
            out.push(("FOG: WORLD ONLY | ESC BACK".into(), TEXT));
            out.push(("LEFT/RIGHT: FOCUSED CONTROL".into(), TEXT));
            out
        }
    }
}
fn label(item: Item) -> &'static str {
    match item {
        Item::Resume => "RESUME",
        Item::Settings => "SETTINGS",
        Item::PlayerSettings => "PLAYER SETTINGS...",
        Item::Storage => "STORAGE SETTINGS...",
        Item::StorageItemLimit => "EDIT PER-FILE LIMIT",
        Item::StorageDiskLimit => "EDIT DISK CAP",
        Item::StorageChoose => "CHOOSE CACHE FOLDER...",
        Item::FogEnabled => "TOGGLE FOG",
        Item::FogDistance => "FOG DISTANCE",
        Item::FogTint => "FOG TINT",
        Item::Quit => "QUIT HAIR",
        Item::Smaller => "SMALLER  -",
        Item::Larger => "LARGER  +",
        Item::Reset => "RESET PLAYER DEFAULTS",
        Item::Back => "BACK",
        Item::Appearance => "APPEARANCE...",
        Item::Theme => "NEXT THEME PACK",
        Item::Override(category) => category.label(),
        Item::FileTree => "TOGGLE FILE TREE",
    }
}
/// Panel [x, y, w, h], first text baseline and button rects, all logical pixels.
fn frame(menu: &Menu, percent: u16, physical: [u32; 2]) -> ([f32; 4], Vec<(Item, [f32; 4])>) {
    let [lw, lh] = ui_viewport(physical, percent).map(|v| v as f32);
    let text_rows = lines(menu, percent, physical).len() as f32;
    let items = menu.items();
    if menu.view == View::Storage {
        let buttons = items.iter().enumerate().map(|(i, item)| (*item, [16., lh - 132. + i as f32 * 30., lw - 32., 26.])).collect();
        return ([0., 0., lw, lh], buttons);
    }
    let w = (lw - 32.0).clamp(1.0, 480.0);
    let step = ((lh - 24. - text_rows * LINE - GAP) / items.len() as f32).min(BUTTON + GAP);
    let button = (step - GAP).max(20.);
    let h = 16.0 + text_rows * LINE + GAP + items.len() as f32 * step + 8.0;
    let x = ((lw - w) * 0.5).max(0.0);
    let y = ((lh - h) * 0.5).max(0.0);
    let first = y + 16.0 + text_rows * LINE + GAP;
    let buttons = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            (
                *item,
                [
                    x + 16.0,
                    first + i as f32 * step,
                    w - 32.0,
                    button,
                ],
            )
        })
        .collect();
    ([x, y, w, h], buttons)
}
/// Button rectangles [x, y, w, h] in logical HUD pixels, in item order.
pub fn layout(menu: &Menu, percent: u16, physical: [u32; 2]) -> Vec<(Item, [f32; 4])> {
    frame(menu, percent, physical).1
}
/// Physical pointer (window pixels) to the button rendered under it. Uses the
/// same per-axis logical/physical ratio as normalized overlay vertices.
pub fn hit(menu: &Menu, physical: [u32; 2], percent: u16, point: [f64; 2]) -> Option<Item> {
    if physical.contains(&0) || !point.iter().all(|v| v.is_finite()) {
        return None;
    }
    let logical = ui_viewport(physical, percent);
    let [x, y] = [0, 1].map(|i| (point[i] * logical[i] as f64 / physical[i] as f64) as f32);
    layout(menu, percent, physical)
        .into_iter()
        .find(|(_, [rx, ry, rw, rh])| x >= *rx && x < rx + rw && y >= *ry && y < ry + rh)
        .map(|(item, _)| item)
}
pub fn overlay_with_tree(
    menu: &Menu,
    percent: u16,
    physical: [u32; 2],
    visible: bool,
) -> Vec<crate::assets::Vertex> {
    overlay_state(menu, percent, physical, visible)
}
#[cfg(test)]
#[test]
fn settings_shows_file_tree_state_without_changing_other_menu_geometry() {
    let menu = Menu {
        view: View::PlayerSettings,
        focus: 0,
    };
    let shown = overlay_with_tree(&menu, 100, [640, 480], true);
    let hidden = overlay_with_tree(&menu, 100, [640, 480], false);
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&shown),
        bytemuck::cast_slice::<_, u8>(&hidden)
    );
    let pause = Menu::pause();
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&overlay_with_tree(&pause, 100, [640, 480], true)),
        bytemuck::cast_slice::<_, u8>(&overlay_with_tree(&pause, 100, [640, 480], false))
    );
}

/// Opaque panel in the existing bitmap style, laid out in the logical viewport.
pub fn overlay(menu: &Menu, percent: u16, physical: [u32; 2]) -> Vec<crate::assets::Vertex> {
    overlay_state(menu, percent, physical, false)
}
fn overlay_state(
    menu: &Menu,
    percent: u16,
    physical: [u32; 2],
    visible: bool,
) -> Vec<crate::assets::Vertex> {
    overlay_with_appearance(
        menu,
        percent,
        physical,
        visible,
        &crate::appearance::Policy::default(),
        "",
    )
}
pub fn overlay_with_appearance(
    menu: &Menu,
    percent: u16,
    physical: [u32; 2],
    visible: bool,
    policy: &crate::appearance::Policy,
    theme: &str,
) -> Vec<crate::assets::Vertex> {
    overlay_with_player(
        menu,
        percent,
        physical,
        visible,
        policy,
        theme,
        crate::fog::Fog::default(),
    )
}
pub fn overlay_with_player(
    menu: &Menu,
    percent: u16,
    physical: [u32; 2],
    visible: bool,
    policy: &crate::appearance::Policy,
    theme: &str,
    fog: crate::fog::Fog,
) -> Vec<crate::assets::Vertex> {
    if physical.contains(&0) {
        return Vec::new();
    }
    let [lw, lh] = ui_viewport(physical, percent);
    let ([x, y, w, h], buttons) = frame(menu, percent, physical);
    let mut p = crate::text::Painter {
        vertices: Vec::new(),
        width: lw as f32,
        height: lh as f32,
    };
    let columns = ((w - 32.0) / (6.0 * GLYPH)).max(1.0) as usize;
    let fit = |s: &str| s.chars().take(columns).collect::<String>();
    p.rect(x, y, w, h, PANEL);
    for (i, (line, color)) in lines(menu, percent, physical).iter().enumerate() {
        p.literal_text(
            x + 16.0,
            y + 16.0 + i as f32 * LINE,
            &fit(line),
            GLYPH,
            *color,
        );
    }
    let focus = menu.focused();
    for (item, [bx, by, bw, bh]) in &buttons {
        let color = if *item == focus {
            FOCUS_COLOR
        } else {
            BUTTON_COLOR
        };
        p.rect(*bx, *by, *bw, *bh, color);
    }
    for (item, [bx, by, _, _]) in &buttons {
        let marker = if *item == focus { "> " } else { "  " };
        let dynamic = match item {
            Item::FogEnabled => format!("FOG: {}", if fog.enabled() { "ON" } else { "OFF" }),
            Item::FogDistance => fog.distance_label(),
            Item::FogTint => fog.tint_label(),
            Item::Theme => format!("PACK: {theme}"),
            Item::Override(category) => format!(
                "{}: {}",
                category.label(),
                if policy.overridden(*category) {
                    "ON"
                } else {
                    "OFF"
                }
            ),
            _ => String::new(),
        };
        p.literal_text(
            bx + 8.0,
            by + 7.0,
            &fit(&format!(
                "{marker}{}",
                if !dynamic.is_empty() {
                    dynamic.as_str()
                } else if *item == Item::FileTree {
                    if visible {
                        "FILE TREE: ON"
                    } else {
                        "FILE TREE: OFF"
                    }
                } else {
                    label(*item)
                }
            )),
            GLYPH,
            TEXT,
        );
    }
    p.vertices
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Pause,
    Settings,
    PlayerSettings,
    Appearance,
    Storage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Resume,
    Settings,
    PlayerSettings,
    Storage,
    StorageItemLimit,
    StorageDiskLimit,
    StorageChoose,
    FogEnabled,
    FogDistance,
    FogTint,
    Quit,
    Smaller,
    Larger,
    Reset,
    FileTree,
    Appearance,
    Theme,
    Override(crate::appearance::Category),
    Back,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Menu {
    pub view: View,
    pub focus: usize,
}
impl Menu {
    pub fn pause() -> Self {
        Self {
            view: View::Pause,
            focus: 0,
        }
    }
    /// Q is a deliberate confirmation with Quit focused, never an exit by itself.
    pub fn quit_confirm() -> Self {
        Self {
            view: View::Pause,
            focus: 2,
        }
    }
    pub fn settings() -> Self {
        Self {
            view: View::Settings,
            focus: 0,
        }
    }
    pub fn step(&mut self, reverse: bool) {
        let n = self.items().len();
        self.focus = if reverse {
            (self.focus.min(n - 1) + n - 1) % n
        } else {
            (self.focus.min(n - 1) + 1) % n
        };
    }
    pub fn items(&self) -> &'static [Item] {
        match self.view {
            View::Pause => &[Item::Resume, Item::Settings, Item::Quit],
            View::Settings => &[Item::PlayerSettings, Item::Appearance, Item::Back],
            View::PlayerSettings => &[
                Item::Smaller,
                Item::Larger,
                Item::Reset,
                Item::FileTree,
                Item::FogEnabled,
                Item::FogDistance,
                Item::FogTint,
                Item::Storage,
                Item::Back,
            ],
            View::Storage => &[Item::StorageItemLimit, Item::StorageDiskLimit, Item::StorageChoose, Item::Back],
            View::Appearance => &[
                Item::Theme,
                Item::Override(crate::appearance::Category::Skybox),
                Item::Override(crate::appearance::Category::Walls),
                Item::Override(crate::appearance::Category::Floor),
                Item::Override(crate::appearance::Category::Documents),
                Item::Override(crate::appearance::Category::Doors),
                Item::Override(crate::appearance::Category::Decorations),
                Item::Back,
            ],
        }
    }
    pub fn focused(&self) -> Item {
        let items = self.items();
        items[self.focus.min(items.len() - 1)]
    }
}
