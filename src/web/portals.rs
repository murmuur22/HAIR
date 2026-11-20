//! Bounded grouped portals. Source links own authority; previews are only appearance.
use crate::{
    World,
    content::{Destination, Link},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub destination: String,
    pub recovered: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub destination: String,
    /// Publisher's link label, NOT a fetched destination-page title.
    pub title: String,
    pub index: usize,
    pub count: usize,
    pub last_failure: Option<String>,
    pub recovered: bool,
}
fn groups(world: &World) -> BTreeMap<String, Vec<Link>> {
    let mut out = BTreeMap::<String, Vec<Link>>::new();
    if !world.current.starts_with("https://") {
        return out;
    }
    // Portal authority belongs to the acquired page, not the currently open
    // reading object's filtered link list.
    let links = world
        .site
        .as_ref()
        .and_then(|s| s.records.get(&world.current));
    for link in links.into_iter().flatten().cloned() {
        let Destination::External(raw) = &link.destination else {
            continue;
        };
        let Ok(mut url) = reqwest::Url::parse(raw) else {
            continue;
        };
        url.set_fragment(None);
        out.entry(super::generate::anchor_id(url.as_str()))
            .or_default()
            .push(link);
    }
    for links in out.values_mut() {
        links.sort_by(|a, b| {
            let raw = |l: &Link| match &l.destination {
                Destination::External(s) => s.clone(),
                _ => String::new(),
            };
            raw(a).cmp(&raw(b)).then(a.label.cmp(&b.label))
        });
        links.dedup_by(|a, b| a.destination == b.destination);
    }
    out
}
pub fn view(world: &World, anchor: &str) -> Option<View> {
    let links = groups(world).remove(anchor)?;
    let choice = world.doorway_choices.get(anchor);
    let index = choice
        .and_then(|c| {
            links
                .iter()
                .position(|l| l.destination == Destination::External(c.destination.clone()))
        })
        .unwrap_or(0);
    let link = &links[index];
    let Destination::External(destination) = &link.destination else {
        return None;
    };
    Some(View {
        destination: destination.clone(),
        title: if link.label.trim().is_empty() {
            "Unlabelled link".into()
        } else {
            link.label.clone()
        },
        index,
        count: links.len(),
        last_failure: world
            .pocket
            .as_ref()
            .and_then(|p| p.failed_arrival.as_ref())
            .filter(|(target, _)| target == destination)
            .map(|(_, reason)| {
                reason
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(96)
                    .collect()
            }),
        recovered: choice.is_some_and(|c| c.recovered),
    })
}
pub fn cycle(world: &mut World, anchor: &str, reverse: bool) -> bool {
    let Some(v) = view(world, anchor) else {
        return false;
    };
    let links = groups(world).remove(anchor).unwrap();
    let i = if reverse {
        (v.index + v.count - 1) % v.count
    } else {
        (v.index + 1) % v.count
    };
    let Destination::External(destination) = &links[i].destination else {
        return false;
    };
    world.doorway_choices.insert(
        anchor.into(),
        Choice {
            destination: destination.clone(),
            recovered: false,
        },
    );
    true
}
/// Refresh preserves surviving full targets, not ordinal/source order. Explicit recovery
/// marks a removed selection; an empty group disappears while Return remains available.
pub fn retain(previous: &World, next: &mut World) -> usize {
    let mut removed = 0;
    for anchor in groups(previous).keys() {
        let old = view(previous, anchor).unwrap();
        let links = groups(next).remove(anchor).unwrap_or_default();
        let surviving = links
            .iter()
            .any(|l| l.destination == Destination::External(old.destination.clone()));
        if !surviving {
            removed += 1;
        }
        if let Some(v) = view(next, anchor) {
            next.doorway_choices.insert(
                anchor.clone(),
                Choice {
                    destination: if surviving {
                        old.destination
                    } else {
                        v.destination
                    },
                    recovered: !surviving || old.recovered,
                },
            );
        }
    }
    removed
}
/// An abstract URL fingerprint. Never advertised as an image of the destination.
pub fn fallback(destination: &str) -> [[f32; 3]; 64] {
    let h = Sha256::digest(destination.as_bytes());
    std::array::from_fn(|i| {
        let byte = h[i % 32];
        let on = (byte >> (i / 32)) & 1 != 0;
        if on {
            [
                0.18 + f32::from(h[0]) / 500.0,
                0.25 + f32::from(h[1]) / 500.0,
                0.18 + f32::from(h[2]) / 500.0,
            ]
        } else {
            [0.035, 0.055, 0.045]
        }
    })
}
