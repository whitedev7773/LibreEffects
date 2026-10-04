# Libre Effects engineering handoff

Updated 2026-10-04 UTC. This is a repository-facing engineering record. Read
[STATUS.md](STATUS.md) for the current feature inventory and
[DEVELOPMENT_BACKLOG.md](DEVELOPMENT_BACKLOG.md) for dated history and priorities.
Read the root [AGENTS.md](../../AGENTS.md) before changing code.

## Resume point and owner constraints

- Repository: `whitedev7773/LibreEffects`.
- Use **only `codex/ae-workspace`** for this work. Preserve existing history and
  unrelated changes. Do not work on or move `main`.
- Latest implementation commit: `75f5055e965d31a5b4ec44f0726fc4fab4ae4286`,
  `contents: add guarded cross-parent block dragging`.
- The implementation checkout was clean at that checkpoint. This handoff is a
  subsequent documentation-only commit; use `git log -1` for its exact SHA.
- Development was explicitly paused for a requested cloud-computer recovery.
  The owner then requested this handoff and a push of completed work to the same
  branch. No next-feature implementation has begun. Resume feature development
  only when the owner resumes it.
- Publication authorization is for this completed snapshot on the existing branch.
  Do not create another branch, PR, merge, force-push, deploy or change credentials.
  Verify the remote ref before/after publication; never infer success from a local
  commit or a requested push. Stop on divergence rather than rebasing blindly.
- The owner requested development on the cloud development computer with actual
  application execution and live progress reports. The owner's separate desktop
  is not an automatic substitute for an unavailable cloud screen.
- The native extension is **`.lep` (Libre Effects Project)**. Preserve legacy
  `.lfe.json` import and original inputs. Never rename the format to `.lfe`.
- The About dialog must keep automatic build identification after source-changing
  builds. Do not replace it with a hand-edited counter or commit-count-only value.
- Prioritize editing/UI, effects, text and masks before cache/high-end output work.
  This remains an early 2D editor, not complete AE/AEP/JSX compatibility.

## Publication status at handoff

The owner explicitly authorized the completed snapshot and this note to be pushed
to `codex/ae-workspace` on 2026-10-04. The attempted documentation-inclusive push
of `88db33c8a5026fef4e2bffa04ca41d9f25a23956` failed before authentication:
`could not read Username for 'https://github.com': terminal prompts disabled`.
The existing SSH route also failed hostname resolution. No credentials, host
security settings or network policy were changed to work around these failures.

Read-only Git and the GitHub connector both verified the remote branch remains
`52ab32dcff3e66fd5980255db84e7ce9ac0979fa`; it is an ancestor of the local work.
The subsequent documentation correction records this blocker. No push succeeded,
and no new remote CI was triggered for the completed work. Do not recreate commits
through an API with different authors/timestamps just to imitate a push. Retain
the original complete history in the provided self-contained bundle and use a
normally authenticated Git environment when transfer is available.

## Latest validated implementation

E04 stage1, `20783a9fec8e6576fc36cd37f7faa9c4c99345cd`, added atomic cross-parent
sibling blocks through Move Into/Out and tree-owned Ctrl+Right/Left.
Stage2, `75f5055`, connects guarded drag targets:

- Label top/bottom edges select Before/After; Group centers select Into; a dedicated
  root landing row appends at root. Leaf centers, eye/disclosure and blank space
  cannot become a drop target.
- The same clipped geometry and semantic planner produce previews and the final
  release command. Expanded After markers follow the visible subtree end at the
  parent indentation. Offscreen endpoints are unavailable, not clamped to an edge.
- Source sibling order, stable IDs, complete subtrees, local records, unused path
  poses and keys are preserved. Destination transforms/paint/Trim/isolation apply;
  placement may change. There is no world-space compensation or baking.
- Same-parent drops use Reorder; cross-parent drops use one MoveSiblings command.
  No-op/cancel preserves history. Destination reveal happens only after success.
- Exact one-use down receipts validate before pending-field blur. Full source,
  context, transport/pointer generations, current geometry, focus/activation,
  button/modifier and IME guards prevent stale dispatch. Scrolling, layout change,
  Escape, new press and focus loss cancel. There is no auto-scroll or hover expansion.
- No core renderer, project schema, LEP, VIEW or address-version change in stage2.

Main code: `panels/contents.rs`, `panels/contents/tree.rs`,
`panels/contents/tree_selection.rs`, `panels/contents/tree_drop.rs`,
`color_edit.rs`, and the Inspector scroll guard. Main new integration tests:
`panels/contents/drop_integration_tests.rs`. Paths are under `apps/desktop/src/`.

### Exact release and automated results

- Build number: `20261004.110504-c2a6fb77abcf9417`.
- Linux executable SHA-256:
  `b447fad0f8104981a2d0214b25884e69cf591b5018887f888ce6bf62e271150d`.
- Size: 65,873,472 bytes. Source manifest: 341 watched files, fingerprint
  `c2a6fb77abcf9417`; all hashes matched the committed implementation.
- The executable was compiled before its commit. Its embedded source description
  truthfully says `Git 20783a9fec8e (modified sources)`; the frozen input hashes
  identify the code committed as `75f5055`. Do not rewrite historical metadata.
- Default suite: **1,359 passed** = 450 core + 903 desktop + six build helpers.
- Explicit Linux media suite: **32 passed**. The Windows physical audio-device
  test is not established by Linux results.
- `cargo fmt --all --check`, all-target workspace check, test compilation and a
  normal `cargo build -p libre-effects-desktop --release --locked` passed.
- Focused filters: 61 passed. There are 34 newly named tests and two obsolete
  same-parent expectations were replaced, a net addition of 32 default tests.
- Two independent source reviews found no blocker. Installed GPUI capture/prepaint
  ordering was inspected, but this is not native event-delivery evidence.
- New CLI: two samples, 12 exact RGBA pairs, 460,800 compared pixels, including
  independent analytic rectangles/baked geometry and codec outputs.
- Retained stage1: 40 exact pairs/1,536,000 pixels; retained text references:
  31,940,800 pixels; Trim 21 frames; Luma 18 frames; opacity 46 renders; all-pose
  28 pairs/25,804,800 pixels; Source Text 28 renders/21 pairs/12 rejections that
  preserve existing output. No pixel tolerance or per-pixel repairs were added.
- During state recovery, a duplicate pipeline was discovered and stopped. The
  original workspace terminal exit-0 summary was retained separately because
  the duplicate overwrote its running-stats file. The original media and release
  gates completed. Source hashes were unchanged. Do not treat a leftover
  `running` stats file as proof a command is still active.

No new remote Windows CI result is claimed here. Publication and remote CI must
be verified independently for the exact documentation-inclusive commit.

## Native acceptance blocker: 47 cases unrun

Actual native Linux QA passed earlier milestones through Trim/About build B.
The last fully verified native milestone was `9f108b2` around 06:18 UTC on
2026-10-04. The then-observed saved project was
`trim-paths/native/snapshots/10-two-composition-reopened.lep` under local QA.
That historical observation does not prove the editor is still running.

The supported screen tool has since failed with
`native pipe path unavailable: No such file or directory (os error 2)`.
The latest recorded supported check was 10:23:57 UTC. Resetting the tool did not
restore access. The shell is an isolated process namespace without a host
systemd/desktop service. No supported cloud-host restart control was found;
no host reboot was performed. Do not substitute arbitrary process killing,
private socket access or another computer for supported recovery.

Keep these native cases explicitly **NOT RUN**, independently of green headless
checks:

| Feature | Cases | Local acceptance document |
| --- | ---: | --- |
| Luma Key | 8 | `luma-key/native/` fixtures and reports |
| Text Fill/Stroke opacity | 8 | `text-opacity/` acceptance materials |
| Source Text Hold animation | 8 | `source-text/` native plan |
| Base + all stored path poses | 9 | `whole-path-poses/native-plan.md` |
| Cross-parent buttons/keyboard | 8 | `cross-parent-contents/native-acceptance.md` |
| Guarded cross-parent dragging | 6 | `cross-parent-drag/native-acceptance.md` |

These paths are relative to the **local QA directory**, not tracked repo paths.
After screen recovery, establish the actual application/window/project state,
use a preserved exact release and isolated QA copies, and perform the planned
interactions. Distinguish native Save/Open from test-generated LEP encoding.
Real Korean IME, Windows/macOS, DPI, audio-device and AE comparison limitations
remain explicit. Headless planner tests do not exercise actual ContentsControls
selection/reveal or the OS's event delivery.

## Development setup and commands

Use the versions in `.prototools` and the platform prerequisites in
[README Development](README.md#development). The tested cloud setup used Rust
1.97.0, rootless Debian13 libraries and FFmpeg/FFprobe7.1.5. It has these local
helpers; they are not part of the repository or guaranteed on a fresh machine:

```sh
. /workspace/shared/libreeffects-dev-env.sh
export LD_LIBRARY_PATH=/workspace/shared/devsys/root/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
```

On a fresh Linux desktop install the documented official toolchain, Vulkan/ICD,
Wayland/X11, fontconfig, C/cmake and FFmpeg dependencies. Native file dialogs also
need a running user D-Bus session, xdg-desktop-portal and a compatible backend.
Use the selected desktop session's environment; do not assume a headless shell
can show a window or invoke a chooser.

Standard checks from repository root:

```sh
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo test -p libre-effects-desktop --locked -- --ignored --skip device_clock_minute
cargo build -p libre-effects-desktop --release --locked
cargo run -p libre-effects-desktop --release --locked
```

Moon equivalents are documented in README and AGENTS. The cloud validation used
direct Cargo after an earlier Moon/proto plugin setup failure; do not claim Moon
was validated by these runs. For this constrained worker only, these command-line
metadata overrides were used for check/test, not normal release:

```sh
cargo --config 'profile.dev.package.libre-effects-desktop.debug=0' check --workspace --all-targets --locked
cargo --config 'profile.test.package.libre-effects-desktop.debug=0' --config 'profile.test.package.libre-effects-desktop.strip="debuginfo"' test --workspace --locked
```

Assertions, features and optimization are unchanged by these overrides. Normal
release uses neither. Compile/link was limited to one available CPU via process
affinity; test execution used the normal affinity. Serialize Cargo commands and
retain command exit status/logs. Check existing runs and artifacts before repeating
expensive work. Do not infer the cause of a previous SIGKILL from memory estimates.

For a portable CLI smoke render:

```sh
cargo run -p libre-effects-desktop --release --locked -- --render examples/gradient-study.lfe.json --output /tmp/libreeffects-gradient-smoke.png --size 640x360 --start 0 --end 1 --fonts strict
```

Use a new output destination; strict-font and output-preserving failures are
intentional behavior. GUI launches participate in the single-editor ownership
protocol and may checkpoint/replace an existing instance. Do not casually launch
another build over a user's unsaved session or delete its lock file. CLI rendering
is independent of interactive ownership.

## Format and build identity invariants

[LEP format specification](../../docs/lep-format-v1.md) describes the magic header,
versioned bounded chunks, CRC validation and atomic storage.
Media stays linked; Collect Files packages/rebases links rather than implicitly
embedding all media. Save/Open/Save As/CLI/autosave/recovery must share the codec
and preserve originals on rejection. Current project schema is 53 (Source Text),
LEP container1, VIEW1/2 and numeric graph address1. A project feature schema change
is not automatically a container or VIEW version change.

Build identity is generated by `apps/desktop/build.rs`/`build_support.rs` and
shown by Help → About Libre Effects. It includes UTC time and a deterministic
path/content FNV change identifier. Dirty source changes are covered; cached Cargo
invocations keep their compiled identity. FNV is not a security signature.
Keep generated outputs outside watched source trees. Restoring old mtimes may
require cleaning the desktop package before a trustworthy rebuild.

## Next bounded implementation: shared Contents numeric fields

**Proposal only; no code started.** Detailed portable design:
[CONTENTS_BULK_FIELDS_PLAN.md](CONTENTS_BULK_FIELDS_PLAN.md).

For two or more selected siblings, show the intersection of eligible typed scalar
properties. Display the exact common sample or `Mixed`; an explicit finite
absolute value at the displayed frame applies to all selected items in one Undo.
Static tracks stay static; animated changed samples update only the current key.
Already-equal members must remain byte-for-byte unchanged, including unused data,
keys/handles, declared schema/assets and Redo for whole-command no-ops.

Use a dedicated validated core route. A naive Batch of ordinary Contents Track
writes can insert redundant keys and invoke legacy schema/asset migration.
Exclude gradient stop-ID properties: IDs on different gradients do not establish
correspondence. Shared radial highlight controls require all targets to be radial.
Do not add bulk animation toggles, compound Colors, a multi-field modal or new
renderer/schema work to this first slice.

Capture full project/context, exact selection/parent, frame, transport generation
and a binding serial. document_revision alone does not change for every edit or
history operation. Existing TextField accepts its text before callback validation;
invalid/stale submissions need explicit resynchronization. Preserve before-blur
IME/pending-field guards and keep multi-selection's public singleton target None.
Automated acceptance and eight proposed native cases are in the linked plan;
those eight are future cases and are **not included** in the existing 47.

Other substantial work remains in the complete 77-ID STATUS inventory. Its counts
are 25 Implemented, 35 Partial, eight Not implemented and nine Separate advanced
scope. Counts are not a completion percentage. Compound gradient Colors, rich text,
Text Animator, topology changes, SVG import, advanced compositing/3D/script/plugin
work and productization are not silently complete. Morphology also needs a bounded
owned raster stage; the pinned resvg morphology primitive has kernel/work semantics
that make a naive SVG wrapper unsafe for the intended bounded contract. Preserve
existing mask-expansion pixels while designing it separately.

## Local recovery and evidence inventory

Current local checkout:
`/workspace/scratch/77da71e2f4dc/LibreEffects`.
QA root: `/workspace/shared/libreeffects-qa`.
These are machine-local engineering artifacts; a GitHub clone alone does not
contain them. Source tests and the documents linked above are tracked and portable.
Request/copy the saved artifacts through the authorized file-transfer route if
using a different machine; do not claim local paths are remotely accessible.

Before this handoff, the clean implementation and its complete branch history
were saved as `libreeffects-local-75f5055.bundle` (no prerequisites):
SHA-256 `5275864ff80504cc08e56ccbb6fa6470d8dec176453b3f7b74f6a0a750f1da2b`.

Recovery directory: `recovery-20261004T1111/`.
`committed-code-release-and-qa.tar.gz`: 56,523,273 bytes, 243 files,
SHA-256 `8417170e20a6b05320e1ac4070333f4fe5a119f9d847fb2de7d24ac87788eb6b`.
Archive readback and unchanged original hashes were verified. It contains the
complete bundle, exact stage1/stage2 Linux releases, source manifests, relevant
QA inputs/reports and the then-next design, without credentials. Its manifest
and RESTORE.txt describe verification. This archive predates the handoff commit;
use the latest repository commit or later bundle for these documentation additions.

Latest pin: `libre-effects-guarded-drop-v1-release`.
Record: `cross-parent-drag/release-v1.json`.
Source manifest: `/workspace/shared/guarded-drop-final-v1-source-sha256.json`.
Stage1 record: `cross-parent-contents/release-v1.json`.
Source hash checks must happen again if a code file changes; documentation-only
updates do not require rerunning the application suite. Never edit an old binary,
baseline, frozen input or acceptance report to make a new result appear to pass.

When resuming: verify branch/HEAD/working tree, inspect current supported screen
availability, read STATUS plus the next plan, and preserve any newly present work.
Do not restart the earlier milestones or manufacture a global “all backlog done”
status from these bounded checkpoints.
