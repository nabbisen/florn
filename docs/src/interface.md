# The browser interface

The application of release 0.3 (RFC 005 section 8; the external design, section 3): one window, one scenario, one run at a time, in English or Japanese. The Host runs in a worker; the shell on the main thread shows what the view model holds and draws the scene on a canvas.

## Running it

```sh
cargo install --locked wasm-bindgen-cli --version 0.2.126   # once; the version the crate pins
ci/build-shell.sh
cd ci/browser && npm ci && node serve.js
```

Then open `http://127.0.0.1:8765/`. The server sets the isolation headers and the policy of RFC 005 rule 33; a host that cannot set them still serves the application, which then says in its status line that telemetry uses messages. After one visit the application starts offline and says "Available offline".

Each release ships the built application as `florn-app-<version>.tar.gz`. Serve its contents, with the headers above, at a path that does not change between versions (for example `…/0.3.0/`); the application never updates itself in place, and the person chooses the version they open. The SHA-256 of every file of the bundle is in the release notes.

## What is on the screen

| Area | What it holds |
|---|---|
| Header | The scenario's name; Run, Pause, Step with its unit, Speed, Reset, Go to, Actors (all resident, or swapping beyond the capacity); the language; the status line: the run's state, the instant, the interventions, the transport notice, dropped records, the limiting factor (memory and swapping among them), the swapping counts, the measured draw time, the offline state, the version and origin |
| Learning labs and files | The bundled scenarios, the OSPF lab first, each with a story of what to watch and the interventions to try; open and save a scenario, import a module, export a run record after a preview, replay a run record, load a checkpoint; what you opened, saved or imported, kept in this browser as a cache and listed with its size, each to reopen or export |
| Actors | Every actor by name, one Tab stop, arrow keys inside, Enter selects: the canvas's twin |
| Scenario | Authoring: place actors from the palette, connect ports, set attributes and parameters, schedule changes, apply; every problem at its field, the Host's refusals included |
| Topology | The actors and connections; messages in flight; a link down is dashed and a faulted actor is a cross; drag, wheel and keys pan and zoom; a click selects |
| Inspector | The selected actor, connection, message or world; an actor's configuration as fields; the latest telemetry decoded by the module's layouts; the link and actor interventions, each saying when it applies; for the world, the kept checkpoints with their instants and memory, "Take a checkpoint now", "Export", "Branch from here", and the parent of a branch |
| Timeline | The run's record from the trace and the Host's events, filtered by actor and kind; a row with a send opens the message inspector; "Step to here", backward too |
| Interventions | What was asked and when it applied; those undone by going back, to re-apply at their instants or discard; the ended run's, to keep as scheduled changes |
| States | Every notification as a sentence in the chosen language, with its code |

Replay and playback are two words for two things: the application replays (executes again from the inputs, applying the recorded interventions at their instants); it does not play back a trace in this release.

## Going back, branching and checkpoints

"Go to…" with an instant before now, and "Step to here" on an earlier row, move the run back: the Host restores its nearest kept checkpoint and executes forward to the instant. When interventions were applied after that instant, the application says so first and lists them; once you go back they are undone, listed under Interventions with their instants, and each can be re-applied (a new request at its instant) or discarded. Going back from the end of a run works the same way.

The world inspector shows the checkpoints the Host keeps, with their instants and the memory they use. "Take a checkpoint now" adds one at the next instant boundary; "Export" writes one as a `.florn-checkpoint` file after a preview; "Load a checkpoint…" under Files starts a run from such a file, which must be of the same scenario and modules, or it is refused with the field named. "Branch from here" ends the current run and starts a new one from this instant with the same inputs so far; the branch's inspector names the parent and the instant, the timeline marks it, and the parent's record is kept under Files to export or replay. A branch begins at its instant: going back stops there.

## Actors beyond the capacity

The status line's capacity is what this browser can hold. A scenario of more actors than that is refused when Actors is "all resident", with the need and the capacity. Choose "swapping beyond the capacity" to run it anyway: the Host keeps a resident set and brings each actor into a fresh instance before its activation; the status line says "swapping" with the resident and actor counts, "limited by: swapping" when the swaps hold the run back, and the digest is the same as a resident run's on any Host.

## Files kept in this browser

What you open, save, import or export is kept in this browser as a cache, listed under Files with its kind, name and size, and the total; each entry reopens with one click or exports as a file. The file is the durable form: a browser may let the cache go, and the application then says which entry it lost, as a fact. Nothing is kept that you did not open, save, import or export; the bundled labs are not stored.

## Installing the application

The application is installable where the browser offers it: a web application manifest and its icons come from the application's own origin, the service worker is the one that already serves it offline, and nothing else is added. Installation changes no behaviour: the same files, the same version at the same path, the same offline start. No address ever carries content: the URL names at most a bundled lab and the version's path; scenarios, run records and checkpoints travel only as files you export after their preview.

## Language

The language is switched in the header, without a reload and without losing what is being typed. Japanese text enters the form fields through the input method and is drawn on the canvas as typed. Every sentence shown comes whole from the catalogue; nothing is assembled from fragments. The codes are the same in every language.

## What the conformance kit checks

`ci/browser/journeys.js` drives the journeys J1 to J5 and the procedures P1 to P4 of the acceptance criteria in Chromium, Firefox and WebKit (J5: a module imported from a file with both hashes shown, placed, run and replayed, the kept file exporting byte for byte; P1: going back with the discard notice and the undone list, re-applied; P2: a branch with its parent named and both records replaying natively; P3: a checkpoint exported, the application opened again, the checkpoint loaded and run to the lab's digest, another scenario's refused; P4: a scenario beyond the capacity refused resident and run swapping to the runner's digest), and the checks of release 0.2: the lab runs and its routes converge in the views; a link taken down is listed with its instant and the run reconverges; a frame decodes in the inspector; the exported run record replays to a match; the saved scenario reloads and replays the saved run to the same digest; the triangle authored from an empty scenario runs to the digest the runner recorded. It also switches the language mid-run and checks the typed text stays, reaches every landmark by keyboard within a bound, starts the application offline, measures the draw time, and asserts the network log holds one origin.

The [learning lab](./lab.md) chapter walks through the OSPF lab in order.
