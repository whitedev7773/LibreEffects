# Font coverage test fixture

`CoverageFixture-Regular.ttf` is a printable-ASCII subset of the bundled
Wanted Sans Regular, renamed **LibreEffects Coverage Fixture**. It retains
Wanted Sans shaping/layout tables, outlines and metrics for this bounded subset.
It is test-only and is never loaded into the application's font catalog.

Copyright 2024 The Wanted Sans Project Authors
(https://github.com/wanteddev/wanted-sans).
Licensed under SIL Open Font License 1.1; see `OFL.txt` in this directory.
The original source and upstream details are in `../README.md`.

Regenerate with `python generate.py` from this directory. The checked-in fixture
was generated with FontTools 4.61.1. FontTools is only a fixture-generation tool,
not a runtime or build dependency. The generator preserves attribution and
license name records and renames family/full/PostScript/unique identities on
all original platforms and languages. Fallback tests pair this ASCII-only
fixture with the unmodified bundled Wanted Sans Regular in an isolated fontdb.
