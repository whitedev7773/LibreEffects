# External JSX acceptance harness

`external_script_harness` runs an external UTF-8 script once through the same
killable child-process supervisor and execution budgets as the desktop. It uses
an isolated decoded native project and scripted responses to actual ScriptUI
requests. It does not substitute a parser, replay the script, inject test code,
or grant guest filesystem/network access. The command-line host reads the three
explicit input files. Inputs are never overwritten.

Build with the pinned toolchain, one compiler job and incremental compilation off:

```sh
cargo build -p libre-effects-editor-model --example external_script_harness --locked
target/debug/examples/external_script_harness \
  --project /absolute/path/template.lep \
  --script /absolute/path/external.jsx \
  --plan /absolute/path/events.json \
  --output /absolute/path/new-result-directory
```

The output directory must not exist. A minimal event plan for an independently
authored dialog with one edit field and an Apply button is:

```json
{
  "frame": 0,
  "selected_layer_ids": [],
  "expected": { "kind": "success", "changed": true },
  "steps": [
    { "action": { "kind": "change", "selector": { "kind": "edittext" }, "text": "Example" } },
    { "action": { "kind": "click", "selector": { "kind": "button", "text": "Apply" } }, "required_text": ["Example"] },
    { "action": { "kind": "dismiss_alert" }, "required_fragments": ["complete"] }
  ]
}
```

Selectors must match exactly one visible enabled control, including enabled
ancestors. IDs come from the current request tree. Exact text and fragment checks
can verify validation messages before the next action. Supported actions are
`change`, `click`, `close`, `resize`, `confirm` (with boolean `value`),
`dismiss_alert`, and `cancel` (the final step, interrupting the entire operation).
The action type must match the current modal request, so a dialog click cannot
accidentally accept a nested confirmation. Unknown plan fields and extra or
missing UI requests fail the run. Plans are bounded to 128 steps and 1 MiB.

A rejected/canceled run uses, for example:

```json
{ "expected": { "kind": "failure", "contains": "canceled" }, "steps": [{ "action": { "kind": "cancel" } }] }
```

The existing ScriptUI `window.close(0)` contract reports cancellation as a failed
transaction. A successful source no-op instead expects `success` with
`changed: false`. Failure text is case-sensitive.

Every run seeds an independent Redo branch without changing baseline source.
Successful edits commit through `Editor::commit_automation_project`, then verify
one Undo restores the entire baseline and one Redo restores the entire result.
Rejected and no-op runs must preserve baseline and the exact pre-existing Redo
branch. Native output is decoded and re-encoded to check Save/Reopen identity.
The receipt separately reports whether input serialization itself was canonical.

Outputs include `baseline.lep`, `result.lep`, `ui-transcript.json`, `receipt.json`,
and, for edits, `undo.lep` and `redo.lep`. The transcript preserves unexpected
observed requests too. Independently compare the complete intended source changes;
the harness's history checks alone do not prove that a script produced the right
layers, keyframes, text or pixels. Native dialog rendering and keyboard behavior
also require separate app qualification.

Keep private scripts, their event plans, transcripts and numeric source oracles
outside the repository. Record and verify the private input hash before/after
each acceptance batch. No AEP conversion or Adobe numerical/visual parity is
implied by this native execution test.
