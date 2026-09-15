// Adapted from gxworks-agent connectivity.py (Apache-2.0); modified to use
// bounded pair checks, source indices and block-local nets. See LICENSES/gxworks-agent.txt.
use super::*;
use std::collections::BTreeMap;

fn on(p: Point, w: &StructuredWire) -> bool {
    (w.start.x == w.end.x
        && p.x == w.start.x
        && p.y >= w.start.y.min(w.end.y)
        && p.y <= w.start.y.max(w.end.y))
        || (w.start.y == w.end.y
            && p.y == w.start.y
            && p.x >= w.start.x.min(w.end.x)
            && p.x <= w.start.x.max(w.end.x))
}
fn root(parents: &mut [usize], mut i: usize) -> usize {
    while parents[i] != i {
        parents[i] = parents[parents[i]];
        i = parents[i];
    }
    i
}
fn join(parents: &mut [usize], a: usize, b: usize) {
    let a = root(parents, a);
    let b = root(parents, b);
    if a != b {
        parents[b.max(a)] = a.min(b);
    }
}
pub(super) fn connect(
    block: &StructuredBlock,
    limits: &StructuredLimits,
    checks: &mut u64,
) -> Result<Vec<StructuredNet>, GxwError> {
    let ports: Vec<_> = block
        .nodes
        .iter()
        .enumerate()
        .flat_map(|(n, node)| {
            node.ports
                .iter()
                .enumerate()
                .map(move |(p, port)| (PortRef { node: n, port: p }, port.position))
        })
        .collect();
    let p = ports.len() as u64;
    let w = block.wires.len() as u64;
    let cost = p
        .saturating_mul(w)
        .saturating_add(w.saturating_mul(w.saturating_sub(1)) / 2);
    *checks = checks.saturating_add(cost);
    check(
        "structured connection checks",
        *checks,
        limits.max_connection_checks,
    )?;
    let mut parents: Vec<_> = (0..ports.len() + block.wires.len()).collect();
    let mut points = BTreeMap::new();
    for (i, (_, point)) in ports.iter().enumerate() {
        if let Some(previous) = points.insert(*point, i) {
            join(&mut parents, previous, i);
        }
        for (j, wire) in block.wires.iter().enumerate() {
            if on(*point, wire) {
                join(&mut parents, i, ports.len() + j);
            }
        }
    }
    for (i, a) in block.wires.iter().enumerate() {
        for (j, b) in block.wires.iter().enumerate().take(i) {
            // Interior/interior crossings are not junctions. Endpoints and port points are.
            if on(a.start, b) || on(a.end, b) || on(b.start, a) || on(b.end, a) {
                join(&mut parents, ports.len() + i, ports.len() + j);
            }
        }
    }
    let mut groups: BTreeMap<usize, StructuredNet> = BTreeMap::new();
    for i in 0..parents.len() {
        let key = root(&mut parents, i);
        let net = groups.entry(key).or_insert_with(|| StructuredNet {
            index: 0,
            ports: vec![],
            wires: vec![],
        });
        if i < ports.len() {
            net.ports.push(ports[i].0.clone());
        } else {
            net.wires.push(i - ports.len());
        }
    }
    Ok(groups
        .into_values()
        .enumerate()
        .map(|(i, mut net)| {
            net.index = i;
            net
        })
        .collect())
}
