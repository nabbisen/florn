# External Design

| | |
|---|---|
| **Document** | florn external design, revision 2 |
| **Status** | Accepted 2026-10-14, with the five decisions of §10 taken. Supersedes the draft of 2026-09-28. |
| **Owner** | nabbisen |
| **Scope** | What a person or a program sees of florn: the system's boundaries, the personas and their journeys, the user interface, the control interface, the files, the failure semantics, the hosting, and the vocabulary. |
| **Not here** | How the Host is built (RFC 005), what a guest is (RFC 002), how a module is admitted and limited (RFC 003), the scenario model and package (RFC 004), determinism (RFC 001), the threat model (RFC 006), the error catalogue (RFC 007). This document cites them and does not restate them. |
| **Change history** | Revision 1, 2026-09-28: draft, with a role model, a workspace model, an authorization matrix, a data model, and a threat model. Revision 2, 2026-10-14: the role, workspace, and authorization models are retired (florn is a single-user local tool, SEC-010); the data model and the threat model moved to the RFCs; the user interface, the personas, the control interface, and the vocabulary are designed from the accepted requirements, RFC 001 to 005, and the Stage 0 proofs of concept. Accepted the same day; §10 records the decisions. |

Requirement identifiers refer to the [requirements specification](./requirements.md). RFC numbers refer to `rfcs/` in the repository.

## 1. System context

florn is one application that runs on one person's machine. At its boundary there are five parties.

| Party | What it is | What crosses the boundary |
|---|---|---|
| **The user** | One person, at a graphical interface or a command line. There are no accounts, roles, or shared workspaces (SEC-010). | Commands, files the user chooses, and what the user reads on the screen. |
| **The application** | The Browser Host with its user interface, served from a versioned address and running entirely in the browser; or the Native Host, headless from the first release and graphical later. | Nothing leaves the machine unless the user exports it (WEB-002). The browser application makes no request to any origin but its own (WEB-009). |
| **Guest modules** | WebAssembly modules the user supplies or that come bundled. Untrusted (SEC-001). | Only the functions of their contract, bound at admission (SEC-002). No clock, file, network, or process access (SEC-003). |
| **Files** | Scenarios, packages, modules, run records, and the diagnostic log, on the user's disk. | The durable form of everything. Storage inside the browser is a cache (WEB-012). |
| **The distribution origin** | The address from which a version of the application is loaded. Each released version has an immutable address and never updates itself under a user (WEB-010). | The application's own files, once; afterwards the application starts offline (WEB-003). |

A sixth party, native external resources — real network interfaces, host files, devices — exists only for the Native Host, only through explicit capability profiles, and only after release 0.6 (MIG-006). Nothing in this document depends on it, and no interface in florn pretends to be portable while depending on it.

## 2. Personas and journeys

### 2.1 Personas

| Persona | Who, in the first releases | Works with | Designed for in |
|---|---|---|---|
| **Learner-operator** (primary) | The owner | The graphical interface: build, run, break, inspect, save, replay | Release 0.2 |
| **Module author** | The developer, then the owner | The guest contract, the SDK, the conformance kit | Release 0.1 |
| **Automation user** | The verifier and continuous integration | The headless runner, manifests, run records, digests | Release 0.1 |
| **Domain developer** | The developer | The core crates, a domain's world and contract | Release 0.2 |
| **Newcomer** | Nobody yet: a person who did not build florn | The same graphical interface | Before any public release |

The learner-operator wants to understand *why* a system behaves as it does by watching it and changing conditions; does not want to read a protocol specification first, to configure a solver, or to guess what a control will do; and succeeds when, after a session, they can explain what they saw. The newcomer is the design target for clarity: the interface shows the minimum first, and the owner's familiarity with florn is not the standard (UI-012).

### 2.2 Journeys

A journey is one complete task. A release is accepted when its journeys work (requirements §22).

| # | Journey | Release | Acceptance |
|---|---|---|---|
| J1 | Open a bundled learning lab, run it, watch neighbours form and routes converge in the topology, the timeline, and the inspector. | 0.2 | AC-008 |
| J3 | Break a link during the run; watch reconvergence; step through events; inspect a packet. Replay the run with its intervention and get the same digest. | 0.2 | AC-009 |
| J4 | Save the scenario to a file; reload it; replay the same run, including the intervention. | 0.2 | AC-010 |
| J2 | Build a topology from nothing: place routers, connect them, set link delay and loss. | 0.2 | AC-011 |
| J5 | Import a module from a file, see its manifest and hash, place it in a topology, run. | 0.3 | §22.3 |
| J6 | Run the same scenario headless on the Native Host and compare the digest with the browser's. | 0.1 | AC-001, AC-003 |

Across all journeys: the language switches without reload and Japanese is typed through an input method (AC-012); no request leaves the origin (AC-013); the application starts offline (AC-014); every control works by keyboard (AC-015); the owner confirms that each control did what its label led them to expect (AC-016).

## 3. The user interface

### 3.1 Principles

| Principle | Statement | Requirement |
|---|---|---|
| Minimum first | The default view shows what the current task needs. Advanced information is one deliberate step away, never hidden and never forced. | UI-012 |
| A control says what it does | Every control's label states its effect, including the unit of a step and the instant at which an intervention applies. No control changes the logical world without saying so. | UI-003, DES-011 |
| States, not failures | A fault, a limit, an invalid scenario, or a capacity problem is shown as a state with a cause and a next step, in the user's language, never as a generic error. | UI-009, CTL-003 |
| Nothing silent | What the application cannot do, it says: shared memory unavailable, a limit approached, actors swapping, a run that diverges on replay. | WEB-007, NFR-004 |
| The same thing has one name | Labels use the glossary term (§9). Replay and playback are two words for two things. | UI-011, glossary |
| Canvas content has a twin | Everything drawn on a canvas exists as page content too: a list or table with the same elements, selectable by keyboard. | UI-008 |
| The display never touches the run | Rendering, pacing, panels, and inspectors cannot change the trace. Inspection never calls a live guest. | UI-007, DES-012 |
| Emulation has limits, and they are stated | Where a result is shown, the interface says that guests run under zero-time execution without an operating system, and what that proves and does not prove. | UI-014 |

### 3.2 Layout

One window, one scenario, one run at a time.

```text
+------------------------------------------------------------------------------+
| florn  [scenario name]   Run ▸  Pause ‖  Step ▾  Speed ▾  Reset  Go to…      |
|         Language ▾   Status: t = 12.000 s · running at 1×   · Available offline |
+---------------------+-------------------------------------+------------------+
| Actors              |                                     | Inspector        |
| (list: the twin of  |   Topology / scene                  |  of the selected |
|  the canvas)        |   (2D canvas; pan, zoom, select)    |  actor, link,    |
|                     |                                     |  message, world  |
+---------------------+-------------------------------------+------------------+
| Timeline  (events in order; filters; "step to here")        | Plots (optional) |
+------------------------------------------------------------------------------+
```

| Area | Content | Requirement |
|---|---|---|
| Header | Scenario name; run controls; language; status line (logical time, pacing, offline state, capacity or swapping notice when relevant) | UI-003, UI-011, WEB-007 |
| Actor list | Every actor by name, in a list that is one Tab stop; arrow keys move inside it; Enter selects. The accessible twin of the canvas. | UI-008 |
| Topology or scene | The domain's view: nodes, connections, moving messages; a waveform scene for the circuit domain; a book and tape for finance. Drawn by the scene layer on a 2D canvas (RFC 005 §8). | UI-001, NET-008, PHY-011, FIN-007 |
| Inspector | The selected element's details (§3.6). Fields are enabled only when something is selected. | UI-004 |
| Timeline | The ordered events of the run (§3.7). | OBS-004 |
| Plots | Telemetry over time, when a domain defines them. Visible together with the rest when the window allows (UI-005); otherwise a tab. | OBS-005, PHY-011 |

Each domain has its workspace with the same frame and run controls and its own scene, inspectors, and plots (UI-001, OBS-009). Panels can be resized; none can be lost: a hidden panel is reachable from the header.

### 3.3 Run controls

| Control | Label (English) | What it does | Effect on the logical world |
|---|---|---|---|
| Run | Run | Advances the run at the chosen speed until paused or until `end`. | None: pacing (RFC 001 rule 9) |
| Pause | Pause | Stops advancing at the next instant boundary. The status line shows the instant. | None |
| Step | Step one event · one instant · one tick · to a tick | Advances by the unit shown on the control (RFC 001 §2.1). The unit is part of the label; the default is one event. | None |
| Speed | 0.1× · 1× · 10× · as fast as possible | Sets pacing: logical time per wall-clock time. | None |
| Reset | Reset | Ends the run and returns to tick 0 with the same inputs. Interventions of the ended run are kept as a list the user can keep as scheduled changes or discard. | A new run |
| Go to time | Go to… | Enters a tick or a time with a unit. Backward: restores the nearest checkpoint and executes forward (RFC 001 rule 53; release 0.3). Forward: runs to there. | None |
| Continue past end | Continue to… | Offered when the run has reached `end`. Sets a later end. | An intervention, recorded (RFC 004 rule 14) |

The status line always shows the current instant as a time with a unit and as a tick on request, whether the run is paused or running, the pacing, and whether the run has interventions ("This run has 2 interventions; replay applies them.").

### 3.4 Authoring

| Action | How | Rule |
|---|---|---|
| Place an actor | From a palette of actor kinds: the bundled modules and built-ins of the scenario's domains, and imported modules. The interface assigns the identity (RFC 004 rule 7) and a name the user can change. | UI-002, BUS-008 |
| Connect | Drag from a port to a port; or select two ports and connect. A connection may join more than two ports where the domain allows (RFC 004 rule 2). | BUS-001 |
| Set attributes | A form from the actor kind's parameters (RFC 002 rule 12) or the domain's connection attributes. Quantities are typed as one token with a unit (`5ms`, `1.2ohm`); the field shows the expected dimension and refuses a bare number with the reason (RFC 004 rules 23 to 25). | DATA-010 |
| Schedule a change | "At 30 s, take this link down" is a scheduled change in the scenario (RFC 004 §3.8), listed under the timeline before the run starts. | RFC 001 §7.1 |
| Validate | Continuously, with every error shown at the field it concerns and in a list, each with its code and text (RFC 004 §7). A scenario with errors cannot run; the Run control says why. | SEC-005, UI-009 |
| Edit during a run | Allowed as interventions: the change applies at the next instant boundary, the interface shows at which, and the run's intervention list grows (§3.5). | DES-011, BUS-008 |

Every scenario has an `end` (RFC 004 rule 14). The interface asks for it when a scenario is created and shows it before the run starts.

### 3.5 Interventions

An intervention is any change the user makes to the running world: breaking or restoring a link, changing a parameter, adding or removing an actor, continuing past `end`.

1. The control that causes it says so ("Take link l31 down at the next instant").
2. The Host applies it at the next instant boundary and reports the instant; the interface shows it ("applied at 30.000 012 s") and adds it to the run's intervention list.
3. The timeline shows it as an event of its own class, distinguishable from the scenario's scheduled changes.
4. Replay applies the recorded interventions at their instants (RFC 001 rule 36); the interface says so before replay starts.
5. The user can turn a run's interventions into scheduled changes of a new scenario ("Save as scenario with these changes").

### 3.6 Inspectors

| Selected | Shows | Never |
|---|---|---|
| Actor | Name and identity; kind and module hash; configuration as declared by the manifest with units; status (running, faulted with the fault kind and metered work, not resident); the faults of this run; the telemetry channels with their latest values and a link to the plot; "open in timeline". | Reads the guest's memory; calls the guest (DES-012). Inspection that needs guest code runs on a copy restored from exported state (RFC 005 rule 19). |
| Connection | Name and identity; its ports; the domain's attributes with units; the messages in flight on it. | |
| Message | Origin, sequence number, instant sent and delivered; the raw bytes; the decoded fields where the domain provides a decoder (NET-007). | Decodes with guest code. |
| World | The domain's state: a solver's step and convergence, a venue's book, the link table. | |

Editing in an inspector before a run changes the scenario; during a run it is an intervention (§3.5).

### 3.7 Timeline

The ordered record of the run: activations, messages, timers, faults, interventions, scheduled changes, and domain events, each with its instant, its origin, and its target. Filters by actor, by kind, and by time window. "Step to here" advances or, from release 0.3, moves backward to the selected event's instant. The timeline reads the trace; it is never a source of truth of its own (RFC 001 rule 44).

### 3.8 Faults, limits, and errors

Every message the Host produces for the user is a stable code with typed parameters; the interface turns it into text in the user's language (CTL-003, RFC 007). The interface presents it as a state with three parts.

| Part | Content |
|---|---|
| What happened | One sentence in the glossary's words. |
| Why | The cause the Host knows: the fault kind, the limit and its value, the field and the rule, the capacity and the need. |
| What to do | The next step the user can take: raise a limit in the scenario, fix the field, reduce the scenario, replace the module, export the diagnostic log. |

| Situation | Example of the state as shown |
|---|---|
| An actor's activation exhausted its budget | "Router r3 stopped at 12.4 s: its activation exceeded the execution budget (10,000,000 units). Its later events are dropped, as the scenario's policy says. Raise `budget` for class `router` in the scenario, or inspect the module." |
| A scenario field is wrong | "`delay` on link l31 needs a unit: 5 is not a duration. Write `5ms`." |
| The scenario is larger than this browser can hold | "This scenario needs 300 resident actors; this browser can hold 124. Run it on the Native Host, or reduce the scenario." (From release 0.3: "…or let florn swap actors, which slows the run by about 10×.") |
| Shared memory is unavailable | One line in the status area: "Telemetry uses messages on this page; shared memory is unavailable." The run is unaffected. |
| Replay diverges | "Replay diverged between records 12,000 and 22,000. The run's inputs or this version of florn differ from the recorded ones. Export both run records to compare." |
| The machine failed the Host | "The run ended: the machine could not provide memory within the limits. This condition is not reproducible; it is recorded in the diagnostic log." (RFC 001 rule 65) |

Three families are never mixed (CTL-004): a value a guest receives from a domain function is the guest's business and is not shown as an error; a fault is a trace record and a state of an actor; an error of the user's input is a state of the form.

### 3.9 Files: save, load, export, import

| What | File | Contains | Rule |
|---|---|---|---|
| Scenario | `name.toml`, the manifest | The scenario (RFC 004 §3) | Readable in a diff; saved and loaded by the user (DATA-004) |
| Package | `name.florn` | The manifest and the modules it references, by hash (RFC 004 §6) | For sharing a world |
| Module | `name.wasm` | A guest with its manifest (RFC 002 §3) | Imported by the user; admitted before it can act |
| Run record | `name.florn-run` | The run's identity and inputs, the digests, optionally the trace and the diagnostic log (RFC 005 rule 29) | For reproducing and comparing a run; never contains scenario text or module bytes |
| Diagnostic log | Part of the run record, or alone | Admission results, faults, limit events, interventions, capacity notices (SEC-008) | Exportable on its own |

Rules the interface follows:

1. **The file is the durable form.** Scenarios and run records kept inside the browser are a cache, and the interface says so where they are listed ("kept in this browser; export to keep") (WEB-012).
2. **Export is explicit and previewed.** Before any file is written, the interface lists what it will contain — every module by name and hash, whether payload bytes are included, the size — and the user confirms (RFC 004 rule 37, RFC 005 rule 30, SEC-007).
3. **Import shows before it admits.** A module imported from a file is hashed and its manifest shown — contract, kind, ports, parameters, needs — before it can be placed; a module without a manifest is refused with the reason (WEB-005, ACT-009). A package lists its modules and their hashes; a partial package lists what is missing (RFC 004 rule 35).
4. **Nothing leaves by itself.** There is no sharing feature that sends anything anywhere; sharing is a file the user hands over (SEC-006).

### 3.10 Replay and playback

| | Replay | Playback |
|---|---|---|
| What it is | Executing the run again from its inputs (RFC 001 rule 49) | Displaying a stored trace; nothing executes |
| Control | "Replay this run" | "Play back the recorded trace" |
| Result shown | "Replay matches the recorded run at every stored point", or the first interval of divergence (§3.8) | The timeline and the scene move; no actor runs |
| Needs | The scenario, the modules, the version of florn that produced the run | The trace with payloads |

The two words are never exchanged, in the interface or in the documentation.

### 3.11 Accessibility

The conformance target of UI-008 is **WCAG 2.2 level AA** for everything that is page content, plus these rules for the canvas, which the standard does not cover:

| Rule | Why |
|---|---|
| Every canvas has an accessible twin (§3.2) and a text description of what it shows, updated when the selection or the counts change. | Text on a canvas is invisible to assistive technology. |
| Every list is one Tab stop; arrow keys move inside it; the focused element is always visible and inside the viewport. | A 500-item list was 500 Tab stops in a proof of concept until this rule existed. |
| Status is never colour alone: a faulted actor has a shape and a word, not only a colour. | UI-008 |
| Live status changes (instant, fault, limit) are announced through a live region, at most a few times per second. | Screen readers; the status line is reactive at 2 Hz, not 60 Hz. |
| Motion can be reduced: with the operating system's reduced-motion preference, messages are shown at their positions without animation, and the frame rate of the scene is lowered. | Vestibular safety; also performance. |
| Keyboard tests of the conformance kit bound the number of Tab presses between landmarks. | A test that is allowed to press Tab 600 times proves nothing. |

### 3.12 Internationalisation

| Rule | Detail |
|---|---|
| Every user-visible string comes from a catalogue; keys are checked at compile time; a missing key is a build error. | UI-011; verified in the UI proof of concept |
| First release: English and Japanese. The language switches without reload and keeps everything the user typed. | AC-012 |
| Strings are whole sentences with named placeholders; no concatenation of fragments; plural forms per language. | Japanese and English order their parts differently. |
| Numbers, units, and times are formatted in the interface from values with units; the logical world knows no locale (RFC 001 rule 63). | DES-010, DATA-010 |
| Text fields work with input methods: the interface never writes into a field while the user composes; the field is set only when the selection changes. | Verified with fcitx5 and mozc in Chromium and Firefox |
| Glossary terms have one translation each, used everywhere (§9). | Consistency |

### 3.13 Performance transparency

The interface says what limits a run (NFR-004): the status line shows "limited by: actors · solver · telemetry · display · memory · swapping" when a run cannot keep its chosen pacing, the probed capacity at start-up ("this browser holds about 124 resident actors"), when actors are being swapped and at what cost, and dropped telemetry records on the display path if any (the trace is never affected). The frame budget is a measured figure, not an assumption: draw time at the 95th percentile at most 8 ms and at most 1% of frames longer than twice the display's cadence, with 500 nodes and 200 moving messages (RFC 005 rule 40).

### 3.14 Structure

The interface has three layers — view model, scene, shell — as RFC 005 §8 defines; the browser shell uses Leptos on a 2D canvas. The native shell is decided at the start of Phase 6, by an evaluation and a one-screen spike like the browser shell's, before any Phase 6 implementation; the roadmap carries that as a scheduled item so that it cannot drift. Everything in §3 is a property of the view model and the scene except the framework-specific rendering, so it holds for both shells.

## 4. The control interface

Every front end — the browser interface, the native interface, the headless runner, a script — is a client of one transport-independent interface of commands, queries, subscriptions, and notifications (CTL-001, RFC 005 §6). This section shows it from the client's side.

| Journey step | Commands and queries |
|---|---|
| Open a scenario | `load-scenario` (manifest or package); `scenario` query for the interface to render; validation notifications |
| Start | `new-run` with a seed the user chose or "choose for me" (the seed is then shown; it is an input); `support` and `capacity` queries at start-up |
| Run, pause, step, speed | `advance` with its unit, `pause`, `set-pacing` — none changes the logical world |
| Break a link | `intervene` with the domain's payload; the notification names the instant at which it applied |
| Inspect | `actor`, `trace-window` queries; `telemetry` and `events` subscriptions; nothing calls a guest |
| Save, export | `export` with a preview first; the file kinds of §3.9 |
| Replay | `replay` with a run record; the comparison notification |
| Go back, branch (0.3) | `checkpoint`, `restore`, `branch` |

Rules that every client can rely on: a command is validated before it acts, and an invalid command changes nothing and produces a coded notification (CTL-002); only `intervene` (and, from 0.3, `restore` and `branch`) changes the logical world, and each such command is recorded as an input (DES-011); every notification is a code with typed parameters, and the client owns the text (CTL-003); a client and a Host of different interface versions refuse each other with a code.

The headless runner (`florn` on the command line) is the automation user's front end: `florn run <scenario> [--seed N]` prints the digest and writes a run record; `florn replay <run> <scenario> --modules <dir>` prints the comparison (a run record carries the hashes of the scenario and the modules, not their bytes, so both are given again and checked; amended 2026-10-20 with release 0.1.0); `florn admit <module>` prints the manifest and the admission result; `florn probe` prints the capacity (AUTO-001). The whole control interface also has a **text form on standard streams**, designed now on one assumption and implemented with the native shell in Phase 6, or earlier if a script needs it first. The assumption: `florn serve --stdio` reads one command or query per line and writes one notification or answer per line, each a JSON object with the same names, fields, and codes as the canonical interface (RFC 005 §6), with binary payloads as base64 and the interface version in the first line each way. Nothing in the interface is text-specific, so this form can be added without changing a client's model; the native shell's in-process transport and this text transport are designed together because they are the two transports the browser does not use.

## 5. Failure and recovery semantics

What the user sees and what is recorded, for every class of failure.

| Failure | Scope | What the user sees | What is recorded | Recovery |
|---|---|---|---|---|
| Guest trap, budget, stack, quota, or contract violation | One activation, then the actor | The actor's state changes to faulted with the kind; the scenario's policy for its later events is shown (ACT-011) | A fault record in the trace; the diagnostic log | Reset the run; raise the limit; replace the module. The run stays reproducible: the fault is part of the trace. |
| Memory refused to a guest within its limit, or the engine's own stack exhausted | The run | "The run ended; this condition is not reproducible" (RFC 001 rule 65) | The diagnostic log, marked not reproducible | Run again; or on the Native Host |
| Invalid scenario | Loading | Every error at its field with code and text | Nothing runs | Fix the fields |
| Module refused at admission | Import or loading | The reason, with the feature, import, or limit named | The diagnostic log | Replace the module; raise the limit |
| Capacity exceeded | Starting | The need and the capacity, before anything is instantiated | The diagnostic log | Reduce; the Native Host; swapping (0.3) |
| Solver non-convergence | A domain's step | A state of the world inspector with the step and the policy applied | A domain event in the trace | Change the solver profile or the step |
| Telemetry dropped on the display path | Display | A count in the status area | Not in the trace | Nothing to recover: the trace is complete |
| Replay divergence | A replay | The first divergent interval and the likely reasons | The comparison in the diagnostic log | Export both run records |
| Corrupt or mismatched checkpoint | Restore (0.3) | Refused with the mismatch named (module hash, admission version, cost model) | The diagnostic log | Use a checkpoint of this run |
| Browser storage evicted | The cache | Items that were only in the browser are gone; the interface said they were a cache | — | Files on disk are unaffected |

The principle: a recoverable failure never changes the meaning of a run silently. Faults are visible in replay; a restored checkpoint preserves scenario and module identity; loss on the display path is never confused with loss of state.

## 6. Hosting and distribution

| Rule | Detail |
|---|---|
| Each released version lives at an immutable address: `…/0.2.0/`. The application never updates itself under a user; the user opens the version they want (WEB-010). | A run recorded under one version can be reproduced under it. |
| The hosting requirement is the isolation headers and the Content Security Policy of RFC 005 rule 33, on every path. | Shared memory for telemetry; nothing cross-origin; no inline or generated script. |
| A host that cannot set headers can still serve florn; the application then uses the message transport and says so in one line. | Measured at no cost at 2,000 telemetry records per frame. |
| After one visit the application starts offline; the interface shows "Available offline" once the cache is complete (WEB-003). | The service worker serves cached responses with the same headers. |
| The interface shows the version it is, where it was loaded from, and whether it is cross-origin isolated, on request. | Support and bug reports. |

## 7. Limits of emulation, as stated to the user

The interface and the documentation state, where a result is shown (UI-014):

> Actors run their real code, but under florn's rules: an activation takes no logical time, there is no operating system, no clock, and no network stack beneath the actor, and the world around it — links, a circuit, a market — is simulated by florn. What you see proves how the actor's logic behaves under those rules. It does not measure the actor's speed on real hardware, and it does not prove that a real network or device would behave the same way. The metered work of each activation is recorded, so that a later version can relate it to time.

## 8. Vocabulary

The glossary of the requirements specification (§1) binds this document, the interface, and the public interfaces. Rules for its use (from the glossary paper test):

| Surface | Rule |
|---|---|
| Interface labels | The glossary term in English; one translation per language in the catalogue. |
| Public interfaces: WIT, manifest, control interface, files | The glossary term in kebab-case: `actor`, `connection`, `scheduled-change`, `intervention`, `run-record`. |
| Code | Type and module names follow the same words. A `Link` type exists only inside the network domain. |
| Documentation | The glossary term; a new term is defined in the glossary before it is used. |

Retired words never appear in the interface: *Pattern 1*, *Pattern 3*, *testbed*, *workspace* as a container, *snapshot* (it is *checkpoint*; *export* is the actor-level operation), *gas* and *fuel* (it is *budget*), *simulation* as a name for the product, *agent* and *node* as synonyms of actor.

Catalogue defaults for the run controls and the main nouns, English and Japanese. The owner corrects the Japanese; the catalogue is the source afterwards.

| English | Japanese |
|---|---|
| Run · Pause · Step · Speed · Reset · Go to… | 実行 · 一時停止 · ステップ · 速度 · リセット · 時刻へ移動… |
| Scenario · Run (noun) · Actor · Connection · Port · Message · Timer · Fault · Intervention · Scheduled change | シナリオ · 実行記録 · アクター · 接続 · ポート · メッセージ · タイマー · フォールト · 介入 · 予定変更 |
| Replay · Playback · Checkpoint · Trace · Digest | 再実行 · 再生 · チェックポイント · トレース · ダイジェスト |
| Learning lab · Package · Run record · Diagnostic log | 学習ラボ · パッケージ · 実行記録ファイル · 診断ログ |

## 9. What this revision removed, and where it went

| Revision 1 content | Now |
|---|---|
| Human roles (owner, designer, operator, observer, publisher, administrators), authorization model, resource scopes, permission set | Retired: florn is a single-user local tool (SEC-010). The personas of §2 describe needs, not permissions. |
| Workspace and member binding | Retired. "Domain workspace" survives as the name of a view. |
| Control, data, and telemetry planes of the browser | RFC 005 §3 and §7. |
| Security objectives, trust zones, threat model, security controls, operational security scenarios | RFC 006 (security baseline for the browser Host), scoped to what is built. |
| Entity model and data lifecycles | RFC 004 (scenario, manifest, package) and RFC 005 (run record, trace, checkpoint). |
| Domain external models | The domain RFCs, from the network domain on. |
| WIT examples | RFC 002 §4 and §7. |
| Sharing and collaboration, import trust rules | §3.9 here, reduced to files and the two trust states *validated* and *rejected*. |
| Open decisions on authentication, identity, server-backed workspaces, signing, retention, role delegation | Closed as out of scope before 1.0 (requirements §3.2); a managed deployment is a separate future design. |

## 10. Open decisions

Decided by the owner on 2026-10-14.

| # | Decision | Outcome |
|---|---|---|
| 1 | The accessibility target (§3.11) | **Accepted:** WCAG 2.2 level AA for page content plus the canvas rules. |
| 2 | Dropped telemetry records on the display path (§3.13) | **Accepted:** shown as a count in the status area. |
| 3 | The Japanese catalogue defaults (§8) | **Confirmed** as defaults, to be improved as appropriate; the catalogue is the source afterwards. |
| 4 | The native shell (§3.14) | **Accepted** for release 0.6, with the decision scheduled: evaluation and spike at the start of Phase 6, before implementation. |
| 5 | A text form of the control interface (§4) | **Designed now on a stated assumption**, implemented with the native shell in Phase 6, or earlier if a script needs it. |

## 11. Traceability

| Section | Requirements |
|---|---|
| 1 | ARCH-007, WEB-001, WEB-002, WEB-009, WEB-010, SEC-001 to SEC-003, SEC-010, MIG-006 |
| 2 | §2.1, §22.2, §22.3 |
| 3.1 | UI-003, UI-007, UI-008, UI-009, UI-011, UI-012, UI-014, DES-011, DES-012, WEB-007, NFR-004 |
| 3.2 | UI-001, UI-004, UI-005, OBS-004, OBS-009, NET-008 |
| 3.3 | UI-003, DES-004, RFC 001 §2.1, RFC 004 rule 14 |
| 3.4 | UI-002, BUS-001, BUS-008, DATA-010, SEC-005, RFC 004 §3, §4, §7 |
| 3.5 | DES-011, RFC 001 §7 |
| 3.6 | UI-004, OBS-005, OBS-006, NET-007, DES-012 |
| 3.7 | OBS-004, NET-009 |
| 3.8 | UI-009, CTL-003, CTL-004, RFC 001 rule 65 |
| 3.9 | DATA-002, DATA-004, DATA-006, WEB-005, WEB-012, SEC-006, SEC-007, SEC-008, ACT-009 |
| 3.10 | DES-007, RFC 001 §11 |
| 3.11 | UI-008 |
| 3.12 | UI-011, DES-010, RFC 001 rule 63 |
| 3.13 | NFR-004, NFR-005, OBS-003, OBS-007 |
| 3.14 | UI-013, RFC 005 §8 |
| 4 | CTL-001 to CTL-004, AUTO-001, AUTO-002, RFC 005 §6 |
| 5 | ACT-005, ACT-011, UI-009, DATA-005, RFC 001 rules 24, 50, 65 |
| 6 | WEB-003, WEB-009, WEB-010, WEB-011, RFC 005 §7 |
| 7 | UI-014, NFR-014 |
| 8 | glossary, PKG-007 |
