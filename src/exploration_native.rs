//! Commit-only map observation: no network, page retention or navigation authority.
use super::*;
use hair::exploration::{Connection, Map, RoomId};
pub(super) fn observe(
    map: &mut Map,
    world: &World,
    host: &hair::host_scene::HostScene,
    kind: Connection,
    refresh: bool,
) {
    let Some(pocket) = &world.pocket else {
        return;
    };
    let address = &world.current;
    let window = pocket.room.map_or(0, |r| r.0);
    let title = world
        .site
        .as_ref()
        .and_then(|s| s.pages.get(address))
        .and_then(|s| s.lines().next())
        .unwrap_or(address);
    let mut exits = Vec::new();
    if let Some(links) = world.site.as_ref().and_then(|s| s.records.get(address)) {
        for link in links {
            if let hair::content::Destination::External(target) = &link.destination {
                exits.push(RoomId::new(target, 0));
            }
        }
    }
    if let Some((index, previous, next)) = pocket.room {
        if previous && index > 0 {
            exits.push(RoomId::new(address, index - 1));
        }
        if next {
            exits.push(RoomId::new(address, index + 1));
        }
    }
    map.arrive(RoomId::new(address, window), title, exits, kind, refresh);
    if let Some(index) = map.current {
        map.rooms[index].layout = hair::exploration::RoomLayout::observe(host, world);
    }
}
