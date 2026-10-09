# After Effects UI/UX alignment

Compared with the running After Effects 2026 project on 2026-10-08. The AEP on
disk is unchanged. Libre Effects retains its GPUI layout and native LEP model.

| Area | Previously in Libre Effects | Updated behavior |
| --- | --- | --- |
| Workspaces | A static Default label and reset button | Standard, Small Screen, Effects and Text buttons apply real panel proportions and visibility; also available through Window and command search |
| Composition navigation | Every project composition appeared in the viewer | Only visited compositions appear; viewer and timeline share tabs; closing a tab retains the composition in Project; opening another document resets visited tabs |
| Render Queue | Separate timeline replacement with a return button | A Render Queue tab shares the timeline dock with visited compositions |
| Timeline columns | Width determined which columns were shown | F4 or the footer button switches between layer switches and Modes; at 620 px or wider, Modes includes Track Matte |
| Timeline density | 23 px layer headers and 25 px property rows | 21 px layer headers, 22 px property rows and 20 px numeric inputs; new documents start with properties collapsed |
| Project list | 29 px rows, trailing folder chevrons, shifting type labels | 22 px rows, leading folder disclosure and fixed type/action columns |
| Preview | Transport in Preview, audio/loop/speed in Audio | Five transport controls, audio, scrub, work-area looping, speed, resolution and RAM cache in Preview; Audio shows the output status and actual peak/RMS meters |
| Panel treatment | Blue underlines on all panel titles | Neutral title underlines, compact disclosure headers and a blue focus border on Project and Timeline |

Standard initially opens Preview. The other presets open the relevant existing
Effects or Character/Paragraph controls. Applying a preset preserves snapping,
alignment preference, source content, selection, time and document history.
Existing saved VIEW layouts continue to load; use Standard or Reset workspace
to replace one. Presets use the existing optional VIEW schema without changing
the LEP format. Visited tabs and the F4 column choice are transient UI state.

Keyboard input remains owned by its field or dialog. F4 requires Timeline focus;
the footer button returns focus there. Preview setting buttons consume their own
Space/Enter activation so changing a setting cannot also start playback.
Playback still waits for each completed frame, and CUDA/NVDEC and CPU fallback
paths are unchanged by the UI work.

## Additional editing improvements (v5)

All nine findings from the follow-up comparison are implemented:

| Area | Updated behavior |
| --- | --- |
| Property disclosure | Multiple layers and their Transform, Contents, mask, effect, text, audio and time-remap groups expand independently; filtering reveals matching properties |
| Timeline header | Search and layer filters share the compact toolbar; empty marker strips take no space |
| Current time | Edit timecodes, absolute frame numbers or relative offsets such as `+20`, `-20` and `+1s`; Alt+Shift+J focuses the field; invalid/out-of-range input is rejected |
| Preview resolution | Auto caps the longest side at 1280 pixels; Full uses native dimensions, Half and Quarter divide native dimensions; the actual rendered dimensions are shown |
| Preview settings | Explicit keyboard-accessible selectors for speed, resolution, range, play-from position and RAM budget; audio, scrub, loop and cache-before-playback controls are grouped together |
| Composition navigation | Breadcrumbs follow actual parent/precomposition links; back/forward history and Alt+Left/Right navigate visited compositions |
| Project panel | Details and additional filters open on demand; selected-item actions are available through the menu/right-click; drag the Name/Type divider to resize columns |
| User workspaces | Workspaces opens named layout save/rename/restore; presets match the complete layout rather than one split fraction |
| Time ruler | Zoom changes use aligned frame, second and minute intervals instead of fractional-second labels |

Preview range can be the work area, entire composition or the current time ±2
seconds, bounded by the composition. Playback can start at the current time or
the range start. With RAM enabled, cache-before-playback waits for the complete
selected range; an insufficient budget stops warming and reports the limitation.
Choose a shorter range, lower resolution or larger budget. With RAM disabled,
ordinary frame-complete playback remains available. The target-fps label is the
composition rate multiplied by the selected speed, not a measured rendering rate.
First/last transport buttons continue to seek composition bounds.

Independent layer/group disclosure is optional desktop VIEW metadata v3. Older
VIEW v1/v2 projects remain readable, and projects without the new disclosure
state preserve their existing metadata version. Named layouts live in the local
profile's `workspaces.json` (up to 16 names); saving a layout does not edit source
content or add document undo history. Navigation history is transient.

This does not implement arbitrary floating/dockable panels, all AE workspace
types, or every AE Preview option.

References: Adobe's [workspaces and panels](https://helpx.adobe.com/in/after-effects/desktop/get-started/get-familiar-with-the-interface/workspaces-panels-viewers.html),
[preview behavior](https://helpx.adobe.com/after-effects/desktop/view-and-preview/preview-video-and-audio/previewing.html?set=--fundamentals--essential-tasks),
and [F4 keyboard binding](https://helpx.adobe.com/uk/after-effects/desktop/get-started/keyboard-shortcuts/keyboard-shortcuts-reference.html).

Validation results and native interaction receipts are recorded with the v5
delivery in `dist/ayase-compatible-ae-ui-v5/validation.json`. The companion LEP
is byte-identical to the v3 project; its absence of saved VIEW metadata lets the
updated application defaults take effect immediately.
