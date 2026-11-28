#!/usr/bin/env python3
"""RFC 005 rules 1 and 43: the dependency direction of the workspace, and the absence of platform
and engine crates from the core. RFC 013 rule 19 (control 2): the circuit domain's direct external
dependencies are `libm` and nothing else, and every direct external dependency of a normal or
build kind is pinned exactly (`=x.y.z`). Reads `cargo metadata`, or the file given with
`--metadata FILE` (what the gate tests of `ci/test-gates.sh` plant a violation in); fails on the
first violation."""
import json, subprocess, sys

if len(sys.argv) == 3 and sys.argv[1] == "--metadata":
    with open(sys.argv[2], "rb") as f:
        meta = json.load(f)
else:
    meta = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--locked"]))
by_id = {p["id"]: p for p in meta["packages"]}
name_of = {p["id"]: p["name"] for p in meta["packages"]}
resolve = {n["id"]: n for n in meta["resolve"]["nodes"]}
workspace = {name_of[i] for i in meta["workspace_members"]}

def deps(name, kinds=("normal",)):
    """Transitive dependency names of a workspace crate, for the given dependency kinds."""
    start = next(i for i in meta["workspace_members"] if name_of[i] == name)
    seen, stack = set(), [start]
    while stack:
        node = resolve[stack.pop()]
        for d in node["deps"]:
            if any((k["kind"] or "normal") in kinds for k in d["dep_kinds"]):
                if d["pkg"] not in seen:
                    seen.add(d["pkg"]); stack.append(d["pkg"])
    return {name_of[i] for i in seen}

def direct(name):
    start = next(i for i in meta["workspace_members"] if name_of[i] == name)
    return {name_of[d["pkg"]] for d in resolve[start]["deps"] if any((k["kind"] or "normal") == "normal" for k in d["dep_kinds"])}

PLATFORM = {"web-sys", "js-sys", "wasm-bindgen", "wasmtime", "tokio", "leptos"}
CORE = ["florn-kernel", "florn-abi", "florn-model", "florn-admission", "florn-control", "florn-host", "florn-ui-model", "florn-ui-scene", "florn-engine", "florn-platform"]
# Normal dependencies only: a dev-dependency (a test tool) may be anything the licence gate allows.
# Allowed direct florn-* dependencies per crate (RFC 005 rule 43). Absent = no florn-* dependency allowed.
ALLOWED = {
    "florn-model": set(),
    "florn-admission": {"florn-model", "florn-abi"},
    "florn-control": {"florn-model"},
    "florn-engine": {"florn-abi", "florn-kernel"},
    "florn-platform": {"florn-control"},
    "florn-host": {"florn-kernel", "florn-abi", "florn-model", "florn-admission", "florn-control", "florn-engine", "florn-platform"},
    "florn-engine-wasmtime": {"florn-engine", "florn-abi"},
    "florn-engine-web": {"florn-engine", "florn-abi"},
    "florn-platform-native": {"florn-platform", "florn-control"},
    "florn-platform-web": {"florn-platform", "florn-control"},
    "florn-ui-model": {"florn-control"},
    "florn-ui-scene": {"florn-ui-model"},
    "florn-ui-web": {"florn-control", "florn-ui-model", "florn-ui-scene", "florn-host", "florn-engine", "florn-engine-web", "florn-platform-web", "florn-domain-network", "florn-domain-circuit"},
    "florn-cli": {"florn-control", "florn-host", "florn-engine-wasmtime", "florn-platform-native", "florn-model", "florn-domain-network", "florn-domain-circuit"},
    "florn-domain-network": {"florn-host", "florn-kernel", "florn-model", "florn-control", "florn-abi", "florn-admission"},
    "florn-domain-circuit": {"florn-host", "florn-kernel", "florn-model", "florn-control", "florn-abi", "florn-admission"},
    "florn-guest-network": {"florn-guest"},
    "florn-guest-circuit": {"florn-guest"},
    "florn-conformance": {"florn-host", "florn-domain-circuit", "florn-model", "florn-control", "florn-engine", "florn-engine-wasmtime", "florn-platform", "florn-platform-native", "florn-guest", "florn-kernel", "florn-admission", "florn-abi", "florn-test-guests", "florn-domain-network"},
    "florn-test-guests": {"florn-model", "florn-admission"},
    "florn-guest": set(),
    "florn-kernel": set(),
    "florn-abi": set(),
    "florn": {"florn-cli"},
}
# The direct external dependencies (normal and build kinds) a crate is limited to. A crate that is
# not listed is limited by `cargo deny` alone; every external dependency is pinned exactly.
EXTERNAL = {
    "florn-domain-circuit": {"libm"},
}
bad = 0
for p in meta["packages"]:
    if p["name"] not in workspace:
        continue
    for d in p["dependencies"]:
        if d["name"] in workspace or d["kind"] == "dev":
            continue
        if not d["req"].startswith("="):
            print(f"{p['name']}: {d['name']} is required as {d['req']}, not pinned exactly (=x.y.z)"); bad += 1
    if p["name"] in EXTERNAL:
        external = {d["name"] for d in p["dependencies"] if d["name"] not in workspace and d["kind"] != "dev"}
        extra = external - EXTERNAL[p["name"]]
        if extra:
            print(f"{p['name']}: external dependencies {sorted(extra)}, only {sorted(EXTERNAL[p['name']])} allowed (RFC 013 rule 19)"); bad += 1
# `libm` with a feature is `libm` that may call the platform: `arch` (a default feature) selects the
# target's own instructions for the operations it can, which is what rule 19 keeps out. Wherever a
# workspace crate depends on it, it is `default-features = false` and no feature (amended 2026-11-28).
for p in meta["packages"]:
    if p["name"] not in workspace:
        continue
    for d in p["dependencies"]:
        if d["name"] == "libm" and (d.get("features") or d.get("uses_default_features")):
            how = f"features {sorted(d['features'])}" if d.get("features") else "its default features"
            print(f"{p['name']}: libm with {how}; it takes default-features = false and no feature (RFC 013 rule 19)"); bad += 1
for crate in sorted(workspace):
    florn_direct = {d for d in direct(crate) if d.startswith("florn")}
    extra = florn_direct - ALLOWED.get(crate, set())
    if extra:
        print(f"{crate}: depends on {sorted(extra)}, not allowed by RFC 005 rule 43"); bad += 1
for crate in CORE:
    hit = deps(crate) & PLATFORM
    if hit:
        print(f"{crate}: platform or engine crate in its tree: {sorted(hit)} (RFC 005 rule 1)"); bad += 1
if "florn-host" in workspace:
    hit = deps("florn-host") & {"florn-engine-wasmtime", "florn-engine-web", "florn-platform-native", "florn-platform-web"}
    if hit:
        print(f"florn-host: depends on an adapter implementation {sorted(hit)} (RFC 005 rule 43)"); bad += 1
print(f"checked {len(workspace)} crates; {bad} violation(s)")
sys.exit(1 if bad else 0)
