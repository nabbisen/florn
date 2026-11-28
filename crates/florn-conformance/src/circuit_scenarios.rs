//! The circuit scenarios the kit ships (RFC 012 rule 27), written as manifest text: the plant is
//! built-ins on named pins and signals, which the numbered ports of [`Scenario`](crate::scenario)
//! cannot say.

use crate::scenario::Scenario;

/// `rc-step` (RFC 012 rule 27, RFC 013 rule 21): a 1 kohm resistor and a 1 uF capacitor with
/// `ic = 0V` (RC = 1 ms) on a voltage source of 1 V, stepped to 0 V at 2 ms by a scheduled change,
/// a voltmeter across the capacitor with period 100 us, to 5 ms, at the default profile. The
/// analytic solution is its golden; the results file is its record across the engines.
pub fn rc_step() -> Scenario {
    let deck = crate::circuit::rc_step().change("2ms", "world:circuit", "actor = 1\nv = \"0V\"");
    // The end in nanoseconds, as the other shipped scenarios write it: the browser kit reads the
    // half of a run from this line to take its cross-Host checkpoint there.
    let text = deck.text().replace("end = \"5ms\"", "end = \"5000000ns\"");
    Scenario::raw("rc-step", 5_000_000, text, &[])
}

/// `motor-loop` (RFC 012 rule 27, the closed loop of the exit criterion): a `pid` controller
/// drives a voltage source into a DC motor, the motor's `speed` comes back to the controller, the
/// set point and the load torque are each a `constant`, and a load step is scheduled at one second.
///
/// The controller samples every millisecond one tick after the motor's sensor, so that it reads the
/// sample of the same millisecond (RFC 012 rule 10: with `phase = 1` it reads the current one). Its
/// output is a command between 0 and 1 that scales the source's 12 V.
pub fn motor_loop() -> Scenario {
    Scenario::raw("motor-loop", 1_500_000_000, motor_loop_text(), &["pid"])
}

/// The manifest text of `motor-loop`, with `@pid@` for the controller module's hash: the tests of
/// the controller's rows change a line of it.
pub fn motor_loop_text() -> String {
    r#"florn = 1
name = "motor-loop"
domains = ["core", "circuit"]

[time]
tick = "1ns"
end = "1500000000ns"

[order]
tie = "by-identity"

[seed]
root = 1

[limits.default]
memory = "2MiB"

[modules.pid]
sha256 = "@pid@"
contract = "florn:circuit/controller@0.1.0"

[[actors]]
id = 1
name = "vs"
kind = "builtin"
builtin = "circuit/voltage-source"
ports = [{ id = 0, name = "p", type = "circuit/pin" }, { id = 1, name = "n", type = "circuit/pin" }, { id = 2, name = "cmd", type = "circuit/signal-in" }]
[actors.config]
v = "0V"
gain = "12V"

[[actors]]
id = 2
name = "m1"
kind = "builtin"
builtin = "circuit/dc-motor"
ports = [{ id = 0, name = "a+", type = "circuit/pin" }, { id = 1, name = "a-", type = "circuit/pin" }, { id = 2, name = "speed", type = "circuit/signal-out" }, { id = 3, name = "load", type = "circuit/signal-in" }]
[actors.config]
r = "1ohm"
l = "500uH"
kt = "0.05Nm/A"
ke = "0.05Vs/rad"
j = "0.0005kgm2"
b = "0.0001Nms"
period = "1ms"

[[actors]]
id = 3
name = "sp"
kind = "builtin"
builtin = "circuit/constant"
ports = [{ id = 0, name = "y", type = "circuit/signal-out" }]
[actors.config]
value = "100rad/s"

[[actors]]
id = 4
name = "tl"
kind = "builtin"
builtin = "circuit/constant"
ports = [{ id = 0, name = "y", type = "circuit/signal-out" }]
[actors.config]
value = "0Nm"

[[actors]]
id = 5
name = "pid"
kind = "module"
module = "pid"
ports = [{ id = 0, name = "measurement", type = "circuit/signal-in" }, { id = 1, name = "setpoint", type = "circuit/signal-in" }, { id = 2, name = "output", type = "circuit/signal-out" }]
[actors.config]
period = "1ms"
phase = "1ns"
kp = 0.004
ki = 0.0004
kd = 0.0
out-min = 0.0
out-max = 1.0

[[connections]]
id = 100
name = "bus"
type = "circuit/net"
ports = ["vs.p", "m1.a+"]

[[connections]]
id = 101
name = "gnd"
type = "circuit/net"
ports = ["vs.n", "m1.a-"]
[connections.attrs]
ground = true

[[connections]]
id = 102
name = "speed"
type = "circuit/signal"
ports = ["m1.speed", "pid.measurement"]

[[connections]]
id = 103
name = "setpoint"
type = "circuit/signal"
ports = ["sp.y", "pid.setpoint"]

[[connections]]
id = 104
name = "command"
type = "circuit/signal"
ports = ["pid.output", "vs.cmd"]

[[connections]]
id = 105
name = "load"
type = "circuit/signal"
ports = ["tl.y", "m1.load"]

[[changes]]
at = "1000000000ns"
target = "world:circuit"
[changes.payload]
actor = 4
value = "0.02Nm"
"#
    .to_string()
}

/// The head of a shipped circuit scenario: the manifest's own sections up to the actors, with the
/// modules it uses (`@module@` stands for a module's hash), its `[circuit]` profile lines and its
/// scheduled changes after them.
fn head(name: &str, end_ns: u64, modules: &[(&str, &str)], profile: &str) -> String {
    let mut t = format!(
        "florn = 1\nname = \"{name}\"\ndomains = [\"core\", \"circuit\"]\n\n[time]\ntick = \"1ns\"\nend = \"{end_ns}ns\"\n\n[order]\ntie = \"by-identity\"\n\n[seed]\nroot = 1\n\n[limits.default]\nmemory = \"2MiB\"\n"
    );
    for (module, contract) in modules {
        t.push_str(&format!(
            "\n[modules.{module}]\nsha256 = \"@{module}@\"\ncontract = \"{contract}\"\n"
        ));
    }
    if !profile.is_empty() {
        t.push_str(&format!("\n[circuit]\n{profile}\n"));
    }
    t
}

/// A built-in actor of the text: its identity, name, kind, ports (name and type) and its config.
fn builtin(id: u32, name: &str, kind: &str, ports: &[(&str, &str)], config: &str) -> String {
    let ports: Vec<String> = ports
        .iter()
        .enumerate()
        .map(|(i, (n, t))| format!("{{ id = {i}, name = \"{n}\", type = \"{t}\" }}"))
        .collect();
    let mut t = format!(
        "\n[[actors]]\nid = {id}\nname = \"{name}\"\nkind = \"builtin\"\nbuiltin = \"circuit/{kind}\"\nports = [{}]\n",
        ports.join(", ")
    );
    if !config.is_empty() {
        t.push_str(&format!("[actors.config]\n{config}\n"));
    }
    t
}

/// A guest actor of the text with the pins of a two-terminal device and its config.
fn device(id: u32, name: &str, module: &str, config: &str) -> String {
    let mut t = format!(
        "\n[[actors]]\nid = {id}\nname = \"{name}\"\nkind = \"module\"\nmodule = \"{module}\"\nports = [{{ id = 0, name = \"a\", type = \"circuit/pin\" }}, {{ id = 1, name = \"k\", type = \"circuit/pin\" }}]\n"
    );
    if !config.is_empty() {
        t.push_str(&format!("[actors.config]\n{config}\n"));
    }
    t
}

/// A connection of the text.
fn connection(id: u32, name: &str, ty: &str, ports: &[&str], ground: bool) -> String {
    let list: Vec<String> = ports.iter().map(|p| format!("\"{p}\"")).collect();
    let mut t = format!(
        "\n[[connections]]\nid = {id}\nname = \"{name}\"\ntype = \"circuit/{ty}\"\nports = [{}]\n",
        list.join(", ")
    );
    if ground {
        t.push_str("[connections.attrs]\nground = true\n");
    }
    t
}

/// A scheduled change of a source's voltage.
fn source_change(at_ns: u64, volts: &str) -> String {
    format!(
        "\n[[changes]]\nat = \"{at_ns}ns\"\ntarget = \"world:circuit\"\n[changes.payload]\nactor = 1\nv = \"{volts}\"\n"
    )
}

/// The plant of `diode-clip` and `guest-diode` (RFC 012 rule 27): a 5 V source through a 1 kohm
/// resistor into a diode, a voltmeter across the diode every 10 us, and the source stepped to −5 V
/// at 0.4 ms, back to +5 V at 0.8 ms and to −5 V at 1.2 ms: the nonlinear iteration and the
/// restarts. The diode is a built-in (`diode-clip`) or the guest `guest_diode` (`guest-diode`),
/// with the same parameters, so that the two voltmeters' samples are the same plant.
fn clip(name: &str, guest: bool) -> String {
    let modules: &[(&str, &str)] = if guest {
        &[("guest_diode", "florn:circuit/device@0.1.0")]
    } else {
        &[]
    };
    let mut t = head(name, 1_600_000, modules, "");
    t.push_str(&builtin(
        1,
        "vs",
        "voltage-source",
        &[("p", "circuit/pin"), ("n", "circuit/pin")],
        "v = \"5V\"",
    ));
    t.push_str(&builtin(
        2,
        "r",
        "resistor",
        &[("a", "circuit/pin"), ("b", "circuit/pin")],
        "r = \"1kohm\"",
    ));
    if guest {
        t.push_str(&device(3, "d", "guest_diode", ""));
    } else {
        t.push_str(&builtin(
            3,
            "d",
            "diode",
            &[("a", "circuit/pin"), ("k", "circuit/pin")],
            "",
        ));
    }
    t.push_str(&builtin(
        4,
        "vm",
        "voltmeter",
        &[("p", "circuit/pin"), ("n", "circuit/pin")],
        "period = \"10us\"",
    ));
    t.push_str(&connection(100, "in", "net", &["vs.p", "r.a"], false));
    t.push_str(&connection(
        101,
        "mid",
        "net",
        &["r.b", "d.a", "vm.p"],
        false,
    ));
    t.push_str(&connection(
        102,
        "gnd",
        "net",
        &["vs.n", "d.k", "vm.n"],
        true,
    ));
    for (at, v) in [(400_000, "-5V"), (800_000, "5V"), (1_200_000, "-5V")] {
        t.push_str(&source_change(at, v));
    }
    t
}

/// `diode-clip` (RFC 012 rule 27): the plant above with the built-in diode.
pub fn diode_clip() -> Scenario {
    Scenario::raw("diode-clip", 1_600_000, clip("diode-clip", false), &[])
}

/// `guest-diode` (RFC 012 rule 27): the same plant with the guest device in the diode's place.
pub fn guest_diode() -> Scenario {
    Scenario::raw(
        "guest-diode",
        1_600_000,
        clip("guest-diode", true),
        &["guest_diode"],
    )
}

/// A 5 V source through a 1 kohm resistor into a `bad_device` whose parameters are `config`, a
/// voltmeter across it every 100 us: the plant of the hostile device scenarios. The device is a
/// conductance of 1 mS when it behaves.
fn hostile(name: &str, end_ns: u64, config: &str, profile: &str) -> String {
    let mut t = head(
        name,
        end_ns,
        &[("bad_device", "florn:circuit/device@0.1.0")],
        profile,
    );
    t.push_str(&builtin(
        1,
        "vs",
        "voltage-source",
        &[("p", "circuit/pin"), ("n", "circuit/pin")],
        "v = \"5V\"",
    ));
    t.push_str(&builtin(
        2,
        "r",
        "resistor",
        &[("a", "circuit/pin"), ("b", "circuit/pin")],
        "r = \"1kohm\"",
    ));
    t.push_str(&device(3, "d", "bad_device", config));
    t.push_str(&builtin(
        4,
        "vm",
        "voltmeter",
        &[("p", "circuit/pin"), ("n", "circuit/pin")],
        "period = \"100us\"",
    ));
    t.push_str(&connection(100, "in", "net", &["vs.p", "r.a"], false));
    t.push_str(&connection(
        101,
        "mid",
        "net",
        &["r.b", "d.a", "vm.p"],
        false,
    ));
    t.push_str(&connection(
        102,
        "gnd",
        "net",
        &["vs.n", "d.k", "vm.n"],
        true,
    ));
    t
}

/// `hostile-stamp` (RFC 012 rule 27): a device that answers a NaN stamp at its fifth evaluation.
/// The solver discards the step, the Host records a Fault of kind contract, parameter 12020, and
/// `circuit.stamp-invalid` and `circuit.device-faulted` are notified; the plant continues without
/// the device under `faulted-policy = "drop"`.
pub fn hostile_stamp() -> Scenario {
    Scenario::raw(
        "hostile-stamp",
        1_000_000,
        hostile("hostile-stamp", 1_000_000, "mode = \"nan\"\nat = 5", ""),
        &["bad_device"],
    )
}

/// `hostile-diverge` (RFC 013 rule 17): a device whose stamps never converge from its third
/// evaluation on. The initial solve succeeds, the first step fails at every size, and the run ends
/// with `circuit.non-convergence` and the end reason `non-convergence` (`on-non-convergence`'s
/// default, `stop-run`).
pub fn hostile_diverge() -> Scenario {
    Scenario::raw(
        "hostile-diverge",
        1_000_000,
        hostile(
            "hostile-diverge",
            1_000_000,
            "mode = \"diverge\"\nat = 3",
            "",
        ),
        &["bad_device"],
    )
}

/// The text of a hostile scenario with another device config and profile: the kit's tests change a
/// line of it (a mode, an `at`, `on-non-convergence`, `faulted-policy`).
pub fn hostile_text(name: &str, end_ns: u64, config: &str, profile: &str) -> String {
    hostile(name, end_ns, config, profile)
}

/// Replace the one occurrence of `old` in `text` by `new`: a scenario text derived from another's
/// stops at once when the other changes.
fn swap(text: &str, old: &str, new: &str) -> String {
    assert_eq!(text.matches(old).count(), 1, "one occurrence of {old:?}");
    text.replacen(old, new, 1)
}

/// `cps-loop` (RFC 012 rule 27, rule 22): `motor-loop` with the controller a `networked_pid`, one
/// actor of two domains. Its `eth` port is a network interface (no peer: a frame reaches it as a
/// `message` intervention), its set point is the parameter of 100 rad/s until a frame says another,
/// and there is no `constant` for the set point. The scenario declares the network and the circuit
/// domain.
pub fn cps_loop() -> Scenario {
    Scenario::raw(
        "cps-loop",
        1_500_000_000,
        cps_loop_text(),
        &["networked_pid"],
    )
}

/// The manifest text of `cps-loop`, with `@networked_pid@` for the controller module's hash.
pub fn cps_loop_text() -> String {
    let t = motor_loop_text();
    let t = swap(&t, "name = \"motor-loop\"", "name = \"cps-loop\"");
    let t = swap(
        &t,
        "domains = [\"core\", \"circuit\"]",
        "domains = [\"core\", \"network\", \"circuit\"]",
    );
    let t = swap(
        &t,
        "[modules.pid]\nsha256 = \"@pid@\"\ncontract = \"florn:circuit/controller@0.1.0\"",
        "[modules.networked_pid]\nsha256 = \"@networked_pid@\"\ncontract = \"florn:cps/networked-controller@0.1.0\"",
    );
    // The set point is a parameter and a frame: no `constant`, no signal for it.
    let t = swap(
        &t,
        "[[actors]]\nid = 3\nname = \"sp\"\nkind = \"builtin\"\nbuiltin = \"circuit/constant\"\nports = [{ id = 0, name = \"y\", type = \"circuit/signal-out\" }]\n[actors.config]\nvalue = \"100rad/s\"\n\n",
        "",
    );
    let t = swap(
        &t,
        "[[connections]]\nid = 103\nname = \"setpoint\"\ntype = \"circuit/signal\"\nports = [\"sp.y\", \"pid.setpoint\"]\n\n",
        "",
    );
    let t = swap(
        &t,
        "kind = \"module\"\nmodule = \"pid\"\nports = [{ id = 0, name = \"measurement\", type = \"circuit/signal-in\" }, { id = 1, name = \"setpoint\", type = \"circuit/signal-in\" }, { id = 2, name = \"output\", type = \"circuit/signal-out\" }]",
        "kind = \"module\"\nmodule = \"networked_pid\"\nports = [{ id = 0, name = \"measurement\", type = \"circuit/signal-in\" }, { id = 1, name = \"eth\", type = \"network/interface\" }, { id = 2, name = \"output\", type = \"circuit/signal-out\" }]",
    );
    swap(&t, "out-max = 1.0\n", "out-max = 1.0\nsetpoint = 100.0\n")
}
