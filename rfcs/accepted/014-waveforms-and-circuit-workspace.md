# RFC 014 — Waveforms and the circuit workspace: the display path for sampled values, the plots panel, the schematic scene, the solver in the inspectors, the motor lab

**Status.** Accepted 2026-10-31, the owner's word on the day it was proposed, as proposed; for release 0.4.0 (Phase 4 of the roadmap). Companion of RFC 012 (the domain) and RFC 013 (the solver); this RFC owns what a person sees of a circuit run in the application and in the runner, and the external design's revision 3 that goes with it.

**Tracks.** Requirements PHY-011, UI-001, UI-004, UI-005, UI-008, UI-009, OBS-003, OBS-005, OBS-009, NFR-004, AC-008 (its form, for the new journey). Requirement IDs refer to `docs/src/requirements.md`.

**Touches.** The display stream of RFC 005 rule 14 (one record kind added); the view model and the scene (RFC 005 section 8): series, a plots panel, a schematic scene for the circuit domain; the inspectors' solver state; the status line's `limited by: solver`; the catalogues in English and Japanese; the learning lab `motor-lab` and the journey J7; the external design revision 3.

## 1. What a person sees of a plant

A network run is watched as messages moving on a topology. A circuit run has nothing moving: it has values changing in time. What the learner wants (PHY-011) is the **waveform** — a sampled value plotted against logical time, the speed settling to the set point, the voltage rising on the capacitor — beside the schematic that says which value it is, and the controller's samples marked on it so that the sample-and-hold of RFC 012 rule 10 is visible rather than explained. The rest of the workspace is the one every domain shares (UI-001, OBS-009): the same frame, run controls, timeline, inspectors and files; only the scene and the plots are the circuit's.

## 2. The data path

**Rule 1.** A **series** is the sequence of samples of one sampled signal: a sensor built-in's `probe` reports (RFC 012 rule 23) or a controller's `control` reports (RFC 012 rule 24), each at the instant of the activation that reported it. There is no other source: the solver's internal steps are not series (OBS-003, the Host reduces first; RFC 012's alternatives), and a value a learner wants to see is a sensor they place, as on a bench.

**Rule 2.** Live, the series come over the display stream (RFC 005 rule 14): the Host writes one `sample` record per report of the `probe` and `control` channels in the batch, before the batch's `instant` record. Otherwise — a run record opened from a file, a window before the kept records after going back (RFC 009 rule 9) — the view model rebuilds the series from the trace records' telemetry through `trace-window`, decoded with the module's or the built-in's layout as the inspector already does (RFC 002 rule 16). Both give the same values; the live path exists so that the plot moves at the display's cadence.

**Rule 3.** The display record kind 3, `sample`, in the 16-byte form of RFC 005 rule 14 (RFC 012 finding F-6):

| Offset | Size | `sample` (kind 3) |
|---|---|---|
| 0 | 1 | 3 |
| 1 | 1 | The field index within the channel's layout (0 for `probe`'s `value`; 0, 1, 2 for `control`'s three) |
| 2 | 2 | The series' actor index in id order (an actor beyond the 65,535th gets no record, as a connection does for `flight`) |
| 4 | 4 | The value as a 32-bit float |
| 8 | 8 | The tick of the report |

The value is a float of 32 bits because the display interpolates and draws; the exact f64 is in the trace and in `trace-window`, and the comparison of RFC 013 rule 20 never reads the display path. The reader's rule, stated now for every later kind: a record of a kind the reader does not know is skipped by its fixed size; the display stream's version stays 1. (A 0.3 reader never meets a 0.4 Host: the bundle ships both halves at one version.)

**Rule 4.** Reduction above the transport (RFC 005 rule 14's `Reduced`): the view model keeps, per series, a ring of the most recent 4,096 samples plus the min and max per pixel column of the visible window, so that drawing costs the window's width and not the sample count; a series that has produced more samples than the ring holds and is scrolled back is filled from `trace-window` on demand. The display ceiling of 2,000 drawn objects per frame (RFC 005 rule 14's measurement) is kept by drawing each series as one polyline per visible window.

**Rule 5.** A series' identity and label: the actor's name and the port or field (`m1.speed`, `pid.output`), its unit from the signal's unit (RFC 012 rule 4) or the controller's layout, formatted by the front end for the language with the unit symbol untranslated (RFC 007 rule 19).

## 3. The workspace

**Rule 6.** The scene (RFC 005 section 8's scene layer) for a scenario that declares the circuit domain is the **schematic**: every actor as a labelled box with its pins and signal ports on its edges; every `net` as a hyperedge drawn as one orthogonal bus joining its pins, the ground net marked; every `signal` as an arrow from its producer to its consumers; a two-domain controller's interface drawn as the topology draws one, so that `cps-loop` shows a wire into a controller into a plant. Positions come from the scenario's `[view]` metadata when present (DATA-001; RFC 004 excludes views from the canonical form) and from a deterministic layered layout otherwise (sources left, sensors right, nets ordered by id); the layout is the scene's, computed once and tested natively like the topology's. Activity (RFC 005 rule 14's `activity` records) marks a controller at its sample; nothing flies.

**Rule 7.** The **plots panel** (OBS-005, UI-005): the waveform scene, drawn by the scene layer on the canvas: a shared time axis in the run's time unit (RFC 007 rule 19), one or more series with their own value axes grouped by unit, the timeline's current instant as a cursor, each controller sample marked on its series as a step (the held value drawn flat until the next sample, which is what the plant sees). The visible window follows the run while it advances and is released when the person scrolls or zooms; "fit" returns it. Series are chosen by toggles beside the panel, all sensors on by default and controllers off; the choice is a per-browser convenience, not state of the run. The panel is visible beside the schematic when the window allows and a tab otherwise (UI-005).

**Rule 8.** The accessible twin (UI-008): the plots panel is one Tab stop; the arrow keys move the cursor by one sample of the focused series (Home and End to its ends; Page keys by the window); a table beside the panel lists every visible series with its value at the cursor and its unit, and is the twin a screen reader reads. Colour is never the only cue: series carry their labels at the cursor and distinct dash patterns; contrast meets WCAG 2.2 AA against the canvas as the external design section 10 requires.

**Rule 9.** Inspectors (UI-004, UI-009). The **world** inspector gains the solver's state from the `solver` events and the run state (RFC 013 rule 17): the current step in ticks and seconds, the method and control in force, the last step's Newton iterations, the rejected steps so far, and the non-convergence state when there is one — as a state with its cause (the device named) and its next step (lower `step`, raise `max-iterations`, `gear2`, or the `hold` policy), never a generic failure. A **built-in**'s inspector shows its parameters and, at the current instant, its pin voltages and current from the last accepted step (through the `actor` query, which the domain answers for a built-in). A **controller**'s shows its parameters and its last `control` report. A **net**'s shows its voltage; a **signal**'s its held value and its producer.

**Rule 10.** The status line's `limited by: solver` (NFR-004; the enumeration value exists since 0.1 and is first used here): the Host's own figure from RFC 005's paced slice (`host.limited-by` as PR-28 made it) names `solver` when the plant's steps took the largest share of a slice that overran its share of the second; beside it the current step is shown (`step 50 µs`), so that a learner who lowered `step` sees why the run slowed.

## 4. The lab and the journey

**Rule 11.** The learning lab of 0.4, `motor-lab`, is RFC 012 rule 27's `motor-loop` with its story in both languages in the application's lab list, as `ospf-lab` and `bgp-lab` are: a `pid` controller guest samples a `dc-motor`'s speed every millisecond and drives a `voltage-source`; the set point is a `constant` built-in's signal; a load step is scheduled at one second. The story walks the learner through the start-up transient, the disturbance, the controller's response, and a set-point change by the `source` intervention on the constant, each with what to look at in the plot and in the inspectors.

**Rule 12.** Journey **J7** (the external design's table): open `motor-lab`; run; watch the speed settle to the set point in the plot and the controller's samples step along it; see the load step at one second and the recovery; change the set point by the intervention; export the run record; replay it and read `matched` or `within-tolerance` with the first differing report named if any. Acceptance: the journey runs in Chromium, Firefox and WebKit by `ci/browser/journeys.js`, and the runner's replay of the browser's record under the native Host prints `matched` or `within-tolerance` (RFC 013 rule 20).

## 5. The catalogues and the external design

**Rule 13.** Every label of the plots panel, the schematic, the inspectors' solver fields and the lab's story is a catalogue key in English and Japanese (RFC 007 rules 14 to 17). The glossary gains the circuit's terms, one translation each, bound in the `term` section: plant (プラント), device model (デバイスモデル), sampled controller (サンプル制御器), net (ネット), signal (信号), sample (サンプル), step (ステップ), convergence (収束), waveform (波形), set point (目標値), load (負荷). Unit symbols stay as written in the scenario (RFC 007 rule 19). The codes of RFC 007 table 4.7 get their `what`, `why` and `next` sentences in both languages.

**Rule 14.** The external design's revision 3, proposed with this RFC and accepted with it: section 3.2's layout row for the scene names the schematic and the plots as this RFC defines them; section 3.6 gains the inspector rows of rule 9; the journey table gains J7; section 5's "solver non-convergence" row cites RFC 013 rule 17's state and next steps; section 9's traceability row for 3.2 names PHY-011 (the audit's finding); the glossary gains rule 13's terms. Nothing of the network's design changes.

## 6. Verification

| Property | Test |
|---|---|
| Series | In the browser run of `motor-loop`, the last value of each series equals the last `probe` or `control` report of the trace to f32 rounding, and the series' sample count equals the report count |
| Both paths | A run record of `motor-loop` opened from a file shows the same series as the live run did, from `trace-window` |
| Reduction | A scenario with a sensor at every tick for a million ticks keeps the display's cadence; the ring and the per-column min and max hold the drawn object count under the ceiling |
| Record kind | The kit's display check (RFC 005 rule 14's ring against messages) reads `sample` records on both paths with the same values; a reader fed a record of an unknown kind skips it |
| Schematic | The scene's layout of every circuit scenario is identical natively and in the browser (computed once, tested natively); `cps-loop` draws the interface and the plant in one scene |
| Keyboard | The plots panel is reached within the keyboard bound of the journeys; the cursor moves by sample; the twin table reads the values |
| Inspectors | After `hostile-diverge`, the world inspector shows the non-convergence state with the device named and the next steps; a built-in's pin voltages equal the trace's last accepted step |
| Status line | A `step` of one tick on `motor-loop` makes `limited by: solver` appear in all three browsers |
| J7 | The journey in Chromium, Firefox and WebKit; the browser's record replayed natively prints `matched` or `within-tolerance` |
| Languages | Every key of rule 13 exists in both catalogues (the build fails otherwise); the lab's story renders in Japanese with the input method's text kept across a language switch |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| Streaming every solver step's full solution to the view | OBS-003; and a learner reads sensors, not matrices. Sensors give the scenario the data rate |
| A separate plotting application or a chart library | The scene layer already draws on the canvas natively and tested; a library would be a dependency under RFC 006 rule 6 and a second drawing path to keep accessible |
| Plot values as f64 on the display path | The record is 16 bytes by RFC 005 rule 14; the display interpolates anyway; the exact values are in the trace |
| A new query for series instead of `trace-window` | `trace-window` already answers any window by re-execution (RFC 009 rule 9); the series are a decoding of what it returns |
| Drawing the plant as the network's topology (nodes and links) | A net joins many pins and a signal is directed; a schematic is what the learner knows |

## Consequences for other RFCs and the requirements

| RFC | Must provide |
|---|---|
| 005 | Rule 14's table gains kind 3 and the reader's unknown-kind rule; section 8's scene layer gains the schematic and the waveform scene |
| 007 | The catalogue keys of rule 13; table 4.7's sentences |
| 009 | `trace-window` as the source of series beyond the kept records (no change) |
| 012 | The `constant` built-in the lab's set point needs (RFC 012 rule 23) |
| 013 | The solver's state and events the inspectors show (rule 17 there) |
| External design | Revision 3 (rule 14) |
| Requirements | PHY-011 is rules 1 to 8; UI-004 and UI-009 for the solver are rule 9; OBS-005's plots exist from 0.4 |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | The ring's size (rule 4) and whether it is a profile field or fixed | The implementing pull request, with the browser figures |
| 2 | Whether the schematic's automatic layout is enough for the lab or `motor-lab` ships `[view]` positions | The lab's pull request; the story's screenshots decide |
| 3 | Whether controllers' series are off by default (rule 7) once the lab exists | The usability test's findings, if any reach 0.4 |
