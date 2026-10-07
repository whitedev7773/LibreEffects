# Native opacity73 qualification probe

`probe.rs` is a standalone binary entry, deliberately **not** `main.rs` and not
a Cargo integration test. It imports the production Renderer, adjustment
compositor, graph-channel admission, and production automation process module
without GPUI. `main` dispatches the real expression worker before parsing probe
arguments. No substitute expression engine is used. The only rejection stubs
are external video decoding and media import, neither of which any fixture uses.

Build this entry with the workspace's pinned Rust toolchain and the existing
fresh debug dependency artifacts, after the core/model source gate has rebuilt
the schema73 libraries. Use a single compiler, incremental off, and no `--test`.
Do not build a monolithic desktop test executable for this probe. Its direct
external crates are `libre_effects_core`, `libre_effects_editor_model`,
`libre_effects_ae_expressions`, `image`, `resvg`, `base64`, `serde`, `serde_json`,
`libc`, `unicode_segmentation`, `rustybuzz`, `unicode_bidi`, `unicode_script`, and
`unicode_linebreak`; resolve all of them from one matching Cargo build receipt.

## Run

```sh
opacity-probe suite /path/to/qualification
opacity-probe generate /path/to/fixtures
opacity-probe render INPUT.lep FRAME REFERENCE.svg /path/to/pixels
opacity-probe reject INPUT.lep FRAME 'expected error fragment'
```

The suite writes independently authored native `.lep` fixtures, standalone
literal SVG references, a `render-cases.tsv` manifest, and actual/reference PNGs
for every render case. These files can also be consumed by the final desktop
binary's CLI and native UI qualification. Helper execution is not final-binary
or native-UI evidence.

## Independent oracles

All scenes are 96 × 64. An opaque blue canvas and a centered 40 × 24 red plate
make a zero-clamped result visible rather than an uninformative blank image.
No production sampling, geometry, or rendering creates an expected SVG.

- Equal endpoint keys `50,50`, one second apart, have incoming/outgoing Bezier
  influence `100/3` percent. Signed speeds `s,-s` give the independently reduced
  polynomial `50 + s*t*(1-t)`. At the midpoint, `s=-600` yields `-100`, `s=600`
  yields `200`, and `s=100` yields `75`. Raw helpers and expressions retain those
  values; painting alone clamps to 0–100.
- Equal zero endpoints with speeds `1e-300,-1e-300` have midpoint `2.5e-301`.
  Relative comparisons preserve a meaningful tiny-value oracle. Dormant first
  incoming and final outgoing speeds, including signed zero, survive native
  roundtrip with their exact authored sign and value.
- The mixed fixture has keys `0:20,30:80,60:40,90:20`. The first segment is Hold,
  then Linear/Bezier and Bezier/Linear. Independent midpoint expectations are
  `20`, `62.5`, and `47.5`, respectively. Both endpoint sides remain authored.
- Identity and arithmetic expressions run in actual supervised child workers.
  Authored samples `-100` and `200` enter the worker unchanged; identity retains
  them and arithmetic brings both to `50`. The detached render view cannot be
  saved; original native bytes must remain unchanged.
- A full-canvas zero-brightness adjustment must preserve the lower image for
  negative opacity and produce opaque black above 100. Its 75% midpoint leaves
  color channel 64, independently derived from 8-bit premultiplied blending.
- Nested 60fps source compositions sit in a 30fps parent. Parent frame 15 maps
  to source frame 30, and the source's one-second curve gives opacity 75. Using
  the parent frame duration would yield 100, so these images detect an FPS leak
  in normal, expression, and adjustment sampling. Separate nested low/high
  curves check paint clamps.

Each of 22 RGBA pairs requires exact equality with its literal SVG at full
resolution, agreement between `render`, `render_preview`, and `render_output`,
and unchanged authored native bytes. The helper also checks graph exclusion for
annotated transform Opacity while explicitly authored text Fill Opacity and
Stroke Opacity remain available (untouched sparse defaults remain unavailable);
invalid rates, unsupported auto/continuous timing, invalid influence,
ambiguous animated static writes, malformed annotation coverage, and unmatched
incoming Hold fail explicitly. Unmatched incoming Hold remains structurally
storable for native repair but fails interior sampling, animation validation,
automation host finish, and automation commit without changing source bytes.
Three native negative fixtures require explicit failure from all three render
entry points, with no authored fallback; `rejection-cases.tsv` lists their inputs.

No AEP, supplied JSX, template project, or externally authored expression is
copied or executed. This is bounded native opacity qualification, not a claim
that another application's defaults or a complete external workflow are emulated.
