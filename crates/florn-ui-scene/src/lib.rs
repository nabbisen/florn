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
mod tests {
    use super::*;
    use florn_ui_model::florn_control::message::{
        ActorSummary, ConnectionSummary, PortSummary, ScenarioSummary,
    };
    use florn_ui_model::florn_control::{Answer, Message};

    fn triangle() -> ViewModel {
        let port = |id: u32| PortSummary {
            id,
            name: format!("p{id}"),
            ty: "network/interface".into(),
        };
        let actor = |id: u32| ActorSummary {
            id,
            name: format!("r{id}"),
            module: Some("dv".into()),
            class: "default".into(),
            ports: vec![port(0), port(1)],
            config: Vec::new(),
        };
        let link = |id: u32, a: (u32, u32), b: (u32, u32)| ConnectionSummary {
            id,
            name: format!("c{id}"),
            ports: vec![a, b],
            ty: Some("network/link".into()),
            attrs: Vec::new(),
            per_port: Vec::new(),
        };
        let s = ScenarioSummary {
            name: "t".into(),
            sha256: String::new(),
            tick: "1ns".into(),
            end: 100,
            tie: "by-identity".into(),
            seed: 1,
            actors: vec![actor(1), actor(2), actor(3)],
            connections: vec![
                link(1000, (1, 0), (2, 0)),
                link(1001, (2, 1), (3, 0)),
                link(1002, (3, 1), (1, 1)),
            ],
            modules: Vec::new(),
            missing_modules: Vec::new(),
            domains: vec!["core".into(), "network".into()],
            changes: Vec::new(),
            classes: Vec::new(),
            limits: Vec::new(),
        };
        let mut vm = ViewModel::new();
        vm.apply(&Message::Answer(Answer::Scenario(Some(s))));
        fit_all(&mut vm);
        vm
    }

    #[test]
    fn the_triangle_is_drawn_whole_and_a_flight_moves() {
        use florn_ui_model::florn_control::display::Record;
        let mut vm = triangle();
        let s = build(&vm, 0);
        assert_eq!(s.nodes_visible, 3);
        assert_eq!(s.description.links, 3);
        let segs = s
            .items
            .iter()
            .find_map(|i| match i {
                Item::Lines { segments, .. } => Some(segments.len()),
                _ => None,
            })
            .unwrap();
        assert_eq!(segs, 3);
        // A flight on c0 from r1 (port index 0): at its sent tick it is on r1, half way at
        // half its delay, and gone once it arrived.
        vm.display(&[
            Record::Flight {
                connection: 0,
                port: 0,
                delay: 10.0,
                sent: 20,
            },
            Record::Instant { tick: 20 },
        ]);
        let at = |vm: &ViewModel, tick: u64| -> Vec<Point> {
            build(vm, tick)
                .items
                .iter()
                .find_map(|i| match i {
                    Item::Circles { centers, fill, .. } if *fill == FLIGHT => Some(centers.clone()),
                    _ => None,
                })
                .unwrap()
        };
        let l = Layout::of(&vm.topology);
        let r1 = vm.topology.viewport.world_to_screen(l.nodes[0]);
        let r2 = vm.topology.viewport.world_to_screen(l.nodes[1]);
        let p0 = at(&vm, 20)[0];
        let p5 = at(&vm, 25)[0];
        assert!((p0.x - r1.x).abs() < 1e-3 && (p0.y - r1.y).abs() < 1e-3);
        assert!(((p5.x - (r1.x + r2.x) / 2.0).abs()) < 1e-3);
        assert_eq!(
            at(&vm, 30).len(),
            1,
            "at its destination, the delivery not yet handled"
        );
        vm.display(&[Record::Instant { tick: 31 }]);
        assert!(at(&vm, 31).is_empty());
        assert_eq!(build(&vm, 31).description.flights, 0);
    }

    #[test]
    fn hit_testing_finds_nodes_and_links_and_states_show_by_shape() {
        let mut vm = triangle();
        let l = Layout::of(&vm.topology);
        let vp = vm.topology.viewport;
        let r2 = vp.world_to_screen(l.nodes[1]);
        assert_eq!(hit_test(&vm, r2, 8.0), Selection::Actor(2));
        let r3 = vp.world_to_screen(l.nodes[2]);
        let mid = Point {
            x: (r2.x + r3.x) / 2.0,
            y: (r2.y + r3.y) / 2.0,
        };
        assert_eq!(hit_test(&vm, mid, 8.0), Selection::Connection(1001));
        assert_eq!(
            hit_test(
                &vm,
                Point {
                    x: -500.0,
                    y: -500.0
                },
                8.0
            ),
            Selection::None
        );
        vm.topology.link_changed(1001, "enabled=false delay=2");
        vm.topology.faulted(3);
        vm.select(Selection::Actor(1));
        let s = build(&vm, 0);
        assert_eq!(s.description.links_down, 1);
        assert_eq!(s.description.faulted, 1);
        assert_eq!(s.description.selected.as_deref(), Some("r1"));
        assert!(
            s.items
                .iter()
                .any(|i| matches!(i, Item::Lines { stroke, .. } if stroke.dashed))
        );
        assert!(s.items.iter().any(|i| matches!(i, Item::Crosses { .. })));
    }
}
