use super::{Door, Generated, Page, WindowAction, WindowDoor};
use crate::{
    Binding, Graph, Plan, Result, World,
    content::{Destination, Site},
    host_scene::{Anchor, HostScene, Wall, WallAperture},
    navigation::Navigation,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
pub(super) fn slot(resource: &str) -> usize {
    usize::from(Sha256::digest(resource.as_bytes())[0] % 8)
}
pub(super) fn anchor_id(resource: &str) -> String {
    format!("web-slot-{}", slot(resource))
}
/// Eight immutable physical link slots. Continuation has separate typed actions
/// and opposing side apertures, never fabricated HTTPS destinations.
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
    let mut groups = BTreeMap::<usize, (String, Vec<String>)>::new();
    for link in &page.links {
        if let Destination::External(raw) = &link.destination {
            let mut url = reqwest::Url::parse(raw).map_err(|e| e.to_string())?;
            url.set_fragment(None); // Resource placement only; full targets survive.
            let (representative, targets) = groups
                .entry(slot(url.as_str()))
                .or_insert_with(|| (url.to_string(), Vec::new()));
            if url.as_str() < representative.as_str() {
                *representative = url.to_string();
            }
            if !targets.contains(raw) {
                targets.push(raw.clone());
            }
        }
    }
    for (_, destinations) in groups.values_mut() {
        destinations.sort();
    }
    let mut host = HostScene::shared_default();
    host.version = 2;
    host.name = "Generated web room v3 (grouped fixed slots)".into();
    host.room = [-8.0, -3.75, 8.0, 3.75];
    host.anchors.retain(|a| a.role != "portal");
    for a in &mut host.anchors {
        if a.role == "document" {
            a.binding = "explicit-page".into();
        }
        if a.role == "image" {
            a.at = [-8.2, 0.7, 0.0];
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
    for (slot, (resource, destinations)) in groups {
        let anchor = anchor_id(&resource);
        let cross_origin = destinations.iter().any(|raw| {
            reqwest::Url::parse(raw).is_ok_and(|u| u.origin() != page.address.url.origin())
        });
        host.anchors.push(Anchor {
            id: anchor.clone(),
            role: "portal".into(),
            at: [-7.2 + slot as f32 * 1.8, 0.1, -4.0],
            size: [1.4, 2.5, 0.2],
            binding: "explicit-door".into(),
        });
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
            -8.5,
        ),
        (
            next,
            "web-next-window",
            "Next room — more content in same held response",
            Wall::Right,
            8.5,
        ),
    ] {
        if let Some(cursor) = cursor {
            host.anchors.push(Anchor {
                id: anchor.into(),
                role: "portal".into(),
                at: [x, 0.0, 2.0],
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
    host = HostScene::parse(&toml::to_string(&host).map_err(|e| e.to_string())?)?;
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
