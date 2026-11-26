use super::{
    Painter,
    directory_map::{Controls, short},
};
use crate::{
    directory_map::{Map, Target},
    directory_map_view::{Rect, View, confirmation_rects, history_buttons},
};
const INK: [f32; 3] = [0.79, 0.87, 0.81];
const MUTED: [f32; 3] = [0.43, 0.56, 0.48];
const GREEN: [f32; 3] = [0.50, 0.85, 0.53];
fn button(p: &mut Painter, r: Rect, label: &str, active: bool, hovered: bool) {
    p.rect(
        r[0],
        r[1],
        r[2] - r[0],
        r[3] - r[1],
        if active {
            [0.16, 0.31, 0.21]
        } else {
            [0.08, 0.15, 0.11]
        },
    );
    if hovered {
        let c = [0.75, 1.0, 0.79];
        p.rect(r[0], r[1], r[2] - r[0], 2.0, c);
        p.rect(r[0], r[3] - 2.0, r[2] - r[0], 2.0, c);
        p.rect(r[0], r[1], 2.0, r[3] - r[1], c);
        p.rect(r[2] - 2.0, r[1], 2.0, r[3] - r[1], c);
    }
    p.text(
        r[0] + 8.0,
        r[1] + 8.0,
        label,
        1.15,
        if active { GREEN } else { INK },
    );
}
pub(super) fn draw(p: &mut Painter, map: &Map, view: &View, controls: &Controls<'_>) {
    let (w, h) = (p.width, p.height);
    let side = view.side;
    if side > 0.0 {
        p.rect(w - side, 0.0, side, h - 76.0, [0.035, 0.065, 0.049]);
        p.text(w - side + 16.0, 28.0, "ROUTE HISTORY", 1.8, INK);
        p.text(w - side + 16.0, 62.0, "VISITS / NEWEST FIRST", 1.15, MUTED);
        let n = &map.nodes[map.current.unwrap_or(0)];
        p.text(
            w - side + 16.0,
            98.0,
            &format!("{}D / {}F / {} LINKS", n.folders, n.files, n.other),
            1.15,
            MUTED,
        );
        p.text(w - side + 16.0, 126.0, "COUNTS LAST SEEN", 1.15, MUTED);
        p.text(w - side + 16.0, 162.0, "CLICK TO SELECT", 1.2, GREEN);
        p.text(w - side + 16.0, 182.0, "THEN CONFIRM TRAVEL", 1.2, GREEN);
    } else {
        p.text(26.0, h - 150.0, "ROUTE / NEWEST FIRST", 1.1, MUTED);
    }
    for &(i, r) in &view.route_rows {
        let entry = &map.history[i];
        let latest = i + 1 == map.history.len();
        p.rect(
            r[0],
            r[1],
            r[2] - r[0],
            r[3] - r[1],
            if controls.hover
                == Some(crate::directory_map_view::Action::Destination(
                    Target::History(i),
                ))
            {
                [0.14, 0.28, 0.18]
            } else if latest {
                [0.09, 0.18, 0.12]
            } else {
                [0.055, 0.10, 0.073]
            },
        );
        let cols = ((r[2] - r[0] - 14.0) / 7.0) as usize;
        p.text(
            r[0] + 7.0,
            r[1] + 5.0,
            &short(&format!("{} {}", i + 1, entry.name), cols),
            1.1,
            if latest { GREEN } else { INK },
        );
        p.text(
            r[0] + 7.0,
            r[1] + 23.0,
            &format!(
                "PAGE {}{}",
                entry.page + 1,
                if latest { " / YOU" } else { "" }
            ),
            1.0,
            MUTED,
        );
    }
    for (i, r) in history_buttons(w as u32, h as u32).into_iter().enumerate() {
        button(
            p,
            r,
            if i == 0 { "OLDER [" } else { "NEWER ]" },
            false,
            controls.hover
                == Some(if i == 0 {
                    crate::directory_map_view::Action::Older
                } else {
                    crate::directory_map_view::Action::Newer
                }),
        );
    }
    if let Some(c) = controls.selection {
        p.rect(0.0, 0.0, w, h, [0.015, 0.029, 0.023]);
        let (r, buttons) = confirmation_rects(w as u32, h as u32);
        let x = r[0] + 24.0;
        let y = r[1] + 24.0;
        p.rect(r[0], r[1], r[2] - r[0], r[3] - r[1], [0.055, 0.105, 0.077]);
        p.text(x, y, "TRAVEL TO THIS ROOM?", 1.8, GREEN);
        let label = short(&format!("{} / PAGE {}", c.name, c.page + 1), 84);
        let chars: Vec<_> = label.chars().collect();
        for (i, chunk) in chars.chunks(42).enumerate() {
            p.text(
                x,
                y + 40.0 + i as f32 * 20.0,
                &chunk.iter().collect::<String>(),
                1.4,
                INK,
            );
        }
        p.text(
            x,
            y + 92.0,
            "ENTRY DOORWAY / FACING INTO THE ROOM",
            1.2,
            INK,
        );
        p.text(
            x,
            y + 114.0,
            "FILESYSTEM ROOT USES SAFE ENTRY POSITION",
            1.05,
            MUTED,
        );
        p.text(
            x,
            y + 140.0,
            if matches!(c.target, Target::Node(_)) {
                "JUMP ADDS A VISIT TO ROUTE HISTORY"
            } else {
                "RETURN TO VISIT / REMOVE NEWER ROUTE ENTRIES"
            },
            1.15,
            MUTED,
        );
        p.text(
            x,
            y + 166.0,
            "TRAVEL CLOSES MAP / CANCEL LEAVES IT OPEN",
            1.15,
            MUTED,
        );
        p.text(
            x,
            y + 192.0,
            "TAB CHOOSES / ENTER ACTIVATES / ESC CANCELS",
            1.1,
            MUTED,
        );
        if let Some(error) = &c.error {
            p.text(
                x,
                y + 222.0,
                &short(&format!("REFUSED: {error}"), 68),
                1.1,
                [0.95, 0.67, 0.49],
            );
        }
        button(
            p,
            buttons[0],
            "CANCEL",
            !controls.confirm_travel,
            controls.hover == Some(crate::directory_map_view::Action::Cancel),
        );
        button(
            p,
            buttons[1],
            "TRAVEL",
            controls.confirm_travel,
            controls.hover == Some(crate::directory_map_view::Action::Travel),
        );
    }
}
