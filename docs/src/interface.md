# The browser interface

The application of release 0.2 (RFC 005 section 8; the external design, section 3): one window, one scenario, one run at a time, in English or Japanese. The Host runs in a worker; the shell on the main thread shows what the view model holds and draws the scene on a canvas.

## Running it

```sh
cargo install --locked wasm-bindgen-cli --version 0.2.126   # once; the version the crate pins
ci/build-shell.sh
cd ci/browser && npm ci && node serve.js
```

Then open `http://127.0.0.1:8765/`. The server sets the isolation headers and the policy of RFC 005 rule 33; a host that cannot set them still serves the application, which then says in its status line that telemetry uses messages. After one visit the application starts offline and says "Available offline".

## What is on the screen

| Area | What it holds |
|---|---|
| Header | The scenario's name; Run, Pause, Step with its unit, Speed, Reset, Go to; the language; the status line: the run's state, the instant, the interventions, the transport notice, dropped records, the limiting factor, the measured draw time, the offline state, the version and origin |
| Learning labs and files | The bundled scenarios, the OSPF lab first, each with a story of what to watch and the interventions to try; open and save a scenario, import a module, export a run record after a preview, replay a run record |
| Actors | Every actor by name, one Tab stop, arrow keys inside, Enter selects: the canvas's twin |
| Scenario | Authoring: place actors from the palette, connect ports, set attributes and parameters, schedule changes, apply; every problem at its field, the Host's refusals included |
| Topology | The actors and connections; messages in flight; a link down is dashed and a faulted actor is a cross; drag, wheel and keys pan and zoom; a click selects |
| Inspector | The selected actor, connection, message or world; an actor's configuration as fields; the latest telemetry decoded by the module's layouts; the link and actor interventions, each saying when it applies |
| Timeline | The run's record from the trace and the Host's events, filtered by actor and kind; a row with a send opens the message inspector; "Step to here" |
| Interventions | What was asked and when it applied; the ended run's, to keep as scheduled changes |
| States | Every notification as a sentence in the chosen language, with its code |

Replay and playback are two words for two things: the application replays (executes again from the inputs, applying the recorded interventions at their instants); it does not play back a trace in this release.

## What the conformance kit checks

`ci/browser/journeys.js` drives the journeys J1 to J4 of the acceptance criteria in Chromium, Firefox and WebKit: the lab runs and its routes converge in the views; a link taken down is listed with its instant and the run reconverges; a frame decodes in the inspector; the exported run record replays to a match; the saved scenario reloads and replays the saved run to the same digest; the triangle authored from an empty scenario runs to the digest the runner recorded. It also switches the language mid-run and checks the typed text stays, reaches every landmark by keyboard within a bound, starts the application offline, measures the draw time, and asserts the network log holds one origin.
