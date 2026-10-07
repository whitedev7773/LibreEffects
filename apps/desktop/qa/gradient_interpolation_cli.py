#!/usr/bin/env python3
"""Exact CLI acceptance for generated E04 interpolation fixtures.

Requires Pillow in the selected Python environment. Inputs and existing outputs
are never replaced. This is render/codec evidence, not native UI evidence.

python gradient_interpolation_cli.py --fixtures /absolute/generated \
  --output /absolute/new-cli-directory --binary /absolute/frozen-release \
  --legacy-binary /absolute/prior-frozen-release
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
    parser.add_argument("--legacy-binary", type=Path)
    args = parser.parse_args()
    fixtures = args.fixtures.resolve(strict=True)
    binary = args.binary.resolve(strict=True)
    legacy = args.legacy_binary.resolve(strict=True) if args.legacy_binary else None
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    manifest_path = fixtures / "cases.json"
    manifest = json.loads(manifest_path.read_text())
    width, height = manifest["dimensions"]
    records: list[dict] = []
    pairs: list[dict] = []
    cache: dict[tuple[Path, Path, int], bytes] = {}
    binaries = {b: sha256(b.read_bytes()) for b in (binary, legacy) if b}

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
        if legacy:
            legacy_sources = sorted(fixtures.glob("legacy-*.lfe.json"))
            legacy_sources += sorted(fixtures.glob("*-hold.lfe.json"))
            assert len(legacy_sources) == 8, "Four legacy static and four schema54 Hold fixtures"
            for source in legacy_sources:
                for frame in manifest["frames"]:
                    compare(source.name, frame, "previous/current legacy",
                            render(binary, source, frame), render(legacy, source, frame))
    except BaseException as exc:
        error = f"{type(exc).__name__}: {exc}"
        raise
    finally:
        report = {"fixture_manifest": str(manifest_path),
                  "fixture_manifest_sha256": sha256(manifest_path.read_bytes()),
                  "render_count": len(records), "exact_rgba_pairs": len(pairs),
                  "compared_pixels": sum(pair["pixels"] for pair in pairs),
                  "error": error, "renders": records, "pairs": pairs}
        (output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({k: v for k, v in report.items() if k not in ("renders", "pairs")}))


if __name__ == "__main__":
    main()
