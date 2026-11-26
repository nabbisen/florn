#!/usr/bin/env python3
"""Independent reference model of RFC 013 rules 6, 7, 10 to 14 (PR-45 verifier).

Written from the RFC text only (RFC 013 and RFC 012 rules 7 to 10), before the implementation of
the unit was read.  Plain Python floats (IEEE binary64), stdlib only.  Two places follow the
rulings made on the verifier's report (RFC 013 rules 6 and 12 as amended 2026-11-26) and are marked
`Ruling`: a retried step is never longer than the attempt it replaces, and no Gear 2 step has an
omega above 2.

The plant: a voltage source V(t) feeding independent RC branches (R, C, ic) to ground; the state of
each branch is the capacitor voltage.  Newton on a linear plant is one linear solve, done here in
closed form.  Everything the RFC specifies about *steps* (start-up, error estimate, accept/reject,
proposal, bounds in ticks, restarts) is modelled literally; where the RFC is silent the choice is
marked  # AMBIGUOUS-n  and listed in AMBIGUITIES.
"""
import math, json, sys

VNTOL = 1e-9
ABSTOL = 1e-12

AMBIGUITIES = {
    1: "RFC 013 rule 11: the error ratio's denominator uses |x| of the CANDIDATE point (x_{n+1}); the text says only |x|.",
    2: "RFC 013 rule 12: an error-test failure of a step SHORTER than min-step (the whole distance to a limit, rule 13) - 'at n = min-step' does not literally cover n < min-step.",
    3: "RFC 013 rule 12: after a rejection the retried size goes through rule 13 again (the remainder clause may lengthen it). RULED 2026-11-26: a retry is never longer than the attempt it replaces.",
    4: "RFC 013 rule 12: the 'twice the step just taken' cap under Gear 2 is applied whenever the NEXT step will be a Gear step (also after the order-1 backward Euler step 3).",
    5: "RFC 013 rule 7: after a cut the proposal returns to the profile's `step` (rule 2 'the step after every breakpoint').",
    6: "RFC 013 rule 12: the first step after an accepted step uses `previous proposal` = the proposal in force before that step was attempted.",
    10: "RFC 012 rule 29 stores 'the unconstrained next step': the Gear 2 cap of rule 12 is applied when the step is chosen, not written into the stored proposal.",
    11: "RFC 013 rule 12: 'shortened to end at a limit' = shaped by rule 13; a retry after a rejection is not 'shortened'.",
}


def bits(x):
    import struct
    return struct.unpack("<Q", struct.pack("<d", x))[0]


class Plant:
    def __init__(self, branches, vs=1.0, tick=1e-9, method="trapezoidal", control="adaptive",
                 step=None, min_step=1, max_step=None, ltol=1e-3, max_rej=60, gmin=1e-12):
        self.br = branches                      # list of dict(R, C, ic, name)
        self.tick = tick
        self.method = method
        self.control = control
        self.step = step if step is not None else max(1, round(1e-6 / tick))
        self.min_step = min_step
        self.max_step = max_step if max_step is not None else 100 * self.step
        self.ltol = ltol
        self.max_rej = max_rej
        self.gmin = gmin
        self.v = vs
        self.bps = []                           # pending (tick, value), in order recorded
        self.x = [b["ic"] for b in branches]
        self.xd = [0.0 for _ in branches]       # derivative at the last accepted point
        self.solved = 0
        self.pts = [(0, list(self.x))]          # accepted points since the last cut: (tick, x)
        self.k = 0                              # steps since the last cut
        self.prop = self.step                   # AMBIGUOUS-5 (reset at a cut)
        self.hlast = None                       # ticks of the last accepted step
        self.log = []                           # accepted steps
        self.notes = []                         # notifications
        self.stuck = None

    # -- inputs
    def set_input(self, tick, value):
        self.bps.append((tick, value))

    def _apply(self, now):
        cut = False
        keep = []
        for (t, val) in self.bps:
            if t <= now:
                self.v = val
                cut = True
            else:
                keep.append((t, val))
        self.bps = keep
        if cut:
            self.pts = [(now, list(self.x))]
            self.k = 0
            self.prop = self.step
            self.hlast = None

    # -- one attempt of n ticks from the last accepted point; returns (xnew, xdnew, alpha, hist)
    def _solve(self, n, meth, k_hist):
        h = n * self.tick                       # rule 14: ticks accumulate, never seconds
        out = []
        for i, b in enumerate(self.br):
            x_n = self.x[i]
            xd_n = self.xd[i]
            if meth == "be":
                alpha = 1.0 / h
                hist = x_n / h
            elif meth == "trapezoidal":
                alpha = 2.0 / h
                hist = 2.0 * x_n / h + xd_n
            else:                               # gear2 variable step
                hn = self.hlast * self.tick
                w = h / hn
                alpha = (1.0 + 2.0 * w) / ((1.0 + w) * h)
                x_nm1 = self.pts[-2][1][i]
                hist = ((1.0 + w) * x_n - (w * w / (1.0 + w)) * x_nm1) / h
            # KCL at the capacitor node: (x - V)/R + gmin*x + C*(alpha*x - hist) = 0
            g = 1.0 / b["R"] + self.gmin + b["C"] * alpha
            xnew = (self.v / b["R"] + b["C"] * hist) / g
            out.append((xnew, alpha * xnew - hist))
        return out

    def _ratio(self, n, meth, order, cand):
        """The error ratio of rule 11 for the candidate point."""
        h = n * self.tick
        r = 0.0
        worst = 0
        for i, b in enumerate(self.br):
            if order == 1:                      # backward Euler: h^2 |dd2|, points n-1, n, n+1
                (t1, a), (t2, bb) = self.pts[-2], self.pts[-1]
                t3 = self.pts[-1][0] + n
                x1, x2, x3 = a[i], bb[i], cand[i][0]
                d12 = (x2 - x1) / ((t2 - t1) * self.tick)
                d23 = (x3 - x2) / ((t3 - t2) * self.tick)
                dd2 = (d23 - d12) / ((t3 - t1) * self.tick)
                est = h * h * abs(dd2)
            else:
                (t0, p0), (t1, p1), (t2, p2) = self.pts[-3], self.pts[-2], self.pts[-1]
                t3 = t2 + n
                xs = [p0[i], p1[i], p2[i], cand[i][0]]
                ts = [t0, t1, t2, t3]
                d1 = [(xs[j + 1] - xs[j]) / ((ts[j + 1] - ts[j]) * self.tick) for j in range(3)]
                d2 = [(d1[j + 1] - d1[j]) / ((ts[j + 2] - ts[j]) * self.tick) for j in range(2)]
                dd3 = (d2[1] - d2[0]) / ((ts[3] - ts[0]) * self.tick)
                if meth == "trapezoidal":
                    est = 6.0 * (1.0 / 12.0) * h ** 3 * abs(dd3)
                else:
                    w = h / (self.hlast * self.tick)
                    est = (1.0 + w) ** 2 / (w * (1.0 + 2.0 * w)) * h ** 3 * abs(dd3)
            xabs = abs(cand[i][0])              # AMBIGUOUS-1
            den = self.ltol * max(xabs, VNTOL) + VNTOL
            if est / den > r:
                r, worst = est / den, i
        return r, worst

    def _bound(self, n, d):
        """Rule 13 (and rule 10 for fixed)."""
        if self.control == "fixed":
            return min(self.step, d)
        if n >= d:
            return d
        if d - n < self.min_step:
            return d - self.min_step if d >= 2 * self.min_step else d
        return n

    def _clampstep(self, n):
        return max(self.min_step, min(self.max_step, n))

    def advance(self, target):
        """Advance from `solved` to `target` ticks; returns the number of steps taken."""
        taken = 0
        while self.solved < target:
            self._apply(self.solved)
            nxt = [t for (t, _) in self.bps if t > self.solved]
            limit = min([target] + nxt)
            d = limit - self.solved
            k = self.k
            fixed = self.control == "fixed"
            startup = k < 2
            want = self.prop
            fm = "be" if k < (2 if fixed else 3) else self.method
            if (not fixed) and fm == "gear2" and self.hlast:   # AMBIGUOUS-4/10: cap applied at use
                want = max(self.min_step, min(want, 2 * self.hlast))
            n = self._bound(want, d)
            attempts = 0
            tries = 0
            first_cause = None
            while True:
                tries += 1
                meth = "be" if k < (2 if fixed else 3) else self.method
                # Ruling (rule 6): no Gear 2 step with omega above 2, under either control; after a
                # short whole-distance step the next is backward Euler, no cut, order-1 test.
                if meth == "gear2" and self.hlast and n > 2 * self.hlast:
                    meth = "be"
                order = 1 if (meth == "be") else 2
                cand = self._solve(n, meth, k)
                if fixed or startup:
                    r = 0.0
                    worst = 0
                    ok = True
                else:
                    r, worst = self._ratio(n, meth, order, cand)
                    ok = r <= 1.0
                if ok:
                    break
                attempts += 1
                if first_cause is None:
                    first_cause = "error"
                if n <= self.min_step:          # AMBIGUOUS-2 (n < min_step falls here too)
                    self.notes.append(("step-minimum", self.solved, n))
                    break
                if attempts > self.max_rej:
                    self.notes.append(("non-convergence", self.solved, n))
                    self.stuck = (self.solved, r, worst)
                    return taken
                e = -0.5 if order == 1 else -1.0 / 3.0
                fac = min(0.9, max(0.25, 0.9 * r ** e))
                want = max(self.min_step, math.floor(n * fac))
                # Ruling (rule 12): a retry is never longer than the attempt it replaces; if rule 13
                # would shape it back to the failed size it keeps its own size.
                shaped = self._bound(want, d)
                n = want if shaped >= n else shaped
            shortened = n < want                # AMBIGUOUS-11: shaped by a limit, not by a retry
            if attempts:
                self.notes.append(("step-rejected", self.solved, attempts, first_cause))
            self.log.append(dict(start=self.solved, n=n, meth=meth, r=r, attempts=attempts, tries=tries))
            for i in range(len(self.br)):
                self.x[i], self.xd[i] = cand[i]
            self.solved += n
            self.pts.append((self.solved, list(self.x)))
            self.pts = self.pts[-4:]
            self.k = k + 1
            self.hlast = n
            taken += 1
            if (not fixed) and not startup:
                e = -0.5 if order == 1 else -1.0 / 3.0
                f = 2.0 if r == 0.0 else min(2.0, 0.9 * r ** e)
                newp = self._clampstep(math.floor(n * f))
                if shortened:
                    newp = max(newp, self.prop)
                self.prop = newp
        return taken

    def _clampstep_prop(self):
        return self.prop


def analytic_rc(t, tc, vstep_tick, tick, v0=1.0, v1=0.0):
    ts = vstep_tick * tick
    if t <= ts:
        return v0 * (1 - math.exp(-t / tc))
    vs = v0 * (1 - math.exp(-ts / tc))
    return v1 + (vs - v1) * math.exp(-(t - ts) / tc)


def run_rc_step(method, control="adaptive", **kw):
    """rc-step of RFC 013 rule 21: samples every 100 us to 5 ms, source 1V -> 0V at 2 ms."""
    p = Plant([dict(R=1e3, C=1e-6, ic=0.0, name="c")], method=method, control=control, **kw)
    samples = []
    p.set_input(2_000_000, 0.0)
    for k in range(0, 50):
        t = k * 100_000
        p.advance(t)
        samples.append((t, p.x[0]))
    return p, samples


def rc_step_error(method, **kw):
    p, s = run_rc_step(method, **kw)
    worst = max(abs(v - analytic_rc(t * 1e-9, 1e-3, 2_000_000, 1e-9)) for t, v in s)
    return p, s, worst


def order_test(method, divisor):
    """The order test of RFC 013 section 11: fixed, period 1 ms, step = RC/divisor, error at 2 ms."""
    step = 1_000_000 // divisor
    p = Plant([dict(R=1e3, C=1e-6, ic=0.0, name="c")], method=method, control="fixed", step=step,
              max_step=max(step * 100, 2), )
    for k in (1, 2):
        p.advance(k * 1_000_000)
    return abs(p.x[0] - (1 - math.exp(-2.0))), p


def main():
    print("== golden figures (RFC 013 rule 21: 0.26 and 0.83 times ltol*V)")
    for m in ("trapezoidal", "gear2"):
        p, s, w = rc_step_error(m)
        print(f"  {m:12s} worst sample error {w:.6e}  = {w/1e-3:.4f} ltol*V   steps={len(p.log)}  notes={len(p.notes)}")
    print("== golden with max-step = RC/10")
    for m in ("trapezoidal", "gear2"):
        p, s, w = rc_step_error(m, max_step=100_000)
        print(f"  {m:12s} {w/1e-3:.4f} ltol*V steps={len(p.log)}")
    print("== order test (RFC: 3.89..3.97 trapezoidal, 4.01..4.11 gear2)")
    for m in ("trapezoidal", "gear2"):
        errs = [order_test(m, d)[0] for d in (8, 16, 32, 64)]
        print(f"  {m:12s} errors {['%.3e' % e for e in errs]} ratios {[round(errs[i]/errs[i+1], 3) for i in range(3)]}")


if __name__ == "__main__":
    main()
