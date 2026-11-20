//! Shared prompt/action targeting. Host anchors, never appearance meshes, own identity.
use crate::{Pose, World, host_scene::HostScene};
use glam::Vec3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Read,
    Enter,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub anchor: String,
    pub kind: Kind,
    pub page: String,
    pub destination: Option<String>,
    pub prompt: String,
}
/// Explicit eligible identity wins; without one retain the original narrow aim contract.
pub fn resolve(host: &HostScene, pose: &Pose, world: &World) -> Option<Target> {
    resolve_with_windows(host, pose, world, &[])
}
/// App-owned typed bindings, never publisher text or an invented destination.
/// No replay, generation or acquisition is performed by targeting.
pub fn resolve_with_windows(
    host: &HostScene,
    pose: &Pose,
    world: &World,
    windows: &[crate::web::WindowDoor],
) -> Option<Target> {
    if let Some(selected) = &world.explicit_target
        && let Some(target) = candidates(host, pose, world, windows, 0.5)
            .into_iter()
            .find(|t| t == selected)
    {
        return Some(target);
    }
    candidates(host, pose, world, windows, 0.9)
        .into_iter()
        .next()
}
pub fn refresh(host: &HostScene, pose: &Pose, world: &mut World) {
    refresh_with_windows(host, pose, world, &[]);
}
pub fn refresh_with_windows(
    host: &HostScene,
    pose: &Pose,
    world: &mut World,
    windows: &[crate::web::WindowDoor],
) {
    if let Some(selected) = world.explicit_target.as_ref() {
        world.explicit_target = candidates(host, pose, world, windows, 0.5)
            .into_iter()
            .find(|t| t.anchor == selected.anchor && t.page == selected.page);
    }
    world.interaction = resolve_with_windows(host, pose, world, windows);
}
pub fn cycle(host: &HostScene, pose: &Pose, world: &mut World, reverse: bool) {
    cycle_with_windows(host, pose, world, reverse, &[]);
}
pub fn cycle_with_windows(
    host: &HostScene,
    pose: &Pose,
    world: &mut World,
    reverse: bool,
    windows: &[crate::web::WindowDoor],
) {
    let current = resolve_with_windows(host, pose, world, windows);
    // Generated portals cycle destinations INSIDE the aimed doorway. Reader mode
    // has no eligible world targets and is routed separately by the input adapter.
    if let Some(t) = &current
        && crate::web::portals::cycle(world, &t.anchor, reverse)
    {
        let next = candidates(host, pose, world, windows, 0.5)
            .into_iter()
            .find(|n| n.anchor == t.anchor);
        world.explicit_target = next.clone();
        world.interaction = next;
        return;
    }
    let mut eligible = candidates(host, pose, world, windows, 0.5);
    eligible.sort_by(|a, b| a.anchor.cmp(&b.anchor));
    let index = current
        .as_ref()
        .and_then(|t| eligible.iter().position(|e| e == t));
    let next = if eligible.is_empty() {
        None
    } else {
        let n = eligible.len();
        let i = match index {
            Some(i) if reverse => (i + n - 1) % n,
            Some(i) => (i + 1) % n,
            None if reverse => n - 1,
            None => 0,
        };
        Some(eligible[i].clone())
    };
    world.explicit_target = next.clone();
    world.interaction = next;
}
fn candidates(
    host: &HostScene,
    pose: &Pose,
    world: &World,
    windows: &[crate::web::WindowDoor],
    min_aim: f32,
) -> Vec<Target> {
    if world.reading
        || !pose
            .position
            .iter()
            .chain([&pose.yaw, &pose.pitch])
            .all(|v| v.is_finite())
    {
        return Vec::new();
    }
    let eye = Vec3::new(pose.position[0], 1.65, pose.position[1]);
    let direction = Vec3::new(
        pose.yaw.sin() * pose.pitch.cos(),
        pose.pitch.sin(),
        -pose.yaw.cos() * pose.pitch.cos(),
    );
    let walls = host.walls();
    let mut eligible = Vec::new();
    for anchor in host
        .anchors
        .iter()
        .filter(|a| matches!(a.role.as_str(), "document" | "portal"))
    {
        let centre = Vec3::from_array(anchor.at) + Vec3::Y * anchor.size[1] * 0.5;
        let delta = centre - eye;
        let reach = (pose.position[0] - anchor.at[0]).hypot(pose.position[1] - anchor.at[2]);
        let aim = delta.normalize_or_zero().dot(direction);
        if reach >= 1.7 || aim < min_aim {
            continue;
        }
        let occluded = host
            .geometry
            .iter()
            .chain(host.overhead.iter())
            .chain(walls.iter())
            .any(|g| segment_hits_box(eye, centre, g.at, g.size))
            || host
                .anchors
                .iter()
                .filter(|a| {
                    a.id != anchor.id && matches!(a.role.as_str(), "document" | "decoration")
                })
                .any(|a| segment_hits_box(eye, centre, a.at, a.size));
        if occluded {
            continue;
        }
        let (kind, destination, prompt) = if let Some(view) = &world.folder_view {
            if let Some(action) = view.continuations.get(&anchor.id) {
                if action.page(view).is_none() {
                    continue;
                }
                (Kind::Enter, None, format!("E - {}", action.label(view)))
            } else if view.parent.as_deref() == Some(anchor.id.as_str()) {
                (
                    Kind::Enter,
                    None,
                    format!("E - {}", crate::folder_scene::PARENT_LABEL),
                )
            } else {
                let Some(bound) = view.bindings.get(&anchor.id) else {
                    continue;
                };
                let folder = bound.entry.kind == crate::local_explorer::Kind::Folder;
                (
                    if folder { Kind::Enter } else { Kind::Read },
                    Some(crate::folder_scene::display(bound.entry.path.as_os_str())),
                    format!(
                        "E - {} {} [{}] | I source",
                        if folder { "Enter" } else { "Inspect" },
                        crate::folder_scene::display(&bound.entry.name),
                        crate::folder_scene::kind(&bound.entry)
                    ),
                )
            }
        } else if let Some(graph) = &world.spaces {
            let Ok(space) = graph.space(&world.current_space) else {
                continue;
            };
            if anchor.role == "document" {
                let Some(d) = space.documents.iter().find(|d| d.anchor == anchor.id) else {
                    continue;
                };
                (
                    Kind::Read,
                    Some(d.page.clone()),
                    format!("E - Read {}", d.page),
                )
            } else {
                let Some(d) = space.doors.iter().find(|d| d.anchor == anchor.id) else {
                    continue;
                };
                (
                    Kind::Enter,
                    Some(d.page.clone()),
                    format!("E - Enter {} / {}", d.space, d.page),
                )
            }
        } else if world.current.starts_with("https://") && anchor.binding == "explicit-door" {
            if let Some(binding) = windows.iter().find(|d| d.anchor == anchor.id) {
                (Kind::Enter, None, format!("E - {}", binding.label))
            } else if anchor.id == "return-history" {
                let prompt = match world.pocket.as_ref().and_then(|p| p.previous.as_ref()) {
                    Some(previous) => {
                        let title: String = previous
                            .title
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(64)
                            .collect();
                        format!(
                            "E - Back to {} | {} (visitor history)",
                            if title.trim().is_empty() {
                                "untitled page"
                            } else {
                                title.as_str()
                            },
                            previous.address
                        )
                    }
                    None => "E - Return unavailable: no previous page in visitor history".into(),
                };
                (Kind::Enter, None, prompt)
            } else {
                let Some(v) = crate::web::portals::view(world, &anchor.id) else {
                    continue;
                };
                let raw = v.destination;
                let origin = reqwest::Url::parse(&raw)
                    .ok()
                    .zip(reqwest::Url::parse(&world.current).ok())
                    .map(|(a, b)| a.origin() != b.origin())
                    .unwrap_or(false);
                (
                    Kind::Enter,
                    Some(raw.clone()),
                    format!(
                        "E - Follow {}/{} {}{} | {raw}",
                        v.index + 1,
                        v.count,
                        if origin { "[other origin] " } else { "" },
                        v.title
                    ),
                )
            }
        } else if anchor.role == "document" {
            let label = world
                .pocket
                .as_ref()
                .and_then(|p| p.documents.get(&anchor.id));
            let prompt = label
                .map(|d| format!("E - Read {}", d.title))
                .unwrap_or_else(|| "E - Read full page".into());
            (
                Kind::Read,
                None,
                if world.pocket.is_some() {
                    prompt
                } else {
                    "E - Read".into()
                },
            )
        } else {
            let id = if world.site.is_some() {
                match world.targets().get(world.selected).map(|l| &l.destination) {
                    Some(crate::content::Destination::Page(id)) => Some(id.clone()),
                    _ => None,
                }
            } else {
                world
                    .plan
                    .bindings
                    .iter()
                    .find(|b| b.page == world.current)
                    .map(|b| b.destination.clone())
            };
            let Some(id) = id else {
                continue;
            };
            let label: String = id.chars().filter(|c| !c.is_control()).take(32).collect();
            (Kind::Enter, Some(id), format!("E - Enter {label}"))
        };
        eligible.push((
            aim,
            reach,
            Target {
                anchor: anchor.id.clone(),
                kind,
                page: world.current.clone(),
                destination,
                prompt,
            },
        ));
    }
    eligible.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.anchor.cmp(&b.2.anchor))
    });
    eligible.into_iter().map(|(_, _, target)| target).collect()
}

// Slab intersection against bottom-centred host boxes, before the target centre.
// Fixed work: three axes per box; no mesh traversal or appearance dependency.
pub(crate) fn segment_hits_box(from: Vec3, to: Vec3, at: [f32; 3], size: [f32; 3]) -> bool {
    let min = Vec3::from_array(at) - Vec3::new(size[0] * 0.5, 0.0, size[2] * 0.5);
    let max = min + Vec3::from_array(size);
    let delta = to - from;
    let (mut enter, mut exit) = (0.0_f32, 1.0_f32);
    for axis in 0..3 {
        if delta[axis].abs() < 0.000001 {
            if from[axis] < min[axis] || from[axis] > max[axis] {
                return false;
            }
        } else {
            let a = (min[axis] - from[axis]) / delta[axis];
            let b = (max[axis] - from[axis]) / delta[axis];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
            if enter > exit {
                return false;
            }
        }
    }
    enter < 0.9999 && exit > 0.0001
}
