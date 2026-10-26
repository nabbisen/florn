# Phase 2 — Task breakdown and pull-request plan

Entry point: [`README.md`](./README.md). One pull request per task, in this order unless marked independent. Each PR names the RFC rules it implements, the tests it adds, and the checklist items the verifier runs. The progress table at the end is updated with every merge.

| # | Task | Implements | Tests added | Verifier checks | Size |
|---|---|---|---|---|---|
| PR-15 | **Domain interface.** The `Domain` trait, the composite world, domain streams, intervention kinds, domain events, built-ins, checkpoint state; `type` on connections; the adapters' signature tables; a kit `test-domain` | RFC 008 §2; RFC 004 rule 19 amendment; RFC 005 rules 3, 7 | The `test-domain` over every operation; every 0.1 digest unchanged | C15 | L |
| PR-16 | **Network domain and contract.** `florn-domain-network`; `florn:network@0.1.0` as the contract of record; the SDK's `router` bindings; codes; the `blackhole` built-in; decoders scaffold | RFC 008 §3 to §9; RFC 007 table 4.6 | RFC 008 §12 rows but the ladder's, natively and in three browsers | C16 | L |
| PR-17 | **Guest ladder.** `net-echo`, `distance-vector`; their scenarios with recorded digests in the kit; their decoders; the telemetry layouts in use | RFC 008 rules 17, 20 | The ladder row of §12; J3's digest equality for a link break and its replay | C17 | M |
| PR-18 | **Telemetry transport.** The 16-byte records, the ring and the message fallback, the reduction, the notifications | RFC 005 rule 14 | Identical records on both paths; 2,000 records per frame | C18 | M |
| PR-19 | **View model and scene.** `florn-ui-model`, `florn-ui-scene` over the control interface, tested natively | RFC 005 rules 37, 41 | The rule 41 tests without a browser; the dependency check | C19 | L |
| PR-20 | **Browser shell.** Leptos on the main thread; every section of the external design §3; English and Japanese; versioned hosting and the offline signal; Playwright journeys J1 to J4 in CI | RFC 005 rules 35, 38 to 41; RFC 007 rules 14 to 20 | J1 to J4 in three browsers; the catalogue build; the forbidden words; the network log | C20 (AC-008 to AC-011) | XL |
| PR-21 | **OSPF subset and the lab.** The guest of rule 21 with its decoder; the bundled learning lab of NET-010 with its story and interventions | RFC 008 rule 21; NET-004, NET-010 | The lab runs through J1 and J3 on every engine; the subset's refusals are logged | C21 | L |
| PR-22 | **Release 0.2.0.** Documentation in both languages, the security review, release notes with figures, RFC 008 to `rfcs/done/`, the tag, the crates published | RFC 006 rule 5; RFC 000 | The release checklist | C22; the owner accepts Phase 2 against J1 to J4 | M |

Independent of the order above: PR-18 may follow PR-15 directly; PR-19 may start beside PR-16 once the control interface's 0.2 additions are fixed by PR-15.

## Progress

| PR | State | Merged | Verifier record |
|---|---|---|---|
| PR-15 | merged | `e31cce4`, 2026-10-27 | C15 pending |
| PR-16 | not started | | |
| PR-17 | not started | | |
| PR-18 | not started | | |
| PR-19 | not started | | |
| PR-20 | not started | | |
| PR-21 | not started | | |
| PR-22 | not started | | |
