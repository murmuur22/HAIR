use super::{Door, Generated, Page, WindowAction, WindowDoor};
use crate::{
    Binding, Graph, Plan, Result, World,
    content::{Destination, Site},
    host_scene::{Anchor, HostScene, Wall, WallAperture},
    navigation::Navigation,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
pub(super) fn anchor_id(resource: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(resource.as_bytes()));
    format!("web-slot-{}", &digest[..32])
}
/// One stable identity for each full resolved destination. Physical placement
/// is deterministic within the acquired room, not an eight-way hash bucket.
pub(super) fn build(page: Page) -> Result<Generated> {
    if page.links.len() > 64
        || page.text.len() > 16384
        || page.title.len() > 512
        || page.headings.len() > 64
        || page.images.len() > 16
        || page.sections.len() > 64
        || page
            .sections
            .iter()
            .try_fold(0usize, |n, section| n.checked_add(section.text.len()))
            .is_none_or(|n| n > 16384)
        || page
            .sections
            .iter()
            .any(|section| section.title.len() > 16384)
        || page.code.len() > super::MAX_CODE_SPANS
        || page.section_code.iter().map(Vec::len).sum::<usize>() > super::MAX_CODE_SPANS
    {
        return Err("generated page semantic budget exceeded".into());
    }
    // This also rejects public completeness/source fields detached from their
    // opaque session-bound room metadata, before advertising any navigation.
    let disclosure = page.window_disclosure()?;
    let previous = page.previous_cursor()?;
    let next = page.next_cursor()?;
    let id = page.address.original.clone();
    let mut groups = BTreeMap::<String, (String, Vec<String>)>::new();
    for link in &page.links {
        if let Destination::External(raw) = &link.destination {
            reqwest::Url::parse(raw).map_err(|e| e.to_string())?;
            groups
                .entry(raw.clone())
                .or_insert_with(|| (raw.clone(), vec![raw.clone()]));
        }
    }
    let count = groups.len();
    let mut host = HostScene::shared_default();
    host.version = 3;
    host.name = "Generated individual-door web room".into();
    // Content-density tiers extend the hallway axis independently. Bounds stay
    // within the existing host coordinate and route-work budgets.
    let length = match count {
        0..=8 => 7.5,
        9..=16 => 11.5,
        17..=32 => 19.5,
        _ => 26.0,
    };
    let side_count = count.saturating_sub(9).div_ceil(2);
    let length = f32::max(length, 6.0 + side_count as f32 * 2.4);
    let half_width = 8.0 + page.sections.len().saturating_sub(1).min(4) as f32;
    host.room = [-half_width, -length * 0.5, half_width, length * 0.5];
    host.anchors.retain(|a| a.role != "portal");
    for a in &mut host.anchors {
        if a.role == "document" {
            a.binding = "explicit-page".into();
        }
        if a.role == "image" {
            // Reserve the front corner before the first side opening; a
            // centre-wall image would span a doorway in dense hallways.
            a.at = [host.room[0] - 0.2, 0.7, host.room[1] + 0.8];
            a.size = [0.04, 1.0, 1.5];
        }
    }
    // Visitor-history return: a real opening in the rear wall, opposite the
    // open-front link doors, facing inward. Bound only to the typed history
    // action by the App; it never names a URL parent.
    host.anchors.push(Anchor {
        id: "return-history".into(),
        role: "portal".into(),
        at: [0.0, 0.0, host.room[3] + 0.5],
        size: [1.4, 2.1, 0.2],
        binding: "explicit-door".into(),
    });
    host.wall_apertures.push(WallAperture {
        anchor: "return-history".into(),
        wall: Wall::Rear,
        width: 1.6,
        height: 2.3,
    });
    let mut doors = Vec::new();
    for (index, (_, (resource, destinations))) in groups.into_iter().enumerate() {
        let anchor = anchor_id(&resource);
        let cross_origin = destinations.iter().any(|raw| {
            reqwest::Url::parse(raw).is_ok_and(|u| u.origin() != page.address.url.origin())
        });
        let (at, size, wall) = if index < 5 {
            (
                [-4.8 + index as f32 * 2.4, 0.1, host.room[1] - 0.25],
                [1.4, 2.5, 0.2],
                None,
            )
        } else if index < 9 {
            (
                [[-4.8, -2.4, 2.4, 4.8][index - 5], 0.0, host.room[3] + 0.5],
                [1.4, 2.1, 0.2],
                Some(Wall::Rear),
            )
        } else {
            let ordinal = index - 9;
            let wall = if ordinal.is_multiple_of(2) {
                Wall::Left
            } else {
                Wall::Right
            };
            let x = if wall == Wall::Left {
                host.room[0] - 0.5
            } else {
                host.room[2] + 0.5
            };
            (
                [x, 0.0, host.room[1] + 2.4 + (ordinal / 2) as f32 * 2.4],
                [0.2, 2.1, 1.4],
                Some(wall),
            )
        };
        host.anchors.push(Anchor {
            id: anchor.clone(),
            role: "portal".into(),
            at,
            size,
            binding: "explicit-door".into(),
        });
        if let Some(wall) = wall {
            host.wall_apertures.push(WallAperture {
                anchor: anchor.clone(),
                wall,
                width: 1.6,
                height: 2.3,
            });
        }
        doors.push(Door {
            anchor,
            resource,
            destinations,
            cross_origin,
        });
    }
    let room = disclosure.as_ref().map(|_| {
        (
            page.window.as_ref().map_or(0, |w| w.index()),
            previous.is_some(),
            next.is_some(),
        )
    });
    let mut window_doors = Vec::new();
    for (cursor, anchor, label, wall, x) in [
        (
            previous,
            "web-previous-window",
            "Previous room — same held response",
            Wall::Left,
            host.room[0] - 0.5,
        ),
        (
            next,
            "web-next-window",
            "Next room — more content in same held response",
            Wall::Right,
            host.room[2] + 0.5,
        ),
    ] {
        if let Some(cursor) = cursor {
            host.anchors.push(Anchor {
                id: anchor.into(),
                role: "portal".into(),
                at: [x, 0.0, host.room[3] - 1.6],
                size: [0.2, 2.1, 1.4],
                binding: "explicit-door".into(),
            });
            host.wall_apertures.push(WallAperture {
                anchor: anchor.into(),
                wall,
                width: 1.6,
                height: 2.3,
            });
            let action = if wall == Wall::Left {
                WindowAction::Previous(cursor)
            } else {
                WindowAction::Next(cursor)
            };
            window_doors.push(WindowDoor {
                anchor: anchor.into(),
                label: label.into(),
                action,
            });
        }
    }
    let mut pocket = super::pocket::populate(&page, &mut host, disclosure.as_deref());
    host = HostScene::parse_generated(&toml::to_string(&host).map_err(|e| e.to_string())?)?;
    let mut text = if let Some(disclosure) = &disclosure {
        format!("{}\n{}\n\n{}\n\n", page.title, id, disclosure)
    } else {
        format!("{}\n{}\n\n", page.title, id)
    };
    pocket.code = super::pocket::shifted(&page.code, &page.text, text.len());
    pocket.room = room;
    pocket.visited = vec![id.clone()];
    text.push_str(&page.text);
    if !page.warnings.is_empty() {
        text.push_str(&format!("\n\nLIMITS: {}", page.warnings.join("; ")));
    }
    let site = Site {
        root: std::path::PathBuf::new(),
        start: id.clone(),
        pages: BTreeMap::from([(id.clone(), text)]),
        links: BTreeMap::from([(id.clone(), Vec::new())]),
        records: BTreeMap::from([(id.clone(), page.links.clone())]),
        images: BTreeMap::new(),
        skipped: Vec::new(),
    };
    let world = World {
        folder_view: None,
        graph: Graph {
            version: 1,
            start: id.clone(),
            pages: vec![crate::Page {
                id: id.clone(),
                file: id.clone(),
                next: String::new(),
            }],
        },
        plan: Plan {
            version: 1,
            grid: 0.5,
            bindings: vec![Binding {
                page: id.clone(),
                document_role: "desk".into(),
                document: id.clone(),
                portal_role: "portal".into(),
                destination: String::new(),
            }],
        },
        current: id,
        reading: false,
        pocket: Some(pocket),
        documents: Default::default(),
        site: Some(site),
        selected: 0,
        interaction: None,
        explicit_target: None,
        doorway_choices: Default::default(),
        reader_link: None,
        navigation: Navigation::flat(&[]),
        tree_collapsed: true,
        spaces: None,
        current_space: String::new(),
    };
    Ok(Generated {
        page,
        world,
        host,
        doors,
        window_doors,
    })
}
