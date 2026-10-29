# Phase 3 handoff package — depth, release 0.3.0

Entry point for the implementation of RFC 009, 010 and 011 and the 0.3 rules of RFC 001 (51 to 55), RFC 003 (rules 20, 22, 23 and section 7) and RFC 005 (rules 17, 29, 36). Written 2026-10-29 on the acceptance of the three RFCs; the architect and the developer are one person in Phase 3 as before, and the owner verifies.

| Document | What it is |
|---|---|
| [`implementation-handoff.md`](./implementation-handoff.md) | What each crate must do and when it is done; the cross-cutting rules |
| [`task-breakdown-pr-plan.md`](./task-breakdown-pr-plan.md) | Eight pull requests in order, from checkpoints to the release; the progress table |
| [`acceptance-qa-checklist.md`](./acceptance-qa-checklist.md) | The owner's checks C23 to C30, journey J5 and the navigation and branching procedures, the usability test, and the release review |

## What Phase 3 delivers

The roadmap's Phase 3, depth: checkpoints, backward navigation and branching; actor swapping beyond the browser's instance ceiling; import of a user's module through the application; cost-model version 2 with florn's own metering pass; a BGP guest and its lab; a usability test with people who did not build florn. The exit criteria: a run restored from a checkpoint continues with the digest of an uninterrupted run; a user imports a module and runs it in a topology; the cost benchmark meets its target under version 2 or the owner has decided again on the figures; runs recorded under version 1 still replay; a BGP guest exchanges routes.

## Order of work and why

1. **Checkpoints in the Host and the control interface** (RFC 009 section 2, section 8) come first: every later feature of RFC 009 restores a checkpoint, and the file form proves DATA-005 across the two Hosts before any interface shows it.
2. **Cost-model version 2** (RFC 010) comes second and independently, so that its figures reach the owner early: the roadmap's step 4 is a decision on measured numbers, and the decision must not wait for the end of the phase.
3. **Backward navigation and detail by replay** (RFC 009 section 3) and **branching** (section 4) follow on checkpoints; each is a Host operation with a control message and a kit test before it has a button.
4. **Actor swapping** (section 5) is the last Host change: it changes the instance pool, which everything else leaves alone.
5. **The application's depth** (sections 6 and 7) is one pull request over a finished Host: the controls for going back, branching and checkpoints, origin storage, journey J5, the installable application, the status line.
6. **The BGP subset and its lab** (RFC 011) arrive as content over the finished interface, as the OSPF lab did in Phase 2.
7. **Release 0.3.0** repeats the release discipline of 0.2.0.

## What stays out

A second domain (0.4), the native shell (0.6), a text form of the control interface for automation beyond the runner's commands (RFC 005 open question 4, waiting for the first script that needs it), decoders as sandboxed modules (RFC 008 open question 4), Components on import (decided out of 0.3 by RFC 009 rule 18). Anything the owner did not decide is an open question in an RFC or in this package, never a quiet choice in code.
