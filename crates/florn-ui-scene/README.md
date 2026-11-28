# florn-ui-scene

The scene of florn's user interface (RFC 005 rule 37): what to draw, as lists of lines, circles, marks and labels in screen pixels, each with a style, built from the view model for one instant. It depends on `florn-ui-model` only and compiles and tests natively.

| Module | What |
|---|---|
| `lib.rs` | `build(view model, instant)`: the links (dashed when down), the hubs of lans, the nodes (a cross when faulted, besides the colour), the flights at the progress their sent tick and delay give for the instant, the labels when there is room; `hit_test` for what is under a point; `fit_all` for the viewport; the `Description` for the canvas's accessible twin |
| `plot.rs` | `waveforms(view model)`: the plots of RFC 014 rule 7 as one lane for each unit with its value axis, a shared time axis, the instant's cursor and each series as one polyline over the window (a step for a controller's), drawn from the series' per-column extremes when the window holds more samples than it has columns; the primitives it uses (`Polyline`, `Rects`, `Axis`, `Clip`) are in `lib.rs` beside `Item::objects`, which counts what a frame draws against the display's ceiling |
| `layout.rs` | Where nodes and hubs go: a ring for small topologies, a grid beyond, the same every time |

A shell interpolates the instant between the Host's batches and builds a scene per frame: the frames move without one update per event (NET-008). Flights beyond the display's ceiling of 2,000 are counted, not drawn (RFC 005 rule 14).
