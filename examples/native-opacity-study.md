# Native Opacity timing studies

Open `native-opacity-low.lep` or `native-opacity-high.lep` in LibreEffects. Both
are independently authored 96×64, 30fps native projects with a red plate over an
opaque blue canvas. No AEP or external media is needed.

The plate has equal 50% keys at frames0 and30. At frame15, signed native Bezier
speeds produce raw−100% in the low study and raw200% in the high study. Painting
clips those to0% and100%; source timing and values remain intact. Inspector and
Timeline show the raw native value and read-only timing/key count.

At frame15, open Layer Fill Color. Accepting unchanged alpha, retyping displayed
0/100, or changing only RGB preserves the whole raw Opacity source. A genuinely
changed alpha inserts or edits a native value key through its timing owner. Use
Undo/Redo and save a copy to retain the bundled baseline. Native Graph operations
that cannot represent the independent timing sides are explicitly unavailable.

The adjacent implementation contract is
`apps/desktop/NATIVE_OPACITY_TIMING.md`. These native studies establish the
implemented formula and source-preservation behavior; they do not claim After
Effects numerical defaults or full supplied-script compatibility.
