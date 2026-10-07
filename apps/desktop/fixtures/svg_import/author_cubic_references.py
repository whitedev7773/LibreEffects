#!/usr/bin/env python3
"""Author independent cubic-equivalent fixture references from literal plans.

No SVG path parser, importer, native Contents model or renderer is used here.
Each endpoint/control point below was transcribed from its authored source SVG.
Lines use the exact affine parameterization; quadratics use degree elevation.
The original source fixtures stay byte-identical. Circles, ellipses and rounded
rectangles retain their literal primitive definitions and pinned SVG semantics.
This produces separate test references, never source-imported projects.
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def path(plan):
    output = []
    current = start = None
    for segment in plan:
        verb, *values = segment
        if verb == "M":
            current = start = tuple(values)
            output.append(f"M{values[0]} {values[1]}")
        elif verb in ("L", "Z"):
            target = start if verb == "Z" else tuple(values)
            a = tuple(current[i] + (target[i] - current[i]) / 3 for i in (0, 1))
            b = tuple(target[i] - (target[i] - current[i]) / 3 for i in (0, 1))
            output.append("C" + " ".join(map(str, (*a, *b, *target))))
            current = target
            if verb == "Z":
                output.append("Z")
        elif verb == "Q":
            control, target = tuple(values[:2]), tuple(values[2:])
            a = tuple(current[i] + (control[i] - current[i]) * 2 / 3 for i in (0, 1))
            b = tuple(target[i] + (control[i] - target[i]) * 2 / 3 for i in (0, 1))
            output.append("C" + " ".join(map(str, (*a, *b, *target))))
            current = target
        elif verb == "C":
            output.append("C" + " ".join(map(str, values)))
            current = tuple(values[-2:])
        else:
            raise AssertionError(verb)
    return " ".join(output)


def poly(points, closed=True):
    return [("M", *points[0]), *(("L", *p) for p in points[1:]), *(("Z",) for _ in range(closed))]


def replace(source, old, new):
    assert source.count(old) == 1, old
    return source.replace(old, new)


plans = {
    "primitives": [
        ('<line x1="12" y1="74" x2="64" y2="100"', '<path d="' + path(poly([(12, 74), (64, 100)], False)) + '"'),
        ('<polyline points="84,106 95,69 113,105 133,70"', '<path d="' + path(poly([(84, 106), (95, 69), (113, 105), (133, 70)], False)) + '"'),
        ('<polygon points="154,65 185,83 174,115 143,96"', '<path d="' + path(poly([(154, 65), (185, 83), (174, 115), (143, 96)])) + '"'),
        ('M199 70 h27 v40 h-27 z', path(poly([(199, 70), (226, 70), (226, 110), (199, 110)]))),
    ],
    "curves": [
        ('M10 49 C24 7 43 7 57 49 S89 90 102 49 Q120 12 135 49 T171 49 L175 84 H11 Z', path([
            ("M", 10, 49), ("C", 24, 7, 43, 7, 57, 49), ("C", 71, 91, 89, 90, 102, 49),
            ("Q", 120, 12, 135, 49), ("Q", 150, 86, 171, 49), ("L", 175, 84), ("L", 11, 84), ("Z",)])),
        ('m15 121 c18 -36 33 -36 49 0 s33 36 49 0 q18 -25 34 0 t34 0 h38 v23 h-204 z', path([
            ("M", 15, 121), ("C", 33, 85, 48, 85, 64, 121), ("C", 80, 157, 97, 157, 113, 121),
            ("Q", 131, 96, 147, 121), ("Q", 163, 146, 181, 121), ("L", 219, 121), ("L", 219, 144), ("L", 15, 144), ("Z",)])),
    ],
    "compound-winding": [
        ('M8 12 H70 V74 H8 Z M23 27 H55 V59 H23 Z', path(poly([(8,12),(70,12),(70,74),(8,74)]) + poly([(23,27),(55,27),(55,59),(23,59)]))),
        ('M88 12 H150 V74 H88 Z M103 27 H135 V59 H103 Z', path(poly([(88,12),(150,12),(150,74),(88,74)]) + poly([(103,27),(135,27),(135,59),(103,59)]))),
        ('M168 12 H230 V74 H168 Z M183 27 V59 H215 V27 Z', path(poly([(168,12),(230,12),(230,74),(168,74)]) + poly([(183,27),(183,59),(215,59),(215,27)]))),
        ('M10 96 H69 V148 H10 Z M22 108 H57 V136 H22 Z M89 96 H149 V148 H89 Z M101 108 V136 H137 V108 Z', path(poly([(10,96),(69,96),(69,148),(10,148)]) + poly([(22,108),(57,108),(57,136),(22,136)]) + poly([(89,96),(149,96),(149,148),(89,148)]) + poly([(101,108),(101,136),(137,136),(137,108)]))),
        ('M171 143 L190 95 L212 143 M179 132 L207 132', path(poly([(171,143),(190,95),(212,143)],False) + poly([(179,132),(207,132)],False))),
    ],
    "nested-affine": [
        ('M8 8 L34 13 L15 37 Z', path(poly([(8,8),(34,13),(15,37)]))),
        ('M8 9 L45 21 L30 61 L5 43 Z', path(poly([(8,9),(45,21),(30,61),(5,43)]))),
        ('M0 0 L96 0 L96 30 L0 30 Z', path(poly([(0,0),(96,0),(96,30),(0,30)]))),
    ],
    "inherited-paint": [
        ('M15 20 L80 20 L80 75 L15 75 Z', path(poly([(15,20),(80,20),(80,75),(15,75)]))),
        ('M52 46 L115 46 L115 106 L52 106 Z', path(poly([(52,46),(115,46),(115,106),(52,106)]))),
        ('M97 79 L146 79 L146 140 L97 140 Z', path(poly([(97,79),(146,79),(146,140),(97,140)]))),
        ('M172 16 L215 65 L169 69 Z', path(poly([(172,16),(215,65),(169,69)]))),
        ('M170 102 L219 139', path(poly([(170,102),(219,139)],False))),
    ],
    "viewbox-none": [('M7 92 H87', path(poly([(7,92),(87,92)],False)))],
    "viewport-clipping": [('M42 -12 L89 90', path(poly([(42,-12),(89,90)],False)))],
    "viewbox-slice": [('M12 25 L87 74', path(poly([(12,25),(87,74)],False)))],
    "stroke-styles": [
        ('M16 17 L43 51 L70 17', path(poly([(16,17),(43,51),(70,17)],False))),
        ('M95 17 L122 51 L149 17', path(poly([(95,17),(122,51),(149,17)],False))),
        ('M172 17 L199 51 L226 17', path(poly([(172,17),(199,51),(226,17)],False))),
        ('M0 0 H120 V50 H0 Z', path(poly([(0,0),(120,0),(120,50),(0,50)]))),
    ],
}
# Exact author-selected viewBox mappings. These avoid the SVG normalizer's
# separate f32 viewport arithmetic while preserving mathematical placement.
viewboxes = {
    "viewbox-meet": (' viewBox="10 20 100 100"', '1.6 0 0 1.6 24 -32'),
    "viewbox-none": (' viewBox="-20 -10 120 120" preserveAspectRatio="none"', '2 0 0 1.3333333333333333 40 13.333333333333334'),
    "viewport-clipping": (' viewBox="0 0 120 80"', '1.25 0 0 1.25 5 0'),
    "viewbox-slice": (' viewBox="0 0 100 100" preserveAspectRatio="xMidYMid slice"', '2.4 0 0 2.4 0 -40'),
}

def main():
    output = ROOT / "cubic-references"
    output.mkdir(exist_ok=True)
    for original in sorted(ROOT.glob("*.svg")):
        if original.stem == "fractional-viewport":
            continue  # Explicitly unsupported, retained as a rejection fixture.
        source = original.read_text()
        for old, new in plans.get(original.stem, []):
            source = replace(source, old, new)
        if original.stem in viewboxes:
            attrs, matrix = viewboxes[original.stem]
            source = replace(source, attrs, "")
            head, body = source.split(">", 1)
            source = head + ">\n<g transform='matrix(" + matrix + ")'>" + body.replace("</svg>", "</g></svg>")
        target = output / original.name
        if target.exists():
            assert target.read_text() == source, f"immutable reference differs: {target}"
        else:
            target.write_text(source)


if __name__ == "__main__":
    main()
