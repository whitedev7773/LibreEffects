#!/usr/bin/env python3
"""Independent E05 actual-binary64 cubic arc-length references (mpmath 1.3.0)."""
from __future__ import annotations
import argparse, hashlib, json, math, platform, random, sys, time
from pathlib import Path
from fractions import Fraction
import mpmath as mp

VERSION = 1
KAPPA = 0.5522847498307936
SEED = 20261004


def exact(x):
    n, d = float(x).as_integer_ratio()
    return mp.mpf(n) / d


def dec(x):
    return mp.nstr(x, 72, strip_zeros=False)


def line(a, b):
    return [a, a, b, b]


def from_vertices(vertices, closed):
    # Match only the existing primitive's binary64 position+offset operations.
    out = []
    for i in range(len(vertices) - (not closed)):
        a, b = vertices[i], vertices[(i + 1) % len(vertices)]
        out.append([a[0], [a[0][j] + a[2][j] for j in (0, 1)],
                    [b[0][j] + b[1][j] for j in (0, 1)], b[0]])
    return out


def definitions():
    cases = []
    def add(id, segments, note, closed=False, fractions=None, analytic=None):
        cases.append(dict(id=id, segments=segments, closed=closed, note=note,
                          fractions=fractions or [0., .125, .25, .5, .75, .875, 1.],
                          analytic=analytic))
    add('constant_true_zero', [[[13., -7.]] * 4], 'All four controls coincide; partial trim is empty.', analytic='zero')
    add('zero_handle_straight_100', [line([0., 0.], [100., 0.])],
        'Stored parameter is smoothstep, not distance. Quarter cuts are x=25 and x=75.', analytic='straight')
    add('unequal_straight_chain', [line([x, 0.], [y, 0.]) for x, y in [(0., 25.), (25., 100.), (100., 300.)]],
        'Segment lengths 25, 75, 200; binary64 fractions near segment boundaries are intentionally not snapped.',
        fractions=[0., 1/12, .25, 1/3, .5, .75, 1.], analytic='straight')
    add('unequal_dyadic_boundary_chain', [line([x, 0.], [y, 0.]) for x, y in [(0., 16.), (16., 64.), (64., 256.)]],
        'Exact binary64 cumulative boundary fractions 1/16 and 1/4; unequal lengths 16, 48, 192.',
        fractions=[0., .0625, .125, .25, .5, .875, 1.], analytic='straight')
    add('mixed_repeat_block', [line([0., 0.], [100., 0.]),
        [[100., 0.], [150., 40.], [90., 70.], [100., 100.]],
        [[100., 100.], [50., 150.], [0., 150.], [0., 100.]],
        line([0., 100.], [0., 0.])],
        'Connected four-segment mixed closed block. Repeat 256 times for a 1024-segment stress reference.', True)
    rect = [[0., 0.], [100., 0.], [100., 40.], [0., 40.]]
    for suffix, vertices in [('', rect), ('_reversed', [rect[0]] + list(reversed(rect[1:]))), ('_shifted_first', rect[2:] + rect[:2])]:
        add('unequal_rectangle' + suffix, [line(vertices[i], vertices[(i+1)%4]) for i in range(4)],
            'Closed 100 by 40 rectangle, perimeter 280. Original seam is a topological boundary.', True,
            fractions=[0., .125, .25, .5, .75, .875, 1.], analytic='rectangle')
    add('actual_binary64_parabola', [[[0., 0.], [1/3, 0.], [2/3, 1/3], [1., 1.]]],
        'Integrates the actual binary64 cubic. Ideal (t,t^2) analytic length is recorded separately.', analytic='parabola')
    add('collinear_zero_chord_backtrack', [[[0., 0.], [100., 0.], [-100., 0.], [0., 0.]]],
        'Positive-length zero-chord curve; two interior stationary roots must be split.', analytic='backtrack')
    add('noncollinear_zero_chord_loop', [[[0., 0.], [120., 200.], [-80., 180.], [0., 0.]]],
        'Positive-length coincident endpoints, not a constant cubic.')
    add('noncollinear_exact_cusp', [[[3., -1.], [-1., 1.], [-1., -1.], [3., 1.]]],
        'Exact cusp at t=1/2: B=(12(t-1/2)^2, 8(t-1/2)^3).', analytic='cusp')
    add('noncollinear_near_cusp', [[[3., -1.], [-1., 1.], [-1., -1. + 2**-24], [3., 1.]]],
        'Near cusp with small nonzero speed, geometric quadrature mesh resolves the minimum.')
    w, h = 200., 120.
    rx, ry = w / 2, h / 2
    vertices = [([w, ry], [0., -KAPPA * ry], [0., KAPPA * ry]),
                ([rx, h], [KAPPA * rx, 0.], [-KAPPA * rx, 0.]),
                ([0., ry], [0., KAPPA * ry], [0., -KAPPA * ry]),
                ([rx, 0.], [-KAPPA * rx, 0.], [KAPPA * rx, 0.])]
    ellipse = from_vertices(vertices, True)
    add('kappa_ellipse_quarter_actual_controls', ellipse[:1], 'Actual existing KAPPA ellipse cubic, not an ideal circular/elliptic arc.')
    add('kappa_ellipse_closed_actual_controls', ellipse, 'All four actual existing KAPPA ellipse cubics.', True)
    r = 20.; k = KAPPA * r
    rounded = from_vertices([([r, 0.], [-k, 0.], [0., 0.]), ([w-r, 0.], [0., 0.], [k, 0.]),
        ([w, r], [0., -k], [0., 0.]), ([w, h-r], [0., 0.], [0., k]),
        ([w-r, h], [k, 0.], [0., 0.]), ([r, h], [0., 0.], [-k, 0.]),
        ([0., h-r], [0., k], [0., 0.]), ([0., r], [0., 0.], [0., -k])], True)
    add('kappa_rounded_corner_actual_controls', rounded[1:2], 'Top-right corner from 200 by 120 rounded rectangle, radius 20.')
    add('kappa_rounded_rectangle_actual_controls', rounded, 'Actual eight-segment existing rounded rectangle, including zero-handle straight edges.', True)
    rng = random.Random(SEED)
    for name, scale in [('tiny', 2**-40), ('normal', 1.), ('source_limit', 2**16)]:
        pts = [[rng.randint(-64, 64) / 16 * scale for _ in range(2)] for _ in range(4)]
        add('seeded_' + name, [pts], f'Deterministic Python Random seed {SEED}, scale {float(scale).hex()}.',
            fractions=[0., 2**-20, .125, .5, .875, 1.-2**-20, 1.])
    add('tiny_zero_chord_loop', [[[0., 0.], [2**-40, 2**-39], [-2**-40, 2**-39], [0., 0.]]],
        'Tiny positive-length loop; absolute-only accuracy would erase meaningful features.')
    add('source_limit_long_handles', [[[0., 0.], [1000000., 1000000.], [-1000000., 1000000.], [1., 0.]]],
        'Source-admissible million-unit tangent offsets, almost closed, long curved path.')
    add('source_limit_close_controls', [[[999999., 999999.], [999999. + 2**-30, 999999.],
        [999999. + 2**-29, 999999. + 2**-30], [999999. + 2**-28, 999999. + 2**-29]]],
        'Few-ULP controls near source coordinate limit; correct positive length despite large translation.')
    return cases


def poly(c, t):
    return mp.polyval(c, t)


def real_roots_unit(coeff):
    """Isolate real polynomial roots via derivative critical points, then bisection."""
    while coeff and coeff[0] == 0:
        coeff = coeff[1:]
    if len(coeff) <= 1:
        return []
    if len(coeff) == 2:
        r = -coeff[1] / coeff[0]
        return [r] if 0 < r < 1 else []
    n = len(coeff) - 1
    extrema = real_roots_unit([c * (n-i) for i, c in enumerate(coeff[:-1])])
    grid = [mp.mpf(0)] + extrema + [mp.mpf(1)]
    scale = max(abs(c) for c in coeff)
    eps = mp.power(10, -(mp.mp.dps - 8)) * scale
    roots = [x for x in extrema if abs(poly(coeff, x)) <= eps]
    for a, b in zip(grid, grid[1:]):
        fa, fb = poly(coeff, a), poly(coeff, b)
        if abs(fa) <= eps or abs(fb) <= eps or fa * fb >= 0:
            continue
        for _ in range(mp.mp.prec + 4):
            m = (a+b)/2; fm = poly(coeff, m)
            if fm == 0:
                a = b = m; break
            if fa * fm < 0:
                b = m
            else:
                a = m; fa = fm
        roots.append((a+b)/2)
    return sorted(set(roots))


class Arc:
    def __init__(self, controls):
        self.p = [[exact(x) for x in pt] for pt in controls]
        self.v = [[3 * (-self.p[0][j] + 3*self.p[1][j] - 3*self.p[2][j] + self.p[3][j]),
                   6 * (self.p[0][j] - 2*self.p[1][j] + self.p[2][j]),
                   3 * (self.p[1][j] - self.p[0][j])] for j in range(2)]
        self.constant = all(x == self.p[0] for x in self.p)
        self.coord_scale = max([mp.mpf(1)] + [abs(x) for p in self.p for x in p])
        # Roots of derivative components and of d(speed^2)/dt. This is polynomial
        # root isolation, not production chord/control-polygon subdivision.
        roots = [r for v in self.v for r in real_roots_unit(v)]
        a,b,c = self.v[0]; d,e,f = self.v[1]
        speed2prime = [4*(a*a+d*d), 6*(a*b+d*e), 2*(b*b+e*e+2*a*c+2*d*f), 2*(b*c+e*f)]
        roots += real_roots_unit(speed2prime)
        eps = mp.power(10, -(mp.mp.dps - 12))
        nodes = [mp.mpf(0), mp.mpf(1)]
        for r in sorted(roots):
            if all(abs(r-x) > eps for x in nodes):
                nodes.append(r)
        # Resolve narrow speed minima explicitly. A merely repeated precision
        # run could otherwise miss the same near-cusp feature twice.
        for r in list(nodes):
            acceleration = mp.sqrt(sum((2*v[0]*r + v[1])**2 for v in self.v))
            if acceleration == 0:
                continue
            width = self.speed(r) / acceleration
            if width <= eps or width >= mp.mpf('.02'):
                continue
            while width < 1:
                for x in [r-width, r+width]:
                    if 0 < x < 1:
                        nodes.append(x)
                width *= 8
        self.nodes = sorted(set(nodes))
        self.panel_lengths = [self.integrate(a,b) for a,b in zip(self.nodes, self.nodes[1:])]
        self.length = mp.fsum(self.panel_lengths)

    def speed(self, t):
        return mp.sqrt(sum(poly(v,t)**2 for v in self.v))

    def point(self, t):
        # Bernstein evaluation independent of extraction's de Casteljau.
        u = 1-t
        return [u**3*self.p[0][j] + 3*u*u*t*self.p[1][j] + 3*u*t*t*self.p[2][j] + t**3*self.p[3][j] for j in range(2)]

    def integrate(self, a, b):
        if self.constant or a == b:
            return mp.mpf(0)
        return mp.quad(self.speed, [a,b], method='tanh-sinh')

    def prefix(self, t):
        total = mp.mpf(0)
        for a,b,length in zip(self.nodes,self.nodes[1:],self.panel_lengths):
            if t >= b:
                total += length
            elif t > a:
                return total + self.integrate(a,t)
            else:
                return total
        return total

    def invert(self, target):
        if target <= 0:
            return mp.mpf(0), mp.mpf(0), 0
        if target >= self.length:
            return mp.mpf(1), self.length, 0
        tol = self.length * mp.power(10, -(mp.mp.dps-18))
        prefix = mp.mpf(0)
        for a,b,length in zip(self.nodes,self.nodes[1:],self.panel_lengths):
            if abs(target-prefix) < tol:
                return a, prefix, 0
            if abs(target-prefix-length) < tol:
                return b, prefix+length, 0
            if target < prefix+length:
                break
            prefix += length
        base = a; wanted = target-prefix
        t = a+(b-a)*wanted/length
        # Safeguarded Newton with a bracketing fallback. The independent
        # quadrature is recomputed at each candidate, no production table reuse.
        for iteration in range(mp.mp.prec + 32):
            value = self.integrate(base,t)
            residual = value-wanted
            if abs(residual) <= tol:
                return t, prefix+value, iteration+1
            if residual > 0: b=t
            else: a=t
            speed = self.speed(t)
            candidate = t-residual/speed if speed else (a+b)/2
            if not a < candidate < b or candidate == t:
                candidate = (a+b)/2
            t = candidate
        raise ArithmeticError('Root did not converge')


def evaluate(case, dps):
    with mp.workdps(dps):
        arcs = [Arc(s) for s in case['segments']]
        lengths = [a.length for a in arcs]; total = mp.fsum(lengths)
        out = dict(total=total, lengths=lengths, panels=[a.nodes for a in arcs], cuts=[])
        if total == 0:
            out['ideal_length'] = mp.mpf(0)
            return out
        for f in case['fractions']:
            target = exact(f)*total; prefix = mp.mpf(0)
            selected = len(arcs)-1
            for i,a in enumerate(arcs):
                # Start boundary follows, end boundary precedes. The stored
                # representation is canonical preceding segment at equality.
                if target <= prefix+a.length:
                    selected=i; break
                prefix += a.length
            t,local,iterations = arcs[selected].invert(target-prefix)
            t_f64 = float(t)
            rounded_prefix = prefix + arcs[selected].prefix(exact(t_f64))
            out['cuts'].append(dict(fraction=f, segment_index=selected, t=t,
                point=arcs[selected].point(t), prefix_length=prefix+local,
                target_length=target, residual=abs(prefix+local-target), iterations=iterations,
                t_f64=t_f64, t_f64_prefix_length=rounded_prefix,
                t_f64_arc_rounding_residual=abs(rounded_prefix-target),
                t_f64_point=arcs[selected].point(exact(t_f64))))
        if case.get('analytic') == 'parabola':
            out['ideal_length'] = mp.sqrt(5)/2 + mp.asinh(2)/4
        elif case.get('analytic') == 'backtrack':
            out['ideal_length'] = 200*mp.sqrt(3)/3
        elif case.get('analytic') == 'cusp':
            # |B'| = 24 |s| sqrt(1+s^2), s=t-1/2.
            out['ideal_length'] = 16*((mp.mpf(5)/4)**mp.mpf('1.5')-1)
        elif case.get('analytic') == 'straight':
            out['ideal_length'] = abs(exact(case['segments'][-1][-1][0])-exact(case['segments'][0][0][0]))
        elif case.get('analytic') == 'rectangle':
            out['ideal_length'] = mp.mpf(280)
        elif case.get('analytic') == 'zero':
            out['ideal_length'] = mp.mpf(0)
        return out


def compare(case, a, b):
    with mp.workdps(140):
        length_delta = max([abs(a['total']-b['total'])] + [abs(x-y) for x,y in zip(a['lengths'],b['lengths'])])
        point_delta = mp.mpf(0); parameter_delta = mp.mpf(0); residual_max = mp.mpf(0)
        for x,y in zip(a['cuts'],b['cuts']):
            if x['segment_index'] != y['segment_index']:
                raise AssertionError(f"{case['id']}: segment ownership changed")
            parameter_delta = max(parameter_delta, abs(x['t']-y['t']))
            point_delta = max(point_delta, *[abs(p-q) for p,q in zip(x['point'],y['point'])])
            residual_max = max(residual_max, x['residual'], y['residual'])
            length_delta = max(length_delta, abs(x['t_f64_prefix_length']-y['t_f64_prefix_length']))
            point_delta = max(point_delta, *[abs(p-q) for p,q in zip(x['t_f64_point'],y['t_f64_point'])])
        scale = max([mp.mpf(1), b['total']] + [abs(exact(x)) for s in case['segments'] for p in s for x in p])
        allowed = scale*mp.mpf('1e-55')
        assert length_delta < allowed and point_delta < allowed, (case['id'],length_delta,point_delta,allowed)
        if case.get('analytic') and case['analytic'] != 'parabola':
            assert abs(b['total']-b['ideal_length']) < allowed, (case['id'], 'analytic crosscheck')
        return dict(length_absolute_delta_80_vs_120=dec(length_delta),
            point_max_absolute_delta_80_vs_120=dec(point_delta),
            parameter_max_absolute_delta_80_vs_120=dec(parameter_delta),
            max_inversion_residual_both_precisions=dec(residual_max),
            convergence_limit=dec(allowed), empirical_reference_uncertainty=dec(allowed),
            uncertainty_kind='conservative empirical allowance, not an interval proof')



def directed_binary64(decimal_text):
    """Minimal binary64 hull of the exact stored decimal, without uncertainty padding."""
    value = Fraction(decimal_text)
    rounded = float(value)
    represented = Fraction.from_float(rounded)
    lower = math.nextafter(rounded, -math.inf) if represented > value else rounded
    upper = math.nextafter(rounded, math.inf) if represented < value else rounded
    return lower, upper


def add_directed_bounds(record, field):
    value = record[field]
    if isinstance(value, list):
        bounds = [directed_binary64(x) for x in value]
        lower, upper = [x[0] for x in bounds], [x[1] for x in bounds]
    else:
        lower, upper = directed_binary64(value)
    record[field + '_f64_lower'] = lower
    record[field + '_f64_upper'] = upper


def serialize(case, result, convergence):
    def ratio(x):
        n,d=float(x).as_integer_ratio(); return [str(n),str(d)]
    record = {k:case[k] for k in ['id','closed','note','segments']}
    record['input_hex'] = [[[float(x).hex() for x in p] for p in s] for s in case['segments']]
    record['input_ratios'] = [[[ratio(x) for x in p] for p in s] for s in case['segments']]
    record['total_length'] = dec(result['total'])
    record['segment_lengths'] = [dec(x) for x in result['lengths']]
    add_directed_bounds(record, 'total_length')
    add_directed_bounds(record, 'segment_lengths')
    record['quadrature_panels'] = [[dec(x) for x in ns] for ns in result['panels']]
    record['convergence'] = convergence
    record['cuts'] = []
    for cut in result['cuts']:
        c = {k:cut[k] for k in ['fraction','segment_index','iterations']}
        c['fraction_hex'] = cut['fraction'].hex(); c['fraction_ratio'] = ratio(cut['fraction'])
        for k in ['t','prefix_length','target_length','residual']:
            c[k] = dec(cut[k])
        c['point'] = [dec(x) for x in cut['point']]
        c['t_f64'] = cut['t_f64']
        c['t_f64_hex'] = cut['t_f64'].hex()
        c['t_f64_prefix_length'] = dec(cut['t_f64_prefix_length'])
        c['t_f64_arc_rounding_residual'] = dec(cut['t_f64_arc_rounding_residual'])
        c['t_f64_point'] = [dec(x) for x in cut['t_f64_point']]
        c['empirical_reference_uncertainty'] = convergence['empirical_reference_uncertainty']
        for field in ['t', 'point', 'prefix_length', 'target_length', 't_f64_prefix_length']:
            add_directed_bounds(c, field)
        record['cuts'].append(c)
    if 'ideal_length' in result:
        record['analytic_crosscheck'] = {'kind':case['analytic'], 'length':dec(result['ideal_length']),
            'actual_minus_analytic':dec(result['total']-result['ideal_length'])}
    return record


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--output',type=Path,default=Path(__file__).with_name('trim-arc-oracles.json'))
    parser.add_argument('--checkpoint-dir',type=Path)
    parser.add_argument('--case',action='append')
    args=parser.parse_args()
    if mp.__version__ != '1.3.0': raise RuntimeError('Regeneration requires mpmath 1.3.0')
    cases = [c for c in definitions() if not args.case or c['id'] in args.case]
    records=[]
    for case in cases:
        started=time.monotonic()
        print(f"START {case['id']}", flush=True)
        low=evaluate(case,80); high=evaluate(case,120)
        convergence=compare(case,low,high)
        with mp.workdps(140):
            record=serialize(case,high,convergence)
        records.append(record)
        if args.checkpoint_dir:
            args.checkpoint_dir.mkdir(parents=True,exist_ok=True)
            (args.checkpoint_dir / (case['id']+'.json')).write_text(json.dumps(record,indent=2)+'\n')
        print(f"PASS {case['id']} cuts={len(record['cuts'])} length={float(high['total']):.17g} seconds={time.monotonic()-started:.2f}",flush=True)
    document=dict(schema_version=VERSION,provenance=dict(generator=Path(__file__).name,
        generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        python=platform.python_version(),mpmath=mp.__version__,random_seed=SEED,
        precisions_decimal_digits=[80,120],stored_significant_digits=72,
        method='Exact binary64 ratios; derivative-speed tanh-sinh quadrature; polynomial stationary-root splitting and graded near-cusp mesh; safeguarded Newton/bisection inversion.',
        independent_of='No production Trim code, chord/control-polygon bounds, Simpson formula, output SVG, or production subdivision is used.',
        convergence_requirement='Length and cut-point changes less than 1e-55 times max(1, total length, max absolute input coordinate). Reference uncertainty is empirical, not a rigorous enclosure.',
        directed_binary64_bounds='Minimal binary64 hull of each exact stored decimal string, computed by rational comparison and nextafter; no empirical uncertainty is added. Equal bounds mean the stored decimal is exactly representable, not that numerical quadrature has become a formal proof.',
        cut_boundary='Canonical preceding segment at exact cumulative equality; t=0/1 retained exactly.',
        production_cut_budget='min(1/1024, min(retained arc, removed arc)/8). Full-span identity and equal-endpoint empty are exact topology rules.'),cases=records)
    block = next((c for c in records if c['id'] == 'mixed_repeat_block'), None)
    document['stress_cases'] = []
    if block:
        with mp.workdps(120):
            document['stress_cases'].append(dict(id='mixed_1024_segments', block_case_id='mixed_repeat_block',
                repeat_count=256, segment_count=1024, closed=True,
                total_length=dec(mp.mpf(block['total_length'])*256),
                empirical_reference_uncertainty=dec(mp.mpf(block['convergence']['empirical_reference_uncertainty'])*256),
                note='Expand the connected four-segment block 256 times, preserving traversal order. Length additivity gives the reference; no new quadrature algorithm.'))
    for stress in document['stress_cases']:
        add_directed_bounds(stress, 'total_length')
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(document,indent=2)+'\n')
    print(f"WROTE {args.output} cases={len(records)} cuts={sum(len(c['cuts']) for c in records)}",flush=True)

if __name__ == '__main__': main()
