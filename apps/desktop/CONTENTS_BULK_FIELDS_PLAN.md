# E04 proposal: shared numeric fields for sibling selections

Prepared 2026-10-04 from a read-only source review. **Proposal only: not implemented or tested.** The design review changed no application source and ran no tests, native actions or network operations. The cross-parent drag implementation is now committed as `75f5055`; its verification is recorded in [HANDOFF.md](HANDOFF.md) and [STATUS.md](STATUS.md). This proposal itself adds no implementation.

## Recommendation

Ship one bounded numeric-editing slice before compound Gradient Colors: select two or more Contents siblings, show their common scalar properties, and assign one explicit absolute value to all selected items at the displayed frame in one transaction. Each field submission is one Undo. Multiple field submissions remain separate edits. No multi-field draft modal or live numeric preview is needed.

Keep singleton controls unchanged. Keep name/type/paint mode, Fill Rule, gradient topology/picker/ramp, path geometry, and animation/key toggles singleton-only. Bulk animation toggles need a separate contract for mixed animated/static state and destructive key removal. Compound Colors requires a separate value/topology model; this slice neither implements nor prepares an implicit stop correspondence.

## Source facts and limitations

- `apps/desktop/src/panels/contents/tree_selection.rs`: selection is transient, restricted to immediate siblings, and tracks stable item IDs plus parent/anchor/cursor. Descendants of a selected Group are not separately selected.
- `panels/contents.rs:713` currently returns “Select one item for editing controls” for multiselection. `publish_tree_selection` in `panels/contents/tree.rs` clears singleton fields and publishes `EditorState.contents_selection = None` for a block. Preserve that singleton-only public identity so Pen/gradient overlays do not acquire an arbitrary bulk target.
- `ContentsNode::parameter_order`, `parameters`, `value_at`, and `ContentsParam::bounds` supply typed identity, order, sampled/clamped display values and constraints. Full source validation enforces the exact parameter set for each kind/schema.
- `time_remap::edit_track` implements existing Contents value semantics: no keys means write static base; otherwise insert/replace a key at the frame, retaining an existing key's interpolation/temporal handles and using Linear/default handles for a new key. There is no separate `auto_key` state found in the reviewed tree.
- Ordinary Contents value writes do not skip a value equal to the sampled value. Only Trim has a dedicated sampled no-op guard and pure-value no-op route. A generic `Batch` of ordinary Track edits can add unnecessary keys and enter legacy schema/asset migration. Reusing it without a bounded core path is insufficient.
- `document_revision` is not a general edit serial: ordinary edits and Undo/Redo can leave it unchanged. Tree contexts additionally use exact source and transport generation. `TextField` commits on Enter, blur and outside mouse-down; it marks its text accepted before the callback. These details must be designed into stale/invalid handling.
- STATUS currently records bounded sibling-tree native coverage separately from cross-parent headless evidence/native pending. Those earlier native results do not cover bulk fields. This investigation ran no tests and no native actions.

## Common-property contract

1. Resolve the selected layer and exact nonempty sibling set in source order. UI requires at least two; the core command may accept any nonempty explicit set. Reject stale/missing/duplicate IDs and mixed parents; never silently shrink a command's target set.
2. Intersect typed `ContentsParam` values actually present on every validated selected node. Do not intersect labels: Fill Opacity, Stroke Opacity and Group Opacity are different channels. Preserve the first selected node's `parameter_order`, using source sibling order rather than click/ID order.
3. Exclude every `ContentsParam::Gradient(p)` for which `p.stop().is_some()`. Stop IDs are local to each gradient; two different gradients can both have stop 1 with unrelated meaning, even when one originated as a duplicate. Never infer correspondence from ID, sorted location or array index.
4. Gradient Start/End X/Y are eligible across Gradient Fill/Stroke. Highlight Length/Angle are eligible only when every selected gradient is radial, matching existing visible relevance. No gradient-type change is included.
5. Existing Dash/Gap indices are eligible only if present on every selected stroke; they already encode a pattern position, unlike gradient stop IDs. Never add dashes. Rounded rectangle/star/polygon parameters appear only where the exact intersection permits them. Paths have no numeric Contents parameters, so including a Path can correctly yield an empty intersection.
6. Values are raw local scalars in the displayed units. Assigning equal Position values is not a delta, distribution, world alignment, transform compensation or recursive Group edit. Disabled nodes remain editable; layer lock is the existing lock boundary.

Examples: rectangle + ellipse share Width, Height and Position X/Y; polygon + star additionally share Points; Fill + Gradient Fill share Fill Opacity; Stroke + Gradient Stroke share existing stroke scalars but not solid RGB; Group + parametric item share Position X/Y; Fill + Stroke have no shared numeric channel.

## Minimal UI

- Replace the multiselect fallback with “N items · Shared numeric properties”. Keep the existing tree and hierarchy actions. With no common property, show “No shared numeric properties”.
- Each row has the full property label and one text field. Display the full-precision sampled value only when every selected sample compares equal; otherwise display “Mixed”. Use exact numeric equality, not formatted strings or an epsilon. The placeholder/sentinel is presentation, never an f64 fallback or zero.
- A field accepts an explicit finite absolute value; Enter or ordinary blur submits one guarded command. An untouched Mixed field, focus alone, selection alone, Escape, or typing away and back to the original uniform text does not submit. Empty/invalid/nonfinite/out-of-bounds input produces an error and restores the source display; no partial edits. Keep ordinary fields text-only initially: no mixed numeric scrubbing, delta entry or live source preview.
- Provide concise help: “Sets this value on all selected items. Static properties stay static; animated properties update at the playhead.” An optional noninteractive per-row animation summary can say “Static”, “Animated” or “Mixed animation”; no bulk stopwatch/key/Graph action in this milestone.
- After a successful field commit, retain selection, anchor/cursor, disclosures and Graph pins/ranges. Rebuild/rebind fields to the new source so a second property can be edited. History keeps its existing selected-key clearing behavior; do not broaden this feature into changing it.

## Core transaction

Add a dedicated nonserialized edit, for example:

`ContentsEdit::SetSharedValue { parent: u64, items: Vec<u64>, parameter: ContentsParam, frame: Frame, value: f64 }`

Wrapped by the existing `Command::Contents { id: layer, edit }`. It changes no project/LEP/VIEW/address schema. Its name is illustrative; the behavior is the requirement.

1. Validate original project and metadata budget before applying the new pure command path. Check active-layer existence/type/lock, frame within duration, nonempty unique IDs, exact immediate-parent membership, the common parameter and the same exclusions/relevance rules as UI. Validate finite value and inclusive parameter bounds before any equality shortcut. A no-op cannot make an invalid target valid.
2. Plan all target track updates from the original candidate. For each item compare the requested value with `node.value_at(parameter, frame)`, the same clamped sample displayed and rendered. If equal, preserve that entire track, including base, dormant data, keys/handles and signed-zero representation. Use exact equality; retain real small changes.
3. For each changed item call the existing track-edit semantics. Static stays static. Animated changes only the current key; new keys are Linear/default handles, existing eased/temporal metadata is retained. In a mixed static/animated selection, both behaviors happen atomically. Skip already-equal members even when others change.
4. Validate the completed candidate and final metadata budget, then accept once through normal history. Exact source equality means no history entry and Redo survives. One invalid member, key-count overflow or document-budget failure preserves the complete source, history, Redo and selection.
5. Give only this dedicated command, and nonempty recursively pure batches of it if supported, the source-preserving acceptance route. Do not change the legacy ordinary Track, mixed-batch or empty-batch semantics incidentally. Preserve declared schema and assets for changed and unchanged transactions because the operation materializes no new property kind. Validate original/final budgets, rather than disallowing a pure batch merely for temporary intermediate size.

## Context, input and pending-draft rules

Store a bulk field context in `ContentsControls`, separate from `ContentsFieldTarget`: immutable project snapshot, document revision, composition/layer, exact selected ID set and parent, field parameter, frame, transport generation and a monotonic binding/session serial. Include relevant selected-layer/modal state; selection changes must advance the serial even if they later return to the same IDs. Capture/check through the owning panel, not only `EditorState.contents_selection`, which is intentionally None for every bulk selection.

- Source edits, Undo/Redo round trips, Open/new/active-composition changes, seek/step/play/stop, layer changes, lock, deleted/reused item IDs, selection changes, and conflicting color/gradient/vertex/text sessions invalidate old callbacks before rebinding. Never reinterpret an old value at the new playhead or on a reduced/new selection. Transport generation plus local serial prevents source-equal return paths from reviving a callback.
- Ordinary pointer selection after a valid pending field may commit that field to its original selection first, then perform the selection action. Use the existing tree pointer-before-blur receipt/revalidation flow; after the known synchronous field commit replan from the new document, never simply relax all source checks. A context already stale before the pointer press is rejected before any flush.
- Marked IME must be checked before an outside mouse-down can submit. The existing tree/hierarchy opt-in IME-preserving path is relevant; a late check in an `on_click` handler is insufficient. Keep the same input ownership so Delete/Ctrl+A/arrows in a field do not become tree or layer commands.
- Invalid field input is not a source edit. Explicitly resync/recreate that field after rejection because current `TextField::submit` first updates its `original` text. Do not let invalid text become the persistent accepted baseline or let its later blur replay it. A new bulk-only text-field policy may be preferable if it keeps common TextField behavior unchanged.
- Do not queue/merge new bulk changes into an existing ramp/color/vertex draft. Exclude editing during playback and modal drafts. Opening another relevant editor either follows its existing explicit flush policy or invalidates pending bulk text; it must not accidentally apply both sources.
- UI should dispatch only a current synchronous command; source and lock validation also remains in core. Errors must identify the issue without reporting a successful edit. No source mutation occurs while typing.

## Acceptance required before claiming this slice complete

### Automated/model

1. Heterogeneous intersections above, deterministic source order, no common properties, existing dash index limits, radial-only highlights, and exclusion of matching-but-unrelated stop IDs. Selection/display alone leaves exact project/history unchanged.
2. Shared versus Mixed values use exact sampled values and full precision, including visually similar unequal numbers, -0, tiny changes and clamped animation overshoot. Mixed blank/unchanged/invalid cannot become zero.
3. One static, one between-key animated and one existing eased-key member: one assignment yields the independently expected complete tracks, preserves unselected nodes/subtrees and performs exactly one Undo/Redo. An already-equal animated member gains no redundant key.
4. All-equal submission, Escape, rejected inputs and pure exact-return transactions preserve source bytes, declared schema/assets and Redo. Failed frame/type/parent/duplicate/missing/lock/gradient-stop targets and 10,000-key/document-budget boundaries reject atomically. A stored invalid source cannot be silently repaired. Test inactive composition and legacy schema as well as active/new files.
5. Actual field/session callbacks reject selection A→B→A, edit→Undo source equality, seek→return, playback stop, lock/unlock, Open with reused IDs, deleted targets and modal changes. Exercise pending input before tree press, invalid input, IME gating and duplicate Enter/blur receipts. Helper tests are not native event evidence.
6. Independent literal expected documents plus official codec round trips; selected current/surrounding-frame render comparisons for nested/reflected Groups, heterogeneous geometry, solid paints and gradient endpoints. Preserve stable Graph addresses/pins/ranges and all unrelated path poses/keys.

### Bounded native gate (record as unrun until actually available)

Use a pinned final release and saved fixtures. Check: (1) multi-selection/intersection/Mixed presentation; (2) mouse entry and keyboard Enter for one uniform and one Mixed scalar; (3) blur-to-another-field and blur-to-tree selection commit once to the old target; (4) invalid/empty/Escape and real marked-IME ownership; (5) mixed static/animated current-frame edits including an eased key, no-op and exact Undo/Redo; (6) retained selection after two consecutive edits and hierarchy movement; (7) stale/lock/playback/modal context rejection; (8) actual native Save/Open/resave preserving resulting PROJ/VIEW. Capture screenshots and exact output assertions; headless fixture encoding is not native Save/Open. If native access remains unavailable, ship only as headless-verified/native-pending and leave E04 Partial. Compound Colors still remains.

Run the repository's required Rust checks against the final source after implementation, then build/attribute release and retain previous renderer/codec regression evidence. This design review has not run those checks.

## Proposed ownership and integration boundaries

- **Core worker:** new `crates/core/src/contents_bulk_fields.rs` (or isolated helpers in `shape_contents.rs`) and dedicated `contents_bulk_fields_tests.rs`: validation/planning/atomic value logic. Small shared edits to `shape_contents.rs` for enum/dispatch, and `lib.rs` for module/pure-command acceptance. Coordinate those shared files explicitly.
- **Desktop worker:** new `apps/desktop/src/panels/contents/bulk_fields.rs` for intersection, binding/session model and rendering; focused tests alongside it. Minimal integration in `panels/contents.rs` and `panels/contents/tree.rs` for multiselect publish/invalidation. These files overlap the current drag work, so begin after its gate or let one integrator own them.
- **Integration/QA owner:** new `apps/desktop/src/contents_bulk_fields_render_tests.rs`, module registration, prepared CLI/native fixtures outside source, exact source pin and aggregate/release/native gate. Review `TextField` and `color_edit` opt-in input policy changes centrally; avoid broad common-component behavior changes.
- **Integrator only after verified outcomes:** STATUS/DEVELOPMENT_BACKLOG/README updates that distinguish implemented behavior, automated results, native results and remaining compound/bulk-animation scope. No renderer rewrite, serialization change, gradient stop reidentification or global history cleanup is needed.

Main source references: `crates/core/src/shape_contents.rs` (typed parameters/defaults, validation, Track edit), `crates/core/src/time_remap.rs:188` (track write policy), `crates/core/src/lib.rs:1403` (candidate/history/migration routes), `apps/desktop/src/panels/contents.rs:33` (current singleton field target), `apps/desktop/src/panels/contents/tree.rs` (selection publication and guarded input contexts), `apps/desktop/src/components/text_field.rs` (commit/blur/IME behavior), `apps/desktop/src/color_edit.rs:11` (source-bound input receipt pattern). Line numbers are review-time hints and may change during concurrent drag integration.
