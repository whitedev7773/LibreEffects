#!/usr/bin/env python3
"""Read completed probe outputs only. Never imports/runs production code or fixtures."""
import argparse
import json
from pathlib import Path
from PIL import Image

HERE = Path(__file__).resolve().parent


def inside(x, y, r):
    return r[0] <= x < r[0] + r[2] and r[1] <= y < r[1] + r[3]


def equal_pixel(actual, expected):
    return actual == tuple(expected) if expected[3] else actual[3] == 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", action="append", help="Check only this named case (repeatable)")
    parser.add_argument("--include-optional", action="store_true")
    parser.add_argument("--renders", type=Path, default=HERE / "renders")
    args = parser.parse_args()
    plan = json.loads((HERE / "run-plan.json").read_text())
    selected = [c for c in plan["cases"] if (c["name"] in args.case if args.case else args.include_optional or not c.get("optional"))]
    if args.case:
        assert {c["name"] for c in selected} == set(args.case), "Unknown case name"
    cache = {}

    def load(name):
        if name not in cache:
            with Image.open(args.renders / name / "actual.png") as image:
                cache[name] = image.convert("RGBA")
        return cache[name]

    report = []
    for c in selected:
        name = c["name"]
        result = json.loads((args.renders / name / "result.json").read_text())
        if "expected_error" in c:
            assert result["exit_code"] != 0, f"{name}: rejection did not reach process status"
            for phrase in c["expected_error"]["stderr_contains"]:
                assert phrase in result["stderr"], f"{name}: missing explicit failure {phrase}"
            assert not (args.renders / name / c["expected_error"]["must_not_write"]).exists(), f"{name}: failed render published an image"
            report.append({"case": name, "explicit_rejection": "pass"})
            continue
        assert result["exit_code"] == 0, f"{name}: probe failed: {result.get('stderr', '')}"
        stdout = [s for s in result["stdout"].splitlines() if s.strip()]
        receipt = json.loads(stdout[-1])
        assert receipt["preview_export_equal"] is True and receipt["source_unchanged"] is True, name
        assert [receipt["width"], receipt["height"]] == c["size"] and receipt["frame"] == c["frame"], name
        image = load(name)
        assert image.size == tuple(c["size"]), name
        width, height = image.size
        pixels = list(image.getdata())

        def pixel(x, y, expected):
            actual = pixels[y * width + x]
            assert equal_pixel(actual, expected), f"{name}: pixel {(x, y)} {actual} != {expected}"

        for a in c["assertions"]:
            kind = a["type"]
            if kind == "literal_rectangles":
                for y in range(height):
                    for x in range(width):
                        expected = [0, 0, 0, 0]
                        for r in a["rectangles"]:
                            if inside(x, y, r):
                                expected = r[4]
                        pixel(x, y, expected)
            elif kind == "alpha_zero_outside":
                for y in range(height):
                    for x in range(width):
                        if not any(inside(x, y, r) for r in a["rectangles"]):
                            pixel(x, y, [0, 0, 0, 0])
            elif kind == "literal_pixels":
                for x, y, rgba in a["pixels"]:
                    pixel(x, y, rgba)
            elif kind == "equal_image":
                assert image.size == load(a["other"]).size and image.tobytes() == load(a["other"]).tobytes(), f"{name}: paired image mismatch"
            elif kind == "translated_image":
                other = load(a["other"])
                dx, dy = a["offset"]
                for y in range(height):
                    for x in range(width):
                        rgba = other.getpixel((x-dx, y-dy)) if 0 <= x-dx < other.width and 0 <= y-dy < other.height else (0, 0, 0, 0)
                        pixel(x, y, rgba)
            elif kind == "adjustment_literal":
                for y in range(height):
                    for x in range(width):
                        expected = [255, 0, 0, 255] if x < 32 else [0, 0, 255, 255]
                        if inside(x, y, a["effective_coverage"]) and x < 37:
                            expected = a["transition_rgba"][x-32]
                        pixel(x, y, expected)
            else:
                raise AssertionError(f"Unknown assertion {kind}")
        report.append({"case": name, "output_assertions": len(c["assertions"]),
                       "status": "pass", "preview_export_equal": True, "source_unchanged": True})
    print(json.dumps({"status": "pass", "cases": report,
                      "checked_output_only": True,
                      "optional_diagnostic_included": any(c.get("optional") for c in selected)}, indent=2))


if __name__ == "__main__":
    main()
