//! File-only rendered tracer. Parent owns compiled behavioral RED -> text fix -> GREEN.
//! Public compositor bitmap vertices, NOT framebuffer/native visual acceptance.
use hair::{
    World,
    assets::Vertex,
    folder_scene,
    local_explorer::{Entry, Kind, Snapshot},
    text,
};
use std::collections::BTreeMap;

const PANEL: [f32; 3] = [0.012; 3];
const ORDINARY: [f32; 3] = [0.85; 3];
const CURRENT: [f32; 3] = [0.6, 1.0, 0.7];
const SOURCE: [f32; 3] = [0.5, 1.0, 0.65];

fn key(v: &Vertex) -> [u32; 9] {
    [
        v.position[0].to_bits(),
        v.position[1].to_bits(),
        v.position[2].to_bits(),
        v.color[0].to_bits(),
        v.color[1].to_bits(),
        v.color[2].to_bits(),
        v.uv[0].to_bits(),
        v.uv[1].to_bits(),
        v.texture,
    ]
}
fn colored(ui: &[Vertex], color: [f32; 3]) -> Vec<Vertex> {
    ui.iter().filter(|v| v.color == color).copied().collect()
}
fn tree(w: &World, size: [u32; 2]) -> Vec<Vertex> {
    let ui = text::overlay(w, size[0], size[1], 0);
    let start = ui
        .iter()
        .position(|v| v.color == PANEL)
        .expect("tree panel");
    let panel = &ui[start..start + 6];
    let b = bounds(panel, size);
    for v in &ui[start..] {
        let [x, y] = pixel(v, size);
        assert!(x >= b[0] - 0.01 && x <= b[2] + 0.01);
        assert!(y >= b[1] - 0.01 && y <= b[3] + 0.01);
    }
    ui[start..].to_vec()
}
fn pixel(v: &Vertex, [w, h]: [u32; 2]) -> [f32; 2] {
    assert!(v.position.iter().all(|v| v.is_finite()));
    [
        (v.position[0] + 1.0) * 0.5 * w as f32,
        (1.0 - v.position[1]) * 0.5 * h as f32,
    ]
}
fn bounds(ink: &[Vertex], size: [u32; 2]) -> [f32; 4] {
    assert!(!ink.is_empty(), "missing bitmap ink");
    let mut b = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for v in ink {
        let [x, y] = pixel(v, size);
        assert!(
            x >= -0.01 && y >= -0.01 && x <= size[0] as f32 + 0.01 && y <= size[1] as f32 + 0.01
        );
        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    b
}
// Exact multiplicity subtraction after panel isolation, as in the glyph-control
// recipe. An unchanged technical [id] suffix cannot contribute label evidence.
fn difference(actual: &[Vertex], blank: &[Vertex], color: [f32; 3]) -> Vec<Vertex> {
    let mut counts = BTreeMap::new();
    for v in blank.iter().filter(|v| v.color == color) {
        *counts.entry(key(v)).or_insert(0_usize) += 1;
    }
    actual
        .iter()
        .filter(|v| v.color == color)
        .filter_map(|v| {
            if let Some(n) = counts.get_mut(&key(v))
                && *n > 0
            {
                *n -= 1;
                None
            } else {
                Some(*v)
            }
        })
        .collect()
}
fn normalized(ink: &[Vertex], size: [u32; 2]) -> Vec<[i32; 2]> {
    if ink.is_empty() {
        return Vec::new();
    }
    let b = bounds(ink, size);
    ink.iter()
        .map(|v| {
            let [x, y] = pixel(v, size);
            [
                ((x - b[0]) * 100.0).round() as i32,
                ((y - b[1]) * 100.0).round() as i32,
            ]
        })
        .collect()
}
fn literal(s: &str, size: [u32; 2]) -> Vec<[i32; 2]> {
    normalized(
        &colored(&text::pack_overlay(s, size[0], size[1]), ORDINARY),
        size,
    )
}
fn label_ink(w: &mut World, row: usize, size: [u32; 2]) -> Vec<Vertex> {
    let actual = tree(w, size);
    let saved = w.navigation.rows[row].label.clone();
    // A SHORT blank avoids a generated truncation marker in the control, which
    // would otherwise cancel the very leading-ellipsis glyph under test.
    w.navigation.rows[row].label = " ".into();
    let blank = tree(w, size);
    w.navigation.rows[row].label = saved;
    let color = if w.navigation.rows[row].id == w.current {
        CURRENT
    } else {
        ORDINARY
    };
    difference(&actual, &blank, color)
}
// pack_overlay is always 2x; the existing closed 640px source header is 1x.
// Widen only the REFERENCE canvas so pack_overlay cannot truncate a 1x-header
// candidate to its own 2x budget. Keep every glyph vertex and its multiplicity.
fn literal_at_scale(s: &str, size: [u32; 2], scale: u32) -> Vec<[i32; 2]> {
    assert!(scale == 1 || scale == 2);
    let reference = [size[0].max(s.chars().count() as u32 * 12 + 24), size[1]];
    literal(s, reference)
        .into_iter()
        .map(|p| p.map(|v| v * scale as i32 / 2))
        .collect()
}
fn tail_match(ink: &[Vertex], full: &str, tail: &str, size: [u32; 2], scale: u32) -> bool {
    let actual = normalized(ink, size);
    let mut closest = None;
    let mut best = usize::MAX;
    // Do not prescribe a new helper/API or an exact column cap. Accept the
    // established visible '~' ellipsis convention or three literal dots, but
    // require a complete leading marker AND the distinguishing literal tail.
    for marker in ["~", "..."] {
        let columns = (size[0] as usize - 24) / (6 * scale as usize);
        for n in tail.chars().count()
            ..=full
                .chars()
                .count()
                .min(columns.saturating_sub(marker.len()))
        {
            let suffix: String = full.chars().skip(full.chars().count() - n).collect();
            // escape_display wraps names containing spaces in literal quotes.
            // Require the complete distinguishing tail and its exact closing
            // delimiter, rather than rejecting correctly preserved OS display.
            let quoted_tail = format!("{tail}\"");
            if !suffix.ends_with(tail) && !(full.ends_with('\"') && suffix.ends_with(&quoted_tail))
            {
                continue;
            }
            let candidate = format!("{marker}{suffix}");
            // Keep the original 2x row oracle unchanged. Only the demonstrably
            // mismatched 1x header needs a differently scaled reference.
            let expected = if scale == 2 {
                literal(&candidate, size)
            } else {
                literal_at_scale(&candidate, size, scale)
            };
            if actual == expected {
                return true;
            }
            let unequal = actual.iter().zip(&expected).filter(|(a, b)| a != b).count()
                + actual.len().abs_diff(expected.len());
            if unequal < best {
                best = unequal;
                closest = Some((candidate, expected));
            }
        }
    }
    if let Some((candidate, expected)) = closest {
        let mismatch = actual.iter().zip(&expected).position(|(a, b)| a != b);
        let mut actual_bag = actual.clone();
        let mut expected_bag = expected.clone();
        actual_bag.sort_unstable();
        expected_bag.sort_unstable();
        println!(
            "LOCAL_NAME_TAIL_DIAGNOSTIC size={size:?} scale={scale} full={full:?} tail={tail:?} actual_vertices={} actual_mod6={} bounds={:?} closest={candidate:?} expected_vertices={} unequal={best} first_mismatch={mismatch:?} actual_point={:?} expected_point={:?} same_vertex_multiset={}",
            actual.len(),
            actual.len() % 6,
            bounds(ink, size),
            expected.len(),
            mismatch.and_then(|i| actual.get(i)),
            mismatch.and_then(|i| expected.get(i)),
            actual_bag == expected_bag
        );
    }
    false
}
fn fixture(prefix: &str) -> World {
    let folder = format!("/{prefix}/CURRENT-{}-FolderTail", "ancestry-".repeat(12));
    let names = [
        "Child024".to_string(),
        "Child025".to_string(),
        format!("{}ending024.txt", "Shared production notes ".repeat(5)),
        format!("{}ending025.txt", "Shared production notes ".repeat(5)),
    ];
    let snapshot = Snapshot {
        folder: folder.clone().into(),
        entries: names
            .into_iter()
            .enumerate()
            .map(|(i, name)| Entry {
                path: format!("{folder}/{name}").into(),
                name: name.into(),
                kind: if i < 2 { Kind::Folder } else { Kind::File },
            })
            .collect(),
    };
    folder_scene::build(&snapshot, 0, false).unwrap().0
}

#[test]
fn local_names_own_bitmap_columns_and_keep_distinguishing_tails_at_both_sizes() {
    let mut failures = Vec::new();
    for size in [[960, 640], [640, 480]] {
        // Same basename/contents, drastically different ancestor/source IDs.
        let mut short = fixture("S");
        let mut long = fixture(&"different-raw-source-ancestor/".repeat(18));
        let saved = (
            short.current.clone(),
            short.graph.start.clone(),
            short.navigation.clone(),
            short.folder_view.clone(),
            short.document().unwrap().to_string(),
        );
        assert_eq!(
            short.navigation.rows.len(),
            5,
            "current plus all four nearby entries"
        );
        let mut pair_ink = Vec::new();
        for row in 0..5 {
            let name = short.navigation.rows[row].label.clone();
            assert_eq!(long.navigation.rows[row].label, name);
            assert_ne!(short.navigation.rows[row].id, long.navigation.rows[row].id);
            let a = label_ink(&mut short, row, size);
            let b = label_ink(&mut long, row, size);
            let old_id = long.navigation.rows[row].id.clone();
            long.navigation.rows[row].id = format!("x{row}");
            if row == 0 {
                long.current = format!("x{row}");
            }
            let tiny_id = label_ink(&mut long, row, size);
            long.navigation.rows[row].id = old_id.clone();
            if row == 0 {
                long.current = old_id;
            }
            if a.is_empty() {
                failures.push(format!("{size:?} row{row}: nearby name is not visible"));
                continue;
            }
            // Absolute positions, not just shape: raw-source length may not
            // steal or shift a single local-name column.
            let absolute = |ink: &[Vertex]| {
                ink.iter()
                    .map(|v| {
                        let [x, y] = pixel(v, size);
                        [(x * 100.0).round() as i32, (y * 100.0).round() as i32]
                    })
                    .collect::<Vec<_>>()
            };
            if absolute(&a) != absolute(&b) || absolute(&a) != absolute(&tiny_id) {
                failures.push(format!(
                    "{size:?} row{row}: source-ID variation stole label columns"
                ));
            }
            let good = match row {
                1 | 2 => normalized(&a, size) == literal(&name, size),
                0 => tail_match(&a, &name, "FolderTail", size, 2),
                3 => tail_match(&a, &name, "024.txt", size, 2),
                4 => tail_match(&a, &name, "025.txt", size, 2),
                _ => unreachable!(),
            };
            if !good {
                // Diagnostic only, NOT an alternative passing oracle. Re-render
                // the predicted bounded string through the same real row. This
                // separates a tail-selection defect from a reference/subtraction
                // defect without dropping literal-marker/tail qualification.
                let panel_width = (size[0] as f32 * 0.32).min(260.0);
                let columns = ((panel_width - 12.0) / 12.0) as usize;
                let prefix = 4 + short.navigation.rows[row].depth.min(4);
                let available = columns.saturating_sub(prefix);
                let suffix: String = name
                    .chars()
                    .skip(
                        name.chars()
                            .count()
                            .saturating_sub(available.saturating_sub(1)),
                    )
                    .collect();
                let candidate = format!("~{suffix}");
                short.navigation.rows[row].label = candidate.clone();
                let control = label_ink(&mut short, row, size);
                short.navigation.rows[row].label = name.clone();
                println!(
                    "LOCAL_NAME_COMPOSITOR_DIAGNOSTIC size={size:?} row={row} columns={columns} prefix={prefix} available={available} candidate={candidate:?} candidate_vertices={} full_equals_candidate={} candidate_equals_pack={} candidate_bounds={:?}",
                    control.len(),
                    absolute(&a) == absolute(&control),
                    normalized(&control, size) == literal(&candidate, size),
                    bounds(&control, size)
                );
            }
            if !good {
                failures.push(format!("{size:?} row{row}: label ink lost full short name or leading ellipsis/distinguishing tail"));
            }
            if (1..=4).contains(&row) {
                pair_ink.push(normalized(&a, size));
            }
            println!(
                "LOCAL_NAME_INK size={size:?} row={row} label_vertices={} bounds={:?} qualified={good}",
                a.len(),
                bounds(&a, size)
            );
        }
        if pair_ink.len() == 4 {
            if pair_ink[0] == pair_ink[1] {
                failures.push(format!("{size:?}: Child024/Child025 label ink identical"));
            }
            if pair_ink[2] == pair_ink[3] {
                failures.push(format!(
                    "{size:?}: same-prefix filename label ink identical"
                ));
            }
        }
        // Positive glyph-control sanity check uses the real local compositor.
        let label = short.navigation.rows[1].label.clone();
        short.navigation.rows[1].label = "H".into();
        let h = label_ink(&mut short, 1, size);
        assert_eq!(
            h.len(),
            17 * 6,
            "single-H control must isolate one complete bitmap glyph"
        );
        assert_eq!(normalized(&h, size), literal("H", size));
        short.navigation.rows[1].label = label;

        let source = colored(&text::overlay(&short, size[0], size[1], 0), SOURCE);
        let header_scale = if size[0] >= 800 && size[1] >= 500 {
            2
        } else {
            1
        };
        let header_columns = (size[0] as usize - 24) / (6 * header_scale as usize);
        // Verify the role's actual glyph scale rather than enlarging production
        // chrome to make a 2x reference pass. The 52px header/layout stays intact.
        let saved_current = short.current.clone();
        short.current = "H".into();
        let header_control = colored(&text::overlay(&short, size[0], size[1], 0), SOURCE);
        short.current = saved_current;
        assert_eq!(
            normalized(&header_control, size),
            literal_at_scale(
                &format!("HAIR {} | H", env!("CARGO_PKG_VERSION")),
                size,
                header_scale
            ),
            "source-header reference must match the existing role's scale"
        );
        println!(
            "LOCAL_NAME_HEADER_DIAGNOSTIC size={size:?} scale={header_scale} columns={header_columns} vertices={} bounds={:?}",
            source.len(),
            bounds(&source, size)
        );
        let full = format!("HAIR {} | {}", env!("CARGO_PKG_VERSION"), short.current);
        let app_prefix = format!("HAIR {} | ", env!("CARGO_PKG_VERSION"));
        let source_with_app_prefix = ["~", "..."].iter().any(|marker| {
            ("FolderTail".len()..=header_columns - app_prefix.len() - marker.len()).any(|n| {
                let suffix: String = short
                    .current
                    .chars()
                    .skip(short.current.chars().count() - n)
                    .collect();
                normalized(&source, size)
                    == literal_at_scale(
                        &format!("{app_prefix}{marker}{suffix}"),
                        size,
                        header_scale,
                    )
            })
        });
        if !tail_match(&source, &full, "FolderTail", size, header_scale) && !source_with_app_prefix
        {
            failures.push(format!(
                "{size:?}: breadcrumb needs leading ellipsis and literal current-path tail"
            ));
        }
        assert!(
            short.document().unwrap().contains(&short.current),
            "full literal source must remain inspectable, not only the bounded header"
        );
        assert_eq!(
            (
                short.current.clone(),
                short.graph.start.clone(),
                short.navigation.clone(),
                short.folder_view.clone(),
                short.document().unwrap().to_string()
            ),
            saved
        );
        short.tree_collapsed = true;
        let collapsed = tree(&short, size);
        assert!(
            colored(&collapsed, ORDINARY).is_empty() && colored(&collapsed, CURRENT).is_empty()
        );
        assert_eq!(short.current, saved.0);
        assert_eq!(short.navigation, saved.2);
    }
    assert!(
        failures.is_empty(),
        "rendered local-name priority (not data rows or technical-ID ink): {failures:#?}"
    );
}
