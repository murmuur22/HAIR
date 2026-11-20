use crate::assets::Vertex;
pub mod exploration_map;
// Informational 2x bitmap sidebar; current-centred window, no navigation actions.
const TREE_SCALE: f32 = 2.0;
const TREE_TOP: f32 = 76.0;
const TREE_HEADER_HEIGHT: f32 = 24.0;
const TREE_ROW_PITCH: f32 = 20.0;
fn tree_width(width: u32, height: u32) -> f32 {
    if width < 320 || height < 240 {
        0.0
    } else {
        (width as f32 * 0.32).min(260.0)
    }
}
const WEB_HISTORY: [f32; 3] = [0.6, 0.75, 0.95];
const WEB_ROOM: [f32; 3] = [0.9, 0.8, 0.5];
const WEB_LINK: [f32; 3] = [0.85, 0.85, 0.85];
const WEB_VISITED: [f32; 3] = [0.55, 0.55, 0.55];
/// Current-relative web context in display order and the current row index.
/// '<' visitor-history trail (R/return door), '=' same held-response room,
/// '>' current page, '*' visited link, '-' unvisited link; '^' marks another
/// origin. Only data the session already holds: no crawl or preload.
fn web_rows(world: &crate::World) -> (Vec<(String, [f32; 3])>, usize) {
    let mut rows = Vec::new();
    let pocket = world.pocket.as_ref();
    for previous in pocket.map_or(&[][..], |p| p.history.as_slice()) {
        let label = if previous.title.trim().is_empty() {
            &previous.address
        } else {
            &previous.title
        };
        rows.push((format!("< {label}"), WEB_HISTORY));
    }
    let room = pocket.and_then(|p| p.room);
    if let Some((_, true, _)) = room {
        rows.push(("= previous room (same page)".into(), WEB_ROOM));
    }
    let title = world
        .site
        .as_ref()
        .and_then(|s| s.pages.get(&world.current))
        .and_then(|t| t.lines().next())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(&world.current);
    let current = rows.len();
    rows.push((
        match room {
            Some((index, ..)) => format!("> {title} [room {}]", index + 1),
            None => format!("> {title}"),
        },
        [0.6, 1.0, 0.7],
    ));
    if let Some((_, _, true)) = room {
        rows.push(("= next room (same page)".into(), WEB_ROOM));
    }
    let origin = reqwest::Url::parse(&world.current).ok().map(|u| u.origin());
    let mut seen = Vec::new();
    for link in world
        .site
        .as_ref()
        .and_then(|s| s.records.get(&world.current))
        .into_iter()
        .flatten()
    {
        let crate::content::Destination::External(target) = &link.destination else {
            continue;
        };
        if seen.contains(&target) || seen.len() >= 64 {
            continue;
        }
        seen.push(target);
        let visited = pocket.is_some_and(|p| p.visited.contains(target));
        let other = reqwest::Url::parse(target)
            .ok()
            .is_some_and(|u| Some(u.origin()) != origin);
        rows.push((
            format!(
                "{}{} {}",
                if visited { '*' } else { '-' },
                if other { "^" } else { "" },
                if link.label.trim().is_empty() {
                    target
                } else {
                    &link.label
                }
            ),
            if visited { WEB_VISITED } else { WEB_LINK },
        ));
    }
    (rows, current)
}
#[cfg(test)]
fn tree_overlay(world: &crate::World, width: u32, height: u32) -> Vec<Vertex> {
    tree_overlay_above(world, width, height, f32::INFINITY)
}
/// The panel ends above `bottom` (an aimed-door panel's top); with no room
/// even for its header it yields to that transient panel instead of overlapping.
fn tree_overlay_above(world: &crate::World, width: u32, height: u32, bottom: f32) -> Vec<Vertex> {
    let w = tree_width(width, height);
    if w == 0.0 {
        return Vec::new();
    }
    let mut p = Painter {
        vertices: Vec::new(),
        width: width as f32,
        height: height as f32,
    };
    let cols = ((w - 12.0) / (6.0 * TREE_SCALE)) as usize;
    // Closed local rooms can use the space immediately below the 52px header.
    // Readers and non-local worlds retain the original top and row budget.
    let tree_top = if world.folder_view.is_some() && !world.reading {
        52.0
    } else {
        TREE_TOP
    };
    // Keep the complete panel above the viewport midpoint with bottom clearance.
    if bottom - 4.0 < tree_top + TREE_HEADER_HEIGHT {
        return Vec::new();
    }
    let max_rows =
        ((((height as f32 * 0.5 - 16.0).min(bottom - 4.0) - tree_top - TREE_HEADER_HEIGHT)
            / TREE_ROW_PITCH)
            .max(0.0) as usize)
            .min(6);
    if world.current.starts_with("https://") {
        // A link/history graph, labelled as such; never a folder hierarchy.
        let (rows, current) = web_rows(world);
        let count = if world.tree_collapsed {
            0
        } else {
            max_rows.min(rows.len())
        };
        let start = current
            .saturating_sub(count / 2)
            .min(rows.len().saturating_sub(count));
        p.rect(
            12.0,
            tree_top,
            w,
            TREE_HEADER_HEIGHT + count as f32 * TREE_ROW_PITCH,
            [0.012, 0.012, 0.012],
        );
        let suffix = match (world.reading, world.tree_collapsed) {
            (false, true) => " SHIFT M SHOW",
            (false, false) => " SHIFT M HIDE",
            (true, true) => " M SHOW",
            (true, false) => " M HIDE",
        };
        p.text(
            18.0,
            tree_top + 4.0,
            &format!(
                "{}{suffix}",
                bounded_line("WEB LINKS/HISTORY", cols.saturating_sub(suffix.len()))
            ),
            TREE_SCALE,
            [0.8, 0.8, 0.8],
        );
        for (i, (line, color)) in rows.iter().enumerate().skip(start).take(count) {
            let y = tree_top + TREE_HEADER_HEIGHT + 4.0 + (i - start) as f32 * TREE_ROW_PITCH;
            p.literal_text(18.0, y, &bounded_line(line, cols), TREE_SCALE, *color);
        }
        return p.vertices;
    }
    let count = if world.tree_collapsed {
        0
    } else {
        max_rows.min(world.navigation.rows.len())
    };
    let current = world.navigation.current_index(&world.current);
    let start = current
        .unwrap_or(0)
        .saturating_sub(count / 2)
        .min(world.navigation.rows.len().saturating_sub(count));
    p.rect(
        12.0,
        tree_top,
        w,
        TREE_HEADER_HEIGHT + count as f32 * TREE_ROW_PITCH,
        [0.012, 0.012, 0.012],
    );
    let suffix = match (world.tree_collapsed, cols >= 10) {
        (true, true) => " | M SHOW",
        (false, true) => " | M HIDE",
        (true, false) => " M SHOW",
        (false, false) => " M HIDE",
    };
    let title = bounded_line(&world.navigation.title, cols.saturating_sub(suffix.len()));
    p.text(
        18.0,
        tree_top + 4.0,
        &format!("{title}{suffix}"),
        TREE_SCALE,
        [0.8, 0.8, 0.8],
    );
    for (i, row) in world
        .navigation
        .rows
        .iter()
        .enumerate()
        .skip(start)
        .take(count)
    {
        let marker = if Some(i) == current { ">" } else { " " };
        // Human-facing labels take priority over indentation and technical IDs.
        // Retain the ordinal where it fits; never let a long source eat the label.
        let mut prefix = format!("{marker}{:02} ", i + 1);
        let prefix_budget = cols.saturating_sub(4);
        if prefix.len() > prefix_budget {
            prefix = format!("{marker} ");
        }
        prefix.push_str(
            &" ".repeat(
                row.depth
                    .min(4)
                    .min(prefix_budget.saturating_sub(prefix.len())),
            ),
        );
        let id_budget = cols.saturating_sub(prefix.len() + 8 + 3).min(10);
        let suffix = if world.folder_view.is_some() {
            String::new()
        } else if id_budget >= 2 {
            let id_len = row.id.chars().count();
            let id = if id_len > id_budget {
                let tail: String = row.id.chars().skip(id_len - (id_budget - 1)).collect();
                format!("~{tail}")
            } else {
                row.id.clone()
            };
            format!(" [{id}]")
        } else {
            String::new()
        };
        let available = cols.saturating_sub(prefix.len() + suffix.chars().count());
        let label = if world.folder_view.is_some() {
            bounded_tail(&row.label, available)
        } else {
            bounded_line(&row.label, available)
        };
        let line = format!("{prefix}{label}{suffix}");
        let y = tree_top + TREE_HEADER_HEIGHT + 4.0 + (i - start) as f32 * TREE_ROW_PITCH;
        let color = if Some(i) == current {
            [0.6, 1.0, 0.7]
        } else {
            [0.85; 3]
        };
        if world.folder_view.is_some() {
            p.literal_text(18.0, y, &line, TREE_SCALE, color);
        } else {
            p.text(18.0, y, &line, TREE_SCALE, color);
        }
    }
    p.vertices
}
fn bounded_tail(text: &str, columns: usize) -> String {
    let count = text.chars().count();
    if count <= columns {
        return text.to_string();
    }
    if columns == 0 {
        return String::new();
    }
    format!(
        "~{}",
        text.chars()
            .skip(count - columns.saturating_sub(1))
            .collect::<String>()
    )
}
/// A visible '~' marks truncation; never normalize the underlying source/target.
fn bounded_line(text: &str, columns: usize) -> String {
    let mut line: String = text.chars().take(columns).collect();
    if text.chars().count() > columns && columns > 0 {
        line.pop();
        line.push('~');
    }
    line
}
/// Two bounded neutral status lines for the closed-reader context only.
pub fn pack_overlay(message: &str, width: u32, height: u32) -> Vec<Vertex> {
    if width < 320 || height < 240 || message.is_empty() {
        return Vec::new();
    }
    let mut p = Painter {
        vertices: Vec::new(),
        width: width as f32,
        height: height as f32,
    };
    let columns = (width as usize).saturating_sub(24) / 12;
    let three_lines = message.lines().take(3).count() == 3;
    for (i, line) in message.lines().take(3).enumerate() {
        let text = bounded_line(line, columns);
        let y = height as f32 - if three_lines { 96.0 } else { 76.0 } + i as f32 * 20.0;
        p.rect(
            10.0,
            y - 2.0,
            width as f32 - 20.0,
            18.0,
            [0.025, 0.025, 0.025],
        );
        p.literal_text(12.0, y, &text, 2.0, [0.85, 0.85, 0.85]);
    }
    p.vertices
}
struct ReaderLayout {
    scale: f32,
    sidebar: f32,
    columns: usize,
    document_y: f32,
    rows: usize,
}
// Shared by drawing and local keyboard scroll bounds. The complete document
// rewraps at the active scale and the width reclaimed by collapsing the tree.
fn reader_layout(world: &crate::World, width: u32, height: u32) -> ReaderLayout {
    let scale = if (width >= 800 && height >= 500)
        || (world.folder_view.is_some() && world.reading && width >= 640 && height >= 480)
    {
        2.0
    } else {
        1.0
    };
    let tree = tree_width(width, height);
    let sidebar = if tree > 0.0 && world.reading && !world.tree_collapsed {
        tree + 12.0
    } else {
        0.0
    };
    let columns = ((width as f32 - sidebar - 48.0) / (6.0 * scale)).max(1.0) as usize;
    let document_y = if world.tree_collapsed && tree > 0.0 {
        TREE_TOP + TREE_HEADER_HEIGHT + 8.0
    } else {
        80.0
    };
    let rows = ((height as f32 - document_y - 84.0) / (9.0 * scale)).max(1.0) as usize;
    ReaderLayout {
        scale,
        sidebar,
        columns,
        document_y,
        rows,
    }
}
fn wrapped_document_rows(document: &str, columns: usize) -> usize {
    // Match overlay's str::lines + character chunks, not byte chunks. Each empty
    // logical line occupies one row; a terminal newline adds no extra line.
    document
        .lines()
        .map(|line| line.chars().count().div_ceil(columns).max(1))
        .sum()
}
/// Last real wrapped row of the complete local reader, including source metadata.
/// Recount without allocating wrapped strings so body/tree/viewport changes are
/// reflected immediately. Local folder readers have no selected-link prefix.
pub fn local_reader_scroll_max(world: &crate::World, width: u32, height: u32) -> usize {
    reader_scroll_max(world, width, height)
}
/// Last real wrapped row of any reader, including the selected-address prefix
/// that the overlay draws before the document.
pub fn reader_scroll_max(world: &crate::World, width: u32, height: u32) -> usize {
    let layout = reader_layout(world, width, height);
    if world.document_code().is_empty() {
        let prefix = selected_address(world)
            .map_or(0, |prefix| wrapped_document_rows(&prefix, layout.columns));
        return (prefix
            + wrapped_document_rows(world.document().unwrap_or("DOCUMENT ERROR"), layout.columns))
        .saturating_sub(1);
    }
    reader_rows(world, layout.columns).len().saturating_sub(1)
}
const BODY: [f32; 3] = [0.9, 0.96, 0.91];
const CODE_BG: [f32; 3] = [0.035, 0.07, 0.09];
const CODE_PLAIN: [f32; 3] = [0.82, 0.9, 1.0];
const CODE_GUTTER: [f32; 3] = [0.4, 0.5, 0.55];
fn code_color(class: crate::highlight::Class) -> [f32; 3] {
    use crate::highlight::Class;
    match class {
        Class::Plain => CODE_PLAIN,
        Class::Keyword => [1.0, 0.62, 0.42],
        Class::String => [0.98, 0.86, 0.45],
        Class::Comment => [0.55, 0.62, 0.58],
        Class::Number => [0.78, 0.6, 1.0],
    }
}
/// One drawn reader row. Code-block rows carry a gutter marker drawn outside
/// the text: '|' starts a source line, '>' continues a soft-wrapped one.
struct ReaderRow {
    chars: Vec<(char, [f32; 3])>,
    gutter: Option<char>,
}
fn push_wrapped(rows: &mut Vec<ReaderRow>, chars: &[(char, [f32; 3])], columns: usize, code: bool) {
    let columns = columns.max(1);
    if chars.is_empty() {
        rows.push(ReaderRow {
            chars: Vec::new(),
            gutter: code.then_some('|'),
        });
    }
    for (i, chunk) in chars.chunks(columns).enumerate() {
        rows.push(ReaderRow {
            chars: chunk.to_vec(),
            gutter: code.then_some(if i == 0 { '|' } else { '>' }),
        });
    }
}
/// Reader rows in display order. Prose wraps by characters exactly as before;
/// inline code keeps its place in prose with a code colour. Block code keeps
/// every source line, expands tabs to 4-column stops for display only, and
/// soft-wraps two columns narrower to leave room for the gutter marker.
fn reader_rows(world: &crate::World, columns: usize) -> Vec<ReaderRow> {
    let mut rows = Vec::new();
    if let Some(prefix) = selected_address(world) {
        for line in prefix.lines() {
            let chars: Vec<_> = line.chars().map(|c| (c, BODY)).collect();
            push_wrapped(&mut rows, &chars, columns, false);
        }
    }
    let document = world.document().unwrap_or("DOCUMENT ERROR");
    let code: Vec<_> = world
        .document_code()
        .iter()
        .filter(|c| document.get(c.range.clone()).is_some())
        .collect();
    // (block index, its classes, char offset of the current line in the block)
    let mut block: Option<(usize, Vec<crate::highlight::Class>, usize, usize)> = None;
    for line in document.lines() {
        let start = line.as_ptr() as usize - document.as_ptr() as usize;
        let end = start + line.len();
        let inside = code
            .iter()
            .position(|c| c.block && c.range.start <= start && start < c.range.end);
        let Some(index) = inside else {
            let chars: Vec<_> = line
                .char_indices()
                .map(|(i, c)| {
                    let at = start + i;
                    let inline = code
                        .iter()
                        .any(|s| !s.block && s.range.start <= at && at < s.range.end);
                    (c, if inline { CODE_PLAIN } else { BODY })
                })
                .collect();
            push_wrapped(&mut rows, &chars, columns, false);
            continue;
        };
        let span = code[index];
        // Classify each block once; advance its char offset line by line.
        match &mut block {
            Some((i, _, offset, previous)) if *i == index => {
                *offset += document[*previous..start].chars().count();
                *previous = start;
            }
            _ => {
                block = Some((
                    index,
                    crate::highlight::classify(
                        span.language.as_deref(),
                        &document[span.range.clone()],
                    ),
                    document[span.range.start..start].chars().count(),
                    start,
                ));
            }
        }
        let Some((_, classes, offset, _)) = &block else {
            continue;
        };
        let mut chars = Vec::new();
        for (k, c) in document[start..end.min(span.range.end)].chars().enumerate() {
            let color = code_color(
                classes
                    .get(*offset + k)
                    .copied()
                    .unwrap_or(crate::highlight::Class::Plain),
            );
            if c == '\t' {
                let stop = (chars.len() / 4 + 1) * 4;
                chars.resize(stop, (' ', color));
            } else {
                chars.push((c, color));
            }
        }
        push_wrapped(&mut rows, &chars, columns.saturating_sub(2), true);
    }
    rows
}
/// Logical pixel pitch of one reader row; wheel pixel deltas use the same metric.
pub fn reader_row_height(world: &crate::World, width: u32, height: u32) -> f32 {
    9.0 * reader_layout(world, width, height).scale
}
fn selected_address(world: &crate::World) -> Option<String> {
    world.selected_reader_link().and_then(|selection| {
        if let crate::content::Destination::External(destination) = &selection.link.destination {
            Some(format!(
                "SELECTED LINK ADDRESS (exact; Up/Down inspect):\n{destination}\n\n"
            ))
        } else {
            None
        }
    })
}
pub fn image_overlay(world: &crate::World, width: u32, height: u32) -> Vec<Vertex> {
    let mut painter = Painter {
        vertices: Vec::new(),
        width: width as f32,
        height: height as f32,
    };
    painter.rect(0.0, 0.0, width as f32, height as f32, [0.025; 3]);
    painter.rect(0.0, 0.0, width as f32, 52.0, [0.015, 0.04, 0.025]);
    let columns = (width as usize).saturating_sub(24) / 12;
    painter.literal_text(
        12.0,
        8.0,
        &bounded_tail(&world.current, columns),
        2.0,
        [0.5, 1.0, 0.65],
    );
    painter.text(
        12.0,
        32.0,
        "+/- ZOOM | ARROWS PAN | 0 FIT | IMAGE",
        2.0,
        [0.9; 3],
    );
    painter.text(
        24.0,
        height as f32 - 72.0,
        "V RAW / TEXT / IMAGE | I SOURCE / BODY",
        2.0,
        [0.6, 1.0, 0.7],
    );
    painter.text(
        24.0,
        height as f32 - 48.0,
        "ESC CLOSE | C RETRY SAME HANDLE",
        2.0,
        [0.6, 1.0, 0.7],
    );
    painter.vertices
}
pub fn overlay(world: &crate::World, width: u32, height: u32, scroll: usize) -> Vec<Vertex> {
    overlay_with_web_feedback(world, width, height, scroll, false)
}
/// Active attempt feedback replaces only the selected-portal preview, not help,
/// reader, tree, interaction state or activation. It is never bound to a slot.
pub fn overlay_with_web_feedback(
    world: &crate::World,
    width: u32,
    height: u32,
    scroll: usize,
    feedback_active: bool,
) -> Vec<Vertex> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let mut painter = Painter {
        vertices: Vec::new(),
        width: width as f32,
        height: height as f32,
    };
    let ReaderLayout {
        scale,
        sidebar,
        columns: document_columns,
        document_y,
        rows,
    } = reader_layout(world, width, height);
    let columns = ((width as f32 - 48.0) / (6.0 * scale)).max(8.0) as usize;
    let web = world.current.starts_with("https://");
    painter.rect(0.0, 0.0, width as f32, 52.0, [0.015, 0.04, 0.025]);
    painter.literal_text(
        12.0,
        8.0,
        &if world.folder_view.is_some() {
            bounded_tail(
                &format!("HAIR {} | {}", env!("CARGO_PKG_VERSION"), world.current),
                ((width as f32 - 24.0) / (6.0 * scale)).max(0.0) as usize,
            )
        } else {
            bounded_line(
                &format!("HAIR {} | {}", env!("CARGO_PKG_VERSION"), world.current),
                ((width as f32 - 24.0) / (6.0 * scale)).max(0.0) as usize,
            )
        },
        scale,
        [0.5, 1.0, 0.65],
    );
    painter.text(
        12.0,
        28.0,
        if world.folder_view.is_some() && world.reading {
            "READ-ONLY LOCAL | ~ TRUNCATED | EXACT SOURCE ABOVE | ASCII GLYPH FALLBACK"
        } else if world.folder_view.is_some() {
            "WASD | CLICK LOOK | E INSPECT/ENTER | I SOURCE | O CHOOSE | T PACK | M TREE"
        } else if world.reading && web {
            "STATIC HTML | ~ = TRUNCATED | EXACT SOURCE BELOW; UP/DOWN/WHEEL TO INSPECT"
        } else if world.reading {
            "LOCAL MARKDOWN | ASCII READER"
        } else if web {
            "WASD | CLICK LOOK | T PACK | M MAP | SHIFT M LINKS | ~ READ FOR FULL ADDRESS"
        } else {
            "WASD | CLICK LOOK | E INTERACT | TAB/SHIFT TAB TARGET | T THEME"
        },
        1.0,
        [0.9, 0.95, 0.9],
    );
    if !world.current_space.is_empty() {
        painter.text(
            12.0,
            55.0,
            &format!("SPACE: {}", world.current_space)
                .chars()
                .take((width as usize / 6).saturating_sub(4))
                .collect::<String>(),
            1.0,
            [0.6, 1.0, 0.7],
        );
    }
    if world.reading {
        painter.rect(
            12.0,
            64.0,
            width as f32 - 24.0,
            height as f32 - 76.0,
            [0.025, 0.045, 0.035],
        );
        // Keep the bounded footer as a preview. The selected HTTPS target also
        // belongs to the existing scrollable reader, before the document, so
        // every literal byte (including a long tail) is reachable with Up/Down.
        let lines = reader_rows(world, document_columns);
        let x = 24.0 + sidebar;
        for (row, line) in lines
            .iter()
            .skip(scroll.min(lines.len().saturating_sub(1)))
            .take(rows)
            .enumerate()
        {
            let y = document_y + row as f32 * 9.0 * scale;
            let mut left = x;
            if let Some(gutter) = line.gutter {
                painter.rect(
                    x - 3.0,
                    y - scale,
                    (document_columns as f32 * 6.0 + 1.0) * scale + 6.0,
                    9.0 * scale,
                    CODE_BG,
                );
                painter.literal_text(x, y, &gutter.to_string(), scale, CODE_GUTTER);
                left += 12.0 * scale;
            }
            let mut start = 0;
            while start < line.chars.len() {
                let color = line.chars[start].1;
                let end = line.chars[start..]
                    .iter()
                    .position(|(_, c)| *c != color)
                    .map_or(line.chars.len(), |n| start + n);
                let run: String = line.chars[start..end].iter().map(|(c, _)| *c).collect();
                painter.literal_text(left + start as f32 * 6.0 * scale, y, &run, scale, color);
                start = end;
            }
        }
        painter.rect(
            20.0,
            height as f32 - 82.0,
            width as f32 - 40.0,
            24.0,
            [0.06, 0.16, 0.10],
        );
        painter.literal_text(
            24.0,
            height as f32 - 78.0,
            &bounded_line(
                &world.reader_link_description(),
                (width as usize).saturating_sub(48) / 6,
            ),
            1.0,
            [0.6, 1.0, 0.7],
        );
        painter.text(
            24.0,
            height as f32 - 52.0,
            if world.folder_view.is_some() {
                "LEFT/RIGHT CHUNK | V TEXT/RAW | I METADATA/BODY | C RETRY | LINKS INACTIVE"
            } else if world.pocket.as_ref().is_some_and(|p| p.reading.is_some()) {
                "ESC CLOSE COLLECTION | T PACK | L ADDRESS"
            } else {
                "TAB/SHIFT TAB LINK | E FOLLOW | T PACK | L ADDRESS | WHEEL/HELD UP-DOWN | M MAP"
            },
            1.0,
            [0.6, 0.9, 0.7],
        );
        painter.text(
            24.0,
            height as f32 - 32.0,
            if world.folder_view.is_some() {
                "ESC CLOSE | UP/DOWN SCROLL | LOCAL READ-ONLY"
            } else if web {
                "ESC CLOSE | UP/DOWN SCROLL | STATIC HTML | R RETURN | F5 REFRESH"
            } else {
                "ESC CLOSE | UP/DOWN SCROLL | LOCAL MARKDOWN - ASCII READER"
            },
            scale,
            [0.4, 0.95, 0.6],
        );
    } else {
        if world.folder_view.is_some() {
            painter.literal_text(
                12.0,
                height as f32 - 28.0,
                &bounded_line(&world.target_description(), columns),
                scale,
                [0.1, 0.95, 0.35],
            );
        } else {
            painter.text(
                12.0,
                height as f32 - 28.0,
                &world
                    .target_description()
                    .chars()
                    .take(columns)
                    .collect::<String>(),
                scale,
                [0.1, 0.95, 0.35],
            );
        }
        // Local image targets already emit this same prompt through the literal
        // bottom target_description line, with E/I also in the top help. Omit only
        // its duplicate above-footer panel, not navigation, status or source access.
        if let Some(target) = &world.interaction
            && crate::web::portals::view(world, &target.anchor).is_none()
            && !is_return_door(world, &target.anchor)
            && !world
                .folder_view
                .as_ref()
                .and_then(|view| view.bindings.get(&target.anchor))
                .is_some_and(|bound| {
                    bound.entry.kind == crate::local_explorer::Kind::File
                        && std::path::Path::new(&bound.entry.name)
                            .extension()
                            .is_some_and(|suffix| {
                                [b"png".as_slice(), b"jpg".as_slice(), b"jpeg".as_slice()]
                                    .iter()
                                    .any(|s| suffix.as_encoded_bytes().eq_ignore_ascii_case(s))
                            })
                })
        {
            let prompt: String = if world.folder_view.is_some() {
                bounded_line(
                    &target.prompt,
                    ((width as usize).saturating_sub(24) / (6 * scale as usize)).min(64),
                )
            } else {
                target.prompt.chars().take(42).collect()
            };
            let x = (width as f32 * 0.5 - prompt.len() as f32 * 3.0 * scale).max(8.0);
            // Local files now show real content at the crosshair. Keep the
            // interaction label above bottom status, not across the image.
            let prompt_top = if world.folder_view.is_some() {
                height as f32 - 128.0
            } else {
                height as f32 * 0.5 + 14.0
            };
            painter.rect(
                x - 6.0,
                prompt_top,
                prompt.len() as f32 * 6.0 * scale + 12.0,
                11.0 * scale,
                [0.015, 0.04, 0.025],
            );
            if world.folder_view.is_some() {
                painter.literal_text(x, prompt_top + 3.0, &prompt, scale, [0.6, 1.0, 0.7]);
            } else {
                painter.text(x, prompt_top + 3.0, &prompt, scale, [0.6, 1.0, 0.7]);
            }
        }
        painter.rect(
            width as f32 / 2.0 - 2.0,
            height as f32 / 2.0 - 2.0,
            4.0,
            4.0,
            [0.2, 1.0, 0.4],
        );
    }
    let portal = if feedback_active {
        Vec::new()
    } else {
        portal_overlay(world, width, height)
    };
    let portal_top = portal.first().map_or(f32::INFINITY, |v| {
        (1.0 - v.position[1]) * 0.5 * height as f32
    });
    painter.vertices.extend(portal);
    painter
        .vertices
        .extend(tree_overlay_above(world, width, height, portal_top));
    painter.vertices
}
/// Address entry and aimed-door panels share the reader/menu 2x bitmap baseline
/// (14 logical ink rows); Settings scale multiplies it like every other overlay.
const PANEL_GLYPH: f32 = 2.0;
const PANEL_LINE: f32 = 20.0;
/// Header strip height; screen-space panels stay below it.
const HEADER_BOTTOM: f32 = 52.0;
fn panel_columns(width: u32) -> usize {
    ((width as f32 - 48.0) / (6.0 * PANEL_GLYPH)).max(8.0) as usize
}
/// ASCII-only display text split into exact `cols`-character rows.
fn chunk_rows(text: &str, cols: usize) -> Vec<String> {
    text.as_bytes()
        .chunks(cols.max(1))
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect()
}
/// At most `max_rows` rows of `text`. When it does not fit, the leading rows,
/// an explicit omitted-character count and the exact final row are shown, so
/// a long value never loses its distinguishing tail silently.
fn fit_rows(text: &str, cols: usize, max_rows: usize, where_full: &str) -> Vec<String> {
    let rows = chunk_rows(text, cols);
    if rows.len() <= max_rows.max(1) {
        return rows;
    }
    let head = max_rows.saturating_sub(2);
    let tail = rows.last().cloned().unwrap_or_default();
    let shown = head * cols + tail.len();
    let mut out: Vec<String> = rows.into_iter().take(head).collect();
    out.extend(chunk_rows(
        &format!("~ {} CHARS NOT SHOWN; {where_full}", text.len() - shown),
        cols,
    ));
    out.push(tail);
    out
}
/// Case-preserving ASCII address at the 2x baseline, wrapped and bounded to the
/// viewport between the header and the bottom status/target lines. A value
/// taller than the panel keeps its head and exact tail with an explicit count;
/// the complete literal destination stays scrollable in the reader (Tab).
pub fn portal_overlay(world: &crate::World, width: u32, height: u32) -> Vec<Vertex> {
    let lines = portal_rows(world, width, height);
    if lines.is_empty() {
        return Vec::new();
    }
    let h = lines.len() as f32 * PANEL_LINE + 12.0;
    let top = portal_bottom(height) - h;
    let mut p = Painter {
        vertices: Vec::new(),
        width: width as f32,
        height: height as f32,
    };
    p.rect(12.0, top, width as f32 - 24.0, h, [0.015, 0.04, 0.025]);
    for (i, line) in lines.iter().enumerate() {
        p.literal_text(
            24.0,
            top + 6.0 + i as f32 * PANEL_LINE,
            line,
            PANEL_GLYPH,
            [0.65, 1.0, 0.75],
        );
    }
    p.vertices
}
/// Clears the three-line status block (height - 98) and target line below it.
fn portal_bottom(height: u32) -> f32 {
    height as f32 - 100.0
}
/// Exact display rows of the aimed-door panel in the logical viewport.
pub fn portal_rows(world: &crate::World, width: u32, height: u32) -> Vec<String> {
    if world.reading || width < 640 || height < 480 {
        return Vec::new();
    }
    let Some(target) = &world.interaction else {
        return Vec::new();
    };
    let cols = panel_columns(width);
    let budget =
        (((portal_bottom(height) - HEADER_BOTTOM - 4.0 - 12.0) / PANEL_LINE).max(4.0)) as usize;
    let ascii = |s: &str, limit: usize| {
        s.chars()
            .take(limit)
            .map(|c| {
                if c.is_ascii_graphic() || c == ' ' {
                    c
                } else {
                    '?'
                }
            })
            .collect::<String>()
    };
    if is_return_door(world, &target.anchor) {
        let mut blocks = vec![Block::Fixed(
            "E RETURN = R | VISITOR HISTORY, NOT URL PARENT".into(),
        )];
        match world.pocket.as_ref().and_then(|p| p.previous.as_ref()) {
            Some(previous) => {
                blocks.push(Block::Fixed(
                    "PREVIOUS PAGE TITLE (fetched page title):".into(),
                ));
                blocks.push(Block::Capped(ascii(&previous.title, 256), 2));
                blocks.push(Block::Fixed("PREVIOUS ADDRESS (exact ASCII):".into()));
                blocks.push(Block::Rest(ascii(&previous.address, 2048)));
            }
            None => blocks.push(Block::Fixed(
                "UNAVAILABLE: no previous page in visitor history".into(),
            )),
        }
        layout_blocks(blocks, cols, budget, "R RETURNS THERE")
    } else {
        let Some(v) = crate::web::portals::view(world, &target.anchor) else {
            return Vec::new();
        };
        layout_blocks(
            portal_lines(world, &v),
            cols,
            budget,
            "FULL IN READER (TAB)",
        )
    }
}
/// Panel content: fixed labels, values capped to a few rows, and one value
/// (the address) that takes every remaining row of the viewport budget.
enum Block {
    Fixed(String),
    Capped(String, usize),
    /// Capped, but fully inspectable somewhere other than `full`.
    CappedTo(String, usize, &'static str),
    Rest(String),
}
fn layout_blocks(blocks: Vec<Block>, cols: usize, budget: usize, full: &str) -> Vec<String> {
    let used: usize = blocks
        .iter()
        .map(|b| match b {
            Block::Fixed(s) => chunk_rows(s, cols).len().max(1),
            Block::Capped(s, cap) | Block::CappedTo(s, cap, _) => {
                chunk_rows(s, cols).len().clamp(1, *cap)
            }
            Block::Rest(_) => 0,
        })
        .sum();
    let rest = budget.saturating_sub(used).max(3);
    blocks
        .into_iter()
        .flat_map(|b| match b {
            Block::Fixed(s) => chunk_rows(&s, cols),
            Block::Capped(s, cap) => fit_rows(&s, cols, cap, full),
            Block::CappedTo(s, cap, to) => fit_rows(&s, cols, cap, to),
            Block::Rest(s) => fit_rows(&s, cols, rest, full),
        })
        .collect()
}
fn is_return_door(world: &crate::World, anchor: &str) -> bool {
    anchor == "return-history" && world.current.starts_with("https://")
}
fn portal_lines(world: &crate::World, v: &crate::web::portals::View) -> Vec<Block> {
    let mut lines = vec![
        Block::Fixed(format!(
            "E FOLLOW {}/{} | TAB/SHIFT TAB{}",
            v.index + 1,
            v.count,
            if reqwest::Url::parse(&v.destination)
                .ok()
                .zip(reqwest::Url::parse(&world.current).ok())
                .is_some_and(|(a, b)| a.origin() != b.origin())
            {
                " | OTHER ORIGIN"
            } else {
                ""
            }
        )),
        Block::Fixed(if v.title.chars().count() > 512 {
            "LINK TITLE (source label; first 512 chars):".into()
        } else if v.title.is_ascii() {
            "LINK TITLE (source label, not page title):".into()
        } else {
            "LINK TITLE (ASCII fallback; non-ASCII = ?):".into()
        }),
    ];
    let title = v
        .title
        .chars()
        .take(512)
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
        .collect::<String>();
    lines.push(Block::Capped(title, 2));
    lines.push(Block::Fixed("ADDRESS (exact ASCII):".into()));
    lines.push(Block::Rest(v.destination.clone()));
    lines.push(Block::Fixed(
        "PLACEHOLDER: URL fingerprint, NOT a site image".into(),
    ));
    if let Some(reason) = &v.last_failure {
        lines.push(Block::Fixed(
            "LAST ATTEMPT FAILED | E RETRY | R RETURN".into(),
        ));
        // Failure reasons are fully inspectable only in F8 details.
        lines.push(Block::CappedTo(reason.clone(), 2, "FULL IN F8"));
    }
    if v.recovered {
        lines.push(Block::Fixed(
            "Destination removed; first surviving selected".into(),
        ));
    }
    lines
}
/// Transient literal entry at the 2x baseline, the edit cursor visible.
pub fn address_overlay(raw: &str, error: Option<&str>, width: u32, height: u32) -> Vec<Vertex> {
    address_overlay_scrolled(raw, error, 0, width, height)
}
/// `back` scrolls the wrapped input/error rows up from the tail (0 shows the
/// cursor). The panel stays inside the viewport; when the full bounded buffer
/// does not fit, a position line discloses the shown rows and Up/Down reaches
/// every literal character and error row.
pub fn address_overlay_scrolled(
    raw: &str,
    error: Option<&str>,
    back: usize,
    width: u32,
    height: u32,
) -> Vec<Vertex> {
    let lines = address_rows(raw, error, back, width, height);
    if lines.is_empty() {
        return Vec::new();
    }
    let top = ADDRESS_TOP;
    let mut p = Painter {
        vertices: Vec::new(),
        width: width as f32,
        height: height as f32,
    };
    p.rect(
        12.0,
        top,
        width as f32 - 24.0,
        lines.len() as f32 * PANEL_LINE + 12.0,
        [0.015, 0.04, 0.025],
    );
    for (i, line) in lines.iter().enumerate() {
        p.literal_text(
            24.0,
            top + 6.0 + i as f32 * PANEL_LINE,
            line,
            PANEL_GLYPH,
            [0.65, 1.0, 0.75],
        );
    }
    p.vertices
}
/// Below the header and the 16px web-feedback strip it may own (52..68).
const ADDRESS_TOP: f32 = 70.0;
/// Exact display rows of the address panel in the logical viewport.
pub fn address_rows(
    raw: &str,
    error: Option<&str>,
    back: usize,
    width: u32,
    height: u32,
) -> Vec<String> {
    let Some((mut lines, mut body, room)) = address_parts(raw, error, width, height) else {
        return Vec::new();
    };
    if body.len() <= room {
        lines.extend(body);
        return lines;
    }
    let visible = room - 1;
    let end = body.len() - back.min(body.len() - visible);
    let start = end - visible;
    lines.push(format!(
        "~ ROWS {}-{} OF {} | Up/Down scroll",
        start + 1,
        end,
        body.len()
    ));
    lines.extend(body.drain(start..end));
    lines
}
/// Largest useful `back` for address_rows at this viewport (0 when it fits).
pub fn address_scroll_max(raw: &str, error: Option<&str>, width: u32, height: u32) -> usize {
    address_parts(raw, error, width, height).map_or(0, |(_, body, room)| {
        if body.len() <= room {
            0
        } else {
            body.len() - (room - 1)
        }
    })
}
/// Fixed header rows, scrollable input/error rows and the rows available to them.
fn address_parts(
    raw: &str,
    error: Option<&str>,
    width: u32,
    height: u32,
) -> Option<(Vec<String>, Vec<String>, usize)> {
    if width < 640 || height < 480 {
        return None;
    }
    let cols = panel_columns(width);
    let mut lines = Vec::new();
    for fixed in [
        "HTTPS ADDRESS | Enter opens | Escape cancels",
        "Backspace edits | Delete clears | Up/Down scroll",
        "Literal input (non-ASCII shown as ?):",
    ] {
        lines.extend(chunk_rows(fixed, cols));
    }
    let mut body = Vec::new();
    let display: String = raw
        .chars()
        .take(2048)
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
        .collect();
    body.extend(chunk_rows(&display, cols));
    body.push("_".into());
    if let Some(error) = error {
        let display: String = error
            .chars()
            .take(512)
            .map(|c| {
                if c.is_ascii_graphic() || c == ' ' {
                    c
                } else {
                    '?'
                }
            })
            .collect();
        body.extend(chunk_rows(&display, cols));
    }
    // Rows that fit between ADDRESS_TOP and a 12px bottom margin.
    let capacity = ((height as f32 - ADDRESS_TOP - 12.0 - 12.0) / PANEL_LINE) as usize;
    let room = capacity.saturating_sub(lines.len()).max(2);
    Some((lines, body, room))
}
#[cfg(test)]
mod address_ui_tests {
    fn web_world(source: &str) -> crate::World {
        crate::web::Page::html(
            crate::web::Address::parse(source).unwrap(),
            b"<title>Read</title><p>Content</p>",
        )
        .unwrap()
        .generate()
        .unwrap()
        .world
    }
    #[test]
    fn capped_failure_reason_points_to_f8_and_destination_to_reader() {
        let world = web_world("https://example.org/start");
        // A multibyte char straddles the 64-byte row boundary; the reason's
        // exact tail identifies the actual error.
        let reason = format!("a{}{}ERR-TAIL-77", "é".repeat(40), "x".repeat(40));
        let destination = format!("https://example.org/{}END", "d".repeat(400));
        let v = crate::web::portals::View {
            destination: destination.clone(),
            title: "Link".into(),
            index: 0,
            count: 1,
            last_failure: Some(reason.clone()),
            recovered: false,
        };
        let cols = 64;
        let rows = super::layout_blocks(
            super::portal_lines(&world, &v),
            cols,
            12,
            "FULL IN READER (TAB)",
        );
        for r in &rows {
            assert!(r.chars().count() <= cols, "{r:?}");
        }
        let omitted: Vec<_> = rows.iter().filter(|r| r.contains("NOT SHOWN")).collect();
        assert_eq!(omitted.len(), 2, "{rows:?}");
        assert!(omitted[0].ends_with("FULL IN READER (TAB)"), "{rows:?}");
        assert!(omitted[1].ends_with("FULL IN F8"), "{rows:?}");
        assert!(rows.iter().any(|r| r.ends_with("dddEND")), "{rows:?}");
        let failed = rows
            .iter()
            .position(|r| r.starts_with("LAST ATTEMPT"))
            .unwrap();
        assert!(
            rows[failed + 1].contains("NOT SHOWN; FULL IN F8"),
            "{rows:?}"
        );
        let tail = &rows[failed + 2];
        assert!(
            !tail.is_empty() && reason.ends_with(tail.as_str()),
            "{rows:?}"
        );
    }
    #[test]
    fn long_source_header_is_bounded_at_normal_and_small_sizes() {
        let source = format!("https://example.org/{}?q=aB%2Fcd#Source", "aB".repeat(990));
        let mut world = web_world(&source);
        for reading in [false, true] {
            world.reading = reading;
            for (w, h) in [(960, 640), (640, 480)] {
                let ui = super::overlay(&world, w, h, 0);
                for v in ui.iter().filter(|v| v.color == [0.5, 1.0, 0.65]) {
                    assert!(
                        (-1.0..=1.0).contains(&v.position[0]),
                        "source header clipped: {w}x{h}, reader={reading}"
                    );
                }
                assert_eq!(
                    world.current, source,
                    "display cannot mutate navigation source"
                );
            }
        }
    }
    #[test]
    fn reader_exact_source_is_literal_in_scrollable_document() {
        let mut lower = web_world("https://example.org/aB?q=aB%2Fcd#Case");
        let mut upper = web_world("https://example.org/AB?q=AB%2FCD#CASE");
        lower.reading = true;
        upper.reading = true;
        for (w, h) in [(960, 640), (640, 480)] {
            let body = |world: &crate::World| {
                super::overlay(world, w, h, 0)
                    .into_iter()
                    .filter(|v| v.color == [0.9, 0.96, 0.91])
                    .collect::<Vec<_>>()
            };
            assert_ne!(
                bytemuck::cast_slice::<_, u8>(&body(&lower)),
                bytemuck::cast_slice::<_, u8>(&body(&upper)),
                "scrollable source was uppercased"
            );
        }
    }
    #[test]
    fn truncated_status_is_explicit_and_case_preserving() {
        for (w, h) in [(960, 640), (640, 480)] {
            // Parent's bottom-glyph slice already changed the rendered geometry
            // to 2x. Keep the exact literal-case and explicit-marker oracle.
            let cols = (w as usize - 24) / 12;
            let message = "aB".repeat(100);
            let actual = super::pack_overlay(&message, w, h);
            let mut expected = super::Painter {
                vertices: Vec::new(),
                width: w as f32,
                height: h as f32,
            };
            expected.rect(10.0, h as f32 - 78.0, w as f32 - 20.0, 18.0, [0.025; 3]);
            expected.literal_text(
                12.0,
                h as f32 - 76.0,
                &format!("{}~", message.chars().take(cols - 1).collect::<String>()),
                2.0,
                [0.85; 3],
            );
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(&actual),
                bytemuck::cast_slice::<_, u8>(&expected.vertices),
                "status must mark truncation and preserve literal case"
            );
        }
    }
    #[test]
    fn reader_selected_destination_preserves_case() {
        let make = |target: &str| {
            let mut world = crate::web::Page::html(
                crate::web::Address::parse("https://example.org/Read").unwrap(),
                format!("<a href='{target}'>Next</a>").as_bytes(),
            )
            .unwrap()
            .generate()
            .unwrap()
            .world;
            world.reading = true;
            world.cycle_reader_link(false);
            world
        };
        for (w, h) in [(960, 640), (640, 480)] {
            let footer = |world: &crate::World| {
                super::overlay(world, w, h, 0)
                    .into_iter()
                    .filter(|v| v.color == [0.6, 1.0, 0.7])
                    .collect::<Vec<_>>()
            };
            assert_ne!(
                bytemuck::cast_slice::<_, u8>(&footer(&make("/aB?q=aB%2Fcd#Case"))),
                bytemuck::cast_slice::<_, u8>(&footer(&make("/AB?q=AB%2FCD#CASE"))),
                "reader selected address was uppercased"
            );
        }
    }
    #[test]
    fn long_reader_destination_is_inspectable_by_scrolling() {
        let make = |tail: &str| {
            let target = format!("https://example.org/{}?q={tail}", "aB".repeat(990));
            let mut world = crate::web::Page::html(
                crate::web::Address::parse("https://example.org/Read").unwrap(),
                format!("<a href='{target}'>Next</a>").as_bytes(),
            )
            .unwrap()
            .generate()
            .unwrap()
            .world;
            world.reading = true;
            world.cycle_reader_link(false);
            world
        };
        let a = make("aB%2Fcd#Case");
        let b = make("AB%2FCD#CASE");
        for (w, h) in [(960, 640), (640, 480)] {
            let all_body = |world: &crate::World| {
                (0..40)
                    .flat_map(|scroll| {
                        super::overlay(world, w, h, scroll)
                            .into_iter()
                            .filter(|v| v.color == [0.9, 0.96, 0.91])
                    })
                    .collect::<Vec<_>>()
            };
            assert_ne!(
                bytemuck::cast_slice::<_, u8>(&all_body(&a)),
                bytemuck::cast_slice::<_, u8>(&all_body(&b)),
                "truncated reader link tail is not inspectable"
            );
        }
    }
    #[test]
    fn source_header_does_not_uppercase_url_paths() {
        let mut world = crate::web::Page::html(
            crate::web::Address::parse("https://example.org/aB").unwrap(),
            b"<p>read</p>",
        )
        .unwrap()
        .generate()
        .unwrap()
        .world;
        let before = super::overlay(&world, 960, 640, 0);
        world.current = "https://example.org/AB".into();
        let after = super::overlay(&world, 960, 640, 0);
        assert_ne!(
            bytemuck::cast_slice::<_, u8>(&before),
            bytemuck::cast_slice::<_, u8>(&after)
        );
    }
    #[test]
    fn full_entry_is_case_preserving_and_bounded_at_small_viewport() {
        let raw = format!("https://example.org/{}", "aB".repeat(1014));
        let vertices = super::address_overlay(&raw, Some("Refused; edit or Escape"), 640, 480);
        assert!(!vertices.is_empty());
        assert!(vertices.iter().all(|v| {
            v.position
                .iter()
                .all(|x| x.is_finite() && (-1.0..=1.0).contains(x))
        }));
        assert_ne!(
            bytemuck::cast_slice::<_, u8>(&super::address_overlay("aB", None, 640, 480)),
            bytemuck::cast_slice::<_, u8>(&super::address_overlay("AB", None, 640, 480))
        );
    }
}
/// Literal bounded labels on local role objects, in shared renderer world space.
/// Labels do not affect collision, reach, or activation identity.
pub fn object_label(text: &str, at: [f32; 3], max_columns: usize) -> Vec<Vertex> {
    let text = bounded_line(text, max_columns);
    let step = 0.012;
    let left = at[0] - text.len() as f32 * 6.0 * step * 0.5;
    let mut vertices = Vec::new();
    for (i, c) in text.chars().enumerate() {
        for (row, bits) in glyph(c).iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) == 0 {
                    continue;
                }
                for (dx, dy) in [
                    (0.0, 0.0),
                    (1.0, 0.0),
                    (1.0, 1.0),
                    (0.0, 0.0),
                    (1.0, 1.0),
                    (0.0, 1.0),
                ] {
                    vertices.push(Vertex {
                        position: [
                            left + (i as f32 * 6.0 + col as f32 + dx) * step,
                            at[1] - (row as f32 + dy) * step,
                            at[2],
                        ],
                        color: [0.6, 1.0, 0.7],
                        uv: [0.0; 2],
                        texture: 0,
                    });
                }
            }
        }
    }
    vertices
}
/// Fixed-cost literal document surface, using the same case-preserving 5x7 font
/// as object labels. Input is the raw reader's escaped ASCII, NOT markup/links.
/// 16 columns x 3 body lines (48 glyphs) at 6x for a larger glanceable prefix;
/// header/footer stay 2x. Fixed 592x240 opaque RGBA = 568320 bytes.
/// Overflow is disclosed on the texture, never parsed; E retains full reading.
pub fn literal_excerpt_texture(escaped: &str, prefix_truncated: bool) -> crate::assets::Texture {
    const WIDTH: usize = 592;
    const HEIGHT: usize = 240;
    const COLUMNS: usize = 16;
    const LINES: usize = 3;
    let mut rgba = [242, 246, 235, 255].repeat(WIDTH * HEIGHT);
    let mut draw = |c: char, x: usize, y: usize, scale: usize| {
        for (row, bits) in glyph(c).iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let at = ((y + row * scale + dy) * WIDTH + x + col * scale + dx) * 4;
                            rgba[at..at + 4].copy_from_slice(&[20, 42, 25, 255]);
                        }
                    }
                }
            }
        }
    };
    for (col, c) in "TEXT: literal prefix".chars().enumerate() {
        draw(c, 8 + col * 12, 8, 2);
    }
    let (mut row, mut col) = (0, 0);
    let mut truncated = prefix_truncated;
    for c in escaped.chars() {
        if row >= LINES {
            truncated = true;
            break;
        }
        if c == '\n' {
            row += 1;
            col = 0;
            continue;
        }
        if col == COLUMNS {
            row += 1;
            col = 0;
        }
        if row >= LINES {
            truncated = true;
            break;
        }
        draw(c, 8 + col * 36, 32 + row * 54, 6);
        col += 1;
    }
    let footer = if truncated {
        "[truncated; E opens reader]"
    } else {
        "[E opens reader]"
    };
    for (col, c) in footer.chars().enumerate() {
        draw(c, 8 + col * 12, 218, 2);
    }
    crate::assets::Texture {
        width: WIDTH as u32,
        height: HEIGHT as u32,
        rgba,
    }
}
pub(crate) struct Painter {
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) width: f32,
    pub(crate) height: f32,
}
impl Painter {
    pub(crate) fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 3]) {
        for (dx, dy) in [(0.0, 0.0), (w, 0.0), (w, h), (0.0, 0.0), (w, h), (0.0, h)] {
            self.vertices.push(Vertex {
                position: [
                    2.0 * (x + dx) / self.width - 1.0,
                    1.0 - 2.0 * (y + dy) / self.height,
                    0.0,
                ],
                color,
                uv: [0.0; 2],
                texture: 0,
            });
        }
    }
    fn text(&mut self, x: f32, y: f32, text: &str, scale: f32, color: [f32; 3]) {
        self.literal_text(x, y, &text.to_ascii_uppercase(), scale, color);
    }
    pub(crate) fn literal_text(&mut self, x: f32, y: f32, text: &str, scale: f32, color: [f32; 3]) {
        for (i, c) in text.chars().enumerate() {
            for (row, bits) in glyph(c).iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        self.rect(
                            x + (i as f32 * 6.0 + col as f32) * scale,
                            y + row as f32 * scale,
                            scale,
                            scale,
                            color,
                        );
                    }
                }
            }
        }
    }
}
// Original tiny 5x7 raster alphabet: intentional ASCII-only reader, no OS fonts/UI stack.
fn glyph(c: char) -> [u8; 7] {
    match c {
        '…' => [0, 0, 0, 0, 0, 21, 0],
        'a' => [0, 0, 14, 1, 15, 17, 15],
        'b' => [16, 16, 30, 17, 17, 17, 30],
        'c' => [0, 0, 14, 16, 16, 17, 14],
        'd' => [1, 1, 15, 17, 17, 17, 15],
        'e' => [0, 0, 14, 17, 31, 16, 14],
        'f' => [6, 8, 8, 28, 8, 8, 8],
        'g' => [0, 0, 15, 17, 15, 1, 14],
        'h' => [16, 16, 30, 17, 17, 17, 17],
        'i' => [4, 0, 12, 4, 4, 4, 14],
        'j' => [2, 0, 6, 2, 2, 18, 12],
        'k' => [16, 16, 18, 20, 24, 20, 18],
        'l' => [12, 4, 4, 4, 4, 4, 14],
        'm' => [0, 0, 26, 21, 21, 21, 21],
        'n' => [0, 0, 30, 17, 17, 17, 17],
        'o' => [0, 0, 14, 17, 17, 17, 14],
        'p' => [0, 0, 30, 17, 30, 16, 16],
        'q' => [0, 0, 15, 17, 15, 1, 1],
        'r' => [0, 0, 22, 25, 16, 16, 16],
        's' => [0, 0, 15, 16, 14, 1, 30],
        't' => [8, 8, 28, 8, 8, 9, 6],
        'u' => [0, 0, 17, 17, 17, 19, 13],
        'v' => [0, 0, 17, 17, 17, 10, 4],
        'w' => [0, 0, 17, 17, 21, 21, 10],
        'x' => [0, 0, 17, 10, 4, 10, 17],
        'y' => [0, 0, 17, 17, 15, 1, 14],
        'z' => [0, 0, 31, 2, 4, 8, 31],
        '%' => [25, 25, 2, 4, 8, 19, 19],
        '&' => [12, 18, 20, 8, 21, 18, 13],
        '@' => [14, 17, 23, 21, 23, 16, 14],
        '$' => [4, 15, 20, 14, 5, 30, 4],
        '^' => [4, 10, 17, 0, 0, 0, 0],
        '~' => [0, 0, 9, 22, 0, 0, 0],
        '`' => [8, 4, 0, 0, 0, 0, 0],
        '{' => [2, 4, 4, 8, 4, 4, 2],
        '}' => [8, 4, 4, 2, 4, 4, 8],
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ',' => [0, 0, 0, 0, 0, 4, 8],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        ';' => [0, 12, 12, 0, 4, 4, 8],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        '\\' => [16, 8, 8, 4, 2, 2, 1],
        '#' => [10, 31, 10, 10, 31, 10, 0],
        '|' => [4, 4, 4, 4, 4, 4, 4],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '[' => [14, 8, 8, 8, 8, 8, 14],
        ']' => [14, 2, 2, 2, 2, 2, 14],
        '!' => [4, 4, 4, 4, 4, 0, 4],
        '\'' => [4, 4, 0, 0, 0, 0, 0],
        '"' => [10, 10, 0, 0, 0, 0, 0],
        '*' => [0, 21, 14, 31, 14, 21, 0],
        '=' => [0, 31, 0, 31, 0, 0, 0],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '<' => [2, 4, 8, 16, 8, 4, 2],
        '>' => [8, 4, 2, 1, 2, 4, 8],
        ' ' => [0; 7],
        _ => [14, 17, 1, 2, 4, 0, 4],
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn wrapped_row_count_matches_overlay_chunks_including_empty_lines() {
        for document in [
            "",
            "\n",
            "\n\n",
            "a\n\nb\n",
            "abcdef",
            "é🙂漢字\n\nTail",
            "ab\r\n\r\ncd\r\n",
            "\r",
        ] {
            for columns in [1, 2, 3, 7] {
                // Reference the existing overlay's character-chunk policy.
                let expected: usize = document
                    .lines()
                    .map(|line| {
                        let chars: Vec<_> = line.chars().collect();
                        chars.chunks(columns).len().max(1)
                    })
                    .sum();
                assert_eq!(
                    super::wrapped_document_rows(document, columns),
                    expected,
                    "document={document:?}, columns={columns}"
                );
            }
        }
        assert_eq!(super::wrapped_document_rows("", 7), 0);
        assert_eq!(super::wrapped_document_rows("\n\n", 7), 2);
    }
    fn pixel_bounds(vertices: &[crate::assets::Vertex], width: u32, height: u32) -> [f32; 4] {
        assert!(!vertices.is_empty());
        let mut b = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for v in vertices {
            let x = (v.position[0] + 1.0) * 0.5 * width as f32;
            let y = (1.0 - v.position[1]) * 0.5 * height as f32;
            assert!(x.is_finite() && y.is_finite());
            b[0] = b[0].min(x);
            b[1] = b[1].min(y);
            b[2] = b[2].max(x);
            b[3] = b[3].max(y);
        }
        b
    }
    #[test]
    fn reader_tree_header_never_covers_document_in_either_collapse_state() {
        let generic = crate::World::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let snapshot = crate::local_explorer::Snapshot {
            folder: "/Fixture/MiXeD-Source".into(),
            entries: Vec::new(),
        };
        let (mut local, _) = crate::folder_scene::build(&snapshot, 0, false).unwrap();
        local.folder_view.as_mut().unwrap().reader = "H MiXeD source row\n".repeat(200);
        for mut w in [generic, local] {
            w.reading = true;
            for collapsed in [false, true] {
                w.tree_collapsed = collapsed;
                for (width, height) in [(960, 640), (640, 480)] {
                    let overlay = super::overlay(&w, width, height, 0);
                    let panel = super::tree_overlay(&w, width, height);
                    let tree_bounds = pixel_bounds(&panel[..6], width, height);
                    let body: Vec<_> = overlay
                        .iter()
                        .filter(|v| v.color == [0.9, 0.96, 0.91])
                        .copied()
                        .collect();
                    assert!(!body.is_empty());
                    for quad in body.as_chunks::<6>().0 {
                        let b = pixel_bounds(quad, width, height);
                        assert!(b[0] >= 0.0 && b[2] <= width as f32 + 0.01);
                        assert!(
                            b[0] > tree_bounds[2] + 0.01 || b[1] > tree_bounds[3] + 0.01,
                            "reader ink under actual tree panel: collapsed={collapsed} ink={b:?} tree={tree_bounds:?}"
                        );
                        assert!(b[3] < height as f32 - 82.0, "body enters reader footer");
                    }
                    // Check reader-owned footer ink before the appended tree;
                    // its current-row color must not be mistaken for footer ink.
                    let tree_start = overlay.iter().position(|v| v.color == [0.012; 3]).unwrap();
                    let mut last_bottom = height as f32 - 82.0;
                    for color in [[0.6, 1.0, 0.7], [0.6, 0.9, 0.7], [0.4, 0.95, 0.6]] {
                        let ink: Vec<_> = overlay[..tree_start]
                            .iter()
                            .filter(|v| v.color == color)
                            .copied()
                            .collect();
                        // SPACE uses the first color too; isolate the footer.
                        let ink: Vec<_> = ink
                            .as_chunks::<6>()
                            .0
                            .iter()
                            .filter(|q| {
                                pixel_bounds(q.as_slice(), width, height)[1] >= height as f32 - 82.0
                            })
                            .flatten()
                            .copied()
                            .collect();
                        let b = pixel_bounds(&ink, width, height);
                        assert!(b[0] >= 0.0 && b[2] <= width as f32 + 0.01);
                        assert!(b[1] > last_bottom && b[3] < height as f32);
                        last_bottom = b[3];
                    }
                }
            }
        }
    }
    #[test]
    fn generated_web_has_no_navigation_tree_chrome() {
        let g = crate::web::Page::html(
            crate::web::Address::parse("https://example.org/a").unwrap(),
            b"<p>Read</p>",
        )
        .unwrap()
        .generate()
        .unwrap();
        // Narrow 0.4.1 migration: the old oracle asserted an empty panel for
        // https:// (the text.rs HTTPS suppression policy). Web rooms now draw
        // the M-collapsible links/history graph instead. Still no filesystem
        // or site outline: no hierarchy rows exist and the default collapsed
        // panel is a header only, with no outline/link rows.
        assert!(g.world.navigation.rows.is_empty());
        assert!(g.world.tree_collapsed);
        let panel = super::tree_overlay(&g.world, 960, 640);
        assert!(!panel.is_empty());
        assert!(!panel.iter().any(|v| v.color == super::WEB_LINK));
        assert!(!panel.iter().any(|v| v.color == [0.6, 1.0, 0.7]));
    }
    #[test]
    fn navigation_hud_is_clipped_current_windowed_and_collapse_visible() {
        let mut w = crate::World::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let expanded = super::tree_overlay(&w, 960, 640);
        assert!(!expanded.is_empty());
        assert!(
            expanded.iter().any(|v| v.color == [0.6, 1.0, 0.7]),
            "current page highlighted"
        );
        w.tree_collapsed = true;
        let collapsed = super::tree_overlay(&w, 960, 640);
        assert!(collapsed.len() < expanded.len());
        w.tree_collapsed = false;
        let ids: Vec<_> = (0..32)
            .map(|i| format!("page-{i}-very-long-same-prefix-identity.md"))
            .collect();
        w.navigation = crate::navigation::Navigation::flat(&ids);
        w.current = ids[31].clone();
        let panel = super::tree_overlay(&w, 640, 480);
        assert!(
            panel.iter().any(|v| v.color == [0.6, 1.0, 0.7]),
            "window keeps actual current visible"
        );
        assert!(panel.len() < 20000, "bounded raster work");
        for (width, height) in [
            (960, 640),
            (640, 480),
            (320, 240),
            (160, 120),
            (1, 1),
            (0, 0),
        ] {
            for v in super::tree_overlay(&w, width, height) {
                assert!(
                    (-1.0..=1.0).contains(&v.position[0]) && (-1.0..=1.0).contains(&v.position[1])
                );
                let x = (v.position[0] + 1.0) * width as f32 * 0.5;
                let y = (1.0 - v.position[1]) * height as f32 * 0.5;
                assert!(x <= width as f32 * 0.36 + 1.0);
                assert!(y >= 75.0 && y < height as f32 * 0.5 - 12.0);
            }
        }
        w.current = "absent".into();
        assert!(
            !super::tree_overlay(&w, 960, 640)
                .iter()
                .any(|v| v.color == [0.6, 1.0, 0.7])
        );
    }
    #[test]
    fn reader_draws_actual_page_text_not_just_window_title() {
        let mut world =
            crate::World::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        world.reading = true;
        let welcome = super::overlay(&world, 960, 640, 0);
        assert!(welcome.len() > 1000);
        world.current = "workshop".into();
        let workshop = super::overlay(&world, 960, 640, 0);
        assert_ne!(
            bytemuck::cast_slice::<_, u8>(&welcome),
            bytemuck::cast_slice::<_, u8>(&workshop)
        );
        assert!(super::overlay(&world, 0, 0, 0).is_empty());
    }
}
