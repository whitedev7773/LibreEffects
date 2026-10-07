#!/usr/bin/env python3
"""Crosscheck committed tanh-sinh references using independent Gauss-Legendre quadrature."""
import importlib.util, json, math, sys, time
from pathlib import Path
from fractions import Fraction
import mpmath as mp
sys.dont_write_bytecode = True
ROOT=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('oracle_generator',ROOT/'generate_trim_arc_oracles.py')
gen=importlib.util.module_from_spec(spec);spec.loader.exec_module(gen)
doc=json.loads((ROOT/'trim-arc-oracles.json').read_text())
mp.mp.dps=100
checks=0;max_error=mp.mpf(0)
directed_checks=0

def verify_directed(record, field):
    global directed_checks
    values=record[field]
    lower=record[field+'_f64_lower'];upper=record[field+'_f64_upper']
    triples=zip(values,lower,upper) if isinstance(values,list) else [(values,lower,upper)]
    for text,lo,hi in triples:
        value=Fraction(text);qlo=Fraction.from_float(lo);qhi=Fraction.from_float(hi)
        assert qlo<=value<=qhi,(field,text,lo,hi)
        if value==qlo or value==qhi:
            assert lo==hi,(field,'exact decimal must have equal endpoints')
        else:
            assert math.nextafter(lo,math.inf)==hi,(field,'endpoints must be adjacent')
        directed_checks+=1
for case in doc['cases']:
    started=time.monotonic()
    verify_directed(case,'total_length')
    verify_directed(case,'segment_lengths')
    for seg,hexseg,ratioseg in zip(case['segments'],case['input_hex'],case['input_ratios']):
        for pt,hpt,rpt in zip(seg,hexseg,ratioseg):
            for x,h,r in zip(pt,hpt,rpt):
                assert float(x).hex()==h
                assert list(map(str,float(x).as_integer_ratio()))==r
    arcs=[gen.Arc(seg) for seg in case['segments']]
    gl_panels=[[mp.quad(a.speed,[lo,hi],method='gauss-legendre') for lo,hi in zip(a.nodes,a.nodes[1:])] for a in arcs]
    gl_lengths=[mp.fsum(lengths) for lengths in gl_panels]
    allowance=mp.mpf(case['convergence']['empirical_reference_uncertainty'])
    def check(got,expected):
        global checks,max_error
        delta=abs(got-mp.mpf(expected))
        assert delta<allowance, (case['id'],str(delta),str(allowance))
        checks+=1;max_error=max(max_error,delta)
    check(mp.fsum(gl_lengths),case['total_length'])
    for got,expected in zip(gl_lengths,case['segment_lengths']):check(got,expected)
    for cut in case['cuts']:
        for field in ['t','point','prefix_length','target_length','t_f64_prefix_length']:
            verify_directed(cut,field)
        i=cut['segment_index'];a=arcs[i];t=gen.exact(cut['t_f64']);prefix=mp.fsum(gl_lengths[:i])
        for lo,hi,length in zip(a.nodes,a.nodes[1:],gl_panels[i]):
            if t>=hi:prefix+=length
            elif t>lo:
                prefix+=mp.quad(a.speed,[lo,t],method='gauss-legendre');break
            else:break
        check(prefix,cut['t_f64_prefix_length'])
        # Every stored numeric parameter is exactly the hex it claims to be.
        assert cut['t_f64'].hex()==cut['t_f64_hex']
        assert cut['fraction'].hex()==cut['fraction_hex']
    print(f"PASS {case['id']} Gauss-Legendre panels={sum(map(len,gl_panels))} checks={checks} elapsed={time.monotonic()-started:.2f}s",flush=True)
for stress in doc['stress_cases']:
    verify_directed(stress,'total_length')
    block=next(c for c in doc['cases'] if c['id']==stress['block_case_id'])
    assert len(block['segments'])*stress['repeat_count']==stress['segment_count']
    assert abs(mp.mpf(block['total_length'])*stress['repeat_count']-mp.mpf(stress['total_length']))<mp.mpf(stress['empirical_reference_uncertainty'])
for case_id,expected in [('constant_true_zero',0.),('zero_handle_straight_100',100.),('unequal_dyadic_boundary_chain',256.)]:
    exact_case=next(c for c in doc['cases'] if c['id']==case_id)
    assert exact_case['total_length_f64_lower']==exact_case['total_length_f64_upper']==expected
print(f'VERIFIED directed_binary64_hulls={directed_checks}',flush=True)
print(f'VERIFIED cases={len(doc["cases"])} checks={checks} max_absolute_difference={mp.nstr(max_error,15)}',flush=True)
