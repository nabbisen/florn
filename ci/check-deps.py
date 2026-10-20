#!/usr/bin/env python3
"""RFC 005 rules 1 and 43: the dependency direction of the workspace, and the absence of platform
and engine crates from the core. Reads `cargo metadata`; fails on the first violation."""
import json, subprocess, sys

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
    "florn-ui-web": {"florn-control", "florn-ui-model", "florn-ui-scene", "florn-host", "florn-engine", "florn-engine-web", "florn-platform-web"},
    "florn-cli": {"florn-control", "florn-host", "florn-engine-wasmtime", "florn-platform-native", "florn-model"},
    "florn-conformance": {"florn-host", "florn-model", "florn-control", "florn-engine", "florn-engine-wasmtime", "florn-platform", "florn-platform-native", "florn-guest", "florn-kernel", "florn-admission", "florn-abi"},
    "florn-guest": set(),
    "florn-kernel": set(),
    "florn-abi": set(),
    "florn": {"florn-cli"},
}
bad = 0
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
