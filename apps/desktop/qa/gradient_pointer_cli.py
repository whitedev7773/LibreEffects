#!/usr/bin/env python3
"""Exact CLI acceptance for generated E04 compound pointer-drag fixtures.

The resumed comparator is the recovered compatible SVG-inline-style release
fb8b03a, not the preceding multi-key binary. Pin its verified hash in the report.
Requires Pillow in the selected Python environment. Inputs and existing outputs
are never replaced. This is render/codec evidence, not native UI evidence.

python gradient_pointer_cli.py --fixtures /absolute/generated \
  --output /absolute/new-cli-directory --binary /absolute/frozen-release \
  --previous-binary /absolute/recovered-svg-release
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
    inputs_before = {path: sha256(path.read_bytes()) for path in fixtures.iterdir() if path.is_file()}
    records: list[dict] = []
    pairs: list[dict] = []
    cache: dict[tuple[Path, Path, int], bytes] = {}
    binaries = {b: sha256(b.read_bytes()) for b in (binary, previous) if b}

    def render(executable: Path, source: Path, frame: int) -> bytes:
        source = source.resolve(strict=True)
        key = executable, source, frame
        if key in cache:
            return cache[key]
        prefix = "current" if executable == binary else "previous"
        target = output / f"{prefix}-{source.name}-{frame:03}.png"
        assert not target.exists(), target
        command = [str(executable), "--render", str(source), "--output", str(target),
                   "--size", f"{width}x{height}", "--start", str(frame),
                   "--end", str(frame + 1), "--fonts", "strict"]
        result = subprocess.run(command, text=True, capture_output=True, timeout=180)
        log = output / f"{prefix}-{source.name}-{frame:03}.log"
        log.write_text(json.dumps(command) + "\n" + result.stdout + result.stderr)
        assert result.returncode == 0, (command, result.stdout, result.stderr)
        with Image.open(target) as image:
            assert image.size == (width, height), (target, image.size)
            data = image.convert("RGBA").tobytes()
        records.append({"source": str(source), "source_sha256": sha256(source.read_bytes()),
                        "frame": frame, "output": target.name,
                        "binary_sha256": binaries[executable], "rgba_sha256": sha256(data)})
        cache[key] = data
        return data

    def compare(label: str, frame: int, kind: str, left: bytes, right: bytes) -> None:
        assert left == right, f"Full unmasked RGBA mismatch: {label}, frame {frame}, {kind}"
        pairs.append({"label": label, "frame": frame, "kind": kind,
                      "pixels": width * height, "comparison": "exact full RGBA; no masks"})

    error = None
    try:
        for case in manifest["render_cases"]:
            source = fixtures / case["input"]
            expected = fixtures / case["expected"]
            frame = case["frame"]
            actual = render(binary, source, frame)
            reference = render(binary, expected, frame)
            compare(source.name, frame, "independent legacy-static oracle", actual, reference)
            json_source = source.with_name(source.name.removesuffix(".generated.lep") + ".lfe.json")
            compare(source.name, frame, "JSON/LEP", actual, render(binary, json_source, frame))
            if previous:
                # Pointer dragging changes no schema or renderer. Every new
                # animated source and literal static reference is old-readable.
                compare(source.name, frame, "previous/current edited source",
                        actual, render(previous, source, frame))
                compare(expected.name, frame, "previous/current static oracle",
                        reference, render(previous, expected, frame))
    except BaseException as exc:
        error = f"{type(exc).__name__}: {exc}"
        raise
    finally:
        inputs_after = {path: sha256(path.read_bytes()) for path in fixtures.iterdir() if path.is_file()}
        immutable = inputs_before == inputs_after and all(sha256(path.read_bytes()) == digest for path, digest in binaries.items())
        if not immutable:
            error = error or "Input fixture or frozen binary changed during qualification"
        report = {"fixture_manifest": str(manifest_path),
                  "inputs_and_binaries_unchanged": immutable,
                  "input_sha256": {str(path): digest for path, digest in inputs_before.items()},
                  "binaries": {str(path): digest for path, digest in binaries.items()},
                  "fixture_manifest_sha256": sha256(manifest_path.read_bytes()),
                  "render_count": len(records), "exact_rgba_pairs": len(pairs),
                  "compared_pixels": sum(pair["pixels"] for pair in pairs),
                  "error": error, "renders": records, "pairs": pairs}
        (output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({k: v for k, v in report.items() if k not in ("renders", "pairs", "input_sha256")}))
        assert immutable, error


if __name__ == "__main__":
    main()
