# Format selected characters

While editing point text, select a nonempty range in the preview and open
**Character** in the sidebar. The selection controls change only those characters:

- Font family and exact installed font face
- Font size
- Tracking in 1/1000 em, from -1000 to 10000
- Incoming-line leading: 0.1–20480 pixels, `Auto 0.1–10`, or `Inherit`
- Fill color and fill on/off
- Stroke color, stroke on/off, width from 0 to 1000 pixels, and Miter/Round/Bevel joins

A field shows **Mixed** when the selected characters differ. Changing one field
preserves their other attributes. Choosing a family keeps each run's weight and
slant; choosing an exact face sets that face's family, weight and slant together.

The preview updates from the draft. **Undo draft** and **Redo draft** include both
text and formatting. **Done** accepts the complete text draft as one document
Undo step. **Cancel text** discards it. Clicking back into the preview continues
editing the same draft. Enter accepts a numeric or color field; Escape in that field
cancels its pending value. Finish or cancel native input composition before
changing a character setting.

Formatting requires a nonempty selection with whole grapheme boundaries. Emoji,
combining sequences and CRLF line endings cannot be split. Source text and its
line endings are preserved. A same-value setting does not create rich style data
or consume Undo history.

The current range controls support static point text. Paragraph boxes, animated
Source Text, text-parameter storage and text animators are explicitly rejected;
these are not flattened. Spatial composition text geometry retains its existing
scripting-only restriction. Layer transform animation alone does not prevent
static character formatting.

Install the desired fonts through the operating system before launching Libre
Effects. Exact faces are saved by their PostScript identity. Missing fonts or
faces are reported; the original identities remain in the project. Font Manager
includes fonts used only by character runs. Its detailed glyph-coverage checker
also inspects mixed-font point-text runs using the renderer's whole-line geometry.
Requested and fallback faces are compared per positioned glyph. Disabled paint
does not suppress inspection; unavailable faces and rejected saved positioning
remain incomplete. The existing per-layer source, line and glyph limits apply.

For the supplied reference typography, Japanese uses Noto Sans CJK KR Light
(PostScript `NotoSansCJKkr-Light`), Korean uses A2Z 7 Bold (`-7Bold`), and other text
uses the chosen Paperlogy face. Paperlogy Regular, Medium and SemiBold are distinct
faces; choose the intended weight explicitly. Fonts and reference media are not
included in the source package.

This is native character formatting. It does not establish After Effects
TextDocument replacement behavior or full reference-project visual parity.
