//! The scene (RFC 005 rule 37): what to draw, as lists of lines, circles, marks and labels in
//! screen pixels, each with a style, built from the view model for one instant. It depends
//! on the view model only and compiles and tests natively. The frames' animation needs no
//! update per event (NET-008): a flight is drawn between its ends at the progress its sent
//! tick and delay give for the instant asked, so a shell interpolates the instant between
//! the Host's batches and builds a scene per frame. The scene also answers what is under a
//! point, with the same layout it drew.
//!
//! Status is never colour alone (UI-008): a faulted actor is a cross and a link down is a
//! dashed line, besides their colours; the shell's accessible twin lists the same elements
//! from the `Description`.
#![forbid(unsafe_code)]

pub mod layout;

use florn_ui_model::topology::Point;
use florn_ui_model::{Selection, ViewModel};

pub use layout::Layout;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
    pub dashed: bool,
}

/// One batch of items that share a style. Coordinates are screen pixels.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// Line segments `[x0, y0, x1, y1]`.
    Lines {
        segments: Vec<[f32; 4]>,
        stroke: Stroke,
    },
    /// Filled circles of one radius.
    Circles {
        centers: Vec<Point>,
        radius: f32,
        fill: Color,
    },
    /// Crosses of one size: a faulted actor.
    Crosses {
        centers: Vec<Point>,
        size: f32,
        stroke: Stroke,
    },
    /// Text anchored at a point (centred horizontally, above the point).
    Labels {
        labels: Vec<(Point, String)>,
        font_px: f32,
        fill: Color,
    },
}

/// What the scene shows, in words for the canvas's description (§3.11).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Description {
    pub nodes: usize,
    pub links: usize,
    pub links_down: usize,
    pub faulted: usize,
    pub flights: usize,
    pub selected: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub items: Vec<Item>,
    pub description: Description,
    pub nodes_visible: usize,
}

pub const LINK: Color = Color(150, 160, 175, 255);
pub const LINK_DOWN: Color = Color(200, 60, 60, 255);
pub const HUB: Color = Color(120, 130, 145, 255);
pub const NODE: Color = Color(40, 90, 160, 255);
pub const NODE_ACTIVE: Color = Color(60, 140, 220, 255);
pub const NODE_DOWN: Color = Color(140, 140, 140, 255);
pub const NODE_SELECTED: Color = Color(220, 90, 30, 255);
pub const FAULT: Color = Color(200, 40, 40, 255);
pub const FLIGHT: Color = Color(30, 170, 90, 255);
pub const LABEL: Color = Color(30, 30, 30, 255);

/// Labels are drawn when nodes are at least this far apart on screen.
pub const LABEL_MIN_SPACING_PX: f32 = 28.0;
/// The display's ceiling of moving objects (RFC 005 rule 14): beyond it the newest flights
/// are drawn and the rest are counted.
pub const FLIGHT_CEILING: usize = 2_000;

/// Zoom the view model's viewport so that the whole topology is visible.
pub fn fit_all(vm: &mut ViewModel) {
    let l = Layout::of(&vm.topology);
    vm.topology.viewport.fit(l.min, l.max, layout::SPACING);
}

/// The scene at `instant` (a tick the shell chooses between the Host's batches).
pub fn build(vm: &ViewModel, instant: u64) -> Scene {
    let t = &vm.topology;
    let l = Layout::of(t);
    let vp = &t.viewport;
    let margin = 20.0;
    let visible = |p: Point| {
        p.x >= -margin && p.y >= -margin && p.x <= vp.size.x + margin && p.y <= vp.size.y + margin
    };
    let screen: Vec<Point> = l.nodes.iter().map(|p| vp.world_to_screen(*p)).collect();
    let hubs: Vec<Option<Point>> = l
        .hubs
        .iter()
        .map(|h| h.map(|p| vp.world_to_screen(p)))
        .collect();

    let mut up = Vec::new();
    let mut down = Vec::new();
    let mut hub_centers = Vec::new();
    let mut links_down = 0;
    for (i, link) in t.links.iter().enumerate() {
        let segments = if let Some(h) = hubs[i] {
            hub_centers.push(h);
            link.ports
                .iter()
                .filter_map(|(a, _)| {
                    t.node_index(*a)
                        .map(|n| [screen[n].x, screen[n].y, h.x, h.y])
                })
                .collect::<Vec<_>>()
        } else {
            let ends: Vec<Point> = link
                .ports
                .iter()
                .filter_map(|(a, _)| t.node_index(*a).map(|n| screen[n]))
                .collect();
            if ends.len() == 2 {
                vec![[ends[0].x, ends[0].y, ends[1].x, ends[1].y]]
            } else {
                Vec::new()
            }
        };
        if !link.up {
            links_down += 1;
        }
        for s in segments {
            if visible(Point { x: s[0], y: s[1] }) || visible(Point { x: s[2], y: s[3] }) {
                if link.up { &mut up } else { &mut down }.push(s);
            }
        }
    }

    let radius = (vp.zoom * 6.0).clamp(2.0, 14.0);
    let mut plain = Vec::new();
    let mut active = Vec::new();
    let mut downed = Vec::new();
    let mut selected = Vec::new();
    let mut crosses = Vec::new();
    let mut faulted = 0;
    for (n, &p) in t.nodes.iter().zip(&screen) {
        if n.faulted {
            faulted += 1;
        }
        if !visible(p) {
            continue;
        }
        if vm.selection == Selection::Actor(n.id) {
            selected.push(p);
        }
        if n.faulted {
            crosses.push(p);
        } else if n.down {
            downed.push(p);
        } else if n.activity > 0 {
            active.push(p);
        } else {
            plain.push(p);
        }
    }
    let nodes_visible = plain.len() + active.len() + downed.len() + crosses.len();

    let mut flights = Vec::new();
    let newest = t.flights.len().saturating_sub(FLIGHT_CEILING);
    for f in &t.flights[newest..] {
        let Some((a, b)) = l.flight_ends(t, usize::from(f.connection), f.port) else {
            continue;
        };
        let (a, b) = (vp.world_to_screen(a), vp.world_to_screen(b));
        let k = f.progress(instant);
        let p = Point {
            x: a.x + (b.x - a.x) * k,
            y: a.y + (b.y - a.y) * k,
        };
        if visible(p) {
            flights.push(p);
        }
    }

    let spacing_px = layout::SPACING * vp.zoom;
    let mut labels = Vec::new();
    if spacing_px >= LABEL_MIN_SPACING_PX {
        for (n, &p) in t.nodes.iter().zip(&screen) {
            if visible(p) {
                labels.push((
                    Point {
                        x: p.x,
                        y: p.y - radius - 2.0,
                    },
                    n.name.clone(),
                ));
            }
        }
    }
    let font_px = (spacing_px * 0.3).clamp(9.0, 16.0);

    let mut items = Vec::with_capacity(10);
    items.push(Item::Lines {
        segments: up,
        stroke: Stroke {
            color: LINK,
            width: 1.0,
            dashed: false,
        },
    });
    if !down.is_empty() {
        items.push(Item::Lines {
            segments: down,
            stroke: Stroke {
                color: LINK_DOWN,
                width: 1.0,
                dashed: true,
            },
        });
    }
    if !hub_centers.is_empty() {
        items.push(Item::Circles {
            centers: hub_centers,
            radius: (radius * 0.5).max(2.0),
            fill: HUB,
        });
    }
    if !selected.is_empty() {
        items.push(Item::Circles {
            centers: selected,
            radius: radius + 3.0,
            fill: NODE_SELECTED,
        });
    }
    items.push(Item::Circles {
        centers: plain,
        radius,
        fill: NODE,
    });
    if !active.is_empty() {
        items.push(Item::Circles {
            centers: active,
            radius,
            fill: NODE_ACTIVE,
        });
    }
    if !downed.is_empty() {
        items.push(Item::Circles {
            centers: downed,
            radius,
            fill: NODE_DOWN,
        });
    }
    if !crosses.is_empty() {
        items.push(Item::Crosses {
            centers: crosses,
            size: radius,
            stroke: Stroke {
                color: FAULT,
                width: 2.0,
                dashed: false,
            },
        });
    }
    let flight_count = flights.len();
    items.push(Item::Circles {
        centers: flights,
        radius: (radius * 0.6).max(2.0),
        fill: FLIGHT,
    });
    if !labels.is_empty() {
        items.push(Item::Labels {
            labels,
            font_px,
            fill: LABEL,
        });
    }
    let selected_name = match vm.selection {
        Selection::Actor(id) => t.node(id).map(|n| n.name.clone()),
        Selection::Connection(id) => t.link(id).map(|l| l.name.clone()),
        _ => None,
    };
    Scene {
        items,
        description: Description {
            nodes: t.nodes.len(),
            links: t.links.len(),
            links_down,
            faulted,
            flights: flight_count,
            selected: selected_name,
        },
        nodes_visible,
    }
}

/// What is under a screen point within `radius_px`: a node first, then a hub, then a link.
pub fn hit_test(vm: &ViewModel, screen: Point, radius_px: f32) -> Selection {
    let t = &vm.topology;
    let l = Layout::of(t);
    let vp = &t.viewport;
    let r2 = radius_px * radius_px;
    let d2 = |p: Point| {
        let s = vp.world_to_screen(p);
        (s.x - screen.x).powi(2) + (s.y - screen.y).powi(2)
    };
    let mut best: Option<(f32, Selection)> = None;
    for (n, p) in t.nodes.iter().zip(&l.nodes) {
        let d = d2(*p);
        if d <= r2 && best.is_none_or(|(b, _)| d < b) {
            best = Some((d, Selection::Actor(n.id)));
        }
    }
    if let Some((_, s)) = best {
        return s;
    }
    for (link, h) in t.links.iter().zip(&l.hubs) {
        if let Some(h) = h {
            let d = d2(*h);
            if d <= r2 && best.is_none_or(|(b, _)| d < b) {
                best = Some((d, Selection::Connection(link.id)));
            }
        }
    }
    if let Some((_, s)) = best {
        return s;
    }
    for (i, link) in t.links.iter().enumerate() {
        if l.hubs[i].is_some() {
            continue;
        }
        let ends: Vec<Point> = link
            .ports
            .iter()
            .filter_map(|(a, _)| t.node_index(*a).map(|n| vp.world_to_screen(l.nodes[n])))
            .collect();
        if ends.len() != 2 {
            continue;
        }
        let d = segment_distance2(screen, ends[0], ends[1]);
        if d <= r2 && best.is_none_or(|(b, _)| d < b) {
            best = Some((d, Selection::Connection(link.id)));
        }
    }
    best.map_or(Selection::None, |(_, s)| s)
}

fn segment_distance2(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let k = if len2 <= 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    let (qx, qy) = (a.x + dx * k, a.y + dy * k);
    (p.x - qx).powi(2) + (p.y - qy).powi(2)
}

#[cfg(test)]
mod tests;
