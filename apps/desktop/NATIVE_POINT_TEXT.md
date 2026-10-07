# Point-origin text and explicit leading

This reference-recreation prerequisite uses schema 74 for new point-origin or
per-character leading fields. Existing rich-text fields remain schema 71 and
serialize exactly as before when the new fields are absent.

`RichText.point_origin` places alignment around local X=0 and the first baseline
at local Y=0, preserving authored layer anchors and parent coordinate systems.
`TextCharacterStyle.leading` optionally selects a fixed pixel distance or an
automatic font-size ratio. Later baselines use the maximum incoming-line leading;
blank lines retain their owning insertion/terminator style. One shared pure
line-metrics helper drives rendering, caret/selection layout and hit geometry.
Without either new feature, the established native progression remains exact.

Validation at this intermediate checkpoint:
- 51 focused model tests passed: nine new point-layout tests plus existing rich,
  selected-character and typed-text conversion regressions.
- Independent source-manifest verification matched 137 available cached reference
  baselines with maximum error 0.00001 pixels. This verifies recovered layout
  semantics, not AE pixel equivalence or the generated LEP.
- The combined desktop all-target check passed in 1m 47s, including the generic
  private constructor and the two new renderer/layout regression sources. No
  monolithic desktop test executable was run. Native/render qualification was pending
  at this source-only checkpoint; the bounded actual-file result follows below.
- This checkpoint does not resolve expression deadline failures, ligature/locale
  parity, joined 2D motion, or every private project-recreation gap.

Private original project, expression text, fonts and media remain outside Git.
The generic constructor reads reviewed data at runtime and contains no original
project text or programs. Its first actual-source Lyric output passed supervised
expression execution at two times, deterministic native roundtrip and independent
whole-source audit. The generated project and portable package remain private;
they are partial reconstructions requiring this schema-74 build. The bounded native
visible-frame gate below now passes. Expression deadline reliability remains open.


## Actual-file native qualification

The canonical release of a6ad52a9 was built once and independently pinned to
751 source inputs, fingerprint 442f2a01db0db3d0. The corrected private package was
freshly extracted with exact archive/member hashes and process-local packaged
fonts. Its actual 72-layer/192-binding/186-marker Lyric composition opened at
frame 4360, rendered Japanese/Korean caption groups, and navigated to 13501 without
an observed error. Project Media found the one original Audio.mp3 source with
zero missing links. No original JSX or AEP application import was run.

One strict-font final CLI export of frame 4360 succeeded in 0.170s, with empty
stderr and opaque 1920×886 output on the authored black background. It shows the
real reconstructed source, not a synthetic sample. No AE rendered oracle exists;
visible layout and native source preservation are the qualified outcomes.

First native Save materializes VIEW and serializes PROJ object keys in a different
order from the GPUI-free constructor. The original strict byte-check rejection
is retained. Independent token-level comparison confirms all 43,846 raw tokens and
array order are unchanged; sorting 1,742 reordered objects exactly recreates the
original PROJ bytes. Complete authored values, numeric bits, strings, fields,
media paths and original media hashes remain exact. A separately explicit
re-encoding mode records the raw inequality without silently relaxing source
comparison. Desktop serde_json/preserve_order feature unification explains the
representation difference. Later native Save/Open/Save is wholly byte-identical.

The saved app closed normally at 14:49:59 UTC. No further build, retry, model replay
or broad UI matrix ran. Known partial-reconstruction gaps, unmeasured native audio
mixing, text-composer/AE pixel parity and existing expression deadline reliability
remain unresolved. Private original files and font/media bytes remain outside Git.
