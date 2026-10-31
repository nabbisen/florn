# Phase 2 — Acceptance and QA checklist

Entry point: [`README.md`](./README.md). The owner's checks for release 0.2.0, one per pull request, and the journeys of the external design as procedures. Records go in the verifier's private records, one per ID, as Phase 1's section D says.

## A. The journeys (external design §2; requirements §22.2)

| ID | Procedure | Expected |
|---|---|---|
| AC-008 (J1) | Open the bundled lab in the shell; run; watch the topology, the timeline and an inspector. | Neighbours form and routes converge as the lab's story says; the timeline shows protocol events; the inspector shows a router's routes. |
| AC-009 (J3) | During the run, take a link down; watch reconvergence; step through events; inspect a frame; export the run record; replay it. | The intervention is listed with its instant; routes reconverge; the frame decodes; the replay matches at every stored point. |
| AC-010 (J4) | Save the scenario; reload it; replay the saved run with its intervention. | The same digest. |
| AC-011 (J2) | From an empty scenario, place three routers, connect them, set a link's delay and loss, run. | Validation at each field; the run converges; the digest equals the runner's for the saved scenario. |

## B. The checks per pull request

| Check | Crate | Procedure | Pass |
|---|---|---|---|
| C15 | domain interface | Run the kit's `test-domain` rows; run every 0.1 shipped scenario. | Every operation exercised; every 0.1 digest unchanged. |
| C16 | `florn-domain-network` | Run the RFC 008 §12 rows natively and in the browser driver. | Pass on every engine; the codes of rule 18 appear for the built scenarios. |
| C17 | ladder | Run `net-echo` and the triangle scenarios natively and in three browsers; break the link at the stated tick in a second run; replay both. | Recorded digests; reconvergence within the bound; replays matched. |
| C18 | telemetry transport | Run one scenario with the ring (isolated page) and with messages (`sw=0`, headers off). | Identical records; the frame rate holds at 2,000 records per frame. |
| C19 | ui-model, ui-scene | `cargo test` natively; `ci/check-deps.py`. | Pass; no framework crate in either tree. |
| C20 | shell | The journeys J1 to J4 by Playwright in Chromium, Firefox and WebKit; switch the language mid-run; the network log; the forbidden words. | Pass; every text changes without a reload; one origin; no forbidden word. |
| C21 | OSPF and the lab | J1 and J3 over the lab; a scenario using a feature outside the subset. | The story holds; the guest logs its refusal and keeps running. |
| C22 | release | The release checklist of `ROADMAP.md`; the RFC 006 review's seven steps for the added flows. | Figures and amendments agree; the review is in the release notes. |

## C. The release review (RFC 006 rule 5)

The flows 0.2 adds: file import and export in the shell (scenarios, run records), the shared ring between the worker and the page, the catalogues' text. Each is named in the review with its control and its test.
