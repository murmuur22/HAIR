//! Bounded orthogonal routing on the gutters between schematic rooms.
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
#[cfg(test)]
#[path = "route_paths_tests.rs"]
mod tests;
type Point = [i32; 2];
type Segment = (Point, Point);
pub(super) fn segment(a: Point, b: Point) -> Segment {
    if a < b { (a, b) } else { (b, a) }
}

/// Room centres occupy odd/odd lattice points; even rows/columns are reserved
/// gutters. The lattice is bounded by the viewport, independently of page data.
pub(super) fn route(
    start: Point,
    end: Point,
    extent: Point,
    rooms: &HashSet<Point>,
    used: &HashMap<Segment, u16>,
    captions: bool,
) -> Option<Vec<Point>> {
    let width = (extent[0] + 1) as usize;
    let count = width * (extent[1] + 1) as usize * 5;
    let index = |p: Point, d: usize| (p[1] as usize * width + p[0] as usize) * 5 + d;
    let point = |i: usize| [((i / 5) % width) as i32, ((i / 5) / width) as i32];
    let heuristic = |p: Point| (p[0].abs_diff(end[0]) + p[1].abs_diff(end[1])) * 10;
    let first = index(start, 4);
    let mut costs = vec![u32::MAX; count];
    let mut parents = vec![usize::MAX; count];
    let mut queue = BinaryHeap::new();
    costs[first] = 0;
    queue.push(Reverse((heuristic(start), 0, first)));
    while let Some(Reverse((_, cost, state))) = queue.pop() {
        if costs[state] != cost {
            continue;
        }
        let p = point(state);
        if p == end {
            let mut path = vec![p];
            let mut cursor = state;
            while cursor != first {
                cursor = parents[cursor];
                path.push(point(cursor));
            }
            path.reverse();
            return Some(path);
        }
        for (direction, delta) in [[-1, 0], [1, 0], [0, -1], [0, 1]].into_iter().enumerate() {
            let next = [p[0] + delta[0], p[1] + delta[1]];
            if next[0] < 0 || next[1] < 0 || next[0] > extent[0] || next[1] > extent[1] {
                continue;
            }
            if rooms.contains(&next) && next != end {
                continue;
            }
            // Labels sit below rooms: use side/top ports, never cut a caption.
            if captions && ((p == start && delta == [0, 1]) || (next == end && delta == [0, -1])) {
                continue;
            }
            let bend = u32::from(state % 5 != 4 && state % 5 != direction) * 4;
            let congestion = u32::from(*used.get(&segment(p, next)).unwrap_or(&0)) * 12;
            let new_cost = cost + 10 + bend + congestion;
            let next_state = index(next, direction);
            if new_cost < costs[next_state] {
                costs[next_state] = new_cost;
                parents[next_state] = state;
                queue.push(Reverse((new_cost + heuristic(next), new_cost, next_state)));
            }
        }
    }
    None
}
