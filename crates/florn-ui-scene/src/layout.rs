//! The layout of the topology (RFC 005 rule 37): where each node and each hub is placed in
//! the world plane, from the view model alone, the same every time. Small topologies go on a
//! ring, larger ones on a grid; a lan's hub sits at the centre of its ports' nodes.
use florn_ui_model::topology::{Point, Topology};

/// Nodes on a ring up to this many; a grid beyond.
pub const RING_MAX: usize = 24;
/// The spacing of nodes on the grid and the ring's growth per node, in world units.
pub const SPACING: f32 = 60.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    /// One per node, in the topology's node order.
    pub nodes: Vec<Point>,
    /// One per link, in the topology's link order; a link between two ports has no hub.
    pub hubs: Vec<Option<Point>>,
    pub min: Point,
    pub max: Point,
}

impl Layout {
    pub fn of(t: &Topology) -> Self {
        let n = t.nodes.len();
        let mut nodes = Vec::with_capacity(n);
        if n <= RING_MAX {
            let r = if n <= 1 {
                0.0
            } else {
                (SPACING * n as f32 / std::f32::consts::TAU).max(SPACING)
            };
            for i in 0..n {
                let a = std::f32::consts::TAU * i as f32 / n.max(1) as f32
                    - std::f32::consts::FRAC_PI_2;
                nodes.push(Point {
                    x: r * a.cos(),
                    y: r * a.sin(),
                });
            }
        } else {
            let cols = (n as f32).sqrt().ceil().max(1.0) as usize;
            for i in 0..n {
                nodes.push(Point {
                    x: (i % cols) as f32 * SPACING,
                    y: (i / cols) as f32 * SPACING,
                });
            }
        }
        let hubs = t
            .links
            .iter()
            .map(|l| {
                if !l.is_hub() {
                    return None;
                }
                let ends: Vec<Point> = l
                    .ports
                    .iter()
                    .filter_map(|(a, _)| t.node_index(*a).and_then(|i| nodes.get(i).copied()))
                    .collect();
                if ends.is_empty() {
                    return None;
                }
                let k = ends.len() as f32;
                Some(Point {
                    x: ends.iter().map(|p| p.x).sum::<f32>() / k,
                    y: ends.iter().map(|p| p.y).sum::<f32>() / k,
                })
            })
            .collect();
        let mut min = Point {
            x: f32::MAX,
            y: f32::MAX,
        };
        let mut max = Point {
            x: f32::MIN,
            y: f32::MIN,
        };
        for p in &nodes {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
        if nodes.is_empty() {
            min = Point::default();
            max = Point::default();
        }
        Self {
            nodes,
            hubs,
            min,
            max,
        }
    }

    /// The two world points a flight on link `link` from its port `port` moves between: the
    /// sender's node, then the other end of a link or the hub of a lan.
    pub fn flight_ends(&self, t: &Topology, link: usize, port: u8) -> Option<(Point, Point)> {
        let l = t.links.get(link)?;
        let (from_actor, _) = *l.ports.get(usize::from(port))?;
        let from = self.nodes[t.node_index(from_actor)?];
        let to = match self.hubs.get(link).copied().flatten() {
            Some(h) => h,
            None => {
                let (other, _) = *l.ports.iter().find(|(a, _)| *a != from_actor)?;
                self.nodes[t.node_index(other)?]
            }
        };
        Some((from, to))
    }
}
