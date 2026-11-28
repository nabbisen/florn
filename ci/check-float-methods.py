#!/usr/bin/env python3
"""RFC 013 rule 19, controls 1 and 4: the lint list of the circuit domain is complete.

`crates/florn-domain-circuit/clippy.toml` and the device SDK's `crates/florn-guest-device/clippy.toml`
must each list as `disallowed-methods`
  * every stable inherent method of `f64` and of `f32` in the pinned toolchain that is not one of
    the operations the rule allows (a toolchain update that adds a method fails this check until
    the method is listed or allowed here), and
  * every public function of the locked `libm` that the table of `bit_identity` does not name
    (`exp`, `log`, `cbrt`: `crate::math` is the one place that calls them).
The methods of the toolchain are read from its documentation (`rust-docs`, a component of
`rust-toolchain.toml`); the functions of `libm` from the source of the locked crate.

Usage: ci/check-float-methods.py            check the files in the repository
       ci/check-float-methods.py --print    print the file it expects
       ci/check-float-methods.py --clippy-toml <path> [--docs <dir>] [--libm <dir>]
"""
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The crates that compute with floats for the solver's results (RFC 013 rule 19): the domain, and the
# device SDK, whose `libm` is the one the solver uses. Each has the same `clippy.toml`.
CRATES = ["crates/florn-domain-circuit", "crates/florn-guest-device"]

# What the rule allows: IEEE `+ - * /`, negation, `sqrt`, `abs`, `floor`, `ceil`, `trunc`,
# `copysign`, comparisons and casts (RFC 013 rule 19). Classification and the bit casts are
# comparisons and casts: they read or move the bits and compute nothing.
ALLOWED = {
    "sqrt", "abs", "floor", "ceil", "trunc", "copysign",
    "classify", "is_finite", "is_infinite", "is_nan", "is_normal", "is_sign_negative",
    "is_sign_positive", "is_subnormal", "total_cmp",
    "from_bits", "to_bits", "from_be_bytes", "from_le_bytes", "from_ne_bytes",
    "to_be_bytes", "to_le_bytes", "to_ne_bytes",
}
# The libm functions the table of `bit_identity` names, and so `crate::math` may call.
LIBM_ALLOWED = {"exp", "log", "cbrt"}

REASON = "RFC 013 rule 19: a float method outside the allowed list; use libm through crate::math"
REASONS = {
    "mul_add": "RFC 013 rule 19: no fused multiply-add",
    "min": "RFC 013 rule 19: use crate::math::fmin (signed zeros)",
    "max": "RFC 013 rule 19: use crate::math::fmax (signed zeros)",
}
LIBM_REASON = "RFC 013 rule 19: libm is called from crate::math only, and only for the functions of the table of bit_identity"

HEADER = """# RFC 013 rule 19, control 1: the solver's float arithmetic is IEEE `+ - * /`, negation, `sqrt`,
# `abs`, `floor`, `ceil`, `trunc`, `copysign`, comparisons and casts, and nothing else from `std`.
# Every other function comes from `libm` (one pure-Rust implementation in both Hosts), and the
# crate has its own `fmin` and `fmax` (`math.rs`). `cargo clippy -D warnings` fails on a use; the
# crate's root forbids an `allow` of the lint. The list is every stable `f64` and `f32` method of
# the pinned toolchain outside the allowed operations, and every `libm` function the table of
# `bit_identity` does not name; `ci/check-float-methods.py` compares it with both (and
# `--print` writes it).
"""


def sysroot():
    return subprocess.run(["rustc", "--print", "sysroot"], capture_output=True, text=True, check=True).stdout.strip()


def toolchain_methods(docs, ty):
    """The stable inherent methods of `ty` ("f64" or "f32") from the toolchain's documentation."""
    path = os.path.join(docs, f"primitive.{ty}.html")
    html = open(path, encoding="utf-8").read()
    names = set()
    for chunk in html.split('<details class="toggle method-toggle"')[1:]:
        m = re.search(r'<section id="method\.([a-z_0-9]+)" class="method">', chunk)
        if not m:
            continue  # a method of a trait implementation
        head = chunk.split("</summary>", 1)[0]
        if "stab unstable" in head:
            continue
        names.add(m.group(1))
    return names


def libm_functions(libm_dir):
    """The public functions of `libm`: the `pub use self::name::name;` lines of its math module."""
    text = open(os.path.join(libm_dir, "src/math/mod.rs"), encoding="utf-8").read()
    names = set(re.findall(r"^\s*pub use self::(\w+)::\1;", text, re.M))
    if len(names) < 50:
        raise SystemExit(f"error: read only {len(names)} libm functions from {libm_dir}: the layout of the crate changed")
    return names


def locked_libm():
    meta = subprocess.run(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=ROOT,
                          capture_output=True, text=True, check=True).stdout
    for p in json.loads(meta)["packages"]:
        if p["name"] == "libm":
            return os.path.dirname(p["manifest_path"])
    raise SystemExit("error: libm is not in the lock")


def expected(docs, libm_dir):
    entries = []
    for ty in ("f64", "f32"):
        for name in sorted(toolchain_methods(docs, ty) - ALLOWED):
            entries.append((f"{ty}::{name}", REASONS.get(name, REASON)))
    for name in sorted(libm_functions(libm_dir) - LIBM_ALLOWED):
        entries.append((f"libm::{name}", LIBM_REASON))
    return entries


def render(entries):
    lines = [HEADER + "disallowed-methods = ["]
    for path, reason in entries:
        lines.append(f'    {{ path = "{path}", reason = "{reason}" }},')
    lines.append("]")
    return "\n".join(lines) + "\n"


def listed(text):
    return re.findall(r'path = "([^"]+)"', text)


def main(argv):
    args = argv[1:]
    tomls = [os.path.join(ROOT, c, "clippy.toml") for c in CRATES]
    docs = os.path.join(sysroot(), "share/doc/rust/html/std")
    libm_dir = None
    print_only = False
    while args:
        a = args.pop(0)
        if a == "--print":
            print_only = True
        elif a == "--clippy-toml":
            tomls = [args.pop(0)]
        elif a == "--docs":
            docs = args.pop(0)
        elif a == "--libm":
            libm_dir = args.pop(0)
        else:
            raise SystemExit(f"unknown argument {a}")
    if not os.path.exists(os.path.join(docs, "primitive.f64.html")):
        print(f"error: the toolchain's documentation is not installed ({docs}); `rust-docs` is a component of rust-toolchain.toml", file=sys.stderr)
        return 2
    entries = expected(docs, libm_dir or locked_libm())
    if print_only:
        sys.stdout.write(render(entries))
        return 0
    want = [p for p, _ in entries]
    bad = False
    for toml in tomls:
        name = os.path.relpath(toml, ROOT) if toml.startswith(ROOT) else toml
        have = listed(open(toml, encoding="utf-8").read())
        missing = sorted(set(want) - set(have))
        extra = sorted(set(have) - set(want))
        dup = sorted({p for p in have if have.count(p) > 1})
        for p in missing:
            print(f"error: {p} is not in {name} (RFC 013 rule 19, control 1)", file=sys.stderr)
        for p in extra:
            print(f"error: {name} lists {p}, which is not a stable method of the toolchain or a libm function the rule disallows", file=sys.stderr)
        for p in dup:
            print(f"error: {name} lists {p} twice", file=sys.stderr)
        if missing or extra or dup:
            print(f"regenerate it with: ci/check-float-methods.py --print > {name}", file=sys.stderr)
            bad = True
    if bad:
        return 1
    print(f"{len(tomls)} clippy.toml file(s) list all {len(want)} methods and functions (f64, f32, libm)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
