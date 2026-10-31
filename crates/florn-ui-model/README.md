# florn-ui-model

The view model of florn's user interface (RFC 005 rule 37): what is shown, over the control interface's types only. A shell feeds it every message the Host sends and every display batch, sends what its outbox asks, and draws the scene from it. It compiles and tests natively with no framework, `web-sys`, `js-sys` or `wasm-bindgen`; the dependency check in CI keeps it so.

## What is here

| Module | What |
|---|---|
| `lib.rs` | `ViewModel`: the scenario, the run as the status line shows it, the status notices (transport, dropped records, limiting factor), the selection and the inspector, the interventions asked and applied, the outbox; what a person does (`open_scenario`, `new_run`, `run`, `pause`, `step`, `step_to`, `goto` and `confirm_go_back` with the undone interventions to `reapply` or `discard_undone`, `rewind`, `branch`, `take_checkpoint`, `load_checkpoint`, `set_residency`, `set_link`, `set_node`, `intervene`, `replay`, export with `keep_only`, the kept files through `set_files` and `evicted`) and what the Host says (`apply`, `display`) |
| `topology.rs` | The actors as nodes and the connections as links, each link's state from the domain's events, the flights and the activity from the display stream, the viewport |
| `timeline.rs` | The ordered record of the run from the trace and the notifications: activations, faults, interventions, scheduled changes, domain events, the reports the view reads; filters by actor, kind and window |
| `inspector.rs` | The selected actor, connection, message or world: configuration as fields, status and faults, the latest reports decoded by the module's layouts, a message's bytes and decoded fields |
| `draft.rs` | Authoring: a scenario as fields, its manifest text, the Host's refusals attached to the fields they name |
| `fields.rs` | A field a person types into: the text is theirs until committed, the model never writes into it while they type |

No text a person reads is here: every state is a code, a name or a number the shell's catalogue renders.

The scene that draws it is `florn-ui-scene`; the browser shell is `florn-ui-web`.
