"""Regenerate deterministic OFL fixtures with FontTools (not a build dependency).

Run from this directory: python generate.py
Generated with FontTools 4.61.1 from the bundled WantedSans-Regular.ttf.
"""
from pathlib import Path
from fontTools import subset
from fontTools.ttLib import TTFont

root = Path(__file__).resolve().parent
font = TTFont(root.parent / 'WantedSans-Regular.ttf', recalcTimestamp=False)
options = subset.Options()
options.name_IDs = ['*']
options.name_legacy = True
options.name_languages = ['*']
options.recalc_timestamp = False
subsetter = subset.Subsetter(options=options)
subsetter.populate(unicodes=range(0x20, 0x7F))
subsetter.subset(font)
# Rename all family/style/PostScript/unique/full-name identities on every platform.
# Keep the original copyright, attribution and OFL records.
identities = {
    1: 'LibreEffects Coverage Fixture', 2: 'Regular',
    3: 'LibreEffects-Coverage-Fixture-Regular-1',
    4: 'LibreEffects Coverage Fixture Regular',
    6: 'LibreEffectsCoverageFixture-Regular',
    16: 'LibreEffects Coverage Fixture', 17: 'Regular',
}
for record in font['name'].names:
    if record.nameID in identities:
        record.string = identities[record.nameID].encode(record.getEncoding())
font.save(root / 'CoverageFixture-Regular.ttf')
