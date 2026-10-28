# The learning lab

The lab is the bundled scenario `ospf-lab`: four routers running the OSPF subset of RFC 008 rule 21, in a ring with a chord, with a link that breaks and heals while the run goes on. It is the first entry under "Learning labs" in the application, and it is the scenario the journeys of the acceptance criteria drive in every browser. This chapter is a walk through it, then a word on the other bundled scenarios.

## Before you start

Open the application (the [interface chapter](./interface.md) says how to serve or build it) and choose "OSPF: a ring with a chord" under "Learning labs". The topology shows r1 to r4 in a ring, r1-r2, r2-r3, r3-r4 and r4-r1, and a fifth connection, the chord r1-r3, whose ports carry a cost of 5 where every other port costs 1. The actors list beside the canvas is its twin: each router there is a Tab stop, and Enter selects it as a click on the canvas does.

Nothing has run yet. Set the speed to a slow value first, 100 ticks per second, so that frames are drawn in flight; the lab's own pace, 1,000 ticks per second and more, comes later.

## What you will see

**Hellos and adjacencies, ticks 0 to 200.** Every router says hello on each of its interfaces every 50 ticks: the packet carries its router id, its intervals and the ids it has heard on that interface. A router that hears its own id in a neighbour's hello declares the adjacency full. Select r1 and watch the inspector: its latest telemetry lists a `neighbour` report per port, and each goes from down to full within two hello periods. The timeline shows the activations as they happen; filter by actor to follow one router.

**Flooding, ticks 100 to 400.** A router whose adjacency went full sends its whole link-state database to the new neighbour and originates its own router-LSA, a list of its neighbours with their costs, with a sequence number. Each router floods what it receives to its other full adjacencies and acknowledges it. In the timeline, the sends with an `update` packet are the flood; open one with its row and the message inspector decodes it: the type, the sender, the area, then each LSA as `origin#sequence[neighbour:cost, ...]`.

**Convergence, from tick 400.** With every LSA in every database, each router runs Dijkstra over the graph and reports a `route` per destination: the next hop's port and the cost. In r1's inspector, r3 is reached the short way round, through r2 or r4 at cost 2, not over the chord that costs 5. The inspector shows the routes as they are reported; a route that changes is reported again with its new next hop, and a route that disappears is withdrawn.

**The break, tick 1,500.** The scenario schedules the link r2-r3 to be disabled at 1,500. The connection is drawn dashed; frames that were in flight on it still arrive, and sends after the instant are dropped. The routers notice through the dead interval: 200 ticks without a hello and the adjacency goes down. Both r2 and r3 then withdraw the adjacency, re-originate their LSAs with a higher sequence number, and the flood carries the change round the ring. Routes to the far side move: r2 now reaches r3 round the other way, through r1 and r4, at cost 3; the chord would cost 6. Watch the `route` reports in r2's inspector change within two hello periods of the dead interval's end.

**The repair, tick 3,000.** The link is enabled again. Hellos cross it, the adjacency goes full once more, the LSAs are re-originated and the routes return to their first values. The run ends at 4,500.

## What to try

- **Break a different link.** Select the connection r3-r4 on the canvas, or from the timeline, and use "Take this link down at the next instant" in its inspector. The intervention is listed under "Interventions" with the instant at which it applied, and r4 now reaches the others over r1. Bring it back up the same way.
- **Take a router down.** Select r2 and take it down. Its neighbours' dead intervals expire, they re-originate, and r2's own LSA ages out of every other router's routes; the destination disappears from the inspectors.
- **Step through the break.** Pause before 1,500, set the step unit to one event, and step: each activation is one line in the timeline and one change on the canvas.
- **Replay.** Export the run record after the run ends (the preview says what the file contains: the inputs, the interventions and the trace, never a payload unless you kept them). Then replay it: the application executes the scenario again from the inputs, applies your interventions at their instants, and reports whether the digest matched. It will.
- **Save and reload.** Save the scenario, open it again, and the schedule, the attributes and the costs are as you left them; the ended run's interventions can be kept as scheduled changes.

The same scenario runs on the command line, with the seed the kit records in `scenarios/digests.toml`:

```sh
florn run scenarios/ospf-lab.toml --modules scenarios/modules --seed 1
```

The `chain` line is the run's digest, and it is the same digest the application shows, on every supported engine.

## What the subset leaves out

The guest implements one area, point-to-point adjacencies, router-LSAs with sequence numbers and acknowledgements, and Dijkstra. Anything else is refused rather than approximated, and the refusal is a line in the diagnostic log and under "States": a router whose `area` is not 0 stays idle and says why; a second router on the same interface is refused once per port; hellos with other intervals or from another area are ignored with a reason. The bundled scenario `ospf-outside` shows the first of these: r2 is in area 1, logs its refusal, and r1 keeps saying hello to nobody.

## The other bundled scenarios

| Scenario | What to watch |
|---|---|
| `dv-triangle` | Three distance-vector routers advertising every 100 ticks with split horizon; the link r1-r2 breaks at 1,000 and heals at 2,000; the routes reconverge through r3 and back within three periods. |
| `dv-chain` | Four distance-vector routers in a line with `infinity = 3`: the far ends never install each other, and a route beyond the horizon is never advertised. |
| `net-delay` | A 1,500-byte frame over a 20 ns link at 1000 Gbit/s arrives 32 ticks after it was sent, and the echo 32 later: delay plus serialisation. |
| `net-loss` | Ten thousand frames at `loss = 0.1`, drawn from the link's own stream: between 900 and 1,100 are lost, the same ones on every engine. |
| `net-jitter` | Every arrival within `[delay, delay + jitter]`. |
| `net-link-down` | The link goes down at 300 and up at 600; `on-link` at both ends; frames in flight arrive; sends while down are dropped. |
| `net-lan` | A lan of three: a frame reaches the two others, each with its own loss draw. |
| `net-blackhole` | The `network/blackhole` built-in drops every frame and reports the count, unmetered. |

Each has a story under "Learning labs" in both languages, and each has a recorded digest in `scenarios/digests.toml` that the conformance kit checks natively and in Chromium, Firefox and WebKit.
