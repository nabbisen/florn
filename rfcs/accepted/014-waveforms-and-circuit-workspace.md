# RFC 014 — Waveforms and the circuit workspace: the display path for sampled values, the plots panel, the schematic scene, the solver in the inspectors, the motor lab

**Status.** Accepted 2026-10-31, the owner's word on the day it was proposed, as proposed; for release 0.4.0 (Phase 4 of the roadmap). **Amended 2026-10-31 (AD-25)** after the architect's independent entry review: the sources of series are corrected (there is no playback of a run record from a file, and `trace-window` carries no report payloads yet; rules 1 and 2); the display record `sample` gets a field index that the view model can compute and the stream's version, which no reader checked, becomes 2 and is checked (rule 3); the units of a controller's series, the Host figure behind `limited by: solver` and the inspector of a built-in are given a source (rules 5, 9 and 10); the scope of the user-interface work that the first text left implicit is stated (rules 15 and 16); the claim that a missing translation key fails the build is replaced by a test (rule 13); and the drafting errors are corrected. Rule numbers of the first text are kept; new rules are numbered from 15. Companion of RFC 012 (the domain) and RFC 013 (the solver); this RFC owns what a person sees of a circuit run in the application and in the runner, and the external design's revision 3 that goes with it, which is written and proposed to the owner (`docs/src/external-design.md`). A second reader checked the amended text; its corrections are in rules 9, 12 and 14 and the verification table: the inspectors are answered by a new query, and a replay without payloads is `needs-payloads`.

**Tracks.** Requirements PHY-011, UI-001, UI-004, UI-005, UI-008, UI-009, OBS-003, OBS-009, NFR-004, AC-008 (its form, for the new journey). PHY-011 and UI-005 are the plots; OBS-005 (actor inspection) is the inspector of rule 9. Requirement IDs refer to `docs/src/requirements.md`.

**Touches.** The display stream of RFC 005 rule 14 (one record kind added, the version raised); the `trace-window` query and a new `inspect` query of the control interface (additions within version 1); the view model and the scene (RFC 005 section 8): series, a plots panel, a schematic scene for the circuit domain, a workspace per domain; the authoring draft; the inspectors' solver state; the status line's `limited by: solver`; the catalogues in English and Japanese; the learning lab `motor-lab` and the journey J7; the external design revision 3.

## 1. What a person sees of a plant

A network run is watched as messages moving on a topology. A circuit run has nothing moving: it has values changing in time. What the learner wants (PHY-011) is the **waveform** — a sampled value plotted against logical time, the speed settling to the set point, the voltage rising on the capacitor — beside the schematic that says which value it is, and the controller's samples marked on it so that the sample-and-hold of RFC 012 rules 10 and 11 is visible rather than explained. The rest of the workspace is the one every domain shares (UI-001, OBS-009): the same frame, run controls, timeline, inspectors and files; only the scene and the plots are the circuit's.

## 2. The data path

**Rule 1.** A **series** is the sequence of samples of one sampled value: a sensor built-in's `probe` reports (RFC 012 rule 23) or a controller's `control` reports (RFC 012 rule 24), each at the instant of the activation that reported it. There is no other source: the solver's internal steps are not series (OBS-003, the Host reduces first; RFC 012's alternatives), and a value a learner wants to see is a sensor they place, as on a bench.

**Rule 2.** Live, the series come over the display stream (RFC 005 rule 14): the Host writes one `sample` record per `f64` field of a report of the `probe` and `control` channels in the batch, before the batch's `instant` record. Otherwise — a window before the kept records after going back (RFC 009 rule 9) — the view model rebuilds the series from the `trace-window` query, whose entries **gain the reports' payloads** (the Host keeps them for the window it re-executes with recording on; an addition within control version 1, recorded in the compatibility page), decoded with the channel's layout as the inspector already does (RFC 002 rule 16). A window is requested in pieces of at most the 10,000 records of RFC 006 section 7. The application replays a run record and does not play back a trace (the interface chapter; external design section 3.10): a replay produces the live path again, so there is no third source. Both paths give the same values, the live one to the precision of an `f32` and the other exactly.

**Rule 3.** The display record kind 3, `sample`, in the 16-byte form of RFC 005 rule 14 (RFC 012 finding F-6):

| Offset | Size | `sample` (kind 3) |
|---|---|---|
| 0 | 1 | 3 |
| 1 | 1 | The index of the `f64` field among the actor's declared `f64` fields, counted in channel order then field order (so the view model computes it from the channel summaries it holds) |
| 2 | 2 | The series' actor index in id order (an actor beyond the 65,535th gets no record, as a connection does for `flight`) |
| 4 | 4 | The value as a 32-bit float |
| 8 | 8 | The tick of the report |

The value is a float of 32 bits because the display interpolates and draws; the exact f64 is in the trace and in `trace-window`, and the comparison of RFC 013 rule 20 never reads the display path. **Reader rules**, new code in this release, not a statement of existing behaviour: a record of a kind the reader does not know is skipped by its fixed size (today `decode_all` returns `None` and the shell drops the batch); the stream's version, declared as 1 and read nowhere, becomes **2**, is carried in the ring's header and in the first word of every message batch, and a reader of a newer version refuses the stream before reading a record, as the compatibility page already claims. The `Reduced` fold keeps every `sample` of a batch (they are bounded by the sensors' periods) and drops the oldest at the ring's capacity.

**Rule 4.** Reduction above the transport (RFC 005 rule 14's `Reduced`): the view model keeps, per series, a ring of the most recent 4,096 samples plus the min and max per pixel column of the visible window, so that drawing costs the window's width and not the sample count; a series that has produced more samples than the ring holds and is scrolled back is filled from `trace-window` on demand. The display ceiling of 2,000 drawn objects per frame (RFC 005 rule 14's measurement) is kept by drawing each series as one polyline per visible window.

**Rule 5.** A series' identity and label: the actor's name and the port or field (`m1.speed`, `pid.output`). Its unit comes from the signal's unit (RFC 012 rule 4) for a sensor and from the controller's `control-units` (RFC 012 rule 18) for a controller; a series with no unit is drawn on the axis of dimensionless values. The unit is formatted by the front end for the language with the unit symbol untranslated (RFC 007 rule 19).

## 3. The workspace

**Rule 6.** The scene (RFC 005 section 8's scene layer) for a scenario that declares the circuit domain is the **schematic**: every actor as a labelled box with its pins and signal ports on its edges; every `net` as a hyperedge drawn as one orthogonal bus joining its pins, the ground net marked; every `signal` as an arrow from its producer to its consumers; a two-domain controller's interface drawn as the topology draws one, so that `cps-loop` shows a wire into a controller into a plant. Positions come from the scenario's `[views]` metadata when present (DATA-001; RFC 004 section 3.9 excludes views from the canonical form) and from a deterministic layered layout otherwise (sources left, sensors right, nets ordered by id); the layout is the scene's, computed once and tested natively like the topology's. Activity (RFC 005 rule 14's `activity` records) marks a controller at its sample; nothing flies.

**Rule 7.** The **plots panel** (UI-005, PHY-011): the waveform scene, drawn by the scene layer on the canvas: a shared time axis in the run's time unit (RFC 007 rule 19), one or more series with their own value axes grouped by unit, the timeline's current instant as a cursor, each controller sample marked on its series as a step (the held value drawn flat until the next sample, which is what the plant sees). The visible window follows the run while it advances and is released when the person scrolls or zooms; "fit" returns it. Series are chosen by toggles beside the panel, all sensors on by default and controllers off; the choice is a per-browser convenience, not state of the run. The panel is visible beside the schematic when the window allows and a tab otherwise (UI-005).

**Rule 8.** The accessible twin (UI-008): the plots panel is one Tab stop; the arrow keys move the cursor by one sample of the focused series (Home and End to its ends; Page keys by the window); a table beside the panel lists every visible series with its value at the cursor and its unit, and is the twin a screen reader reads. Colour is never the only cue: series carry their labels at the cursor and distinct dash patterns; contrast meets WCAG 2.2 AA (external design section 3.11) against the canvas.

**Rule 9.** Inspectors (UI-004, UI-009). The **world** inspector gains the solver's state from the world inspection (RFC 012 rule 31 and finding F-12; RFC 013 rule 17): the current step proposal in ticks and seconds, the method and control in force, the last advance's steps and Newton iterations, the rejected attempts so far, and the non-convergence state when there is one — as a state with its cause (the device named) and its next step (raise `newton.max-iterations`, choose `gear2`, lower `min-step` where it is above one tick, check the device model, or use the `hold` policy), never a generic failure. A **built-in**'s inspector shows its kind, its parameters and, from the last accepted step, its pin voltages and current: the Host answers the new `inspect` query through the domain (RFC 012 rule 31 and finding F-12) with a list of named fields, text and, for a real, its value and unit, which the front end formats. A **controller**'s shows its parameters and its last `control` report. A **net**'s shows its voltage; a **signal**'s its held value, its unit and its producer; the same query answers both.

**Rule 10.** The status line's `limited by: solver` (NFR-004; the enumeration value exists since 0.1 and nothing emits it): the Host measures the time its domains spend handling `advance` events with `Platform::monotonic_us`, as PR-28 measures a paced slice for `swapping`, and names `solver` when that time is the largest share of a slice that overran its share of the second; beside it the current step is shown (`step 50 µs`), so that a learner who lowered `step` sees why the run slowed.

## 4. The lab and the journey

**Rule 11.** The learning lab of 0.4, `motor-lab`, is RFC 012 rule 27's `motor-loop` with its story in both languages in the application's lab list, as `ospf-lab` and `bgp-lab` are: a `pid` controller guest samples a `dc-motor`'s speed every millisecond and drives a `voltage-source`; the set point is a `constant` built-in's signal and the load torque another; a load step is a scheduled change of the second at one second. The story walks the learner through the start-up transient, the disturbance, the controller's response, and a set-point change by the `source` intervention on the first `constant`, each with what to look at in the plot and in the inspectors. The scenario gives the sensors lower identities than the controllers, though the order at a tick is by structure (RFC 012 rule 10).

**Rule 12.** Journey **J7** (the external design's table): open `motor-lab`; run; watch the speed settle to the set point in the plot and the controller's samples step along it; see the load step at one second and the recovery; change the set point by the intervention; **export the run record with its trace and payloads** (the preview says so, RFC 005 rule 30); replay it and read `matched` or `within-tolerance`, with the first differing report named if any. Acceptance: the journey runs in Chromium, Firefox and WebKit by `ci/browser/journeys.js`, and the runner's replay of the browser's record under the native Host prints `matched` or `within-tolerance` (RFC 013 rule 20); a record exported without payloads is compared by its chains alone: `matched` when they agree, as they are expected to, and `needs-payloads`, with the interface saying why, when they do not.

## 5. The catalogues and the external design

**Rule 13.** Every label of the plots panel, the schematic, the inspectors' solver fields and the lab's story is a catalogue key in English and Japanese (RFC 007 rules 14 to 17). The glossary gains the circuit's terms, one translation each: plant (プラント), device model (デバイスモデル), sampled controller (サンプル制御器), net (ネット), signal (信号), sample (サンプル), step (ステップ), convergence (収束), waveform (波形), set point (目標値), load (負荷); unit symbols stay as written in the scenario (RFC 007 rule 19). The codes of RFC 007 table 4.7 get their `what`, `why` and `next` sentences in both languages. **A missing key in either language of the interface labels is a failing test, not a build error**: the label compiler turns it into a deprecation warning with a fallback and the generated module allows deprecations, so the claim of the first text (and of external design section 3.12) was not true; the unit adds a test that the key sets of `locales/en.json` and `locales/ja.json` are equal, as `tests/catalogue.rs` does for the codes.

**Rule 14.** The external design's revision 3 is written (`docs/src/external-design.md`) and proposed to the owner for acceptance as revision 2 was; the first text said it was proposed with this RFC, and it was not written then: section 3.2's layout row for the scene names the schematic and the plots as this RFC defines them; section 3.6 gains the inspector rows of rule 9; the journey table gains J7; section 5's "solver non-convergence" row cites RFC 013 rule 17's state and next steps; **section 11's** traceability row for 3.2 names PHY-011 (the audit's finding); the glossary gains rule 13's terms; and the Newcomer persona's "designed for in" cell is re-planned with the usability test's findings (audit L1). Nothing of the network's design changes.

## 6. What the user-interface work contains

**Rule 15.** The first text named a panel, a scene and a record kind; the entry review of the code found what they stand on is not there yet, and the units of the plan carry it:

1. *The authoring draft keeps built-ins.* `Draft::from_scenario` skips an actor without a module and `to_toml` writes only `kind = "module"`, so a scenario made of built-ins cannot be loaded, edited and saved. The draft reads and writes `kind = "builtin"` with its `config`.
2. *A workspace per domain.* The view model has no notion of one; the layout is fixed. It gains the choice of the scene and the panels by the domains a scenario declares (UI-001), the network's remaining as they are.
3. *Series with retention* (rule 4): today the view model keeps only the latest report of a channel.
4. *Plot primitives in the scene layer*: a joined polyline, a rectangle, an axis, ticks and a clip, and a second viewport (time against value) beside the topology's. Today the scene draws discs, crosses, lines, circles and centred labels.
5. *The timeline reads a domain's channels by their declared layouts*, not by a list of the network's channel names.
6. *The draw budget is asserted as documented* (external design section 3.13, RFC 005 rule 40: p95 at most 8 ms with 500 nodes and 200 moving messages): a journey with a scenario that large for the schematic and, with series running, for the plots; today the journey asserts 16 ms on three actors.

**Rule 16.** Not in 0.4: *authoring a plant graphically*. A circuit scenario is loaded (a bundled lab, a file), its parameters, its profile and its interventions are edited, and it is saved; placing built-ins and drawing nets is a later release, and the release notes say so (UI-002 for the circuit domain is recorded as a limitation, not met). The roadmap's Phase 4 scope names the waveform view and one controller against one plant.

## 7. Verification

| Property | Test |
|---|---|
| Series | In the browser run of `motor-loop`, the last value of each series equals the last `probe` or `control` report of the trace to `f32` rounding, and the series' sample count equals the report count |
| Two paths | After going back past the kept records, the series rebuilt from `trace-window` equal the live ones to `f32` rounding; the window is requested in pieces of at most 10,000 records |
| Reduction | A scenario with a sensor at every tick for a million ticks keeps the display's cadence; the ring and the per-column min and max hold the drawn object count under the ceiling |
| Record kind and version | The kit's display check (RFC 005 rule 14's ring against messages) reads `sample` records on both paths with the same values; a reader fed a record of an unknown kind skips it by size; a reader fed a stream of a newer version refuses it before a record |
| Schematic | The scene's layout of every circuit scenario is identical natively and in the browser (computed once, tested natively); `cps-loop` draws the interface and the plant in one scene |
| Keyboard | The plots panel is reached within the keyboard bound of the journeys; the cursor moves by sample; the twin table reads the values |
| Inspectors | After `hostile-diverge`, the world inspector shows the non-convergence state with the device named and the next steps; a built-in's pin voltages equal the last sample of a sensor across the same pins when read at that sample's tick; opening an inspector changes no digest (RFC 001 rule 39) |
| Status line | A `step` of one tick on `motor-loop` makes `limited by: solver` appear in all three browsers |
| Draft | `motor-loop` loaded into the draft and saved reads back to the same scenario hash |
| Draw budget | The documented budget holds with 500 nodes and 200 messages, and with the plots running |
| J7 | The journey in Chromium, Firefox and WebKit; the browser's record replayed natively prints `matched` or `within-tolerance`; one exported without payloads prints `matched` when the chains agree, and `needs-payloads` when they do not |
| Languages | The key sets of the two label files are equal (the test of rule 13); the lab's story renders in Japanese with the input method's text kept across a language switch |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| Streaming every solver step's full solution to the view | OBS-003; and a learner reads sensors, not matrices. Sensors give the scenario the data rate |
| A separate plotting application or a chart library | The scene layer already draws on the canvas natively and tested; a library would be a dependency under RFC 006 rule 6 and a second drawing path to keep accessible |
| Plot values as f64 on the display path | The record is 16 bytes by RFC 005 rule 14; the display interpolates anyway; the exact values are in the trace |
| A new query for series instead of `trace-window` | `trace-window` already answers any window by re-execution (RFC 009 rule 9); the series are a decoding of what it returns, once it carries the payloads |
| Opening a run record from a file as a playback source | Playback needs the trace with payloads and the application does not play back (external design section 3.10); replay already reproduces the live path |
| Drawing the plant as the network's topology (nodes and links) | A net joins many pins and a signal is directed; a schematic is what the learner knows |
| Graphical authoring of plants in 0.4 | Not in the roadmap's scope for the phase and a large piece of interface by itself (rule 16) |

## Consequences for other RFCs and the requirements

| RFC | Must provide |
|---|---|
| 005 | Rule 14's table gains kind 3 and the reader rules, and the stream's version 2; section 8's scene layer gains the schematic, the plot primitives and the waveform scene; rule 26's `trace-window` entries carry report payloads and a new `inspect` query answers for built-ins, nets, signals and the world (RFC 012 finding F-12); rule 40's budget is asserted as written |
| 007 | The catalogue keys of rule 13; table 4.7's sentences; the label test |
| 009 | `trace-window` as the source of series beyond the kept records, with payloads |
| 012 | The `constant` built-in the lab's set point and load need (RFC 012 rule 23); the `inspect` query (rule 31, finding F-12) |
| 013 | The solver's state the inspectors show (rule 17 there), through the world inspection |
| External design | Revision 3 (rule 14) |
| Requirements | PHY-011 is rules 1 to 8; UI-004 and UI-009 for the solver are rule 9; UI-005's plots exist from 0.4; UI-002 for the circuit domain is a recorded limitation (rule 16) |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | The ring's size (rule 4) and whether it is a profile field or fixed | The implementing pull request, with the browser figures |
| 2 | Whether the schematic's automatic layout is enough for the lab or `motor-lab` ships `[views]` positions | The lab's pull request; the story's screenshots decide |
| 3 | Whether controllers' series are off by default (rule 7) once the lab exists | The usability test's findings, if any reach 0.4 |
