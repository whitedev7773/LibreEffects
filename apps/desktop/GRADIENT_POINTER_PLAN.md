# Compound Gradient Colors pointer-drag contract

Bounded E04 continuation, 2026-10-04, from clean `d629a52` on
`codex/ae-workspace` in the existing dot-cloud checkout. Local commits only;
no push, PR, deployment or user-desktop work. Preserve earlier saved native
projects and the GitHub tab. The [multi-key contract](GRADIENT_MULTIKEY_PLAN.md)
continues to define the dedicated source-preserving MoveKeys transaction,
paint-local identities, interpolation, selection ownership and repeated Delete
protection. No schema, LEP/VIEW or scalar property-address changes.

## Bounded gesture

Only the dedicated whole-Colors Timeline lane is eligible, on one paint. Plain
press on a selected key retains its selected peers while a drag is possible;
releasing an ordinary click selects only that key. Press on an unselected key
selects it. Shift-click toggles membership and is selection-only. A horizontal
four-pixel threshold separates click from drag; vertical movement alone cannot
retime keys. Modifier changes that invalidate admission cancel safely.

The press freezes the lane mapping, key set, source, ownership and interaction
context. A drag translates every selected key by one integer-frame delta,
preserving gaps, snapshots, ordered stop IDs and outgoing modes. Pointer offset
inside a glyph must not cause a jump. Timeline scroll/zoom/viewport changes must
retire the gesture rather than reinterpret it. There is no automatic scrolling
or scaling. Composition bounds clamp the common delta without crushing spacing.
Snapping uses the frozen playhead and unselected same-paint keys within eight
screen pixels; ascending selected-anchor and target order resolves equal ties.
Alt bypasses snapping. A collision with an unselected
same-paint key rejects the whole drop; selected-old-frame overlap is allowed.

## Preview, capture and cancellation

Click release may seek as before; a crossed-threshold drag never seeks.
During movement only transient Timeline ghost glyphs/status may change. Source,
assets, VIEW, playhead and Undo/Redo remain untouched. Release recomputes from
the final pointer and dispatches at most one existing atomic MoveKeys command.
Exact return to the start and zero-frame translations preserve source/history,
including Redo. A successful drop remaps the transient selected frames.

Use the GPUI event-routing equivalent of pointer capture, including release
outside the lane. Escape, focus/deactivation, missing-button/cancel signals,
newer selection, source/history ABA, transport, modal, lock and pending/composing
input must never commit a stale gesture. Refuse pending input before focus can
blur it. Keep compound ownership after deleted/retired selections until explicit
exit so repeated Delete/Cut/Copy/Duplicate cannot fall through to a whole layer.
No source preview command or VIEW persistence is introduced for this gesture.

## Required evidence and exclusions

Focused math/receipt tests cover threshold, vertical/away-back, nonzero start,
zoom/scroll mapping, common bounds, deterministic snapping, collision, final
release position, cancellation/stale guards, selection and repeated shortcuts.
Independent literal full-source/VIEW and static-render expectations cover the
four Fill/Stroke × Linear/Radial paints, interpolation/topology, JSON/LEP,
atomic Undo/Redo/no-op and unrelated data. Generated fixtures remain immutable
and distinct from actual native saves. Run the pinned full default/media/vendor/
font/check/format/release gates and attribute bounded native gestures and files
to exact code/build identities. Report coordinate precision and atomic input
limitations honestly; automated cancellation tests are not held-pointer native
observations. Finish documentation and verified complete-history bundle before
starting another feature.

Marquee/box selection, cross-paint dragging, time scaling, compound Graph,
Bezier/velocity, topology reconciliation and cross-project interchange remain
separate. E04 and the 77-ID inventory remain Partial. Earlier native gaps,
IME/adversarial/held-event timing, Windows/macOS and DPI/device coverage remain
separately unqualified. Cache/GPU/output, 3D/tracking and Adobe interchange stay
behind editing-first work.
