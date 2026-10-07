#!/usr/bin/env python3
"""Author synthetic schema-78 LEPs; no production imports, builds, or renders."""
import copy
import hashlib
import json
from pathlib import Path
import struct
import zlib

HERE = Path(__file__).resolve().parent
NEXT_LAYER = 1


def track(v):
    return {"value": v, "keys": {}}


def layer(name, x, y, w, h, color=0x00FFFF, content="Solid", opacity=100):
    global NEXT_LAYER
    n = NEXT_LAYER
    NEXT_LAYER += 1
    return {"id": n, "name": name, "content": content, "visible": True,
            "locked": False, "width": w, "height": h, "color": color,
            "properties": {k: track(v) for k, v in {
                "PositionX": x, "PositionY": y, "AnchorX": 0, "AnchorY": 0,
                "ScaleX": 100, "ScaleY": 100, "Rotation": 0, "Opacity": opacity,
            }.items()}, "effects": {"blur": 0, "brightness": 1, "grayscale": False},
            "next_effect_id": 1, "next_mask_id": 1, "mask": None,
            "in_frame": 0, "out_frame": None, "parent": None,
            "transform_offset": [1, 0, 0, 1, 0, 0]}


def effect(kind, params, repeat=None, bypass=False):
    v = {"id": 1, "kind": kind, "name": kind, "bypassed": bypass,
         "color_space": "Srgb", "parameters": {k: track(x) for k, x in params.items()}}
    if repeat is not None:
        v["gaussian_edge_mode"] = "Repeat" if repeat else "Transparent"
    return v


def blur(sigma=2, repeat=True, bypass=False):
    return effect("GaussianBlur", {"Radius": sigma}, repeat, bypass)


def stack(l, effects):
    l["effect_stack"] = copy.deepcopy(effects)
    for i, e in enumerate(l["effect_stack"], 1):
        e["id"] = i
    l["next_effect_id"] = len(effects) + 1
    return l


def comp(name, w, h, layers):
    return {"name": name, "width": w, "height": h, "fps": 60,
            "display_start": 0, "duration": 60, "background_color": 0, "layers": layers}


def project(comps):
    return {"version": 78, "composition_id": 1,
            "next_composition_id": max(comps) + 1, "next_layer_id": NEXT_LAYER,
            "composition": comps[1],
            "other_compositions": {str(k): v for k, v in comps.items() if k != 1}}


def native_bytes(value):
    payload = json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()
    prefix = struct.pack("<4sHHQ", b"PROJ", 1, 0, len(payload))
    chunk = prefix + struct.pack("<I", zlib.crc32(prefix + payload)) + payload
    header = struct.pack("<8sHHIQI", b"\x89LEP\r\n\x1a\n", 1, 32, 0, 32 + len(chunk), 1)
    return header + struct.pack("<I", zlib.crc32(header)) + chunk


def write(name, comps):
    p = project(comps)
    (HERE / (name + ".native.json")).write_text(json.dumps(p, indent=2) + "\n")
    b = native_bytes(p)
    (HERE / (name + ".lep")).write_bytes(b)
    return {"path": name + ".lep", "bytes": len(b), "sha256": hashlib.sha256(b).hexdigest()}


CASES = []


def case(group, cid, name, size, assertions, **extra):
    CASES.append({"name": name, "input": group + ".lep", "composition": cid,
                  "frame": 0, "size": size, "output_directory": "renders/" + name,
                  "probe_argv": [group + ".lep", str(cid), "0", "renders/" + name],
                  "assertions": assertions, **extra})


def constant_cases():
    comps = {90: comp("Preblur premultiplied partial-alpha input", 20, 20,
                     [layer("Red alpha128 before outer Gaussian", 0, 0, 20, 20, 0xFF0000, opacity=50)])}
    variants = [(1, "constant-repeat", blur()),
                (2, "constant-omitted-off", blur(repeat=None)),
                (3, "constant-explicit-off", blur(repeat=False)),
                (4, "constant-radius0-repeat", blur(0)),
                (5, "constant-radius0-off", blur(0, None)),
                (6, "constant-bypassed-repeat", blur(bypass=True))]
    for cid, name, fx in variants:
        opaque = stack(layer("Opaque cyan source", 16, 20, 20, 20), [fx])
        partial = stack(layer("Actual partial-alpha precomposition", 88, 20, 20, 20,
                              content={"Composition": {"composition": 90, "start_frame": 0}}), [fx])
        comps[cid] = comp(name, 128, 64, [opaque, partial])
        if cid == 1:
            assertions = [{"type": "literal_rectangles", "rectangles": [
                [8, 12, 36, 36, [0, 255, 255, 255]], [80, 12, 36, 36, [255, 0, 0, 128]]]}]
        elif cid in (2, 3):
            assertions = [{"type": "alpha_zero_outside", "rectangles": [[8, 12, 36, 36], [80, 12, 36, 36]]},
                          {"type": "literal_pixels", "pixels": [
                              [16, 30, [0, 255, 255, 154]], [17, 30, [0, 255, 255, 201]],
                              [18, 30, [0, 255, 255, 233]], [19, 30, [0, 255, 255, 249]],
                              [20, 30, [0, 255, 255, 254]], [21, 30, [0, 255, 255, 255]],
                              [88, 30, [255, 0, 0, 77]], [89, 30, [255, 0, 0, 101]],
                              [90, 30, [255, 0, 0, 117]], [91, 30, [255, 0, 0, 125]],
                              [92, 30, [255, 0, 0, 127]], [93, 30, [255, 0, 0, 128]]]}]
            if cid == 3:
                assertions.append({"type": "equal_image", "other": "constant-omitted-off"})
        else:
            assertions = [{"type": "literal_rectangles", "rectangles": [
                [16, 20, 20, 20, [0, 255, 255, 255]], [88, 20, 20, 20, [255, 0, 0, 128]]]}]
        case("constant-boundaries", cid, name, [128, 64], assertions,
             purpose="Original input D is each 20x20 source. Repeat O retains finite sigma*4 padding; partial alpha is inside the precomposition before filtering.")
    return comps


def adjustment_cases():
    comps = {}
    for cid, name, constant, repeat in [(1, "adjustment-contrast-repeat", False, True),
                                        (2, "adjustment-constant-repeat", True, True),
                                        (3, "adjustment-contrast-off", False, None)]:
        matte = layer("Disabled matte, tighter than adjustment", 32, 28, 24, 8, 0xFFFFFF)
        matte["visible"] = False
        adj = stack(layer("Adjustment coverage only", 32, 24, 32, 16, content="Adjustment"), [blur(2, repeat)])
        adj["track_matte"] = {"source": matte["id"], "mode": "Alpha"}
        lower = [layer("Full lower input", 0, 0, 96, 64, 0x00FFFF if constant else 0x0000FF)]
        if not constant:
            lower.insert(0, layer("Red input outside the coverage rectangle", 0, 0, 32, 64, 0xFF0000))
        comps[cid] = comp(name, 96, 64, [matte, adj] + lower)
        if constant:
            assertions = [{"type": "literal_rectangles", "rectangles": [[0, 0, 96, 64, [0, 255, 255, 255]]]}]
        else:
            assertions = [{"type": "adjustment_literal", "effective_coverage": [32, 28, 24, 8],
                           "transition_rgba": [[101, 0, 154, 255], [54, 0, 201, 255],
                                               [22, 0, 233, 255], [6, 0, 249, 255], [1, 0, 254, 255]]}]
            if cid == 3:
                assertions.append({"type": "equal_image", "other": "adjustment-contrast-repeat"})
        case("interior-adjustment", cid, name, [96, 64], assertions,
             purpose="D is full lower composition [0,0,96,64], expressed in adjustment local space as [-32,-24,96,64]. Coverage [32,24,32,16] intersects hidden matte [32,28,24,8]. Red must cross the coverage left boundary into filtered blue; no coverage or matte expansion.")
    return comps


def shadow_cases():
    comps = {}
    shadow = effect("DropShadow", {"Radius": 0, "OffsetX": 8, "OffsetY": 0,
                                  "Red": 0, "Green": 0, "Blue": 0, "Opacity": 100})
    for cid, name, x, y, scale, size in [(1, "shadow-then-repeat", 24, 24, 100, [128, 80]),
                                        (2, "shadow-translated", 40, 32, 100, [128, 80]),
                                        (3, "shadow-half-scale", 12, 12, 50, [64, 40])]:
        l = stack(layer("Ordered black shadow then Repeat", x, y, 20, 20), [shadow, blur()])
        l["properties"]["ScaleX"] = l["properties"]["ScaleY"] = track(scale)
        comps[cid] = comp(name, *size, [l])
        if cid == 3:
            assertions = [{"type": "alpha_zero_outside", "rectangles": [[4, 8, 26, 18]]},
                          {"type": "literal_pixels", "pixels": [[28, 17, [0, 0, 0, 255]],
                                                                    [30, 17, [0, 0, 0, 0]],
                                                                    [5, 17, [0, 0, 0, 0]]]}]
        else:
            dx, dy = x - 24, y - 24
            assertions = [{"type": "alpha_zero_outside", "rectangles": [[8 + dx, 16 + dy, 52, 36]]},
                          {"type": "literal_pixels", "pixels": [
                              [17 + dx, 30 + dy, [0, 0, 0, 0]],
                              [56 + dx, 30 + dy, [0, 0, 0, 255]],
                              [59 + dx, 30 + dy, [0, 0, 0, 255]],
                              [60 + dx, 30 + dy, [0, 0, 0, 0]],
                              [32 + dx, 17 + dy, [0, 255, 255, 255]],
                              [32 + dx, 51 + dy, [0, 255, 255, 255]]]}]
            if cid == 2:
                assertions.append({"type": "translated_image", "other": "shadow-then-repeat", "offset": [16, 8]})
        case("ordered-shadow", cid, name, size, assertions,
             purpose="Shadow D becomes [-8,0,36,20]; Repeat reads that preceding stage and emits O [-16,-8,52,36]. Cyan source local [0,0,20,20]; shadow extends to x28. Right black edge repeats to x36, while the transparent left domain edge remains transparent.",
             reduced_note="Half-scale composition uses 50% source transform. It exercises the raster transform and finite extent; not a test of the render_output resize API." if cid == 3 else None)
    return comps


def rejection_cases():
    comps = {}
    for cid, name in [(1, "reject-rotation"), (2, "reject-shear")]:
        l = stack(layer(name, 30, 20, 20, 20), [blur()])
        if cid == 1:
            l["properties"]["Rotation"] = track(30)
        else:
            l["transform_offset"] = [1, 0, 0.25, 1, 0, 0]
        comps[cid] = comp(name, 96, 64, [l])
        case("unsupported-transforms", cid, name, [96, 64], [],
             expected_error={"stderr_contains": ["Repeat Edge Pixels", "UnsupportedTransform"],
                             "nonzero_exit": True, "must_not_write": "actual.png"},
             purpose="An ordinary non-axis-aligned source must report rejection, never silently render transparent-edge blur or blank success.")
    return comps


def diagnostic_case():
    adj = stack(layer("Dimensions-only native sigma70 diagnostic", 60, 60, 1800, 682, content="Adjustment"), [blur(70)])
    lower = layer("Synthetic constant full-composition input", 0, 0, 1920, 960)
    name = "white-dimensions-diagnostic"
    case("white-dimensions-diagnostic", 1, name, [1920, 960],
         [{"type": "literal_rectangles", "rectangles": [[0, 0, 1920, 960, [0, 255, 255, 255]]]}],
         purpose="Synthetic large-canvas stress diagnostic: 1920x960 canvas and1800x682 adjustment at(60,60). Native sigma70 is a deliberate stress value, not a calibration to any third-party blur parameter. No external media or project content is used.",
         optional=True)
    return {1: comp(name, 1920, 960, [adj, lower])}


def main():
    files = [write("constant-boundaries", constant_cases()),
             write("interior-adjustment", adjustment_cases()),
             write("ordered-shadow", shadow_cases()),
             write("unsupported-transforms", rejection_cases()),
             write("white-dimensions-diagnostic", diagnostic_case())]
    plan = {"schema": 1, "fixture_project_schema": 78, "prepared_against": "ad3bd23 model; current production render modules", "rendered_during_preparation": False,
            "cwd": str(HERE), "probe_contract": "PROBE INPUT COMPOSITION FRAME OUTDIR; supply a verified schema78 production-module probe",
            "capture_contract": "For every case save renders/NAME/result.json containing exit_code, stdout and stderr after probe exit. The probe requires OUTDIR not to exist when invoked. Capture first, then create OUTDIR if necessary and write result.json. Never fabricate success receipts.",
            "png_encoding": "8-bit straight RGBA PNG; internal filtering is premultiplied. RGB is ignored only when expected alpha is zero.",
            "oracle_provenance": "Authored source/coverage geometry and constant RGBA invariants. Frozen nonzero default-off step sentinels at native sigma2 use five width3, byte-rounded averaging passes: opaque edge [154,201,233,249,254,255], alpha128 edge [77,101,117,125,127,128]. This is a native legacy compatibility sentinel, not an ideal-Gaussian or AE-equivalence oracle. No production renderer was imported or executed to obtain expected pixels.",
            "cases": CASES, "files": files,
            "limits": ["Bounded metadata, coverage, ordering, output extent and failure-route checks; vendor tests own broad kernel accuracy.", "No media, original expression/program payload, original-file modification or fake missing-MV content.", "Half-scale is a native smaller composition plus scaled source transform; separate probe resize API coverage remains the lead's responsibility."]}
    (HERE / "run-plan.json").write_text(json.dumps(plan, indent=2) + "\n")
    print(f"Prepared {len(files)} LEPs, {len(CASES)} probe cases; no renders.")


if __name__ == "__main__":
    main()
