//! The workspace of a scenario (RFC 014 rule 15, item 2; UI-001): which scene and which panels the
//! domains it declares call for. The network's stay as they are; a scenario that declares the
//! circuit domain also shows the schematic and the plots (RFC 014 rules 6 and 7, drawn by the
//! units that follow). What a domain draws as a series is its own knowledge and is kept here, in
//! one place, as the declaration of what its sensors and controllers report and which of a guest's
//! channels the timeline lists.

/// What the canvas shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneKind {
    /// Nodes and links: the network's topology.
    Topology,
    /// The circuit's schematic (RFC 014 rule 6).
    Schematic,
}

/// A panel a domain adds beside the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelKind {
    /// The waveforms (RFC 014 rule 7).
    Plots,
}

/// A built-in's report that draws a series: its `<domain>/<name>`, the channel it reports on, the
/// name of the field and its unit as the sensor declares it (RFC 012 rule 24).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltinSeries {
    pub builtin: &'static str,
    pub channel: u32,
    pub field: &'static str,
    pub unit: Option<&'static str>,
}

/// What a domain draws as series: the guests' channels by name (a guest declares its layouts; the
/// domain says which of them it draws) and the built-ins' reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drawn {
    pub channels: &'static [&'static str],
    pub builtins: &'static [BuiltinSeries],
}

impl Drawn {
    pub const NONE: Drawn = Drawn {
        channels: &[],
        builtins: &[],
    };

    /// The circuit domain's (RFC 012 rule 24; RFC 014 rules 2 and 3): a controller's `control`, a
    /// sensor's `probe` (channel 1) and a detector's `condition` (channel 2).
    pub const CIRCUIT: Drawn = Drawn {
        channels: &["control"],
        builtins: &[
            BuiltinSeries {
                builtin: "circuit/voltmeter",
                channel: 1,
                field: "v",
                unit: Some("V"),
            },
            BuiltinSeries {
                builtin: "circuit/ammeter",
                channel: 1,
                field: "i",
                unit: Some("A"),
            },
            BuiltinSeries {
                builtin: "circuit/dc-motor",
                channel: 1,
                field: "speed",
                unit: Some("rad/s"),
            },
            BuiltinSeries {
                builtin: "circuit/comparator",
                channel: 2,
                field: "y",
                unit: None,
            },
        ],
    };
}

/// The scene and the panels of a scenario, by the domains it declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    /// The scenes the canvas can show, the first by default.
    pub scenes: Vec<SceneKind>,
    pub panels: Vec<PanelKind>,
    pub drawn: Drawn,
    /// The channels of a guest the timeline lists as reports (RFC 008 rule 17 for the network): a
    /// report on one is decoded by the layout the module declares for it, whatever the domain.
    pub reads: &'static [&'static str],
}

/// What the network's views read (RFC 008 rule 17; RFC 011 rule 8).
pub const NETWORK_READS: &[&str] = &["neighbour", "route", "peer"];

impl Default for Workspace {
    fn default() -> Self {
        Self::of(&[])
    }
}

impl Workspace {
    /// The workspace of the domains a scenario declares (`core` is always there and calls for
    /// nothing of its own). A scenario with no domain of the workspaces below keeps the network's
    /// layout, as before.
    pub fn of(domains: &[String]) -> Self {
        let has = |d: &str| domains.iter().any(|x| x == d);
        if has("circuit") {
            let mut scenes = vec![SceneKind::Schematic];
            if has("network") {
                scenes.push(SceneKind::Topology);
            }
            return Self {
                scenes,
                panels: vec![PanelKind::Plots],
                drawn: Drawn::CIRCUIT,
                // A controller's reports are its series (rule 3); the timeline lists the activations.
                reads: &[],
            };
        }
        Self {
            scenes: vec![SceneKind::Topology],
            panels: Vec::new(),
            drawn: Drawn::NONE,
            reads: NETWORK_READS,
        }
    }

    /// The scene the canvas starts with.
    pub fn scene(&self) -> SceneKind {
        self.scenes.first().copied().unwrap_or(SceneKind::Topology)
    }

    pub fn shows(&self, panel: PanelKind) -> bool {
        self.panels.contains(&panel)
    }
}

#[cfg(test)]
mod tests;
