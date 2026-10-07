# Bounded compound Gradient Colors contract

This slice adds complete color/opacity stop snapshots with **Hold-only** time
sampling. It does not infer stop correspondence, interpolate topology, match
stops across paints, add numeric Graph lanes, or implement AE color interpolation.

## Source and time

- `ShapeGradient.colors_animation` is optional. When absent it is omitted from
  JSON, and the existing scalar stops, renderer, source ordering and project
  version remain unchanged.
- When present its `keys` map contains complete ordered color and opacity rows.
  Each color stop has `id`, `position`, `midpoint`, `red`, `green`, `blue`; each
  opacity stop has `id`, `position`, `midpoint`, `opacity`. Positions/midpoints and
  opacity use percent; RGB uses 0–255. Color and opacity counts are independent.
- A key takes effect exactly at its frame and holds until the next key. Before
  the first key its complete snapshot is held. Coincident spatial stops retain
  explicit row order. There is no temporal interpolation setting to corrupt.
- The old scalar rows remain a static base while animated. Enabling rejects any
  legacy animated stop track rather than sampling away existing animation.
  Endpoint/highlight, paint opacity, stroke and group tracks stay independent.
- Disable bakes the current sample into legacy static rows; deleting the final
  key bakes that deleted key's sample. Both remove the compound representation.
  This is one Undo step, and Undo restores exact source storage and version.

## Editing and identities

`ContentsEdit::GradientColors` accepts `GradientColorsEdit::{SetAnimation,
ToggleKey, Set, Value, Color, AddStop, RemoveStop, MoveKey}`. Every frame is checked
against the composition. `Set` accepts a complete snapshot and is the modal
transaction boundary. Bit-exact sampled equality is a no-op, retaining storage,
allocator, version, history and Redo. Signed-zero positions remain distinct because
the retained spatial sampler uses total ordering for coincident stops. The
snapshot itself has no allocator, so a
discarded draft's add/remove allocator changes cannot leak through acceptance.

IDs are nonzero paint-local u64s, unique across both rows in each snapshot. A
known ID cannot switch between color and opacity rows in any stored snapshot.
New accepted IDs advance the paint allocator; deletion never rewinds it. Distinct
paint IDs never imply correspondence. Generic scalar stop editing, key paste,
Graph/Timeline track access and old Add/Remove routes are unavailable while
compound animation is active; all stop mutation uses the compound API.

`colors_at` and `sampled_node` expose the same complete sampled rows used by the
ramp and renderer. Modal previews may use a temporary editor that bakes only its
cloned current sample, run the existing static stop editor, then submit one `Set`
snapshot against the untouched original project.

## Bounds and persistence

Each row has 2–32 stops. Values must be finite and meet existing scalar bounds,
including 1–99% midpoints. Each gradient has at most 1,000 keys and 32,768 stored
stops across keys. Composition frame bounds, project 16 MiB metadata budget,
locked targets and whole-document validity are checked before accepting history.
Invalid commands and source metadata leave the project and Undo/Redo unchanged.

Project schema maximum becomes 54; an edit advances a project's declared version
only when compound keys actually materialize. No schema upgrade occurs on legacy
scalar edits, rejected promotion, no-op or cancellation. Existing version54
projects do not downgrade when disabled. Unsupported future versions and
compound data declared as version53 or older are rejected before file output.
Native LEP1, VIEW1/2 and numeric-address1 are unchanged.

Layer shift and clipboard frame-rate conversion retime complete snapshots with
range/collision checks. Layer split, layer/Contents duplicate and reparent retain
all stored poses, including out-of-layer-range keys. Inactive compositions,
assets, unused paths and unrelated tracks are preserved. No stop clipboard or
cross-paint bulk compound operation is introduced.

## Acceptance

Core tests cover independent rows, changing topology, boundary frames, spatial
midpoints/coincidence, no-op/Undo/Redo, scalar-route rejection, malformed IDs and
values, frame/stop/key/storage/document bounds, schema, JSON/native codec,
shift/split/duplicate and inactive data. Independent desktop tests compare
compound frames with literal static legacy source and preserve endpoint Graph
VIEW state. Native UI/IME/DPI and Windows acceptance must be reported separately
from headless validation; Hold-only topology is a bounded E04 extension.
