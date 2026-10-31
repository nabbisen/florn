# Contributing

## The routine

Every change runs this before it is pushed; continuous integration runs the same on three native architectures and three browsers.

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --locked --target wasm32-unknown-unknown -p florn-engine-web -p florn-platform-web -p florn-ui-web -- -D warnings
cargo clippy --locked --target wasm32-unknown-unknown -p florn-ui-web --features shell --bin florn-ui-web -- -D warnings
cargo test --workspace --locked
cargo deny check licenses advisories bans sources
python3 -I ci/check-deps.py
ci/check-rfc-index.sh
ci/forbidden-words.sh
ci/check-shell-exports.sh
```

After any change to the SDK or an example guest, regenerate the shipped modules and digests, and commit them:

```sh
cargo run --release -p florn-conformance -- scenarios
```

Even a formatting change to a guest changes its bytes (panic locations), so the kit's test would otherwise fail. The release profile strips symbols; a change to the profile changes the modules too.

The browser run needs Node, the Playwright browsers and the wasm-bindgen command of the version the shell pins:

```sh
cargo install --locked wasm-bindgen-cli --version 0.2.126
ci/build-shell.sh
cd ci/browser && npm ci && npx playwright install --with-deps chromium firefox webkit
node run.js --browsers chromium,firefox,webkit          # the kit under the browser Host
node journeys.js --browsers chromium,firefox,webkit     # the journeys through the application
```

`ci/check-shell-exports.sh` keeps the shell on the control interface alone, and `cargo test -p florn-ui-web --features shell` checks the catalogues.

Fuzzing needs the nightly toolchain and `cargo-fuzz`; `ci/fuzz.sh 60` runs every target for a minute. The weekly CI job runs them for three minutes each.

## Rules

- Every dependency is pinned to an exact version in `Cargo.toml` and added by the change that needs it, with its licence on the allow-list of `deny.toml` (RFC 006 rule 6). A version is changed by a pull request that says why.
- `unsafe` appears only in the adapters, the ABI crate and the guest SDK, with the invariant stated at each use; every other crate forbids it.
- Every message a person sees is a code with typed parameters from the catalogue of RFC 007; the English table in `florn-cli` is the source text, kept equal to the catalogue by a test.
- A change of a file format or of an encoding needs a new version number and a new recorded value in the kit's format tests; a silent edit fails them.
- Decisions are recorded as RFCs under `rfcs/` and never deleted; `rfcs/README.md` is the index, checked in CI.
- Commit messages: a digest subject; a body of at most 400 characters when needed; no section sign, the word "section" instead.
- A change is one unit of the handoff's plan (its units are named PR-NN and are commits on `main`); unrelated refactoring, cleanup and feature work are not combined.
- Private working files live outside the repository; a public file cites no private path.

## Roles

florn is developed by a small team under one owner. Each role answers for what the table says and for nothing else; a question that belongs to another role is put to that role in writing, not settled in place.

| Role | Answers for | Does not |
|---|---|---|
| Owner | Direction, priorities and scope; the roadmap and every change to it; the acceptance of an RFC and of a phase; the approval of every release; the publication of the crates; the repository's settings, secrets and collaborators | — |
| Architect | Requirements, external design, RFCs and the handoff package of each phase; the release cycle and the release recommendation; the review of every change against the RFCs and the handoff, from its review request, after the push (the work is serial and on `main` alone); the decision register | Decide scope, a release or a version on the owner's behalf; implement in the team's place beyond narrow emergency support the owner asks for |
| Developer | Implementation from an accepted RFC and its handoff; the unit tests and the kit's tests the handoff names; the routine above; a review request with evidence for every change | Change a requirement, a contract, a file format, a security boundary or a dependency without a change request; tag, release or publish |
| Verifier | Acceptance checks written from the RFC and the handoff, independently of the implementation | Edit a failing check to make it pass; read the implementation before the check is written |

A developer who finds a defect in the design reports it to the architect, with a proposal when there is one, and does not redesign in place. A failing acceptance check is resolved by changing the implementation or by a ruling from the architect.

## Authorisations

**Status.** Decided by the owner on 2026-10-31: the work is serial, on `main` alone, and the roles exchange files, not prompts. No architect's job and no development team's job run at the same time, so there is one change in flight at any moment and no branch beside `main`.

| Action | Who | Gate |
|---|---|---|
| Commit and push to `main` | developer, architect | The routine green locally; the commit message rules above; one unit of the handoff's plan per change; nothing else in flight |
| Accept a change as done | architect, from the review request file, after the push | CI green on `main`; the review's outcome recorded; the shipped modules and digests regenerated when the SDK, a guest or admission changed; the progress table of the handoff updated in the same change or the next. A change the review rejects is corrected by a following commit, never by rewriting history |
| Amend an RFC, the roadmap, a contract or a file format | architect | An RFC amendment or a new RFC; the owner's acceptance when the roadmap or an open question reserves the decision to the owner |
| Tag a release | architect, on the owner's approval of the release recommendation | The release checklist of `ROADMAP.md`; the release notes; the RFC index; the version bumped first |
| Publish the crates | owner | From the tagged commit; `ci/publish.sh --dry-run` after the version bump |
| Add or change a dependency | developer, in the change that needs it | Exact pin; licence on the allow-list; `cargo deny` green; the reason in the commit message |
| Repository settings, secrets, collaborators | owner | — |

Every session acts under the owner's account, so the repository cannot tell the roles apart: the controls are the routine, CI on `main`, and the review request and review files that every change leaves behind.

## Publishing to crates.io

The crates people consume and their dependency chain are publishable: `florn` (the runner), `florn-guest` and `florn-guest-network` (the SDKs), `florn-domain-network` and the library crates between them; the kit, the browser crates, the UI crates and the test guests are not. `ci/publish.sh --dry-run` packages and verifies every publishable crate in dependency order; `ci/publish.sh` publishes them with the owner's token after `cargo login`. A release's crates are published from the release's commit, with the version the tag names; the SDK's readme then names the registry form of the dependency. crates.io admits a burst of five new crates and one more every ten minutes after that; when a first publication stops there, `cargo publish --workspace` will not resume past the crates already present, so the rest are published one at a time with `cargo publish -p <crate> --locked` in dependency order. A new version of an existing crate is not counted against that limit.
