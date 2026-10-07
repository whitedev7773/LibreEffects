#!/usr/bin/env python3
"""Exact release-CLI QA against immutable independent cubic-reference PNGs.

The Rust exporter creates cases.json, source SVG, independently authored
cubic-reference and raw-source PNGs, and imported JSON/LEP. This script never
parses SVG or regenerates references. Its output is render/codec evidence, not native interaction evidence.

Use the pinned QA Python with Pillow. All inputs and existing output directories
remain untouched. --previous-binary optionally qualifies old renderer/schema
compatibility using the same current generated project files.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from PIL import Image


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--previous-binary", type=Path)
    args = parser.parse_args()
    fixtures = args.fixtures.resolve(strict=True)
    binary = args.binary.resolve(strict=True)
    previous = args.previous_binary.resolve(strict=True) if args.previous_binary else None
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    manifest_path = fixtures / "cases.json"
    manifest = json.loads(manifest_path.read_text())
    width, height = manifest["dimensions"]
    input_hashes = {str(path): sha256(path.read_bytes())
                    for path in fixtures.iterdir() if path.is_file()}
    binaries = {path: sha256(path.read_bytes()) for path in (binary, previous) if path}
    renders, pairs, raw_source_deltas = [], [], []
    checked_raw = set()
    cache: dict[tuple[Path, Path, int], bytes] = {}

    def rgba(path: Path) -> bytes:
        with Image.open(path) as image:
            assert image.size == (width, height), (path, image.size)
            return image.convert("RGBA").tobytes()

    def render(executable: Path, source: Path, frame: int) -> bytes:
        source = source.resolve(strict=True)
        key = executable, source, frame
        if key in cache:
            return cache[key]
        prefix = "current" if executable == binary else "previous"
        stem = f"{prefix}-{source.name}-{frame:03}"
        target = output / f"{stem}.png"
        command = [str(executable), "--render", str(source), "--output", str(target),
                   "--size", f"{width}x{height}", "--start", str(frame),
                   "--end", str(frame + 1), "--fonts", "strict"]
        result = subprocess.run(command, text=True, capture_output=True, timeout=180)
        (output / f"{stem}.log").write_text(json.dumps(command) + "\n" + result.stdout + result.stderr)
        assert result.returncode == 0, (command, result.stdout, result.stderr)
        data = rgba(target)
        renders.append({"binary_sha256": binaries[executable], "source": str(source),
                        "source_sha256": input_hashes[str(source)], "frame": frame,
                        "output": target.name, "rgba_sha256": sha256(data)})
        cache[key] = data
        return data

    def compare(label: str, frame: int, kind: str, actual: bytes, expected: bytes) -> None:
        if actual != expected:
            changed = sum(actual[i:i + 4] != expected[i:i + 4]
                          for i in range(0, len(actual), 4))
            maximum = max(abs(a - b) for a, b in zip(actual, expected))
            raise AssertionError(f"{label} frame {frame} {kind}: {changed} changed pixels, "
                                 f"max channel difference {maximum}")
        pairs.append({"label": label, "frame": frame, "kind": kind,
                      "pixels": width * height, "comparison": "exact full unmasked RGBA"})

    error = None
    try:
        for case in manifest["render_cases"]:
            label, frame = case["label"], case["frame"]
            reference = rgba(fixtures / case["expected_png"])
            if label not in checked_raw:
                raw = rgba(fixtures / case["raw_source_expected_png"])
                changed = sum(raw[i:i+4] != reference[i:i+4] for i in range(0, len(raw), 4))
                maximum = max(abs(a-b) for a,b in zip(raw,reference))
                assert (changed,maximum) == (case["raw_source_changed_pixels"],case["raw_source_max_channel_delta"]), label
                for i in range(0,len(raw),4):
                    if raw[i:i+4] == reference[i:i+4]:
                        continue
                    x,y=(i//4)%width,(i//4)//width
                    edge = any(raw[4*(ny*width+nx):4*(ny*width+nx)+4] != raw[i:i+4]
                               for ny in range(max(0,y-1),min(height,y+2))
                               for nx in range(max(0,x-1),min(width,x+2)))
                    assert edge, (label,x,y,"normalization changed uniform interior")
                raw_source_deltas.append({"label":label,"changed_pixels":changed,"max_channel_delta":maximum,
                                          "comparison":"entire unmasked RGBA; exact declared edge-only difference count and max"})
                checked_raw.add(label)
            lep = fixtures / case["input"]
            json_source = fixtures / case["json"]
            native_pixels = render(binary, lep, frame)
            json_pixels = render(binary, json_source, frame)
            compare(label, frame, "LEP versus independent cubic-reference", native_pixels, reference)
            compare(label, frame, "JSON versus independent cubic-reference", json_pixels, reference)
            compare(label, frame, "JSON/LEP", json_pixels, native_pixels)
            if previous:
                compare(label, frame, "previous/current LEP", render(previous, lep, frame), native_pixels)
                compare(label, frame, "previous/current JSON", render(previous, json_source, frame), json_pixels)
        assert input_hashes == {str(path): sha256(path.read_bytes())
                                for path in fixtures.iterdir() if path.is_file()}, "fixtures changed"
        assert binaries == {path: sha256(path.read_bytes()) for path in binaries}, "binary changed"
    except BaseException as exc:
        error = f"{type(exc).__name__}: {exc}"
        raise
    finally:
        report = {"manifest": str(manifest_path), "manifest_sha256": input_hashes[str(manifest_path)],
                  "render_count": len(renders), "exact_rgba_pairs": len(pairs),
                  "compared_pixels": sum(pair["pixels"] for pair in pairs),
                  "renders": renders, "pairs": pairs, "raw_source_deltas": raw_source_deltas, "error": error,
                  "qualification": "generated-source release rendering only; no native interaction claim"}
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"{len(renders)} renders, {len(pairs)} exact RGBA pairs, "
          f"{sum(pair['pixels'] for pair in pairs)} pixels")


if __name__ == "__main__":
    main()
