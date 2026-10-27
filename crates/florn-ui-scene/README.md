# florn-ui-scene

The scene of florn's user interface (RFC 005 rule 37): what to draw, as lists of lines, circles, marks and labels in screen pixels, each with a style, built from the view model for one instant. It depends on `florn-ui-model` only and compiles and tests natively.

| Module | What |
|---|---|
| `lib.rs` | `build(view model, instant)`: the links (dashed when down), the hubs of lans, the nodes (a cross when faulted, besides the colour), the flights at the progress their sent tick and delay give for the instant, the labels when there is room; `hit_test` for what is under a point; `fit_all` for the viewport; the `Description` for the canvas's accessible twin |
| `layout.rs` | Where nodes and hubs go: a ring for small topologies, a grid beyond, the same every time |

A shell interpolates the instant between the Host's batches and builds a scene per frame: the frames move without one update per event (NET-008). Flights beyond the display's ceiling of 2,000 are counted, not drawn (RFC 005 rule 14).
