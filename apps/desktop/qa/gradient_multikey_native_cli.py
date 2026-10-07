#!/usr/bin/env python3
"""Supplementary CLI render comparison for independently recorded native saves.

The source/VIEW verifier must run separately; this does not assert native actions
occurred. --qa-root contains generated/cases.json, native-cases.json and native/
actual saves. Each native case declares name, reference, frame, recorded_source
and recorded_release. No source or existing output is overwritten.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from PIL import Image


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def safe_stem(value: str) -> str:
    assert value and Path(value).name == value and value not in (".", ".."), value
    return value


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qa-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--previous-binary", type=Path)
    args = parser.parse_args()
    root = args.qa_root.resolve(strict=True)
    binary = args.binary.resolve(strict=True)
    previous = args.previous_binary.resolve(strict=True) if args.previous_binary else None
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    cases_path = root / "native-cases.json"
    cases = json.loads(cases_path.read_text())
    generated = json.loads((root / "generated/cases.json").read_text())
    width, height = generated["dimensions"]
    binaries = {path: sha256(path) for path in (binary, previous) if path}
    records: list[dict] = []
    pairs: list[dict] = []

    def render(executable: Path, source: Path, case: str, frame: int, kind: str) -> bytes:
        source = source.resolve(strict=True)
        target = output / f"{case}-{frame:03}-{kind}.png"
        assert not target.exists(), target
        command = [str(executable), "--render", str(source), "--output", str(target),
                   "--size", f"{width}x{height}", "--start", str(frame),
                   "--end", str(frame + 1), "--fonts", "strict"]
        result = subprocess.run(command, text=True, capture_output=True, timeout=180)
        (output / f"{case}-{frame:03}-{kind}.log").write_text(
            json.dumps(command) + "\n" + result.stdout + result.stderr)
        assert result.returncode == 0, (command, result.stdout, result.stderr)
        with Image.open(target) as image:
            assert image.size == (width, height), (target, image.size)
            rgba = image.convert("RGBA").tobytes()
        records.append({"case": case, "frame": frame, "kind": kind,
                        "source": str(source), "source_sha256": sha256(source),
                        "binary_sha256": binaries[executable], "output": target.name,
                        "rgba_sha256": hashlib.sha256(rgba).hexdigest()})
        return rgba

    error = None
    try:
        for case in cases:
            name, reference = safe_stem(case["name"]), safe_stem(case["reference"])
            assert case["recorded_source"] and case["recorded_release"], case
            actual = root / "native" / f"{name}.lep"
            expected = root / "generated" / f"{reference}.generated.lep"
            for frame in generated["frames"]:
                left = render(binary, actual, name, frame, "actual")
                right = render(binary, expected, name, frame, "expected")
                assert left == right, f"Full RGBA mismatch: {name}, frame {frame}"
                pairs.append({"name": name, "frame": frame, "pixels": width * height,
                              "kind": "actual/reference full unmasked RGBA"})
                if previous:
                    old = render(previous, actual, name, frame, "previous-actual")
                    assert left == old, f"Previous/current mismatch: {name}, frame {frame}"
                    pairs.append({"name": name, "frame": frame, "pixels": width * height,
                                  "kind": "previous/current actual full unmasked RGBA"})
    except BaseException as exc:
        error = f"{type(exc).__name__}: {exc}"
        raise
    finally:
        report = {"cases_manifest": str(cases_path), "cases_sha256": sha256(cases_path),
                  "binaries": {str(path): digest for path, digest in binaries.items()},
                  "render_count": len(records), "exact_rgba_pairs": len(pairs),
                  "compared_pixels": sum(pair["pixels"] for pair in pairs),
                  "error": error, "renders": records, "pairs": pairs}
        (output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({key: value for key, value in report.items()
                          if key not in ("renders", "pairs")}))


if __name__ == "__main__":
    main()
