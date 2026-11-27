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
