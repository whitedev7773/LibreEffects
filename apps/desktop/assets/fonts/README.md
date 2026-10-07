# Bundled Wanted Sans

The seven static TTF files are Wanted Sans 1.003, from
[wanteddev/wanted-sans](https://github.com/wanteddev/wanted-sans/tree/02c9b822349c188ada95f9e2d90c2ed18f853235/packages/wanted-sans/fonts/ttf).
They are redistributed without modification under the adjacent `OFL.txt`.
Regular remains the application UI and default composition font.

Medium, SemiBold, Bold, ExtraBold, Black and ExtraBlack are available for text
layers. Black and ExtraBlack both declare OS/2 weight 900, so the editor also
stores the font's PostScript face name. The renderer assigns an internal family
alias to each discovered face; this does not modify the font files.

Other installed fonts are referenced by family and face name, not embedded or
copied into saved projects. Install the same font versions on another machine
to reproduce those glyphs. Missing families fall back to bundled Wanted Sans.
