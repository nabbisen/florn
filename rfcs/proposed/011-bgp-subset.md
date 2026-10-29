# RFC 011 — The BGP subset and its learning lab

**Status.** Proposed 2026-10-29 for release 0.3.0 (Phase 3 of `ROADMAP.md`: "BGP guest"). The subset's boundary is the decision requested from the owner (open question 1).

**Tracks.** Requirements NET-005, NET-007, NET-010, NET-012; the 0.3 row of the acceptance table ("a BGP guest exchanges routes"). Requirement IDs refer to `docs/src/requirements.md`.

**Touches.** RFC 008 rule 20 (the ladder's table gains a rung), rule 16 (a protocol name and its decoder), section 7 (telemetry layouts), open question 4 (left as it is); nothing in the contract `florn:network@0.1.0`, the domain, the kernel or the Host.

## 1. Why a subset, and which

BGP is the second protocol family the roadmap names, chosen because it is unlike OSPF in every way a learner should see: policy over topology, paths over costs, sessions over adjacencies, a decision process with named tie-breakers, and failure by timers rather than by flooding. The whole protocol is thousands of pages of behaviour, most of it about transport, capabilities and operator policy. As with OSPF (RFC 008 rule 21), the guest implements a declared subset and refuses what lies outside it with a logged reason; nothing is approximated.

The subset is the smallest set that lets the lab show a path chosen by local preference, a tie broken by AS-path length and by MED, a withdrawal after a hold timer, and the return of the preferred path after a repair.

## 2. The subset

**Rule 1.** Sessions. A BGP router is one autonomous system (`as`, a parameter); a session is one point-to-point link whose two ends are in different autonomous systems (external BGP). The link is the transport: there is no TCP, no port, no collision detection; a session's frames are the four messages below over the interface the link uses. A link between two routers of one autonomous system is refused by both with a logged reason (internal BGP is outside the subset). A second router on an interface (a lan) is refused once per port, as the OSPF guest does.

**Rule 2.** The session machine has four states, `idle`, `open-sent`, `open-confirm`, `established`, and these transitions:

| From | On | To | Sends |
|---|---|---|---|
| `idle` | the link up (`on-link`) | `open-sent` | `open` |
| `open-sent` | a valid `open` | `open-confirm` | `keepalive` |
| `open-sent` | an `open` naming my own AS, or an unknown version | `idle` | `notification`; the reason logged |
| `open-confirm` | a `keepalive` | `established` | the whole Adj-RIB-Out as `update`s |
| `established` | a `keepalive` or an `update` | `established` | — |
| any | the link down, a `notification`, or the hold timer | `idle` | — ; every route learned on the session withdrawn |

A router sends `keepalive` every `keepalive` ticks (a parameter, default a third of `hold`) and expects one frame of any kind every `hold` ticks (default 90 s in the scenario's unit); the hold timer is restarted by every valid frame. The subset does not negotiate the hold time: a peer whose `open` names another hold time is refused with a `notification` and a logged reason.

**Rule 3.** Routes. A router originates the prefixes of its `networks` parameter (a list of prefixes, each a text such as `10.4.0.0/16`, carried opaquely as bytes) with an empty AS path. Per peer the router keeps an Adj-RIB-In (what the peer advertised, as received), a Loc-RIB (the best path per prefix after the decision of rule 4) and an Adj-RIB-Out (what it advertised to the peer). An `update` carries withdrawn prefixes and advertised prefixes with their path attributes: `as-path` (a sequence of AS numbers), `next-hop` (the sender's router id, informational: the next hop of a route is the port the update arrived on), `local-pref` (set by the receiver from its port's `local-pref` parameter, never carried between autonomous systems), `med` (set by the originator per prefix from `med`, a parameter, compared by every receiver). An update whose AS path contains the receiver's own AS is dropped as a loop and counted.

**Rule 4.** The decision, per prefix, among the paths in every Adj-RIB-In: the highest `local-pref`; then the shortest `as-path`; then the lowest `med`, compared across all peers (the subset has no "same neighbouring AS" condition, so that the lab's tie by MED is visible); then the lowest router id of the advertising peer. The winner is the Loc-RIB entry and is reported as a `route`; a change of winner is reported again with the new next hop; a prefix with no path left is withdrawn and reported so.

**Rule 5.** Advertisement. The Loc-RIB's winner for a prefix is advertised to every established peer except the one it was learned from, with the router's own AS prepended to the AS path; a change of winner is advertised as an `update` to the same peers; a prefix withdrawn from the Loc-RIB is withdrawn from them. An Adj-RIB-Out changes only when the winner changes, so that the lab's timeline shows one update per decision and not per hello.

**Rule 6.** Outside the subset and refused with a logged reason, naming the feature: internal BGP; route reflection and confederations; capabilities and the four-byte AS capability's negotiation (AS numbers are `u32` in the frame, so no capability is needed); communities; aggregation and `atomic-aggregate`; multiprotocol attributes; route refresh; graceful restart; `origin` other than the subset's single value; any message type other than the four.

## 3. The frames

**Rule 7.** The protocol's name in the module manifest (RFC 008 rule 16) is `bgp`; the frames are version 1 of this layout, every integer little-endian:

| Message | Layout |
|---|---|
| Header | `version u8 = 1`, `type u8` (1 `open`, 2 `update`, 3 `keepalive`, 4 `notification`), `sender-as u32`, `router-id u64` |
| `open` | `hold u32` |
| `update` | `withdrawn-count u16`, then prefixes; `advertised-count u16`, then per prefix: the prefix, `path-length u16`, `as u32 × path-length`, `med u32` |
| `keepalive` | nothing |
| `notification` | `code u8` (1 bad version, 2 own AS, 3 hold time, 4 feature outside the subset), `text` |

A prefix is `length u8` and that many bytes. The decoder of RFC 008 rule 16, in `florn-domain-network`, decodes every message into named fields: the type, the sender's AS and id, and for an `update` the withdrawn prefixes and each advertised prefix as `prefix [as, as, …] med`; a frame that is not this layout decodes to `unknown` with the first bytes shown, as the OSPF decoder does (NET-007).

## 4. Telemetry

**Rule 8.** Channel 1, `peer`: the port, the state of rule 2 and the number of routes in the Adj-RIB-In, reported on every state change. Channel 2, `route`: the prefix, the next hop's port, the AS path's length, the `local-pref`, the `med` and whether it is the Loc-RIB's winner, reported on every decision that changes the winner and on every withdrawal. The layouts are declared in the module manifest as the OSPF guest declares its own, and the inspector decodes them with no change to the view model.

## 5. The lab

**Rule 9.** The shipped scenario `bgp-lab`: four routers in four autonomous systems, r1 (AS 1) with sessions to r2 (AS 2) and r3 (AS 3), both with sessions to r4 (AS 4), which originates `10.4.0.0/16`. r1's port to r2 sets `local-pref = 200`, the others 100; r4 sets `med = 10` towards r2 and `med = 20` towards r3. The story in both languages: the sessions come up and the first updates flood (watch the `peer` reports reach `established` and the `route` reports appear); r1 chooses the path through AS 2 by local preference although both paths have one AS between; at a scheduled instant the link r1-r2 goes down, r1's hold timer expires, the route is withdrawn and r1 chooses the path through AS 3; at a later instant the link returns, the session re-establishes, and local preference brings the path through AS 2 back. What to try: take r2 down from its inspector and watch r4's choice between its two peers fall to MED; set `local-pref` equal on both of r1's ports before the run and watch the tie go to MED, then to the lower router id; replay and see the same digest.

**Rule 10.** A second shipped scenario, `bgp-outside`, puts two routers of one AS on a link: both refuse the session with the reason logged, their `peer` reports stay `idle`, and the run ends with no route; the kit's test reads the refusal through the diagnostic log as the OSPF test does.

## 6. Verification

| Property | Test |
|---|---|
| Rule 2 | Every transition of the table, driven by frames and by `on-link`, in a native kit test; the hold timer expires at `hold` ticks after the last valid frame, on every engine |
| Rules 3 to 5 | In the lab: r1's winner is the path through AS 2 before the break, through AS 3 after the hold timer, through AS 2 again after the repair, each within `hold + 2 × keepalive` ticks of the event; r4 learns nothing it originated; a looped path is dropped and counted |
| Rule 4 | With equal `local-pref`, the winner follows the AS-path length, then MED, then the router id, each case a scenario of the kit |
| Rule 6 | Each refusal is logged with the feature named; the router keeps running |
| Rule 7 | Every frame of the lab decodes; a truncated frame decodes to `unknown` without a fault |
| Rule 8 | The inspector shows the `peer` and `route` fields decoded in the application, in the journeys |
| Rules 9 and 10 | The two scenarios' digests recorded natively and reproduced in Chromium, Firefox and WebKit; the lab's story present in both catalogues |
| RFC 008 rule 20 | The ladder's table has the rung; `labs.txt` lists it after the OSPF lab |

## Alternatives considered

| Alternative | Reason for rejection |
|---|---|
| A TCP model under the session | The domain has links, not transports; the session machine's `connect` and `active` states exist only for TCP, and the lab is about paths, not handshakes |
| Internal BGP in the subset | Needs full-mesh or reflection and the "same AS" conditions in the decision; the lab shows policy between autonomous systems and nothing of this |
| Carrying `local-pref` in the frame | Between autonomous systems it is never sent; the subset keeps the attribute where the protocol keeps it, at the receiver |
| MED compared only between paths from the same neighbouring AS | The protocol's default; in a four-router lab it would never apply, and the subset's "always compare" is a documented operator option |
| A BGP guest without a lab | The exit criterion is "exchanges routes"; the roadmap's purpose is a learner watching it |

## Prior art

| Topic | Precedent |
|---|---|
| The session machine and the message types | RFC 4271's finite state machine, reduced; the header's marker, length and TCP removed because the link carries frames |
| The decision process | RFC 4271 section 9.1 reduced to its first four steps, with "always compare MED" as the subset's rule |
| A declared subset with logged refusals | RFC 008 rule 21 and the OSPF guest of 0.2.0 |

## Consequences for other RFCs and the requirements

| RFC or document | Consequence |
|---|---|
| RFC 008 | Rule 20's table gains the rung; rule 16's protocol list gains `bgp`; section 7 gains the two layouts; open question 4 stays as it is: the decoder is Host code in the domain crate, as the OSPF decoder is |
| The SDK | A fourth example of `florn-guest-network`, `bgp`; the readme's ladder sentence |
| The application | A lab story in both catalogues; `labs.txt` |
| Requirements | NET-005 at 0.3; NET-007's "BGP UPDATE attributes" |

## Open questions

| # | Question | Decided by |
|---|---|---|
| 1 | The subset's boundary as rules 1 to 6 state it; in particular that internal BGP and capabilities are outside it | The owner, at acceptance |
| 2 | The lab's intervals (`hold`, `keepalive`) and the instants of the break and the repair, before the digests are recorded | The implementing pull request, with the lab's story |
