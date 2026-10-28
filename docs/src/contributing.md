# Contributing

## The routine

Every change runs this before it is pushed; continuous integration runs the same on three native architectures and three browsers.

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --locked --target wasm32-unknown-unknown -p florn-engine-web -p florn-platform-web -p florn-ui-web -- -D warnings
cargo test --workspace --locked
cargo deny check licenses advisories bans sources
python3 -I ci/check-deps.py
ci/check-rfc-index.sh
ci/forbidden-words.sh
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

- Every dependency is pinned to an exact version in `Cargo.toml` and added by the pull request that needs it, with its licence on the allow-list of `deny.toml` (RFC 006 rule 6). A version is changed by a pull request that says why.
- `unsafe` appears only in the adapters, the ABI crate and the guest SDK, with the invariant stated at each use; every other crate forbids it.
- Every message a person sees is a code with typed parameters from the catalogue of RFC 007; the English table in `florn-cli` is the source text, kept equal to the catalogue by a test.
- A change of a file format or of an encoding needs a new version number and a new recorded value in the kit's format tests; a silent edit fails them.
- Decisions are recorded as RFCs under `rfcs/` and never deleted; `rfcs/README.md` is the index, checked in CI.
- Commit messages: a digest subject; a body of at most 400 characters when needed.

## Publishing to crates.io

The crates people consume and their dependency chain are publishable: `florn` (the runner), `florn-guest` and `florn-guest-network` (the SDKs), `florn-domain-network` and the library crates between them; the kit, the browser crates, the UI crates and the test guests are not. `ci/publish.sh --dry-run` packages and verifies every publishable crate in dependency order; `ci/publish.sh` publishes them with the owner's token after `cargo login`. A release's crates are published from the release's commit, with the version the tag names; the SDK's readme then names the registry form of the dependency. crates.io admits a burst of five new crates and one more every ten minutes after that; when a first publication stops there, `cargo publish --workspace` will not resume past the crates already present, so the rest are published one at a time with `cargo publish -p <crate> --locked` in dependency order. A new version of an existing crate is not counted against that limit.
