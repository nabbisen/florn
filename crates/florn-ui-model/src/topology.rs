//! The topology as shown (external design §3.2): the actors as nodes, the connections as
//! links, each link's state from the domain's events, the deliveries in flight and each
//! actor's activity from the display stream (RFC 005 rule 14), and the visible area. Positions
//! are the scene's (rule 37: the layout is the scene's); the view model holds what is true of
//! the run, not where it is drawn.
use florn_control::display::Record;
use florn_control::message::{PortSummary, ScenarioSummary};

/// A point in world coordinates (the topology's own plane) or on the screen (CSS pixels).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// The visible area: `center` is the world point shown at the middle of the screen; `zoom` is
/// screen pixels per world unit; `size` is the screen size in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub center: Point,
    pub zoom: f32,
    pub size: Point,
}

impl Viewport {
    pub const MIN_ZOOM: f32 = 0.05;
    pub const MAX_ZOOM: f32 = 40.0;

    pub fn world_to_screen(&self, p: Point) -> Point {
        Point {
            x: (p.x - self.center.x) * self.zoom + self.size.x / 2.0,
            y: (p.y - self.center.y) * self.zoom + self.size.y / 2.0,
        }
    }
    pub fn screen_to_world(&self, p: Point) -> Point {
        Point {
            x: (p.x - self.size.x / 2.0) / self.zoom + self.center.x,
            y: (p.y - self.size.y / 2.0) / self.zoom + self.center.y,
        }
    }
    /// Pan by a screen-space delta (a drag).
    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.center.x -= dx / self.zoom;
        self.center.y -= dy / self.zoom;
    }
    /// Zoom by `factor` keeping the world point under `screen` fixed (the wheel).
    pub fn zoom_at(&mut self, screen: Point, factor: f32) {
        let before = self.screen_to_world(screen);
        self.zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        let after = self.screen_to_world(screen);
        self.center.x += before.x - after.x;
        self.center.y += before.y - after.y;
    }
    /// Show the world rectangle `min..max` whole, with a margin in world units.
    pub fn fit(&mut self, min: Point, max: Point, margin: f32) {
        let w = (max.x - min.x).max(1.0) + 2.0 * margin;
        let h = (max.y - min.y).max(1.0) + 2.0 * margin;
        self.center = Point {
            x: (min.x + max.x) / 2.0,
            y: (min.y + max.y) / 2.0,
        };
        self.zoom = (self.size.x / w)
            .min(self.size.y / h)
            .clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            center: Point::default(),
            zoom: 1.0,
            size: Point { x: 800.0, y: 600.0 },
        }
    }
}

/// An actor as a node of the topology.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: u32,
    pub name: String,
    /// The module's name, or `None` for a built-in.
    pub module: Option<String>,
    pub class: String,
    pub ports: Vec<PortSummary>,
    /// Faulted in this run (RFC 003 rule 13): shown by shape and word, never colour alone.
    pub faulted: bool,
    /// Taken down by the domain (`node-changed` with `up=false`).
    pub down: bool,
    /// Activations in the last display batch, and the tick of the last one.
    pub activity: u32,
    pub last_active: u64,
}

/// A connection as a link of the topology: a line between two ports, or a hub joining more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub id: u32,
    pub name: String,
    /// `<domain>/<name>`; `None` for the core's.
    pub ty: Option<String>,
    pub ports: Vec<(u32, u32)>,
    /// From the domain's `link-changed` events; up until told otherwise.
    pub up: bool,
    pub attrs: Vec<(String, String)>,
}

impl Link {
    /// More than two ports: drawn as a hub (a lan).
    pub fn is_hub(&self) -> bool {
        self.ports.len() > 2
    }
}

/// A delivery in flight (RFC 005 rule 14, `flight`): on the connection at `connection` (its
/// index in id order) from its port `port`, for `delay` ticks from `sent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    pub connection: u16,
    pub port: u8,
    pub sent: u64,
    pub delay: f32,
}

impl Flight {
    /// How far along at `instant`, 0 to 1; beyond 1 it has arrived.
    pub fn progress(&self, instant: u64) -> f32 {
        if self.delay <= 0.0 {
            return 1.0;
        }
        (instant.saturating_sub(self.sent) as f32 / self.delay).clamp(0.0, 1.0)
    }
    /// Arrived once the instant is past its delivery: at `sent + delay` the delivery is the
    /// next event, not yet handled (every event before the instant is), so the frame is still
    /// shown, at its destination.
    pub fn arrived(&self, instant: u64) -> bool {
        instant.saturating_sub(self.sent) as f32 > self.delay
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Topology {
    pub nodes: Vec<Node>,
    /// In id order, as the display stream indexes them.
    pub links: Vec<Link>,
    pub flights: Vec<Flight>,
    /// The tick the Host reached (every event before it is handled), from the display stream.
    pub instant: u64,
    pub viewport: Viewport,
}

impl Topology {
    pub fn from_scenario(s: &ScenarioSummary) -> Self {
        let mut links: Vec<Link> = s
            .connections
            .iter()
            .map(|c| Link {
                id: c.id,
                name: c.name.clone(),
                ty: c.ty.clone(),
                ports: c.ports.clone(),
                up: true,
                attrs: c.attrs.clone(),
            })
            .collect();
        links.sort_by_key(|l| l.id);
        Self {
            nodes: s
                .actors
                .iter()
                .map(|a| Node {
                    id: a.id,
                    name: a.name.clone(),
                    module: a.module.clone(),
                    class: a.class.clone(),
                    ports: a.ports.clone(),
                    faulted: false,
                    down: false,
                    activity: 0,
                    last_active: 0,
                })
                .collect(),
            links,
            flights: Vec::new(),
            instant: 0,
            viewport: Viewport::default(),
        }
    }

    /// A new run: what a run told is forgotten; the scenario stays.
    pub fn reset_run(&mut self) {
        for n in &mut self.nodes {
            n.faulted = false;
            n.down = false;
            n.activity = 0;
            n.last_active = 0;
        }
        for l in &mut self.links {
            l.up = true;
        }
        self.flights.clear();
        self.instant = 0;
    }

    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
    pub fn node_mut(&mut self, id: u32) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }
    pub fn node_index(&self, id: u32) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }
    pub fn link(&self, id: u32) -> Option<&Link> {
        self.links.iter().find(|l| l.id == id)
    }
    pub fn link_mut(&mut self, id: u32) -> Option<&mut Link> {
        self.links.iter_mut().find(|l| l.id == id)
    }
    pub fn link_index(&self, id: u32) -> Option<usize> {
        self.links.iter().position(|l| l.id == id)
    }
    /// The link a port is on.
    pub fn link_of_port(&self, actor: u32, port: u32) -> Option<&Link> {
        self.links.iter().find(|l| l.ports.contains(&(actor, port)))
    }
    /// The flights on a link, by its index.
    pub fn in_flight(&self, link: usize) -> usize {
        self.flights
            .iter()
            .filter(|f| usize::from(f.connection) == link)
            .count()
    }

    /// A display batch (rule 14): flights join, arrivals leave, activity is this batch's,
    /// the instant moves on.
    pub fn apply_display(&mut self, records: &[Record]) {
        let mut seen_activity = false;
        for r in records {
            match *r {
                Record::Flight {
                    connection,
                    port,
                    delay,
                    sent,
                } => self.flights.push(Flight {
                    connection,
                    port,
                    sent,
                    delay,
                }),
                Record::Activity {
                    actor,
                    count,
                    faulted,
                    last,
                } => {
                    if !seen_activity {
                        for n in &mut self.nodes {
                            n.activity = 0;
                        }
                        seen_activity = true;
                    }
                    if let Some(n) = self.node_mut(actor) {
                        n.activity = u32::from(count);
                        n.last_active = last;
                        n.faulted |= faulted;
                    }
                }
                Record::Instant { tick } => {
                    self.instant = tick;
                    self.flights.retain(|f| !f.arrived(tick));
                }
            }
        }
    }

    /// A `link-changed` event of the network domain: its detail says `enabled=…`.
    pub fn link_changed(&mut self, target: u32, detail: &str) {
        if let Some(l) = self.link_mut(target)
            && let Some(v) = value_of(detail, "enabled")
        {
            l.up = v == "true";
        }
    }
    /// A `node-changed` event: `up=…`.
    pub fn node_changed(&mut self, target: u32, detail: &str) {
        if let Some(n) = self.node_mut(target)
            && let Some(v) = value_of(detail, "up")
        {
            n.down = v != "true";
        }
    }
    pub fn faulted(&mut self, actor: u32) {
        if let Some(n) = self.node_mut(actor) {
            n.faulted = true;
        }
    }
}

/// `key=value` pairs separated by spaces, as the domains write their details.
pub fn value_of<'a>(detail: &'a str, key: &str) -> Option<&'a str> {
    detail
        .split_whitespace()
        .find_map(|pair| pair.strip_prefix(key)?.strip_prefix('='))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        let mut v = Viewport::default();
        let s = Point { x: 123.0, y: 456.0 };
        let before = v.screen_to_world(s);
        v.zoom_at(s, 1.7);
        let after = v.screen_to_world(s);
        assert!((before.x - after.x).abs() < 1e-3 && (before.y - after.y).abs() < 1e-3);
    }

    #[test]
    fn flights_arrive_and_details_parse() {
        let f = Flight {
            connection: 0,
            port: 0,
            sent: 100,
            delay: 50.0,
        };
        assert_eq!(f.progress(100), 0.0);
        assert_eq!(f.progress(125), 0.5);
        assert_eq!(f.progress(150), 1.0);
        assert!(f.arrived(151));
        assert!(!f.arrived(150));
        assert_eq!(
            value_of("enabled=false delay=2 bandwidth=0", "enabled"),
            Some("false")
        );
        assert_eq!(value_of("up=true", "enabled"), None);
    }
}
