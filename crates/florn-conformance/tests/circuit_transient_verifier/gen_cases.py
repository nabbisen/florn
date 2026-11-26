#!/usr/bin/env python3
"""Generate cases.rs: scripts and the reference model's expectations (verifier PR-45).

Run `python3 gen_cases.py` in this directory after a change to the model; the test file includes
the result."""
import os, sys, math
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import refmodel as rm

TICK = 1e-9


def rs_f64(x):
    return repr(float(x)) if math.isfinite(x) else "f64::NAN"


def case(name, branches, ops, profile, model_kw, vs=1.0):
    """branches: list of (R ohm, C farad, ic, rstr, cstr)."""
    br = [dict(R=b[0], C=b[1], ic=b[2], name=f"c{i}") for i, b in enumerate(branches)]
    p = rm.Plant(br, vs=vs, tick=TICK, **model_kw)
    volts, stuck_at = [], None
    for idx, op in enumerate(ops):
        if op[0] == "bp":
            p.set_input(op[1], op[2])
        else:
            p.advance(op[1])
            if p.stuck:
                stuck_at = (idx, p.stuck[0], p.stuck[2])
                break
            volts.append(list(p.x))
    return dict(name=name, branches=branches, ops=ops, profile=profile, p=p, volts=volts, stuck=stuck_at)


def ticks(n):
    return f"{n}ns"


def prof(method="trapezoidal", control="adaptive", step=None, min_step=None, max_step=None,
         ltol=None, max_rej=None):
    lines = [f'method = "{method}"', f'control = "{control}"']
    kw = dict(method=method, control=control)
    if step is not None:
        lines.append(f'step = "{ticks(step)}"'); kw["step"] = step
    if min_step is not None:
        lines.append(f'min-step = "{ticks(min_step)}"'); kw["min_step"] = min_step
    if max_step is not None:
        lines.append(f'max-step = "{ticks(max_step)}"'); kw["max_step"] = max_step
    if ltol is not None:
        lines.append(f"ltol = {ltol!r}"); kw["ltol"] = ltol
    if max_rej is not None:
        lines.append(f"max-rejections = {max_rej}"); kw["max_rej"] = max_rej
    return "\n".join(lines), kw


RC = (1e3, 1e-6, 0.0, "1kohm", "1uF")
cases = []

# V1/V2/V3: rc-step, both methods, default profile and max-step = 100 us
for m in ("trapezoidal", "gear2"):
    for extra in (None, 100_000):
        pr, kw = prof(method=m, max_step=extra)
        ops = [("bp", 2_000_000, 0.0)] + [("adv", k * 100_000) for k in range(1, 51)]
        cases.append(case(f"rcstep-{m}-{extra}", [RC], ops, pr, kw))

# V4: order test
for m in ("trapezoidal", "gear2"):
    for div in (8, 16, 32, 64):
        n = 1_000_000 // div
        pr, kw = prof(method=m, control="fixed", step=n)
        ops = [("adv", 1_000_000), ("adv", 2_000_000)]
        cases.append(case(f"order-{m}-{div}", [RC], ops, pr, kw))

# V9: shaping with min-step 3000 / step 10000 / max-step 20000, distances incl. below min-step
targets, t = [], 0
for dist in [25000, 31000, 12500, 7777, 2500, 1200, 90000, 5000, 2999, 3000, 3001, 5999, 6000, 6001,
             9999, 10000, 10001, 13000, 17000, 20000, 20001, 40000, 40001, 59999]:
    t += dist
    targets.append(t)
for m in ("trapezoidal", "gear2"):
    for ltol in (None, 1e-5):
        pr, kw = prof(method=m, step=10_000, min_step=3_000, max_step=20_000, ltol=ltol)
        cases.append(case(f"shape-{m}-{ltol}", [RC], [("adv", x) for x in targets], pr, kw))
    # the livelock of rule 13's remainder clause against rule 12's shrink (finding)
    pr, kw = prof(method=m, step=10_000, min_step=3_000, max_step=20_000, ltol=1e-5)
    cases.append(case(f"livelock-{m}", [RC], [("adv", 25_000), ("adv", 40_000)], pr, kw))

# V8: fixed shaping
pr, kw = prof(method="trapezoidal", control="fixed", step=10_000, min_step=3_000, max_step=20_000)
cases.append(case("fixed-shape", [RC], [("adv", x) for x in targets], pr, kw))
pr, kw = prof(method="gear2", control="fixed", step=10_000, min_step=3_000, max_step=20_000)
cases.append(case("fixed-shape-gear2", [RC], [("adv", x) for x in targets], pr, kw))

# V11: no dynamic element
pr, kw = prof()
cases.append(case("divider", [], [("adv", k * 1_000_000) for k in range(1, 11)], pr, kw))
pr, kw = prof(step=1000, max_step=64_000)
cases.append(case("divider-small-max", [], [("adv", k * 1_000_000) for k in range(1, 11)], pr, kw))

# V10/V13/V22: Gear cap after a tiny step 3, restarts, proposal kept apart
for m in ("gear2", "trapezoidal"):
    pr, kw = prof(method=m)
    ops = [("adv", 100_000), ("bp", 100_000, 0.0), ("adv", 101_000), ("adv", 102_000), ("adv", 102_010),
           ("adv", 300_000), ("bp", 300_000, 1.0), ("adv", 300_001), ("adv", 300_002), ("adv", 300_003),
           ("adv", 301_000), ("adv", 900_000), ("bp", 905_000, 0.5), ("adv", 920_000), ("adv", 2_000_000)]
    cases.append(case(f"restart-{m}", [RC], ops, pr, kw))

# V14: rejections (tight ltol), two branches
RC2 = (2e3, 5e-6, 0.0, "2kohm", "5uF")
for m in ("trapezoidal", "gear2"):
    pr, kw = prof(method=m, ltol=1e-7)
    ops = [("bp", 3_000_000, 0.0)] + [("adv", k * 250_000) for k in range(1, 25)]
    cases.append(case(f"tight-{m}", [RC, RC2], ops, pr, kw))

# V15: error test fails at min-step: accepted and notified
for m in ("trapezoidal", "gear2"):
    pr, kw = prof(method=m, ltol=1e-9, step=1000, min_step=1000, max_step=4000)
    ops = [("adv", k * 50_000) for k in range(1, 12)]
    cases.append(case(f"atmin-{m}", [RC], ops, pr, kw))

# V5: a prescribed step pattern of ratios 0.5..2 (fixed control, one limit per step), halved
CYC = [4, 8, 6, 3, 5, 4, 2, 4]          # quarter-units; consecutive ratios 2, .75, .5, 1.67, .8, .5, 2, 1
for m in ("trapezoidal", "gear2"):
    for q in (16000, 8000, 4000, 2000, 1000):
        ops, t, k = [], 0, 0
        while t < 1_728_000:
            t += CYC[k % len(CYC)] * q
            ops.append(("adv", t)); k += 1
        assert t == 1_728_000, t
        pr, kw = prof(method=m, control="fixed", step=50_000_000)
        cases.append(case(f"pattern-{m}-{q}", [RC], ops, pr, kw))

# V16: non-convergence by the error test names the owner of the state with the largest ratio
for nm_, order in (("ab", [RC, RC2]), ("ba", [RC2, RC]), ("cd", [(1e3, 1e-6, 0.0, "1kohm", "1uF"), (1e3, 22e-6, 0.0, "1kohm", "22uF")]),
                   ("dc", [(1e3, 22e-6, 0.0, "1kohm", "22uF"), (1e3, 1e-6, 0.0, "1kohm", "1uF")])):
    pr, kw = prof(method="trapezoidal", step=1_000_000, ltol=1e-12, max_rej=2)
    cases.append(case(f"owner-{nm_}", order, [("adv", 3_000_000)], pr, kw))

out = []
out.append("// Generated by gen_cases.py from refmodel.py; do not edit.\n")
out.append("pub struct Br { pub r: &'static str, pub c: &'static str }\n")
out.append("pub enum Op { Adv(u64), Bp(u64, f64) }\n")
out.append("pub struct Step { pub start: u64, pub ticks: u64, pub f: char, pub tries: u32 }\n")
out.append("pub struct Case { pub name: &'static str, pub profile: &'static str, pub br: &'static [Br], pub ops: &'static [Op], pub steps: &'static [Step], pub volts: &'static [&'static [f64]], pub minimum: usize, pub stuck: Option<(usize, u64, usize)> }\n")
out.append("pub fn cases() -> Vec<Case> { vec![\n")
for c in cases:
    p = c["p"]
    brs = ", ".join(f'Br {{ r: "{b[3]}", c: "{b[4]}" }}' for b in c["branches"])
    ops = ", ".join(f"Op::Adv({o[1]})" if o[0] == "adv" else f"Op::Bp({o[1]}, {rs_f64(o[2])})" for o in c["ops"])
    fmap = {"be": "B", "trapezoidal": "T", "gear2": "G"}
    steps = ", ".join(f"Step {{ start: {s['start']}, ticks: {s['n']}, f: '{fmap[s['meth']]}', tries: {s['tries']} }}" for s in p.log)
    volts = ", ".join("&[" + ", ".join(rs_f64(x) for x in v) + "]" for v in c["volts"])
    minimum = sum(1 for n in p.notes if n[0] == "step-minimum")
    st = "None" if c["stuck"] is None else f"Some(({c['stuck'][0]}, {c['stuck'][1]}, {c['stuck'][2]}))"
    out.append(f'  Case {{ name: "{c["name"]}", profile: "{c["profile"].replace(chr(34), chr(92) + chr(34)).replace(chr(10), chr(92) + "n")}", br: &[{brs}], ops: &[{ops}], steps: &[{steps}], volts: &[{volts}], minimum: {minimum}, stuck: {st} }},\n')
out.append("] }\n")
open(os.path.join(HERE, "cases.rs"), "w").write("".join(out))
print(len(cases), "cases;", sum(len(c["p"].log) for c in cases), "steps; notes:",
      {c["name"]: sum(1 for n in c["p"].notes if n[0] != "step-rejected") for c in cases if any(n[0] != "step-rejected" for n in c["p"].notes)})
for c in cases:
    if c["p"].stuck:
        print("model stuck in", c["name"], c["p"].stuck)
