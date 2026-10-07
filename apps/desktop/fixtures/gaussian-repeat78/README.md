# Gaussian Repeat Edge Pixels fixtures

These small projects and literal pixel expectations are independently authored,
synthetic native schema78 data. They contain no external media, fonts or original
reference-project content. Open a project and switch to each named composition to
inspect the mode, its omitted/explicit default, radius0, bypass and error cases.
The final large-canvas case is a native sigma70 stress diagnostic, not AE calibration.

`generate.py` regenerates the LEP envelopes and JSON sidecars with Python's standard
library; it neither imports production code nor renders pixels. `run-plan.json`
records the selected composition, frame, expected geometry/alpha and explicit
error cases. A GPUI-free production-renderer harness uses the listed four-argument
probe contract and records `result.json` beside each `actual.png`. `check_renders.py`
requires Pillow, reads completed output only, and never runs the renderer. Use
`--include-optional` to include the large-canvas diagnostic.

The shadow half-scale case samples a smaller composition with transformed sources;
it does not claim coverage of an export-resize API. Radius has the native existing
sigma semantics. Repeat output keeps the renderer's finite filter extent; coverage
and matte rectangles do not expand. These are native correctness checks, not an
independent raster engine or an AE pixel reference.
