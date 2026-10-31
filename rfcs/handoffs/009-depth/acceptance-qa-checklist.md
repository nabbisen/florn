# Phase 3 — Acceptance and QA checklist

Entry point: [`README.md`](./README.md). The owner's checks for release 0.3.0, one per pull request, journey J5 and the navigation and branching procedures, the usability test, and the release review. Records go in the verifier's private records, one per ID, as Phase 1's section D says.

## A. The journeys and procedures (external design section 2; RFC 009 section 7)

| ID | Procedure | Expected |
|---|---|---|
| J5 | Import a module from a file (the SDK's `distance_vector` example built locally); read its manifest; place it in an empty scenario beside two bundled routers; connect; run; replay. | The manifest and both hashes are shown before placing; the module is in the palette and under "Files"; the run converges; the replay matches; the exported module's bytes equal the imported file's. |
| P1 (going back) | Open the OSPF lab; run past the break at 1,500 and take a link down yourself at about 2,000; pause at 2,500; "Go to…" 1,200; then "Step to here" on a timeline row near 400. | Before each move the application says what it will discard; afterwards the timeline ends at the chosen instant, your intervention is listed as undone with its instant and can be re-applied; the run continued to the end gives the lab's digest when nothing is re-applied. |
| P2 (branching) | At 1,200 of the lab, "Branch from here"; in the branch, take the chord down at 1,300; run both to the end (the parent from its kept record). | The world inspector names the parent and the instant; the timeline marks it; the two records' chains agree at every stored point up to 1,200 and differ after; each replays to a match on its own. |
| P3 (checkpoints as files) | Export a checkpoint at 2,000 of the lab; close the application; open it, load the scenario, load the checkpoint; run to the end. | The preview said what the file holds; the resumed run ends with the lab's digest; a checkpoint of another scenario is refused with the field named. |
| P4 (swapping) | On a browser whose capacity the status line shows, author a scenario of more routers than that; run resident; then run with swapping. | The first is refused with the need and the capacity; the second runs, the status line says "swapping" with the counts, and the digest equals the native runner's. |

## B. The checks per pull request

| Check | Crate | Procedure | Pass |
|---|---|---|---|
| C23 | checkpoints | `florn run --checkpoint-at 2000 --checkpoint-out c.florn-checkpoint` then `florn resume` natively and through the kit's page in a browser, for every ladder scenario; a run with checkpoints every 100 events and three restores. | The recorded digest in every case; a file one byte over a maximum refused in bounded memory. |
| C24 | cost model 2 | The kit with `--cost-model 2`; the cost benchmark on every engine; a 0.1.0 and a 0.2.0 record replayed on the new Host. | Every digest reproduced but the hostile `budget` scenario's, recorded for version 2; the figures against the target (at most 1.5× protocol-style, 2× compute-style) or the owner's decision recorded in RFC 010's status; the old records matched under version 1. |
| C25 | backward navigation | P1; the kit's back-to-every-chain-point test; a trace window before the last checkpoint. | The expected digests; the window equal to a fully recorded run's; the present unchanged. |
| C26 | branching | P2; the kit's branch tests. | The parent named; replay without the parent; difference after `T` only. |
| C27 | swapping | P4; the kit with pools of one and two on every engine. | One digest; the notices with the numbers. |
| C28 | application | J5, P1, P2, P3 and P4 in the application; install it; the address bar after every journey; switch the language on every new panel. | Pass; the installed application is the same version at the same path and starts offline; the address holds only the origin, the version path and a lab name; every string in both languages. |
| C29 | BGP | `bgp-lab` natively and in three browsers; the story's events at their instants; `bgp-outside`; a frame in the inspector. | The recorded digests; local preference, the hold timer's withdrawal and the return; the refusal logged; the update decoded with its attributes. |
| C30 | release | The release checklist of `ROADMAP.md`; the RFC 006 review's seven steps for the added flows; the exit criteria of Phase 3. | Figures and amendments agree; the review is in the release notes; every exit criterion has its evidence. |

## C. The release review (RFC 006 rule 5)

The flows 0.3 adds: the checkpoint file (a new input under the maxima of section 7, read by the Host's parser), origin storage holding scenarios, modules, run records and checkpoints as a cache (Z4, the user's data, evictable, exported only after the preview), the installable application (the same files at the same immutable path; installation grants nothing), and a second transformation of untrusted modules (the metering pass of RFC 010, fuzzed as the first is). Each is named in the review with its control and its test.

## D. The usability test (roadmap, Phase 3)

Two or three people who did not build florn, each alone with the application and no guidance beyond the labs' stories, asked to do J1, J3, J4, J5 and P1 in that order while thinking aloud; the owner observes and does not help until a person gives up on a step. Recorded per person: each step as done, done with difficulty, or abandoned, with the words the person used for what confused them; the time per step. Findings become issues, each citing the requirement or the external design rule it bears on (UI-009, UI-012 in particular), for 0.3.x or for Phase 4. The test runs at the owner's pace; the release notes of 0.3.0 say whether it ran before the tag.
